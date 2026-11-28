# OpenDash — Hợp đồng Bộ khung Lõi (Core Kernel) & Quy chuẩn Code (v1.2)

- **Phiên bản:** `1.5` (cập nhật đồng bộ codebase `0001`–`0062`, bổ sung Unified JS Runtime, Render Hooks, Scoped Custom Routes, Dev Live-Edit theo [`unified-js-runtime-render-hooks-and-dev-mode.md`](../designs/unified-js-runtime-render-hooks-and-dev-mode.md), Entry Lifecycle / Quick Edit / Bulk Actions theo [`entry-lifecycle-bulk-quick-edit.md`](../designs/entry-lifecycle-bulk-quick-edit.md), và Ảnh Responsive / Tối ưu WebP / Replace Media theo [`responsive-media-and-replace.md`](../designs/responsive-media-and-replace.md))
- **Trạng thái:** `AGREED` · `implemented`
- **Tài liệu liên quan:** [Yêu cầu sản phẩm](../product/PRODUCT_REQUIREMENTS.md) · [Kiến trúc CMS](../architecture/CMS_ARCHITECTURE.md) · [Plugin ABI v1](PLUGIN_ABI.md) · [M8 Writer Inventory](M8_WRITER_INVENTORY.md)

---

## 1. Mục đích

Tài liệu này định nghĩa **Bộ khung Lõi (Core Kernel)** của OpenDash và **Quy chuẩn Code bắt buộc** để mọi tính năng sử dụng chung một chuẩn duy nhất, không viết lặp hàm hoặc phá vỡ ranh giới kiến trúc.

---

## 2. Danh mục Base APIs bắt buộc tái sử dụng (Single Source of Truth)

Trước khi thêm bất kỳ tính năng, route, bảng dữ liệu hay màn hình Admin nào, developer và AI bắt buộc tra bảng dưới đây (kèm CodeGraph và mã nguồn thực tế) và gọi đúng module gốc:

