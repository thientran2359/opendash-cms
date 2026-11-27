# SQLite Storage Layer (`opendash-server/src/storage/`)

## Overview

Owns **100% of SQLite queries (`sqlx::query*`, `sqlx::raw_sql`)** in OpenDash across 34 domain storage modules (`admin_extensions`–`theme`, `fields`, `redirects`, `entry_lifecycle`, and `contact`) wired through [`mod.rs`](mod.rs) (plus `s03_media_acl_tests.rs`, `theme_tests/`, `operations_tests.rs`, `smtp_tests.rs`, and `settings_timezones_2026f.txt`). Configures SQLite WAL mode, `foreign_keys = ON`, and `5s` `busy_timeout`, runs `preflight_builtin_key_collisions`, applies migrations `0001_builder_slice.sql` through `0057_manual_redirects_and_404_log.sql` (`0016` omitted) from `cms/migrations/`, and seeds embedded core blocks in `[mod.rs:init_db]`.

See [Core Foundation](../../../../../docs/contracts/CORE_FOUNDATION.md) and [M8 Writer Inventory](../../../../../docs/contracts/M8_WRITER_INVENTORY.md) for persistence and admission rules.

## Interface

- **Pool Bootstrap & Health** ([`mod.rs`](mod.rs)): `[mod.rs:init_db]`, `[mod.rs:check_db_ready]`.
- **Auth & Dashboard** ([`auth.rs`](auth.rs), [`dashboard.rs`](dashboard.rs)): `[auth.rs:UserSummary]`, `[auth.rs:UsersPage]` (plus `pub(crate)` user/session CRUD), `[dashboard.rs:get_counts]`, `[dashboard.rs:DashboardCounts]`, `[dashboard.rs:PublicationCounts]`.
- **Content Types, Field Groups, Entries, Lifecycle, Patterns & Taxonomy** ([`content.rs`](content/mod.rs), [`entries.rs`](entries/mod.rs), [`entry_lifecycle.rs`](entry_lifecycle/mod.rs), [`entry_table.rs`](entry_table/mod.rs), [`patterns.rs`](patterns.rs), [`taxonomy.rs`](taxonomy/mod.rs)):
  - `[content.rs:load_effective_content_schema]`, `[content.rs:load_effective_content_schema_in_tx]`, `[content.rs:insert_user_content_type]`, `[content.rs:update_user_content_type]`, `[content.rs:list_content_types_with_meta]`, `[content.rs:load_content_type_catalog]`, `[content.rs:create_user_field_group]`, `[content.rs:update_user_field_group]`, `[content.rs:withdraw_user_field_group]`, `[content.rs:sync_theme_declarations_in_tx]`, `[content.rs:sync_plugin_cnt_declarations_in_tx]`, `[content.rs:get_editor_layout]`, `[content.rs:put_editor_layout]`, `[content.rs:get_effective_editor_config]`.
  - `[entries.rs:get_entry]`, `[entries.rs:list_entries]`, `[entries.rs:list_published_page]`, `[entries.rs:create_entry_with_fence]`, `[entries.rs:update_entry_with_fence]`, `[entries.rs:prepare_composite_entry_save]`, `[entries.rs:commit_composite_entry_save]`, `[entries.rs:prepare_entry_translation]`, `[entries.rs:commit_entry_translation]`, `[entries.rs:delete_entry]`, `[entries.rs:set_publication_status]`, `[entries.rs:PluginActionFence]`.
  - `[entry_lifecycle.rs:LifecycleOperation]`, `[entry_lifecycle.rs:LifecycleCommand]`, `[entry_lifecycle.rs:EntryMutationKind]`, `[entry_lifecycle.rs:EntryCommitReceipt]`, `[entry_lifecycle.rs:LifecycleScope]`, `[entry_lifecycle.rs:LifecycleConfirmationEvidence]`, `[entry_lifecycle.rs:LifecycleAuthority]`, `[entry_lifecycle.rs:SecondaryOutcome]`, `[entry_lifecycle.rs:ProjectionOutcome]`, `[entry_lifecycle.rs:execute_lifecycle_transition]` (plus internal `apply_lifecycle_in_tx`, `physical_delete_cleanup_in_tx`, `ensure_active_entry_version*`).
  - `[entry_table.rs:query]`, `[entry_table.rs:EntryTableFilter]`, `[entry_table.rs:EntryTableResult]`.
  - `[patterns.rs:list]`, `[patterns.rs:get]`, `[patterns.rs:create]`, `[patterns.rs:update]`, `[patterns.rs:delete]`, `[patterns.rs:list_theme_patterns]`, `[patterns.rs:get_theme_pattern]`.
  - `[taxonomy.rs:list_definitions]`, `[taxonomy.rs:create_definition]`, `[taxonomy.rs:patch_definition]`, `[taxonomy.rs:delete_definition]`, `[taxonomy.rs:list_terms_for_locale]`, `[taxonomy.rs:create_term_with_locale]`, `[taxonomy.rs:patch_term]`, `[taxonomy.rs:delete_term]`.
