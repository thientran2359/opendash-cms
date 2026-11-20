# Content foundation — November 2026

> [!NOTE]
> **Trạng thái hiện hành (2026-11-20):** **Implemented** (migrations `0038`–`0041`, bao gồm `0041_provider_rewrite_defaults.sql` cho C4; xem [`docs/designs/SITEMAP.md`](../designs/SITEMAP.md)). Header `implementation NOT_STARTED, verification NOT_RUN` bên dưới là snapshot lịch sử lúc chốt thiết kế.

Decision owner/coordinator: Aster. Implementer: Harbor. Effective after merged
ADR-0019; design AGREED then, implementation NOT_STARTED, verification NOT_RUN.
This is a bounded extension of CNT-01/03/04, SITE-02 and PLG-03, not WordPress API compatibility.

## 1. Evidence and alternatives (G1)

Inspected source baseline `2ebe51cbe1099c4721304e2b31895aa78b7c1241` via
CodeGraph then real files. ContentTypeDefinitionInput/ContentTypeDefinition in
core content.rs contain only key/label/fields. storage/entries.rs hardcodes
post/page draft semantics; Section has a separate private contract. Migration0012
restricts terms to category/tag and category parents,0014 links IDs. Core
CntDeclarations rejects other kinds. public_context.rs builds fixed /content and
/taxonomy URLs; storage/seo.rs adds per-entry paths/redirects. None of these is a
custom taxonomy registry or global permalink configuration. Admin term panels
and provider columns from ADR-0018 do not supply either missing feature.

Choose additive canonical configuration and a single resolver. Separate provider
registries/storage or frontend-only URL rewrites would duplicate authority.
Unrestricted regex/PHP-like filters would add execution/ambiguity boundaries
without fulfilling a necessary first slice. Keep typed declarations and existing
Wasmi hooks, with tests proving their interaction with new content settings.
Graph is a locator, not runtime evidence; stale symbols were checked on disk.

## 2. Content type settings (C1)

Add optional `settings` to existing CPT create/update/read DTOs and canonical
schema JSON. All fields deny unknown keys. Settings contain `public: bool`,
`has_archive: bool`, `publication: immediate|draft_publish`,
`supports: [editor,featured_image]` (unique enum set). Title/slug/custom fields
remain core data, not toggleable. Missing settings on create/import means legacy
defaults: ordinary CPT public/archive true, immediate, both supports; post is
public/archive true, draft_publish; page public/archive true,draft_publish.
Omitted settings on update preserve current settings. Reads return effective settings.
Legacy Page archive remains valid until explicitly changed; migration must
not silently remove an existing public URL. Existing built-in body/image fields
stay; supports controls editing, never deletes values/schema/media references.
No post/page status migration. Switching an existing immediate CPT to draft_publish
keeps published entries published; new entries default draft. Reverse switch is
409 while drafts exist: no implicit bulk publication. Public false hides entries,
archives, sitemap and media access derived solely from those entries; it does not
prevent authorized editing/preview. has_archive only controls archive exposure.
Section is immutable private and uses its existing dedicated editor/document policy;
settings cannot expose it. Post/page immutable publication mode. No entry hierarchy
in this increment; custom fields are not an alternative hidden parent relationship.

Use current CPT schema token/CAS mechanism for settings; no parallel version counter.
Admin-only schema mutation with existing auth/CSRF/Origin and CAS; stale version
412. Editor reads effective configuration and edits allowed entries only. Existing
save/publish endpoints and ContentSave use the same effective publication policy.
Schema token includes settings. Configuration, values and provider ownership use
existing canonical storage/registry; no SQL in routes.

## 3. Taxonomy definitions and membership (C2)

New canonical TaxonomyDefinition: `key,label,hierarchical,public,version,owner`.
Input key is immutable valid core identifier (same key validation rules as CPT),
label uses valid_label. User create supplies key,label,hierarchical,public;
version/owner server supplied. Reserve category/tag and all core reserved keys.
Seed category(hierarchical true) and tag(false),public true. Their hierarchy flag
and keys immutable. Max64 definitions/site, label100 characters. Custom taxonomy
hierarchical default false/public true. Keys are globally unique across taxonomy
owners; collisions fail, never adopt another owner's definition.

