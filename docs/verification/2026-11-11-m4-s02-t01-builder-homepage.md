# M4-S02-T01 — Builder homepage Open / Preview / Save (Windows, G4)

**Issue:** [#78](https://github.com/thientran2359/opendash-cms/issues/78) · **Agent:** Forge · **Scope:** BLD-01 / BLD-01.1 · **Gate:** G3 READY ([approved source](../plans/milestones/M4/slices/2026-11-11-m4-foundation-g3.md) at `0e3d9886afa6115f84d97aa8905bc29f18fa56cb`).

**Implementation base:** `origin/main@c6668b73a10c4ff7d990e61a2d14d30bba4a8842` (Builder core #77 and Taxonomy #85 already merged). Dedicated task branch/worktree `task/forge-m4-s02-t01`. **Implementation head:** `368ac40479f8d29c4ae5ea1deb57d9e9a1a643d5`, merged by [PR #88](https://github.com/thientran2359/opendash-cms/pull/88) into `main@113b8351eb1d5ca527eed68f2ee210495d3b85c0`. MERGED and G5 VERIFIED are evidenced separately below.

## Implementation and contracts

- Migration `0013_builder_homepage.sql` adds the site-owned singleton with `version=1`, `document_json=NULL`, plus a saved-only public media reference table. The migration does **not** project or overwrite M3 homepage at startup. `home_page` remains untouched for backup/restore. SQL ownership is confined to `storage/`.
- `GET /api/admin/builder/homepage` returns `{document,source,version}`, ETag `"vN"`, no-store, for Admin/Editor. A deterministic M3 single-block tree projection retains original block ID, field values and stable node IDs across reads/restarts/theme changes. Before the first Save, `source` is `empty` or `legacy`. If a tree has been saved, `source=layout`.
- `POST /api/admin/builder/homepage/preview` validates typed AST/core element props and M3 descriptor using #77, checks image registration, renders escaped HTML using the current theme from a consistent SQLite read snapshot, returns `{html,diagnostics}`, and never writes document or public media ACL. Preview Image uses authenticated `/api/admin/media/{id}/file`.
- `PUT /api/admin/builder/homepage` accepts only JSON ≤256 KiB, session + CSRF/Origin + exact `If-Match: "vN"`. Missing precondition **428**, malformed **400**, stale version **412**. The singleton writer transaction serializes validation, theme selection, render, write, incremented version and replacement of image refs. Invalid input/hook/render errors before commit do not create a partial tree/ACL. The response contains document/source/version/html/diagnostics and an updated ETag. Hook `page/after_save` runs only **after** commit (failures logged without losing the save).
- Legacy `/save` increments the same singleton version while M3 remains authoritative and fails with **409** after conversion; legacy `/page` also becomes **409**. Builder Save first converts the public source exactly once. Concurrent Save, legacy Save and theme activation use SQLite writer arbitration; public `/` renders a consistent saved document/theme snapshot.
- Core elements Heading, Text, Image, Button render with HTML/attribute escaping. Legacy nodes use the active theme's compatible template; nodes not supported by the theme remain in the document, receive per-node diagnostics and are hidden from public HTML. An unknown historical payload can round-trip only unchanged from the trusted stored document and is not rendered. The tree cannot silently replace a missing descriptor. Public image/thumbnail ACL derives from *saved* core/image refs union existing published CPT references; removing the final saved ref revokes new unauthenticated requests (404) without deleting Media Library assets. Existing M3 media ACL stays intact.
- Existing Post/Page/CPT URLs and Publish semantics are unchanged. The M3 theme list's single-block compatibility flag becomes null when the Builder tree is authoritative rather than incorrectly describing a retained backup block.

## Local G4 verification (Windows 11 Pro)

Commands executed from the repository root unless noted. Tests run on **disposable site directories**, not the real user CMS database.

| Check | Observed result |
| --- | --- |
| `cd cms/admin; npm ci --no-audit --no-fund; npm run build` | **PASS** (Vite 62 modules) |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | **PASS** after formatting |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS**, using `CARGO_TARGET_DIR` pointing to the reusable task-target cache |
| `cargo test --manifest-path cms/Cargo.toml --workspace --locked` | **PASS 82/82** (25 core + 57 server, 0 doctests) including new Builder after-commit hook failure / payload tests |
| `python cms/scripts/builder_homepage_smoke.py --binary <built-opendash.exe> --m3-binary <actual-M3-release-opendash.exe>` | **PASS** (details below) |
| `python cms/scripts/themes_smoke.py --binary <built-opendash.exe> --baseline-db cms/fixtures/baseline_0001.sqlite` | **PASS** (M3 themes, restart, old DB upgrade) |
| `python cms/scripts/media_smoke.py --binary <built-opendash.exe>` | **PASS** (auth, upload, thumbs, references, restart) |
| `python cms/scripts/public_content_smoke.py --binary <built-opendash.exe>` | **PASS** (CPT/publication/media ACL, escaping, restart) |
| `python cms/scripts/taxonomy_terms_smoke.py --binary <built-opendash.exe> --pre-m4-binary <actual-M3-release-opendash.exe>` | **PASS** (P09 taxonomy, race and M3 migration/rollback) |
| `python cms/scripts/check-standards.py`, `python cms/scripts/check-plans.py`, `python cms/scripts/check-plans.py --self-test` | **PASS** (12 plan checker fixtures) |
| `git diff --check` | **PASS** |

New `builder_homepage_smoke.py` checks: guest 401; Admin/Editor access; CSRF and hostile Origin 403; JSON type/256 KiB body protection including a valid 75 KiB draft; invalid URL/unknown node/missing image 400; Open/Preview leave SQLite and public unchanged; first Save switches source while preserving the M3 backup; stale requests before and after conversion 412; any subsequent legacy writer 409; HTML escaping/CSP; media ACL 404 → Save 200 → remove-reference 404 while private file remains; restart preserves document, stable IDs, order and field values; A→B uses B markup, A→C hides only incompatible legacy node, C→A restores; per-node diagnostics; concurrent Save yields 200/412; concurrent theme activation plus Save yields a coherent final public snapshot. A test injects a historical unresolved element into a temporary database and proves unchanged round-trip, while edits/reposition are rejected.

**Genuine M3 binary rollback:** the **unmodified Windows M3 release binary** from `main@d2b6bfe1e7b6c2f6895802b8ed609292747d3062` has SHA256 `85be89f684a57c49dc8f45905ff87c4006c53ffc1b251f192d650d5db34300e9`, verified against the M3 [release report](2026-11-11-m3-s30-t01-release-windows.md). Test creates homepage under that binary, copies the entire site before M4 migration, opens/upgrades/saves the working copy using the new binary, then reopens the **untouched snapshot** with the same M3 binary and confirms preserved legacy homepage/public content. Running the old binary directly against a 0013-migrated DB is **not** claimed or supported.

## Security and rollback boundaries

- No UI tree/canvas/inspector, responsive settings, list block, bindings or P03 sandbox editor interactions are implemented by this task. Admin consumes the new REST API; browser-side editor is a separate slice.
- Preview's JSON response contains escaped HTML/diagnostics; there is no new arbitrary HTML/JS/CSS input in schema v1. Public `/` response specifies CSP including `img-src 'self'` and `script-src 'none'`; internal preview images use session-protected Media Library routes.
- `page/after_save` event uses the original `block_type` for exactly one legacy element; otherwise `core/layout` with numeric `layout_version`; draft props are never included in hook payload. An injected failing hook is exercised post-commit in the Rust test and leaves the saved layout intact. Event plugin-delivery logic remains the existing HookDispatcher; no new plugin permissions.
- Migration 0013 is additive; preserve a consistent database+uploads snapshot **before upgrade** for binary-M3 rollback. Never downgrade in-place using SQLx migration row deletion. A restored M3 binary uses its corresponding pre-upgrade data.
- The integration must still pass required GitHub Ubuntu/Windows checks at the **final PR head**. Windows native G5 after merge is a separate sign-off; no release/deploy from this task.

## G5 — Native Windows validation on merged main (2026-11-11)

**Verdict: G5 VERIFIED for M4-S02-T01 / Builder homepage API and public renderer.** This is an independent run **after** PR #88 merged, checked out at the exact product source `main@113b8351eb1d5ca527eed68f2ee210495d3b85c0` in a new dedicated Windows worktree `opendash-cms-forge-m4-s02-g5` (initially detached at that commit). It is not an inference from PR CI, the pre-merge worktree or an updated `main` at a later SHA.

| Actual command (root unless noted) | Post-merge result |
| --- | --- |
| `cd cms/admin; npm ci --no-audit --no-fund; npm run build` | **PASS**, 62 Vite modules, embedded Admin assets produced |
| `cargo fmt --manifest-path cms/Cargo.toml --all --check` | **PASS** |
| `cargo build --manifest-path cms/Cargo.toml --locked` | **PASS**, Windows native Rust, recompiled core/server from G5 worktree source |
| `cargo test --manifest-path cms/Cargo.toml --workspace --locked` | **PASS 82/82** (25 core, 57 server; 0 doc tests) |
| `python cms/scripts/check-standards.py` | **PASS** |
| `python cms/scripts/check-plans.py`, `python cms/scripts/check-plans.py --self-test` | **PASS**, 12 fixtures in self-test |
| `python cms/scripts/builder_homepage_smoke.py --binary <new-exe> --m3-binary <real-m3-release-exe>` | **PASS**, all Builder/M3/Open/Preview/Save/ACL/CAS/race/rollback groups |
| `python cms/scripts/themes_smoke.py --binary <new-exe> --baseline-db cms/fixtures/baseline_0001.sqlite` | **PASS** |
| `python cms/scripts/media_smoke.py --binary <new-exe>` | **PASS** |
| `python cms/scripts/public_content_smoke.py --binary <new-exe>` | **PASS** |
| `python cms/scripts/taxonomy_terms_smoke.py --binary <new-exe> --pre-m4-binary <real-m3-release-exe>` | **PASS**, including actual M3 backup/restore |

`CARGO_TARGET_DIR` points to the task cache folder `../opendash-cms-forge-m4-s01-t01/cms/target`; Cargo **recompiled opendash-core and opendash-server from the post-merge G5 source**, not from the pre-merge build. Python HTTP/SQLite smoke uses the new binary compiled by the G5 build and temporary site roots; no user site or production database was touched. The genuine Windows M3 release binary's checked SHA256 matches the G4 section. The G5 worktree was clean after executing tests, before changing this verification document.

**Required GitHub CI:** OpenDash Governance on exact implementation head `368ac40479f8d29c4ae5ea1deb57d9e9a1a643d5`, [run 43030674547](https://github.com/thientran2359/opendash-cms/actions/runs/43030674547): **Ubuntu SUCCESS, Windows SUCCESS**, prior to merge. Forge performed a documented self-review at that exact head under ADR-0008 (not independent peer review); PR merged with `expected_head_sha`.

**G5 limits:** Task #78 implements Builder document source/REST/public renderer but **not** P03 canvas, inspector or frontend layout editing controls. No Linux/macOS native release was run for this task. No in-place SQLx downgrade was attempted. Binary-M3 restore uses only a preserved pre-migration snapshot. Product source remained pinned to `113b835`; evidence-only commits after this section cannot substitute for new product test results.

**Sign-off:** Forge (Windows), M4-S02-T01 — **G5 VERIFIED**. Eligible to transition Issue #78 and Project #3 to `Done`/completed, without implying G5 for unrelated M4 slices.
