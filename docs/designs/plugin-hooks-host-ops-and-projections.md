# Đặc tả Thiết kế EGM — Spec 2: Plugin Hooks, Host Ops v1.1, Owner Settings (`SecretStore`) & Write-Time Projections

- **Status:** Verified
- **Approved By:** Owner
- **Approval Evidence:** Owner chat decisions 1A-4A & directive 2026-11-24 (Issue từng Slice cho Nova)
- **Goldfish Review:** 3-round zero-context Goldfish review (egm-design, egm-codebase-details, egm-implementation-readiness) · 2026-11-24 · 3/3 APPROVED
- **Implementation & Closure Evidence:** S01 `#524` / PR `#536` (`e48a65d`), S02 `#525` / PR `#540` (`e6e1934`), S03 `#526` / PR `#542` (`c3dd2e4`), S04 `#527` / PR `#543` (`65dd049`), drift report & hardening closure: [2026-11-24 drift report](../../.agents/research/2026-11-24_SPEC2_PLUGIN_HOOKS_DRIFT_REPORT.md) (`0 Blocker · 0 Major · 2 Minor · 2 Nit` sau hardening) và [2026-11-24 closure](../verification/2026-11-24-spec2-plugin-hooks-closure.md).
- **Date:** 2026-11-24
- **Author:** Orion (Elephant)
- **Thuộc Lộ trình:** Giai đoạn 2 & Giai đoạn 3 (Tinh gọn) trong Tầm nhìn 6 Giai đoạn (`concepts_brief.md`: Decisions `2A`, `3A`, `4A`)
- **Thành phần chạm tới:** `cms/crates/opendash-core/src/{hooks,plugin}.rs`, `cms/migrations/0050_plugin_extensibility_and_projections.sql`, `cms/migrations/README.md`, `cms/crates/opendash-server/src/{app,contact,content_render,hooks,main,operations,paths,secrets,seo}.rs`, `cms/crates/opendash-server/src/plugin/{host_ops,manager,package,review_regressions,runtime,tests,worker}.rs`, `cms/crates/opendash-server/src/storage/{mod,media_refs,overrides,plugin,plugin_kv,plugin_projections,seo,settings}.rs`, `cms/crates/opendash-server/src/routes/{contact,entries,plugin,public_content}.rs`, `cms/crates/opendash-server/tests/{content_foundation_m6_s11_t01.rs,m6_s04_t01_egress_ssrf.rs,plugin_extensibility_test.rs}`, `cms/admin/src/types.ts`, `cms/admin/src/api/plugins.ts`, `cms/admin/src/features/shared/OwnerOptionsForm.tsx`, `cms/admin/src/features/themes/ThemeOptionsForm.tsx`, `cms/admin/src/features/plugins/PluginsPanel.tsx`, `docs/contracts/PLUGIN_ABI.md`, `docs/designs/SITEMAP.md`

---

## Overview

Đặc tả này nâng cấp nền tảng **Plugin Runtime, Host API v1.1, Owner Settings (`SecretStore`), Lifecycle Hooks & Typed Pipelines** trên OpenDash CMS mà **không phá vỡ bất kỳ plugin v1/v2 hiện hữu nào**, giữ nguyên mô hình cô lập tiến trình Wasmi (`=2.0.0`), zero-Wasm trên đường nóng public `GET` (`2A`), và kiểm soát chặt trần `MAX_SCHEMA_OBJECTS = 256` (+2 net objects).

Bốn trụ cột kỹ thuật của Spec 2:
1. **Nâng cấp Hợp đồng Guest-Host (`manifest_schema: 3`, `EnvelopeV2`, ABI v1.1 & IPC Framing):** Mở rộng `HookEvent`/`HookExecutionMode`/`ExecutionPrincipal` (`hooks.rs`), phát hành `PluginEnvelopeV2` (giữ `HookEnvelope` cho plugin `<= 2`), bổ sung cặp import ABI v1.1 `host_last_resp_len` / `host_read_last_resp` xử lý phản hồi vượt buffer (`-2`) không chạy lại `host_call`, thay giao thức stdout line-based bằng binary `u32 LE` length-prefixed JSON frames (`2 MiB`), và sửa các lỗi tồn đọng (`plugin/manager.rs:228` kiểm `network`, `hooks.rs:242,389` truyền `actor_role` thật, `host_ops.rs:175` chặn `/api/p/` loopback).
2. **`HostPolicy` & Bộ 11 Host Ops v1.1 + `StagedCommitGuard` Một Giao Dịch:** Cấp `kv_delete`, `kv_list`, `content_get`, `content_list`, `content_delete`, `taxonomy_list`, `media_get`, `site_options_get`, `plugin_settings_get`, `plugin_secret_get`, `log`; lưu tombstone `Option<Value>` trong `HostContext` và commit nguyên tử mọi `staged_kv` + `staged_logs` + `staged_content_*` trong một transaction `BEGIN IMMEDIATE` duy nhất; gắn cache manifest trong `PluginManager` (`O(1)` discovery) và trần tổng worker `MAX_TOTAL_CONCURRENT_WORKERS = 6`.
3. **Owner Settings Đa Chủ Thể + Write-Only `SecretStore` (`0050`):** Sửa khóa chính `theme_option_values` / `theme_option_versions` / `theme_option_media_refs` theo `(owner_kind, owner_id, ...)` (+0 net objects), triển khai trường `secret` lưu qua `secrets::SecretStore` tại `database/secrets/plugins/<plugin_id>/` (`0700`/`0600`), mở `GET`/`PUT /api/admin/plugins/{id}/settings` (CAS `If-Match`), và tách component React dùng chung `<OwnerOptionsForm>`.
4. **`AsyncHookQueue`, Typed Pipelines & Write-Time Materialized Projections:** Bổ sung `AsyncHookQueue` (2 workers, queue `64`, giữ `MutationLease`, hủy tức thì qua `SiteCoordinator::maintenance_cancel_rx()`), pipeline `filter:content/prepare_save` (sparse patch + per-key ACL + full re-validation), `filter:contact/before_submit` (public semaphore riêng 2 slots, chạy trước `MutationLease`/DB), và bảng `plugin_entry_projections` (+2 objects) tính sẵn `filter:seo/head` (`json_ld` escape Unicode chống `</script>`) + `filter:entry/view` (`entry.ext.<plugin_id>.*`) lúc ghi/xuất bản/đổi settings, cùng lệnh offline `opendash --check-plugin <dir|zip>`.

---

## The Problem

Qua kiểm tra mã nguồn `opendash-core/src/{plugin,hooks}.rs`, `plugin/{runtime,worker,host_ops,manager,package}.rs`, `hooks.rs`, `storage/settings.rs` và `migrations/0017`–`0048`:
1. **Plugin thiếu API đọc/xóa dữ liệu & tràn buffer `-2` không đọc lại được:** `host_ops.rs` chỉ có 4 op (`kv_get`, `kv_set`, `http_request`, `content_save`); plugin không thể liệt kê/xóa KV, đọc CPT/entry/taxonomy/media/site options/settings, xóa bài hay ghi log; khi `host_call` trả `-2` (`BUFFER_TOO_SMALL`), guest không có import đọc lại payload đã tính mà phải gọi lại (tốn quota hoặc kích hoạt lại `http_request`).
2. **IPC Worker dùng `println!` + `read_line` & `StagedCommitGuard` chỉ xử lý KV set:** `worker.rs:99` dễ vỡ khi dữ liệu có newline hoặc payload lớn; `runtime.rs` chưa commit hợp nhất `kv_delete`, `plugin_logs` và `content_delete` trong một transaction; `PluginManager` query DB lặp lại để lọc manifest khi dispatch hook; `plugin/manager.rs:228` chỉ kiểm `!manifest.permissions.http.is_empty()` nên từ chối capability `"http"` của plugin dùng `network`.
3. **Hook `content/before_save` / `after_save` hardcode `FieldRole::Admin` & thiếu hook/filter thực tế:** `hooks.rs:242, 389` luôn truyền `FieldRole::Admin` làm hổng mô hình RBAC khi Editor lưu bài; chưa có `content/after_delete`, `content/after_publish`, `content/after_unpublish`, `contact/after_submit`, `plugin/after_enable`, `plugin/after_disable`, `settings/after_save`, hay các typed filter `prepare_save`, `contact/before_submit`, `seo/head`, `entry/view`.
4. **Đụng độ PK bảng Settings khi dùng cho Plugin & thiếu Secret storage:** `0017_theme_options.sql` đặt `PRIMARY KEY (theme_id, option_key)` và `0024_owner_neutral_domain_bridge.sql` chỉ thêm cột `owner_kind`/`owner_id` mà **chưa sửa PK**, dẫn tới `ON CONFLICT(theme_id, option_key)` (`settings.rs:2877`) ghi đè chéo nếu plugin trùng ID với theme; chưa có endpoint Admin Settings cho plugin và chưa có trường `secret` mã hóa ngoài SQLite.
5. **Public SSR không chịu nổi chi phí spawn Wasm trên mỗi request & chưa có validator offline cho plugin:** Chạy Wasmi trên mỗi public `GET` sẽ cạn 4 permit concurrency và phá `Cache-Control`; chưa có công cụ `--check-plugin` kiểm tra thư mục/ZIP plugin trước khi upload.

---

## Goals

1. Mở rộng `manifest_schema: 3` (tương thích ngược 100% với `1` và `2`), `HookEvent`, `HookExecutionMode`, `ExecutionPrincipal` và `PluginEnvelopeV2` (gồm seam `Route`/`Cron` cho Giai đoạn 5–6).
2. Cung cấp ABI v1.1 (`host_last_resp_len`, `host_read_last_resp`), binary length-prefixed Worker IPC (`u32 LE`, cap `2 MiB`), `HostPolicy`, đủ 11 Host Ops mới, `StagedCommitGuard` gom tombstone KV + log + content atomically, manifest cache trong `PluginManager`, và giới hạn tổng `<= 6` workers.
3. Sửa PK `theme_option_*` theo `(owner_kind, owner_id, ...)` trong migration `0050`, hỗ trợ trường `secret` write-only qua `SecretStore`, cung cấp `GET`/`PUT /api/admin/plugins/{id}/settings` (CAS `If-Match`) và `<OwnerOptionsForm>` dùng chung trên React Admin.
4. Triển khai `AsyncHookQueue` có biên gắn `MutationLease` + hủy nhanh khi maintenance, pipeline `filter:content/prepare_save`, `filter:contact/before_submit` (chạy trước `MutationLease`), và `plugin_entry_projections` cho `filter:seo/head` (+ an toàn `json_ld`) & `filter:entry/view` (`entry.ext.*`) đạt **0 Wasm spawn trên public `GET`**.
5. Bổ sung `opendash --check-plugin <dir|zip>` chạy offline trước `SiteCoordinator::acquire` / DB init với bảo vệ symlink/FIFO/case-collision/độ sâu `<= 4`.

