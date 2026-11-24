# SDK AssemblyScript — Plugin ABI v1.1 (`opendash_v1`), `PluginEnvelopeV2` & Host Ops

`cms/sdk/as/opendash.ts` là SDK chính thức cho plugin viết bằng AssemblyScript giao tiếp với host Wasmi của OpenDash CMS theo [Plugin ABI](../../docs/contracts/PLUGIN_ABI.md) (nâng cấp từ ABI v1 lên ABI v1.1 + 15 Host Ops). Toàn bộ API v1 (`getInput`, `setOutput`, `hostCall`, `kvGet`, `kvSet`, `httpRequest`, `escapeJsonString`, `stringifyValue`) vẫn được giữ nguyên để `sample-plugin` v1 biên dịch và chạy không cần sửa mã.

---

## 1. ABI v1.1 & Tự động Readback `-2` (`BUFFER_TOO_SMALL`)

Ngoài 4 hàm gốc (`host_get_input_len`, `host_read_input`, `host_write_output`, `host_call`), SDK bổ sung 2 hàm import trong namespace `"opendash_v1"`:

- `host_last_resp_len(): i32`
- `host_read_last_resp(destPtr: usize, maxLen: i32): i32`

Khi gọi `opendash.hostCall(opJson)`:
1. SDK cấp đệm mặc định `INITIAL_RESP_BUF_CAP = 65536` (64 KiB) và gọi `host_call()`.
2. Nếu phản hồi lớn hơn 64 KiB (≤ `MAX_IPC_FRAME_BYTES = 2 MiB`), host trả về mã `-2`.
3. `hostCall()` tự động gọi `host_last_resp_len()` → cấp đúng `ArrayBuffer(fullLen)` → gọi `host_read_last_resp()` để nhận trọn vẹn JSON mà **không gọi lại `host_call()` lần 2** (chỉ tốn 1 lượt quota `host_calls_count`).

---

## 2. Đọc đầu vào hợp nhất: `readEnvelope()` (`HookEnvelope` v1 & `PluginEnvelopeV2`)

`opendash.readEnvelope(): Envelope | null` đọc `getInput()` và tự động chuẩn hóa cả hai phiên bản envelope:
- Khi có khóa `"envelope_version"` (`PluginEnvelopeV2`): điền đầy đủ `envelopeVersion` (`2`), `kind` (`"hook" | "filter" | "action" | "manual"`), `event`, `occurredAtMs`, `correlationId`, `locale`, `principal` (`"admin" | "editor" | "public" | "system"`), `dispatchDepth`, `payload` (`JSON.Obj`), và `raw`.
- Khi không có `"envelope_version"` (`HookEnvelope` v1): đặt mặc định `envelopeVersion = 1`, `kind = "hook"`, `principal = "system"`, `dispatchDepth = 0`.

---

## 3. Danh sách 15 Host Ops v1.1

Mọi phản hồi từ host hỗ trợ song song định dạng legacy top-level (`kv_get`, `http_request`) và Spec 2 (`{"ok":true,"data":{...}}`).

