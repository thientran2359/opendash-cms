# M6-S11-T01 / #378 — C4 provider CPT, taxonomy, rewrite and Wasmi (G4 candidate)

Date: 2026-11-19. Agent: Harbor. Host: macOS 15.8.2 (Darwin 24.6.0), Intel x86_64.
**Status: implementation candidate, not G4/G5 accepted.** One actionable Admin UI
P2 requires additional coordinator file ownership. Do not merge or close this
task on the strength of this premerge report.

## Authority and source

- [Issue #378](https://github.com/thientran2359/opendash-cms/issues/378)
  is In progress on GitHub Project #3; Harbor implements, Aster coordinates.
  [Approved M6 G3](../plans/milestones/M6/slices/2026-11-18-content-foundation-g3.md)
  source: 695fe94a7c6c6f24e0dd35629a32e8e2ace3bf4d.
  Original inspected baseline: 2ebe51cbe1099c4721304e2b31895aa78b7c1241.
- Predecessor #377 was merged and had separate native macOS G5 (PR #403),
  Project Done before pickup. Integration parent:
  cb76f21a2094ad8a1807addc1fe3187e8480475e.
- Implementation commit: 2add6f029e8499f30c580b5a786ea96d570ebdae;
  tree: a46cc405e781a898889cf0ef82d34225d80409ed.
  Branch: agent/harbor. Git worktree author: Harbor.
- Aster's Issue #378 ownership decision allows the additional C3 storage
  provider-rewrite integration in cms/crates/opendash-server/src/storage/permalinks.rs
  and optional forward migration cms/migrations/0041_provider_rewrite_defaults.sql.
  This implements [C4](../contracts/CONTENT_FOUNDATION_2026_11.md) without
  changing the approved G2 contract or historical migration 0040.

## Implementation and design delta

Optional old-compatible manifest properties now describe provider-owned custom
taxonomies, CPT settings, associations and rewrite bases. The same core
validators reject invalid identifiers, duplicate keys, hierarchy conflicts
and provider ownership takeover. Package registration, provider lifecycle,
revisions and existing C3 namespace claims share a SQLite write transaction.
Failed installs leave the previous active revision and public URL intact.

Migration 0041 creates a separate provider rewrite-default table keyed by
resource kind and stable key, with owner/status/declaration metadata. No
old site choices are backfilled or reinterpreted. At resolution, a site
non-null choice or explicit null beats a package default; only a missing
site map key inherits the current active provider default. Provider changes
advance the C3 settings CAS version when effective URL configuration changes.
The existing C3 collision validation, route redirect history, publication
guards and 409 rollback remain the authority for public URLs. Inactive
provider data and term memberships persist for Admin management.

SDK and sample package documentation describe declarations separately from
real Wasmi ContentBeforeSave Allow/Veto, ContentAfterSave and permission-scoped
ContentSave. Arbitrary content mutation hooks, URL filters and executable
theme code remain unsupported.

## Actual macOS candidate checks

All tested instances used temporary isolated SQLite/site roots and random
loopback ports. The owner live site and port 18964 were not touched.

| Check | Observed result |
| --- | --- |
| cargo test --manifest-path cms/Cargo.toml --locked -p opendash-core (worker core tree) | PASS 44/44, including legacy serialization |
| cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test content_foundation_m6_s11_t01 -- --test-threads=1 | PASS 7/7, real native Wasmi/HTTP/SQLite: old ZIP/ContentSave, Allow/Veto/after-save, custom taxonomy, CAS, generation fence, collisions, theme switch and hierarchy downgrade |
| cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --bin opendash storage::permalinks::tests:: -- --test-threads=1 | PASS 18/18, migration, sparse/null defaults, provider updates, C3 route claims and 50k boundary |
| TMPDIR set to a private canonical 0700 directory; cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server -- --test-threads=1 | PASS, exit 0; full existing server integration suite, including M8 backup/restore. This run predated the final additional C4 tests and narrow provider-status adjustment, so it is not exact-head full-suite proof |
| npm run build --prefix cms/admin | PASS, 98 modules; existing JS chunk warning |
| cargo build --manifest-path cms/Cargo.toml --locked | PASS, native x86_64 Mach-O debug binary |
| python3.13 -m py_compile cms/scripts/content_foundation_m6_s11_t01_e2e.py | PASS |
| Locally isolated Playwright Chrome runner with --binary, --admin-dist and --artifact-dir | PASS 10/10 native Admin/API/SQLite/restart; four screenshots, byte-for-byte embedded Admin index, JS and CSS |
| .g5-checkouts/evidence/c4-playwright-venv/bin/python cms/scripts/content_foundation_m6_s11_t01_backup_e2e.py --binary cms/target/debug/opendash --artifact-dir .g5-checkouts/evidence/c4-native-backup | PASS 6/6 isolated C4 encrypted backup/restore: provider-default CPT/term, snapshot, post-snapshot site mutation, recovery activation, preserved canonical URL/ownership and re-edit |
| rustfmt --edition 2024 --check on the new focused Rust test, plus git diff --check | PASS |

The C4 Chrome runner covers 1280 px and mobile 390 px viewports, isolated Admin,
provider declared CPT and terms, draft and published canonical routes, site's
explicit null override, package update, disable/re-enable, SQLite integrity
and FK validation, owner retention, and restart. The runner stores a local
ignored receipt at .g5-checkouts/evidence/c4-native-browser/receipt.json with
status PASS, port_18964_used false and four image hashes.

Native binary SHA-256: 40fec6ca7f997921e14ac70c612be1a869adf75e486c7469e265c0e7b04844a8.
Admin index SHA-256: a918cace6a5e68e94c1da3bc56f4d9879167e66c3d2a83bfc610d28170178fe2.
Admin JS index-CBZKbQ9J.js: 33978d59d94b6997f2a243901a1060a00e3494a7c8d30e2919e4b77a96ec965c.
Admin CSS index-FE7M3lRd.css: ec113de7634d17c72b958b1944400c65060760846eabcd135bb93e50d1b8b292.
Migration 0041 SHA-256: 7c4b8eb52262f5d19f734899e606ba2d63ac20250673d8ed691260fc6cd8322b.
The pre-existing sample plugin ZIP remains unchanged at SHA-256
b04306779e1a76381ec0ce3e00d416d7b98ce2da045b59845cefb30cdaab2a37.

The separate C4 encrypted backup/restore receipt at
`.g5-checkouts/evidence/c4-native-backup/receipt.json` records PASS, six named
checks and `port_18964_used=false` with a matching native candidate
binary SHA-256 `d2a2d569fa576aba7d5dbd1730c280cf3546ee5897f6dba50f9a27a3d1dde81b`.
The new test runner uses isolated source and restore targets, verifies that a
post-snapshot explicit-null site override disappears after encrypted restore,
then confirms provider-owned term memberships and successful edit on the
restored site after safe-mode recovery. This is candidate evidence, not
exact-merged G5.

Receipt screenshot hashes:

- c4-taxonomy-browser.png: 1f97626faa22a4640c12a565ab48bb80315a7fdece363b2ebb40f664c9c6da0a
- c4-permalink-default.png: a6c9ba21500810c99da00651a4be128e6676b505fd3bdd5174fa0e4bd3783808
- c4-explicit-null-saved.png: 09beafed9fea9a9fc66e879635374f2d28de9d16ad8bd600a5af038911f9b2d6
- c4-permalink-mobile-390.png: 152a2272d4358858d78fa64dfa42394987a70f645e799f48c8fd5a8d1ad2b577

Initial failures were not counted as passing evidence: original full Rust unit
run had 108 pass/9 fail due macOS /var symlink and temp-root ACL, corrected
by private canonical TMPDIR; preimplementation C4 ZIP parser rejected new
manifest keys as expected TDD RED; first Chrome runner used an ambiguous text
locator, corrected and rerun PASS; one new test initially supplied malformed
If-Match and was corrected and rerun PASS. Generated binaries, screenshots,
site DBs and credentials are intentionally excluded from the Git commit.

## Independent reviews and remaining gates

- Worker-1 read-only static C4 provider/theme/taxonomy audit: PASS, no
  actionable P1/P2 confirmed in examined backend.
- Worker-2 independent C3/consumer audit found one actionable P2 at
  cms/admin/src/features/settings/PermalinkSettingsPage.tsx around lines
  329–381. An inherited provider default (site key absent) and a saved
  explicit null both render a blank textbox; the UI incorrectly says blank
  means legacy and provides no action to delete a saved key and restore
  provider inheritance. Fix requires distinguishing inherited versus explicit
  legacy and adding a remove-key control. This UI file is not within the
  current approved Issue #378 ownership. **Await explicit bounded Aster ACK**
  before editing. The current native Chrome runner proves inherited→null,
  not null→inherited.
- Encrypted C4-specific backup→restore→re-edit is now PASS on a matching
  candidate binary. Provider/C3 concurrent writer pressure and independent
  exact-merged macOS release G5 remain NOT_RUN; the full M8 suite alone does
  not replace those separate receipts.
  A distinct public plugin-uninstall API was not exercised; package updates,
  disable/re-enable, theme switch and reactivation were.
- PR-head governance CI, exact-head self-review, G4 merge, exact-merged G5
  and Project Done are still pending. No Windows/Linux release, Beta #302,
  common RC, human-test or deployment claim is made.