## Non-Goals

- Không mở Public Plugin Routes (`/api/p/<plugin>/<route>`), Visitor Session, Scoped Plugin Collections (`plugin_collection_records`), SMTP `mail_send` host op, hay Cron Jobs trong Spec 2 (chỉ khóa seam tại `EnvelopeV2`, `host_ops.rs:175` và `manifest_schema: 3`; triển khai ở Giai đoạn 5–6 sau V1).
- Không sửa SDK AssemblyScript / Rust Guest SDK (`cms/sdk/`) trong Spec 2 (thuộc Spec 3).
- Không cho phép plugin tự chạy `CREATE TABLE` SQL động hay đọc trực tiếp bảng SQLite ngoài `HostOps`.
- Không cho phép `content_save` hay `content_delete` bên trong bất kỳ lifecycle hook hay filter nào (`Standard` action mode duy nhất).

---

## Critical User Journeys (CUJs)

1. **CUJ-1 — Plugin lưu API Secret & tự động đẩy Webhook bất đồng bộ sau khi xuất bản bài:**
   - *Given* admin cài plugin `crm-sync` (`manifest_schema: 3`, `settings_schema` có trường `api_key` kiểu `secret`, quyền `network` tới `https://crm.example.com`, hook `content/after_publish`, `plugin_secrets_read: ["api_key"]`) và nhập API Key tại Admin UI.
   - *When* admin lưu settings (`PUT /api/admin/plugins/crm-sync/settings`) rồi xuất bản một bài viết mới.
   - *Then* API Key chỉ ghi vào `database/secrets/plugins/crm-sync/<ref>.json` (`0600`), `GET` settings chỉ thấy `{"configured": true}`, `after_publish` chạy trên `AsyncHookQueue` đọc secret qua `plugin_secret_get` và gửi `http_request` không chặn HTTP response xuất bản bài.
2. **CUJ-2 — Chuẩn hóa dữ liệu trước khi lưu & chặn spam liên hệ trước khi chạm DB:**
   - *Given* plugin bật `filter:content/prepare_save` (tự sinh field `sku_normalized`) và plugin anti-spam bật `filter:contact/before_submit` (tối đa `1` HTTP call, `0` DB/KV writes).
   - *When* Editor lưu sản phẩm hoặc khách gửi form liên hệ public `POST /api/public/contact/{id}` (`app.rs:179`).
   - *Then* `prepare_save` trả sparse patch, Host kiểm tra `plugin_can_write_field` theo `actor_role` thật của Editor và chạy full re-validation trước `content/before_save`; `contact/before_submit` chạy **sau** Honeypot/Origin/FormSchema và **trước** `MutationLease`/DB insert, trả `422` nếu bị reject mà không để lại rác trong DB.
3. **CUJ-3 — SEO JSON-LD & Dữ liệu mở rộng `entry.ext.*` trên SSR Public (`0` Wasm spawn):**
   - *Given* plugin SEO/E-commerce đăng ký `filter:seo/head` (trả `json_ld` Product schema) và `filter:entry/view` (trả `{"price_formatted": "250.000 đ", "in_stock": true}`).
   - *When* bài viết được xuất bản và khách truy cập trang bài viết hoặc Archive qua Theme v3 (`{{ entry.ext["my-shop"].price_formatted }}`).
   - *Then* projection được tính sẵn lúc xuất bản vào `plugin_entry_projections`; request `GET` công khai chỉ chạy 1 `SELECT` batch, JSON-LD được escape `\u003c` an toàn bên trong `<script type="application/ld+json">`, và plugin không bao giờ gỡ được `noindex: true` trên trang 404/preview.
4. **CUJ-4 — Kiểm tra gói Plugin offline trước khi đóng gói bằng CLI:**
   - *Given* developer có thư mục `./my-plugin` hoặc `./my-plugin.zip` (`manifest_schema: 3`).
   - *When* chạy `./opendash --check-plugin ./my-plugin`.
   - *Then* tiến trình kiểm `manifest.json`, biên dịch thử `plugin.wasm` bằng `wasmi::Module::new`, kiểm C-SEC toàn bộ `blocks/<slug>/render.html`, in 1 dòng JSON chuẩn lên `stdout` với exit `0` (hoặc `{ "ok": false, "error": "..." }` exit `1`) mà không chạm DB/locks.

---

## Technical Plan (Architecture)

### 1. `manifest_schema: 3`, `HookEvent`, `ExecutionPrincipal` & `PluginEnvelopeV2` (`opendash-core/src/{hooks,plugin}.rs`, `hooks.rs`, `plugin/manager.rs`, `routes/entries.rs`)

