import React, { useEffect, useState } from 'react';
import { useI18n } from '../i18n/index.tsx';
import type { AuthUser } from '../types.ts';
import type { OwnedContentType } from '../features/content/types.ts';
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

interface SubMenuItem {
  label: string;
  hash: string;
  adminOnly?: boolean;
  disabled?: boolean;
}

type NavigationGroup = 'content' | 'appearance' | 'system';

interface ParentMenuItem {
  id: string;
  label: string;
  icon: string;
  hash: string;
  group?: NavigationGroup;
  adminOnly?: boolean;
  disabled?: boolean;
  children?: SubMenuItem[];
}

function buildMenuTree(
  t: ReturnType<typeof useI18n>['t'],
  cptTypes: OwnedContentType[],
  isAdmin: boolean,
  cptTaxonomies: Record<string, Array<{ key: string; label: string }>> = {},
): ParentMenuItem[] {
  const customTypes = cptTypes.filter(
    ct => ct.descriptor.key !== 'post' && ct.descriptor.key !== 'page' && ct.descriptor.key !== 'section' && ct.descriptor.key !== 'pattern',
  );
  return [
    { id: 'dashboard', label: t('shell.dashboard'), icon: 'dashboard', hash: '#/dashboard' },
    {
      id: 'posts',
      label: t('shell.posts'),
      icon: 'posts',
      hash: '#/posts',
      group: 'content',
      children: [
        { label: t('shell.all_posts'), hash: '#/posts' },
        { label: t('shell.new_post'), hash: '#/posts/new' },
        { label: t('shell.categories'), hash: '#/posts/categories' },
        { label: t('shell.tags'), hash: '#/posts/tags' },
      ],
    },
    {
      id: 'pages',
      label: t('shell.pages'),
      icon: 'pages',
      hash: '#/pages',
      group: 'content',
      children: [
        { label: t('shell.all_pages'), hash: '#/pages' },
        { label: t('shell.new_page'), hash: '#/pages/new' },
      ],
    },
    ...customTypes.map(ct => ({
      id: `cpt:${ct.descriptor.key}`,
      label: ct.descriptor.label,
      icon: 'content',
      hash: `#/cpt/${ct.descriptor.key}`,
      group: 'content' as const,
      children: [
        { label: t('shell.all_custom', { label: ct.descriptor.label }), hash: `#/cpt/${ct.descriptor.key}` },
        { label: t('shell.new'), hash: `#/cpt/${ct.descriptor.key}/new` },
        ...(cptTaxonomies[ct.descriptor.key] ?? []).map(tax => ({
          label: tax.label,
          hash: `#/cpt/${encodeURIComponent(ct.descriptor.key)}/taxonomy/${encodeURIComponent(tax.key)}`,
        })),
      ],
    })),
    {
      id: 'media',
      label: t('shell.media'),
      icon: 'media',
      hash: '#/media',
      group: 'content',
    },
    {
      id: 'contact',
      label: t('shell.contact'),
      icon: 'mail',
      hash: '#/settings/contact',
      group: 'content',
      adminOnly: true,
    },
    {
      id: 'studio',
      label: t('shell.studio'),
      icon: 'builder',
      hash: '#/studio',
      group: 'appearance',
      adminOnly: true,
    },
    {
      id: 'themes',
      label: t('shell.themes'),
      icon: 'themes',
      hash: '#/appearance/themes',
      group: 'appearance',
      adminOnly: true,
      children: [
        { label: t('shell.installed_themes'), hash: '#/appearance/themes' },
        { label: t('shell.install_theme'), hash: '#/appearance/library' },
      ],
    },
    {
      id: 'patterns',
      label: t('shell.patterns'),
      icon: 'content',
      hash: '#/patterns',
      group: 'appearance',
      adminOnly: true,
      children: [
        { label: t('shell.all_patterns'), hash: '#/patterns' },
        { label: t('shell.new_pattern'), hash: '#/patterns/new' },
      ],
    },
    {
      id: 'schema',
      label: t('shell.schema'),
      icon: 'schema',
      hash: '#/appearance/content-types',
      group: 'system',
      adminOnly: true,
      children: [
        { label: t('shell.content_types'), hash: '#/appearance/content-types' },
        { label: t('shell.taxonomy_definitions'), hash: '#/taxonomy/definitions' },
      ],
    },
    {
      id: 'plugins',
      label: t('shell.plugins'),
      icon: 'plugins',
      hash: '#/plugins',
      group: 'system',
      adminOnly: true,
      children: [
        { label: t('shell.installed_plugins'), hash: '#/plugins' },
        { label: t('shell.install_plugin'), hash: '#/plugins/install' },
      ],
    },
    {
      id: 'users',
      label: t('shell.users'),
      icon: 'users',
      hash: isAdmin ? '#/users' : '#/users/profile',
      group: 'system',
      children: [
        { label: t('shell.all_users'), hash: '#/users', adminOnly: true },
        { label: t('shell.new_user'), hash: '#/users/new', adminOnly: true },
        { label: t('shell.profile'), hash: '#/users/profile' },
      ],
    },
    {
      id: 'tools',
      label: t('shell.tools'),
      icon: 'tools',
      hash: '#/tools/backup',
      group: 'system',
      adminOnly: true,
      children: [
        { label: t('shell.backup'), hash: '#/tools/backup' },
        { label: t('shell.media_reconcile'), hash: '#/tools/media-reconcile' },
        { label: t('shell.health'), hash: '#/tools/health' },
      ],
    },
    {
      id: 'settings',
      label: t('shell.settings'),
      icon: 'settings',
      hash: '#/settings/general',
      group: 'system',
      adminOnly: true,
      children: [
        { label: t('shell.general'), hash: '#/settings/general' },
        { label: t('shell.multilingual'), hash: '#/settings/multilingual', adminOnly: true },
        { label: t('shell.reading'), hash: '#/settings/reading' },
        { label: t('shell.permalinks'), hash: '#/settings/permalinks' },
        { label: t('shell.seo'), hash: '#/settings/seo' },
        { label: t('shell.smtp'), hash: '#/settings/smtp' },
      ],
    },
  ];
}

