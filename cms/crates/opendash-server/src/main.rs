mod app;
mod auth;
mod backup;
mod backup_archive;
mod backup_worker;
mod contact;
mod content_render;
mod document;
mod error;
mod hooks;
mod media;
mod media_lifecycle;
mod media_video;
mod operations;
mod package;
mod page;
mod paths;
mod plugin;
mod public_context;
#[allow(dead_code)]
mod restore;
mod routes;
mod secrets;
mod seo;
mod smtp;
mod storage;
mod template_sec;
mod theme;

use std::{env, net::Ipv4Addr, sync::Arc};

use app::{AppState, build_router};
use hooks::HookDispatcher;
use media::MediaStore;
use plugin::PluginManager;


fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|a| a == "--operations-worker") {
        let code = crate::backup_worker::run_operations_worker_from_stdio();
        std::process::exit(code);
    }

    if std::env::args().any(|a| a == "--plugin-worker")
        || std::env::var("OPENDASH_WORKER_CHILD").is_ok()
    {
        let _ = std::thread::spawn(crate::plugin::worker::run_worker_from_stdio).join();
        return Ok(());
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    let paths = paths::SitePaths::from_env()?;
    let coordinator = Arc::new(operations::SiteCoordinator::acquire(paths)?);
    let paths = coordinator.paths().clone();
    let db = storage::init_db(paths.db_path()).await?;
    storage::import::recover_orphaned_import_runs(&db, &paths.uploads_dir())
        .await
        .map_err(|e| format!("{}: {}", e.0, e.1))?;
    storage::smtp::recover_inflight_notifications(&db)
        .await
        .map_err(|e| format!("{}: {}", e.0, e.1))?;
    storage::operations::mark_interrupted_on_startup(&db)
        .await
        .map_err(|e| format!("{}: {}", e.0, e.1))?;
    let _ = storage::plugin::ensure_plugins_layout_and_quarantine_stale(&paths.plugins_dir());

    let allow_loopback = env::var("OPENDASH_PLUGIN_ALLOW_LOOPBACK")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);
    let plugin_manager = Arc::new(PluginManager::new(db.clone(), allow_loopback));
    let hooks = Arc::new(HookDispatcher::new(plugin_manager.clone()));
    let content_registry = Arc::new(tokio::sync::RwLock::new(
        storage::content::load_content_type_catalog(&db)
            .await
            .map_err(|e| format!("{}: {}", e.0, e.1))?,
    ));
    let state = AppState {
        db: db.clone(),
        plugin_manager,
        hooks,
        media_store: {
            let store = Arc::new(MediaStore::new(paths.uploads_dir()));
            store.cleanup_staging().await;
            store
        },
        content_registry,
        coordinator,
        backup_registry: Arc::new(crate::backup::BackupRuntimeRegistry::new()),
    };
    storage::operations::mark_interrupted_on_startup(&db)
        .await
        .map_err(|e| format!("{}: {}", e.0, e.1))?;
    media_lifecycle::recover_pending_delete_jobs_on_startup(&db, &state.media_store)
        .await
        .map_err(|e| format!("{}: {}", e.0, e.1))?;

    let port = env::var("OPENDASH_PORT")
        .unwrap_or_else(|_| "18766".into())
        .parse::<u16>()?;
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
    let address = listener.local_addr()?;
    let app = build_router(state, address.to_string());

    println!("OpenDash: http://{address}/builder");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
