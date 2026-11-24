# Đặc tả Thiết kế EGM — Spec 3: Dual Guest SDK (`AssemblyScript` & `opendash-plugin-sdk`) & Reference Examples

- **Status:** Approved
- **Approved By:** Owner
- **Approval Evidence:** Owner chat decisions 1A-4A & directive 2026-11-24 (Issue từng Slice cho Nova)
- **Goldfish Review:** 3-round zero-context Goldfish review (egm-design, egm-codebase-details, egm-implementation-readiness) · 2026-11-24 · 3/3 APPROVED
- **Date:** 2026-11-24
- **Author:** Orion (Elephant)
- **Thuộc Lộ trình:** Giai đoạn 4 trong Tầm nhìn 6 Giai đoạn (`concepts_brief.md`: Decision `4A`)
- **Thành phần chạm tới:** `.gitignore`, `cms/Cargo.toml`, `cms/Cargo.lock`, `cms/crates/opendash-plugin-sdk/Cargo.toml`, `cms/crates/opendash-plugin-sdk/src/{abi,host,lib,mock,types}.rs`, `cms/sdk/as/{opendash.ts,test_harness.ts,test_sdk_v11.mjs}`, `cms/sdk/README.md`, `cms/examples/plugins/rust-seo-plugin/{Cargo.toml,README.md,manifest.json,src/lib.rs}`, `cms/examples/themes/sample-theme-v3/{README.md,single.html,theme.json}`, `cms/examples/themes/sample-theme-v3/assets/app.css`, `docs/designs/SITEMAP.md`

---

## Overview

Đặc tả này hoàn thiện **tầng Developer Experience (Giai đoạn 4)** trên nền ABI v1.1, `manifest_schema: 3`, `PluginEnvelopeV2`, 15 Host Ops và `plugin_entry_projections` đã chốt ở Spec 1 & Spec 2:
1. **AssemblyScript SDK v1.1 (`cms/sdk/as/opendash.ts`):** Giữ tương thích ngược 100% với `sample-plugin` hiện có (`getInput`, `setOutput`, `hostCall`, `kvGet`, `kvSet`, `httpRequest`, `escapeJsonString`, `stringifyValue`), đồng thời nâng cấp `hostCall()` tự động đọc lại payload vượt đệm `-2` (`BUFFER_TOO_SMALL`) qua `host_last_resp_len` / `host_read_last_resp`, bổ sung `readEnvelope()` (hỗ trợ cả `HookEnvelope` v1 và `PluginEnvelopeV2`), đầy đủ wrapper cho 11 Host Ops mới, và typed output helpers cho tất cả hooks/filters.
2. **Rust Guest SDK (`cms/crates/opendash-plugin-sdk`):** Crate thuần Rust (`edition = "2024"`, chỉ phụ thuộc workspace `serde` và `serde_json` — **0 dependency ngoài mới**), cung cấp `export_plugin!` macro (`#[unsafe(no_mangle)]`), ABI v1.1 bindings (`unsafe extern "C"`), typed Host Ops & Envelope/Output structs, và **`MockHost` test harness** cho phép chạy `cargo test --workspace --release --locked` trên mọi máy (kể cả khi chưa cài target `wasm32-unknown-unknown`).
3. **Reference Examples (`rust-seo-plugin` & `sample-theme-v3`):** Một plugin mẫu bằng Rust (`cms/examples/plugins/rust-seo-plugin/`, tách `[workspace]` độc lập, trình diễn `manifest_schema: 3`, `filter:seo/head`, `filter:entry/view`, `content/after_publish`, write-only `secret` setting và unit test qua `MockHost`) cùng theme mẫu v3 `cms/examples/themes/sample-theme-v3/` (`theme.json` + `single.html` + `assets/app.css`) sử dụng `theme_assets`, `date_format`, `format_number`, `breadcrumbs`, `related_entries`, và `entry.ext["rust-seo-plugin"] | default({})`.

---

## The Problem

1. **`cms/sdk/as/opendash.ts` hiện chỉ dừng ở ABI v1 (4 op):**
   - `hostCall()` cấp đệm cứng `64 KiB` (`L125`); khi `host_call` trả `-2` (`BUFFER_TOO_SMALL`), SDK trả về chuỗi lỗi cứng mà không gọi `host_last_resp_len()` / `host_read_last_resp()`.
   - Chưa có wrapper cho `kvDelete`, `kvList`, `contentSave`, `contentDelete`, `contentGet`, `contentList`, `taxonomyList`, `mediaGet`, `siteOptionsGet`, `pluginSettingsGet`, `pluginSecretGet`, `log`, cũng chưa có parser `PluginEnvelopeV2` hay builder cho `prepare_save`, `contact/before_submit`, `seo/head`, `entry/view`.
2. **Chưa có Rust Guest SDK chính thức:**
   - Developer viết plugin phức tạp (như E-commerce/SEO) bằng Rust phải tự khai báo `extern "C"` raw pointers, tự quản lý `-2` readback và dễ viết sai JSON envelope hoặc cấu trúc `HostResponse`.
   - Khi viết plugin bằng Rust, cần kiểm thử logic nghiệp vụ nhanh bằng `cargo test` native mà không phải spawn Wasmi cho từng test đơn vị.
3. **Thiếu ví dụ tham chiếu end-to-end gắn kết Spec 1 + Spec 2:**
   - `cms/examples/plugins/sample-plugin` chỉ minh họa `manifest_schema: 1` với `kv_get`/`kv_set`; `cms/examples/themes/sample-theme` chỉ là draft v2 chưa có `theme.json`, chưa có thư mục Theme v3 minh họa `entry.ext.*` và các filter C-SEC mới của Spec 1.

---

## Goals

1. Nâng cấp `cms/sdk/as/opendash.ts` lên ABI v1.1 + 15 Host Ops + `PluginEnvelopeV2` + typed output builders mà **không làm gãy** `cms/examples/plugins/sample-plugin/assembly/index.ts`.
2. Tạo crate `cms/crates/opendash-plugin-sdk` trong `cms/Cargo.toml` (`[workspace.members]`) dùng **0 crate ngoài mới** (chỉ `serde` & `serde_json` đã pin ở workspace), tuân thủ Rust `edition = "2024"` (`#[unsafe(no_mangle)]`, `unsafe extern "C"`), tự động xử lý `-2` readback, và hỗ trợ `MockHost` native.
3. Cung cấp plugin mẫu `cms/examples/plugins/rust-seo-plugin/` (kèm `MockHost` unit tests) và theme mẫu v3 `cms/examples/themes/sample-theme-v3/` minh họa đầy đủ hợp đồng Spec 1 + Spec 2.
4. Giữ **0 DB migrations** và **0 thay đổi** tới `opendash-server` / `opendash-core` runtime.

