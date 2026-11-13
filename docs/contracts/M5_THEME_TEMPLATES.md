# M5 Theme & Templates — selected contracts and blocked sub-contracts

**Owner:** Atlas · **Date:** 2026-11-13 · **Design status:** PROPOSED / BLOCKED pending Nova review and merge.
**Implementation/verification:** NOT_STARTED / NOT_RUN.
[Decision authority, source SHAs and G3 corrections](../decisions/ADR-0012-m5-theme-templates.md).
This file records Atlas selections for review; it does not publish G3 or authorize Project Ready.

## 1. Package identity, revisions and selection (A)

- New v2 package `theme_id` is a stable lowercase kebab identifier (1–64 characters), unrelated to display name/version. Ownership uses `ResourceOwner::Theme(theme_id)`. Full block IDs stay namespaced; never infer package ownership from their prefix.
- Adapt every distinct legacy full block ID to its own internal legacy identity in a reserved identity domain, recorded by an explicit mapping table; filesystem paths use validated digest IDs, not block IDs. A v2 manifest cannot claim that legacy identity. No grouping or takeover of legacy IDs based on namespace.
- Revisions are immutable. A revision records package ID, declared semantic version, canonical file-map digest, source format and provenance; do not guess a v1 original ZIP checksum from stored DB rows. A tagged deterministic legacy snapshot retains exact descriptor/template/compatibility bytes. Same package/version + different digest is 409; identical canonical digest is idempotent. No silent overwrite of old revisions.
- Canonical source is the validated immutable revision file set under the site's `themes/` root, plus DB manifest/selection metadata and digests. Retained original ZIP is provenance, not a second writable template source. Extracted/cache copies are either the canonical set or explicitly rebuildable derivatives. M8 snapshot must capture DB + canonical revisions + uploads + overrides/options/menu; do not back up only a ZIP filename or cache.
- Keep one canonical singleton active selection `revision_id?` + monotonic CAS version. All public, Admin, Builder and v1 adapter readers derive from it. Migration maps old explicit block selection or preserves NULL legacy behavior. The old block column may be retained for historical restore but is no longer a writable/runtime source. Never dual-write two authoritative pointers.
- Install/update validates and stages a revision; **does not activate** it. Activation/rollback uses authenticated Admin + CSRF/Origin, If-Match, one transaction including schema/declaration compatibility and pointer update. Missing precondition 428, malformed 400, stale 412, identity/schema conflict 409, quota 413. Failure leaves the old selection and data intact.
- New v2 design ceilings: compressed ZIP 8 MiB, aggregate decoded entries 32 MiB, 256 files, manifest/template text 256 KiB each, raster asset 8 MiB each, expansion ratio 100:1. Check actual read bytes as well as headers, including zero compressed size edge. These are policy ceilings, not measured capacity. Keep v1's 64 KiB limit unchanged.
- V2 paths: relative ASCII segments, no empty/dot/dot-dot segment, backslash, drive/colon, control, symlink, absolute path, path alias or case-insensitive collision. Explicit file role/extension/MIME allowlist; no arbitrary executable uploads. Extend shared `package` validation with v2 path policy while preserving existing flat plugin/v1 semantics.
- Package assets are revision-scoped and immutable, served by a dedicated route under `/theme/` with nosniff and digest/path validation. They never expose arbitrary site files or grant private uploaded media public access. Exact CSS/HTML/SVG policy is blocked by C-SEC; no claim that extension checks are a sanitizer.

**Acceptance:** legacy NULL/explicit activation migration; multiple full block IDs sharing a prefix; corrupt/missing revision fallback; version/digest collision; concurrent activate/Builder Save with one consistent snapshot; safe ZIP negative corpus; restart; restore pre-M5 snapshot with pinned M4 native binary.

### A-DURABILITY — BLOCKED; independent Nova review required

Canonical revision files must be fully validated and durably published before any committed install metadata or active selection can reference them. S01 must define run-owned staging, immutable publication, idempotent concurrent install, journal/recovery and safe cleanup. Missing/corrupt committed files must block their activation and trigger public safe fallback, never silently serve unverified bytes. This is a package lifecycle contract resolved before S01 release; it must not depend on implementing S05 first. Exact per-OS semantics and any uncertain feasibility require bounded G2 investigation; full installer crash-cutpoint tests are G4/G5.

## 2. Public URL and RequestContext (B)