| Nhu cầu kỹ thuật | Module gốc sở hữu | API / Thành phần chính trong `cms/` |
|---|---|---|
| Kiểm tra định danh, khóa schema, block ID, mã gói, SemVer, `ScopedResourceId`, root hệ thống | `opendash_core::id` | `valid_snake_key`, `valid_snake_key_with_limit`, `valid_kebab_segment_with_limit`, `valid_namespaced_block_id`, `valid_namespaced_snake_key`, `validate_package_id`, `valid_package_id`, `validate_semver`, `ScopedResourceId`, `valid_layout_slice_panel_id`, `RESERVED_SYSTEM_ROOTS`, `is_reserved_system_root`. |
| Quản lý tài nguyên có chủ sở hữu & vòng đời provider | `opendash_core::registry` | `ResourceOwner` (`Core`, `User`, `Theme(id)`, `Plugin(id)`), `ProviderStatus` (`Active`, `ProviderInactive`, `Missing`, `Invalid`), `DeclarationState` (`Declared`, `Withdrawn`), `OwnedDescriptor<T>`, `RegistryCatalog<T>` (`register`, `update`, `insert_loaded`, `redeclare`, `withdraw`, `withdraw_owner_except`, `deactivate_owner`, `activate_owner`). |
| Định nghĩa & điều phối sự kiện (`HookDispatcher`) | `opendash_core::hooks` & `opendash_server::hooks` | `ExtensionOwnerKind` (`Plugin`, `Theme`), `ExtensionRuntimeKind` (`Wasm`, `Js`), `HookEvent` (gồm `ThemeAfterActivate` và render filters `FilterRenderHead`/`FilterRenderFooter`/`FilterRenderSlot`), `HookExecutionMode` (`Standard`, `PureValidator`, `AsyncPostCommit`, `FilterPrepareSave`, `FilterContactBeforeSubmit`, `FilterProjection`, `FilterRender`, `RouteGet`, `RoutePost`), `RenderCachePolicy`, `EnvelopeKind::Route`, `PluginEnvelopeV2`; `HookDispatcher` & `AsyncHookQueue`. |
| Domain Models & Validators thuần Rust | `opendash_core::{admin_extensions, backup, block_schema, builder, builder_blocks, contact, content, field_schema, hooks, i18n, icons, id, l10n, media, plugin, redirects, registry, revisions, richtext, search, theme}` | `ContentTypeDefinition`, `ContentTypeSettings`, `FieldGroupRecord`, `EditorLayoutConfigV1`, `PanelDefinition`, `TaxonomyDefinition`, `BlockSchema`, `FieldSpec`, `FieldType`, `FieldAcl`, `ConditionRule`, `PatchField<T>`, `BuilderDocument`, `BlockNode`, `BuilderContextTarget`, `PluginManifest`, `ExtensionRouteDecl`, `ThemeFunctionsSpec`, `ScopedFieldGrant`, `NetworkScopeGrant`, `BackupManifestV1` (gồm `BackupFileKind::ExtensionSecret`), `PortableRuntimeConfigV1`, `AdminExtensions`, `media::{MediaVariantSpec, MediaVariantView, build_responsive_srcsets, render_responsive_picture_html, parse_variant_spec_ext, parse_media_etag_token}`, `revisions::*`, `search::*`, helpers `redirects::*` (chuẩn hóa đường dẫn/URL đích, RFC 4180 CSV import/export), `sanitize_richtext`, curated `icons`, `i18n` locales, `l10n` gettext catalog. |
| Đọc & kiểm tra an toàn gói ZIP (Theme V1, Plugin, Backup) | `opendash_server::package` | `open_bounded_zip`, `ensure_safe_flat_entry`, `read_bounded_entry`, `is_valid_zip_content_type` (Plugin `5 MiB`), `validate_theme_relative_path`, `read_v2_zip_entries` (Theme v3 bounded archive `8 MiB` / `32 MiB` / `256` files), `preflight_and_open_backup_zip` (Backup ZIP32 `4 GiB`). |
| Quản lý đường dẫn runtime & khóa site | `opendash_server::paths::SitePaths` & `opendash_server::operations::SiteCoordinator` | `SitePaths` (`db_path`, `themes_dir`, `plugins_dir`, `uploads_dir`, `backups_dir`, `backups_staging_dir`, `backups_artifacts_dir`, `backups_restored_dir`, `secrets_dir`, `smtp_secrets_dir`, `plugin_secrets_dir`, `theme_secrets_dir`, `config_dir`, `site_config_path`, `runtime_dir`, `site_lock_path`, `db_lock_path`). `SiteCoordinator`, `AdmittedMutation`, `MutationLease` (chi tiết tại [M8 Writer Inventory](M8_WRITER_INVENTORY.md)). |
| Xác thực phiên, kiểm tra quyền (`Admin`/`Editor`) và CSRF | `opendash_server::auth` | Route đọc dùng `require_session` hoặc `require_admin`. Route thay đổi dữ liệu có phiên (`POST`/`PUT`/`DELETE`/`PATCH`) bắt buộc dùng `require_session_with_csrf` hoặc `require_admin_with_csrf` (riêng `/api/public/p/*` dùng quy tắc `PUBLIC_EXTENSION_POST` kiểm Origin/`Public` hoặc RateLimit+HMAC/`Webhook`). |
| Trả lỗi HTTP từ server | `opendash_server::error::ApiError` | `ApiError` (`Cow<'static, str>`) và `ApiResult<T>`. |
| Render an toàn, Cache, Dev Mode, HTML & Secrets | `opendash_server::{template_sec, document, cache, dev_mode, secrets, config}` | `template_sec` (MiniJinja `C-SEC` + `{{ od_hook(...) }}`, `od_icon()`, `od_image()` với `SafeMediaObject` & `ImageNonceTable`), `document::{normalize_public_document, normalize_public_document_with_options (allow_admin_media_preview), sanitize_render_head_fragment}`, `cache::{SiteCacheEpoch, ExtensionRenderCache}`, `dev_mode::DevOverlayConfig`, `secrets::SecretStore` (ACL-protected JSON cho `smtp/`, `plugins/`, `themes/`), `config::RuntimeConfigV1` (`config/site.json`, `RecoveryState`). |
| Truy vấn cơ sở dữ liệu SQLite | `opendash_server::storage::*` | **100% câu lệnh `sqlx::query*` và `sqlx::raw_sql` phải nằm trong `crates/opendash-server/src/storage/`** (37 module files: `mod` + 36 domain submodules: `admin_extensions`, `auth`, `backup`, `builder`, `contact`, `content`, `dashboard`, `entries`, `entry_lifecycle`, `entry_table`, `fields`, `import`, `media`, `media_lifecycle`, `media_refs`, `menu`, `operations`, `options_pages`, `overrides`, `page`, `patterns`, `permalinks`, `plugin`, `plugin_kv`, `plugin_projections`, `public_content`, `redirects`, `restore`, `revisions`, `search`, `seo`, `settings`, `sitemap`, `smtp`, `taxonomy`, `theme`; cùng các test submodules tương ứng). |
| Gọi HTTP API & giữ draft từ React Admin | `cms/admin/src/api/client.ts` & `EditorDraftStore.ts` | Dùng `requestJson`, `requestText`, `requestBlob`, `ApiClientError`, `getHealth` qua `api/client.ts` (22 API files gồm `client.ts` & `index.ts`). Dùng `EditorDraftStore` / `redirectsDraftStore` để bảo toàn dữ liệu chưa lưu khi gặp `401`/`412`. |
| Thành phần giao diện Admin & Form cơ bản | `cms/admin/src/components/{AdminPrimitives,FormPrimitives,LocaleTabs}.tsx` | Tái sử dụng `AdminButton`, `AdminBadge`, `AdminAlert`, `AdminTabs`, `AdminTabPanel`, `AdminCard` (`AdminPrimitives.tsx`), `TextInput`, `FormGroup`, `TextField`, `SelectField`, `ZipUploadField` (`FormPrimitives.tsx`), và `LocaleTabs`. |