## Non-Goals

- Không thay đổi `opendash-server` hoặc `opendash-core` trong Spec 3 (mọi ABI v1.1 host imports và Host Ops đã được chốt ở Spec 2).
- Không thêm bất kỳ dependency ngoài nào vào `cms/Cargo.toml` hoặc `cms/Cargo.lock` (kể cả `wasm-bindgen` hay `serde-wasm-bindgen`).
- Không biên dịch bắt buộc `wasm32-unknown-unknown` trong `cargo test --workspace` mặc định (vì toolchain CI/máy dev có thể chưa cài target `wasm32-unknown-unknown`; `MockHost` kiểm chứng toàn bộ SDK & `rust-seo-plugin` trên host native, còn build `.wasm` được hướng dẫn và kiểm tra tùy chọn khi có target).

---

## Critical User Journeys (CUJs)

1. **CUJ-1 — Plugin AssemblyScript đọc danh sách bài viết > 64 KiB qua `-2` Readback tự động:**
   - *Given* plugin AssemblyScript dùng `cms/sdk/as/opendash.ts` gọi `opendash.contentList("post", null, 50, 0, true)` và phản hồi JSON từ Host dài `120 KiB` (`> 65536` bytes).
   - *When* `host_call` trả `-2` (`BUFFER_TOO_SMALL`), `hostCall()` tự gọi `host_last_resp_len()` → cấp buffer `120 KiB` → gọi `host_read_last_resp(ptr, len)`.
   - *Then* plugin nhận trọn vẹn JSON `{"ok":true,"data":{"items":[...],"total":...}}` chỉ trong **1 lần thực thi** `content_list`, không tốn quota `host_calls_count`.
2. **CUJ-2 — Developer viết Plugin Rust với `export_plugin!` & kiểm thử 100% Native bằng `MockHost`:**
   - *Given* developer tạo `cms/examples/plugins/rust-seo-plugin` phụ thuộc `opendash-plugin-sdk` và gắn `export_plugin!(handle)`.
   - *When* chạy `cargo test --manifest-path cms/examples/plugins/rust-seo-plugin/Cargo.toml` trên macOS/Windows/Linux mà chưa cài `wasm32-unknown-unknown`.
   - *Then* `MockHost::default()` giả lập cả `PluginEnvelopeV2`, `kv_get`/`kv_set`/`kv_delete`, `plugin_settings_get`, `plugin_secret_get`, `log`, và cả kịch bản tràn đệm `-2` (`with_initial_resp_buf_cap(64)`), pass 100% test native.
3. **CUJ-3 — Theme v3 render dữ liệu từ `rust-seo-plugin` với `0` Wasm spawn:**
   - *Given* `rust-seo-plugin` cài `filter:seo/head` (trả `json_ld` Product/Article) và `filter:entry/view` (trả `{"badge":"Nổi bật","reading_note":"Đã kiểm duyệt"}`).
   - *When* theme mẫu `sample-theme-v3` (`theme.json` `manifest_version: 3`, `assets/app.css`, `single.html`) render `{% set seo_ext = entry.ext["rust-seo-plugin"] | default({}) %}{{ seo_ext.badge | default("") }}`, `{{ entry.published_at | date_format("%d/%m/%Y") }}`, `{{ theme_assets["assets/app.css"] }}`.
   - *Then* theme mẫu vượt qua `./cms/target/release/opendash --check-theme cms/examples/themes/sample-theme-v3` (`ok: true`, Spec 1) và `rust-seo-plugin` vượt qua `--check-plugin` (Spec 2) khi biên dịch ra `plugin.wasm`.
4. **CUJ-4 — Tương thích ngược hoàn toàn với `sample-plugin` v1:**
   - *Given* `cms/examples/plugins/sample-plugin/assembly/index.ts` không sửa bất kỳ dòng nào.
   - *When* biên dịch `sample-plugin` với `cms/sdk/as/opendash.ts` bản v1.1.
   - *Then* toàn bộ `opendash.getInput()`, `opendash.kvGet()`, `opendash.kvSet()`, `opendash.stringifyValue()`, `opendash.setOutput()` biên dịch và chạy giống hệt v1.

---

## Technical Plan (Architecture)

### 1. AssemblyScript SDK v1.1 (`cms/sdk/as/opendash.ts`, `cms/sdk/README.md`)

#### 1.1 ABI v1.1 Imports & Auto `-2` Readback
- Giữ nguyên 4 `@external("opendash_v1", ...)` gốc và bổ sung 2 import từ Spec 2:
  ```ts
  @external("opendash_v1", "host_last_resp_len")
  declare function host_last_resp_len(): i32;

  @external("opendash_v1", "host_read_last_resp")
  declare function host_read_last_resp(destPtr: usize, maxLen: i32): i32;
  ```
- Cập nhật `export function hostCall(opJson: string): string`:
  1. Gọi `host_call(opPtr, opBuf.byteLength, respPtr, maxRespLen)` với `maxRespLen = 65536`.
  2. Nếu `respLen > 0`: decode `respBuf.slice(0, respLen)` như cũ.
  3. Nếu `respLen == 0`: trả `'{"ok":false,"error":"Empty host response"}'`.
  4. **Nếu `respLen == -2` (`BUFFER_TOO_SMALL`):**
     - Gọi `const fullLen = host_last_resp_len();`
     - Kiểm tra `fullLen > 0 && fullLen <= 2097152` (`MAX_IPC_FRAME_BYTES = 2 MiB`; nếu ngoài biên trả `'{"ok":false,"error":"Invalid oversized response length"}'`).
     - Cấp `const fullBuf = new ArrayBuffer(fullLen);` và gọi `const copied = host_read_last_resp(changetype<usize>(fullBuf), fullLen);`
     - Nếu `copied <= 0`: trả `'{"ok":false,"error":"Failed to read oversized host response"}'`.
     - Trả `String.UTF8.decode(fullBuf.slice(0, copied))`.
  5. Các mã lỗi âm khác (`-1`, v.v.): trả `'{"ok":false,"error":"Host call returned error code"}'`.

