import React, { useEffect, useState } from 'react';
import type { AuthUser, Role } from '../types.ts';
import { AuthPanel } from './AuthPanel.tsx';

export type AdminView =
  | 'dashboard' | 'posts' | 'pages' | 'content' | 'media'
  | 'themes' | 'builder' | 'plugins' | 'users' | 'settings';

type NavigationItem = { id: AdminView; label: string; icon: string; adminOnly?: boolean };
type NavigationGroup = { title: string; items: NavigationItem[] };

const navigation: NavigationGroup[] = [
  { title: 'Tổng quan', items: [{ id: 'dashboard', label: 'Bảng điều khiển', icon: 'dashboard' }] },
  {
    title: 'Nội dung',
    items: [
      { id: 'posts', label: 'Bài viết', icon: 'posts' },
      { id: 'pages', label: 'Trang', icon: 'pages' },
      { id: 'content', label: 'Nội dung tùy chỉnh', icon: 'content' },
      { id: 'media', label: 'Thư viện ảnh', icon: 'media' },
    ],
  },
  {
    title: 'Giao diện',
    items: [
      { id: 'themes', label: 'Themes', icon: 'themes', adminOnly: true },
      { id: 'builder', label: 'Builder', icon: 'builder' },
    ],
  },
  {
    title: 'Hệ thống',
    items: [
      { id: 'plugins', label: 'Plugins', icon: 'plugins', adminOnly: true },
      { id: 'users', label: 'Người dùng', icon: 'users', adminOnly: true },
      { id: 'settings', label: 'Cài đặt', icon: 'settings', adminOnly: true },
    ],
  },
];
const allItems = navigation.flatMap(group => group.items);
const titles = Object.fromEntries(allItems.map(item => [item.id, item.label])) as Record<AdminView, string>;
const adminViews = new Set<AdminView>(['themes', 'plugins', 'users', 'settings']);