function NavIcon({ name }: { name: string }) {
  const paths: Record<string, React.ReactNode> = {
    dashboard: <><rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /><rect x="14" y="14" width="7" height="7" rx="1" /></>,
    posts: <><path d="M8 3h9l4 4v14H8z" /><path d="M17 3v5h4M11 12h7M11 16h7M3 7v14" /></>,
    pages: <><path d="M5 3h10l4 4v14H5zM15 3v5h4M9 12h6M9 16h6" /></>,
    content: <><path d="M4 5h16v14H4zM8 9h3v3H8zM14 9h3M14 13h3M8 16h9" /></>,
    media: <><rect x="3" y="4" width="18" height="16" rx="2" /><circle cx="8" cy="9" r="1.5" /><path d="m4 17 5-5 4 3 3-4 5 6" /></>,
    mail: <><rect x="3" y="5" width="18" height="14" rx="2" /><path d="m3 7 9 6 9-6" /></>,
    builder: <><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M9 4v16M9 11h12" /></>,
    themes: <><path d="M12 3 3 8l9 5 9-5-9-5ZM3 12l9 5 9-5M3 16l9 5 9-5" /></>,
    palette: <><circle cx="12" cy="12" r="9" /><circle cx="9" cy="10" r="1" /><circle cx="14" cy="9" r="1" /><circle cx="16" cy="13" r="1" /><circle cx="10" cy="15" r="1" /></>,
    schema: <><ellipse cx="12" cy="5" rx="8" ry="3" /><path d="M4 5v6c0 1.66 3.58 3 8 3s8-1.34 8-3V5M4 11v6c0 1.66 3.58 3 8 3s8-1.34 8-3v-6" /></>,
    plugins: <><path d="M9 3h6v5h5v5h-5v8H9v-8H4V8h5z" /></>,
    users: <><circle cx="9" cy="8" r="3" /><path d="M3 21v-2a6 6 0 0 1 12 0v2M17 5a3 3 0 0 1 0 6M18 15a5 5 0 0 1 3 5v1" /></>,
    tools: <><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" /></>,
    settings: <><circle cx="12" cy="12" r="3" /><path d="m10 2-1 3-3 1-3-1-2 4 3 2v2l-3 2 2 4 3-1 3 1 1 3h4l1-3 3-1 3 1 2-4-3-2v-2l3-2-2-4-3 1-3-1-1-3z" /></>,
  };
  return <svg aria-hidden="true" width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" strokeLinecap="round">{paths[name] ?? paths.content}</svg>;
}

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
  cptTypes: OwnedContentType[];
  cptTaxonomies?: Record<string, Array<{ key: string; label: string }>>;
  onNavigateHash: (hash: string) => void;
  hasUnsavedChanges: boolean;
  contentContent: React.ReactNode;
  schemaSettingsContent: React.ReactNode;
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
  route, routeForbidden, cptTypes, cptTaxonomies, onNavigateHash, hasUnsavedChanges,
  contentContent, schemaSettingsContent,
  postsContent, pagesContent, patternsContent, mediaContent, taxonomyContent, dashboardContent,
  pluginsContent, usersContent, profileContent, toolsContent, settingsContent, themesContent,
  libraryContent, studioContent,
}) => {
  const { t, setSection } = useI18n();
  const [mobileOpen, setMobileOpen] = useState(false);
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);

  const menuTree = buildMenuTree(t, cptTypes, user?.role === 'admin', cptTaxonomies);
  const activeParentId =
    route.view === 'cpt' && route.typeKey
      ? `cpt:${route.typeKey}`
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
    { id: 'content', label: t('shell.group_content'), match: 'content' },
    { id: 'appearance', label: t('shell.group_appearance'), match: 'appearance' },
    { id: 'system', label: t('shell.group_system'), match: 'system' },
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
    route.view === 'themes' ? `#/appearance/${route.sub}` :
    route.view === 'taxonomy'
      ? (route.typeKey && route.taxonomyKind
        ? `#/cpt/${encodeURIComponent(route.typeKey)}/taxonomy/${encodeURIComponent(route.taxonomyKind)}`
        : route.taxonomyMode === 'definitions' || (route.taxonomyKind !== 'category' && route.taxonomyKind !== 'tag')
          ? '#/taxonomy/definitions' : `#/posts/${route.taxonomyKind === 'tag' ? 'tags' : 'categories'}`) :
    route.view === 'profile' ? '#/users/profile' :
    route.view === 'studio' ? '#/studio' :
    route.view === 'tools' ? `#/tools/${route.sub}` :
    route.view === 'settings'
      ? (route.sub === 'content-types' ? '#/appearance/content-types' : `#/settings/${route.sub}`) :
    route.view === 'media' ? '#/media' :
    route.view === 'plugins' ? (route.sub === 'install' ? '#/plugins/install' : '#/plugins') :
    route.view === 'users' ? (route.sub === 'new' ? '#/users/new' : '#/users') :
    route.rawHash.split('?')[0];
  const activeChild = activeParent?.children?.find(child => child.hash === selectedNavHash);
  const subLabels: Record<string, string> = {
    list: t('shell.list'), new: t('shell.new'), edit: t('shell.edit'), library: t('shell.library'),
    general: t('shell.general'), multilingual: t('shell.multilingual'), reading: t('shell.reading'), permalinks: t('shell.permalinks'), seo: t('shell.seo'), contact: t('shell.contact'), smtp: t('shell.smtp'),
    'content-types': t('shell.content_types'), themes: t('shell.themes'), menus: t('shell.menus'),
    definitions: t('shell.taxonomy_definitions'), terms: t('shell.terms'),
    options: t('shell.theme_options'), health: t('shell.health'),
    'media-reconcile': t('shell.media_reconcile'), backup: t('shell.backup'),
    install: t('shell.settings'), profile: t('shell.profile'),
  };
  const pageTitle = routeForbidden
    ? t('shell.forbidden_title')
    : activeChild?.label ?? (route.sub === 'edit' ? t('shell.edit') : subLabels[route.sub]) ?? activeParent?.label ?? t('shell.dashboard');
  const documentSection = user ? pageTitle : t('shell.login_title');
  useEffect(() => { setSection(documentSection); }, [documentSection, setSection]);
  const breadcrumbParts = [
    t('shell.admin'),
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
            <h1 className="mt-1.5 text-2xl font-bold tracking-tight text-zinc-950">{t('shell.login_title')}</h1>
            <p className="mb-6 mt-1.5 text-sm text-zinc-600">{t('shell.login_description')}</p>
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
        {mobileOpen && <button type="button" className="od-drawer-overlay" aria-label={t('shell.close_menu')} onClick={() => setMobileOpen(false)} />}
        <aside id="admin-navigation" className={'od-sidebar ' + (mobileOpen ? 'od-sidebar-open' : '')} aria-label={t('shell.navigation')}>
          <div className="od-brand flex items-center gap-2.5 px-4 py-3.5">
            <div className="od-logo" aria-hidden="true">O</div>
            <div className="min-w-0">
              <div className="text-sm font-bold tracking-tight text-zinc-950">OpenDash</div>
              <div className="text-[11px] text-zinc-500">{t('shell.content_management')}</div>
            </div>
          </div>
          <nav className="od-nav overflow-y-auto px-2.5 py-3" aria-label={t('shell.nav_items')}>
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
                        title={item.disabled ? t('shell.unavailable') : undefined}
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
                          {item.disabled && <span className="od-nav-coming-soon">{t('shell.coming_soon')}</span>}
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
                                ? <span key={sub.hash} className="od-nav-sub-disabled" aria-disabled="true">{t('shell.disabled_submenu', { label: sub.label })}</span>
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
          <div className="od-sidebar-bottom px-4 py-2.5 text-[11px]">{t('shell.footer')}</div>
        </aside>

        <div className="od-workspace min-w-0">
          <header className="od-topbar flex flex-wrap items-center justify-between gap-3 px-4 py-3 sm:px-6">
            <div className="flex min-w-0 items-center gap-3">
              <button type="button" className="od-menu-toggle" aria-label={t('shell.open_menu')} aria-controls="admin-navigation" aria-expanded={mobileOpen} onClick={() => setMobileOpen(!mobileOpen)}>
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
              {hasUnsavedChanges && <span className="od-unsaved-label" role="status">{t('shell.unsaved')}</span>}
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
                <h2 className="text-lg font-semibold text-rose-900">{t('shell.forbidden_heading')}</h2>
                <p className="mt-2 text-sm text-rose-800">
                  {t('shell.forbidden_message', { path: route.rawHash })}
                </p>
                <button
                  type="button"
                  onClick={() => navigate('#/dashboard')}
                  className="mt-4 inline-flex h-9 items-center rounded-md bg-zinc-900 px-3.5 text-sm font-medium text-white hover:bg-zinc-800"
                >
                  {t('shell.back_dashboard')}
                </button>
              </section>
            )}

            {user && !routeForbidden && route.view === 'dashboard' && (
              <div className="od-dashboard space-y-6" id="dashboard-main">
                <div className="od-welcome rounded-xl px-5 py-4 sm:px-6 sm:py-5">
                  <div className="text-[11px] font-semibold uppercase tracking-wider text-od-accent">{t('shell.dashboard_heading')}</div>
                  <h2 className="mt-1 text-xl font-semibold tracking-tight text-zinc-950">{t('shell.welcome', { username: user.username })}</h2>
                  <p className="mt-1 max-w-2xl text-sm text-zinc-600">{t('shell.dashboard_description')}</p>
                </div>
                {dashboardContent}
                {customCptTypes.length > 0 && (
                  <section className="space-y-3" aria-labelledby="dashboard-cpt-heading" data-testid="dashboard-cpt-shortcuts">
                    <div>
                      <h2 id="dashboard-cpt-heading" className="text-base font-semibold text-zinc-950">{t('shell.custom_types')}</h2>
                      <p className="mt-0.5 text-sm text-zinc-600">{t('shell.custom_types_description')}</p>
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
                              {t('shell.list')}
                            </a>
                            <a
                              href={`#/cpt/${ct.descriptor.key}/new`}
                              onClick={e => { e.preventDefault(); navigate(`#/cpt/${ct.descriptor.key}/new`); }}
                              className="inline-flex h-8 items-center rounded-md bg-od-accent px-2.5 text-xs font-medium text-white hover:bg-od-accent-hover"
                            >
                              {t('shell.add_new_short')}
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
                <section className={user && route.view === 'posts' ? `od-editor-panel${postsLegacy}` : 'hidden'} aria-label={t('shell.posts')} aria-hidden={!user || route.view !== 'posts'}>{postsContent}</section>
                <section className={user && route.view === 'pages' ? `od-editor-panel${pagesLegacy}` : 'hidden'} aria-label={t('shell.pages')} aria-hidden={!user || route.view !== 'pages'}>{pagesContent}</section>
                {user && route.view === 'media' && mediaContent}
                {user && route.view === 'taxonomy' && <div className="od-admin-card min-w-0 max-w-full">{taxonomyContent}</div>}
                {user?.role === 'admin' && route.view === 'themes' && route.sub === 'themes' && <div className="od-admin-card min-w-0">{themesContent}</div>}
                {user?.role === 'admin' && route.view === 'themes' && route.sub === 'library' && <div className="od-legacy-scope">{libraryContent}</div>}
                {user?.role === 'admin' && route.view === 'patterns' && <div className="od-legacy-scope">{patternsContent}</div>}

                <section className={user && route.view === 'cpt' ? `od-editor-panel${cptLegacy}` : 'hidden'} aria-label={t('shell.custom_content')} aria-hidden={!user || route.view !== 'cpt'}>
                  <div id="content-main">{contentContent}</div>
                </section>

                {user?.role === 'admin' && route.view === 'plugins' && <div className="od-admin-card">{pluginsContent}</div>}
                <section className={user?.role === 'admin' && route.view === 'users' ? 'od-admin-card min-w-0 max-w-full overflow-x-clip' : 'hidden'} aria-label={t('shell.users')} aria-hidden={user?.role !== 'admin' || route.view !== 'users'}>
                  {usersContent}
                </section>
                {user && route.view === 'profile' && <div className="od-admin-card min-w-0">{profileContent}</div>}
                {user?.role === 'admin' && route.view === 'tools' && <div className="od-admin-card min-w-0">{toolsContent}</div>}
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