---

## 3. Quy chuẩn Định danh (Identifier Standards — `opendash_core::id`)

1. **`SnakeKey` (`valid_snake_key`, `valid_snake_key_with_limit` — `[a-z][a-z0-9_]*`):** Dùng cho `type_key` của CPT, `taxonomy_key`, `field_key` (`1..=64` ký tự trong [`FieldSpec`](UNIFIED_FIELD_SCHEMA.md), mặc định `1..=64`; một số khóa block kế thừa tiền-V1 có thể ngắn hơn khi kiểm giới hạn riêng).
2. **`PackageId` (`validate_package_id`, `valid_package_id` — `[a-z0-9-]`, `3..=64` ký tự):** Dùng cho `plugin_id` và `theme_id` (không bắt đầu hoặc kết thúc bằng `-`).
3. **`KebabSegment` & `NamespacedBlockId` (`valid_kebab_segment_with_limit`, `valid_namespaced_block_id`):** Dùng cho định danh block `<namespace>/<name>`.
4. **`NamespacedSnakeKey` (`valid_namespaced_snake_key`):** Dùng cho khóa schema `<namespace>/<name>` và core hook handler ID (`core/page_saved`).
5. **`ScopedResourceId`:** Biểu diễn tài nguyên gắn quyền sở hữu `core:<key>`, `user:<key>`, `theme:<pkg_id>:<key>`, `plugin:<pkg_id>:<key>`, và `layout:<slice_key>` (`valid_layout_slice_panel_id`).
6. **`SemVer` (`validate_semver`):** Kiểm tra phiên bản SemVer 2.0.0.

---

## 4. Quy chuẩn Phân tầng & Tổ chức Code

1. **`routes/*`:** Chỉ thực hiện guard xác thực/CSRF (`require_*`), `AdmittedMutation`, kiểm tra header/Content-Type, giải mã input HTTP và ánh xạ kết quả sang HTTP response. Không đặt truy vấn SQL, không gọi trực tiếp `PluginManager::run_plugin*` ngoài `HookDispatcher` / `plugin_action`.
2. **Tầng nghiệp vụ (`opendash_server::*`):** Điều phối luồng nhiều bước (validate → ghi SQLite qua `storage` → phát sự kiện sau commit qua `HookDispatcher`).
3. **`opendash_core`:** Thuần Rust, chỉ phụ thuộc `serde`, `serde_json`, `url`. Tuyệt đối không dùng `axum`, `sqlx`, `wasmi`, `tokio`, `reqwest` hoặc `std::{fs, io, net, process, thread}`.
4. **`opendash_server::storage::*`:** Sở hữu toàn bộ câu lệnh SQL, kết nối/giao dịch SQLite (`BEGIN IMMEDIATE`) và dựng snapshot từ DB. Các hàm trong `storage::plugin_kv` chỉ mượn `&SqlitePool` hoặc `&mut SqliteConnection` của caller để bảo toàn deadline và `StagedCommitGuard`.

---

## 5. Hợp đồng Registry, Quyền sở hữu & Hook

1. **`RegistryCatalog<T>` (`opendash_core::registry`):**
   - Tách 2 trục: trạng thái khai báo (`DeclarationState::Declared | Withdrawn`) và trạng thái nhà cung cấp (`ProviderStatus::Active | ProviderInactive | Missing | Invalid`).
   - Đổi theme hoặc tắt plugin chỉ chuyển sang `ProviderInactive` hoặc `Withdrawn` — **giữ nguyên 100% schema, giá trị `content_entries.values_json` và tham chiếu media (`collect_pins_in_tx`)** để Admin/Editor tiếp tục quản lý.