#### 1.2 Tương thích Dual Response Format (`HostOperationResponse` v1 top-level vs Spec 2 `data`)
- Trong `cms/crates/opendash-server/src/plugin/host_ops.rs:95-110`, các op v1 (`kv_get`, `http_request`) trả field ở top-level (`{"ok":true,"found":true,"value":...}` và `{"ok":true,"status":200,"headers":{...},"body":"..."}`), trong khi Spec 2 định nghĩa 11 op mới trả trong `{"ok":true,"data":{...}}`.
- Trong `cms/sdk/as/opendash.ts`:
  - `kvGet(key: string): string | null` kiểm tra cả `resObj.getValue("data")` (nếu op trả trong `data`) lẫn top-level `found`/`value` (giữ nguyên hành vi hiện tại với `host_ops.rs:95-110`).
  - `httpRequest(...)` giữ nguyên trả raw JSON string để tương thích 100% với code cũ.
  - Thêm helper nội bộ `extractDataObj(resStr: string): JSON.Obj | null` kiểm `ok == true` và trả về `data` object (hoặc `resObj` fallback nếu top-level).

#### 1.3 Envelope, 11 Host Ops Mới & Output Builders trong `opendash.ts`
- **Envelope parser:**
  ```ts
  export class Envelope {
    envelopeVersion: i32 = 1;
    kind: string = "hook";
    event: string = "";
    occurredAtMs: i64 = 0;
    correlationId: string = "";
    locale: string = "";
    principal: string = "system";
    dispatchDepth: i32 = 0;
    payload: JSON.Obj = new JSON.Obj();
    raw: JSON.Obj = new JSON.Obj();
  }
  export function readEnvelope(): Envelope | null
  ```
  - Nếu `raw.has("envelope_version")` → đọc theo `PluginEnvelopeV2` (`envelope_version`, `kind`, `event`, `occurred_at_ms`, `correlation_id`, `locale`, `principal`, `dispatch_depth`, `payload`).
  - Ngược lại → đọc theo `HookEnvelope` v1 (`event`, `correlation_id`, `occurred_at_ms`, `payload`, mặc định `envelopeVersion = 1`, `kind = "hook"`, `principal = "system"`).
- **11 Host Ops v1.1 Wrappers (cùng `contentSave`, khớp 100% snake_case wire keys trong `HostOperationRequest` của Spec 2):**
  - `export function kvDelete(key: string): bool` *(gửi `{"op":"kv_delete","key":...}`; trả `true` khi `ok == true` và `data.deleted == true`, ngược lại `false`)*
  - `export function kvList(prefix: string | null = null, limit: i32 = 50): string` *(gửi `op: "kv_list"`, kèm `"prefix"` nếu non-null/non-empty và `"limit"`; trả raw JSON response)*
  - `export function contentGet(typeKey: string, entryId: i64, locale: string | null = null): string` *(gửi `{"op":"content_get","type_key":...,"entry_id":...}`, kèm `"locale"` nếu non-null/non-empty)*
  - `export function contentList(typeKey: string, locale: string | null = null, limit: i32 = 20, offset: i32 = 0, publishedOnly: bool = false): string` *(gửi `{"op":"content_list","type_key":...,"limit":...,"offset":...,"published_only":...}`, kèm `"locale"` nếu non-null/non-empty)*
  - `export function contentSave(opObj: JSON.Obj): string` *(tự gán `opObj.set("op", "content_save")` nếu chưa có; caller truyền các key snake_case `type_key`, `slug`, `title`, `values`, tùy chọn `entry_id`, `expected_version`, `status`, `term_ids`)*
  - `export function contentDelete(typeKey: string, entryId: i64, expectedVersion: i64): string` *(gửi `{"op":"content_delete","type_key":...,"entry_id":...,"expected_version":...}`)*
  - `export function taxonomyList(kind: string, locale: string | null = null, limit: i32 = 50): string` *(gửi `{"op":"taxonomy_list","kind":...,"limit":...}`, kèm `"locale"` nếu non-null/non-empty)*
  - `export function mediaGet(mediaId: string): string` *(gửi `{"op":"media_get","media_id":...}`)*
  - `export function siteOptionsGet(locale: string | null = null): string` *(gửi `{"op":"site_options_get"}`, kèm `"locale"` nếu non-null/non-empty)*
  - `export function pluginSettingsGet(): string` *(gửi `{"op":"plugin_settings_get"}`)*
  - `export function pluginSecretGet(key: string): string | null` *(gửi `{"op":"plugin_secret_get","key":...}`; kiểm `ok == true`, đọc `data.value`: nếu là `JSON.Str` trả chuỗi giải mã, ngược lại hoặc `!ok` trả `null`)*
  - `export function log(level: string, message: string): bool` *(gửi `{"op":"log","level":...,"message":...}` với `level`: `"info" | "warn" | "error"`; trả `ok == true`)*
- **Typed Output Helpers *(dùng `JSON.Obj` + `stringifyValue` / `escapeJsonString`, không ghép chuỗi thô)*:**
  - `export function outputAllow(): i32` → `setOutput('{"decision":"allow"}')`
  - `export function outputVeto(code: string, message: string, fieldErrorsObj: JSON.Obj | null = null): i32` *(dựng `JSON.Obj` với `decision: "veto"`, `code`, `message`, tùy chọn `field_errors` rồi `setOutput(stringifyValue(obj))`)*
  - `export function outputPrepareSavePatch(patchObj: JSON.Obj): i32`
  - `export function outputContactAllow(): i32` → `setOutput('{"decision":"allow"}')`
  - `export function outputContactReject(code: string, message: string): i32` *(dựng `JSON.Obj` với `decision: "reject"`, `code`, `message` rồi `setOutput(stringifyValue(obj))`)*
  - `export function outputSeoHead(seoObj: JSON.Obj): i32`
  - `export function outputEntryView(extObj: JSON.Obj): i32`

---

### 2. Rust Guest SDK (`cms/crates/opendash-plugin-sdk`)

