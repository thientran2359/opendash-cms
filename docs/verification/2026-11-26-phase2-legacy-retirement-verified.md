# Phase 2 Legacy Retirement: Studio Pattern Cutover, M3–M5 Route Removal, Theme v3 Enforcement & PRD Sync — EGM Closure Verification (2026-11-26)

- **Design Spec:** [`docs/designs/phase2-legacy-builder-theme-v3-cleanup.md`](../designs/phase2-legacy-builder-theme-v3-cleanup.md)
- **Drift Report:** [`.agents/research/2026-11-26-phase2-legacy-retirement-drift-report.md`](../../.agents/research/2026-11-26-phase2-legacy-retirement-drift-report.md) (Goldfish `5b88c4d4` trên `42ab55f` -> `0 Blocker · 1 Major · 4 Minor · 3 Nit`; sau hardening `F-01`–`F-06` & `F-08`: **`0 Blocker · 0 Major · 0 Minor · 1 Nit benign`**)
- **Implementation Commits (`agent/nova`):**
  - `9e055c0` — Slices `S01`–`S04` (Docs/PRD Sync, Frontend Studio Pattern Cutover & Removal of `BuilderWorkspace`, Backend M3–M5 Removal & Theme v3 / Media ACL / `.complete` Hardening, Fixtures & Test Alignment)
  - `42ab55f` — Merge `origin/main@fbf50ee` tích hợp `theme-chrome-cache-and-safe-icons` & `trusted-theme-js-svg-and-asset-enqueue`
  - Closure Hardening (`agent/nova`) — Vá `F-01`–`F-06` từ Goldfish `/egm-drift-report` & hoàn thiện suite `admin_i18n_multilingual_e2e.py`
- **Environment:** `Windows-11-10.0.26200-SP0`, Chrome `158.0.8127.37`, Playwright Python `1.64.0`, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, Node `v22.16.0`
  - Release Binary verified (`cms/target/release/opendash.exe`):
    - `BUILD_SHA256`: `9522A39EBCDDEA561B37327E44DC20A61A4C856D820522F2BA672E963A303E51`
    - `POST_TEST_SHA256` (dùng chạy toàn bộ CLI, smoke & 8 kịch bản Playwright E2E): `08CF96DB4855667229F805A2758462612E06036432A8841DD4ABD6BB4653171F`
- **Verifiers:** Nova (Coordinator) + fresh-context Goldfish `egm-codebase-details` (`5b88c4d4-f75a-4179-9d14-c00728414532`)
- **Overall Result:** **`PASS`** — nâng [`docs/designs/phase2-legacy-builder-theme-v3-cleanup.md`](../designs/phase2-legacy-builder-theme-v3-cleanup.md) trên [`SITEMAP.md`](../designs/SITEMAP.md) từ `Implemented` lên **`Verified`**.

---

## 1. Automated Verification Sau Sửa Drift (`agent/nova` Closure)

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `npm --prefix cms/admin run typecheck` | TypeScript toàn React Admin | **PASS** | `tsc --noEmit` exit `0` |
| `npm --prefix cms/admin run build` | Bundle nhúng `rust-embed` | **PASS** | `✓ 121 modules transformed.`, `✓ built in 4.45s` |
| `node cms/scripts/check-i18n.mjs --strict` | Parity `en.ts` ↔ `vi.ts` & quét hardcoded string | **PASS** | `i18n --strict: 2302 translation keys, 96 files checked.` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | Formatting Rust toàn workspace | **PASS** | Exit `0` |
| `cargo build --manifest-path cms/Cargo.toml --release --locked` | Build release binary đơn file | **PASS** | Exit `0` (`BUILD_SHA256=9522A39E...`) |
| `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` | Toàn bộ unit & integration tests Rust (gồm `m8_s01_t02_writer_barrier`, `s03_media_acl_tests`, `theme_tests`, `m8_s08_t01_e2e`, `admin_i18n_s01_test`, `plugin_*`, `studio_*`, `wordpress_permalinks_test`, `theme_chrome_cache_icons_test`, `trusted_theme_js_svg_enqueue_test`) | **PASS** | **`suites=93` · `passed=549` · `failed=0` · `ignored=0`** |
| `cms/target/release/opendash.exe --check-theme cms/fixtures/admin-review/theme` | Kiểm tra fixture Theme v3 sau nâng cấp | **PASS** | `{"ok":true,"theme_id":"review-theme","version":"1.0.0","templates":["single"],...}` |
| `python cms/scripts/windows_smoke.py --binary cms/target/release/opendash.exe` | Smoke Theme v3 ZIP, preview isolation, CAS save, retired M3–M5 `404` & SQLite persistence | **PASS** | `PASS: Theme v3 ZIP install, entry preview isolation, CAS save, rejected inputs, retired M3-M5 404s, and SQLite persistence after restart` |
| `python cms/scripts/windows_auth_smoke.py --binary cms/target/release/opendash.exe --baseline-db cms/fixtures/baseline_0001.sqlite` | Auth, Argon2 concurrency, RBAC, CSRF/Origin, Users CRUD, nâng cấp `baseline_0001.sqlite` | **PASS** *(scoped)* | `PASS: Auth, RBAC, CSRF, races and standard upgrade; pre-0009 rollback SKIPPED (use --baseline-binary)` |
| `python cms/scripts/admin_i18n_multilingual_e2e.py --binary cms/target/release/opendash.exe` | Live Chrome E2E Admin UI `cms_locale`, chuyển ngôn ngữ & Purge đa ngôn ngữ (CUJ-1 → CUJ-4) | **PASS** | `PASS CUJ-1`, `PASS CUJ-2`, `PASS CUJ-3`, `PASS CUJ-4` |
| `python cms/scripts/egm_pattern_library_s03_e2e.py --binary cms/target/release/opendash.exe` | Pattern Library, Studio Mode 2 (`#/studio/entry/pattern/:id`), Admin RBAC, `#/builder` Pattern redirect | **PASS** | `"status": "PASS"`, 11 sub-checks, exit `0` |
| `python cms/scripts/m6_s10_t01_admin_extensions_e2e.py --binary cms/target/release/opendash.exe` | Admin Extensions với v3 `admin-review/theme`, CAS, RBAC, Theme/Plugin rollback | **PASS** | `"status": "PASS"`, 26 sub-checks, `"error": null` |
| `python cms/scripts/m7_s16_t02_navigation_e2e.py --binary cms/target/release/opendash.exe` | Điều hướng 23 màn hình Admin (`35` sub-checks, Chrome `158.0.8127.37`, `390px`, 403 Editor) | **PASS** | `"status": "PASS"`, `"chrome_page_errors_none"`, exit `0` |
| `python cms/scripts/m8_browser_e2e.py --binary cms/target/release/opendash.exe` | Backup `age`, Restore-to-new-root Safe Mode & Activation | **PASS** | `"scenario": "normal"`, `"last_phase": "all_assertions_pass"`, `console_errors: 0`, `page_errors: 0` |
| `python cms/scripts/egm_cpt_builder_s01_e2e.py --binary cms/target/release/opendash.exe` | CPT Builder Studio Mode 2 & toggle `supports.builder` | **PASS** | `"status": "PASS"`, `7` sub-checks, `chrome_js_errors: []` |
| `python cms/scripts/m4_s11_t01_page_builder_e2e.py --binary cms/target/release/opendash.exe` | Page Studio Builder, 401/412 draft recovery, media ACL, CSRF/Origin | **PASS** | `"status": "PASS"`, `16` sub-checks, `chrome_js_errors: []` |
| `python cms/scripts/m5_s12_t01_library_e2e.py --binary cms/target/release/opendash.exe` | Pattern copy, Studio overrides, retired `apply` `404` & typed `reset` `CAS` | **PASS** | `"status": "PASS"`, `22` sub-checks, `chrome_js_errors: []` |

