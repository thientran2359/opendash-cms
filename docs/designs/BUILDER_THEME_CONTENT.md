# Builder content, template resolution and theme structure

* **Author:** Atlas
* **Status:** Proposed — approved source decisions transcribed; EGM review and implementation readiness pending
* **Date:** 2026-11-20
* **Source decision commit:** `4a334f0224866108002e8586b04f5643da0798c9`
* **Source implementation baseline:** `cb76f21a2094ad8a1807addc1fe3187e8480475e`
* **Document conversion owner:** Atlas, explicitly authorized by the owner in this chat on 2026-11-20
* **Implementation:** Not authorized by this document
* **Verification:** Documentation checks only; new runtime behavior not tested

## Overview

OpenDash distinguishes reusable templates and parts from the content of a specific record. Page, Post and custom content types can register content Builder support independently of their ordinary editor. Templates choose the shared layout; editing one record's Builder content changes only that record.

The owner has already settled the theme package structure, Page layout behavior, template hierarchy, database/theme precedence, two Page settings and Section preservation described below. This document transfers those recorded decisions into the official EGM design-document structure. It does not reopen them or claim that the remaining API, storage and package schemas have been designed.

The original product document, resolver contract and ADR remain available at the source commit. Their approval provenance is retained at its original scope. This transcription is not an upstream EGM section-review receipt, a completed Goldfish specification review, a merged contract, or an implementation-ready task.

## The Problem

The existing bounded contracts do not specify the complete target model. The earlier M5 precedence places saved database overrides ahead of theme templates without first comparing candidate specificity. The Page Builder contract is entry-scoped but does not establish generic content-type Builder registration. The source decision record also identifies a separate experimental homepage storage path. These are source-baseline observations, not claims about later main revisions.

Without explicit boundaries, editing content could affect a shared template, a generic override could hide a more specific theme design, or selecting a homepage could expose a draft. Theme package folders alone cannot establish manifest support, typed composition, ownership or preservation. This specification consolidates the settled behavior while identifying the remaining decisions required before implementation.

## Goals

1. Keep reusable templates/parts and per-record content separate in storage targets, permissions, preview and Save.
2. Support content Builder only for content types that register it, preserving editor data, Builder AST and layout when configuration or providers change.
3. Resolve templates by candidate specificity first and source priority within each candidate, with stable identities, safe fallback and visible provenance.
4. Use the agreed theme root and `template-part/`, `page-layouts/`, `sections/`, `assets/` structure without inferring executable resources from directory contents.
5. Let ordinary Pages select named layouts without converting content. Keep homepage and post archive behavior distinct.
6. Let Admin select two different existing Pages for homepage and posts archive through one route authority, without implicit publication or experimental-homepage conversion.
7. Preserve user changes across theme updates, theme switches, provider loss, restart and backup/restore in the target model.
8. Retain every settled requirement, exclusion and unresolved decision from the pinned sources with traceable references.

## Non-Goals

- Backward compatibility with experimental theme/API/schema formats, legacy adapters, dual-read/dual-write or converters. There is no released older product to support.
- Permission to delete existing site data or directories. Removing experimental-format compatibility requirements does not remove content/layout, security, publication or backup/restore preservation requirements in the new model.
- Converting the experimental homepage into Page content or retaining an experimental legacy-homepage runtime mode.
- Synced Section propagation, file watchers, PHP themes, child themes, arbitrary template includes/scripts, or a new revision-history/undo feature.
- Search, author/date archives, attachment pages and embed routes in this resolver delivery. A template does not create a supported route.
- Final custom Builder block authoring syntax/SDK. Earlier custom-title, descriptor, renderer, CSS or blocks-directory examples are suggestions only. Theme/plugin block extensibility remains a V1 product requirement.
- CMS implementation, an Issue claim, Project Ready, Beta acceptance or release authorization from this document conversion.

## Open Questions

These items were already open in the source decisions. No answer is invented during transcription.

