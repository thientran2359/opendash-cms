# HTTP Routes (`opendash-server/src/routes/`)

Tầng tiếp nhận HTTP request của Axum. Mọi handler trong thư mục này phải tuân thủ [Core Foundation](../../../../../docs/contracts/CORE_FOUNDATION.md) và [M8 Writer Inventory](../../../../../docs/contracts/M8_WRITER_INVENTORY.md):
- Không viết `sqlx::query*` trực tiếp trong `routes/`.
- Mọi route ghi có phiên đều gọi `require_session_with_csrf` hoặc `require_admin_with_csrf` (được kiểm tra tự động bởi `cms/scripts/check-standards.py`) và lấy `AdmittedMutation`.

| File | Nhóm Endpoints |
| --- | --- |
| `auth.rs` | `/api/auth/setup`, `/api/auth/login`, `/api/auth/logout`, `/api/auth/me` |
| `users.rs` | `/api/admin/users/*`, `/api/admin/profile` |
| `dashboard.rs` | `/api/admin/dashboard` |
| `content.rs` | `/api/admin/content-types/*`, field groups, editor layout, `effective-editor-config`, `publication` |
| `entries.rs` | `/api/admin/content-types/{type_key}/entries/*`, `composite`, `entry-table`, `admin-actions` |
| `patterns.rs` | `/api/admin/patterns/*` (private Pattern CPT), `/api/admin/patterns/library/*` (theme pattern catalog) |
| `taxonomy.rs` | `/api/admin/taxonomy/definitions/*`, `/terms/*`, `/eligibility/*` |
| `media.rs` | `/api/admin/media/*` (upload ảnh & video MP4), `/media/{id}/file` & `/thumbnail` (Range/ACL) |
| `builder.rs` | `/builder`, `/api/admin/entries/{type_key}/{id}/builder*` (Pattern Admin mutation), template overrides |
| `themes.rs` | `/api/admin/themes/*`, `/theme/*` assets, `/api/admin/library/*` |
| `import.rs` | `/api/admin/themes/{id}/import/dry-run`, `/execute` |
| `menu.rs` | `/api/admin/menus/*`, `/api/admin/menu-slots/{theme_id}` |
| `plugin.rs` | `/api/admin/plugins/*`, `/run`, `/actions/{action}`, `/ui-schema`, `/enable`, `/disable` |
| `settings.rs` | `/api/admin/settings/{general,public,permalinks,theme-options}` |
| `seo.rs` | `/api/admin/.../{route,seo}`, `/api/admin/seo/home`, `public_fallback` (permalinks & 301 redirects) |
| `sitemap.rs` | `/robots.txt`, `/sitemap.xml`, `/sitemaps/{part}` |
| `contact.rs` | `/api/admin/contact/{forms,submissions}`, `/api/public/contact/{id}`, `/received` |
| `smtp.rs` | `/api/admin/settings/smtp`, `/test`, `/api/admin/contact/submissions/{id}/notify` |
| `operations.rs` | `/api/admin/operations/{diagnostics,reconcile/*,quarantine/purge,backups/*,jobs/*,restore-uploads,restores,recovery/activate}` |
| `public.rs` | Trang chủ `/` và render trang công khai cơ bản |
| `public_content.rs` | Render archive, single, taxonomy archive công khai và draft preview có xác thực |
| `health.rs` | `/api/health` |
