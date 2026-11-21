# EGM Spec 2 / Slice S01 — Issue #429 native macOS Intel implementation receipt

- Agent: Harbor — 2026-11-21 (Asia/Bangkok)
- Repository: `thientran2359/opendash-cms`, Issue [#429](https://github.com/thientran2359/opendash-cms/issues/429)
- Approved Design Spec: [`docs/designs/cpt-content-builder-and-pattern-library.md`](../designs/cpt-content-builder-and-pattern-library.md), **Slice S01** only; S02 Pattern CPT / S03 Pattern Library remain out of scope
- Starting `origin/main`: `e4e1fca754614b872b2fd21ed16b5bf0d5c5a78e`; `agent/harbor` fast-forwarded, clean on pickup; previous #428 CLOSED/Project Done
- Scope: opt-in Builder for any active declared CPT; Page default-on, Post default-off; entry-scoped AST/Editor switching with shared entry CAS; private draft/media guards; public `od_content` fallback; Admin Builder/Editor controls and EGM migration `0043`

## Behavior and preservation

- Generic `/api/admin/entries/{type_key}/{id}/builder*` still requires session, CSRF and `AdmittedMutation` on writes; missing `If-Match` 428, stale CAS 412. Disabled Builder returns 403; Section stays on its private S02-predecessor protocol.
- Builder AST belongs solely to the exact entry and has the same `content_entries.version` as Editor saves, not a shared template version. Editor `values_json` and saved Builder AST remain independent; toggling Builder off never clears either.
- Public content uses AST only with an active, declared, public Builder-enabled CPT, published entry in Builder mode and valid stored document. Otherwise public render falls back to Editor body. Draft/disabled/unpublished Builder-only media remain private; template preview reads are not public grants.
- Migration `0043` reuses the existing `0042` AST/media tables (no data copy/drop), adds a generic scan index and aligns an untouched Core Page's default `has_archive=false` while preserving previously customized Page definitions (`version > 1`). This removes the legacy Page CPT archive URL from fresh sitemap pagination; the M7 boundary fixture now correctly counts two base URLs, with no removed assertions.

## Verification on native macOS Intel

- Admin TypeScript typecheck **PASS**; `npm run build` **PASS**, Vite transformed **100 modules**.
- `python3 cms/scripts/check-standards.py` and `--self-test`: **PASS** (Python 3.13). `check-plans.py` and `--self-test`: **PASS**; `.agents/scripts/check-agent-artifacts.py`: **PASS**.
- `rustfmt --edition 2024 --check` for S01 modified Rust files and `git diff --check`: **PASS**.
- `cargo test --manifest-path cms/Cargo.toml -p opendash-core --release --locked`: **54/54 PASS**.
- Generic CPT S01 Rust integration: **2/2 PASS** (Page/Post defaults, custom CPT capability + 403, CAS 428/412, A/B scope isolation, AST/Editor body preservation, fallback when capability removed and restored).
- Existing Rust regressions against pre-final migration: M6 provider/C4 9/9, Page scope 1/1, Reading S03 1/1, M7 sitemap 1/1, M8 writer barrier 2/2: **14/14 PASS**. M7 sitemap and Reading S03 also re-verified **1/1 each PASS** after finalized `0043` default update and mechanical sitemap fixture adjustment.
- Complete native workspace `cargo test --workspace --release --locked -- --test-threads=1`: **PASS** (73 test-suite results, **336/336 tests PASS, 0 failed, 0 ignored**) with private canonical `TMPDIR`. Includes the corrected legacy `m5_s02_t01_public_context` full 8/8 HTTP matrix for the new default `/content/page` 404, all 154 server unit tests and the Wasmi barrier regression in its unfiltered run. The first full invocation failed one obsolete `Page` archive assertion, and was rerun in full after the mechanical update; **only the final rerun is reported PASS**.
- Generic CPT Chrome E2E (headless native Chrome): **7/7 PASS** on pinned release binary SHA256 `daa8f04b52e9debd624879730f0c6e671d9a24e06daa4b9816b778faa29d7a85`; no uncaught page errors. Verified creation of Builder-enabled CPT, private draft with Editor body, exact entry Builder mode, AST Save, return to Editor preserving body+AST, disabled capability denies public/Builder and hides control, re-enabling recovers exact AST.
- Final rebuilt release binary after Page deep-link and 401 draft fix: **PASS**, SHA256 `ea98c55e2e7b5ed5a84bbe0f5481ce85867adcb15d7c359fc59640131ddd193f`. On that same exact pre-merge binary: Page Builder native Chrome **18/18 PASS**, including dirty draft same-account 401 re-login recovery, 412 CAS, role downgrade/account switch, data/media guards; custom CPT native Chrome **7/7 PASS**; private Section native Chrome **19/19 PASS** (including theme/plugin toggle, native restart, encrypted backup/restore); zero JS errors in all three. The prior Page Chrome attempts on older binaries **failed** on legacy `template=page` navigation and draft recovery respectively; both defects were corrected without deleting tests. Authenticated Admin Shell now keeps a stable `main` element while hidden on 401 so the quarantined Builder draft is not unmounted.
- Worker public-content focused regression `s01_generic_builder_body_and_media_follow_mode_capability_and_publication`: **1/1 PASS** in isolated worker build before integration, with official integrated Cargo workspace run tracked separately.

## Local evidence (ignored, never a real site)

- `.g5-checkouts/evidence/egm-429-native-chrome-final/egm_cpt_builder_s01_receipt.json` and `project-a-builder.png`
- `.g5-checkouts/evidence/egm-429-native-chrome-final.log`
- `.g5-checkouts/evidence/egm-429-s01-targeted-integrated-rerun.log`
- `.g5-checkouts/evidence/egm-429-s01-sitemap-final.log`
- `.g5-checkouts/evidence/egm-429-core-release.log`
- `.g5-checkouts/evidence/egm-429-workspace-release-serial-fixed.log` (complete successful 73-suite release run; earlier failed log retained separately)
- `.g5-checkouts/evidence/egm-429-pinned-bin/opendash` (release binary pinned before subsequent builds)
- `.g5-checkouts/evidence/egm-429-pinned-bin-401-fix/opendash` (final rebuilt binary)
- `.g5-checkouts/evidence/egm-429-page-401-retry/receipt.json` (native Page Builder 18/18)
- `.g5-checkouts/evidence/egm-429-cpt-401-retry/egm_cpt_builder_s01_receipt.json` (native CPT Builder 7/7)
- `.g5-checkouts/evidence/egm-429-section-final/receipt.json` (native private Section 19/19)
- `.g5-checkouts/evidence/egm-429-final-binary-build.log`

All native HTTP/Chrome runs use a disposable, canonical, private site root, separate SQLite and random loopback ports, not `127.0.0.1:18964`. No existing user themes, plugins, uploads or live DB used. No force, no branch-protection bypass.

## Known format baseline / pending closure

- `cargo fmt --manifest-path cms/Cargo.toml --all --check` reports differences in unrelated existing source such as `opendash-core/src/backup.rs` and `contact.rs`. No unrelated mass-formatting; every changed Rust file passes direct rustfmt check. This is **not** represented as a workspace-fmt PASS.
- Required PR CI on the **final** updated PR head and postmerge exact-merged-source evidence belong in the final completion addendum; they are not inferred from earlier head checks or pre-merge native evidence.
- Overall EGM Spec 2 includes subsequent S02/S03, so a successful S01 merge must not be represented as `Verified` for the entire Design Spec.