| ID | Required decision before affected implementation |
| --- | --- |
| OQ-01 | Exact Builder capability registration, built-in defaults, DTO/schema, content-mode persistence, switching and fallback behavior when capability/provider is unavailable. |
| OQ-02 | Target theme manifest, typed resource/candidate identities, package validator and storage/DTO mapping for root templates, parts, Page layouts and Sections. Logical names below are not a wire format or filename parser. |
| OQ-03 | Typed composition references, nesting/cycle/depth limits and validation of exactly one entry-content slot where applicable. |
| OQ-04 | Atomic homepage/posts-Page settings API, route claims, collisions, clearing settings, aliases/redirects, canonical and sitemap behavior. |
| OQ-05 | Theme Section stable identity/provenance, import/update/CAS, media remapping, collision handling and export if included. |
| OQ-06 | Exact runtime files, schemas, task slices, dependencies, file ownership and acceptance fixtures after the missing contracts are decided and actual current source is inspected. |
| OQ-07 | Custom Builder block developer contract, deliberately reserved for a separate discussion without dropping V1 extensibility. |

The product/theme no-compatibility directive does not authorize renaming GitHub tasks or deleting dispatch receipts. Task/transport continuity belongs to the separate EGM process migration.

## Critical User Journeys (CUJs)

### CUJ 1: Edit one record with Builder

* **Given:** Page, Post or a custom content type registers Builder, the user can edit record A, and records A and B share a template.
* **When:** The user opens Builder from A's edit screen, previews unsaved changes and saves A.
* **Then:** Every operation retains A's type/ID and effective permissions. B, the shared template, header/footer and public content outside A remain unchanged. Draft/publication and media rules still apply; preview does not write DB or public state.

### CUJ 2: Pick a Page layout

* **Given:** An ordinary Page and named layouts registered by the active theme.
* **When:** The user selects Default, Full width, Sidebar or Canvas where provided.
* **Then:** The Page uses the selected valid layout or the default Page branch. Its editor/Builder content is preserved. Canvas does not compose header/footer. Losing a layout retains its reference/content, reports a diagnostic and falls back safely.

### CUJ 3: Configure homepage and posts archive

* **Given:** Two different public, published Pages and an authorized Admin.
* **When:** Admin selects one as homepage and the other as posts Page.
* **Then:** `/` uses the homepage Page's content through the home/Page template branch. The posts Page's canonical URL shows the public post archive through `archive-post` then `archive`, preserving the Page body without rendering it as the archive. All navigation, canonical and sitemap claims use one authority. Selection never publishes a draft or creates/converts a Page.

### CUJ 4: Update or switch a theme

* **Given:** Site-owned template overrides, edited theme Sections, per-record content and an active theme revision.
* **When:** The theme is updated or switched, including A → B → A, or a referenced provider/layout disappears.
* **Then:** User AST/content, references and provenance survive. The resolver uses only eligible registered resources and active theme defaults; missing providers produce diagnostics without silent node deletion. Admin edits never modify immutable theme revision files.

### CUJ 5: Reset a shared template safely

* **Given:** A saved override, an effective theme selection and possibly an unsaved draft.
* **When:** Admin confirms resetting the current target.
* **Then:** The UI identifies the exact override and actual next source. Only that override is reset. Override and active-selection CAS are rechecked atomically; stale state yields 412 and preserves the draft. Entry content, other overrides, publication and theme selection do not change.

### CUJ 6: Reuse a theme Section

* **Given:** A theme Section original, an independently saved site customization and a destination record.
* **When:** The user edits the library Section or inserts it into the record.
* **Then:** Edits save to a site-owned DB customization. Insertion copies the AST with new node IDs. Later source edits do not propagate into previous insertions; user-created Section IDs cannot be taken over by a theme.

## Technical Plan (Architecture)

This section transcribes settled behavior. It does not resolve Open Questions or define new APIs.

### A. Separate shared layout from record content

Templates define the shared layout and an explicit place for a record's content. A record's editor and Builder sources remain distinct; switching mode does not implicitly convert text to AST or overwrite either source. Having a single template does not register Builder support. UI and backend both enforce effective capability, permissions and the exact immutable record target.

