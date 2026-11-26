use super::*;
use std::sync::Arc;
use std::time::Duration;

async fn test_app_state(label: &str, safe_mode: bool) -> (std::path::PathBuf, AppState) {
    let root =
        std::env::temp_dir().join(format!("opendash-ops-unit-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let site_root = root.join("site");
    let db_path = root.join("cms.sqlite");
    std::fs::create_dir_all(&site_root).expect("site_root");

    let paths = crate::paths::SitePaths::from_parts(site_root, db_path);
    std::fs::create_dir_all(paths.uploads_dir()).expect("uploads dir");
    std::fs::create_dir_all(paths.plugins_dir()).expect("plugins dir");
    std::fs::create_dir_all(paths.runtime_dir()).expect("runtime dir");
    let coordinator = Arc::new(
        crate::operations::SiteCoordinator::acquire(paths.clone()).expect("acquire coord"),
    );
    let db = crate::storage::init_db(paths.db_path())
        .await
        .expect("init_db");
    let runtime_config = crate::config::RuntimeConfigV1::load(&paths).expect("load runtime config");
    let recovery = crate::config::RecoveryState::new(safe_mode);
    let plugin_manager = Arc::new(
        crate::plugin::PluginManager::new(db.clone(), false).with_recovery_state(recovery.clone()),
    );
    let hooks = Arc::new(crate::hooks::HookDispatcher::new(plugin_manager.clone()));
    let content_registry = Arc::new(tokio::sync::RwLock::new(
        crate::storage::content::load_content_type_catalog(&db)
            .await
            .expect("load content catalog"),
    ));

    let state = AppState {
        db,
        ui_locale: Arc::new(std::sync::atomic::AtomicU8::new(0)),
        plugin_manager,
        hooks,
        media_store: Arc::new(crate::media::MediaStore::new(paths.uploads_dir())),
        content_registry,
        coordinator,
        backup_registry: Arc::new(crate::backup::BackupRuntimeRegistry::new()),
        block_template_cache: crate::page::BlockTemplateCache::global().clone(),
        site_cache_epoch: Arc::clone(crate::cache::SiteCacheEpoch::global()),
        render_cache: Arc::new(crate::cache::ExtensionRenderCache::new()),
        recovery,
        runtime_config,
    };
    (root, state)
}

#[tokio::test]
async fn r1_activate_recovery_blocked_during_maintenance_returns_503_and_preserves_safe_mode() {
    let (root, state) = test_app_state("r1-barrier", true).await;
    let paths = state.coordinator.paths().clone();

    // Write READY.json and config/site.json with recovery_safe_mode = true
    let ready = crate::restore::RestoreReadyMarker {
        version: 1,
        job_id: "a".repeat(64),
        schema_version: 1,
        db_sha256: "b".repeat(64),
        file_count: 1,
        recovery_safe_mode: true,
    };
    std::fs::write(
        paths.site_root().join("READY.json"),
        serde_json::to_vec(&ready).expect("ready json"),
    )
    .expect("write READY.json");
    std::fs::create_dir_all(paths.site_config_path().parent().unwrap()).expect("config dir");
    let mut portable = opendash_core::backup::PortableRuntimeConfigV1::default();
    portable.recovery_safe_mode = true;
    std::fs::write(
        paths.site_config_path(),
        serde_json::to_vec(&portable).expect("site config"),
    )
    .expect("write site.json");

    // Hold exclusive maintenance barrier
    let maint = state
        .coordinator
        .begin_maintenance(OperationKind::Backup, Duration::from_secs(5))
        .await
        .expect("enter maintenance");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let router = crate::app::build_router(state.clone(), addr.to_string());
    let srv = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "http://{addr}/api/admin/operations/recovery/activate"
        ))
        .json(&serde_json::json!({
            "expected_generation": "a".repeat(64),
            "password": "Password123!"
        }))
        .send()
        .await
        .expect("activate request");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        resp.headers().get("retry-after").is_some(),
        "503 response must include Retry-After header"
    );

    // Verify safe_mode and config/site.json were NOT modified
    assert!(state.recovery.is_safe_mode());
    let on_disk: opendash_core::backup::PortableRuntimeConfigV1 =
        serde_json::from_slice(&std::fs::read(paths.site_config_path()).expect("read site.json"))
            .expect("parse site.json");
    assert!(on_disk.recovery_safe_mode);

    drop(maint);
    srv.abort();
    state.db.close().await;
    drop(state);
    let _ = std::fs::remove_dir_all(root);
}

struct StalledBody;

impl http_body::Body for StalledBody {
    type Data = axum::body::Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        std::task::Poll::Pending
    }
}

#[tokio::test]
async fn r3_upload_restore_archive_stalled_stream_times_out_and_cleans_part_file() {
    let (root, state) = test_app_state("r3-upload-timeout", false).await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let router = crate::app::build_router(state.clone(), addr.to_string());
    let srv = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });

    let client = reqwest::Client::new();
    let setup = client
        .post(format!("http://{addr}/auth/setup"))
        .json(&serde_json::json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .expect("setup");
    assert_eq!(setup.status(), StatusCode::CREATED);

    let login = client
        .post(format!("http://{addr}/auth/login"))
        .json(&serde_json::json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .expect("login");
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("set-cookie")
        .to_str()
        .expect("cookie str")
        .split(';')
        .next()
        .expect("cookie pair")
        .to_string();
    let login_json: serde_json::Value = login.json().await.expect("login json");
    let csrf = login_json["csrf_token"]
        .as_str()
        .expect("csrf_token")
        .to_string();
    srv.abort();

    let mut headers = HeaderMap::new();
    headers.insert(axum::http::header::COOKIE, cookie.parse().unwrap());
    headers.insert("X-CSRF-Token", csrf.parse().unwrap());
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        "application/octet-stream".parse().unwrap(),
    );

    let body = axum::body::Body::new(StalledBody);
    let res = upload_restore_archive_with_timeout(
        state.clone(),
        headers,
        body,
        Duration::from_millis(50),
    )
    .await;
    match res {
        Err(err) => assert_eq!(err.0, StatusCode::REQUEST_TIMEOUT),
        Ok(_) => panic!("stalled upload must time out"),
    }

    let uploads_dir = state
        .coordinator
        .paths()
        .backups_staging_dir()
        .join("restore-uploads");
    let remaining: Vec<_> = std::fs::read_dir(&uploads_dir)
        .expect("read uploads dir")
        .collect();
    assert!(
        remaining.is_empty(),
        "timed out upload must clean up .part file"
    );

    state.db.close().await;
    drop(state);
    let _ = std::fs::remove_dir_all(root);
}
