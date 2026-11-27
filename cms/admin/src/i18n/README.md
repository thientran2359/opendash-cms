# Admin Internationalization (`cms/admin/src/i18n/`)

## Overview

Site-wide UI localization, pluralization, and locale-aware date/number formatting for the OpenDash React Admin (`cms/admin/`), powered by runtime Gettext catalogs compiled from `.po`/`.mo` via `/api/i18n/admin` and `/api/i18n/admin/catalog`.

## Interface

- **Provider & Gettext Helpers (`index.tsx`):**
  - `I18nProvider`: Loads `AdminI18nBootstrap` and hydrates the active locale's runtime catalog (with `localStorage` ETag caching) before rendering children.
  - `useI18n()`: Returns `{ bootstrap, uiLocale, __, _x, _n, _nx, refreshBootstrap, setSection }`.
  - Module-level helpers: `__(msgid, params?)`, `_x(msgid, context, params?)`, `_n(singular, plural, count, params?)`, `_nx(singular, plural, count, context, params?)`, `getUiLocale()`, `getEffectiveBcp47()`.
- **CSP-Safe Plural Evaluator (`plural.ts`):**
  - `compilePluralForms(header)`: Bounded recursive-descent parser & evaluator for GNU `Plural-Forms` (zero `eval` / `new Function`).
- **Formatting Utilities (`format.ts`):**
  - `formatDateTime`, `formatNumber` bound to `getEffectiveBcp47()`.

## Verification

```powershell
node cms/scripts/check-i18n.mjs
npm --prefix cms/admin run typecheck
npm --prefix cms/admin run build
```