#### 1.1 Mở rộng `opendash-core/src/hooks.rs`
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HookEvent {
    #[serde(rename = "page/after_save")]
    PageAfterSave,
    #[serde(rename = "content/before_save")]
    ContentBeforeSave,
    #[serde(rename = "content/after_save")]
    ContentAfterSave,
    #[serde(rename = "plugin/manual_run")]
    PluginManualRun,
    // Spec 2 — Lifecycle post-commit events (async)
    #[serde(rename = "content/after_delete")]
    ContentAfterDelete,
    #[serde(rename = "content/after_publish")]
    ContentAfterPublish,
    #[serde(rename = "content/after_unpublish")]
    ContentAfterUnpublish,
    #[serde(rename = "contact/after_submit")]
    ContactAfterSubmit,
    #[serde(rename = "plugin/after_enable")]
    PluginAfterEnable,
    #[serde(rename = "plugin/after_disable")]
    PluginAfterDisable,
    #[serde(rename = "settings/after_save")]
    SettingsAfterSave,
    // Spec 2 — Typed synchronous filters
    #[serde(rename = "filter:content/prepare_save")]
    FilterContentPrepareSave,
    #[serde(rename = "filter:contact/before_submit")]
    FilterContactBeforeSubmit,
    #[serde(rename = "filter:seo/head")]
    FilterSeoHead,
    #[serde(rename = "filter:entry/view")]
    FilterEntryView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HookExecutionMode {
    #[default]
    Standard,
    ContentBeforeSave,
    ContentAfterSave,
    AsyncPostCommit,
    FilterContentPrepareSave,
    FilterContactBeforeSubmit,
    FilterProjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPrincipal {
    Admin,
    Editor,
    Public,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeKind {
    Hook,
    Filter,
    Action,
    Manual,
    // Reserved Stage 5-6 seams (rejected in V1 dispatchers)
    Route,
    Cron,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginEnvelopeV2 {
    pub envelope_version: u32, // luôn = 2
    pub kind: EnvelopeKind,
    pub event: String,
    pub occurred_at_ms: u64,
    pub correlation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    pub principal: ExecutionPrincipal,
    pub dispatch_depth: u8,
    pub payload: Value,
}
```
- **Tương thích ngược Envelope:** Plugin có `manifest_schema <= 2` tiếp tục nhận `HookEnvelope` cũ cho 4 sự kiện gốc (`page/after_save`, `content/before_save`, `content/after_save`, `plugin/manual_run`); plugin `manifest_schema >= 3` nhận `PluginEnvelopeV2`.
- Thêm `HookEvent::is_filter(self) -> bool` và `HookEvent::is_async_hook(self) -> bool` trong `opendash-core/src/hooks.rs`.

#### 1.2 Mở rộng `PluginManifest` (`opendash-core/src/plugin.rs`) & Sửa Bug Tồn Đọng
- **Kiểu mới cho Settings (`opendash-core/src/plugin.rs`):**
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum PluginSettingFieldType {
      Text,
      Textarea,
      Number,
      Boolean,
      Select,
      Url,
      Media,
      Secret, // Write-only, lưu ngoài SQLite qua secrets::SecretStore
  }

  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
  #[serde(deny_unknown_fields)]
  pub struct PluginSettingChoice {
      pub value: String,
      pub label: String,
  }

  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
  #[serde(deny_unknown_fields)]
  pub struct PluginSettingField {
      pub key: String, // valid_snake_key
      pub label: String,
      #[serde(rename = "type")]
      pub field_type: PluginSettingFieldType,
      #[serde(default)]
      pub required: bool,
      #[serde(default)]
      pub default: serde_json::Value,
      #[serde(default)]
      pub min: Option<f64>,
      #[serde(default)]
      pub max: Option<f64>,
      #[serde(default)]
      pub choices: Vec<PluginSettingChoice>,
  }
  ```
  *(Cấm `Spacing`, `Richtext`, `css_var`, `responsive` trong `settings_schema`; giới hạn tối đa `64` field mỗi plugin; `Secret` không được đặt giá trị `default` plaintext khác `null`.)*
- **Mở rộng `PluginPermissions` & `PluginManifest`:**
  - `PluginPermissions` thêm (với `#[serde(default)]`): `taxonomy_read: bool`, `media_read: bool`, `site_options_read: bool`, `plugin_settings_read: bool`, `plugin_secrets_read: Vec<String>` (chỉ key snake_case tồn tại trong `settings_schema` kiểu `Secret`, cấm `"*"`), `content_delete: Vec<String>`.
  - `PluginManifest` thêm (với `#[serde(default, skip_serializing_if = "Vec::is_empty")]`): `filters: Vec<crate::hooks::HookEvent>` và `settings_schema: Vec<PluginSettingField>`.
  - Trong `PluginManifest::from_json_str` & `validate`:
    - Chấp nhận `manifest_schema` ∈ `{1, 2, 3}` (đổi toàn bộ kiểm tra `!= 2` tại `L211, L258, L261, L269` sang `< 2`).
    - Yêu cầu `manifest_schema >= 3 && min_host_version.is_some()` khi dùng bất kỳ tính năng mới nào (`filters`, `settings_schema`, các quyền v3 mới, hoặc `hooks` ngoài 3 hook cũ).
    - Kiểm tra `hooks` chỉ nhận lifecycle hooks (không chứa `Filter*` hoặc `PluginManualRun`, không trùng lặp); `filters` chỉ nhận `Filter*` (không trùng lặp).
    - Cập nhật `action.capabilities` allowlist (cả `plugin.rs:397-430` lẫn `plugin/manager.rs:220-231`): hỗ trợ thêm `"content_delete"`, `"taxonomy_read"`, `"media_read"`, `"site_options_read"`, `"plugin_settings_read"`, `"plugin_secrets_read"`, `"kv_list"`, `"kv_delete"`, `"log"`; đồng thời cập nhật `PluginManifest::validate` (`plugin.rs:403-415`) để gom `"kv_delete"` vào cùng cờ `has_kv_write` và từ chối sớm nếu 1 action khai báo đồng thời `content_delete` cùng `kv_write`/`kv_delete` hoặc `content_write`.
- **Sửa 3 lỗi tồn đọng & Đồng bộ Chữ ký (`opendash-core/src/plugin.rs`, `plugin/manager.rs`, tests):**
  1. `plugin/manager.rs:228`: `"http" => !manifest.permissions.http.is_empty() || !manifest.permissions.network.is_empty()`.
  2. `hooks.rs:242, 389` & `routes/entries.rs:423, 438, 491, 509, 680, 689, 740, 749`: truyền `actor_role` thật từ `actor_field_role(&actor)` (thay vì luôn `FieldRole::Admin` khi gọi từ `routes/entries.rs`).
  3. `plugin/host_ops.rs:175`: bổ sung `|| path.starts_with("/api/p/")` vào danh sách chặn loopback (seam Giai đoạn 5).
  4. `plugin_can_write_field` (`opendash-core/src/plugin.rs:635-658`): đổi tham số `action_caps: &[String]` sang `action_caps: Option<&[String]>` (đồng bộ với `plugin_can_read_field` tại `L605-611`), và cập nhật caller tại `plugin/manager.rs:336` sang `Some(&action_decl.capabilities)`.
  5. Cập nhật toàn bộ struct literal `PluginManifest { ... }` không dùng `..Default::default()` trong `plugin/review_regressions.rs`, `plugin/tests.rs` và `tests/m6_s04_t01_egress_ssrf.rs` để điền `filters: Vec::new()` và `settings_schema: Vec::new()`.

---

### 2. ABI v1.1 (`-2` Readback), Length-Prefixed IPC, `HostPolicy`, Host Ops v1.1 & Single-Tx `StagedCommitGuard`

#### 2.1 ABI v1.1 Imports (`plugin/runtime.rs:902`, `plugin/package.rs:187-252`)
Giữ nguyên 4 import gốc (`host_get_input_len`, `host_read_input`, `host_write_output`, `host_call`) và bổ sung 2 import trong module `"opendash_v1"`:
1. **`host_last_resp_len() -> i32`**: trả về `ctx.last_resp_bytes.as_ref().map(|b| b.len() as i32).unwrap_or(0)` (kiểm cancellation + fuel trước khi trả).
2. **`host_read_last_resp(dest_ptr: i32, max_len: i32) -> i32`**: sao chép `ctx.last_resp_bytes` vào guest linear memory (kiểm tràn con trỏ + bounds như `host_read_input`), trả số byte đã copy (hoặc `-1` nếu lỗi con trỏ/không có dữ liệu); chỉ xóa `last_resp_bytes` khi đọc hết toàn bộ hoặc khi gọi `host_call` kế tiếp.
- Trong `host_call`: khi `resp_json.len() > resp_max`, lưu `ctx.last_resp_bytes = Some(resp_json)` (giới hạn tổng `<= MAX_IPC_FRAME_BYTES` `2 MiB`) và trả **`-2`** mà không mất dữ liệu và không phải gọi lại op.
- **Validator Allowlist (`plugin/package.rs:187-252`):** Cập nhật `module.imports()` allowlist để chấp nhận thêm `("opendash_v1", "host_last_resp_len")` (signature `() -> i32`) và `("opendash_v1", "host_read_last_resp")` (signature `(i32, i32) -> i32`); vẫn từ chối mọi import lạ khác.

#### 2.2 Binary Length-Prefixed Worker IPC & Exact Host Ops v1.1 Schemas (`plugin/worker.rs`, `plugin/runtime.rs`, `plugin/host_ops.rs`)
Thay thế `println!("__OPENDASH_IPC__:...")` + `stdin.read_line` bằng khung nhị phân **4-byte `u32 LE` độ dài + UTF-8 JSON bytes** (cứng `MAX_IPC_FRAME_BYTES = 2 * 1024 * 1024`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HostReadRequest {
    KvGet { key: String },
    KvList { #[serde(default)] prefix: Option<String>, #[serde(default)] limit: Option<usize> },
    ContentGet { type_key: String, entry_id: i64, #[serde(default)] locale: Option<String> },
    ContentList {
        type_key: String,
        #[serde(default)] locale: Option<String>,
        #[serde(default)] limit: Option<usize>,
        #[serde(default)] offset: Option<usize>,
        #[serde(default)] published_only: Option<bool>,
    },
    TaxonomyList { kind: String, #[serde(default)] locale: Option<String>, #[serde(default)] limit: Option<usize> },
    MediaGet { media_id: String },
    SiteOptionsGet { #[serde(default)] locale: Option<String> },
    PluginSettingsGet,
    PluginSecretGet { key: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HostOperationRequest {
    KvGet { key: String },
    KvSet { key: String, value: serde_json::Value },
    KvDelete { key: String },
    KvList { #[serde(default)] prefix: Option<String>, #[serde(default)] limit: Option<usize> },
    HttpRequest {
        method: String,
        url: String,
        #[serde(default)] headers: BTreeMap<String, String>,
        #[serde(default)] body: Option<String>,
        #[serde(default)] timeout_ms: Option<u64>,
    },
    ContentSave {
        type_key: String,
        #[serde(default)] entry_id: Option<i64>,
        #[serde(default)] expected_version: Option<i64>,
        #[serde(default)] title: Option<String>,
        #[serde(default)] slug: Option<String>,
        #[serde(default)] status: Option<String>,
        #[serde(default)] values: serde_json::Map<String, serde_json::Value>,
        #[serde(default)] term_ids: Option<Vec<i64>>,
    },
    ContentDelete { type_key: String, entry_id: i64, expected_version: i64 },
    ContentGet { type_key: String, entry_id: i64, #[serde(default)] locale: Option<String> },
    ContentList {
        type_key: String,
        #[serde(default)] locale: Option<String>,
        #[serde(default)] limit: Option<usize>,
        #[serde(default)] offset: Option<usize>,
        #[serde(default)] published_only: Option<bool>,
    },
    TaxonomyList { kind: String, #[serde(default)] locale: Option<String>, #[serde(default)] limit: Option<usize> },
    MediaGet { media_id: String },
    SiteOptionsGet { #[serde(default)] locale: Option<String> },
    PluginSettingsGet,
    PluginSecretGet { key: String },
    Log { level: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostOperationResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum WorkerToHostFrame {
    ReadRequest(HostReadRequest),
    Result(WorkerExecutionResult),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum HostToWorkerFrame {
    ReadResponse(HostOperationResponse),
}
```
- **JSON `data` của `HostOperationResponse` theo từng op mới:**
  - `kv_delete`: `{"deleted": true}`
  - `kv_list`: `{"items": [{"key": String, "value": Value}]}`
  - `content_delete`: `{"staged": true, "entry_id": i64}`
  - `content_get`: `{"entry": {"id": i64, "type_key": String, "slug": String, "title": String, "status": String, "version": i64, "locale": String, "values": Object, "term_ids": [i64]} | null}`
  - `content_list`: `{"items": [<entry>], "total": u64}`
  - `taxonomy_list`: `{"items": [{"id": i64, "kind": String, "slug": String, "name": String, "parent_id": i64 | null}]}`
  - `media_get`: `{"media": {"id": String, "filename": String, "mime_type": String, "url": String, "width": i64 | null, "height": i64 | null, "alt": String | null} | null}`
  - `site_options_get`: `{"options": Object}` (chỉ gồm các option `public = true`)
  - `plugin_settings_get`: `{"settings": Object}` (trường `Secret` trả `{"configured": bool}`)
  - `plugin_secret_get`: `{"value": String | null}`
  - `log`: `{"staged": true}`
- Bổ sung `pub fn read_ipc_frame<R: Read, T: DeserializeOwned>(reader: &mut R, max_bytes: usize) -> Result<T, String>` và `pub fn write_ipc_frame<W: Write, T: Serialize>(writer: &mut W, value: &T, max_bytes: usize) -> Result<(), String>` dùng chung ở cả `worker.rs` và `runtime.rs` (loại bỏ hoàn toàn lỗi vỡ JSON khi value chứa `\n` và hỗ trợ `HostReadRequest` tổng quát cho mọi read op trong Worker).

#### 2.3 `HostPolicy` & Ma trận 15 Host Operations (`plugin/host_ops.rs`)

| Host Op (`op`) | Quyền Manifest Bắt buộc | Mode Cho phép (`HookExecutionMode`) | Giới hạn & Ghi chú |
|---|---|---|---|
| `kv_get` | `sqlite:kv_read` | Tất cả | Đọc từ `HostContext.staged_kv` (kể cả tombstone `None`!) trước khi gửi `ReadRequest` về Host. |
| `kv_set` | `sqlite:kv_write` | `Standard`, `AsyncPostCommit` | Key `<= 256 B`, value `<= 512 KiB`; tổng `staged_kv` `<= 64` ops / `2 MiB`. |
| `kv_delete` | `sqlite:kv_write` | `Standard`, `AsyncPostCommit` | Ghi `staged_kv.insert(key, None)` (tombstone). |
| `kv_list` | `sqlite:kv_read` | `Standard`, `AsyncPostCommit` | `{ prefix?: String, limit?: usize }` (`limit` clamp `1..=100`, mặc định `50`); hợp nhất `DB` + `staged_kv` (ẩn tombstone `None`, chèn staged `Some`). |
| `http_request` | `http` hoặc `network` | `Standard`, `AsyncPostCommit`, `FilterContactBeforeSubmit` *(max 1)* | SSRF DNS-pinning qua `egress.rs`, stream `<= 1 MiB`, timeout `<= 2s`, chặn `/api/admin`, `/save`, `/preview`, `/theme`, `/api/p/`. |
| `content_save` | `content_write` (+ private) | **`Standard` (Action) duy nhất** | **Cấm tuyệt đối** trộn `kv_*` writes hoặc `content_delete` trong cùng invocation (trả lỗi `ERR_MIXED_WRITE_SCOPE`); tối đa 1 `staged_content_command`. |
| `content_delete` | `content_delete` | **`Standard` (Action) duy nhất** | `{ type_key, entry_id, expected_version }`; stage `StagedContentDeleteCommand` vào `manager.rs` (commit transactional kèm `verify_plugin_action_fence` + CAS `expected_version`, **cấm** trộn cùng `kv_*` writes hoặc `content_save`). |
| `content_get` | `content_read` (+ private) | Tất cả *(trừ `FilterContactBeforeSubmit`)* | `{ type_key, entry_id, locale? }`; khi `principal == Public` hoặc `mode == FilterProjection` bắt buộc lọc `publication_status = 'published'`, `provider_status = 'active'` và `settings.public = true`; lọc từng field qua `plugin_can_read_field` theo `principal`/`actor_role`. |
| `content_list` | `content_read` (+ private) | Tất cả *(trừ `FilterContactBeforeSubmit`)* | `{ type_key, locale?, limit?, offset?, published_only? }`; `limit` clamp `1..=50` (mặc định `20`), khi `principal == Public` hoặc `mode == FilterProjection` bắt buộc `published_only = true` + `provider_status = 'active'` + `settings.public = true`; lọc từng field qua `plugin_can_read_field`. |
| `taxonomy_list` | `taxonomy_read` | Tất cả *(trừ `FilterContactBeforeSubmit`)* | `{ kind, locale?, limit? }`; `limit` clamp `1..=100`, chỉ trả taxonomy `public = 1` (trừ `Admin` principal). |
| `media_get` | `media_read` | Tất cả *(trừ `FilterContactBeforeSubmit`)* | `{ media_id }`; trả metadata cơ bản + URL public/admin tương ứng `principal`. |
| `site_options_get` | `site_options_read` | Tất cả *(trừ `FilterContactBeforeSubmit`)* | `{ locale? }`; **chỉ trả các option `public: true`**, không bao giờ rò rỉ option private hay secret. |
| `plugin_settings_get` | `plugin_settings_read` | Tất cả *(trừ `FilterContactBeforeSubmit`)* | Trả các setting không phải `Secret` của chính plugin hiện tại (`Secret` trả `{"configured": bool}`). |
| `plugin_secret_get` | `plugin_secrets_read` chứa `key` | `Standard`, `AsyncPostCommit`, `FilterContactBeforeSubmit` | Đọc `SecretRef` từ `theme_option_values` rồi giải mã từ `SecretStore::read_plugin_secret`; **cấm** trong `FilterProjection`/`ContentBeforeSave` và cấm ghi vào log. |
| `log` | *(mọi plugin)* | Tất cả *(trừ `FilterContactBeforeSubmit`)* | `{ level: "info"\|"warn"\|"error", message }`; max `20` entries/invocation, `sanitize_plain_text(msg, 512)`, stage commit vào `plugin_logs`. |

- **`HostPolicy` & ReadRequest Isolation:**
  - Mọi op đọc DB từ Worker (`kv_get`, `kv_list`, `content_get`, `content_list`, `taxonomy_list`, `media_get`, `site_options_get`, `plugin_settings_get`, `plugin_secret_get`) gửi `WorkerToHostFrame::ReadRequest` qua pipe về tiến trình Host chính (Worker không giữ `SqlitePool`).
  - Host thực thi query với `tokio::time::timeout` giới hạn bởi `remaining_deadline`, áp dụng `HostPolicy` (kiểm tra `manifest` permissions, `execution_mode`, `action_capabilities`, và `principal`/`actor_role`) và trả `HostOperationResponse`.

#### 2.4 Single-Transaction `StagedCommitGuard`, Manifest Cache, `secrets_root` Wiring & Worker Ceiling (`plugin/{runtime,manager}.rs`, `storage/plugin_kv.rs`)
- **`storage/plugin_kv.rs`:** Bổ sung `delete(conn: &mut SqliteConnection, plugin_id: &str, key: &str)` và `list(db: &SqlitePool, plugin_id: &str, prefix: &str, limit: i64) -> Result<Vec<(String, String)>, sqlx::Error>`.
- **`StagedCommitGuard` (`plugin/runtime.rs:537`):** Cập nhật guard tại `runtime.rs:537` từ `execution_mode != HookExecutionMode::Standard` sang cho phép `HookExecutionMode::Standard | HookExecutionMode::AsyncPostCommit` đối với `staged_kv` và `staged_logs` (trong khi `staged_content_command` / `staged_content_delete` vẫn chỉ cho phép ở `HookExecutionMode::Standard`). Nếu `!staged_kv.is_empty() || !staged_logs.is_empty()`, mở **1 giao dịch `BEGIN IMMEDIATE` duy nhất**, kiểm `is_plugin_generation_active` và `is_actor_role_active`, áp dụng mọi `Some(val)` qua `plugin_kv::set`, `None` qua `plugin_kv::delete`, chèn `staged_logs` vào `plugin_logs`, rồi `COMMIT` với handover deadline.
- **`PluginManager` & `PluginRuntime` `secrets_root` Wiring (`plugin/{manager,runtime}.rs`, `main.rs`, `app.rs`):**
  - Bổ sung `pub fn with_roots(db: SqlitePool, allow_loopback: bool, plugins_root: PathBuf, secrets_root: PathBuf) -> Self` trên `PluginManager` (giữ `new()` cho test cũ với fallback temp/default path) và truyền `secrets_root: PathBuf` vào `PluginRuntime` để `HostReadRequest::PluginSecretGet` gọi `SecretStore::for_plugin_dir(&secrets_root.join("plugins").join(plugin_id)).read_plugin_secret(...)`.
  - Trong `main.rs`, khởi tạo `PluginManager::with_roots(db.clone(), allow_loopback, paths.plugins_dir(), paths.secrets_dir())` và cung cấp `paths.secrets_dir()` cho `AppState` / `routes/plugin.rs` (`PUT /api/admin/plugins/{id}/settings`).
- **`PluginManager` Manifest Cache (`manager.rs`):**
  - Thêm `manifest_cache: Arc<tokio::sync::RwLock<BTreeMap<String, CachedActivePlugin>>>` (lưu `manifest: PluginManifest`, `wasm_blob: Vec<u8>`, `revision_generation: i64`, `revision_digest: Option<String>` cho mọi plugin `enabled = 1`).
  - Mọi `hook_subscriptions`, `content_hook_subscriptions`, `filter_subscriptions` đọc trực tiếp từ cache (`O(1)`, 0 SQL); gọi `invalidate_cache()` trong `install_plugin_checked`, `enable_plugin_cas`, `disable_plugin_cas`.
- **Global Worker Ceiling (`runtime.rs`):**
  - Giới hạn `MAX_TOTAL_CONCURRENT_WORKERS = 6` (`4` permits cho `admin_semaphore` — Actions/Admin hooks; `2` permits cho `background_public_semaphore` — `AsyncHookQueue` & `FilterContactBeforeSubmit`). Tổng RAM Wasmi tối đa ~`6 × ~40 MiB` (`< 250 MiB`).

---

### 3. Migration `0050`, Owner Settings PK Fix, Write-Only `SecretStore` & Shared `<OwnerOptionsForm>`

#### 3.1 Migration `0050_plugin_extensibility_and_projections.sql` (+2 net `sqlite_schema` objects)
```sql
-- 1. Rebuild theme_option_values với PRIMARY KEY (owner_kind, owner_id, option_key) (+0 net objects)
CREATE TABLE theme_option_values_0050 (
    theme_id TEXT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin')),
    owner_id TEXT NOT NULL,
    option_key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (owner_kind, owner_id, option_key)
);
INSERT INTO theme_option_values_0050 (theme_id, owner_kind, owner_id, option_key, value_json, updated_at)
SELECT theme_id, owner_kind,
       CASE WHEN owner_id = '' THEN theme_id ELSE owner_id END,
       option_key, value_json, updated_at
  FROM theme_option_values;
DROP TABLE theme_option_values;
ALTER TABLE theme_option_values_0050 RENAME TO theme_option_values;
CREATE INDEX idx_theme_option_values_owner ON theme_option_values (owner_kind, owner_id);

-- 2. Rebuild theme_option_versions với PRIMARY KEY (owner_kind, owner_id) (+0 net objects)
CREATE TABLE theme_option_versions_0050 (
    theme_id TEXT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin')),
    owner_id TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (owner_kind, owner_id)
);
INSERT INTO theme_option_versions_0050 (theme_id, owner_kind, owner_id, version, updated_at)
SELECT theme_id, owner_kind,
       CASE WHEN owner_id = '' THEN theme_id ELSE owner_id END,
       version, updated_at
  FROM theme_option_versions;
DROP TABLE theme_option_versions;
ALTER TABLE theme_option_versions_0050 RENAME TO theme_option_versions;

-- 3. Rebuild theme_option_media_refs với PRIMARY KEY (owner_kind, owner_id, option_key, media_id) (+0 net objects)
CREATE TABLE theme_option_media_refs_0050 (
    theme_id TEXT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT 'theme' CHECK (owner_kind IN ('theme', 'plugin')),
    owner_id TEXT NOT NULL,
    option_key TEXT NOT NULL,
    media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT,
    PRIMARY KEY (owner_kind, owner_id, option_key, media_id)
);
INSERT INTO theme_option_media_refs_0050 (theme_id, owner_kind, owner_id, option_key, media_id)
SELECT theme_id, owner_kind,
       CASE WHEN owner_id = '' THEN theme_id ELSE owner_id END,
       option_key, media_id
  FROM theme_option_media_refs;
DROP TABLE theme_option_media_refs;
ALTER TABLE theme_option_media_refs_0050 RENAME TO theme_option_media_refs;

-- 4. Bảng materialize write-time projections cho public SSR (+2 objects: table + index)
CREATE TABLE plugin_entry_projections (
    entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
    plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    locale TEXT NOT NULL DEFAULT 'vi',
    seo_json TEXT,
    ext_json TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (entry_id, plugin_id)
);
CREATE INDEX idx_plugin_entry_projections_plugin ON plugin_entry_projections (plugin_id, locale);
```
- Cập nhật `cms/migrations/README.md` và `cms/crates/opendash-server/tests/content_foundation_m6_s11_t01.rs:1956` (`assert_eq!(migration_version, 50);`).

#### 3.2 `storage/settings.rs`, `storage/media_refs.rs`, `paths.rs`, & `secrets.rs` (Write-Only `secret`)
- **`paths.rs`:** Thêm `pub fn plugin_secrets_dir(&self, plugin_id: &str) -> PathBuf` trả về `self.secrets_dir().join("plugins").join(plugin_id)`.
- **`secrets.rs`:**
  - Thêm `pub const MAX_PLUGIN_SECRET_BYTES: usize = 4096;` và `pub const MAX_PLUGIN_SECRET_FILE_BYTES: usize = 8192;`.
  - Thêm trên `SecretStore` (hoặc constructor `SecretStore::for_plugin_dir(dir)` tái sử dụng cùng logic `create_protected_empty_file`, `ensure_no_reparse_or_symlink`, `platform::verify_dir_acl`/`verify_file_acl` ở `secrets.rs:57-171`):
    - `pub fn write_plugin_secret(&self, plaintext: &str) -> ApiResult<SecretRef>`
    - `pub fn read_plugin_secret(&self, raw_ref: &str) -> ApiResult<String>`
    - `pub fn remove_plugin_secret(&self, raw_ref: &str)`
- **`storage/settings.rs` (Cập nhật toàn bộ Theme queries + `put_owner_settings_in_conn`):**
  - Cập nhật **tất cả** các truy vấn `theme_option_values`, `theme_option_versions`, `theme_option_media_refs` trong `storage/settings.rs` (`L1771, L1784, L1976, L1986, L1996-1999, L2014-2019, L2031-2035, L2043, L2122, L2249, L2768, L2777, L2830, L2843, L2856, L2875-2877`) để lọc tường minh `owner_kind = 'theme'` / `owner_id` và đổi mọi `ON CONFLICT(theme_id, option_key)` thành **`ON CONFLICT(owner_kind, owner_id, option_key)`**, `ON CONFLICT(theme_id)` thành **`ON CONFLICT(owner_kind, owner_id)`**.
  - Với `theme_option_media_refs` (PK mới `(owner_kind, owner_id, option_key, media_id)`), thực hiện transactional `DELETE FROM theme_option_media_refs WHERE owner_kind = ? AND owner_id = ? AND option_key = ?` rồi `INSERT INTO theme_option_media_refs (...)` khi lưu field `Media` (tách riêng theo `owner_kind`/`owner_id`).
  - Định nghĩa exact DTOs trong `storage/settings.rs`:
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PluginSettingsSnapshot {
        pub plugin_id: String,
        pub version: i64,
        pub schema: Vec<PluginSettingField>,
        pub values: serde_json::Map<String, serde_json::Value>, // Secret -> {"configured": bool}
        pub updated_at: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct PluginSettingsOutcome {
        pub snapshot: PluginSettingsSnapshot,
        pub changed: bool,
        pub has_projection_filters: bool,
    }
    ```
  - Thêm `get_plugin_settings_redacted(db: &SqlitePool, plugin_id: &str, schema: &[PluginSettingField]) -> ApiResult<PluginSettingsSnapshot>` (trường `Secret` chỉ xuất `serde_json::json!({ "configured": true/false })`, **tuyệt đối không xuất `secret_ref` hay plaintext**) và `put_plugin_settings(db: &SqlitePool, secrets_dir: &Path, plugin_id: &str, expected_version: i64, schema: &[PluginSettingField], submitted: &serde_json::Map<String, serde_json::Value>) -> ApiResult<PluginSettingsOutcome>`:
    - Nếu input cho trường `Secret` là `null` hoặc vắng mặt: giữ nguyên `secret_ref` cũ trong DB.
    - Nếu input là `{"clear": true}`: xóa khỏi DB và xóa file cũ sau khi commit thành công.
    - Nếu input là string non-empty (`<= 4096` bytes): gọi `SecretStore::for_plugin_dir(...).write_plugin_secret(&s)` **trước** khi commit, ghi `{"secret_ref": "<64-hex>", "configured": true}` vào `theme_option_values`, và xóa blob cũ sau commit (nếu `PUT` fail/stale, xóa blob mới vừa tạo để tránh file mồ côi).
- **`storage/media_refs.rs:236-255`:** Cập nhật query pin để chọn `owner_kind, owner_id, option_key, value_json` từ `theme_option_values` và `theme_option_media_refs`, phân biệt `theme_option` vs `plugin_option` khi báo cáo pin.

#### 3.3 Admin Endpoints & Shared React UI `<OwnerOptionsForm>`
- **`routes/plugin.rs` & `app.rs`:**
  - `GET /api/admin/plugins/{id}/settings` (`require_admin`) → trả JSON `PluginSettingsSnapshot` (`200 OK`) + `ETag` (`no_store_headers(Some(snapshot.version))`).
  - `PUT /api/admin/plugins/{id}/settings` (`AdmittedMutation` + `require_admin_with_csrf` + `parse_if_match`):
    - Request JSON: `{ "values": { "<key>": <Value | null | {"clear": true}> } }` (header `If-Match: "<version>"` bắt buộc; thiếu trả `428`, lệch trả `412`).
    - Response JSON (`200 OK` + `ETag` mới): `PluginSettingsSnapshot` (trong đó mọi trường `Secret` chỉ trả `{"configured": bool}`).
    - Sau commit: nếu `outcome.changed && outcome.has_projection_filters` kích hoạt batch async `recompute_plugin_projections`; phát `HookEvent::SettingsAfterSave` qua `AsyncHookQueue`.
- **Frontend (`cms/admin/src/`):**
  - `cms/admin/src/types.ts`: định nghĩa `PluginSettingField`, `PluginSettingsSnapshot`, `PluginSecretValue = { configured: boolean } | string | { clear: true } | null`.
  - Tạo `cms/admin/src/features/shared/OwnerOptionsForm.tsx` nhận props `ownerKind: 'theme' | 'plugin'`, `schema`, `fields`, `values`, `version`, hỗ trợ đầy đủ field types kể cả write-only `secret` (ô password có badge `"Đã cấu hình"` / nút `"Xóa bí mật"` gửi `{"clear": true}` hoặc `null` để giữ nguyên).
  - Refactor `ThemeOptionsForm.tsx` và cập nhật `PluginsPanel.tsx` (+ `api/plugins.ts`, `types.ts`) dùng chung `<OwnerOptionsForm>`.

---

### 4. Bounded `AsyncHookQueue`, `filter:content/prepare_save`, & `filter:contact/before_submit`

#### 4.1 `AsyncHookQueue` & Maintenance Drain (`operations.rs`, `hooks.rs`, `main.rs`)
- **`operations.rs` (`begin_maintenance` tại `L214` & `ExclusiveOpsContext::drop` tại `L437-447`):**
  - Thêm `maintenance_cancel_tx: watch::Sender<bool>` vào `SiteCoordinator` và hàm `pub fn maintenance_cancel_rx(&self) -> watch::Receiver<bool>`.
  - Gửi `true` ngay trong `begin_maintenance()` (`operations.rs:214`) và reset `false` trong `ExclusiveOpsContext::drop` (`operations.rs:437-447`).
  - Dùng **2 watch channel tách biệt** trong `HookDispatcher::with_async_queue(plugins: Arc<PluginManager>, maintenance_cancel_rx: watch::Receiver<bool>, shutdown_rx: watch::Receiver<bool>)` (khởi tạo tại `main.rs`, dời `let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);` từ `L138` lên trước lời gọi `HookDispatcher::with_async_queue(...)` tại `L97` để truyền `shutdown_rx.clone()`):
    - Khi `maintenance_cancel_rx == true`: các worker **KHÔNG** thoát hẳn vòng lặp (tránh chết vĩnh viễn sau lần backup đầu tiên!); thay vào đó, worker hủy tiến trình con Wasm đang chạy, **drain & drop toàn bộ `AsyncHookJob` đang chờ trong `mpsc::Receiver`** (nhả tức thì mọi `MutationLease` để không chạm `30s DrainTimeout`), rồi chờ `maintenance_cancel_rx` trở lại `false`.
    - Chỉ khi `shutdown_rx == true` (server tắt), worker mới thoát vòng lặp vĩnh viễn.
- **`hooks.rs`:**
  ```rust
  pub struct AsyncHookJob {
      pub event: HookEvent,
      pub principal: ExecutionPrincipal,
      pub locale: Option<String>,
      pub type_key: Option<String>,
      pub payload: Value,
      pub dispatch_depth: u8,
      pub lease: crate::operations::MutationLease,
  }

  pub struct AsyncHookQueue {
      tx: tokio::sync::mpsc::Sender<AsyncHookJob>,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum ContactBeforeSubmitDecision {
      Allow,
      Reject { code: String, message: String },
  }
  ```
  - Giới hạn cứng: **2 worker tasks**, hàng đợi có biên **`64` jobs** (`try_send` không block HTTP request; nếu đầy ghi log warning vào `plugin_logs`/stderr và thả `lease`).
  - Nếu `job.dispatch_depth >= 2` → bỏ qua (chống vòng lặp đệ quy xuyên async task).
  - Gắn `AsyncHookQueue` vào `HookDispatcher` (qua `with_async_queue(...)`) và bổ sung các method:
    - `pub fn enqueue_async(&self, job: AsyncHookJob)`
    - `pub async fn dispatch_content_prepare_save(&self, type_key: &str, actor_role: opendash_core::content::FieldRole, effective_schema: &opendash_core::content::EffectiveContentSchema, payload: Value) -> ApiResult<ContentPrepareSavePatch>` *(trong đó `payload` là `{ "type_key", "entry_id", "title", "slug", "candidate_values", "term_ids" }`)*
    - `pub async fn dispatch_contact_before_submit(&self, form_id: &str, submission_payload: Value) -> ApiResult<ContactBeforeSubmitDecision>`
    - `pub async fn dispatch_projection_filters(&self, event: HookEvent, plugin_id: Option<&str>, entry_payload: Value) -> ApiResult<BTreeMap<String, Value>>`
  - Phát qua `AsyncHookQueue` các sự kiện `ContentAfterSave`, `ContentAfterDelete`, `ContentAfterPublish`, `ContentAfterUnpublish`, `ContactAfterSubmit`, `PluginAfterEnable`, `PluginAfterDisable`, `SettingsAfterSave`.

#### 4.2 `filter:content/prepare_save` (`hooks.rs`, `routes/entries.rs`)
- Chạy **trước** `content/before_save` trên luồng create/update tại `routes/entries.rs`:
  1. Kiểm `actor_role` thật từ `actor_field_role(&actor)`.
  2. Gọi lần lượt các plugin đăng ký `filter:content/prepare_save` (tối đa `8` plugins, tổng `2s`, mode `FilterContentPrepareSave` — 0 writes, chỉ đọc qua `HostPolicy`).
  3. Mỗi plugin trả sparse patch:
     ```rust
     #[derive(Debug, Deserialize)]
     #[serde(deny_unknown_fields)]
     pub struct ContentPrepareSavePatch {
         #[serde(default)]
         pub title: Option<String>,
         #[serde(default)]
         pub slug: Option<String>,
         #[serde(default)]
         pub values_patch: BTreeMap<String, serde_json::Value>,
         #[serde(default)]
         pub term_ids: Option<Vec<i64>>,
     }
     ```
  4. **Kiểm quyền ghi & ACL từng khóa:**
     - Nếu patch không rỗng (`title.is_some() || slug.is_some() || term_ids.is_some() || !values_patch.is_empty()`), bắt buộc kiểm `manifest.can_write_content_type(type_key)` == `true`; nếu không có quyền ghi trên CPT đó trả ngay `422 Unprocessable Entity`.
     - Với mọi `(k, v)` trong `values_patch`, kiểm tra `effective_schema.effective_fields.get(k)` tồn tại và `plugin_can_write_field(manifest, None, actor_role, type_key, fe)` == `true`; nếu vi phạm trả ngay `422 Unprocessable Entity`.
  5. **Full Re-Validation:** Sau khi áp patch vào `ContentEntryInput`, bắt buộc chạy lại toàn bộ `validate_against_effective(&effective_schema)`, `require_existing_effective_references`, kiểm pattern refs và slug trước khi đi tiếp tới `content/before_save` → commit transaction → tính lại `plugin_entry_projections` → đẩy `AsyncHookQueue`.

#### 4.3 `filter:contact/before_submit` (`contact.rs`, `routes/contact.rs`, `hooks.rs`)
- Chạy bên trong `routes/contact.rs::public_submit` (`POST /api/public/contact/{id}`, `app.rs:179`) / `contact::submit` **SAU** toàn bộ lá chắn giá rẻ (Safe Mode, strict Origin, `application/x-www-form-urlencoded`, Honeypot, `ContactAdmissionLimiter`, per-IP rate limits, `form_version`, `nonce`, `FormSchema` validation) và **TRƯỚC** khi lấy `MutationLease` / ghi DB (`record_submission_in_tx`).
- **Thay đổi extractor & `MutationLease` tại `routes/contact.rs::public_submit` & `contact.rs:107-116`:** Thay `AdmittedMutation` extractor trên handler bằng `State(coordinator): State<Arc<SiteCoordinator>>` (tận dụng `FromRef<AppState> for Arc<SiteCoordinator>` sẵn có tại `app.rs:117-121`). Vì `store::check_and_increment_rate_limits(...)` (`contact.rs:107-116`) ghi SQLite vào bảng `contact_rate_buckets`, lấy một `MutationLease` ngắn hạn bằng hàm **đồng bộ** `let rate_lease = coordinator.admit_mutation()?;` bao quanh `check_and_increment_rate_limits`, sau đó **`drop(rate_lease)` ngay trước khi gọi `hooks.dispatch_contact_before_submit(...)`** (0 DB writes); chỉ khi filter trả `ContactBeforeSubmitDecision::Allow` mới gọi lại `let lease = coordinator.admit_mutation()?;` trước `record_submission_in_tx` và truyền `lease.clone()` vào `smtp_service.spawn_initial_attempt(attempt_id, lease.clone())` (`routes/contact.rs:331`).
- Ngân sách cứng: tối đa **`4` plugins**, tổng **`2s`**, tối đa **`1` HTTP call/plugin**, **`0` DB/KV writes** (`HookExecutionMode::FilterContactBeforeSubmit`), chạy trên semaphore public riêng `2` slots.
- Output hợp lệ: `{ "decision": "allow" }` hoặc `{ "decision": "reject", "code": "...", "message": "..." }` (sanitize UTF-8 `<= 256` chars, trả `422` cho client với `Cache-Control: no-store`).

---

### 5. Write-Time Materialized Projections (`filter:seo/head` + Safe `json_ld`, `filter:entry/view` → `EntryView.ext`)

#### 5.1 Bảng & Storage (`storage/plugin_projections.rs`)
```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntrySeoPluginOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub og_image_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub noindex: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MergedEntryProjection {
    pub seo_override: Option<EntrySeoPluginOverride>,
    pub json_ld: Vec<serde_json::Value>,
    pub ext: BTreeMap<String, serde_json::Value>,
}

pub async fn recompute_entry_projections(
    db: &SqlitePool,
    hooks: &HookDispatcher,
    entry_id: i64,
) -> ApiResult<()>

pub async fn spawn_recompute_plugin_projections_batched(
    db: SqlitePool,
    hooks: Arc<HookDispatcher>,
    coordinator: Arc<SiteCoordinator>,
    plugin_id: String,
    maintenance_cancel_rx: tokio::sync::watch::Receiver<bool>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()>

pub async fn batch_load_entry_projections_in_conn(
    conn: &mut SqliteConnection,
    entry_ids: &[i64],
) -> ApiResult<BTreeMap<i64, MergedEntryProjection>>

pub async fn delete_entry_projections_in_tx(
    tx: &mut SqliteConnection,
    entry_id: i64,
) -> Result<(), sqlx::Error>

pub async fn delete_plugin_projections_in_tx(
    tx: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<(), sqlx::Error>
```
- **Thời điểm kích hoạt tính toán lại:**
  1. Sau khi tạo/sửa/xuất bản/hủy xuất bản entry (`routes/entries.rs`): nếu entry là `published`, gọi `recompute_entry_projections` (luôn dispatch `HookExecutionMode::FilterProjection` với **`ExecutionPrincipal::Public`** và chỉ truyền các field công khai, **không** dùng `actor_role` Admin/Editor để tránh rò rỉ field nội bộ ra projection công khai); nếu chuyển về `draft`/`deleted`, gọi `delete_entry_projections_in_tx` để không rò rỉ bản nháp.
  2. Khi bật plugin (`plugin_enable`) hoặc lưu plugin settings (`PUT /api/admin/plugins/{id}/settings`) cho các plugin đăng ký `filter:seo/head` hoặc `filter:entry/view`: chạy bất đồng bộ qua `spawn_recompute_plugin_projections_batched` (không block HTTP handler!), duyệt theo cursor `entry_id > last_id ORDER BY entry_id ASC LIMIT 100`, mỗi batch xin `coordinator.admit_mutation()?` ngắn hạn (đồng bộ tại `operations.rs:192`) và kiểm tra `maintenance_cancel_rx` / `shutdown_rx` giữa các batch.
  3. Khi tắt/xóa plugin (`plugin_disable` / uninstall): gọi `delete_plugin_projections_in_tx` xóa ngay các dòng `WHERE plugin_id = ?` (kết hợp lọc `JOIN plugins p ON p.id = pep.plugin_id AND p.enabled = 1` trong `batch_load_entry_projections_in_conn`).
- **0 Wasm spawn trên Public `GET`:** `routes/public_content.rs` và `storage/seo.rs` chỉ gọi `batch_load_entry_projections_in_conn` (1 `SELECT` dùng `json_each(?)`, `O(1)` SQL).

#### 5.2 An toàn `filter:seo/head` & `<script type="application/ld+json">` (`seo.rs`, `storage/seo.rs`, `content_render.rs`)
- Output của `filter:seo/head`:
  ```rust
  #[derive(Debug, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct SeoHeadFilterOutput {
      pub title: Option<String>,
      pub description: Option<String>,
      pub og_image_id: Option<String>,
      pub canonical_url: Option<String>,
      #[serde(default)]
      pub noindex: Option<bool>,
      #[serde(default)]
      pub json_ld: Vec<serde_json::Value>,
  }
  ```
- **Ràng buộc an toàn bắt buộc:**
  1. Re-validate mọi trường `title`, `description`, `og_image_id`, `canonical_url` bằng `seo::validate_seo_input` (`seo.rs:149`); với `og_image_id`, kiểm tra thêm media asset tồn tại và ở trạng thái public qua `storage/media` (không phát sinh mutation `seo_public_media_refs` trên đường đọc).
  2. **Cấm hạ `noindex`:** Nếu Core hoặc Entry SEO đặt `noindex == true` (bao gồm 404, preview, `Unconfigured`, hoặc cài đặt `indexing_enabled == false`), plugin **tuyệt đối không thể** ghi đè về `false` (`final_noindex = core_noindex || plugin_noindex.unwrap_or(false)`).
  3. **`json_ld` Bounds:** Mỗi invocation tối đa `4` block; sau khi gộp các plugin trên 1 entry: tổng tối đa **`4` block**, tổng kích thước serialized **`<= 16 KiB`**, độ sâu JSON mỗi block **`<= 8`**, và phần tử gốc mỗi block **bắt buộc là JSON Object** (`Value::Object`).
- **Render an toàn tại `content_render::compose_document_with_seo`:**
  - Thêm `pub json_ld: Vec<serde_json::Value>` vào `SeoView` (`seo.rs:96`).
  - Trong `content_render.rs`: serialize từng block qua helper `serialize_json_ld_safe(val: &Value) -> String` thay thế bắt buộc `<` → `\u003c`, `>` → `\u003e`, `&` → `\u0026`, ký tự Unicode U+2028 → `\u2028`, ký tự Unicode U+2029 → `\u2029` rồi bọc `<script type="application/ld+json">{escaped}</script>` trong `<head>`. (Giữ nguyên kiểm tra `document.rs` bình thường vì `compose_document_with_seo` tự lắp `<head>` sau khi chuẩn hóa body/slots.)

#### 5.3 `filter:entry/view` → `EntryView.ext` (`content_render.rs`, `storage/overrides.rs`, `routes/public_content.rs`)
- Output của `filter:entry/view`: JSON Object (`<= 32 KiB` serialized, độ sâu `<= 6`, tối đa `64` keys).
- Bổ sung `pub ext: BTreeMap<String, serde_json::Value>` vào `EntryView` (`content_render.rs:59-69`) kèm `#[derive(Default)]` (không cần `#[serde(default)]` vì `EntryView` chỉ derive `Debug, Serialize`; đồng thời cập nhật cấu trúc placeholder `EntryView` trong `storage/overrides.rs` với `ext: BTreeMap::new()` để khi chưa có projection, MiniJinja dùng `entry.ext["my-shop"] | default({})` không lỗi `UndefinedBehavior::Strict`).
- Trên mọi đường public (`Single`, `Archive`, `Taxonomy`, `related_entries`, `recent_posts`), nạp `batch_load_entry_projections_in_conn` điền `EntryView.ext[plugin_id] = ext_value`.

---

### 6. Offline CLI Validator `opendash --check-plugin` (`main.rs`, `plugin/package.rs`)

- **Phân tích tham số (`main.rs`):** Dùng `std::env::args_os()` + `OsStrExt` prefix helper (dùng chung pattern với Spec 1 `--check-theme`; trên Windows dùng `#[cfg(windows)] std::os::windows::ffi::OsStrExt::encode_wide`), chấp nhận `--check-plugin <dir|zip>` và `--check-plugin=<dir|zip>`, chạy offline hoàn toàn **trước** `SiteCoordinator::acquire` và DB init. Từ chối thiếu đường dẫn / tham số thừa bằng JSON `{ "ok": false, "error": "..." }` lên `stdout` và `std::process::exit(1)`.
- **`PluginPackage::from_path(path: &Path, engine: &Engine)` & `PluginPackage::from_dir(root: &Path, engine: &Engine)` (`plugin/package.rs`):**
  1. Phân biệt regular file (đọc giới hạn `File::open` + `.take((MAX_PLUGIN_ZIP_BYTES + 1) as u64)` rồi gọi `PluginPackage::from_zip`) vs directory.
  2. Khi là directory: duyệt đệ quy với `MAX_PLUGIN_DIR_DEPTH = 4` (`blocks/<slug>/render.html`), dùng `fs::symlink_metadata` từ chối entry bắt đầu bằng `.`, symlink nội bộ, FIFO/socket/device, tên non-UTF-8, xung đột ASCII case-insensitive (`to_ascii_lowercase()`), kiểm tổng `<= MAX_PLUGIN_FILES` (`72`), đọc từng file qua `File::open` + `.take((cap + 1) as u64)`, xác minh `manifest.json` (`manifest_schema` `1..=3`), `plugin.wasm` (`wasmi::Module::new` + kiểm imports/exports `opendash_v1` + `opendash_run`), và `blocks/<slug>/{block.json,render.html}` qua `validate_block_template_csec`.
- **JSON Output (`stdout`):**
  - Thành công (`exit(0)`): 1 dòng JSON `{ "ok": true, "plugin_id": "...", "version": "...", "manifest_schema": 3, "hooks": [...], "filters": [...], "actions": [...], "blocks": [...], "settings_count": N }` (tất cả mảng `Vec<String>` sort ASCII tăng dần).
  - Thất bại (`exit(1)`): `{ "ok": false, "error": "..." }`.

---

## Alternatives Considered

| Điểm quyết định | Phương án đã bác bỏ | Lý do bác bỏ |
|---|---|---|
| Chạy `filter:seo/head` & `filter:entry/view` | Gọi Wasm đồng bộ trên mỗi HTTP `GET` public | Sẽ cạn 4 worker permits khi có traffic cao, phá `Cache-Control` và gây độ trễ hàng trăm ms; materialized projection giữ public `GET` ở 1 batch `SELECT` (`2A`). |
| Lưu plugin `secret` | Lưu vào `plugin_kv` hoặc plaintext trong `theme_option_values` | Dễ lộ qua SQLite dump / Admin GET / `kv_get` ngoài ý muốn; tái sử dụng `secrets::SecretStore` giữ mã hóa file system `0600` chuẩn M8 (`3A`). |
| Sửa xung đột Settings theme vs plugin | Tạo bảng riêng `plugin_settings` mới | Gây phình `sqlite_schema` gần ngưỡng `MAX_SCHEMA_OBJECTS = 256` và nhân đôi code Admin; rebuild PK `(owner_kind, owner_id, option_key)` trong `0050` tốn `+0` schema objects và dùng chung `<OwnerOptionsForm>`. |
| Giao thức Worker IPC | Giữ `println!("__OPENDASH_IPC__:...")` | Vỡ khung khi payload chứa newline và khó mở rộng cho `HostReadRequest` đa op; framing `u32 LE` giải quyết triệt để và an toàn. |

---

## Non-Functional Requirements

1. **`0` Wasm Spawn & `O(1)` SQL trên Public `GET`:** Đường nóng `routes/public_content.rs` chỉ đọc projection qua 1 batch `SELECT` (`batch_load_entry_projections_in_conn`), không bao giờ khởi chạy `--plugin-worker`.
2. **An toàn `MAX_SCHEMA_OBJECTS = 256`:** Migration `0050` chỉ tạo ròng **+2 objects** (1 bảng `plugin_entry_projections` + 1 index `idx_plugin_entry_projections_plugin`); 3 bảng rebuild giữ +0 net objects.
3. **Ổn định Bộ nhớ & Maintenance Drain:** Tổng worker đồng thời `<= 6` (`~250 MiB` ceiling); `AsyncHookQueue` tối đa `64` jobs, hủy tức thì qua `SiteCoordinator::maintenance_cancel_rx()` để bảo đảm `drain <= 5s`.
4. **Bảo toàn Dữ liệu & Tương thích ngược:** Plugin `manifest_schema` `1`/`2` chạy nguyên trạng với `HookEnvelope` cũ và `host_call` cũ; tắt/xóa plugin không làm mất nội dung CPT hay làm hỏng trang công khai.

---

## Detailed Implementation Plan

### Slice S01: `manifest_schema: 3`, `EnvelopeV2`, ABI v1.1 (`-2` Readback), IPC Framing, Host Ops v1.1 & Manifest Cache
- **Dependencies:** None (nền cốt lõi `opendash-core` + `plugin/` + `hooks.rs` + `routes/entries.rs`).
- **File Ownership:**
  - `cms/crates/opendash-core/src/hooks.rs` (mở rộng `HookEvent`, `HookExecutionMode`, `ExecutionPrincipal`, `EnvelopeKind`, `PluginEnvelopeV2`, helpers `is_filter`/`is_async_hook`)
  - `cms/crates/opendash-core/src/plugin.rs` (`manifest_schema: 3`, đổi `!= 2` → `< 2`, `PluginSettingFieldType`/`PluginSettingField`, `filters`, `settings_schema`, permissions v3 mới + `action.capabilities` allowlist + cấm trộn `content_delete` với `kv_write`/`content_write` tại `L413-415`, sửa `plugin_can_write_field` `L635-658` nhận `Option<&[String]>`)
  - `cms/crates/opendash-server/src/plugin/package.rs` (cập nhật `module.imports()` allowlist tại `L187-252` cho `host_last_resp_len` và `host_read_last_resp`)
  - `cms/crates/opendash-server/src/plugin/runtime.rs` (ABI v1.1 `host_last_resp_len`/`host_read_last_resp`, đọc ghi binary `u32 LE` IPC frames, `HostReadRequest` handler với `secrets_root`, cập nhật `L537` cho phép `AsyncPostCommit` với `staged_kv`/`staged_logs`, single-tx `StagedCommitGuard`, 2 semaphores `4 + 2 = 6` workers)
  - `cms/crates/opendash-server/src/plugin/worker.rs` (binary `u32 LE` IPC frames, `HostContext` mới với `last_resp_bytes` và `HostReadRequest` callback)
  - `cms/crates/opendash-server/src/plugin/host_ops.rs` (`HostPolicy`, `HostOperationRequest`/`HostOperationResponse`, 11 Host Ops mới, chặn `/api/p/` loopback tại `L175`, tombstone `Option<Value>`, cấm trộn `kv_*` với `content_save`/`content_delete`)
  - `cms/crates/opendash-server/src/plugin/manager.rs` (sửa `L228` kiểm `network`, `L336` truyền `Some(&action_decl.capabilities)`, `with_roots(...)`, `manifest_cache` invalidation, thực thi `staged_content_delete` có fence + CAS, truyền `ExecutionPrincipal`/`EnvelopeV2` cho schema `>= 3`)
  - `cms/crates/opendash-server/src/plugin/review_regressions.rs` (cập nhật `PluginManifest { ... }` struct literals thêm `filters`/`settings_schema`)
  - `cms/crates/opendash-server/src/plugin/tests.rs` (cập nhật `PluginManifest { ... }` struct literals thêm `filters`/`settings_schema`)
  - `cms/crates/opendash-server/tests/m6_s04_t01_egress_ssrf.rs` (cập nhật `PluginManifest { ... }` struct literal tại `L20` thêm `filters`/`settings_schema`)
  - `cms/crates/opendash-server/src/storage/plugin.rs` (helper `load_all_enabled_plugins_for_cache` và helper `content_delete` / `plugin_logs` batch)
  - `cms/crates/opendash-server/src/storage/plugin_kv.rs` (`delete` và `list` với prefix + limit)
  - `cms/crates/opendash-server/src/hooks.rs` (sửa `L242, 389` nhận `actor_role` thật và hỗ trợ `filter_subscriptions`)
  - `cms/crates/opendash-server/src/routes/entries.rs` (truyền `actor_field_role(&actor)` thật tại `L423, 438, 491, 509, 680, 689, 740, 749`)
- **Acceptance Criteria:**
  1. Plugin `manifest_schema: 1/2` pass 100% test cũ không đổi hành vi; plugin `manifest_schema: 3` dùng `network` gọi được capability `"http"` (`manager.rs:228`).
  2. ABI v1.1 `host_last_resp_len` + `host_read_last_resp` đọc trọn payload `> resp_max` sau khi `host_call` trả `-2` mà không gọi lại op; IPC framing `u32 LE` xử lý an toàn JSON chứa `\n` tới `2 MiB`.
  3. Toàn bộ 11 Host Ops mới tuân thủ ma trận `HostPolicy`; `kv_delete` tombstone hiển thị ngay trong `kv_get`/`kv_list` cùng invocation và commit atomically trong 1 `BEGIN IMMEDIATE` transaction; tổng concurrent workers không vượt `6`.
- **Rollback:** Revert PR S01; DB schema không đổi.

### Slice S02: Migration `0050`, Owner Settings PK Fix, Write-Only `SecretStore`, Admin Settings API & `<OwnerOptionsForm>`
- **Dependencies:** S01 (`PluginSettingField`, `PluginSettingFieldType::Secret`).
- **File Ownership:**
  - `cms/migrations/0050_plugin_extensibility_and_projections.sql` *(Create)* (rebuild PK 3 bảng `theme_option_*` + tạo `plugin_entry_projections` & index)
  - `cms/migrations/README.md` (cập nhật tới `0050`)
  - `cms/crates/opendash-server/src/paths.rs` (`plugin_secrets_dir`)
  - `cms/crates/opendash-server/src/secrets.rs` (`write_plugin_secret`, `read_plugin_secret`, `remove_plugin_secret`)
  - `cms/crates/opendash-server/src/storage/settings.rs` (`ON CONFLICT(owner_kind, owner_id, option_key)`, `get_plugin_settings_redacted`, `put_plugin_settings` giữ/xóa/xoay `SecretStore`)
  - `cms/crates/opendash-server/src/storage/media_refs.rs` (phân biệt `owner_kind` tại `L236-255`)
  - `cms/crates/opendash-server/src/routes/plugin.rs` (`plugin_get_settings`, `plugin_put_settings` với CAS `If-Match` + `AdmittedMutation`)
  - `cms/crates/opendash-server/src/app.rs` (đăng ký route `/api/admin/plugins/{id}/settings`)
  - `cms/crates/opendash-server/tests/content_foundation_m6_s11_t01.rs` (`assert_eq!(migration_version, 50);`)
  - `cms/admin/src/types.ts` (`PluginSettingField`, `PluginSettingsSnapshot`)
  - `cms/admin/src/api/plugins.ts` (`getPluginSettings`, `savePluginSettings`)
  - `cms/admin/src/features/shared/OwnerOptionsForm.tsx` *(Create)* (hỗ trợ cả Theme options và Plugin settings gồm input write-only `secret`)
  - `cms/admin/src/features/themes/ThemeOptionsForm.tsx` (dùng `<OwnerOptionsForm>`)
  - `cms/admin/src/features/plugins/PluginsPanel.tsx` (nhúng tab/panel cài đặt plugin dùng `<OwnerOptionsForm>`)
- **Acceptance Criteria:**
  1. Migration `0050` áp dụng sạch (+2 net `sqlite_schema` objects); `content_foundation_m6_s11_t01.rs` pass với `migration_version == 50`.
  2. Theme và Plugin trùng ID không ghi đè chéo `theme_option_values` / `theme_option_versions` / `theme_option_media_refs`.
  3. Trường `secret` ghi vào `database/secrets/plugins/<plugin_id>/` (`0600`), `GET` settings không bao giờ lộ plaintext hay `secret_ref`, `PUT` với `null` giữ nguyên secret cũ và `{"clear": true}` xóa sạch cả DB lẫn file; `cms/admin` build & typecheck sạch.
- **Rollback:** Revert PR S02; phục hồi DB bằng snapshot M8 nếu cần hạ schema `0050`.

### Slice S03: `AsyncHookQueue`, Typed Pipelines & Write-Time Materialized Projections (`filter:seo/head`, `filter:entry/view`)
- **Dependencies:** S01, S02 (`plugin_entry_projections` table, `SecretStore` cho `plugin_secret_get` trong `AsyncHookQueue`/`FilterContactBeforeSubmit`).
- **File Ownership:**
  - `cms/crates/opendash-server/src/main.rs` (dời khởi tạo `(shutdown_tx, mut shutdown_rx)` từ `L138` lên trước `L97` để truyền `shutdown_rx.clone()`, khởi tạo `PluginManager::with_roots` và `HookDispatcher::with_async_queue` với `maintenance_cancel_rx` & `shutdown_rx`)
  - `cms/crates/opendash-server/src/operations.rs` (`maintenance_cancel_tx` + `maintenance_cancel_rx()`, gửi `true` trong `begin_maintenance` `L214` và reset `false` trong `ExclusiveOpsContext::drop` `L437-447`)
  - `cms/crates/opendash-server/src/hooks.rs` (`AsyncHookQueue` 2 workers / cap 64 / `MutationLease` / `dispatch_depth <= 2` / drain khi `maintenance_cancel_rx`, `dispatch_content_prepare_save`, `dispatch_contact_before_submit`, `dispatch_projection_filters`)
  - `cms/crates/opendash-server/src/contact.rs` (gọi `filter:contact/before_submit` trước ghi DB; giữ `MutationLease` ngắn hạn cho `check_and_increment_rate_limits` rồi drop trước filter)
  - `cms/crates/opendash-server/src/routes/contact.rs` (đổi extractor `AdmittedMutation` → `State(coordinator): State<Arc<SiteCoordinator>>`, kiểm lá chắn -> `filter:contact/before_submit` -> `let lease = coordinator.admit_mutation()?;` -> lưu DB -> `smtp_service.spawn_initial_attempt(attempt_id, lease.clone())` -> phát `ContactAfterSubmit`)
  - `cms/crates/opendash-server/src/routes/entries.rs` (chạy `filter:content/prepare_save` + `can_write_content_type` + re-validate -> `content/before_save` -> commit -> `recompute_entry_projections` -> phát `after_save` / `after_publish` / `after_unpublish` / `after_delete`)
  - `cms/crates/opendash-server/src/storage/mod.rs` (export `pub mod plugin_projections;`)
  - `cms/crates/opendash-server/src/storage/plugin_projections.rs` *(Create)* (`recompute_entry_projections`, `spawn_recompute_plugin_projections_batched`, `batch_load_entry_projections_in_conn`, `delete_entry_projections_in_tx`, `delete_plugin_projections_in_tx`)
  - `cms/crates/opendash-server/src/storage/overrides.rs` (cập nhật placeholder `EntryView` thêm `ext: BTreeMap::new()`)
  - `cms/crates/opendash-server/src/storage/seo.rs` (trộn `seo_override` & `json_ld` từ `batch_load_entry_projections_in_conn`, cấm gỡ `noindex: true`)
  - `cms/crates/opendash-server/src/seo.rs` (`SeoView.json_ld` + validator `validate_json_ld_blocks`)
  - `cms/crates/opendash-server/src/content_render.rs` (`EntryView` + `#[derive(Default)]` + `ext`, `serialize_json_ld_safe` escape `\u003c`/`\u003e`/`\u0026`/`\u2028`/`\u2029` trong `<head>`)
  - `cms/crates/opendash-server/src/routes/public_content.rs` (nạp `batch_load_entry_projections_in_conn` điền `EntryView.ext` trên mọi route public với 0 Wasm spawn)
  - `cms/crates/opendash-server/src/routes/plugin.rs` (kích hoạt `spawn_recompute_plugin_projections_batched` khi enable hoặc lưu settings; xóa projections khi disable/uninstall; phát `PluginAfterEnable`/`PluginAfterDisable`/`SettingsAfterSave`)
  - `cms/crates/opendash-server/src/storage/plugin.rs` (xóa projections khi uninstall/cleanup)
- **Acceptance Criteria:**
  1. `AsyncHookQueue` chạy `after_*` bất đồng bộ giữ `MutationLease` và drain hoàn tất `<= 5s` khi `SiteCoordinator` vào maintenance.
  2. `filter:content/prepare_save` áp sparse patch qua kiểm tra `plugin_can_write_field` + full re-validation; `filter:contact/before_submit` chạy trước `MutationLease`/DB với max 4 plugins / 2s / 1 HTTP / 0 DB writes.
  3. Public `GET` chạy **0 Wasm spawn**, chỉ đọc `plugin_entry_projections` trong 1 batch query; `json_ld` chứa `</script>` bị escape thành `\u003c/script>`; `entry.ext.<plugin_id>` render an toàn dưới `UndefinedBehavior::Strict`.
- **Rollback:** Revert PR S03; vô hiệu hóa plugin projections để quay về mặc định.

### Slice S04: Offline `opendash --check-plugin` CLI, Cập nhật Hợp đồng `PLUGIN_ABI.md` & E2E Test Suite
- **Dependencies:** S01, S02, S03.
- **File Ownership:**
  - `cms/crates/opendash-server/src/main.rs` (`--check-plugin <path>` / `--check-plugin=<path>` dùng `std::env::args_os()` + `OsStrExt`)
  - `cms/crates/opendash-server/src/plugin/package.rs` (`PluginPackage::from_path`, `PluginPackage::from_dir` với `MAX_PLUGIN_DIR_DEPTH = 4`, `.take((cap + 1) as u64)`, chống symlink/hidden/non-UTF-8/FIFO/case-collision)
  - `cms/crates/opendash-server/tests/plugin_extensibility_test.rs` *(Create)* (E2E kiểm chứng CUJ-1..CUJ-4, ABI `-2` readback, IPC newline, `kv_delete`/`kv_list`, `content_delete` CAS, `SecretStore` redact + `plugin_secret_get`, `prepare_save` ACL bypass, `contact/before_submit` trước DB, `json_ld` `</script>` escape, `noindex` protection, `entry.ext.*` 0 Wasm spawn, `AsyncHookQueue` maintenance drain, và `--check-plugin`)
  - `docs/contracts/PLUGIN_ABI.md` (cập nhật ABI v1.1, `manifest_schema: 3`, IPC framing, 15 Host Ops & `HostPolicy`)
  - `docs/designs/SITEMAP.md` (cập nhật trạng thái Spec 2)
- **Acceptance Criteria:**
  1. `opendash --check-plugin <dir|zip>` chạy offline không cần DB/locks, in JSON chuẩn lên `stdout` với exit `0`/`1`.
  2. Toàn bộ `plugin_extensibility_test.rs`, `cargo test --workspace --release --locked`, `cms/admin` build/typecheck và governance checks xanh 100%.
- **Rollback:** Revert PR S04; không tác động DB hay plugin runtime.

### Target Files to Create/Modify
- `cms/crates/opendash-core/src/hooks.rs` *(Modify)*
- `cms/crates/opendash-core/src/plugin.rs` *(Modify)*
- `cms/migrations/0050_plugin_extensibility_and_projections.sql` *(Create)*
- `cms/migrations/README.md` *(Modify)*
- `cms/crates/opendash-server/src/app.rs` *(Modify)*
- `cms/crates/opendash-server/src/contact.rs` *(Modify)*
- `cms/crates/opendash-server/src/content_render.rs` *(Modify)*
- `cms/crates/opendash-server/src/hooks.rs` *(Modify)*
- `cms/crates/opendash-server/src/main.rs` *(Modify)*
- `cms/crates/opendash-server/src/operations.rs` *(Modify)*
- `cms/crates/opendash-server/src/paths.rs` *(Modify)*
- `cms/crates/opendash-server/src/secrets.rs` *(Modify)*
- `cms/crates/opendash-server/src/seo.rs` *(Modify)*
- `cms/crates/opendash-server/src/plugin/host_ops.rs` *(Modify)*
- `cms/crates/opendash-server/src/plugin/manager.rs` *(Modify)*
- `cms/crates/opendash-server/src/plugin/package.rs` *(Modify)*
- `cms/crates/opendash-server/src/plugin/review_regressions.rs` *(Modify)*
- `cms/crates/opendash-server/src/plugin/runtime.rs` *(Modify)*
- `cms/crates/opendash-server/src/plugin/tests.rs` *(Modify)*
- `cms/crates/opendash-server/src/plugin/worker.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/mod.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/media_refs.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/overrides.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/plugin.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/plugin_kv.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/plugin_projections.rs` *(Create)*
- `cms/crates/opendash-server/src/storage/seo.rs` *(Modify)*
- `cms/crates/opendash-server/src/storage/settings.rs` *(Modify)*
- `cms/crates/opendash-server/src/routes/contact.rs` *(Modify)*
- `cms/crates/opendash-server/src/routes/entries.rs` *(Modify)*
- `cms/crates/opendash-server/src/routes/plugin.rs` *(Modify)*
- `cms/crates/opendash-server/src/routes/public_content.rs` *(Modify)*
- `cms/crates/opendash-server/tests/content_foundation_m6_s11_t01.rs` *(Modify)*
- `cms/crates/opendash-server/tests/m6_s04_t01_egress_ssrf.rs` *(Modify)*
- `cms/crates/opendash-server/tests/plugin_extensibility_test.rs` *(Create)*
- `cms/admin/src/types.ts` *(Modify)*
- `cms/admin/src/api/plugins.ts` *(Modify)*
- `cms/admin/src/features/shared/OwnerOptionsForm.tsx` *(Create)*
- `cms/admin/src/features/themes/ThemeOptionsForm.tsx` *(Modify)*
- `cms/admin/src/features/plugins/PluginsPanel.tsx` *(Modify)*
- `docs/contracts/PLUGIN_ABI.md` *(Modify)*
- `docs/designs/SITEMAP.md` *(Modify)*

### Verification Plan
Chạy tuần tự sau mỗi slice và trước khi đóng PR *(dùng `python` và `.\cms\target\release\opendash.exe` trên Windows cho Forge; các lệnh gắn `(S02+)` / `(S04+)` chỉ chạy từ slice tương ứng)*:
```shell
cargo fmt --manifest-path cms/Cargo.toml --all --check
python3 cms/scripts/check-standards.py
python3 cms/scripts/check-standards.py --self-test
python3 cms/scripts/check-plans.py
python3 cms/scripts/check-plans.py --self-test
python3 .agents/scripts/check-agent-artifacts.py
cargo build --manifest-path cms/Cargo.toml --locked
cargo test --manifest-path cms/Cargo.toml -p opendash-server --bin opendash --release --locked
cargo test --manifest-path cms/Cargo.toml --workspace --release --locked
npm --prefix cms/admin run build # (S02+)
cargo build --manifest-path cms/Cargo.toml --release --locked # (S04+)
cargo test --manifest-path cms/Cargo.toml -p opendash-server --test plugin_extensibility_test --release --locked # (S04+)
git diff --check
```
Kiểm chứng thủ công `--check-plugin` *(S04+)*: chạy `./cms/target/release/opendash --check-plugin <fixture-dir-or-zip>` và xác nhận exit code `0` cùng JSON hợp lệ trên `stdout`.
