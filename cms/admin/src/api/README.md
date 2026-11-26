# Admin HTTP API Client (`cms/admin/src/api/`)

## Overview

Single HTTP transport and typed endpoint layer for the OpenDash React Admin (`cms/admin/`). All requests to `/auth/*`, `/api/health`, `/api/i18n/admin`, `/api/themes/*`, and `/api/admin/*` flow through [`client.ts`](client.ts) across 21 TypeScript modules (`index.ts` barrel plus 20 domain modules). See [Core Foundation](../../../../docs/contracts/CORE_FOUNDATION.md) and [CMS Architecture](../../../../docs/architecture/CMS_ARCHITECTURE.md).

## Interface

- **Transport & Error Primitives (`client.ts`, `index.ts`):**
  - `[client.ts:requestJson]`, `[client.ts:requestText]`, `[client.ts:requestBlob]`, `[client.ts:handleResponseText]`: Core request helpers sharing `[client.ts:ApiRequestOptions]`.
  - `[client.ts:ApiClientError]`, `[client.ts:PermalinkConflictItem]`, `[client.ts:getHealth]`: Structured HTTP error wrapper (status, parsed message, optional `409` permalink conflict items, `X-OpenDash-Error-Code`) and health probe; `[index.ts]` re-exports primary API functions and types.
- **Auth, Users & Dashboard (`auth.ts`, `users.ts`, `dashboard.ts`):**
  - `[auth.ts:getAuthMe]`, `[auth.ts:setupAdmin]`, `[auth.ts:login]`, `[auth.ts:logout]`, `[auth.ts:getAdminI18nBootstrap]`, `[auth.ts:listUsers]`, `[auth.ts:createUser]`, `[auth.ts:updateUser]`, `[auth.ts:resetUserPassword]`, `[auth.ts:removeUser]`, `[auth.ts:createEditor]`.
  - `[users.ts:getProfile]`, `[users.ts:updateProfile]`; `[dashboard.ts:getDashboard]`.
- **Content, Taxonomy, Patterns & Media (`content.ts`, `taxonomy.ts`, `patterns.ts`, `sections.ts`, `media.ts`):**
  - `[content.ts:listContentTypes]`, `[content.ts:createContentType]`, `[content.ts:updateContentType]`, `[content.ts:listFieldGroups]`, `[content.ts:createFieldGroup]`, `[content.ts:updateFieldGroup]`, `[content.ts:deleteFieldGroup]`, `[content.ts:getEditorLayout]`, `[content.ts:putEditorLayout]`, `[content.ts:getEffectiveEditorConfig]`, `[content.ts:getEffectiveContentTypeTaxonomies]`, `[content.ts:listContentEntries]`, `[content.ts:queryContentEntryTable]`, `[content.ts:queryRetainedTrashTable]`, `[content.ts:getEffectiveEntryListConfig]`, `[content.ts:runEntryProviderAction]`, `[content.ts:getContentEntry]`, `[content.ts:createEntryComposite]`, `[content.ts:updateEntryComposite]`, `[content.ts:changeContentEntryPublication]`, `[content.ts:createEntryTranslation]`, `[content.ts:trashContentEntry]`, `[content.ts:restoreContentEntry]`, `[content.ts:purgeContentEntry]`, `[content.ts:deleteContentEntry]`, `[content.ts:effectiveContentTypeSettings]`, `[content.ts:contentEntryPreviewPath]`.
  - `[taxonomy.ts:listTaxonomyDefinitions]`, `[taxonomy.ts:getTaxonomyDefinition]`, `[taxonomy.ts:createTaxonomyDefinition]`, `[taxonomy.ts:updateTaxonomyDefinition]`, `[taxonomy.ts:deleteTaxonomyDefinition]`, `[taxonomy.ts:listEffectiveTaxonomies]`, `[taxonomy.ts:updateTaxonomyEligibility]`, `[taxonomy.ts:listTaxonomyTerms]`, `[taxonomy.ts:listAllTaxonomyTerms]`, `[taxonomy.ts:resolveTaxonomyTermLabels]`, `[taxonomy.ts:getTaxonomyTerm]`, `[taxonomy.ts:getEffectiveTermConfig]`, `[taxonomy.ts:createTaxonomyTerm]`, `[taxonomy.ts:updateTaxonomyTerm]`, `[taxonomy.ts:deleteTaxonomyTerm]`, `[taxonomy.ts:appendEntryTermFilters]`.
  - `[patterns.ts:listPatterns]`, `[patterns.ts:getPattern]`, `[patterns.ts:createPattern]`, `[patterns.ts:updatePattern]`, `[patterns.ts:deletePattern]`, `[patterns.ts:listThemePatterns]`, `[patterns.ts:getThemePattern]`.
  - `[sections.ts:getLocalTemplateLibrary]`.
  - `[media.ts:listMedia]`, `[media.ts:uploadMedia]`, `[media.ts:uploadVideo]`, `[media.ts:deleteMedia]`, `[media.ts:mediaThumbnailUrl]`, `[media.ts:mediaOriginalUrl]`, `[media.ts:MediaDeleteConflictError]`.
