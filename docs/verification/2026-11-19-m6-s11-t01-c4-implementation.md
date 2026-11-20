# M6-S11-T01 / #378 — C4 provider CPT, taxonomy, rewrite and Wasmi (G4 candidate)

Date: 2026-11-19. Agent: Harbor. Host: macOS 15.8.2 (Darwin 24.6.0), Intel x86_64.
**Status: implementation candidate, not G4/G5 accepted.** The Admin UI P2 was
corrected under Aster's bounded Issue #378 ACK 7014797743 and RESUME_CURRENT
authorization 7015143039. Exact-head PR CI/self-review and separate merged-source
native G5 remain independently gated; do not close this Issue from this report.

## Authority and source

- [Issue #378](https://github.com/thientran2359/opendash-cms/issues/378)
  is In progress on GitHub Project #3; Harbor implements, Aster coordinates.
  [Approved M6 G3](../archive/plans/milestones/M6/slices/2026-11-18-content-foundation-g3.md)
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
- Aster's subsequent [bounded Admin ownership ACK](https://github.com/thientran2359/opendash-cms/issues/378#issuecomment-7014797743)
  permits exactly cms/admin/src/features/settings/PermalinkSettingsPage.tsx,
  this task's Chrome runner, and verification evidence. The
  [resume authorization](https://github.com/thientran2359/opendash-cms/issues/378#issuecomment-7015143039)
  pinned Draft PR #406 head f66ba19d24f6a1d3990336b03a3e41b5c9ab4693
  and base cb76f21a2094ad8a1807addc1fe3187e8480475e before editing.

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
| cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test content_foundation_m6_s11_t01 -- --test-threads=1 | PASS 9/9 in Harbor prime and independent worker; real native Wasmi/HTTP/SQLite: old ZIP/ContentSave, Allow/Veto/after-save, custom taxonomy, CAS, generation fence, collisions, theme switch, hierarchy downgrade, provider rewrite add/change/remove with 409 rollback and retained redirects, and populated on-disk 0040→0041 upgrade |
| cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --bin opendash storage::permalinks::tests:: -- --test-threads=1 | PASS 18/18, migration, sparse/null defaults, provider updates, C3 route claims and 50k boundary |
| TMPDIR set to a private canonical 0700 directory; cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server -- --test-threads=1 | PASS, exit 0; full existing server suite on the final C4 production source, including M8 backup/restore. The two subsequently added C4 test-only cases were separately executed with focused target 9/9 PASS. Full log: .g5-checkouts/evidence/c4-full-suite-head.log (ignored local receipt) |
| npm run build --prefix cms/admin | PASS, 98 modules; existing JS chunk warning |
| cargo build --manifest-path cms/Cargo.toml --locked | PASS, native x86_64 Mach-O debug binary |
| python3.13 -m py_compile cms/scripts/content_foundation_m6_s11_t01_e2e.py | PASS |
| Initial locally isolated Playwright Chrome runner with --binary, --admin-dist and --artifact-dir | PASS 15/15 native Admin/API/SQLite/restart; four screenshots, byte-for-byte embedded Admin index, JS and CSS. Superseded by the bounded UI acceptance below. |
| .g5-checkouts/evidence/c4-playwright-venv/bin/python cms/scripts/content_foundation_m6_s11_t01_backup_e2e.py --binary cms/target/debug/opendash --artifact-dir .g5-checkouts/evidence/c4-native-backup | PASS 6/6 isolated C4 encrypted backup/restore: provider-default CPT/term, snapshot, post-snapshot site mutation, recovery activation, preserved canonical URL/ownership and re-edit |
| rustfmt --edition 2024 --check on the new focused Rust test, plus git diff --check | PASS |

The C4 Chrome runner covers 1280 px and mobile 390 px viewports, isolated Admin,
provider declared CPT and terms, draft and published canonical routes, site's
explicit null override, package update, disable/re-enable, SQLite integrity
and FK validation, owner retention, and restart. Five additional API checks
prove a full-form GET→PUT keeps absent site keys sparse, explicit null→inherit,
site non-null choice→inherit, provider update increments C3 CAS and rejects stale
If-Match with 412, and stable-ID redirects do not form chains. The runner stores a local
ignored receipt at .g5-checkouts/evidence/c4-native-browser/receipt.json with
status PASS, port_18964_used false and four image hashes.

Pre-UI candidate binary SHA-256: d2a2d569fa576aba7d5dbd1730c280cf3546ee5897f6dba50f9a27a3d1dde81b.
Pre-UI Admin index SHA-256: a918cace6a5e68e94c1da3bc56f4d9879167e66c3d2a83bfc610d28170178fe2.
Pre-UI Admin JS index-CBZKbQ9J.js: 33978d59d94b6997f2a243901a1060a00e3494a7c8d30e2919e4b77a96ec965c.
Pre-UI Admin CSS index-FE7M3lRd.css: ec113de7634d17c72b958b1944400c65060760846eabcd135bb93e50d1b8b292.
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
- c4-permalink-default.png: 551383f79d60b31a749b1e1a8025b85ec2233bbb424c4d141cc8d69d3d19ae0b
- c4-explicit-null-saved.png: 011d1362171fbda6fab05bfd046bae915e5b785276d5ed8a52c89d54270f0094
- c4-permalink-mobile-390.png: 25c540a86e25493f58255c191b4515e1bf7c00df4db3813e3544fe09d71c7ca1

Initial failures were not counted as passing evidence: original full Rust unit
run had 108 pass/9 fail due macOS /var symlink and temp-root ACL, corrected
by private canonical TMPDIR; preimplementation C4 ZIP parser rejected new
manifest keys as expected TDD RED; first Chrome runner used an ambiguous text
locator, corrected and rerun PASS; one new test initially supplied malformed
If-Match and was corrected and rerun PASS. Generated binaries, screenshots,
site DBs and credentials are intentionally excluded from the Git commit.

## Bounded Admin UI correction — native macOS acceptance

After ACK 7014797743, the Admin CPT/taxonomy rows distinguish three *site*
choices using own-property membership in each sparse draft map:

- Absent key: inherit the active provider rewrite default, or the legacy
  family when the provider has no applicable default.
- Present `null`: explicitly opt out of provider default and use the legacy
  family for this site.
- Present nonempty string: use the site's custom base.

The UI has separate keyboard-accessible **Dùng đường dẫn legacy** and
**Kế thừa mặc định** controls per active row; inheritance removes only its own
key from a cloned draft object with `delete`. Inactive-provider rows remain
uneditable and expose only the existing pending-draft revert action. No new
HTTP DELETE operation, DTO, schema, historical migration, or API change.
Existing 401 draft RAM scope, 409/412 stale guards, dirty comparison and
full-form C3 PUT remain authoritative.

The following results were observed on the candidate macOS Intel host, each
using isolated root/SQLite and random loopback port:

| Exact command / scope | Observed result |
| --- | --- |
| `npm run typecheck --prefix cms/admin` | PASS, exit 0 |
| `npm run build --prefix cms/admin` | PASS, 98 modules, existing chunk size advisory |
| `cargo build --manifest-path cms/Cargo.toml --locked` | PASS, debug x86_64 server with compiled Admin |
| `python3.13 -m py_compile cms/scripts/content_foundation_m6_s11_t01_e2e.py` | PASS |
| `.g5-checkouts/evidence/c4-playwright-venv/bin/python cms/scripts/content_foundation_m6_s11_t01_e2e.py --binary cms/target/debug/opendash --admin-dist cms/admin/dist --artifact-dir .g5-checkouts/evidence/c4-native-browser-ui` | **PASS 23/23**, six native Chrome screenshots and byte-exact embedded assets; `port_18964_used=false` |
| `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test content_foundation_m6_s11_t01 -- --test-threads=1` | **PASS 9/9**, C4 provider/Wasm/migration HTTP/SQLite |
| `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --bin opendash storage::permalinks::tests:: -- --test-threads=1` | **PASS 18/18**, C3 CAS/claims/history/50k-claims |
| `.g5-checkouts/evidence/c4-playwright-venv/bin/python cms/scripts/content_foundation_m7_s17_t02_e2e.py --binary cms/target/debug/opendash --admin-dist cms/admin/dist --artifact-dir .g5-checkouts/evidence/c4-native-c3-regression` | **PASS**, eight C3 API/native checks and ten independent Chrome checks, including published/draft privacy and stale/409 retained drafts |
| `.g5-checkouts/evidence/c4-playwright-venv/bin/python cms/scripts/content_foundation_m6_s11_t01_backup_e2e.py --binary cms/target/debug/opendash --artifact-dir .g5-checkouts/evidence/c4-native-backup-ui` | **PASS 6/6**, encrypted backup/restore/recovery on same candidate generation |
| `git diff --check` | PASS |

Chrome verified both CPT and taxonomy flows: inherited→explicit legacy→inherited
and custom→inherited, absent/null/nonempty JSON choices, unrelated Post save
retaining sparse maps, unchanged public URLs before Save, stable-ID redirects
after Save, browser reload and process restart, no-provider legacy fallback,
provider upgrade, native keyboard and 390 px controls. Its 409 test confirms
no conflicting route/site mutation and retains the form draft; its 412 test
confirms newer provider CAS cannot be bypassed by toggling choices or rechecking
the server version. A real expired session recovers the same user's RAM draft
after login and does not disclose it to another account. Inactive provider
changes retain or revert safely.

The runner also starts **two concurrent HTTP connections** behind a
`threading.Barrier`: one full-form settings PUT and one provider package
upgrade. The observed status was settings **200**, provider **201**; in either
valid ordering a stale settings 412 is also acceptable. Assertions inspect
the whole final sparse/explicit map, provider ownership provenance in SQLite,
integrity and foreign keys, canonical routes and redirects, and post-restart
URLs. The race exercised real writer overlap but does not prove every scheduler
interleaving. C4 uninstall through a separate public endpoint remains
untested; package update/disable/re-enable/theme switches were exercised.

Final corrected-UI native candidate (rebuilt and browser-tested after all
Cargo tests), all hashes SHA-256:

- Binary `cms/target/debug/opendash`:
  `c03a86c03dcbb0fc62ad5ddef93d6082f2576cc2b70e7e06585a4464d94b983d`.
- Admin `cms/admin/dist/index.html`:
  `2abb70657de1a07b18cc6433da35ef4c4fcffce6f0292ca15b4bacd27feea1b8`.
- Admin JS `index-BpgMsRsS.js`:
  `6a7c76e661b0af4904068db7eb91d0a2b9f60666e54c66fc74c102167406225a`.
- Admin CSS `index-FE7M3lRd.css`:
  `ec113de7634d17c72b958b1944400c65060760846eabcd135bb93e50d1b8b292`.
- Final six browser screenshots recorded in receipt:
  `c4-taxonomy-browser.png` (`8dc7b3f5cbfa137791560b342d9aa8ef3ce94b6841d23cc89e41ce6f115632fa`),
  `c4-permalink-default.png` (`3e0d34f60d296be7f33d769fa01235d315937c56f528b48ee611bfa25e7a1bd4`),
  `c4-explicit-null-saved.png` (`e01897a9d7901cfab9083ec248dd5c26d0ab59e9b750721c287dfb32d344b767`),
  `c4-permalink-mobile-390.png` (`431a90d8b6814e91c16c58484ee3ed74d09f374472f0198f9b78b3ecd5fd3ff8`),
  `c4-ui-stale-412.png` (`cdf1792bf1f3ad8843d62bbe11b226e81a9c790219937635ade86a2aa2895782`),
  `c4-ui-conflict-409.png` (`6d5a0b089dc01b127fa1632898aea52bf1773ae035811819831ad53a94997a59`).

Local ignored receipts:
`.g5-checkouts/evidence/c4-native-browser-ui/receipt.json`,
`.g5-checkouts/evidence/c4-native-c3-regression/`, and
`.g5-checkouts/evidence/c4-native-backup-ui/`. Screenshots and site databases
are not committed. Exact merged-source release G5 still requires a separate
native test and receipt after merge.

## Independent reviews and remaining gates

- Worker-1 read-only static C4 provider/theme/taxonomy audit: PASS, no
  actionable P1/P2 confirmed in examined backend.
- Worker-2 identified the inherited/explicit-null Admin UI ambiguity.
  Aster approved bounded ownership in Issue comment 7014797743; the UI
  correction and full native Chrome acceptance above address that finding.
  Final exact-head independent review and CI are separate gates.
- Encrypted C4-specific backup→restore→re-edit is PASS on an isolated
  candidate binary. Provider/C3 concurrent writer acceptance was exercised,
  including the observed 200/201 successful overlap. Independent exact-merged
  macOS release G5 remains NOT_RUN; the full M8 suite alone does not replace it.
  A distinct public plugin-uninstall API was not exercised; package updates,
  disable/re-enable, theme switch and reactivation were.
- PR-head governance CI, exact-head self-review, G4 merge, exact-merged G5
  and Project Done are still pending. No Windows/Linux release, Beta #302,
  common RC, human-test or deployment claim is made.
