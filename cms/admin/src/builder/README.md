# Visual Builder Core (`cms/admin/src/builder/`)

## Overview

Custom React + TypeScript 4-layer AST visual page builder (`Section -> Row -> Column -> Element`, zero `dnd-kit`/`Puck` dependencies) powering [OpenDash Studio](../studio/README.md) across Mode 1 (`#/studio`) and Mode 2 (`#/studio/entry/:typeKey/:id`, including Admin-only `pattern`). Includes AST document helpers, schema-driven block forms, responsive field controls, entry field bindings, content-list/contact-form inspectors, pattern insertion modal, rich-text lightbox sanitizer, and sandboxed live preview bridge. See [package.json](../../package.json) and [Core Foundation](../../../../docs/contracts/CORE_FOUNDATION.md).

## Interface

- **AST Model & Pure Document Transforms (`document.ts`):**
  - Types & constants: `[document.ts:BuilderDocument]`, `[document.ts:BuilderSection]`, `[document.ts:BuilderRow]`, `[document.ts:BuilderColumn]`, `[document.ts:BuilderElement]`, `[document.ts:NodeLocation]`, `[document.ts:VisibleNodeLocation]`, `[document.ts:StudioBuilderClipboard]`, `[document.ts:ELEMENT_TYPES]`, `[document.ts:EMPTY_DOCUMENT]`, `[document.ts:VIEWPORT_WIDTHS]`, `[document.ts:VIEWPORT_LABELS]`, `[document.ts:RESPONSIVE_FIELDS]`, `[document.ts:VALID_TEMPLATE_PART_KEY]`.
  - Inspection & protection: `[document.ts:allNodes]`, `[document.ts:locate]`, `[document.ts:visibleNodesInTree]`, `[document.ts:carrierLocation]`, `[document.ts:isElement]`, `[document.ts:isSyntheticNode]`, `[document.ts:isLockedElement]`, `[document.ts:isProtectedNode]`, `[document.ts:preservesAnchors]`, `[document.ts:canDeleteElement]`, `[document.ts:countElementsByType]`, `[document.ts:countDescendantElementsByType]`, `[document.ts:effectiveResponsive]`, `[document.ts:rowUsesGrid]`, `[document.ts:nodeTitle]`, `[document.ts:nodeTypeTitle]`.
  - Immutable mutations: `[document.ts:newNodeId]`, `[document.ts:appendNode]`, `[document.ts:appendRowWithColumns]`, `[document.ts:appendFreeformElement]`, `[document.ts:duplicateNode]`, `[document.ts:removeNode]`, `[document.ts:moveNode]`, `[document.ts:reorderSiblingNode]`, `[document.ts:copyNodeOptions]`, `[document.ts:canPasteOptions]`, `[document.ts:pasteNodeOptions]`, `[document.ts:setNodeField]`, `[document.ts:patchNodeFields]`, `[document.ts:setNodeMeta]`, `[document.ts:setResponsiveField]`, `[document.ts:setContainerBoxSpacing]`, `[document.ts:syncContainerResponsiveToDesktop]`, `[document.ts:setSectionStyle]`, `[document.ts:setElementBinding]`, `[document.ts:setListProps]`, `[document.ts:cloneWithFreshNodeIds]`, `[document.ts:createStarterTemplateDocument]`.
- **Core Editor (`BuilderEditorCore.tsx`):**
  - `[BuilderEditorCore.tsx:BuilderEditorCore]` (`StudioBuilderView` `'tree' | 'properties' | 'add'`): Shared editing core used by `StudioWorkspace` and `StudioCustomizerNav` (paired with `[../studio/StudioBlockTree.tsx:StudioBlockTree]` and `[BuilderPreviewFrame.tsx:BuilderPreviewFrame]`).
- **Sandboxed Live Preview (`BuilderPreviewFrame.tsx`):**
  - `[BuilderPreviewFrame.tsx:preparePreviewMarkup]`, `[BuilderPreviewFrame.tsx:PreparePreviewOptions]`, `[BuilderPreviewFrame.tsx:BuilderPreviewFrame]`, `[BuilderPreviewFrame.tsx:BuilderPreviewFrameProps]`: Prepares server-rendered preview HTML for `<iframe sandbox>` and bridges interactive pick/edit/menu/hover/scroll messages.
- **Inspector & Specialized Field Panels (`BuilderInspector.tsx`, `BuilderBindingFields.tsx`, `BuilderListFields.tsx`, `BuilderContactFormFields.tsx`, `BlockSchemaForm.tsx`, `PatternLibraryModal.tsx`):**
  - `[BuilderInspector.tsx:BuilderInspector]`: Contextual property editor for containers, core blocks, `legacy/block`, and declarative `plugin/block` elements.
  - `[BlockSchemaForm.tsx:BlockSchemaForm]`: Declarative `BlockSchemaV1` renderer supporting `group`, `repeater`, `show_if`, responsive overrides, and media picking.
  - `[BuilderBindingFields.tsx:BuilderBindingFields]`, `[BuilderListFields.tsx:BuilderListFields]`, `[BuilderContactFormFields.tsx:BuilderContactFormFields]`, `[PatternLibraryModal.tsx:PatternLibraryModal]`.