- **Studio, Builder, Themes & Menus (`studio.ts`, `builder.ts`, `themes.ts`, `menus.ts`):**
  - `[studio.ts:getStudioTargets]`, `[studio.ts:postStudioPreview]`.
  - `[builder.ts:getTemplateOverrides]`, `[builder.ts:putTemplateOverride]`, `[builder.ts:previewTemplateOverride]`, `[builder.ts:resetTemplateOverride]`, `[builder.ts:getEntryBuilder]`, `[builder.ts:previewEntryBuilder]`, `[builder.ts:saveEntryBuilder]`, `[builder.ts:resetEntryBuilder]`, `[builder.ts:getBlockSchemas]`, `[builder.ts:getPrivatePreviewMedia]`.
  - `[themes.ts:getThemes]`, `[themes.ts:installTheme]`, `[themes.ts:activateTheme]`, `[themes.ts:activateThemeRevision]`, `[themes.ts:getActiveThemeLayouts]`, `[themes.ts:getThemeOptions]`, `[themes.ts:saveThemeOptions]`, `[themes.ts:dryRunSampleImport]`, `[themes.ts:executeSampleImport]`, `[themes.ts:isV3ThemeInstall]`.
  - `[menus.ts:listMenus]`, `[menus.ts:createMenu]`, `[menus.ts:getMenu]`, `[menus.ts:updateMenu]`, `[menus.ts:deleteMenu]`, `[menus.ts:previewMenu]`, `[menus.ts:getMenuSlots]`, `[menus.ts:putMenuSlots]`.
