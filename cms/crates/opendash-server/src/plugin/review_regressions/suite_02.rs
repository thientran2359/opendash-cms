use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn abort_during_staged_commit_must_cleanup_connection_and_pool() {
    let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
    let temp_db = std::env::temp_dir().join(format!(
        "staged_abort_{}.sqlite",
        crate::auth::generate_random_token()
    ));
    let _ = std::fs::remove_file(&temp_db);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&temp_db)
                .create_if_missing(true),
        )
        .await
        .unwrap();
    sqlx::query("CREATE TABLE plugin_kv(owner_kind TEXT NOT NULL DEFAULT 'plugin', plugin_id TEXT, key TEXT, value_json TEXT, updated_at TEXT, PRIMARY KEY(owner_kind,plugin_id,key))")
        .execute(&pool)
        .await
        .unwrap();

    let initial_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&pool)
        .await
        .unwrap();

    let req = json!({"op":"kv_set","key":"staged_key","value":{"hello":"world"}}).to_string();
    let esc = |s: &str| {
        s.as_bytes()
            .iter()
            .map(|b| format!("\\{b:02x}"))
            .collect::<String>()
    };
    let wat = format!(
        r#"(module
    (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32)(result i32)))
    (import "opendash_v1" "host_write_output" (func $out (param i32 i32)(result i32)))
    (memory (export "memory") 1)
    (data (i32.const 0) "{}")
    (data (i32.const 2048) "{{\"ok\":true}}")
    (func (export "opendash_run")(result i32)
    (drop (call $call (i32.const 0)(i32.const {})(i32.const 4096)(i32.const 4096)))
    (drop (call $out (i32.const 2048)(i32.const 11)))
    (i32.const 0)))"#,
        esc(&req),
        req.len()
    );

    let manifest = PluginManifest {
        hooks: vec![],
        id: "staged-abort-probe".into(),
        version: "1.0.0".into(),
        api_version: 1,
        manifest_schema: 1,
        min_host_version: None,
        declarations: Default::default(),
        hook_priority: 0,
        actions: vec![],
        name: "Probe".into(),
        description: String::new(),
        entrypoint: "plugin.wasm".into(),
        permissions: PluginPermissions {
            sqlite: vec!["kv_write".into()],
            http: vec![],
            content_read: vec![],
            network: vec![],
            content_write: vec![],
            ..Default::default()
        },
        filters: vec![],
        editor_panels: vec![],
        settings_schema: vec![],
        ..Default::default()
    };

    let runtime = PluginRuntime::new();
    let wasm = wat::parse_str(wat).unwrap();

    // Enable staged write pause barrier
    runtime::TEST_STAGE_WRITE_BARRIER_HIT.store(false, Ordering::SeqCst);
    runtime::TEST_STAGE_WRITE_PAUSE.store(true, Ordering::SeqCst);

    let rt = runtime.clone();
    let p = pool.clone();
    let task = tokio::spawn(async move {
        rt.run_plugin(
            wasm,
            manifest,
            json!({}),
            p,
            "test-staged-abort".into(),
            false,
        )
        .await
    });

    // Deterministically wait until BEGIN IMMEDIATE and INSERT have executed
    let barrier_deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !runtime::TEST_STAGE_WRITE_BARRIER_HIT.load(Ordering::SeqCst) {
        if task.is_finished() || std::time::Instant::now() >= barrier_deadline {
            runtime::TEST_STAGE_WRITE_PAUSE.store(false, Ordering::SeqCst);
            let res = task.await;
            panic!("staged write barrier not hit before task finished/timed out: {res:?}");
        }
        tokio::task::yield_now().await;
    }

    // Caller aborts during staged write, before COMMIT is sent
    task.abort();
    let _ = task.await;

    // Turn off pause
    runtime::TEST_STAGE_WRITE_PAUSE.store(false, Ordering::SeqCst);

    // Reacquire connection from the 1-connection pool.
    // If the connection was returned dirty to the pool with an open transaction,
    // trying to BEGIN IMMEDIATE on this reacquired connection will FAIL!
    let mut conn = pool.acquire().await.expect("pool acquire must succeed");

    let begin_res = sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await;
    assert!(
        begin_res.is_ok(),
        "Connection returned to pool had open transaction! BEGIN failed: {:?}",
        begin_res.err()
    );
    sqlx::query("ROLLBACK").execute(&mut *conn).await.unwrap();

    // Check busy_timeout was not leaked
    let current_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(
        current_timeout, initial_timeout,
        "PRAGMA busy_timeout leaked to pool connection!"
    );

    // Check no uncommitted/staged row was committed
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_kv WHERE key = 'staged_key'")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(count, 0, "Staged row must NOT be committed on abort!");

    // Check permit recovery
    assert_eq!(runtime.active_workers(), 0);
    assert_eq!(runtime.semaphore().available_permits(), 4);

    drop(conn);
    pool.close().await;
    let _ = std::fs::remove_file(&temp_db);
}

