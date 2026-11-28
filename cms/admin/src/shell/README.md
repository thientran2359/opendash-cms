# Admin Shell & Router (`cms/admin/src/shell/`)

## Overview

Application frame, authentication surface, RBAC enforcement, and hash URL router for the OpenDash React Admin (`cms/admin/`). Implements the 3-group navigation layout (`Content`, `Appearance`, `System` plus `Dashboard`) defined in [Admin Navigation & Edit Screen](../../../../docs/product/ADMIN_NAVIGATION_AND_EDIT_SCREEN.md).

## Interface

- **Shell Layout (`AdminShell.tsx`):**
  - `[AdminShell.tsx:AdminShell]`: Renders guest login/setup view (`#auth-main`), fullscreen Studio container (`[data-testid="studio-shell"]`), or standard sidebar + topbar + workspace (`[data-testid="admin-layout"]`), along with pure-accordion expandable groups, modifier-safe `<a href>` leaf/child links, interactive profile entry in `[data-testid="sidebar-profile-link"]` + `[AuthPanel.tsx:AuthPanel]`, mobile drawer, breadcrumbs, unsaved indicator, and RBAC forbidden notice (`[data-testid="rbac-forbidden"]`).
  - `[AdminShellMenu.tsx:buildMenuTree]`: Builds top-level and nested sidebar links dynamically from `cptTypes`, `cptTaxonomies`, `optionsPages`, and `user.role === 'admin'` (`Themes` → `Customize Studio` [sub-links: `Site identity`, `Options`, `Menus`, active `appearance` Custom Options Pages, `Templates`] → `Patterns` incl. `Template library` under Appearance; `Tools` starting with `Redirects & 404`; `Settings` with `Homepage SEO`; `Users` admin-only without `My profile`).
  - Re-exports router symbols: `[router.ts:isRouteAllowed]`, `[router.ts:isViewAllowed]`, `[router.ts:parseAdminRoute]`, `[router.ts:parseAdminView]`, `[router.ts:AdminView]`, `[router.ts:ParsedRoute]`.
- **Hash Router & RBAC (`router.ts`, `studioRouter.ts`):**
  - `[router.ts:parseAdminRoute]` & `[studioRouter.ts:parseStudioRoute]`: Parses `window.location.hash` and `pathname` (`/builder` -> `#/studio`) into `[router.ts:ParsedRoute]`, supporting canonical Studio sub-routes (`#/studio/{identity,options,options/global/:section,options/theme[/:section],options/custom/:pageKey[/:groupId],menus,reading,templates[/:targetKey]}`), `#/contact`, `#/appearance/themes/install`, `#/content-structure/{content-types,taxonomies}`, while single-pass redirecting legacy aliases (`#/studio?panel=*`, `#/appearance/options`, `#/appearance/menus`, `#/builder?*`, `#/settings/contact*`, `#/themes/install`) to canonical hashes.
  - `[router.ts:parseAdminView]`, `[router.ts:isRouteAllowed]`, `[router.ts:isViewAllowed]`: Role-based (`'admin' | 'editor'`) and parameter-validity route guards.
  - Types: `[router.ts:AdminView]`, `[studioRouter.ts:StudioPanelId]`, `[studioRouter.ts:StudioOptionsScope]`, `[router.ts:ParsedRoute]`.
- **Authentication & Media Seam (`AuthPanel.tsx`, `MediaRoute.tsx`):**
  - `[AuthPanel.tsx:AuthPanel]`: Initial Admin setup (`onSetup`), sign-in form (`onLogin`), clickable signed-in identity linking to `#/users/profile` (`[data-testid="topbar-profile-link"]`) + logout bar (`onLogout`), and status/error banner.
  - `[MediaRoute.tsx:MediaRoute]`, `[MediaRoute.tsx:MediaRouteProps]`: Route wrapper rendering `[../features/media/MediaLibraryPage.tsx:MediaLibraryPage]`.

## Invariants

- **RBAC Enforcement:** `editor` users are restricted by `[router.ts:isRouteAllowed]` and `[router.ts:isViewAllowed]` from `contact`, `themes`, `plugins`, `users`, `tools`, `settings`, `patterns`, `studio`, taxonomy `definitions`/creation, and `#/studio/entry/pattern/:id`; `adminOnly` nav items are omitted from `visibleMenu` (so Editors never see `Appearance` or `System` groups).
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
  npm --prefix cms/admin run test:nav
  npm --prefix cms/admin run build
  ```
- Standards, i18n, and navigation E2E checks (`cms/scripts/`):
  ```powershell
  python cms/scripts/check-standards.py
  node cms/scripts/check-i18n.mjs
  ```