- **Settings, Locales, SEO, Plugins, Contact & Operations (`settings.ts`, `locales.ts`, `seo.ts`, `plugins.ts`, `contact.ts`, `smtp.ts`, `operations.ts`):**
  - `[settings.ts:getGeneralSettings]`, `[settings.ts:saveGeneralSettings]`, `[settings.ts:getTimezoneCatalog]`, `[settings.ts:getPublicSiteSettings]`, `[settings.ts:savePublicSiteSettings]`, `[settings.ts:getPermalinkSettings]`, `[settings.ts:savePermalinkSettings]`, `[settings.ts:getReadingSettings]`, `[settings.ts:saveReadingSettings]`, `[settings.ts:getGlobalSiteOptions]`, `[settings.ts:saveGlobalSiteOptions]`.
  - `[locales.ts:listLocales]`, `[locales.ts:deriveAdminLocaleState]`, `[locales.ts:createLocale]`, `[locales.ts:updateLocale]`, `[locales.ts:setDefaultLocale]`, `[locales.ts:setMultilingualEnabled]`, `[locales.ts:deleteLocale]`, `[locales.ts:listOrphanedLocales]`, `[locales.ts:purgeOrphanedLocale]`.
  - `[seo.ts:getEntryRoute]`, `[seo.ts:putEntryRoute]`, `[seo.ts:getEntrySeo]`, `[seo.ts:putEntrySeo]`, `[seo.ts:getHomeSeo]`, `[seo.ts:putHomeSeo]`.
  - `[plugins.ts:getPlugins]`, `[plugins.ts:installPlugin]`, `[plugins.ts:runPlugin]`, `[plugins.ts:enablePlugin]`, `[plugins.ts:disablePlugin]`, `[plugins.ts:getPluginUiSchema]`, `[plugins.ts:invokePluginAction]`, `[plugins.ts:runPluginAction]`, `[plugins.ts:confirmPluginLifecycleAction]`, `[plugins.ts:getPluginSettings]`, `[plugins.ts:savePluginSettings]`.
  - `[contact.ts:listContactForms]`, `[contact.ts:createContactForm]`, `[contact.ts:updateContactForm]`, `[contact.ts:deleteContactForm]`, `[contact.ts:listContactSubmissions]`, `[contact.ts:getContactSubmission]`, `[contact.ts:deleteContactSubmission]`.
  - `[smtp.ts:getSmtpSettings]`, `[smtp.ts:updateSmtpSettings]`, `[smtp.ts:testSmtpSettings]`, `[smtp.ts:notifyContactSubmission]`.
  - `[operations.ts:getOperationsDiagnostics]`, `[operations.ts:planReconcile]`, `[operations.ts:applyReconcile]`, `[operations.ts:purgeQuarantine]`, `[operations.ts:listBackups]`, `[operations.ts:createBackup]`, `[operations.ts:getOperationJob]`, `[operations.ts:cancelOperationJob]`, `[operations.ts:deleteBackupArtifact]`, `[operations.ts:getBackupDownloadUrl]`, `[operations.ts:uploadRestoreArchive]`, `[operations.ts:createRestoreJob]`, `[operations.ts:getRecoveryStatus]`, `[operations.ts:activateRecovery]`.

## Invariants

- **Single `fetch` Boundary:** 100% of `fetch` calls across `cms/admin/src/` originate inside `[client.ts:requestJson]`, `[client.ts:requestText]`, and `[client.ts:requestBlob]`, enforced by [check-standards.py](../../../scripts/check-standards.py).
- **Same-Origin Credentials & Security Headers:** Every call sets `credentials: 'same-origin'`; mutations attach `X-CSRF-Token`, optimistic concurrency guards serialize `If-Match: "vN"` (`ifMatchVersion`) or explicit `If-Match` (`ifMatch`, e.g. `"gv{generation}-v{version}"` in `[media.ts:deleteMedia]`), and binary uploads pass `X-File-Name: encodeURIComponent(uploadFilename)`.
- **Private Media Isolation:** `[client.ts:requestBlob]` enforces `cache: 'no-store'` and forwards `AbortSignal` when resolving authenticated preview media.
- **Deterministic Error & Localization Contract:** `client.ts` normalizes non-2xx responses into `[client.ts:ApiClientError]`, caps parsed `409` `conflicts` arrays at 20 items, maps `413` to `oversizeErrorMessage` in `[client.ts:requestJson]` (while `[client.ts:handleResponseText]`, `[client.ts:requestText]`, and `[client.ts:requestBlob]` fall back to `defaultMessage`), and sources fallback strings from `[../i18n/index.tsx:tStatic]` (`../i18n/README.md`).
- **Batch Bound Validation:** `[taxonomy.ts:resolveTaxonomyTermLabels]` rejects requests locally unless `ids` contains 1–100 unique positive safe integers.

## Side Effects

- Executes HTTP `GET`, `POST`, `PUT`, `PATCH`, and `DELETE` requests against the same-origin Axum backend, mutating server-persisted sessions, content entries, taxonomies, media assets, Studio templates, theme/plugin revisions, settings, and operational jobs.
- Sends `AbortSignal` cancellations through native `fetch` when callers abort stale lookups or previews.

## Verification

- TypeScript check & production bundle (`cms/admin/package.json`):
  ```powershell
  npm --prefix cms/admin run typecheck
  npm --prefix cms/admin run build
  ```
- Standards & i18n checks from workspace root (`cms/scripts/`):
  ```powershell
  python cms/scripts/check-standards.py
  node cms/scripts/check-i18n.mjs
  ```
