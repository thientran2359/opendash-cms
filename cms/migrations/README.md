# SQLite Migrations (`cms/migrations/`)

## Overview

`cms/migrations/` holds the forward-only SQLite schema history for OpenDash CMS: **54** `.sql` files numbered `0001` through `0055` (with `0016` skipped — no `0016_*.sql` file exists). All migrations are embedded directly into the `opendash-server` executable via `sqlx::migrate!("../../migrations")` in [`storage/mod.rs:init_db`](../crates/opendash-server/src/storage/mod.rs) and applied automatically on startup.

| Group / Milestone | Migrations | Primary Tables & Schema Objects |
| --- | --- | --- |
| **M1–M2 Baseline** | [`0001_builder_slice.sql`](0001_builder_slice.sql) – [`0003_plugins.sql`](0003_plugins.sql) | `theme_blocks`, `home_page`, `users`, `sessions`, `bootstrap`, `plugins`, `plugin_kv`, `plugin_logs`. |
| **M3 Core CMS & Media** | [`0004_content_types.sql`](0004_content_types.sql) – [`0011_theme_activation.sql`](0011_theme_activation.sql) | `content_types`, `content_entries` (`post`/`page` publication + `content_entries_core_insert_draft`), `media_assets`, `theme_blocks` templates (`0007`, `0011`), `users.version` (`0009`), `site_general_settings`, `site_active_theme`. |
| **M4 Builder & Taxonomy** | [`0012_taxonomy_terms.sql`](0012_taxonomy_terms.sql) – [`0014_entry_terms.sql`](0014_entry_terms.sql) | `taxonomy_terms` (`taxonomy_terms_parent_{insert,update}` kind/hierarchy triggers; cycle/depth-8 enforced in `storage/taxonomy.rs`), `builder_homepage`, `builder_homepage_media`, `entry_terms`. |
| **M5 Theme & Templates** | [`0015_theme_packages_revisions.sql`](0015_theme_packages_revisions.sql), [`0017_theme_options.sql`](0017_theme_options.sql) – [`0021_theme_import_journal.sql`](0021_theme_import_journal.sql) | `theme_packages`, `theme_revisions`, `theme_legacy_mappings`, `theme_active_selection`, `theme_option_*`, `template_overrides`, `site_menus`/`site_menu_items`/`site_menu_slot_assignments`, `content_type_taxonomy_eligibility`, `import_runs`/`import_provenance`/`import_tombstones`. |
| **M6 Plugin Extensions** | [`0022_plugin_revisions.sql`](0022_plugin_revisions.sql) – [`0024_owner_neutral_domain_bridge.sql`](0024_owner_neutral_domain_bridge.sql) | `plugin_revisions`, `plugins.{active_revision_id,revision_generation,cas_version}`, owner-neutral CNT (`0023`) & theme/override (`0024`) indexes/columns. |
| **M7 Site Features & Video** | [`0025_site_public_settings.sql`](0025_site_public_settings.sql) – [`0031_video_builder_refs.sql`](0031_video_builder_refs.sql) | `site_public_settings`, `site_menu_slot_versions`, `entry_public_paths`/`entry_redirects`/`entry_seo`/`site_home_seo`/`seo_public_media_refs`, `contact_*`, `site_smtp_settings`/`contact_notification_attempts`, `media_assets` MP4 video rebuild (`0030`), `builder_homepage_video_refs`/`template_override_media_refs`. |
| **M8 Operations & Lifecycle** | [`0032_operations_jobs.sql`](0032_operations_jobs.sql), [`0033_media_lifecycle.sql`](0033_media_lifecycle.sql) | `operation_jobs`, `maintenance_generation`, `media_generations`, `media_delete_jobs`, and `media_assets` lifecycle columns. |
| **Admin Nav & Review** | [`0034_field_groups_and_editor_layout.sql`](0034_field_groups_and_editor_layout.sql) – [`0037_section_documents.sql`](0037_section_documents.sql) | `content_field_groups`/`content_field_group_targets`/`content_editor_layouts`, remove default taxonomy from `page` (`0035`), `admin_extensions`/`term_extension_values`, `section_documents`/`section_document_media_refs` + section triggers. |
| **Content Foundation** | [`0038_content_type_settings.sql`](0038_content_type_settings.sql) – [`0041_provider_rewrite_defaults.sql`](0041_provider_rewrite_defaults.sql) | `content_types` settings (`public`, `has_archive`, `publication`, `supports`), `taxonomy_definitions`/`content_type_taxonomy_overrides`, `site_permalink_settings`/`permalink_route_claims`/`permalink_route_redirects`, `provider_rewrite_defaults`. |
| **Theme v3, CPT Builder & Patterns** | [`0042_theme_v3_resolver_and_reading_settings.sql`](0042_theme_v3_resolver_and_reading_settings.sql) – [`0044_pattern_cpt_and_refs.sql`](0044_pattern_cpt_and_refs.sql) | `site_reading_settings`, `entry_builder_documents`/`entry_builder_media_refs`, `owner_template_selections`, `content_entries.{page_layout_key,custom_template_key,content_mode}`, CPT builder index (`0043`), `pattern` CPT triggers & `entry_pattern_refs`. |
| **WordPress Permalinks (Phase 1)** | [`0045_wordpress_permalinks.sql`](0045_wordpress_permalinks.sql) | WordPress-style default routes (`/{slug}`, `/{cpt}/{slug}`, `/{tax}/{slug}`, `/posts`), `primary_category_term_id`, `has_been_published` + monotonic triggers, and `permalink_route_claims` rebuild. |
| **Declarative Block Engine (`block.json` v1)** | [`0046_declarative_block_engine.sql`](0046_declarative_block_engine.sql) | `builder_block_definitions` (`core`/`theme`/`plugin` registry) and `plugin_revisions.blocks_json`. |
| **Multilingual & Theme L10n (Layers 1–2)** | [`0047_multilingual_content_and_theme_l10n.sql`](0047_multilingual_content_and_theme_l10n.sql) | `site_locales`, `site_locales_state`, topological rebuild of `content_entries`/`taxonomy_terms` + 8 FK child tables, rebuild of `site_reading_settings`/`site_home_seo`/`site_menu_slot_*`/`permalink_route_*`, `site_general_settings_locales`, `theme_option_locale_*`, and `*_locale_*` integrity triggers. |
| **Unified Studio & Global Site Options** | [`0048_global_site_options.sql`](0048_global_site_options.sql) | `site_global_option_version`, `site_global_option_values`, `site_global_option_media_refs` persisted across theme switches. |
| **Theme Ergonomics (`published_at`)** | [`0049_entry_published_at.sql`](0049_entry_published_at.sql) | Adds `content_entries.published_at` and backfills from `updated_at` for published entries. |
| **Plugin Extensibility & Projections** | [`0050_plugin_extensibility_and_projections.sql`](0050_plugin_extensibility_and_projections.sql) | Rebuilds PKs on `theme_option_values`, `theme_option_versions`, and `theme_option_media_refs` keyed by `(owner_kind, owner_id, ...)`, and adds `plugin_entry_projections` + `idx_plugin_entry_projections_plugin`. |
| **Admin I18n & Multilingual Settings (Layer 3)** | [`0051_admin_i18n_and_multilingual_settings.sql`](0051_admin_i18n_and_multilingual_settings.sql) | Adds `site_general_settings.cms_locale` (default `en`), `site_locales_state.multilingual_enabled`, and `site_locales.deleted_at`. |
| **Taxonomy Term Featured Image** | [`0052_taxonomy_term_featured_image.sql`](0052_taxonomy_term_featured_image.sql) | Adds `taxonomy_terms.featured_image_media_id` (`REFERENCES media_assets(id) ON DELETE RESTRICT`) and partial index `idx_taxonomy_terms_featured_image_media_id`. |
| **Theme Functions & Unified JS Runtime** | [`0053_theme_functions_runtime.sql`](0053_theme_functions_runtime.sql) | Rebuilds `plugin_kv`, `plugin_entry_projections`, `plugin_logs` (`owner_kind` composite keys, dropping FK to `plugins(id)`) and `theme_option_{values,versions,media_refs}` (`owner_kind` including `theme_functions`), recreating `idx_theme_option_values_owner` (`+0` net schema objects). |
| **Entry Lifecycle (Trash)** | [`0054_entry_lifecycle.sql`](0054_entry_lifecycle.sql) | Additive nullable UTC column `content_entries.trashed_at` (`DEFAULT NULL`, preserving all existing rows as active) and composite index `idx_content_entries_lifecycle_list (content_type_key, locale, trashed_at, publication_status, id)`. Forward-only; downgrade by dropping columns or opening with an older binary is unsupported (SQLx compiled migration validation refuses unknown applied migrations; rollback uses separate-root pre-upgrade backup recovery). |
| **Unified Field Engine (`0055`)** | [`0055_theme_option_locale_media_refs.sql`](0055_theme_option_locale_media_refs.sql) | Rebuilds `theme_option_media_refs` adding `locale TEXT NOT NULL DEFAULT ''` to `PRIMARY KEY (owner_kind, owner_id, locale, option_key, media_id)` so translatable theme option media references track per-locale refs cleanly (`+0` net schema objects). |