#### 2.1 Crate Manifest & Workspace Integration
- **`cms/Cargo.toml`:** Thêm `"crates/opendash-plugin-sdk"` vào `[workspace.members]` (và cập nhật `cms/Cargo.lock`).
- **`cms/crates/opendash-plugin-sdk/Cargo.toml`:**
  ```toml
  [package]
  name = "opendash-plugin-sdk"
  version = "0.1.0"
  edition = "2024"

  [dependencies]
  serde = { workspace = true }
  serde_json = { workspace = true }
  ```
  *(Không thêm crate ngoài mới; tuân thủ `check-standards.py`.)*

#### 2.2 ABI Layer (`src/abi.rs`)
- **Constants:**
  - `pub const INITIAL_RESP_BUF_CAP: usize = 64 * 1024;`
  - `pub const MAX_IPC_FRAME_BYTES: usize = 2 * 1024 * 1024;`
- **Wasm32 Foreign Imports (Rust 2024 `unsafe extern "C"`):**
  ```rust
  #[cfg(target_arch = "wasm32")]
  #[link(wasm_import_module = "opendash_v1")]
  unsafe extern "C" {
      fn host_get_input_len() -> i32;
      fn host_read_input(dest_ptr: i32, max_len: i32) -> i32;
      fn host_write_output(src_ptr: i32, len: i32) -> i32;
      fn host_call(op_ptr: i32, op_len: i32, resp_ptr: i32, max_resp_len: i32) -> i32;
      fn host_last_resp_len() -> i32;
      fn host_read_last_resp(dest_ptr: i32, max_len: i32) -> i32;
  }
  ```
- **Public ABI Functions (`abi.rs`):**
  - `pub fn read_input_bytes() -> Result<Vec<u8>, SdkError>`: đọc toàn bộ input (`0..=MAX_IPC_FRAME_BYTES`); trên `not(target_arch = "wasm32")` đọc từ thread-local `MockHost`.
  - `pub fn write_output_bytes(bytes: &[u8]) -> Result<(), SdkError>`: ghi output; trả `SdkError::HostError` nếu kết quả `< 0`.
  - `pub fn host_call_raw(req_json: &[u8]) -> Result<Vec<u8>, SdkError>`:
    1. Khởi tạo buffer đã zero-init `let mut buf = vec![0u8; INITIAL_RESP_BUF_CAP];` (tuyệt đối không dùng `Vec::with_capacity` chưa khởi tạo khi truyền raw pointer cho host) và thử `host_call`.
    2. Nếu trả `-2` (`BUFFER_TOO_SMALL`): gọi `host_last_resp_len()`, kiểm tra `1..=MAX_IPC_FRAME_BYTES`, cấp `let mut full_buf = vec![0u8; full_len as usize];` và gọi `host_read_last_resp(full_buf.as_mut_ptr() as i32, full_len)` rồi truncate về số byte copy được (không gọi lại `host_call`).
    3. Trên `not(target_arch = "wasm32")`: đi qua cùng logic `MockHost::call_with_buf_cap` để kiểm chứng được cả nhánh `-2` readback trên native tests!

#### 2.3 Exact Types & Error Enum (`src/types.rs`)
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdkError {
    InputTooLarge(usize),
    InvalidInputUtf8(String),
    Json(String),
    HostError(String),
    UnexpectedResponse(String),
    MockNotInitialized,
}

// Bắt buộc triển khai `std::fmt::Display` và `std::error::Error` cho `SdkError`
// để `export_plugin!` (`&err.to_string()`) và `?` hoạt động chuẩn.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(default = "default_envelope_version")]
    pub envelope_version: u32,
    #[serde(default = "default_envelope_kind")]
    pub kind: String,
    pub event: String,
    #[serde(default)]
    pub occurred_at_ms: u64,
    #[serde(default)]
    pub correlation_id: String,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default = "default_principal")]
    pub principal: String,
    #[serde(default)]
    pub dispatch_depth: u8,
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Hỗ trợ cả phản hồi legacy v1 (top-level `found`/`value`/`status`/`headers`/`body`)
/// lẫn Spec 2 (`data: Option<serde_json::Value>`).
#[derive(Debug, Clone, Deserialize)]
pub struct HostResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    #[serde(default)]
    pub found: Option<bool>,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub body: Option<String>,
}
```
- Định nghĩa đầy đủ các DTO guest tương ứng Spec 2:
  - `KvItem { pub key: String, pub value: serde_json::Value }`
  - `HttpRequestInput<'a> { pub method: &'a str, pub url: &'a str, pub headers: BTreeMap<String, String>, pub body: Option<&'a str>, pub timeout_ms: Option<u64> }`
  - `HttpResponse { pub status: u16, pub headers: BTreeMap<String, String>, pub body: String }`
  - `ContentSaveInput { pub type_key: String, pub entry_id: Option<i64>, pub expected_version: Option<i64>, pub title: Option<String>, pub slug: Option<String>, pub status: Option<String>, pub values: serde_json::Map<String, serde_json::Value>, pub term_ids: Option<Vec<i64>> }`
  - `ContentEntry { pub id: i64, pub type_key: String, pub slug: String, pub title: String, pub status: String, pub version: i64, pub locale: String, pub values: serde_json::Map<String, serde_json::Value>, pub term_ids: Vec<i64> }`
  - `ContentListResult { pub items: Vec<ContentEntry>, pub total: u64 }`
  - `TaxonomyTerm { pub id: i64, pub kind: String, pub slug: String, pub name: String, pub parent_id: Option<i64> }`
  - `MediaAsset { pub id: String, pub filename: String, pub mime_type: String, pub url: String, pub width: Option<i64>, pub height: Option<i64>, pub alt: Option<String> }`
  - `LogLevel` (`Info`, `Warn`, `Error` — serialize `snake_case`)
  - `BeforeSaveDecision` (`Allow` / `Veto { code: String, message: String, field_errors: BTreeMap<String, String> }` — `#[serde(tag = "decision", rename_all = "snake_case")]`)
  - `ContentPrepareSavePatch { pub title: Option<String>, pub slug: Option<String>, pub values_patch: BTreeMap<String, serde_json::Value>, pub term_ids: Option<Vec<i64>> }`
  - `ContactBeforeSubmitDecision` (`Allow` / `Reject { code: String, message: String }` — `#[serde(tag = "decision", rename_all = "snake_case")]`)
  - `SeoHeadOutput { pub title: Option<String>, pub description: Option<String>, pub og_image_id: Option<String>, pub canonical_url: Option<String>, pub noindex: Option<bool>, pub json_ld: Vec<serde_json::Value> }`
  - `pub type EntryViewOutput = serde_json::Map<String, serde_json::Value>;`

