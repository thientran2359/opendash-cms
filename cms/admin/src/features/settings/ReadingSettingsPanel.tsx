import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { queryContentEntryTable } from '../../api/content.ts';
import {
  getReadingSettings,
  saveReadingSettings,
  type ReadingPageStatus,
  type ReadingSettings,
  type ReadingSettingsDraft,
} from '../../api/settings.ts';
import type { AuthUser } from '../../types.ts';

interface Props {
  user: AuthUser | null;
  siteInstanceId: string;
  csrfToken: string | null;
  onApiError?: (error: unknown) => void;
  onDirtyChange?: (dirty: boolean) => void;
}

type PageChoice = { id: number; title: string };
type PendingDraft = { baseVersion: number; values: ReadingSettingsDraft };

// Session-memory only. Never persist a draft belonging to another site/account.
const pendingDrafts = new Map<string, PendingDraft>();

export function discardReadingDraft(siteId: string, userId: number): void {
  pendingDrafts.delete(`${siteId}:${userId}`);
}

export function clearReadingDrafts(): void {
  pendingDrafts.clear();
}

function snapshot(settings: ReadingSettings): ReadingSettingsDraft {
  return { homepage_page_id: settings.homepage_page_id, posts_page_id: settings.posts_page_id };
}

function sameDraft(a: ReadingSettingsDraft, b: ReadingSettingsDraft): boolean {
  return a.homepage_page_id === b.homepage_page_id && a.posts_page_id === b.posts_page_id;
}

function statusLabel(status: ReadingPageStatus): string {
  switch (status) {
    case 'published': return 'Đã xuất bản';
    case 'draft': return 'Page hiện là bản nháp (đường dẫn công khai trả 404)';
    case 'deleted': return 'Page đã xóa (đường dẫn công khai trả 404)';
    case 'unconfigured': return 'Chưa chọn Page';
  }
}

function pageId(value: string): number | null {
  if (value === '') return null;
  const id = Number(value);
  return Number.isSafeInteger(id) && id > 0 ? id : null;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Không kết nối được với máy chủ.';
}