- **Builder, Block Catalog, Overrides, Themes, Import & Menus** ([`page.rs`](page/mod.rs), [`builder.rs`](builder/mod.rs), [`builder_binding.rs`](builder_binding.rs), [`overrides.rs`](overrides/mod.rs), [`theme.rs`](theme/mod.rs), [`import.rs`](import/mod.rs), [`menu.rs`](menu/mod.rs)):
  - `[page.rs:ensure_embedded_core_blocks]`, `[page.rs:load_unified_block_catalog_in_conn]`, `[page.rs:sync_owner_blocks_in_tx]`, `[page.rs:set_owner_block_status_in_tx]`, `[page.rs:resync_plugin_block_acls_in_tx]`.
  - `[builder.rs:BuilderRenderContext]`, `[builder.rs:read_render_context_tx]`, `[builder.rs:resolve_patterns_in_tx_with_budget]`, `[builder.rs:validate_media_refs_in_tx]` (plus entry-builder storage helpers).
  - `[builder_binding.rs:resolve]`, `[builder_binding.rs:resolve_in_tx_with_route_and_locale]`.
  - `[overrides.rs:resolve_template_2axis]`, `[overrides.rs:save_typed_override]`, `[overrides.rs:reset_typed_override]`, `[overrides.rs:preview_typed_override_with_cache_and_locale]`, `[overrides.rs:render_resolved_public_builder_with_cache_for_locale]`, `[overrides.rs:list_studio_targets]`, `[overrides.rs:preview_studio_with_cache_and_locale]`.
  - `[theme.rs:stage_and_commit_v3_revision]`, `[theme.rs:ensure_default_starter_theme]`, `[theme.rs:activate_revision]`, `[theme.rs:load_active_theme_v3]`, `[theme.rs:sync_dev_theme_in_db]`, `[theme.rs:list]`, `[theme.rs:verify_revision_durability]`.
  - `[import.rs:ImportPlan]`, `[import.rs:DryRunImportInput]`, `[import.rs:ThemeSampleManifest]` (dry-run, execution, and orphan import recovery).
  - `[menu.rs:create_menu_in_tx]`, `[menu.rs:replace_menu_items_in_tx]`, `[menu.rs:get_slot_version_for_locale_in_tx]`, `[menu.rs:MAX_MENU_ITEMS]`, `[menu.rs:MAX_MENU_DEPTH]`.
- **Plugins, Theme Functions, KV, Projections & Admin Extensions** ([`plugin.rs`](plugin/mod.rs), [`plugin_kv.rs`](plugin_kv.rs), [`plugin_projections.rs`](plugin_projections/mod.rs), [`admin_extensions.rs`](admin_extensions.rs)):
  - `[plugin.rs:install_plugin_revision]`, `[plugin.rs:set_plugin_enabled_cas]`, `[plugin.rs:load_plugin_executable_with_generation]`, `[plugin.rs:is_plugin_generation_active]`, `[plugin.rs:verify_active_theme_fence_in_tx]`, `[plugin.rs:is_actor_role_active]`, `[plugin.rs:delete_entry_with_plugin_fence]`, `[plugin.rs:record_owner_execution_result]`, `[plugin.rs:list_owner_logs]`, `[plugin.rs:sync_dev_plugin_in_db]` (`owner_kind` isolation per `0053`).
  - `[plugin_kv.rs:get]`, `[plugin_kv.rs:set]`, `[plugin_kv.rs:delete]`, `[plugin_kv.rs:list]`, `[plugin_kv.rs:begin_immediate]`, `[plugin_kv.rs:commit]`, `[plugin_kv.rs:rollback]`.
  - `[plugin_projections.rs:recompute_entry_projections]`, `[plugin_projections.rs:spawn_recompute_owner_projections_batched]`, `[plugin_projections.rs:batch_load_entry_projections]`, `[plugin_projections.rs:delete_entry_projections_in_tx]` (`Admin > Plugin > Theme` precedence + `SiteCacheEpoch` invalidation).
  - `[admin_extensions.rs:sync_owner_in_conn]`, `[admin_extensions.rs:effective_list]`, `[admin_extensions.rs:effective_term]`, `[admin_extensions.rs:update_term_values_in_conn]`.