#### 2.4 Typed Host Functions (`src/host.rs`)
- Đọc/ghi Envelope & Output:
  - `pub fn read_envelope() -> Result<Envelope, SdkError>`
  - `pub fn write_output<T: Serialize>(value: &T) -> Result<(), SdkError>`
- 15 Host Ops typed:
  - `pub fn kv_get(key: &str) -> Result<Option<serde_json::Value>, SdkError>` *(tự đọc `data.found`/`data.value` hoặc top-level `found`/`value`)*
  - `pub fn kv_set(key: &str, value: &serde_json::Value) -> Result<(), SdkError>`
  - `pub fn kv_delete(key: &str) -> Result<bool, SdkError>`
  - `pub fn kv_list(prefix: Option<&str>, limit: Option<usize>) -> Result<Vec<KvItem>, SdkError>`
  - `pub fn http_request(req: &HttpRequestInput<'_>) -> Result<HttpResponse, SdkError>` *(hỗ trợ cả `data` và top-level `status`/`headers`/`body`)*
  - `pub fn content_save(input: &ContentSaveInput) -> Result<serde_json::Value, SdkError>`
  - `pub fn content_delete(type_key: &str, entry_id: i64, expected_version: i64) -> Result<(), SdkError>`
  - `pub fn content_get(type_key: &str, entry_id: i64, locale: Option<&str>) -> Result<Option<ContentEntry>, SdkError>`
  - `pub fn content_list(type_key: &str, locale: Option<&str>, limit: Option<usize>, offset: Option<usize>, published_only: Option<bool>) -> Result<ContentListResult, SdkError>`
  - `pub fn taxonomy_list(kind: &str, locale: Option<&str>, limit: Option<usize>) -> Result<Vec<TaxonomyTerm>, SdkError>`
  - `pub fn media_get(media_id: &str) -> Result<Option<MediaAsset>, SdkError>`
  - `pub fn site_options_get(locale: Option<&str>) -> Result<serde_json::Map<String, serde_json::Value>, SdkError>`
  - `pub fn plugin_settings_get() -> Result<serde_json::Map<String, serde_json::Value>, SdkError>`
  - `pub fn plugin_secret_get(key: &str) -> Result<Option<String>, SdkError>`
  - `pub fn log(level: LogLevel, message: &str) -> Result<(), SdkError>`

#### 2.5 Native `MockHost` Test Harness (`src/mock.rs`, `#[cfg(not(target_arch = "wasm32"))]`)
```rust
pub struct MockHost {
    pub kv: BTreeMap<String, serde_json::Value>,
    pub entries: BTreeMap<(String, i64), ContentEntry>,
    pub taxonomies: BTreeMap<String, Vec<TaxonomyTerm>>,
    pub media: BTreeMap<String, MediaAsset>,
    pub site_options: serde_json::Map<String, serde_json::Value>,
    pub plugin_settings: serde_json::Map<String, serde_json::Value>,
    pub plugin_secrets: BTreeMap<String, String>,
    pub logs: Vec<(LogLevel, String)>,
    pub http_handler: Option<Box<dyn Fn(&str, &str, &BTreeMap<String, String>, Option<&str>) -> Result<HttpResponse, String>>>,
    pub initial_resp_buf_cap: usize, // mặc định 65536; có thể đặt nhỏ (vd 64) để test -2 readback!
}

pub struct MockHostOutcome<R> {
    pub result: R,
    pub output_bytes: Option<Vec<u8>>,
    pub kv: BTreeMap<String, serde_json::Value>,
    pub entries: BTreeMap<(String, i64), ContentEntry>,
    pub staged_content_saves: Vec<ContentSaveInput>,
    pub staged_content_deletes: Vec<(String, i64, i64)>,
    pub logs: Vec<(LogLevel, String)>,
    pub host_calls: usize,
    pub oversized_readbacks: usize,
}
```
- `pub fn run_in_mock_host<F, R>(host: MockHost, envelope: &Envelope, f: F) -> MockHostOutcome<R>`: gắn `MockHost` vào `thread_local! RefCell<Option<ActiveMockState>>` (dọn sạch bằng RAII `Drop` guard kể cả khi panic), chạy `f()`, và trả về `MockHostOutcome`.
- Trong `MockHost::dispatch_call`: hỗ trợ đầy đủ **15/15 Host Ops**, bao gồm `content_save` (cập nhật/tạo mới `MockHost.entries` kèm kiểm CAS `expected_version` và ghi nhận `staged_content_saves`) và `content_delete` (xóa khỏi `MockHost.entries` kèm kiểm `expected_version` và ghi nhận `staged_content_deletes`). Nếu `resp_bytes.len() > initial_resp_buf_cap`, lưu `last_resp_bytes = Some(resp_bytes)`, tăng `oversized_readbacks += 1`, và trả `-2` để `abi::host_call_raw` chạy đúng đường `host_last_resp_len()` + `host_read_last_resp()` mà không tăng `host_calls` lần hai!

#### 2.6 `export_plugin!` Macro (`src/lib.rs`)
```rust
#[macro_export]
macro_rules! export_plugin {
    ($handler:path) => {
        #[cfg(target_arch = "wasm32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn opendash_run() -> i32 {
            let handler: fn($crate::Envelope) -> ::std::result::Result<(), $crate::SdkError> = $handler;
            match $crate::host::read_envelope().and_then(handler) {
                Ok(()) => 0,
                Err(err) => {
                    let _ = $crate::host::log($crate::LogLevel::Error, &err.to_string());
                    1
                }
            }
        }
    };
}
```
*(Lưu ý Rust `edition = "2024"`: bắt buộc dùng `#[unsafe(no_mangle)]` và `unsafe extern "C"`.)*

---

### 3. Reference Examples (`rust-seo-plugin`, `sample-theme` v3) & Documentation

