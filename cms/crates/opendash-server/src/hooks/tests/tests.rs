use super::*;
use std::sync::Mutex;
#[tokio::test]
async fn ordered_handlers_continue_after_failure_and_timeout() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let mut dispatcher = HookDispatcher::new(Arc::new(PluginManager::new(pool, false)));
    let seen = Arc::new(Mutex::new(Vec::new()));
    for (id, priority) in [("core/last", 30), ("core/first", 10)] {
        let seen = seen.clone();
        dispatcher
            .register_handler(id, HookEvent::PageAfterSave, priority, move |env| {
                let seen = seen.clone();
                Box::pin(async move {
                    seen.lock().unwrap().push((id, env));
                    Ok(())
                })
            })
            .unwrap();
    }
    assert!(
        dispatcher
            .register_handler("core/first", HookEvent::PageAfterSave, 0, |_| Box::pin(
                async { Ok(()) }
            ))
            .is_err()
    );
    dispatcher
        .register_handler("core/fail", HookEvent::PageAfterSave, 15, |_| {
            Box::pin(async { Err("failed".into()) })
        })
        .unwrap();
    dispatcher
        .register_handler("core/slow", HookEvent::PageAfterSave, 20, |_| {
            Box::pin(std::future::pending())
        })
        .unwrap();
    let report = dispatcher
        .dispatch(HookEvent::PageAfterSave, serde_json::json!({"saved": true}))
        .await;
    {
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen.iter().map(|v| v.0).collect::<Vec<_>>(),
            ["core/first", "core/last"]
        );
        assert_eq!(seen[0].1, seen[1].1);
    }
    assert!(report.envelope.occurred_at_ms > 0);
    assert_eq!(
        report.outcomes.iter().filter(|v| v.error.is_some()).count(),
        2
    );
    assert!(
        report
            .outcomes
            .iter()
            .any(|v| v.handler_id == "core/slow" && v.error.is_some())
    );
    let unsupported = dispatcher
        .dispatch(HookEvent::ContentBeforeSave, Value::Null)
        .await;
    assert!(unsupported.outcomes[0].error.is_some());
    let nested = IN_DISPATCH
        .scope(
            true,
            dispatcher.dispatch(HookEvent::PageAfterSave, Value::Null),
        )
        .await;
    assert!(nested.outcomes[0].error.is_some());
}