Forward migration creates taxonomy_definitions keyed by text, with ownership and
version consistent with registry. Rebuild taxonomy_terms replacing kind CHECK with
FK kind→definition key, preserving all IDs, versions, parent links, extensions and
entry_terms. Keep wire field `kind` as stable taxonomy key for compatibility.
Parent must be in same taxonomy; only hierarchical taxonomies accept parent,
max depth8 including root, no cycles. Generalize triggers and storage validation;
writer reservation precedes ancestry reads. Copy/rebuild dependent FKs safely under
migration transaction; PRAGMA foreign_key_check and integrity_check must pass.
Applied0012/0014 are immutable. All existing eligibility rows survive unchanged.

New Admin endpoints `/api/admin/taxonomy/definitions` GET/POST and
`/api/admin/taxonomy/definitions/{key}` GET/PATCH/DELETE. PATCH/DELETE require
If-Match version; 412 stale,400 invalid,409 ownership/in-use conflict,404 missing.
Only user definitions may be deleted; reject with409 if terms, associations or
provider references exist. Reject hierarchical true→false while parents exist.
Post/page eligibility defaults remain existing policy (page none); configurable
association endpoint PUT `/api/admin/taxonomy/eligibility/{content_type_key}`
accepts `{taxonomies:[key],expected_version}` with CPT/schema CAS. Admin can set
site override for any editable CPT, except Section, with precedence over provider
defaults. Max64 unique keys, all must exist; inactive retained definitions remain
manageable. Removing association preserves existing links but hides them from
picker/filter/public queries; entry update with omitted term_ids preserves links,
explicit term_ids may only select eligible terms. No cross-taxonomy parent/link
leak. Old category/tag endpoints keep their methods and payloads.

Admin configuration belongs alongside content types/fields, a separate taxonomy
definition route; each definition has a term list and dedicated term editor.
Hierarchical picker tree versus flat tag picker follows definition, never key name.
Generalize table filters, menus, Builder taxonomy selectors, template/public query
and term extension panels. Existing category_id/tag_id query keys remain aliases;
new `taxonomy_key`+`term_id` pair supports one generic filter, positive canonical ID,
reject half pair/mixed aliases and cap pagination as today. Descendants only when
selected taxonomy is hierarchical. Do not silently ignore unsupported filters.

### C2 corrective read projections and CAS clarification — 19/11/2026

