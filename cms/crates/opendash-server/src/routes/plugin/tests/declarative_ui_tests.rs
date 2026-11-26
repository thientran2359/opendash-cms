use super::*;

#[test]
fn schema_v1_enforces_quotas_forbids_secrets_and_rejects_forged_actions() {
    let allowed = vec!["sync".to_string()];

    // 1. Secret field rejected
    let secret_schema = DeclarativeUiSchemaV1 {
        plugin_id: "p1".into(),
        schema_version: 1,
        screens: vec![UiScreen {
            id: "s1".into(),
            title: "Main".into(),
            nodes: vec![UiNode {
                kind: "field".into(),
                id: "api_key".into(),
                label: Some("API Key".into()),
                field_type: Some("secret".into()),
                action_id: None,
                page_size: None,
                columns: vec![],
                options: vec![],
                children: vec![],
            }],
        }],
    };
    assert!(validate_declarative_ui_schema_v1(&secret_schema, &allowed).is_err());

    // 2. Depth > 8 rejected
    let mut deep = UiNode {
        kind: "tabs".into(),
        id: "d9".into(),
        label: None,
        field_type: None,
        action_id: None,
        page_size: None,
        columns: vec![],
        options: vec![],
        children: vec![],
    };
    for i in (1..9).rev() {
        deep = UiNode {
            kind: "tabs".into(),
            id: format!("d{i}"),
            label: None,
            field_type: None,
            action_id: None,
            page_size: None,
            columns: vec![],
            options: vec![],
            children: vec![deep],
        };
    }
    let deep_schema = DeclarativeUiSchemaV1 {
        plugin_id: "p1".into(),
        schema_version: 1,
        screens: vec![UiScreen {
            id: "s1".into(),
            title: "Main".into(),
            nodes: vec![deep],
        }],
    };
    assert!(validate_declarative_ui_schema_v1(&deep_schema, &allowed).is_err());

    // 3. Forged action rejected
    let forged_schema = DeclarativeUiSchemaV1 {
        plugin_id: "p1".into(),
        schema_version: 1,
        screens: vec![UiScreen {
            id: "s1".into(),
            title: "Main".into(),
            nodes: vec![UiNode {
                kind: "button".into(),
                id: "b1".into(),
                label: Some("Hack".into()),
                field_type: None,
                action_id: Some("forged_action".into()),
                page_size: None,
                columns: vec![],
                options: vec![],
                children: vec![],
            }],
        }],
    };
    assert!(validate_declarative_ui_schema_v1(&forged_schema, &allowed).is_err());
}

