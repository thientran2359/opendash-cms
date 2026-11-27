# OpenDash Server (`opendash-server`)

## Overview

Single-binary Axum server (`[[bin]] name = "opendash"` in [`Cargo.toml`](Cargo.toml)) depending on [`opendash-core`](../opendash-core/README.md) and embedding the built React Admin bundle (`cms/admin/dist/`) via `rust-embed` (`[app.rs:AdminAssets]`). Orchestrates HTTP routing ([`src/routes/README.md`](src/routes/README.md)), SQLite persistence and migrations `0001_builder_slice.sql`–`0057_manual_redirects_and_404_log.sql` ([`src/storage/README.md`](src/storage/README.md)), and the unified isolated Wasmi `=2.0.0` + Boa `=0.22.0` JS/Wasm runtime ([`src/plugin/README.md`](src/plugin/README.md)), alongside top-level infrastructure modules (`src/*.rs` excluding `main.rs` and `app.rs`), `src/builtin_blocks/`, `src/builtin_themes/`, and domain-grouped `tests/`.

See [Core Foundation](../../../docs/contracts/CORE_FOUNDATION.md), [CMS Architecture](../../../docs/architecture/CMS_ARCHITECTURE.md), and [M8 Writer Inventory](../../../docs/contracts/M8_WRITER_INVENTORY.md).

## Interface

- **CLI & Process Entrypoints** ([`src/main.rs`](src/main.rs)):
  - HTTP server (`async_main`, with optional local Dev Live-Edit flags `--dev-theme <dir>`, `--dev-plugin <dir>`, `--allow-dev-remote`), `--check-theme <path>`, `--check-plugin <path>`, `--check-config`, `--operations-worker` (`[backup_worker.rs:run_operations_worker_from_stdio]`), and `--plugin-worker` (`[plugin/worker.rs:run_worker_from_stdio]`).
- **Router, Middleware & App State** ([`src/app.rs`](src/app.rs), [`src/state.rs`](src/state.rs)):
  - `[state.rs:AppState]`, `[app.rs:AdminAssets]`, `[app.rs:build_router]`, `[app.rs:build_router_with_themes_root]`, and `[app.rs:build_router_with_telemetry]` (Host/Origin validation, i18n error localization, safe-mode `X-Robots-Tag`, fallback routing, redirect telemetry extension).
- **Subsystems**:
  - [`src/routes/README.md`](src/routes/README.md): 25 Axum handler modules (`auth`, `builder`, `contact`, `content`, `entries`, `extensions`, `fields`, `operations`, `plugin`, `public`, `public_content`, `redirects`, `seo`, `settings`, `themes`, etc.).
  - [`src/storage/README.md`](src/storage/README.md): 34 SQLite storage domain modules owning 100% of `sqlx::query*` calls and migrations `0001`–`0057`.
  - [`src/plugin/README.md`](src/plugin/README.md): Unified Wasmi `=2.0.0` + Boa `=0.22.0` (`[plugin/js_runtime.rs]`) sandbox, `--plugin-worker` IPC (128 MiB OS memory limit), `StagedCommitGuard`, and DNS-pinned HTTP egress.