Record content follows record publication: Save of a draft remains private, Publish is explicit, and Save of a published record updates that record's public destination. This includes a Page selected as homepage. Template/part editing retains separate Admin authority; permission to edit one record does not grant global-template editing.

### B. Theme package structure

Main templates and header/footer files are at the theme root. There is no `templates/` or `parts/` wrapper for that group.

```text
my-theme/
├── theme.json
├── index.html
├── home.html
├── page.html
├── single.html
├── single-post.html
├── archive.html
├── archive-post.html
├── taxonomy.html
├── 404.html
├── header.html
├── footer.html
├── template-part/
│   ├── post-card.html
│   ├── breadcrumbs.html
│   └── pagination.html
├── page-layouts/
│   ├── full-width.html
│   ├── sidebar.html
│   └── canvas.html
├── sections/
│   └── hero.json
└── assets/
    ├── styles.css
    └── images/
```

These examples do not make every file mandatory or prove corresponding routes exist. Resources must be declared in a validated target manifest; files are not executed merely because they occur in a directory.

| Group | Settled role |
| --- | --- |
| Root HTML | Main templates and header/footer. Header/footer remain parts semantically despite being at root. |
| `template-part/` | Small reusable components such as cards, breadcrumbs and pagination through typed core composition. |
| `page-layouts/` | Named layouts offered on the Page edit screen, independent of record content/publication. |
| `sections/` | Theme Builder Section JSON linked to the Admin Section library. |
| `assets/` | CSS, images and static resources admitted by package validation. |

### C. Candidate hierarchy before source priority

Core checks route identity, publication, visibility, permissions, provider state and archive capability before resolving a template. Templates do not create routes, make private data public, query SQLite or construct canonical URLs independently.

```text
authorized request context
  → ordered typed candidates
  → for each candidate: site-owned DB override → active theme resource
  → selected template + explicitly selected typed parts + permitted content
  → context-preserving core fallback if required
```

Do not search all DB overrides before searching the theme. A theme-specific Project template wins over a generic DB single override. A DB override for that same Project candidate wins over the theme. An eligible explicitly assigned custom template precedes both.

| Context | Candidate order |
| --- | --- |
| One Post/CPT record outside the Page branch | Assigned custom template → single `(type, entry ID)` → single `(type, slug)` → single by type → single → singular → index |
| Ordinary Page | Assigned valid Page layout → page → index |
| CPT archive | Archive by type → archive → index |
| Custom taxonomy term | Taxonomy `(key, term ID)` → taxonomy `(key, term slug)` → taxonomy by key → taxonomy → archive → index |
| Core category | Category by term ID → category by slug → category → archive → index |
| Core tag | Tag by term ID → tag by slug → tag → archive → index |
| Configured homepage `/` | home → selected homepage Page's layout/page branch |
| Configured posts Page | archive-post → archive → index; post archive context, not the Page single/body branch |
| Missing homepage/posts configuration | Explicit unconfigured state; no Page creation, slug inference or experimental-homepage fallback |
| Not found | 404 → index with NotFound context; preserve HTTP 404 and do not render another record |

`home` means homepage. `archive-post` is a type-specific archive candidate, not a new role. No `front-page` or `blog` role is introduced. Post/CPT have single/archive branches; Page uses its specialized `page` default and optional layouts. `has_archive` governs Page archives too; private Section/CPT resources do not become public merely because a template exists.

Candidate identity is typed: kind, content/taxonomy key, stable ID and/or selector slug. Logical names are not a schema, a path assembled from a slug or a key split on hyphens. Stable ID precedes slug. Custom assignments must reference registered identities of the right kind and allowed type; unavailable assignments retain their reference/data, report a diagnostic and fall back to the ordinary branch rather than another custom template.

### D. Ownership, sources and fallback

Theme resources belong to `(theme_id, revision_id, resource key)` and revision files are immutable. Saved user customizations are site-owned, addressed by logical target; their base theme/revision records provenance without transferring ownership to the theme.