---

## 2. Xác nhận Khắc phục Drift (`F-01`–`F-06`, `F-08`)

1. **`F-01` (`Major` -> RESOLVED):** Đồng bộ [`CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md) và [`CMS_ARCHITECTURE.md`](../architecture/CMS_ARCHITECTURE.md) khớp 100% số lượng thực tế trên đĩa (`0001–0052` / `51` migrations, `15` module `opendash-core`, `23` file `routes/`, `32` file `storage/` gồm `patterns` & `plugin_projections` và bỏ `sections`, `21` file `api/`, `17` thư mục `features/`, thêm `studio/` & `i18n/`).
2. **`F-02` (`Minor` -> RESOLVED):** Gỡ bỏ toàn bộ `OverrideRole`, `OverrideScope`, `candidate_for_legacy_scope`, `get_override`, `get_override_in_tx`, `save_override_and_render`, `reset_override`, `validate_scope_and_load_cpt_fields` trong [`storage/overrides.rs`](../../cms/crates/opendash-server/src/storage/overrides.rs) và thay bằng `validate_cpt_target_fields_in_tx`.
3. **`F-03` & `F-06` (`Minor` / `Nit` -> RESOLVED):** Gỡ `HomepageOpen` & `OverrideRole` khỏi [`api/builder.ts`](../../cms/admin/src/api/builder.ts) & [`api/index.ts`](../../cms/admin/src/api/index.ts) (giữ `HomepagePreview` & `BuilderDiagnostic` đang dùng cho `api/studio.ts`); mở rộng `source_format` trong [`api/themes.ts`](../../cms/admin/src/api/themes.ts) thành `'v1_legacy' | 'v2' | 'v3'`.
4. **`F-04` (`Minor` -> RESOLVED):** Gỡ `Theme` & `render()` thừa trong [`src/theme.rs`](../../cms/crates/opendash-server/src/theme/mod.rs); đồng bộ thông báo lỗi manifest sang `"Manifest theme v3 không hợp lệ."` và `theme.invalid_v3_theme_manifest` trong [`src/i18n.rs`](../../cms/crates/opendash-server/src/i18n.rs).
5. **`F-05` (`Minor` -> RESOLVED):** Cập nhật [`M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md) (`0001–0052`) và đồng bộ `required_handlers` trong [`m8_s01_t02_writer_barrier.rs`](../../cms/crates/opendash-server/tests/m8_s01_t02_writer_barrier.rs).
6. **`F-07` (`Nit` -> DOCUMENTED):** Giữ nguyên các tên nội bộ `V2_*` trong `package.rs` và guard loại trừ `"section"` làm lớp an toàn nâng cấp.

---

## 3. Limits & Honest Notes

1. **`windows_auth_smoke.py` pre-0009 rollback (`SKIPPED`):** Chạy với `--baseline-db cms/fixtures/baseline_0001.sqlite` mà không truyền `--baseline-binary`, nên nhánh synthetic pre-0009 rollback được bỏ qua theo thiết kế.
2. **Môi trường local Windows & Cross-platform CI:** Kiểm chứng thực thi cục bộ chạy trên Windows 11 + Chrome `158.0.8127.37`; phạm vi Linux/macOS/Windows CI chuẩn (`governance-*`, `cms-ci`) được kiểm chứng qua GitHub Actions trên PR merge vào `main`.