export function parseAdminView(hash: string, pathname: string = '/admin/'): AdminView {
  // Preserve the historical /builder entry point when no client hash is set.
  if (!hash || hash === '#') return pathname.replace(/\/$/, '') === '/builder' ? 'builder' : 'dashboard';
  const id = hash.replace(/^#\/?/, '').split('/')[0];
  return allItems.find(item => item.id === id)?.id ?? 'dashboard';
}

export function isViewAllowed(view: AdminView, role: Role): boolean {
  return role === 'admin' || !adminViews.has(view);
}

function NavIcon({ name }: { name: string }) {
  const paths: Record<string, React.ReactNode> = {
    dashboard: <><rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /><rect x="14" y="14" width="7" height="7" rx="1" /></>,
    posts: <><path d="M8 3h9l4 4v14H8z" /><path d="M17 3v5h4M11 12h7M11 16h7M3 7v14" /></>,
    pages: <><path d="M5 3h10l4 4v14H5zM15 3v5h4M9 12h6M9 16h6" /></>,
    content: <><path d="M4 5h16v14H4zM8 9h3v3H8zM14 9h3M14 13h3M8 16h9" /></>,
    media: <><rect x="3" y="4" width="18" height="16" rx="2" /><circle cx="8" cy="9" r="1.5" /><path d="m4 17 5-5 4 3 3-4 5 6" /></>,
    themes: <><path d="M12 3 3 8l9 5 9-5-9-5ZM3 12l9 5 9-5M3 16l9 5 9-5" /></>,
    builder: <><path d="m14 3 7 7-11 11H3v-7L14 3ZM12 5l7 7M3 17h5" /></>,
    plugins: <><path d="M9 3h6v5h5v5h-5v8H9v-8H4V8h5z" /></>,
    users: <><circle cx="9" cy="8" r="3" /><path d="M3 21v-2a6 6 0 0 1 12 0v2M17 5a3 3 0 0 1 0 6M18 15a5 5 0 0 1 3 5v1" /></>,
    settings: <><circle cx="12" cy="12" r="3" /><path d="m10 2-1 3-3 1-3-1-2 4 3 2v2l-3 2 2 4 3-1 3 1 1 3h4l1-3 3-1 3 1 2-4-3-2v-2l3-2-2-4-3 1-3-1-1-3z" /></>,
  };
  return <svg aria-hidden="true" width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" strokeLinecap="round">{paths[name]}</svg>;
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
  activeView: AdminView;
  onViewChange: (view: AdminView) => void;
  hasUnsavedChanges: boolean;
  sidebarContent: React.ReactNode;
  previewContent: React.ReactNode;
  contentContent: React.ReactNode;
  postsContent: React.ReactNode;
  pagesContent: React.ReactNode;
  mediaContent: React.ReactNode;
  dashboardContent: React.ReactNode;
  pluginsContent: React.ReactNode;
  usersContent: React.ReactNode;
  settingsContent: React.ReactNode;
  themesContent: React.ReactNode;
}

export const AdminShell: React.FC<AdminShellProps> = ({
  user, authUsername, authPassword, statusMessage, isError, isProcessing,
  onUsernameChange, onPasswordChange, onSetup, onLogin, onLogout,
  activeView, onViewChange, hasUnsavedChanges, sidebarContent, previewContent,
  contentContent, postsContent, pagesContent, mediaContent, dashboardContent, pluginsContent, usersContent, settingsContent, themesContent,
}) => {
  const [mobileOpen, setMobileOpen] = useState(false);
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

  const navigate = (view: AdminView) => {
    if (!user || !isViewAllowed(view, user.role)) return;
    onViewChange(view);
    setMobileOpen(false);
  };

  return (
    <div className="od-root min-h-screen">
      {!user && (
        <div className="od-auth-layout flex min-h-screen items-center justify-center px-4 py-12">
          <section className="od-auth-card w-full max-w-lg rounded-3xl bg-white p-7 shadow-xl sm:p-10">
            <div className="od-logo od-logo-large" aria-hidden="true">O</div>
            <div className="mt-5 text-xs font-semibold uppercase tracking-widest text-sky-700">OpenDash CMS</div>
            <h1 className="mt-2 text-3xl font-bold tracking-tight text-slate-900">Đăng nhập quản trị</h1>
            <p className="mb-6 mt-2 text-slate-600">Quản lý nội dung và thiết kế website trong một không gian.</p>
            <AuthPanel
              user={user} authUsername={authUsername} authPassword={authPassword}
              statusMessage={statusMessage} isError={isError} isProcessing={isProcessing}
              onUsernameChange={onUsernameChange} onPasswordChange={onPasswordChange}
              onSetup={onSetup} onLogin={onLogin} onLogout={onLogout}
            />
          </section>
        </div>
      )}
      <div className={user ? 'od-layout' : 'hidden'} data-testid="admin-layout">
        {mobileOpen && <button type="button" className="od-drawer-overlay" aria-label="Đóng menu" onClick={() => setMobileOpen(false)} />}
        <aside id="admin-navigation" className={'od-sidebar ' + (mobileOpen ? 'od-sidebar-open' : '')} aria-label="Điều hướng quản trị">
          <div className="od-brand flex items-center gap-3 px-5 py-6">
            <div className="od-logo" aria-hidden="true">O</div>
            <div>
              <div className="text-base font-bold tracking-tight text-white">OpenDash</div>
              <div className="text-xs text-slate-400">Content Management</div>
            </div>
          </div>
          <nav className="od-nav overflow-y-auto px-3 pb-8" aria-label="Các mục quản trị">
            {navigation.map(group => {
              const items = group.items.filter(item => !item.adminOnly || user?.role === 'admin');
              if (items.length === 0) return null;
              return (
                <div key={group.title} className="mb-5">
                  <p className="px-3 pb-2 text-xs font-semibold uppercase tracking-wider text-slate-400">{group.title}</p>
                  <div className="flex flex-col gap-1">
                    {items.map(item => (
                      <button
                        key={item.id} type="button" className={'od-nav-link ' + (activeView === item.id ? 'od-nav-link-active' : '')}
                        data-view={item.id} aria-current={activeView === item.id ? 'page' : undefined}
                        onClick={() => navigate(item.id)}
                      >
                        <NavIcon name={item.icon} />
                        <span>{item.label}</span>
                        {item.id === 'content' && hasUnsavedChanges && <span className="od-nav-dot" aria-label="Có thay đổi chưa lưu" />}
                      </button>
                    ))}
                  </div>
                </div>
              );
            })}
          </nav>
          <div className="od-sidebar-bottom px-5 py-4 text-xs text-slate-400">OpenDash · M3 Admin</div>
        </aside>

        <div className="od-workspace min-w-0">
          <header className="od-topbar flex flex-wrap items-center justify-between gap-4 px-5 py-4 sm:px-8">
            <div className="flex min-w-0 items-center gap-3">
              <button type="button" className="od-menu-toggle" aria-label="Mở menu" aria-controls="admin-navigation" aria-expanded={mobileOpen} onClick={() => setMobileOpen(!mobileOpen)}>
                <span aria-hidden="true">☰</span>
              </button>
              <div className="min-w-0">
                <p className="text-xs font-medium text-slate-500">Không gian quản trị / {user?.role === 'admin' ? 'Quản trị viên' : 'Biên tập viên'}</p>
                <h1 className="mt-1 truncate text-xl font-bold tracking-tight text-slate-900">{titles[activeView]}</h1>
              </div>
            </div>
            <div className="od-topbar-actions flex flex-wrap items-center gap-3">
              {hasUnsavedChanges && <span className="od-unsaved-label" role="status">● Có thay đổi chưa lưu</span>}
              {user && <AuthPanel
                user={user} authUsername={authUsername} authPassword={authPassword}
                statusMessage={statusMessage} isError={isError} isProcessing={isProcessing}
                onUsernameChange={onUsernameChange} onPasswordChange={onPasswordChange}
                onSetup={onSetup} onLogin={onLogin} onLogout={onLogout}
              />}
            </div>
          </header>

          <div className="od-page-container px-4 py-6 sm:px-8 sm:py-8">
            {user && activeView === 'dashboard' && (
              <main className="od-dashboard" id="dashboard-main">
                <div className="od-welcome mb-7 rounded-2xl p-7 sm:p-9">
                  <div className="text-xs font-semibold uppercase tracking-widest text-sky-200">Trang tổng quan</div>
                  <h2 className="mt-3 text-2xl font-bold text-white sm:text-3xl">Xin chào, {user.username}!</h2>
                  <p className="mt-3 max-w-xl text-sm leading-6 text-slate-200">Chọn một khu vực bên dưới để tiếp tục công việc. Các tính năng đang phát triển được ghi rõ trạng thái.</p>
                </div>
                {dashboardContent}
                <h2 className="mb-4 text-lg font-semibold text-slate-900">Truy cập nhanh</h2>
                <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-3">
                  {allItems.filter(item => ['content', 'builder', 'posts', 'pages', 'media', 'plugins', 'users'].includes(item.id) && (!item.adminOnly || user.role === 'admin')).map(item => (
                    <button key={item.id} type="button" className="od-quick-link" onClick={() => navigate(item.id)}>
                      <span className="od-quick-icon"><NavIcon name={item.icon} /></span>
                      <span className="flex-1 text-left">
                        <span className="block font-semibold text-slate-900">{item.label}</span>
                        <span className="mt-1 block text-xs text-slate-500">
                          {item.id === 'builder' || item.id === 'content' || item.id === 'posts' || item.id === 'pages' || item.id === 'plugins' || item.id === 'users'
                            ? 'Mở công cụ hiện có' : 'Xem trạng thái phát triển'}
                        </span>
                      </span>
                      <span aria-hidden="true" className="text-slate-400">›</span>
                    </button>
                  ))}
                </div>
              </main>
            )}
            <section className={user && activeView === 'posts' ? 'od-editor-panel' : 'hidden'} aria-label="Bài viết" aria-hidden={!user || activeView !== 'posts'}>{postsContent}</section>
            <section className={user && activeView === 'pages' ? 'od-editor-panel' : 'hidden'} aria-label="Trang" aria-hidden={!user || activeView !== 'pages'}>{pagesContent}</section>
            {user && activeView === 'media' && mediaContent}
            {user?.role === 'admin' && activeView === 'themes' && <main className="od-admin-card min-w-0">{themesContent}</main>}

            {/* Keep both editors mounted when navigating: forms and preview must survive switching sections. */}
            <section className={user && activeView === 'builder' ? 'od-editor-panel' : 'hidden'} aria-label="Builder" aria-hidden={!user || activeView !== 'builder'}>
              <main id="builder-main">
                <section className="editor-sidebar">{sidebarContent}</section>
                {previewContent}
              </main>
            </section>
            <section className={user && activeView === 'content' ? 'od-editor-panel' : 'hidden'} aria-label="Nội dung tùy chỉnh" aria-hidden={!user || activeView !== 'content'}>
              <main id="content-main">{contentContent}</main>
            </section>
            {user?.role === 'admin' && activeView === 'plugins' && <main className="od-admin-card">{pluginsContent}</main>}
            <section className={user?.role === 'admin' && activeView === 'users' ? 'od-admin-card min-w-0 max-w-full overflow-x-clip' : 'hidden'} aria-label="Người dùng" aria-hidden={user?.role !== 'admin' || activeView !== 'users'}>
              {usersContent}
            </section>
            {user?.role === 'admin' && activeView === 'settings' && <main className="od-admin-card min-w-0">{settingsContent}</main>}
          </div>
        </div>
      </div>
    </div>
  );
};
