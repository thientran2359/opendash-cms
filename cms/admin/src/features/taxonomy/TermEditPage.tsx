import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { createTaxonomyTerm, getTaxonomyTerm, listAllTaxonomyTerms, updateTaxonomyTerm,
  type TaxonomyKind, type TaxonomyTerm, type TaxonomyTermChanges } from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';

interface TermEditProps {
  kind: TaxonomyKind;
  mode: 'new' | 'edit';
  termId: number | null;
  user: AuthUser | null;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
}

interface FormValues { name: string; slug: string; parentId: string }
const blank = (): FormValues => ({ name: '', slug: '', parentId: '' });
const fromTerm = (term: TaxonomyTerm): FormValues => ({
  name: term.name, slug: term.slug, parentId: term.parent_id === null ? '' : String(term.parent_id),
});
const fieldClass = 'mt-1 block w-full min-w-0 rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm focus-visible:outline-2 focus-visible:outline-blue-600 disabled:opacity-50';
const buttonClass = 'rounded-lg border border-slate-300 bg-white px-4 py-2 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50';

function slugFromName(name: string): string {
  return name.replace(/[đĐ]/g, 'd').normalize('NFD').replace(/[\u0300-\u036f]/g, '')
    .toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0,64).replace(/-+$/g,'');
}

function allowedParents(terms: TaxonomyTerm[], targetId: number | null): TaxonomyTerm[] {
  const byId = new Map(terms.map(term => [term.id, term]));
  const descendants = (id: number, visited = new Set<number>()): number => {
    if (visited.has(id)) return 9;
    visited.add(id);
    return 1 + Math.max(0, ...terms.filter(t => t.parent_id === id).map(child => descendants(child.id, new Set(visited))));
  };
  const targetHeight = targetId === null ? 1 : descendants(targetId);
  return terms.filter(candidate => {
    let current: TaxonomyTerm | undefined = candidate;
    const seen = new Set<number>();
    let depth = 0;
    while (current) {
      if (seen.has(current.id) || current.id === targetId) return false;
      seen.add(current.id);
      depth++;
      if (depth + targetHeight > 8) return false;
      current = current.parent_id === null ? undefined : byId.get(current.parent_id);
    }
    return true;
  }).sort((a,b) => a.name.localeCompare(b.name,'vi') || a.id-b.id);
}

