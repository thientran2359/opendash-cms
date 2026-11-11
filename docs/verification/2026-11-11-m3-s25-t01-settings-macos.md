# M3-S25-T01 — General Settings, macOS integrated verification

**Issue:** [#49](https://github.com/thientran2359/opendash-cms/issues/49) (closed, completed) · **Agent:** Harbor (CoS macOS) · **Date:** 2026-11-11 · **Status:** **Done / task G5 VERIFIED** on macOS at exact main code merge SHA `08f8d210bd1632f1ae5c3d2b27c62d74be2ff387`; GitHub Project #3 **Done**. M3 full-release E2E/Windows parity gates remain separate.

## Baseline, contracts, and ownership

- [ADR-0006 § P13](../decisions/ADR-0006-m3-admin-and-release-contracts.md) and [G3 Users/Settings](../plans/milestones/M3/slices/2026-11-11-m3-users-settings-g3.md) approved at source SHA d5d206afa75256e51ad52fd60ba8d9f966873e4e.
- Task branch: task/harbor-m3-s25-t01, worktree separate from agent/harbor, baseline origin/main at 6d6f4b5a9e9420d6aba4d72311391c24c12fa3b0.
- [Forge Users #48](https://github.com/thientran2359/opendash-cms/issues/48) landed via [PR #56](https://github.com/thientran2359/opendash-cms/pull/56): main merge SHA **cef71f3bc534dbe28ff6719d00f30cf8327f0f33**, Users head 93d1f59439c0ffd78fcf2185b5823a3434b0b7d2. Forge's Issue comment explicitly **handed shared-file integration ownership to Harbor**. Harbor merged this main into the task branch as **9377810** (no force/rebase), retaining standalone Settings commit **94e2cdb712a18a1acb21551c9abb1bfe642695ef**.
- Shared integration changes now **on the real task branch**: `app.rs`, `routes/mod.rs`, `storage/mod.rs`, `App.tsx`, `AdminShell.tsx`. No modifications to Forge-owned Users/auth logic or migration `0009_users_version.sql`. Settings migration **0010** is after merged Users **0009** and both apply on fresh/upgrade.

## Implementation

- SQLite typed singleton with site_title, tagline, timezone and version, default OpenDash / empty / UTC / v1.
- Private Admin-only GET general, GET timezones, and PUT general with CSRF, full-object CAS, If-Match ETag, no-store, 428 missing version and 412 stale version. Unicode scalar validation, trim and control rejection.
- 313 timezone IDs: 312 from IANA tzdb 2026f zone1970.tab and UTC, sorted. Official [tzdata2026f archive](https://data.iana.org/time-zones/releases/tzdata2026f.tar.gz) SHA-256: 01b4a91cec5167ed48c11e8ab78f2048cead78ce94b0cafcad0ae8c5a3044d3d. Bundle provenance stored with catalog.
- Strictly escaped public fallback strings and custom MiniJinja site.title/site.tagline context. Legacy CPT theme templates, Builder root, publication and media visibility retained.
- GeneralSettingsPanel and typed API: accessible labels, retry/loading/error, stale 412 preserves unsaved text, responsive layout and Intl.DateTimeFormat winter/summer preview. Unsupported browser timezone displays UTC without overwriting settings.
- Real Admin `#/settings` renders General Settings plus the existing System Status; removed the obsolete "in development" placeholder. Backend registers the new Admin-only routes and typed storage module. Parent error callbacks do not accidentally reload/erase a Settings draft.

## Verified locally on Intel macOS

macOS 15.8.2 x86_64; Cargo 1.99.1; Node 24.22.2, Vite 6.4.6; Chrome 157.0.8103.51; Python 3.9 with Playwright 1.55.0 in a throwaway testing venv.

All results below were executed from the **actual integrated task source** in `../harbor-m3-s25-t01`, with embedded React Admin and actual Rust/SQLite, not the detached early validation copy. The same core acceptance suite was repeated **after merge** in a clean worktree checked out at `main@08f8d210`. `PATH` in the CoS shell requires the existing Cargo (`~/.cargo/bin`) and Node (`~/.nvm/versions/node/v24.22.2/bin`) paths.

| Check actually executed | Result |
| --- | --- |
| `npm ci --prefix cms/admin; npm run build --prefix cms/admin` | **PASS** — TypeScript + Vite, **56 modules**, embedded Users + Settings. |
| `cargo test --manifest-path cms/Cargo.toml --locked` | **PASS** — 19 core + 46 server = **65/65**. |
| `cargo build --manifest-path cms/Cargo.toml --locked`; `cargo fmt --manifest-path cms/Cargo.toml --all -- --check` | **PASS** — actual x86_64 macOS binary and Rust format. |
| `python3 cms/scripts/general_settings_smoke.py --binary cms/target/debug/opendash --baseline-binary ../harbor-m3-s25-t01-rollback/opendash-pre-0010` | **PASS** — defaults, Admin/Editor/guest/Origin/CSRF, 400/428/412, concurrent CAS, Unicode, tzdb, escaped public default/custom context, restart; **0009+0010 fresh/upgrade** and legacy timezone. |
| The same Settings smoke: snapshot and rollback with *actual* pre-0010 Users binary built from `main@cef71f3` | **PASS** — SQLite online backup after 0009, new binary migrates 0010 and writes settings, restored snapshot opens in old binary, user login/Builder/theme/homepage intact, no 0010 in restored DB. Never downgrade the migrated DB in place. |
| `../harbor-m3-s25-t01-validation/.venv/bin/python cms/scripts/general_settings_browser.py --binary cms/target/debug/opendash --browser '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'` | **PASS** — real Chrome desktop + 390px keyboard, two-tab stale 412 preserves draft, refresh, restart; fixed Jan/Jul UTC timestamps render **19:00** in Asia/Ho_Chi_Minh and **07:00/08:00** in America/New_York. |
| `python3 cms/scripts/public_content_smoke.py --binary cms/target/debug/opendash` | **PASS** — CPT archive/single, theme templates, XSS, media ACL, pagination and restart. |
| `python3 cms/scripts/content_types_smoke.py ...`; `python3 cms/scripts/content_entries_smoke.py ...` | **PASS** — content CRUD/RBAC/CSRF/migration/publication regressions. |
| Forge `windows_auth_smoke.py`, `windows_users_browser.py` with precompiled `__future__.annotations` (Python 3.9 compatibility, no source edits); user browser via Chrome | **PASS on macOS** — Users management, concurrent last-Admin guard, revoked sessions/reset/login races, SQLite writer lock 503, desktop/390px browser. Windows evidence remains Forge's PR #56, not inferred from this run. |
| `python3 cms/scripts/check-plans.py`; `--self-test`; `python3 .agents/scripts/check-agent-artifacts.py`; `git diff --check` | **PASS**. |
| `check-standards.py` with vendored `pip._vendor.tomli` mapped to missing Python 3.9 stdlib `tomllib` | **PASS** — script unchanged, no dependency added to project. |

All HTTP/browser tests use temporary site roots, loopback ports and SQLite fixtures; none use a live site DB. Baseline `cef71f3` binary was compiled in a **separate detached rollback worktree**; old and new binaries were never started on the same upgraded DB at once. The rollback smoke uses a real SQLite snapshot and exclusively operates on temporary copies.

## G5 post-merge verification and governance

1. **Code PR:** [#57](https://github.com/thientran2359/opendash-cms/pull/57), head `57481e7827a4f98283f307a0a16d05648636cb4a`, base `cef71f3bc534dbe28ff6719d00f30cf8327f0f33`, merge commit **`08f8d210bd1632f1ae5c3d2b27c62d74be2ff387`**. At exact PR head, [OpenDash Governance run 42971340202](https://github.com/thientran2359/opendash-cms/actions/runs/42971340202) finished **SUCCESS** on both Ubuntu and Windows. Mergeability and latest base were rechecked; expected-head self-merge complied with ADR-0005.
2. **Actual post-merge macOS x86_64 at exact SHA `08f8d210`:** a separate clean detached worktree was built after merge with `npm ci --prefix cms/admin`, `npm run build --prefix cms/admin`, `cargo test --manifest-path cms/Cargo.toml --locked` (**65/65 PASS**) and locked Cargo build (**PASS**). `general_settings_smoke.py` with actual pre-0010 binary including backup/restore (**PASS**), `general_settings_browser.py` on real Chrome 390px (**PASS**) and `public_content_smoke.py` (**PASS**) ran again. `cargo fmt --check`, plan checker and self-test, agent-artifact guard, `git diff --check`: **PASS**. Post-merge worktree git status clean before the docs-only update. All used disposable DB/site roots.
3. [Issue #49 G5 evidence comment](https://github.com/thientran2359/opendash-cms/issues/49#issuecomment-6788709692) records actual commands, OS, fixtures, PR and code SHA. Issue closed as **completed** and Project #3 Status verified **Done** afterward. No independent peer review is claimed; autonomous M3 delivery is governed by ADR-0005.
4. Commit author/committer in task and docs worktrees: `Harbor <daiduongtihon.vn@gmail.com>` (worktree-local only), following owner confirmation and [identity policy](../../.agents/GIT_WORKTREE_IDENTITY.md). No global Git identity change.
5. M3 full release macOS/Windows E2E, distribution and milestone closure belong to later M3-S29/S30 tasks; **this G5 is SITE-01.1 task-scoped**. No production deployment or live migration was performed. A restored pre-0010 database must be paired with the pre-0010 executable; an upgraded DB must not be opened by that older binary.