2. **`HookDispatcher` (`opendash_server::hooks`):**
   - `ContentBeforeSave` chạy **trước** write transaction ở chế độ pure bounded validator (`Allow` hoặc `Veto { code, field_errors }`), không được ghi KV/content hay gọi HTTP.
   - `PageAfterSave` và `ContentAfterSave` chạy **sau** commit SQLite, lỗi/timeout được ghi log vào `plugin_logs` và không rollback giao dịch đã commit.
   - `PluginManualRun` và standalone `plugin_action` chạy với kiểm tra `actor RBAC ∩ plugin capabilities`, `revision_generation` fencing và `StagedCommitGuard`.
   - Giới hạn bảo vệ: tối đa `32` handler/lượt, `5s` phase budget, `250ms` timeout cho core handler, `5s` cho plugin worker.

---

## 6. Thuật ngữ chuẩn (Glossary — OpenDash ↔ WordPress)

Bảng dưới đây thống nhất thuật ngữ dùng trong mã nguồn (`cms/`), API, giao diện Admin và Design Spec EGM để tránh đặt tên chồng chéo giữa các agent và developer:

| Thuật ngữ OpenDash | Định danh / Nơi lưu chuẩn | Tương đương WordPress | Ý nghĩa & Hành vi chuẩn |
|---|---|---|---|
| **Pattern (CPT)** / **Reusable Pattern Block** | CPT private `pattern` (`content_entries`) · Block `core/pattern_ref` | Synced Pattern / Reusable Block | Khối do người dùng tạo trong Admin CPT. Khi chèn block `core/pattern_ref` (chọn `pattern_id`) vào nhiều trang (Home, Contact, About…), sửa 1 lần trong Pattern CPT sẽ tự cập nhật đồng bộ ở mọi nơi đang gọi. |
| **Pattern Library** | Thư mục `patterns/*.json` trong Theme + nguồn Cloud (mở rộng) | Block Pattern Directory (`Explore all patterns`) | Thư viện mẫu sẵn mở từ Visual Builder; bấm chọn là **COPY / import AST** (tạo node IDs mới) vào trang đang chỉnh sửa, không liên kết sống với file gốc. |
| **Template** | Root HTML (`index.html`, `page.html`, `single*.html`, `archive*.html`, `taxonomy*.html`, `search.html`, `404.html`) | Block / Classic Templates | Bố cục dùng chung cho từng ngữ cảnh route, khai báo có kiểu trong `theme.json`, cho phép tùy chỉnh site-owned trong `template_overrides`. |
| **Template Part** | Root `header.html`/`footer.html` & `template-part/*.html` · `{{ od_part("<key>") }}` · Block `core/template_part` | Template Parts | Thành phần bố cục dùng lại (header, footer, post-card, breadcrumbs, pagination…), nhúng qua directive tĩnh `od_part` (Theme HTML) hoặc block `core/template_part` (Visual Builder). |
| **Page Layout** | `page-layouts/*.html` (`full_width`, `sidebar`, `canvas`…) | Page Templates | Bố cục có tên chọn ở sidebar màn sửa Page; không thay đổi nội dung hay trạng thái xuất bản của Page. `canvas` không gọi `header`/`footer`. |
| **Content Slot** | `{{ od_content() }}` (Theme HTML) · Block `core/content_slot` (Visual Builder) | `the_content()` / Post Content block | Đúng 1 vị trí do core kiểm soát để chèn nội dung bản ghi (editor HTML đã lọc hoặc Builder AST đã render) vào entry template. |
| **Reading Settings** | `site_reading_settings` · `/api/admin/settings/reading` | Settings → Reading (Homepage / Posts page) | Cấu hình atomic 2 Page ID độc lập (`homepage_page_id` cho `/`, `posts_page_id` cho archive `post`) đi qua 1 route authority (`storage/permalinks.rs`). |
| **C-SEC** | `opendash_server::template_sec` ([M5_THEME_TEMPLATES.md](M5_THEME_TEMPLATES.md)) | N/A (Ràng buộc bảo mật OpenDash) | Tập con an toàn MiniJinja: auto-escape HTML bắt buộc, cấm `{% include %}`/`{% extends %}`/`\|safe`, giới hạn fuel (`10_000`), recursion (`16`) và dung lượng render (`512 KiB`). Đường legacy/untrusted cấm `<script>`/`<svg>`/`on*`; nguồn Theme V1 & Declarative Block được mở inline `<svg>`/`<script>`/`on*` tĩnh (cấm nội suy động) — `Superseded by` [trusted-theme-js-svg-and-asset-enqueue.md](../designs/trusted-theme-js-svg-and-asset-enqueue.md). |

---

## 7. Kiểm tra Quy chuẩn Tự động

Bắt buộc chạy sau mỗi thay đổi code hoặc tài liệu:

```powershell
python cms/scripts/check-standards.py
python cms/scripts/check-standards.py --self-test
python cms/scripts/check-plans.py
python cms/scripts/check-plans.py --self-test
```
