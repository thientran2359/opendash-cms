# Bản địa hóa Giao diện Admin (`cms_locale`), Tách biệt Cài đặt Đa ngôn ngữ & Công cụ Dọn Dữ liệu Mồ côi — EGM Closure Verification (`#548`–`#552`, 2026-11-25)

- **Design Spec:** [`docs/designs/admin-i18n-and-multilingual-settings.md`](../designs/admin-i18n-and-multilingual-settings.md) (`source_sha: b21ff98ac95ecf2fa2490bc129b129f3d3a6ebe9`)
- **Drift Report:** [`.agents/research/2026-11-25-admin-i18n-drift-report.md`](../../.agents/research/2026-11-25-admin-i18n-drift-report.md) (Goldfish `dd3c4b79` đối chiếu `main@38df364` -> `0 Blocker · 1 Major · 5 Minor · 1 Nit`; sau các bản vá `F-01`–`F-04` & `F-07` trên nhánh closure: **`0 Blocker · 0 Major · 2 Minor (pre-existing) · 0 Nit`**)
- **Merged Implementation Slices (`origin/main@38df364e9b45bbd0cf5ac7683fb65cb22ebbdd6a`):**
  - Slice S01 (`#548`): PR `#554` / `#553` (`195f388740b19c7387863fcdac383b582b689603`) — Migration `0051`, PRD sync, `CMS_LANGUAGE_CATALOG` (32 ngôn ngữ), `/api/i18n/admin`, middleware `localize_error_response` & 18 sửa chuỗi mojibake
  - Slice S02 (`#549`): PR `#555` (`d9b5622032a61e0b537a4559664e4ce8151d48c4`) — Toggle `multilingual_enabled` bảo toàn DB, soft-delete/revive (`deleted_at`), `parked_redirects` & Purge tô-pô nguyên tử trên 12 storage modules
  - Slice S03 (`#550`): PR `#556` (`9244d9a9e8dc5c4d2c1640dd360ea9805393bc95`) — Hạ tầng React Admin i18n (`I18nProvider`/`tStatic`), `en`/`vi`, `General -> CMS Language`, `#/settings/multilingual` & `check-i18n.mjs`
  - Slice S04 (`#551`): PR `#557` (`f6ddf1f657da16ca759a731ee8d66d656b09fb8e`) — Di chuyển 100% `18` `api/`, `39` `features/`, `7` `studio/`, `13` `builder/` & bật mặc định `--strict` (`2391` khóa)
  - Slice S05 (`#552`): PR `#558` (`38df364e9b45bbd0cf5ac7683fb65cb22ebbdd6a`) — Đồng bộ `ADMIN_NAVIGATION_AND_EDIT_SCREEN.md`, `M8_WRITER_INVENTORY.md`, tạo `admin_i18n_multilingual_e2e.py` & cập nhật bộ Playwright/smoke scripts
  - Drift Fixes + Verified Closure: nhánh `fix/admin-i18n-verified-closure` (vá `F-01`, `F-02`, `F-03`, `F-04`, `F-07`)
- **Environment:** macOS `15.8.2` Intel (`x86_64`), Chrome `158.0.8127.37`, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, Node `v24.22.2`
  - Binary tested with Live Chrome E2E & representative suite: `cms/target/release/opendash` (SHA-256 `21d940cb13b681c36aac8fe281c41b65b4e306304ed9e438a6a8929dd93ecffc`; SHA-256 sau `cargo test --release --locked`: `8d0ec2fe3df8fd68d3ef32f6524efa190302d456e7f24effb80f5317743c26a1`)
- **Verifiers:** Orion (Coordinator) + fresh-context Goldfish `egm-codebase-details` (`dd3c4b79-a12b-41c6-9353-221b0dec9242`) + GitHub Actions CI
- **Overall Result:** **`PASS`** — nâng [`docs/designs/admin-i18n-and-multilingual-settings.md`](../designs/admin-i18n-and-multilingual-settings.md) trên [`SITEMAP.md`](../designs/SITEMAP.md) từ `Implemented` lên **`Verified`**.

---

## 1. Automated Verification Sau Sửa Drift (`fix/admin-i18n-verified-closure`)

