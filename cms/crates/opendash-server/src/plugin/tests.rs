#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use sqlx::SqlitePool;

    use crate::plugin::{
        PluginManager,
        manifest::{PluginManifest, PluginPermissions},
        package::PluginPackage,
        runtime::{PluginError, PluginRuntime},
    };

    async fn create_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to connect to in-memory SQLite");

        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        pool
    }

    #[test]
    fn test_worker_entry() {
        if std::env::var("OPENDASH_WORKER_CHILD").is_ok() {
            crate::plugin::worker::run_worker_from_stdio();
            std::process::exit(0);
        }
    }

    #[test]
    fn test_manifest_validation() {
        let valid_json = r#"{
            "id": "my-plugin",
            "version": "1.0.0",
            "api_version": 1,
            "name": "My Plugin",
            "description": "Test plugin",
            "entrypoint": "plugin.wasm",
            "permissions": {
                "sqlite": ["kv_read", "kv_write"],
                "http": ["http://localhost:8080/test"]
            }
        }"#;
        let manifest = PluginManifest::from_json_str(valid_json).expect("Manifest hợp lệ");
        assert_eq!(manifest.id, "my-plugin");
        assert!(manifest.has_sqlite_perm("kv_read"));
        assert!(manifest.has_sqlite_perm("kv_write"));
        assert!(manifest.is_http_allowed("http://localhost:8080/test/subpath"));
        assert!(!manifest.is_http_allowed("http://attacker.com/steal"));

        // Invalid api_version
        let invalid_api = valid_json.replace("\"api_version\": 1", "\"api_version\": 2");
        assert!(PluginManifest::from_json_str(&invalid_api).is_err());

        // Invalid ID (uppercase)
        let invalid_id = valid_json.replace("\"my-plugin\"", "\"My-Plugin\"");
        assert!(PluginManifest::from_json_str(&invalid_id).is_err());

        // Invalid sqlite permission
        let invalid_perm = valid_json.replace("\"kv_read\"", "\"raw_sql\"");
        assert!(PluginManifest::from_json_str(&invalid_perm).is_err());
    }

    #[test]
    fn test_package_validation_sample_zip() {
        let runtime = PluginRuntime::new();
        let zip_bytes =
            include_bytes!("../../../../examples/plugins/sample-plugin/sample-plugin.zip");
        let pkg =
            PluginPackage::from_zip(zip_bytes, runtime.engine()).expect("Sample ZIP phải hợp lệ");
        assert_eq!(pkg.manifest.id, "sample-plugin");
        assert!(!pkg.wasm_bytes.is_empty());
    }

    #[tokio::test]
    async fn test_sample_plugin_full_execution() {
        let db = create_test_db().await;
        let manager = PluginManager::new(db.clone(), true);
        let zip_bytes =
            include_bytes!("../../../../examples/plugins/sample-plugin/sample-plugin.zip");

        // 1. Install
        let summary = manager
            .install_plugin(zip_bytes)
            .await
            .expect("Cài đặt plugin thành công");
        assert_eq!(summary.id, "sample-plugin");
        assert!(summary.enabled);

        // 2. Run first time
        let input1 = json!({ "text": "hello opendash" });
        let res1 = manager
            .run_plugin("sample-plugin", input1)
            .await
            .expect("Run 1 thành công");
        assert!(res1.success);
        let data1 = res1.data.unwrap();
        assert_eq!(data1["status"], "ok");
        assert_eq!(data1["transformed"], "HELLO OPENDASH");
        assert_eq!(data1["run_count"], 1);

        // Verify KV in DB
        let val1: String = sqlx::query_scalar("SELECT value_json FROM plugin_kv WHERE plugin_id = 'sample-plugin' AND key = 'run_count'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(val1, "1");

        // 3. Run second time
        let input2 = json!({ "text": "second invocation" });
        let res2 = manager
            .run_plugin("sample-plugin", input2)
            .await
            .expect("Run 2 thành công");
        assert!(res2.success);
        let data2 = res2.data.unwrap();
        assert_eq!(data2["transformed"], "SECOND INVOCATION");
        assert_eq!(data2["run_count"], 2);

        // Verify KV in DB updated
        let val2: String = sqlx::query_scalar("SELECT value_json FROM plugin_kv WHERE plugin_id = 'sample-plugin' AND key = 'run_count'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(val2, "2");
    }

    #[tokio::test]
    async fn test_fuel_exhaustion_infinite_loop() {
        let db = create_test_db().await;
        let runtime = PluginRuntime::new();

        let wat_src = r#"
            (module
                (memory (export "memory") 1)
                (func (export "opendash_run") (result i32)
                    (loop $forever
                        (drop (i32.const 1))
                        (drop (i32.const 2))
                        (drop (i32.const 3))
                        (drop (i32.const 4))
                        (drop (i32.const 5))
                        (drop (i32.const 6))
                        (drop (i32.const 7))
                        (drop (i32.const 8))
                        (drop (i32.const 9))
                        (drop (i32.const 10))
                        (br $forever)
                    )
                    (i32.const 0)
                )
            )
        "#;
        let wasm_bytes = wat::parse_str(wat_src).unwrap();
        let manifest = PluginManifest {
            id: "loop-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            name: "Loop Plugin".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
        };

        let start = std::time::Instant::now();
        let res = runtime
            .run_plugin(
                wasm_bytes,
                manifest,
                json!({}),
                db,
                "corr-loop".into(),
                false,
            )
            .await;
        let elapsed = start.elapsed();

        // Must fail with OutOfFuel
        match res {
            Err(PluginError::OutOfFuel) => {}
            other => panic!("Expected OutOfFuel error, got: {other:?}"),
        }
        // Must fail within fuel limits before the 5s deadline
        assert!(
            elapsed < Duration::from_secs(5),
            "Infinite loop did not terminate quickly: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_zero_late_writes_on_guest_trap() {
        let db = create_test_db().await;
        let runtime = PluginRuntime::new();

        let wat_src = r#"
            (module
                (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "{\"op\":\"kv_set\",\"key\":\"trap_key\",\"value\":\"trap_val\"}")
                (func (export "opendash_run") (result i32)
                    ;; Stage KV write
                    (call $host_call (i32.const 0) (i32.const 48) (i32.const 100) (i32.const 100))
                    (drop)
                    ;; Now intentionally trigger trap
                    (unreachable)
                )
            )
        "#;
        let wasm_bytes = wat::parse_str(wat_src).unwrap();
        let manifest = PluginManifest {
            id: "trap-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            name: "Trap Plugin".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions {
                sqlite: vec!["kv_write".into()],
                http: vec![],
            },
        };

        let res = runtime
            .run_plugin(
                wasm_bytes,
                manifest,
                json!({}),
                db.clone(),
                "corr-trap".into(),
                false,
            )
            .await;

        match res {
            Err(PluginError::Trap(_)) => {}
            other => panic!("Expected Trap error, got: {other:?}"),
        }

        // Verify zero late writes in DB
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM plugin_kv WHERE key = 'trap_key'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(
            count, 0,
            "Dữ liệu KV bị ghi muộn vào database sau khi guest trap!"
        );
    }

    #[tokio::test]
    async fn test_permission_denial_kv_and_loopback() {
        let db = create_test_db().await;
        let runtime = PluginRuntime::new();

        // 1. Module attempts kv_write without manifest permission
        let wat_src = r#"
            (module
                (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
                (import "opendash_v1" "host_write_output" (func $host_write_output (param i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "{\"op\":\"kv_set\",\"key\":\"forbidden\",\"value\":\"val\"}")
                (func (export "opendash_run") (result i32)
                    (local $len i32)
                    (local.set $len (call $host_call (i32.const 0) (i32.const 47) (i32.const 100) (i32.const 200)))
                    (call $host_write_output (i32.const 100) (local.get $len))
                    (drop)
                    (i32.const 0)
                )
            )
        "#;
        let wasm_bytes = wat::parse_str(wat_src).unwrap();
        // Manifest has NO sqlite permissions
        let manifest = PluginManifest {
            id: "no-perm-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            name: "No Perm".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
        };

        let res = runtime
            .run_plugin(
                wasm_bytes,
                manifest,
                json!({}),
                db.clone(),
                "corr-noperm".into(),
                false,
            )
            .await
            .expect("Guest code executes to return response JSON");

        // The response written to output by guest should show permission_denied error
        assert_eq!(res["ok"], false);
        assert!(res["error"].as_str().unwrap().contains("permission_denied"));

        // Verify nothing in DB
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM plugin_kv WHERE key = 'forbidden'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_linear_memory_limit() {
        let db = create_test_db().await;
        let runtime = PluginRuntime::new();

        // 300 pages * 64KB = 19.2MB, which exceeds 16MB limit
        let wat_src = r#"
            (module
                (import "opendash_v1" "host_write_output" (func $host_write_output (param i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "{\"status\":\"ok\"}")
                (func (export "opendash_run") (result i32)
                    (local $grow_res i32)
                    (local.set $grow_res (memory.grow (i32.const 300)))
                    ;; If memory.grow exceeded limit, it must return -1
                    (if (i32.eq (local.get $grow_res) (i32.const -1))
                        (then
                            (call $host_write_output (i32.const 0) (i32.const 15))
                            (drop)
                            (return (i32.const 0))
                        )
                    )
                    (i32.const 1)
                )
            )
        "#;
        let wasm_bytes = wat::parse_str(wat_src).unwrap();
        let manifest = PluginManifest {
            id: "mem-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            name: "Mem Plugin".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
        };

        let res = runtime
            .run_plugin(
                wasm_bytes,
                manifest,
                json!({}),
                db,
                "corr-mem".into(),
                false,
            )
            .await
            .expect("Guest should handle memory limit gracefully without host crash");
        assert_eq!(res["status"], "ok");
    }

    #[tokio::test]
    async fn test_slow_http_timeout_and_cleanup() {
        use tokio::io::AsyncReadExt;

        // Bind dummy slow HTTP server on loopback
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let target_url = format!("http://{addr}/delay");

        // Spawn background task that accepts connection and delays for 5 seconds without response
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });

        let db = create_test_db().await;
        let runtime = PluginRuntime::new();

        let req_json =
            format!("{{\"op\":\"http_request\",\"method\":\"GET\",\"url\":\"{target_url}\"}}");
        let req_bytes = req_json.into_bytes();
        let req_len = req_bytes.len() as i32;

        let escaped_data = std::str::from_utf8(&req_bytes)
            .unwrap()
            .replace('"', "\\\"");
        let wat_src = format!(
            r#"
            (module
                (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
                (import "opendash_v1" "host_write_output" (func $host_write_output (param i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "{escaped_data}")
                (func (export "opendash_run") (result i32)
                    (local $len i32)
                    (local.set $len (call $host_call (i32.const 0) (i32.const {req_len}) (i32.const 500) (i32.const 500)))
                    (call $host_write_output (i32.const 500) (local.get $len))
                    (drop)
                    (i32.const 0)
                )
            )
        "#,
            escaped_data = escaped_data,
            req_len = req_len
        );

        let wasm_bytes = wat::parse_str(&wat_src).unwrap();
        let manifest = PluginManifest {
            id: "http-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            name: "HTTP Plugin".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions {
                sqlite: vec![],
                http: vec![target_url.clone()],
            },
        };

        let start = std::time::Instant::now();
        let res = runtime
            .run_plugin(
                wasm_bytes,
                manifest,
                json!({}),
                db,
                "corr-http".into(),
                true,
            )
            .await
            .expect("Guest should finish after timeout error from host");
        let elapsed = start.elapsed();

        // Must report error from host_call due to HTTP timeout (2s)
        assert_eq!(res["ok"], false);
        assert!(res["error"].as_str().unwrap().contains("Lỗi kết nối HTTP"));
        // Timeout should take ~2 seconds, well under the 5s deadline
        assert!(
            elapsed >= Duration::from_secs(2),
            "Timeout did not wait for 2s HTTP timeout: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(4),
            "Timeout exceeded expected duration: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_sqlite_lock_timeout_and_zero_late_writes() {
        let db_dir =
            std::env::temp_dir().join(format!("opendash_test_lock_{}", std::process::id()));
        std::fs::create_dir_all(&db_dir).unwrap();
        let db_file = db_dir.join("test.sqlite");

        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&db_file)
            .create_if_missing(true)
            .busy_timeout(Duration::from_millis(500));
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .unwrap();

        // Run migrations
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS plugins (
                id TEXT PRIMARY KEY,
                version TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                manifest_json TEXT NOT NULL,
                wasm_blob BLOB NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                last_error TEXT,
                installed_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS plugin_kv (
                plugin_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (plugin_id, key)
            );",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS plugin_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                plugin_id TEXT NOT NULL,
                correlation_id TEXT NOT NULL,
                level TEXT NOT NULL,
                message TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .execute(&pool)
        .await
        .unwrap();

        let manager = PluginManager::new(pool.clone(), true);
        let zip_bytes =
            include_bytes!("../../../../examples/plugins/sample-plugin/sample-plugin.zip");
        manager.install_plugin(zip_bytes).await.unwrap();

        // Connection 1 acquires exclusive write lock
        let mut lock_tx = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO plugin_kv (plugin_id, key, value_json) VALUES ('holder', 'held', '1')",
        )
        .execute(&mut *lock_tx)
        .await
        .unwrap();

        // Attempt invocation while lock is held
        let res = manager
            .run_plugin("sample-plugin", json!({ "text": "lock test" }))
            .await;

        // Invocation commit must fail
        assert!(
            res.is_err(),
            "Plugin run should have failed due to database lock!"
        );
        let err_resp = res.unwrap_err().1;
        assert!(!err_resp.success);

        // Now release lock on connection 1
        lock_tx.rollback().await.unwrap();

        // Verify zero late writes: 'run_count' must NOT exist in DB
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM plugin_kv WHERE key = 'run_count'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            count, 0,
            "Dữ liệu bị ghi muộn vào database sau SQLite lock!"
        );

        // Verify that subsequent invocation succeeds normally now that lock is released
        let retry_res = manager
            .run_plugin("sample-plugin", json!({ "text": "after lock" }))
            .await;
        assert!(
            retry_res.is_ok(),
            "Plugin run failed to recover after lock was released!"
        );

        // Clean up temp file
        let _ = std::fs::remove_dir_all(&db_dir);
    }

    fn build_test_zip(wasm_bytes: &[u8], plugin_id: &str) -> Vec<u8> {
        use std::io::Write;
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cur);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("manifest.json", options).unwrap();
            let manifest_str = format!(
                r#"{{
                    "id": "{}",
                    "version": "1.0.0",
                    "api_version": 1,
                    "name": "Test Plugin",
                    "entrypoint": "plugin.wasm"
                }}"#,
                plugin_id
            );
            zip.write_all(manifest_str.as_bytes()).unwrap();

            zip.start_file("plugin.wasm", options).unwrap();
            zip.write_all(wasm_bytes).unwrap();
            zip.finish().unwrap();
        }
        cur.into_inner()
    }

    #[tokio::test]
    async fn test_complex_json_input_and_escaping() {
        let db = create_test_db().await;
        let manager = PluginManager::new(db.clone(), true);
        let zip_bytes =
            include_bytes!("../../../../examples/plugins/sample-plugin/sample-plugin.zip");
        manager.install_plugin(zip_bytes).await.unwrap();

        // Complex JSON input containing escaped quotes, newlines, backslashes and unicode
        let complex_text = "Hello \"World\"\nLine 2\\Backslash \u{0041}";
        let input = json!({
            "text": complex_text,
            "nested": { "key": [1, 2, 3] },
            "flag": true
        });

        let res = manager
            .run_plugin("sample-plugin", input)
            .await
            .expect("Sample plugin should parse complex JSON without 400 invalid_output");

        assert!(res.success);
        let data = res.data.unwrap();
        assert_eq!(data["status"], "ok");
        assert_eq!(data["transformed"], complex_text.to_uppercase());
        assert_eq!(data["run_count"], 1);
    }

    #[test]
    fn test_manifest_semver_validation() {
        use crate::plugin::manifest::validate_semver;

        // Valid SemVer
        assert!(validate_semver("1.0.0"));
        assert!(validate_semver("0.1.0-alpha.1"));
        assert!(validate_semver("2.0.0+build.123"));
        assert!(validate_semver("1.2.3-rc.1+build.2026"));

        // Invalid SemVer
        assert!(!validate_semver("1"));
        assert!(!validate_semver("1.0"));
        assert!(!validate_semver("01.0.0"));
        assert!(!validate_semver("v1.0.0"));
        assert!(!validate_semver(""));
        assert!(!validate_semver("1.0.0.0"));
        assert!(!validate_semver("1.0.0-"));
        assert!(!validate_semver("1.0.0+"));
    }

    #[test]
    fn test_wasm_import_and_export_rejection() {
        let runtime = PluginRuntime::new();

        // 1. Missing memory export
        let wat_no_mem = r#"(module (func (export "opendash_run") (result i32) (i32.const 0)))"#;
        let wasm_no_mem = wat::parse_str(wat_no_mem).unwrap();
        let res_no_mem = PluginPackage::from_zip(
            &build_test_zip(&wasm_no_mem, "test-no-mem"),
            runtime.engine(),
        );
        assert!(res_no_mem.is_err());
        assert!(res_no_mem.unwrap_err().contains("memory"));

        // 2. Wrong signature for opendash_run (takes param)
        let wat_bad_sig = r#"(module (memory (export "memory") 1) (func (export "opendash_run") (param i32) (result i32) (i32.const 0)))"#;
        let wasm_bad_sig = wat::parse_str(wat_bad_sig).unwrap();
        let res_bad_sig = PluginPackage::from_zip(
            &build_test_zip(&wasm_bad_sig, "test-bad-sig"),
            runtime.engine(),
        );
        assert!(res_bad_sig.is_err());
        assert!(res_bad_sig.unwrap_err().contains("signature"));

        // 3. Unknown namespace import (e.g. WASI)
        let wat_wasi = r#"(module
            (import "wasi_snapshot_preview1" "fd_write" (func $wasi (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32) (i32.const 0))
        )"#;
        let wasm_wasi = wat::parse_str(wat_wasi).unwrap();
        let res_wasi =
            PluginPackage::from_zip(&build_test_zip(&wasm_wasi, "test-wasi"), runtime.engine());
        assert!(res_wasi.is_err());
        assert!(res_wasi.unwrap_err().contains("không được phép"));

        // 4. Unknown function in opendash_v1
        let wat_unknown_fn = r#"(module
            (import "opendash_v1" "unknown_fn" (func $unk (result i32)))
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32) (i32.const 0))
        )"#;
        let wasm_unknown_fn = wat::parse_str(wat_unknown_fn).unwrap();
        let res_unknown_fn = PluginPackage::from_zip(
            &build_test_zip(&wasm_unknown_fn, "test-unk"),
            runtime.engine(),
        );
        assert!(res_unknown_fn.is_err());
    }

    #[tokio::test]
    async fn test_env_abort_traps_execution() {
        let db = create_test_db().await;
        let runtime = PluginRuntime::new();

        let wat_abort = r#"(module
            (import "env" "abort" (func $abort (param i32 i32 i32 i32)))
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32)
                (call $abort (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 1))
                (i32.const 0)
            )
        )"#;
        let wasm_abort = wat::parse_str(wat_abort).unwrap();
        let manifest = PluginManifest {
            id: "abort-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            name: "Abort".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
        };

        let res = runtime
            .run_plugin(
                wasm_abort,
                manifest,
                json!({}),
                db,
                "corr-abort".into(),
                false,
            )
            .await;

        assert!(matches!(res, Err(PluginError::Trap(_))));
    }

    #[tokio::test]
    async fn test_host_imports_pointer_and_utf8_validation() {
        let db = create_test_db().await;
        let runtime = PluginRuntime::new();

        // Guest attempts to write invalid UTF-8 bytes to host_write_output
        let wat_invalid_utf8 = r#"(module
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            ;; Write invalid UTF-8 byte 0xFF at offset 0
            (data (i32.const 0) "\ff\ff")
            (func (export "opendash_run") (result i32)
                (local $res i32)
                (local.set $res (call $out (i32.const 0) (i32.const 2)))
                ;; If host_write_output returned -3 (invalid UTF-8), return 0 (success for test)
                (if (i32.eq (local.get $res) (i32.const -3))
                    (then (return (i32.const 0)))
                )
                (i32.const 1)
            )
        )"#;
        let wasm_utf8 = wat::parse_str(wat_invalid_utf8).unwrap();
        let manifest = PluginManifest {
            id: "utf8-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            name: "UTF8".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
        };

        let res = runtime
            .run_plugin(
                wasm_utf8,
                manifest,
                json!({}),
                db,
                "corr-utf8".into(),
                false,
            )
            .await;

        // Guest succeeded in catching -3 error code from host_write_output
        // (and failed to write output, so host returns InvalidOutput)
        assert!(matches!(res, Err(PluginError::InvalidOutput(_))));
    }

    #[test]
    fn test_forbidden_ip_checker() {
        use crate::plugin::host_ops::is_forbidden_ip;
        use std::net::IpAddr;

        // Loopback
        assert!(is_forbidden_ip("127.0.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ip("::1".parse::<IpAddr>().unwrap()));

        // Private IPv4
        assert!(is_forbidden_ip("10.0.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ip("172.16.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ip("192.168.1.1".parse::<IpAddr>().unwrap()));

        // Carrier-grade NAT (100.64.0.0/10)
        assert!(is_forbidden_ip("100.64.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ip(
            "100.127.255.255".parse::<IpAddr>().unwrap()
        ));

        // Link-local
        assert!(is_forbidden_ip("169.254.1.1".parse::<IpAddr>().unwrap()));

        // Documentation / Unspecified
        assert!(is_forbidden_ip("0.0.0.0".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ip("192.0.2.1".parse::<IpAddr>().unwrap()));
        assert!(is_forbidden_ip(
            "255.255.255.255".parse::<IpAddr>().unwrap()
        ));

        // Public IP should NOT be forbidden
        assert!(!is_forbidden_ip("8.8.8.8".parse::<IpAddr>().unwrap()));
        assert!(!is_forbidden_ip("1.1.1.1".parse::<IpAddr>().unwrap()));
    }

    #[tokio::test]
    async fn test_worker_lifecycle_and_teardown() {
        let db = create_test_db().await;
        let runtime = PluginRuntime::new();
        let wasm_echo = wat::parse_str(
            r#"(module
                (import "opendash_v1" "host_get_input_len" (func $in_len (result i32)))
                (import "opendash_v1" "host_read_input" (func $read_in (param i32 i32) (result i32)))
                (import "opendash_v1" "host_write_output" (func $write_out (param i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func (export "opendash_run") (result i32)
                    (local $len i32)
                    (local.set $len (call $in_len))
                    (drop (call $read_in (i32.const 0) (local.get $len)))
                    (drop (call $write_out (i32.const 0) (local.get $len)))
                    (i32.const 0)
                )
            )"#,
        )
        .unwrap();

        let manifest = PluginManifest {
            id: "worker-teardown".into(),
            version: "1.0.0".into(),
            api_version: 1,
            name: "Worker Teardown Test".into(),
            description: String::new(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
        };

        // Assert no lingering workers before execution
        assert_eq!(
            runtime.active_workers(),
            0,
            "Initial active workers must be 0"
        );

        let res = runtime
            .run_plugin(
                wasm_echo,
                manifest,
                json!({"ok": true}),
                db,
                "corr-teardown".into(),
                false,
            )
            .await;

        assert!(res.is_ok());
        // Assert worker thread exited and drop guard decremented active_workers cleanly
        assert_eq!(
            runtime.active_workers(),
            0,
            "Active workers must be 0 after execution terminates"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_worker_termination_and_permit_reclamation_on_timeout() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for _ in 0..3 {
                let (mut socket, _) = listener.accept().unwrap();
                std::thread::spawn(move || {
                    let mut buf = [0; 4096];
                    let _ = std::io::Read::read(&mut socket, &mut buf);
                    std::thread::sleep(Duration::from_secs(6));
                });
            }
        });

        let db = create_test_db().await;
        let runtime = PluginRuntime::new();
        assert_eq!(runtime.active_workers(), 0);

        let req =
            json!({"op":"http_request","method":"GET","url":format!("{origin}/slow")}).to_string();
        let esc = |s: &str| {
            s.as_bytes()
                .iter()
                .map(|b| format!("\\{b:02x}"))
                .collect::<String>()
        };
        let wat = format!(
            r#"(module
                (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "{}")
                (func (export "opendash_run") (result i32)
                    (drop (call $call (i32.const 0) (i32.const {}) (i32.const 1024) (i32.const 1024)))
                    (drop (call $call (i32.const 0) (i32.const {}) (i32.const 1024) (i32.const 1024)))
                    (drop (call $call (i32.const 0) (i32.const {}) (i32.const 1024) (i32.const 1024)))
                    (i32.const 0)
                )
            )"#,
            esc(&req),
            req.len(),
            req.len(),
            req.len()
        );

        let wasm = wat::parse_str(wat).unwrap();
        let manifest = PluginManifest {
            id: "slow-worker-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            name: "Slow Worker".into(),
            description: String::new(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions {
                sqlite: vec![],
                http: vec![origin.clone()],
            },
        };

        let res = runtime
            .run_plugin(
                wasm,
                manifest,
                json!({}),
                db.clone(),
                "corr-slow".into(),
                true,
            )
            .await;

        assert!(
            matches!(res, Err(PluginError::Timeout)),
            "expected timeout, got {res:?}"
        );

        // PROOF 1: active_workers is 0 immediately after timeout
        assert_eq!(
            runtime.active_workers(),
            0,
            "active_workers must be 0 after timeout"
        );

        // PROOF 2: All 4 permits can be acquired (zero leaked permits)
        let p1 = runtime
            .semaphore()
            .clone()
            .try_acquire_owned()
            .expect("permit 1 must be available");
        let p2 = runtime
            .semaphore()
            .clone()
            .try_acquire_owned()
            .expect("permit 2 must be available");
        let p3 = runtime
            .semaphore()
            .clone()
            .try_acquire_owned()
            .expect("permit 3 must be available");
        let p4 = runtime
            .semaphore()
            .clone()
            .try_acquire_owned()
            .expect("permit 4 must be available");
        assert!(runtime.semaphore().clone().try_acquire_owned().is_err());
        drop(p1);
        drop(p2);
        drop(p3);
        drop(p4);

        // PROOF 3: Next valid plugin invocation succeeds immediately
        let wat_out = r#"(module
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "{}")
            (func (export "opendash_run") (result i32)
                (drop (call $out (i32.const 0) (i32.const 2)))
                (i32.const 0)
            )
        )"#;
        let wasm_out = wat::parse_str(wat_out).unwrap();
        let manifest_echo = PluginManifest {
            id: "echo-recovery".into(),
            version: "1.0.0".into(),
            api_version: 1,
            name: "Echo Recovery".into(),
            description: String::new(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
        };
        let res_next = runtime
            .run_plugin(
                wasm_out,
                manifest_echo,
                json!({}),
                db,
                "corr-next".into(),
                false,
            )
            .await;
        assert!(
            res_next.is_ok(),
            "Next invocation must succeed: {res_next:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_worker_termination_on_caller_cancellation() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buf = [0; 4096];
            let _ = std::io::Read::read(&mut socket, &mut buf);
            std::thread::sleep(Duration::from_secs(6));
        });

        let db = create_test_db().await;
        let runtime = PluginRuntime::new();
        assert_eq!(runtime.active_workers(), 0);

        let req =
            json!({"op":"http_request","method":"GET","url":format!("{origin}/hang")}).to_string();
        let esc = |s: &str| {
            s.as_bytes()
                .iter()
                .map(|b| format!("\\{b:02x}"))
                .collect::<String>()
        };
        let wat = format!(
            r#"(module
                (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "{}")
                (func (export "opendash_run") (result i32)
                    (drop (call $call (i32.const 0) (i32.const {}) (i32.const 1024) (i32.const 1024)))
                    (drop (call $call (i32.const 0) (i32.const {}) (i32.const 1024) (i32.const 1024)))
                    (drop (call $call (i32.const 0) (i32.const {}) (i32.const 1024) (i32.const 1024)))
                    (i32.const 0)
                )
            )"#,
            esc(&req),
            req.len(),
            req.len(),
            req.len()
        );

        let wasm = wat::parse_str(wat).unwrap();
        let manifest = PluginManifest {
            id: "hang-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            name: "Hang Plugin".into(),
            description: String::new(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions {
                sqlite: vec![],
                http: vec![origin.clone()],
            },
        };

        let rt = runtime.clone();
        let task = tokio::spawn(async move {
            rt.run_plugin(wasm, manifest, json!({}), db, "corr-hang".into(), true)
                .await
        });

        // Let the worker process start and enter the blocking HTTP call
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            runtime.active_workers(),
            1,
            "worker must be active during call"
        );

        // Caller cancels the future (abort task)
        task.abort();
        let _ = task.await;

        // Allow guard drop and process kill
        tokio::time::sleep(Duration::from_millis(200)).await;

        // PROOF: worker terminated, active_workers == 0, all permits recovered
        assert_eq!(
            runtime.active_workers(),
            0,
            "active_workers must be 0 after caller cancellation"
        );
        let p1 = runtime
            .semaphore()
            .clone()
            .try_acquire_owned()
            .expect("permit 1 must be available");
        let p2 = runtime
            .semaphore()
            .clone()
            .try_acquire_owned()
            .expect("permit 2 must be available");
        let p3 = runtime
            .semaphore()
            .clone()
            .try_acquire_owned()
            .expect("permit 3 must be available");
        let p4 = runtime
            .semaphore()
            .clone()
            .try_acquire_owned()
            .expect("permit 4 must be available");
        drop(p1);
        drop(p2);
        drop(p3);
        drop(p4);
    }
}
