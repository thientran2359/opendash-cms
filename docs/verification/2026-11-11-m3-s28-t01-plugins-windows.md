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

## Security, rollback and outstanding gates

- **Security:** Backend Admin/Editor/guest access, CSRF and origin checks stay unchanged; end-to-end tests exercise both API and navigation. React escapes manifest/last-error content.
- **Rollback:** No schema migration. Code-only rollback can revert these task-owned UI/API/test changes; plugin ZIP/KV data remains server-owned and is not deleted by the UI. Browser/HTTP tests run only in temporary site fixtures.
- **G5:** Local implementation checks are **PASS**, but merged PR, CI on its exact head, formal M3 release verification and Project `Done` are separate gates. **Do not label G5 VERIFIED from this report alone.**
