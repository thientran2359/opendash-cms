use super::runtime;
use super::{
    manifest::{PluginManifest, PluginPermissions},
    runtime::{PluginError, PluginRuntime},
};
use serde_json::json;
use std::{io::Read, net::TcpListener, sync::atomic::Ordering, time::Duration};

#[tokio::test(flavor = "multi_thread")]
async fn invocation_timeout_must_not_commit_later() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for _ in 0..3 {
            let (mut socket, _) = listener.accept().unwrap();
            std::thread::spawn(move || {
                let mut buf = [0; 4096];
                let _ = socket.read(&mut buf);
                std::thread::sleep(Duration::from_secs(3));
            });
        }
    });
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("CREATE TABLE plugin_kv(plugin_id TEXT, key TEXT, value_json TEXT, updated_at TEXT, PRIMARY KEY(plugin_id,key))")
        .execute(&pool).await.unwrap();
    let req =
        json!({"op":"http_request","method":"GET","url":format!("{origin}/slow")}).to_string();
    let write = json!({"op":"kv_set","key":"late","value":true}).to_string();
    let escape = |s: &str| {
        s.as_bytes()
            .iter()
            .map(|b| format!("\\{b:02x}"))
            .collect::<String>()
    };
    let wat = format!(
        r#"(module
      (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
      (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 0) "{}")
      (data (i32.const 1024) "{}")
      (data (i32.const 2048) "{{}}")
      (func (export "opendash_run") (result i32)
       (drop (call $call (i32.const 0) (i32.const {}) (i32.const 4096) (i32.const 4096)))
       (drop (call $call (i32.const 0) (i32.const {}) (i32.const 4096) (i32.const 4096)))
       (drop (call $call (i32.const 0) (i32.const {}) (i32.const 4096) (i32.const 4096)))
       (drop (call $call (i32.const 1024) (i32.const {}) (i32.const 4096) (i32.const 4096)))
       (drop (call $out (i32.const 2048) (i32.const 2))) (i32.const 0)))"#,
        escape(&req),
        escape(&write),
        req.len(),
        req.len(),
        req.len(),
        write.len()
    );
    let manifest = PluginManifest {
        hooks: vec![],
        id: "late-probe".into(),
        version: "1.0.0".into(),
        api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
        name: "Probe".into(),
        description: String::new(),
        entrypoint: "plugin.wasm".into(),
        permissions: PluginPermissions {
            sqlite: vec!["kv_write".into()],
            http: vec![origin],
        },
    };
    let result = PluginRuntime::new()
        .run_plugin(
            wat::parse_str(wat).unwrap(),
            manifest,
            json!({}),
            pool.clone(),
            "review-timeout".into(),
            true,
        )
        .await;
    assert!(
        matches!(result, Err(PluginError::Timeout)),
        "expected timeout, got {result:?}"
    );
    tokio::time::sleep(Duration::from_secs(3)).await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM plugin_kv WHERE key='late'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "worker committed AFTER caller had already received timeout"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn timeout_during_commit_must_not_write_later() {
    assert_no_late_write_with_sqlite_lock(false).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn timeout_after_sqlite_receives_commit_must_not_write_later() {
    assert_no_late_write_with_sqlite_lock(true).await;
}

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
    sqlx::query("CREATE TABLE plugin_kv(plugin_id TEXT, key TEXT, value_json TEXT, updated_at TEXT, PRIMARY KEY(plugin_id,key))").execute(&pool).await.unwrap();
    let mut lock = pool.begin().await.unwrap();
    if reader_blocks_commit {
        // A reader in rollback-journal mode allows the INSERT but blocks COMMIT.
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM plugin_kv")
            .fetch_one(&mut *lock)
            .await
            .unwrap();
    } else {
        sqlx::query("INSERT INTO plugin_kv VALUES ('holder','lock','1','now')")
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
        name: "Probe".into(),
        description: String::new(),
        entrypoint: "plugin.wasm".into(),
        permissions: PluginPermissions {
            sqlite: vec!["kv_write".into()],
            http: vec![origin],
        },
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

#[tokio::test(flavor = "multi_thread")]
async fn timeout_must_not_be_reported_if_commit_succeeded() {
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
        "opendash-review-commit-success-{}.sqlite",
        crate::auth::generate_random_token()
    ));
    // Rollback-journal Delete mode so readers block COMMIT specifically
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&file)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Delete)
        .busy_timeout(Duration::from_secs(5));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE plugin_kv(plugin_id TEXT, key TEXT, value_json TEXT, updated_at TEXT, PRIMARY KEY(plugin_id,key))")
        .execute(&pool)
        .await
        .unwrap();

    // Lock 1: Writer holds lock, blocking INSERT from ~4200ms to 4600ms (400ms wait)
    let mut writer_lock = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO plugin_kv VALUES ('holder','wlock','1','now')")
        .execute(&mut *writer_lock)
        .await
        .unwrap();

    // Lock 2: Reader holds SHARED lock, blocking COMMIT from 4600ms to 5050ms (450ms wait)
    let mut reader_lock = pool.begin().await.unwrap();
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM plugin_kv")
        .fetch_one(&mut *reader_lock)
        .await
        .unwrap();

    let unlock = tokio::spawn(async move {
        // Unblock INSERT at 4600ms
        tokio::time::sleep(Duration::from_millis(4600)).await;
        let _ = writer_lock.rollback().await;

        // Unblock COMMIT at 5050ms (past the 5000ms deadline)
        tokio::time::sleep(Duration::from_millis(450)).await;
        let _ = reader_lock.rollback().await;
    });

    let req =
        json!({"op":"http_request","method":"GET","url":format!("{origin}/fast")}).to_string();
    let write = json!({"op":"kv_set","key":"written_key","value":true}).to_string();
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
        id: "commit-success-probe".into(),
        version: "1.0.0".into(),
        api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
        name: "Probe".into(),
        description: String::new(),
        entrypoint: "plugin.wasm".into(),
        permissions: PluginPermissions {
            sqlite: vec!["kv_write".into()],
            http: vec![origin],
        },
    };

    let result = PluginRuntime::new()
        .run_plugin(
            wat::parse_str(wat).unwrap(),
            manifest,
            json!({}),
            pool.clone(),
            "review-commit-success".into(),
            true,
        )
        .await;

    let _ = unlock.await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_kv WHERE key='written_key'")
        .fetch_one(&pool)
        .await
        .unwrap();

    pool.close().await;
    let _ = std::fs::remove_file(file);

    // INVARIANT: API outcome MUST match database truth!
    // If result is Timeout, count MUST be 0 (no late write).
    // If count is 1 (committed), result MUST NOT be Timeout!
    if matches!(result, Err(PluginError::Timeout)) {
        assert_eq!(
            count, 0,
            "API reported Timeout for a transaction that actually COMMITTED to SQLite! (count={count})"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn ipc_kv_get_stuck_parent_must_cancel_and_clean_up() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("CREATE TABLE plugin_kv(plugin_id TEXT, key TEXT, value_json TEXT, updated_at TEXT, PRIMARY KEY(plugin_id,key))")
        .execute(&pool)
        .await
        .unwrap();

    // Acquire the only connection in the pool so any subsequent query must wait for acquire
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
        id: "ipc-hang-probe".into(),
        version: "1.0.0".into(),
        api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
        name: "Probe".into(),
        description: String::new(),
        entrypoint: "plugin.wasm".into(),
        permissions: PluginPermissions {
            sqlite: vec!["kv_read".into()],
            http: vec![],
        },
    };

    let runtime = PluginRuntime::new();
    let wasm = wat::parse_str(wat).unwrap();

    let rt = runtime.clone();
    let p = pool.clone();
    let m = manifest.clone();
    let task = tokio::spawn(async move {
        rt.run_plugin(wasm, m, json!({}), p, "test-ipc-hang".into(), false)
            .await
    });

    // Deterministically wait until parent task has entered kv_get and is waiting on pool/SQLite
    while !runtime::TEST_IPC_KV_GET_WAITING.load(Ordering::SeqCst) {
        tokio::task::yield_now().await;
    }

    // Caller cancels invocation
    task.abort();
    let _ = task.await;

    // Check that permit was only returned after cleanup, and all 4 slots are available
    assert_eq!(
        runtime.active_workers(),
        0,
        "active_workers must be 0 after cancel"
    );
    assert_eq!(
        runtime.semaphore().available_permits(),
        4,
        "all 4 permits must be recovered"
    );

    // Drop holder to free the connection
    drop(holder);

    // If parent task was detached and still waiting on pool, it would now acquire connection and keep pool busy.
    // pool.close() must complete cleanly without hanging.
    let close_res = tokio::time::timeout(Duration::from_millis(500), pool.close()).await;
    assert!(
        close_res.is_ok(),
        "pool.close() hung because detached parent worker_task grabbed connection after cancel!"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn abort_during_staged_commit_must_cleanup_connection_and_pool() {
    let temp_db = std::env::temp_dir().join(format!("staged_abort_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&temp_db);
    let db_url = format!(
        "sqlite://{}?mode=rwc",
        temp_db.to_str().unwrap().replace('\\', "/")
    );
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE plugin_kv(plugin_id TEXT, key TEXT, value_json TEXT, updated_at TEXT, PRIMARY KEY(plugin_id,key))")
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
        name: "Probe".into(),
        description: String::new(),
        entrypoint: "plugin.wasm".into(),
        permissions: PluginPermissions {
            sqlite: vec!["kv_write".into()],
            http: vec![],
        },
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
    while !runtime::TEST_STAGE_WRITE_BARRIER_HIT.load(Ordering::SeqCst) {
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
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("CREATE TABLE plugin_kv(plugin_id TEXT, key TEXT, value_json TEXT, updated_at TEXT, PRIMARY KEY(plugin_id,key))")
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
        name: "Probe".into(),
        description: String::new(),
        entrypoint: "plugin.wasm".into(),
        permissions: PluginPermissions {
            sqlite: vec!["kv_read".into()],
            http: vec![],
        },
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
