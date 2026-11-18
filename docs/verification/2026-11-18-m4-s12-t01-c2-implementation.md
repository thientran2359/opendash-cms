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
as a **file-ownership Design Delta to be acknowledged before merge**. No
changes were made to ADR-0019, the C2 contract, or immutable G3 planning.

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
