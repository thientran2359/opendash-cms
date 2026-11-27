use super::*;

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

        crate::storage::MIGRATOR
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        pool
    }

    #[tokio::test]
    async fn host_locale_visibility_and_list_total_follow_public_scope() {
        use crate::plugin::{host_ops::HostReadRequest, runtime::execute_host_read_request};
        use opendash_core::hooks::{ExecutionPrincipal, HookExecutionMode};

        let db = create_test_db().await;
        sqlx::query("INSERT INTO site_locales (code,bcp47_tag,label,direction,is_default,enabled,position) VALUES ('en','en','English','ltr',0,1,1)")
            .execute(&db).await.unwrap();
        for locale in ["vi", "en"] {
            sqlx::query("INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id) VALUES('post','hello','Hello','{}','draft',?,'host-locale-group')")
                .bind(locale).execute(&db).await.unwrap();
            sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Hello','hello',?,'host-term-group')")
                .bind(locale).execute(&db).await.unwrap();
        }
        sqlx::query("UPDATE content_entries SET publication_status='published' WHERE translation_group_id='host-locale-group'")
            .execute(&db).await.unwrap();
        let en_id: i64 = sqlx::query_scalar("SELECT id FROM content_entries WHERE locale='en'")
            .fetch_one(&db)
            .await
            .unwrap();
        let manifest = PluginManifest::from_json_str(r#"{"id":"locale-reader","version":"1.0.0","api_version":1,"manifest_schema":3,"min_host_version":"0.1.0","name":"Locale reader","entrypoint":"plugin.wasm","permissions":{"content_read":["post"],"taxonomy_read":true}}"#).unwrap();
        for (state, public_count, admin_count) in [("off", 1, 2), ("on", 2, 2), ("deleted", 1, 1)] {
            match state {
                "on" => {
                    sqlx::query("UPDATE site_locales_state SET multilingual_enabled=1 WHERE id=1")
                        .execute(&db)
                        .await
                        .unwrap();
                }
                "deleted" => {
                    sqlx::query("UPDATE site_locales SET enabled=0,deleted_at='2026-01-01T00:00:00Z' WHERE code='en'").execute(&db).await.unwrap();
                }
                _ => {}
            }
            for (principal, mode, expected) in [
                (
                    ExecutionPrincipal::Public,
                    HookExecutionMode::Standard,
                    public_count,
                ),
                (
                    ExecutionPrincipal::Admin,
                    HookExecutionMode::FilterProjection,
                    public_count,
                ),
                (
                    ExecutionPrincipal::Admin,
                    HookExecutionMode::Standard,
                    admin_count,
                ),
            ] {
                let req = HostReadRequest::ContentList {
                    type_key: "post".into(),
                    locale: None,
                    limit: Some(1),
                    offset: None,
                    published_only: Some(true),
                };
                let response = execute_host_read_request(
                    &db,
                    &manifest.id,
                    &manifest,
                    mode,
                    principal,
                    None,
                    None,
                    req,
                )
                .await;
                assert!(response.ok, "{state}: {:?}", response.error);
                let data = response.data.unwrap();
                assert_eq!(data["total"], expected, "{state}");
                assert_eq!(data["items"].as_array().unwrap().len(), 1);
                let req = HostReadRequest::ContentGet {
                    type_key: "post".into(),
                    entry_id: en_id,
                    locale: None,
                };
                let response = execute_host_read_request(
                    &db,
                    &manifest.id,
                    &manifest,
                    mode,
                    principal,
                    None,
                    None,
                    req,
                )
                .await;
                assert!(response.ok, "{state}: {:?}", response.error);
                assert_eq!(
                    response.data.unwrap()["entry"].is_null(),
                    expected == 1,
                    "{state}"
                );
                let req = HostReadRequest::TaxonomyList {
                    kind: "category".into(),
                    locale: None,
                    limit: None,
                };
                let response = execute_host_read_request(
                    &db,
                    &manifest.id,
                    &manifest,
                    mode,
                    principal,
                    None,
                    None,
                    req,
                )
                .await;
                assert!(response.ok, "{state}: {:?}", response.error);
                assert_eq!(
                    response.data.unwrap()["items"].as_array().unwrap().len(),
                    expected as usize,
                    "{state}"
                );
            }
        }
    }

    #[test]
    fn test_worker_entry() {
        if std::env::var("OPENDASH_WORKER_CHILD").is_ok() {
            if let Ok(handle) = std::thread::Builder::new()
                .name("opendash-plugin-worker-test".into())
                .stack_size(8 * 1024 * 1024)
                .spawn(crate::plugin::worker::run_worker_from_stdio)
            {
                let _ = handle.join();
            }
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
            include_bytes!("../../../../../examples/plugins/sample-plugin/sample-plugin.zip");
        let pkg =
            PluginPackage::from_zip(zip_bytes, runtime.engine()).expect("Sample ZIP phải hợp lệ");
        assert_eq!(pkg.manifest.id, "sample-plugin");
        assert!(!pkg.wasm_bytes.is_empty());
    }

    #[tokio::test]
    async fn test_sample_plugin_full_execution() {
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
        let db = create_test_db().await;
        let manager = PluginManager::new(db.clone(), true);
        let zip_bytes =
            include_bytes!("../../../../../examples/plugins/sample-plugin/sample-plugin.zip");

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
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
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
            hooks: vec![],
            id: "loop-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
            declarations: Default::default(),
            hook_priority: 0,
            actions: vec![],
            name: "Loop Plugin".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
            filters: vec![],
            editor_panels: vec![],
            settings_schema: vec![],
            ..Default::default()
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
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
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
            hooks: vec![],
            id: "trap-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
            declarations: Default::default(),
            hook_priority: 0,
            actions: vec![],
            name: "Trap Plugin".into(),
            description: "".into(),
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
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
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
            hooks: vec![],
            id: "no-perm-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
            declarations: Default::default(),
            hook_priority: 0,
            actions: vec![],
            name: "No Perm".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
            filters: vec![],
            editor_panels: vec![],
            settings_schema: vec![],
            ..Default::default()
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
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
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
            hooks: vec![],
            id: "mem-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
            declarations: Default::default(),
            hook_priority: 0,
            actions: vec![],
            name: "Mem Plugin".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
            filters: vec![],
            editor_panels: vec![],
            settings_schema: vec![],
            ..Default::default()
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
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
        use tokio::io::AsyncReadExt;

        // Bind dummy slow HTTP server on loopback
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let target_url = format!("http://{addr}/delay");

        // Hold the fixture open longer than the invocation budget. Assert that
        // the guest reports the HTTP timeout while the server is still silent,
        // instead of treating process-spawn/scheduler latency as HTTP latency.
        let (request_seen_tx, request_seen_rx) = tokio::sync::oneshot::channel();
        let slow_server = tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                if socket.read(&mut buf).await.unwrap() > 0 {
                    let _ = request_seen_tx.send(());
                }
                tokio::time::sleep(Duration::from_secs(10)).await;
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
            hooks: vec![],
            id: "http-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
            declarations: Default::default(),
            hook_priority: 0,
            actions: vec![],
            name: "HTTP Plugin".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions {
                sqlite: vec![],
                http: vec![target_url.clone()],
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
        request_seen_rx
            .await
            .expect("the HTTP fixture must observe a real outbound request");
        assert!(
            !slow_server.is_finished(),
            "the guest must time out before the fixture responds"
        );
        slow_server.abort();
        // The HTTP timeout is 2s and the invocation budget remains 5s;
        // allow time for child-process startup and thread scheduling under
        // the full workspace's default parallelism, but not an unbounded wait.
        assert!(
            elapsed >= Duration::from_secs(2),
            "Timeout did not wait for 2s HTTP timeout: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(8),
            "Timeout exceeded expected duration: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_sqlite_lock_timeout_and_zero_late_writes() {
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
        let db_dir = std::env::temp_dir().join(format!(
            "opendash_test_lock_{}_{}",
            std::process::id(),
            crate::auth::generate_random_token()
        ));
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

        // Use the actual schema; plugin installation now consults the theme,
        // extension and content registries as well as the original plugin
        // tables, so a partial hand-built schema is no longer representative.
        crate::storage::MIGRATOR.run(&pool).await.unwrap();
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
                active_revision_id INTEGER,
                revision_generation INTEGER NOT NULL DEFAULT 1,
                cas_version INTEGER NOT NULL DEFAULT 1,
                installed_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS plugin_revisions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                plugin_id TEXT NOT NULL,
                version TEXT NOT NULL,
                revision_digest TEXT NOT NULL,
                manifest_schema INTEGER NOT NULL DEFAULT 1,
                manifest_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(plugin_id, version),
                UNIQUE(plugin_id, revision_digest)
            );",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS plugin_kv (
                owner_kind TEXT NOT NULL DEFAULT 'plugin',
                plugin_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (owner_kind, plugin_id, key)
            );",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS plugin_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                owner_kind TEXT NOT NULL DEFAULT 'plugin',
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
            include_bytes!("../../../../../examples/plugins/sample-plugin/sample-plugin.zip");
        manager.install_plugin(zip_bytes).await.unwrap();

        let initial_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_kv")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            initial_count, 0,
            "Test must start with a fresh isolated database"
        );

        // Connection 1 acquires exclusive write lock
        let mut lock_tx = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO plugin_kv (plugin_id, key, value_json) VALUES ('sample-plugin', 'held', '1')",
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

        // Close Windows SQLite handles before deleting this unique test-owned directory.
        drop(manager);
        pool.close().await;
        std::fs::remove_dir_all(&db_dir).unwrap();
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
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
        let db = create_test_db().await;
        let manager = PluginManager::new(db.clone(), true);
        let zip_bytes =
            include_bytes!("../../../../../examples/plugins/sample-plugin/sample-plugin.zip");
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
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
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
            hooks: vec![],
            id: "abort-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
            declarations: Default::default(),
            hook_priority: 0,
            actions: vec![],
            name: "Abort".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
            filters: vec![],
            editor_panels: vec![],
            settings_schema: vec![],
            ..Default::default()
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
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
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
            hooks: vec![],
            id: "utf8-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
            declarations: Default::default(),
            hook_priority: 0,
            actions: vec![],
            name: "UTF8".into(),
            description: "".into(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
            filters: vec![],
            editor_panels: vec![],
            settings_schema: vec![],
            ..Default::default()
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
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
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
            hooks: vec![],
            id: "worker-teardown".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
            declarations: Default::default(),
            hook_priority: 0,
            actions: vec![],
            name: "Worker Teardown Test".into(),
            description: String::new(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
            filters: vec![],
            editor_panels: vec![],
            settings_schema: vec![],
            ..Default::default()
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
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
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
            hooks: vec![],
            id: "slow-worker-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
            declarations: Default::default(),
            hook_priority: 0,
            actions: vec![],
            name: "Slow Worker".into(),
            description: String::new(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions {
                sqlite: vec![],
                http: vec![origin.clone()],
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
            hooks: vec![],
            id: "echo-recovery".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
            declarations: Default::default(),
            hook_priority: 0,
            actions: vec![],
            name: "Echo Recovery".into(),
            description: String::new(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions::default(),
            filters: vec![],
            editor_panels: vec![],
            settings_schema: vec![],
            ..Default::default()
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
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (request_started_tx, request_started_rx) = tokio::sync::oneshot::channel();
        let (abort_handle_tx, abort_handle_rx) =
            std::sync::mpsc::sync_channel::<tokio::task::AbortHandle>(1);

        let db = create_test_db().await;
        let runtime = PluginRuntime::new();
        assert_eq!(runtime.active_workers(), 0);
        let runtime_at_request = runtime.clone();
        std::thread::spawn(move || {
            // Arrange cancellation on the I/O thread that first sees the
            // request. The async test task may not be rescheduled until after
            // the 2-second egress timeout when other tests run in parallel.
            let abort_handle = abort_handle_rx.recv().unwrap();
            let (mut socket, _) = listener.accept().unwrap();
            let mut buf = [0; 4096];
            let bytes_read = std::io::Read::read(&mut socket, &mut buf).unwrap();
            assert!(bytes_read > 0, "worker must send an HTTP request");
            let active_at_request = runtime_at_request.active_workers();
            abort_handle.abort();
            let _ = request_started_tx.send(active_at_request);
            std::thread::sleep(Duration::from_secs(6));
        });

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
            hooks: vec![],
            id: "hang-plugin".into(),
            version: "1.0.0".into(),
            api_version: 1,
            manifest_schema: 1,
            min_host_version: None,
            declarations: Default::default(),
            hook_priority: 0,
            actions: vec![],
            name: "Hang Plugin".into(),
            description: String::new(),
            entrypoint: "plugin.wasm".into(),
            permissions: PluginPermissions {
                sqlite: vec![],
                http: vec![origin.clone()],
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

        let rt = runtime.clone();
        let task = tokio::spawn(async move {
            rt.run_plugin(wasm, manifest, json!({}), db, "corr-hang".into(), true)
                .await
        });
        abort_handle_tx.send(task.abort_handle()).unwrap();

        // The I/O thread snapshots active workers and aborts the caller in
        // the same scheduling turn, before the simulated egress can time out.
        let active_at_request = tokio::time::timeout(Duration::from_secs(10), request_started_rx)
            .await
            .expect("worker did not reach the HTTP server within 10 seconds")
            .expect("HTTP server stopped before the worker sent its request");
        assert_eq!(
            active_at_request, 1,
            "worker must be active at cancellation"
        );

        // Caller cancellation must be observed, not a natural worker timeout.
        let cancelled = task.await.expect_err("caller task must be aborted");
        assert!(
            cancelled.is_cancelled(),
            "caller abort must cancel the task"
        );

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

    struct S01TempDir(std::path::PathBuf);

    impl S01TempDir {
        fn new(prefix: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), nanos));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for S01TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn build_zip_from_manifest(manifest_json: &str, wasm_bytes: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cur);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("manifest.json", options).unwrap();
            zip.write_all(manifest_json.as_bytes()).unwrap();
            zip.start_file("plugin.wasm", options).unwrap();
            zip.write_all(wasm_bytes).unwrap();
            zip.finish().unwrap();
        }
        cur.into_inner()
    }

    fn wat_escape(s: &str) -> String {
        s.as_bytes()
            .iter()
            .map(|b| format!("\\{b:02x}"))
            .collect::<String>()
    }

    fn decode_hex_bytes(hex: &str) -> Vec<u8> {
        let bytes = hex.as_bytes();
        assert_eq!(bytes.len() % 2, 0, "hex string must have even length");
        (0..bytes.len())
            .step_by(2)
            .map(|i| {
                let s = std::str::from_utf8(&bytes[i..i + 2]).unwrap();
                u8::from_str_radix(s, 16).unwrap()
            })
            .collect()
    }

    fn decode_hex_json(hex: &str) -> serde_json::Value {
        let raw = decode_hex_bytes(hex);
        serde_json::from_slice(&raw).unwrap_or_else(|e| {
            panic!(
                "invalid json in hex response: {e}; raw={}",
                String::from_utf8_lossy(&raw)
            )
        })
    }

    /// Build a WAT module that executes each JSON host request sequentially with `resp_cap`
    /// (exercising `-2` + `host_last_resp_len` + `host_read_last_resp` whenever response > `resp_cap`)
    /// and writes the raw response bytes as hex into either `plugin_kv` (`kv_prefix_<i>`)
    /// or `plugin_logs` (`use_log = true`).
    fn build_s01_capture_wasm(requests: &[String], resp_cap: usize, use_log: bool) -> Vec<u8> {
        let mut data_segments = String::new();
        let mut req_offsets = Vec::new();
        let mut cursor = 0usize;
        for req in requests {
            let bytes = req.as_bytes();
            data_segments.push_str(&format!(
                "  (data (i32.const {cursor}) \"{}\")\n",
                wat_escape(req)
            ));
            req_offsets.push((cursor, bytes.len()));
            cursor += bytes.len() + 4;
        }

        let ok_out = "{\"ok\":true}";
        let ok_out_off = cursor;
        data_segments.push_str(&format!(
            "  (data (i32.const {ok_out_off}) \"{}\")\n",
            wat_escape(ok_out)
        ));
        cursor += ok_out.len() + 4;

        let suffix = "\"}";
        let suffix_off = cursor;
        data_segments.push_str(&format!(
            "  (data (i32.const {suffix_off}) \"{}\")\n",
            wat_escape(suffix)
        ));
        cursor += suffix.len() + 4;

        let mut prefix_offsets = Vec::new();
        for idx in 0..requests.len() {
            let prefix = if use_log {
                format!("{{\"op\":\"log\",\"level\":\"info\",\"message\":\"{idx}:")
            } else {
                format!("{{\"op\":\"kv_set\",\"key\":\"cap_{idx}\",\"value\":\"")
            };
            data_segments.push_str(&format!(
                "  (data (i32.const {cursor}) \"{}\")\n",
                wat_escape(&prefix)
            ));
            prefix_offsets.push((cursor, prefix.len()));
            cursor += prefix.len() + 4;
        }
        assert!(cursor < 16384, "static WAT data exceeded 16 KiB");

        let mut body = String::new();
        for (idx, ((req_off, req_len), (pfx_off, pfx_len))) in
            req_offsets.into_iter().zip(prefix_offsets).enumerate()
        {
            body.push_str(&format!(
                r#"
    (local.set $n (call $exec_and_read (i32.const {req_off}) (i32.const {req_len}) (i32.const {resp_cap})))
    (if (i32.lt_s (local.get $n) (i32.const 0))
      (then (return (i32.sub (i32.const 0) (local.get $n))))
    )
    (memory.copy (i32.const 50000) (i32.const {pfx_off}) (i32.const {pfx_len}))
    (local.set $hlen (call $hex_encode (i32.const 16384) (local.get $n) (i32.const {})))
    (memory.copy (i32.add (i32.const {}) (local.get $hlen)) (i32.const {suffix_off}) (i32.const {}))
    (local.set $total (i32.add (i32.const {}) (local.get $hlen)))
    (drop (call $host_call (i32.const 50000) (local.get $total) (i32.const 102400) (i32.const 4096)))
    ;; record call {idx}
"#,
                50000 + pfx_len,
                50000 + pfx_len,
                suffix.len(),
                pfx_len + suffix.len(),
            ));
        }

        let wat_src = format!(
            r#"(module
  (import "opendash_v1" "host_get_input_len" (func $host_get_input_len (result i32)))
  (import "opendash_v1" "host_read_input" (func $host_read_input (param i32 i32) (result i32)))
  (import "opendash_v1" "host_write_output" (func $host_write_output (param i32 i32) (result i32)))
  (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
  (import "opendash_v1" "host_last_resp_len" (func $host_last_resp_len (result i32)))
  (import "opendash_v1" "host_read_last_resp" (func $host_read_last_resp (param i32 i32) (result i32)))
  (memory (export "memory") 3)
{data_segments}
  (func $nibble (param $v i32) (result i32)
    (if (result i32) (i32.lt_u (local.get $v) (i32.const 10))
      (then (i32.add (local.get $v) (i32.const 48)))
      (else (i32.add (local.get $v) (i32.const 87)))
    )
  )
  (func $hex_encode (param $src i32) (param $len i32) (param $dst i32) (result i32)
    (local $i i32)
    (local $b i32)
    (local $out i32)
    (local.set $i (i32.const 0))
    (loop $lp
      (if (i32.lt_u (local.get $i) (local.get $len))
        (then
          (local.set $b (i32.load8_u (i32.add (local.get $src) (local.get $i))))
          (local.set $out (i32.add (local.get $dst) (i32.mul (local.get $i) (i32.const 2))))
          (i32.store8 (local.get $out) (call $nibble (i32.shr_u (local.get $b) (i32.const 4))))
          (i32.store8 (i32.add (local.get $out) (i32.const 1)) (call $nibble (i32.and (local.get $b) (i32.const 15))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $lp)
        )
      )
    )
    (i32.mul (local.get $len) (i32.const 2))
  )
  (func $exec_and_read (param $req_ptr i32) (param $req_len i32) (param $cap i32) (result i32)
    (local $rc i32)
    (local $full_len i32)
    (local.set $rc (call $host_call (local.get $req_ptr) (local.get $req_len) (i32.const 16384) (local.get $cap)))
    (if (i32.eq (local.get $rc) (i32.const -2))
      (then
        (local.set $full_len (call $host_last_resp_len))
        (if (i32.le_s (local.get $full_len) (i32.const 0))
          (then (return (i32.const -98)))
        )
        (local.set $rc (call $host_read_last_resp (i32.const 16384) (i32.const 32768)))
        (if (i32.ne (local.get $rc) (local.get $full_len))
          (then (return (i32.const -99)))
        )
      )
    )
    (local.get $rc)
  )
  (func (export "opendash_run") (result i32)
    (local $n i32)
    (local $hlen i32)
    (local $total i32)
    (local $in_len i32)
{body}
    (local.set $in_len (call $host_get_input_len))
    (if (i32.and (i32.gt_s (local.get $in_len) (i32.const 0)) (i32.le_s (local.get $in_len) (i32.const 16384)))
      (then
        (drop (call $host_read_input (i32.const 130000) (local.get $in_len)))
        (drop (call $host_write_output (i32.const 130000) (local.get $in_len)))
      )
      (else
        (drop (call $host_write_output (i32.const {ok_out_off}) (i32.const {ok_out_len})))
      )
    )
    (i32.const 0)
  )
)"#,
            ok_out_len = ok_out.len(),
        );
        wat::parse_str(&wat_src).expect("capture WAT must compile")
    }

    async fn read_captured_kv(db: &SqlitePool, plugin_id: &str, idx: usize) -> serde_json::Value {
        let key = format!("cap_{idx}");
        let raw: String =
            sqlx::query_scalar("SELECT value_json FROM plugin_kv WHERE plugin_id = ? AND key = ?")
                .bind(plugin_id)
                .bind(&key)
                .fetch_one(db)
                .await
                .unwrap_or_else(|e| panic!("missing captured kv {key}: {e}"));
        let hex: String = serde_json::from_str(&raw).unwrap();
        decode_hex_json(&hex)
    }

    #[tokio::test]
    async fn test_s01_abi_v1_1_large_readback_and_binary_ipc_newline() {
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
        let db = create_test_db().await;
        let manager = PluginManager::new(db.clone(), true);

        let large_text = format!(
            "Tiếng Việt \"quote\" \\ backslash\nline2\n{}",
            "payload-line\n".repeat(220)
        );
        assert!(large_text.len() > 2500);

        let requests = vec![json!({ "op": "kv_get", "key": "big" }).to_string()];
        let wasm = build_s01_capture_wasm(&requests, 32, false);
        let manifest_json = json!({
            "id": "s01-readback",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "S01 Readback",
            "entrypoint": "plugin.wasm",
            "permissions": {
                "sqlite": ["kv_read", "kv_write"]
            }
        })
        .to_string();
        let zip = build_zip_from_manifest(&manifest_json, &wasm);
        manager.install_plugin(&zip).await.unwrap();

        sqlx::query("INSERT INTO plugin_kv (plugin_id, key, value_json) VALUES (?, 'big', ?)")
            .bind("s01-readback")
            .bind(serde_json::to_string(&large_text).unwrap())
            .execute(&db)
            .await
            .unwrap();

        let res = manager.run_plugin("s01-readback", json!({})).await.unwrap();
        assert!(res.success, "run failed: {:?}", res.error);

        let captured = read_captured_kv(&db, "s01-readback", 0).await;
        assert_eq!(captured["found"], true);
        assert_eq!(captured["value"], large_text);
    }

    #[tokio::test]
    async fn test_s01_v3_host_ops_secret_redaction_and_atomic_tombstone_logs() {
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
        let db = create_test_db().await;
        let temp = S01TempDir::new("s01_host_ops");
        let plugins_root = temp.0.join("plugins");
        let secrets_root = temp.0.join("secrets");
        let manager =
            PluginManager::with_roots(db.clone(), true, plugins_root, secrets_root.clone());

        let secret_ref = crate::secrets::SecretStore::for_plugin_dir(
            secrets_root.join("plugins").join("s01-ops"),
        )
        .write_plugin_secret("top-secret-999")
        .unwrap();

        let med_pub = "a".repeat(64);
        let med_priv = "b".repeat(64);

        // Seed taxonomy, media, entries, and plugin settings.
        sqlx::query(
            "INSERT INTO taxonomy_terms (kind, locale, translation_group_id, name, slug, version)
             VALUES ('category', 'vi', 'tg-cat-1', 'Tin tức', 'tin-tuc', 1)",
        )
        .execute(&db)
        .await
        .unwrap();

        for (id, filename) in [(&med_pub, "hero.png"), (&med_priv, "priv.png")] {
            crate::storage::media::save(
                &db,
                &crate::storage::media::MediaAsset {
                    id: id.clone(),
                    kind: "image".into(),
                    filename: filename.into(),
                    mime_type: "image/png".into(),
                    extension: "png".into(),
                    byte_size: 123,
                    width: 100,
                    height: 100,
                    thumb_width: Some(50),
                    thumb_height: Some(50),
                    duration_ms: None,
                    video_codec: None,
                    audio_codec: None,
                    lifecycle_state: "active".into(),
                    generation: 1,
                    version: 1,
                    created_at: String::new(),
                },
            )
            .await
            .unwrap();
        }

        let pub_values = json!({ "featured_image": med_pub }).to_string();
        sqlx::query(
            "INSERT INTO content_entries
             (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status)
             VALUES
             ('post', 'vi', 'eg-pub', 'hello-pub', 'Bài đã xuất bản', ?, 'draft'),
             ('post', 'vi', 'eg-dft', 'hello-dft', 'Bài nháp', '{}', 'draft')",
        )
        .bind(&pub_values)
        .execute(&db)
        .await
        .unwrap();

        sqlx::query(
            "UPDATE content_entries SET publication_status = 'published'
             WHERE content_type_key = 'post' AND slug = 'hello-pub'",
        )
        .execute(&db)
        .await
        .unwrap();

        let pub_entry_id: i64 = sqlx::query_scalar(
            "SELECT id FROM content_entries WHERE content_type_key = 'post' AND slug = 'hello-pub'",
        )
        .fetch_one(&db)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO entry_builder_media_refs (entry_id, media_id, ref_role) VALUES (?, ?, 'image')",
        )
        .bind(pub_entry_id)
        .bind(&med_pub)
        .execute(&db)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO theme_option_values (theme_id, option_key, value_json, owner_kind, owner_id)
             VALUES ('s01-ops', 'api_token', ?, 'plugin', 's01-ops'),
                    ('s01-ops', 'secret_key', '\"should-be-redacted\"', 'plugin', 's01-ops'),
                    ('s01-ops', 'mode', '\"fast\"', 'plugin', 's01-ops')",
        )
        .bind(json!({ "secret_ref": secret_ref.as_str(), "configured": true }).to_string())
        .execute(&db)
        .await
        .unwrap();

        let requests = vec![
            json!({ "op": "kv_set", "key": "keep_a", "value": 42 }).to_string(),
            json!({ "op": "kv_delete", "key": "del_b" }).to_string(),
            json!({ "op": "kv_get", "key": "del_b" }).to_string(),
            json!({ "op": "kv_list", "prefix": "keep_", "limit": 50 }).to_string(),
            json!({ "op": "content_list", "type_key": "post", "limit": 10 }).to_string(),
            json!({ "op": "content_get", "type_key": "post", "entry_id": pub_entry_id })
                .to_string(),
            json!({ "op": "taxonomy_list", "kind": "category" }).to_string(),
            json!({ "op": "media_get", "media_id": med_pub }).to_string(),
            json!({ "op": "site_options_get" }).to_string(),
            json!({ "op": "plugin_settings_get" }).to_string(),
            json!({ "op": "plugin_secret_get", "key": "api_token" }).to_string(),
            json!({ "op": "log", "level": "info", "message": "atomic commit check" }).to_string(),
        ];
        let wasm = build_s01_capture_wasm(&requests, 4096, false);
        let manifest_json = json!({
            "id": "s01-ops",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.0",
            "name": "S01 Ops",
            "entrypoint": "plugin.wasm",
            "permissions": {
                "sqlite": ["kv_read", "kv_write"],
                "content_read": ["post"],
                "taxonomy_read": true,
                "media_read": true,
                "site_options_read": true,
                "plugin_settings_read": true,
                "plugin_secrets_read": ["api_token"]
            },
            "settings_schema": [
                { "key": "api_token", "label": "API Token", "type": "secret" },
                { "key": "secret_key", "label": "Secret", "type": "secret" },
                { "key": "mode", "label": "Mode", "type": "text" }
            ]
        })
        .to_string();
        let zip = build_zip_from_manifest(&manifest_json, &wasm);
        manager.install_plugin(&zip).await.unwrap();

        sqlx::query(
            "INSERT INTO plugin_kv (plugin_id, key, value_json) VALUES ('s01-ops', 'del_b', '123')",
        )
        .execute(&db)
        .await
        .unwrap();

        let res = manager.run_plugin("s01-ops", json!({})).await.unwrap();
        assert!(res.success, "run failed: {:?}", res.error);

        // kv_delete tombstone visible mid-invocation + deleted from DB after single tx commit
        let kv_get_deleted = read_captured_kv(&db, "s01-ops", 2).await;
        assert_eq!(kv_get_deleted["ok"], true);
        assert_eq!(kv_get_deleted["found"], false);
        let kv_list = read_captured_kv(&db, "s01-ops", 3).await;
        assert_eq!(kv_list["ok"], true);
        let kv_items = kv_list["data"]["items"].as_array().unwrap();
        assert_eq!(kv_items.len(), 1);
        assert_eq!(kv_items[0]["key"], "keep_a");
        assert_eq!(kv_items[0]["value"], 42);

        let del_remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM plugin_kv WHERE plugin_id = 's01-ops' AND key = 'del_b'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(del_remaining, 0);

        let log_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM plugin_logs WHERE plugin_id = 's01-ops' AND message = 'atomic commit check'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(log_count, 1);

        // content_list + content_get in Admin principal sees both posts
        let list_res = read_captured_kv(&db, "s01-ops", 4).await;
        assert_eq!(list_res["ok"], true);
        assert!(list_res["data"]["items"].as_array().unwrap().len() >= 2);
        assert!(list_res["data"]["total"].as_i64().unwrap() >= 2);
        let get_res = read_captured_kv(&db, "s01-ops", 5).await;
        assert_eq!(get_res["ok"], true);
        assert_eq!(get_res["data"]["entry"]["slug"], "hello-pub");

        // taxonomy_list + media_get + site_options_get
        let tax_res = read_captured_kv(&db, "s01-ops", 6).await;
        assert_eq!(tax_res["ok"], true);
        assert_eq!(tax_res["data"]["items"][0]["slug"], "tin-tuc");

        let media_res = read_captured_kv(&db, "s01-ops", 7).await;
        assert_eq!(media_res["ok"], true);
        assert_eq!(media_res["data"]["media"]["id"], med_pub);

        let site_opt_res = read_captured_kv(&db, "s01-ops", 8).await;
        assert_eq!(site_opt_res["ok"], true);
        assert!(site_opt_res["data"]["options"].is_object());

        // plugin_settings_get redacts secret fields
        let settings_res = read_captured_kv(&db, "s01-ops", 9).await;
        assert_eq!(settings_res["ok"], true);
        assert_eq!(settings_res["data"]["settings"]["mode"], "fast");
        assert_eq!(
            settings_res["data"]["settings"]["api_token"],
            json!({ "configured": true })
        );
        assert_eq!(
            settings_res["data"]["settings"]["secret_key"],
            json!({ "configured": false })
        );

        // plugin_secret_get returns real secret in Admin invocation
        let secret_res = read_captured_kv(&db, "s01-ops", 10).await;
        assert_eq!(secret_res["ok"], true, "secret get failed: {secret_res}");
        assert_eq!(secret_res["data"]["value"], "top-secret-999");

        // Trap rollback check: trapping guest never deletes staged tombstone or inserts staged log
        let trap_wat = format!(
            r#"(module
  (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "{}")
  (func (export "opendash_run") (result i32)
    (drop (call $host_call (i32.const 0) (i32.const {}) (i32.const 1024) (i32.const 1024)))
    unreachable
  )
)"#,
            wat_escape(
                &json!({ "op": "log", "level": "warn", "message": "must-not-commit" }).to_string()
            ),
            json!({ "op": "log", "level": "warn", "message": "must-not-commit" })
                .to_string()
                .len()
        );
        let trap_manifest = json!({
            "id": "s01-trap",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.0",
            "name": "S01 Trap",
            "entrypoint": "plugin.wasm"
        })
        .to_string();
        manager
            .install_plugin(&build_zip_from_manifest(
                &trap_manifest,
                &wat::parse_str(&trap_wat).unwrap(),
            ))
            .await
            .unwrap();
        let trap_res = manager.run_plugin("s01-trap", json!({})).await;
        assert!(trap_res.is_err());
        let bad_logs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM plugin_logs WHERE plugin_id = 's01-trap' AND message = 'must-not-commit'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(bad_logs, 0);
    }

    #[tokio::test]
    async fn test_s01_policy_matrix_public_scope_filters_and_content_delete_cas() {
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
        let db = create_test_db().await;
        let temp = S01TempDir::new("s01_policy_delete");
        let plugins_root = temp.0.join("plugins");
        let secrets_root = temp.0.join("secrets");
        let manager = std::sync::Arc::new(PluginManager::with_roots(
            db.clone(),
            true,
            plugins_root,
            secrets_root.clone(),
        ));
        let secret_ref = crate::secrets::SecretStore::for_plugin_dir(
            secrets_root.join("plugins").join("s01-policy"),
        )
        .write_plugin_secret("secret")
        .unwrap();

        let med_pub = "c".repeat(64);
        let med_unpub = "d".repeat(64);
        for (id, filename) in [(&med_pub, "a.png"), (&med_unpub, "b.png")] {
            crate::storage::media::save(
                &db,
                &crate::storage::media::MediaAsset {
                    id: id.clone(),
                    kind: "image".into(),
                    filename: filename.into(),
                    mime_type: "image/png".into(),
                    extension: "png".into(),
                    byte_size: 100,
                    width: 100,
                    height: 100,
                    thumb_width: Some(50),
                    thumb_height: Some(50),
                    duration_ms: None,
                    video_codec: None,
                    audio_codec: None,
                    lifecycle_state: "active".into(),
                    generation: 1,
                    version: 1,
                    created_at: String::new(),
                },
            )
            .await
            .unwrap();
        }

        let pub_values = json!({ "featured_image": med_pub }).to_string();
        sqlx::query(
            "INSERT INTO content_entries
             (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status)
             VALUES
             ('post', 'vi', 'eg-p', 'pub-post', 'Pub', ?, 'draft'),
             ('post', 'vi', 'eg-d', 'draft-post', 'Draft', '{}', 'draft')",
        )
        .bind(&pub_values)
        .execute(&db)
        .await
        .unwrap();

        sqlx::query(
            "UPDATE content_entries SET publication_status = 'published'
             WHERE content_type_key = 'post' AND slug = 'pub-post'",
        )
        .execute(&db)
        .await
        .unwrap();

        let pub_id: i64 =
            sqlx::query_scalar("SELECT id FROM content_entries WHERE slug = 'pub-post'")
                .fetch_one(&db)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO entry_builder_media_refs (entry_id, media_id, ref_role) VALUES (?, ?, 'image')",
        )
        .bind(pub_id)
        .bind(&med_pub)
        .execute(&db)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO theme_option_values (theme_id, option_key, value_json, owner_kind, owner_id)
             VALUES ('s01-policy', 'k', ?, 'plugin', 's01-policy')",
        )
        .bind(json!({ "secret_ref": secret_ref.as_str(), "configured": true }).to_string())
        .execute(&db)
        .await
        .unwrap();

        let (draft_id, draft_version): (i64, i64) =
            sqlx::query_as("SELECT id, version FROM content_entries WHERE slug = 'draft-post'")
                .fetch_one(&db)
                .await
                .unwrap();

        // 1. Public principal cannot read draft posts, unreferenced media, or secrets.
        let public_reqs = vec![
            json!({ "op": "content_get", "type_key": "post", "entry_id": draft_id }).to_string(),
            json!({ "op": "media_get", "media_id": med_unpub }).to_string(),
            json!({ "op": "plugin_secret_get", "key": "k" }).to_string(),
        ];
        let public_wasm = build_s01_capture_wasm(&public_reqs, 2048, true);
        let manifest_json = json!({
            "id": "s01-policy",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.0",
            "name": "S01 Policy",
            "entrypoint": "plugin.wasm",
            "permissions": {
                "sqlite": ["kv_read", "kv_write"],
                "content_read": ["post"],
                "content_write": ["post"],
                "content_delete": ["post"],
                "media_read": true,
                "plugin_secrets_read": ["k"]
            },
            "settings_schema": [
                { "key": "k", "label": "K", "type": "secret" }
            ]
        })
        .to_string();
        manager
            .install_plugin(&build_zip_from_manifest(&manifest_json, &public_wasm))
            .await
            .unwrap();

        let disp = manager
            .run_plugin_dispatch(
                "s01-policy",
                opendash_core::hooks::EnvelopeKind::Filter,
                json!({}),
                opendash_core::hooks::HookEvent::FilterEntryView.as_str(),
                1_700_000_000_000,
                "corr-pub".into(),
                Some("vi".into()),
                opendash_core::hooks::ExecutionPrincipal::Public,
                0,
                json!({}),
                opendash_core::hooks::HookExecutionMode::FilterProjection,
                None,
            )
            .await
            .unwrap();
        assert!(disp.success, "public dispatch failed: {:?}", disp.error);
        let msgs: Vec<String> = sqlx::query_scalar(
            "SELECT message FROM plugin_logs WHERE plugin_id = 's01-policy' AND correlation_id = 'corr-pub' AND message GLOB '[0-9]*:*' ORDER BY id ASC",
        )
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(msgs.len(), 3);
        let r0 = decode_hex_json(msgs[0].split_once(':').unwrap().1);
        let r1 = decode_hex_json(msgs[1].split_once(':').unwrap().1);
        let r2 = decode_hex_json(msgs[2].split_once(':').unwrap().1);
        assert_eq!(r0["ok"], true);
        assert!(r0["data"]["entry"].is_null(), "draft leaked: {r0}");
        assert_eq!(r1["ok"], true);
        assert!(
            r1["data"]["media"].is_null(),
            "unreferenced media leaked: {r1}"
        );
        assert_eq!(r2["ok"], false);
        assert!(r2["error"].is_string(), "secret must be denied: {r2}");

        // 2. Mixed write scope block (run_plugin) + CAS precondition 412 + actual fenced delete
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role) VALUES (1, 'admin_s01', 'h', 'admin')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, csrf_token, expires_at)
             VALUES ('t', 1, 'c', ?)",
        )
        .bind(crate::auth::current_time_secs() + 3600)
        .execute(&db)
        .await
        .unwrap();
        let actor = crate::auth::SessionContext {
            user: crate::auth::AuthUser {
                id: 1,
                username: "admin_s01".into(),
                role: crate::auth::Role::Admin,
            },
            csrf_token: "c".into(),
            token_hash: "t".into(),
        };
        let content_reg = std::sync::Arc::new(tokio::sync::RwLock::new(
            opendash_core::registry::RegistryCatalog::default(),
        ));
        let hooks = crate::hooks::HookDispatcher::new(manager.clone());

        let stale_reqs = vec![
            json!({ "op": "content_delete", "type_key": "post", "entry_id": draft_id, "expected_version": draft_version + 99 }).to_string(),
            json!({ "op": "kv_set", "key": "mix", "value": 1 }).to_string(),
        ];
        let stale_wasm = build_s01_capture_wasm(&stale_reqs, 2048, true);
        let mut stale_manifest: serde_json::Value = serde_json::from_str(&manifest_json).unwrap();
        stale_manifest["id"] = json!("s01-del-stale");
        stale_manifest["actions"] = json!([{
            "id": "clean",
            "title": "Clean",
            "required_role": "admin",
            "capabilities": ["content_delete", "log"]
        }]);
        manager
            .install_plugin(&build_zip_from_manifest(
                &stale_manifest.to_string(),
                &stale_wasm,
            ))
            .await
            .unwrap();

        // In Standard mode (action_caps = None), mixing content_delete + kv_set hits ERR_MIXED_WRITE_SCOPE
        let mix_res = manager
            .run_plugin("s01-del-stale", json!({}))
            .await
            .unwrap();
        assert!(mix_res.success);
        let mix_msgs: Vec<String> = sqlx::query_scalar(
            "SELECT message FROM plugin_logs WHERE plugin_id = 's01-del-stale' AND message GLOB '[0-9]*:*' ORDER BY id ASC",
        )
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(mix_msgs.len(), 2);
        let del_staged = decode_hex_json(mix_msgs[0].split_once(':').unwrap().1);
        let mixed = decode_hex_json(mix_msgs[1].split_once(':').unwrap().1);
        assert_eq!(del_staged["ok"], true);
        assert_eq!(mixed["ok"], false);
        assert!(
            mixed["error"]
                .as_str()
                .unwrap_or("")
                .contains("ERR_MIXED_WRITE_SCOPE"),
            "expected ERR_MIXED_WRITE_SCOPE, got: {mixed}"
        );

        // Action execution with stale expected_version hits CAS 412 Precondition Failed
        let stale_err = manager
            .execute_plugin_action(
                "s01-del-stale",
                "clean",
                &actor,
                json!({}),
                &content_reg,
                &hooks,
                "corr-stale".into(),
                crate::plugin::lifecycle_confirmation::LifecycleActionOrigin::Generic,
                0,
            )
            .await
            .expect_err("stale content_delete must return 412");
        assert_eq!(stale_err.0, axum::http::StatusCode::PRECONDITION_FAILED);

        let still_exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM content_entries WHERE id = ?")
                .bind(draft_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(still_exists, 1);

        // Action execution with matching expected_version moves entry to recoverable Trash (S02)
        let ok_reqs = vec![
            json!({ "op": "content_delete", "type_key": "post", "entry_id": draft_id, "expected_version": draft_version }).to_string(),
        ];
        let ok_wasm = build_s01_capture_wasm(&ok_reqs, 2048, true);
        let mut ok_manifest: serde_json::Value = serde_json::from_str(&manifest_json).unwrap();
        ok_manifest["id"] = json!("s01-del-ok");
        ok_manifest["actions"] = json!([{
            "id": "clean",
            "title": "Clean",
            "required_role": "admin",
            "capabilities": ["content_delete", "log"]
        }]);
        manager
            .install_plugin(&build_zip_from_manifest(&ok_manifest.to_string(), &ok_wasm))
            .await
            .unwrap();

        let ok_outcome = manager
            .execute_plugin_action(
                "s01-del-ok",
                "clean",
                &actor,
                json!({}),
                &content_reg,
                &hooks,
                "corr-del-ok".into(),
                crate::plugin::lifecycle_confirmation::LifecycleActionOrigin::Generic,
                0,
            )
            .await
            .expect("matching content_delete must succeed");
        assert!(ok_outcome.response.success);
        assert_eq!(
            ok_outcome.committed.as_ref().map(|r| r.operation),
            Some(crate::storage::entry_lifecycle::EntryMutationKind::Trash)
        );

        // S02 recoverable Trash: canonical DB row retained with trashed_at set, excluded from active read.
        let retained_trashed: Option<String> =
            sqlx::query_scalar("SELECT trashed_at FROM content_entries WHERE id = ?")
                .bind(draft_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert!(retained_trashed.is_some());
        assert!(
            crate::storage::entries::get_entry(&db, "post", draft_id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_s01_bugfixes_cache_network_loopback_actor_role_and_semaphores() {
        let _serial_plugin_test = crate::plugin::PLUGIN_TEST_SERIAL.acquire().await.unwrap();
        let db = create_test_db().await;
        let runtime = PluginRuntime::new();

        // 1. Semaphore 4 + 2 = 6 split
        assert_eq!(crate::plugin::runtime::MAX_TOTAL_CONCURRENT_WORKERS, 6);
        let mut admin_permits = Vec::new();
        for _ in 0..4 {
            admin_permits.push(runtime.semaphore().clone().try_acquire_owned().unwrap());
        }
        assert!(runtime.semaphore().clone().try_acquire_owned().is_err());
        let bg1 = runtime
            .background_public_semaphore()
            .clone()
            .try_acquire_owned()
            .unwrap();
        let bg2 = runtime
            .background_public_semaphore()
            .clone()
            .try_acquire_owned()
            .unwrap();
        assert!(
            runtime
                .background_public_semaphore()
                .clone()
                .try_acquire_owned()
                .is_err()
        );
        drop(bg1);
        drop(bg2);
        drop(admin_permits);

        // 2. Network HTTP bugfix + /api/p/ loopback block
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let origin = format!("http://{addr}");
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for _ in 0..2 {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buf = [0u8; 2048];
                    let _ = stream.read(&mut buf);
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    );
                    let _ = stream.flush();
                }
            }
        });

        let reqs = vec![
            json!({ "op": "http_request", "method": "GET", "url": format!("{origin}/api/p/loopback") }).to_string(),
            json!({ "op": "http_request", "method": "GET", "url": format!("{origin}/ok") }).to_string(),
        ];
        let wasm = build_s01_capture_wasm(&reqs, 2048, true);
        let manifest_json = json!({
            "id": "s01-net",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.0",
            "name": "S01 Net",
            "entrypoint": "plugin.wasm",
            "hooks": ["content/before_save"],
            "permissions": {
                "network": [{
                    "scheme": "http",
                    "host": "127.0.0.1",
                    "port": addr.port(),
                    "methods": ["GET"]
                }],
                "content_read": ["post"]
            },
            "actions": [{
                "id": "ping",
                "title": "Ping",
                "required_role": "editor",
                "capabilities": ["http", "log"]
            }]
        })
        .to_string();

        let temp = S01TempDir::new("s01_net");
        let manager = std::sync::Arc::new(PluginManager::with_roots(
            db.clone(),
            true,
            temp.0.join("plugins"),
            temp.0.join("secrets"),
        ));
        manager
            .install_plugin(&build_zip_from_manifest(&manifest_json, &wasm))
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role) VALUES (2, 'editor1', 'h', 'editor')",
        )
        .execute(&db)
        .await
        .unwrap();
        let editor_actor = crate::auth::SessionContext {
            user: crate::auth::AuthUser {
                id: 2,
                username: "editor1".into(),
                role: crate::auth::Role::Editor,
            },
            csrf_token: "c2".into(),
            token_hash: "t2".into(),
        };
        let content_reg = std::sync::Arc::new(tokio::sync::RwLock::new(
            opendash_core::registry::RegistryCatalog::default(),
        ));

        let hook = crate::hooks::HookDispatcher::new(manager.clone());
        let _ = hook
            .dispatch_content_before_save_for_actor(
                "post",
                opendash_core::content::FieldRole::Editor,
                json!({ "title": "hi" }),
            )
            .await;

        let action_outcome = manager
            .execute_plugin_action(
                "s01-net",
                "ping",
                &editor_actor,
                json!({ "hello": 1 }),
                &content_reg,
                &hook,
                "corr-net".into(),
                crate::plugin::lifecycle_confirmation::LifecycleActionOrigin::Generic,
                0,
            )
            .await
            .expect("network http action must succeed");
        assert!(action_outcome.response.success);
        let echoed_env = action_outcome.response.data.expect("echoed envelope");
        assert_eq!(echoed_env["envelope_version"], 2);
        assert_eq!(echoed_env["kind"], "action");
        assert_eq!(echoed_env["principal"], "editor");
        assert_eq!(echoed_env["payload"]["hello"], 1);

        let net_msgs: Vec<String> = sqlx::query_scalar(
            "SELECT message FROM plugin_logs WHERE plugin_id = 's01-net' AND correlation_id = 'corr-net' AND message GLOB '[0-9]*:*' ORDER BY id ASC",
        )
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(net_msgs.len(), 2);
        let blocked = decode_hex_json(net_msgs[0].split_once(':').unwrap().1);
        let ok_http = decode_hex_json(net_msgs[1].split_once(':').unwrap().1);
        assert_eq!(
            blocked["ok"], false,
            "/api/p/ loopback must fail: {blocked}"
        );
        assert_eq!(
            ok_http["status"], 200,
            "permissions.network http must succeed: {ok_http}"
        );

        // 3. Manifest cache invalidation on disable/enable
        assert_eq!(
            manager
                .hook_subscriptions(opendash_core::hooks::HookEvent::ContentBeforeSave)
                .await
                .unwrap()
                .len(),
            1
        );
        manager.disable_plugin("s01-net").await.unwrap();
        assert!(
            manager
                .hook_subscriptions(opendash_core::hooks::HookEvent::ContentBeforeSave)
                .await
                .unwrap()
                .is_empty()
        );
        manager.enable_plugin("s01-net").await.unwrap();
        assert_eq!(
            manager
                .hook_subscriptions(opendash_core::hooks::HookEvent::ContentBeforeSave)
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