Updating or switching themes preserves site-owned overrides and content. Eligible overrides still apply at their exact candidate; they are not automatically rebased or overwritten with a new default. This does not copy a theme A design into theme B. Only resources valid under current registries, validators and permissions may execute. Defaults from inactive revisions are not used, though references are retained for explicit reuse/editing.

Missing provider nodes keep their AST/data for round-trip and use the existing missing-block diagnostics/policy. A corrupt or unsafe package/AST is not silently treated as absent: report it and use context-safe core fallback; if no safe fallback can be rendered, return generic noindex 503. Preserve the original data. NotFound remains HTTP 404. Selected Pages that become draft/private/deleted produce 404/noindex for their assigned function rather than exposing content or silently selecting a different Page.

Preview can overlay only its authorized candidate/target. It neither changes public resolution nor writes DB or grants public media access.

### E. Parts, content slot and Section composition

An explicitly referenced part resolves by the same key: DB part → active theme part → that part's core fallback. Do not substitute a differently named part. Do not add header/footer when the template did not request them; compose once. Only core performs typed composition. Do not enable arbitrary MiniJinja include, raw HTML interpolation, recursive includes, script execution or a C-SEC bypass. The precise references and depth/cycle limits remain OQ-03.

An entry template has exactly one core-controlled content slot populated from the authorized, identified record's selected editor/Builder source, with publication, field ACL and media rules. Archive/no-entry contexts do not require a fictional entry-content slot; they use their own permitted context/list.

Theme Sections have stable identities with theme provenance and site-owned DB customizations. Theme update keeps user changes; adopting a new original is explicit. Switching themes does not delete saved Sections or steal user-created IDs. Admin Save does not write immutable theme files. Insertion remains COPY AST with new IDs, never live propagation.

### F. Page layouts, shared template Save/reset and site settings

An ordinary Page defaults to `page.html` and offers Default or registered named layouts in the sidebar. Canvas omits header/footer; other layouts declare their parts explicitly and compose them once. Layout changes preserve content. Losing a layout retains its reference/content, reports it in Admin and falls back `page → index → core`. This supersedes the earlier Page-only-single-by-type decision. A posts Page stays on the post archive branch; a homepage still tries `home` first.

Shared template/part Save requires Admin plus existing guards/CAS, writes only the target DB override, and does not edit revision files or record content. Reset confirms the exact target, discarded customization/draft and actual next source. With no corresponding theme resource, offer an explicitly named fallback reset instead of promising a theme original. Entry content has a separate reset/mode contract. Reset checks both override CAS and active-theme selection/revision in one transaction; stale yields 412 and preserves the draft. It does not reset other candidates, content, publication or active theme. A more general DB candidate may legitimately become the next source. Preview/GET never reset state.

Homepage and posts settings hold stable IDs of two different existing Pages. Only Admin may change them through canonical settings guards/CAS. Selected Pages must be public and published when applied; selection never publishes them. The homepage supplies content at `/`. The posts Page supplies the canonical URL and permitted title/metadata for a post archive; its body/AST remain intact when assigned, changed or cleared. Only public posts appear, with existing pagination, filters/counts, media ACL and SEO. On clearing its role, the Page returns to its normal layout/page branch.

One core route authority owns both claims, links, canonical, menu and sitemap output. Do not create competing content at the same URL or move redirect logic into the template resolver. Atomic update, conflicts, old aliases/redirects and clearing semantics require OQ-04. An unconfigured site reports that state in Admin without creating a Page, choosing by slug, converting experimental homepage data or retaining a legacy runtime. Target fixtures use the new single format/API/schema; existing sites are not deleted or altered to simulate implementation.

## Alternatives Considered

These dispositions come from the pinned source decisions; no new architectural selection is made here.

