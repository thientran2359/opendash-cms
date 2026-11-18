import React, { useState, useEffect, useRef, useCallback } from 'react';
import { AuthUser, PluginSummary, type Role } from './types.ts';
import * as api from './api/index.ts';
import { AdminShell, isRouteAllowed, parseAdminRoute, type ParsedRoute } from './shell/AdminShell.tsx';
import { listContentTypes } from './api/content.ts';
import type { OwnedContentType } from './features/content/types.ts';
import { ContentTypeSchemaSettings } from './features/content/ContentTypeSchemaSettings.tsx';
import { UserProfilePanel } from './features/users/UserProfilePanel.tsx';
import { SystemStatus } from './features/system/SystemStatus.tsx';
import { GeneralSettingsPanel } from './features/system/GeneralSettingsPanel.tsx';
import { BuilderWorkspace } from './builder/BuilderWorkspace.tsx';
import { UsersPanel } from './features/users/UsersPanel.tsx';
import { PluginsPanel } from './features/plugins/PluginsPanel.tsx';
import { SingleEditWorkspace } from './features/editor/SingleEditWorkspace.tsx';
import { editorDraftStore } from './features/editor/EditorDraftStore.ts';

import { MediaRoute } from './shell/MediaRoute.tsx';
import { DashboardPage } from './features/dashboard/DashboardPage.tsx';
import { ThemesPanel } from './features/themes/ThemesPanel.tsx';
import { MenusPanel } from './features/menus/MenusPanel.tsx';
import { SeoPanel } from './features/seo/SeoPanel.tsx';
import { ContactPanel } from './features/contact/ContactPanel.tsx';
import { SmtpSettingsPanel } from './features/contact/SmtpSettingsPanel.tsx';
import { ReconcilePanel } from './features/operations/ReconcilePanel.tsx';
import { BackupsPanel } from './features/operations/BackupsPanel.tsx';
import { RestorePanel } from './features/operations/RestorePanel.tsx';
import { TaxonomyPage } from './features/taxonomy/TaxonomyPage.tsx';
import { TermEditPage } from './features/taxonomy/TermEditPage.tsx';
import { SectionsPage } from './features/sections/SectionsPage.tsx';
import { TemplateLibraryPage } from './features/library/TemplateLibraryPage.tsx';

const HISTORY_INDEX = '__openDashAdminHistoryIndex';

function draftUuidFromHash(hash: string): string | null {
  const param = new URLSearchParams(hash.split('?')[1] ?? '').get('draft');
  return param && /^[a-zA-Z0-9_-]{8,64}$/.test(param) ? param : null;
}

