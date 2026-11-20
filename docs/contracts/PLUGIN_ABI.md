# Hợp đồng Plugin ABI v1 — OpenDash

**Phiên bản API**: `1` (kèm Manifest Schema `1` & `2` đã triển khai)\
**Trạng thái**: `AGREED` · `implemented`\
**Tài liệu tham chiếu**: [docs/product/PRODUCT_REQUIREMENTS.md](../product/PRODUCT_REQUIREMENTS.md), [M6 Plugin Extensions](M6_PLUGIN_EXTENSIONS.md), [Wasmi 2.0.0 Documentation](https://docs.rs/wasmi/2.0.0/wasmi/)

---

## 1. Cấu trúc Gói Plugin ZIP & Revisions

Gói ZIP cài đặt vào OpenDash phải tuân thủ nghiêm ngặt các giới hạn an toàn:

1. **Các file trong ZIP**:
   - `manifest.json` (bắt buộc, tối đa `64 KiB`): Chứa thông tin định danh, phiên bản, ranh giới quyền và khai báo mở rộng.
   - `plugin.wasm` (bắt buộc, tối đa `2 MiB`): File nhị phân WebAssembly biên dịch cho `wasm32-unknown-unknown`.
   - `README.md` (tùy chọn).
2. **Quy tắc an toàn khi giải nén**:
   - Số lượng file tối đa trong ZIP: **10 files**, kích thước ZIP tối đa **5 MiB**, tổng dung lượng giải nén tối đa **10 MiB**.
   - Từ chối đường dẫn tuyệt đối, thư mục con, path traversal (`..`, `/`, `\`), symlink hoặc mục trùng lặp.
   - **Không thực thi code tại thời điểm cài đặt**; host chỉ kiểm tra manifest và chữ ký module Wasm (`wasmi::Module::new`).
3. **Immutable Revisions (`plugins/revisions/<revision_digest>/`)**:
   - Staging qua `.staging/<run_uuid>/`, ghi `.complete` và cập nhật bảng `plugin_revisions`, cột `active_revision_id`, `revision_generation` và `cas_version` trên bảng `plugins` (`0022_plugin_revisions.sql`).

---

## 2. Đặc tả `manifest.json` (`PluginManifest`)

```json
{
  "id": "my-plugin",
  "version": "1.0.0",
  "api_version": 1,
  "manifest_schema": 2,
  "min_host_version": "0.1.0",
  "name": "Tên hiển thị của Plugin",
  "description": "Mô tả ngắn gọn về chức năng của plugin",
  "entrypoint": "plugin.wasm",
  "hooks": ["page/after_save", "content/before_save", "content/after_save"],
  "hook_priority": 100,
  "permissions": {
    "sqlite": ["kv_read", "kv_write"],
    "http": ["https://api.example.com"],
    "network": [{ "scheme": "https", "host": "api.example.com", "port": 443, "methods": ["GET", "POST"], "path_prefix": "/v1/" }],
    "content_read": ["post"],
    "content_write": ["post"],
    "content_read_private": [{ "type_key": "post", "field_keys": ["internal_note"] }],
    "content_write_private": [{ "type_key": "post", "field_keys": ["internal_note"] }]
  },
  "actions": [],
  "declarations": {},
  "editor_panels": [],
  "admin_extensions": {}
}
```

### Các trường dữ liệu chính (`cms/crates/opendash-core/src/plugin.rs`):
- **Thông tin cơ bản:** `id` (`PackageId` 3–64 ký tự `[a-z0-9-]`), `version` (SemVer), `api_version` (`1`), `manifest_schema` (`1` hoặc `2`, tùy chọn), `min_host_version` (SemVer, tùy chọn), `name` (1–100 ký tự), `description`, `entrypoint` (`"plugin.wasm"`).
- **Hooks & Actions:** `hooks` (`page/after_save`, `content/before_save`, `content/after_save`), `hook_priority` (`i32`), `actions` (`PluginActionDeclaration`: `id`, `title`, `required_role`, `capabilities`).
- **Quyền (`PluginPermissions`):**
  - `sqlite`: `"kv_read"`, `"kv_write"`.
  - `http`: Allowlist origin v1; hoặc `network`: `NetworkScopeGrant` v2 (`scheme`, `host`, `port`, `methods`, `path_prefix`).
  - `content_read`, `content_write`: Danh sách `type_key` CPT được phép đọc/ghi các trường `public`.
  - `content_read_private`, `content_write_private`: `ScopedFieldGrant` cấp quyền tường minh trên từng trường `private` của một CPT.
- **Khai báo mở rộng (Declarative Extensions):** `declarations` (`CntDeclarations`: `content_types`, `taxonomy_definitions`, `taxonomy_eligibility`, `field_groups`, `rewrite_base`), `editor_panels` (`ManifestActionPanelInput`), `admin_extensions` (`AdminExtensions`: columns, entry actions, term panels).

---

## 3. WebAssembly Interface (`opendash_v1`)

### 3.1. Guest Exports (Guest cung cấp cho Host)
1. `memory`: Linear memory WebAssembly chuẩn.
2. `opendash_run() -> i32`: `0` = thành công, khác `0` = mã lỗi guest.

### 3.2. Host Imports (Host cung cấp cho Guest trong module `"opendash_v1"`)
- `host_get_input_len() -> i32`
- `host_read_input(dest_ptr: i32, max_len: i32) -> i32`
- `host_write_output(src_ptr: i32, len: i32) -> i32`
- `host_call(op_ptr: i32, op_len: i32, resp_ptr: i32, max_resp_len: i32) -> i32` (trả `-2` nếu `BUFFER_TOO_SMALL`).
- Hỗ trợ thêm `env:abort(i32, i32, i32, i32)` cho AssemblyScript runtime.

---

## 4. Định dạng Host Operations qua `host_call` (`plugin/host_ops.rs`)

1. **`kv_get`** (`sqlite:kv_read`): `{ "op": "kv_get", "key": "..." }` → `{ "ok": true, "found": true, "value": ... }`. Giới hạn key `256` bytes.
2. **`kv_set`** (`sqlite:kv_write`): `{ "op": "kv_set", "key": "...", "value": ... }` → `{ "ok": true }`. Giới hạn value `512 KiB`; bị chặn trong `content/before_save` và `content/after_save`.
3. **`http_request`** (`http` / `network`): `{ "op": "http_request", "method": "GET", "url": "...", "headers": {...}, "body": null }`. Tối đa `10` HTTP calls/invocation; DNS pinning chống SSRF; bị chặn trong save hooks.
4. **`content_save`** (`content_write` / `content_write_private`):
   - Tạo hoặc cập nhật entry ở chế độ `HookExecutionMode::Standard` (bị chặn trong save hooks):
   ```json
   {
     "op": "content_save",
     "type_key": "post",
     "entry_id": null,
     "expected_version": null,
     "slug": "bai-viet-moi",
     "title": "Tiêu đề",
     "values": { "body": "Nội dung" },
     "term_ids": [1]
   }
   ```
   - Host kiểm tra `plugin_can_write_field`, `verify_plugin_action_fence` (`enabled = 1`, `revision_generation`), CAS `expected_version`, media pins và `sync_claims_in_tx` trong cùng transaction `StagedCommitGuard`.

---

## 5. Giới hạn Tài nguyên, Cô lập Worker & Commit Handover

| Tài nguyên | Giới hạn | Cơ chế kiểm soát |
|---|---|---|
| **Fuel (CPU)** | 20.000.000 units / invocation | Wasmi engine fuel metering (`OutOfFuel` trap). |
| **Linear Memory** | 16 MB (256 pages Wasm) | `StoreLimitsBuilder::memory_size(16 << 20)`. |
| **Upload Plugin ZIP** | 5 MB | `DefaultBodyLimit::max(5MB)` trên `POST /api/admin/plugins`. |
| **Input / Output Payload** | 1 MB | `DefaultBodyLimit::max(1MB)` và kiểm tra `host_write_output`. |
| **KV Key / Value** | 256 bytes / 512 KiB | `MAX_KV_KEY_BYTES` / `MAX_KV_VALUE_BYTES` trong `host_ops.rs`. |
| **Host Calls / HTTP Sub-quota** | 100 tổng / 10 HTTP calls | Bộ đếm trong invocation; HTTP timeout `2s`, response stream cap `1 MiB`. |
| **Invocation Deadline** | 5 giây / invocation | Tokio deadline + `ProcessGuard` (`--plugin-worker`). |
| **Concurrency Pool** | 4 tác vụ đồng thời | Tokio Semaphore fail-fast `1000ms`. |

- **Worker cô lập & Zero Late Writes:** Mọi lượt chạy tạo tiến trình con `--plugin-worker` từ cùng binary; timeout/hủy sẽ kill/wait tiến trình con đồng bộ, `kv_set` và `content_save` được stage trong worker và chỉ commit vào SQLite qua `StagedCommitGuard` khi chưa quá hạn.
- **HTTP SSRF & DNS Pinning (`plugin/egress.rs`):** Phân giải DNS và chặn toàn bộ IP loopback/private/CGNAT/link-local/multicast/ULA, pin `SocketAddr`, bật `.no_proxy()`, tắt redirect (`Policy::none()`), đọc stream tối đa `1 MiB`.
