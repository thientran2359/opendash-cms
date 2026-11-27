# Admin Feature Modules (`cms/admin/src/features/`)

## Overview

Domain feature panels and workspaces of the OpenDash React Admin (`cms/admin/`), mounted inside [AdminShell](../shell/README.md) and backed by [the HTTP API client](../api/README.md), [shared UI components](../components/README.md), and [i18n](../i18n/README.md). Designed per [Admin Navigation & Edit Screen](../../../../docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md) and [Core Foundation](../../../../docs/contracts/CORE_FOUNDATION.md).

## Interface

Active `features/*` subdirectories (17 directories on disk; no per-subdirectory `README.md` files):

- `contact/`: `[ContactPanel.tsx:ContactPanel]` (form builder and submissions inbox) and `[SmtpSettingsPanel.tsx:SmtpSettingsPanel]` (SMTP host, TLS, credentials, test email).
- `content/`: `[EntryListPage.tsx:EntryListPage]` (`parseEntryTableHash`, `entryTableHash`, `safePublicPath`), `[bulkEntryActions.ts:runSequentialBulk]` (`MAX_BULK_ITEMS`, `captureBulkRows`, `validateBulkSelection`, `buildBulkConfirmationPrompt`, `reconcileBulkResults`), `[QuickEditForm.tsx:QuickEditForm]` (`formatUtcToSiteLocal`), `[ContentTypeSchemaSettings.tsx:ContentTypeSchemaSettings]`, `[ContentTypeForm.tsx:ContentTypeForm]` (`DraftField`, `ContentTypeCreationDraft`, `blankContentTypeCreationDraft`, `isContentTypeCreationDraftDirty`), `[FieldGroupsManager.tsx:FieldGroupsManager]`, `[EditorLayoutConfigurator.tsx:EditorLayoutConfigurator]`, `[UnifiedFieldForm.tsx:UnifiedFieldForm]`, and wire types in `types.ts` (`[types.ts:ContentEntryRecord]`, `[types.ts:EntryTableResult]`, `[types.ts:MetadataPatch]`, `[types.ts:EffectiveEditorConfigProjection]`).
- `dashboard/`: `[DashboardPage.tsx:DashboardPage]` (post and page publication counts).
- `editor/`: `[SingleEditWorkspace.tsx:SingleEditWorkspace]` (unified list/new/edit workspace for `post`, `page`, and custom content types), `[EditorDraftStore.ts:EditorDraftStore]` / `[EditorDraftStore.ts:editorDraftStore]` (`DraftStatus`, `EditorDraftPayload`, `EditorDraftEntry`, `QuickEditDraftEntry`, `diffEditorPayload`, `isQuickEditPatchDirty`), and `[PanelRegionRenderer.tsx:PanelRegionRenderer]` (`header_bar`, `main_top`, `main_primary`, `main_bottom`, `sidebar`).
- `extensions/`: `[AdminContributionRenderer.tsx:AdminColumnCell]`, `[AdminContributionRenderer.tsx:AdminTermPanels]`, and `[AdminContributionRenderer.tsx:safeAdminProviderLink]` for theme/plugin entry columns, row actions, and term extension panels.
- `library/`: `[TemplateLibraryPage.tsx:TemplateLibraryPage]` (shared patterns, theme template roles, and site template override resets).
- `media/`: `[MediaLibraryPage.tsx:MediaLibraryPage]`, `[MediaDetailsPanel.tsx:MediaDetailsPanel]` (responsive variant inspection and Admin-only in-place Replace File), `[MediaVariantsRegenerateCard.tsx:MediaVariantsRegenerateCard]` (cooperative multi-batch variant regeneration with cancellation), `[MediaPicker.tsx:MediaPicker]` (single image and gallery modal), `[library/fileValidation.ts:validateImageUpload]`, `[library/fileValidation.ts:validateVideoUpload]`, `[library/fileValidation.ts:validateReplaceMediaFile]`, `[library/fileValidation.ts:MediaUploadValidationError]`, `[library/format.ts:formatMediaBytes]`, `[library/format.ts:formatMediaDate]`, and `media-library.css`.
- `operations/`: `[BackupToolsPanel.tsx:BackupToolsPanel]`, `[BackupsPanel.tsx:BackupsPanel]`, `[RestorePanel.tsx:RestorePanel]`, `[ReconcilePanel.tsx:ReconcilePanel]` (encrypted backup creation/download, restore + safe-mode activation, canonical/variant orphan media reconciliation, and responsive variant regeneration controls), `[RedirectsToolsPanel.tsx:RedirectsToolsPanel]`, `[ManualRedirectsPanel.tsx:ManualRedirectsPanel]`, `[RedirectFormModal.tsx:RedirectFormModal]`, `[CsvImportModal.tsx:CsvImportModal]`, `[NotFoundLogPanel.tsx:NotFoundLogPanel]`, and `[redirectsDraftStore.ts:getRedirectsDraft]` / `setRedirectsDraft` / `discardRedirectsDraft` / `clearRedirectsDrafts` (manual 301/302 redirect rules, CSV import/export, and aggregated 404 log).
- `patterns/`: `[PatternsPage.tsx:PatternsPage]` (private Pattern CPT CRUD and conflict-aware deletion for `core/pattern_ref`).
- `plugins/`: `[PluginsPanel.tsx:PluginsPanel]`, `[useAdminPlugins.ts:useAdminPlugins]`, `[DeclarativePluginUi.tsx:DeclarativePluginUi]`, `[PluginLifecycleConfirmation.tsx:PluginLifecycleConfirmation]` (core-owned host confirmation UI for plugin purge/public-restore), and `[declarativeSchema.ts:validateDeclarativeSchemaV1]` (`UI_SCHEMA_VERSION`, `MAX_UI_SCREENS`, `MAX_UI_DEPTH`, `MAX_UI_NODES`, `MAX_UI_PAGE_SIZE`, `buildDraftKey`, `InMemoryDraftStore`) for plugin ZIP install, enable/disable, settings, WASM run, and declarative UI.
- `seo/`: `[SeoPanel.tsx:SeoPanel]` (locale-aware homepage SEO controls; per-entry route & SEO fields are edited atomically inside `SingleEditWorkspace.tsx`).
- `settings/`: `[MultilingualSettingsPage.tsx:MultilingualSettingsPage]`, `[PermalinkSettingsPage.tsx:PermalinkSettingsPage]` (`discardPermalinkDraft`, `clearPermalinkDrafts`), and `[ReadingSettingsPanel.tsx:ReadingSettingsPanel]` (`discardReadingDraft`, `clearReadingDrafts`).
- `shared/`: `[OwnerOptionsForm.tsx:OwnerOptionsForm]`, `[OwnerOptionsForm.tsx:OwnerKind]`, `[OwnerOptionsForm.tsx:OwnerOptionsFormProps]`, `[OwnerOptionsForm.tsx:sameOwnerOptionValues]`, `[OwnerOptionsForm.tsx:isSecretConfiguredValue]`, `[OwnerOptionsForm.tsx:isClearSecretMarker]`, `[OwnerOptionsForm.tsx:normalizeOwnerValuesForFields]`, and `[OwnerOptionsForm.tsx:buildOwnerOptionsSubmitPayload]` shared between theme and plugin option screens.
- `system/`: `[GeneralSettingsPanel.tsx:GeneralSettingsPanel]` (`site_title`, `tagline`, `timezone`, `cms_locale`, public origin/indexing) and `[SystemStatus.tsx:SystemStatus]`.
- `taxonomy/`: `[TaxonomyDefinitionsPage.tsx:TaxonomyDefinitionsPage]`, `[TaxonomyPage.tsx:TaxonomyPage]`, `[TermEditPage.tsx:TermEditPage]`, `[TermPicker.tsx:TermPicker]`, and `[TermPicker.tsx:TermFilters]`.
- `themes/`: `[ThemesPanel.tsx:ThemesPanel]` (Theme v3 ZIP install, revision activation, selective sample-data dry-run/execute) and `[ThemeOptionsForm.tsx:ThemeOptionsForm]`.
- `users/`: `[UsersPanel.tsx:UsersPanel]` and `[UserProfilePanel.tsx:UserProfilePanel]`.

