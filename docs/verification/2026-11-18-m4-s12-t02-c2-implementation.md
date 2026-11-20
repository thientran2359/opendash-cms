# M4-S12-T02 / #375 — Custom taxonomy Admin and Builder implementation

**Agent:** Harbor (implementation/self-review); **coordinator:** Aster. **Date:** 2026-11-18.
**Governance:** [Issue #375](https://github.com/thientran2359/opendash-cms/issues/375),
[immutable G3](../archive/plans/milestones/M4/slices/2026-11-18-content-foundation-g3.md)
at `695fe94a7c6c6f24e0dd35629a32e8e2ace3bf4d`,
[ADR-0019](../decisions/ADR-0019-content-foundation.md),
[C2 contract](../contracts/CONTENT_FOUNDATION_2026_11.md).
**Prerequisite:** #374 technical native macOS Intel G5 Verified, exact merged source
`86efbc8382f7d6878cb0cae501bbe794b7223ef5`.
**Initial implementation base:** `afb3363f0b20319e244812c1500e345077d31eac`.

## Implementation coverage

- Admin taxonomy definitions and term routes support custom flat/hierarchical
  kinds and retain legacy Category/Tag aliases. Core Category/Tag labels and
  public flags remain Admin-editable while their keys/hierarchy and deletion
  remain locked; provider-owned definitions remain read-only. Editor can edit permitted term
  extensions, but not definitions/new taxonomy schemas.
- CPT Settings and the definitions screen edit the **complete configured**
  association list with CPT-version CAS, not only the filtered `effective` list.
  A user edit cannot silently erase dormant provider defaults/overrides;
  `section` has no taxonomy control and Page has no core taxonomy by default.
- Single Edit uses effective eligible taxonomies and preserves unknown saved
  memberships rather than deleting associations on schema withdrawal.
  Entry tables support a strict `taxonomy_key` + `term_id` pair, optional
  hierarchical descendants, without colliding with Category/Tag aliases.
- Builder AST accepts generic taxonomy pairs while preserving existing
  Category/Tag serialization. Storage passes the generic pair to the published
  list resolver **before pagination**. Missing/withdrawn/inactive/flat-incompatible
  saved references degrade to empty `MissingConfig` instead of a public 400;
  incomplete/mixed direct query requests remain 400. Drafts stay nonpublic.
- Taxonomy definition/term edit forms retain in-memory, account/site-scoped
  drafts across a 401 sign-out and same-account reauthentication, with
  version/schema-token conflict handling and explicit discard. Other accounts
  cannot restore a previous actor's form. User-approved route navigation that
  discards a draft is explicitly disclosed.
- CPT Settings taxonomy eligibility draft follows the same site/account/CPT
  scope, restores after 401, and rebases 412 by reapplying only the user's
  changed associations onto latest configured server associations (including
  inactive provider keys). A pending response from an unmounted former auth
  session cannot clear the current user's newer RAM draft or revoke a newer
  session through a stale 401 callback.
- The previously mutable `site_instance_id` has been bounded to the existing
  canonical SQLite DB path and backing-file identity (Unix device/inode) rather
  than editable site title or first surviving Admin timestamp. No new migration
  or mutable runtime setting was introduced. If a reliable backing file is
  unavailable, login sends no verified site ID and the Admin disables RAM
  recovery instead of inheriting a previous site's identity. Moving/restoring
  the actual DB file can intentionally invalidate in-memory drafts; in-place
  overwrite preserving the file identity is not a cryptographically unique
  persisted installation generation. A persistent UUID would require a
  separately coordinated storage/migration contract change.

### Bounded Design Delta — coordinator ownership ACK received

The original #375 exclusive file list does not enumerate all required
storage integrations. `storage/builder.rs` had to forward generic filters,
`storage/entries.rs` had to gate persisted Builder references in one DB
snapshot, and `storage/content.rs` plus `routes/taxonomy.rs` expose the
unfiltered configured keys required to prevent provider-association data loss.
`storage/entries.rs` also guards hierarchical→flat saved-list transitions.
`app.rs` is a permitted common route-wiring file under G3. These are bounded
C2 correctness/security integrations, **not** authority to change immutable G3,
schema migration ownership, or #376 public route contracts. **Coordinator Aster
explicitly ACKed** this bounded file ownership extension, including
`admin/builder/document.ts` and `PanelRegionRenderer.tsx`, in the review on
[PR #390](https://github.com/thientran2359/opendash-cms/pull/390).
The follow-up scoped `storage/auth.rs` correction ensures 401 draft recovery
does not silently lose data after renaming a site or deleting the first Admin;
it too is included in Aster's ACK. This does not waive the four P2 findings
in that same review, required new-head CI or exact merged-source G5.

### Scope boundary with #376

Menu picker displays eligible custom taxonomy terms, but #375 does **not**
invent a custom-term public permalink. Canonical term URL generation and
routing belong to #376/C3. A selected unsupported term target must not produce
a fabricated public href. Custom menu-target acceptance is constrained by
this preexisting C2/C3 boundary and must be rechecked after #376.

## Premerge validation and independent review

Host: **macOS 15.8.2 Intel x86_64**, Chrome **158.0.8127.36**.
All commands run in an isolated workspace; Chrome acceptance uses temporary
SQLite/uploads and random loopback ports, **never port 18964**. Runtime
receipts, credentials, generated sites, and binary outputs remain ignored.

| Check | Observed |
| --- | --- |
| `npm run build --prefix cms/admin` | PASS (97 modules; existing large-chunk warning) |
| `cargo test --locked -p opendash-core` | **39/39 PASS**, final worktree |
| `cargo test --locked -p opendash-server --test content_foundation_m4_s12_t02` | **2/2 PASS** |
| `cargo test --locked -p opendash-server --bin opendash storage::entries::tests::builder_` | **3/3 PASS**, earlier targeted check |
| `TMPDIR=<isolated-artifact-tmp> cargo test --locked -p opendash-server --bin opendash -- --test-threads=4` | **95/95 PASS**, latest corrective worktree |
| `cargo build --locked -p opendash-server` | PASS |
| `python3 -m py_compile cms/scripts/content_foundation_m4_s12_t02_e2e.py` | PASS |
| `python3 cms/scripts/check-plans.py` and `--self-test` | PASS, 12 fixtures |
| `git diff --check` | PASS |
| Initial isolated Chrome/API/SQLite acceptance | **11/11 named checks PASS** on prior premerge snapshot; superseded by expanded final run |
| Native isolated Chrome/API/SQLite before Aster review | **19/19 named checks PASS**, exit 0, Chrome 158, Intel x86_64 |
| Native isolated Chrome/API/SQLite after four Aster P2 corrections | **23/23 named checks PASS**, exit 0, Chrome 158, Intel x86_64 |

The expanded native acceptance runner is
`cms/scripts/content_foundation_m4_s12_t02_e2e.py`; it exercises custom
definition hierarchy, term parent restrictions, CPT associations/version
CAS, Page defaults, generic filtering, public Builder visibility and
withdrawal, inactive-provider preservation, hierarchy→flat safe degradation,
UI association editing, same-user 401 draft recovery, 390px responsive layout,
SQLite integrity/FK/membership, and native restart persistence. Its final
run additionally covers cross-account 401 quarantine, distinct site roots with
matching first Admin timestamp, deleting first Admin without identity rotation,
site-title change without identity rotation, CPT Settings 401 restoration and
explicit discard, and CPT Settings 412 rebase preserving a concurrent taxonomy
association. **All 19 named checks PASS**. The premerge debug binary SHA-256 is
`b93684815670e8006f049b3a682300a6441aed9d36f471055e5d17d25a3d7639`;
runner SHA-256 is
`f7a11a16316b486f6ea53df2d9a321dd2b8c19033aba518142cf1d25b466623c`.
Ignored receipt: `.g5-checkouts/evidence/c2-t02-premerge-final/receipt.json`.
Screenshot SHA-256 values: `taxonomy-definitions-desktop.png`
`6673ba76847e71e89ffb30cd2edb3af65f788c9fab9dc54435ec9b35b489c9b7`,
`taxonomy-terms-desktop.png`
`5af45319fb940a42966a023d45cf45ad9309ad236fd5b7d6a806420756fb8e39`,
`single-edit-taxonomy-desktop.png`
`3ddd2f8ff9400dc8d04fe0f2a2c98ea5d83dfed9b03d738e3950783a876e5956`,
`single-edit-390.png`
`09f525e118b07859397abdb1fa36760ef9fe1246d1d871ac40b74d84386bae26`.
The receipt correctly marks the final **dirty premerge source** with changed
paths and commit `45568c7`; it is not an exact merged release provenance.
Earlier expanded runner attempts that failed on stale CPT options, a selector
label mismatch or a transient toast check are retained as negative runner
diagnostics, never represented as passing product checks.

Independent reviewers found: missing Builder filter propagation (fixed),
silent dormant association loss (configured GET and full selection fixed),
hierarchy→flat saved-list public 400 (safe `MissingConfig` fixed),
incomplete table filters (fixed), Editor term-edit route blocked (fixed),
double navigation on dirty CPT schemas (fixed), and taxonomy 401/412 form
loss/conflict (scoped in-memory recovery and rebase integrated). Subsequent
independent review reported P2 site identity instability and stale 401/late
success cross-account races. The last two follow-ups are corrected in
`storage/auth.rs`, the scoped App callbacks, and unmount guards in all three
forms. The earlier internal read-only review reported PASS within that review's
scope, but did not replace Aster's later official review; see the four P2
corrections and new acceptance tests below.

### PR #390 — Aster review corrective round

At reviewed head `818b1cedd3b43408be8c352f9399f29d73896476`, Aster
approved the bounded ownership Design Delta but requested **four P2 fixes**
([PR review](https://github.com/thientran2359/opendash-cms/pull/390)).
The implementation corrections are:

1. **Atomic CPT version/association snapshot:**
   `storage/content.rs::load_configured_taxonomies` reads `type_version`,
   persisted `taxonomies` and `effective` within one SQLite read transaction.
   Both Admin forms retain this CAS version; a mismatched independently fetched
   CPT version blocks save/rebase, preserving local unsaved selections until
   retry. PUT still requires If-Match on the current version; the same response
   supplies the new committed version. No cross-snapshot version assembly.
2. **Provider-owned CPT overrides:** Site Admin can configure taxonomy
   eligibility for existing plugin/theme CPTs independently of provider-owned
   schema edit permissions; Editor/Section restrictions remain unchanged.
   Inactive provider defaults/overrides stay preserved across reactivation.
3. **Entry table labels:** All effective custom taxonomy columns resolve term
   names, not just the selected filter. Subsequent performance review replaced
   the full per-taxonomy catalog scan with a bounded, authenticated bulk
   projection of **only IDs on the visible entry-table page** (100 per request,
   2 concurrent requests). Filter catalogs load lazily on focus. Legacy
   Category/Tag and strict pair filtering remain supported.
4. **Core Category/Tag definition edits:** Admin can update `label` and
   `public` on Core definitions; `key`, `hierarchical`, and delete remain
   prohibited for Core. Provider-owned definitions remain immutable to Admin.

New Chrome/API/SQLite premerge acceptance checks exercise: deliberately
**out-of-order configured vN/list vN+1** responses during Definitions rebase
(Save blocked, old draft retained), successful retry preserving a concurrently
added `tag` association, visible names for **two** custom taxonomy columns
both unfiltered and filtered by only one, Core Category edit/reload while key
and hierarchy stay immutable, and plugin-owned CPT site override while schema
remains read-only and provider deactivation/reactivation preserves data.
**Native result: 23/23 PASS** on macOS Intel, Chrome 158; ignored receipt:
`.g5-checkouts/evidence/c2-t02-aster-review/receipt.json`, debug binary
SHA-256 `4af1ff426b085410b3fefc9d49b811ebbc59b77136b344403b4c67e363e32f15`,
runner SHA-256
`7a6090ae0bde8f2a3c308fbb4a211afc181ec0d5feb4aac64c009f406357fb04`.
Earlier failed runner receipts from premature UI reads, stale browser state
after an external provider-owner DB change, and pre-sync embedded Admin binaries
are retained as diagnostics, **not successful product tests**. That 23/23 was
superseded by subsequent expanded acceptance; it is **NOT G5**.

### Independent follow-up P2 findings after `b6aac15`

The internal cross-review of the new GitHub head found further correctness
issues not included in Aster's original four comments. These are corrections
within #375, not new scope or acceptance waivers:

- **Provider redeclaration / stale Admin CAS:** Package CNT synchronization
  previously added, withdrew or restored provider eligibility without changing
  the CPT version. An Admin could therefore PUT an old site override with a
  still-valid version, silently masking an association added by a plugin in the
  meantime. `storage/content.rs` now bumps the owning existing CPT's version
  exactly once per changed provider-eligibility or meaningful provider CPT
  schema/status sync, in the same writer transaction. Plugin enable/disable
  and theme deactivation also advance CPT CAS when provider status changes,
  including eligibility-only transitions of a withdrawn provider CPT. Each
  affected CPT is bumped once even if both its own state and links change.
  Rust regression covers association addition, schema-only re-declaration,
  stale CAS rejection, idempotence, provider status and withdrawal without
  deleting the link row.
- **Out-of-order configured GETs:** Definitions selector, manual reload and
  rebase now share a request epoch. A stale initial GET cannot overwrite a
  later successful refresh, checkbox draft or matching CAS version. The
  browser regression holds one configured response until a newer refresh has
  applied and releases it afterward.
- **Unsaved CPT creation draft:** Creating a new CPT previously bypassed the
  App dirty guard; 401 or navigation could discard incomplete keys/fields and
  select options. `ContentTypeForm` publishes the complete draft immediately
  on input, while `ContentTypeSchemaSettings` scopes its transient RAM snapshot
  to the verified site and Admin account. Same-account re-login restores;
  another account cannot read it; explicit discard/success clear it. Draft
  changes reach the existing App dirty-navigation guard.
- **Existing CPT settings 401/412 recovery:** The label/public/archive/
  publication/supports editor now stores unsaved changes in site/account/CPT-
  scoped RAM, including a baseline CAS version. Same-account 401 re-login
  restores the draft; different account cannot read it. Stale 412 blocks Save
  until explicit rebase, which reapplies only locally changed fields onto the
  server's latest settings and checks conflicting fields before overriding.
  Successful save and explicit discard clear the snapshot.
- **Create POST interleaved with newer input:** Successful CPT creation clears
  the App dirty guard synchronously *only when no other settings/taxonomy edit
  appeared while the POST was pending*. If other edits occur, the new CPT is
  recorded as created but the current form is not navigated away from and the
  newer draft remains. E2E intentionally holds the create response, edits the
  existing CPT and releases the response; no post-commit false cancellation or
  silent loss is permitted. The provider-owned CPT variant surfaces successful
  creation at top level even though no schema settings editor is available;
  concurrent unsaved taxonomy overrides remain intact and visible.
- **Unbounded Admin term enumeration:** The intermediate all-column fix could
  scan 64 entire catalogs on every table load. The new
  `GET /api/admin/taxonomy/terms/resolve?ids=...` endpoint validates 1–100
  distinct positive IDs and requires authentication; storage returns only
  requested labels using bound `json_each`, no-store. EntryList only queries
  current row IDs in bounded batches; full taxonomy term catalogs load only
  when the corresponding filter receives focus. Two focused Rust tests cover
  parser limits and bounded projections; the older M7 table acceptance runner
  was updated to focus filters before selecting options. This exposed an
  existing tag-filter behavior that had reset Category descendants; the
  Category+Tag AND selection now retains the active descendant option.

### Earlier final-candidate native acceptance (superseded by latest corrective round)

Host: **macOS 15.8.2 Intel x86_64**, Chrome **158.0.8127.36**.
The expanded C2 runner recorded **29/29 named checks PASS** across API,
SQLite, the native debug binary, Chrome and restart using only disposable
roots; `port_18964_used=false`. Among the added checks: create CPT 401
same-account restore/cross-account quarantine, existing CPT settings 401 and
412 rebase retaining concurrent unrelated changes, post-201 create navigation,
delayed POST 201 with a second unsaved edit, controlled out-of-order configured
GETs and all custom table column labels via a bounded ID projection.
Receipt: `.g5-checkouts/evidence/c2-t02-final-candidates/c2-native/receipt.json`;
debug binary SHA-256
`17076a0d6b2b3674fd167c18c2e05b247a1fb9943e1a74e67085d5b944327aa7`,
runner SHA-256
`70f4f7c7303c322881de1b6fce9943b07dcf3fc3477b8fd073f6b1ec696d1517`.

The independent M7 entry-table runner also recorded **21/21 named checks
PASS** against the identical native debug binary, including real category +
descendants + tag + month AND filtering, 125-entry server pagination, bulk
CAS, network failure, history, viewport 390px and restart.
Receipt: `.g5-checkouts/evidence/c2-t02-final-candidates/m7-table/receipt.json`.
The older M7 test was changed only to focus lazily populated dropdowns before
choosing a term; its semantic server-side expected results are unchanged.

The runtime receipts explicitly record the current **dirty premerge source**
paths (base head `b6aac15`), not a cleaned exact pushed head. They are not
evidence of G4 merge or G5 release. Exact-head acceptance, CI and Aster
re-review remain distinct gates.

All these fixes remain subject to new-head exact-source acceptance, regression
tests and Aster re-review; a passing earlier source receipt does **not** prove
the later changes or authorize a G4 merge.

### Latest corrective-round premerge validation — 2026-11-19

All validation below was observed against the **12-file dirty corrective
worktree**, based on `b6aac15ab847e239ae117b91970ee712ae7d3a3d`.
The reports' `source_sha` records that last commit and their changed-path
inventory records the newer uncommitted changes. These checks are **not**
post-push exact-source CI or exact merged-source G5 receipts.

- `npm --prefix cms/admin run typecheck`: **PASS**; `npm run build --prefix
  cms/admin`: **PASS**, 97 modules, existing chunk-size warning.
- `cargo build --manifest-path cms/Cargo.toml --locked -p opendash-server`:
  **PASS**, embedding the just-built Admin assets. The debug binary SHA-256 is
  `1c5640914a76c21c054f982f26b6b4272fdec1c0cb98e7c5c83852b559b99cec`.
- Full server Rust test suite with canonical, non-symlinked workspace
  `TMPDIR`: **95/95 PASS**, log
  `.g5-checkouts/evidence/c2-t02-final-round/rust-full.log`.
  Core: **39/39 PASS**; focused C2 integration: **2/2 PASS**.
  The additional provider redeclaration/withdrawn eligibility-only CAS
  targeted regression: **1/1 PASS**, log
  `.g5-checkouts/evidence/c2-t02-final-candidates/rust-provider-final.log`.
- Native isolated C2 Chrome/API/SQLite/restart acceptance: **30/30 PASS**;
  receipt `.g5-checkouts/evidence/c2-t02-final-round/c2-native/receipt.json`.
  It includes the late create-201 notice for a provider-owned CPT with an
  unsaved taxonomy edit, plus site/account 401 recovery, 412 rebase, provider
  eligibility versioning, table term-name projection, Builder/public safety
  and SQLite integrity. Runner SHA-256:
  `17dc2b6f894f369a97ff38356e55c45c6d1a1d19eff1e7007ef90e7465930a67`.
  Receipt explicitly confirms `port_18964_used=false`.
- Native independent M7 entry table Chrome/API/SQLite acceptance:
  **21/21 PASS** using the same binary. Receipt:
  `.g5-checkouts/evidence/c2-t02-final-round/m7-table/receipt.json`.
  Runner SHA-256:
  `94b58b8169fbfd4f8bf18b0bb550598170b33b8b19f9abd58f6d74cf304849c4`.
- `python3 cms/scripts/check-plans.py`, `--self-test` (12 fixtures),
  Python runner compilation and `git diff --check`: **PASS**.
- Independent Admin reviewer returned **PASS**, no actionable P1/P2 in the
  final RAM-draft/create-notice scope; independent backend reviewer returned
  **PASS**, no actionable P1/P2 in withdrawn provider eligibility-only CAS,
  theme/plugin activation, deduplicated version bumps, SQL bindings or writer
  transaction boundaries. Independent reviews did not replace coordinator
  Aster's final-head acceptance.

The first new C2 attempt failed only because a Playwright text locator matched
two intentionally visible identical create notices; it was corrected to target
the top-level `role=status`. The subsequent 30/30 run passed, including the
provider-owned branch. A subsequent clean-head repetition at `315464b` exposed
an additional test synchronization issue: the test checked 412 CPT settings
rebase immediately after clicking an asynchronous fetch action, sometimes
before React had applied the result. The runner now waits for the explicit
successful rebase `role=alert` before asserting local label/public and remote
archive fields. With that wait, the repeat **30/30 PASS** on the same source
and embedded Admin binary; receipt
`.g5-checkouts/evidence/c2-t02-final-round/c2-exacthead-rebase/receipt.json`,
binary SHA-256 `23505c10b0b66cbb08c4a9067fe14c5fc901e912f1728267592843fec134beaf`,
runner SHA-256
`fe92ac98cba44964957a4741ee37f156400391a00ffe6a66c347672237c4c76e`.
This repeat was made with the corrected runner dirty but no product-source
changes; a clean source/runner repeat is required after its own commit. None
of these premerge receipts is a G5 release artifact.

## G4/G5 release gates

- GitHub [PR #390](https://github.com/thientran2359/opendash-cms/pull/390)
  is Draft. Aster ownership ACK is **recorded** but final corrective-head
  acceptance, new-head self-review, required governance CI, and actual G4
  merge are **NOT YET RECORDED** here.
- A premerge debug binary and dirty worktree acceptance are **NOT exact-source
  G5**. After merge, build release from the **actual merge SHA** with its own
  embedded Admin assets and run isolated native macOS Intel E2E and required
  upgrade/restart/snapshot restoration evidence. Record hashes, artifact path,
  commands and results in separate G5 evidence before updating TASK_INDEX or
  marking Issue/Project Done.
- Windows/Linux native acceptance, human dogfood, common RC/V1 and parent
  [#302](https://github.com/thientran2359/opendash-cms/issues/302) are
  **NOT RUN/NOT CLOSED** by this task.
