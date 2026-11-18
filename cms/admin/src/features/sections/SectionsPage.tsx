import { useEffect, useRef, useState } from 'react';
import type { AuthUser } from '../../types.ts';
import type { ParsedRoute } from '../../shell/router.ts';
import * as sections from '../../api/sections.ts';

interface Props {
  route: ParsedRoute;
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (cause: unknown, message: string) => void;
  onNavigateHash: (hash: string) => void;
  onDirtyChange: (dirty: boolean) => void;
}

export function SectionsPage({ route, user, csrfToken, onApiError, onNavigateHash, onDirtyChange }: Props) {
  const [items, setItems] = useState<sections.SectionRecord[]>([]);
  const [current, setCurrent] = useState<sections.SectionRecord | null>(null);
  const [title, setTitle] = useState('');
  const [slug, setSlug] = useState('');
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState('');
  const [dirty, setDirty] = useState(false);
  const loadSeq = useRef(0);
  const userId = user?.id ?? null;
  const role = user?.role ?? null;
  const isEdit = route.sub === 'edit';
  const isNew = route.sub === 'new';

  const markDirty = (value: boolean) => { setDirty(value); onDirtyChange(value); };
  useEffect(() => {
    const seq = ++loadSeq.current;
    setCurrent(null); setTitle(''); setSlug(''); setItems([]); setNotice('');
    markDirty(false);
    if (!userId || !role || !csrfToken) return;
    if (isNew) return;
    setLoading(true);
    const promise = isEdit && route.entryId
      ? sections.getSection(route.entryId)
      : sections.listSections();
    promise.then(data => {
      if (seq !== loadSeq.current) return;
      if (Array.isArray(data)) setItems(data);
      else { setCurrent(data); setTitle(data.title); setSlug(data.slug); }
    }).catch(error => {
      if (seq !== loadSeq.current) return;
      onApiError(error, 'Không thể tải Section.');
    }).finally(() => { if (seq === loadSeq.current) setLoading(false); });
    return () => { loadSeq.current += 1; };
  }, [route.rawHash, userId, role, csrfToken]);

  const save = async () => {
    if (!csrfToken || !user || saving || !title.trim() || !/^[a-z][a-z0-9-]{0,63}$/.test(slug)) {
      setNotice('Cần tiêu đề và slug hợp lệ (chữ thường, số và dấu -).');
      return;
    }
    const seq = loadSeq.current;
    setSaving(true);
    try {
      const updated = current
        ? await sections.updateSection(current.id, { title: title.trim(), slug }, current.version, csrfToken)
        : await sections.createSection({ title: title.trim(), slug }, csrfToken);
      if (seq !== loadSeq.current) return;
      setCurrent(updated);
      markDirty(false);
      setNotice('Đã lưu Section riêng tư. Không có đường dẫn public.');
      if (!current) onNavigateHash(`#/sections/${updated.id}/edit`);
    } catch (error) {
      if (seq === loadSeq.current) {
        setNotice('Lưu thất bại; bản nhập chưa lưu vẫn còn.');
        onApiError(error, 'Không thể lưu Section.');
      }
    } finally { if (seq === loadSeq.current) setSaving(false); }
  };

  const remove = async () => {
    if (!current || !csrfToken || !user || !window.confirm(`Xóa Section #${current.id}? Chỉ xóa Section và tham chiếu riêng của nó.`)) return;
    const seq = loadSeq.current;
    setSaving(true);
    try {
      await sections.deleteSection(current.id, current.version, csrfToken);
      if (seq === loadSeq.current) { markDirty(false); onNavigateHash('#/sections'); }
    } catch (error) { if (seq === loadSeq.current) onApiError(error, 'Không thể xóa Section.'); }
    finally { if (seq === loadSeq.current) setSaving(false); }
  };

  return <main className="od-admin-card min-w-0 space-y-4" data-testid="section-workspace">
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div><h2 className="text-xl font-semibold">{isNew ? 'Tạo Section' : isEdit ? 'Chỉnh sửa Section riêng tư' : 'Section riêng tư'}</h2>
        <p className="mt-1 text-sm text-slate-600">Mỗi Section có Builder độc lập và không bao giờ được xuất bản trực tiếp.</p></div>
      <button type="button" className="btn-secondary" onClick={() => onNavigateHash('#/sections')}>Tất cả Section</button>
    </header>
    {loading && <p role="status">Đang tải Section…</p>}
    {notice && <p role="status" className="text-sm text-slate-700">{notice}</p>}
    {!isEdit && !isNew && <>
      <button type="button" onClick={() => onNavigateHash('#/sections/new')}>Thêm Section</button>
      <div className="overflow-x-auto"><table className="min-w-full text-left text-sm"><thead><tr><th className="p-2">ID</th><th className="p-2">Tiêu đề</th><th className="p-2">Slug</th><th className="p-2">Thao tác</th></tr></thead><tbody>
        {items.map(item => <tr key={item.id} className="border-t border-slate-200"><td className="p-2">#{item.id}</td><td className="p-2">{item.title}</td><td className="p-2">{item.slug}</td><td className="p-2"><button type="button" onClick={() => onNavigateHash(`#/sections/${item.id}/edit`)}>Chỉnh sửa</button></td></tr>)}
      </tbody></table></div>
      {!items.length && !loading && <p>Chưa có Section. Chọn “Thêm Section” để bắt đầu.</p>}
    </>}
    {(isNew || isEdit) && <form className="grid gap-4 max-w-2xl" onSubmit={event => { event.preventDefault(); void save(); }}>
      <label className="grid gap-1 text-sm font-medium">Tiêu đề Section
        <input aria-label="Tiêu đề Section" className="w-full" maxLength={200} value={title} onChange={e => { setTitle(e.target.value); markDirty(true); }} required />
      </label>
      <label className="grid gap-1 text-sm font-medium">Slug riêng
        <input aria-label="Slug Section" className="w-full" maxLength={64} value={slug} onChange={e => { setSlug(e.target.value); markDirty(true); }} required />
      </label>
      {current && <p className="text-sm text-slate-600">Section #{current.id} · phiên bản "v{current.version}" · không có trạng thái Published</p>}
      <div className="flex flex-wrap gap-2">
        <button type="submit" disabled={saving || !user || !csrfToken || (isEdit && !dirty)}>{saving ? 'Đang lưu…' : 'Lưu Section'}</button>
        {current && <button type="button" className="btn-secondary" onClick={() => onNavigateHash(`#/builder?template=section&type=section&entry=${current.id}`)}>Mở Section Builder</button>}
        {current && <button type="button" className="btn-secondary" disabled={saving} onClick={() => void remove()}>Xóa Section</button>}
      </div>
    </form>}
  </main>;
}
