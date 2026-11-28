# Visual Builder Core (`cms/admin/src/builder/`)

## Overview

Custom React + TypeScript Unified Recursive AST visual page builder (`BlockNode[]` up to depth `8`, `schema_version: 1`, zero `dnd-kit`/`Puck` dependencies) powering [OpenDash Studio](../studio/README.md) across Mode 1 (`#/studio`) and Mode 2 (`#/studio/entry/:typeKey/:id`, including Admin-only `pattern`). Includes AST document helpers, declarative schema validation, recursive drag-mask computation, schema-driven block inspection (`UnifiedFieldForm`), responsive field controls, pattern insertion modal, rich-text lightbox sanitizer, and sandboxed live preview bridge. See [package.json](../../package.json) and [Core Foundation](../../../../docs/contracts/CORE_FOUNDATION.md).

## Interface

- **AST Model & Pure Document Transforms (`document.ts` / `document/`):**
  - Types & constants: `[document.ts:BuilderDocument]`, `[document.ts:BlockNode]`, `[document.ts:NodeLocation]`, `[document.ts:NodeMeta]`, `[document.ts:NodeVisibility]`, `[document.ts:BuilderContextTarget]`, `[document.ts:DragCompatibilityMask]`, `[document.ts:StudioBuilderClipboard]`, `[document.ts:BuilderViewport]`, `[document.ts:BoxSpacing]`, `[document.ts:EMPTY_DOCUMENT]`, `[document.ts:VIEWPORT_WIDTHS]`, `[document.ts:VALID_TEMPLATE_PART_KEY]`, `[document.ts:MAX_BUILDER_DEPTH]`, `[document.ts:MAX_BUILDER_NODES]`, `[document.ts:MAX_BUILDER_BYTES]`, `[document.ts:MAX_BUILDER_LISTS]`, `[document.ts:MAX_BUILDER_PATTERN_INSTANCES]`, `[document.ts:MAX_BUILDER_CONTENT_SLOTS]`.
  - Inspection & validation: `[document.ts:allNodes]`, `[document.ts:locate]`, `[document.ts:findNode]`, `[document.ts:visibleNodesInTree]`, `[document.ts:maxSubtreeDepth]`, `[document.ts:findCatalogItem]`, `[document.ts:findActiveCatalogItem]`, `[document.ts:findBlockSchema]`, `[document.ts:isContainerBlock]`, `[document.ts:isLockedCompoundChildSchema]`, `[document.ts:getLockedCompoundChild]`, `[document.ts:isLockedNode]`, `[document.ts:hasLockedAncestor]`, `[document.ts:isProtectedNode]`, `[document.ts:preservesAnchors]`, `[document.ts:canDeleteNode]`, `[document.ts:canParentAcceptChild]`, `[document.ts:canInsertBlockAt]`, `[document.ts:computeDragMask]`, `[document.ts:matchesPatternList]`, `[document.ts:matchesContextTarget]`, `[document.ts:countBlocksByType]`, `[document.ts:countDescendantBlocksByType]`, `[document.ts:withinBuilderLimits]`, `[document.ts:pluginValuesReadyForFreshNode]`, `[document.ts:distinctPatternIdCount]`, `[document.ts:nodeTitle]`, `[document.ts:nodeTypeTitle]`, `[document.ts:parsePositiveInt]`.
  - Immutable mutations & clipboard: `[document.ts:newNodeId]`, `[document.ts:clone]`, `[document.ts:createStarterTemplateDocument]`, `[document.ts:insertBlock]`, `[document.ts:instantiateBlockNode]`, `[document.ts:initialBlockProps]`, `[document.ts:duplicateNode]`, `[document.ts:removeNode]`, `[document.ts:moveNode]`, `[document.ts:moveNodeSibling]`, `[document.ts:setNodeProps]`, `[document.ts:patchNodeProps]`, `[document.ts:setNodeMeta]`, `[document.ts:copyNodeOptions]`, `[document.ts:canPasteOptions]`, `[document.ts:pasteNodeOptions]`, `[document.ts:cloneWithFreshNodeIds]`.