function normalizeEditorHash(hash: string): string {
  const normalized = hash.startsWith('#') ? hash : '#' + hash;
  if (!/^#\/(posts|pages|cpt\/[a-zA-Z0-9_-]+)\/new(?:\?|$)/.test(normalized)) return normalized;
  if (draftUuidFromHash(normalized)) return normalized;
  const [path, search] = normalized.split('?');
  const params = new URLSearchParams(search ?? '');
  // A URL identity, not draft data: new Post/Page/CPT forms never collide.
  params.set('draft', `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`);
  return `${path}?${params.toString()}`;
}

function editorIdentity(route: ParsedRoute, hash: string): string | null {
  if (!['posts', 'pages', 'cpt'].includes(route.view) || !['edit', 'new'].includes(route.sub)) return null;
  const typeKey = route.view === 'posts' ? 'post' : route.view === 'pages' ? 'page' : route.typeKey;
  if (!typeKey) return null;
  if (route.sub === 'edit' && route.entryId != null) return `${typeKey}:${route.entryId}`;
  const uuid = draftUuidFromHash(hash);
  return uuid ? `${typeKey}:new:${uuid}` : null;
}

function builderIdentity(route: ParsedRoute): string | null {
  if (route.view !== 'builder') return null;
  return `${route.sub}:${route.templateRole ?? 'homepage'}:${route.typeKey ?? ''}:${route.entryId ?? ''}`;
}

export const App: React.FC = () => {
  const [user, setUser] = useState<AuthUser | null>(null);
  const [csrfToken, setCsrfToken] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<string>('Đang kiểm tra trạng thái phiên...');
  const [isError, setIsError] = useState<boolean>(false);
  const [isProcessing, setIsProcessing] = useState<boolean>(false);
  const [route, setRoute] = useState<ParsedRoute>(() => parseAdminRoute(window.location.hash, window.location.pathname));
  const [cptTypes, setCptTypes] = useState<OwnedContentType[]>([]);
  const [selectedSchemaTypeKey, setSelectedSchemaTypeKey] = useState<string | null>(null);
  const [siteInstanceId, setSiteInstanceId] = useState<string>('default-site');
  const [authGeneration, setAuthGeneration] = useState<number>(1);
  const [contentDirty, setContentDirty] = useState(false);
  const [builderDirty, setBuilderDirty] = useState(false);
  const [postsDirty, setPostsDirty] = useState(false);
  const [pagesDirty, setPagesDirty] = useState(false);
  const [sectionsDirty, setSectionsDirty] = useState(false);
  const sectionsDirtyRef = useRef(false);
  const markSectionsDirty = (value: boolean) => { sectionsDirtyRef.current = value; setSectionsDirty(value); };
  const [termDirty, setTermDirty] = useState(false);
  const termDirtyRef = useRef(false);
  const markTermDirty = (next: boolean) => {
    termDirtyRef.current = next;
    setTermDirty(next);
  };
  const [contentRevision, setContentRevision] = useState(0);
  const acceptedHashRef = useRef(window.location.hash || '#/dashboard');
  const historyIndexRef = useRef(0);
  const undoingHistoryRef = useRef(false);
  const routeRef = useRef(route);
  routeRef.current = route;
  const userRef = useRef(user);
  userRef.current = user;
  const builderDirtyRef = useRef(builderDirty);
  builderDirtyRef.current = builderDirty;
  const lastBuilderRouteRef = useRef<ParsedRoute>(parseAdminRoute('#/builder'));
  if (route.view === 'builder') lastBuilderRouteRef.current = route;

  // Auth inputs
  const [authUsername, setAuthUsername] = useState<string>('');
  const [authPassword, setAuthPassword] = useState<string>('');

  // Keep the Users draft when an expired session signs back in to the same account.
  const [accountContextId, setAccountContextId] = useState<number | null>(null);
  const [accountContextRole, setAccountContextRole] = useState<Role | null>(null);

  // Plugin management state (Admin only)
  const [plugins, setPlugins] = useState<PluginSummary[]>([]);
  const [selectedPluginId, setSelectedPluginId] = useState<string>('');
  const [pluginInputJson, setPluginInputJson] = useState<string>('{\n  "text": "OpenDash Plugin"\n}');
  const [pluginRunOutput, setPluginRunOutput] = useState<string>('');

  const [builderReset, setBuilderReset] = useState(0);
  const [themeRevision, setThemeRevision] = useState(0);
  // Retain the Builder draft across a 401 only when the same account signs back in.
  const lastAuthenticatedUserId = useRef<number | null>(null);

  // Hash routing works from /admin/ and /builder without server catch-all.
  // The existing editors stay mounted in AdminShell across route changes.
  const loadCptTypes = useCallback(async () => {
    try {
      const list = await listContentTypes();
      setCptTypes(list);
    } catch {
      // Non-fatal on boot; panels handle explicit errors.
    }
  }, []);

  const applyAcceptedRoute = (hash: string) => {
    const requested = parseAdminRoute(hash, window.location.pathname);
    const next = { ...requested, rawHash: hash };
    acceptedHashRef.current = hash;
    routeRef.current = next;
    setRoute(next);
    if (requested.view === 'settings' && requested.sub === 'content-types' && requested.typeKey) {
      setSelectedSchemaTypeKey(requested.typeKey);
    }
  };

  const mayLeave = (destination: string): boolean => {
    if (!userRef.current || acceptedHashRef.current === destination) return true;
    const current = routeRef.current;
    const next = parseAdminRoute(destination, window.location.pathname);
    const currentEditor = editorIdentity(current, acceptedHashRef.current);
    const nextEditor = editorIdentity(next, destination);
    const leavingDirtyEditor = currentEditor !== null && currentEditor !== nextEditor &&
      editorDraftStore.hasDirtyKey(currentEditor);
    const switchingBuilderTarget = current.view === 'builder'
      && builderIdentity(current) !== builderIdentity(next);
    const leavingBuilder = switchingBuilderTarget && builderDirtyRef.current;
    const leavingSection = current.view === 'sections' && current.rawHash !== destination && sectionsDirtyRef.current;
    const leavingTerm = current.view === 'taxonomy' && current.taxonomyMode !== 'list'
      && termDirtyRef.current && acceptedHashRef.current !== destination;
    return !(leavingDirtyEditor || leavingBuilder || leavingTerm || leavingSection) ||
      window.confirm(switchingBuilderTarget
        ? 'Builder có thay đổi chưa lưu. Bỏ nháp và chuyển sang trang khác?'
        : 'Có dữ liệu chưa lưu. Rời màn hình? Nháp sẽ được giữ trong phiên hiện tại.');
  };

  useEffect(() => {
    const currentIndex = window.history.state?.[HISTORY_INDEX];
    historyIndexRef.current = Number.isSafeInteger(currentIndex) ? currentIndex : 0;
    window.history.replaceState({ ...(window.history.state ?? {}), [HISTORY_INDEX]: historyIndexRef.current }, '');

    const synchronize = (event?: Event) => {
      if (undoingHistoryRef.current) {
        if (window.location.hash === acceptedHashRef.current) undoingHistoryRef.current = false;
        return;
      }
      let target = normalizeEditorHash(window.location.hash || '#/dashboard');
      const requested = parseAdminRoute(target, window.location.pathname);
      if (userRef.current && requested.isLegacyContentRedirect) {
        target = userRef.current.role === 'admin' ? '#/appearance/content-types' : '#/posts';
      }
      if (target === acceptedHashRef.current && event) return;
      if (!mayLeave(target)) {
        const incomingIndex = window.history.state?.[HISTORY_INDEX];
        const delta = Number.isSafeInteger(incomingIndex) ? historyIndexRef.current - incomingIndex : 0;
        if (event?.type === 'popstate' && delta !== 0) {
          undoingHistoryRef.current = true;
          window.history.go(delta);
        } else if (event?.type === 'popstate' && !Number.isSafeInteger(incomingIndex)) {
          // A direct location.hash assignment creates a new history entry
          // with null state. Undo that entry, never overwrite its destination.
          undoingHistoryRef.current = true;
          window.history.back();
        } else if (event?.type === 'hashchange') {
          undoingHistoryRef.current = true;
          window.history.back();
        } else {
          window.history.replaceState({ ...(window.history.state ?? {}), [HISTORY_INDEX]: historyIndexRef.current }, '',
            window.location.pathname + window.location.search + acceptedHashRef.current);
        }
        return;
      }
      const incomingIndex = window.history.state?.[HISTORY_INDEX];
      if (event?.type === 'popstate' && Number.isSafeInteger(incomingIndex)) {
        historyIndexRef.current = incomingIndex;
      } else if (target !== acceptedHashRef.current && event) {
        historyIndexRef.current += 1;
      }
      // Also stamp popstate entries with null state. Without this the next
      // rejected Back/Forward cannot compute the inverse history delta.
      window.history.replaceState({ ...(window.history.state ?? {}), [HISTORY_INDEX]: historyIndexRef.current }, '',
        window.location.pathname + window.location.search + target);
      applyAcceptedRoute(target);
    };
    synchronize();
    window.addEventListener('hashchange', synchronize);
    window.addEventListener('popstate', synchronize);
    return () => {
      window.removeEventListener('hashchange', synchronize);
      window.removeEventListener('popstate', synchronize);
    };
  }, [user?.role]);

  useEffect(() => {
    const guard = (event: BeforeUnloadEvent) => {
      if (!editorDraftStore.hasAnyDirty() && !(builderDirtyRef.current || contentDirty || postsDirty || pagesDirty || termDirtyRef.current || sectionsDirtyRef.current)) return;
      event.preventDefault();
      event.returnValue = '';
    };
    window.addEventListener('beforeunload', guard);
    return () => window.removeEventListener('beforeunload', guard);
  }, [contentDirty, postsDirty, pagesDirty, termDirty, sectionsDirty]);

  const navigateHash = (hash: string) => {
    if (!userRef.current) return;
    const target = normalizeEditorHash(hash);
    if (!mayLeave(target)) return;
    if (window.location.hash !== target) {
      historyIndexRef.current += 1;
      window.history.pushState({ ...(window.history.state ?? {}), [HISTORY_INDEX]: historyIndexRef.current }, '',
        window.location.pathname + window.location.search + target);
    }
    applyAcceptedRoute(target);
  };

  const setStatus = (msg: string, error: boolean = false) => {
    setStatusMessage(msg);
    setIsError(error);
  };

  const handleApiError = useCallback((err: unknown, fallback: string) => {
    const is401 = err instanceof api.ApiClientError && err.status === 401;
    const msg = err instanceof Error ? err.message : fallback;
    setStatus(msg, true);
    if (is401) {
      editorDraftStore.quarantineAll();
      setUser(null);
      setCsrfToken(null);
    }
  }, []);

  const loadPlugins = async () => {
    try {
      const list = await api.getPlugins();
      setPlugins(list);
      if (list.length > 0) {
        setSelectedPluginId(prev => (prev && list.some(p => p.id === prev) ? prev : list[0].id));
      }
    } catch (err) {
      handleApiError(err, 'Lỗi khi tải danh sách plugin.');
    }
  };

  useEffect(() => {
    (async () => {
      try {
        const auth = await api.getAuthMe();
        lastAuthenticatedUserId.current = auth.user.id;
        setAccountContextId(auth.user.id);
        setAccountContextRole(auth.user.role);
        if (auth.site_instance_id) setSiteInstanceId(auth.site_instance_id);
        setUser(auth.user);
        setCsrfToken(auth.csrf_token);
        setStatus(`Đang đăng nhập: ${auth.user.username} (${auth.user.role})`);
        await loadCptTypes();
        if (auth.user.role === 'admin') {
          await loadPlugins();
        }
      } catch {
        setUser(null);
        setCsrfToken(null);
        setStatus('Vui lòng đăng nhập để sử dụng builder.');
      }
    })();
  }, []);

  const handleSetup = async () => {
    if (!authUsername || !authPassword) {
      setStatus('Vui lòng nhập tên đăng nhập và mật khẩu.', true);
      return;
    }
    setIsProcessing(true);
    try {
      await api.setupAdmin(authUsername, authPassword);
      setStatus('Đã tạo quản trị viên. Hãy đăng nhập.', false);
    } catch (err) {
      handleApiError(err, 'Lỗi khi thiết lập quản trị viên.');
    } finally {
      setIsProcessing(false);
    }
  };

  const handleLogin = async () => {
    if (!authUsername || !authPassword) {
      setStatus('Vui lòng nhập tên đăng nhập và mật khẩu.', true);
      return;
    }
    setIsProcessing(true);
    try {
      const res = await api.login(authUsername, authPassword);
      const nextSiteId = res.site_instance_id ?? siteInstanceId;
      if (res.site_instance_id && res.site_instance_id !== siteInstanceId) {
        editorDraftStore.clear();
        setSiteInstanceId(res.site_instance_id);
        setBuilderReset(value => value + 1);
        setBuilderDirty(false);
        markSectionsDirty(false);
      } else if (lastAuthenticatedUserId.current !== null &&
          lastAuthenticatedUserId.current !== res.user.id) {
        editorDraftStore.clear();
        setBuilderReset(value => value + 1);
        setBuilderDirty(false);
        markSectionsDirty(false);
      } else {
        setAuthGeneration(g => g + 1);
      }
      if (!res.site_instance_id && nextSiteId !== siteInstanceId) setSiteInstanceId(nextSiteId);
      lastAuthenticatedUserId.current = res.user.id;
      setAccountContextId(res.user.id);
      setAccountContextRole(res.user.role);
      setUser(res.user);
      setCsrfToken(res.csrf_token);
      setAuthPassword('');
      setStatus('Đăng nhập thành công.', false);
      await loadCptTypes();
      if (res.user.role === 'admin') {
        await loadPlugins();
      }
    } catch (err) {
      handleApiError(err, 'Lỗi khi đăng nhập.');
    } finally {
      setIsProcessing(false);
    }
  };

  const handleLogout = async () => {
    if ((editorDraftStore.hasAnyDirty() || contentDirty || builderDirty || postsDirty || pagesDirty || termDirtyRef.current || sectionsDirtyRef.current) && !window.confirm('Có dữ liệu chưa lưu. Đăng xuất sẽ bỏ bản nhập hiện tại. Tiếp tục?')) {
      return;
    }
    setIsProcessing(true);
    try {
      await api.logout(csrfToken);
      editorDraftStore.clear();
      setUser(null);
      setCsrfToken(null);
      setAuthUsername('');
      setAuthPassword('');
      setAccountContextId(null);
      setAccountContextRole(null);
      setPlugins([]);
      setSelectedPluginId('');
      setPluginRunOutput('');
      setBuilderReset(value => value + 1);
      setContentRevision(previous => previous + 1);
      setContentDirty(false);
      setBuilderDirty(false);
      setPostsDirty(false);
      setPagesDirty(false);
      markSectionsDirty(false);
      markTermDirty(false);
      lastAuthenticatedUserId.current = null;
      setRoute(parseAdminRoute('#/dashboard', window.location.pathname));
      setCptTypes([]);
      window.history.replaceState(null, '', window.location.pathname + window.location.search + '#/dashboard');
      acceptedHashRef.current = '#/dashboard';
      setStatus('Đã đăng xuất.', false);
    } catch (err) {
      handleApiError(err, 'Lỗi khi đăng xuất.');
    } finally {
      setIsProcessing(false);
    }
  };

  const handleOwnAccountChanged = () => {
    editorDraftStore.quarantineAll();
    setUser(null);
    setCsrfToken(null);
    setAuthPassword('');
    setStatus('Phiên tài khoản đã được thu hồi. Vui lòng đăng nhập lại.', false);
  };
  const handlePluginUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file || !csrfToken) return;
    setIsProcessing(true);
    setStatus('Đang cài đặt plugin...', false);
    try {
      const summary = await api.installPlugin(file, csrfToken);
      setStatus(`Đã cài đặt plugin ${summary.name} (${summary.id}).`, false);
      setSelectedPluginId(summary.id);
      await loadPlugins();
    } catch (err) {
      handleApiError(err, 'Không thể cài đặt plugin.');
    } finally {
      setIsProcessing(false);
      e.target.value = '';
    }
  };

  const handleDisablePlugin = async (id: string) => {
    if (!csrfToken) return;
    setIsProcessing(true);
    try {
      await api.disablePlugin(id, csrfToken);
      setStatus(`Đã vô hiệu hóa plugin ${id}.`, false);
      await loadPlugins();
    } catch (err) {
      handleApiError(err, 'Không thể vô hiệu hóa plugin.');
    } finally {
      setIsProcessing(false);
    }
  };

  const handleEnablePlugin = async (id: string) => {
    if (!csrfToken) return;
    setIsProcessing(true);
    try {
      await api.enablePlugin(id, csrfToken);
      setStatus(`Đã kích hoạt plugin ${id}.`, false);
      await loadPlugins();
    } catch (err) {
      handleApiError(err, 'Không thể kích hoạt plugin.');
    } finally {
      setIsProcessing(false);
    }
  };

  const handleRunPlugin = async () => {
    if (!selectedPluginId || !csrfToken) return;
    setIsProcessing(true);
    setStatus(`Đang thực thi plugin ${selectedPluginId}...`, false);
    setPluginRunOutput('');
    try {
      const res = await api.runPlugin(selectedPluginId, pluginInputJson, csrfToken);
      setPluginRunOutput(JSON.stringify(res, null, 2));
      if (res.success) {
        setStatus(`Thực thi thành công [correlation: ${res.correlation_id}]`, false);
      } else {
        setStatus(`Plugin lỗi: ${res.error || res.code} [correlation: ${res.correlation_id}]`, true);
      }
      await loadPlugins();
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Lỗi khi thực thi plugin.';
      setPluginRunOutput(
        JSON.stringify(
          {
            success: false,
            error: msg,
            status: err instanceof api.ApiClientError ? err.status : undefined,
          },
          null,
          2,
        ),
      );
      handleApiError(err, 'Lỗi khi thực thi plugin.');
    } finally {
      setIsProcessing(false);
    }
  };

  return (
    <AdminShell
      user={user}
      authUsername={authUsername}
      authPassword={authPassword}
      statusMessage={statusMessage}
      isError={isError}
      isProcessing={isProcessing}
      onUsernameChange={setAuthUsername}
      onPasswordChange={setAuthPassword}
      onSetup={handleSetup}
      onLogin={handleLogin}
      onLogout={handleLogout}
      route={route}
      routeForbidden={Boolean(user && !isRouteAllowed(route, user.role))}
      cptTypes={cptTypes}
      onNavigateHash={navigateHash}
      dashboardContent={<DashboardPage onApiError={handleApiError} />}
      hasUnsavedChanges={contentDirty || builderDirty || postsDirty || pagesDirty || termDirty || sectionsDirty}
      sidebarContent={<BuilderWorkspace
        key={builderIdentity(lastBuilderRouteRef.current) ?? 'homepage'}
        route={lastBuilderRouteRef.current}
        active={route.view === 'builder' && isRouteAllowed(route, user?.role ?? 'editor')}
        onNavigateHash={navigateHash}
        user={user} csrfToken={csrfToken} resetToken={builderReset} themeRevision={themeRevision}
        onDirtyChange={setBuilderDirty} onApiError={handleApiError}
      />}
      previewContent={null}
      themesContent={<ThemesPanel mode="themes" csrfToken={csrfToken} onApiError={handleApiError}
        onInstalled={() => setThemeRevision(value => value + 1)}
        onActivated={() => setThemeRevision(value => value + 1)}
      />}
      menusContent={<MenusPanel csrfToken={csrfToken} themeRevision={themeRevision} onApiError={(error) => handleApiError(error, 'Lỗi quản lý menu.')} />}
      themeOptionsContent={<ThemesPanel mode="options" csrfToken={csrfToken} onApiError={handleApiError}
        onInstalled={() => setThemeRevision(value => value + 1)}
        onActivated={() => setThemeRevision(value => value + 1)}
      />}
      libraryContent={<TemplateLibraryPage key={`${siteInstanceId}:${user?.id ?? 'none'}:${user?.role ?? 'none'}`}
        user={user} csrfToken={csrfToken} themeRevision={themeRevision}
        onApiError={handleApiError} onNavigateHash={navigateHash}
        onApplied={() => { setThemeRevision(v => v + 1); setBuilderReset(v => v + 1); }} />}
      schemaSettingsContent={
        <ContentTypeSchemaSettings
          selectedTypeKey={selectedSchemaTypeKey}
          onSelectTypeKey={key => {
            setSelectedSchemaTypeKey(key);
            if (key) navigateHash(`#/appearance/content-types/${key}`);
          }}
          csrfToken={csrfToken}
          onApiError={handleApiError}
          onContentTypesChanged={() => {
            void loadCptTypes();
          }}
        />
      }
      profileContent={
        <UserProfilePanel
          csrfToken={csrfToken}
          onApiError={handleApiError}
          onProfileUpdated={prof => {
            setUser(prev => (prev ? { ...prev, username: prof.username } : prev));
          }}
        />
      }
      toolsContent={route.sub === 'backup'
        ? <>
            <BackupsPanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, 'Lỗi sao lưu mã hóa.')} />
            <RestorePanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, 'Lỗi khôi phục & safe mode.')} />
          </>
        : route.sub === 'media-reconcile'
          ? <ReconcilePanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, 'Lỗi đối soát & dọn cách ly.')} />
          : route.sub === 'health' ? <SystemStatus /> : null}
      settingsContent={route.sub === 'general'
        ? <GeneralSettingsPanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, 'Lỗi cài đặt website.')} />
        : route.sub === 'seo'
          ? <SeoPanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, 'Lỗi SEO trang chủ.')} />
          : route.sub === 'contact'
            ? <ContactPanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, 'Lỗi quản lý form liên hệ.')} />
            : route.sub === 'smtp'
              ? <SmtpSettingsPanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, 'Lỗi cấu hình SMTP.')} />
              : null}
      usersContent={accountContextRole === 'admin' && <UsersPanel
        key={accountContextId ?? 'anonymous'}
        mode={route.view === 'users' && route.sub === 'new' ? 'new' : 'list'}
        csrfToken={csrfToken}
        currentUserId={accountContextId}
        onApiError={handleApiError}
        onOwnAccountChanged={handleOwnAccountChanged}
      />}
      pluginsContent={<PluginsPanel
        mode={route.view === 'plugins' && route.sub === 'install' ? 'install' : 'list'}
        plugins={plugins}
        selectedPluginId={selectedPluginId}
        pluginInputJson={pluginInputJson}
        pluginRunOutput={pluginRunOutput}
        isProcessing={isProcessing}
        onPluginUpload={handlePluginUpload}
        onSelectPlugin={setSelectedPluginId}
        onDisablePlugin={handleDisablePlugin}
        onEnablePlugin={handleEnablePlugin}
        onPluginInputChange={setPluginInputJson}
        onRunPlugin={handleRunPlugin}
        csrfToken={csrfToken}
        onUnauthorized={() => {
          editorDraftStore.quarantineAll();
          setUser(null);
          setCsrfToken(null);
        }}
      />}
      contentContent={
        route.view === 'cpt' && route.typeKey ? (
          <SingleEditWorkspace
            key={`cpt-${route.typeKey}-${contentRevision}`}
            typeKey={route.typeKey}
            mode={route.sub === 'new' ? 'new' : route.sub === 'edit' ? 'edit' : 'list'}
            entryId={route.entryId ?? null}
            user={user}
            siteInstanceId={siteInstanceId}
            authGeneration={authGeneration}
            draftUuid={draftUuidFromHash(route.rawHash) ?? undefined}
            routeHash={route.rawHash}
            isActive={route.view === 'cpt'}
            csrfToken={csrfToken}
            onApiError={handleApiError}
            onDirtyChange={setContentDirty}
            onNavigateHash={navigateHash}
          />
        ) : null
      }
      postsContent={
        <SingleEditWorkspace
          key={'posts-' + contentRevision}
          typeKey="post"
          mode={route.view === 'posts' ? (route.sub === 'new' ? 'new' : route.sub === 'edit' ? 'edit' : 'list') : 'list'}
          entryId={route.view === 'posts' ? (route.entryId ?? null) : null}
          user={user}
          siteInstanceId={siteInstanceId}
          authGeneration={authGeneration}
          draftUuid={draftUuidFromHash(route.rawHash) ?? undefined}
          routeHash={route.view === 'posts' ? route.rawHash : '#/posts'}
          isActive={route.view === 'posts'}
          csrfToken={csrfToken}
          onApiError={handleApiError}
          onDirtyChange={setPostsDirty}
          onNavigateHash={navigateHash}
        />
      }
      pagesContent={
        <SingleEditWorkspace
          key={'pages-' + contentRevision}
          typeKey="page"
          mode={route.view === 'pages' ? (route.sub === 'new' ? 'new' : route.sub === 'edit' ? 'edit' : 'list') : 'list'}
          entryId={route.view === 'pages' ? (route.entryId ?? null) : null}
          user={user}
          siteInstanceId={siteInstanceId}
          authGeneration={authGeneration}
          draftUuid={draftUuidFromHash(route.rawHash) ?? undefined}
          routeHash={route.view === 'pages' ? route.rawHash : '#/pages'}
          isActive={route.view === 'pages'}
          csrfToken={csrfToken}
          onApiError={handleApiError}
          onDirtyChange={setPagesDirty}
          onNavigateHash={navigateHash}
        />
      }
      sectionsContent={
        <SectionsPage key={`${siteInstanceId}:${user?.id ?? 'none'}:${user?.role ?? 'none'}`}
          route={route} user={user} csrfToken={csrfToken} onApiError={handleApiError}
          onDirtyChange={markSectionsDirty} onNavigateHash={navigateHash} />
      }
      mediaContent={<MediaRoute user={user} csrfToken={csrfToken} onApiError={handleApiError} />}
      taxonomyContent={route.taxonomyMode === 'edit' || route.taxonomyMode === 'new'
        ? <TermEditPage key={`term-${route.taxonomyKind}-${route.taxonomyMode}-${route.entryId ?? 'new'}-${user?.id ?? 'guest'}-${user?.role ?? 'none'}`}
            kind={route.taxonomyKind ?? 'category'} mode={route.taxonomyMode}
            termId={route.entryId ?? null} user={user} csrfToken={csrfToken}
            onApiError={handleApiError} onDirtyChange={markTermDirty} onNavigateHash={navigateHash} />
        : <TaxonomyPage kind={route.taxonomyKind ?? 'category'} user={user} csrfToken={csrfToken}
            onApiError={handleApiError} onNavigateHash={navigateHash} />}
    />
  );
};