- Keep `/`, `/content/{type_key}`, `/content/{type_key}/{slug}`; Page uses the same content namespace in M5. Add only `/taxonomy/category/{slug}` and `/taxonomy/tag/{slug}`. No theme-declared route or root-level entry permalink.
- Core owns typed Home/Single/Archive/TaxonomyArchive/NotFound and ID-based canonical URL helpers; templates receive filtered view models and cannot query DB or dispatch routes.
- Old slug returns **404**, no redirect registry or hidden alias. Stable entry/term IDs in menus resolve to the current slug. No cross-entity fallback on deleted/unpublished IDs.
- Reserve existing Admin/API/auth/media/assets/builder/theme/page/registry/preview/save/content/taxonomy namespaces. Reject encoded separators, ambiguous/double decoding, traversal and malformed route identity. Do not decode path twice. Invalid identity/not-found/draft returns 404; malformed recognized query returns 400.
- Preserve content archive page size 20, max page 10,000. Keep M4 list's independent 1–100 paging and page size 1–20 contract. Do not reduce existing archive limits to list limits.
- Taxonomy archive spans eligible published content types, deterministic entry-ID ordering, page size 20/max 10,000. Category matches exact term by default; descendants require explicit true. Category/tag predicates remain AND before pagination, no duplicate rows. An unknown/deleted term or wrong kind route is 404.
- Recognized query keys are typed, bounded and single-valued; duplicates/invalid recognized keys are 400. Ignore unknown tracking keys and exclude them from canonical output. Context never contains session/token/private path or entry data outside publication/role policy.
- Public queries enforce publication in storage before counting/paging. Preview is authenticated, CSRF/Origin protected, no-store; authorized draft entry preview is a separate typed context, never a public route bypass.

**Acceptance:** old M3 URLs; slug-change/new canonical; reserved/encoded paths; duplicate query/invalid bounds; private/missing targets; filters across pages; no draft displacement/count leak.

## 3. Templates and overrides (C/H)

Select roles `home|page|header|footer|archive|single|taxonomy_archive`. Context selects page for built-in Page, single for other content, and the corresponding archive role. Selector identity uses role + optional validated type/entry scope; a display variant is not a new route.

Resolution: most specific saved site override → less specific saved override for the role → active revision's applicable default → core safe fallback. Header/body/footer compose once in core; no raw HTML interpolation convention for nesting. Existing saved Builder homepage is the authoritative home body and must not be overwritten by theme install/update or a parallel home override table.

Overrides are site-owned typed AST, with version, base revision/provenance and stable node IDs. They are not arbitrary MiniJinja source. First Save creates override; Preview never writes. Reset to current default is explicit, confirmed and CAS checked. Save follows M4 snapshot/412/draft preservation; post/page draft Save does not publish, saved Builder role design updates public rendering without publishing a draft entry. Keep unresolvable nodes for Admin editing and preserve-only round-trip.

M4 binding resolves one exact `type_key,entry_slug`; it cannot implement a single-template field bound to **the current route entry**. S03 must specify an explicit typed route-context binding source and allowlist, keeping P05 behavior backward compatible, or keep that sub-contract BLOCKED. Never use string replacement or implicit global lookup to simulate current-entry binding.

### C-SEC — BLOCKED; Nova independent review required

Selected security requirements:

- Keep MiniJinja **3.0.5** and locked dependencies. Start from an explicitly restricted environment, always HTML autoescape, Strict undefined, fuel at most 10,000 per bounded render; explicit recursion/output limits. No filesystem loader, network, SQL, process, secret/session globals or callbacks that expose host methods.
- No `safe`/aliases, autoescape disabling, arbitrary method/global calls, dynamic include/import, unbounded recursion or content compiled into template source. Includes, if approved, must resolve only validated templates within the selected revision and share the total budget. Fuel alone is not an output-byte cap.
- Typed text is escaped; rich_text remains escaped plain text per existing contract. URL and CSS destinations require contextual validation. No raw string options inserted into style declarations.
- HTML/attributes reject scripts, event handlers, base/meta redirects, embedded frames/objects, form actions, unsafe schemes, private media URL fabrication and executable SVG/foreign markup. CSS cannot issue arbitrary remote requests or consume untyped user strings.
- Public and Preview use the same safe renderer; Preview iframe remains opaque `sandbox=""`. Preserve restrictive CSP including script/object/base/form prohibitions; any asset/style CSP expansion must be explicit and tested.
- Invalid revision/role renders safe fallback with public-safe diagnostics; never silently execute an inactive theme's template.

**Unresolved evidence:** concrete grammar/parser and contextual HTML/CSS validator, pinned implementation/dependency choice, compatibility treatment of legacy v1 templates, current-entry binding DTO, numeric recursion/output composition budgets and adversarial corpus. MiniJinja compile success, removal of filters, CSP or a regex scan does not resolve this blocker. S01-T02/S03 cannot activate unsafe templates pending that resolution; S07 is regression, not deferred initial protection.

## 4. Theme Options and SITE-01 menus (D/F)

