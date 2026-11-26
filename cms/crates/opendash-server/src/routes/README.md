# HTTP Routes (`opendash-server/src/routes/`)

## Overview

Axum HTTP presentation and request-validation layer for OpenDash, exporting 23 handler modules via [`mod.rs`](mod.rs) and mounted by [`../app.rs`](../app.rs) (`build_router_with_themes_root`). Handlers enforce authentication, RBAC, CSRF tokens, `If-Match` / ETag optimistic concurrency, body size limits, and `AdmittedMutation` maintenance leases while delegating all SQLite queries to [`../storage/README.md`](../storage/README.md).

See [Core Foundation](../../../../../docs/contracts/CORE_FOUNDATION.md) and [M8 Writer Inventory](../../../../../docs/contracts/M8_WRITER_INVENTORY.md) for writer admission and boundary rules.

## Interface

- **Module Index** ([`mod.rs`](mod.rs)): exports 23 submodules (`auth`, `builder`, `contact`, `content`, `dashboard`, `entries`, `extensions`, `health`, `import`, `media`, `menu`, `operations`, `patterns`, `plugin`, `public`, `public_content`, `seo`, `settings`, `sitemap`, `smtp`, `taxonomy`, `themes`, `users`).
- **Auth, Users & Dashboard** ([`auth.rs`](auth.rs), [`users.rs`](users.rs), [`dashboard.rs`](dashboard.rs)):
  - `[auth.rs:auth_setup]`, `[auth.rs:auth_login]`, `[auth.rs:auth_me]`, `[auth.rs:auth_logout]` (`/auth/*`).
  - `[users.rs:user_list]`, `[users.rs:user_create]`, `[users.rs:user_patch]`, `[users.rs:user_reset_password]`, `[users.rs:user_delete]`, `[users.rs:profile_get]`, `[users.rs:profile_put]` (`/api/admin/users/*`, `/api/admin/profile`).
  - `[dashboard.rs:dashboard_get]` (`/api/admin/dashboard`).
- **Content Types, Entries, Patterns & Taxonomy** ([`content.rs`](content.rs), [`entries.rs`](entries.rs), [`patterns.rs`](patterns.rs), [`taxonomy.rs`](taxonomy.rs)):
  - `[content.rs:content_type_list]`, `[content.rs:content_type_create]`, `[content.rs:content_type_update]`, `[content.rs:field_group_list]`, `[content.rs:field_group_create]`, `[content.rs:field_group_update]`, `[content.rs:field_group_delete]`, `[content.rs:editor_layout_get]`, `[content.rs:editor_layout_put]`, `[content.rs:effective_editor_config_get]`, `[content.rs:content_entry_publication]`, `[content.rs:content_entry_preview]`.
  - `[entries.rs:entry_list]`, `[entries.rs:entry_get]`, `[entries.rs:entry_create]`, `[entries.rs:entry_update]`, `[entries.rs:entry_delete]`, `[entries.rs:entry_composite_create]`, `[entries.rs:entry_composite_update]`, `[entries.rs:entry_translation_create]`, `[entries.rs:entry_table_get]`, `[entries.rs:entry_table_effective_config]`, `[entries.rs:entry_table_provider_action]`, `[entries.rs:entry_publication]`.
  - `[patterns.rs:pattern_list]`, `[patterns.rs:pattern_get]`, `[patterns.rs:pattern_create]`, `[patterns.rs:pattern_update]`, `[patterns.rs:pattern_delete]`, `[patterns.rs:pattern_library_list]`, `[patterns.rs:pattern_library_theme_get]`.
  - `[taxonomy.rs:taxonomy_definitions_list]`, `[taxonomy.rs:taxonomy_definitions_get]`, `[taxonomy.rs:taxonomy_definitions_create]`, `[taxonomy.rs:taxonomy_definitions_patch]`, `[taxonomy.rs:taxonomy_definitions_delete]`, `[taxonomy.rs:taxonomy_list]`, `[taxonomy.rs:taxonomy_terms_resolve]`, `[taxonomy.rs:taxonomy_get]`, `[taxonomy.rs:taxonomy_create]`, `[taxonomy.rs:taxonomy_patch]`, `[taxonomy.rs:taxonomy_delete]`, `[taxonomy.rs:taxonomy_effective_editor_config]`, `[taxonomy.rs:taxonomy_eligibility_list]`, `[taxonomy.rs:taxonomy_eligibility_put]`, `[taxonomy.rs:taxonomy_eligibility_effective]`, `[taxonomy.rs:taxonomy_eligibility_configured]`.
