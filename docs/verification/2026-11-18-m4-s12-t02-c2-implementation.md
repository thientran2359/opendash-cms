# M4-S12-T02 / #375 — Custom taxonomy Admin and Builder implementation

**Agent:** Harbor (implementation/self-review); **coordinator:** Aster. **Date:** 2026-11-18.
**Governance:** [Issue #375](https://github.com/thientran2359/opendash-cms/issues/375),
[immutable G3](../plans/milestones/M4/slices/2026-11-18-content-foundation-g3.md)
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
| `cargo test --locked -p opendash-server --bin opendash storage::entries::tests::builder_` | **3/3 PASS** |
| `TMPDIR=<isolated-artifact-tmp> cargo test --locked -p opendash-server --bin opendash -- --test-threads=4` | **92/92 PASS**, final worktree |
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
3. **Entry table labels:** All effective custom taxonomy columns fetch their
   corresponding term labels via a bounded concurrency-4 queue, not just the
   selected filter. Loading/failure states are differentiated from genuinely
   empty associations; legacy Category/Tag and strict pair filtering persist.
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
are retained as diagnostics, **not successful product tests**. The final 23/23
is still on a dirty premerge worktree; it is **NOT G5**.

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
