# Admin Shell & Router (`cms/admin/src/shell/`)

## Overview

Application frame, authentication surface, RBAC enforcement, and hash URL router for the OpenDash React Admin (`cms/admin/`). Implements the 3-group navigation layout (`Content`, `Appearance`, `System` plus `Dashboard`) defined in [Admin Navigation & Edit Screen](../../../../docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md).

## Interface

- **Shell Layout (`AdminShell.tsx`):**
  - `[AdminShell.tsx:AdminShell]`: Renders guest login/setup view (`#auth-main`), fullscreen Studio container (`[data-testid="studio-shell"]`), or standard sidebar + topbar + workspace (`[data-testid="admin-layout"]`), along with mobile drawer, breadcrumbs, unsaved indicator, and RBAC forbidden notice (`[data-testid="rbac-forbidden"]`).
  - `[AdminShellMenu.tsx:buildMenuTree]`: Builds top-level and nested sidebar links dynamically from `cptTypes`, `cptTaxonomies`, and `user.role === 'admin'` (including `#/tools/redirects` under System → Tools).
  - Re-exports router symbols: `[router.ts:isRouteAllowed]`, `[router.ts:isViewAllowed]`, `[router.ts:parseAdminRoute]`, `[router.ts:parseAdminView]`, `[router.ts:AdminView]`, `[router.ts:ParsedRoute]`.
- **Hash Router & RBAC (`router.ts`):**
  - `[router.ts:parseAdminRoute]`: Parses `window.location.hash` and `pathname` (`/builder` -> `#/studio`) into `[router.ts:ParsedRoute]`, mapping legacy aliases (`#/content`, `#/sections/*`, `#/appearance/options`, `#/appearance/menus`, `#/contact/*`, `#/tools/restore`, `#/tools/404-log`, `#/builder?...`) to canonical views or `redirectHash`.
  - `[router.ts:parseAdminView]`, `[router.ts:isRouteAllowed]`, `[router.ts:isViewAllowed]`: Role-based (`'admin' | 'editor'`) and parameter-validity route guards.
  - Types: `[router.ts:AdminView]`, `[router.ts:StudioPanelId]`, `[router.ts:ParsedRoute]`.
- **Authentication & Media Seam (`AuthPanel.tsx`, `MediaRoute.tsx`):**
  - `[AuthPanel.tsx:AuthPanel]`: Initial Admin setup (`onSetup`), sign-in form (`onLogin`), signed-in identity + logout bar (`onLogout`), and status/error banner.
  - `[MediaRoute.tsx:MediaRoute]`, `[MediaRoute.tsx:MediaRouteProps]`: Route wrapper rendering `[../features/media/MediaLibraryPage.tsx:MediaLibraryPage]`.

## Invariants

- **RBAC Enforcement:** `editor` users are restricted by `[router.ts:isRouteAllowed]` and `[router.ts:isViewAllowed]` from `themes`, `plugins`, `users`, `tools`, `settings`, `patterns`, `studio`, taxonomy `definitions`/creation, and `#/studio/entry/pattern/:id`; `adminOnly` nav items are omitted from `visibleMenu`.
- **Draft Preservation Across Transient 401s:** `[AdminShell.tsx:AdminShell]` keeps initialized panel subtrees (`postsContent`, `pagesContent`, `contentContent`, `studioContent`, `usersContent`) mounted and hidden via `.hidden` / `aria-hidden` while logged out or `!routeForbidden` (in `App.tsx`, `studioContent` mounts lazily after first visit, `usersContent` mounts for admins, and CPT `SingleEditWorkspace` unmounts when leaving `view === 'cpt'` while preserving drafts in `editorDraftStore`).
- **Canonical Routing:** `#/builder` (including `?template=entry&type=pattern&entry=<id>`), `#/appearance/options`, `#/appearance/menus`, and entry/template `#/builder?...` URLs produce `redirectHash` targets in `#/studio` or `#/studio/entry/...`.
- **Single `<main>` Landmark:** `routeOwnsMain` switches `#admin-main` from `<main>` to `<div>` when mounting `PatternsPage`, `TemplateLibraryPage`, or `StudioWorkspace` so only one `<main>` landmark exists per view.

## Side Effects

- Synchronizes document `<title>` section text via `useI18n().setSection`.
- Locks `document.body.style.overflow = 'hidden'` and registers a `window` `keydown` (`Escape`) listener while the mobile navigation drawer (`mobileOpen`) is open.
- Emits navigation events through `onNavigateHash` without directly mutating `window.location` (managed by [App.tsx](../App.tsx)).

## Verification

- TypeScript check & Vite build (`cms/admin/package.json`):
  ```powershell
  npm --prefix cms/admin run typecheck
  npm --prefix cms/admin run build
  ```
- Standards, i18n, and navigation E2E checks (`cms/scripts/`):
  ```powershell
  python cms/scripts/check-standards.py
  node cms/scripts/check-i18n.mjs
  ```
