import React, { useEffect, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  activateTheme,
  dryRunSampleImport,
  executeSampleImport,
  getThemes,
  installTheme,
  type ImportExecutionOutcome,
  type ImportPlan,
  type SampleGroup,
  type ThemesSummary,
} from '../../api/themes.ts';
import { ThemeOptionsForm } from './ThemeOptionsForm.tsx';

interface Props {
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onInstalled: () => void;
  onActivated: () => void;
}

export const ThemesPanel: React.FC<Props> = ({ csrfToken, onApiError, onInstalled, onActivated }) => {
  const [data, setData] = useState<ThemesSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const [sampleGroups, setSampleGroups] = useState<SampleGroup[]>(['pages', 'menus', 'media', 'settings']);
  const [importPlan, setImportPlan] = useState<ImportPlan | null>(null);
  const [ackPublic, setAckPublic] = useState(false);
  const [importOutcome, setImportOutcome] = useState<ImportExecutionOutcome | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const onErrorRef = useRef(onApiError);
  onErrorRef.current = onApiError;
  const mounted = useRef(true);

  const refresh = async () => {
    setLoading(true);
    setError('');
    try {
      const result = await getThemes();
      if (mounted.current) setData(result);
    } catch (cause) {
      if (mounted.current) {
        setError(cause instanceof Error ? cause.message : 'Không thể tải themes.');
        onErrorRef.current(cause, 'Không thể tải themes.');
      }
    } finally {
      if (mounted.current) setLoading(false);
    }
  };

  useEffect(() => {
    mounted.current = true;
    void refresh();
    return () => { mounted.current = false; };
  }, []);

  const upload = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file || !csrfToken || busy) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      const item = await installTheme(file, csrfToken);
      setNotice(`Đã cài theme ${item.id}. Chọn Kích hoạt để đổi giao diện công khai.`);
      onInstalled();
      await refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Không thể cài theme.');
      onErrorRef.current(cause, 'Không thể cài theme.');
    } finally {
      setBusy(false);
    }
  };

  const activate = async (id: string) => {
    if (!csrfToken || !data || busy) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      const next = await activateTheme(id, data.version, csrfToken);
      setData(next);
      setNotice(`Đã kích hoạt ${id}. Nội dung Builder được giữ nguyên.`);
      onActivated();
    } catch (cause) {
      const stale = cause instanceof ApiClientError && cause.status === 412;
      setError(stale
        ? 'Theme đã đổi ở tab khác (412). Hãy Tải lại rồi chọn lại. Nội dung Builder không bị mất.'
        : cause instanceof Error ? cause.message : 'Không thể kích hoạt theme.');
      onErrorRef.current(cause, 'Không thể kích hoạt theme.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="min-w-0 space-y-5" aria-label="Quản lý Themes" data-testid="themes-panel">
      <div>
        <p className="text-xs font-semibold uppercase tracking-widest text-sky-700">Giao diện / Themes</p>
        <h2 className="mt-2 text-2xl font-bold text-slate-900">Quản lý Themes</h2>
        <p className="mt-2 text-sm text-slate-600">Kích hoạt theme không thay đổi block, các trường hoặc dữ liệu đã lưu trong Builder.</p>
      </div>
      <label className="block rounded-xl border border-dashed border-sky-300 bg-sky-50 p-4">
        <span className="block font-semibold text-slate-900">Cài theme từ ZIP</span>
        <span className="block text-sm text-slate-600">block.json, block.html và homepage-compat.json tùy chọn (tối đa 64 KiB).</span>
        <input data-testid="themes-upload" type="file" accept=".zip,application/zip" onChange={upload} disabled={!csrfToken || busy} className="mt-3 block max-w-full text-sm" />
      </label>
      {notice && <p role="status" className="rounded-lg bg-emerald-50 p-3 text-sm text-emerald-900">{notice}</p>}
      {error && <p role="alert" className="rounded-lg bg-red-50 p-3 text-sm text-red-900">{error}</p>}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h3 className="text-lg font-semibold text-slate-900">Đã cài ({data?.items.length ?? 0})</h3>
        <button type="button" onClick={() => void refresh()} disabled={busy || loading}
          className="rounded-lg border border-slate-300 px-4 py-2 text-sm font-medium">Tải lại danh sách</button>
      </div>
      {loading && <p role="status">Đang tải danh sách themes…</p>}
      {!loading && data && (
        <>
          <p className="text-sm text-slate-700" data-testid="active-theme">
            Theme đang kích hoạt: <strong>{data.active_id ?? 'Chế độ legacy (theo homepage)'}</strong> · Phiên bản {data.version}
          </p>
          {data.homepage_compatible === false && (
            <p role="alert" data-testid="theme-compat-warning" className="rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-950">
              Theme hiện tại không hỗ trợ block homepage. Trang công khai ẩn block, nhưng tất cả dữ liệu Builder vẫn được giữ.
            </p>
          )}
          {data.homepage_compatible === true && (
            <p role="status" data-testid="theme-compat-ok" className="text-sm text-emerald-800">Theme hỗ trợ block homepage đã lưu.</p>
          )}
          {data.items.length === 0 && <p>Chưa có theme nào. Hãy cài ZIP để bắt đầu.</p>}
          <ul className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">
            {data.items.map(item => (
              <li key={item.id} className="min-w-0 rounded-xl border border-slate-200 bg-white p-4" data-theme-id={item.id}>
                <h4 className="break-all font-semibold text-slate-900">{item.id}</h4>
                <p className="mt-1 break-words text-xs text-slate-600">Alias: {item.compatible_block_id ?? 'Không khai báo'}</p>
                {data.active_id === item.id
                  ? <span className="mt-3 inline-block text-sm font-semibold text-emerald-800">Đang kích hoạt</span>
                  : <button type="button" onClick={() => void activate(item.id)} disabled={busy || loading}
                      className="mt-3 rounded-lg bg-sky-700 px-4 py-2 text-sm font-semibold text-white disabled:opacity-50"
                      aria-label={`Kích hoạt theme ${item.id}`}>Kích hoạt</button>}
              </li>
            ))}
          </ul>
          {data.active_id && (
            <div className="rounded-xl border border-slate-200 bg-slate-50 p-4 space-y-3" data-testid="sample-import-box">
              <h3 className="text-base font-semibold text-slate-900">Nhập dữ liệu mẫu có chọn lọc</h3>
              <div className="flex flex-wrap gap-3 text-sm">
                {(['pages', 'menus', 'media', 'settings'] as SampleGroup[]).map(g => (
                  <label key={g} className="inline-flex items-center gap-1.5">
                    <input
                      type="checkbox"
                      checked={sampleGroups.includes(g)}
                      onChange={e => {
                        setImportPlan(null);
                        setImportOutcome(null);
                        setSampleGroups(prev => e.target.checked ? [...prev, g] : prev.filter(x => x !== g));
                      }}
                    />
                    <span>{g}</span>
                  </label>
                ))}
              </div>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  disabled={!csrfToken || busy || sampleGroups.length === 0}
                  onClick={() => {
                    if (!data.active_id || !csrfToken) return;
                    setBusy(true);
                    setError('');
                    setImportOutcome(null);
                    void dryRunSampleImport(data.active_id, sampleGroups, csrfToken)
                      .then(plan => { setImportPlan(plan); setAckPublic(false); })
                      .catch(cause => setError(cause instanceof Error ? cause.message : 'Dry-run thất bại.'))
                      .finally(() => setBusy(false));
                  }}
                  className="rounded-lg border border-slate-300 bg-white px-3 py-1.5 text-sm font-medium"
                >
                  Xem trước (Dry-run)
                </button>
                {importPlan && (
                  <button
                    type="button"
                    disabled={!csrfToken || busy || (importPlan.requires_public_ack && !ackPublic)}
                    onClick={() => {
                      if (!data.active_id || !csrfToken) return;
                      setBusy(true);
                      setError('');
                      void executeSampleImport(data.active_id, importPlan, ackPublic, csrfToken)
                        .then(out => { setImportOutcome(out); setImportPlan(null); })
                        .catch(cause => setError(cause instanceof Error ? cause.message : 'Execute thất bại.'))
                        .finally(() => setBusy(false));
                    }}
                    className="rounded-lg bg-sky-700 px-3 py-1.5 text-sm font-semibold text-white disabled:opacity-50"
                  >
                    Thực thi nhập mẫu
                  </button>
                )}
              </div>
              {importPlan && (
                <div className="space-y-2 text-xs text-slate-700">
                  <p>Plan digest: <code>{importPlan.plan_digest.slice(0, 12)}</code> · {importPlan.items.length} mục</p>
                  {importPlan.unselected_dependencies.length > 0 && (
                    <p className="text-amber-800">Thiếu nhóm phụ thuộc: {importPlan.unselected_dependencies.map(d => `${d.required_group}:${d.object_key}`).join(', ')}</p>
                  )}
                  {importPlan.requires_public_ack && (
                    <label className="flex items-center gap-2 text-amber-900">
                      <input type="checkbox" checked={ackPublic} onChange={e => setAckPublic(e.target.checked)} />
                      <span>Tôi xác nhận thay đổi menu/options có hiệu lực công khai.</span>
                    </label>
                  )}
                </div>
              )}
              {importOutcome && (
                <p role="status" className="text-sm text-emerald-800">
                  Đã nhập: {importOutcome.created.length} · Bỏ qua: {importOutcome.skipped.length}
                </p>
              )}
            </div>
          )}
          {data.active_id && (
            <ThemeOptionsForm themeId={data.active_id} csrfToken={csrfToken} onApiError={onApiError} />
          )}
        </>
      )}
    </section>
  );
};