- **Public Content, Permalinks, Manual Redirects, SEO, Sitemap & Settings** ([`public_content.rs`](public_content/mod.rs), [`permalinks.rs`](permalinks/mod.rs), [`redirects/`](redirects/mod.rs), [`seo.rs`](seo/mod.rs), [`sitemap.rs`](sitemap.rs), [`settings.rs`](settings/mod.rs)):
  - `[public_content.rs:get_by_slug_for_locale]`, `[public_content.rs:get_published_by_id]`, `[public_content.rs:load_entry_render_snapshot_with_options]`, `[public_content.rs:list_published_with_total]`, `[public_content.rs:taxonomy_public_page_for_locale]`, `[public_content.rs:media_is_referenced]`.
  - `[permalinks.rs:resolve_public_path]`, `[permalinks.rs:sync_claims_in_tx]`, `[permalinks.rs:put_settings]`, `[permalinks.rs:sync_provider_rewrite_defaults_in_tx]`, `[permalinks.rs:canonical_entry_path_in_tx]`, `[permalinks/live_claims.rs:find_live_claim_kinds_for_paths_in_conn]`, `[permalinks/live_claims.rs:load_live_auto_redirect_edges_in_conn]`.
  - `[redirects/mod.rs:find_active_manual_redirect]`, `[redirects/mod.rs:flush_telemetry_batch]`, `[redirects/rules.rs:list_manual_redirects]`, `[redirects/rules.rs:get_manual_redirect_by_id]`, `[redirects/rules.rs:create_manual_redirect]`, `[redirects/rules.rs:update_manual_redirect_cas]`, `[redirects/rules.rs:delete_manual_redirect]`, `[redirects/rules.rs:export_manual_redirects_csv]`, `[redirects/graph_and_import.rs:import_manual_redirects_csv]`, `[redirects/not_found_logs.rs:list_not_found_logs]`, `[redirects/not_found_logs.rs:delete_not_found_log]`, `[redirects/not_found_logs.rs:bulk_delete_not_found_logs]`, `[redirects/not_found_logs.rs:clear_not_found_logs]`.
  - `[seo.rs:get_entry_route]`, `[seo.rs:put_entry_route]`, `[seo.rs:get_entry_seo]`, `[seo.rs:put_entry_seo]`, `[seo.rs:build_entry_seo_view_for_locale]`, `[seo.rs:build_home_seo_view_for_locale]`, `[seo.rs:reading_page_state_for_locale]`.
  - `[sitemap.rs:count_indexable_routes_in_tx]`, `[sitemap.rs:list_indexable_routes_in_tx]` (`SITEMAP_PART_SIZE = 5_000`, `SITEMAP_MAX_TOTAL_URLS = 1_000_000`).
  - `[settings.rs:list_locales]`, `[settings.rs:create_locale]`, `[settings.rs:update_locale]`, `[settings.rs:delete_locale]`, `[settings.rs:set_default_locale]`, `[settings.rs:set_multilingual_enabled]`, `[settings.rs:purge_orphan_locale]`, `[settings.rs:get_general_for_locale]`, `[settings.rs:save_general_for_locale]`, `[settings.rs:get_reading_for_locale]`, `[settings.rs:save_reading_for_locale]`, `[settings.rs:get_global_site_options]`, `[settings.rs:put_global_site_options]`, `[settings.rs:get_theme_options_for_locale]`, `[settings.rs:put_theme_options_for_locale]`, `[settings.rs:get_active_public_theme_options_for_locale]`, `[settings.rs:get_admin_i18n_bootstrap]`, `[settings.rs:timezone_catalog]`.
