# M3-S27-T01 / THM-01.1 — Themes macOS integrated verification

**Agent:** Harbor (CoS macOS) · **Issue:** [#51](https://github.com/thientran2359/opendash-cms/issues/51) · **Date:** 2026-11-11

**G2:** [ADR-0006](../decisions/ADR-0006-m3-admin-and-release-contracts.md), P14. **G3 source:** `d5d206afa75256e51ad52fd60ba8d9f966873e4e`.
**Integration base:** `main@51aae56ed232e4d7be881b7baab01977c5881d11`, [Forge Dashboard PR #60](https://github.com/thientran2359/opendash-cms/pull/60) merged. Shared-file ownership released by Forge in [Issue #51 handoff](https://github.com/thientran2359/opendash-cms/issues/51#issuecomment-6789327765). Task branch: `task/harbor-m3-s27-t01`, no force-push. **Code source SHA and PR CI:** add after commit; **G5** not implied by local tests and requires post-merge verification on exact main SHA.

## What was implemented

- Forward-only migration `0011_theme_activation.sql`: optional `theme_blocks.compatible_block_id`, singleton `site_active_theme(id=1,block_id=NULL,version=1)` with FK and CAS. Existing `home_page`, Post/Page, CPT, media refs and auth remain untouched. NULL preserves legacy rendering until Admin explicitly activates a theme.
- ZIP parser accepts at most five **flat** allowlisted files. Optional `homepage-compat.json` has exact schema `{"version":1,"accepts":"installed-owner/block"}`; validation rejects aliases not installed, self aliases, invalid version, wrong field keys, extra paths and ZIP duplicates **before insertion**. Original registry owner/fields stay authoritative.
- Authenticated Admin-only `GET/POST /api/admin/themes`, `PUT /api/admin/themes/active` using `If-Match:"vN"`, 428 missing / 400 malformed / 412 stale, body cap, CSRF, Origin, ETag and `Cache-Control: no-store`. Legacy `POST /theme` remains supported.
- Public homepage resolves saved block and active selection **in one SQL snapshot**, uses active `block.html` when ID/alias matches, otherwise responds HTTP 200 with empty safe content (no stale theme HTML or saved values). Public content archive/single templates follow active theme, falling back to built-in when missing; Settings SiteContext still HTML-escapes. Preview and Save validate **original block schema**, respect active selection, return HTML with `X-OpenDash-Block-Visibility: rendered|unsupported` and no-store. Save selects active template and commits homepage update in one write transaction, **before** after-Save hooks.
- Admin `#/themes`: install/list/activate/refresh, compatibility/unsupported notice, stale 412 retry without overwriting a Builder draft, 390px responsive and keyboard. Builder compatibility notice is **outside sandbox iframe**. Dashboard mount/role links and Settings are retained.

## Primary Acceptance: 3 mandatory theme switches

| Case | Real SQLite / HTTP | Real Chrome / Admin |
| --- | --- | --- |
| **A → B compatible** | PASS: save original block `qa-a/banner` with title/note; B explicitly aliases A and renders same saved values using B's markup. Original `home_page` row unchanged. | PASS: install both ZIPs, activate B, public & Preview show B; unsaved Preview draft not public. |
| **A → C unsupported** | PASS: activate C without alias; public 200 with no saved title or A/B/C markup; `/page` retains the exact block ID and values; Preview/Save report `unsupported`, no-store. Save while hidden stores the new text without exposing it publicly. | PASS: unsupported warning outside iframe; Builder still has unsaved title/note after tab switch and Preview; Save persists. |
| **C → A restore** | PASS: activate A, original template + values saved under C reappear without input, survive server restart. Pointer version increments and SQLite row remains. | PASS: A public + Preview show restored text, Builder fields remain; tab-2 stale activation 412 retains form; 390px no overflow. |

## Test execution and results

Environment: **Intel x86_64, macOS 15.8.2 (24H232)**; Cargo **1.99.1**, Node **24.22.2**, Python **3.9.6**; Google Chrome **157.0.8103.51** with Playwright installed in the existing isolated macOS test venv. Binary debug embeds 60 Vite Admin modules, produced in the *separate task target* `/Users/thientran/Local Sites/opendash-tasks/harbor-m3-s25-t01/cms/target/debug/opendash` (`CARGO_TARGET_DIR` reused, **not** the executable of the user's localhost 18766 server).

For commands below, define `BIN` as that exact absolute binary path and `CHROME` as `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`; `PY` is `/Users/thientran/Local Sites/opendash-tasks/harbor-m3-s25-t01-validation/.venv/bin/python`. Commands run at task worktree root unless explicitly marked `cms/`. Python 3.9 lacks the `str|None` annotation runtime; existing Forge browser/auth harnesses were invoked via `compile(..., flags=__future__.annotations.compiler_flag)` without modifying their source.

| Command / verification | Result |
| --- | --- |
| `npm ci --prefix cms/admin`; `npm run build --prefix cms/admin` | **PASS** — TypeScript + Vite 60 modules, 0 npm audit vulnerabilities. |
| `cargo fmt --manifest-path cms/Cargo.toml --all -- --check`; `CARGO_TARGET_DIR=... cargo test --manifest-path cms/Cargo.toml --locked`; `cargo build ... --locked` | **PASS** — 19 core + 48 server = **67/67** Rust tests; binary rebuilt after atomic Save transaction. |
| `python3 cms/scripts/themes_smoke.py --binary "$BIN" --baseline-binary /Users/thientran/Local\ Sites/opendash-workspace/opendash-cms-mac-cos/cms/target/debug/opendash` | **PASS** — A/B/C, ZIP errors no partial insert, legacy ZIP, 401/403, Origin/CSRF 403, 428/400/412, preview/Save, immutable `home_page` on activation, restart, fresh & historical 0001→0011 migrations, snapshot rollback using the *real pre-0011 binary*. |
| `"$PY" cms/scripts/themes_browser.py --binary "$BIN" --browser "$CHROME"` | **PASS** — real embedded Chrome Admin A/B/C, unsaved draft and Save, theme warning, stale-tab 412, mobile 390px/keyboard, restart. |
| `python3 cms/scripts/dashboard_smoke.py --binary "$BIN"` (Python-3.9 compatible annotation compile) + `"$PY" cms/scripts/dashboard_browser.py ...` (same compatibility) | **PASS** — Dashboard API post/page aggregates, published/draft, role and 503, desktop/mobile 390px UI. |
| `python3 cms/scripts/content_types_smoke.py ...`; `content_entries_smoke.py`; `media_smoke.py`; `public_content_smoke.py` | **PASS** — content CRUD/CAS/RBAC, media privacy/thumbs/restart, CPT template autoescape/XSS and public/media ACL regressions. |
| Forge `windows_auth_smoke.py` on macOS under Python 3.9 compatibility | **PASS** — session/CSRF/Origin, Admin-last, race, SQLite BUSY/503 and upgrade regression. |
| Chrome `windows_admin_browser.py`, `general_settings_browser.py`, `windows_users_browser.py`, `windows_plugins_browser.py` (annotation compatibility) | **PASS on macOS** — Builder unsaved/relogin/save, Settings two-tab 412/timezone/390px, Users CAS/session revoke, Plugins ZIP/recovery/390px. These are not independent Windows release results. |
| `python3 cms/scripts/check-plans.py` and `--self-test`; `python3 .agents/scripts/check-agent-artifacts.py` | **PASS** — plan links/IDs and 12 fixture self-tests; artifact/ownership guard. |
| `check-standards.py` with vendored Python-3.9 `tomli` compatibility; `python3 -m py_compile` for new scripts; `cargo fmt --check`; `git diff --check` | **PASS** — SQL exclusively in storage; canonical CSRF guard names, code formatting, Python syntax and whitespace. |

### Data safety and rollback

All smoke and browser scripts use disposable site roots/SQLite and loopback ports. Historical fixture is opened with SQLite URI `mode=ro&immutable=1` to avoid creating WAL/SHM sidecars; the original fixture is not modified. Actual pre-0011 binary and a complete SQLite snapshot are used to demonstrate rollback: run old binary → backup DB → run new binary and activate → stop → **restore snapshot** → run old binary (never open an upgraded 0011 DB with the old executable). Existing Builder homepage content persists. The user's running Admin server at port 18766, its installed account and database are untouched.

### Pending delivery / G5

Complete task-branch commit with `Agent: Harbor`, non-force push, PR to main; verify required Ubuntu/Windows GitHub governance checks at exact PR head and mergeability per ADR-0005. Self-merge only after all gates PASS; rerun core tests **on actual main merge SHA**, log G5 with exact provenance, then update Project #3/Issue #51 and prepare separate #53 macOS E2E against the agreed release candidate. Merge alone is not G5 and does not imply Windows release verification.
