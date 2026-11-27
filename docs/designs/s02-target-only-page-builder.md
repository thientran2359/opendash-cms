# S02 target-only Page Builder and provider selections

**Author:** Aster
**Status:** Verified
**Date:** 2026-11-21
**Approved By:** Owner (target-only scope and sequence); Aster (delegated technical closure).
**Approval Evidence:** [Owner direction recorded on #427](https://github.com/thientran2359/opendash-cms/issues/427#issuecomment-7059083925) and current-chat instruction to finish design/review/handoff in one run.
**Goldfish Review:** [PR #437 review closure](https://github.com/thientran2359/opendash-cms/pull/437#issuecomment-7059352039): design_critic, codebase_details and implementation_readiness; rounds 5 and 6 PASS in fresh contexts; all earlier findings resolved. Implementation requires merge and exact-source handoff.
**Baseline:** main `29681bd336a0f20e876d6b9273b45aaaf9d6c304`; implementation PR #436 `a5b4f1d23c475076cba62b11403bc2cbe8af9811`.
**Owner Direction:** [Owner clarification recorded on Issue #427](https://github.com/thientran2359/opendash-cms/issues/427#issuecomment-7059083925). Owner subsequently instructed Aster to finish drafting, review, coordination and handoff in one run without repeated section approval stops.

## Overview

OpenDash is an unreleased prototype. Build one target schema and API. Do not migrate historical Page layouts or plugin selections, retain old API aliases, maintain dual stores, or support old theme formats. Existing running site directories are outside this task and must not be reset or deleted.

Bring forward the Page-only portion of the entry Builder foundation from the [CPT Builder design](cpt-content-builder-and-pattern-library.md) into Theme S02. Entry content and shared site templates are distinct. Keep generic CPT enablement in CPT S01, Pattern features in CPT S02/S03, and Reading Settings in Theme S03.

## The Problem

PR #436 replaces `template_overrides` with a table keyed by a typed shared-template candidate. That target cannot hold individual Page bodies. The Page editor still calls old scoped override endpoints and requires Editor-authorized read/save/unsaved preview/reset. Sending Page content to the global `page` template would alter other Pages.

The existing M6 contract also requires owner-neutral selection storage. Its experimental tests write owner fields into the old shared override table. Replace their fixture shape with the target selection store; no old-row conversion is needed. This is current target functionality, not historical compatibility.

## Goals

- Ship Page Builder and shared template overrides with independent persistence and correct authorization.
- Use the final entry document tables now, with one entry CAS token and existing Page publication behavior.
- Retain the M6 target's owner-neutral selection/settings/AST behavior, without turning plugin selection metadata into a shared site template.
- Give Harbor exact file ownership and test expectations, then resume #427 after the amendment merges.

## Non-Goals

No historical migration, downgrade support, old route aliases, old-format theme acceptance, generic CPT Builder UI, Pattern features or S03 Reading Settings. No dependency upgrade, live site operation, security waiver or test skipping.

## Technical Plan

### 1. Ordering and schema

Theme S02 includes this Page foundation before it is accepted; do not merge an S02 build that leaves Page Builder broken. Amend the unmerged `0042_theme_v3_resolver_and_reading_settings.sql`. Keep applied migrations 0001–0041 unchanged. `0043_cpt_content_builder.sql` later EXTENDS these tables/columns and must not recreate them; `0044_pattern_cpt_and_refs.sql` remains reserved for Patterns. SQLx checksum errors in throwaway development fixtures are handled by creating a new isolated fixture root, not deleting migration history or touching a running site.

Add `content_entries.content_mode TEXT NOT NULL DEFAULT 'editor' CHECK(content_mode IN ('editor','builder'))`.

Create `entry_builder_documents(entry_id INTEGER PRIMARY KEY REFERENCES content_entries(id) ON DELETE CASCADE, document_json TEXT NOT NULL CHECK(json_valid(document_json)), updated_at TEXT NOT NULL)`. There is no independent document version; `content_entries.version` is the sole CAS token. No row means no saved Builder document and normal text rendering. Entry content_mode chooses which saved source to execute; switching modes preserves both sources until an explicit reset.

Create `entry_builder_media_refs(entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE, media_id TEXT NOT NULL REFERENCES media_assets(id) ON DELETE RESTRICT, ref_role TEXT NOT NULL CHECK(ref_role IN ('image','video','poster')), PRIMARY KEY(entry_id,media_id,ref_role))`, plus an index on media_id. Persisted refs pin media regardless of publication; public access additionally requires eligible published entry and Builder mode.
The typed `template_overrides` table remains exclusively shared site templates. It never stores Page bodies. Do not copy rows from its old experimental shape. The existing bounded theme-package/revision table rebuild in PR #436 is otherwise unchanged.

### 2. Page-only capability and API

Add the already planned pure `ContentSupport::Builder` and `ContentMode` types in core. Built-in page enables Builder by default; post/custom CPTs remain disabled at this stage. Defer schema-settings UI and generic enablement to CPT S01. In this stage APIs reject type_key other than `page` with 403 after authentication, and return 404 for a mismatched/missing entry. No Section behavior changes.

Register a single target family:

| Method and path | Request | Response |
| --- | --- | --- |
| GET `/api/admin/entries/page/{id}/builder` | Session with existing Page Editor access | 200, no-store, ETag vN; `{entry_id,type_key,content_mode,version,document: BuilderDocument or null}` |
| PUT same path | Editor session+CSRF, admission lease, required If-Match vN, JSON BuilderDocument at most 256 KiB | 200, ETag vN+1; same read DTO after save |
| POST same path + `/preview` | Editor session+CSRF; JSON unsaved BuilderDocument at most 256 KiB | 200 no-store; existing RenderedLayout `{html,diagnostics}` JSON shape (contact-form flag is internal and sets response CSP); no DB writes, hooks or public ACL grants |
| POST same path + `/reset` | Editor session+CSRF, admission lease, required If-Match vN; `{confirm:true}` | 200, ETag vN+1; read DTO with document null and content_mode editor |

Missing If-Match returns 428, stale token 412, invalid JSON/AST/confirmation 400, wrong content type 415. Reuse existing auth, session recheck during write, parse_if_match/no-store helpers and ApiError. Explicitly reject a preview target supplied in query/body; preview is bound to the path entry. Unknown fields are rejected. Anonymous requests return 401 before capability or entry inspection. Both Admin and Editor use the same entry authorization path; shared templates remain Admin-only.

PUT validates the AST against the active descriptor registry and its existing document for retained inactive blocks; preserves current unknown-block round trips, route-context/field ACL, media validation, sanitize/C-SEC and resource limits. It saves the AST, sets content_mode=builder, replaces entry media refs, increments entry version once and updates timestamp atomically. It leaves values_json/body, other entries, shared templates and publication_status unchanged. Published Page Save updates immediately; draft Page Save stays private. This preserves PRD section 2, including its specific exact Page Builder rule; it does not introduce a published-document snapshot or a new publish workflow.

Before Save and Reset, prepare the current Page schema token and current entry data, then invoke `dispatch_content_before_save_for_actor(type_key, actual_actor_role, payload)` before acquiring the SQLite writer. Payload is the existing M6 envelope `{operation:"update", entry_id, expected_version, schema_token, title, slug, candidate_values, term_ids}` from the current entry, plus `builder_operation:"save"|"reset"`. No document/AST is exposed through the hook payload and no new plugin capability is introduced. Use the existing actor-aware field filtering (never the convenience Admin default). Veto returns existing 422 and changes nothing. Inside the admitted writer recheck session/role, schema token, entry CAS, type/capability, registry validation and media existence before mutation; a race returns 401/403/412 as appropriate without partial writes. After commit invoke existing bounded ContentAfterSave once with the standard actor-filtered `{operation:"update", entry_id, version, type_key, values, term_ids}` plus builder_operation. Preview fires neither hook. Reuse existing hook manager envelopes rather than implement a second dispatcher.

Reset requires confirm=true and a current entry CAS. It deletes only that entry document and its refs, switches content_mode=editor, increments entry version once and returns document null. It does not alter text values, Page B or shared templates. On a published Page this immediately restores normal text rendering, matching existing exact Page Builder Reset semantics; a draft Page remains private. Missing document reset returns 404 without advancing the version. A later Save recreates the row using the current ENTRY token, never v0.
Composite entry saves accept `content_mode: PatchField<ContentMode>`: omission preserves mode, null is rejected, editor/builder are accepted only for a Page with Builder support; builder requires a saved document. Ordinary entry edits and Builder edits share the entry version. After either succeeds, the UI reloads authoritative entry metadata/version without replacing a dirty local draft. 401/412 never discard local drafts or silently overwrite the remote document.

### 3. Existing publication and rendering

Keep both existing publication entry points: composite save and `/api/admin/content-types/{type_key}/entries/{id}/publication` (`routes/content.rs::content_entry_publication` -> `storage/entries.rs::set_publication_status`). Neither performs snapshot promotion because there is no snapshot table. Both use existing entry CAS/status/permalink rules. Explicit Publish makes saved Builder content eligible for public rendering/media if content_mode=builder; draft/unpublish removes that eligibility through current status predicates. Ordinary Builder Save never changes publication_status. Test the actual registered dedicated publication API as well as composite Publish, not an unused handler in routes/entries.rs.

A public Page first passes existing public-entry/provider/status ACL. In a single read transaction capture entry, saved Builder document and selected template needed for coherent rendering. If mode is builder and a valid document exists, render it as entry body inserted into od_content/core/content_slot; otherwise render normal body. Apply the approved page layout/template wrapper and site header/footer once, forward contact-form CSP flags, and never recurse from entry AST into the template resolver. In entry AST reject ContentSlot and TemplatePart as template-only elements; retained inactive plugin nodes follow existing safe fallback. Preview uses the path Page's authorized fields and unsaved AST, published-only list results, private preview media authorization and existing total node/output limits. Render limits are the existing Builder 512-node and 512 KiB output caps (the later Pattern expansion to 1024 nodes is outside D02), with the same descriptor/binding/list/form limits as normal Page Builder.

Media public ACL requires a public/published/active-provider Page, content_mode=builder and a saved entry ref. Draft entries and previews never grant public access. Deletion pins scan the saved JSON even if mode is editor or the provider is withdrawn; malformed persisted JSON fails closed under existing media lifecycle policy. Deleting an entry cascades its document/refs in the admitted entry writer. No saved AST is erased by mode switching or theme/plugin withdrawal. Backup/restore of this TARGET schema includes the document, mode and refs through the full DB snapshot.

### 4. Provider selections are not shared overrides

Create `owner_template_selections(id INTEGER PRIMARY KEY AUTOINCREMENT, owner_kind TEXT NOT NULL CHECK(owner_kind IN ('theme','plugin')), owner_id TEXT NOT NULL, role TEXT NOT NULL CHECK(role IN ('page','header','footer','archive','single','taxonomy_archive')), scope_type_key TEXT NOT NULL DEFAULT '', template_selection_key TEXT NULL, ast_json TEXT NOT NULL CHECK(json_valid(ast_json)), version INTEGER NOT NULL DEFAULT 1 CHECK(version>0), updated_at TEXT NOT NULL)`, unique `(owner_kind,owner_id,role,scope_type_key)` and index `(owner_kind,owner_id)`. Empty stored scope means None in the existing helper DTO. The role vocabulary here is selection metadata, not the typed site-template resolver vocabulary. Validate IDs with existing core validators/registry ownership. Upsert updates only the matching owner's selection, increments its version and never overwrites another owner or site override.

Retarget `storage/theme.rs::{OwnerTemplateSelection,upsert_owner_template_selection_in_conn,get_owner_template_selection_in_conn}` to this store. Keep the existing in-connection helper signatures; do not invent a public plugin template execution or selection API. Existing helpers have no production callers at the inspected baseline. This change preserves the existing owner-neutral domain storage boundary while the typed resolver reads only site overrides/active theme resources. Stored selection AST and keys remain unchanged during disable/restart/reinstall of the same provider; no foreign key to a transient provider registration may cascade them away. A missing provider does not execute its saved AST. Selection JSON participates in conservative media pins but never independently grants public media access.

Rebaseline the two M6 integration tests to this target table/helper. Keep assertions for owner separation, selection key and AST round trip, disabled plugin nodes, settings and content retention across disable/restart/reinstall and snapshot/restore of the target version. There is no assertion that an obsolete table or endpoint remains compatible.

### 5. Client transition and tests

Page-scoped Builder and SingleEditWorkspace use only the new entry Builder client. Shared-template Builder uses typed Admin template APIs; Section Builder keeps its existing private API. Remove old scoped Page endpoint calls instead of preserving aliases. Keep independent homepage APIs only until Theme S03, as already ACKed. Template shared-preview gaps remain subject to S02 acceptance; this delta must not let entry Page preview call shared Admin mutation routes.

Rebaseline Page integration/browser tests to entry version tokens and new paths, preserving A/B independence, Editor/Admin boundaries, CSRF, stale-write conflict, preview non-persistence, reset confirmation and local 401/412 draft retention. Add published-Page Save/reset coverage: public output and media ACL update immediately without changing entry status; draft Page Save/Preview remain private until explicit publication. Verify draft-mode-only edits are detected as dirty and included in composite payloads, 401/412 retains selected mode and unsaved AST, account/entry switching never restores another draft, mode switching retains AST and body, reset discards only the chosen entry document, and both publication endpoints respect current entry CAS. Do not replace failing Page tests with global-template tests or skip them.

## Alternatives Considered

- Keeping old Page override routes/table until CPT S01 would add a temporary second content model and violate Owner's target-only direction.
- Mapping each Page to the shared page template breaks Page isolation and Editor access.
- Shipping S02 with the Page Builder disabled would remove required target behavior and defer a known broken caller.
- Bringing forward all Generic CPT/Pattern/Reading work would enlarge this correction beyond the current blocker.

## Detailed Implementation Plan

### Slice D02: Complete Page-only target foundation inside Theme S02

**Dependencies:** Theme S01 merged; this amendment reviewed, approved and merged. Owner clarification is recorded above. Theme S02 and its Page foundation are one acceptance unit. Theme S03 still depends on complete S02. CPT S01 still depends on Theme S03 but extends the already-created store; it owns generic capability/UI and must retain existing Page Save/publication rules. CPT S02/0044 and CPT S03 retain their existing dependency order.

**File Ownership:** Harbor is the only implementation writer. This amendment adds the paths below to #427 and retains prior ACKed paths; no other writer may modify them concurrently. Aster owns documentation and the merged-source handoff.

| File | Operation | Change and reason |
| --- | --- | --- |
| `cms/migrations/0042_theme_v3_resolver_and_reading_settings.sql` | Modify (create on Harbor if absent) | Add final entry document/media/selection tables and mode column; no historical copies or compatibility APIs. |
| `cms/crates/opendash-core/src/content.rs` | Modify | Pure ContentSupport::Builder/ContentMode and a Page-specific constructor/default path; retain builtin_publication() for Post unchanged. |
| `cms/crates/opendash-server/src/storage/content.rs` | Modify | Update builtin_definition and default settings for Page specifically; it currently uses the same builtin_publication() constructor as Post. Fresh DB effective Page includes builder, Post does not; no global constructor flip. |
| `cms/crates/opendash-server/src/storage/builder.rs` | Modify | Entry read/save/preview/reset use cases, registry/field/media validation and entry CAS; all SQL stays in storage. Proposed public storage signatures: read_entry_builder(db, actor, type_key, id); save_entry_builder(db, actor, hooks, type_key, id, expected_version, document); preview_entry_builder(db, actor, type_key, id, document); reset_entry_builder(db, actor, hooks, type_key, id, expected_version, confirmed). DTOs follow the API table. |
| `cms/crates/opendash-server/src/storage/entries.rs` | Modify | Composite content_mode patch; existing status writer retains entry CAS/public predicates; entry deletion cleanup through FK cascade. |
| `cms/crates/opendash-server/src/routes/builder.rs` | Modify | Thin entry handlers; existing guards, admitted mutations, strict requests and If-Match. |
| `cms/crates/opendash-server/src/routes/content.rs` | Modify | Exercise and retain the registered dedicated publication endpoint; ensure Builder changes do not bypass its status/CAS/auth rules. |
| `cms/crates/opendash-server/src/routes/entries.rs` | Modify | Parse/return content_mode in existing composite entry contract; preserve validation and hooks. |
| `cms/crates/opendash-server/src/app.rs` | Modify | Register only the target entry family, remove retired Page override aliases, retain pre-S03 homepage route set. |
| `cms/crates/opendash-server/src/storage/{public_content,media_refs}.rs` | Modify | Publication-aware entry media ACL, conservative pins for saved entry/selection JSON and coherent public reads. |
| `cms/crates/opendash-server/src/storage/theme.rs` | Modify | Retarget owner selection helpers and validate owner identity; no template resolver precedence change. |
| `cms/crates/opendash-server/src/routes/public_content.rs` and `cms/crates/opendash-server/src/page.rs` | Modify | Render eligible entry Builder as body within approved template wrapper, with CSP and coherent storage snapshot; routes contain no SQL. |
| `cms/crates/opendash-server/src/backup_worker.rs` | Modify | Verify existing full-DB backup includes these target tables; add focused tests only if current table filtering excludes them. Do not add legacy restore conversion. |
| `cms/crates/opendash-server/src/content_render.rs` | Modify | Keep prior test-only entry literal ACK; hydrate/consume content_mode if required by the current entry DTO. |
| `cms/admin/src/features/content/types.ts` | Modify | Add ContentMode to the actual ContentEntryRecord/CompositeEntrySaveInput types imported by api/content.ts; keep absent patch distinct from null. |
| `cms/admin/src/features/editor/EditorDraftStore.ts` | Modify | Include content_mode in draft capture/restore, dirty comparison and payload generation; retain chosen mode and unsaved fields/AST across 401/412/account quarantine without changing another account or entry. |
| `cms/admin/src/api/{builder,content}.ts` | Modify | Typed target clients/DTOs via client.ts only, publication/reset CAS response handling. |
| `cms/admin/src/builder/BuilderWorkspace.tsx` | Modify | Separate entry/shared/Section targets; Page open/save/unsaved preview/reset and retained drafts. |
| `cms/admin/src/features/editor/SingleEditWorkspace.tsx` | Modify | Use entry CAS/version and existing Publish with current Page mode; no generic CPT configuration UI. |
| `cms/crates/opendash-server/tests/m4_s11_t01_page_scope.rs` | Modify | New entry API isolation/ACL/CAS/reset/preview and publication regression. |
| `cms/crates/opendash-server/tests/m6_s05_t01_domain_bridge.rs` | Modify | Target selection helper/table owner separation and inactive AST regression. |
| `cms/crates/opendash-server/tests/m6_s08_t01_plugin_e2e.rs` | Modify | Target-version selection/settings/content persistence, disable/restart/reinstall and matching-binary restore. |
| `cms/crates/opendash-server/tests/m7_s12_t01_navigation_and_schema_ui.rs` | Modify | Retired shared override reads become typed template reads with unchanged auth/navigation assertions. |
| `cms/scripts/page_builder_scope_e2e.py` and `cms/scripts/template_library_e2e.py` | Modify | Browser callers use target Page and typed shared-template APIs; target v3 fixtures and shared entry CAS. |
| `docs/contracts/M8_WRITER_INVENTORY.md` | Modify | List Page save/reset writers and Page document/ref mutation boundaries. |
| `docs/contracts/M6_PLUGIN_EXTENSIONS.md` | Modify | Describe current owner selection store, separate from shared templates; no old schema guarantee. |
| `docs/designs/theme-format-and-template-resolver.md` and `docs/designs/cpt-content-builder-and-pattern-library.md` | Modify | Link overriding D02; update S02 and later S01 creation/extension ownership. |
| `docs/designs/SITEMAP.md` | Modify | Register this amendment and keep implementation status truthful. |
| `docs/designs/s02-target-only-page-builder.md` | Create | This exact contract, review/provenance and handoff reference. |

**Acceptance Criteria:** Every behavior in sections 1–5 is covered by focused API/storage/browser tests. Target schema initializes on a fresh site, no old alias remains, different owners do not overwrite each other, Page edits never write shared templates or Page B. Published Page output/ACL update on Builder Save/reset; draft Page remains private until explicit publication. Required checks and full locked workspace release tests pass on the exact PR head; native browser acceptance is recorded. No live site operation is permitted.

**Rollback:** Before implementation revert this docs-only amendment. After implementation use a protected revert and a new disposable test root with the matching binary. There is no automatic downgrade migration, deletion of live site data or old-version compatibility promise. Retain target-version full backups for runtime backup/restore tests.

### Verification Plan

Run the repository-required standards/plans checks and their self-tests, check-agent-artifacts, cargo fmt --all --check, cargo build --locked, cargo test --workspace --release --locked and git diff --check. Run Admin npm run build and npm exec tsc -- --noEmit. Run the Page scope, M6 domain/plugin, relevant M5/M7/M8 backup/media suites and the two affected browser scripts using the freshly built binary and isolated site roots. Record commands, exact source/head, pass/fail and limits; CI governance Ubuntu/Windows and cms-ci must all pass before implementation merge. Do not label the whole feature Verified until merged-source validation and the required drift report are completed.

For the documentation amendment run the same mandated checks against the Aster checkout, record actual results, and reindex docs. Independent review uses design_critic, codebase_details and implementation_readiness in fresh contexts, explicit core context files and exact source snapshots. Two clean review rounds are required under egm-review-loop. No test or reviewer result is claimed until actually observed.


## D03 shared-template draft preview supplement

See [D03 typed shared-template draft preview](typed-template-draft-preview.md). This Approved supplement requires merge and exact-source handoff before implementation. It supplies the previously missing shared-template unsaved Preview API; entry Page Preview and existing publication semantics remain unchanged.
