# Build, Standards, CodeGraph & Test Scripts (`cms/scripts/`)

## Overview

`cms/scripts/` contains **31** operational scripts covering workspace builds, release packaging, architecture/documentation/i18n gates, CodeGraph indexing/querying, API/SQLite smoke checks, and Playwright live-browser E2E suites. See also the root [workspace verification commands](../../README.md#kiểm-chứng-tại-workspace).

## Interface

### Build, Packaging & Health (6)

| Script | Purpose & CLI Interface |
| --- | --- |
| [`build.ps1`](build.ps1) | Windows build pipeline (`[-Release]`): runs `npm ci && npm run build` in `cms/admin` and `cms/examples/plugins/sample-plugin`, then `cargo build --locked -p opendash-server`. |
| [`build-release-linux.sh`](build-release-linux.sh) | Builds reproducible-input `x86_64-unknown-linux-musl` release bundle (`[--out-dir <dir>] [--skip-npm-ci]`) with `opendash`, `deploy/`, `release-manifest.json`, and `checksums.txt`. |
| [`verify-release-artifact.py`](verify-release-artifact.py) | Verifies release bundle manifest, SHA-256 checksums, secret/runtime file hygiene, static ELF linking on Linux, and `--check-config` (`verify_bundle`, `run_self_test`; accepts `<bundle_dir>` or `--self-test`). |
| [`check-health.ps1`](check-health.ps1) | Executes a single `Invoke-RestMethod` GET check against `http://127.0.0.1:<Port>/api/health` (`[-Port 18766]`) and asserts `status == "ok"`, `database == "ready"`, `scaffold == false`. |
| [`test_dev_launcher.py`](test_dev_launcher.py) | `unittest` suite (`DevLauncherTests`) covering site-root resolution and isolation in root `dev.py`. |
| [`demo-website-runner.py`](demo-website-runner.py) | Isolated "Mộc Studio" demo site harness (`seed \| smoke \| capture \| all \| serve \| reset`) writing only under `runtime/demo-website/` and `screenshots/demo-website/`. |

### Standards, Documentation, i18n & CodeGraph (5)

| Script | Purpose & CLI Interface |
| --- | --- |
| [`check-standards.py`](check-standards.py) | Static architecture boundary checker (pure Python 3.11+ stdlib; supports `--self-test`), enforcing layer rules across [`cms/crates/`](../Cargo.toml) and `cms/admin/`. |
| [`check-plans.py`](check-plans.py) | Validates relative Markdown links, [`docs/designs/SITEMAP.md`](../../docs/designs/SITEMAP.md) coverage/provenance, and `docs/verification/` evidence links (`[--self-test]`). |
| [`check-i18n.mjs`](check-i18n.mjs) | Validates Admin locale catalog parity (`[--strict \| --mode=s03]`) in `cms/admin/src/i18n/locales/`. |
| [`codegraph-index-docs.mjs`](codegraph-index-docs.mjs) | Indexes canonical contracts, ADRs, and `docs/designs/*.md` into local CodeGraph (`[--list-sources]` works without the engine binary). |
| [`codegraph-query.mjs`](codegraph-query.mjs) | CLI wrapper for CodeGraph queries: `symbol <query>`, `callgraph\|callers\|callees <file> <line>`, `docs <query>`, `entrypoints`. |

### API, Auth, Media & Migration Smoke Tests (7)

All smoke scripts launch isolated server instances (ephemeral loopback ports and `TemporaryDirectory` DB/site roots) via `running(...)` or apply SQLite migrations in memory, accepting `--binary <path>` where a server executable is required:

- [`windows_smoke.py`](windows_smoke.py) — Core HTTP v3 theme, entry builder, retired M3–M5 `404` checks, and launcher helper (`running`, `sample_theme`).
- [`windows_auth_smoke.py`](windows_auth_smoke.py) — Auth, RBAC, session expiry, CSRF, concurrent setup, and DB upgrade tests.
- [`make_baseline_fixture.py`](make_baseline_fixture.py) — Generates or verifies `cms/fixtures/baseline_0001.sqlite` (`create_baseline_fixture`).
- [`content_types_smoke.py`](content_types_smoke.py) — CPT API CRUD & SQLite smoke (`running`, `login`, `request`).
- [`content_entries_smoke.py`](content_entries_smoke.py) — Content entry CRUD, concurrency, and media wiring smoke (`api`).
- [`media_smoke.py`](media_smoke.py) — Media library upload/PNG helper and lifecycle smoke (`make_png`, `upload_image`).
- [`egm_pattern_s02_migration.py`](egm_pattern_s02_migration.py) — In-memory SQLite migration test applying `0001`–`0043` then `0044` (`apply_through`) to verify Section-to-Pattern upgrade integrity.

### Live Browser E2E Suites (13)

Require Python Playwright and Chromium/Chrome (`--binary <path>`, plus script-specific `--artifact-dir` / `--browser` flags):

- [`admin_i18n_multilingual_e2e.py`](admin_i18n_multilingual_e2e.py) — Admin i18n and multilingual settings (`0051`) browser journeys.
- [`admin_nav_single_edit_e2e.py`](admin_nav_single_edit_e2e.py) — Bounded Chrome Admin navigation, RBAC, and single-edit acceptance.
- [`egm_cpt_builder_s01_e2e.py`](egm_cpt_builder_s01_e2e.py) — CPT builder browser smoke on an isolated site.
- [`egm_pattern_library_s03_e2e.py`](egm_pattern_library_s03_e2e.py) — Pattern library, Studio Mode 2 Pattern (`#/studio/entry/pattern/:id`), and RBAC Chrome + API E2E suite.
- [`entry_lifecycle_s02_e2e.py`](entry_lifecycle_s02_e2e.py) — Entry Trash/Restore/Purge, Reading guard, dormant retained-Trash maintenance, Editor vs Admin RBAC, Studio/Editor trashed guards, and plugin host confirmation Chrome E2E suite.
- [`general_settings_browser.py`](general_settings_browser.py) — General Settings (`SITE-01`) embedded Admin browser test.
- [`m4_s11_t01_page_builder_e2e.py`](m4_s11_t01_page_builder_e2e.py) — Page builder Studio Mode 2 E2E acceptance (`ui_create_page`, `new_heading`, `change_heading`).
- [`m5_s12_t01_library_e2e.py`](m5_s12_t01_library_e2e.py) — Local template library, Pattern CRUD (`create_ui_pattern`), and Studio override E2E test.
- [`m6_s10_t01_admin_extensions_e2e.py`](m6_s10_t01_admin_extensions_e2e.py) — Plugin provider Admin columns, actions, and term panels E2E.
- [`m7_beta_menu_picker_e2e.py`](m7_beta_menu_picker_e2e.py) — Admin menu picker E2E and shared launcher/theme helpers (`launch_server`, `stop_server`, `admin_login`, `api_json`, `set_cms_locale`, `theme_zip`).
- [`m7_s16_t02_navigation_e2e.py`](m7_s16_t02_navigation_e2e.py) — Grouped Admin routes and one-screen-per-destination E2E.
- [`m7_s16_t06_entry_table_e2e.py`](m7_s16_t06_entry_table_e2e.py) — Entry tables, bulk CAS, URL redirect history, and mobile viewport E2E.
- [`m8_browser_e2e.py`](m8_browser_e2e.py) — Backup → restore → re-auth → activate → re-edit → Save → public E2E.
- [`windows_users_browser.py`](windows_users_browser.py) — Embedded Admin Users UI test at desktop and 390px viewports.

## Invariants

- **Site data isolation:** Smoke and E2E scripts must use `TemporaryDirectory` (`OPENDASH_SITE_ROOT` / `OPENDASH_DB_PATH`) and ephemeral loopback ports; they must never write to root `database/`, `themes/`, `plugins/`, `uploads/`, or target a running owner server.
- **Deterministic builds:** [`build.ps1`](build.ps1) and [`build-release-linux.sh`](build-release-linux.sh) use `npm ci` (unless `--skip-npm-ci` is passed) and `cargo --locked` against [`cms/Cargo.lock`](../Cargo.lock), [`cms/admin/package.json`](../admin/package.json), and [`cms/examples/plugins/sample-plugin/package.json`](../examples/plugins/sample-plugin/package.json).
- **No external runtime deps for core gates:** [`check-standards.py`](check-standards.py), [`check-plans.py`](check-plans.py), and [`verify-release-artifact.py`](verify-release-artifact.py) rely strictly on Python 3.11+ stdlib; [`check-i18n.mjs`](check-i18n.mjs) relies only on Node.js stdlib.

## Side Effects

- [`build.ps1`](build.ps1) and [`build-release-linux.sh`](build-release-linux.sh) install `node_modules` and write build artifacts into `cms/admin/dist/`, `cms/examples/plugins/sample-plugin/`, and `cms/target/`.
- [`demo-website-runner.py`](demo-website-runner.py) creates disposable site trees under `runtime/demo-website/` and screenshots under `screenshots/demo-website/`.
- [`make_baseline_fixture.py`](make_baseline_fixture.py) writes `cms/fixtures/baseline_0001.sqlite` when explicitly executed to regenerate the fixture.
- [`codegraph-index-docs.mjs`](codegraph-index-docs.mjs) updates the local CodeGraph markdown index store.

## Verification

Run core static and self-test checks from the repository root:

```powershell
python cms/scripts/check-standards.py
python cms/scripts/check-standards.py --self-test
python cms/scripts/check-plans.py
python cms/scripts/check-plans.py --self-test
python cms/scripts/verify-release-artifact.py --self-test
node cms/scripts/check-i18n.mjs --strict
node cms/scripts/codegraph-index-docs.mjs --list-sources
python -m unittest cms/scripts/test_dev_launcher.py
```