Aster Final Decision for [#375](https://github.com/thientran2359/opendash-cms/issues/375),
reviewed corrective head `9017b9cb2af9b71c8b6975a88c69db6209f160d7`.
These additive Admin reads support the existing C2 configuration and entry tables.

- Admin GET `/api/admin/taxonomy/eligibility/{content_type_key}/configured`
  returns `{taxonomies:[key],has_override:bool,effective:[key],type_version:i64}`.
  The configured selection preserves inactive/withdrawn provider associations;
  the effective selection retains current visibility rules. All fields, including
  the CPT CAS version, come from one SQLite read transaction. Section rejects400;
  missing CPT404; existing Admin authentication applies; response is no-store.
  A client saves only against the version matched to its configured baseline.
  A separately fetched CPT version cannot silently replace this token. Rebase
  reapplies local additions/removals to a fresh matched snapshot and leaves
  Save blocked when the displayed snapshots disagree. A later writer still
  causes412 at the existing eligibility PUT.
- Session-authenticated GET `/api/admin/taxonomy/terms/resolve?ids=...` returns
  an array of `{id:i64,kind:key,name:string}` for requested existing term IDs.
  Follow the existing Admin term-read permission boundary, including Editor.
  Require1–100 distinct canonical positive i64 IDs separated by commas, maximum
  2000 characters; malformed/duplicate/overflow inputs400, missing session401.
  Missing IDs are omitted. Return no-store and only the requested projection,
  using parameterized storage queries. Entry tables request visible-row IDs in
  batches of at most100, with at most two concurrent label requests; full term
  catalogs load only for actively used filters. Report loading/failure explicitly.
- Taxonomy eligibility site overrides are an Admin operation for eligible
  non-Section CPTs, including plugin/theme CPTs. This does not grant provider
  schema-definition edits. Core category/tag label/public edits remain supported;
  their key/hierarchy and user-only deletion constraints remain in force.
- Meaningful existing provider CPT schema/status/declaration or eligibility
  changes advance the owning CPT CAS in the same writer transaction, once per
  affected key in that operation. Identical redeclarations/status calls are
  idempotent. Preserve links on withdrawal/deactivation, including separately
  declared eligibility on a withdrawn CPT. No second version counter or migration.

These reads do not generate canonical public custom-term URLs. C3 remains the
authority for that behavior. Exact merged-source native release G5 is a separate
gate from this design clarification and premerge regression evidence.

## 4. Permalinks and route authority (C3)

One storage-backed resolver for entry/term/archive/home with stable target IDs,
used by public matching, canonical/OG/sitemap, menus, Builder bindings/list items,
editor View links and redirects. No consumer may concatenate a competing URL.
Default is legacy fixed routes, unchanged on upgrade. Global post structure has
presets legacy `/content/post/%postname%`, `/%postname%`, `/blog/%postname%` and
custom bounded format: literal kebab path segments plus exactly one terminal
`%postname%` or `%post_id%`; max8 segments,512 bytes. No regex, date/category token,
query/fragment, encoded slash, empty/dot traversal segment. Page stays legacy in
this increment; hierarchical Page URLs explicitly deferred. CPT entry format is
`/<rewrite_base>/%postname%` with base1–3 kebab segments; optional archive at base.
Taxonomy term format `/<rewrite_base>/%term%`; hierarchy affects terms, not URL.
Null base retains legacy path. Per-entry custom_path has precedence over generated
entry path. `public`/publication/provider activity guards precede all resolution.

Admin GET/PUT `/api/admin/settings/permalinks`, DTO
`{version,post_structure,content_type_bases:{key:base|null},taxonomy_bases:{key:base|null}}`;
PUT requires If-Match. Unknown/private keys reject400. Route roots /api,/admin,
/uploads, assets, auth, health, sitemap/robots and all current reserved paths remain
reserved, including prefixes. Root / stays homepage. Use existing path validation
and expand it centrally, not a second allowlist. Serialized writer transaction
validates complete proposed namespace including templates/static routes, custom
paths, old redirect claims, generated entries and term/archive routes; duplicates
or overlapping dynamic patterns409 with target IDs, never first-match ambiguity.
Own redirect source can become own canonical path; no redirect loops/chains.

Persist site permalink config/version and canonical route claims keyed by stable
target kind/ID, with unique normalized path; retain historical paths as301 targets
by stable ID, including archive/term changes. Recompute on settings, slug, term,
publication, CPT/public/eligibility/provider changes in the same write transaction.
Any collision aborts all content/config/claims changes. No manual flush/restart.
Bulk settings update is atomic;50000 affected claims max, reject larger request
without change (offline migration outside this scope). Draft paths may be reserved
but never resolve publicly; redirects to missing/private/draft target return404.
Old legacy routes301 to new canonical once opted in; no mass redirects on upgrade.

## 5. Plugin/theme declarations and hooks (C4)

Extend optional CntDeclarations with `taxonomy_definitions` (key,label,hierarchical,
public), content_types[].settings and optional `rewrite_base` for declared CPTs and
taxonomies. Old manifests remain valid without changed behavior. New defaults use
same validators/domain APIs as Admin. A provider may declare own CPT associations
to core/site taxonomies and its own declared taxonomies, not claim someone else's
key. Existing category/tag eligibility continues. Apply package declarations as
one transaction, reject conflict before activation; preserve previous active package
on failure. Provider-generation validation remains mandatory.

Ownership immutable; site overrides win over package defaults. Package update,
disable/uninstall, theme switch/restart keep canonical definitions, terms, entries,
links, field values, overrides and editable fallback. Retained content follows
existing inactive-provider public policy; do not turn private content public.
Re-enable restores matching ownership only. Rewrite default changes obey C3 claims
and atomic collisions, not overwrite site choices. Theme is declarative only.
Plugin runtime uses existing ContentBeforeSave Allow/Veto, ContentAfterSave and
ContentSave host operation/permissions; no arbitrary value mutation or URL filter
hook is introduced. Prove those hooks operate on new CPTs with custom term IDs,
publication rules, CAS and provider generation; update SDK/examples and support
matrix to explicitly distinguish declarations, actions, veto and unsupported filters.

## 6. Delivery, migration and limits

Six serial tasks in linked G3 plans. Reuse core id/registry/HookDispatcher,
server package/auth/ApiError and storage transaction APIs. No dependency upgrade.
Migration0038 CPT settings,0039 taxonomy registry/rebuild,0040 route config/claims
reserved in this order against baseline0037; re-fetch before use, never renumber
an applied migration. New persistence participates in existing backup manifest,
restore validation and startup reconciliation; no broader recovery redesign.
Test isolated macOS root and port, never owner's18964 data. Preserve source DB,
forward migration/recovery on copy, no claim that Git revert downgrades schema.
Native macOS G5, CI, Windows/Linux and human Beta verdicts are separate. Keep #302
Open and Project untouched. UI polish, full Builder redesign, entry hierarchy,
advanced WP rewrite and unrestricted plugin/theme execution are outside this scope.