| Alternative | Mechanism, benefit and limitation | Recorded disposition |
| --- | --- | --- |
| All DB overrides before every theme candidate | Simple source-wide override priority, but a generic DB single hides a type-specific theme template. | Rejected by Atlas in the resolver decision; use candidate hierarchy first, then source priority. |
| Page always uses single-by-type | Reuses a generic single branch but does not implement the explicitly selected `page.html` default and named Page layouts. | Superseded by the owner's 2026-11-20 Page/layout decision. |
| Convert the experimental homepage or maintain an adapter | Could retain an experimental runtime path, but introduces compatibility work the owner explicitly excluded. | Rejected by the owner; keep site data safe without creating a legacy runtime, conversion or dual-write contract. |
| Automatically propagate Section edits | Makes inserted instances live-linked, changing the approved COPY behavior. | Excluded from this scope; keep independent AST copies with new IDs. |
| Treat custom block examples as the developer API | Would prematurely fix descriptor/renderer/schema choices. | Deferred by the owner; preserve V1 extensibility and settle authoring separately. |

## Non-Functional Requirements

Retain Rust/SQLite, existing core/storage separation, canonical registries and route authority. Reuse shared identifier, registry, hook, safe ZIP/package, authentication/CSRF, ApiError, storage, CAS, AST, MiniJinja/CSP and C-SEC boundaries. Template resolution does not introduce route SQL or bypass resource validation.

Keep unsaved drafts on 401/412 and correct target changes; test Admin/Editor, CSRF, publication, media visibility and provider eligibility. Permissions on record content and shared templates remain separate. Package validation, typed composition and safe fallback must resist traversal and unsafe input without deleting user data or widening validators.

Preserve backup/restore and restart behavior on the new target baseline. No experimental-format upgrade compatibility is required. Existing failures and native/security/release obligations remain historical evidence; this design does not change completed-task acceptance, grant Beta, clear F08/F09, replace native checks or human dogfood, or authorize public release.

No new latency/benchmark target is claimed by the source decisions. Implementers must measure relevant behavior when the exact resolver and fixtures are designed, rather than adding ungrounded performance guarantees here.

## Detailed Implementation Plan

### Admission Boundary

This is an EGM feature-specification transcription, not an executable CMS implementation plan. OQ-01 through OQ-06 prevent implementation readiness. Do not generate an Approved implementation Issue from this document or treat the file inventory below as runtime file ownership. Before code: settle the missing contracts, inspect current owning source/CodeGraph, enumerate exact files and tasks, record immutable source/base and ownership, verify dependencies and live Project Ready, then meet the applicable design review/approval gates.

### Target Files to Create/Modify

The present, owner-authorized conversion changes documentation only:

| File | Action | Reason |
| --- | --- | --- |
| `docs/designs/BUILDER_THEME_CONTENT.md` | Create | Consolidate the settled Builder/theme decisions under the official EGM template, preserve provenance and enumerate unresolved implementation design. |
| `docs/README.md` | Modify | Make the EGM feature specification discoverable and state its actual review/readiness limits. |

The eight source documents were committed first at `4a334f0224866108002e8586b04f5643da0798c9`, per the owner's instruction. This conversion does not delete them or rewrite prior decisions/verification. No `docs/designs/SITEMAP.md` exists in this checkout; the separate EGM process integration must register this document alongside Nova's sitemap without overwriting either branch.

### Verification Plan

**Documentation conversion:** run `python cms/scripts/check-plans.py`, `python cms/scripts/check-plans.py --self-test`, staged whitespace checks, the normal CodeGraph document indexing command and explicit indexing of this new document if the existing fixed source inventory omits it. Reconcile the source-to-spec table below against the three pinned primary documents. These checks do not constitute Goldfish or runtime acceptance.

**Required future behavior evidence:**

