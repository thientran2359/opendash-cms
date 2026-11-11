# M3-S28-T01 — Plugins UI Windows implementation evidence

- **Issue:** [#52](https://github.com/thientran2359/opendash-cms/issues/52)
- **Agent / OS:** Forge · Windows 11 Pro (NT 10.0.26200), Chrome 157.0.8103.50
- **Gate:** Fast-Track; G2 AGREED / G3 READY from ADR-0006 and merged source `d5d206afa75256e51ad52fd60ba8d9f966873e4e`.
- **Implementation base:** `origin/main@cef71f3bc534dbe28ff6719d00f30cf8327f0f33` (Users PR #56 already merged).
- **Branch:** `task/forge-m3-s28-t01`; Project #3 self-confirmed Ready then claimed In progress on owner instruction.
- **Toolchain:** Rust/Cargo 1.99.1, Node 22.16.0, Python 3.13.4.

## Scope and data-safety contract

Only task-owned files changed: `cms/admin/src/features/plugins/PluginsPanel.tsx`,
`cms/admin/src/api/plugins.ts`, `cms/scripts/windows_plugin_smoke.py`,
`cms/scripts/windows_plugins_browser.py` and this report.

- UI presents the server's plugin list with status, manifest permissions (SQLite and HTTP), last worker error, install/reload/enable/disable and a manual run form.
- ZIP install is explicitly described as **replace-and-enable**, including confirmation when at least one plugin exists. Upload is withheld while server state is unknown/unavailable; enabled actions require a confirmation showing the plugin's manifest grants.
- Loading/pending UI locks repeated mutations; failed list refresh retains the previous list and offers retry. Errors are plain React text; untrusted values are not passed as HTML. 390px layout avoids document overflow and keeps keyboard access.
- Existing App callbacks remain the sole mutation owner and continue handling status/error/expired login; the panel only performs supplementary GET refreshes. No router, AdminShell, Wasmi runtime, plugin ABI, migration, policy or SQLite mutation code changed.
- Tests use disposable site roots/SQLite and the committed sample-plugin ZIP. No real site data or other agent worktrees were modified.

## Executed checks (Windows)

Run from repo root unless marked `cd cms`:

| Check | Result | Evidence |
| --- | --- | --- |
| `npm ci --prefix cms/admin` | **PASS** | 83 npm packages, zero reported vulnerabilities |
| `npm run build --prefix cms/admin` | **PASS** | TypeScript + Vite build, embedded Admin artifacts |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** | Debug binary `cms/target/debug/opendash.exe` |
| `cargo test --manifest-path cms/Cargo.toml --locked` | **PASS** | 61/61 unit tests: 19 core + 42 server |
| `cd cms; python scripts/windows_plugin_smoke.py --binary target/debug/opendash.exe` | **PASS** | Guest 401; Editor list/install/run/enable/disable 403; CSRF/Origin 403; invalid/traversal/missing-manifest ZIP 400; install/list/disable/enable/run; replace-and-enable; KV survives re-install and restart; payload limits |
| `cd cms; python scripts/windows_plugins_browser.py --binary target/debug/opendash.exe --browser 'C:/Program Files/Google/Chrome/Application/chrome.exe'` | **PASS** | Chromium desktop/390px, install/invalid ZIP, permission text, run, disable/enable confirmation, replacement warning, GET 503 retry, worker error display and disable, Editor guard, keyboard, page overflow, restart with disabled state |
| `cd cms; python scripts/windows_admin_browser.py --binary target/debug/opendash.exe --browser 'C:/Program Files/Google/Chrome/Application/chrome.exe'` | **PASS** | Existing Admin/Builder plugin upload/run and unsaved 401 recovery regression |
| `python cms/scripts/check-plans.py` | **PASS** | Existing planning/link invariants |
| `python cms/scripts/check-plans.py --self-test` | **PASS** | 12 fixtures |
| `python cms/scripts/check-standards.py` | **PASS** | Repo standards |
| `git diff --check` | **PASS** | No whitespace errors; Windows line-ending conversion warning only |

Browser test simulates a server-reported `last_error="Worker timeout"` on a GET response to verify **UI recovery controls** while checking the real disable API separately. Actual worker timeouts, fuel exhaustion and zero-late-writes are enforced by existing Rust tests (all included in 61/61); this browser test does **not** claim to trigger an actual Wasmi worker timeout.

## Revalidation after concurrent Settings integration

Harbor merged Settings #49 while this Plugins branch was in progress. Forge fetched
`main@255ff3ff6cf35253afdd133382b3a6c2cabb5381`, merged it without conflicts
using `a6124be` (no force push), then rebuilt/retested on the **combined** source:

| Combined-build check | Result |
| --- | --- |
| `npm run build --prefix cms/admin` | **PASS** — Settings + Plugins frontend |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** |
| `cargo test --manifest-path cms/Cargo.toml --locked` | **PASS 65/65** — 19 core + 46 server |
| `cd cms; python scripts/windows_plugins_browser.py --binary target/debug/opendash.exe --browser 'C:/Program Files/Google/Chrome/Application/chrome.exe'` | **PASS** |
| `cd cms; python scripts/windows_admin_browser.py --binary target/debug/opendash.exe --browser 'C:/Program Files/Google/Chrome/Application/chrome.exe'` | **PASS** |
| `cd cms; python scripts/windows_plugin_smoke.py --binary target/debug/opendash.exe` | **PASS** |
| `cd cms; python scripts/general_settings_smoke.py --binary target/debug/opendash.exe --baseline fixtures/baseline_0001.sqlite` | **PASS** — Settings defaults, RBAC, CSRF, CAS, persistence and baseline upgrade |
| `check-plans.py`, `--self-test`, `check-standards.py`, `git diff --check` | **PASS** |

The Settings smoke harness defaults to a repo-root-relative fixture; for the
`cd cms` invocation above the explicit `--baseline fixtures/baseline_0001.sqlite`
is required. An initial smoke invocation omitted that argument and failed only
at fixture lookup; the corrected command passed. This is not a CMS regression.

## Security, rollback and outstanding gates

- **Security:** Backend Admin/Editor/guest access, CSRF and origin checks stay unchanged; end-to-end tests exercise both API and navigation. React escapes manifest/last-error content.
- **Rollback:** No schema migration. Code-only rollback can revert these task-owned UI/API/test changes; plugin ZIP/KV data remains server-owned and is not deleted by the UI. Browser/HTTP tests run only in temporary site fixtures.
- **G5 at implementation time:** local checks and the initial PR merge were **not sufficient on their own**. A fresh post-merge Windows task-level G5 verification was subsequently completed on integrated `main`; see the section below. Formal M3 release parity remains a separate gate.

## Post-merge G5 verification — Windows (2026-11-11)

- **Task:** M3-S28-T01 / Issue #52 only (not M3-wide release G5).
- **Code PR:** [#59](https://github.com/thientran2359/opendash-cms/pull/59), final integration PR head `ae5b86bab6b69d27b090ce1f6254232bb93f9fd2`, code merge `e8a3eaccd891eb4d640f9e3b5407c9a890c9ad94`.
- **Governance CI:** [run 42973714101](https://github.com/thientran2359/opendash-cms/actions/runs/42973714101), Ubuntu and Windows jobs **SUCCESS** on the final code PR head.
- **Actual post-merge source:** clean independent Windows worktree at **`origin/main@51aae56ed232e4d7be881b7baab01977c5881d11`** (all Users, Settings, Plugins and Dashboard code integrated). Evidence-only branch: `docs/forge-m3-s28-t01-g5`, starting from later documentation-only main `e2be749df330b1bcf55eaec841090c64c3453b3b` with **no CMS source changes** after the tested `51aae56`.
- **Environment:** Windows 11 Pro, Rust/Cargo 1.99.1, Node 22.16.0, Python 3.13.4, real Google Chrome 157.0.8103.50; locked Rust binary `cms/target/debug/opendash.exe` with embedded freshly built Admin assets.

| Executed post-merge command | Result |
| --- | --- |
| `npm ci --prefix cms/admin`; `npm run build --prefix cms/admin` | **PASS** — 83 packages, Vite 58 modules |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check`; `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** |
| `cargo test --manifest-path cms/Cargo.toml --locked` | **PASS 65/65** (19 core + 46 server), including actual Wasmi timeout/fuel/late-write tests |
| `cd cms; python scripts/windows_plugin_smoke.py --binary target/debug/opendash.exe` | **PASS** — guest/Editor permissions, CSRF/Origin, invalid ZIP, plugin install/replace/run, KV and restart, payload boundaries |
| `cd cms; python scripts/windows_plugins_browser.py --binary target/debug/opendash.exe --browser 'C:/Program Files/Google/Chrome/Application/chrome.exe'` | **PASS** — real Chrome, permission warnings, install/replace, GET-error retry, disable, keyboard and 390px, Editor guard |
| `cd cms; python scripts/windows_admin_browser.py --binary target/debug/opendash.exe --browser 'C:/Program Files/Google/Chrome/Application/chrome.exe'` | **PASS** — Admin/Builder/plugin regression and unsaved 401 recovery |
| `cd cms; python scripts/dashboard_smoke.py --binary target/debug/opendash.exe` and `dashboard_browser.py` with Chrome | **PASS** — integrated Dashboard regression, Admin/Editor roles, 390px |
| `cd cms; python scripts/content_entries_smoke.py --binary target/debug/opendash.exe` | **PASS** — CRUD, RBAC/CSRF/CAS, restart, upgrade |
| `cd cms; python scripts/public_content_smoke.py --binary target/debug/opendash.exe` | **PASS** — public ACL, XSS, theme templates, restart |
| `cd cms; python scripts/general_settings_smoke.py --binary target/debug/opendash.exe --baseline fixtures/baseline_0001.sqlite` | **PASS** — Settings defaults, CAS, upgrade and restart |
| Python `py_compile`, `check-plans.py`, `check-plans.py --self-test`, `check-standards.py` | **PASS**, 12 planning fixtures |

The browser check simulated a returned `last_error="Worker timeout"` to exercise recovery controls and separately confirmed real disable API behavior. It did **not** induce a genuine Wasmi worker timeout in Chrome. Rust runtime tests independently covered actual worker timeout, fuel exhaustion and zero-late-writes. This distinction is unchanged from the original evidence.

**Task-level G5 verdict: VERIFIED on Windows for M3-S28-T01**, based on post-merge API/browser/runtime tests rather than merge alone. Task UI adds no schema or persistence changes; rollback is code-only and does not delete plugin data. Test sites and databases were disposable. This is **not** release G5 for M3; macOS/Windows same-release-SHA acceptance in #53/#54 is still required.
