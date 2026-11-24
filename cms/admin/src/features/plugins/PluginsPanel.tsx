import React, { useEffect, useRef, useState } from 'react';
import type { PluginSettingsSnapshot, PluginSummary } from '../../types.ts';
import { ApiClientError } from '../../api/client.ts';
import { getPluginSettings, getPlugins, savePluginSettings } from '../../api/plugins.ts';
import { ZipUploadField } from '../../components/FormPrimitives.tsx';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import {
  buildOwnerOptionsSubmitPayload,
  OwnerOptionsForm,
} from '../shared/OwnerOptionsForm.tsx';
import { DeclarativePluginUi } from './DeclarativePluginUi.tsx';

interface PluginsPanelProps {
  mode?: 'list' | 'install';
  plugins: PluginSummary[];
  selectedPluginId: string;
  pluginInputJson: string;
  pluginRunOutput: string;
  isProcessing: boolean;
  onPluginUpload: (event: React.ChangeEvent<HTMLInputElement>) => void;
  onSelectPlugin: (id: string) => void;
  onDisablePlugin: (id: string) => void;
  onEnablePlugin: (id: string) => void;
  onPluginInputChange: (json: string) => void;
  onRunPlugin: () => void;
  csrfToken?: string | null;
  onUnauthorized?: () => void;
}

function permissionDetails(plugin: PluginSummary): string[] {
  const permissions = plugin.permissions;
  return [
    ...(permissions?.sqlite ?? []).map(value => 'SQLite: ' + value),
    ...(permissions?.http ?? []).map(value => 'HTTP: ' + value),
  ];
}

function readableError(error: unknown): string {
  return error instanceof Error ? error.message : 'Không thể tải danh sách plugin.';
}

/**
 * Scope M3-S28 + Plugin S02: keep App-owned lifecycle/run mutations intact while
 * hosting owner-isolated plugin settings (`GET/PUT /api/admin/plugins/{id}/settings`)
 * through shared `<OwnerOptionsForm>`.
 */