#[tokio::test]
async fn plugin_settings_http_routes_enforce_cas_no_store_redaction_and_secret_lifecycle() {
    use std::io::Write as _;

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!(
        "s02_http_plugin_settings_{}_{}",
        std::process::id(),
        nanos
    ));
    std::fs::create_dir_all(&temp_root).unwrap();

    let paths = crate::paths::SitePaths::resolve_from(
        Some(temp_root.as_os_str().to_os_string()),
        None,
        &temp_root,
    );
    let coordinator = Arc::new(crate::operations::SiteCoordinator::acquire(paths.clone()).unwrap());
    let db = crate::storage::init_db(paths.db_path()).await.unwrap();
    let plugin_manager = Arc::new(PluginManager::with_roots(
        db.clone(),
        false,
        paths.plugins_dir(),
        paths.secrets_dir(),
    ));
    let hooks = Arc::new(HookDispatcher::new(plugin_manager.clone()));
    let content_registry = Arc::new(tokio::sync::RwLock::new(
        crate::storage::content::load_content_type_catalog(&db)
            .await
            .unwrap(),
    ));
    let runtime_config = crate::config::RuntimeConfigV1::load(&paths).unwrap();
    let state = crate::app::AppState {
        db: db.clone(),
        ui_locale: Arc::new(std::sync::atomic::AtomicU8::new(0)),
        plugin_manager: plugin_manager.clone(),
        hooks,
        media_store: Arc::new(crate::media::MediaStore::new(paths.uploads_dir())),
        content_registry,
        coordinator,
        backup_registry: Arc::new(crate::backup::BackupRuntimeRegistry::new()),
        block_template_cache: crate::page::BlockTemplateCache::global().clone(),
        site_cache_epoch: Arc::clone(crate::cache::SiteCacheEpoch::global()),
        render_cache: Arc::new(crate::cache::ExtensionRenderCache::new()),
        recovery: crate::config::RecoveryState::new(false),
        runtime_config,
    };

    let wasm_bytes = wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32) i32.const 0)
        )"#,
    )
    .unwrap();
    let manifest_json = serde_json::json!({
        "id": "settings-e2e",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": opendash_core::plugin::CURRENT_HOST_VERSION,
        "name": "Settings E2E",
        "settings_schema": [
            {
                "key": "api_key",
                "label": "API Key",
                "type": "secret",
                "required": true
            },
            {
                "key": "opt_secret",
                "label": "Optional Secret",
                "type": "secret",
                "required": false
            },
            {
                "key": "endpoint",
                "label": "Endpoint",
                "type": "url",
                "required": true,
                "default": "https://example.com"
            }
        ]
    })
    .to_string();

    let mut zip_buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut zip_buf);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest_json.as_bytes()).unwrap();
        zip.start_file("plugin.wasm", opts).unwrap();
        zip.write_all(&wasm_bytes).unwrap();
        zip.finish().unwrap();
    }
    plugin_manager
        .install_plugin_checked(&zip_buf.into_inner())
        .await
        .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let expected_host = format!("127.0.0.1:{port}");
    let origin = format!("http://{expected_host}");
    let router =
        crate::app::build_router_with_themes_root(state, expected_host.clone(), paths.themes_dir());
    let server_task = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    let client = reqwest::Client::new();
    let setup_resp = client
        .post(format!("{origin}/auth/setup"))
        .header("Origin", &origin)
        .json(&serde_json::json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(setup_resp.status().is_success());

    let login_resp = client
        .post(format!("{origin}/auth/login"))
        .header("Origin", &origin)
        .json(&serde_json::json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(login_resp.status().is_success());
    let cookie = login_resp
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let me: Value = client
        .get(format!("{origin}/auth/me"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let csrf = me["csrf_token"].as_str().unwrap().to_string();

    // 404 on invalid or missing plugin id.
    let not_found_resp = client
        .get(format!(
            "{origin}/api/admin/plugins/missing-plugin/settings"
        ))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(not_found_resp.status(), reqwest::StatusCode::NOT_FOUND);

    // Disable plugin and verify GET/PUT settings remain accessible while disabled.
    let disable_resp = client
        .post(format!("{origin}/api/admin/plugins/settings-e2e/disable"))
        .header("Origin", &origin)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();
    assert!(disable_resp.status().is_success());

    // Initial GET -> v1, no-store, ETag "v1", redacted unconfigured.
    let get1 = client
        .get(format!("{origin}/api/admin/plugins/settings-e2e/settings"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(get1.status(), reqwest::StatusCode::OK);
    assert_eq!(
        get1.headers().get(reqwest::header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    assert_eq!(get1.headers().get(reqwest::header::ETAG).unwrap(), "\"v1\"");
    let snap1: Value = get1.json().await.unwrap();
    assert_eq!(snap1["version"], 1);
    assert_eq!(
        snap1["values"]["api_key"],
        serde_json::json!({ "configured": false })
    );

    // Missing If-Match -> 428.
    let r428 = client
        .put(format!("{origin}/api/admin/plugins/settings-e2e/settings"))
        .header("Origin", &origin)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&serde_json::json!({
            "values": {
                "api_key": "http_secret_v1",
                "endpoint": "https://one.example.com"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r428.status(), reqwest::StatusCode::PRECONDITION_REQUIRED);

    let sec_dir = paths.plugin_secrets_dir("settings-e2e");
    let file_count = |dir: &std::path::Path| -> usize {
        if !dir.exists() {
            0
        } else {
            std::fs::read_dir(dir)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .count()
        }
    };

    // Stale If-Match -> 412 with 0 orphan secret files.
    let r412 = client
        .put(format!("{origin}/api/admin/plugins/settings-e2e/settings"))
        .header("Origin", &origin)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v99\"")
        .json(&serde_json::json!({
            "values": {
                "api_key": "http_secret_orphan",
                "endpoint": "https://one.example.com"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r412.status(), reqwest::StatusCode::PRECONDITION_FAILED);
    assert_eq!(file_count(&sec_dir), 0);

    // Save v1 -> v2 with two secrets; verify redaction & SecretStore readback.
    let put1 = client
        .put(format!("{origin}/api/admin/plugins/settings-e2e/settings"))
        .header("Origin", &origin)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&serde_json::json!({
            "values": {
                "api_key": "http_secret_v1",
                "opt_secret": "opt_v1",
                "endpoint": "https://one.example.com"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put1.status(), reqwest::StatusCode::OK);
    assert_eq!(put1.headers().get(reqwest::header::ETAG).unwrap(), "\"v2\"");
    let put1_text = put1.text().await.unwrap();
    assert!(!put1_text.contains("http_secret_v1"));
    assert!(!put1_text.contains("secret_ref"));
    let put1_json: Value = serde_json::from_str(&put1_text).unwrap();
    assert_eq!(
        put1_json["values"]["api_key"],
        serde_json::json!({ "configured": true })
    );
    assert_eq!(
        put1_json["values"]["opt_secret"],
        serde_json::json!({ "configured": true })
    );
    assert_eq!(file_count(&sec_dir), 2);

    // Clear opt_secret & null-preserve api_key across endpoint change -> v3.
    let put2 = client
        .put(format!("{origin}/api/admin/plugins/settings-e2e/settings"))
        .header("Origin", &origin)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v2\"")
        .json(&serde_json::json!({
            "values": {
                "api_key": null,
                "opt_secret": { "clear": true },
                "endpoint": "https://two.example.com"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put2.status(), reqwest::StatusCode::OK);
    assert_eq!(put2.headers().get(reqwest::header::ETAG).unwrap(), "\"v3\"");
    let put2_json: Value = put2.json().await.unwrap();
    assert_eq!(
        put2_json["values"]["api_key"],
        serde_json::json!({ "configured": true })
    );
    assert_eq!(
        put2_json["values"]["opt_secret"],
        serde_json::json!({ "configured": false })
    );
    assert_eq!(put2_json["values"]["endpoint"], "https://two.example.com");
    assert_eq!(file_count(&sec_dir), 1);

    let raw_opts = crate::storage::plugin::read_raw_plugin_option_map(&db, "settings-e2e")
        .await
        .unwrap();
    let stored_ref = raw_opts["api_key"]["secret_ref"].as_str().unwrap();
    assert_eq!(
        crate::secrets::SecretStore::for_plugin_dir(sec_dir.clone())
            .read_plugin_secret(stored_ref)
            .unwrap(),
        "http_secret_v1"
    );

    server_task.abort();
    drop(db);
    let _ = std::fs::remove_dir_all(&temp_root);
}