#### 3.1 `cms/examples/plugins/rust-seo-plugin/` & `.gitignore`
- **`.gitignore`:** Thêm `/cms/examples/plugins/rust-seo-plugin/target/` và `/cms/examples/plugins/rust-seo-plugin/Cargo.lock` để chạy `cargo test`/`cargo build` trên example độc lập không để lại untracked files trong repo.
- **`Cargo.toml`:** Khai báo `[workspace]` rỗng để Cargo không coi là thành viên bị bỏ quên của `cms/Cargo.toml`:
  ```toml
  [package]
  name = "rust-seo-plugin"
  version = "0.1.0"
  edition = "2024"

  [workspace]

  [lib]
  crate-type = ["cdylib", "rlib"]

  [dependencies]
  opendash-plugin-sdk = { path = "../../../crates/opendash-plugin-sdk" }
  serde = { version = "1", features = ["derive"] }
  serde_json = "1"
  ```
- **`manifest.json` (`manifest_schema: 3`, đủ toàn bộ trường bắt buộc của `PluginManifest`):**
  - `id: "rust-seo-plugin"`, `version: "0.1.0"`, `api_version: 1`, `manifest_schema: 3`, `min_host_version: "0.1.0"`, `name: "Rust SEO & View Projection Example"`, `description: "Plugin mẫu minh họa Rust Guest SDK v1.1, filters seo/head + entry/view và secret webhook"`, `entrypoint: "plugin.wasm"`.
  - `hooks: ["content/after_publish"]`, `filters: ["filter:seo/head", "filter:entry/view"]`.
  - `permissions`: `sqlite: ["kv_read", "kv_write"]`, `plugin_settings_read: true`, `plugin_secrets_read: ["webhook_token"]`, `network: [{ scheme: "https", host: "hooks.example.com", port: 443, methods: ["POST"], path_prefix: "/seo-ping" }]`.
  - `settings_schema`: `brand_name` (`text`), `default_badge` (`text`, default `"Mới"`), `webhook_token` (`secret`).
- **`src/lib.rs`:**
  - `pub fn handle(env: Envelope) -> Result<(), SdkError>` xử lý:
    - `"filter:seo/head"`: đọc `brand_name` từ `plugin_settings_get()`, sinh `SeoHeadOutput` kèm `json_ld` (`Article`/`Product` schema với `@context`, `@type`, `headline` dạng `Value::Object` thuần — việc escape `\u003c` khi render `<script type="application/ld+json">` do Host `content_render.rs` đảm nhiệm theo Spec 2).
    - `"filter:entry/view"`: đọc `default_badge` từ `plugin_settings_get()`, trả `EntryViewOutput` (`{"badge": ..., "seo_managed": true}`).
    - `"content/after_publish"`: tăng counter `kv_get`/`kv_set`, đọc `plugin_secret_get("webhook_token")` (nếu có thì gửi `http_request` kèm `Authorization: Bearer ...` — không bao giờ ghi token vào `log()`).
  - Gắn `opendash_plugin_sdk::export_plugin!(handle);` và bộ `#[cfg(test)] mod tests` kiểm thử cả 3 event bằng `run_in_mock_host`.
- **`README.md`:** Hướng dẫn `cargo test`, `cargo build --target wasm32-unknown-unknown --release`, đóng gói `manifest.json` + `plugin.wasm` thành `.zip`, và kiểm tra bằng `./cms/target/release/opendash --check-plugin`.

#### 3.2 `cms/examples/themes/sample-theme-v3/` (Theme v3 Hoàn Chỉnh Khớp `ThemeV3Manifest`)
- **`theme.json` *(Create)*:** Khớp chính xác `ThemeV3Manifest` (`opendash-core/src/theme.rs:350-452`, `#[serde(deny_unknown_fields)]`, `templates` dùng file `.html` ở root theo `valid_root_html`):
  ```json
  {
    "manifest_version": 3,
    "theme_id": "sample-theme-v3",
    "name": "Sample Theme v3",
    "version": "1.0.0",
    "templates": [{ "kind": "single", "file": "single.html" }],
    "assets": ["assets/app.css"]
  }
  ```
- **`assets/app.css` *(Create)*:** Stylesheet tối giản cho ví dụ `sample-theme-v3`.
- **`single.html` *(Create)*:** Template chi tiết đặt tại root của theme (`valid_root_html`), tuân thủ 100% C-SEC (Spec 1), dùng `{{ theme_assets["assets/app.css"] }}`, `{% for crumb in breadcrumbs %}`, `{{ entry.published_at | date_format("%d/%m/%Y") }}`, `{{ entry.reading_time_minutes }}`, truy cập `entry.ext` an toàn 2 bước dưới `UndefinedBehavior::Strict`:
  ```jinja
  {% set seo_ext = entry.ext["rust-seo-plugin"] | default({}) %}
  {% if seo_ext.badge | default("") %}
    <span class="badge">{{ seo_ext.badge | default("") }}</span>
  {% endif %}
  ```
  và `{% for rel in related_entries(entry.type_key, entry.id, 3) %}`. *(Ghi chú: Test render runtime cho `entry.ext` dưới `UndefinedBehavior::Strict` thuộc phạm vi `plugin_extensibility_test.rs` trong `opendash-server` ở Spec 2, giữ Spec 3 không đụng `opendash-server`.)*
- **`README.md` *(Create)*:** Hướng dẫn cấu trúc `sample-theme-v3`, chạy `./cms/target/release/opendash --check-theme cms/examples/themes/sample-theme-v3` và tích hợp `entry.ext.*` từ `rust-seo-plugin`. *(Thư mục `cms/examples/themes/sample-theme/` cũ giữ nguyên làm tài liệu v2 tham khảo.)*

---

## Alternatives Considered

| Điểm quyết định | Phương án đã bác bỏ | Lý do bác bỏ |
|---|---|---|
| Cách test Rust Guest SDK trong `cargo test --workspace` | Bắt buộc cài `wasm32-unknown-unknown` + chạy Wasmi cho mọi unit test SDK | Làm gãy CI/máy dev chưa cài target `wasm32-unknown-unknown` và làm chậm test suite; `MockHost` native chạy tức thì và vẫn test được cả `-2` readback. |
| Dependency cho `opendash-plugin-sdk` | Thêm `wit-bindgen`, `wasm-bindgen`, hoặc `thiserror` | Trái quy tắc hạn chế dependency mới và Wasm ABI của OpenDash là custom `"opendash_v1"` JSON, không dùng WIT/Component Model; dùng `serde` + `serde_json` đã pin sẵn ở workspace giữ binary nhỏ và zero new crates. |
| Đặt `rust-seo-plugin` vào `[workspace.members]` của `cms/Cargo.toml` | Gộp chung `rust-seo-plugin` vào workspace chính | `cdylib` Wasm example tách `[workspace]` riêng để tránh phát sinh artifact thư viện động ngoài ý muốn khi build `cms/`, đồng thời vẫn trỏ path cục bộ tới `../../../crates/opendash-plugin-sdk`. |

