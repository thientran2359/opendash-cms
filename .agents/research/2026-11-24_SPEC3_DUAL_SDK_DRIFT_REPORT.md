# EGM Drift Report — Spec 3: Dual Guest SDK (`AssemblyScript` & `opendash-plugin-sdk`) & Reference Examples (`plugin-dual-sdk-and-examples.md`)

- **Date:** 2026-11-24
- **Spec Audited:** [`docs/designs/plugin-dual-sdk-and-examples.md`](../../docs/designs/plugin-dual-sdk-and-examples.md)
- **Evaluated Baseline & Fix Scope:** `origin/main@6bcd3d59dd14a9d235020d89af10817bf51ab256` (S01 `#528` / PR `#538` `9d5c003`, S02 `#529` / PR `#539` `5e4c2fb`, S03 `#530` / PR `#545` `6bcd3d5`) + fixes trên nhánh `fix/spec3-drift-fixes-and-verified-closure`
- **Independent Goldfish Auditor:** `egm-codebase-details` (fresh-context subagent `e139fb38-c636-4a28-9078-05517c56fb28` đối chiếu `origin/main@6bcd3d5`)
- **Coordinator:** Orion
- **Initial Audit Verdict on `main@6bcd3d5`:** `FAIL` (`1 Blocker · 1 Major · 4 Minor · 4 Nit`)
- **Post-Fix Verdict:** **`PASS` (`0 Blocker · 0 Major · 1 Minor · 1 Nit` non-blocking)**

---

## 1. Ma trận Kiểm chứng theo Slice (`S01`–`S03` & ABI/Security Parity)

| Phạm vi | Trạng thái | Đối chiếu mã nguồn thực tế (`file:line` audit `e139fb38` + fixes) |
| --- | --- | --- |
| **S01 (`#528` / PR `#538` `9d5c003` — AssemblyScript SDK v1.1)** | **PASS** *(sau fix `F-01`, `F-06`)* | 6 imports `opendash_v1`, đệm `64 KiB` / trần `2 MiB`, tự động readback `-2` qua `host_last_resp_len` + `host_read_last_resp` ([`opendash.ts`](../../cms/sdk/as/opendash.ts)); `readEnvelope()` hỗ trợ dual `HookEnvelope` v1 & `PluginEnvelopeV2`; đủ 15 Host Ops + `extractDataObj` fallback; 7 output builders + giữ nguyên tương thích [`sample-plugin`](../../cms/examples/plugins/sample-plugin/assembly/index.ts); [`test_harness.ts`](../../cms/sdk/as/test_harness.ts) & [`test_sdk_v11.mjs`](../../cms/sdk/as/test_sdk_v11.mjs); [`cms/sdk/README.md`](../../cms/sdk/README.md). |
| **S02 (`#529` / PR `#539` `5e4c2fb` — Rust Guest SDK `opendash-plugin-sdk`)** | **PASS** *(sau fix `F-02`, `F-04`)* | Thành viên workspace [`cms/Cargo.toml`](../../cms/Cargo.toml) (`0` crate ngoài mới ở [`cms/Cargo.lock`](../../cms/Cargo.lock), `edition = "2024"` tại [`opendash-plugin-sdk/Cargo.toml`](../../cms/crates/opendash-plugin-sdk/Cargo.toml)); ABI v1.1 + zero-init `vec![0u8; cap]` + `-2` readback ([`abi.rs`](../../cms/crates/opendash-plugin-sdk/src/abi.rs)); `export_plugin!` ([`lib.rs`](../../cms/crates/opendash-plugin-sdk/src/lib.rs)); đủ DTO & 15 Host Ops gồm `ContentSaveInput.locale` ([`types.rs`](../../cms/crates/opendash-plugin-sdk/src/types.rs), [`host.rs`](../../cms/crates/opendash-plugin-sdk/src/host.rs)); `MockHost` (`clamp(1, 100)`, CAS conflicts, RAII panic cleanup) ([`mock.rs`](../../cms/crates/opendash-plugin-sdk/src/mock.rs)). |
| **S03 (`#530` / PR `#545` `6bcd3d5` — Reference Examples)** | **PASS** *(sau fix `F-05`, `N-01`, `N-02`)* | Plugin mẫu [`rust-seo-plugin`](../../cms/examples/plugins/rust-seo-plugin/src/lib.rs) tách `[workspace]` độc lập ([`Cargo.toml`](../../cms/examples/plugins/rust-seo-plugin/Cargo.toml), [`manifest.json`](../../cms/examples/plugins/rust-seo-plugin/manifest.json) `manifest_schema: 3`, [`README.md`](../../cms/examples/plugins/rust-seo-plugin/README.md)), `seo/head` JSON-LD `Value::Object`, `entry/view`, `content/after_publish` không lộ `webhook_token`, 4 unit test qua `MockHost`. Theme mẫu [`sample-theme-v3`](../../cms/examples/themes/sample-theme-v3/theme.json) (`manifest_version: 3`, [`assets/app.css`](../../cms/examples/themes/sample-theme-v3/assets/app.css), [`single.html`](../../cms/examples/themes/sample-theme-v3/single.html) C-SEC compliant + `entry.ext` 2-bước, [`theme.rs`](../../cms/crates/opendash-server/src/theme/mod.rs) `walk_v3_dir` skip `README.md`), PASS `--check-theme`. |
| **Cross-Spec Wasm ABI & Security** | **PASS** | Chữ ký import/export `opendash_v1` ở cả `opendash.ts` lẫn `opendash-plugin-sdk` khớp 100% [`plugin/package.rs`](../../cms/crates/opendash-server/src/plugin/package.rs) & [`plugin/runtime.rs`](../../cms/crates/opendash-server/src/plugin/runtime.rs); `ContentEntry` khớp projection ở `runtime.rs`; `single.html` & `app.css` sạch C-SEC (`template_sec.rs`, `document.rs`). |

