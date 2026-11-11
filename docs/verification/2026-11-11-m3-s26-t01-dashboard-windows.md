# M3-S26-T01 — Dashboard Windows implementation verification

- **Issue:** [#50](https://github.com/thientran2359/opendash-cms/issues/50)
- **Agent:** Forge (Chat On Steroids Windows)
- **Gates:** Full-Track, G2 `AGREED`, G3 `READY` under [ADR-0006](../decisions/ADR-0006-m3-admin-and-release-contracts.md) and approved source `d5d206afa75256e51ad52fd60ba8d9f966873e4e`.
- **Base:** `origin/main@e8a3eaccd891eb4d640f9e3b5407c9a890c9ad94` (Users, Settings, Plugins already merged).
- **Worktree:** `task/forge-m3-s26-t01`; Project #3 was verified and moved from Backlog → Ready → In progress on owner's delegated authorization.
- **Environment:** Windows 11 Pro, real Google Chrome, Rust/Cargo locked, Node/TypeScript, Python/Playwright. All test DBs and site roots are disposable; production data untouched.

## Implementation and invariants

- `GET /api/admin/dashboard` uses `require_session`, so both Admin and Editor are authorized, unauthenticated users are denied (401).
- The response only contains `{post:{draft,published},page:{draft,published}}`. Four nonnegative integer counts come from **one SQLite aggregate SELECT** in `storage/dashboard.rs`, restricted to built-in `post` and `page` keys; custom CPTs are not counted. No per-user IDs, usernames, logs, or private content are included. Success uses `Cache-Control: no-store`; empty DB returns four zeros.
- The Dashboard React view re-fetches on each entry, does not reuse stale values after navigating to Posts/Pages, distinguishes loading, genuine zero, and fetch error with retry. The existing role-aware shortcuts remain; Pages is now directly accessible from Dashboard. 390px viewport and keyboard focus are exercised.
- No migrations, publication-role policy, editor lifecycle, preview/Save contract, or persistent site schema changed. Existing App/Router/AdminShell edits are within Issue #50's explicit shared-file ownership; P14 Themes must not modify those files until Dashboard merges/handoff.

## Commands executed and results

Run from repository root unless prefixed by `cd cms`:

| Check | Result |
| --- | --- |
| `npm ci --prefix cms/admin` | **PASS**; existing locked dependencies |
| `npm run build --prefix cms/admin` | **PASS** TypeScript + embedded production assets |
| `cargo fmt --manifest-path cms/Cargo.toml --all` / `--check` | **PASS** |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS** |
| `cargo test --manifest-path cms/Cargo.toml --locked` | **PASS 65/65** (19 core + 46 server; includes Rust worker/security regression tests) |
| `cd cms; python scripts/dashboard_smoke.py --binary target/debug/opendash.exe` | **PASS**: zero, guest 401, Editor/Admin, built-in-only, draft/published transitions, CPT exclusion, no-store, restart |
| `cd cms; python scripts/dashboard_browser.py --binary target/debug/opendash.exe --browser 'C:/Program Files/Google/Chrome/Application/chrome.exe'` | **PASS**: Chrome real Admin/Editor, loading/zero/error/retry, new counts after navigating back, role-aware links, keyboard, 390px, no overflow |
| `cd cms; python scripts/windows_admin_browser.py --binary target/debug/opendash.exe --browser 'C:/Program Files/Google/Chrome/Application/chrome.exe'` | **PASS**: embedded Admin/Builder, expired-session unsaved draft, Plugins |
| `cd cms; python scripts/content_entries_smoke.py --binary target/debug/opendash.exe` | **PASS**: content CRUD, RBAC, CSRF, CAS, restart and upgrade |
| `cd cms; python scripts/public_content_smoke.py --binary target/debug/opendash.exe` | **PASS**: public archive/single, templates, escaping, media ACL, pagination, restart |
| `cd cms; python scripts/general_settings_smoke.py --binary target/debug/opendash.exe --baseline fixtures/baseline_0001.sqlite` | **PASS**: settings permissions/CAS and fixture upgrade |
| `python -m py_compile cms/scripts/dashboard_smoke.py cms/scripts/dashboard_browser.py` | **PASS** |
| `python cms/scripts/check-plans.py` and `--self-test` | **PASS** (12 fixtures) |
| `python cms/scripts/check-standards.py` / `git diff --check` | **PASS** |

## Rollback and release handoff

Task adds no migration and stores no Dashboard-owned data; rollback is a code-only revert of route, storage, API, UI and tests. SQLite content remains unchanged. The Dashboard counts are not analytics and are not intended to include custom content types or recently modified items.

**Do not mark G5 VERIFIED from local implementation tests or merge alone.** Formal M3 macOS/Windows end-to-end/release acceptance and Project `Done` remain separate steps.