- Extend SITE-01 using current General Settings conventions and auth/CAS APIs. There is **no generic options engine today**. Theme keys are scoped by stable theme ID; site General Settings retain their core owner/meaning.
- Descriptor types initially string, finite number, boolean, select and image_ref with typed bounds/defaults. Arbitrary regex/code/secret types and untyped CSS strings are excluded. Exact DTO limits must be in the corrected S04 brief.
- Store user value separately from revision default. Absent value resolves current valid default; compatible update preserves explicit value, removed option becomes dormant retained data, incompatible type/default change rejects activation with 409 unless a separately approved explicit migration exists. Never reinterpret/drop a value or reset it by reinstall.
- Admin-only mutation. Templates receive only explicit public exposure keys. image_ref must resolve under transactional saved-reference ACL (including removal/replacement/theme switch), never because a dry-run/import file or unsaved default exists. No synthetic public media URL bypass. S04 must show changed public header/footer output, not just a saved form.
- Site-owned menu and item IDs; menu version covers ordering/target mutations. Parent must belong to the same menu; maximum 200 items/menu, depth 8, deterministic sibling order with unique position; cycles/duplicate origin or sibling positions reject atomically.
- Targets are typed entry ID, term ID or validated external HTTP(S) URL with no credentials/controls/protocol-relative form. Canonical URLs resolve at render time. Removed/private/ineligible target yields Admin diagnostic and public omission; do not delete the item or reuse an unrelated slug.
- Menu slot definition belongs to a revision; slot assignment is a retained site choice scoped by theme ID/slot key. Switching a theme does not delete menu/assignment. Public header/footer render the selected menu through typed view models. M5 includes the minimum display path for import; M7 owns full CRUD/drag-and-drop menu UI.
- Import uses transaction-aware SITE-01 APIs. No shadow tables in theme modules. Editor cannot mutate Theme Options/install/import merely because Editor may edit entries.

**Acceptance:** saved value/default distinction; absent/withdrawn option; invalid type/selection/media; 401/403/CSRF/Origin/412; public output/ACL revocation; menu cycles/depth/order; slug changes, missing target, switch/update/restart.

## 5. CNT lifecycle and taxonomy (E)

- Persist canonical definitions with immutable `ResourceOwner::Theme(theme_id)`, separate from runtime template availability. Other-owner collision stays 409 even if that owner is inactive/withdrawn. Core post/page are never claimable.
- State axes: declaration declared/withdrawn; provider active/inactive/missing/invalid. Switching theme changes availability, not declaration ownership. Updating a revision to omit a declaration marks it withdrawn, retaining schema/values. Explicit same-owner re-declaration requires compatibility checks; bulk `activate_owner` is not that operation.
- Runtime block/template execution requires active eligible provider. Admin/Editor entry and field-value editing uses retained valid schema independent of provider availability. Content/public fallback follows published policy without resurrecting inactive theme code. Do not globally hide all published content merely because the original theme is inactive.
- Same-owner label/help/default-only changes may be compatible. Optional new fields preserve old entries. Removal/type change/key rename/required tightening/select-option removal is rejected if it invalidates retained data; no automatic dropping. Content-type withdrawal retains valid fields and entry editability. Destructive changes require another explicit migration/rollback contract.
- S10 builds canonical transaction-aware CNT registration/evolution APIs in server storage, using core validators and owned registry; they are future work, not current APIs. In-memory registry is refreshed only from committed canonical DB state, not published before commit.
- Taxonomy eligibility declarations only refer to core category/tag and a compatible owned type. Terms and entry links stay site-owned; core term kinds and existing user types cannot be taken over. Withdrawal does not remove existing term membership or prevent editing retained content.
- Never change released migration 0004 or use a parallel registry in a theme route.

**Acceptance:** core/other-owner takeover; same-owner idempotency; incompatible schema/old data; omission vs switch vs missing file; re-declare after withdrawal; partial registration failure; restart; actual Admin/Editor editability under inactive provider; no ownership leak.

## 6. Selective import and provenance (G)

Select bounded all-or-nothing visibility for one chosen import run, not one giant transaction for the entire installed package. A dry-run is DB/files read-only. The plan includes selected groups, dependency closure, conflict/skip reasons, expected versions/digests, publication implications and exact logical targets. Unselected dependencies are reported for explicit selection, not silently imported.

Logical provenance identity is **site + theme_id + sample_key + resource_kind + object_key**, **excluding revision_id**; record revision/digest as execution provenance. This avoids reimporting a duplicate on A1→A2. Deleted targets leave tombstones; changed targets are skipped with diagnostics, never recreated or overwritten by retry. A user may explicitly request a distinct new import identity in a separate approved operation, not through a hidden revision change.

A stale active revision, target/version/schema/menu/options change invalidates the plan (412, rerun dry-run). Precondition checking and successful DB mutations share a serialized transaction. Repeated execute with the same accepted identity returns the existing outcome.