- **Core Editor (`BuilderEditorCore.tsx`):**
  - `[BuilderEditorCore.tsx:BuilderEditorCore]` (`StudioBuilderView` `'tree' | 'properties' | 'add'`): Shared editing core used by `StudioWorkspace` and `StudioCustomizerNav` (paired with `[../studio/StudioBlockTree.tsx:StudioBlockTree]`, `[../studio/StudioBlockCatalog.tsx:StudioBlockCatalog]`, and `[BuilderPreviewFrame.tsx:BuilderPreviewFrame]`).
- **Sandboxed Live Preview (`BuilderPreviewFrame.tsx`, `previewMediaRewrite.ts`):**
  - `[BuilderPreviewFrame.tsx:preparePreviewMarkup]`, `[BuilderPreviewFrame.tsx:PreparePreviewOptions]`, `[BuilderPreviewFrame.tsx:BuilderPreviewFrame]`, `[BuilderPreviewFrame.tsx:BuilderPreviewFrameProps]`: Prepares server-rendered preview HTML for `<iframe sandbox>`, mounts canvas gear controls (`data-od-kind` `section|container|block`), auto-opens enclosing `<details>` when selecting descendants, and bridges interactive pick/edit/menu/hover/scroll messages.
  - `[previewMediaRewrite.ts:PRIVATE_MEDIA_PATH]`, `[previewMediaRewrite.ts:CSS_MEDIA_URL]`, `[previewMediaRewrite.ts:rewriteRootRelativeUrl]`, `[previewMediaRewrite.ts:rewritePreviewSrcset]`, `[previewMediaRewrite.ts:collectPrivateMediaPaths]`, `[previewMediaRewrite.ts:applyPreviewMediaRewrites]`: Handles versioned private/public media URLs, CSS `url(...)`, and responsive `img`/`source[srcset]` rewriting in preview markup.
- **Inspector & Pattern Modal (`BuilderInspector.tsx`, `PatternLibraryModal.tsx`):**
  - `[BuilderInspector.tsx:BuilderInspector]`: Thin unified inspector wrapping `[../fields/UnifiedFieldForm.tsx:UnifiedFieldForm]` (`context="studio_block"`, `layoutMode="sidebar"`) plus `NodeMeta` attributes/visibility controls, with read-only lock presentation for unavailable providers.
  - `[PatternLibraryModal.tsx:PatternLibraryModal]`.
- **Field Controls & Rich Text (`StudioFieldControls.tsx`, `StudioRichTextLightbox.tsx`):**
  - `[StudioFieldControls.tsx:STUDIO_INPUT_CLASS]`, `[StudioFieldControls.tsx:TW_PADDING_STEPS]`, `[StudioFieldControls.tsx:TW_MARGIN_STEPS]`, `[StudioFieldControls.tsx:TW_GAP_STEPS]`, `[StudioFieldControls.tsx:pxToTwStep]`, `[StudioFieldControls.tsx:formatGapSummary]`, `[StudioFieldControls.tsx:formatSpacingSummary]`, `[StudioFieldControls.tsx:ResponsiveDotInfo]`, `[StudioFieldControls.tsx:ResponsiveDot]`, `[StudioFieldControls.tsx:ToggleSwitchControl]`, `[StudioFieldControls.tsx:RangeNumberControl]`, `[StudioFieldControls.tsx:TailwindGapControl]`, `[StudioFieldControls.tsx:SegmentedButtonsControl]`, `[StudioFieldControls.tsx:ColorSwatchControl]`, `[StudioFieldControls.tsx:MediaCardControl]`, `[StudioFieldControls.tsx:SpacingBoxControl]`, `[StudioFieldControls.tsx:LinkControl]`, `[StudioFieldControls.tsx:LightboxRichTextTrigger]`.
  - `[StudioRichTextLightbox.tsx:StudioRichTextLightbox]`, `[StudioRichTextLightbox.tsx:serializeCleanRichHtml]`, `[StudioRichTextLightbox.tsx:stripRichHtmlToPreview]`.

