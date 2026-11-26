# Admin Internationalization (`cms/admin/src/i18n/`)

## Overview

Site-wide UI localization, pluralization, and locale-aware date/number formatting for the OpenDash React Admin (`cms/admin/`). Supports `en` and `vi` catalogs backed by a pre-auth server bootstrap from `/api/i18n/admin`. See [package.json](../../package.json) and [check-i18n.mjs](../../../scripts/check-i18n.mjs).

## Interface

- **Provider & Translation Hooks (`index.tsx`):**
  - `[index.tsx:I18nProvider]`: React context provider accepting `loadBootstrap: (signal?: AbortSignal) => Promise<AdminI18nBootstrap>`; defers rendering children until the initial bootstrap settles.
  - `[index.tsx:useI18n]`: Returns `{ bootstrap, uiLocale, t, tCount, refreshBootstrap, setSection }`; throws if called outside `I18nProvider`.
  - `[index.tsx:tStatic]`, `[index.tsx:tCountStatic]`: Non-React helpers reading module-level `currentBootstrap` for API modules and static helpers.
  - `[index.tsx:getUiLocale]`, `[index.tsx:getEffectiveBcp47]`: Active UI locale (`'en' | 'vi'`) and BCP-47 tag readers.
  - Types: `[index.tsx:UiLocale]`, `[index.tsx:TranslationKey]`, `[index.tsx:TranslationParams]`.
- **Formatting Utilities (`format.ts`):**
  - `[format.ts:formatDateTime]`, `[format.ts:formatNumber]`: Thin `Intl.DateTimeFormat` and `Intl.NumberFormat` wrappers bound to `[index.tsx:getEffectiveBcp47]`.
- **Static Locale Catalogs (`locales/en.ts`, `locales/vi.ts`):**
  - `[locales/en.ts:en]`: Canonical `as const` flat object defining all `TranslationKey` entries (2,144 keys).
  - `[locales/vi.ts:vi]`: Type-checked `Record<TranslationKey, string>` Vietnamese translation catalog.

## Invariants

- **Site-Wide Authority:** The Admin UI locale is determined strictly by the server's `AdminI18nBootstrap` response; browser `navigator.language` and per-user preferences never override it, and auth screens do not flash English/Vietnamese before `ready` becomes `true`.
- **Safe Fallback:** On initial bootstrap failure, `[index.tsx:I18nProvider]` falls back to `DEFAULT_BOOTSTRAP` (`en`, `en-US`, `ltr`).
- **Catalog & Interpolation Parity:** `locales/en.ts` and `locales/vi.ts` must maintain 1:1 key parity, identical `{{param}}` interpolation placeholders, paired `_one`/`_other` plural keys (`Intl.PluralRules`), and non-empty string values; no hardcoded Vietnamese characters are permitted outside `locales/vi.ts` (enforced by [check-i18n.mjs](../../../scripts/check-i18n.mjs)).
- **Request Serialization:** Both the mount `useEffect` (with an `AbortController`) and `refreshBootstrap` use an incrementing `sequence` ref (`mounted` ref checks in `refreshBootstrap`) so stale responses cannot overwrite newer state.

## Side Effects

- Calls `loadBootstrap` (normally `[../api/auth.ts:getAdminI18nBootstrap]`) on mount and when `refreshBootstrap()` is invoked.
- Updates module-level `currentBootstrap` for `[index.tsx:tStatic]` / `[index.tsx:formatDateTime]` / `[index.tsx:formatNumber]`.
- Mutates `document.documentElement.lang`, `document.documentElement.dir`, and `document.title` (`app.document_title`) whenever `bootstrap`, `t`, or `section` changes.

## Verification

- i18n parity & hardcoded-string audit (`cms/scripts/check-i18n.mjs`):
  ```powershell
  node cms/scripts/check-i18n.mjs
  ```
- TypeScript check & production bundle (`cms/admin/package.json`):
  ```powershell
  npm --prefix cms/admin run typecheck
  npm --prefix cms/admin run build
  ```