- **Builder, Themes, Studio, Menus & Import** ([`builder.rs`](builder.rs), [`themes.rs`](themes.rs), [`import.rs`](import.rs), [`menu.rs`](menu.rs)):
  - `[builder.rs:builder]`, `[builder.rs:serve_asset]`, `[builder.rs:block_schemas]`, `[builder.rs:entry_builder_get]`, `[builder.rs:entry_builder_put]`, `[builder.rs:entry_builder_preview]`, `[builder.rs:entry_builder_reset]`.
  - `[themes.rs:theme_list]`, `[themes.rs:theme_install]`, `[themes.rs:theme_activate]`, `[themes.rs:active_layouts]`, `[themes.rs:theme_asset]`, `[themes.rs:local_library_list]`, `[themes.rs:template_overrides_list]`, `[themes.rs:template_overrides_put]`, `[themes.rs:template_overrides_preview]`, `[themes.rs:template_overrides_reset]`, `[themes.rs:studio_targets]`, `[themes.rs:studio_preview]` (`/api/admin/studio/*`), `[themes.rs:theme_functions_get_settings]`, `[themes.rs:theme_functions_put_settings]`, `[themes.rs:theme_active_action]`, `[themes.rs:theme_functions_logs]`, `[themes.rs:dev_sync]` (`/api/admin/dev/sync`).
  - `[import.rs:import_dry_run]`, `[import.rs:import_execute]`.
  - `[menu.rs:menus_list]`, `[menu.rs:menus_create]`, `[menu.rs:menus_get]`, `[menu.rs:menus_put]`, `[menu.rs:menus_delete]`, `[menu.rs:menus_preview]`, `[menu.rs:menu_slots_get]`, `[menu.rs:menu_slots_put]`.
- **Media, Plugins & Scoped Extension Routes** ([`media.rs`](media.rs), [`plugin.rs`](plugin.rs), [`extensions.rs`](extensions.rs)):
  - `[media.rs:media_list]`, `[media.rs:media_upload]`, `[media.rs:media_video_upload]`, `[media.rs:media_get]`, `[media.rs:media_delete]`, `[media.rs:media_original]`, `[media.rs:media_thumbnail]`.
  - `[plugin.rs:plugin_list]`, `[plugin.rs:plugin_install]`, `[plugin.rs:plugin_run]`, `[plugin.rs:plugin_action]`, `[plugin.rs:plugin_ui_schema]`, `[plugin.rs:plugin_get_settings]`, `[plugin.rs:plugin_put_settings]`, `[plugin.rs:plugin_enable]`, `[plugin.rs:plugin_disable]`, `[plugin.rs:validate_declarative_ui_schema_v1]`.
  - `[extensions.rs:extension_public_get_root]`, `[extensions.rs:extension_public_get]` (`/p/{pkg}/*`), `[extensions.rs:extension_api_get_root]`, `[extensions.rs:extension_api_get]`, `[extensions.rs:extension_api_post_root]`, `[extensions.rs:extension_api_post]` (`/api/p/{pkg}/*`), `[extensions.rs:extension_public_post_root]`, `[extensions.rs:extension_public_post]` (`/api/public/p/{pkg}`, `/api/public/p/{pkg}/*`).
- **Settings, Locales, SEO, Sitemap, Contact, SMTP & Operations** ([`settings.rs`](settings.rs), [`seo.rs`](seo.rs), [`sitemap.rs`](sitemap.rs), [`contact.rs`](contact.rs), [`smtp.rs`](smtp.rs), [`operations.rs`](operations.rs)):
  - `[settings.rs:admin_i18n_bootstrap]`, `[settings.rs:general_get]`, `[settings.rs:general_put]`, `[settings.rs:public_get]`, `[settings.rs:public_put]`, `[settings.rs:reading_get]`, `[settings.rs:reading_put]`, `[settings.rs:site_options_get]`, `[settings.rs:site_options_put]`, `[settings.rs:theme_options_get]`, `[settings.rs:theme_options_put]`, `[settings.rs:permalinks_get]`, `[settings.rs:permalinks_put]`, `[settings.rs:timezones_get]`, and locale management `[settings.rs:locales_get]`, `[settings.rs:locales_create]`, `[settings.rs:locales_update]`, `[settings.rs:locales_delete]`, `[settings.rs:locales_set_default]`, `[settings.rs:locales_multilingual_put]`, `[settings.rs:locales_orphans_get]`, `[settings.rs:locales_orphans_purge]` (`/api/admin/locales*`).
  - `[seo.rs:get_entry_route]`, `[seo.rs:put_entry_route]`, `[seo.rs:get_entry_seo]`, `[seo.rs:put_entry_seo]`, `[seo.rs:get_home_seo]`, `[seo.rs:put_home_seo]`, `[seo.rs:public_fallback]`.
  - `[sitemap.rs:robots_txt]`, `[sitemap.rs:sitemap_index]`, `[sitemap.rs:sitemap_part]`.
  - `[contact.rs:public_submit]`, `[contact.rs:public_received]`, `[contact.rs:admin_list_forms]`, `[contact.rs:admin_get_form]`, `[contact.rs:admin_create_form]`, `[contact.rs:admin_update_form]`, `[contact.rs:admin_delete_form]`, `[contact.rs:admin_list_submissions]`, `[contact.rs:admin_get_submission]`, `[contact.rs:admin_delete_submission]`.
  - `[smtp.rs:get_smtp_settings]`, `[smtp.rs:put_smtp_settings]`, `[smtp.rs:test_smtp_settings]`, `[smtp.rs:notify_submission]`.
  - `[operations.rs:diagnostics]`, `[operations.rs:reconcile_plan]`, `[operations.rs:reconcile_apply]`, `[operations.rs:quarantine_purge]`, `[operations.rs:create_backup]`, `[operations.rs:list_backups]`, `[operations.rs:download_backup_file]`, `[operations.rs:delete_backup_artifact]`, `[operations.rs:get_operation_job]`, `[operations.rs:cancel_operation_job]`, `[operations.rs:upload_restore_archive]`, `[operations.rs:create_restore]`, `[operations.rs:get_recovery_status]`, `[operations.rs:activate_recovery]`.