---

## Non-Functional Requirements

1. **Zero Migration & Zero Server Impact:** Spec 3 không thêm migration SQL nào (`0` net `sqlite_schema` objects) và không sửa code runtime của `opendash-server` / `opendash-core`.
2. **Zero New External Dependencies:** `opendash-plugin-sdk` chỉ dùng `serde` và `serde_json` từ `cms/Cargo.toml`; `cms/sdk/as/opendash.ts` chỉ dùng `assemblyscript-json` sẵn có.
3. **100% Native Testability:** `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` và `cargo test --manifest-path cms/examples/plugins/rust-seo-plugin/Cargo.toml --release` chạy pass 100% mà không cần `wasm32-unknown-unknown` hay network.
4. **Tương thích ngược AssemblyScript:** `cms/examples/plugins/sample-plugin` biên dịch và chạy không cần sửa code.

---

## Detailed Implementation Plan

### Slice S01: AssemblyScript SDK v1.1 (`cms/sdk/as/opendash.ts`, Test Harness & `cms/sdk/README.md`)
- **Dependencies:** Spec 2 ABI v1.1 (`host_last_resp_len`, `host_read_last_resp`) & 15 Host Ops.
- **File Ownership:**
  - `cms/sdk/as/opendash.ts` *(Modify)* (thêm 2 ABI v1.1 imports, auto `-2` readback trong `hostCall`, `Envelope` + `readEnvelope()`, 11 Host Ops mới + `contentSave`, `locale: string | null = null`, dual top-level/`data` extraction, và 7 output helpers dùng `JSON.Obj` + `stringifyValue`; giữ nguyên toàn bộ v1 exports)
  - `cms/sdk/as/test_harness.ts` *(Create)* (harness AssemblyScript gọi `readEnvelope()`, toàn bộ 15 Host Ops wrappers, và `-2` readback)
  - `cms/sdk/as/test_sdk_v11.mjs` *(Create)* (Node `WebAssembly.instantiate` runner giả lập `opendash_v1` kể cả `-2` readback để chạy test tự động không cần thêm npm dependency)
  - `cms/sdk/README.md` *(Modify)* (cập nhật tài liệu ABI v1.1, `-2` readback, `PluginEnvelopeV2`, 15 Host Ops và ví dụ filter)
- **Acceptance Criteria:**
  1. `cms/examples/plugins/sample-plugin` (`npm --prefix cms/examples/plugins/sample-plugin run asbuild`) biên dịch sạch ra `plugin.wasm` không cần sửa code v1.
  2. `test_sdk_v11.mjs` chạy trên `cms/target/opendash_sdk_test.wasm` xác minh `hostCall` xử lý `-2` readback, `readEnvelope()` đọc đúng cả v1 và v2, và đầy đủ wrapper 15 Host Ops.
- **Rollback:** Revert PR S01; không ảnh hưởng server hay DB.

### Slice S02: Rust Guest SDK (`cms/crates/opendash-plugin-sdk` + Workspace)
- **Dependencies:** Spec 2 ABI v1.1 & `HostOperationRequest`/`HostOperationResponse`.
- **File Ownership:**
  - `cms/Cargo.toml` *(Modify)* (thêm `"crates/opendash-plugin-sdk"` vào `[workspace.members]`)
  - `cms/Cargo.lock` *(Modify)* (cập nhật lockfile nội bộ cho workspace member mới — 0 crate ngoài mới)
  - `cms/crates/opendash-plugin-sdk/Cargo.toml` *(Create)* (`edition = "2024"`, chỉ phụ thuộc `serde` & `serde_json` workspace)
  - `cms/crates/opendash-plugin-sdk/src/abi.rs` *(Create)* (`unsafe extern "C"` imports `opendash_v1`, `read_input_bytes`, `write_output_bytes`, `host_call_raw` với `vec![0u8; INITIAL_RESP_BUF_CAP]` kèm `-2` readback, dispatch sang `MockHost` khi `not(target_arch = "wasm32")`)
  - `cms/crates/opendash-plugin-sdk/src/types.rs` *(Create)* (`SdkError` + `Display` + `std::error::Error`, `Envelope`, `HostResponse` dual v1/`data`, toàn bộ DTO input/output cho 15 Host Ops và 4 filter/hook)
  - `cms/crates/opendash-plugin-sdk/src/host.rs` *(Create)* (`read_envelope`, `write_output`, 15 typed Host Ops functions)
  - `cms/crates/opendash-plugin-sdk/src/mock.rs` *(Create)* (`#[cfg(not(target_arch = "wasm32"))]` `MockHost` hỗ trợ 15/15 Host Ops gồm `content_save` & `content_delete`, `MockHostOutcome`, `run_in_mock_host` với `initial_resp_buf_cap` test `-2` readback)
  - `cms/crates/opendash-plugin-sdk/src/lib.rs` *(Create)* (`export_plugin!` với `#[unsafe(no_mangle)]`, re-exports, unit tests toàn diện cho tất cả 15 Host Ops & `-2` readback)
- **Acceptance Criteria:**
  1. `cargo test --manifest-path cms/Cargo.toml -p opendash-plugin-sdk --release --locked` pass 100% native tests (gồm test `-2` readback, 15/15 Host Ops, legacy top-level `HostResponse` và Spec 2 `data` envelope).
  2. `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` và `check-standards.py` xanh 100%.
- **Rollback:** Revert PR S02; không ảnh hưởng `opendash-server`.

