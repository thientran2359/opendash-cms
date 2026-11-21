import { useEffect, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import * as patterns from '../../api/patterns.ts';
import type { AuthUser } from '../../types.ts';
import type { ParsedRoute } from '../../shell/router.ts';

interface Props {
  route: ParsedRoute;
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (cause: unknown, message: string) => void;
  onNavigateHash: (hash: string) => void;
  onDirtyChange: (dirty: boolean) => void;
}

function affectedEntries(error: unknown): patterns.PatternDeleteConflict | null {
  if (!(error instanceof ApiClientError) || error.status !== 409) return null;
  try {
    const body: unknown = JSON.parse(error.message);
    if (!body || typeof body !== 'object') return null;
    const info = body as Partial<patterns.PatternDeleteConflict>;
    return Number.isSafeInteger(info.referencing_count) && Array.isArray(info.referencing_entries)
      ? info as patterns.PatternDeleteConflict : null;
  } catch { return null; }
}

export function PatternsPage({ route, user, csrfToken, onApiError, onNavigateHash, onDirtyChange }: Props) {
  const [items, setItems] = useState<patterns.PatternRecord[]>([]);
  const [current, setCurrent] = useState<patterns.PatternRecord | null>(null);
  const [title, setTitle] = useState('');
  const [slug, setSlug] = useState('');
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState('');
  const [dirty, setDirty] = useState(false);
  const dirtyRef = useRef(false);
  const ownerRef = useRef<{ id: number; role: AuthUser['role'] } | null>(null);
  const openedRouteRef = useRef(route.rawHash);
  const seqRef = useRef(0);
  const isEdit = route.sub === 'edit';
  const isNew = route.sub === 'new';
  const canWrite = user?.role === 'admin' && Boolean(csrfToken);
  const markDirty = (next: boolean) => { dirtyRef.current = next; setDirty(next); onDirtyChange(next); };

  useEffect(() => {
    const seq = ++seqRef.current;
    // Quarantine the draft during 401. A same-account re-login can continue
    // editing with the original If-Match; a different account clears it.
    if (!user || !csrfToken) { setLoading(false); setSaving(false); return; }
    const sameOwner = ownerRef.current?.id === user.id && ownerRef.current?.role === user.role;
    const sameRoute = openedRouteRef.current === route.rawHash;
    ownerRef.current = { id: user.id, role: user.role };
    openedRouteRef.current = route.rawHash;
    if (sameOwner && sameRoute && dirtyRef.current) {
      setLoading(false);
      setSaving(false);
      onDirtyChange(true);
      setNotice('Đã giữ bản nhập chưa lưu sau khi xác thực lại. Phiên bản cũ được dùng để kiểm tra xung đột.');
      return;
    }
    setCurrent(null); setItems([]); setTitle(''); setSlug(''); setNotice('');
    markDirty(false);
    if (isNew) return;
    setLoading(true);
    const request = isEdit && route.entryId ? patterns.getPattern(route.entryId) : patterns.listPatterns();
    request.then(result => {
      if (seq !== seqRef.current) return;
      if ('items' in result) setItems(result.items);
      else { setCurrent(result); setTitle(result.title); setSlug(result.slug); }
    }).catch(cause => {
      if (seq === seqRef.current) onApiError(cause, 'Không mở được Patterns.');
    }).finally(() => { if (seq === seqRef.current) setLoading(false); });
    return () => { seqRef.current += 1; };
  }, [route.rawHash, user?.id, user?.role, csrfToken]);

  const save = async () => {
    if (!canWrite || !csrfToken || saving || !title.trim() || !/^[a-z][a-z0-9-]{0,63}$/.test(slug)) {
      setNotice('Cần tiêu đề và slug hợp lệ (chữ thường, số và dấu -).');
      return;
    }
    const seq = seqRef.current;
    setSaving(true);
    try {
      const saved = current
        ? await patterns.updatePattern(current.id, { title: title.trim(), slug }, current.version, csrfToken)
        : await patterns.createPattern({ title: title.trim(), slug }, csrfToken);
      if (seq !== seqRef.current) return;
      setCurrent(saved);
      markDirty(false);
      setNotice('Pattern đã lưu. Bố cục của Pattern sẽ đồng bộ tới các trang đang tham chiếu.');
      if (!current) onNavigateHash(`#/patterns/${saved.id}/edit`);
    } catch (cause) {
      if (seq !== seqRef.current) return;
      setNotice(cause instanceof ApiClientError && cause.status === 412
        ? 'Pattern đã thay đổi ở phiên khác (412). Bản nhập của bạn vẫn còn; hãy đối chiếu trước khi lưu lại.'
        : 'Không lưu được Pattern. Bản nhập chưa lưu vẫn còn.');
      onApiError(cause, 'Không thể lưu Pattern.');
    } finally { if (seq === seqRef.current) setSaving(false); }
  };

  const remove = async () => {
    if (!current || !csrfToken || !canWrite || saving || dirty) return;
    const target = current;
    if (!window.confirm(`Xóa Pattern “${target.title}” (#${target.id})? Đây là thao tác không thể hoàn tác.`)) return;
    const seq = seqRef.current;
    setSaving(true);
    try {
      try {
        await patterns.deletePattern(target.id, target.version, csrfToken);
      } catch (cause) {
        const conflict = affectedEntries(cause);
        if (!conflict) throw cause;
        const rows = conflict.referencing_entries.map(item =>
          `#${item.id} · ${item.type_key} · ${item.title}`).join('\n');
        const warning = `Pattern đang được ${conflict.referencing_count} nội dung tham chiếu.\n${rows}\n\nXóa cưỡng bức? Các trang này sẽ bỏ qua block Pattern đã mất.`;
        if (!window.confirm(warning)) {
          if (seq === seqRef.current) setNotice('Giữ nguyên Pattern đang được sử dụng.');
          return;
        }
        await patterns.deletePattern(target.id, target.version, csrfToken, true);
      }
      if (seq === seqRef.current) { markDirty(false); onNavigateHash('#/patterns'); }
    } catch (cause) {
      if (seq !== seqRef.current) return;
      setNotice(cause instanceof ApiClientError && cause.status === 412
        ? 'Pattern đã thay đổi (412). Hãy tải lại trước khi xóa.'
        : 'Không thể xóa Pattern; dữ liệu vẫn còn.');
      onApiError(cause, 'Không thể xóa Pattern.');
    } finally { if (seq === seqRef.current) setSaving(false); }
  };

  return <main className="od-admin-card min-w-0 space-y-4" data-testid="pattern-workspace">
    <header className="flex flex-wrap items-center justify-between gap-3">
      <div><h2 className="text-xl font-semibold">{isNew ? 'Tạo Pattern' : isEdit ? 'Chỉnh sửa Pattern' : 'Patterns'}</h2>
        <p className="mt-1 text-sm text-slate-600">Pattern riêng tư có bố cục Builder dùng chung. Sửa một lần sẽ cập nhật nơi đang tham chiếu.</p></div>
      <button type="button" className="btn-secondary" onClick={() => onNavigateHash('#/patterns')}>Tất cả Pattern</button>
    </header>
    {loading && <p role="status">Đang tải Pattern…</p>}
    {notice && <p role="status" className="text-sm text-slate-700">{notice}</p>}
    {!isEdit && !isNew && <>
      {canWrite && <button type="button" onClick={() => onNavigateHash('#/patterns/new')}>Thêm Pattern</button>}
      <div className="overflow-x-auto"><table className="min-w-full text-left text-sm">
        <thead><tr><th className="p-2">ID</th><th className="p-2">Tiêu đề</th><th className="p-2">Slug</th><th className="p-2">Đang được dùng</th><th className="p-2">Thao tác</th></tr></thead>
        <tbody>{items.map(item => <tr key={item.id} className="border-t border-slate-200">
          <td className="p-2">#{item.id}</td><td className="p-2">{item.title}</td><td className="p-2">{item.slug}</td>
          <td className="p-2">{item.referencing_count} nội dung</td>
          <td className="p-2"><button type="button" onClick={() => onNavigateHash(`#/patterns/${item.id}/edit`)}>Xem Pattern</button></td>
        </tr>)}</tbody>
      </table></div>
      {!items.length && !loading && <p>Chưa có Pattern. Hãy tạo Pattern để tái sử dụng qua Builder.</p>}
    </>}
    {(isNew || isEdit) && <form className="grid max-w-2xl gap-4" onSubmit={event => { event.preventDefault(); void save(); }}>
      <label className="grid gap-1 text-sm font-medium">Tiêu đề Pattern
        <input aria-label="Tiêu đề Pattern" className="w-full" maxLength={200} value={title}
          onChange={event => { setTitle(event.target.value); markDirty(true); }} required disabled={!canWrite} />
      </label>
      <label className="grid gap-1 text-sm font-medium">Slug riêng
        <input aria-label="Slug Pattern" className="w-full" maxLength={64} value={slug}
          onChange={event => { setSlug(event.target.value); markDirty(true); }} required disabled={!canWrite} />
      </label>
      {current && <p className="text-sm text-slate-600">Pattern #{current.id} · phiên bản v{current.version} · {current.referencing_count} nội dung đang dùng · không có URL public</p>}
      <div className="flex flex-wrap gap-2">
        {canWrite && <button type="submit" disabled={saving || (isEdit && !dirty)}>{saving ? 'Đang lưu…' : 'Lưu Pattern'}</button>}
        {current && canWrite && <button type="button" className="btn-secondary"
          onClick={() => onNavigateHash(`#/builder?template=entry&type=pattern&entry=${current.id}`)}>Mở Pattern Builder</button>}
        {current && canWrite && <button type="button" className="btn-secondary" disabled={saving || dirty}
          onClick={() => void remove()}>Xóa Pattern</button>}
      </div>
      {dirty && current && <p className="text-xs text-amber-800">Hãy lưu hoặc bỏ chỉnh sửa metadata trước khi xóa Pattern.</p>}
    </form>}
  </main>;
}