## Invariants

- **API & Component Discipline:** Feature modules never issue raw `fetch` calls; every network operation goes through `../api/` and reuses `../components/` and `../i18n/`.
- **In-Memory Draft Quarantine:** Unsaved drafts live exclusively in JavaScript memory (`Map`) across `[EditorDraftStore.ts:EditorDraftStore]`, `[PermalinkSettingsPage.tsx:PermalinkSettingsPage]`, `[ReadingSettingsPanel.tsx:ReadingSettingsPanel]`, `[TaxonomyDefinitionsPage.tsx:TaxonomyDefinitionsPage]`, `[ContentTypeSchemaSettings.tsx:ContentTypeSchemaSettings]` (`taxonomyDraftMemory`, `settingsDraftMemory`, `createCptDraftMemory`), `[TermEditPage.tsx:TermEditPage]` (`termDrafts`), `[redirectsDraftStore.ts:getRedirectsDraft]` (`setRedirectsDraft` / `discardRedirectsDraft` / `clearRedirectsDrafts`, all scoped by site/actor context), and `[DeclarativePluginUi.tsx:DeclarativePluginUi]` (keyed by `(plugin_id, screen_id, form_id, objectId ?? 'new', schema_version)` via `[declarativeSchema.ts:buildDraftKey]`), surviving transient `401` re-authentication and `412` CAS conflicts without touching `localStorage` or `sessionStorage`.
- **Safe Extension Rendering:** `[AdminContributionRenderer.tsx:safeAdminProviderLink]` restricts extension links to same-origin root-relative paths; `[declarativeSchema.ts:validateDeclarativeSchemaV1]` enforces `UI_SCHEMA_VERSION = 1`, depth `<= 8`, nodes `<= 200`, screens `<= 20`, and rejects `<`, `>`, or `javascript:` strings.
- **Publication & Media Preflight:** `[SingleEditWorkspace.tsx:SingleEditWorkspace]` respects `immediate` vs `draft_publish` publication policies via `[../api/content.ts:effectiveContentTypeSettings]`; `[library/fileValidation.ts:validateImageUpload]` (`8 MiB`, `6000 px`, `20M` pixels) and `[library/fileValidation.ts:validateVideoUpload]` (`100 MiB` MP4) pre-validate before upload while Rust remains authoritative.

## Side Effects

- Invokes authenticated Admin HTTP endpoints via `../api/` to mutate entries, schemas, field groups, taxonomies, media, themes, plugins, settings, users, contact forms, and backup/reconcile jobs.
- Registers `AbortController` cleanup, `URL` hash navigation callbacks (`onNavigateHash`), dirty-leave flags (`onDirtyChange`), and modal UI overlays (`MediaPicker` ARIA dialog, inline native `<dialog>` in `SingleEditWorkspace.tsx`, and local `ConfirmDialog` in `MultilingualSettingsPage.tsx`).

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