Post/Page sample entries are created draft; no automatic Publish. Existing CPT Save semantics are preserved; any sample CPT entry that would become public must have an explicit publication acknowledgement in the reviewed plan/confirmation contract or remain unsupported. Imported media stays private unless a separate authorized public reference is committed; imported menu/setting/home design changes with immediate public effect require explicit plan acknowledgement, never silent application.

### G-RECOVERY — BLOCKED; Nova independent review required

Selected invariant: no committed visible entry/menu/option/provenance may reference unfinished media. Prepare bounded files first under a run-owned staging namespace, verify bytes/digests, keep a durable journal, then atomically commit DB/domain objects and completed outcome. A process crash before commit may leave quarantined files; it must not expose partial selected objects or delete user/shared media.

**Missing contract/evidence:** exact journal states/transitions and durable writes, original+thumbnail publication ordering, concurrent upload/reference locks, safe shared-file reuse, crash rollback/retry and cleanup eligibility, startup/read gate before serving damaged committed references, and Windows/macOS flush/rename guarantees. Existing `write_once` is not an import journal. DB pool helpers that start independent transactions cannot be composed into all-or-nothing import.

G2 must settle that state machine, ownership and visibility contract, with pinned API/OS documentation and bounded prototype evidence where feasibility is uncertain. It does not require the full import implementation to exist before G3 publication. Required G4/G5 implementation proof includes cutpoints before/after each journal write, original/thumbnail finalization and DB commit, retry across revision changes, concurrent shared references, disk full/permission failure/corruption and recovery restart. Compare stable DB IDs/fingerprints and media hashes; no mutation of pre-existing user data. A successful process-kill experiment is not proof against all power loss.

## 7. Ownership, sequencing and release fixture (H)

- Coordinator Atlas; one implementer per task branch, one writer per shared file. Proposed task ownership remains Forge for S01/S03/S10/S06 and Harbor for S02/S04/S09/S05/S07; this is design allocation, not task claim/Ready.
- Seam order: S01-T01 → S01-T02 → S10 for theme/activation; S02 URL consumer changes precede S03/S09; S03 precedes S06 then S07. S04/S09/S10 provide transaction-aware seams before S05 executes. No fake existing API input or dependency cycle from activation to undeveloped declarations.
- Shared `app.rs`, `routes/mod.rs`, `storage/mod.rs`, `theme.rs`, `page.rs`, `content_render.rs`, `registry.rs`, Admin shell and migration numbering are handed off explicitly. Future briefs must enumerate exact new modules/tests and owner, not “adapter module” alone.
- Atlas allocates each next forward migration against fetched main; last verified M4 is 0014, no preallocation of conflicting numbers and no edits to 0001–0014. Rollback uses complete pre-upgrade site snapshot and matching executable, never deleting SQLx migration rows.
- S08 accountable implementer Forge, coordinator Atlas, independent native Mac verifier Harbor; distinct per-OS runners/evidence files. Final common source SHA is selected after all M5 product fixes; record separate native binary hashes and exact runner source.
- Deterministic fixture: M4 v1 NULL/explicit active + saved/unsaved legacy projection; A v2 declares CPT/fields/category/tag, creates Post/Page draft/published, menu/options/media and role overrides; selected import/retry; Publish/Save/Preview; A1→B missing provider→restart→A2; update/rollback; stable IDs and editable content preserved. Include the complete negative matrix above.
- Inactive-theme entry editing does not grant Editor Admin-only rights. Browser must verify page/current-entry binding, header/footer/menu/theme options, public fallback, no draft/media leak and no console error.
- Windows/macOS full G5 must use one immutable source SHA with genuine pre-M5 native binaries and DB+media+revision snapshots. M4 Windows optional pre-0009 rollback SKIP remains disclosed in historical evidence; required M3 v0011 rollback passed. Do not inherit that optional skip as a missing M5 mandatory restore test.

## Release gates and validation

C-SEC, route-context binding, A-DURABILITY and G-RECOVERY are **BLOCKED sub-contracts**, not security/data risk acceptance. Nova reviews independently; Forge supplies missing contract details and bounded feasibility evidence where needed before overall G2 is released. Full implementation/native security and recovery acceptance remains G4/G5. No unsupported template/upload implementation becomes eligible merely because other selections are recorded.

PR #135 remains **G3 DRAFT / NOT RELEASED**. Follow the owner-directed promotion order in ADR-0012, with merged authority, all task coverage, exact ownership/dependencies and immutable source. Do not create implementation Issues or change Project Ready/Done from this document.

This docs-only change must pass planning checker, self-test, artifact guard and diff check. M5 runtime/security/migration/crash/browser tests are NOT_RUN and not replaced by documentation checks.
