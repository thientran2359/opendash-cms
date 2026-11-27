# Hợp đồng Plugin ABI v1 / v1.1 — OpenDash

**Phiên bản ABI:** `1` & `1.1` (hỗ trợ `manifest_schema` `1`, `2`, `3` đã triển khai; bổ sung Unified JS Runtime `plugin.js` / `functions.js` qua `boa_engine = "=0.22.0"`, Render Hooks, Scoped Custom Routes & Dev Live-Edit)\
**Trạng thái:** `AGREED` · `Implemented` (M6 baseline + [Spec 2](../designs/plugin-hooks-host-ops-and-projections.md) + [Spec B: Unified JS Runtime, Render Hooks & Dev Mode](../designs/unified-js-runtime-render-hooks-and-dev-mode.md))\
**Tài liệu tham chiếu:** [docs/product/PRODUCT_REQUIREMENTS.md](../product/PRODUCT_REQUIREMENTS.md), [CORE_FOUNDATION.md](CORE_FOUNDATION.md), [M6 Plugin Extensions](M6_PLUGIN_EXTENSIONS.md), [M8 Writer Inventory](M8_WRITER_INVENTORY.md), [Wasmi 2.0.0 Documentation](https://docs.rs/wasmi/2.0.0/wasmi/)

---

## 1. Cấu trúc Gói Plugin (ZIP / Directory) & Kiểm tra Offline (`--check-plugin`)

Gói plugin cài đặt vào OpenDash (dạng `.zip` qua Admin API hoặc thư mục/ZIP qua CLI) phải tuân thủ nghiêm ngặt các giới hạn an toàn:

1. **Các file được hỗ trợ trong gói (`cms/crates/opendash-server/src/plugin/package.rs`):**
   - `manifest.json` (bắt buộc, tối đa `64 KiB` — `MAX_MANIFEST_BYTES`): Định danh, phiên bản, phạm vi quyền, hooks/filters, actions, declarations và `settings_schema`.
   - `plugin.wasm` **hoặc** `plugin.js` (bắt buộc đúng 1 entrypoint khớp `manifest.entrypoint`, tối đa `2 MiB` — `MAX_WASM_BYTES`): Module WebAssembly (`wasm32-unknown-unknown`) hoặc script JavaScript ES2024 chạy zero-build trên `boa_engine = "=0.22.0"` dùng API `globalThis.od.*` (`cms/sdk/js/opendash.d.ts`). Theme v3 có thể đặt tùy chọn `functions.js` ở root khớp trường `functions` trong `theme.json`.
   - `README.md` (tùy chọn).
   - `blocks/<slug>/block.json` + `blocks/<slug>/render.html` (tùy chọn, luôn đủ cặp, mỗi file text tối đa `256 KiB` — `V2_MAX_TEXT_BYTES`): Khai báo block Builder theo `BlockSchemaV1` (`cms/crates/opendash-core`) và template MiniJinja C-SEC; tối đa **32 blocks**, mỗi `slug` tối đa 32 ký tự kebab-case, `schema.id == "<plugin-id>/<slug>"`, cấm `plugin-id` là `core` hoặc `legacy`.
   - `languages/*.{pot,po,mo}` (tùy chọn, tối đa **32 file**, mỗi file tối đa `256 KiB`): File bản địa hóa GNU gettext theo `text_domain` (mặc định là `plugin_id`).
2. **Quy tắc an toàn khi giải nén ZIP & quét thư mục:**
   - Tối đa **104 file thường** (`MAX_PLUGIN_FILES`), tối đa **256 ZIP entries**; kích thước ZIP tối đa **5 MiB** (`MAX_PLUGIN_ZIP_BYTES`), tổng kích thước giải nén tối đa **10 MiB** (`MAX_UNCOMPRESSED_TOTAL_BYTES`), tỷ lệ giải nén mỗi file ZIP tối đa **100:1** (`V2_MAX_EXPANSION_RATIO`).
   - Từ chối đường dẫn tuyệt đối, traversal (`..`), backslash, entry ẩn (bắt đầu bằng `.`), tên non-UTF-8, FIFO/socket/device, symlink/reparse point nội bộ, độ sâu thư mục vượt quá **`MAX_PLUGIN_DIR_DEPTH = 4`**, file ngoài hợp đồng, hoặc xung đột tên không phân biệt chữ hoa/thường (`to_ascii_lowercase()`).
   - Đọc mọi file (ZIP và directory) qua `.take((cap + 1) as u64)` có giới hạn trước khi parse. Không thực thi Wasm hoặc JS lúc cài đặt hoặc kiểm tra; chỉ biên dịch tĩnh bằng `wasmi::Module::new` (cho `plugin.wasm`) hoặc kiểm tra cú pháp tĩnh bằng `boa_engine` (`validate_js_module_syntax`, không thực thi top-level, cho `plugin.js` / `functions.js`).
3. **Validator CLI offline `opendash --check-plugin <path>` (`--check-plugin=<path>`):**
   - Chạy hoàn toàn offline **trước** `SiteCoordinator::acquire` và SQLite init (hỗ trợ cả file ZIP kể cả không có đuôi `.zip` và thư mục giải nén).
   - Thành công (`exit(0)`): In 1 dòng JSON lên `stdout` với tất cả mảng sort ASCII tăng dần:
     ```json
     { "ok": true, "plugin_id": "...", "version": "...", "manifest_schema": 3, "hooks": [], "filters": [], "actions": [], "blocks": [], "settings_count": 0 }
     ```
   - Thất bại (`exit(1)`): In `{ "ok": false, "error": "..." }` lên `stdout`.
4. **Immutable Revisions (`plugins/revisions/<revision_digest>/`):**
   - Staging qua `.staging/<run_uuid>/`, `.complete`, bảng `plugin_revisions`, cột `active_revision_id`, `revision_generation` và `cas_version` (`0022_plugin_revisions.sql`).
   - Plugin có block ghi `blocks.json` canonical sort theo slug và đồng bộ `builder_block_definitions`; xung đột ID block với owner khác trả `409`. Tắt plugin chuyển declarations sang `provider_inactive` mà không xóa dữ liệu/nội dung.

---

## 2. Đặc tả `manifest.json` (`PluginManifest`) & Owner Settings (`SecretStore`)

```json
{
  "id": "my-plugin",
  "version": "1.1.0",
  "api_version": 1,
  "manifest_schema": 3,
  "min_host_version": "0.1.1",
  "name": "Tên hiển thị của Plugin",
  "description": "Mô tả ngắn gọn về chức năng của plugin",
  "entrypoint": "plugin.wasm",
  "hooks": ["content/before_save", "content/after_publish", "settings/after_save"],
  "filters": ["filter:content/prepare_save", "filter:seo/head", "filter:entry/view"],
  "hook_priority": 100,
  "permissions": {
    "sqlite": ["kv_read", "kv_write"],
    "http": ["https://api.example.com"],
    "network": [{ "scheme": "https", "host": "api.example.com", "port": 443, "methods": ["GET", "POST"], "path_prefix": "/v1/" }],
    "content_read": ["post"],
    "content_write": ["post"],
    "content_delete": ["post"],
    "content_trash_read": ["post"],
    "content_restore": ["post"],
    "content_purge": ["post"],
    "content_read_private": [{ "content_type": "post", "field_keys": ["internal_note"] }],
    "content_write_private": [{ "content_type": "post", "field_keys": ["internal_note"] }],
    "taxonomy_read": true,
    "media_read": true,
    "site_options_read": true,
    "plugin_settings_read": true,
    "plugin_secrets_read": ["api_key"]
  },
  "actions": [],
  "declarations": {},
  "editor_panels": [],
  "admin_extensions": {},
  "settings_schema": [
    { "key": "api_key", "label": "API Key", "type": "secret", "required": false },
    { "key": "mode", "label": "Mode", "type": "text", "default": "auto" }
  ]
}
```

### Quy tắc `PluginManifest` (`cms/crates/opendash-core/src/plugin.rs`) & Theme `functions.js` (`opendash-core/src/theme.rs`):
- **`manifest_schema` & `entrypoint`:** Hỗ trợ `1`, `2`, `3` (mặc định `1`). `entrypoint` chấp nhận `"plugin.wasm"` (Wasmi `=2.0.0`) hoặc `"plugin.js"` (Boa `=0.22.0`, bắt buộc `manifest_schema >= 3`). Theme v3 có thể khai báo mục `functions` trong `theme.json` trỏ tới `"functions.js"` ở root (mặc định mở `content_read_all_public`, `taxonomy_read`, `media_read`, `site_options_read`, `plugin_settings_read`, `sqlite: ["kv_read", "kv_write"]`, `render_trusted_html: true` trừ khi chủ theme đặt tường minh `false`; cấm khai báo `content_trash_read`/`content_restore`/`content_purge`). Khai báo bất kỳ tính năng v3 (`filters`, `routes`, `render_cache`, `settings_schema`, quyền v3 `taxonomy_read`/`media_read`/`site_options_read`/`plugin_settings_read`/`plugin_secrets_read`/`content_delete`/`content_read_all_public`, hoặc lifecycle `hooks` v3 ngoài `page/after_save`, `content/before_save`, `content/after_save`) bắt buộc đặt `manifest_schema >= 3` và khai báo `min_host_version`; khai báo bất kỳ quyền vòng đời Trash mới nào (`content_trash_read`, `content_restore`, `content_purge`) bắt buộc `manifest_schema >= 3` và **`min_host_version >= "0.1.1"`** (`render_trusted_html` mặc định `true` ở mọi schema và không tự ép nâng `manifest_schema >= 3`).
- **Lifecycle `hooks`:** `page/after_save`, `content/before_save`, `content/after_save`, và các post-commit async events (`content/after_delete`, `content/after_publish`, `content/after_unpublish`, `contact/after_submit`, `plugin/after_enable`, `plugin/after_disable`, `settings/after_save`, `theme/after_activate`). Không chứa `plugin/manual_run` hay `filter:*`.
- **Synchronous `filters`:**
  - Write/Submit/Projection filters: `filter:content/prepare_save`, `filter:contact/before_submit`, `filter:seo/head`, `filter:entry/view`.
  - Public Render filters (Spec B S02): `filter:render/head`, `filter:render/footer`, `filter:render/slot` (nhúng qua `{{ od_hook("slot", expr?) }}` trong template/block; cache qua `ExtensionRenderCache` theo `SiteCacheEpoch` và `render_cache: "by_route" | "global" | "none"`). Mặc định `render_trusted_html: true` (opt-out `false` áp dụng kiểm duyệt C-SEC nghiêm ngặt).
- **Scoped Custom Routes (`routes`, tối đa 32/gói — Spec B S03):** Khai báo `{ id, method: "GET"|"POST", path, access: "public"|"session"|"admin"|"webhook", capabilities?, cache?: "by_route"|"global"|"none" }` (`ExtensionRouteDecl` với `#[serde(deny_unknown_fields)]`; `content_type` là trường response runtime trong `OdRouteResponse`, không phải trường manifest) phục vụ qua `/p/{pkg}/*`, `/api/p/{pkg}/*`, `/api/public/p/{pkg}` và `/api/public/p/{pkg}/*` (`GET` cấm `access: "webhook"`; `/api/public/p/` áp `Public` strict Origin hoặc `Webhook` raw-body + allowlisted HMAC headers + RateLimit và chặn `content_save`/`content_delete`).
- **Actions & `capabilities`:** Allowlist gồm `kv_read`, `kv_list`, `kv_write`, `kv_delete`, `http`, `content_read`, `content_read_private`, `content_write`, `content_write_private`, `content_delete`, `content_trash_read`, `content_restore`, `content_purge`, `taxonomy_read`, `media_read`, `site_options_read`, `plugin_settings_read`, `plugin_secrets_read`, `log`. Trong một action **không được kết hợp** nhiều scope ghi khác loại (`kv_write`/`kv_delete` vs `content_write` vs `content_delete`/`content_restore`/`content_purge` ⇒ từ chối ngay lúc validate/runtime với `ERR_MIXED_WRITE_SCOPE`), và action khai báo `content_restore` hoặc `content_purge` **bị cấm kết hợp capability `http`** ở cả manifest validation lẫn runtime defense-in-depth.
- **Settings, Isolation & `SecretStore` (Migrations `0050` & `0053`):**
  - Migration `0053_theme_functions_runtime.sql` tách `owner_kind` (`'plugin'` vs `'theme'` / `'theme_functions'`) trên `plugin_kv`, `plugin_entry_projections`, `plugin_logs`, và `theme_option_{values,versions,media_refs}` (không giữ FK cứng trỏ tới `plugins(id)`).
  - `settings_schema` tối đa `64` trường (`text`, `textarea`, `number`, `boolean`, `select`, `url`, `media`, `secret`). Trường `secret` cấm đặt `default` khác `null`, lưu ngoài SQLite qua `SecretStore` tại `database/secrets/plugins/<plugin_id>/<64-hex>.json` và `database/secrets/themes/<theme_id>/<64-hex>.json` (giới hạn `4096 B` plaintext, quyền thư mục `0700` / file `0600` trên Unix, Protected DACL trên Windows; được sao lưu & phục hồi qua `BackupFileKind::ExtensionSecret`).
  - Admin endpoints `GET`/`PUT /api/admin/plugins/{id}/settings` và `GET`/`PUT /api/admin/themes/active/functions/settings` (yêu cầu Admin + CSRF + `If-Match`) luôn ẩn `secret` và `secret_ref`, chỉ trả `{"configured": bool}`; `PUT` với `null` giữ nguyên secret cũ và `{"clear": true}` xóa sạch cả DB lẫn file. Theme Functions còn cung cấp `POST /api/admin/themes/active/actions/{action_id}` và `GET /api/admin/themes/active/functions/logs`.

---

## 3. WebAssembly Interface (`opendash_v1`) & Envelope Formats

### 3.1. Guest Exports
1. `memory`: Linear memory WebAssembly chuẩn.
2. `opendash_run() -> i32`: `0` = thành công, khác `0` = mã lỗi guest.

### 3.2. Host Imports (`opendash_v1` + `env:abort`)
- **ABI v1 (cốt lõi):**
  - `host_get_input_len() -> i32`
  - `host_read_input(dest_ptr: i32, max_len: i32) -> i32`
  - `host_write_output(src_ptr: i32, len: i32) -> i32`
  - `host_call(op_ptr: i32, op_len: i32, resp_ptr: i32, max_resp_len: i32) -> i32`: trả về số byte đã ghi khi vừa buffer, hoặc **`-2`** (`BUFFER_TOO_SMALL`) khi `resp.len() > max_resp_len` (và `<= MAX_IPC_FRAME_BYTES` `2 MiB`).
  - `env:abort(i32, i32, i32, i32)` cho AssemblyScript runtime.
- **ABI v1.1 (Readback không chạy lại `host_call`):**
  - `host_last_resp_len() -> i32`: trả về độ dài byte phản hồi `host_call` gần nhất chưa đọc hết (hoặc `0`).
  - `host_read_last_resp(dest_ptr: i32, max_len: i32) -> i32`: sao chép tối đa `max_len` byte phản hồi gần nhất vào guest memory (kiểm bounds/overflow, trả `-1` nếu lỗi con trỏ hoặc rỗng; tự xóa buffer nội bộ sau khi đọc đủ độ dài hoặc khi `host_call` mới được gọi).

### 3.3. Envelope Compatibility (`HookEnvelope` vs `PluginEnvelopeV2`)
- Plugin `manifest_schema <= 2` tiếp tục nhận `HookEnvelope` (`event`, `occurred_at_ms`, `correlation_id`, `locale?`, `translation_group_id?`, `payload`) cho các sự kiện cũ.
- Plugin `manifest_schema >= 3` nhận **`PluginEnvelopeV2`** (`cms/crates/opendash-core/src/hooks.rs`):
  ```json
  {
    "envelope_version": 2,
    "kind": "hook | filter | action | manual | route",
    "event": "content/after_publish",
    "occurred_at_ms": 1732450000000,
    "correlation_id": "...",
    "locale": "vi",
    "principal": "admin | editor | public | system",
    "dispatch_depth": 0,
    "payload": {}
  }
  ```
  *(Biến thể `EnvelopeKind::Route` được kích hoạt cho Scoped Custom Routes `/p/{pkg}/*`, `/api/p/{pkg}/*`, `/api/public/p/{pkg}/*`; chỉ `EnvelopeKind::Cron` tiếp tục được giữ chỗ và từ chối ở V1.)*
- **JavaScript Runtime API (`cms/sdk/js/opendash.d.ts`, `plugin/js_runtime.rs`):** Mọi `plugin.js` / `functions.js` chạy trong Boa `Context` sạch mỗi invocation với đối tượng `globalThis.od` bất biến (`od.on`, `od.addFilter`, `od.addAction`, `od.addRoute`, `od.kv`, `od.content`, `od.taxonomy`, `od.media`, `od.site`, `od.settings`, `od.secret`, `od.http`, `od.log`, `od.escapeHtml`, `od.escapeAttr`). Mọi lời gọi host I/O ở top-level khi load script đều bị chặn (`top_level_host_call_forbidden`); mọi handler nhận trọn `OdEnvelope` (`env.payload`).

---

## 4. Binary Worker IPC Framing & Ma trận 19 Host Operations (`HostPolicy`)

- **Worker IPC Framing (`cms/crates/opendash-server/src/plugin/worker.rs`, `runtime.rs`):**
  - Tiến trình con `--plugin-worker` gửi magic `\x00ODIPC2\x00` trên `stdout`, sau đó Host ↔ Worker giao tiếp bằng **binary `u32 LE` length prefix + UTF-8 JSON payload** (`MAX_IPC_FRAME_BYTES = 2 MiB`) hỗ trợ dữ liệu chứa `\n` mà không vỡ khung.
  - Worker không giữ `SqlitePool`; mọi `HostReadRequest` (`kv_get`, `kv_list`, `content_get`, `content_list`, `content_trash_get`, `content_trash_list`, `taxonomy_list`, `media_get`, `site_options_get`, `plugin_settings_get`, `plugin_secret_get`) đi qua `WorkerToHostFrame::ReadRequest` / `HostToWorkerFrame::ReadResponse` về Host chính chịu kiểm soát `HostPolicy` và `Actor`/`Principal`.

### Bảng 19 Host Operations (`plugin/host_ops.rs`)

| Host Op (`op`) | Quyền Manifest | Mode Cho phép (`HookExecutionMode`) | Phạm vi, Giới hạn & Ghi chú |
|---|---|---|---|
| `kv_get` | `sqlite:kv_read` | Tất cả | Đọc `staged_kv` (kể cả tombstone `None`) trước khi truy vấn DB; trả `{"ok":true,"found":bool,"value":...}`. |
| `kv_set` | `sqlite:kv_write` | `Standard`, `AsyncPostCommit`, `RoutePost` | Key `<= 256 B`, value `<= 512 KiB`; tổng `staged_kv <= 64` ops / `2 MiB`. Cấm trộn với `content_*`. |
| `kv_delete` | `sqlite:kv_write` | `Standard`, `AsyncPostCommit`, `RoutePost` | Ghi tombstone `None` vào `staged_kv`; trả `{"ok":true,"data":{"deleted":true}}`. |
| `kv_list` | `sqlite:kv_read` | `Standard`, `AsyncPostCommit`, `FilterRender`, `RouteGet`, `RoutePost` | `{ prefix?, limit? }` (`limit` clamp `1..=100`, mặc định `50`; riêng `FilterRender`/`RouteGet` clamp `1..=50`); hợp nhất DB + `staged_kv`; trả `{"items":[{"key","value"}]}`. |
| `http_request` | `http` / `network` | `Standard`, `AsyncPostCommit`, `FilterContactBeforeSubmit` *(max 1)*, `RoutePost`, `RouteGet` *(max 2)*, `FilterRender` *(max 2 khi `render_cache != "none"`)* | SSRF DNS-pinning qua `egress.rs`, `timeout <= 2s`, max `1 MiB` response, chặn `/api/admin`, `/save`, `/preview`, `/theme`, `/p/`, `/api/p/`, `/api/public/p/` (bị cấm ở `FilterRender` khi `render_cache == "none"`). |
| `content_save` | `content_write` (+ private) | `Standard` (Action), `RoutePost` *(Admin/Session; cấm `Public`)* | Tối đa `1` command/invocation; kiểm `plugin_can_write_field_for_owner` theo `actor_role`, fence + CAS. Cấm trộn KV/lifecycle. |
| `content_delete` | `content_delete` | `Standard` (Action), `RoutePost` *(Admin/Session; cấm `Public`)* | `{ type_key, entry_id, expected_version }`; chuyển vào **Thùng rác có thể khôi phục** (`StagedLifecycleOperation::Trash`) với fence + CAS; giữ alias tương thích `deleted_entry_id`/`deleted_type_key` và trả thêm `data.lifecycle`. Không phát `content/after_delete`. Cấm trộn KV/`content_save`. |
| `content_trash_get` | `content_trash_read` *(schema 3, `min_host_version >= 0.1.1`)* | `Standard` (Action) | `{ type_key, entry_id, locale? }`; trả `{ entry: TrashEntryMetadata \| null }` chỉ gồm siêu dữ liệu (không trả body/values/private SEO/public URL). Cấm trên `/run`, route và hook. |
| `content_trash_list` | `content_trash_read` *(schema 3, `min_host_version >= 0.1.1`)* | `Standard` (Action) | `{ type_key, locale?, limit?, offset? }`; `limit` clamp `1..=50` (mặc định 20), `offset <= 10_000`, sắp xếp `trashed_at DESC, id DESC`, trả `{ items, total, limit, offset }`. Cấm trên `/run`, route và hook. |
| `content_restore` | `content_restore` *(schema 3, `min_host_version >= 0.1.1`)* | `Standard` (Action, cấm trộn `http`) | `{ type_key, entry_id, expected_version }`; khôi phục `draft_publish` về `draft` trực tiếp; CPT `immediate` công khai yêu cầu **Host Confirmation** (`409 confirmation_required` → `POST .../confirm-lifecycle`). |
| `content_purge` | `content_purge` *(schema 3, `min_host_version >= 0.1.1`, Admin-only)* | `Standard` (Action, cấm trộn `http`) | `{ type_key, entry_id, expected_version }`; luôn yêu cầu **Host Confirmation** một lần (`409 confirmation_required`, TTL 120s, ràng buộc actor/session/plugin generation/maintenance epoch); chỉ khi xác nhận mới xóa vật lý và phát duy nhất 1 `content/after_delete`. |
| `content_get` | `content_read` (+ private / `content_read_all_public`) | Tất cả *(trừ `FilterContactBeforeSubmit`)* | `{ type_key, entry_id, locale? }`; chỉ trả bài active (`trashed_at IS NULL`); ở public scope (`principal == Public` như `FilterRender` / `/p/` `RouteGet`, hoặc `FilterProjection`) chỉ trả bài `published` thuộc CPT active & public, lọc field qua `plugin_can_read_field_for_owner` (còn `/api/p/` `RouteGet` dùng principal phiên `Admin`/`Editor`). |
| `content_list` | `content_read` (+ private / `content_read_all_public`) | Tất cả *(trừ `FilterContactBeforeSubmit`)* | `{ type_key, locale?, limit?, offset?, published_only? }`; chỉ liệt kê bài active (`trashed_at IS NULL`); `limit` clamp `1..=50`, `offset <= 10_000`; ở public scope (`principal == Public` hoặc `FilterProjection`) ép `published_only = true`. |
| `taxonomy_list` | `taxonomy_read` | Tất cả *(trừ `FilterContactBeforeSubmit`)* | `{ kind, locale?, limit? }`; `limit` clamp `1..=100`; từ chối taxonomy private nếu `principal != Admin`. |
| `media_get` | `media_read` | Tất cả *(trừ `FilterContactBeforeSubmit`)* | `{ media_id }`; ở public scope (`principal == Public` hoặc `FilterProjection`) yêu cầu `media_is_referenced` và trả URL public `/media/{id}/file?v={version}`; trả đầy đủ `version`, `generation`, `content_sha256`, `thumbnail_url`, `variants`, `webp_srcset` và `fallback_srcset`. |
| `site_options_get` | `site_options_read` | Tất cả *(trừ `FilterContactBeforeSubmit`)* | `{ locale? }`; trả global + theme options đã gộp với `public = true`, hydrate theo Unified Field Engine V1 (`media`/`gallery`/`link`/`icon`/`spacing`/`entry_ref`/`term_ref` dạng object, `richtext` trả HTML đã sanitize). |
| `plugin_settings_get` | `plugin_settings_read` | Tất cả *(trừ `FilterContactBeforeSubmit`)* | Trả settings của chính owner (`plugin` hoặc `theme_functions`); mọi trường `secret` chỉ xuất `{"configured": bool}`. |
| `plugin_secret_get` | `plugin_secrets_read` chứa `key` | `Standard`, `AsyncPostCommit`, `FilterContactBeforeSubmit`, `RoutePost` | Giải mã plaintext từ `SecretStore` (`plugins/` hoặc `themes/`); bị cấm trong `FilterProjection`, `FilterRender`, `RouteGet` và `ContentBeforeSave`. |
| `log` | *(mọi plugin/theme)* | Tất cả *(bị từ chối lúc gọi trong `FilterContactBeforeSubmit`; bị bỏ qua không ghi DB ở `FilterRender`/`RouteGet`)* | `{ level: "info"\|"warn"\|"error", message }`; tối đa `20` logs/invocation, sanitize `512` chars vào `plugin_logs` (theo `owner_kind`). |
| `l10n_translate` | *(mọi plugin/theme)* | Tất cả | `{ text_domain?, msgid, msgid_plural?, count?, context?, locale? }` (và `od.i18n` trong JS); quota riêng `MAX_L10N_CALLS = 256` calls/run, không trừ vào `MAX_HOST_CALLS` (`100`). |

---

## 5. Giới hạn Tài nguyên, `StagedCommitGuard`, Pipelines & Materialized Projections

| Tài nguyên | Giới hạn | Cơ chế kiểm soát |
|---|---|---|
| **CPU / Loops** | Wasm `20.000.000` fuel · Boa `5.000.000` loops (`128` recursion, `512` stack) | Wasmi fuel metering (`OutOfFuel`) + Boa `RuntimeLimits`. |
| **Memory (Wasm + OS Worker Cap)** | Wasm `16 MiB` linear memory · `--plugin-worker` OS cap **`128 MiB`** | `StoreLimitsBuilder::memory_size(16 << 20)` + OS Job Object (Windows) / `RLIMIT_AS` (Linux) / kernel Jetsam `posix_spawnattr_setjetsam_ext` + `BOOTSTRAP_FD = 3` (macOS) qua `operations_worker_process.rs`. |
| **Input / Output / IPC Frame** | `1 MiB` I/O · `2 MiB` IPC frame | `MAX_IPC_FRAME_BYTES` + `host_write_output`. |
| **KV Key / Value / Staged Batch** | `256 B` key · `512 KiB` val · `64` ops / `2 MiB` | `host_ops.rs` & single `BEGIN IMMEDIATE` tx (cô lập theo `owner_kind`). |
| **Host Calls / HTTP Sub-quota** | `100` calls/run (`64` trong `FilterRender`/`RouteGet`) · `10` HTTP (`1` contact; `2` `RouteGet`/cached `FilterRender`; `0` uncached `FilterRender`) | `MAX_HOST_CALLS = 100`, `MAX_RENDER_HOST_CALLS = 64`, `MAX_RENDER_HTTP_CALLS = 2`, `HTTP_TIMEOUT = 2s`, max `1 MiB` response stream. |
| **Invocation Deadline** | `5s` (`2s` cho `prepare_save`/`contact/before_submit`; `200ms`/`1000ms` cho `FilterRender`; `1s` cho `RouteGet`; `5s` cho `RoutePost`) | Tokio deadline (`ROUTE_GET_TIMEOUT = 1000ms`) + `ProcessGuard` (`--plugin-worker`). |
| **Concurrency Pool** | `4` Admin + `2` Background/Async + `4` Public Render/Route = `10` max workers | Ba Tokio Semaphores độc lập (`MAX_CONCURRENT_INVOCATIONS = 4`, `MAX_BACKGROUND_PUBLIC_INVOCATIONS = 2`, `public_render_semaphore = 4`). |
| **`AsyncHookQueue`** | `2` workers · queue `64` jobs · `dispatch_depth < 2` | Giữ `MutationLease`, hủy & drain tức thì khi `SiteCoordinator::maintenance_cancel_rx() == true`, thoát khi `shutdown_rx == true`. |

- **Single-Tx `StagedCommitGuard` (`plugin/runtime.rs`) & Trusted Commit Receipts (`hooks.rs`, `plugin/manager.rs`, `plugin/lifecycle_confirmation.rs`):** Chỉ cho phép commit `staged_kv` trong `HookExecutionMode::{Standard, AsyncPostCommit, RoutePost}`, `staged_logs` trong mọi mode trừ `FilterRender`/`RouteGet`/`FilterContactBeforeSubmit` (không ghi DB trên đường nóng public GET), `staged_content_command` và `staged_lifecycle_command` (`Trash`) trong `HookExecutionMode::{Standard, RoutePost}` (khi có `actor` đăng nhập hợp lệ), và `Restore`/`Purge` độc quyền trong `HookExecutionMode::Standard`. Hành động khai báo `content_restore` hoặc `content_purge` bị cấm kết hợp capability `http` ở cả bước kiểm manifest lẫn defense-in-depth trên host runtime (chặn mạng trước khi kết nối). Toàn bộ writes + tombstones + `plugin_logs` commit trong một transaction `BEGIN IMMEDIATE` sau khi kiểm `OwnerFence` (`revision_generation` cho Plugin hoặc `(revision_id, selection_version)` cho Active Theme) + `actor_role`. `HookDispatcher::enqueue_plugin_action_hooks` chỉ nhận `Option<&EntryCommitReceipt>` do storage tạo trong transaction đã commit (không bao giờ suy diễn từ JSON phản hồi của guest). Các giao thức xác nhận do Host sở hữu (`POST /api/admin/plugins/{id}/actions/{action}/confirm-lifecycle`) không bao giờ giữ SQLite writer hay chạy lại Wasm/JS khi người dùng xác nhận.
- **Typed Synchronous Pipelines (`hooks.rs`, `routes/entries.rs`, `routes/contact.rs`, `routes/public_content.rs`):**
  - `filter:content/prepare_save`: chạy **trước** `content/before_save`, áp sparse patch kiểm quyền CPT & từng field qua `plugin_can_write_field_for_owner` theo `actor_role` thật (`Admin`/`Editor`), sau đó bắt buộc re-validate đầy đủ (`validate_against_effective` + media refs + slug). Trên đường dẫn Sửa nhanh (`PATCH .../entries/{id}/metadata`), payload đính kèm `mutation_scope: { mode: "metadata", requested_title, requested_slug, taxonomy_key }`; bộ lọc vẫn chịu đầy đủ kiểm quyền field ACL trước, được phép echo không đổi (no-op) nhưng mọi thay đổi thực sự vào `values_patch`, trường không được yêu cầu hoặc phân loại ngoài `taxonomy_key` đều bị từ chối với `422 metadata_hook_scope_conflict` (hướng dẫn mở Single Edit). `publication_status` và `published_at` chỉ do tác nhân chỉ định, không thuộc đầu ra filter.
  - `filter:contact/before_submit`: chạy sau Honeypot/Origin/RateLimit/FormSchema (đã drop `rate_lease` ngắn hạn) và **trước** khi cấp `MutationLease` ghi `contact_submissions`; rejection trả HTTP `422` escaped HTML + `Cache-Control: no-store`.
  - `filter:render/{head,footer,slot}`: chạy trên public GET qua `ExtensionRenderCache` (bounded LRU `512` entries, max `256 KiB`/entry, vô hiệu hóa thuần theo `SiteCacheEpoch` không dùng TTL, bypass ở draft preview & Safe Mode, fail-open không cache lỗi); `sanitize_render_head_fragment` bảo toàn tuyệt đối Core SEO (`<title>`, `robots`, `http-equiv`, `canonical`/`alternate`, `application/ld+json`).
- **Write-Time Materialized Projections (`0050` & `0053`, `storage/plugin_projections.rs`, `seo.rs`, `content_render.rs`):**
  - Bảng `plugin_entry_projections (entry_id, owner_kind, plugin_id, locale, seo_json, ext_json, updated_at)` được tính sẵn bằng `ExecutionPrincipal::Public` cho cả Plugin (`entry.ext`) và Active Theme (`entry.theme_ext`) theo thứ tự ưu tiên **`Admin > Plugin > Theme`** khi tạo/sửa/xuất bản entry (xóa khi chuyển về `draft`/`deleted`), và tính lại theo batch async khi enable plugin / kích hoạt theme / lưu settings (kèm `SiteCacheEpoch::global().bump()` sau mỗi commit).
  - Đường nóng public `GET` chỉ chạy 1 batch `SELECT` (`batch_load_entry_projections_in_conn`) với **`0` Wasm/JS spawn** cho projection: nạp `EntryView.ext[<plugin_id>]` và `EntryView.theme_ext`, trộn `SeoView`, bảo vệ bất biến **`final_noindex = core_noindex || plugin_noindex`**, kiểm bounds `json_ld` (`<= 4` blocks, `<= 16 KiB`, depth `<= 8`, `Value::Object`), và escape `<`/`>`/`&`/U+2028/U+2029 khi serialize `<script type="application/ld+json">`.