## Interface

- **Embedded migrator:** [`storage/mod.rs:init_db`](../crates/opendash-server/src/storage/mod.rs) configures `SqliteConnectOptions` (`create_if_missing(true)`, `foreign_keys(true)`, `SqliteJournalMode::Wal`, `busy_timeout(5s)`), runs preflight checks, executes `sqlx::migrate!("../../migrations").run(&db)`, and seeds core blocks.
- **Pre-migration hook:** [`storage/content.rs:preflight_builtin_key_collisions`](../crates/opendash-server/src/storage/content.rs) validates built-in `post`/`page` descriptor keys before SQLx applies migration `0008`.
- **Post-migration seed:** [`storage/page.rs:ensure_embedded_core_blocks`](../crates/opendash-server/src/storage/page.rs) upserts `core`-owned rows in `builder_block_definitions`.
- **Release manifest tracking:** [`build-release-linux.sh`](../scripts/build-release-linux.sh) hashes every `cms/migrations/*.sql` file into `release-manifest.json` (`migration_checksums`).

## Invariants

- **Immutable applied migrations:** Never edit SQL content or change line endings (LF) of existing migration files; SQLx verifies checksums in `_sqlx_migrations` against embedded bytes.
- **Monotonic numbering:** Every new schema change must be added as the next sequential `NNNN_<snake_case>.sql` file (`0056_*` onward; `0016` remains permanently unused).
- **Pre-existing ownership safety:** `preflight_builtin_key_collisions` must run before `0008` so user/theme/plugin-owned `post`/`page` descriptors are never overwritten during bootstrap.
- **Database integrity enforcement:** Triggers (`content_entries_core_insert_draft`, `taxonomy_terms_parent_*`, `content_entries_pattern_private_*`, `content_entries_mark_has_been_published_*`, and `*_locale_*` triggers from `0047`) enforce domain invariants at the SQLite layer alongside `PRAGMA foreign_keys = ON`.
- **Storage layering:** All runtime SQL queries (`sqlx::query*`) against these tables belong in `cms/crates/opendash-server/src/storage/`, per [CORE_FOUNDATION.md](../../docs/contracts/CORE_FOUNDATION.md).

## Side Effects

- Running `init_db` creates the parent database directory if absent, initializes or upgrades the SQLite file (`opendash.sqlite`) with WAL/SHM journal files, creates/updates `_sqlx_migrations`, and inserts/updates `core` rows in `builder_block_definitions`.

## Verification

Run from the repository root:

```powershell
cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
python cms/scripts/egm_pattern_s02_migration.py
python cms/scripts/windows_auth_smoke.py --binary cms/target/debug/opendash.exe --baseline-db cms/fixtures/baseline_0001.sqlite
```

On Linux/macOS, pass `--binary cms/target/debug/opendash` to smoke scripts.
