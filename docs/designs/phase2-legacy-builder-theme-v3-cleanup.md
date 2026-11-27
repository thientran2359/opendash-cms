# Phase 2 Legacy Retirement: Studio Pattern Cutover, M3–M5 Route Removal, Theme v3 Enforcement & PRD Sync

* **Author:** Nova
* **Status:** Verified
* **Date:** 2026-11-25
* **Approved By:** Owner
* **Approval Evidence:** Owner direct instruction in Antigravity chat 2026-11-25
* **Goldfish Review:** Brainstorm 7-lens Goldfish synthesizer (dae24751) · 2026-11-25; egm-codebase-details post-implementation drift audit (5b88c4d4) · 2026-11-26 (0 Blocker / 0 Major after F-01–F-06 hardening)
* **Approval Gate Note:** Updates `docs/product/PRODUCT_REQUIREMENTS.md` and removes legacy HTTP/Admin endpoints, therefore final approval requires `Approved By: Owner` per [ADR-0010](../decisions/ADR-0010-agent-project-and-merge-authority.md).

--------------------------------------------------------------------------------

## Overview

Phase 2 completes OpenDash CMS's transition to **OpenDash Studio** and **Theme Format v3** by removing the remaining M1–M5 builder UI (`BuilderWorkspace`, `#/builder`), legacy HTTP builder/block endpoints (`/registry`, `/page`, `/preview`, `/save`), Theme v1/v2 parsing/compatibility paths (`parse_theme_zip_v2`, legacy `ThemeList` payloads), and runtime reads or media ACL checks touching inert legacy homepage/block SQLite tables (`home_page`, `builder_homepage`, `builder_homepage_media`, `builder_homepage_video_refs`, `theme_blocks`). It also migrates existing tests and fixtures (`admin-review/theme`) to Theme v3 and Studio routes, and synchronizes [`docs/product/PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md) §2–§5 with shipped V1 behavior.

## The Problem

1. **Duplicate Builder Shell & Redundant Registry Roundtrip:**
   - `StudioWorkspace.tsx` (`BuilderEditorCore`) already loads declarative block schemas via `GET /api/admin/builder/blocks`, yet still calls `getRegistry()` (`GET /registry` at `StudioWorkspace.tsx:410,511`) solely to pass `registry` for `legacy/block`, while `PatternsPage.tsx:165` still routes Pattern editing to the standalone `BuilderWorkspace` (`#/builder?template=entry&type=pattern&entry=<id>`) instead of Studio Mode 2, and `ThemesPanel.tsx:205` links to `#/builder`.
2. **Active Privacy Leak & Unreachable Orphan Media Locks:**
   - Even though `routes/public.rs` serves `/` via `site_reading_settings` rather than `builder_homepage`, `storage/public_content.rs:1281-1285,1320-1324` (`media_is_referenced`) still exposes `/media/{id}/file` publicly when `media_id` appears in `builder_homepage_media` or `builder_homepage_video_refs` alongside non-null `builder_homepage.document_json`. Meanwhile `storage/page.rs:554-586` (`resync_plugin_block_acls_in_tx`) keeps refreshing those legacy tables on theme/plugin changes, and `storage/media_refs.rs:113-167` (`collect_pins_in_tx`) returns `409` delete blocks for media pinned only in inert M3–M5 tables.
3. **Overloaded `HomeState` / `read_home_tx` & Legacy Fallback Templates:**
   - Active public CPT/Page rendering (`storage/public_content.rs:327`) and `storage/overrides.rs` still depend on `storage/builder.rs:367-434` (`read_home_tx` -> `HomeState`), which couples runtime context (`declared_blocks`, `site_options`, `timezone`) to queries against `home_page`, `builder_homepage`, and `theme_blocks`. Separately, `routes/public_content.rs:551,795`, `routes/content.rs:157`, and `routes/entries.rs:24` still call `storage::page::active_content_templates(&db)` reading legacy `theme_blocks`/`home_page` state.
4. **Theme v2 Test/DTO Carryover & Restore Verification Gap:**
   - Production theme install/activation enforces Theme Format v3 (`storage/theme.rs:905`), yet `src/theme.rs` still carries `#[cfg(test)]` `ThemeV2Manifest`/`parse_theme_zip_v2` parsers, `GET /api/admin/themes` (`ThemeList`) still queries `theme_blocks`/`home_page`/`builder_homepage` for legacy `items`, `active_id`, and `homepage_compatible` flags, and `storage/restore.rs:337` (`verify_restored_db_references`) checks `source_format == "package_zip_v2"` -> `themes/<id>/revisions/<digest>/.complete` instead of `"v3"` -> `themes/revisions/<digest>/.complete`.
