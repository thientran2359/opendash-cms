import React, { useEffect, useRef, useState } from 'react';
import type { PluginSummary } from '../../types.ts';
import { getPlugins } from '../../api/plugins.ts';
import { ZipUploadField } from '../../components/FormPrimitives.tsx';

interface PluginsPanelProps {
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
 * Scope M3-S28: keep the existing App-owned mutations. This panel owns only its
 * read/retry UX; it never changes Wasmi endpoints, role guards or App/AdminShell.
 */
export const PluginsPanel: React.FC<PluginsPanelProps> = ({
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
}) => {
  const [visiblePlugins, setVisiblePlugins] = useState<PluginSummary[]>(plugins);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState('');
  const [pendingAction, setPendingAction] = useState('');
  const [notice, setNotice] = useState('');
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

  const busy = isProcessing || Boolean(pendingAction);
  const selected = visiblePlugins.find(plugin => plugin.id === selectedPluginId);
  const canRun = Boolean(selected?.enabled) && !busy;

  return (
    <div id="plugin-management-section" className="min-w-0 space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-xl font-semibold text-slate-900">Quản lý Plugins</h2>
          <p className="mt-1 text-sm leading-6 text-slate-600">
            Cài gói ZIP, kiểm tra quyền và bật hoặc tắt plugin chạy trong Wasmi sandbox.
          </p>
        </div>
        <button type="button" className="btn-secondary" onClick={() => void refresh()} disabled={busy || loading}>
          {loading ? 'Đang tải...' : 'Tải lại danh sách'}
        </button>
      </div>

      <section className="rounded-xl border border-slate-200 bg-slate-50 p-4 sm:p-5" aria-labelledby="plugin-upload-heading">
        <h3 id="plugin-upload-heading" className="mb-2 font-semibold text-slate-900">Cài đặt plugin</h3>
        <ZipUploadField id="plugin-zip" label="Chọn tệp plugin ZIP (tối đa 5 MiB)" onChange={upload}
          disabled={busy || loading || Boolean(loadError)} />
        <p className="mt-2 text-sm leading-6 text-amber-800">
          Lưu ý: tải ZIP sẽ tự bật plugin. Nếu ID đã tồn tại, hệ thống thay thế gói cũ và bật lại plugin đó.
          Hãy kiểm tra quyền trong danh sách trước khi bật plugin đã tắt.
        </p>
      </section>

      {pendingAction && <p role="status" aria-live="polite" className="text-sm font-medium text-sky-800">{pendingAction}</p>}
      {notice && <p role="alert" className="text-sm text-rose-700">{notice}</p>}

      <section className="space-y-3" aria-labelledby="plugin-list-heading">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 id="plugin-list-heading" className="text-lg font-semibold text-slate-900">
            Plugins đã cài ({visiblePlugins.length})
          </h3>
          {loading && <span role="status" className="text-sm text-slate-500">Đang đồng bộ với máy chủ...</span>}
        </div>
        {loadError && (
          <div role="alert" className="rounded-lg border border-rose-200 bg-rose-50 p-3 text-sm text-rose-800">
            <p>Không cập nhật được danh sách: {loadError}</p>
            <button type="button" className="btn-secondary mt-3" disabled={busy || loading} onClick={() => void refresh()}>
              Thử tải lại
            </button>
          </div>
        )}
        {!loading && visiblePlugins.length === 0 && !loadError && (
          <p className="rounded-xl border border-dashed border-slate-300 p-5 text-sm text-slate-600">
            Chưa có plugin nào. Chọn gói ZIP để cài đặt.
          </p>
        )}
        <div className="grid min-w-0 grid-cols-1 gap-3 xl:grid-cols-2">
          {visiblePlugins.map(plugin => {
            const grants = permissionDetails(plugin);
            return (
              <article key={plugin.id} className="min-w-0 rounded-xl border border-slate-200 bg-white p-4 shadow-sm" data-plugin-id={plugin.id}>
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <h4 className="break-words font-semibold text-slate-900">{plugin.name}</h4>
                    <p className="mt-1 break-all text-xs text-slate-500">{plugin.id} · v{plugin.version}</p>
                  </div>
                  <span className={'shrink-0 rounded-full px-3 py-1 text-xs font-semibold ' +
                    (plugin.enabled ? 'bg-emerald-50 text-emerald-800' : 'bg-slate-100 text-slate-700')}>
                    {plugin.enabled ? 'Đang bật' : 'Đã tắt'}
                  </span>
                </div>
                {plugin.description && <p className="mt-3 break-words text-sm text-slate-600">{plugin.description}</p>}
                <div className="mt-3 rounded-lg bg-slate-50 p-3">
                  <h5 className="text-xs font-semibold uppercase tracking-wide text-slate-700">Quyền theo manifest</h5>
                  {grants.length === 0
                    ? <p className="mt-1 text-xs text-slate-600">Không yêu cầu quyền SQLite hoặc HTTP.</p>
                    : <ul className="mt-2 space-y-1 text-xs text-slate-700">
                        {grants.map((grant, index) => <li key={index} className="break-all">{grant}</li>)}
                      </ul>}
                </div>
                {plugin.last_error && (
                  <p role="status" className="mt-3 break-words rounded-lg bg-rose-50 p-3 text-sm text-rose-800">
                    Lỗi gần nhất: {plugin.last_error}
                  </p>
                )}
                <div className="mt-4 flex flex-wrap gap-2">
                  {plugin.enabled
                    ? <button type="button" className="btn-secondary" disabled={busy} onClick={() => disable(plugin)}
                        aria-label={'Tắt plugin ' + plugin.name}>Tắt</button>
                    : <button type="button" className="btn-secondary" disabled={busy} onClick={() => enable(plugin)}
                        aria-label={'Bật plugin ' + plugin.name}>Bật / Thử lại</button>}
                  <button type="button" className="btn-secondary" disabled={busy || !plugin.enabled}
                    onClick={() => onSelectPlugin(plugin.id)}
                    aria-pressed={selectedPluginId === plugin.id}>
                    {selectedPluginId === plugin.id ? 'Đã chọn để chạy' : 'Chọn chạy'}
                  </button>
                </div>
              </article>
            );
          })}
        </div>
      </section>

      <section className="min-w-0 space-y-3 border-t border-slate-200 pt-5" aria-labelledby="plugin-run-heading">
        <h3 id="plugin-run-heading" className="text-lg font-semibold text-slate-900">Thử thực thi plugin</h3>
        {selected
          ? <p className="break-all text-sm text-slate-600">Plugin đã chọn: {selected.name} ({selected.id})</p>
          : <p className="text-sm text-slate-600">Chưa chọn plugin để chạy.</p>}
        {selected && !selected.enabled && (
          <p role="status" className="text-sm text-amber-800">Plugin đang tắt. Có thể bật lại sau khi kiểm tra quyền.</p>
        )}
        <div className="form-group">
          <label htmlFor="plugin-input-json">Payload JSON</label>
          <textarea id="plugin-input-json" rows={4} className="w-full min-w-0 rounded-lg border border-slate-300 p-3 font-mono text-sm"
            value={pluginInputJson} onChange={event => onPluginInputChange(event.target.value)}
            disabled={busy} placeholder='{"text": "Dữ liệu mẫu"}' />
        </div>
        <button type="button" id="btn-run-plugin" className="od-primary-action" onClick={() => void invoke('Đang thực thi plugin...', onRunPlugin)}
          disabled={!canRun}>Chạy Plugin</button>
        {pluginRunOutput && (
          <div>
            <p className="mb-2 text-sm font-semibold text-slate-800">Kết quả</p>
            <pre id="plugin-output" className="max-h-48 max-w-full overflow-auto whitespace-pre-wrap break-all rounded-lg border border-slate-200 bg-slate-50 p-3 text-xs">{pluginRunOutput}</pre>
          </div>
        )}
      </section>
    </div>
  );
};