#[tokio::test(flavor = "multi_thread")]
async fn ipc_kv_get_stuck_parent_must_timeout_and_clean_up() {
    let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("CREATE TABLE plugin_kv(owner_kind TEXT NOT NULL DEFAULT 'plugin', plugin_id TEXT, key TEXT, value_json TEXT, updated_at TEXT, PRIMARY KEY(owner_kind,plugin_id,key))")
        .execute(&pool)
        .await
        .unwrap();

    // Acquire the only connection in the pool so kv_get must wait for acquire
    let holder = pool.acquire().await.unwrap();

    let req = json!({"op":"kv_get","key":"hang_key"}).to_string();
    let esc = |s: &str| {
        s.as_bytes()
            .iter()
            .map(|b| format!("\\{b:02x}"))
            .collect::<String>()
    };
    let wat = format!(
        r#"(module
    (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32)(result i32)))
    (import "opendash_v1" "host_write_output" (func $out (param i32 i32)(result i32)))
    (memory (export "memory") 1)
    (data (i32.const 0) "{}")
    (data (i32.const 2048) "{{}}")
    (func (export "opendash_run")(result i32)
    (drop (call $call (i32.const 0)(i32.const {})(i32.const 4096)(i32.const 4096)))
    (drop (call $out (i32.const 2048)(i32.const 2)))
    (i32.const 0)))"#,
        esc(&req),
        req.len()
    );

    let manifest = PluginManifest {
        hooks: vec![],
        id: "ipc-timeout-probe".into(),
        version: "1.0.0".into(),
        api_version: 1,
        manifest_schema: 1,
        min_host_version: None,
        declarations: Default::default(),
        hook_priority: 0,
        actions: vec![],
        name: "Probe".into(),
        description: String::new(),
        entrypoint: "plugin.wasm".into(),
        permissions: PluginPermissions {
            sqlite: vec!["kv_read".into()],
            http: vec![],
            content_read: vec![],
            network: vec![],
            content_write: vec![],
            ..Default::default()
        },
        filters: vec![],
        editor_panels: vec![],
        settings_schema: vec![],
        ..Default::default()
    };

    let runtime = PluginRuntime::new();
    let wasm = wat::parse_str(wat).unwrap();

    runtime::TEST_IPC_KV_GET_WAITING.store(false, Ordering::SeqCst);

    let rt = runtime.clone();
    let p = pool.clone();
    let m = manifest.clone();
    let task = tokio::spawn(async move {
        rt.run_plugin(wasm, m, json!({}), p, "test-ipc-timeout".into(), false)
            .await
    });

    // Deterministically wait until parent task is actually waiting on pool
    while !runtime::TEST_IPC_KV_GET_WAITING.load(Ordering::SeqCst) {
        tokio::task::yield_now().await;
    }

    // Do NOT abort caller - let deadline expire (5s)
    let res = task.await.unwrap();
    assert!(
        matches!(res, Err(PluginError::Timeout)),
        "Expected PluginError::Timeout on expired deadline, got {:?}",
        res
    );

    // Verify parent & child cleanup and permit recovery
    assert_eq!(runtime.active_workers(), 0);
    assert_eq!(runtime.semaphore().available_permits(), 4);

    // Release holder and confirm pool is usable
    drop(holder);
    let mut check_conn = pool.acquire().await.unwrap();
    let val: i64 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&mut *check_conn)
        .await
        .unwrap();
    assert_eq!(val, 1);
}
