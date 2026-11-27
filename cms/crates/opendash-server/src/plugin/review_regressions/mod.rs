use super::runtime;
use super::{
    manifest::{PluginManifest, PluginPermissions},
    runtime::{PluginError, PluginRuntime},
};
use serde_json::json;
use std::{io::Read, net::TcpListener, sync::atomic::Ordering, time::Duration};

async fn assert_no_late_write_with_sqlite_lock(reader_blocks_commit: bool) {
    use std::io::Write;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for _ in 0..3 {
            let (mut socket, _) = listener.accept().unwrap();
            std::thread::spawn(move || {
                let mut buf = [0; 4096];
                let _ = socket.read(&mut buf);
                std::thread::sleep(Duration::from_millis(1400));
                let _ = socket.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            });
        }
    });
    let file = std::env::temp_dir().join(format!(
        "opendash-review-commit-{}.sqlite",
        crate::auth::generate_random_token()
    ));
    let mut options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&file)
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(5));
    if reader_blocks_commit {
        options = options.journal_mode(sqlx::sqlite::SqliteJournalMode::Delete);
    }
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE plugin_kv(owner_kind TEXT NOT NULL DEFAULT 'plugin', plugin_id TEXT, key TEXT, value_json TEXT, updated_at TEXT, PRIMARY KEY(owner_kind,plugin_id,key))").execute(&pool).await.unwrap();
    let mut lock = pool.begin().await.unwrap();
    if reader_blocks_commit {
        // A reader in rollback-journal mode allows the INSERT but blocks COMMIT.
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM plugin_kv")
            .fetch_one(&mut *lock)
            .await
            .unwrap();
    } else {
        sqlx::query("INSERT INTO plugin_kv (plugin_id, key, value_json, updated_at) VALUES ('holder','lock','1','now')")
            .execute(&mut *lock)
            .await
            .unwrap();
    }
    let unlock = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5500)).await;
        lock.rollback().await.unwrap();
    });
    let req =
        json!({"op":"http_request","method":"GET","url":format!("{origin}/fast")}).to_string();
    let write = json!({"op":"kv_set","key":"late_commit","value":true}).to_string();
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
    (data (i32.const 0) "{}") (data (i32.const 1024) "{}") (data (i32.const 2048) "{{}}")
    (func (export "opendash_run")(result i32)
    (drop (call $call (i32.const 0)(i32.const {})(i32.const 4096)(i32.const 4096)))
    (drop (call $call (i32.const 0)(i32.const {})(i32.const 4096)(i32.const 4096)))
    (drop (call $call (i32.const 0)(i32.const {})(i32.const 4096)(i32.const 4096)))
    (drop (call $call (i32.const 1024)(i32.const {})(i32.const 4096)(i32.const 4096)))
    (drop (call $out (i32.const 2048)(i32.const 2))) (i32.const 0)))"#,
        esc(&req),
        esc(&write),
        req.len(),
        req.len(),
        req.len(),
        write.len()
    );
    let manifest = PluginManifest {
        hooks: vec![],
        id: "commit-probe".into(),
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
            http: vec![origin],
            content_read: vec![],
            network: vec![],
            content_write: vec![],
            ..Default::default()
        },
        filters: vec![],
        editor_panels: vec![],
        text_domain: None,
        settings_schema: vec![],
        ..Default::default()
    };
    let result = PluginRuntime::new()
        .run_plugin(
            wat::parse_str(wat).unwrap(),
            manifest,
            json!({}),
            pool.clone(),
            "review-commit".into(),
            true,
        )
        .await;
    assert!(
        matches!(result, Err(PluginError::Timeout)),
        "expected deadline, got {result:?}"
    );
    unlock.await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_kv WHERE key='late_commit'")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    let _ = std::fs::remove_file(file);
    assert_eq!(
        count, 0,
        "commit continued after invocation timeout despite cancellation flag"
    );
}

mod suite_01;
mod suite_02;