export function ReadingSettingsPanel({ user, siteInstanceId, csrfToken, onApiError, onDirtyChange }: Props) {
  const actorId = user?.id ?? null;
  const draftKey = actorId === null ? null : `${siteInstanceId}:${actorId}`;
  const [saved, setSaved] = useState<ReadingSettings | null>(null);
  const [draft, setDraft] = useState<ReadingSettingsDraft | null>(null);
  const [baseVersion, setBaseVersion] = useState<number | null>(null);
  const [choices, setChoices] = useState<PageChoice[]>([]);
  const [pageOffset, setPageOffset] = useState(0);
  const [pageTotal, setPageTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [stale, setStale] = useState(false);
  const onErrorRef = useRef(onApiError);
  onErrorRef.current = onApiError;
  const onDirtyRef = useRef(onDirtyChange);
  onDirtyRef.current = onDirtyChange;
  const loadSequence = useRef(0);

  const dirty = Boolean(saved && draft && !sameDraft(draft, snapshot(saved)));

  useEffect(() => {
    onDirtyRef.current?.(dirty);
    if (draftKey && saved && draft && baseVersion !== null) {
      if (dirty) pendingDrafts.set(draftKey, { baseVersion, values: { ...draft } });
      else pendingDrafts.delete(draftKey);
    }
  }, [draftKey, saved, draft, baseVersion, dirty]);

  useEffect(() => () => { onDirtyRef.current?.(false); }, []);

  const load = useCallback(async (signal?: AbortSignal) => {
    if (!draftKey) return;
    const serial = ++loadSequence.current;
    setLoading(true);
    setError(null);
    setNotice(null);
    try {
      const [settings, firstPage] = await Promise.all([
        getReadingSettings(signal),
        queryContentEntryTable('page', { status: 'published', sort: 'title', order: 'asc', limit: 100, offset: 0 }, signal),
      ]);
      if (signal?.aborted || serial !== loadSequence.current) return;
      const pending = pendingDrafts.get(draftKey);
      setSaved(settings);
      setDraft(pending ? { ...pending.values } : snapshot(settings));
      setBaseVersion(pending?.baseVersion ?? settings.version);
      setStale(Boolean(pending && pending.baseVersion !== settings.version));
      setChoices(firstPage.items.map(({ id, title }) => ({ id, title })));
      setPageOffset(firstPage.items.length);
      setPageTotal(firstPage.total);
    } catch (reason) {
      if (!signal?.aborted && serial === loadSequence.current) {
        setError(errorMessage(reason));
        onErrorRef.current?.(reason);
      }
    } finally {
      if (!signal?.aborted && serial === loadSequence.current) setLoading(false);
    }
  }, [draftKey]);

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => { controller.abort(); loadSequence.current += 1; };
  }, [load]);

  async function loadMorePages() {
    if (loadingMore || pageOffset >= pageTotal) return;
    setLoadingMore(true);
    try {
      const page = await queryContentEntryTable('page', {
        status: 'published', sort: 'title', order: 'asc', limit: 100, offset: pageOffset,
      });
      setChoices(previous => {
        const byId = new Map(previous.map(item => [item.id, item]));
        for (const item of page.items) byId.set(item.id, { id: item.id, title: item.title });
        return Array.from(byId.values());
      });
      setPageOffset(pageOffset + page.items.length);
      setPageTotal(page.total);
    } catch (reason) {
      setError(errorMessage(reason));
      onErrorRef.current?.(reason);
    } finally {
      setLoadingMore(false);
    }
  }

  function updateDraft(next: ReadingSettingsDraft) {
    setDraft(next);
    setError(null);
    setNotice(null);
    // This is recorded synchronously so a 401 on the next request cannot erase it.
    if (draftKey && baseVersion !== null) {
      if (saved && sameDraft(next, snapshot(saved))) pendingDrafts.delete(draftKey);
      else pendingDrafts.set(draftKey, { baseVersion, values: { ...next } });
    }
  }

  async function handleSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!draft || !saved || !csrfToken || baseVersion === null || saving || stale) return;
    if (draft.homepage_page_id !== null && draft.homepage_page_id === draft.posts_page_id) {
      setError('Trang chủ và Trang bài viết phải là hai Page khác nhau.');
      return;
    }
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const settings = await saveReadingSettings(draft, baseVersion, csrfToken);
      pendingDrafts.delete(draftKey ?? '');
      setSaved(settings);
      setDraft(snapshot(settings));
      setBaseVersion(settings.version);
      setStale(false);
      setNotice('Đã lưu Trang chủ và Trang bài viết trong cùng một lần cập nhật.');
    } catch (reason) {
      if (reason instanceof ApiClientError && reason.status === 412) {
        setStale(true);
        setError('Cấu hình đã được cập nhật ở phiên khác. Lựa chọn của bạn vẫn được giữ; hãy tải phiên bản mới trước khi lưu lại.');
      } else {
        setError(errorMessage(reason));
      }
      onErrorRef.current?.(reason);
    } finally {
      setSaving(false);
    }
  }

  function picker(
    field: keyof ReadingSettingsDraft,
    label: string,
    status: ReadingPageStatus,
    explanation: string,
  ) {
    if (!draft || !saved) return null;
    const selected = draft[field];
    const hasSelected = selected !== null && choices.some(item => item.id === selected);
    const savedId = saved[field];
    return <div className="space-y-2">
      <label htmlFor={`reading-${field}`} className="block text-sm font-semibold text-slate-900">{label}</label>
      <p className="text-sm text-slate-600">{explanation}</p>
      <select
        id={`reading-${field}`}
        className="w-full max-w-xl rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 disabled:opacity-60"
        value={selected ?? ''}
        disabled={saving || loading}
        onChange={event => updateDraft({ ...draft, [field]: pageId(event.target.value) })}
      >
        <option value="">Chưa chọn</option>
        {!hasSelected && selected !== null && <option value={selected}>Page #{selected} (không có trong danh sách đã tải)</option>}
        {choices.map(item => <option key={item.id} value={item.id}>{item.title || '(Không có tiêu đề)'} · #{item.id}</option>)}
      </select>
      <p className="text-xs text-slate-600" role="status">
        Trạng thái Page đã lưu: <strong>{statusLabel(status)}</strong>{savedId !== null ? ` · ID ${savedId}` : ''}
      </p>
    </div>;
  }

  return <section className="min-w-0 space-y-6" data-testid="reading-settings" aria-labelledby="reading-heading">
    <div>
      <h2 id="reading-heading" className="text-xl font-semibold text-slate-900">Trang chủ và Trang bài viết</h2>
      <p className="mt-1 text-sm text-slate-600">
        Chọn Page đã xuất bản để hiển thị ở trang chủ và chọn một Page khác làm địa chỉ danh sách bài viết Blog.
        Việc dùng Page làm Trang bài viết không xóa nội dung đã nhập của Page đó.
      </p>
    </div>
    {loading && <p role="status" className="text-sm text-slate-600">Đang tải các Page và cấu hình hiện tại…</p>}
    {error && <p role="alert" className="rounded-lg border border-rose-200 bg-rose-50 p-3 text-sm text-rose-800">{error}</p>}
    {notice && <p role="status" className="rounded-lg border border-emerald-200 bg-emerald-50 p-3 text-sm text-emerald-800">{notice}</p>}
    {!loading && (!saved || !draft) && <button type="button" onClick={() => void load()} className="od-primary-action">Tải lại</button>}
    {!loading && saved && draft && <form className="space-y-6" onSubmit={event => void handleSave(event)}>
      {picker('homepage_page_id', 'Trang chủ', saved.homepage_status, 'Page được hiển thị tại /; địa chỉ cũ của Page chuyển hướng về /.')}
      {picker('posts_page_id', 'Trang bài viết', saved.posts_page_status, 'Page giữ URL riêng cho archive bài viết; nội dung Page vẫn được lưu trong cơ sở dữ liệu.')}
      {pageOffset < pageTotal && <button type="button" disabled={loadingMore} onClick={() => void loadMorePages()}
        className="rounded-lg border border-slate-300 px-3 py-2 text-sm font-medium text-slate-800">
        {loadingMore ? 'Đang tải…' : `Tải thêm Page đã xuất bản (${pageOffset}/${pageTotal})`}
      </button>}
      {stale && <div className="space-y-2 rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900">
        <p>Phiên bản máy chủ đã thay đổi. Bản nháp đang nhập vẫn được giữ.</p>
        <button type="button" className="underline" disabled={saving} onClick={() => {
          setBaseVersion(saved.version);
          setStale(false);
          if (draftKey) pendingDrafts.set(draftKey, { baseVersion: saved.version, values: { ...draft } });
        }}>Dùng bản nháp này với phiên bản máy chủ mới ({saved.version})</button>
      </div>}
      {draft.homepage_page_id !== null && draft.homepage_page_id === draft.posts_page_id &&
        <p role="alert" className="text-sm text-rose-800">Hai lựa chọn không được trùng Page.</p>}
      <div className="flex flex-wrap items-center gap-3">
        <button type="submit" className="od-primary-action"
          disabled={!dirty || !csrfToken || saving || stale || (draft.homepage_page_id !== null && draft.homepage_page_id === draft.posts_page_id)}>
          {saving ? 'Đang lưu…' : 'Lưu Reading Settings'}
        </button>
        <button type="button" className="rounded-lg border border-slate-300 px-4 py-2 text-sm font-medium text-slate-800"
          disabled={saving} onClick={() => void load()}>Làm mới trạng thái (giữ nháp)</button>
        {dirty && <button type="button" className="text-sm font-medium text-slate-700 underline" disabled={saving}
          onClick={() => {
            if (!window.confirm('Bỏ các lựa chọn chưa lưu và khôi phục cấu hình đang hiển thị từ máy chủ?')) return;
            if (draftKey) pendingDrafts.delete(draftKey);
            setDraft(snapshot(saved));
            setBaseVersion(saved.version);
            setStale(false);
            setError(null);
          }}>Bỏ bản nháp</button>}
        {dirty && <span className="text-sm text-amber-800" role="status">Có lựa chọn chưa lưu</span>}
        <span className="text-xs text-slate-500">Phiên bản {saved.version}</span>
      </div>
    </form>}
  </section>;
}
