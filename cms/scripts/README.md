# Build, kiểm chuẩn và kiểm thử (`cms/scripts/`)

Xem [lệnh chạy các bộ kiểm thử](../../README.md#kiểm-chứng-tại-workspace). Các script smoke/E2E dùng `OPENDASH_SITE_ROOT` / thư mục tạm riêng, không ghi đè dữ liệu SQLite hay `uploads/` thực của dự án.

- **Build & health:** `build.ps1` (cài npm lockfiles, build Admin + `sample-plugin` WASM, build Cargo `--locked`; dùng `-Release` cho bản phân phối), `build-release-linux.sh`, `verify-release-artifact.py`, `check-health.ps1`, `test_dev_launcher.py`, `demo-website-runner.py`.
- **Kiểm chuẩn tự động & CodeGraph:** `check-standards.py`, `check-plans.py`, `check-i18n.mjs`, `codegraph-index-docs.mjs`, `codegraph-query.mjs`.
- **Smoke API / DB nền:** `windows_smoke.py`, `windows_auth_smoke.py`, `make_baseline_fixture.py`, `content_types_smoke.py`, `content_entries_smoke.py`, `media_smoke.py`, `public_content_smoke.py`.
- **Live Browser E2E đại diện:** `admin_i18n_multilingual_e2e.py`, `admin_nav_single_edit_e2e.py`, `general_settings_browser.py`, `windows_users_browser.py`, `egm_cpt_builder_s01_e2e.py`, `egm_pattern_s02_migration.py`, `egm_pattern_library_s03_e2e.py`, `m4_s11_t01_page_builder_e2e.py`, `m4_s11_t02_sections_e2e.py`, `m5_beta_theme_v2_e2e.py`, `m5_s12_t01_library_e2e.py`, `m6_s10_t01_admin_extensions_e2e.py`, `m7_beta_menu_picker_e2e.py`, `m7_s16_t02_navigation_e2e.py`, `m7_s16_t06_entry_table_e2e.py`, `m8_browser_e2e.py`.
