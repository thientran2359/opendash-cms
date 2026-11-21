import React, { useEffect, useState } from 'react';
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

type NavigationGroup = 'content' | 'appearance' | 'customize';

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

function buildMenuTree(cptTypes: OwnedContentType[], isAdmin: boolean): ParentMenuItem[] {
  const customTypes = cptTypes.filter(
    ct => ct.descriptor.key !== 'post' && ct.descriptor.key !== 'page' && ct.descriptor.key !== 'section' && ct.descriptor.key !== 'pattern',
  );
  return [
    { id: 'dashboard', label: 'Tổng quan', icon: 'dashboard', hash: '#/dashboard' },
    {
      id: 'posts',
      label: 'Bài viết',
      icon: 'posts',
      hash: '#/posts',
      group: 'content',
      children: [
        { label: 'Tất cả bài viết', hash: '#/posts' },
        { label: 'Viết bài mới', hash: '#/posts/new' },
        { label: 'Chuyên mục', hash: '#/posts/categories' },
        { label: 'Thẻ', hash: '#/posts/tags' },
      ],
    },
    {
      id: 'pages',
      label: 'Trang',
      icon: 'pages',
      hash: '#/pages',
      group: 'content',
      children: [
        { label: 'Tất cả trang', hash: '#/pages' },
        { label: 'Thêm trang mới', hash: '#/pages/new' },
      ],
    },
    ...customTypes.map(ct => ({
      id: `cpt:${ct.descriptor.key}`,
      label: ct.descriptor.label,
      icon: 'content',
      hash: `#/cpt/${ct.descriptor.key}`,
      group: 'content' as const,
      children: [
        { label: `Tất cả ${ct.descriptor.label}`, hash: `#/cpt/${ct.descriptor.key}` },
        { label: 'Thêm mới', hash: `#/cpt/${ct.descriptor.key}/new` },
      ],
    })),
    {
      id: 'media',
      label: 'Thư viện media',
      icon: 'media',
      hash: '#/media',
      group: 'content',
      children: [
        { label: 'Thư viện', hash: '#/media' },
        { label: 'Tải lên', hash: '#/media/upload' },
      ],
    },
    {
      id: 'patterns',
      label: 'Patterns',
      icon: 'content',
      hash: '#/patterns',
      group: 'content',
      adminOnly: true,
      children: [
        { label: 'Tất cả Pattern', hash: '#/patterns' },
        { label: 'Thêm Pattern', hash: '#/patterns/new' },
      ],
    },
    {
      id: 'themes',
      label: 'Giao diện',
      icon: 'themes',
      hash: '#/appearance/themes',
      group: 'appearance',
      adminOnly: true,
      children: [
        { label: 'Themes', hash: '#/appearance/themes' },
        { label: 'Menus', hash: '#/appearance/menus' },
      ],
    },
    {
      id: 'theme-customize',
      label: 'Tùy chỉnh giao diện',
      icon: 'themes',
      hash: '#/appearance/options',
      group: 'appearance',
      adminOnly: true,
      children: [
        { label: 'Tùy chọn theme', hash: '#/appearance/options' },
        { label: 'Visual Builder', hash: '#/builder' },
      ],
    },
    {
      id: 'content-types',
      label: 'Loại nội dung & Trường',
      icon: 'content',
      hash: '#/appearance/content-types',
      group: 'appearance',
      adminOnly: true,
    },
    {
      id: 'taxonomies',
      label: 'Taxonomy tùy chỉnh',
      icon: 'content',
      hash: '#/taxonomy/definitions',
      group: 'appearance',
      adminOnly: true,
    },
    {
      id: 'library',
      label: 'Thư viện mẫu',
      icon: 'content',
      hash: '#/appearance/library',
      group: 'appearance',
      adminOnly: true,
    },
    {
      id: 'plugins',
      label: 'Plugins',
      icon: 'plugins',
      hash: '#/plugins',
      group: 'customize',
      adminOnly: true,
      children: [
        { label: 'Plugins đã cài', hash: '#/plugins' },
        { label: 'Cài plugin', hash: '#/plugins/install' },
      ],
    },
    {
      id: 'users',
      label: 'Người dùng',
      icon: 'users',
      hash: isAdmin ? '#/users' : '#/users/profile',
      group: 'customize',
      children: [
        { label: 'Tất cả người dùng', hash: '#/users', adminOnly: true },
        { label: 'Thêm người dùng', hash: '#/users/new', adminOnly: true },
        { label: 'Hồ sơ của tôi', hash: '#/users/profile' },
      ],
    },
    {
      id: 'tools',
      label: 'Công cụ',
      icon: 'tools',
      hash: '#/tools/backup',
      group: 'customize',
      adminOnly: true,
      children: [
        { label: 'Sao lưu & Phục hồi', hash: '#/tools/backup' },
        { label: 'Đối soát Media', hash: '#/tools/media-reconcile' },
        { label: 'Tình trạng hệ thống', hash: '#/tools/health' },
      ],
    },
    {
      id: 'settings',
      label: 'Cài đặt',
      icon: 'settings',
      hash: '#/settings/general',
      group: 'customize',
      adminOnly: true,
      children: [
        { label: 'Tổng quát', hash: '#/settings/general' },
        { label: 'Trang chủ & Bài viết', hash: '#/settings/reading' },
        { label: 'Đường dẫn tĩnh', hash: '#/settings/permalinks' },
        { label: 'SEO', hash: '#/settings/seo' },
        { label: 'Liên hệ', hash: '#/settings/contact' },
        { label: 'Thông báo Email (SMTP)', hash: '#/settings/smtp' },
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
    themes: <><path d="M12 3 3 8l9 5 9-5-9-5ZM3 12l9 5 9-5M3 16l9 5 9-5" /></>,
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
  onNavigateHash: (hash: string) => void;
  hasUnsavedChanges: boolean;
  sidebarContent: React.ReactNode;
  previewContent: React.ReactNode;
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
  menusContent: React.ReactNode;
  themeOptionsContent: React.ReactNode;
  libraryContent: React.ReactNode;
}

export const AdminShell: React.FC<AdminShellProps> = ({
  user, authUsername, authPassword, statusMessage, isError, isProcessing,
  onUsernameChange, onPasswordChange, onSetup, onLogin, onLogout,
  route, routeForbidden, cptTypes, onNavigateHash, hasUnsavedChanges,
  sidebarContent, previewContent, contentContent, schemaSettingsContent,
  postsContent, pagesContent, patternsContent, mediaContent, taxonomyContent, dashboardContent,
  pluginsContent, usersContent, profileContent, toolsContent, settingsContent, themesContent,
  menusContent, themeOptionsContent, libraryContent,
}) => {
  const [mobileOpen, setMobileOpen] = useState(false);
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);

  const menuTree = buildMenuTree(cptTypes, user?.role === 'admin');
  const activeParentId =
    route.view === 'cpt' && route.typeKey
      ? `cpt:${route.typeKey}`
      : route.view === 'taxonomy'
        ? (route.taxonomyMode === 'definitions' || (route.taxonomyKind !== 'category' && route.taxonomyKind !== 'tag')
          ? 'taxonomies' : 'posts')
        : route.view === 'profile'
          ? 'users'
          : route.view === 'builder'
            ? 'theme-customize'
            : route.view === 'themes'
              ? (route.sub === 'options' ? 'theme-customize' : route.sub === 'library' ? 'library' : 'themes')
              : route.view === 'settings' && route.sub === 'content-types'
                ? 'content-types'
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
    { id: 'content', label: 'Nội dung', match: 'content' },
    { id: 'appearance', label: 'Giao diện', match: 'appearance' },
    { id: 'customize', label: 'Tùy chỉnh', match: 'customize' },
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
      ? (route.taxonomyMode === 'definitions' || (route.taxonomyKind !== 'category' && route.taxonomyKind !== 'tag')
        ? '#/taxonomy/definitions' : `#/posts/${route.taxonomyKind === 'tag' ? 'tags' : 'categories'}`) :
    route.view === 'profile' ? '#/users/profile' :
    route.view === 'builder' ? '#/builder' :
    route.view === 'tools' ? `#/tools/${route.sub}` :
    route.view === 'settings' ? `#/settings/${route.sub}` :
    route.view === 'media' ? (route.sub === 'upload' ? '#/media/upload' : '#/media') :
    route.view === 'plugins' ? (route.sub === 'install' ? '#/plugins/install' : '#/plugins') :
    route.view === 'users' ? (route.sub === 'new' ? '#/users/new' : '#/users') :
    route.rawHash.split('?')[0];
  const activeChild = activeParent?.children?.find(child => child.hash === selectedNavHash);
  const subLabels: Record<string, string> = {
    list: 'Danh sách', new: 'Thêm mới', edit: 'Chỉnh sửa', library: 'Thư viện',
    general: 'Tổng quát', reading: 'Trang chủ & Bài viết', permalinks: 'Đường dẫn tĩnh', seo: 'SEO', contact: 'Liên hệ', smtp: 'Thông báo Email (SMTP)',
    'content-types': 'Loại nội dung & Trường', themes: 'Themes', menus: 'Menus',
    definitions: 'Taxonomy tùy chỉnh', terms: 'Các term',
    options: 'Tùy chọn theme', health: 'Tình trạng hệ thống',
    'media-reconcile': 'Đối soát Media', backup: 'Sao lưu & Phục hồi',
    install: 'Cài đặt', profile: 'Hồ sơ của tôi',
  };
  const pageTitle = routeForbidden
    ? '403 Không có quyền truy cập'
    : route.view === 'themes' && route.sub === 'library'
      ? 'Chưa khả dụng'
      : activeChild?.label ?? (route.sub === 'edit' ? 'Chỉnh sửa' : subLabels[route.sub]) ?? activeParent?.label ?? 'Tổng quan';
  const breadcrumbParts = [
    'Quản trị',
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
  // PatternsPage and TemplateLibraryPage own their own <main> landmark.
  const routeOwnsMain = Boolean(
    user && !routeForbidden && (
      route.view === 'patterns' ||
      (route.view === 'themes' && route.sub === 'library')
    ),
  );
  // Keep the Builder subtree mounted through a transient 401/login cycle so
  // its quarantined, unsaved AST survives same-account reauthentication.
  // The admin layout is hidden while logged out, so its <main> is not an
  // additional visible landmark beside the public login screen.
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
            <h1 className="mt-1.5 text-2xl font-bold tracking-tight text-zinc-950">Đăng nhập quản trị</h1>
            <p className="mb-6 mt-1.5 text-sm text-zinc-600">Quản lý nội dung và thiết kế website trong một không gian.</p>
            <AuthPanel
              user={user} authUsername={authUsername} authPassword={authPassword}
              statusMessage={statusMessage} isError={isError} isProcessing={isProcessing}
              onUsernameChange={onUsernameChange} onPasswordChange={onPasswordChange}
              onSetup={onSetup} onLogin={onLogin} onLogout={onLogout}
            />
          </section>
        </main>
      )}

      <div className={user ? 'od-layout' : 'hidden'} data-testid="admin-layout">
        {mobileOpen && <button type="button" className="od-drawer-overlay" aria-label="Đóng menu" onClick={() => setMobileOpen(false)} />}
        <aside id="admin-navigation" className={'od-sidebar ' + (mobileOpen ? 'od-sidebar-open' : '')} aria-label="Điều hướng quản trị">
          <div className="od-brand flex items-center gap-2.5 px-4 py-3.5">
            <div className="od-logo" aria-hidden="true">O</div>
            <div className="min-w-0">
              <div className="text-sm font-bold tracking-tight text-zinc-950">OpenDash</div>
              <div className="text-[11px] text-zinc-500">Content Management</div>
            </div>
          </div>
          <nav className="od-nav overflow-y-auto px-2.5 py-3" aria-label="Các mục quản trị">
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
                        title={item.disabled ? 'Chưa khả dụng — sẽ được bật khi tính năng hoàn thành' : undefined}
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
                          {item.disabled && <span className="od-nav-coming-soon">Sắp có</span>}
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
                                ? <span key={sub.hash} className="od-nav-sub-disabled" aria-disabled="true">{sub.label} · Sắp có</span>
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
          <div className="od-sidebar-bottom px-4 py-2.5 text-[11px]">OpenDash · Admin</div>
        </aside>

        <div className="od-workspace min-w-0">
          <header className="od-topbar flex flex-wrap items-center justify-between gap-3 px-4 py-3 sm:px-6">
            <div className="flex min-w-0 items-center gap-3">
              <button type="button" className="od-menu-toggle" aria-label="Mở menu" aria-controls="admin-navigation" aria-expanded={mobileOpen} onClick={() => setMobileOpen(!mobileOpen)}>
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
              {hasUnsavedChanges && <span className="od-unsaved-label" role="status">● Có thay đổi chưa lưu</span>}
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
                <h2 className="text-lg font-semibold text-rose-900">403 — Không có quyền truy cập</h2>
                <p className="mt-2 text-sm text-rose-800">
                  Tài khoản hiện tại không có quyền mở đường dẫn <code>{route.rawHash}</code>.
                </p>
                <button
                  type="button"
                  onClick={() => navigate('#/dashboard')}
                  className="mt-4 inline-flex h-9 items-center rounded-md bg-zinc-900 px-3.5 text-sm font-medium text-white hover:bg-zinc-800"
                >
                  Quay về Tổng quan
                </button>
              </section>
            )}

            {user && !routeForbidden && route.view === 'dashboard' && (
              <div className="od-dashboard space-y-6" id="dashboard-main">
                <div className="od-welcome rounded-xl px-5 py-4 sm:px-6 sm:py-5">
                  <div className="text-[11px] font-semibold uppercase tracking-wider text-od-accent">Trang tổng quan</div>
                  <h2 className="mt-1 text-xl font-semibold tracking-tight text-zinc-950">Xin chào, {user.username}</h2>
                  <p className="mt-1 max-w-2xl text-sm text-zinc-600">Chọn một khu vực từ menu bên trái để quản lý nội dung và cấu hình hệ thống.</p>
                </div>
                {dashboardContent}
                {customCptTypes.length > 0 && (
                  <section className="space-y-3" aria-labelledby="dashboard-cpt-heading" data-testid="dashboard-cpt-shortcuts">
                    <div>
                      <h2 id="dashboard-cpt-heading" className="text-base font-semibold text-zinc-950">Loại nội dung tùy chỉnh</h2>
                      <p className="mt-0.5 text-sm text-zinc-600">Lối tắt quản lý các loại nội dung mở rộng đang hoạt động.</p>
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
                              Danh sách
                            </a>
                            <a
                              href={`#/cpt/${ct.descriptor.key}/new`}
                              onClick={e => { e.preventDefault(); navigate(`#/cpt/${ct.descriptor.key}/new`); }}
                              className="inline-flex h-8 items-center rounded-md bg-od-accent px-2.5 text-xs font-medium text-white hover:bg-od-accent-hover"
                            >
                              + Mới
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
                <section className={user && route.view === 'posts' ? `od-editor-panel${postsLegacy}` : 'hidden'} aria-label="Bài viết" aria-hidden={!user || route.view !== 'posts'}>{postsContent}</section>
                <section className={user && route.view === 'pages' ? `od-editor-panel${pagesLegacy}` : 'hidden'} aria-label="Trang" aria-hidden={!user || route.view !== 'pages'}>{pagesContent}</section>
                {user && route.view === 'media' && mediaContent}
                {user && route.view === 'taxonomy' && <div className="od-admin-card min-w-0 max-w-full">{taxonomyContent}</div>}
                {user?.role === 'admin' && route.view === 'themes' && route.sub === 'themes' && <div className="od-admin-card min-w-0">{themesContent}</div>}
                {user?.role === 'admin' && route.view === 'themes' && route.sub === 'menus' && <div className="od-admin-card min-w-0">{menusContent}</div>}
                {user?.role === 'admin' && route.view === 'themes' && route.sub === 'options' && <div className="od-admin-card min-w-0">{themeOptionsContent}</div>}
                {user?.role === 'admin' && route.view === 'themes' && route.sub === 'library' && <div className="od-legacy-scope">{libraryContent}</div>}
                {user?.role === 'admin' && route.view === 'patterns' && <div className="od-legacy-scope">{patternsContent}</div>}

                <section className={user && route.view === 'builder' ? 'od-editor-panel od-legacy-scope' : 'hidden'} aria-label="Builder" aria-hidden={!user || route.view !== 'builder'}>
                  <div id="builder-main">
                    <section className="editor-sidebar">{sidebarContent}</section>
                    {previewContent}
                  </div>
                </section>

                <section className={user && route.view === 'cpt' ? `od-editor-panel${cptLegacy}` : 'hidden'} aria-label="Nội dung tùy chỉnh" aria-hidden={!user || route.view !== 'cpt'}>
                  <div id="content-main">{contentContent}</div>
                </section>

                {user?.role === 'admin' && route.view === 'plugins' && <div className="od-admin-card">{pluginsContent}</div>}
                <section className={user?.role === 'admin' && route.view === 'users' ? 'od-admin-card min-w-0 max-w-full overflow-x-clip' : 'hidden'} aria-label="Người dùng" aria-hidden={user?.role !== 'admin' || route.view !== 'users'}>
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