| Nhóm | Hàm SDK (`cms/sdk/as/opendash.ts`) | Wire `op` | Ghi chú |
| --- | --- | --- | --- |
| **KV Store** | `kvGet(key)` | `kv_get` | Trả `string \| null` (chuỗi JSON raw của `value`) |
| | `kvSet(key, valueJson)` | `kv_set` | Trả `bool`; kiểm `value` là JSON hợp lệ |
| | `kvDelete(key)` | `kv_delete` | Trả `bool` (`ok == true && data.deleted == true`) |
| | `kvList(prefix = null, limit = 50)` | `kv_list` | Chỉ dùng trong `action` hoặc `async post-commit hook` |
| **HTTP Egress** | `httpRequest(method, url, headersJson = "{}", body = "")` | `http_request` | Tuân thủ DNS-pinned allowlist trong `permissions.network` |
| **Content & Taxonomy** | `contentGet(typeKey, entryId, locale = null)` | `content_get` | Trả chuỗi JSON phản hồi đầy đủ `{"ok":true,"data":{"entry":{...}|null}}` từ `hostCall` |
| | `contentList(typeKey, locale = null, limit = 20, offset = 0, publishedOnly = false)` | `content_list` | Phân trang có giới hạn; tự động readback `-2` nếu > 64 KiB |
| | `contentSave(opObj)` | `content_save` | Action `content_write`; truyền `type_key`, `slug`, `title`, `values`, tùy chọn `entry_id`, `expected_version`, `status`, `locale`, `term_ids` |
| | `contentDelete(typeKey, entryId, expectedVersion)` | `content_delete` | Action `content_delete`; kiểm CAS `expected_version` |
| | `taxonomyList(kind, locale = null, limit = 50)` | `taxonomy_list` | Yêu cầu `permissions.taxonomy_read: true` |
| **Media & Settings** | `mediaGet(mediaId)` | `media_get` | Yêu cầu `permissions.media_read: true` |
| | `siteOptionsGet(locale = null)` | `site_options_get` | Yêu cầu `permissions.site_options_read: true` |
| | `pluginSettingsGet()` | `plugin_settings_get` | Yêu cầu `plugin_settings_read: true`; trường `secret` chỉ trả `{configured: bool}` |
| | `pluginSecretGet(key)` | `plugin_secret_get` | Trả `string \| null`; chỉ khóa khai báo trong `plugin_secrets_read` |
| **Diagnostics** | `log(level, message)` | `log` | `level`: `"info" \| "warn" \| "error"`; tối đa 20 dòng/lần chạy |

---

## 4. Typed Output Helpers cho Hooks & Filters

Mọi helper đều tạo cấu trúc qua `JSON.Obj` + `stringifyValue()` (an toàn UTF-8/RFC 8259, không ghép chuỗi JSON thô):

- **Hook `content/before_save`:**
  - `opendash.outputAllow()` → `{"decision":"allow"}`
  - `opendash.outputVeto(code, message, fieldErrorsObj = null)` → `{"decision":"veto",...}`
- **Filter `filter:content/prepare_save`:**
  - `opendash.outputPrepareSavePatch(patchObj)` (ví dụ `{"title":"...","values_patch":{...}}`)
- **Filter `filter:contact/before_submit`:**
  - `opendash.outputContactAllow()` → `{"decision":"allow"}`
  - `opendash.outputContactReject(code, message)` → `{"decision":"reject","code":...,"message":...}`
- **Filter `filter:seo/head`:**
  - `opendash.outputSeoHead(seoObj)` (`title`, `description`, `og_image_id`, `canonical_url`, `noindex`, `json_ld[]`)
- **Filter `filter:entry/view`:**
  - `opendash.outputEntryView(extObj)` (dữ liệu nạp vào `entry.ext.<plugin_id>` lúc ghi)

### Ví dụ viết Filter trong AssemblyScript

```ts
import * as opendash from "../../../../sdk/as/opendash";
import { JSON } from "assemblyscript-json/assembly";

export function opendash_run(): i32 {
  const env = opendash.readEnvelope();
  if (env === null) return 1;

  if (env.event == "filter:entry/view") {
    const ext = new JSON.Obj();
    ext.set("badge", "Verified");
    return opendash.outputEntryView(ext) < 0 ? 1 : 0;
  }

  if (env.event == "content/before_save") {
    return opendash.outputAllow() < 0 ? 1 : 0;
  }

  return 0;
}
```

---

## 5. Kiểm thử tự động bằng `test_sdk_v11.mjs`

Biên dịch `test_harness.ts` bằng `asc` của `sample-plugin` và chạy bằng Node (không cần phụ thuộc ngoài):

```bash
npm --prefix cms/examples/plugins/sample-plugin ci
npm --prefix cms/examples/plugins/sample-plugin run asbuild
./cms/examples/plugins/sample-plugin/node_modules/.bin/asc \
  cms/sdk/as/test_harness.ts \
  --outFile cms/target/opendash_sdk_test.wasm \
  --bindings raw --optimize \
  --lib cms/examples/plugins/sample-plugin/node_modules
node cms/sdk/as/test_sdk_v11.mjs
```

Xem ví dụ plugin v1 tại [sample-plugin](../examples/plugins/sample-plugin/README.md) và hướng dẫn tổng quan tại [PLUGIN_ABI.md](../../docs/contracts/PLUGIN_ABI.md).