5. **Stale Product Requirements Documentation:**
   - [`docs/product/PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md) §2–§5 still describes visual builder saves as always immediate, separates `Theme Options` and `Visual Builder` outside OpenDash Studio, marks `Pattern Library` as future work, and omits Spec 2/3 hooks, filters, projections, `SecretStore`, and the Dual Guest SDK.

## Goals

* Extract `BuilderEditorCore` into its own module, migrate `PatternsPage.tsx` and legacy `#/builder?...` Pattern links into Studio Mode 2 (`#/studio/entry/pattern/:id`) with Admin-only RBAC, disabled `core/pattern_ref` + `core/list` + `RouteContext`, `#/patterns/:id/edit` return navigation, and private-pattern save copy, then remove `BuilderWorkspace.tsx`, `ThemesPanel.tsx` `#/builder` links, and `getRegistry()`.
* Keep HTTP `GET /builder` -> `/admin/#/studio` redirect and `main.rs:239` startup URL intact (`D1`), while removing legacy routes `/registry`, `/page`, `/preview`, and `/save` from `app.rs` and `routes/builder.rs`.
* Split `HomeState`/`read_home_tx` into a clean `BuilderRenderContext` (`declared_blocks`, `site_options`, `timezone`), remove `active_content_templates`, `resync_plugin_block_acls_in_tx` legacy homepage writes, `builder_homepage*` ACL leaks in `public_content::media_is_referenced`, and inert table pin checks in `media_refs::collect_pins_in_tx`.
* Remove `items`, `active_id`, `homepage_compatible`, and `selection.legacy_block_id` from `GET /api/admin/themes` (`ThemeList` / `ThemesSummary`, `D2`), remove v1/v2 ZIP parsers/corpus from `src/theme.rs`, treat pre-`0042` `'v1_legacy'`/`'v2'` rows in `theme_packages`/`theme_revisions` safely as read-only non-activatable legacy records (preventing HTTP `500` on upgraded DBs), and fix `storage/restore.rs:337` `.complete` check for `"v3"`.
* Treat historical `legacy/block` (`KnownElement::Legacy`) AST nodes in saved documents as permanently locked read-only anchors (`D3`) without querying `/registry`, forbidding new `legacy/block` inserts or edits.
* Upgrade `cms/fixtures/admin-review/theme/theme.json` to `manifest_version: 3`, extract `theme_zip` helper out of `m5_beta_theme_v2_e2e.py`, migrate active Rust/E2E tests (`admin_i18n_s01_test.rs`, `m8_s01_t02_writer_barrier.rs`, `s03_media_acl_tests.rs`, `m6_s10_t01`, `m5_s12_t01`, `egm_pattern_library_s03`, `m7_s16_t02`, `operations_backup_restore_e2e.py`), and remove dead M1–M5 legacy-only tests.
* Synchronize `docs/product/PRODUCT_REQUIREMENTS.md` §2–§5, `docs/contracts/CORE_FOUNDATION.md`, `docs/contracts/M8_WRITER_INVENTORY.md`, and affected `README.md` contracts.

## Non-Goals

* Dropping historical SQLite tables (`home_page`, `builder_homepage`, `builder_homepage_media`, `builder_homepage_video_refs`, `theme_blocks`) or modifying embedded migrations `0001`–`0052` (must remain intact for `baseline_0001.sqlite` upgrades and M8 `restore.rs` schema verification).
* Breaking JSON deserialization of pre-existing documents containing `KnownElement::Legacy` AST nodes.
* Removing `/save`, `/preview`, or `/theme` from the SSRF egress path denylist in `plugin/egress.rs` and `plugin/host_ops.rs` (`PLUGIN_ABI.md` defense-in-depth).
* Renaming `/api/admin/builder/blocks` to `/api/admin/builder/block-schemas`.
* Removing `cms/fixtures/baseline_0001.sqlite` or `windows_auth_smoke.py --baseline-db` upgrade verification.

## Critical User Journeys (CUJs)

### CUJ 1: Admin Edits a Private Pattern in OpenDash Studio Mode 2
* **Given**: An authenticated `Admin` visits `#/patterns` (`PatternsPage.tsx`), or follows a bookmarked `#/builder?template=entry&type=pattern&entry=<id>` hash URL; an `Editor` is also tested against `#/studio/entry/pattern/<id>`.
* **When**: The Admin clicks "Edit Layout in Builder" (or opens `#/studio/entry/pattern/<id>`), edits blocks inside `BuilderEditorCore` using `/api/admin/builder/blocks` schemas, saves changes, and clicks Exit. Separately, an `Editor` navigates directly to `#/studio/entry/pattern/<id>`.
* **Then**: Legacy Pattern `#/builder?...` redirects to `#/studio/entry/pattern/<id>`; `StudioWorkspace` loads without calling `GET /registry`, blocks adding `core/pattern_ref`, `core/list`, or `RouteContext` bindings, displays private-pattern save wording (never "published"), returns to `#/patterns/<id>/edit` on Exit, and `isRouteAllowed` denies `Editor` access (`forbidden`) before mounting Studio.

### CUJ 2: Public Media ACL Isolation & Safe Orphan Deletion After M3–M5 Route Removal
* **Given**: An upgraded site database retains rows in `builder_homepage`, `builder_homepage_media`, and `home_page` referencing a media asset `M` that is no longer referenced by any active Page, Post, CPT, Template Override, Pattern, or Theme Option; active client requests hit `/registry`, `/page`, `/preview`, `/save`, and `/media/{M}/file`.
* **When**: An unauthenticated client requests `/media/{M}/file` or any legacy route (`/registry`, `/page`, `/preview`, `/save`), and an `Admin` deletes media `M` in Media Library.
* **Then**: `/registry`, `/page`, `/preview`, and `/save` return `404` (while `GET /builder` continues redirecting to `/admin/#/studio`); `/media/{M}/file` returns `404` because `public_content::media_is_referenced` no longer consults `builder_homepage*`; and `Admin` media deletion succeeds (`200`) rather than failing with `409` pins on inert tables.

### CUJ 3: Theme v3 Enforcement, Upgraded DB Listing & Backup Restore Verification
* **Given**: One site is upgraded from `baseline_0001.sqlite` (retaining historical `'v2'`/`'v1_legacy'` DB rows in `theme_packages`/`theme_revisions`), and another active Theme v3 site is backed up and restored via M8 operations.
* **When**: The Admin lists/installs themes via `GET /api/admin/themes` and `POST /api/admin/themes/install`, and runs M8 backup/restore (`restore_to_new_root` -> `verify_restored_db_references`).
* **Then**: Non-v3 ZIP uploads are rejected fail-closed (`400`/`422`); `GET /api/admin/themes` returns clean v3 `ThemeList` payloads without HTTP `500` or legacy `items`/`homepage_compatible`/`active_id` queries; historical non-v3 rows remain non-activatable legacy entries; and `restore.rs` verifies `themes/revisions/<digest>/.complete` for `source_format == "v3"` before opening the restored site.

### CUJ 4: Owner Reviews Synchronized PRD Against Shipped V1 Capabilities
* **Given**: `docs/product/PRODUCT_REQUIREMENTS.md` §2–§5 reflects OpenDash Studio navigation, `immediate` vs `draft_publish` in Studio Mode 2, shipped Pattern Library, Theme Format v3, and Spec 2/3 Plugin hooks, filters, projections, `SecretStore`, and Dual SDK.
* **When**: Owner reviews the Design Spec alongside PRD diff and Goldfish review summary.
* **Then**: Owner signs off (`Approved By: Owner`) per ADR-0010 before slices proceed.

--------------------------------------------------------------------------------

## Technical Plan (Architecture)

### 1. Target Architecture & Decoupling Diagram

```text
React Admin (OpenDash Studio Only)              Axum Routes & Storage                   SQLite Substrate (0001–0052 Intact)
──────────────────────────────────              ─────────────────────                   ───────────────────────────────────
#/patterns/:id/edit ──┐
#/builder?...pattern ─┼─> router.ts
                      │   (#/studio/entry/pattern/:id, Admin RBAC)
                      ▼
StudioWorkspace.tsx (Mode 1 & Mode 2)
  ├─ BuilderEditorCore.tsx ──> GET  /api/admin/builder/blocks  ──> page.rs (declared blocks) ──> builder_block_definitions
  ├─ Entry Builder API     ──> GET/PUT/POST/DEL /api/admin/entries/{type}/{id}/builder*      ──> entry_builder_documents
  ├─ Studio/Overrides API  ──> /api/admin/template-overrides* & /api/admin/studio/*          ──> template_overrides, site_*
  └─ ThemesPanel.tsx       ──> GET /api/admin/themes (ThemeList v3)                          ──> theme_revisions, theme_active_selection

Removed: BuilderWorkspace.tsx (#/builder), getRegistry(), GET /registry, GET /page, POST /preview, POST /save
Inert (no runtime reads/writes/ACLs): home_page, builder_homepage, builder_homepage_media, builder_homepage_video_refs, theme_blocks
Fixed M8 Restore Gate: restore.rs:337 verifies themes/revisions/<digest>/.complete when source_format == "v3"
```

### 2. Frontend Consolidation (`cms/admin/src/`)

1. **Extract `BuilderEditorCore.tsx` & Delete `BuilderWorkspace.tsx`:**
   - Move `BuilderEditorCore`, `BuilderEditorCoreProps`, and `StudioBuilderView` into `cms/admin/src/builder/BuilderEditorCore.tsx`.
   - Remove `registry: BlockRegistry` from `BuilderEditorCoreProps`, `StudioCustomizerNavProps`, `StudioWorkspace.tsx`, `BuilderInspector.tsx`, and `document.ts` helper signatures that only checked `Object.hasOwn(registry, blockId)`. In `document.ts:isLockedElement`, mark `'legacy/block'` unconditionally `true` (`D3`) so pre-existing legacy block nodes remain immutable read-only anchors without issuing `GET /registry`.
   - Update imports in `StudioWorkspace.tsx` and `StudioCustomizerNav.tsx`, remove `sidebarContent` / `builderDirty` / `lastBuilderRouteRef` from `App.tsx` and `AdminShell.tsx`, remove `getRegistry` from `api/builder.ts` and `api/index.ts`, and delete `BuilderWorkspace.tsx`.
2. **Pattern Mode 2 Support in Studio (`#/studio/entry/pattern/:id`):**
   - **`router.ts`:** Remove `rawTypeKey !== 'pattern'` in `parseAdminRoute` (`studio-entry` parser); replace the `#/builder` Pattern `view: 'builder'` arm with a canonical `redirectHash: '#/studio/entry/pattern/${entryId}'`; enforce explicit Admin-only RBAC in `isRouteAllowed` (`if (route.view === 'studio-entry' && route.typeKey === 'pattern' && role !== 'admin') return false;` placed before `return true` for other `studio-entry` routes); remove `view: 'builder'` from `AdminView` (keep `/builder` pathname redirect -> `#/studio`).
   - **`PatternsPage.tsx` & `ThemesPanel.tsx`:** Route Pattern edit buttons to `#/studio/entry/pattern/${current.id}`; update `ThemesPanel.tsx` `<a href="#/studio">` and remove `items`, `active_id`, `homepage_compatible` rendering (`D2`).
   - **`StudioWorkspace.tsx` & `StudioSidebarFooter.tsx`:** Introduce `isPatternEntry = isMode2 && entryTypeKey === 'pattern'`:
     - Pass `allowPatternRef={!isPatternEntry}`, `allowList={!isPatternEntry}`, and `allowRouteContext={false}` to `BuilderEditorCore` (enforcing `0044` nesting/list limits client-side).
     - Update `handleExit()` to navigate to `#/patterns/${entryId}/edit` when `isPatternEntry` is `true`.
     - Extend `StudioSavePolicy` (`isPrivatePattern?: boolean`) and add localized i18n keys (`studio.save_pattern`, `studio.saved_pattern_v_version`) so Pattern saves never claim `"Saved and published"`.

### 3. Backend Route, Storage, Theme v3 & Restore Cutover (`cms/crates/`)

1. **HTTP Route & App Wiring (`app.rs`, `routes/builder.rs`, `routes/public_content.rs`, `routes/entries.rs`, `routes/content.rs`, `i18n.rs`):**
   - Remove `.route("/registry", ...)`, `.route("/page", ...)`, `.route("/preview", ...)`, and `.route("/save", ...)` from `app.rs` and delete `registry`, `page`, `preview`, `save`, and `builder_rendered` handlers from `routes/builder.rs` (keeping HTTP `GET /builder` & `/builder/` -> `/admin/#/studio` redirect per `D1`).
   - Remove `storage::page::active_content_templates(&db)` calls in `routes/public_content.rs`, `routes/entries.rs`, and `routes/content.rs` (passing `None` for fallback legacy template).
   - Sync `i18n.rs` error-localization path tables where obsolete `/registry`, `/page`, `/preview`, `/save` paths were enumerated.
2. **Decouple `HomeState` / `read_home_tx` & Sever Legacy Tables (`storage/{builder,page,public_content,media_refs,overrides,plugin}.rs`):**
   - Split `HomeState` / `read_home_tx` in `storage/builder.rs` into `BuilderRenderContext` / `read_render_context_tx` (fetching strictly `declared_blocks`, `site_options`, `timezone`) and migrate active callers in `storage/public_content.rs` and `storage/overrides.rs`.
   - Delete dead legacy functions (`legacy_page`, `save_page`, `save_and_render`, `render_block`, `load_home_page`, `insert_theme_block`, `active_content_templates`, `resolve_effective_override`).
   - Stop `storage/page.rs:load_unified_block_catalog_in_conn`, `sync_owner_blocks_in_tx`, `storage/plugin.rs`, and `resync_plugin_block_acls_in_tx` from reading/checking `theme_blocks` or scanning `builder_homepage` to rewrite `builder_homepage_media` / `builder_homepage_video_refs`.
   - Remove `builder_homepage_media` and `builder_homepage_video_refs` clauses from `storage/public_content.rs:media_is_referenced` (closing the public media privacy leak), and remove inert `home_page`, `theme_blocks`, `builder_homepage*` scans from `storage/media_refs.rs:collect_pins_in_tx`.
3. **Theme v3 Consolidation & Restore Durability Fix (`theme.rs`, `storage/{theme,import,restore}.rs`, `opendash-core`):**
   - Remove legacy `items`, `active_id`, `homepage_compatible`, and `selection.legacy_block_id` fields/queries from `storage/theme.rs:list` and `ThemeList` (`D2`), while keeping `ThemeSourceFormat::{V1Legacy, V2}` tolerant in `from_db` so upgraded pre-`0042` DB rows classify safely as read-only non-activatable legacy records rather than throwing HTTP `500`.
   - Delete dead `#[cfg(test)]` `ThemeV2Manifest`/`parse_theme_zip_v2` code in `src/theme.rs` and rename internal path helper `import.rs:validate_v2_relative_path` -> `validate_theme_relative_path`.
   - Keep `KnownElement::Legacy` deserialization in `opendash-core/src/builder.rs` (`D3`) so pre-existing saved documents open safely while blocking edits/inserts.
   - Fix `storage/restore.rs:337` (`verify_restored_db_references`) to validate `themes/revisions/<digest>/.complete` when `tr.source_format == "v3"`.

### 4. Documentation, Contracts, Fixtures & Test Alignment

- **PRD & Contracts (`docs/`):** Sync [`docs/product/PRODUCT_REQUIREMENTS.md`](../product/PRODUCT_REQUIREMENTS.md) §2–§5 (`Studio` workflows, `immediate` vs `draft_publish`, Pattern Library, Theme v3, Plugin `manifest_schema: 3` & Dual SDK); update [`docs/contracts/CORE_FOUNDATION.md`](../contracts/CORE_FOUNDATION.md), [`docs/contracts/M8_WRITER_INVENTORY.md`](../contracts/M8_WRITER_INVENTORY.md) (remove `routes::builder::save`), and affected `cms/**/README.md` contracts.
- **Fixtures, Rust Tests & Scripts (`cms/`):**
  - Extract shared `theme_zip` builder into `cms/scripts/menu_picker_e2e.py` before removing `m5_beta_theme_v2_e2e.py` (`admin_navigation_e2e.py` continues passing).
  - Upgrade `cms/fixtures/admin-review/theme/theme.json` to `manifest_version: 3` (`admin_extensions_e2e.py`).
  - Update `m8_s01_t02_writer_barrier.rs`, `admin_i18n_s01_test.rs`, `storage/s03_media_acl_tests.rs`, `pattern_library_studio_e2e.py`, `template_library_e2e.py`, `admin_navigation_e2e.py`, `operations_backup_restore_e2e.py` to Studio Mode 2 / `/api/admin/builder/blocks` / Theme v3; prune dead M1–M5 legacy-only tests (`public_content_smoke.py`, `m5_beta_theme_v2_e2e.py`, `m4_s11_t02_sections_e2e.py` after extracting `create_ui_section`).

--------------------------------------------------------------------------------

## Alternatives Considered

### Alternative A: Drop Historical SQLite Tables via New Migration `0053` (`DROP TABLE`)
* **Pros**: Completely removes `home_page`, `builder_homepage`, `builder_homepage_media`, `builder_homepage_video_refs`, and `theme_blocks` from future database schemas.
* **Cons**: Breaks DDL schema fingerprint verification in `storage/restore.rs` for pre-cutover encrypted backups unless complex schema-mapping exceptions are added, forces re-baselining `cms/fixtures/m8/schema_fingerprints.json` and upgrade coverage, and risks permanently discarding raw historical homepage blobs before site owners migrate manually.
* **Reason for Rejection**: Rejected by author: Keeping SQLite migrations `0001`–`0052` unchanged and severing all active runtime queries, fallback templates, and media ACL checks over those tables eliminates runtime risk with zero restore breakage.

### Alternative B: Hard-Delete `ThemeSourceFormat::{V1Legacy, V2}` DB Parsing (`from_db` Arms)
* **Pros**: Removes `"v1_legacy"` and `"v2"` enum variants completely from `opendash-server/src/storage/theme.rs`.
* **Cons**: As demonstrated in `storage/theme_tests.rs:127-180`, upgraded databases originally seeded on pre-`0042` versions still contain `"v1_legacy"`/`"v2"` strings in `theme_packages` and `theme_revisions`; deleting `from_db` arms without a mandatory data migration causes `GET /api/admin/themes` (`list`) to return HTTP `500` (`"Định dạng theme không hợp lệ trong DB."`).
* **Reason for Rejection**: Rejected by author: Removing all v1/v2 ZIP installation, compatibility checks (`homepage_compatible`, `items`), and runtime parsing while keeping `from_db` tolerant (treating pre-`0042` rows as read-only non-activatable legacy records) prevents upgraded sites from crashing without needing migration churn.

### Alternative C: Delete HTTP `GET /builder` & Change `main.rs:239` Startup URL to `/admin/`
* **Pros**: Eliminates even the lightweight `/builder` redirect route in `app.rs` and prints `/admin/` on CLI startup.
* **Cons**: `cms/scripts/windows_smoke.py` parses stdout via `removesuffix("/builder")` (shared by `windows_auth_smoke.py` upgrade tests), and saved operator bookmarks hitting `/builder` directly would immediately 404.
* **Reason for Rejection**: Rejected by author (Decision `D1`): Retaining the 1-line HTTP redirect `/builder` -> `/admin/#/studio` and leaving `main.rs:239` untouched preserves compatibility while still deleting the entire `#/builder` React workspace (`BuilderWorkspace.tsx`).

### Alternative D: Keep `GET /api/admin/themes` Legacy DTO Stubs & Split PRD Update Out
* **Pros**: Returning empty `items: []`, `active_id: null`, `homepage_compatible: null` in `ThemeList` would avoid touching frontend DTO fields immediately.
* **Cons**: Leaves dead fields in `api/themes.ts` (`ThemesSummary`) and `ThemesPanel.tsx` plus confusing compatibility concepts for agents reading the contract; similarly, delaying `PRODUCT_REQUIREMENTS.md` sync leaves PRD contradicting the actual Studio/Plugin architecture.
* **Reason for Rejection**: Rejected by author (Decision `D2`): Removing the unused DTO fields cleanly across backend, frontend, and active tests alongside synchronizing `PRODUCT_REQUIREMENTS.md` §2–§5 in Slice `S01` (under explicit `Approved By: Owner`) eliminates ongoing doc-code drift in one coordinated spec.

--------------------------------------------------------------------------------

## Non-Functional Requirements

* **Security & Privacy**:
  - Closing `builder_homepage_media` and `builder_homepage_video_refs` in `storage/public_content.rs:media_is_referenced` prevents unauthenticated `/media/{id}/file` downloads of assets no longer present on any public route.
  - Mode 2 Pattern routes (`#/studio/entry/pattern/:id`) must block `Editor` accounts in `router.ts:isRouteAllowed` before mounting `StudioWorkspace`, matching server-side Admin guards in `routes/patterns.rs` and `routes/builder.rs`.
  - Keep `/save`, `/preview`, and `/theme` in SSRF URL denylists inside `plugin/egress.rs` and `plugin/host_ops.rs` (`PLUGIN_ABI.md`).
* **Data Preservation & Upgrade Safety**:
  - Upgraded `baseline_0001.sqlite` databases and pre-cutover `.odbackup.age` archives must open, list themes, and restore without HTTP `500` errors; `KnownElement::Legacy` AST nodes remain deserializable and locked (`D3`); `storage/restore.rs:337` verifies `themes/revisions/<digest>/.complete` for `"v3"` themes.
* **Performance & Token Footprint**:
  - Eliminating `GET /registry` from `StudioWorkspace.tsx` removes an extra HTTP roundtrip on every Studio load; retiring `BuilderWorkspace.tsx`, legacy M3–M5 handlers, and `HomeState` legacy queries reduces bundle size and agent context noise.
* **Rollout & Rollback**:
  - Changes are partitioned into 4 independent Slices (`S01` Docs/PRD, `S02` Frontend, `S03` Backend/Restore, `S04` Fixtures/Tests) with explicit per-slice file ownership and `git revert` rollback paths; no destructive `DROP TABLE` migration is performed, making backend rollback safe.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

Implementation is organized into **4 sequential/partially-parallel slices** (`S01` → `S02` + `S03` → `S04`) with disjoint file ownership so execution proceeds cleanly without merge conflicts.

### Slice S01: Sync PRD (§2–§5) & Foundation Contracts

* **Dependencies:** None (approved via `Approved By: Owner`).
* **File Ownership:**
  - `docs/product/PRODUCT_REQUIREMENTS.md`
  - `docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md`
  - `docs/contracts/CORE_FOUNDATION.md`
  - `docs/contracts/M8_WRITER_INVENTORY.md`
  - `docs/architecture/CMS_ARCHITECTURE.md`
* **Acceptance Criteria:**
  - [ ] `docs/product/PRODUCT_REQUIREMENTS.md` §2–§5 matches shipped V1 behavior: Visual Builder edits template overrides, CPT builders, and private Patterns inside OpenDash Studio respecting `immediate` vs `draft_publish`; §3 reflects unified OpenDash Studio (`Theme Options`, `Menus`, `Reading`, `Visual Builder`) under `Giao diện`; §4 marks `Pattern Library` and Theme v3 as shipped; §5 lists `filter:*` hooks, write-time projections, `SecretStore`, and Dual Guest SDK (Rust + AssemblyScript).
  - [ ] `docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md` reflects `#/studio/entry/pattern/:id` and retired `#/builder` (`BuilderWorkspace`).
  - [ ] `docs/contracts/CORE_FOUNDATION.md` and `docs/contracts/M8_WRITER_INVENTORY.md` remove `routes::builder::save`, `/registry`, `/page`, `/preview`, `/save`, and runtime legacy `home_page`/`builder_homepage`/`theme_blocks` writers while preserving inert table schema notes.
  - [ ] `docs/architecture/CMS_ARCHITECTURE.md` removes `BuilderWorkspace.tsx` and adds `BuilderEditorCore.tsx` + `StudioWorkspace.tsx`.
  - [ ] `python cms/scripts/check-plans.py`, `python cms/scripts/check-standards.py`, and `git diff --check` pass.
* **Rollback:** `git revert <S01_merge_sha>`.

### Slice S02: Frontend Studio Cutover (`BuilderEditorCore` & Pattern Mode 2)

* **Dependencies:** S01
* **File Ownership:**
  - `cms/admin/src/builder/BuilderEditorCore.tsx` (Create)
  - `cms/admin/src/builder/BuilderWorkspace.tsx` (Delete)
  - `cms/admin/src/builder/BuilderCanvas.tsx`
  - `cms/admin/src/builder/BuilderInspector.tsx`
  - `cms/admin/src/builder/BuilderTree.tsx`
  - `cms/admin/src/builder/document.ts`
  - `cms/admin/src/builder/README.md`
  - `cms/admin/src/studio/StudioWorkspace.tsx`
  - `cms/admin/src/studio/StudioCustomizerNav.tsx`
  - `cms/admin/src/studio/StudioSidebarFooter.tsx`
  - `cms/admin/src/studio/README.md`
  - `cms/admin/src/shell/router.ts`
  - `cms/admin/src/shell/AdminShell.tsx`
  - `cms/admin/src/shell/README.md`
  - `cms/admin/src/App.tsx`
  - `cms/admin/src/features/patterns/PatternsPage.tsx`
  - `cms/admin/src/features/themes/ThemesPanel.tsx`
  - `cms/admin/src/api/builder.ts`
  - `cms/admin/src/api/themes.ts`
  - `cms/admin/src/api/index.ts`
  - `cms/admin/src/api/README.md`
  - `cms/admin/src/i18n/locales/en.ts`
  - `cms/admin/src/i18n/locales/vi.ts`
  - `cms/admin/src/types.ts`
* **Acceptance Criteria:**
  - [ ] `BuilderEditorCore`, `BuilderEditorCoreProps`, and `StudioBuilderView` live in `cms/admin/src/builder/BuilderEditorCore.tsx` without any `registry: BlockRegistry` prop; `BuilderWorkspace.tsx` is deleted.
  - [ ] `StudioWorkspace.tsx` and `StudioCustomizerNav.tsx` remove all `getRegistry()` (`GET /registry`) calls and rely exclusively on `GET /api/admin/builder/blocks`; `api/builder.ts`, `api/index.ts`, and `types.ts` remove `getRegistry` and `BlockRegistry` (removing `registry` parameters from `BuilderCanvas`, `BuilderInspector`, `BuilderTree`, and `document.ts` while marking `'legacy/block'` unconditionally `isLockedElement = true`).
  - [ ] `router.ts` parses `#/studio/entry/pattern/:id` cleanly, redirects `#/builder?template=entry&type=pattern&entry=<id>` to `#/studio/entry/pattern/<id>`, deletes `AdminView` `'builder'`, and enforces `if (route.view === 'studio-entry' && route.typeKey === 'pattern' && role !== 'admin') return false;` in `isRouteAllowed` before returning `true` for editor CPT builders.
  - [ ] `App.tsx` and `AdminShell.tsx` drop `sidebarContent`, `builderDirty`, `lastBuilderRouteRef`, and `builderIdentity`; `PatternsPage.tsx` links to `#/studio/entry/pattern/${current.id}`; `ThemesPanel.tsx` links to `#/studio` and drops `items`, `active_id`, `homepage_compatible` UI/types (`api/themes.ts` `ThemesSummary` updated per `D2`).
  - [ ] In Mode 2 for `entryTypeKey === 'pattern'`, `StudioWorkspace.tsx` sets `allowPatternRef={false}`, `allowList={false}`, `allowRouteContext={false}`, navigates Exit to `#/patterns/${entryId}/edit`, and `StudioSidebarFooter.tsx` + `StudioWorkspace.tsx` show private Pattern save labels/status (`studio.save_pattern`, `studio.saved_pattern_v_version` symmetric in `en.ts`/`vi.ts`, no "published" wording).
  - [ ] `npm --prefix cms/admin run typecheck`, `npm --prefix cms/admin run build`, `node cms/scripts/check-i18n.mjs --strict`, and `python cms/scripts/check-standards.py` pass.
* **Rollback:** `git revert <S02_merge_sha>`.

### Slice S03: Backend M3–M5 Cutover, `BuilderRenderContext` Split, Media ACL & Restore v3 Fix

* **Dependencies:** S01 (coordinate merge with S02 so Admin no longer calls `/registry` or reads legacy `ThemeList` fields).
* **File Ownership:**
  - `cms/crates/opendash-core/src/builder.rs`
  - `cms/crates/opendash-server/src/app.rs`
  - `cms/crates/opendash-server/src/i18n.rs`
  - `cms/crates/opendash-server/src/page.rs`
  - `cms/crates/opendash-server/src/theme.rs`
  - `cms/crates/opendash-server/src/routes/builder.rs`
  - `cms/crates/opendash-server/src/routes/content.rs`
  - `cms/crates/opendash-server/src/routes/entries.rs`
  - `cms/crates/opendash-server/src/routes/public_content.rs`
  - `cms/crates/opendash-server/src/routes/themes.rs`
  - `cms/crates/opendash-server/src/routes/README.md`
  - `cms/crates/opendash-server/src/storage/builder.rs`
  - `cms/crates/opendash-server/src/storage/import.rs`
  - `cms/crates/opendash-server/src/storage/media_refs.rs`
  - `cms/crates/opendash-server/src/storage/overrides.rs`
  - `cms/crates/opendash-server/src/storage/page.rs`
  - `cms/crates/opendash-server/src/storage/plugin.rs`
  - `cms/crates/opendash-server/src/storage/public_content.rs`
  - `cms/crates/opendash-server/src/storage/restore.rs`
  - `cms/crates/opendash-server/src/storage/theme.rs`
  - `cms/crates/opendash-server/src/storage/theme_tests.rs`
  - `cms/crates/opendash-server/src/storage/s03_media_acl_tests.rs`
  - `cms/crates/opendash-server/src/storage/README.md`
  - `cms/crates/opendash-server/src/package.rs`
  - `cms/crates/opendash-server/src/plugin/package.rs`
* **Acceptance Criteria:**
  - [ ] `app.rs` removes `.route("/registry", ...)`, `.route("/page", ...)`, `.route("/preview", ...)`, and `.route("/save", ...)`, while retaining `.route("/builder", get(builder))` and `.route("/builder/", get(builder))` (`D1`). `routes/builder.rs` deletes `registry`, `page`, `preview`, `save`, and `builder_rendered` handlers. `i18n.rs` updates error-path lists without `/registry`, `/page`, `/preview`, `/save`.
  - [ ] `storage/builder.rs` introduces `BuilderRenderContext` (`declared_blocks`, `site_options`, `timezone`) and `read_render_context_tx`, deletes `HomeState`, `read_home_tx`, `read`, `read_public_resolved`, `resolve_preview`, `legacy_page`, and `save_and_render`, and updates `storage/public_content.rs`, `storage/overrides.rs`, and `page.rs` to consume `BuilderRenderContext`.
  - [ ] `routes/public_content.rs`, `routes/entries.rs`, and `routes/content.rs` remove `active_content_templates(&db)` calls; `storage/page.rs` deletes `load_home_page`, `insert_theme_block`, `active_content_templates`, `render_legacy_state`, `render_block`, `save_page`, removes `theme_blocks` reads/checks in `load_unified_block_catalog_in_conn`, `sync_owner_blocks_in_tx`, & `storage/plugin.rs`, and stops `resync_plugin_block_acls_in_tx` from parsing `builder_homepage` or mutating `builder_homepage_media`/`builder_homepage_video_refs`.
  - [ ] `storage/public_content.rs:media_is_referenced` removes `builder_homepage_media` and `builder_homepage_video_refs` checks; `storage/media_refs.rs:collect_pins_in_tx` removes `home_page`, `theme_blocks`, `builder_homepage`, `builder_homepage_media`, and `builder_homepage_video_refs` pin queries (and `s03_media_acl_tests.rs` proves unreferenced legacy `builder_homepage` media returns `404` publicly and deletes cleanly without `409`).
  - [ ] `storage/theme.rs` removes `ThemeItem`, `active_id`, `homepage_compatible`, and `selection.legacy_block_id` (`ThemeList { version, revisions, selection }`), removes `home_page`/`builder_homepage`/`theme_blocks` queries in `list`, keeps `ThemeSourceFormat::from_db` tolerant for historical `'v1_legacy'`/`'v2'` DB rows (non-activatable, returning clean summary without HTTP `500`), and `src/theme.rs` removes `#[cfg(test)]` `ThemeV2Manifest`/`ThemeV2Package`/`parse_theme_zip_v2` plus renames `package::validate_v2_relative_path` -> `validate_theme_relative_path`.
  - [ ] `storage/restore.rs:337` (`verify_restored_db_references`) checks `if tr.source_format == "v3"` -> `site_root.join("themes").join("revisions").join(&tr.revision_digest).join(".complete")` and fails closed when missing.
  - [ ] `plugin/egress.rs` and `plugin/host_ops.rs` keep `/save` and `/preview` in SSRF URL denylists (`Non-Goals`); `opendash-core/src/builder.rs` preserves `KnownElement::Legacy` deserialization as immutable anchor (`D3`).
  - [ ] `cargo fmt --manifest-path cms/Cargo.toml --all --check`, `cargo build --manifest-path cms/Cargo.toml --locked`, and unit tests inside `opendash-core` and `opendash-server --lib` pass.
* **Rollback:** `git revert <S03_merge_sha>`.

### Slice S04: Fixture v3 Upgrade, Integration Test & E2E Script Alignment

* **Dependencies:** S02, S03
* **File Ownership:**
  - `cms/fixtures/admin-review/theme/theme.json`
  - `cms/crates/opendash-server/tests/admin_i18n_s01_test.rs`
  - `cms/crates/opendash-server/tests/m5_s11_t01_revision_discovery.rs`
  - `cms/crates/opendash-server/tests/m8_s01_t02_writer_barrier.rs`
  - `cms/crates/opendash-server/tests/m8_s08_t01_e2e.rs`
  - `cms/scripts/menu_picker_e2e.py`
  - `cms/scripts/admin_i18n_multilingual_e2e.py`
  - `cms/scripts/cpt_builder_e2e.py`
  - `cms/scripts/pattern_library_studio_e2e.py`
  - `cms/scripts/page_builder_scope_e2e.py`
  - `cms/scripts/m4_s11_t02_sections_e2e.py` (Delete after moving `create_ui_section` & Pattern/Admin checks)
  - `cms/scripts/m5_beta_theme_v2_e2e.py` (Delete after moving `theme_zip`)
  - `cms/scripts/template_library_e2e.py`
  - `cms/scripts/admin_extensions_e2e.py`
  - `cms/scripts/admin_navigation_e2e.py`
  - `cms/scripts/operations_backup_restore_e2e.py`
  - `cms/scripts/public_content_smoke.py` (Delete dead M3–M5 legacy `/save`/`/page` script)
  - `cms/scripts/windows_auth_smoke.py`
  - `cms/scripts/windows_smoke.py`
  - `cms/scripts/README.md`
  - `cms/fixtures/README.md`
  - `cms/fixtures/demo-website/README.md`
  - `cms/crates/opendash-server/README.md`
* **Acceptance Criteria:**
  - [ ] `cms/fixtures/admin-review/theme/theme.json` is valid Theme Format v3 (`manifest_version: 3`, `single.html` declared) and passes `--check-theme` and `admin_extensions_e2e.py`.
  - [ ] `menu_picker_e2e.py` exports a clean v3 `theme_zip` helper; `m5_beta_theme_v2_e2e.py`, `m4_s11_t02_sections_e2e.py`, and `public_content_smoke.py` are removed and `cms/scripts/README.md` + `cms/fixtures/demo-website/README.md` updated accordingly.
  - [ ] `m8_s01_t02_writer_barrier.rs` drops `"save"` from `"builder.rs"`; `admin_i18n_s01_test.rs` & `admin_i18n_multilingual_e2e.py` replace `/registry` & `/page` probes with active protected Admin endpoints; `m5_s11_t01_revision_discovery.rs` asserts the clean v3 `ThemeList` shape without `active_id`/`items`/`homepage_compatible`; `m8_s08_t01_e2e.rs` verifies `restore.rs` fails closed when a v3 revision's `.complete` marker is removed.
  - [ ] `cpt_builder_e2e.py`, `pattern_library_studio_e2e.py`, `page_builder_scope_e2e.py`, `template_library_e2e.py`, `admin_navigation_e2e.py`, `operations_backup_restore_e2e.py`, and `windows_auth_smoke.py` run cleanly against Studio Mode 2 (`#/studio/entry/...`, `studio-workspace`), Theme v3, and RBAC guards (`Editor` denied on `#/studio/entry/pattern/:id`).
  - [ ] All checks in Verification Plan pass (`0` failures).
* **Rollback:** `git revert <S04_merge_sha>`.

### Target Files to Create/Modify

1. **`docs/product/PRODUCT_REQUIREMENTS.md` (`S01`, Modify):** Synchronize §2–§5 (`Studio` workflows, `immediate` vs `draft_publish`, Pattern Library shipped, Theme v3, Spec 2/3 Plugin hooks, projections, `SecretStore`, Dual SDK) under `Approved By: Owner`.
2. **`docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md` (`S01`, Modify):** Document `#/studio/entry/pattern/:id` and mark legacy `#/builder` retired (only HTTP `/builder` redirect remains).
3. **`docs/contracts/CORE_FOUNDATION.md`, `docs/contracts/M8_WRITER_INVENTORY.md`, `docs/architecture/CMS_ARCHITECTURE.md` (`S01`, Modify):** Remove `/registry`, `/page`, `/preview`, `/save`, `routes::builder::save`, and `BuilderWorkspace.tsx`; document `BuilderEditorCore.tsx` and inert SQLite legacy tables.
4. **`cms/admin/src/builder/BuilderEditorCore.tsx` (`S02`, Create) & `cms/admin/src/builder/BuilderWorkspace.tsx` (`S02`, Delete):** Extract `BuilderEditorCore(props)` minus `registry`, removing standalone `BuilderWorkspace`.
5. **`cms/admin/src/builder/{BuilderCanvas.tsx,BuilderInspector.tsx,BuilderTree.tsx,document.ts}` & `cms/admin/src/api/{builder,themes,index}.ts` & `cms/admin/src/types.ts` (`S02`, Modify):** Remove `BlockRegistry` / `getRegistry()`, update `ThemesSummary`, mark `'legacy/block'` locked in `isLockedElement(element, blockCatalog)`.
6. **`cms/admin/src/studio/{StudioWorkspace,StudioCustomizerNav,StudioSidebarFooter}.tsx`, `cms/admin/src/shell/{router,AdminShell}.tsx`, `cms/admin/src/App.tsx`, `cms/admin/src/features/{patterns/PatternsPage,themes/ThemesPanel}.tsx`, `cms/admin/src/i18n/locales/{en,vi}.ts` (`S02`, Modify):** Enable Admin-only Mode 2 Pattern editing (`#/studio/entry/pattern/:id`), redirect legacy `#/builder` Pattern links, disable `PatternRef`/`List`/`RouteContext` for `pattern`, exit to `#/patterns/:id/edit`, add private Pattern save copy, point `ThemesPanel` to `#/studio`, remove `sidebarContent`/`builderDirty`/`lastBuilderRouteRef`.
7. **`cms/crates/opendash-core/src/builder.rs` (`S03`, Modify):** Keep `KnownElement::Legacy` deserialization intact (`is_preserved`) while keeping legacy creation disabled.
8. **`cms/crates/opendash-server/src/{app,i18n,page,theme,package}.rs` & `plugin/package.rs` (`S03`, Modify):** Unwire `/registry`, `/page`, `/preview`, `/save`, delete v2 ZIP parser (`#[cfg(test)]`) & `render_block`/`save_page`/`render_legacy_state`, rename `validate_theme_relative_path`, keep `GET /builder` HTTP redirect (`D1`).
9. **`cms/crates/opendash-server/src/routes/{builder,content,entries,public_content,themes}.rs` (`S03`, Modify):** Remove `registry`/`page`/`preview`/`save`, drop `active_content_templates(&db)` calls, align `themes.rs` with updated `ThemeList`.
10. **`cms/crates/opendash-server/src/storage/{builder,page,plugin,public_content,media_refs,overrides,theme,import,restore}.rs` (`S03`, Modify):** Replace `HomeState`/`read_home_tx` with `BuilderRenderContext`/`read_render_context_tx`; delete legacy readers/syncs; remove `builder_homepage*` ACL leaks in `media_is_referenced` and dead pins in `collect_pins_in_tx`; clean `ThemeList` & keep tolerant historical `ThemeSourceFormat::from_db`; fix `restore.rs:337` `"v3"` `.complete` verification.
11. **`cms/crates/opendash-server/src/storage/{theme_tests,s03_media_acl_tests}.rs` & `tests/{admin_i18n_s01_test,m5_s11_t01_revision_discovery,m8_s01_t02_writer_barrier,m8_s08_t01_e2e}.rs` (`S03`/`S04`, Modify):** Update assertions for Theme v3 DTOs, ACL isolation, writer inventory, and restore v3 `.complete` test coverage.
12. **`cms/fixtures/admin-review/theme/theme.json` & `cms/scripts/*.py` & affected `cms/**/README.md` (`S04`, Modify/Delete):** Convert `admin-review/theme` to v3; extract `theme_zip` into `menu_picker_e2e.py`; update active Python smoke/Playwright runners to `#/studio`; delete `m5_beta_theme_v2_e2e.py`, `public_content_smoke.py`, and stale `m4_s11_t02_sections_e2e.py` (preserving unique Pattern coverage); update READMEs.

### Verification Plan

* **Compilation/Build**:
  - `npm --prefix cms/admin run typecheck`
  - `npm --prefix cms/admin run build`
  - `cargo build --manifest-path cms/Cargo.toml --locked`
* **Unit & Integration Tests**:
  - `cargo test --manifest-path cms/Cargo.toml --workspace --release --locked`
  - Explicit assertions in `storage/s03_media_acl_tests.rs` (unreferenced `builder_homepage` media returns public `404` & deletes cleanly), `storage/theme_tests.rs` (upgraded pre-`0042` DB rows list cleanly without HTTP `500`), `m8_s08_t01_e2e.rs` (missing v3 `.complete` fails restore), and `m8_s01_t02_writer_barrier.rs`.
* **Static Analysis & Governance**:
  - `python cms/scripts/check-standards.py && python cms/scripts/check-standards.py --self-test`
  - `python cms/scripts/check-plans.py && python cms/scripts/check-plans.py --self-test`
  - `python .agents/scripts/check-agent-artifacts.py`
  - `node cms/scripts/check-i18n.mjs --strict`
  - `cargo fmt --manifest-path cms/Cargo.toml --all --check`
  - `git diff --check`
* **E2E / Integration (Live Chrome & Upgrade Smoke)**:
  - `cms/target/release/opendash.exe --check-theme cms/fixtures/admin-review/theme`
  - `python cms/scripts/windows_auth_smoke.py --binary cms/target/release/opendash.exe --baseline-db cms/fixtures/baseline_0001.sqlite`
  - `python cms/scripts/pattern_library_studio_e2e.py --binary cms/target/release/opendash.exe` (covering Pattern Mode 2 `#/studio/entry/pattern/:id`, Admin-only RBAC, exit to `#/patterns/:id/edit`, and legacy `#/builder` Pattern redirect)
  - `python cms/scripts/admin_extensions_e2e.py --binary cms/target/release/opendash.exe` (using v3 `admin-review/theme`)
  - `python cms/scripts/admin_navigation_e2e.py --binary cms/target/release/opendash.exe`
  - `python cms/scripts/operations_backup_restore_e2e.py --binary cms/target/release/opendash.exe`