- **Core Infrastructure Modules (`src/*.rs`)**:
  - **Config, Paths, Errors & Auth** ([`src/config.rs`](src/config.rs), [`src/paths.rs`](src/paths.rs), [`src/error.rs`](src/error.rs), [`src/auth.rs`](src/auth.rs), [`src/secrets.rs`](src/secrets/mod.rs), [`src/i18n.rs`](src/i18n.rs), [`src/i18n_redirects.rs`](src/i18n_redirects.rs), [`src/redirects.rs`](src/redirects.rs)): `[config.rs:RuntimeConfigV1]`, `[config.rs:RecoveryState]`, `[paths.rs:SitePaths]`, `[error.rs:ApiError]`, `[error.rs:ApiResult]`, `[auth.rs:SessionContext]`, `[auth.rs:Role]`, `[secrets.rs:SecretStore]`, `[i18n.rs:localize_error_response]`, `[i18n.rs:PrelocalizedErrorResponse]`, and `[redirects.rs:RedirectTelemetryBuffer]`.
  - **Hooks, Cache, Dev Mode & Packages** ([`src/hooks.rs`](src/hooks/mod.rs), [`src/cache.rs`](src/cache.rs), [`src/dev_mode.rs`](src/dev_mode/mod.rs), [`src/package.rs`](src/package.rs), [`src/theme.rs`](src/theme/mod.rs)): `[hooks.rs:HookDispatcher]`, `[hooks.rs:AsyncHookQueue]`, `[cache.rs:SiteCacheEpoch]`, `[cache.rs:ExtensionRenderCache]`, `[dev_mode.rs:DevOverlayConfig]`, bounded ZIP archive readers (`package.rs`, `[package.rs:validate_theme_relative_path]`), `[theme.rs:parse_theme_zip_v3]`, and `[theme.rs:check_theme_path]`.
  - **Rendering, Template Security & SEO** ([`src/page.rs`](src/page/mod.rs), [`src/template_sec.rs`](src/template_sec/mod.rs), [`src/content_render.rs`](src/content_render/mod.rs), [`src/document.rs`](src/document/mod.rs), [`src/public_context.rs`](src/public_context.rs), [`src/seo.rs`](src/seo.rs)): `[page.rs:BlockTemplateCache]`, `[template_sec.rs:restricted_environment_with_config]`, `[template_sec.rs:validate_and_compile_csec_composition_with_theme_urls]`, `[template_sec.rs:validate_safe_scoped_block_css]`, `[public_context.rs:PublicRequestContext]`, `[seo.rs:validate_seo_input]`, `[seo.rs:render_robots_txt]`, and `[seo.rs:render_sitemap_index]`.
  - **Media, Contact & SMTP** ([`src/media.rs`](src/media/mod.rs), [`src/media_video.rs`](src/media_video/mod.rs), [`src/media_lifecycle.rs`](src/media_lifecycle/mod.rs), [`src/contact.rs`](src/contact.rs), [`src/smtp.rs`](src/smtp.rs)): `[media.rs:MediaStore]`, pure-Rust MP4 validation (`media_video.rs`), `[media_lifecycle.rs:recover_pending_delete_jobs_on_startup]`, contact admission/validation (`contact.rs`), and `[smtp.rs:SmtpService]`.
  - **Operations, Backup & Restore** ([`src/operations.rs`](src/operations.rs), [`src/operations_worker_process.rs`](src/operations_worker_process.rs), [`src/backup.rs`](src/backup.rs), [`src/backup_archive.rs`](src/backup_archive.rs), [`src/backup_worker.rs`](src/backup_worker/mod.rs), [`src/restore.rs`](src/restore.rs)): `[operations.rs:SiteCoordinator]`, `[operations.rs:AdmittedMutation]`, `[operations.rs:MutationLease]`, `[operations.rs:ExclusiveOpsContext]`, `[restore.rs:restore_to_new_root]`, and `age`-encrypted backup archive workers.

## Invariants

- **Single-Instance & Maintenance Admission**: `SiteCoordinator::acquire` enforces an exclusive per-site lockfile, and `AdmittedMutation` gates concurrent writers during maintenance/backup/restore windows (`[main.rs:async_main]`, `[operations.rs:SiteCoordinator]`).
- **Host/Origin & CSRF Protection**: `build_router_with_themes_root` validates a single `Host` header against the bound address or `trusted_host`, and rejects cross-origin mutating requests (except `/api/public/contact/*`) before handler dispatch (`[app.rs:build_router_with_themes_root]`).
- **Startup Crash Recovery**: Before accepting HTTP traffic, `async_main` recovers orphaned theme imports, inflight SMTP notifications, interrupted operation jobs, pending media delete jobs, and stale plugin staging directories (`[main.rs:async_main]`).
- **Template & Subprocess Sandboxing**: All theme/block rendering runs in MiniJinja `C-SEC` mode with `TEMPLATE_FUEL_LIMIT = 10_000`, `TEMPLATE_RECURSION_LIMIT = 16`, and `MAX_RENDERED_HTML_BYTES = 512 KiB` (`[template_sec.rs:restricted_environment_with_config]`); Wasm plugins and backup/restore archives execute in dedicated `--plugin-worker` / `--operations-worker` child processes (`[main.rs:main]`).
- **Graceful Drain on Shutdown**: On `SIGINT`/`SIGTERM`, `async_main` waits up to `10s` for HTTP drain, `30s` for `BackupRuntimeRegistry::cancel_and_drain_all`, and `15s` for `SiteCoordinator::drain_for_shutdown` before closing SQLite (`[main.rs:async_main]`).

## Side Effects

- Binds a TCP listener on `listen_ip:port`, acquires the site lockfile, opens SQLite WAL, and runs startup recovery tasks (`[main.rs:async_main]`).
- Reads and writes runtime files under `SitePaths` (`database/`, `themes/`, `plugins/`, `uploads/`, `secrets/`).
- Spawns `--plugin-worker` and `--operations-worker` subprocesses, background async hook workers (`ASYNC_HOOK_WORKERS = 2`), media cleanup/delete jobs, and outbound SMTP connections via `lettre` (`[main.rs:async_main]`, `[hooks.rs:HookDispatcher]`, `[smtp.rs:SmtpService]`).

## Verification

Run the standards, formatting, build, and full server test suites from the repository root:

```powershell
python cms/scripts/check-standards.py
cargo fmt --manifest-path cms/Cargo.toml --all --check
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml -p opendash-server --release --locked
```
