# M3-S25-T01 — General Settings, macOS verification (standalone phase)

**Issue:** [#49](https://github.com/thientran2359/opendash-cms/issues/49) · **Agent:** Harbor (CoS macOS) · **Date:** 2026-11-11 · **Status:** In progress. G5 NOT VERIFIED; integration, PR/CI pending.

## Baseline, contracts, and ownership

- [ADR-0006 § P13](../decisions/ADR-0006-m3-admin-and-release-contracts.md) and [G3 Users/Settings](../plans/milestones/M3/slices/2026-11-11-m3-users-settings-g3.md) approved at source SHA d5d206afa75256e51ad52fd60ba8d9f966873e4e.
- Task branch: task/harbor-m3-s25-t01, worktree separate from agent/harbor, baseline origin/main at 6d6f4b5a9e9420d6aba4d72311391c24c12fa3b0.
- [Forge Users #48](https://github.com/thientran2359/opendash-cms/issues/48) is still In progress. Under G3, app.rs, routes/mod.rs, storage/mod.rs, error.rs, App.tsx, AdminShell.tsx, and api/index.ts belong to the Users integration until that PR merges or exclusive handoff is recorded. **No shared files were edited in the task branch.** Proposed migration is 0010 after Users 0009; validate against new main on handoff.

## Standalone implementation

- SQLite typed singleton with site_title, tagline, timezone and version, default OpenDash / empty / UTC / v1.
- Private Admin-only GET general, GET timezones, and PUT general with CSRF, full-object CAS, If-Match ETag, no-store, 428 missing version and 412 stale version. Unicode scalar validation, trim and control rejection.
- 313 timezone IDs: 312 from IANA tzdb 2026f zone1970.tab and UTC, sorted. Official [tzdata2026f archive](https://data.iana.org/time-zones/releases/tzdata2026f.tar.gz) SHA-256: 01b4a91cec5167ed48c11e8ab78f2048cead78ce94b0cafcad0ae8c5a3044d3d. Bundle provenance stored with catalog.
- Strictly escaped public fallback strings and custom MiniJinja site.title/site.tagline context. Legacy CPT theme templates, Builder root, publication and media visibility retained.
- GeneralSettingsPanel and typed API: accessible labels, retry/loading/error, stale 412 preserves unsaved text, responsive layout and Intl.DateTimeFormat winter/summer preview. Unsupported browser timezone displays UTC without overwriting settings.

## Verified locally on Intel macOS

macOS 15.8.2 x86_64; Cargo 1.99.1; Node 24.22.2, Vite 6.4.6; Chrome 157.0.8103.51; Python 3.9 with Playwright 1.55.0 in a throwaway testing venv.

For early runtime verification, Harbor created a **separate detached validation worktree** at ../harbor-m3-s25-t01-validation from the same baseline, copied task-owned changes, temporarily registered the backend modules and routes, wired the UI there, then built the Rust binary with embedded Admin. **These shared-file edits exist only in the validation copy.** Results cannot establish integration on the task branch or G5.

| Check actually executed | Result |
| --- | --- |
| npm ci --prefix cms/admin; npm run build --prefix cms/admin (task source) | **PASS** — TypeScript + Vite; standalone feature. |
| Same npm commands in validation copy with AdminShell wired | **PASS** — 55 modules built. |
| cargo build --manifest-path cms/Cargo.toml --locked (validation; CARGO_TARGET_DIR=../harbor-m3-s25-t01/cms/target) | **PASS** — real macOS binary and embedded Admin. |
| cargo test --manifest-path cms/Cargo.toml --locked (validation; same target) | **PASS** — 19 core and 44 server unit tests, zero failures. |
| python3 cms/scripts/general_settings_smoke.py --binary ../harbor-m3-s25-t01/cms/target/debug/opendash (validation) | **PASS** — fresh defaults, authentication/ACL/Origin/CSRF, 400/428/412, CAS race, Unicode, timezone, escaped default and custom public template, persistence/restart. |
| Same smoke: upgrade from copied cms/fixtures/baseline_0001.sqlite | **PASS** — original homepage and theme rows retained; migration to 0010; default singleton and grandfathered timezone verified. |
| python3 cms/scripts/public_content_smoke.py --binary ../harbor-m3-s25-t01/cms/target/debug/opendash (validation) | **PASS** — archive/single, themes, XSS, media ACL, pagination and restart regressions. |
| .venv/bin/python cms/scripts/general_settings_browser.py --binary ../harbor-m3-s25-t01/cms/target/debug/opendash --browser /Applications/Google Chrome.app/Contents/MacOS/Google Chrome (validation) | **PASS** — actual Chrome, 390px no overflow, keyboard form, 2-tab stale 412 preserving draft, refresh and restart. |
| Same validation-copy Admin build, Rust locked build and Chrome browser smoke after wiring an unstable parent onApiError callback (2026-11-11) | **PASS** — React loader remains stable with a callback ref, avoiding accidental reloads that would erase the local form during parent renders. |
| cargo fmt --manifest-path cms/Cargo.toml --all -- --check; rustfmt --edition 2024 on new Rust modules; Python py_compile; git diff --check | **PASS**. |

All HTTP/browser tests use temporary site roots, loopback ports and SQLite fixtures; none use a live site DB. No final PR/CI/G5 verification yet.

## Integration and delivery still required

1. Recheck main, #48 Users handoff, migration 0009, branch base and ownership before touching shared files. Register routes::settings and storage::settings and GET/PUT routes in app.rs; wire GeneralSettingsPanel with csrfToken in App.tsx and remove AdminShell placeholder.
2. Rebuild and repeat smoke/browser tests on the **actual** post-handoff task source; also check upgraded database with Users 0009, rollback snapshot/baseline binary, Admin/Editor regression and CI.
3. Worktree-specific Git user.name=Harbor and user.email=daiduongtihon.vn@gmail.com are both configured in config.worktree following owner confirmation. Global Git identity remains unchanged (unset). The [identity policy](../../.agents/GIT_WORKTREE_IDENTITY.md) is followed for this task worktree.
4. After integrated validation, stage owned files, commit with Task ID and Agent: Harbor trailer, push non-force and create PR; G5 and Project Done require separate verified evidence.
