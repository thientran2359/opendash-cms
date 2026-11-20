# M4-S12-T01 / #374 — Content Foundation C2, implementation verification

**Agent:** Harbor (self-review); **coordinator:** Aster. **Date:** 2026-11-18.
**Governance:** Issue [#374](https://github.com/thientran2359/opendash-cms/issues/374); immutable
G3 [2026-11-18-content-foundation-g3.md](../plans/milestones/M4/slices/2026-11-18-content-foundation-g3.md),
merged with PR #372; ADR-0019 and
[Content Foundation contract C2](../contracts/CONTENT_FOUNDATION_2026_11.md).
**Prerequisite:** #373 task-local native macOS Intel G5 Verified; #373 closed and Project #3 Done.
**Implementation base:** `6c26e9812042b852517acbed5ddbe0df1b9105e0` (G5 evidence PR #381 merge).
**Review head / PR / merged release SHA:** to be recorded on GitHub/after merge.

## Delivered C2 backend

- Forward-only `0039_taxonomy_definitions.sql` creates the canonical taxonomy
  registry, seeding Core `category` (hierarchical) and `tag` (flat). It rebuilds
  term, membership, extension, and eligibility foreign keys without changing
  existing term IDs, parent relationships, entry memberships, owner/status,
  versions, extension payloads, or legacy category/tag semantics.
- Canonical domain `TaxonomyDefinition/Input/Patch` validates core-reserved
  names, labels, defaults and schema; owner/status reuse Core registry enums.
  Admin authenticated definition CRUD has CAS `If-Match` 428/412, owner/in-use
  conflict 409, writer serialization and per-site 64-definition cap. Deleting
  Core/provider definitions is forbidden.
- Generalized same-kind parent validation, cycles and full-subtree height-8
  bounds; flat taxonomies reject parents, including when switching flags.
  Slugs are unique within each taxonomy, not across taxonomies.
- CPT eligibility site override uses existing CPT version CAS, with provider
  default fallback, empty/omitted distinction and Section exclusion. Page has
  no Core taxonomy by default. Removing association never deletes entry links;
  explicit `term_ids` must target eligible current definitions, while omitting
  `term_ids` retains previous values. Existing generic entry create/save and
  composite paths use a single writer snapshot.
- Query `taxonomy_key` + `term_id` is validated as a pair, disallowing mixed
  `category_id`/`tag_id` aliases. Hierarchical descendants are bounded to 8.
  Admin entry tables, shared public archives and category/tag archives apply
  eligibility/public gates *before* pagination, keeping drafts and ineligible
  private associations out of public result pages. Existing alias semantics
  continue to work.
- Existing saved Builder list filters degraded to `MissingConfig` when the
  admin removes an association, rather than causing a 400 for the entire
  homepage; sitemap only emits public, associated category/tag term URLs and
  never manufactures a tag URL for a custom taxonomy kind.

### Reviewed integration delta (file ownership)

G3's original exclusive-file list for #374 did **not** explicitly name
`cms/crates/opendash-server/src/storage/sitemap.rs`. Independent review found
that this existing public query would expose the slug of a new private custom
taxonomy using a bogus `/taxonomy/tag/<slug>` path and fail to honor withdrawn
CPT eligibility. The small sitemap SQL public/eligibility guard is a required
security integration for C2 (not a new rewrite feature); it is disclosed here
  as a **file-ownership Design Delta**. **Aster explicitly ACKNOWLEDGED/APPROVED**
  this bounded `storage/sitemap.rs` extension in the coordinator review on
  [PR #383](https://github.com/thientran2359/opendash-cms/pull/383), at head
  `491a5b8e8f370e9066ccae98dcd89422bb94d9e3`. Aster also confirmed the
  paired parser integration in `routes/entries.rs` and default wiring in
  `storage/builder.rs`; no competing writer was assigned. This clears the
  ownership hold **only for this scoped C2 integration**, not a general file
  handoff. No changes to ADR-0019, the C2 contract, or immutable G3 planning.

### Coordinator P2 correctness review and fix

Aster's same independent review identified a blocking P2: category/tag
archive-root kinds were not included in CPT effective eligibility unless
they happened to be supplied as extra query filters. The root kind must be
checked independently of optional `taxonomy_key` + `term_id`; otherwise an
entry with preserved old memberships could leak into the archive after an
override/provider association was withdrawn. Both SQL `COUNT` and paged row
bindings in `storage/public_content.rs::list_taxonomy_published_in_tx` now
bind `kind.as_str()` as the first required taxonomy, while leaving the generic
filter CTE seeded from `filters.taxonomy_key`. The new storage regression
checks count, offsets, category and tag roots, plain and valid generic extra
filters, draft exclusion, withdrawn override/provider/defaults and unchanged
membership links; the native E2E runner now exercises both archive URLs
before/after eligibility changes. This P2 must pass on the updated head
before PR ready/merge; Aster's old-head approval is not a test verdict on
the new head.

## Pre-merge validation — actual observed results

Host: **macOS 15.8.2 Intel x86_64**, Chrome **158.0.8127.36**; native Mach-O OpenDash debug binary:
`5488a5bf756b1aa318cf964021f6350e355c9d5b4171b711beff08fd68cb1564`
(SHA-256). This is a **dirty implementation-worktree pre-merge build**, not
postmerge native G5. Its receipts record `source_worktree_clean=false` and the
actual changed path list, never pretending a release source SHA.

| Test | Observed |
| --- | --- |
| `cargo check --manifest-path cms/Cargo.toml --locked -p opendash-server` | PASS |
| `cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --test content_foundation_m4_s12_t01` | **3/3 PASS** |
| `TMPDIR="$PWD/.g5-checkouts/evidence/c2-tmp" cargo test --manifest-path cms/Cargo.toml --locked -p opendash-server --bin opendash -- --test-threads=4` | **90/90 PASS** |
| `cargo build --manifest-path cms/Cargo.toml --locked -p opendash-server` | PASS |
| `python3 cms/scripts/check-plans.py` | PASS |
| `python3 cms/scripts/check-plans.py --self-test` | **12 fixtures PASS** |
| `python3 -m py_compile` (both C2 runners) | PASS |
| `git diff --check` | PASS |

Native pre-merge Chrome + API + SQLite acceptance command (exit 0):

```bash
.g5-checkouts/evidence/venv/bin/python cms/scripts/content_foundation_m4_s12_t01_e2e.py \
  --binary cms/target/debug/opendash \
  --artifact-dir .g5-checkouts/evidence/c2-acceptance
```

**16/16 named checks PASS**: built-ins, guest/editor/CSRF ACL, genre/brand
registration and slug collisions, parent/cycle/depth 8 constraints, missing/
stale/concurrent CAS (one 200, one 412), CPT eligibility/version, Section
private, omitted/explicit memberships and deletion in-use 409, valid/malformed
generic public/table filtering (canonical numeric IDs, cross category/tag
archives), taxonomy private flag, sitemap visibility, Builder list safe
degradation when association is removed, Chrome Admin desktop/390px no
overflow, SQLite FK/integrity and persistence across native restart.

Native pre-merge old/new/old rollback command (exit 0):

```bash
.g5-checkouts/evidence/venv/bin/python cms/scripts/content_foundation_m4_s12_t01_upgrade_g5.py \
  --old-binary .g5-checkouts/evidence/c1-release/opendash-merged-767940b \
  --new-binary cms/target/debug/opendash \
  --artifact-dir .g5-checkouts/evidence/c2-upgrade
```

**5/5 named checks PASS**: old release 0038 with category tree, tag,
membership, term extensions and upload snapshot; new 0039 retains all term
IDs/version/membership/extension/ownership and validates FK/integrity;
custom taxonomy same-slug separation and preservation of historical
AUTOINCREMENT high-water after deleting term ID 1000; **pre-0039 SQLite+uploads snapshot
restored before booting old binary**; new binary can upgrade that snapshot
again and edit terms. There is **no in-place downgrade** of schema 0039.

Old binary: C1 exact merge `767940b`, SHA-256
`2d23c7ee74e95fba4b9e577950f0f06deba6d0b06e4db3fdfc47dc941ea0de9e`.
Receipts retained locally (ignored; not committed):
`.g5-checkouts/evidence/c2-acceptance/receipt.json` and
`.g5-checkouts/evidence/c2-upgrade/upgrade_receipt.json`.
Native screenshots (SHA-256):
`c2-desktop.png` `05a1ed39b563b59e529196670129b42d2a0aa1df9b5eaa17e3d9f6314660134f`,
`c2-mobile-390.png` `c06affc86181404459636bb7209505d054670cdb7c454fdbe1b7c127fa23659b`.
Migration 0039 SHA-256:
`47a21581228ac7feb98a5c5a311097c95850e3014e1274f66272484c6a779b07`.

Both runner sites used disposable isolated SQLite/uploads and random
`127.0.0.1` ports. **Port 18964 and owner review data were not accessed.**

### Aster P2 follow-up — new-head premerge validation

After receiving coordinator review on PR #383, Harbor changed both archive
root eligibility query bindings and added the Rust storage + real-browser
regressions described above. The following **actual** macOS 15.8.2 x86_64
checks were rerun against this follow-up working tree (not yet an exact
merged-source G5 release):

| Check | Result |
| --- | --- |
| Rust `opendash -- c2_archive_root_eligibility_gates_count_rows_and_cross_filters` | **1/1 PASS** |
| Rust server binary unit suite `--test-threads=4` | **91/91 PASS** |
| C2 focused domain tests | **3/3 PASS** |
| `cargo build --locked -p opendash-server` | PASS |
| Native Chrome 158 + API/SQLite isolated C2 E2E | **17/17 PASS** |
| Native old 0038 → new 0039 → restored 0038 → re-upgrade | **5/5 PASS** |
| Plans checker and 12-fixture self-test, Python compile, diff whitespace check | PASS |

The newly added 17th E2E case tests category/tag archive routes both with
no filters and with a valid extra `brand` filter, first eligible, then each
root independently removed by CPT override; it asserts old membership IDs
are not destroyed. Rust asserts matching `total` and paged row counts for
both kinds and both filter modes, plus provider-inactive and withdrawn
default declarations. This closes the reviewer-reproduced P2 on the tested
working tree; the new pushed head, governance CI and independent follow-up
review remain recorded separately on PR #383.

Debug binary SHA-256: `49b4860726bffc4bf599dfc5005f55d5f1feffa5c7b78a93d5d9a8c30d45af18`.
Ignored receipts: `.g5-checkouts/evidence/c2-p2-acceptance/receipt.json`
SHA-256 `bc99d84b0063a68c93f4e4512ec1cc8ba71cad467a5903051b1cbde836c3a528`;
`.g5-checkouts/evidence/c2-p2-upgrade/upgrade_receipt.json`
SHA-256 `729d13d3fb4f6bfa4a0aed0e46c4cde2d7fc8691f13c6dfe206a3353a1ce268e`.
The E2E receipt intentionally records `source_worktree_clean=false` for
this precommit run; these artifacts are not postmerge G5 evidence.
`cargo fmt --all -- --check` was also attempted, but reported existing
workspace-wide formatting drift in many unrelated Rust files, so no global
reformat was made; `git diff --check` is PASS.

## G4/G5 distinction, scope and known limits

- Required PR governance CI and self-review apply to the **exact G4 head**;
  CI/PR/merge values are recorded at actual PR completion, not predicted here.
- Native **G5 is NOT YET VERIFIED**: after G4 merge, build the release binary
  from the **exact merge SHA** and repeat Chrome/API/SQLite plus old/new/old on
  the exact release, pin hashes in separate G5 report and merge evidence before
  Issue/Project closure or #375 pickup.
- Admin taxonomy/Builder selector UX is expressly **#375 (T02)**, not this
  backend slice; custom rewrite/permalink URL authority belongs to later C3.
  Theme/plugin provider *declaration* work belongs to a later designated task.
- Existing multiple-read public snapshots can complete in-flight responses
  against a visibility decision made before a concurrent admin toggle;
  response caching remains `no-store`. Strict cross-route linearizability is
  not claimed by this slice.
- Windows/Linux **native product** G5, human dogfood, common RC/V1 gates are
  **NOT RUN**. **Issue #302 remains OPEN** and this task does not close Beta.
