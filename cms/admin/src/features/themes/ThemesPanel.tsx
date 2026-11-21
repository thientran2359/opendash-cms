import React, { useEffect, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  activateThemeRevision,
  dryRunSampleImport,
  executeSampleImport,
  getThemes,
  installTheme,
  isV3ThemeInstall,
  type ImportExecutionOutcome,
  type ImportPlan,
  type SampleGroup,
  type ThemesSummary,
} from '../../api/themes.ts';
import { ThemeOptionsForm } from './ThemeOptionsForm.tsx';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';

interface Props {
  mode?: 'themes' | 'options';
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onInstalled: () => void;
  onActivated: () => void;
}

export const ThemesPanel: React.FC<Props> = ({ mode = 'themes', csrfToken, onApiError, onInstalled, onActivated }) => {
  const [data, setData] = useState<ThemesSummary | null>(null);
  const [busy, setBusy] = useState(false);
  // The chosen revision is an explicit Admin draft, not the active selection.
  // Keep it untouched on stale 412 and permission failures.
  const [selectedRevisionId, setSelectedRevisionId] = useState<string | null>(null);
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

  const refresh = async (): Promise<boolean> => {
    setLoading(true);
    setError('');
    try {
      const result = await getThemes();
      if (mounted.current) setData(result);
      return mounted.current;
    } catch (cause) {
      if (mounted.current) {
        setError(cause instanceof Error ? cause.message : 'Không thể tải themes.');
        onErrorRef.current(cause, 'Không thể tải themes.');
      }
      return false;
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
      if (isV3ThemeInstall(item)) {
        setSelectedRevisionId(item.revision_id);
        setNotice(`Đã cài ${item.theme_id} v${item.version} (revision ${item.revision_id.slice(0, 16)}…). Chưa kích hoạt.`);
      }
      onInstalled();
      await refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Không thể cài theme.');
      onErrorRef.current(cause, 'Không thể cài theme.');
    } finally {
      setBusy(false);
    }
  };

  const activateRevision = async () => {
    if (!csrfToken || !data || busy || !selectedRevisionId) return;
    const revisionId = selectedRevisionId;
    if (!data.revisions.some(rev => rev.source_format === 'v3' && rev.revision_id === revisionId)) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      // Revision activation returns ThemeSelection, not the legacy ThemeList.
      // Always read the complete projection after the mutation.
      await activateThemeRevision(revisionId, data.selection.version, csrfToken);
      const reloaded = await refresh();
      if (reloaded) setSelectedRevisionId(null);
      setImportPlan(null);
      setImportOutcome(null);
      setAckPublic(false);
      onActivated();
      if (reloaded) setNotice('Đã kích hoạt revision đã chọn. Nội dung, menu và tùy chọn đã lưu được giữ nguyên.');
      else setError('Đã gửi kích hoạt nhưng chưa tải lại được trạng thái server. Revision đã chọn vẫn được giữ.');
    } catch (cause) {
      const stale = cause instanceof ApiClientError && cause.status === 412;
      setError(stale
        ? 'Theme đã đổi ở tab khác (412). Revision đã chọn vẫn được giữ. Hãy Tải lại để xem phiên bản mới rồi thử tiếp.'
        : cause instanceof Error ? cause.message : 'Không thể kích hoạt revision.');
      onErrorRef.current(cause, 'Không thể kích hoạt revision.');
    } finally {
      setBusy(false);
    }
  };

  const v3Revisions = data?.revisions.filter(revision => revision.source_format === 'v3') ?? [];
  const activeThemeKey = data?.selection.source_format === 'v3'
    ? data.selection.theme_id
    : data?.active_id;

  if (mode === 'options') {
    return (
      <section className="min-w-0 space-y-5" aria-label="Tùy chỉnh giao diện" data-testid="theme-options-screen">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-od-accent">Giao diện / Tùy chỉnh giao diện</p>
          <h2 className="mt-1.5 text-xl font-bold tracking-tight text-zinc-950">Tùy chọn giao diện</h2>
          <p className="mt-1 text-sm text-zinc-600">Cấu hình theme đang hoạt động. Cài và kích hoạt theme trong màn hình Giao diện riêng.</p>
        </div>
        {loading && <p role="status" className="text-sm text-zinc-600">Đang tải tùy chọn theme…</p>}
        {error && <AdminAlert tone="error">{error}</AdminAlert>}
        {!loading && data && !activeThemeKey && (
          <AdminAlert tone="warning">
            Chưa có theme được kích hoạt để tùy chỉnh. Hãy mở Giao diện để cài và kích hoạt theme trước.
          </AdminAlert>
        )}
        {!loading && data && activeThemeKey && (
          <ThemeOptionsForm key={`${activeThemeKey}@${data.selection.revision_id ?? data.selection.version}`}
            themeId={activeThemeKey} csrfToken={csrfToken} onApiError={onApiError} />
        )}
      </section>
    );
  }

  return (
    <section className="min-w-0 space-y-5" aria-label="Quản lý Themes" data-testid="themes-panel">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-od-accent">Giao diện / Themes</p>
          <h2 className="mt-1.5 text-xl font-bold tracking-tight text-zinc-950">Quản lý Themes</h2>
          <p className="mt-1 text-sm text-zinc-600">Kích hoạt theme không thay đổi block, các trường hoặc dữ liệu đã lưu trong Builder.</p>
        </div>
        <AdminButton variant="secondary" size="sm" onClick={() => void refresh()} disabled={busy || loading}>
          Tải lại danh sách
        </AdminButton>
      </div>
      <label className="block rounded-xl border border-dashed border-zinc-300 bg-zinc-50/70 p-4 transition-colors hover:border-od-accent/60 hover:bg-od-accent-soft/25">
        <span className="block text-sm font-semibold text-zinc-900">Cài theme từ ZIP</span>
        <span className="mt-0.5 block text-xs text-zinc-600">Theme v3 (manifest_version=3, theme.json, templates, parts và assets được khai báo).</span>
        <input data-testid="themes-upload" type="file" accept=".zip,application/zip" onChange={upload} disabled={!csrfToken || busy} className="mt-3 block max-w-full text-sm text-zinc-700 file:mr-3 file:rounded-md file:border file:border-zinc-300 file:bg-white file:px-3 file:py-1.5 file:text-xs file:font-medium file:text-zinc-800 hover:file:bg-zinc-50" />
      </label>
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-zinc-200 pb-2.5">
        <h3 className="text-base font-semibold text-zinc-900">Đã cài ({v3Revisions.length} revision v3)</h3>
        {!loading && data && (
          <p className="text-xs text-zinc-600" data-testid="active-theme">
            Theme đang kích hoạt: <strong className="font-semibold text-zinc-900">{data.selection.source_format === 'v3' ? `${data.selection.theme_id} (v3)` : 'Chưa có theme'}</strong> · Phiên bản chọn {data.selection.version}
          </p>
        )}
      </div>
      {loading && <p role="status" className="text-sm text-zinc-600">Đang tải danh sách themes…</p>}
      {!loading && data && (
        <>
          {data.homepage_compatible === false && (
            <AdminAlert tone="warning" role="alert" data-testid="theme-compat-warning">
              Theme hiện tại không hỗ trợ block homepage. Trang công khai ẩn block, nhưng tất cả dữ liệu Builder vẫn được giữ.
            </AdminAlert>
          )}
          {data.homepage_compatible === true && (
            <AdminAlert tone="success" data-testid="theme-compat-ok">Theme hỗ trợ block homepage đã lưu.</AdminAlert>
          )}
          {v3Revisions.length === 0 && <p className="text-sm text-zinc-500">Chưa có theme nào. Hãy cài ZIP để bắt đầu.</p>}
          {v3Revisions.length > 0 && (
            <section className="space-y-3" aria-label="Revision theme v3" data-testid="theme-v3-revisions">
              <h4 className="text-sm font-semibold text-zinc-700">Các revision v3 đã cài ({v3Revisions.length})</h4>
              <ul className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">
                {v3Revisions.map(revision => {
                  const isActive = data.selection.revision_id === revision.revision_id;
                  const isSelected = selectedRevisionId === revision.revision_id;
                  return (
                    <li key={revision.revision_id} data-theme-revision-id={revision.revision_id}
                      className={`min-w-0 space-y-2 rounded-xl border p-4 transition-colors ${
                        isActive
                          ? 'border-emerald-200 bg-emerald-50/30'
                          : isSelected
                            ? 'border-od-accent bg-od-accent-soft/20'
                            : 'border-zinc-200 bg-white'
                      }`}>
                      <div className="flex items-start justify-between gap-2">
                        <div className="break-all font-semibold text-zinc-950">{revision.theme_id}</div>
                        {isActive && <AdminBadge tone="success">Đang kích hoạt</AdminBadge>}
                      </div>
                      <p className="break-all font-mono text-xs text-zinc-500">Version {revision.version} · SHA {revision.revision_digest.slice(0, 12)}…</p>
                      {!isActive && (
                        <label className="inline-flex cursor-pointer items-center gap-2 text-sm text-zinc-700">
                          <input type="radio" name="selected-theme-revision" value={revision.revision_id}
                            checked={isSelected}
                            onChange={() => setSelectedRevisionId(revision.revision_id)} disabled={busy || loading} />
                          Chọn revision để kích hoạt
                        </label>
                      )}
                    </li>
                  );
                })}
              </ul>
              <div className="flex flex-wrap items-center gap-3">
                <AdminButton variant="primary" size="md" onClick={() => void activateRevision()}
                  disabled={!csrfToken || busy || loading || !selectedRevisionId || !v3Revisions.some(rev => rev.revision_id === selectedRevisionId && data.selection.revision_id !== rev.revision_id)}>
                  Kích hoạt revision đã chọn
                </AdminButton>
                <p className="text-xs text-zinc-500">Cài ZIP không tự kích hoạt. Revision đang hoạt động được lấy từ phiên bản chọn trên server.</p>
              </div>
            </section>
          )}
          {data.selection.source_format === 'v3' && (
            <nav aria-label="Chỉnh sửa template theme" className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-zinc-200 bg-white p-4 text-sm">
              <p className="text-zinc-700">Để chỉnh header, footer, single hoặc archive, mở Visual Builder và chọn đúng Role trong bộ chọn.</p>
              <a href="#/builder" className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 hover:border-zinc-400 hover:bg-zinc-50">
                Mở Visual Builder
              </a>
            </nav>
          )}
          {activeThemeKey && (
            <div className="space-y-3 rounded-xl border border-zinc-200 bg-zinc-50/70 p-4" data-testid="sample-import-box">
              <h3 className="text-sm font-semibold text-zinc-900">Nhập dữ liệu mẫu có chọn lọc</h3>
              <div className="flex flex-wrap gap-3 text-sm text-zinc-700">
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
                <AdminButton
                  variant="secondary"
                  size="sm"
                  disabled={!csrfToken || busy || sampleGroups.length === 0}
                  onClick={() => {
                    if (!activeThemeKey || !csrfToken) return;
                    setBusy(true);
                    setError('');
                    setImportOutcome(null);
                    void dryRunSampleImport(activeThemeKey, sampleGroups, csrfToken)
                      .then(plan => { setImportPlan(plan); setAckPublic(false); })
                      .catch(cause => setError(cause instanceof Error ? cause.message : 'Dry-run thất bại.'))
                      .finally(() => setBusy(false));
                  }}
                >
                  Xem trước (Dry-run)
                </AdminButton>
                {importPlan && (
                  <AdminButton variant="ghost" size="sm" disabled={busy} onClick={() => { setImportPlan(null); setAckPublic(false); }}>
                    Huỷ xem trước
                  </AdminButton>
                )}
                {importPlan && (
                  <AdminButton
                    variant="primary"
                    size="sm"
                    disabled={!csrfToken || busy || (importPlan.requires_public_ack && !ackPublic)}
                    onClick={() => {
                      if (!activeThemeKey || !csrfToken) return;
                      setBusy(true);
                      setError('');
                      void executeSampleImport(activeThemeKey, importPlan, ackPublic, csrfToken)
                        .then(out => { setImportOutcome(out); setImportPlan(null); })
                        .catch(cause => setError(cause instanceof Error ? cause.message : 'Execute thất bại.'))
                        .finally(() => setBusy(false));
                    }}
                  >
                    Thực thi nhập mẫu
                  </AdminButton>
                )}
              </div>
              {importPlan && (
                <div className="space-y-2 rounded-lg border border-zinc-200 bg-white p-3 text-xs text-zinc-700">
                  <p>Plan digest: <code className="rounded bg-zinc-100 px-1 py-0.5 font-mono">{importPlan.plan_digest.slice(0, 12)}</code> · {importPlan.items.length} mục</p>
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
                <AdminAlert tone="success">
                  Đã nhập: {importOutcome.created.length} · Bỏ qua: {importOutcome.skipped.length}
                </AdminAlert>
              )}
            </div>
          )}
        </>
      )}
    </section>
  );
};
