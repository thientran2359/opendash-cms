import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import {
  createTaxonomyTerm,
  deleteTaxonomyTerm,
  listTaxonomyTerms,
  updateTaxonomyTerm,
  type TaxonomyKind,
  type TaxonomyTerm,
  type TaxonomyTermChanges,
} from '../../api/taxonomy.ts';
import { ApiClientError } from '../../api/client.ts';
import type { AuthUser } from '../../types.ts';

interface TaxonomyPageProps {
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
}

interface TermDraft {
  id: number | null;
  version: number | null;
  name: string;
  slug: string;
  parentId: string;
}

const FETCH_SIZE = 100;
const PAGE_SIZE = 20;
const fieldClass = 'mt-1 block w-full min-w-0 rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-600';
const buttonClass = 'rounded-lg border border-slate-300 px-3 py-2 text-sm font-medium text-slate-700 hover:bg-slate-50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-600 disabled:cursor-not-allowed disabled:opacity-50';

function blankDraft(): TermDraft {
  return { id: null, version: null, name: '', slug: '', parentId: '' };
}

function draftFromTerm(term: TaxonomyTerm): TermDraft {
  return {
    id: term.id,
    version: term.version,
    name: term.name,
    slug: term.slug,
    parentId: term.parent_id === null ? '' : String(term.parent_id),
  };
}

function slugFromName(name: string): string {
  return name
    .replace(/[đĐ]/g, 'd')
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 64)
    .replace(/-+$/g, '');
}

function sortByName(a: TaxonomyTerm, b: TaxonomyTerm): number {
  return a.name.localeCompare(b.name, 'vi') || a.id - b.id;
}

// A hierarchy is for navigation only. All validity checks are repeated by storage
// under a write transaction, since a concurrent Admin can reparent the same branch.
function orderCategories(terms: TaxonomyTerm[]): Array<{ term: TaxonomyTerm; depth: number }> {
  const children = new Map<number | null, TaxonomyTerm[]>();
  const ids = new Set(terms.map(term => term.id));
  for (const term of terms) {
    const parent = term.parent_id !== null && ids.has(term.parent_id) ? term.parent_id : null;
    children.set(parent, [...(children.get(parent) ?? []), term]);
  }
  for (const group of children.values()) group.sort(sortByName);
  const result: Array<{ term: TaxonomyTerm; depth: number }> = [];
  const visited = new Set<number>();
  function visit(parent: number | null, depth: number) {
    for (const term of children.get(parent) ?? []) {
      if (visited.has(term.id)) continue;
      visited.add(term.id);
      result.push({ term, depth });
      visit(term.id, depth + 1);
    }
  }
  visit(null, 1);
  // Never hide a term if the server supplied an orphaned or cyclic tree.
  for (const term of [...terms].sort(sortByName)) {
    if (visited.has(term.id)) continue;
    visited.add(term.id);
    result.push({ term, depth: 1 });
    visit(term.id, 2);
  }
  return result;
}

function descendantsHeight(id: number, terms: TaxonomyTerm[], seen = new Set<number>()): number {
  if (seen.has(id)) return 9;
  const visited = new Set(seen);
  visited.add(id);
  const children = terms.filter(term => term.parent_id === id);
  return 1 + Math.max(0, ...children.map(child => descendantsHeight(child.id, terms, visited)));
}

function validParent(
  candidate: TaxonomyTerm,
  draft: TermDraft,
  termMap: Map<number, TaxonomyTerm>,
  branchHeight: number,
): boolean {
  let next: TaxonomyTerm | undefined = candidate;
  let depth = 0;
  const seen = new Set<number>();
  while (next) {
    if (seen.has(next.id) || next.id === draft.id) return false;
    seen.add(next.id);
    depth += 1;
    if (depth + branchHeight > 8) return false;
    next = next.parent_id === null ? undefined : termMap.get(next.parent_id);
  }
  return true;
}

