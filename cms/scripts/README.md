# Build, Standards, CodeGraph & Test Scripts (`cms/scripts/`)

## Overview

`cms/scripts/` contains **20** operational scripts covering workspace builds, grouped fast Rust testing, release packaging, architecture/documentation/i18n gates, CodeGraph indexing/querying, shared E2E/benchmark helpers, canonical lifecycle benchmarks, and current Playwright live-browser E2E suites. See also the root [workspace verification commands](../../README.md#kiểm-chứng-tại-workspace).

## Interface

### Build, Packaging, Test Runner & Health (7)

| Script | Purpose & CLI Interface |
| --- | --- |
| [`run-tests.py`](run-tests.py) | Grouped & change-aware Rust test runner (`--list`, `--check`, `--changed [--base origin/main]`, `--group <theme\|builder\|content\|plugin\|i18n\|site-media-seo\|ops\|core\|all>`, `--filter <pattern>`, `--release`, `--dry-run`, `--self-test`). Maps changed files (with smart wiring-only detection) to domain test groups, auto-resolves macOS `TMPDIR`, and uses the fast `dev` profile (`[profile.dev.package."*"] opt-level = 2`) by default. |
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

### Shared Helpers, Baseline Fixture & Performance Benchmark (3)

Shared stdlib test utilities (`e2e_helpers.py`), baseline DB fixture generator, and deterministic lifecycle benchmark runner:

- [`e2e_helpers.py`](e2e_helpers.py) — Shared pure-stdlib helpers (`free_port`, `wait_for_health`, `launch_server`, `stop_server`, `api_json`, `set_cms_locale`, `admin_login`, `make_png`) imported by the kept E2E and benchmark suites.
- [`make_baseline_fixture.py`](make_baseline_fixture.py) — Generates or verifies `cms/fixtures/baseline_0001.sqlite` (`create_baseline_fixture`).
- [`entry_lifecycle_benchmark.py`](entry_lifecycle_benchmark.py) — Canonical S04 baseline-vs-candidate performance benchmark (`--baseline-binary`, `--candidate-binary`, `--artifact-dir`, optional `--sizes`) verifying NFR gates P1–P3 across deterministic 1k/10k/50k fixtures.

### Live Browser E2E Suites (5)

Require Python Playwright and Chromium/Chrome (`--binary <path>`, plus script-specific `--artifact-dir` / `--browser` flags):

- [`admin_i18n_multilingual_e2e.py`](admin_i18n_multilingual_e2e.py) — Admin i18n and multilingual settings (`0051`) browser journeys.
- [`entry_lifecycle_trash_e2e.py`](entry_lifecycle_trash_e2e.py) — Entry Trash/Restore/Purge, Reading guard, dormant retained-Trash maintenance, Editor vs Admin RBAC, Studio/Editor trashed guards, and plugin host confirmation Chrome E2E suite.
- [`entry_lifecycle_quick_edit_e2e.py`](entry_lifecycle_quick_edit_e2e.py) — Inline Quick Edit, site-timezone `published_at`, single-taxonomy delta, 412 diff + explicit rebase, simultaneous Single Edit + Quick Edit 401/Cancel isolation, trashed-target 409 guard, and 390px mobile Chrome E2E suite.
- [`entry_lifecycle_bulk_actions_e2e.py`](entry_lifecycle_bulk_actions_e2e.py) — Bounded current-page Bulk Actions (broadened `draft_publish` CPT publication, single-taxonomy add/remove/replace, mixed-locale block, Trash/Restore/Admin-only Purge, mixed 200/412 + uncertain reconciliation without replaying successes, EN/VI, 390px mobile) Chrome E2E suite.
- [`manual_redirects_404_e2e.py`](manual_redirects_404_e2e.py) — Manual redirects (301/302), CSV dry-run/import/BOM export, aggregated 404 log, 1-click redirect, `401`/`412` draft recovery, RBAC, and `en`/`vi`/mobile Chrome E2E.

## Invariants

- **Site data isolation:** Benchmark and E2E scripts must use `TemporaryDirectory` (`OPENDASH_SITE_ROOT` / `OPENDASH_DB_PATH`) and ephemeral loopback ports; they must never write to root `database/`, `themes/`, `plugins/`, `uploads/`, or target a running owner server.
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
python cms/scripts/run-tests.py --self-test
python cms/scripts/run-tests.py --list
node cms/scripts/check-i18n.mjs --strict
node cms/scripts/codegraph-index-docs.mjs --list-sources
python -m unittest cms/scripts/test_dev_launcher.py
```