1. Matrix of ordered candidate × DB/theme × target: theme-specific beats DB-generic, DB same-candidate beats theme, eligible custom assignment first, assignment loss and stable ID/slug changes.
2. Page/Post/custom CPT Builder registration and denial in UI/backend; editing A leaves B, shared template and parts untouched. Mode/capability/provider changes retain both editor/AST sources.
3. Draft Save, unsaved preview, publication and media privacy, including the selected homepage Page; published Save updates only the correct destination. Admin/Editor, CSRF, 401/412 and dirty-target navigation preserve permissions and drafts.
4. Root/package manifest validation, explicit typed parts, exactly-once header/footer, exactly-one entry-content slot where required, nesting/cycle bounds, Canvas and missing-layout preservation/fallback.
5. Exact-target template Save/reset, correct next-source UI, absent-theme-resource reset and override/active-selection CAS races without modifying files, unrelated overrides or record content.
6. Theme A → B → A, update, missing blocks versus corrupt/unsafe documents, no inactive defaults, preserved references/data and 404/noindex/503 semantics.
7. Two independent Page settings, existence/type/visibility/publication validation, no implicit publication or Page creation/conversion; archive-post behavior, retained posts-Page body and counts/filter/pagination/SEO/media ACL.
8. Single route authority for homepage/posts claims, collisions, clearing, canonical/redirect/menu/sitemap behavior once OQ-04 is settled.
9. Theme Section identity, user-ID collision, original/customized update, CAS/media remap and COPY with fresh node IDs. No immutable-file edits or automatic propagation.
10. Restart and backup/restore on the target baseline preserve relevant data/layout without creating unexpected Pages. No experimental upgrade or compatibility suite is added.

Each future task must bind fixtures, commands and results to the reviewed head and candidate/platform. CI, post-merge acceptance, native platform tests, security/data gates, human dogfood and release authorization remain separate obligations. Runtime tests are NOT_RUN in this documentation conversion.

### Documentation Conversion Receipt — 2026-11-20

- `python cms/scripts/check-plans.py`: PASS on the current Atlas checkout.
- `python cms/scripts/check-plans.py --self-test`: PASS, 12 fixtures.
- A read-only `python -B -c` comparison confirmed the complete theme example tree is identical to the committed product source.
- The three primary source files have no diff from `4a334f0224866108002e8586b04f5643da0798c9` after conversion.
- `node cms/scripts/codegraph-index-docs.mjs`: PASS. Its current fixed source list omits these new Builder/theme documents, so the same engine was explicitly run with `codegraph_index_markdown` and the actual Atlas workspace for the three primary sources and this EGM document. This is indexing, not design approval.
- `git diff --cached --check`: PASS for the staged documentation conversion.
- Goldfish specification review, runtime/build/native tests, human dogfood and release acceptance: NOT_RUN in this conversion. The earlier seven-lens EGM process-migration brainstorm is not a review of this feature specification.

## Source-to-Spec Traceability

All primary sources below refer to content committed at `4a334f0224866108002e8586b04f5643da0798c9`. Local links provide navigation; that SHA pins the imported version.

