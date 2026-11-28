# Spec 3: Dual Guest SDK (`AssemblyScript` & `opendash-plugin-sdk`) & Reference Examples — EGM Closure Verification (`#528`–`#530`, 2026-11-24)

- **Design Spec:** [`docs/designs/plugin-dual-sdk-and-examples.md`](../designs/plugin-dual-sdk-and-examples.md)
- **Drift Report:** [`.agents/research/2026-11-24_SPEC3_DUAL_SDK_DRIFT_REPORT.md`](../../.agents/research/2026-11-24_SPEC3_DUAL_SDK_DRIFT_REPORT.md) (Goldfish `e139fb38` đối chiếu `6bcd3d5` -> `1 Blocker · 1 Major · 4 Minor · 4 Nit`; sau các bản vá trước closure: **`PASS: 0 Blocker · 0 Major · 1 Minor · 1 Nit`**)
- **Merged Implementation PRs (`origin/main@6bcd3d5`):**
  - Slice S01 (`#528`): PR `#538` (`9d5c00373fdfd1c6203286cabaa367d4ef62349d`) — AssemblyScript SDK v1.1 (`-2` readback, `readEnvelope`, 15 Host Ops, 7 output helpers)
  - Slice S02 (`#529`): PR `#539` (`5e4c2fbc4efe174781f3851bc1f3aff00a4b6475`) — Rust Guest SDK `opendash-plugin-sdk` (`edition = "2024"`, `0` deps ngoài mới, `MockHost`, `export_plugin!`)
  - Slice S03 (`#530`): PR `#545` (`6bcd3d59dd14a9d235020d89af10817bf51ab256`) — Reference Examples `rust-seo-plugin` + `sample-theme-v3` & `walk_v3_dir` `README.md` ignore
  - Drift Fixes + Verified Closure: nhánh `fix/spec3-drift-fixes-and-verified-closure` (vá `F-01`, `F-02`, `F-04`, `F-05`, `F-06`, `N-01`, `N-02`, `N-04`)
- **Environment:** macOS Intel (`x86_64`), release binary `cms/target/release/opendash` (SHA-256 trên `6bcd3d5`: `f8e3f2fb5dc641f54c6b106080fb3c812e9d70285b1367e709b34e208c4c77c8`), `rustc 1.99.1`, `Node v24.22.2`
- **Verifiers:** Orion (Coordinator) + fresh-context Goldfish `egm-codebase-details` (`e139fb38-c636-4a28-9078-05517c56fb28`) + GitHub Actions CI
- **Overall Result:** **`PASS`** — nâng [`docs/designs/plugin-dual-sdk-and-examples.md`](../designs/plugin-dual-sdk-and-examples.md) từ `Implemented` lên **`Verified`**.

---

## 1. Automated Verification Sau Sửa Drift (`fix/spec3-drift-fixes-and-verified-closure`)

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `npm --prefix cms/examples/plugins/sample-plugin run asbuild` | Tương thích ngược plugin v1 AssemblyScript với `opendash.ts` v1.1 | **PASS** | Exit `0` |
| `./cms/examples/plugins/sample-plugin/node_modules/.bin/asc cms/sdk/as/test_harness.ts --outFile cms/target/opendash_sdk_test.wasm --bindings raw --optimize --lib cms/examples/plugins/sample-plugin/node_modules` + `node cms/sdk/as/test_sdk_v11.mjs cms/target/opendash_sdk_test.wasm` | AS SDK v1.1 (`-2` readback, `Envelope` v1/v2, 15 Host Ops, 7 output helpers, CLI argv `F-06`) | **PASS** | `PASS test_sdk_v11.mjs: all ABI v1.1 (-2 readback), Envelope v1/v2, 15 Host Ops & output helper checks passed.` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` & `cargo fmt --manifest-path cms/examples/plugins/rust-seo-plugin/Cargo.toml --all --check` | Formatting Rust toàn workspace & standalone crate `rust-seo-plugin` | **PASS** | Exit `0` |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/Cargo.toml -p opendash-plugin-sdk --release --locked` | Unit test Rust Guest SDK (`MockHost`, `-2` readback, CAS, `ContentSaveInput.locale` `F-02`, 15 ops) | **PASS** | `4 passed; 0 failed; 0 ignored` (`0.00s`) |
| `TMPDIR=/private/tmp cargo test --manifest-path cms/examples/plugins/rust-seo-plugin/Cargo.toml --target-dir /private/tmp/rust-seo-target --release` | Unit test plugin mẫu `rust-seo-plugin` (`manifest_schema: 3`, `filter:seo/head`, `filter:entry/view`, `content/after_publish` + fallback `content_get` `slug`) | **PASS** | `4 passed; 0 failed; 0 ignored` (`0.00s`) |
| `./cms/target/release/opendash --check-theme cms/examples/themes/sample-theme-v3` | Offline `--check-theme` CLI kiểm tra C-SEC & manifest v3 của `sample-theme-v3` (sau fix Strict `locale` `N-01`) | **PASS** | Exit `0`; `{"ok":true,"theme_id":"sample-theme-v3","version":"1.0.0","templates":["single"],"parts":[],"page_layouts":[],"patterns":[],"blocks":[],"assets":["assets/app.css"],"options_count":0}` |
| `python3 cms/scripts/check-standards.py` + `--self-test` | Ranh giới kiến trúc, zero new deps ở `opendash-core` & CSRF | **PASS** | Exit `0`, `88/88` mutation handlers resolved (`84` direct + `1` delegated + `1` public Origin + `2` bootstrap exemptions); `43 + 16 + 10` negative regressions PASS |
| `python3 cms/scripts/check-plans.py` + `--self-test` | Liên kết Markdown tương đối, `SITEMAP.md` `Verified` links & self-tests | **PASS** | Exit `0`, `13` fixtures (`valid + 12 failures`) PASS |
| `python3 .agents/scripts/check-agent-artifacts.py` + `git diff --check` | Metadata `.agents/` và kiểm tra whitespace | **PASS** | Exit `0` |