- **Public Delivery & Health** ([`public.rs`](public.rs), [`public_content.rs`](public_content.rs), [`health.rs`](health.rs)):
  - `[public.rs:public_page]`, `[public.rs:rendered_layout_with_form_csp]`, `[public.rs:rendered_layout]`.
  - `[public_content.rs:media_file]`, `[public_content.rs:media_thumb]`, `[public_content.rs:ResolvedPublicHtml]`.
  - `[health.rs:status]`, `[health.rs:Health]` (`/api/health`).

## Invariants

- **Zero Direct SQL**: No `sqlx::query*` or `sqlx::raw_sql` calls exist in `src/routes/`; all SQLite queries live in `../storage/` (enforced by `cms/scripts/check-standards.py`).
- **Session, CSRF & Admission Leases**: Authenticated state-mutating routes enforce `require_session_with_csrf` or `require_admin_with_csrf` and serialize writes through `AdmittedMutation` (`../operations.rs`), except `operations.rs` maintenance endpoints (`reconcile_*`, `quarantine_purge`, `create_backup` using `SiteCoordinator::begin_maintenance`; `cancel_operation_job`, `delete_backup_artifact`, `create_restore` guarded without `AdmittedMutation`) and read-only CSRF-guarded `POST` preview endpoints (`builder.rs`, `themes.rs:studio_preview`, `themes.rs:template_overrides_preview`, `menu.rs:menus_preview`).
- **Optimistic Concurrency (`If-Match`)**: Builder, settings, locale, route, and SEO updates require a valid `"vN"` `If-Match` ETag header and return `Cache-Control: no-store` (`[builder.rs:parse_builder_if_match]`, `[settings.rs:parse_if_match]`, `[settings.rs:no_store_headers]`, `[seo.rs:put_entry_route]`).
- **Public CSP & Secret-Free Projection**: Public HTML responses emit `Content-Security-Policy` (`script-src 'self' 'unsafe-inline'` for trusted Theme v3 / trusted extension markup, `object-src 'none'`, `frame-ancestors 'none'`, `form-action 'self' | 'none'`) and `X-Content-Type-Options: nosniff` (`[public.rs:rendered_layout_with_form_csp]`, `[extensions.rs:format_route_output]`, `[themes.rs:studio_preview]`). Block catalogs strip template sources before returning DTOs (`[builder.rs:block_schemas]`).
- **Public Media & Draft Isolation**: Public media delivery serves only assets referenced by published public surfaces (`[public_content.rs:media_file]`, `[public_content.rs:media_thumb]`); unconfigured or missing homepage reading targets fall back to `noindex,nofollow` or 404 without leaking drafts (`[public.rs:render_home_for_locale]`, `[seo.rs:public_fallback]`).

## Side Effects

- Delegates SQLite reads/writes, filesystem media/theme/plugin/backup operations, SMTP dispatches, and Wasm plugin executions through `../storage/`, `../plugin/`, `../media.rs`, `../smtp.rs`, and `../operations.rs`.
- Enqueues asynchronous post-commit plugin hooks (`ContentAfterSave`, etc.) and projection recomputation jobs after committed mutations (`[builder.rs:enqueue_entry_builder_after_save]`, `[plugin.rs:maybe_spawn_projection_recompute]`).
- Sets/clears `opendash_session` HTTP cookies on login and logout (`[auth.rs:auth_login]`, `[auth.rs:auth_logout]`).

## Verification

Run the server route integration tests and standards boundary checks from the repository root:

```powershell
python cms/scripts/check-standards.py
cargo test --manifest-path cms/Cargo.toml -p opendash-server --locked
```
