# SQLite Storage Layer (`opendash-server/src/storage/`)

Sở hữu **100% câu lệnh SQL (`sqlx::query*`, `sqlx::raw_sql`)** của OpenDash (31 files). Khởi tạo pool WAL + `foreign_keys = ON` và chạy migrations `0001`–`0041` tại `mod.rs::init_db`.

| Nhóm | Modules | Trách nhiệm chính |
| --- | --- | --- |
| **Core & Auth** | `mod.rs`, `auth.rs`, `dashboard.rs` | Khởi tạo SQLite pool, migrations, preflight builtin collisions, users/sessions/bootstrap, thống kê Dashboard. |
| **Content & Taxonomy** | `content.rs`, `entries.rs`, `entry_table.rs`, `taxonomy.rs`, `sections.rs` | CPT definitions/settings (`0038`), field groups & editor layouts (`0034`), entry CRUD & composite save, bảng `/entry-table`, custom taxonomies (`0039`), private Section CPT (`0037`). |
| **Builder & Themes** | `page.rs`, `builder.rs`, `builder_binding.rs`, `overrides.rs`, `theme.rs`, `theme_tests.rs`, `import.rs`, `menu.rs` | Legacy `home_page`, Builder homepage AST, `FixedEntry`/`RouteContext` binding, template overrides (`0018`), theme packages/revisions/options (`0015`/`0017`), selective sample import (`0021`), site menus & slots (`0019`/`0026`). |
| **Plugins & Extensions** | `plugin.rs`, `plugin_kv.rs`, `admin_extensions.rs` | Plugin metadata/revisions/logs (`0003`/`0022`), isolated KV store (`StagedCommitGuard`), declarative admin columns/actions/term panels (`0036`). |
| **Public, SEO & Permalinks** | `public_content.rs`, `permalinks.rs`, `seo.rs`, `sitemap.rs`, `settings.rs` | Public content queries & `media_is_referenced`, single route authority (`0040`/`0041` `permalink_route_claims` & redirects), entry/home SEO (`0027`), sitemap XML, site general/public/permalink settings. |
| **Media & Lifecycle** | `media.rs`, `media_refs.rs`, `media_lifecycle.rs` | Metadata ảnh & video MP4 (`0006`/`0030`), `collect_pins_in_tx` bảo vệ media đang tham chiếu, tombstone/quarantine/reconcile (`0033`). |
| **Contact & SMTP** | `contact.rs`, `smtp.rs` | Contact forms, submissions, dedup, rate buckets (`0028`), SMTP settings & notification attempts (`0029`). |
| **Operations & Recovery** | `operations.rs`, `backup.rs`, `restore.rs` | `operation_jobs` ledger (`0032`), `VACUUM INTO` snapshot & inventory, kiểm định schema & phục hồi DB khi restore. |