### Slice S03: Reference Examples (`rust-seo-plugin` + `sample-theme-v3`) & SITEMAP
- **Dependencies:** S02 (`opendash-plugin-sdk`), Spec 1 (`--check-theme` / helpers), Spec 2 (`--check-plugin` / `entry.ext.*`).
- **File Ownership:**
  - `.gitignore` *(Modify)* (ignore `/cms/examples/plugins/rust-seo-plugin/target/` và `/cms/examples/plugins/rust-seo-plugin/Cargo.lock`)
  - `cms/examples/plugins/rust-seo-plugin/Cargo.toml` *(Create)* (`[workspace]`, `crate-type = ["cdylib", "rlib"]`, `edition = "2024"`, `serde` với `features = ["derive"]`)
  - `cms/examples/plugins/rust-seo-plugin/manifest.json` *(Create)* (đủ trường bắt buộc của `PluginManifest`, `manifest_schema: 3`, `filters: ["filter:seo/head", "filter:entry/view"]`, `hooks: ["content/after_publish"]`, `settings_schema` có `secret`)
  - `cms/examples/plugins/rust-seo-plugin/src/lib.rs` *(Create)* (`handle` + `export_plugin!(handle)` + `MockHost` unit tests)
  - `cms/examples/plugins/rust-seo-plugin/README.md` *(Create)* (hướng dẫn test, build `wasm32-unknown-unknown`, đóng gói ZIP và `--check-plugin`)
  - `cms/examples/themes/sample-theme-v3/theme.json` *(Create)* (`manifest_version: 3`, `theme_id: "sample-theme-v3"`, `templates: [{ "kind": "single", "file": "single.html" }]`, `assets: ["assets/app.css"]`)
  - `cms/examples/themes/sample-theme-v3/assets/app.css` *(Create)* (stylesheet mẫu cho `sample-theme-v3`)
  - `cms/examples/themes/sample-theme-v3/single.html` *(Create)* (template gốc mẫu dùng `theme_assets["assets/app.css"]`, `date_format`, `breadcrumbs`, `related_entries`, `{% set seo_ext = entry.ext["rust-seo-plugin"] | default({}) %}`)
  - `cms/examples/themes/sample-theme-v3/README.md` *(Create)* (hướng dẫn `sample-theme-v3`, `--check-theme` và `entry.ext.*`)
  - `docs/designs/SITEMAP.md` *(Modify)* (cập nhật trạng thái Spec 3)
- **Acceptance Criteria:**
  1. `cargo test --manifest-path cms/examples/plugins/rust-seo-plugin/Cargo.toml --release` pass 100%.
  2. `./cms/target/release/opendash --check-theme cms/examples/themes/sample-theme-v3` trả `ok: true` (kiểm tra `ThemeV3Manifest`, asset và C-SEC cho `single.html`); nếu máy có `wasm32-unknown-unknown`, `cargo build --target wasm32-unknown-unknown --release` ra `plugin.wasm` và `./cms/target/release/opendash --check-plugin <zip-path>` trả `ok: true`.
- **Rollback:** Revert PR S03; chỉ là thư mục examples & docs.

### Target Files to Create/Modify
- `.gitignore` *(Modify)*
- `cms/Cargo.toml` *(Modify)*
- `cms/Cargo.lock` *(Modify)*
- `cms/crates/opendash-plugin-sdk/Cargo.toml` *(Create)*
- `cms/crates/opendash-plugin-sdk/src/abi.rs` *(Create)*
- `cms/crates/opendash-plugin-sdk/src/host.rs` *(Create)*
- `cms/crates/opendash-plugin-sdk/src/lib.rs` *(Create)*
- `cms/crates/opendash-plugin-sdk/src/mock.rs` *(Create)*
- `cms/crates/opendash-plugin-sdk/src/types.rs` *(Create)*
- `cms/sdk/as/opendash.ts` *(Modify)*
- `cms/sdk/as/test_harness.ts` *(Create)*
- `cms/sdk/as/test_sdk_v11.mjs` *(Create)*
- `cms/sdk/README.md` *(Modify)*
- `cms/examples/plugins/rust-seo-plugin/Cargo.toml` *(Create)*
- `cms/examples/plugins/rust-seo-plugin/README.md` *(Create)*
- `cms/examples/plugins/rust-seo-plugin/manifest.json` *(Create)*
- `cms/examples/plugins/rust-seo-plugin/src/lib.rs` *(Create)*
- `cms/examples/themes/sample-theme-v3/README.md` *(Create)*
- `cms/examples/themes/sample-theme-v3/assets/app.css` *(Create)*
- `cms/examples/themes/sample-theme-v3/single.html` *(Create)*
- `cms/examples/themes/sample-theme-v3/theme.json` *(Create)*
- `docs/designs/SITEMAP.md` *(Modify)*

### Verification Plan
Chạy tuần tự sau mỗi slice và trước khi đóng PR *(dùng `python` trên Windows cho Forge; lệnh đánh dấu `(S01)`/`(S02)`/`(S03)` chạy từ slice tương ứng)*:
```shell
cargo fmt --manifest-path cms/Cargo.toml --all --check
python3 cms/scripts/check-standards.py
python3 cms/scripts/check-standards.py --self-test
python3 cms/scripts/check-plans.py
python3 cms/scripts/check-plans.py --self-test
python3 .agents/scripts/check-agent-artifacts.py
npm --prefix cms/examples/plugins/sample-plugin run asbuild # (S01+)
npx --prefix cms/examples/plugins/sample-plugin asc cms/sdk/as/test_harness.ts --outFile cms/target/opendash_sdk_test.wasm --exportMemory --bindings raw # (S01+)
node cms/sdk/as/test_sdk_v11.mjs cms/target/opendash_sdk_test.wasm # (S01+)
cargo build --manifest-path cms/Cargo.toml --locked # (S02+)
cargo test --manifest-path cms/Cargo.toml -p opendash-plugin-sdk --release --locked # (S02+)
cargo test --manifest-path cms/Cargo.toml --workspace --release --locked # (S02+)
cargo test --manifest-path cms/examples/plugins/rust-seo-plugin/Cargo.toml --release # (S03+)
cargo build --manifest-path cms/Cargo.toml --release --locked # (S03+)
./cms/target/release/opendash --check-theme cms/examples/themes/sample-theme-v3 # (S03+)
git diff --check
```
Kiểm chứng tùy chọn khi có target `wasm32-unknown-unknown` *(S03+)*: build `rust-seo-plugin` với `--target wasm32-unknown-unknown --release`, đóng gói `manifest.json` + `plugin.wasm` thành `/tmp/rust-seo-plugin.zip` và chạy `./cms/target/release/opendash --check-plugin /tmp/rust-seo-plugin.zip`.
