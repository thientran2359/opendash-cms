mod app;
mod auth;
mod backup;
mod backup_archive;
mod backup_worker;
mod builtin_themes;
mod cache;
mod config;
mod contact;
mod content_render;
mod dev_mode;
mod document;
mod error;
mod field_hydrate;
mod hooks;
mod i18n;
#[cfg(test)]
mod i18n_redirects;
mod l10n;
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
mod redirects;
#[allow(dead_code)]
mod restore;
mod routes;
mod secrets;
mod seo;
mod smtp;
mod state;
mod storage;
mod template_sec;
mod theme;

use std::{
    env,
    ffi::{OsStr, OsString},
    path::Path,
    sync::Arc,
};

use app::{AppState, build_router_with_telemetry};
use hooks::HookDispatcher;
use media::MediaStore;
use plugin::PluginManager;
use redirects::RedirectTelemetryBuffer;

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

fn run_check_cli_flag<T: serde::Serialize + Send + 'static>(
    args: &[OsString],
    flag: &str,
    check_fn: impl FnOnce(&Path) -> Result<T, String> + Send + 'static,
) {
    let eq_flag = format!("{flag}=");
    if !args
        .iter()
        .any(|a| a == flag || strip_os_prefix(a, &eq_flag).is_some())
    {
        return;
    }

    let path_arg: Result<OsString, String> = if args.len() == 1 {
        match strip_os_prefix(&args[0], &eq_flag) {
            Some(p) if !p.is_empty() => Ok(p),
            Some(_) => Err(format!("Thiếu đường dẫn sau '{eq_flag}'.")),
            None => Err(format!("Thiếu đối số đường dẫn cho '{flag}'.")),
        }
    } else if args.len() == 2 && args[0] == flag && !args[1].is_empty() {
        Ok(args[1].clone())
    } else {
        Err(format!(
            "Cú pháp đúng: opendash {flag} <path> hoặc {eq_flag}<path>"
        ))
    };

    let result = path_arg.and_then(|p| {
        std::thread::Builder::new()
            .name("opendash-cli-check".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || check_fn(Path::new(&p)))
            .map_err(|e| format!("Không thể khởi tạo thread kiểm tra: {e}"))?
            .join()
            .map_err(|_| "Thread kiểm tra kết thúc bất thường.".to_owned())?
    });
    match result {
        Ok(summary) => {
            if let Ok(json) = serde_json::to_string(&summary) {
                println!("{json}");
                std::process::exit(0);
            }
            println!(
                "{}",
                serde_json::json!({ "ok": false, "error": "Không thể serialize kết quả JSON." })
            );
            std::process::exit(1);
        }
        Err(err) => {
            println!("{}", serde_json::json!({ "ok": false, "error": err }));
            std::process::exit(1);
        }
    }
}

fn cli_flag_value<'a>(args: &'a [OsString], flag: &str) -> Option<&'a OsStr> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].as_os_str())
}

fn run_l10n_cli_flags(args: &[OsString]) {
    if let Some(src) = cli_flag_value(args, "--l10n-extract") {
        let Some(out) = cli_flag_value(args, "--out") else {
            eprintln!("Thiếu --out <file.pot> cho --l10n-extract.");
            std::process::exit(1);
        };
        let domain = cli_flag_value(args, "--domain").and_then(|d| d.to_str());
        match l10n::extract::run_extract_cli(Path::new(src), Path::new(out), domain) {
            Ok(count) => {
                println!("{}", serde_json::json!({ "ok": true, "entries": count }));
                std::process::exit(0);
            }
            Err(err) => {
                println!("{}", serde_json::json!({ "ok": false, "error": err }));
                std::process::exit(1);
            }
        }
    }
    if let Some(po_path) = cli_flag_value(args, "--l10n-compile") {
        let Some(out) = cli_flag_value(args, "--out") else {
            eprintln!("Thiếu --out <file.mo> cho --l10n-compile.");
            std::process::exit(1);
        };
        let pot = cli_flag_value(args, "--pot").map(Path::new);
        match l10n::extract::run_compile_cli(Path::new(po_path), Path::new(out), pot) {
            Ok(count) => {
                println!("{}", serde_json::json!({ "ok": true, "entries": count }));
                std::process::exit(0);
            }
            Err(err) => {
                println!("{}", serde_json::json!({ "ok": false, "error": err }));
                std::process::exit(1);
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();

    run_check_cli_flag(&args, "--check-theme", theme::check_theme_path);
    run_check_cli_flag(&args, "--check-plugin", plugin::check_plugin_path);
    run_l10n_cli_flags(&args);

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
        if let Ok(handle) = std::thread::Builder::new()
            .name("opendash-plugin-worker".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(crate::plugin::worker::run_worker_from_stdio)
        {
            let _ = handle.join();
        }
        return Ok(());
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main(args))
}

struct DevCliOptions {
    theme_dir: Option<std::path::PathBuf>,
    plugin_dirs: Vec<std::path::PathBuf>,
    allow_dev_remote: bool,
}

fn parse_dev_cli_options(args: &[OsString]) -> Result<DevCliOptions, String> {
    let mut theme_dir: Option<std::path::PathBuf> = None;
    let mut plugin_dirs: Vec<std::path::PathBuf> = Vec::new();
    let mut allow_dev_remote = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--allow-dev-remote" {
            allow_dev_remote = true;
            i += 1;
            continue;
        }
        if arg == "--dev-theme" {
            let next = args
                .get(i + 1)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| "Thiếu đường dẫn thư mục sau '--dev-theme'.".to_owned())?;
            theme_dir = Some(std::path::PathBuf::from(next));
            i += 2;
            continue;
        }
        if let Some(rest) = strip_os_prefix(arg, "--dev-theme=") {
            if rest.is_empty() {
                return Err("Thiếu đường dẫn thư mục sau '--dev-theme='.".to_owned());
            }
            theme_dir = Some(std::path::PathBuf::from(rest));
            i += 1;
            continue;
        }
        if arg == "--dev-plugin" {
            let next = args
                .get(i + 1)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| "Thiếu đường dẫn thư mục sau '--dev-plugin'.".to_owned())?;
            plugin_dirs.push(std::path::PathBuf::from(next));
            i += 2;
            continue;
        }
        if let Some(rest) = strip_os_prefix(arg, "--dev-plugin=") {
            if rest.is_empty() {
                return Err("Thiếu đường dẫn thư mục sau '--dev-plugin='.".to_owned());
            }
            plugin_dirs.push(std::path::PathBuf::from(rest));
            i += 1;
            continue;
        }
        i += 1;
    }

    if theme_dir.is_none()
        && let Some(env_theme) = env::var_os("OPENDASH_DEV_THEME_DIR")
        && !env_theme.is_empty()
    {
        theme_dir = Some(std::path::PathBuf::from(env_theme));
    }
    if plugin_dirs.is_empty()
        && let Some(env_plugins) = env::var_os("OPENDASH_DEV_PLUGIN_DIR")
        && !env_plugins.is_empty()
    {
        for p in env::split_paths(&env_plugins) {
            if !p.as_os_str().is_empty() {
                plugin_dirs.push(p);
            }
        }
    }

    Ok(DevCliOptions {
        theme_dir,
        plugin_dirs,
        allow_dev_remote,
    })
}