- **Media, Contact, SMTP, Operations, Backup & Restore** ([`media.rs`](media.rs), [`media_refs.rs`](media_refs.rs), [`media_lifecycle.rs`](media_lifecycle.rs), [`contact.rs`](contact.rs), [`smtp.rs`](smtp.rs), [`operations.rs`](operations.rs), [`backup.rs`](backup.rs), [`restore.rs`](restore.rs)):
  - `[media.rs:find]`, `[media.rs:list_filtered]`, `[media.rs:save]`, `[media.rs:require_existing_effective_references]`.
  - `[media_refs.rs:collect_pins_in_tx]`, `[media_refs.rs:ensure_media_active_in_tx]`, `[media_refs.rs:load_media_inventory_bounded]`, `[media_refs.rs:is_media_unreferenced]`.
  - `[media_lifecycle.rs:tombstone_media_in_tx]`, `[media_lifecycle.rs:finalize_delete_job]`, `[media_lifecycle.rs:list_pending_delete_jobs]`.
  - `[contact.rs:create_form]`, `[contact.rs:update_form]`, `[contact.rs:delete_form]`, `[contact.rs:list_submissions]`, rate-limit (`RATE_LIMIT_PER_MIN = 5`, `RATE_LIMIT_PER_HOUR = 20`, `RATE_LIMIT_GLOBAL_PER_MIN = 100`) and dedup (`DEDUP_TTL_SECS = 86_400`).
  - `[smtp.rs:get_smtp_settings]`, `[smtp.rs:put_smtp_settings_cas]`, `[smtp.rs:create_initial_attempt_in_conn]`, `[smtp.rs:claim_pending_attempt]`, `[smtp.rs:finish_attempt]`, `[smtp.rs:recover_inflight_notifications]`.
  - `[operations.rs:create_job]`, `[operations.rs:transition_job_cas]`, `[operations.rs:mark_interrupted_on_startup]`, `[operations.rs:bump_maintenance_generation]`, `[operations.rs:prune_old_receipts]`.
  - `[backup.rs:vacuum_into_snapshot]`, `[backup.rs:collect_backup_inventory]`.
  - `[restore.rs:build_expected_schema_objects]`, `[restore.rs:validate_restored_db_read_only]`, `[restore.rs:validate_and_rebase_copy]`, `[restore.rs:verify_restored_db_references]`.

## Invariants

- **Single SQL Boundary**: 100% of `sqlx::query*` and `sqlx::raw_sql` invocations reside in `src/storage/` (verified by `cms/scripts/check-standards.py`).
- **Bootstrap & Collision Safety**: `init_db` runs `content::preflight_builtin_key_collisions` before `sqlx::migrate!("../../migrations")` so built-in `post`/`page` descriptors never overwrite existing user/theme/plugin claims, and verifies embedded core blocks afterwards (`[mod.rs:init_db]`).
- **Transactional Write Integrity & CAS**: Schema transitions, composite saves (`[entries.rs:commit_composite_entry_save]`), plugin revision installs (`[plugin.rs:install_plugin_revision]`), KV commits (`[plugin_kv.rs:begin_immediate]`), permalink claim updates (`[permalinks.rs:sync_claims_in_tx]`), and media tombstoning (`[media_lifecycle.rs:tombstone_media_in_tx]`, `[media_refs.rs:collect_pins_in_tx]`) run inside explicit SQLite transactions with CAS/generation/schema-token fencing.
- **Single Public Route Authority**: `permalink_route_claims` and `permalinks::resolve_public_path` enforce collision-free canonical paths and 301 redirect chains (`[permalinks.rs:sync_claims_in_tx]`, `[permalinks.rs:resolve_public_path]`).
- **Schema Fingerprint Verification on Restore**: `validate_restored_db_read_only` compares restored database objects against `build_expected_schema_objects` within bounded object/SQL size limits (`[restore.rs:validate_restored_db_read_only]`).

## Side Effects

- Creates/opens the site SQLite database in WAL mode, executes migrations `0001`–`0057`, and reads/writes all CMS tables (`[mod.rs:init_db]`).
- Stages and verifies immutable theme and plugin revision directories and `.complete` markers on disk under `themes_root` and `plugins_root` (`[theme.rs:stage_and_commit_v3_revision]`, `[theme.rs:ensure_default_starter_theme]`, `[plugin.rs:stage_and_publish_plugin_revision_with_extensions]`).
- Creates consistent SQLite snapshot files via `VACUUM INTO` during backup jobs (`[backup.rs:vacuum_into_snapshot]`).

## Verification

Run the storage unit/integration tests and standards checks from the repository root:

```powershell
python cms/scripts/check-standards.py
cargo test --manifest-path cms/Cargo.toml -p opendash-server --locked storage::
```