async function fetchAllTerms(kind: TaxonomyKind, signal?: AbortSignal): Promise<TaxonomyTerm[]> {
  const result: TaxonomyTerm[] = [];
  let offset = 0;
  while (true) {
    const response = await listTaxonomyTerms(kind, FETCH_SIZE, offset, signal);
    result.push(...response.items);
    offset += response.items.length;
    if (response.items.length === 0 || offset >= response.total) break;
  }
  // Concurrent creates can shift offset between requests. The ID is stable.
  return [...new Map(result.map(term => [term.id, term])).values()];
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

export function TaxonomyPage({ user, csrfToken, onApiError }: TaxonomyPageProps) {
  const [kind, setKind] = useState<TaxonomyKind>('category');
  const [terms, setTerms] = useState<TaxonomyTerm[]>([]);
  const [page, setPage] = useState(0);
  const [search, setSearch] = useState('');
  const [draft, setDraft] = useState<TermDraft>(blankDraft);
  const [baseline, setBaseline] = useState<TermDraft>(blankDraft);
  const [manualSlug, setManualSlug] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [conflict, setConflict] = useState<'edit' | 'delete' | null>(null);
  const [latestTerm, setLatestTerm] = useState<TaxonomyTerm | null>(null);
  const requestId = useRef(0);
  const lastUser = useRef<number | null>(null);
  const onErrorRef = useRef(onApiError);
  const nameRef = useRef<HTMLInputElement>(null);
  onErrorRef.current = onApiError;

  const admin = user?.role === 'admin';
  const dirty = draft.name !== baseline.name || draft.slug !== baseline.slug ||
    draft.parentId !== baseline.parentId || draft.id !== baseline.id;

  useEffect(() => {
    if (!dirty) return;
    const preventUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = '';
    };
    window.addEventListener('beforeunload', preventUnload);
    return () => window.removeEventListener('beforeunload', preventUnload);
  }, [dirty]);

  useEffect(() => {
    if (!user) return;
    if (lastUser.current !== null && lastUser.current !== user.id) {
      // A different account must not see the previous account's unsaved form.
      setDraft(blankDraft());
      setBaseline(blankDraft());
      setTerms([]);
      setKind('category');
      setSearch('');
      setPage(0);
      setConflict(null);
      setLatestTerm(null);
      setManualSlug(false);
    }
    lastUser.current = user.id;
  }, [user?.id]);

  const refresh = useCallback(async (targetKind: TaxonomyKind, signal?: AbortSignal) => {
    const request = ++requestId.current;
    setLoading(true);
    try {
      const next = await fetchAllTerms(targetKind, signal);
      if (signal?.aborted || request !== requestId.current) return;
      setTerms(next);
      setError('');
    } catch (cause) {
      if (signal?.aborted || request !== requestId.current) return;
      setError(errorMessage(cause, 'Không thể tải category/tag.'));
      onErrorRef.current(cause, 'Không thể tải category/tag.');
    } finally {
      if (!signal?.aborted && request === requestId.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!user) {
      requestId.current += 1;
      setTerms([]);
      setLoading(false);
      return;
    }
    const controller = new AbortController();
    void refresh(kind, controller.signal);
    return () => {
      controller.abort();
      requestId.current += 1;
    };
  }, [kind, user?.id, refresh]);

  useEffect(() => {
    if (draft.id !== null) nameRef.current?.focus();
  }, [draft.id]);

  const termMap = useMemo(() => new Map(terms.map(term => [term.id, term])), [terms]);
  const ordered = useMemo(() =>
    kind === 'category'
      ? orderCategories(terms)
      : [...terms].sort(sortByName).map(term => ({ term, depth: 1 })),
    [terms, kind],
  );
  const query = search.trim().toLocaleLowerCase('vi');
  const filtered = ordered.filter(({ term }) =>
    !query || term.name.toLocaleLowerCase('vi').includes(query) ||
    term.slug.toLowerCase().includes(query) || String(term.id) === query,
  );
  const pageCount = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  useEffect(() => {
    if (page >= pageCount) setPage(pageCount - 1);
  }, [page, pageCount]);
  const visible = filtered.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);
  const branchHeight = draft.id === null ? 1 : descendantsHeight(draft.id, terms);
  const parentCandidates = ordered
    .filter(({ term }) => term.kind === 'category' && validParent(term, draft, termMap, branchHeight));

  const canAbandon = (): boolean =>
    !dirty || window.confirm('Bỏ thay đổi category/tag chưa lưu?');

  const switchKind = (next: TaxonomyKind) => {
    if (next === kind || busy || !canAbandon()) return;
    setKind(next);
    setTerms([]);
    setPage(0);
    setSearch('');
    setDraft(blankDraft());
    setBaseline(blankDraft());
    setManualSlug(false);
    setError('');
    setNotice('');
    setConflict(null);
    setLatestTerm(null);
  };

  const startCreate = () => {
    if (!canAbandon()) return;
    setDraft(blankDraft());
    setBaseline(blankDraft());
    setManualSlug(false);
    setError('');
    setConflict(null);
    setLatestTerm(null);
    nameRef.current?.focus();
  };

  const startEdit = (term: TaxonomyTerm) => {
    if (term.id === draft.id || !canAbandon()) return;
    const next = draftFromTerm(term);
    setDraft(next);
    setBaseline(next);
    setManualSlug(true);
    setError('');
    setNotice('');
    setConflict(null);
    setLatestTerm(null);
  };

  const runMutation = async (action: () => Promise<void>, fallback: string, source: 'edit' | 'delete') => {
    if (busy || !csrfToken) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      await action();
    } catch (cause) {
      if (cause instanceof ApiClientError && cause.status === 412) {
        setConflict(source);
        setLatestTerm(null);
        setError(source === 'edit'
          ? 'Mục này đã thay đổi ở phiên khác (412). Bản nhập vẫn còn. Hãy xem phiên bản mới trước khi thử lại.'
          : 'Mục đã thay đổi ở phiên khác (412). Hãy làm mới và kiểm tra lại trước khi xóa.');
      } else {
        setError(errorMessage(cause, fallback));
      }
      onErrorRef.current(cause, fallback);
    } finally {
      setBusy(false);
    }
  };

  const validateDraft = (): { name: string; slug: string; parent_id: number | null } => {
    const name = draft.name.trim();
    const slug = draft.slug.trim();
    if (!name || [...name].length > 120 || /[\u0000-\u001f\u007f]/.test(name)) {
      throw new Error('Tên phải có từ 1 đến 120 ký tự Unicode và không chứa ký tự điều khiển.');
    }
    if (!/^[a-z][a-z0-9-]{0,63}$/.test(slug)) {
      throw new Error('Slug phải bắt đầu bằng a-z, chỉ chứa a-z, 0-9 hoặc dấu -, tối đa 64 ký tự.');
    }
    const parentId = kind === 'category' && draft.parentId ? Number(draft.parentId) : null;
    if (parentId !== null) {
      const parent = termMap.get(parentId);
      if (!Number.isSafeInteger(parentId) || !parent || !validParent(parent, draft, termMap, branchHeight)) {
        throw new Error('Category cha không hợp lệ hoặc làm cây vượt quá 8 cấp.');
      }
    }
    return { name, slug, parent_id: parentId };
  };

  const save = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!admin || !csrfToken || busy || conflict) return;
    void runMutation(async () => {
      const values = validateDraft();
      if (draft.id === null) {
        const created = await createTaxonomyTerm({
          kind, name: values.name, slug: values.slug,
          ...(kind === 'category' ? { parent_id: values.parent_id } : {}),
        }, csrfToken);
        setDraft(blankDraft());
        setBaseline(blankDraft());
        setManualSlug(false);
        setPage(0);
        setNotice(`Đã tạo ${kind === 'category' ? 'category' : 'tag'} #${created.id}.`);
      } else {
        const changes: TaxonomyTermChanges = {};
        if (values.name !== baseline.name) changes.name = values.name;
        if (values.slug !== baseline.slug) changes.slug = values.slug;
        if (kind === 'category' && values.parent_id !== (baseline.parentId ? Number(baseline.parentId) : null)) {
          changes.parent_id = values.parent_id;
        }
        if (Object.keys(changes).length === 0) {
          setNotice('Không có thay đổi cần lưu.');
          return;
        }
        const updated = await updateTaxonomyTerm(draft.id, draft.version!, changes, csrfToken);
        const next = draftFromTerm(updated);
        setDraft(next);
        setBaseline(next);
        setNotice(`Đã lưu mục #${updated.id}, phiên bản v${updated.version}.`);
      }
      await refresh(kind);
    }, 'Không thể lưu category/tag.', 'edit');
  };

  const remove = (term: TaxonomyTerm) => {
    if (!admin || !csrfToken || busy) return;
    if (!window.confirm(`Xóa vĩnh viễn ${term.kind === 'category' ? 'category' : 'tag'} "${term.name}" (#${term.id})? Mục còn category con hoặc liên kết nội dung sẽ bị máy chủ chặn.`)) return;
    void runMutation(async () => {
      await deleteTaxonomyTerm(term.id, term.version, csrfToken);
      if (draft.id === term.id) {
        setDraft(blankDraft());
        setBaseline(blankDraft());
        setManualSlug(false);
      }
      setConflict(null);
      setLatestTerm(null);
      setNotice(`Đã xóa mục #${term.id}.`);
      if (visible.length === 1 && page > 0) setPage(current => Math.max(0, current - 1));
      await refresh(kind);
    }, 'Không thể xóa category/tag.', 'delete');
  };

  const inspectConflict = async () => {
    if (!draft.id || busy) return;
    setBusy(true);
    setError('');
    try {
      const updated = await fetchAllTerms(kind);
      setTerms(updated);
      const latest = updated.find(term => term.id === draft.id) ?? null;
      setLatestTerm(latest);
      if (!latest) setError('Mục đã bị xóa. Bản nhập vẫn được giữ để bạn sao chép khi cần.');
    } catch (cause) {
      setError(errorMessage(cause, 'Không thể tải phiên bản mới.'));
      onErrorRef.current(cause, 'Không thể tải phiên bản mới.');
    } finally {
      setBusy(false);
    }
  };

  const acceptLatestVersion = () => {
    if (!latestTerm || draft.id !== latestTerm.id) return;
    setDraft(previous => ({ ...previous, version: latestTerm.version }));
    setConflict(null);
    setLatestTerm(null);
    setError('');
    setNotice(`Đang dùng phiên bản v${latestTerm.version}. Các trường bạn nhập vẫn giữ nguyên; chỉ trường đã sửa mới được gửi khi Lưu.`);
  };

  return (
    <section aria-labelledby="taxonomy-heading" data-testid="taxonomy-page" className="w-full min-w-0 space-y-6">
      <div>
        <p className="text-xs font-semibold uppercase tracking-widest text-sky-700">Nội dung / Taxonomy</p>
        <h2 id="taxonomy-heading" className="mt-2 text-2xl font-bold text-slate-900">Danh mục và thẻ</h2>
        <p className="mt-2 text-sm text-slate-600">Category có tối đa 8 cấp; tag không có cha. ID giữ nguyên khi đổi tên, slug hoặc chuyển category cha.</p>
        {!admin && user && <p className="mt-2 text-sm text-slate-600">Biên tập viên có thể xem danh sách; chỉ Admin mới tạo, sửa, chuyển cấp hoặc xóa.</p>}
      </div>

      <div role="group" aria-label="Loại taxonomy" className="flex flex-wrap gap-2">
        <button type="button" className={kind === 'category' ? 'od-primary-action' : buttonClass} disabled={busy}
          aria-pressed={kind === 'category'} onClick={() => switchKind('category')}>Danh mục (Category)</button>
        <button type="button" className={kind === 'tag' ? 'od-primary-action' : buttonClass} disabled={busy}
          aria-pressed={kind === 'tag'} onClick={() => switchKind('tag')}>Thẻ (Tag)</button>
      </div>

      {error && <div role="alert" className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-800">
        {error}
        {conflict === 'delete' && <button type="button" className="ml-2 underline" disabled={loading || busy}
          onClick={() => { setConflict(null); void refresh(kind); }}>Làm mới danh sách</button>}
      </div>}
      {notice && <p role="status" className="rounded-lg bg-emerald-50 p-3 text-sm text-emerald-900">{notice}</p>}

      <div className="grid min-w-0 gap-6 xl:grid-cols-[minmax(0,1fr)_minmax(280px,360px)]">
        <div className="min-w-0 space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h3 className="text-lg font-semibold text-slate-900">{kind === 'category' ? 'Danh mục' : 'Thẻ'} ({terms.length})</h3>
            <div className="flex flex-wrap gap-2">
              {admin && <button type="button" className={buttonClass} disabled={busy} onClick={startCreate}>Thêm mới</button>}
              <button type="button" className={buttonClass} disabled={loading || busy}
                onClick={() => void refresh(kind)}>Làm mới</button>
            </div>
          </div>
          <label htmlFor="taxonomy-search" className="block text-sm font-medium text-slate-700">
            Tìm theo tên, slug hoặc ID
            <input id="taxonomy-search" className={fieldClass} type="search" value={search}
              onChange={event => { setSearch(event.target.value); setPage(0); }}
              placeholder="Nhập tên, slug hoặc ID..." />
          </label>
          {loading && <p role="status" className="text-sm text-slate-600">Đang tải danh sách...</p>}
          <div className="space-y-2" aria-busy={loading}>
            {!loading && visible.length === 0 && <p className="rounded-xl border border-slate-200 p-4 text-sm text-slate-500">
              {search ? 'Không tìm thấy mục phù hợp.' : 'Chưa có mục nào. Admin có thể thêm ở biểu mẫu bên cạnh.'}
            </p>}
            {visible.map(({ term, depth }) => (
              <article key={term.id} className={'min-w-0 rounded-xl border p-3 sm:p-4 ' +
                (draft.id === term.id && admin ? 'border-blue-300 bg-blue-50/40' : 'border-slate-200 bg-white')}>
                <div className="flex min-w-0 flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0 flex-1 space-y-1">
                    <h4 className="break-words text-sm font-semibold text-slate-900">
                      {kind === 'category' && depth > 1 && <span className="mr-1 text-slate-400" aria-hidden="true">{'— '.repeat(Math.min(depth - 1, 7))}</span>}
                      {term.name}
                    </h4>
                    <p className="break-all text-xs text-slate-600">/{term.slug} · ID #{term.id} · v{term.version}</p>
                    {kind === 'category' && <p className="text-xs text-slate-500">
                      {term.parent_id === null ? 'Danh mục gốc' : `Cha: ${termMap.get(term.parent_id)?.name ?? 'Không tìm thấy'} (#${term.parent_id})`} · Cấp {depth}
                    </p>}
                  </div>
                  {admin && <div className="flex flex-wrap gap-2">
                    <button type="button" className={buttonClass} disabled={busy}
                      aria-label={`Sửa ${term.name}, ID ${term.id}`} onClick={() => startEdit(term)}>Sửa</button>
                    <button type="button" className={buttonClass} disabled={busy}
                      aria-label={`Xóa ${term.name}, ID ${term.id}`} onClick={() => remove(term)}>Xóa</button>
                  </div>}
                </div>
              </article>
            ))}
          </div>
          <nav aria-label="Phân trang taxonomy" className="flex flex-wrap items-center justify-between gap-3 text-sm text-slate-600">
            <span>Hiển thị {filtered.length ? page * PAGE_SIZE + 1 : 0}–{Math.min(filtered.length, (page + 1) * PAGE_SIZE)} / {filtered.length} · Trang {page + 1}/{pageCount}</span>
            <div className="flex gap-2">
              <button type="button" className={buttonClass} disabled={loading || page === 0}
                onClick={() => setPage(current => current - 1)}>Trước</button>
              <button type="button" className={buttonClass} disabled={loading || page + 1 >= pageCount}
                onClick={() => setPage(current => current + 1)}>Sau</button>
            </div>
          </nav>
        </div>

        {admin && <div className="min-w-0">
          <form onSubmit={save} className="min-w-0 space-y-4 rounded-xl border border-slate-200 bg-white p-4"
            aria-labelledby="taxonomy-form-heading">
            <div>
              <h3 id="taxonomy-form-heading" className="text-lg font-semibold text-slate-900">
                {draft.id === null ? `Thêm ${kind === 'category' ? 'danh mục' : 'thẻ'}` : `Sửa mục #${draft.id}`}
              </h3>
              {draft.id !== null && <p className="mt-1 text-xs text-slate-600">Phiên bản đang sửa: v{draft.version}</p>}
            </div>
            <label htmlFor="taxonomy-name" className="block text-sm font-medium text-slate-800">
              Tên
              <input ref={nameRef} id="taxonomy-name" className={fieldClass} required maxLength={120}
                value={draft.name} onChange={event => setDraft(previous => ({
                  ...previous, name: event.target.value,
                  slug: manualSlug ? previous.slug : slugFromName(event.target.value),
                }))} disabled={busy} />
            </label>
            <label htmlFor="taxonomy-slug" className="block text-sm font-medium text-slate-800">
              Slug
              <input id="taxonomy-slug" className={fieldClass} required maxLength={64}
                pattern="[a-z](?:[a-z0-9]|-)*" aria-describedby="taxonomy-slug-help"
                value={draft.slug} onChange={event => { setManualSlug(true); setDraft(previous => ({ ...previous, slug: event.target.value })); }}
                disabled={busy} />
              <span id="taxonomy-slug-help" className="mt-1 block text-xs font-normal text-slate-500">Chữ thường a-z, số 0-9, dấu gạch ngang; bắt đầu bằng chữ. Slug duy nhất trong từng loại.</span>
            </label>
            {kind === 'category' && <label htmlFor="taxonomy-parent" className="block text-sm font-medium text-slate-800">
              Danh mục cha
              <select id="taxonomy-parent" className={fieldClass} value={draft.parentId} disabled={busy || loading}
                onChange={event => setDraft(previous => ({ ...previous, parentId: event.target.value }))}>
                <option value="">Không có (danh mục gốc)</option>
                {parentCandidates.map(({ term, depth }) => (
                  <option key={term.id} value={term.id}>{'— '.repeat(Math.min(depth - 1, 7))}{term.name} (#{term.id})</option>
                ))}
                {draft.parentId && !parentCandidates.some(({ term }) => String(term.id) === draft.parentId) &&
                  <option value={draft.parentId} disabled>Cha không còn hợp lệ (#{draft.parentId})</option>}
              </select>
              <span className="mt-1 block text-xs font-normal text-slate-500">Có thể chuyển cha khi sửa. Các lựa chọn gây vòng lặp hoặc vượt quá 8 cấp được ẩn.</span>
            </label>}
            {conflict === 'edit' && draft.id !== null && <div className="space-y-3 rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-950">
              <p role="status">Đã có thay đổi từ phiên khác. Dữ liệu đang nhập của bạn vẫn được giữ.</p>
              {!latestTerm && <button type="button" className={buttonClass} disabled={busy}
                onClick={() => void inspectConflict()}>Tải bản mới để so sánh</button>}
              {latestTerm && <>
                <p>Bản trên máy chủ: v{latestTerm.version} · {latestTerm.name} · /{latestTerm.slug}
                  {kind === 'category' && ` · Cha #${latestTerm.parent_id ?? 'gốc'}`}</p>
                <button type="button" className={buttonClass} disabled={busy} onClick={acceptLatestVersion}>
                  Tiếp tục với v{latestTerm.version} (giữ bản nhập)
                </button>
              </>}
            </div>}
            <div className="flex flex-wrap gap-2">
              <button type="submit" className="od-primary-action" disabled={!csrfToken || busy || loading || conflict !== null}>
                {busy ? 'Đang lưu...' : draft.id === null ? 'Tạo mới' : 'Lưu thay đổi'}
              </button>
              {draft.id !== null && <button type="button" className={buttonClass} disabled={busy} onClick={startCreate}>
                Hủy sửa
              </button>}
            </div>
          </form>
        </div>}
      </div>
    </section>
  );
}