async fn async_main(args: Vec<OsString>) -> Result<(), Box<dyn std::error::Error>> {
    let dev_opts = parse_dev_cli_options(&args)?;
    let paths = paths::SitePaths::from_env()?;
    let coordinator = Arc::new(operations::SiteCoordinator::acquire(paths)?);
    let paths = coordinator.paths().clone();
    let runtime_cfg =
        config::RuntimeConfigV1::load(&paths).map_err(|e| format!("{}: {}", e.0, e.1))?;
    let has_dev_flags = dev_opts.theme_dir.is_some() || !dev_opts.plugin_dirs.is_empty();
    dev_mode::validate_startup_guards(
        has_dev_flags,
        &runtime_cfg.portable,
        dev_opts.allow_dev_remote,
    )?;
    let recovery = config::RecoveryState::new(runtime_cfg.portable.recovery_safe_mode);
    if has_dev_flags {
        let dev_engine = wasmi::Engine::default();
        let dev_cfg = dev_mode::DevOverlayConfig::load_and_validate(
            dev_opts.theme_dir,
            dev_opts.plugin_dirs,
            recovery.clone(),
            &dev_engine,
        )?;
        eprintln!(
            "OpenDash DEV MODE active: theme={:?}, plugins={:?}",
            dev_cfg.dev_theme_id(),
            dev_cfg.dev_plugin_ids()
        );
        dev_mode::DevOverlayConfig::init_global(Arc::new(dev_cfg));
    }
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
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let plugin_manager = Arc::new(
        PluginManager::with_roots(
            db.clone(),
            allow_loopback,
            paths.plugins_dir(),
            paths.secrets_dir(),
        )
        .with_themes_root(paths.themes_dir())
        .with_recovery_state(recovery.clone()),
    );
    let hooks = Arc::new(HookDispatcher::with_async_queue(
        plugin_manager.clone(),
        coordinator.maintenance_cancel_rx(),
        shutdown_rx.clone(),
    ));
    let content_registry = Arc::new(tokio::sync::RwLock::new(
        storage::content::load_content_type_catalog(&db)
            .await
            .map_err(|e| format!("{}: {}", e.0, e.1))?,
    ));
    let configured_ui_locale = {
        let mut conn = db.acquire().await?;
        storage::settings::get_cms_locale_in_conn(&mut conn)
            .await
            .map_err(|e| format!("{}: {}", e.0, e.1))?
    };
    let state = AppState {
        db: db.clone(),
        ui_locale: Arc::new(std::sync::RwLock::new(i18n::encode_ui_locale_str(
            &configured_ui_locale,
        ))),
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
        site_cache_epoch: Arc::clone(crate::cache::SiteCacheEpoch::global()),
        render_cache: Arc::new(crate::cache::ExtensionRenderCache::new()),
        catalog_registry: {
            let reg = Arc::clone(crate::l10n::CatalogRegistry::global());
            let _ = reg.reload_site_with_active_packages(&db, &paths).await;
            reg
        },
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
    let themes_root = crate::paths::SitePaths::from_env()
        .map(|p| p.themes_dir())
        .unwrap_or_else(|_| std::path::PathBuf::from("themes"));
    let (telemetry, telemetry_drop_guard) = RedirectTelemetryBuffer::spawn_with_coordinator(
        state.db.clone(),
        state.coordinator.clone(),
    );
    let app = build_router_with_telemetry(
        state,
        address.to_string(),
        themes_root,
        Some(telemetry.clone()),
    );

    println!("OpenDash: http://{address}/builder");
    let coordinator_for_signal = state_for_shutdown.coordinator.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        coordinator_for_signal.notify_shutdown();
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
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), telemetry.flush_now()).await;
    drop(telemetry_drop_guard);
    drop(telemetry);
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
