import React, { useState, useEffect, useRef } from 'react';
import { AuthUser, BlockData, BlockRegistry, PluginSummary } from './types.ts';
import * as api from './api/index.ts';
import { AdminShell } from './shell/AdminShell.tsx';
import { SystemStatus } from './features/system/SystemStatus.tsx';
import { BuilderControls, BuilderPreview, normalizeValues } from './builder/BuilderWorkspace.tsx';
import { UsersPanel } from './features/users/UsersPanel.tsx';
import { PluginsPanel } from './features/plugins/PluginsPanel.tsx';

export const App: React.FC = () => {
  const [user, setUser] = useState<AuthUser | null>(null);
  const [csrfToken, setCsrfToken] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<string>('Đang kiểm tra trạng thái phiên...');
  const [isError, setIsError] = useState<boolean>(false);
  const [isProcessing, setIsProcessing] = useState<boolean>(false);

  // Auth inputs
  const [authUsername, setAuthUsername] = useState<string>('');
  const [authPassword, setAuthPassword] = useState<string>('');

  // Admin user creation inputs
  const [editorUsername, setEditorUsername] = useState<string>('');
  const [editorPassword, setEditorPassword] = useState<string>('');

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
  blockDataRef.current = blockData;

  const setStatus = (msg: string, error: boolean = false) => {
    setStatusMessage(msg);
    setIsError(error);
  };

  const handleApiError = (err: unknown, fallback: string) => {
    const is401 = err instanceof api.ApiClientError && err.status === 401;
    const msg = err instanceof Error ? err.message : fallback;
    setStatus(msg, true);
    if (is401) {
      setUser(null);
      setCsrfToken(null);
      // Session expired: preserve blockData (unsaved inputs) as required by contract!
    }
  };

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
    setIsProcessing(true);
    try {
      await api.logout(csrfToken);
      setUser(null);
      setCsrfToken(null);
      setAuthUsername('');
      setAuthPassword('');
      setEditorUsername('');
      setEditorPassword('');
      setPlugins([]);
      setSelectedPluginId('');
      setPluginRunOutput('');
      setRegistry({});
      setBlockData({ type: '', values: {} });
      setPreviewHtml('');
      setStatus('Đã đăng xuất.', false);
    } catch (err) {
      handleApiError(err, 'Lỗi khi đăng xuất.');
    } finally {
      setIsProcessing(false);
    }
  };

  const handleCreateEditor = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!csrfToken) return;
    if (!editorUsername || !editorPassword) {
      setStatus('Vui lòng nhập đủ thông tin biên tập viên.', true);
      return;
    }
    setIsProcessing(true);
    try {
      const created = await api.createEditor(editorUsername, editorPassword, csrfToken);
      setStatus(`Đã tạo biên tập viên "${created.username}".`, false);
      setEditorUsername('');
      setEditorPassword('');
    } catch (err) {
      handleApiError(err, 'Lỗi khi tạo biên tập viên.');
    } finally {
      setIsProcessing(false);
    }
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
    setBlockData(prev => ({
      type: newType,
      values: normalizeValues(newType, registry, prev.values),
    }));
  };

  const handleFieldChange = (name: string, val: string) => {
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
      sidebarContent={
        <>
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
          {user?.role === 'admin' && (
            <>
              <SystemStatus />
              <UsersPanel
                editorUsername={editorUsername}
                editorPassword={editorPassword}
                isProcessing={isProcessing}
                onEditorUsernameChange={setEditorUsername}
                onEditorPasswordChange={setEditorPassword}
                onCreateEditor={handleCreateEditor}
              />
              <PluginsPanel
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
              />
            </>
          )}
        </>
      }
      previewContent={<BuilderPreview previewHtml={previewHtml} />}
    />
  );
};
