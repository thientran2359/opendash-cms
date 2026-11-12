# M4-S09-T01 / #107 — Builder Preview & Save integrated acceptance (G4)

**Scope:** Fast-Track BLD-03 / BLD-03.1; integrated real-Chrome
Preview/Save/ETag/session/theme/List/Binding/Responsive on **one isolated
SQLite fixture**. This is acceptance of the #107 test integration, **not**
a product runtime change, production deployment, or independent G5 sign-off.

## Immutable contract and ownership

| Item | Evidence |
| --- | --- |
| Task | [Issue #107](https://github.com/thientran2359/opendash-cms/issues/107), [G3 integration specification](../plans/milestones/M4/slices/2026-11-11-m4-builder-remaining-g3.md#m4-s09-t01--nghiệm-thu-tích-hợp-preview-và-save) |
| Immutable G3 source | `690dfa1167f11fba701cf8a0cfa4c36cc5ed29a0` |
| G4 starting main | `f917ae1db6312426012d1a05b0f5e6650ba6ba2a` (contains #103, #104, #105, #106 G5 evidence; #106 G4 release merge `09900e6`). |
| Dependency handoff | Forge #102/#103, Harbor #104/#105/#106 verified and merged; [#106 G5 report](2026-11-12-m4-s08-t02-g5-macos.md). |
| Agent transfer | Original approved G3 listed Forge. Owner directed reassignment to **Harbor** on 2026-11-12; **Design Delta D1** on #107 changes only executor/label, not G3 SHA, scope, acceptance or test-file ownership. No Forge branch or PR was overwritten. |
| Changed files | `cms/scripts/builder_editor_integration_browser.py` and this acceptance report, exactly the original G3 file ownership. |
| Environment | macOS 15.8.2, Darwin x86_64/Intel, Cargo 1.99.1, Node v24.22.2, Python 3.13.18, real Google Chrome via Playwright; temp SQLite database and media uploads. |
| G4 test binary | Independently compiled from source `f917ae1db6312426012d1a05b0f5e6650ba6ba2a`; Mach-O x86_64, SHA-256 `970f790a861b2ce1a8c6f2a888725833fe1641507e2bd86fb97a89e59b076e76`. #107 adds test/documentation only, no runtime source changes. |
| Locked release build | **PASS:** Admin `npm ci`, typecheck and Vite build, Rust `cargo fmt --check` and `cargo build --locked --release` on isolated Cargo target. |
| Rust release test suite | **PASS 90/90:** core 29/29 and server 61/61 under `cargo test --workspace --locked --release`. |

## Real integrated test matrix

| Requirement | Result | Observed behavior |
| --- | --- | --- |
| Legacy layout → Builder | **PASS** | A legacy `qa-a/banner` theme block was saved via the M3 endpoint; its projected node then shared a typed Builder document with a Heading, Field Binding, two typed List Blocks and row Responsive overrides. |
| Draft Preview, public Save | **PASS** | New Heading text visible in opaque Preview iframe; public remained at baseline, SQLite row fingerprint/ETag unchanged. After Save, the public homepage changed and ETag advanced once. Preview rendered first page of both lists without draft contents or pagination. |
| Real Chrome two-tab CAS | **PASS** | Two independent browser contexts opened ETag `v3`; first tab saved (200, `v4`), stale second tab Save surfaced **412**, preserving its dirty draft without modifying public data. |
| Session expiry/recovery | **PASS** | The first Chrome session was logged out while retaining an unsaved draft; attempted Save caused 401/login; logging back into the **same account** recovered the draft, which saved as `v5`. |
| CSRF / Origin / auth | **PASS** | Unauthenticated Preview 401; Preview and Save without valid CSRF 403; cross-origin Preview/Save 403. Invalid list page size 21 was rejected with 400; no failing request modified persisted document or ETag. |
| Responsive + Field Binding + List | **PASS** | Same persisted document kept desktop/mobile overrides (including zero), bound `projects/alpha` title, and two independent Post/CPT lists, with escaped titles and canonical URLs. Public per-node pages differed independently; duplicate/noncanonical/out-of-range params did not leak drafts. |
| Theme A → incompatible B → A | **PASS** | Incompatible B stopped rendering the legacy block while core and list content remained public; Preview returned `unsupported_by_theme` diagnostic. JSON for legacy node remained untouched and A re-rendered it after switching back. |
| SQLite and media restart/rollback | **PASS** | SQLite online backup taken before new Chrome Save. Restart retained `v5`, legacy and new content. A **separate** site restored original backup and actual uploaded media bytes; full relevant SQLite table row fingerprint, schema v0014, integrity/foreign keys, original document and public homepage matched before-change state. |
| Browser JS exceptions | **PASS** | No unhandled page errors from either Chrome context. |
| Live production / Windows parity | **NOT_RUN (out of scope)** | Every write, migration and rollback used an ephemeral test site; no live CMS database/asset or production website was touched. |

An integrated invocation printed these markers:

```text
PASS #107: old-format theme + binding + responsive + two lists share one SQLite document
PASS #107 Chrome: draft-only Preview, public Save, two independent tabs -> 200/412
PASS #107 Chrome: same-account 401 draft recovery; A→B→A preserves legacy anchor
PASS #107: restart, SQLite v14/FK/integrity, online snapshot restore and media hashes
PASS #107: integration G4 Chrome/HTTP/SQLite
```

### Independently rerun focused regression suites

All of the following also **PASS** against the same G4 Mach-O release binary:
`builder_editor_browser.py`, `builder_responsive_browser.py`,
`builder_binding_inspector_browser.py`, `builder_list_browser.py`,
`builder_field_binding_smoke.py`, `builder_list_smoke.py`,
`builder_homepage_smoke.py`, `public_content_smoke.py`,
`entry_terms_smoke.py` and `media_smoke.py`. These cover 401/412,
responsive breakpoints/cascade, Inspector lookup failure, category/tag AND,
the 8-list/20-item/100-page limits, public draft/ACL, media uploads and
restart. The optional **M3-binary** restore path in
`builder_homepage_smoke.py` was **NOT_RUN**, because no M3 binary was passed;
this is outside #107's integrated M4-snapshot acceptance.

The new integration test complements rather than replaces these focused
negative suites. All G4 checks ran against the pinned
`970f790a861b2ce1a8c6f2a888725833fe1641507e2bd86fb97a89e59b076e76`
release binary.

## Reproduction

From a checkout that contains all merged #106 prerequisites, build the
corresponding embedded Admin before Cargo (fresh worktrees have no `dist`).
Use a **separate** Cargo target directory to avoid cross-SHA cache mixing:

```sh
export PATH="$HOME/.cargo/bin:$HOME/.nvm/versions/node/v24.22.2/bin:$PATH"
npm ci --prefix cms/admin --prefer-offline --no-audit
npm run typecheck --prefix cms/admin
npm run build --prefix cms/admin
cargo fmt --manifest-path cms/Cargo.toml --all -- --check
export CARGO_TARGET_DIR="$PWD/cms/target"
cargo build --manifest-path cms/Cargo.toml --locked --release
cargo test --manifest-path cms/Cargo.toml --workspace --locked --release
python cms/scripts/builder_editor_integration_browser.py \
  --binary cms/target/release/opendash \
  --browser "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
python cms/scripts/builder_editor_browser.py \
  --binary cms/target/release/opendash --browser "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
python cms/scripts/builder_responsive_browser.py \
  --binary cms/target/release/opendash --browser "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
python cms/scripts/builder_binding_inspector_browser.py \
  --binary cms/target/release/opendash --browser "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
python cms/scripts/builder_list_browser.py \
  --binary cms/target/release/opendash --browser "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
python cms/scripts/builder_field_binding_smoke.py --binary cms/target/release/opendash
python cms/scripts/builder_list_smoke.py --binary cms/target/release/opendash
python cms/scripts/public_content_smoke.py --binary cms/target/release/opendash
python cms/scripts/entry_terms_smoke.py --binary cms/target/release/opendash
python cms/scripts/builder_homepage_smoke.py --binary cms/target/release/opendash
python cms/scripts/check-plans.py
python cms/scripts/check-plans.py --self-test
python cms/scripts/check-standards.py
python .agents/scripts/check-agent-artifacts.py
git diff --check
```

The runner prints a new fingerprint for every disposable fixture. The
fingerprint is compared against its own SQLite online backup and restored
database, not a hardcoded magic hash. Raw logs, x86_64 release binary and
temporary site assets remain local, not in the Git history.

## Review / release boundary

**G4 acceptance on the integrated fixture: PASS.** All tests that involve
runtime behavior use the existing Builder APIs and renderer; #107 adds no
schema migration, production route, ACL or runtime code change. Any
newly-discovered runtime defect belongs with the owning implementation task
and must be fixed/reverified before release; the test cannot weaken
acceptance to hide a defect.

**G5 is separate:** after this G4 PR passes required governance CI and
merges, rerun the same integration and mandatory regressions against a Mac
Intel release binary compiled from that **exact immutable merge SHA**,
recheck snapshot restore, publish G5 evidence in a separate PR and only
then close Issue #107/mark Project Done and synchronize `TASK_INDEX.md`.

Agent: **Harbor (macOS Intel / Chat On Steroids)** · 2026-11-12.