#[tokio::test]
async fn async_hook_queue_drains_for_maintenance_and_survives_drop() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("opendash-hooks-s03-{}-{nanos}", std::process::id()));
    let site_dir = root.join("site");
    let db_path = site_dir.join("database").join("opendash.sqlite");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let plugins_root = site_dir.join("plugins");
    let secrets_root = site_dir.join("database").join("secrets").join("plugins");
    std::fs::create_dir_all(&plugins_root).unwrap();
    std::fs::create_dir_all(&secrets_root).unwrap();

    let opts = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let coord = Arc::new(
        crate::operations::SiteCoordinator::acquire(crate::paths::SitePaths::from_parts(
            site_dir.clone(),
            db_path.clone(),
        ))
        .unwrap(),
    );
    let pm = Arc::new(PluginManager::with_roots(
        pool.clone(),
        false,
        plugins_root,
        secrets_root,
    ));
    let dispatcher = HookDispatcher::with_async_queue(
        Arc::clone(&pm),
        coord.maintenance_cancel_rx(),
        coord.shutdown_rx(),
    );

    let build_zip = |manifest: &str, wasm: &[u8]| -> Vec<u8> {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let zip_opts = zip::write::SimpleFileOptions::default();
            zip.start_file("manifest.json", zip_opts).unwrap();
            zip.write_all(manifest.as_bytes()).unwrap();
            zip.start_file("plugin.wasm", zip_opts).unwrap();
            zip.write_all(wasm).unwrap();
            zip.finish().unwrap();
        }
        buf.into_inner()
    };

    let spin_wasm = wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32)
                (loop $l
                    br $l)
                i32.const 0)
        )"#,
    )
    .unwrap();
    let spin_manifest = serde_json::json!({
        "id": "spin-after",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "name": "Spin After",
        "hooks": ["content/after_save"],
        "permissions": { "content_read": ["post"] }
    })
    .to_string();
    pm.install_plugin_checked(&build_zip(&spin_manifest, &spin_wasm))
        .await
        .unwrap();

    let lease = coord.admit_mutation().unwrap();
    dispatcher.enqueue_async(AsyncHookJob {
        event: HookEvent::ContentAfterSave,
        principal: ExecutionPrincipal::Admin,
        locale: Some("vi".to_owned()),
        type_key: Some("post".to_owned()),
        payload: serde_json::json!({ "id": 1, "values": {} }),
        dispatch_depth: 0,
        lease,
    });

    tokio::time::sleep(Duration::from_millis(80)).await;
    let drain_started = Instant::now();
    let exclusive = tokio::time::timeout(
        Duration::from_secs(5),
        coord.begin_maintenance(
            crate::storage::operations::OperationKind::Backup,
            Duration::from_secs(5),
        ),
    )
    .await
    .expect("begin_maintenance must drain AsyncHookQueue within <= 5s")
    .expect("begin_maintenance succeeds");
    assert!(drain_started.elapsed() < Duration::from_secs(5));
    assert!(*coord.maintenance_cancel_rx().borrow());

    // Drop maintenance guard -> workers must stay alive and clear maintenance_cancel_rx.
    drop(exclusive);
    assert!(!*coord.maintenance_cancel_rx().borrow());

    pm.disable_plugin_cas("spin-after", None).await.unwrap();

    let kv_req = r#"{"op":"kv_set","key":"revived","value":"ok"}"#;
    let kv_out = "{}";
    let kv_esc: String = kv_req
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect();
    let out_esc: String = kv_out
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect();
    let kv_wasm = wat::parse_str(&format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $set_output (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{kv_esc}")
            (data (i32.const 1536) "{out_esc}")
            (func (export "opendash_run") (result i32)
                (drop (call $host_call (i32.const 1024) (i32.const {rlen}) (i32.const 2048) (i32.const 512)))
                (drop (call $set_output (i32.const 1536) (i32.const {olen})))
                i32.const 0)
        )"#,
        rlen = kv_req.len(),
        olen = kv_out.len()
    ))
    .unwrap();
    let kv_manifest = serde_json::json!({
        "id": "kv-revive-after",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "name": "KV Revive After",
        "hooks": ["content/after_save"],
        "permissions": { "content_read": ["post"], "sqlite": ["kv_write"] }
    })
    .to_string();
    pm.install_plugin_checked(&build_zip(&kv_manifest, &kv_wasm))
        .await
        .unwrap();

    let post_lease = coord.admit_mutation().unwrap();
    dispatcher.enqueue_async(AsyncHookJob {
        event: HookEvent::ContentAfterSave,
        principal: ExecutionPrincipal::Admin,
        locale: Some("vi".to_owned()),
        type_key: Some("post".to_owned()),
        payload: serde_json::json!({ "id": 2, "values": {} }),
        dispatch_depth: 0,
        lease: post_lease,
    });

    let deadline = Instant::now() + Duration::from_secs(6);
    let mut saw = None;
    while Instant::now() < deadline {
        if let Ok(Some(val)) =
            crate::storage::plugin_kv::get(&pool, "kv-revive-after", "revived").await
        {
            saw = Some(val);
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    if saw.is_none() {
        let logs: Vec<(String, String, String)> =
            sqlx::query_as("SELECT plugin_id, level, message FROM plugin_logs ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap_or_default();
        panic!("timed out waiting for kv-revive-after/revived; logs = {logs:?}");
    }
    assert_eq!(saw.as_deref(), Some("\"ok\""));

    coord.notify_shutdown();
    pool.close().await;
    drop(coord);
    let _ = std::fs::remove_dir_all(&root);
}
