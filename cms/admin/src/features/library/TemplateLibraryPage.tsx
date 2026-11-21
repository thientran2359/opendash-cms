import { useEffect, useRef, useState } from 'react';
import type { AuthUser } from '../../types.ts';
import { ApiClientError } from '../../api/client.ts';
import {
  getTemplateOverrides, resetTemplateOverride, type TypedTemplateOverride,
} from '../../api/builder.ts';
import { getLocalTemplateLibrary, type LocalTemplateLibrary } from '../../api/sections.ts';
import { listPatterns, type PatternRecord } from '../../api/patterns.ts';

interface Props {
  user: AuthUser | null;
  csrfToken: string | null;
  themeRevision: number;
  onNavigateHash: (hash: string) => void;
  onApiError: (cause: unknown, fallback: string) => void;
  onApplied: () => void;
}

export function TemplateLibraryPage({ user, csrfToken, themeRevision, onNavigateHash, onApiError, onApplied }: Props) {
  const [catalog, setCatalog] = useState<LocalTemplateLibrary | null>(null);
  const [patterns, setPatterns] = useState<PatternRecord[]>([]);
  const [overrides, setOverrides] = useState<TypedTemplateOverride[]>([]);
  const [loading, setLoading] = useState(false);
  const [applying, setApplying] = useState<string | null>(null);
  const [notice, setNotice] = useState('');
  const requestSeq = useRef(0);
  const currentAccount = user?.id ?? null;
  const currentRole = user?.role ?? null;

  const refresh = () => {
    if (currentRole !== 'admin' || !csrfToken) return;
    const seq = ++requestSeq.current;
    setLoading(true);
    Promise.all([getLocalTemplateLibrary(), getTemplateOverrides(), listPatterns()]).then(([data, rows, patternList]) => {
      if (seq === requestSeq.current) {
        setCatalog(data);
        setOverrides(rows.items);
        setPatterns(patternList.items);
      }
    }).catch(error => {
      if (seq === requestSeq.current) onApiError(error, 'Không tải được thư viện mẫu.');
    }).finally(() => { if (seq === requestSeq.current) setLoading(false); });
  };

  useEffect(() => {
    requestSeq.current += 1;
    setCatalog(null);
    setPatterns([]);
    setOverrides([]);
    setApplying(null);
    if (currentRole !== 'admin' || !csrfToken) return;
    const seq = ++requestSeq.current;
    setLoading(true);
    Promise.all([getLocalTemplateLibrary(), getTemplateOverrides(), listPatterns()]).then(([data, rows, patternList]) => {
      if (seq === requestSeq.current) {
        setCatalog(data);
        setOverrides(rows.items);
        setPatterns(patternList.items);
      }
    }).catch(error => {
      if (seq === requestSeq.current) onApiError(error, 'Không tải được thư viện mẫu.');
    }).finally(() => { if (seq === requestSeq.current) setLoading(false); });
    return () => { requestSeq.current += 1; };
  }, [currentAccount, currentRole, csrfToken, themeRevision]);

  // Only reset the confirmed site-owned row. Never translate a theme role into
  // a global write: the typed row ID and If-Match make the operation specific
  // and protect another editor's concurrent modifications.
  const resetSiteOverride = async (item: TypedTemplateOverride) => {
    if (!user || user.role !== 'admin' || !csrfToken || applying) return;
    const seq = requestSeq.current;
    const key = item.canonical_target_key;
    if (!window.confirm(`Reset site template “${key}” về nguồn theme/core tiếp theo? Bản AST hiện tại sẽ bị xóa và không thể hoàn tác.`)) return;
    setApplying(key);
    try {
      const result = await resetTemplateOverride(item.id, item.version, csrfToken);
      if (seq !== requestSeq.current) return;
      setNotice(`Đã reset ${result.canonical_target_key}. Nguồn hiệu lực: ${result.next_effective_source.label}.`);
      setOverrides(rows => rows.filter(row => row.id !== item.id));
      onApplied();
    } catch (error) {
      if (seq !== requestSeq.current) return;
      if (error instanceof ApiClientError && error.status === 412) {
        setNotice('Xung đột 412: bản trên server đã thay đổi; không reset. Hãy mở lại và so sánh.');
      } else onApiError(error, 'Không thể reset site template override.');
    } finally { if (seq === requestSeq.current) setApplying(null); }
  };

  if (!user || user.role !== 'admin' || !csrfToken) return null;
  return <main className="od-admin-card min-w-0 space-y-5" data-testid="local-template-library">
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div><h2 className="text-xl font-semibold">Thư viện mẫu cục bộ</h2>
        <p className="mt-1 text-sm text-slate-600">Pattern dùng chung thuộc website và template role của theme đang kích hoạt.</p></div>
      <button type="button" className="btn-secondary" disabled={loading || !!applying} onClick={refresh}>Tải lại thư viện</button>
    </header>
    {loading && <p role="status">Đang đọc thư viện…</p>}
    {notice && <p role="status" className="text-sm text-slate-700">{notice}</p>}
    {catalog && <>
      <section className="space-y-3" aria-label="Pattern cục bộ">
        <h3 className="font-semibold">Pattern đồng bộ ({patterns.length})</h3>
        <p className="text-sm text-slate-600">Thêm block Reusable Pattern từ Visual Builder rồi chọn Pattern cần dùng. Thay đổi bản gốc sẽ cập nhật mọi nơi đang tham chiếu.</p>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          {patterns.map(item => <article key={item.id} className="min-w-0 rounded-lg border border-slate-200 p-3">
            <h4 className="break-words font-medium">{item.title}</h4>
            <p className="mt-1 break-words text-xs text-slate-500">#{item.id} · {item.slug} · {item.referencing_count} nội dung tham chiếu</p>
            <button type="button" className="btn-secondary mt-3" onClick={() => onNavigateHash(`#/patterns/${item.id}/edit`)}>Mở Pattern</button>
          </article>)}
        </div>
        {!patterns.length && <p className="text-sm">Chưa có Pattern đồng bộ.</p>}
        <button type="button" onClick={() => onNavigateHash('#/patterns/new')}>Tạo Pattern mới</button>
      </section>
      <section className="space-y-3 border-t border-slate-200 pt-4" aria-label="Mẫu theme cục bộ">
        <h3 className="font-semibold">Mẫu theme đang kích hoạt</h3>
        <p className="text-sm text-slate-600">Theme: {catalog.active_theme_id ?? 'Không có'} · {catalog.theme_state}</p>
        {catalog.theme_templates.map(item => <article key={item.role} className="flex min-w-0 flex-wrap items-center justify-between gap-3 rounded-lg border border-slate-200 p-3">
          <div className="min-w-0"><strong>{item.role}</strong>
            <p className="break-all text-xs text-slate-500">Revision: {item.revision_id} · {item.revision_digest.slice(0, 12)}</p>
            {item.reason && <p className="text-xs text-amber-800">{item.reason}</p>}
          </div>
          {item.can_apply && <span className="text-xs text-slate-600">Mẫu thuộc active theme</span>}
        </article>)}
        {!catalog.theme_templates.length && <p className="text-sm text-slate-600">Theme không cung cấp mẫu role cục bộ hợp lệ. Pattern CPT được lưu riêng và vẫn được bảo toàn.</p>}
      </section>
      <section className="space-y-3 border-t border-slate-200 pt-4" aria-label="Site template overrides">
        <h3 className="font-semibold">Site template overrides ({overrides.length})</h3>
        <p className="text-sm text-slate-600">Bản AST thuộc website ưu tiên hơn template của theme ở cùng target. Reset đúng target sẽ hiện nguồn theme/core kế tiếp; không thay đổi các template khác.</p>
        {overrides.map(item => <article key={item.id} className="flex min-w-0 flex-wrap items-center justify-between gap-3 rounded-lg border border-slate-200 p-3">
          <div className="min-w-0">
            <strong className="break-all">{item.canonical_target_key}</strong>
            <p className="text-xs text-slate-600">Phiên bản v{item.version} · {item.effective_source?.label ?? 'Site override'}</p>
            <p className="break-all text-xs text-slate-500">Theme gốc: {item.base_theme_id ?? 'Core'} · {item.base_revision_digest?.slice(0, 12) ?? 'Không có revision'}</p>
          </div>
          <button type="button" className="btn-secondary" disabled={!!applying || loading} onClick={() => void resetSiteOverride(item)}>
            {applying === item.canonical_target_key ? 'Đang reset…' : 'Reset override'}
          </button>
        </article>)}
        {!overrides.length && <p className="text-sm text-slate-600">Chưa có site-owned override; resolver sử dụng active theme/core theo thứ tự candidate.</p>}
      </section>
    </>}
  </main>;
}
