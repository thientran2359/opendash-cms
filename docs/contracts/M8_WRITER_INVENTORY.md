# M8 Writer & Recovery Inventory (`M8-S01-T02`)

Tài liệu này liệt kê toàn bộ top-level mutators, async/background writers và startup recovery paths trong `opendash-server`, gắn với cơ chế khóa bảo trì của [`SiteCoordinator`](../../cms/crates/opendash-server/src/operations.rs).

## 1. Quy tắc chung

- **Exclusive process lock:** `SiteCoordinator::acquire(paths)` giữ exclusive OS file locks (`site.lock` và `<db>.opendash.lock`) trước `storage::init_db` và startup recovery trong [`main.rs`](../../cms/crates/opendash-server/src/main.rs).
- **HTTP mutation admission:** Mọi mutating route handler khai báo [`AdmittedMutation`](../../cms/crates/opendash-server/src/operations.rs) extractor; khi maintenance đang drain hoặc frozen, request mới bị từ chối với `503 Service Unavailable` + `Retry-After: 3` trước khi parse body hay ghi DB/FS/secrets.
- **Background completion:** `MutationLease` implements `Clone` để các tác vụ nền được sinh ra từ request đã admit (ví dụ [`SmtpService::spawn_initial_attempt`](../../cms/crates/opendash-server/src/smtp.rs)) giữ lease tới khi ghi DB/FS hoàn tất; `begin_maintenance()` chờ drain tối đa 30s trước khi cấp [`ExclusiveOpsContext`](../../cms/crates/opendash-server/src/operations.rs).
- **Nested calls:** Lưu/hook/plugin lồng nhau trong cùng request dùng chung `MutationLease` đã cấp ở top-level handler, không reacquire exclusive và không deadlock.

## 2. Inventory theo nhóm writer

| Domain | Top-level source functions | Storage / FS / Secret targets | Lease / Barrier owner |
| --- | --- | --- | --- |
| Auth | `routes::auth::{auth_setup, auth_login, auth_logout}` | `users`, `sessions`, `bootstrap` (`storage::auth`, `auth.rs`) | `AdmittedMutation` |
| Users | `routes::users::{user_create, user_patch, user_reset_password, user_delete}` | `users`, `sessions` (`storage::auth`) | `AdmittedMutation` |
| Content Types & Entries | `routes::content::content_type_create`, `routes::entries::{entry_create, entry_update, entry_delete, entry_publication}` | `content_types`, `content_entries`, `HookDispatcher` (`storage::content`, `storage::entries`, `hooks.rs`) | `AdmittedMutation` |
| Builder & Legacy Page | `routes::builder::{install, save, homepage_put, override_put, override_reset}` | `theme_blocks`, `home_page`, `builder_homepage`, `builder_overrides` (`page.rs`, `storage::builder`, `storage::overrides`, `storage::page`) | `AdmittedMutation` |
| Themes & Options | `routes::themes::{theme_install, theme_activate}`, `routes::settings::theme_options_put` | `themes/`, `runtime/staging/`, `site_active_theme`, `theme_options` (`theme.rs`, `storage::theme`, `storage::settings`) | `AdmittedMutation` |
| Theme Import | `routes::import::{import_dry_run, import_execute}` | `import_runs`, `uploads/`, `content_entries`, `media_assets` (`storage::import`) | `AdmittedMutation` |
| Settings | `routes::settings::{general_put, public_put}` | `site_general_settings`, `site_public_settings` (`storage::settings`) | `AdmittedMutation` |
| Menus & Slots | `routes::menu::{menus_create, menus_put, menus_delete, menu_slots_put}` | `site_menus`, `site_menu_items`, `site_theme_menu_slots` (`storage::menu`) | `AdmittedMutation` |
| Taxonomy | `routes::taxonomy::{taxonomy_create, taxonomy_patch, taxonomy_delete}` | `taxonomy_terms`, `entry_taxonomy_terms` (`storage::taxonomy`) | `AdmittedMutation` |
| SEO & Routes | `routes::seo::{put_entry_route, put_entry_seo, put_home_seo}` | `content_entries` route/SEO, `site_home_seo` (`storage::seo`) | `AdmittedMutation` |
| Media & Video | `routes::media::{media_upload, media_video_upload}` | `uploads/`, `runtime/staging/`, `media_assets` (`media.rs`, `media_video.rs`, `storage::media`) | `AdmittedMutation` (giữ qua `spawn_blocking`) |
| Plugins, KV & Hooks | `routes::plugin::{plugin_install, plugin_run, plugin_action, plugin_disable, plugin_enable}` | `plugins/`, `plugins`, `plugin_kv`, `plugin_logs` (`plugin::manager`, `plugin::runtime`, `plugin::worker`, `plugin::host_ops`, `storage::plugin`, `storage::plugin_kv`) | `AdmittedMutation` |
| Contact Forms & Submissions | `routes::contact::{admin_create_form, admin_update_form, admin_delete_form, admin_delete_submission, public_submit}` | `contact_forms`, `contact_submissions`, `notification_attempts` (`contact.rs`, `storage::contact`) | `AdmittedMutation` + `lease.clone()` cho SMTP nền |
| SMTP & Secrets | `routes::smtp::{put_smtp_settings, test_smtp_settings, notify_submission}`, `smtp::SmtpService::spawn_initial_attempt` | `database/secrets/smtp/`, `smtp_settings`, `notification_attempts` (`secrets.rs`, `smtp.rs`, `storage::smtp`) | `AdmittedMutation` + `MutationLease` move vào `tokio::spawn` |
| Startup Recovery | `main.rs` (`init_db`, `recover_orphaned_import_runs`, `recover_inflight_notifications`, `mark_interrupted_on_startup`, `ensure_plugins_layout_and_quarantine_stale`, `media_store.cleanup_staging`) | `import_runs`, `notification_attempts`, `operation_jobs`, `plugins/`, `uploads/` | Chạy sau `SiteCoordinator::acquire` và trước `TcpListener::bind` |
