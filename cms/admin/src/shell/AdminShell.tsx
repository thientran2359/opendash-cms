import React, { useEffect, useState } from 'react';
import { useI18n } from '../i18n/index.tsx';
import type { AuthUser } from '../types.ts';
import type { OwnedContentTypeWithSettings } from '../api/content.ts';
import { AuthPanel } from './AuthPanel.tsx';
import {
  isRouteAllowed,
  isViewAllowed,
  parseAdminRoute,
  parseAdminView,
  type AdminView,
  type ParsedRoute,
} from './router.ts';

export {
  isRouteAllowed,
  isViewAllowed,
  parseAdminRoute,
  parseAdminView,
  type AdminView,
  type ParsedRoute,
};

import {
  buildMenuTree,
  NavIcon,
  type NavigationGroup,
  type OptionsPageMenuItem,
} from './AdminShellMenu';

interface AdminShellProps {
  user: AuthUser | null;
  authUsername: string;
  authPassword: string;
  statusMessage: string;
  isError: boolean;
  isProcessing: boolean;
  onUsernameChange: (value: string) => void;
  onPasswordChange: (value: string) => void;
  onSetup: () => void;
  onLogin: () => void;
  onLogout: () => void;
  route: ParsedRoute;
  routeForbidden: boolean;
  cptTypes: OwnedContentTypeWithSettings[];
  cptTaxonomies?: Record<string, Array<{ key: string; label: string }>>;
  optionsPages?: OptionsPageMenuItem[];
  onNavigateHash: (hash: string) => void;
  hasUnsavedChanges: boolean;
  devMode?: boolean;
  devTheme?: string | null;
  devPlugins?: string[];
  devSyncing?: boolean;
  onSyncDevManifest?: () => void;
  contentContent: React.ReactNode;
  schemaSettingsContent: React.ReactNode;
  contentStructureContent?: React.ReactNode;
  optionsPageContent?: React.ReactNode;
  postsContent: React.ReactNode;
  pagesContent: React.ReactNode;
  patternsContent: React.ReactNode;
  mediaContent: React.ReactNode;
  taxonomyContent: React.ReactNode;
  dashboardContent: React.ReactNode;
  pluginsContent: React.ReactNode;
  usersContent: React.ReactNode;
  profileContent: React.ReactNode;
  toolsContent: React.ReactNode;
  settingsContent: React.ReactNode;
  themesContent: React.ReactNode;
  libraryContent: React.ReactNode;
  studioContent?: React.ReactNode;
}

