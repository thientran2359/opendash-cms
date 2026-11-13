mod app;
mod auth;
mod content_render;
mod error;
mod hooks;
mod media;
mod package;
mod page;
mod paths;
mod plugin;
mod public_context;
mod routes;
mod storage;
mod theme;

use std::{env, net::Ipv4Addr, sync::Arc};

use app::{AppState, build_router};
use hooks::HookDispatcher;
use media::MediaStore;
use plugin::PluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|a| a == "--plugin-worker")
        || std::env::var("OPENDASH_WORKER_CHILD").is_ok()
    {
        crate::plugin::worker::run_worker_from_stdio();
        return Ok(());
    }

    let paths = paths::SitePaths::from_env()?;
    let db = storage::init_db(paths.db_path()).await?;

    let allow_loopback = env::var("OPENDASH_PLUGIN_ALLOW_LOOPBACK")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);
    let plugin_manager = Arc::new(PluginManager::new(db.clone(), allow_loopback));
    let hooks = Arc::new(HookDispatcher::new(plugin_manager.clone()));
    let state = AppState {
        db: db.clone(),
        plugin_manager,
        hooks,
        media_store: Arc::new(MediaStore::new(paths.uploads_dir())),
    };

    let port = env::var("OPENDASH_PORT")
        .unwrap_or_else(|_| "18766".into())
        .parse::<u16>()?;
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
    let address = listener.local_addr()?;
    let app = build_router(state, address.to_string());

    println!("OpenDash: http://{address}/builder");
    axum::serve(listener, app).await?;
    Ok(())
}