export const PluginsPanel: React.FC<PluginsPanelProps> = ({
  mode = 'list',
  plugins,
  selectedPluginId,
  pluginInputJson,
  pluginRunOutput,
  isProcessing,
  onPluginUpload,
  onSelectPlugin,
  onDisablePlugin,
  onEnablePlugin,
  onPluginInputChange,
  onRunPlugin,
  csrfToken,
  onUnauthorized,
}) => {
  const [visiblePlugins, setVisiblePlugins] = useState<PluginSummary[]>(plugins);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState('');
  const [pendingAction, setPendingAction] = useState('');
  const [notice, setNotice] = useState('');
  const [settingsPluginId, setSettingsPluginId] = useState('');
  const [settingsSnapshot, setSettingsSnapshot] = useState<PluginSettingsSnapshot | null>(null);
  const [settingsDraft, setSettingsDraft] = useState<Record<string, unknown>>({});
  const [settingsLoading, setSettingsLoading] = useState(false);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [settingsNotice, setSettingsNotice] = useState('');
  const [settingsError, setSettingsError] = useState('');
  const [settingsReloadTick, setSettingsReloadTick] = useState(0);
  const latestRequest = useRef(0);
  const pendingRef = useRef(false);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    const controller = new AbortController();
    const current = ++latestRequest.current;
    getPlugins(controller.signal)
      .then(list => {
        if (mounted.current && latestRequest.current === current) {
          setVisiblePlugins(list);
          setLoadError('');
          setLoading(false);
        }
      })
      .catch(error => {
        if (controller.signal.aborted || !mounted.current || latestRequest.current !== current) return;
        setLoadError(readableError(error));
        setLoading(false);
      });
    return () => {
      mounted.current = false;
      controller.abort();
      latestRequest.current += 1;
    };
  }, []);

  // App's existing callbacks own mutation requests, global status and 401 logout.
  useEffect(() => {
    setVisiblePlugins(plugins);
  }, [plugins]);

  const settingsTargetPlugin = visiblePlugins.find(
    plugin => plugin.id === (settingsPluginId || selectedPluginId),
  );
  const effectiveSettingsPluginId = settingsTargetPlugin?.id ?? '';

  useEffect(() => {
    if (mode !== 'list' || !effectiveSettingsPluginId) {
      setSettingsSnapshot(null);
      setSettingsDraft({});
      setSettingsError('');
      setSettingsNotice('');
      setSettingsLoading(false);
      return;
    }
    const controller = new AbortController();
    let active = true;
    setSettingsLoading(true);
    setSettingsError('');
    setSettingsNotice('');
    void getPluginSettings(effectiveSettingsPluginId, controller.signal)
      .then(snapshot => {
        if (!active) return;
        setSettingsSnapshot(snapshot);
        setSettingsDraft(snapshot.values);
      })
      .catch(cause => {
        if (!active || controller.signal.aborted) return;
        if (cause instanceof ApiClientError && cause.status === 401) {
          onUnauthorized?.();
        }
        setSettingsSnapshot(null);
        setSettingsError(
          cause instanceof Error ? cause.message : 'Không thể tải cài đặt plugin.',
        );
      })
      .finally(() => {
        if (active) setSettingsLoading(false);
      });
    return () => {
      active = false;
      controller.abort();
    };
  }, [mode, effectiveSettingsPluginId, settingsReloadTick, onUnauthorized]);

  const refresh = async () => {
    const current = ++latestRequest.current;
    setLoading(true);
    try {
      const list = await getPlugins();
      if (mounted.current && latestRequest.current === current) {
        setVisiblePlugins(list);
        setLoadError('');
      }
    } catch (error) {
      if (mounted.current && latestRequest.current === current) {
        setLoadError(readableError(error));
      }
    } finally {
      if (mounted.current && latestRequest.current === current) setLoading(false);
    }
  };

  const invoke = async (label: string, operation: () => void) => {
    if (pendingRef.current || isProcessing) return;
    pendingRef.current = true;
    setPendingAction(label);
    setNotice('');
    try {
      // The current App callbacks are async functions typed as void in this UI
      // contract. Await their actual returned promise before re-reading server state.
      await operation();
      await refresh();
    } finally {
      pendingRef.current = false;
      if (mounted.current) setPendingAction('');
    }
  };

  const upload = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.currentTarget.files?.[0];
    if (!file || pendingRef.current || isProcessing) return;
    if (!file.name.toLowerCase().endsWith('.zip')) {
      setNotice('Chỉ chấp nhận tệp .zip của plugin.');
      event.currentTarget.value = '';
      return;
    }
    if (file.size > 5 * 1024 * 1024) {
      setNotice('Gói plugin vượt giới hạn tải lên 5 MiB.');
      event.currentTarget.value = '';
      return;
    }
    // The install endpoint is replace-and-enable, not a dry-run. Warn before
    // submitting whenever an existing plugin might be replaced.
    if (visiblePlugins.length > 0 &&
        !window.confirm('Cài ZIP sẽ tự kích hoạt plugin. Nếu ID trùng plugin đã cài, phiên bản hiện tại sẽ bị THAY THẾ và BẬT LẠI. Bạn có muốn tiếp tục?')) {
      event.currentTarget.value = '';
      return;
    }
    void invoke('Đang cài đặt plugin ZIP...', () => onPluginUpload(event));
  };

  const enable = (plugin: PluginSummary) => {
    const grants = permissionDetails(plugin);
    const permissionText = grants.length > 0 ? grants.join('\n') : 'Không khai báo quyền SQLite hoặc HTTP.';
    if (!window.confirm('Bật plugin ' + plugin.name + ' (' + plugin.id + ')?\n\nQuyền theo manifest:\n' + permissionText)) return;
    void invoke('Đang bật ' + plugin.name + '...', () => onEnablePlugin(plugin.id));
  };

  const disable = (plugin: PluginSummary) => {
    void invoke('Đang tắt ' + plugin.name + '...', () => onDisablePlugin(plugin.id));
  };

  const handleSaveSettings = async () => {
    if (!settingsSnapshot || !csrfToken || settingsSaving) return;
    setSettingsSaving(true);
    setSettingsError('');
    setSettingsNotice('');
    try {
      const payload = buildOwnerOptionsSubmitPayload(settingsSnapshot.schema, settingsDraft);
      const saved = await savePluginSettings(
        settingsSnapshot.plugin_id,
        payload,
        settingsSnapshot.version,
        csrfToken,
      );
      setSettingsSnapshot(saved);
      setSettingsDraft(saved.values);
      setSettingsNotice('Đã lưu cài đặt plugin.');
    } catch (cause) {
      if (cause instanceof ApiClientError && cause.status === 401) {
        onUnauthorized?.();
        setSettingsError(
          'Phiên đăng nhập đã hết hạn (401). Thay đổi chưa lưu vẫn được giữ lại.',
        );
      } else if (cause instanceof ApiClientError && cause.status === 412) {
        setSettingsError(
          'Cài đặt plugin đã thay đổi ở phiên khác (412). Dữ liệu bạn vừa sửa vẫn được giữ lại.',
        );
      } else {
        setSettingsError(
          cause instanceof Error ? cause.message : 'Không thể lưu cài đặt plugin.',
        );
      }
    } finally {
      setSettingsSaving(false);
    }
  };

  const handleSettingsApiError = (cause: unknown, fallback: string) => {
    if (cause instanceof ApiClientError && cause.status === 401) {
      onUnauthorized?.();
    }
    setSettingsError(cause instanceof Error ? cause.message : fallback);
  };

  const busy = isProcessing || Boolean(pendingAction);
  const selected = visiblePlugins.find(plugin => plugin.id === selectedPluginId);
  const canRun = Boolean(selected?.enabled) && !busy;

  return (
    <div id="plugin-management-section" className="min-w-0 space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">{mode === 'install' ? 'Cài đặt plugin' : 'Quản lý Plugins'}</h2>
          <p className="mt-0.5 text-sm text-zinc-600">
            {mode === 'install'
              ? 'Cài gói plugin ZIP. Gói đã cài sẽ được tự bật sau khi được xác thực.'
              : 'Kiểm tra quyền, cấu hình cài đặt và bật hoặc tắt plugin chạy trong Wasmi sandbox.'}
          </p>
        </div>
        <AdminButton variant="secondary" size="sm" onClick={() => void refresh()} disabled={busy || loading}>
          {loading ? 'Đang tải...' : 'Tải lại danh sách'}
        </AdminButton>
      </div>

      {mode === 'install' && <section className="max-w-2xl rounded-xl border border-zinc-200 bg-zinc-50 p-4 sm:p-5" aria-labelledby="plugin-upload-heading">
        <h3 id="plugin-upload-heading" className="mb-2 text-sm font-semibold text-zinc-950">Cài đặt plugin</h3>
        <ZipUploadField id="plugin-zip" label="Chọn tệp plugin ZIP (tối đa 5 MiB)" onChange={upload}
          disabled={busy || loading || Boolean(loadError)} />
        <p className="mt-2.5 text-xs leading-5 text-amber-800">
          Lưu ý: tải ZIP sẽ tự bật plugin. Nếu ID đã tồn tại, hệ thống thay thế gói cũ và bật lại plugin đó.
          Hãy kiểm tra quyền trong danh sách trước khi bật plugin đã tắt.
        </p>
      </section>}

      {pendingAction && <AdminAlert tone="info" aria-live="polite">{pendingAction}</AdminAlert>}
      {notice && <AdminAlert tone="error">{notice}</AdminAlert>}

      {mode === 'list' && <>
      <section className="space-y-3" aria-labelledby="plugin-list-heading">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 id="plugin-list-heading" className="text-sm font-semibold text-zinc-900">
            Plugins đã cài ({visiblePlugins.length})
          </h3>
          {loading && <span role="status" className="text-xs text-zinc-500">Đang đồng bộ với máy chủ...</span>}
        </div>
        {loadError && (
          <AdminAlert tone="error" className="space-y-2">
            <p>Không cập nhật được danh sách: {loadError}</p>
            <AdminButton variant="secondary" size="sm" disabled={busy || loading} onClick={() => void refresh()}>
              Thử tải lại
            </AdminButton>
          </AdminAlert>
        )}
        {!loading && visiblePlugins.length === 0 && !loadError && (
          <p className="rounded-xl border border-dashed border-zinc-300 p-5 text-sm text-zinc-500">
            Chưa có plugin nào. Chọn gói ZIP để cài đặt.
          </p>
        )}
        <div className="grid min-w-0 grid-cols-1 gap-3 xl:grid-cols-2">
          {visiblePlugins.map(plugin => {
            const grants = permissionDetails(plugin);
            const isSettingsOpen = effectiveSettingsPluginId === plugin.id;
            return (
              <article key={plugin.id} className="min-w-0 rounded-xl border border-zinc-200 bg-white p-4" data-plugin-id={plugin.id}>
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <h4 className="break-words text-sm font-semibold text-zinc-950">{plugin.name}</h4>
                    <p className="mt-0.5 break-all text-xs text-zinc-500">{plugin.id} · v{plugin.version}</p>
                  </div>
                  <AdminBadge tone={plugin.enabled ? 'success' : 'neutral'}>
                    {plugin.enabled ? 'Đang bật' : 'Đã tắt'}
                  </AdminBadge>
                </div>
                {plugin.description && <p className="mt-2.5 break-words text-sm text-zinc-600">{plugin.description}</p>}
                <div className="mt-3 rounded-lg border border-zinc-100 bg-zinc-50 p-3">
                  <h5 className="text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Quyền theo manifest</h5>
                  {grants.length === 0
                    ? <p className="mt-1 text-xs text-zinc-500">Không yêu cầu quyền SQLite hoặc HTTP.</p>
                    : <ul className="mt-1.5 space-y-1 text-xs text-zinc-700">
                        {grants.map((grant, index) => <li key={index} className="break-all">{grant}</li>)}
                      </ul>}
                </div>
                {plugin.last_error && (
                  <AdminAlert tone="error" role="status" className="mt-3 break-words text-xs">
                    Lỗi gần nhất: {plugin.last_error}
                  </AdminAlert>
                )}
                <div className="mt-3.5 flex flex-wrap gap-2">
                  {plugin.enabled
                    ? <AdminButton variant="secondary" size="sm" disabled={busy} onClick={() => disable(plugin)}
                        aria-label={'Tắt plugin ' + plugin.name}>Tắt</AdminButton>
                    : <AdminButton variant="secondary" size="sm" disabled={busy} onClick={() => enable(plugin)}
                        aria-label={'Bật plugin ' + plugin.name}>Bật / Thử lại</AdminButton>}
                  <AdminButton
                    variant={isSettingsOpen ? 'primary' : 'secondary'}
                    size="sm"
                    disabled={busy}
                    onClick={() => setSettingsPluginId(plugin.id)}
                    aria-pressed={isSettingsOpen}
                    aria-label={'Cài đặt plugin ' + plugin.name}
                  >
                    {isSettingsOpen ? 'Đang cấu hình' : 'Cài đặt'}
                  </AdminButton>
                  <AdminButton variant={selectedPluginId === plugin.id ? 'primary' : 'secondary'} size="sm" disabled={busy || !plugin.enabled}
                    onClick={() => {
                      setSettingsPluginId(plugin.id);
                      onSelectPlugin(plugin.id);
                    }}
                    aria-pressed={selectedPluginId === plugin.id}>
                    {selectedPluginId === plugin.id ? 'Đã chọn để chạy' : 'Chọn chạy'}
                  </AdminButton>
                </div>
              </article>
            );
          })}
        </div>
      </section>

      {settingsTargetPlugin && (
        <section className="min-w-0 max-w-2xl space-y-3 border-t border-zinc-200 pt-5" aria-labelledby="plugin-settings-heading">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h3 id="plugin-settings-heading" className="text-sm font-semibold text-zinc-950">
              Cài đặt plugin: {settingsTargetPlugin.name} ({settingsTargetPlugin.id})
            </h3>
            <AdminButton
              variant="secondary"
              size="sm"
              disabled={settingsLoading || settingsSaving}
              onClick={() => setSettingsReloadTick(n => n + 1)}
            >
              Tải lại từ máy chủ
            </AdminButton>
          </div>
          {settingsLoading && (
            <p role="status" className="text-sm text-zinc-600">Đang tải cài đặt plugin…</p>
          )}
          {!settingsLoading && !settingsSnapshot && settingsError && (
            <AdminAlert tone="error">{settingsError}</AdminAlert>
          )}
          {!settingsLoading && settingsSnapshot && settingsSnapshot.schema.length === 0 && (
            <p className="text-xs text-zinc-500">
              Plugin này không khai báo schema cài đặt (settings_schema).
            </p>
          )}
          {!settingsLoading && settingsSnapshot && settingsSnapshot.schema.length > 0 && (
            <OwnerOptionsForm
              ownerKind="plugin"
              ownerId={settingsSnapshot.plugin_id}
              title={`Cài đặt plugin (${settingsTargetPlugin.name})`}
              schema={settingsSnapshot.schema}
              values={settingsDraft}
              savedValues={settingsSnapshot.values}
              version={settingsSnapshot.version}
              csrfToken={csrfToken ?? null}
              saving={settingsSaving}
              notice={settingsNotice}
              error={settingsError}
              submitLabel={settingsSaving ? 'Đang lưu…' : 'Lưu cài đặt'}
              testId="plugin-settings-form"
              onChange={(next) => {
                setSettingsDraft(next);
                setSettingsNotice('');
                setSettingsError('');
              }}
              onSubmit={() => void handleSaveSettings()}
              onApiError={handleSettingsApiError}
            />
          )}
        </section>
      )}

      <section className="min-w-0 max-w-2xl space-y-3 border-t border-zinc-200 pt-5" aria-labelledby="plugin-run-heading">
        <h3 id="plugin-run-heading" className="text-sm font-semibold text-zinc-950">Thử thực thi plugin</h3>
        {selected
          ? <p className="break-all text-xs text-zinc-600">Plugin đã chọn: <strong className="text-zinc-900">{selected.name}</strong> ({selected.id})</p>
          : <p className="text-xs text-zinc-500">Chưa chọn plugin để chạy.</p>}
        {selected && !selected.enabled && (
          <p role="status" className="text-xs text-amber-800">Plugin đang tắt. Có thể bật lại sau khi kiểm tra quyền.</p>
        )}
        <label htmlFor="plugin-input-json" className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
          <span>Payload JSON</span>
          <textarea id="plugin-input-json" rows={4} className="w-full min-w-0 rounded-md border border-zinc-300 bg-white p-2.5 font-mono text-xs text-zinc-900"
            value={pluginInputJson} onChange={event => onPluginInputChange(event.target.value)}
            disabled={busy} placeholder='{"text": "Dữ liệu mẫu"}' />
        </label>
        <AdminButton id="btn-run-plugin" variant="primary" onClick={() => void invoke('Đang thực thi plugin...', onRunPlugin)}
          disabled={!canRun}>Chạy Plugin</AdminButton>
        {pluginRunOutput && (
          <div>
            <p className="mb-1.5 text-xs font-semibold text-zinc-700">Kết quả</p>
            <pre id="plugin-output" className="max-h-48 max-w-full overflow-auto whitespace-pre-wrap break-all rounded-lg border border-zinc-200 bg-zinc-50 p-3 text-xs text-zinc-800">{pluginRunOutput}</pre>
          </div>
        )}
      </section>
      {selected && selected.enabled && (
        <DeclarativePluginUi
          pluginId={selected.id}
          csrfToken={csrfToken ?? null}
          onUnauthorized={onUnauthorized ?? (() => {})}
        />
      )}
      </>}
    </div>
  );
};