## Invariants

- **Document Bounds & Anchor Preservation:** `[document.ts:withinBuilderLimits]` and `[BuilderEditorCore.tsx:BuilderEditorCore]` bound documents to depth `<= 8`, `<= 512` nodes, `<= 256 KiB` UTF-8 bytes, `<= 8` `core/list` blocks, `<= 16` `core/pattern-ref` instances/distinct IDs, and `<= 1` `core/content-slot`. `[document.ts:preservesAnchors]` guarantees inactive/unavailable nodes and their descendant subtrees remain immutable across all edits, while `[document.ts:hasLockedAncestor]` forbids inserting, editing, reordering, or deleting inside inactive containers.
- **Schema-Driven Composition:** Container parent/child compatibility and context availability are governed solely by `BlockSchema` (`supports_children`, `allowed_parents`, `allowed_children`, `allowed_contexts`, `default_children`, `presets`) via `[document.ts:canParentAcceptChild]`, `[document.ts:matchesContextTarget]`, and `[document.ts:computeDragMask]`.
- **Slot & Template Part Rules:** `[document.ts:canDeleteNode]` blocks removing the last `core/content-slot` when `requiresContentSlot` is `true`, and `core/template-part` requires `VALID_TEMPLATE_PART_KEY`.
- **Sandboxed Preview Security:** `[BuilderPreviewFrame.tsx:preparePreviewMarkup]` strips `<script>` and CSP/bridge `<meta>` tags from server HTML, embeds at most `64` private media images/MP4 videos (`48 MiB` total budget) as data URLs, enforces a strict CSP (`script-src 'sha256-...'` when interactive or `'none'` otherwise, `form-action 'none'`), and verifies `event.source === iframe.contentWindow` plus per-render `nonce` on every `postMessage`.
- **Sanitized Rich Text:** `[StudioRichTextLightbox.tsx:serializeCleanRichHtml]` serializes visual edits strictly to allowlisted tags (`p`, `strong`, `em`, `u`, `ul`, `ol`, `li`, `blockquote`, `br`, `a` with validated `http:`, `https:`, `mailto:`, `tel:`, `/`, or `#` hrefs).

## Side Effects

- Loads and saves block catalogs (`[../api/builder.ts:getBlockSchemas]`), entry/template Builder previews and saves (`[../api/builder.ts:previewEntryBuilder]`, `[../api/studio.ts:postStudioPreview]`, `[../api/builder.ts:saveEntryBuilder]`, `[../api/builder.ts:putTemplateOverride]`), patterns (`[../api/patterns.ts:listPatterns]`, `[../api/patterns.ts:listThemePatterns]`, `[../api/patterns.ts:getThemePattern]`), contact forms, CPT/taxonomy metadata, entry table pickers (`[../api/content.ts:queryContentEntryTable]`), and private media blobs (`[../api/builder.ts:getPrivatePreviewMedia]`).
- Registers `window` `focus` and `message` listeners (`BuilderEditorCore`, `BuilderPreviewFrame`), opens modal portals (`MediaPicker`, `PatternLibraryModal`, `StudioRichTextLightbox`), and executes `postMessage` calls to the preview `iframe`.

## Verification

- TypeScript check & Vite build (`cms/admin/package.json`):
  ```powershell
  npm --prefix cms/admin run typecheck
  npm --prefix cms/admin run build
  npm --prefix cms/admin run test:media-preview
  ```
- Standards, i18n, and Builder tests (`cms/scripts/`, `cms/admin/scripts/`):
  ```powershell
  python cms/scripts/check-standards.py
  node cms/scripts/check-i18n.mjs
  node --test cms/admin/scripts/test-studio-ux.mjs
  ```