- **Field Controls & Rich Text (`StudioFieldControls.tsx`, `StudioRichTextLightbox.tsx`):**
  - `[StudioFieldControls.tsx:STUDIO_INPUT_CLASS]`, `[StudioFieldControls.tsx:TW_PADDING_STEPS]`, `[StudioFieldControls.tsx:TW_MARGIN_STEPS]`, `[StudioFieldControls.tsx:TW_GAP_STEPS]`, `[StudioFieldControls.tsx:pxToTwStep]`, `[StudioFieldControls.tsx:formatGapSummary]`, `[StudioFieldControls.tsx:formatSpacingSummary]`, `[StudioFieldControls.tsx:ResponsiveDotInfo]`, `[StudioFieldControls.tsx:ResponsiveDot]`, `[StudioFieldControls.tsx:ToggleSwitchControl]`, `[StudioFieldControls.tsx:RangeNumberControl]`, `[StudioFieldControls.tsx:TailwindGapControl]`, `[StudioFieldControls.tsx:SegmentedButtonsControl]`, `[StudioFieldControls.tsx:ColorSwatchControl]`, `[StudioFieldControls.tsx:MediaCardControl]`, `[StudioFieldControls.tsx:SpacingBoxControl]`, `[StudioFieldControls.tsx:LinkControl]`, `[StudioFieldControls.tsx:LightboxRichTextTrigger]`.
  - `[StudioRichTextLightbox.tsx:StudioRichTextLightbox]`, `[StudioRichTextLightbox.tsx:serializeCleanRichHtml]`, `[StudioRichTextLightbox.tsx:stripRichHtmlToPreview]`.

## Invariants

- **Document Bounds & Anchor Preservation:** Internal validation in `document.ts` (`withinBuilderLimits`) bounds documents to `<= 512` nodes and `<= 256 KiB` UTF-8 bytes, while live edit guards in `[BuilderEditorCore.tsx:BuilderEditorCore]` check `JSON.stringify(...).length > 256 * 1024` (UTF-16 code units), at most `8` `core/list` blocks, and at most `16` distinct `core/pattern_ref` IDs. `[document.ts:preservesAnchors]` guarantees unresolved, retired `legacy/block`, or inactive-provider elements (`[document.ts:isLockedElement]`) retain their exact node ID, ancestor IDs, path index, and payload across all edits.
- **Freeform Synthetic Wrappers:** `[document.ts:appendFreeformElement]` and `[document.ts:appendRowWithColumns]` mark auto-generated structural containers with `meta.synthetic = true`; `[document.ts:visibleNodesInTree]` hides them in the tree while `[document.ts:carrierLocation]` lifts delete/move/duplicate/reorder operations to the full carrier.
- **Desktop -> Tablet -> Mobile Cascade:** `[document.ts:effectiveResponsive]` cascades properties from `desktop` -> `tablet` -> `mobile` without mutating unset viewports; `[BuilderInspector.tsx:BuilderInspector]` and `[BlockSchemaForm.tsx:BlockSchemaForm]` preserve unknown persisted props/keys across upgrades.
- **Slot & Template Part Rules:** `[document.ts:canDeleteElement]` blocks removing the last `core/content_slot` when `requiresContentSlot` is `true`, and `core/template_part` requires `VALID_TEMPLATE_PART_KEY`.
- **Sandboxed Preview Security:** `[BuilderPreviewFrame.tsx:preparePreviewMarkup]` strips `<script>` and CSP/bridge `<meta>` tags from server HTML, embeds at most `64` private media images (`48 MiB` total budget) as data URLs, enforces a strict CSP (`script-src 'sha256-...'` when interactive or `'none'` otherwise, `form-action 'none'`), and verifies `event.source === iframe.contentWindow` plus per-render `nonce` on every `postMessage`.
- **Sanitized Rich Text:** `[StudioRichTextLightbox.tsx:serializeCleanRichHtml]` serializes visual edits strictly to allowlisted tags (`p`, `strong`, `em`, `u`, `ul`, `ol`, `li`, `blockquote`, `br`, `a` with validated `http:`, `https:`, `mailto:`, `tel:`, `/`, or `#` hrefs).

## Side Effects

- Loads and saves block catalogs (`[../api/builder.ts:getBlockSchemas]`), entry/template Builder previews and saves (`[../api/builder.ts:previewEntryBuilder]`, `[../api/builder.ts:previewTemplateOverride]`, `[../api/builder.ts:saveEntryBuilder]`, `[../api/builder.ts:putTemplateOverride]`), patterns (`[../api/patterns.ts:listPatterns]`, `[../api/patterns.ts:listThemePatterns]`, `[../api/patterns.ts:getThemePattern]`), contact forms, CPT/taxonomy metadata, entry table pickers (`[../api/content.ts:queryContentEntryTable]`), and private media blobs (`[../api/builder.ts:getPrivatePreviewMedia]`).
- Registers `window` `focus` and `message` listeners (`BuilderEditorCore`, `BuilderPreviewFrame`), opens modal portals (`MediaPicker`, `PatternLibraryModal`, `StudioRichTextLightbox`), and executes `postMessage` calls to the preview `iframe`.

## Verification

- TypeScript check & Vite build (`cms/admin/package.json`):
  ```powershell
  npm --prefix cms/admin run typecheck
  npm --prefix cms/admin run build
  ```
- Standards, i18n, and Builder E2E checks (`cms/scripts/`):
  ```powershell
  python cms/scripts/check-standards.py
  node cms/scripts/check-i18n.mjs
  ```
