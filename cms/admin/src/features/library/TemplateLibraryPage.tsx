import { useEffect, useRef, useState } from 'react';
import type { AuthUser } from '../../types.ts';
import { ApiClientError } from '../../api/client.ts';
import { getOverride, type OverrideRole } from '../../api/builder.ts';
import {
  applyLocalThemeTemplate, getLocalTemplateLibrary,
  type LocalTemplateLibrary, type LocalThemeTemplateDescriptor,
} from '../../api/sections.ts';

interface Props {
  user: AuthUser | null;
  csrfToken: string | null;
  themeRevision: number;
  onNavigateHash: (hash: string) => void;
  onApiError: (cause: unknown, fallback: string) => void;
  onApplied: () => void;
}

const GLOBAL_ROLES: OverrideRole[] = ['header', 'footer', 'archive', 'taxonomy_archive'];

export function TemplateLibraryPage({ user, csrfToken, themeRevision, onNavigateHash, onApiError, onApplied }: Props) {
  const [catalog, setCatalog] = useState<LocalTemplateLibrary | null>(null);
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
    getLocalTemplateLibrary().then(data => {
      if (seq === requestSeq.current) setCatalog(data);
    }).catch(error => {
      if (seq === requestSeq.current) onApiError(error, 'Không tải được thư viện mẫu.');
    }).finally(() => { if (seq === requestSeq.current) setLoading(false); });
  };

  useEffect(() => {
    requestSeq.current += 1;
    setCatalog(null);
    setApplying(null);
    if (currentRole !== 'admin' || !csrfToken) return;
    const seq = ++requestSeq.current;
    setLoading(true);
    getLocalTemplateLibrary().then(data => {
      if (seq === requestSeq.current) setCatalog(data);
    }).catch(error => {
      if (seq === requestSeq.current) onApiError(error, 'Không tải được thư viện mẫu.');
    }).finally(() => { if (seq === requestSeq.current) setLoading(false); });
    return () => { requestSeq.current += 1; };
  }, [currentAccount, currentRole, csrfToken, themeRevision]);

  // A supported theme role is already the fallback. Applying it explicitly
  // resets only its own override, using the version from an authenticated GET.
  // The server compares If-Match again; a stale version must never erase the
  // other editor's changes. Cancel does not write anything.
  const applyTheme = async (item: LocalThemeTemplateDescriptor) => {
    const role = item.role;
    if (!user || user.role !== 'admin' || !csrfToken || applying || !GLOBAL_ROLES.includes(role as OverrideRole)) return;
    const seq = requestSeq.current;
    setApplying(role);
    try {
      const opened = await getOverride(role as OverrideRole);
      if (seq !== requestSeq.current) return;
      if (!window.confirm(`Áp dụng mẫu theme cho ${role}? ${opened.exists ? 'Override hiện tại sẽ bị reset, không thể hoàn tác.' : 'Vai trò này đã dùng mẫu theme; không có override cần reset.'} Chỉ ảnh hưởng role này.`)) {
        setNotice('Đã hủy. Thiết kế hiện tại giữ nguyên.');
        return;
      }
      // The server compares the catalog's exact active revision AND override
      // version inside one SQLite writer transaction. A theme switch between
      // opening the dialog and confirming fails closed with 412.
      const result = await applyLocalThemeTemplate(item, opened.version, csrfToken);
      if (seq === requestSeq.current) {
        setNotice(`Mẫu theme cho ${role} ${result.reset ? 'đã được áp dụng.' : 'đang được sử dụng.'}`);
        onApplied();
      }
    } catch (error) {
      if (seq !== requestSeq.current) return;
      if (error instanceof ApiClientError && error.status === 412) {
        setNotice('Xung đột 412: bản trên server đã thay đổi; không reset. Hãy mở lại và so sánh.');
      } else onApiError(error, 'Không thể áp dụng mẫu theme.');
    } finally { if (seq === requestSeq.current) setApplying(null); }
  };

  if (!user || user.role !== 'admin' || !csrfToken) return null;
  return <main className="od-admin-card min-w-0 space-y-5" data-testid="local-template-library">
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div><h2 className="text-xl font-semibold">Thư viện mẫu cục bộ</h2>
        <p className="mt-1 text-sm text-slate-600">Mẫu Section thuộc website và template của theme đang kích hoạt. Không kết nối cloud.</p></div>
      <button type="button" className="btn-secondary" disabled={loading || !!applying} onClick={refresh}>Tải lại thư viện</button>
    </header>
    {loading && <p role="status">Đang đọc thư viện…</p>}
    {notice && <p role="status" className="text-sm text-slate-700">{notice}</p>}
    {catalog && <>
      <section className="space-y-3" aria-label="Section cục bộ">
        <h3 className="font-semibold">Section riêng tư ({catalog.sections.length})</h3>
        <p className="text-sm text-slate-600">Chọn “Chèn bản sao” trong Builder đích. Chèn không lưu, không xuất bản, không liên kết ngược Section gốc.</p>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          {catalog.sections.map(item => <article key={item.id} className="min-w-0 rounded-lg border border-slate-200 p-3">
            <h4 className="break-words font-medium">{item.title}</h4>
            <p className="mt-1 break-words text-xs text-slate-500">#{item.id} · {item.slug} · riêng tư</p>
            <button type="button" className="btn-secondary mt-3" onClick={() => onNavigateHash(`#/sections/${item.id}/edit`)}>Mở Section</button>
          </article>)}
        </div>
        {!catalog.sections.length && <p className="text-sm">Chưa có Section. Tạo một Section trước khi sử dụng thư viện.</p>}
        <button type="button" onClick={() => onNavigateHash('#/sections/new')}>Tạo Section mới</button>
      </section>
      <section className="space-y-3 border-t border-slate-200 pt-4" aria-label="Mẫu theme cục bộ">
        <h3 className="font-semibold">Mẫu theme đang kích hoạt</h3>
        <p className="text-sm text-slate-600">Theme: {catalog.active_theme_id ?? 'Không có'} · {catalog.theme_state}</p>
        {catalog.theme_templates.map(item => <article key={item.role} className="flex min-w-0 flex-wrap items-center justify-between gap-3 rounded-lg border border-slate-200 p-3">
          <div className="min-w-0"><strong>{item.role}</strong>
            <p className="break-all text-xs text-slate-500">Revision: {item.revision_id} · {item.revision_digest.slice(0, 12)}</p>
            {item.reason && <p className="text-xs text-amber-800">{item.reason}</p>}
          </div>
          {item.can_apply && GLOBAL_ROLES.includes(item.role as OverrideRole) &&
            <button type="button" disabled={!!applying} onClick={() => void applyTheme(item)}>
              {applying === item.role ? 'Đang áp dụng…' : 'Áp dụng mẫu theme'}
            </button>}
        </article>)}
        {!catalog.theme_templates.length && <p className="text-sm text-slate-600">Theme không cung cấp mẫu role cục bộ hợp lệ. Section vẫn được bảo toàn; không tải nội dung từ mạng.</p>}
      </section>
    </>}
  </main>;
}