/** A full-route editor; extension mount slots intentionally contain no fake fields. */
export function TermEditPage({kind, mode, termId, user, csrfToken, onApiError,
  onDirtyChange, onNavigateHash}: TermEditProps) {
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [notFound, setNotFound] = useState(false);
  const [current, setCurrent] = useState<TaxonomyTerm | null>(null);
  const [allCategories, setAllCategories] = useState<TaxonomyTerm[]>([]);
  const [form, setForm] = useState<FormValues>(blank);
  const [baseline, setBaseline] = useState<FormValues>(blank);
  const [version, setVersion] = useState<number | null>(null);
  const [manualSlug, setManualSlug] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [latest, setLatest] = useState<TaxonomyTerm | null>(null);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const savingRef = useRef(false);
  const onErrorRef = useRef(onApiError);
  onErrorRef.current = onApiError;
  const reportDirtyRef = useRef(onDirtyChange);
  reportDirtyRef.current = onDirtyChange;
  const listHash = `#/posts/${kind === 'category' ? 'categories' : 'tags'}`;
  const dirty = form.name !== baseline.name || form.slug !== baseline.slug || form.parentId !== baseline.parentId;
  const parentOptions = useMemo(() => allowedParents(allCategories, current?.id ?? null), [allCategories, current?.id]);

  useEffect(() => { reportDirtyRef.current(dirty); }, [dirty]);
  useEffect(() => () => reportDirtyRef.current(false), []);

  useEffect(() => {
    const controller = new AbortController();
    const { signal } = controller;
    setLoading(true); setNotFound(false); setError(''); setNotice(''); setConflict(false);
    setCurrent(null); setLatest(null); setForm(blank()); setBaseline(blank()); setVersion(null);
    setManualSlug(false);
    if (!user) { setLoading(false); return () => controller.abort(); }
    (async () => {
      try {
        if (mode === 'edit' && (!Number.isSafeInteger(termId) || (termId ?? 0) <= 0)) {
          if (!signal.aborted) setNotFound(true);
          return;
        }
        const [saved, categories] = await Promise.all([
          mode === 'edit' ? getTaxonomyTerm(termId!, signal) : Promise.resolve(null),
          kind === 'category' ? listAllTaxonomyTerms('category', signal) : Promise.resolve([]),
        ]);
        if (signal.aborted) return;
        if (saved && saved.kind !== kind) { setNotFound(true); return; }
        setAllCategories(categories);
        if (saved) {
          const next = fromTerm(saved);
          setCurrent(saved); setVersion(saved.version);
          setBaseline(next); setForm(next); setManualSlug(true);
        }
      } catch (cause) {
        if (signal.aborted) return;
        if (cause instanceof ApiClientError && cause.status === 404) setNotFound(true);
        else { setError(cause instanceof Error ? cause.message : 'Không thể tải term.');
          onErrorRef.current(cause, 'Không thể tải term.'); }
      } finally {
        if (!signal.aborted) setLoading(false);
      }
    })();
    return () => controller.abort();
  }, [kind, mode, termId, user?.id, user?.role]);

  const inspectConflict = async () => {
    if (!termId || busy) return;
    setBusy(true);
    try {
      const fresh = await getTaxonomyTerm(termId);
      setLatest(fresh.kind === kind ? fresh : null);
      if (fresh.kind !== kind) setNotFound(true);
    } catch (cause) {
      if (cause instanceof ApiClientError && cause.status === 404) setNotFound(true);
      else { setError(cause instanceof Error ? cause.message : 'Không thể đọc bản mới.');
        onErrorRef.current(cause, 'Không thể đọc bản mới.'); }
    } finally { setBusy(false); }
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (user?.role !== 'admin' || !csrfToken || savingRef.current || busy || loading || notFound || conflict) return;
    const name = form.name.trim();
    const slug = form.slug.trim();
    if (!name || [...name].length > 120 || /[\u0000-\u001f\u007f]/.test(name)
      || !/^[a-z][a-z0-9-]{0,63}$/.test(slug)) {
      setError('Tên hoặc slug không hợp lệ (tên 1–120 ký tự, slug kebab-case tối đa 64).');
      return;
    }
    const parentId = kind === 'category' && form.parentId ? Number(form.parentId) : null;
    if (parentId !== null && (!Number.isSafeInteger(parentId) || !parentOptions.some(t => t.id === parentId))) {
      setError('Danh mục cha không hợp lệ hoặc vượt quá 8 cấp.'); return;
    }
    savingRef.current = true;
    setBusy(true); setError(''); setNotice('');
    (async () => {
      try {
        let saved: TaxonomyTerm;
        if (mode === 'new') {
          saved = await createTaxonomyTerm({kind, name, slug,
            ...(kind === 'category' ? {parent_id: parentId} : {})}, csrfToken);
        } else {
          if (!current || version === null) return;
          const changes: TaxonomyTermChanges = {};
          if (name !== baseline.name) changes.name = name;
          if (slug !== baseline.slug) changes.slug = slug;
          const oldParent = baseline.parentId ? Number(baseline.parentId) : null;
          if (kind === 'category' && parentId !== oldParent) changes.parent_id = parentId;
          if (!Object.keys(changes).length) { setNotice('Không có thay đổi cần lưu.'); return; }
          saved = await updateTaxonomyTerm(current.id, version, changes, csrfToken);
        }
        const next = fromTerm(saved);
        setCurrent(saved); setVersion(saved.version); setBaseline(next); setForm(next);
        setConflict(false); setLatest(null);
        reportDirtyRef.current(false);
        setNotice(`Đã lưu ${kind === 'category' ? 'danh mục' : 'thẻ'} #${saved.id} (v${saved.version}).`);
        if (mode === 'new') onNavigateHash(`${listHash}/${saved.id}/edit`);
      } catch (cause) {
        if (cause instanceof ApiClientError && cause.status === 412) {
          setConflict(true);
          setError('Phiên bản đã thay đổi (412). Bản nhập chưa lưu vẫn được giữ.');
        } else { setError(cause instanceof Error ? cause.message : 'Không thể lưu term.'); }
        onErrorRef.current(cause, 'Không thể lưu term.');
      } finally { savingRef.current = false; setBusy(false); }
    })();
  };

  if (!user) return null;
  if (notFound) return (
    <section data-testid="term-not-found" className="space-y-3">
      <h2 className="text-xl font-bold text-slate-900">404 Không tìm thấy {kind === 'category' ? 'danh mục' : 'thẻ'}</h2>
      <p className="text-sm text-slate-600">ID không tồn tại hoặc thuộc loại taxonomy khác.</p>
      <button type="button" className={buttonClass} onClick={() => onNavigateHash(listHash)}>Quay lại danh sách</button>
    </section>
  );
  if (loading) return <p role="status">Đang tải màn hình chỉnh sửa taxonomy...</p>;

  return (
    <section className="mx-auto w-full min-w-0 max-w-4xl space-y-5" data-testid="term-edit-page"
      data-term-kind={kind} data-term-id={current?.id ?? ''}>
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="text-xs uppercase tracking-widest text-sky-700">Nội dung / {kind === 'category' ? 'Danh mục' : 'Thẻ'}</p>
          <h2 className="mt-1 text-2xl font-bold text-slate-900">
            {mode === 'new' ? `Thêm ${kind === 'category' ? 'danh mục' : 'thẻ'}` : `Chỉnh sửa ${kind === 'category' ? 'danh mục' : 'thẻ'} #${current?.id}`}
          </h2>
          {current && <p className="text-xs text-slate-600">ID #{current.id} · Phiên bản v{version} · {dirty ? 'Chưa lưu' : 'Đã đồng bộ'}</p>}
        </div>
        <button type="button" className={buttonClass} disabled={busy} onClick={() => onNavigateHash(listHash)}>
          Hủy / Về danh sách
        </button>
      </header>

      {error && <p role="alert" className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-800">{error}</p>}
      {notice && <p role="status" className="rounded-lg bg-emerald-50 p-3 text-sm text-emerald-900">{notice}</p>}

      <div data-extension-slot="term-before-core" aria-hidden="true" />
      <form onSubmit={submit} className="min-w-0 space-y-4 rounded-xl border border-slate-200 bg-white p-5"
        aria-label="Biểu mẫu chỉnh sửa taxonomy">
        <label className="block text-sm font-medium text-slate-800" htmlFor="term-name">
          Tên *
          <input id="term-name" className={fieldClass} maxLength={120} required disabled={busy}
            value={form.name} onChange={event => setForm(previous => ({
              ...previous, name: event.target.value,
              slug: manualSlug ? previous.slug : slugFromName(event.target.value),
            }))} />
        </label>
        <label className="block text-sm font-medium text-slate-800" htmlFor="term-slug">
          Slug *
          <input id="term-slug" className={fieldClass} maxLength={64} required
            pattern="[a-z](?:[a-z0-9]|-)*" disabled={busy} value={form.slug}
            onChange={event => { setManualSlug(true); setForm(previous => ({...previous, slug: event.target.value})); }} />
          <span className="mt-1 block text-xs font-normal text-slate-600">Slug duy nhất trong loại, bắt đầu bằng a-z, tối đa 64 ký tự.</span>
        </label>
        {kind === 'category' && <label className="block text-sm font-medium text-slate-800" htmlFor="term-parent">
          Danh mục cha
          <select id="term-parent" className={fieldClass} value={form.parentId} disabled={busy}
            onChange={event => setForm(previous => ({...previous, parentId: event.target.value}))}>
            <option value="">Không có (danh mục gốc)</option>
            {parentOptions.map(term => <option key={term.id} value={term.id}>{term.name} (#{term.id})</option>)}
            {form.parentId && !parentOptions.some(term => String(term.id) === form.parentId) &&
              <option value={form.parentId} disabled>Cha không còn hợp lệ (#{form.parentId})</option>}
          </select>
          <span className="mt-1 block text-xs font-normal text-slate-600">Không thể chọn hậu duệ hoặc chuyển nhánh vượt 8 cấp.</span>
        </label>}

        {conflict && <div className="space-y-2 rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-950">
          <p role="status">Dữ liệu máy chủ thay đổi. Các trường đã nhập vẫn được giữ.</p>
          {!latest ? <button className={buttonClass} type="button" disabled={busy}
            onClick={() => void inspectConflict()}>Tải bản mới để so sánh</button> : <>
            <p>Bản trên máy chủ: v{latest.version} · {latest.name} · /{latest.slug}</p>
            <button className={buttonClass} type="button" disabled={busy} onClick={() => {
              setVersion(latest.version); setConflict(false); setLatest(null); setError('');
              setNotice(`Tiếp tục với v${latest.version}; các trường đang nhập vẫn giữ nguyên.`);
            }}>Tiếp tục với v{latest.version} (giữ bản nhập)</button>
          </>}
        </div>}
        <div className="flex flex-wrap gap-3">
          <button type="submit" className="od-primary-action" disabled={busy || conflict || !csrfToken || user.role !== 'admin'}>
            {busy ? 'Đang lưu...' : mode === 'new' ? 'Tạo mới' : 'Lưu thay đổi'}
          </button>
          <button type="button" className={buttonClass} disabled={busy} onClick={() => onNavigateHash(listHash)}>
            Hủy sửa
          </button>
        </div>
      </form>
      <div data-extension-slot="term-after-core" aria-hidden="true" />
    </section>
  );
}