---

## 2. Xác nhận Khắc phục Drift (`F-01`, `F-02`, `Minor`/`Nit`)

1. **`F-01` (`Blocker` -> RESOLVED):** Đồng bộ [`cms/sdk/README.md`](../../cms/sdk/README.md) để ghi rõ `contentGet()` trả chuỗi JSON envelope đầy đủ (`{"ok":true,"data":{"entry":{...}|null}}`) giống `contentList()`/`taxonomyList()` và khớp [`cms/sdk/as/opendash.ts`](../../cms/sdk/as/opendash.ts).
2. **`F-02` (`Major` -> RESOLVED):** Bổ sung `pub locale: Option<String>` vào `ContentSaveInput` ([`types.rs`](../../cms/crates/opendash-plugin-sdk/src/types.rs)) và `MockHost` ([`mock.rs`](../../cms/crates/opendash-plugin-sdk/src/mock.rs)) kèm test round-trip tại [`lib.rs`](../../cms/crates/opendash-plugin-sdk/src/lib.rs).
3. **`F-04`, `F-05`, `F-06`, `N-01`, `N-02`, `N-04` (`RESOLVED`):** Sửa `clamp(1, 100)` trên `MockHost` `KvList`/`TaxonomyList`; thêm `content_get` fallback cho `slug` và chỉnh `timeout_ms: Some(2000)` trong [`rust-seo-plugin/src/lib.rs`](../../cms/examples/plugins/rust-seo-plugin/src/lib.rs); hỗ trợ `process.argv[2]` trong `test_sdk_v11.mjs`; dùng truy cập 2 bước `current_locale = locale | default({})` trong [`sample-theme/single.html`](../../cms/examples/themes/sample-theme/single.html); đồng bộ spec & `SITEMAP.md`.

---

## 3. Limits & Honest Notes

1. **Build `wasm32-unknown-unknown` & `--check-plugin` trên `rust-seo-plugin.zip` (`NOT RUN` trên máy local):** Máy macOS hiện tại chỉ cài target `x86_64-apple-darwin` (chưa cài `wasm32-unknown-unknown`), nên bước biên dịch `rust_seo_plugin.wasm` và chạy `--check-plugin` trên `/tmp/rust-seo-plugin.zip` là **`NOT RUN`** trên máy này (theo đúng Spec 3 đây là kiểm chứng tùy chọn; toàn bộ unit tests `MockHost` của `rust-seo-plugin` và `opendash-plugin-sdk` đã pass trên bản `--release`).
2. **CodeGraph docs indexer (`NOT RUN (tooling)`):** `node cms/scripts/codegraph-index-docs.mjs` chưa chạy trên máy macOS do thiếu binary `@astudioplus/codegraph-mcp`.
3. **Full `--workspace` CI đa nền tảng:** Chạy tự động qua GitHub Actions CI (`OpenDash CMS Build & Test`, `OpenDash Governance`) trên PR closure này trước khi squash-merge vào `main`.