---

## 2. Chi tiết Phát hiện Goldfish (`e139fb38`) & Tình trạng Xử lý

| ID | Mức ban đầu | Trạng thái sau Fix | Chi tiết xử lý & vị trí |
| --- | --- | --- | --- |
| **F-01** | **Blocker** | **RESOLVED** | [`cms/sdk/README.md#L40`](../../cms/sdk/README.md#L40) đã sửa mô tả `contentGet(typeKey, entryId, locale = null)` thành trả chuỗi JSON phản hồi đầy đủ `{"ok":true,"data":{"entry":{...}\|null}}` từ `hostCall` (khớp [`opendash.ts#L351-L360`](../../cms/sdk/as/opendash.ts#L351-L360) và `test_sdk_v11.mjs`); bổ sung `status` tại dòng `contentSave`. |
| **F-02** | **Major** | **RESOLVED** | Bổ sung `#[serde(default, skip_serializing_if = "Option::is_none")] pub locale: Option<String>` vào `ContentSaveInput` tại [`types.rs#L112-L129`](../../cms/crates/opendash-plugin-sdk/src/types.rs#L112-L129); cập nhật `MockOpRequest::ContentSave`, `dispatch_call` và `abi_host_call` tại [`mock.rs`](../../cms/crates/opendash-plugin-sdk/src/mock.rs) để lưu/cập nhật `locale`; bổ sung test assert round-trip `locale` tại [`lib.rs#L155-L220`](../../cms/crates/opendash-plugin-sdk/src/lib.rs#L155-L220) và đồng bộ spec tại [`plugin-dual-sdk-and-examples.md`](../../docs/designs/plugin-dual-sdk-and-examples.md). |
| **F-03** | Minor | **OPEN (Out-of-Scope Non-Blocking Minor)** | Macro `export_plugin!` gọi `log(LogLevel::Error, &err.to_string())` trước khi trả exit `1` ([`lib.rs#L31-L37`](../../cms/crates/opendash-plugin-sdk/src/lib.rs#L31-L37)), trong khi runtime [`plugin/worker.rs#L262-L275`](../../cms/crates/opendash-server/src/plugin/worker.rs#L262-L275) của `opendash-server` bỏ `staged_logs` khi `exit_code != 0`. Giữ nguyên ngoài phạm vi Spec 3 (`Guest SDK + Examples`); có thể xem xét giữ log lỗi ở `worker.rs` trong bản hardening runtime riêng. |
| **F-04** | Minor | **RESOLVED** | [`mock.rs#L70`](../../cms/crates/opendash-plugin-sdk/src/mock.rs#L70) (`KvList`) và [`mock.rs#L244`](../../cms/crates/opendash-plugin-sdk/src/mock.rs#L244) (`TaxonomyList`) đổi sang `.clamp(1, 100)` khớp [`host_ops.rs#L790`](../../cms/crates/opendash-server/src/plugin/host_ops.rs#L790) & `runtime.rs#L1096-L1103` (`TaxonomyTerm` không lưu `locale` trong DTO guest nên giữ `_locale` là hợp lệ). |
| **F-05** | Minor | **RESOLVED** | `handle_after_publish` trong [`rust-seo-plugin/src/lib.rs#L114-L137`](../../cms/examples/plugins/rust-seo-plugin/src/lib.rs#L114-L137) bổ sung fallback gọi `content_get(type_key, entry_id, env.locale.as_deref())` khi `env.payload` không có `"slug"` (như sự kiện `ContentAfterPublish` thực tế từ `opendash-server`); đồng bộ mô tả `brand_name` & `format_number` trong Spec 3. |
| **F-06** | Minor | **RESOLVED** | [`test_sdk_v11.mjs#L7-L9`](../../cms/sdk/as/test_sdk_v11.mjs#L7-L9) nhận `process.argv[2]` tùy chọn với fallback mặc định `../../target/opendash_sdk_test.wasm`, khớp Spec 3 line 500; đồng bộ lệnh `asc` trong Spec 3. |
| **N-01** | Nit | **RESOLVED** | [`sample-theme-v3/single.html#L1-L3`](../../cms/examples/themes/sample-theme-v3/single.html#L1-L3) dùng truy cập 2 bước `{% set current_locale = locale | default({}) %}` / `{{ current_locale.code | default('vi') }}`, an toàn dưới `UndefinedBehavior::Strict` ở mọi ngữ cảnh render. |
| **N-02** | Nit | **RESOLVED** | `rust-seo-plugin/src/lib.rs#L152` đặt `timeout_ms: Some(2000)` đúng trần `PLUGIN_ABI.md`; đồng bộ các tham chiếu dòng (`host_ops.rs:243-260`, `MockHost::with_initial_resp_buf_cap`) trong Spec 3. |
| **N-03** | Nit | **OPEN (Non-Blocking Nit)** | AS `Envelope.locale` dùng kiểu `string` (`""` khi `null`) khác `Option<String>` bên Rust; nằm trong thiết kế tương thích AssemblyScript v1/v2. |
| **N-04** | Nit | **RESOLVED** | Cập nhật Spec 3 `Status: Verified`, S03 PR `#545` (`6bcd3d5`) và [`docs/designs/SITEMAP.md#L29`](../../docs/designs/SITEMAP.md#L29). |

---

## 3. Kết luận

- **Trạng thái cuối cùng:** **`PASS`** (`0 Blocker · 0 Major · 1 Minor · 1 Nit` non-blocking)
- Đủ điều kiện đóng EGM Spec 3 và nâng [`docs/designs/plugin-dual-sdk-and-examples.md`](../../docs/designs/plugin-dual-sdk-and-examples.md) từ `Implemented` lên **`Verified`** cùng [`docs/verification/2026-11-24-spec3-dual-sdk-closure.md`](../../docs/verification/2026-11-24-spec3-dual-sdk-closure.md).
