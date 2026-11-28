# OpenDash Studio Workspace (`cms/admin/src/studio/`)

## Overview

Fullscreen visual customization and page-building workspace (`#/studio` and `#/studio/entry/:typeKey/:id`) pairing a 320px left sidebar with a live [BuilderPreviewFrame](../builder/README.md) canvas. Unifies site identity, global/theme options, menu management, reading settings, typed template overrides (Mode 1), and entry-scoped visual page building (Mode 2). See [Admin Navigation & Edit Screen](../../../../docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md) and [CMS Architecture](../../../../docs/architecture/CMS_ARCHITECTURE.md).

## Interface

- **Workspace Orchestrator (`StudioWorkspace.tsx`, `useStudioMode1Controller.ts`, `useStudioOptionsState.ts`):**
  - `[StudioWorkspace.tsx:StudioWorkspace]`: Coordinates Mode 1 (`route.view === 'studio'`, Admin-only customizer & typed template overrides) and Mode 2 (`route.view === 'studio-entry'`, entry builder), 2-way URL sub-route sync (`#/studio/{identity,options/...,menus,reading,templates/...}`), `50`-step Undo/Redo history, `250 ms` debounced live preview (`[../api/studio.ts:postStudioPreview]` incl. fail-closed `transient.options_pages` / `[../api/builder.ts:previewEntryBuilder]`), preview locale switching, `409`/`412` conflict handling, and `401` draft quarantine.
  - `[useStudioOptionsState.ts:useStudioOptionsState]` & `[useStudioMode1Controller.ts:useStudioMode1Controller]`: Manage theme options, attached `site_global`/`theme:<id>` groups, lazy-loaded `appearance` Custom Options Pages, recursive public-preview sanitization, and per-domain baseline commits.
- **Customizer Navigation & Options Sub-Router (`StudioCustomizerNav.tsx`, `StudioOptionsPanel.tsx`, `studioOptionsSections.ts`):**
  - `[StudioCustomizerNav.tsx:StudioCustomizerNav]`: Renders Mode 1 root & sub-panels (`'root'`, `'identity'`, `'options'`, `'menus'`, `'reading'`, `'templates'`), delegating `'options'` to `[StudioOptionsPanel.tsx:StudioOptionsPanel]` (3-level zero-accordion drill-down over Global site options, Theme options, and `appearance` Custom Options Pages via `[studioOptionsSections.ts:partitionFieldSections]`), grouped template targets, and `[../builder/BuilderEditorCore.tsx:BuilderEditorCore]`.
- **Menu Manager (`StudioMenuManager.tsx`):**
  - `[StudioMenuManager.tsx:StudioMenuManager]`: Two-level sidebar editor for theme menu slot assignments (`getMenuSlots` / `putMenuSlots`) and menu/item CRUD (`createMenu`, `getMenu`, `updateMenu`, `deleteMenu`) with hierarchical indentation (up to depth 7) targeting published entries, public taxonomy terms, or external URLs.
- **Sidebar Builder Controls (`StudioBlockTree.tsx`, `StudioBlockCatalog.tsx`, `StudioSidebarHeader.tsx`, `StudioSidebarFooter.tsx`):**
  - `[StudioBlockTree.tsx:StudioBlockTree]` (`[StudioBlockTree.tsx:StudioMenuRequest]`, `[StudioBlockTree.tsx:StudioDropPosition]`): Collapsible recursive WAI-ARIA tree with 3-zone (`before`/`inside`/`after`) drag-and-drop driven by `[../builder/document.ts:computeDragMask]`, `500 ms` hover-to-expand on collapsed containers, keyboard navigation (`Alt+ArrowUp/Down`, `Shift+F10`), locked compound-child quick add, and floating action popover synced with canvas gear/context-menu picks.
  - `[StudioBlockCatalog.tsx:StudioBlockCatalog]`: Searchable declarative block and preset catalog filtered by `[../builder/document.ts:matchesContextTarget]` and `[../builder/document.ts:canParentAcceptChild]`, enforcing depth `8`, node (`512`), byte (`256 KiB` UTF-8), `core/list` (`8`), `core/pattern-ref` (`16`), and `core/content-slot` (`1`) bounds before insertion via `[../builder/document.ts:canInsertBlockAt]`.
  - `[StudioSidebarHeader.tsx:StudioSidebarHeader]`: Contextual title/badge, back navigation, dirty indicator, overflow reset/reload menu, and exit button.
  - `[StudioSidebarFooter.tsx:StudioSidebarFooter]`, `[StudioSidebarFooter.tsx:StudioSavePolicy]`: Responsive viewport switcher (`desktop`, `tablet`, `mobile`), pattern library shortcut, Undo/Redo buttons, and policy-aware Save/Draft/Publish controls.

## Invariants

- **Mode & Policy Separation:** Mode 1 edits site customizer settings and typed template overrides (`putTemplateOverride`); Mode 2 edits a single entry's Builder AST (`saveEntryBuilder`), guards trashed non-Pattern entries on load/preview/save (`409 entry_trashed`, offering a direct Trash link without discarding drafts), and honors `immediate` vs `draft_publish` publication policies (`[StudioSidebarFooter.tsx:StudioSavePolicy]`).
- **Template Slot & Part Guardrails:** Mode 1 save validates `requires_content_slot` (exactly one `core/content-slot`), `forbids_content_slot` (`0` slots), and `VALID_TEMPLATE_PART_KEY` before submitting.
- **Transient Preview & Explicit Persistence:** `[StudioWorkspace.tsx:StudioWorkspace]` sends unsaved Mode 1 edits (`general`, `global_options`, `theme_options`, `menu_slots`) as transient preview overrides (`[../api/studio.ts:TransientStudioOverrides]`); nothing is written to the database until explicit Save.
- **Locale & Optimistic Concurrency Safety:** `localeDraftBlocked` prevents Mode 1 saves when the selected preview locale is invalid, reloading, or out of sync with the hydrated baseline; HTTP `412` sets `conflict = true` while preserving all unsaved local drafts.
- **Draft Quarantine & Undo Isolation:** On `401` (`!user || !csrfToken`), drafts remain in React state across same-account re-login and reset cleanly on account/role change or `resetToken` bump. Keyboard Undo/Redo shortcuts ignore text inputs (`INPUT`, `TEXTAREA`, `SELECT`, `isContentEditable`).

## Side Effects

- Loads and mutates Studio targets, typed template overrides, entry builders, general/reading settings, global site options, theme options, menus/menu slots, patterns, and declarative block schemas via `../api/`.
- Clears matching entries in `[../features/editor/EditorDraftStore.ts:editorDraftStore]` after saving or publishing in Mode 2.
- Registers `window` `keydown` shortcuts and `document` `pointerdown`/`keydown` listeners for contextual menus and overflow popovers.

## Verification

- TypeScript check & Vite build (`cms/admin/package.json`):
  ```powershell
  npm --prefix cms/admin run typecheck
  npm --prefix cms/admin run build
  ```
- Standards & i18n checks (`cms/scripts/`):
  ```powershell
  python cms/scripts/check-standards.py
  node cms/scripts/check-i18n.mjs
  ```
