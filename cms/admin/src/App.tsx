import React, { useState, useEffect, useRef, useCallback } from 'react';
import { AuthUser, BlockData, BlockRegistry, PluginSummary, type Role } from './types.ts';
import * as api from './api/index.ts';
import { AdminShell, isViewAllowed, parseAdminView, type AdminView } from './shell/AdminShell.tsx';
import { SystemStatus } from './features/system/SystemStatus.tsx';
import { GeneralSettingsPanel } from './features/system/GeneralSettingsPanel.tsx';
import { BuilderControls, BuilderPreview, normalizeValues } from './builder/BuilderWorkspace.tsx';
import { UsersPanel } from './features/users/UsersPanel.tsx';
import { PluginsPanel } from './features/plugins/PluginsPanel.tsx';
import { ContentWorkspace } from './features/content/ContentWorkspace.tsx';
import { PostPageWorkspace } from './features/posts/PostPageWorkspace.tsx';
import { MediaRoute } from './shell/MediaRoute.tsx';

export const App: React.FC = () => {
  const [user, setUser] = useState<AuthUser | null>(null);
  const [csrfToken, setCsrfToken] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<string>('Đang kiểm tra trạng thái phiên...');
  const [isError, setIsError] = useState<boolean>(false);
  const [isProcessing, setIsProcessing] = useState<boolean>(false);
  const [activeView, setActiveView] = useState<AdminView>(() => parseAdminView(window.location.hash, window.location.pathname));
  const [contentDirty, setContentDirty] = useState(false);
  const [builderDirty, setBuilderDirty] = useState(false);
  const [postsDirty, setPostsDirty] = useState(false);
  const [pagesDirty, setPagesDirty] = useState(false);
  const [contentRevision, setContentRevision] = useState(0);

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

  // Registry and editor state
  const [registry, setRegistry] = useState<BlockRegistry>({});
  const [blockData, setBlockData] = useState<BlockData>({ type: '', values: {} });
  const [previewHtml, setPreviewHtml] = useState<string>('');

  // Race prevention for preview responses
  const previewSeq = useRef<number>(0);
  const blockDataRef = useRef<BlockData>(blockData);
  // Retain a draft across a 401 only when the same account signs back in.
  const lastAuthenticatedUserId = useRef<number | null>(null);
  blockDataRef.current = blockData;

  // Hash routing works from /admin/ and /builder without server catch-all.
  // The existing editors stay mounted in AdminShell across route changes.
  useEffect(() => {
    const synchronize = () => {
      const requested = parseAdminView(window.location.hash, window.location.pathname);
      const next = user && !isViewAllowed(requested, user.role) ? 'dashboard' : requested;
      setActiveView(next);
      if (user && next !== requested) {
        window.history.replaceState(null, '', window.location.pathname + window.location.search + '#/' + next);
      }
    };
    synchronize();
    window.addEventListener('hashchange', synchronize);
    return () => window.removeEventListener('hashchange', synchronize);
  }, [user?.role]);

  useEffect(() => {
    if (!(builderDirty || contentDirty || postsDirty || pagesDirty)) return;
    const guard = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = '';
    };
    window.addEventListener('beforeunload', guard);
    return () => window.removeEventListener('beforeunload', guard);
  }, [builderDirty, contentDirty, postsDirty, pagesDirty]);

  const navigateTo = (view: AdminView) => {
    if (!user || !isViewAllowed(view, user.role)) return;
    setActiveView(view);
    if (window.location.hash !== '#/' + view) window.location.hash = '/' + view;
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
      setUser(null);
      setCsrfToken(null);
      // Session expired: preserve blockData (unsaved inputs) as required by contract!
    }
  }, []);

  const loadRegistryAndPage = async (preserveExistingBlock: boolean = false) => {
    try {
      const reg = await api.getRegistry();
      setRegistry(reg);

      const blockKeys = Object.keys(reg);
      if (preserveExistingBlock && blockDataRef.current.type && blockKeys.includes(blockDataRef.current.type)) {
        setBlockData(prev => ({
          type: prev.type,
          values: normalizeValues(prev.type, reg, prev.values),
        }));
        return;
      }

      const savedPage = await api.getPage();
      if (savedPage && savedPage.type && blockKeys.includes(savedPage.type)) {
        setBlockData({
          type: savedPage.type,
          values: normalizeValues(savedPage.type, reg, savedPage.values),
        });
      } else if (blockKeys.length > 0) {
        const firstType = blockKeys[0];
        setBlockData({
          type: firstType,
          values: normalizeValues(firstType, reg, {}),
        });
      }
    } catch (err) {
      handleApiError(err, 'Lỗi khi tải dữ liệu registry hoặc trang.');
    }
  };

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
        setUser(auth.user);
        setCsrfToken(auth.csrf_token);
        setStatus(`Đang đăng nhập: ${auth.user.username} (${auth.user.role})`);
        await loadRegistryAndPage(false);
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
      if (lastAuthenticatedUserId.current !== null &&
          lastAuthenticatedUserId.current !== res.user.id) {
        // Never expose an expired account's unsaved Builder data to another user.
        const emptyBlock: BlockData = { type: '', values: {} };
        blockDataRef.current = emptyBlock;
        setBlockData(emptyBlock);
        setPreviewHtml('');
        setBuilderDirty(false);
      }
      lastAuthenticatedUserId.current = res.user.id;
      setAccountContextId(res.user.id);
      setAccountContextRole(res.user.role);
      setUser(res.user);
      setCsrfToken(res.csrf_token);
      setAuthPassword('');
      setStatus('Đăng nhập thành công.', false);
      const hasUnsaved = Boolean(blockDataRef.current.type);
      await loadRegistryAndPage(hasUnsaved);
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
    if ((contentDirty || builderDirty || postsDirty || pagesDirty) && !window.confirm('Có dữ liệu chưa lưu. Đăng xuất sẽ bỏ bản nhập hiện tại. Tiếp tục?')) {
      return;
    }
    setIsProcessing(true);
    try {
      await api.logout(csrfToken);
      setUser(null);
      setCsrfToken(null);
      setAuthUsername('');
      setAuthPassword('');
      setAccountContextId(null);
      setAccountContextRole(null);
      setPlugins([]);
      setSelectedPluginId('');
      setPluginRunOutput('');
      setRegistry({});
      setBlockData({ type: '', values: {} });
      setPreviewHtml('');
      setContentRevision(previous => previous + 1);
      setContentDirty(false);
      setBuilderDirty(false);
      setPostsDirty(false);
      setPagesDirty(false);
      lastAuthenticatedUserId.current = null;
      setActiveView('dashboard');
      window.history.replaceState(null, '', window.location.pathname + window.location.search + '#/dashboard');
      setStatus('Đã đăng xuất.', false);
    } catch (err) {
      handleApiError(err, 'Lỗi khi đăng xuất.');
    } finally {
      setIsProcessing(false);
    }
  };

  const handleOwnAccountChanged = () => {
    setUser(null);
    setCsrfToken(null);
    setAuthPassword('');
    setStatus('Phiên tài khoản đã được thu hồi. Vui lòng đăng nhập lại.', false);
  };
  const handleThemeUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file || !csrfToken) return;
    setIsProcessing(true);
    setStatus('Đang tải lên gói theme...', false);
    try {
      await api.uploadTheme(file, csrfToken);
      setStatus('Đã nạp theme.', false);
      await loadRegistryAndPage(true);
    } catch (err) {
      handleApiError(err, 'Lỗi khi tải lên gói theme.');
    } finally {
      setIsProcessing(false);
      e.target.value = '';
    }
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

  const handleTypeChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const newType = e.target.value;
    setBuilderDirty(true);
    setBlockData(prev => ({
      type: newType,
      values: normalizeValues(newType, registry, prev.values),
    }));
  };

  const handleFieldChange = (name: string, val: string) => {
    setBuilderDirty(true);
    setBlockData(prev => ({
      ...prev,
      values: {
        ...prev.values,
        [name]: val,
      },
    }));
  };

  const handlePreview = async () => {
    if (!csrfToken || !blockData.type) return;
    const currentSeq = ++previewSeq.current;
    setIsProcessing(true);
    setStatus('Đang tạo bản xem trước...', false);
    const payload: BlockData = {
      type: blockData.type,
      values: normalizeValues(blockData.type, registry, blockData.values),
    };
    try {
      const html = await api.previewBlock(payload, csrfToken);
      if (currentSeq === previewSeq.current) {
        setPreviewHtml(`<meta http-equiv="Content-Security-Policy" content="default-src 'none'">${html}`);
        setStatus('Đang xem trước', false);
      }
    } catch (err) {
      handleApiError(err, 'Lỗi khi xem trước block.');
    } finally {
      setIsProcessing(false);
    }
  };

  const handleSave = async () => {
    if (!csrfToken || !blockData.type) return;
    setIsProcessing(true);
    setStatus('Đang lưu thay đổi...', false);
    const payload: BlockData = {
      type: blockData.type,
      values: normalizeValues(blockData.type, registry, blockData.values),
    };
    try {
      const html = await api.saveBlock(payload, csrfToken);
      if (JSON.stringify(payload) === JSON.stringify(blockDataRef.current)) setBuilderDirty(false);
      setPreviewHtml(`<meta http-equiv="Content-Security-Policy" content="default-src 'none'">${html}`);
      setStatus('Đã lưu', false);
    } catch (err) {
      handleApiError(err, 'Lỗi khi lưu block.');
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
      activeView={activeView}
      onViewChange={navigateTo}
      hasUnsavedChanges={contentDirty || builderDirty || postsDirty || pagesDirty}
      sidebarContent={
        <BuilderControls
          role={user?.role}
          registry={registry}
          blockData={blockData}
          isProcessing={isProcessing}
          onThemeUpload={handleThemeUpload}
          onTypeChange={handleTypeChange}
          onFieldChange={handleFieldChange}
          onPreview={handlePreview}
          onSave={handleSave}
        />
      }
      previewContent={<BuilderPreview previewHtml={previewHtml} />}
      settingsContent={<>
        <GeneralSettingsPanel csrfToken={csrfToken} onApiError={(error) => handleApiError(error, 'Lỗi cài đặt website.')} />
        <div className="mt-8 border-t border-slate-200 pt-6"><SystemStatus /></div>
      </>}
      usersContent={accountContextRole === 'admin' && <UsersPanel
        key={accountContextId ?? 'anonymous'}
        csrfToken={csrfToken}
        currentUserId={accountContextId}
        onApiError={handleApiError}
        onOwnAccountChanged={handleOwnAccountChanged}
      />}
      pluginsContent={<PluginsPanel
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
      />}
      contentContent={
        <ContentWorkspace
          key={contentRevision}
          user={user}
          csrfToken={csrfToken}
          onApiError={handleApiError}
          onDirtyChange={setContentDirty}
        />
      }
      postsContent={
        <PostPageWorkspace
          key={'posts-' + contentRevision}
          type="post"
          user={user}
          csrfToken={csrfToken}
          onApiError={handleApiError}
          onDirtyChange={setPostsDirty}
        />
      }
      pagesContent={
        <PostPageWorkspace
          key={'pages-' + contentRevision}
          type="page"
          user={user}
          csrfToken={csrfToken}
          onApiError={handleApiError}
          onDirtyChange={setPagesDirty}
        />
      }
      mediaContent={<MediaRoute user={user} csrfToken={csrfToken} onApiError={handleApiError} />}
    />
  );
};
