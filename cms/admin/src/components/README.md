# Shared UI Components (`cms/admin/src/components/`)

## Overview

Reusable React presentation, form, and localization UI primitives shared across [shell](../shell/README.md), [builder](../builder/README.md), [studio](../studio/README.md), and [features](../features/README.md). Styled with Tailwind CSS (`4.1.17`) and built on React `18.3.1` ([package.json](../../package.json)).

## Interface

- **Admin Primitives (`AdminPrimitives.tsx`):**
  - `[AdminPrimitives.tsx:AdminButton]` (`AdminButtonProps`, variants `'primary' | 'secondary' | 'ghost' | 'danger'`, sizes `'sm' | 'md'`): Forward-ref button component.
  - `[AdminPrimitives.tsx:AdminBadge]` (`AdminBadgeProps`, tones `'neutral' | 'success' | 'warning' | 'danger'`): Inline status pill.
  - `[AdminPrimitives.tsx:AdminAlert]` (`AdminAlertProps`, tones `'info' | 'success' | 'warning' | 'error'`): Status/error banner.
  - `[AdminPrimitives.tsx:AdminTabs]`, `[AdminPrimitives.tsx:AdminTabPanel]` (`AdminTabItem`, `AdminTabsProps`, `AdminTabPanelProps`): Accessible tabbed container with optional count badges and dirty indicators.
- **Form Primitives (`FormPrimitives.tsx`):**
  - `[FormPrimitives.tsx:TextInput]`: Controlled `<input>` wrapper emitting string values via `onValueChange`.
  - `[FormPrimitives.tsx:FormGroup]`: Standard `.form-group` field container.
  - `[FormPrimitives.tsx:ZipUploadField]`: Labeled `.zip` file input (`accept=".zip"`).
  - `[FormPrimitives.tsx:SelectField]`, `[FormPrimitives.tsx:TextField]`, `[FormPrimitives.tsx:TextAreaField]`, `[FormPrimitives.tsx:CheckboxField]`: Labeled form controls with matching `htmlFor`/`id` bindings.
- **Multilingual Locale Switcher (`LocaleTabs.tsx`):**
  - `[LocaleTabs.tsx:LocaleTabs]`: `tablist` for switching active content/settings locale (`default` and `disabled` badges rendered from `AdminLocaleState`).

## Invariants

- **Zero Network Calls:** Components in `cms/admin/src/components/` never call `fetch` or API modules directly; state and persistence are owned by parent views.
- **WAI-ARIA Tab Semantics:** `[AdminPrimitives.tsx:AdminTabs]` manages `role="tablist"`, `role="tab"`, `aria-selected`, `aria-controls`, roving `tabIndex`, and keyboard navigation (`ArrowRight`, `ArrowLeft`, `Home`, `End`), skipping disabled tabs.
- **Alert Live-Region Mapping:** `[AdminPrimitives.tsx:AdminAlert]` defaults `role` to `'alert'` when `tone === 'error'` and `'status'` for all other tones unless explicitly overridden.
- **Multilingual Gate:** `[LocaleTabs.tsx:LocaleTabs]` returns `null` whenever `localeState.hasMultipleLocales` is `false`, keeping single-locale Admin screens uncluttered.

## Side Effects

- Manages DOM focus on tab buttons via `React.useRef` during keyboard navigation in `[AdminPrimitives.tsx:AdminTabs]`.
- Reads localized labels (`shell.locale_content`, `shell.locale_default`, `shell.locale_disabled`) via `[../i18n/index.tsx:useI18n]` in `[LocaleTabs.tsx:LocaleTabs]`.

## Verification

- TypeScript check & Vite build (`cms/admin/package.json`):
  ```powershell
  npm --prefix cms/admin run typecheck
  npm --prefix cms/admin run build
  ```
- i18n and standards verification (`cms/scripts/`):
  ```powershell
  node cms/scripts/check-i18n.mjs
  python cms/scripts/check-standards.py
  ```
