import { useCallback, useEffect, useMemo, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { deleteTaxonomyTerm, listAllTaxonomyTerms, type TaxonomyKind, type TaxonomyTerm } from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';

interface TaxonomyPageProps {
  kind: TaxonomyKind;
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onNavigateHash: (hash: string) => void;
}

const PAGE_SIZE = 20;
const buttonClass = 'rounded-lg border border-slate-300 px-3 py-2 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50';

function orderedTerms(kind: TaxonomyKind, terms: TaxonomyTerm[]): Array<{ term: TaxonomyTerm; depth: number }> {
  const sort = (a: TaxonomyTerm, b: TaxonomyTerm) => a.name.localeCompare(b.name, 'vi') || a.id - b.id;
  if (kind === 'tag') return [...terms].sort(sort).map(term => ({ term, depth: 0 }));
  const ids = new Set(terms.map(term => term.id));
  const children = new Map<number | null, TaxonomyTerm[]>();
  for (const term of terms) {
    const parent = term.parent_id !== null && ids.has(term.parent_id) ? term.parent_id : null;
    children.set(parent, [...(children.get(parent) ?? []), term]);
  }
  for (const group of children.values()) group.sort(sort);
  const result: Array<{ term: TaxonomyTerm; depth: number }> = [];
  const seen = new Set<number>();
  const visit = (parent: number | null, depth: number) => {
    for (const term of children.get(parent) ?? []) {
      if (seen.has(term.id)) continue;
      seen.add(term.id);
      result.push({ term, depth });
      visit(term.id, depth + 1);
    }
  };
  visit(null, 0);
  for (const term of [...terms].sort(sort)) {
    if (!seen.has(term.id)) { seen.add(term.id); result.push({ term, depth: 0 }); visit(term.id, 1); }
  }
  return result;
}

/** Route chooses the fixed taxonomy kind. Lists never mount an editor form. */
export function TaxonomyPage({ kind, user, csrfToken, onApiError, onNavigateHash }: TaxonomyPageProps) {
  const [terms, setTerms] = useState<TaxonomyTerm[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(0);
  const admin = user?.role === 'admin';
  const listHash = `#/posts/${kind === 'category' ? 'categories' : 'tags'}`;

  const refresh = useCallback(async (signal?: AbortSignal) => {
    if (!user) return;
    setLoading(true);
    try {
      const received = await listAllTaxonomyTerms(kind, signal);
      if (signal?.aborted) return;
      setTerms(received);
      setError('');
    } catch (cause) {
      if (signal?.aborted) return;
      setTerms([]);
      setError(cause instanceof Error ? cause.message : 'Không thể tải taxonomy.');
      onApiError(cause, 'Không thể tải taxonomy.');
    } finally {
      if (!signal?.aborted) setLoading(false);
    }
  }, [kind, user?.id, user?.role]);

  useEffect(() => {
    setSearch(''); setPage(0); setNotice(''); setError(''); setTerms([]);
    const controller = new AbortController();
    void refresh(controller.signal);
    return () => controller.abort();
  }, [refresh]);

  const ordered = useMemo(() => orderedTerms(kind, terms), [kind, terms]);
  const query = search.trim().toLocaleLowerCase('vi');
  const filtered = ordered.filter(({term}) => !query || term.name.toLocaleLowerCase('vi').includes(query)
    || term.slug.includes(query) || String(term.id) === query);
  const pages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const currentPage = Math.min(page, pages - 1);
  const visible = filtered.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE);
  const byId = useMemo(() => new Map(terms.map(term => [term.id, term])), [terms]);

  const remove = async (term: TaxonomyTerm) => {
    if (!admin || !csrfToken || busy) return;
    if (!window.confirm(`Xóa vĩnh viễn ${kind === 'category' ? 'danh mục' : 'thẻ'} "${term.name}" (#${term.id})? Liên kết nội dung hoặc mục con sẽ chặn xóa.`)) return;
    setBusy(true); setError(''); setNotice('');
    try {
      await deleteTaxonomyTerm(term.id, term.version, csrfToken);
      setNotice(`Đã xóa #${term.id}.`);
      await refresh();
    } catch (cause) {
      setError(cause instanceof ApiClientError && cause.status === 412
        ? 'Mục đã thay đổi (412). Hãy tải lại danh sách trước khi xóa.'
        : cause instanceof Error ? cause.message : 'Không thể xóa term.');
      onApiError(cause, 'Không thể xóa term.');
    } finally { setBusy(false); }
  };

  return (
    <section className="w-full min-w-0 space-y-5" data-testid="taxonomy-page" data-taxonomy-kind={kind}>
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-sky-700">Nội dung / Taxonomy</p>
          <h2 className="mt-2 text-2xl font-bold text-slate-900">
            {kind === 'category' ? 'Danh mục' : 'Thẻ'} ({terms.length})
          </h2>
          <p className="mt-1 text-sm text-slate-600">
            {kind === 'category' ? 'Danh mục có tối đa 8 cấp; có thể chuyển cha mà không đổi ID.' : 'Thẻ không có cha; đổi tên hoặc slug không đổi ID.'}
          </p>
          {!admin && user && <p className="text-sm text-slate-600">Editor chỉ xem danh sách; Admin mới được chỉnh sửa.</p>}
        </div>
        <div className="flex flex-wrap gap-2">
          {admin && <button type="button" className="od-primary-action" disabled={busy}
            onClick={() => onNavigateHash(`${listHash}/new`)}>Thêm mới</button>}
          <button type="button" className={buttonClass} disabled={loading || busy}
            onClick={() => void refresh()}>Làm mới</button>
        </div>
      </header>

      {error && <p role="alert" className="rounded-lg bg-red-50 p-3 text-sm text-red-800">{error}</p>}
      {notice && <p role="status" className="rounded-lg bg-emerald-50 p-3 text-sm text-emerald-800">{notice}</p>}
      <label htmlFor="taxonomy-search" className="block text-sm font-medium text-slate-700">
        Tìm theo tên, slug hoặc ID
        <input id="taxonomy-search" type="search" value={search}
          onChange={event => { setSearch(event.target.value); setPage(0); }}
          placeholder="Nhập tên, slug hoặc ID..."
          className="mt-1 block w-full rounded-lg border border-slate-300 px-3 py-2 text-sm" />
      </label>
      {loading && <p role="status">Đang tải danh sách...</p>}
      <div className="space-y-2" aria-busy={loading}>
        {!loading && visible.length === 0 && <p className="rounded-lg border border-slate-200 p-4 text-sm text-slate-500">
          {search ? 'Không tìm thấy mục phù hợp.' : 'Chưa có mục nào.'}
        </p>}
        {visible.map(({term, depth}) => (
          <article key={term.id} data-term-id={term.id} className="rounded-xl border border-slate-200 bg-white p-4">
            <div className="flex min-w-0 flex-wrap items-center justify-between gap-3">
              <div className="min-w-0 space-y-1">
                <h3 className="break-words text-sm font-semibold text-slate-900">
                  {kind === 'category' && depth > 0 && <span aria-hidden="true" className="mr-1 text-slate-400">{'— '.repeat(Math.min(depth, 7))}</span>}
                  {term.name}
                </h3>
                <p className="break-all text-xs text-slate-600">/{term.slug} · ID #{term.id} · v{term.version}</p>
                {kind === 'category' && <p className="text-xs text-slate-500">
                  {term.parent_id === null ? 'Danh mục gốc' : `Cha: ${byId.get(term.parent_id)?.name ?? `#${term.parent_id}`}`} · Cấp {depth + 1}
                </p>}
              </div>
              {admin && <div className="flex flex-wrap gap-2">
                <button type="button" className={buttonClass} disabled={busy}
                  aria-label={`Sửa ${term.name}, ID ${term.id}`}
                  onClick={() => onNavigateHash(`${listHash}/${term.id}/edit`)}>Sửa</button>
                <button type="button" className={buttonClass} disabled={busy}
                  aria-label={`Xóa ${term.name}, ID ${term.id}`}
                  onClick={() => void remove(term)}>Xóa</button>
              </div>}
            </div>
          </article>
        ))}
      </div>
      <nav aria-label="Phân trang taxonomy" className="flex flex-wrap items-center justify-between gap-3 text-sm text-slate-600">
        <span>Hiển thị {filtered.length ? currentPage * PAGE_SIZE + 1 : 0}–{Math.min(filtered.length, (currentPage + 1) * PAGE_SIZE)} / {filtered.length}</span>
        <div className="flex items-center gap-2">
          <button type="button" className={buttonClass} disabled={loading || currentPage === 0}
            onClick={() => setPage(currentPage - 1)}>Trước</button>
          <span>Trang {currentPage + 1}/{pages}</span>
          <button type="button" className={buttonClass} disabled={loading || currentPage + 1 >= pages}
            onClick={() => setPage(currentPage + 1)}>Sau</button>
        </div>
      </nav>
    </section>
  );
}