| Source decision group | Pinned source document/section | Destination in this specification |
| --- | --- | --- |
| Authority, design versus implementation state | [ADR-0020](../decisions/ADR-0020-builder-template-resolver.md), header and scope | Header, Overview, Admission Boundary |
| No experimental backward compatibility; data preservation retained | [Product](../product/BUILDER_TEMPLATE_AND_CONTENT.md), owner directive; [resolver](../contracts/BUILDER_TEMPLATE_RESOLVER.md), directive; ADR directive | Non-Goals, Open Questions boundary, D/F, non-functional requirements |
| Shared templates versus entry content and permissions | Product, two Builder scopes and registration | Goals 1/2, CUJ 1, A/E, behavior evidence 2/3 |
| Editor/Builder independence, switching and provider loss | Product, registration | A, OQ-01, D, evidence 2/3 |
| Publication, homepage draft and unsaved preview | Product, registration and acceptance; resolver 1/4 | CUJ 1/3, A/C/D, evidence 3 |
| Role meanings, Page supersession and archive flags | Product, hierarchy/Page layout; resolver 1/2; ADR additions | B/C/F, candidate matrix, evidence 4/7 |
| Full candidate hierarchy, ID/slug priority and custom assignment | Resolver 1/2 | C and complete candidate matrix, evidence 1 |
| Per-candidate DB/theme priority | Resolver 1/3; ADR decision | C, alternatives, evidence 1 |
| Root templates/header/footer and four directories | Product, theme structure; resolver 9; ADR additions | B with complete source example tree and group table |
| Manifest declaration, example files and target schema limits | Product, theme structure; resolver 1/9 | B, OQ-02, Admission Boundary |
| Page default, layouts, Canvas, missing-layout retention | Product, Page layout; resolver 1/2/9 | CUJ 2, B/C/F, evidence 4 |
| Theme/site ownership, immutable revisions and active-only defaults | Resolver 3 | CUJ 4, D, evidence 6 |
| Missing provider versus unsafe/corrupt data and safe fallback | Resolver 3 | D, evidence 6 |
| Explicit typed parts, no arbitrary include, content slot | Resolver 4 | E, OQ-03, evidence 4 |
| Section originals, stable provenance, DB customizations and preservation | Product, Section theme/Admin; resolver 9; ADR additions | CUJ 6, E, OQ-05, evidence 9 |
| Section COPY and fresh IDs, no propagation/watchers | Product, two scopes/Section; resolver 4/9 | Non-Goals, CUJ 6, E, evidence 9 |
| Shared template Save/reset, UI source, CAS/theme race and no side effects | Resolver 5 | CUJ 5, F, evidence 5 |
| Distinct selected Page IDs, Admin validation, homepage/posts meanings | Product, settings; resolver 6 | CUJ 3, F, evidence 7 |
| Posts body retention, public archive and metadata | Resolver 6 | CUJ 3, F, evidence 7 |
| Route authority, conflicts, aliases/redirects, clearing and unconfigured state | Product, settings; resolver 6 | C/F, OQ-04, evidence 7/8 |
| No homepage conversion, experimental runtime or unrequested data deletion | Product directive/settings; resolver 6; ADR | Non-Goals, F, evidence 7/10 |
| Unsupported routes, child themes and PHP | Product hierarchy; resolver 2 | Non-Goals |
| Custom block authoring deferred without dropping V1 extensibility | Product block paragraph; resolver 9; ADR additions | Non-Goals, OQ-07, alternatives |
| Existing Page-only and C1 contracts are not generic Builder implementation | Product contract relationship; [Admin contract](../contracts/ADMIN_REVIEW_2026_11.md), follow-up; [Content contract](../contracts/CONTENT_FOUNDATION_2026_11.md), follow-up | Problem, OQ-01, Additional Resources |
| Full acceptance obligations and incomplete API/schema/ownership | Product acceptance; resolver 7/8; ADR scope | Goals, Open Questions, Admission Boundary and all behavior evidence |

## Additional Resources

- [Original product decisions](../product/BUILDER_TEMPLATE_AND_CONTENT.md), confirmed by the owner on 2026-11-19/20.
- [Original resolver contract](../contracts/BUILDER_TEMPLATE_RESOLVER.md), Atlas Final Decision with contract effect after merge.
- [ADR-0020](../decisions/ADR-0020-builder-template-resolver.md), preserved decision provenance.
- [Product requirements](../product/PRODUCT_REQUIREMENTS.md), [M5 theme contract](../contracts/M5_THEME_TEMPLATES.md), [Admin contract](../contracts/ADMIN_REVIEW_2026_11.md) and [Content foundation](../contracts/CONTENT_FOUNDATION_2026_11.md).
- [Core Foundation](../contracts/CORE_FOUNDATION.md) and [Plugin ABI](../contracts/PLUGIN_ABI.md), mandatory architectural/security boundaries.
- Installed official EGM 0.1.1 `egm-draft` template and Codex integration guidance. Source decisions remain approved at their recorded scope; the EGM transcription itself remains Proposed pending review. No new API/schema decision or EGM approval receipt is inferred.

## Future Improvements

Complete the open implementation contracts and their exact file/task plans before code. Discuss custom Builder block authoring separately. Any later route expansion requires its own approved context, permissions and acceptance; this document does not grant WordPress parity.

---

*Transcribed into the official EGM design-document structure from pinned, already approved decisions. New EGM specification review and unresolved implementation design remain pending; no production implementation was changed.*