| Check / Command | Scope | Result | Observed Evidence |
| --- | --- | --- | --- |
| `npm --prefix cms/admin run typecheck` | TypeScript toàn React Admin | **PASS** | `tsc --noEmit` exit `0` |
| `npm --prefix cms/admin run build` | Bundle nhúng `rust-embed` | **PASS** | `✓ 123 modules transformed`, built in `2.08s` |
| `node cms/scripts/check-i18n.mjs` | Parity `en.ts` ↔ `vi.ts`, placeholders & quét 100% file `--strict` | **PASS** | `i18n --strict: 2424 translation keys, 99 files checked.` |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | Formatting Rust toàn workspace | **PASS** | Exit `0` |
| `TMPDIR=<canonical_realpath> cargo test --manifest-path cms/Cargo.toml -p opendash-server --bin opendash localization_middleware_tests --release --locked` | Unit test middleware L10n gồm unsized error stream (`F-02`) | **PASS** | `5 passed; 0 failed; 0 ignored` (`232 filtered out`, `0.00s`) |
| `TMPDIR=<canonical_realpath> cargo test --manifest-path cms/Cargo.toml --workspace --release --locked` | Toàn bộ unit & integration tests Rust (gồm `0051`, `admin_i18n_s01_test`, `admin_i18n_s02_multilingual_purge_test`, `multilingual*`, `m8_s01_t02_writer_barrier`) | **PASS** | **`91` targets · `507` passed · `0` failed · `0` ignored** |
| `../.venv-e2e/bin/python3 cms/scripts/admin_i18n_multilingual_e2e.py --binary cms/target/release/opendash` | Live Chrome E2E CUJ-1 → CUJ-4 | **PASS** | `PASS CUJ-1`, `PASS CUJ-2`, `PASS CUJ-3`, `PASS CUJ-4` |
| `../.venv-e2e/bin/python3 cms/scripts/general_settings_browser.py --binary cms/target/release/opendash` | Live Chrome Cài đặt chung (EN default, keyboard, 390px, 412, restart) | **PASS** | `PASS: macOS Settings Chromium, keyboard, 390px, 412 draft, retry, restart` |
| `../.venv-e2e/bin/python3 cms/scripts/windows_auth_smoke.py --binary cms/target/release/opendash --baseline-db cms/fixtures/baseline_0001.sqlite` | Auth, RBAC, CSRF (`/api/admin/themes`), upgrade 0001→0051 | **PASS** (scoped) | `PASS: Auth, RBAC, CSRF, races and standard upgrade; pre-0009 rollback SKIPPED (use --baseline-binary)` |
| `../.venv-e2e/bin/python3 cms/scripts/windows_users_browser.py --binary cms/target/release/opendash --browser "...Google Chrome"` | Users UI desktop/390px, roles, stale draft, revocation | **PASS** | `PASS: Users browser desktop/390px, roles, stale draft, reset, keyboard, session revocation` |
| `../.venv-e2e/bin/python3 cms/scripts/admin_navigation_e2e.py --binary cms/target/release/opendash` | Điều hướng Admin 23 màn hình gồm `#/settings/multilingual` & 403 Editor | **PASS** | `"status": "PASS"`, `35` sub-checks (`chrome_page_errors_none`) |
| `../.venv-e2e/bin/python3 cms/scripts/entry_table_e2e.py --binary cms/target/release/opendash` | Entry table, filters, bulk CAS, locale persistence | **PASS** | `"status": "PASS"`, `21` sub-checks (`error: null`) |
| `python3 cms/scripts/check-standards.py` (+ `--self-test`), `check-plans.py` (+ `--self-test`), `check-agent-artifacts.py`, `git diff --check` | Ranh giới kiến trúc, CSRF/Origin inventory, markdown links & SITEMAP | **PASS** | `91` registrations resolved (`87` direct + `1` delegated + `1` public Origin + `2` bootstrap), `13` fixtures PASS, `check-agent-artifacts.py` & `git diff --check` exit `0` |

---

## 2. Xác nhận Khắc phục Drift (`F-01`–`F-04`, `F-07`)

1. **`F-01` (`Major` -> RESOLVED):** Bổ sung khóa dịch `feature.taxonomy.taxonomypage.parent_prefix` (`"Parent: {{parent}}"` / `"Cha: {{parent}}"`) & `.extensions` thay thế `"Cha: ..."` trong [`TaxonomyPage.tsx`](../../cms/admin/src/features/taxonomy/TaxonomyPage.tsx), đồng thời thay toàn bộ `'vi'` hardcode trong `localeCompare`/`toLocaleLowerCase` bằng `getEffectiveBcp47()` trên 4 module taxonomy.
2. **`F-02` (`Minor` -> RESOLVED):** Chỉnh điều kiện `size_hint().upper()` tại [`cms/crates/opendash-server/src/i18n.rs`](../../cms/crates/opendash-server/src/i18n.rs) sang `.is_some_and(|n| n > MAX_LOCALIZED_ERROR_BYTES as u64)` để stream không rõ độ dài vẫn đi qua bộ đệm có chặn `1 MiB` (`ErrorBodyReplay`), kèm unit test mới `error_body_limit_buffers_unsized_streams_until_cap`.
3. **`F-03`, `F-04`, `F-07` (`Minor`/`Nit` -> RESOLVED):** Dịch `studio.copy_options`/`paste_options` trong `cms/admin/src/i18n/locales/vi.ts` (nay đã chuyển sang [`vi.po`](../../cms/l10n/vi.po)); chuẩn hóa toàn bộ nhãn/placeholder còn sót ở 13 file React (`2424` khóa cân bằng 100%); đồng bộ `'Đã chèn bản sao'` trong `cms/scripts/template_library_e2e.py` *(retired)*.

---

## 3. Limits & Honest Notes

1. **Legacy `windows_auth_smoke.py` pre-0009 rollback (`SKIPPED`):** Chạy với `--baseline-db cms/fixtures/baseline_0001.sqlite` mà không truyền `--baseline-binary` bản nhị phân trước migration 0009, nên riêng nhánh rollback synthetic pre-0009 được script bỏ qua có chủ đích.
2. **Legacy M1–M4 selector/version scripts (`F-05`) & PRD §3 nav table (`F-06`):** Giữ nguyên theo ranh giới đã ghi nhận trong [`.agents/research/2026-11-25-admin-i18n-drift-report.md`](../../.agents/research/2026-11-25-admin-i18n-drift-report.md); ngoài `5` script đại diện đã chạy thực tế + `template_library_e2e.py` đã sửa casing, các script lịch sử khác ngoài danh sách đại diện chỉ được Harbor rà static ở S05.
3. **CodeGraph native docs indexer (`NOT RUN (tooling)`):** Trên máy macOS này kiểm tra non-engine qua `node cms/scripts/codegraph-index-docs.mjs --list-sources`; indexing DB đầy đủ phụ thuộc binary `@astudioplus/codegraph-mcp`.
4. **Cross-platform CI (`Windows`/`Linux`):** Toàn bộ kiểm chứng Windows & Linux chạy tự động trên GitHub Actions CI (`OpenDash CMS Build & Test`, `OpenDash Governance`) trên PR closure trước khi merge vào `main`.
