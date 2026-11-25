# OpenDash Studio Workspace (`cms/admin/src/studio/`)

## Overview

Fullscreen visual customization and page-building workspace (`#/studio` and `#/studio/entry/:typeKey/:id`) pairing a 320px left sidebar with a live [BuilderPreviewFrame](../builder/README.md) canvas. Unifies site identity, global/theme options, menu management, reading settings, typed template overrides (Mode 1), and entry-scoped visual page building (Mode 2). See [Admin Navigation & Edit Screen](../../../../docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md) and [CMS Architecture](../../../../docs/architecture/CMS_ARCHITECTURE.md).

## Interface

- **Workspace Orchestrator (`StudioWorkspace.tsx`):**
  - `[StudioWorkspace.tsx:StudioWorkspace]`: Coordinates Mode 1 (`route.view === 'studio'`, Admin-only customizer & typed template overrides) and Mode 2 (`route.view === 'studio-entry'`, entry builder), `50`-step Undo/Redo history, `250 ms` debounced live preview (`[../api/studio.ts:postStudioPreview]` / `[../api/builder.ts:previewEntryBuilder]`), preview locale switching, `412` conflict handling, and `401` draft quarantine.
- **Customizer Navigation (`StudioCustomizerNav.tsx`):**
  - `[StudioCustomizerNav.tsx:StudioCustomizerNav]`: Renders Mode 1 sub-panels (`'root'`, `'identity'`, `'theme-options'`, `'menus'`, `'reading'`, `'templates'`), grouped template target list (`parts`, `core`, `content_types`, `page_layouts`, `custom_templates`, `overrides`), and `[../builder/BuilderWorkspace.tsx:BuilderEditorCore]` once a template override is initialized.
- **Menu Manager (`StudioMenuManager.tsx`):**
  - `[StudioMenuManager.tsx:StudioMenuManager]`: Two-level sidebar editor for theme menu slot assignments (`getMenuSlots` / `putMenuSlots`) and menu/item CRUD (`createMenu`, `getMenu`, `updateMenu`, `deleteMenu`) with hierarchical indentation (up to depth 7) targeting published entries, public taxonomy terms, or external URLs.
- **Sidebar Builder Controls (`StudioBlockTree.tsx`, `StudioBlockCatalog.tsx`, `StudioSidebarHeader.tsx`, `StudioSidebarFooter.tsx`):**
  - `[StudioBlockTree.tsx:StudioBlockTree]` (`[StudioBlockTree.tsx:StudioMenuRequest]`, `[StudioBlockTree.tsx:StudioDropPosition]`): Collapsible WAI-ARIA tree with drag-and-drop sibling reordering, keyboard navigation (`Alt+ArrowUp/Down`, `Shift+F10`), and floating action popover synced with canvas gear/context-menu picks.
  - `[StudioBlockCatalog.tsx:StudioBlockCatalog]`, `[StudioBlockCatalog.tsx:isBlockAllowedInParent]`, `[StudioBlockCatalog.tsx:studioCatalogAdditionLimit]`: Searchable block/layout catalog enforcing `allowed_parents`/`allowed_children` constraints plus node-count (`512`) and serialized `JSON.stringify(...).length` (`256 * 1024` UTF-16 code units) limits before insertion.
  - `[StudioSidebarHeader.tsx:StudioSidebarHeader]`: Contextual title/badge, back navigation, dirty indicator, overflow reset/reload menu, and exit button.
  - `[StudioSidebarFooter.tsx:StudioSidebarFooter]`, `[StudioSidebarFooter.tsx:StudioSavePolicy]`: Responsive viewport switcher (`desktop`, `tablet`, `mobile`), pattern library shortcut, Undo/Redo buttons, and policy-aware Save/Draft/Publish controls.

## Invariants

- **Mode & Policy Separation:** Mode 1 edits site customizer settings and typed template overrides (`putTemplateOverride`); Mode 2 edits a single entry's Builder AST (`saveEntryBuilder`) and honors `immediate` vs `draft_publish` publication policies (`[StudioSidebarFooter.tsx:StudioSavePolicy]`).
- **Template Slot & Part Guardrails:** Mode 1 save validates `requires_content_slot` (exactly one `core/content_slot`), `forbids_content_slot` (`0` slots), and `VALID_TEMPLATE_PART_KEY` before submitting.
- **Transient Preview & Explicit Persistence:** `[StudioWorkspace.tsx:StudioWorkspace]` sends unsaved Mode 1 edits (`general`, `global_options`, `theme_options`, `menu_slots`) as transient preview overrides (`[../api/studio.ts:TransientStudioOverrides]`); nothing is written to the database until explicit Save.
- **Locale & Optimistic Concurrency Safety:** `localeDraftBlocked` prevents Mode 1 saves when the selected preview locale is invalid, reloading, or out of sync with the hydrated baseline; HTTP `412` sets `conflict = true` while preserving all unsaved local drafts.
- **Draft Quarantine & Undo Isolation:** On `401` (`!user || !csrfToken`), drafts remain in React state across same-account re-login and reset cleanly on account/role change or `resetToken` bump. Keyboard Undo/Redo shortcuts ignore text inputs (`INPUT`, `TEXTAREA`, `SELECT`, `isContentEditable`).

## Side Effects

- Loads and mutates Studio targets, typed template overrides, entry builders, general/reading settings, global site options, theme options, menus/menu slots, patterns, and block registry via `../api/`.
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
