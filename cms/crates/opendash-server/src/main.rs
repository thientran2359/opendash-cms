mod app;
mod auth;
mod backup;
mod backup_archive;
mod backup_worker;
mod builtin_themes;
mod config;
mod contact;
mod content_render;
mod document;
mod error;
mod hooks;
mod media;
mod media_lifecycle;
mod media_video;
mod operations;
mod operations_worker_process;
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

use std::{
    env,
    ffi::{OsStr, OsString},
    path::Path,
    sync::Arc,
};

use app::{AppState, build_router};
use hooks::HookDispatcher;
use media::MediaStore;
use plugin::PluginManager;

fn strip_os_prefix(arg: &OsStr, prefix: &str) -> Option<OsString> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        arg.as_bytes()
            .strip_prefix(prefix.as_bytes())
            .map(|rest| OsString::from_vec(rest.to_vec()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let prefix_units: Vec<u16> = prefix.encode_utf16().collect();
        let arg_units: Vec<u16> = arg.encode_wide().collect();
        arg_units
            .strip_prefix(prefix_units.as_slice())
            .map(OsString::from_wide)
    }
    #[cfg(not(any(unix, windows)))]
    {
        arg.to_str()
            .and_then(|s| s.strip_prefix(prefix))
            .map(OsString::from)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();

    if args
        .iter()
        .any(|a| a == "--check-theme" || strip_os_prefix(a, "--check-theme=").is_some())
    {
        let path_arg: Result<OsString, &'static str> = if args.len() == 1 {
            match strip_os_prefix(&args[0], "--check-theme=") {
                Some(p) if !p.is_empty() => Ok(p),
                Some(_) => Err("Thiếu đường dẫn sau '--check-theme='."),
                None => Err("Thiếu đối số đường dẫn cho '--check-theme'."),
            }
        } else if args.len() == 2 && args[0] == "--check-theme" && !args[1].is_empty() {
            Ok(args[1].clone())
        } else {
            Err("Cú pháp đúng: opendash --check-theme <path> hoặc --check-theme=<path>")
        };

        match path_arg
            .map_err(str::to_owned)
            .and_then(|p| theme::check_theme_path(Path::new(&p)))
        {
            Ok(summary) => {
                println!("{}", serde_json::to_string(&summary)?);
                std::process::exit(0);
            }
            Err(err) => {
                println!("{}", serde_json::json!({ "ok": false, "error": err }));
                std::process::exit(1);
            }
        }
    }

    if args.iter().any(|a| a == "--check-config") {
        let paths = paths::SitePaths::from_env()?;
        let cfg = config::RuntimeConfigV1::load(&paths).map_err(|e| format!("{}: {}", e.0, e.1))?;
        println!("{}", cfg.redacted_summary_json());
        return Ok(());
    }

    if args.iter().any(|a| a == "--operations-worker") {
        let code = crate::backup_worker::run_operations_worker_from_stdio();
        std::process::exit(code);
    }

    if args.iter().any(|a| a == "--plugin-worker")
        || std::env::var_os("OPENDASH_WORKER_CHILD").is_some()
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
    let runtime_cfg =
        config::RuntimeConfigV1::load(&paths).map_err(|e| format!("{}: {}", e.0, e.1))?;
    let recovery = config::RecoveryState::new(runtime_cfg.portable.recovery_safe_mode);
    let db = storage::init_db(paths.db_path()).await?;
    storage::theme::ensure_default_starter_theme(&db, &paths.themes_dir())
        .await
        .map_err(|e| format!("{}: {}", e.0, e.1))?;
    storage::builder::migrate_demo_entry_builder_docs_on_startup(&db)
        .await
        .map_err(|e| format!("{}: {}", e.0, e.1))?;
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
    let plugin_manager = Arc::new(
        PluginManager::new(db.clone(), allow_loopback).with_recovery_state(recovery.clone()),
    );
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
        block_template_cache: crate::page::BlockTemplateCache::global().clone(),
        recovery,
        runtime_config: runtime_cfg.clone(),
    };
    storage::operations::mark_interrupted_on_startup(&db)
        .await
        .map_err(|e| format!("{}: {}", e.0, e.1))?;
    media_lifecycle::recover_pending_delete_jobs_on_startup(&db, &state.media_store)
        .await
        .map_err(|e| format!("{}: {}", e.0, e.1))?;

    let port = runtime_cfg.portable.port;
    /* OPENDASH_PORT precedence handled in RuntimeConfigV1::load */
    let _ = env::var("OPENDASH_PORT")
        .unwrap_or_else(|_| "18766".into())
        .parse::<u16>();
    let listen_ip: std::net::IpAddr = runtime_cfg.portable.listen_ip.parse()?;
    let state_for_shutdown = state.clone();
    let listener = tokio::net::TcpListener::bind((listen_ip, port)).await?;
    let address = listener.local_addr()?;
    let app = build_router(state, address.to_string());

    println!("OpenDash: http://{address}/builder");
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });
    let mut shutdown_rx_for_serve = shutdown_rx.clone();
    let serve_fut = std::future::IntoFuture::into_future(
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx_for_serve.wait_for(|v| *v).await;
        }),
    );
    tokio::pin!(serve_fut);
    let serve_res = tokio::select! {
        res = &mut serve_fut => Some(res),
        _ = shutdown_rx.wait_for(|v| *v) => None,
    };
    match serve_res {
        Some(res) => res?,
        None => {
            if let Ok(res) =
                tokio::time::timeout(std::time::Duration::from_secs(10), &mut serve_fut).await
            {
                res?;
            } else {
                eprintln!(
                    "OpenDash shutdown: HTTP drain timed out after 10s; proceeding to task drain."
                );
            }
        }
    }

    let backup_drain = state_for_shutdown
        .backup_registry
        .cancel_and_drain_all(std::time::Duration::from_secs(30))
        .await;
    let writer_drain = state_for_shutdown
        .coordinator
        .drain_for_shutdown(std::time::Duration::from_secs(15))
        .await;
    state_for_shutdown.db.close().await;
    drop(state_for_shutdown);
    if !backup_drain.drained_cleanly || writer_drain.is_err() {
        return Err(format!(
            "OpenDash shutdown incomplete: timed_out_jobs={:?}, remaining_writers={:?}",
            backup_drain.timed_out_jobs,
            writer_drain.err()
        )
        .into());
    }
    Ok(())
}

static SHUTDOWN_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

extern "C" fn handle_termination_signal(_sig: std::ffi::c_int) {
    SHUTDOWN_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
}

async fn shutdown_signal() {
    unsafe extern "C" {
        fn signal(sig: std::ffi::c_int, handler: extern "C" fn(std::ffi::c_int)) -> usize;
    }
    const SIGINT: std::ffi::c_int = 2;
    const SIGTERM: std::ffi::c_int = 15;
    unsafe {
        let _ = signal(SIGINT, handle_termination_signal);
        let _ = signal(SIGTERM, handle_termination_signal);
    }
    while !SHUTDOWN_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