export const AdminShell: React.FC<AdminShellProps> = ({
  user, authUsername, authPassword, statusMessage, isError, isProcessing,
  onUsernameChange, onPasswordChange, onSetup, onLogin, onLogout,
  route, routeForbidden, cptTypes, cptTaxonomies, optionsPages = [], onNavigateHash, hasUnsavedChanges,
  devMode = false, devTheme = null, devPlugins = [], devSyncing = false, onSyncDevManifest,
  contentContent, schemaSettingsContent, contentStructureContent, optionsPageContent,
  postsContent, pagesContent, patternsContent, mediaContent, taxonomyContent, dashboardContent,
  pluginsContent, usersContent, profileContent, toolsContent, settingsContent, themesContent,
  libraryContent, studioContent,
}) => {
  const { __, _x, setSection } = useI18n();
  const [mobileOpen, setMobileOpen] = useState(false);
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);

  const menuTree = buildMenuTree(cptTypes, user?.role === 'admin', cptTaxonomies, optionsPages);
  const activeParentId =
    route.view === 'cpt' && route.typeKey
      ? `cpt:${route.typeKey}`
      : route.view === 'content-structure'
        ? 'schema'
        : route.view === 'options-page'
          ? (route.typeKey ? `options:${route.typeKey}` : 'schema')
      : route.view === 'taxonomy'
        ? (route.typeKey
          ? `cpt:${route.typeKey}`
          : route.taxonomyMode === 'definitions' || (route.taxonomyKind !== 'category' && route.taxonomyKind !== 'tag')
            ? 'schema' : 'posts')
        : route.view === 'profile'
          ? 'users'
          : route.view === 'studio' || route.view === 'studio-entry'
            ? 'studio'
            : route.view === 'themes'
              ? 'themes'
              : route.view === 'settings' && route.sub === 'content-types'
                ? 'schema'
                : route.view === 'settings' && route.sub === 'contact'
                  ? 'contact'
                  : route.view;

  useEffect(() => {
    setOpenMenuId(activeParentId);
  }, [activeParentId]);

  useEffect(() => {
    if (!mobileOpen) return;
    const originalOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    const onEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setMobileOpen(false);
    };
    window.addEventListener('keydown', onEscape);
    return () => {
      window.removeEventListener('keydown', onEscape);
      document.body.style.overflow = originalOverflow;
    };
  }, [mobileOpen]);

  const navigate = (hash: string) => {
    if (!user) return;
    onNavigateHash(hash);
    setMobileOpen(false);
  };

  const visibleMenu = menuTree.filter(item => !item.adminOnly || user?.role === 'admin');
  const groupOrder: { id: string; label: string | null; match: NavigationGroup | null }[] = [
    { id: 'dashboard', label: null, match: null },
    { id: 'content', label: __("Content"), match: 'content' },
    { id: 'appearance', label: __("Appearance"), match: 'appearance' },
    { id: 'system', label: __("System"), match: 'system' },
  ];
  const menuGroups = groupOrder.map(group => ({
    ...group,
    items: visibleMenu.filter(item => (item.group ?? null) === group.match),
  })).filter(group => group.items.length > 0);
  const activeParent = visibleMenu.find(m => m.id === activeParentId);
  const activeGroupLabel = groupOrder.find(g => g.match === (activeParent?.group ?? null))?.label;
  // Legacy deep links keep rendering their canonical feature; highlighting
  // follows the resolved route, not a potentially obsolete URL spelling.
  const selectedNavHash =
    route.view === 'content-structure' ? `#/content-structure/${route.sub}` :
    route.view === 'options-page' && route.typeKey ? `#/options/${encodeURIComponent(route.typeKey)}` :
    route.view === 'themes' ? `#/appearance/${route.sub}` :
    route.view === 'taxonomy'
      ? (route.typeKey && route.taxonomyKind
        ? `#/cpt/${encodeURIComponent(route.typeKey)}/taxonomy/${encodeURIComponent(route.taxonomyKind)}`
        : route.taxonomyMode === 'definitions' || (route.taxonomyKind !== 'category' && route.taxonomyKind !== 'tag')
          ? '#/content-structure/taxonomies' : `#/posts/${route.taxonomyKind === 'tag' ? 'tags' : 'categories'}`) :
    route.view === 'profile' ? '#/users/profile' :
    route.view === 'studio' ? '#/studio' :
    route.view === 'tools' ? (route.sub === '404-log' ? '#/tools/redirects' : `#/tools/${route.sub}`) :
    route.view === 'settings'
      ? (route.sub === 'content-types' ? '#/content-structure/content-types' : `#/settings/${route.sub}`) :
    route.view === 'media' ? '#/media' :
    route.view === 'plugins' ? (route.sub === 'install' ? '#/plugins/install' : '#/plugins') :
    route.view === 'users' ? (route.sub === 'new' ? '#/users/new' : '#/users') :
    route.rawHash.split('?')[0];
  const activeChild = activeParent?.children?.find(child => child.hash === selectedNavHash);
  const subLabels: Record<string, string> = {
    list: __("List"), new: __("Add new"), edit: _x("Edit", "shell.edit"), library: __("Library"),
    general: __("General"), multilingual: __("Multilingual"), reading: __("Homepage & posts"), permalinks: __("Permalinks"), seo: __("SEO"), contact: __("Contact"), smtp: __("Email notifications (SMTP)"),
    'content-types': __("Content types"), taxonomies: __("Taxonomies"), 'field-groups': __("Field groups"), 'options-pages': __("Options pages"), view: __("Options page"),
    themes: __("Themes"), menus: __("Menus"),
    definitions: __("Custom taxonomies"), terms: _x("Terms", "shell.terms"),
    options: __("Theme options"), health: __("System health"),
    'media-reconcile': __("Media reconciliation"), backup: __("Backup & restore"),
    redirects: __("Redirects & 404"), '404-log': __("Redirects & 404 Log"),
    install: __("Settings"), profile: __("My profile"),
  };
  const pageTitle = routeForbidden
    ? __("403 Access denied")
    : activeChild?.label ?? (route.sub === 'edit' ? _x("Edit", "shell.edit") : subLabels[route.sub]) ?? activeParent?.label ?? _x("Dashboard", "shell.dashboard");
  const documentSection = user ? pageTitle : __("Admin sign in");
  useEffect(() => { setSection(documentSection); }, [documentSection, setSection]);
  const breadcrumbParts = [
    __("Administration"),
    activeGroupLabel,
    activeParent?.label,
    ...(pageTitle !== activeParent?.label ? [pageTitle] : []),
  ].filter((part): part is string => Boolean(part));
  const breadcrumb = breadcrumbParts
    .filter((part, index) => index === 0 || part !== breadcrumbParts[index - 1])
    .join(' / ');
  const customCptTypes = cptTypes.filter(
    ct => ct.descriptor.key !== 'post' && ct.descriptor.key !== 'page' && ct.descriptor.key !== 'section' && ct.descriptor.key !== 'pattern',
  );
  const isStudioView = Boolean(
    user && !routeForbidden && (route.view === 'studio' || route.view === 'studio-entry'),
  );
  // PatternsPage, TemplateLibraryPage, and Studio own their own <main> landmark.
  const routeOwnsMain = Boolean(
    user && !routeForbidden && (
      route.view === 'patterns' ||
      route.view === 'studio' ||
      route.view === 'studio-entry' ||
      (route.view === 'themes' && route.sub === 'library')
    ),
  );
  // Keep the Builder/Editor subtree mounted through a transient 401/login cycle so
  // its quarantined, unsaved drafts survive same-account reauthentication.
  // The admin layout is hidden while logged out or inside fullscreen Studio.
  const PageContainerTag: 'main' | 'div' = routeOwnsMain ? 'div' : 'main';
  const postsLegacy = route.sub !== 'list' ? ' od-legacy-scope' : '';
  const pagesLegacy = route.sub !== 'list' ? ' od-legacy-scope' : '';
  const cptLegacy = route.sub !== 'list' ? ' od-legacy-scope' : '';

  return (
    <div className="od-root min-h-screen">
      {!user && (
        <main id="auth-main" className="od-auth-layout flex min-h-screen items-center justify-center px-4 py-12">
          <section className="od-auth-card w-full max-w-md rounded-2xl p-7 shadow-xs sm:p-9">
            <div className="od-logo od-logo-large" aria-hidden="true">O</div>
            <div className="mt-4 text-xs font-semibold uppercase tracking-widest text-od-accent">OpenDash CMS</div>
            <h1 className="mt-1.5 text-2xl font-bold tracking-tight text-zinc-950">{__("Admin sign in")}</h1>
            <p className="mb-6 mt-1.5 text-sm text-zinc-600">{__("Manage website content and design in one workspace.")}</p>
            <AuthPanel
              user={user} authUsername={authUsername} authPassword={authPassword}
              statusMessage={statusMessage} isError={isError} isProcessing={isProcessing}
              onUsernameChange={onUsernameChange} onPasswordChange={onPasswordChange}
              onSetup={onSetup} onLogin={onLogin} onLogout={onLogout}
            />
          </section>
        </main>
      )}

      {studioContent && (
        <div
          className={isStudioView ? 'h-screen w-screen overflow-hidden bg-zinc-100' : 'hidden'}
          data-testid="studio-shell"
          aria-hidden={!isStudioView}
        >
          {studioContent}
        </div>
      )}

      <div className={user && !isStudioView ? 'od-layout' : 'hidden'} data-testid="admin-layout" aria-hidden={!user || isStudioView}>
        {mobileOpen && <button type="button" className="od-drawer-overlay" aria-label={__("Close menu")} onClick={() => setMobileOpen(false)} />}
        <aside id="admin-navigation" className={'od-sidebar ' + (mobileOpen ? 'od-sidebar-open' : '')} aria-label={__("Admin navigation")}>
          <div className="od-brand flex items-center gap-2.5 px-4 py-3.5">
            <div className="od-logo" aria-hidden="true">O</div>
            <div className="min-w-0">
              <div className="text-sm font-bold tracking-tight text-zinc-950">OpenDash</div>
              <div className="text-[11px] text-zinc-500">{__("Content Management")}</div>
            </div>
          </div>
          <nav className="od-nav overflow-y-auto px-2.5 py-3" aria-label={__("Admin sections")}>
            <div className="flex flex-col gap-0.5">
              {menuGroups.map(group => <div key={group.id} className="od-nav-group" data-nav-group={group.id}>
                {group.label && <p className="od-nav-group-title">{group.label}</p>}
                {group.items.map(item => {
                  const isActiveParent = activeParentId === item.id;
                  const isExpanded = openMenuId === item.id || isActiveParent;
                  const visibleChildren = (item.children ?? []).filter(
                    c => !c.adminOnly || user?.role === 'admin',
                  );
                  return (
                    <div key={item.id} className="mb-0.5">
                      <button
                        type="button"
                        className={'od-nav-link w-full justify-between ' + (isActiveParent ? 'od-nav-link-active' : '')}
                        data-menu-id={item.id}
                        aria-expanded={visibleChildren.length > 0 ? isExpanded : undefined}
                        disabled={item.disabled}
                        title={item.disabled ? __("Unavailable — will be enabled when the feature is complete") : undefined}
                        onClick={() => {
                          if (visibleChildren.length > 0) {
                            setOpenMenuId(isExpanded ? null : item.id);
                          }
                          if (!item.disabled) navigate(item.hash);
                        }}
                      >
                        <span className="flex min-w-0 items-center gap-2">
                          <NavIcon name={item.icon} />
                          <span className="truncate">{item.label}</span>
                          {item.disabled && <span className="od-nav-coming-soon">{__("Coming soon")}</span>}
                        </span>
                        {visibleChildren.length > 0 && (
                          <span className="text-[11px] text-zinc-400" aria-hidden="true">
                            {isExpanded ? '▾' : '▸'}
                          </span>
                        )}
                      </button>
                      {visibleChildren.length > 0 && isExpanded && (
                        <div className="mt-0.5 ml-5 flex flex-col gap-0.5 border-l border-zinc-200 pl-2.5">
                          {visibleChildren.map(sub => {
                            const isSubActive = selectedNavHash === sub.hash;
                            return (
                              sub.disabled
                                ? <span key={sub.hash} className="od-nav-sub-disabled" aria-disabled="true">{__("{{label}} · Coming soon", { label: sub.label })}</span>
                                : <a key={sub.hash} href={sub.hash}
                                  onClick={e => { e.preventDefault(); navigate(sub.hash); }}
                                  aria-current={isSubActive ? 'page' : undefined}
                                  className={
                                  'rounded-md px-2 py-1 text-xs no-underline transition-colors ' +
                                  (isSubActive
                                    ? 'bg-zinc-200/80 font-semibold text-zinc-950'
                                    : 'text-zinc-600 hover:bg-zinc-200/50 hover:text-zinc-950')
                                  }>{sub.label}</a>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>)}
            </div>
          </nav>
          <div className="od-sidebar-bottom px-4 py-2.5 text-[11px]">{__("OpenDash · Admin")}</div>
        </aside>

        <div className="od-workspace min-w-0">
          <header className="od-topbar flex flex-wrap items-center justify-between gap-3 px-4 py-3 sm:px-6">
            <div className="flex min-w-0 items-center gap-3">
              <button type="button" className="od-menu-toggle" aria-label={__("Open menu")} aria-controls="admin-navigation" aria-expanded={mobileOpen} onClick={() => setMobileOpen(!mobileOpen)}>
                <span aria-hidden="true">☰</span>
              </button>
              <div className="min-w-0">
                <p className="text-[11px] font-medium text-zinc-500" data-testid="breadcrumb">
                  {breadcrumb}
                </p>
                <h1 className="truncate text-lg font-semibold tracking-tight text-zinc-950">{pageTitle}</h1>
              </div>
            </div>
            <div className="od-topbar-actions flex flex-wrap items-center gap-2.5">
              {devMode && (
                <div className="flex flex-wrap items-center gap-2" data-testid="dev-mode-banner">
                  <span
                    className="inline-flex items-center rounded-md border border-amber-300 bg-amber-100 px-2 py-0.5 text-xs font-bold tracking-wide text-amber-900"
                    title={[devTheme ? `Theme: ${devTheme}` : null, devPlugins.length > 0 ? `Plugins: ${devPlugins.join(', ')}` : null].filter(Boolean).join(' · ') || undefined}
                  >
                    {__("DEV MODE")}
                  </span>
                  {user?.role === 'admin' && onSyncDevManifest && (
                    <button
                      type="button"
                      data-testid="dev-sync-button"
                      disabled={devSyncing}
                      onClick={onSyncDevManifest}
                      className="inline-flex h-7 items-center rounded-md border border-amber-300 bg-white px-2.5 text-xs font-medium text-amber-900 transition-colors hover:bg-amber-50 disabled:opacity-50"
                    >
                      {devSyncing ? __("Syncing…") : __("Sync Dev Manifest")}
                    </button>
                  )}
                </div>
              )}
              {hasUnsavedChanges && <span className="od-unsaved-label" role="status">{__("● Unsaved changes")}</span>}
              {user && <AuthPanel
                user={user} authUsername={authUsername} authPassword={authPassword}
                statusMessage={statusMessage} isError={isError} isProcessing={isProcessing}
                onUsernameChange={onUsernameChange} onPasswordChange={onPasswordChange}
                onSetup={onSetup} onLogin={onLogin} onLogout={onLogout}
              />}
            </div>
          </header>

          <PageContainerTag id="admin-main" className="od-page-container px-4 py-5 sm:px-6 sm:py-6">
            {user && routeForbidden && (
              <section className="od-admin-card border-rose-200 bg-rose-50" data-testid="rbac-forbidden" role="alert">
                <h2 className="text-lg font-semibold text-rose-900">{__("403 — Access denied")}</h2>
                <p className="mt-2 text-sm text-rose-800">
                  {__("Your account does not have permission to access {{path}}.", { path: route.rawHash })}
                </p>
                <button
                  type="button"
                  onClick={() => navigate('#/dashboard')}
                  className="mt-4 inline-flex h-9 items-center rounded-md bg-zinc-900 px-3.5 text-sm font-medium text-white hover:bg-zinc-800"
                >
                  {__("Back to dashboard")}
                </button>
              </section>
            )}

            {user && !routeForbidden && route.view === 'dashboard' && (
              <div className="od-dashboard space-y-6" id="dashboard-main">
                <div className="od-welcome rounded-xl px-5 py-4 sm:px-6 sm:py-5">
                  <div className="text-[11px] font-semibold uppercase tracking-wider text-od-accent">{_x("Dashboard", "shell.dashboard_heading")}</div>
                  <h2 className="mt-1 text-xl font-semibold tracking-tight text-zinc-950">{__("Hello, {{username}}", { username: user.username })}</h2>
                  <p className="mt-1 max-w-2xl text-sm text-zinc-600">{__("Choose a section from the sidebar to manage content and configure the system.")}</p>
                </div>
                {dashboardContent}
                {customCptTypes.length > 0 && (
                  <section className="space-y-3" aria-labelledby="dashboard-cpt-heading" data-testid="dashboard-cpt-shortcuts">
                    <div>
                      <h2 id="dashboard-cpt-heading" className="text-base font-semibold text-zinc-950">{__("Custom content types")}</h2>
                      <p className="mt-0.5 text-sm text-zinc-600">{__("Shortcuts to manage active custom content types.")}</p>
                    </div>
                    <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
                      {customCptTypes.map(ct => (
                        <article key={ct.descriptor.key} className="flex items-center justify-between gap-3 rounded-xl border border-zinc-200 bg-white p-4">
                          <div className="min-w-0">
                            <h3 className="truncate text-sm font-semibold text-zinc-950">{ct.descriptor.label}</h3>
                            <p className="mt-0.5 truncate text-xs text-zinc-500">/{ct.descriptor.key}</p>
                          </div>
                          <div className="flex shrink-0 items-center gap-2">
                            <a
                              href={`#/cpt/${ct.descriptor.key}`}
                              onClick={e => { e.preventDefault(); navigate(`#/cpt/${ct.descriptor.key}`); }}
                              className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-700 hover:bg-zinc-50"
                            >
                              {__("List")}
                            </a>
                            <a
                              href={`#/cpt/${ct.descriptor.key}/new`}
                              onClick={e => { e.preventDefault(); navigate(`#/cpt/${ct.descriptor.key}/new`); }}
                              className="inline-flex h-8 items-center rounded-md bg-od-accent px-2.5 text-xs font-medium text-white hover:bg-od-accent-hover"
                            >
                              {__("+ New")}
                            </a>
                          </div>
                        </article>
                      ))}
                    </div>
                  </section>
                )}
              </div>
            )}

            {!routeForbidden && (
              <>
                <section className={user && route.view === 'posts' ? `od-editor-panel${postsLegacy}` : 'hidden'} aria-label={__("Posts")} aria-hidden={!user || route.view !== 'posts'}>{postsContent}</section>
                <section className={user && route.view === 'pages' ? `od-editor-panel${pagesLegacy}` : 'hidden'} aria-label={__("Pages")} aria-hidden={!user || route.view !== 'pages'}>{pagesContent}</section>
                {user && route.view === 'media' && mediaContent}
                {user && route.view === 'taxonomy' && <div className="od-admin-card min-w-0 max-w-full">{taxonomyContent}</div>}
                {user?.role === 'admin' && route.view === 'themes' && route.sub === 'themes' && <div className="od-admin-card min-w-0">{themesContent}</div>}
                {user?.role === 'admin' && route.view === 'themes' && route.sub === 'library' && <div className="od-legacy-scope">{libraryContent}</div>}
                {user?.role === 'admin' && route.view === 'patterns' && <div className="od-legacy-scope">{patternsContent}</div>}

                <section className={user && route.view === 'cpt' ? `od-editor-panel${cptLegacy}` : 'hidden'} aria-label={__("Custom content")} aria-hidden={!user || route.view !== 'cpt'}>
                  <div id="content-main">{contentContent}</div>
                </section>

                {user?.role === 'admin' && route.view === 'plugins' && <div className="od-admin-card">{pluginsContent}</div>}
                <section className={user?.role === 'admin' && route.view === 'users' ? 'od-admin-card min-w-0 max-w-full overflow-x-clip' : 'hidden'} aria-label={__("Users")} aria-hidden={user?.role !== 'admin' || route.view !== 'users'}>
                  {usersContent}
                </section>
                {user && route.view === 'profile' && <div className="od-admin-card min-w-0">{profileContent}</div>}
                {user?.role === 'admin' && route.view === 'tools' && <div className="od-admin-card min-w-0">{toolsContent}</div>}
                {user?.role === 'admin' && route.view === 'content-structure' && (
                  <div className="od-admin-card min-w-0">
                    {contentStructureContent ?? schemaSettingsContent}
                  </div>
                )}
                {user && route.view === 'options-page' && (
                  <div className="od-admin-card min-w-0">{optionsPageContent}</div>
                )}
                {user?.role === 'admin' && route.view === 'settings' && (
                  <div className="od-admin-card min-w-0">
                    {route.sub === 'content-types' ? schemaSettingsContent : settingsContent}
                  </div>
                )}
              </>
            )}
          </PageContainerTag>
        </div>
      </div>
    </div>
  );
};
