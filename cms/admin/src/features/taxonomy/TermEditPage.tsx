import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { createTaxonomyTerm, getTaxonomyTerm, getEffectiveTermConfig, getTaxonomyDefinition,
  listAllTaxonomyTerms, updateTaxonomyTerm,
  type TaxonomyDefinition, type TaxonomyKind, type TaxonomyTerm, type TaxonomyTermChanges } from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';
import { AdminTermPanels, type EffectiveTermConfig, type TermExtensionValues } from '../extensions/AdminContributionRenderer.tsx';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';

interface TermEditProps {
  kind: TaxonomyKind;
  mode: 'new' | 'edit';
  termId: number | null;
  user: AuthUser | null;
  /** Site instance from App, required to safely recover a draft after a 401. */
  siteInstanceId?: string;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
}

interface FormValues { name: string; slug: string; parentId: string }
interface TermDraft {
  form: FormValues;
  baseline: FormValues;
  version: number | null;
  manualSlug: boolean;
  extensions: TermExtensionValues;
  extensionBaseline: TermExtensionValues;
  schemaToken: string | null;
  conflict: boolean;
}

// In-memory only: no localStorage/cookies and no cross-site or cross-account
// visibility. A route change discards the draft; a 401 leaves its route hash.
const termDrafts = new Map<string, TermDraft>();
const termScope = (siteId: string | undefined, actor: AuthUser | null,
  kind: TaxonomyKind, mode: 'new' | 'edit', id: number | null): string | null =>
  siteId && siteId !== 'default-site' && actor
    ? JSON.stringify([siteId, actor.id, actor.role, kind, mode, id]) : null;
const cloneExtensions = (source: TermExtensionValues): TermExtensionValues =>
  Object.fromEntries(Object.entries(source).map(([panel, values]) => [panel, { ...values }]));

const blank = (): FormValues => ({ name: '', slug: '', parentId: '' });
const fromTerm = (term: TaxonomyTerm): FormValues => ({
  name: term.name, slug: term.slug, parentId: term.parent_id === null ? '' : String(term.parent_id),
});
const fieldClass = 'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';
const selectClass = 'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';

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
export function TermEditPage({kind, mode, termId, user, siteInstanceId, csrfToken, onApiError,
  onDirtyChange, onNavigateHash}: TermEditProps) {
  const scope = termScope(siteInstanceId, user, kind, mode, termId);
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);
  const recoveredRef = useRef<TermDraft | null>(scope ? termDrafts.get(scope) ?? null : null);
  const recovered = recoveredRef.current;
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [notFound, setNotFound] = useState(false);
  const [current, setCurrent] = useState<TaxonomyTerm | null>(null);
  const [definition, setDefinition] = useState<TaxonomyDefinition | null>(null);
  const [allTerms, setAllTerms] = useState<TaxonomyTerm[]>([]);
  const [form, setForm] = useState<FormValues>(recovered?.form ?? blank);
  const [baseline, setBaseline] = useState<FormValues>(recovered?.baseline ?? blank);
  const [version, setVersion] = useState<number | null>(recovered?.version ?? null);
  const [manualSlug, setManualSlug] = useState(recovered?.manualSlug ?? false);
  const [conflict, setConflict] = useState(recovered?.conflict ?? false);
  const [latest, setLatest] = useState<TaxonomyTerm | null>(null);
  const [extensionConfig, setExtensionConfig] = useState<EffectiveTermConfig | null>(null);
  const [extensions, setExtensions] = useState<TermExtensionValues>(recovered?.extensions ?? {});
  const [extensionBaseline, setExtensionBaseline] = useState<TermExtensionValues>(recovered?.extensionBaseline ?? {});
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const savingRef = useRef(false);
  const onErrorRef = useRef(onApiError);
  onErrorRef.current = onApiError;
  const reportDirtyRef = useRef(onDirtyChange);
  reportDirtyRef.current = onDirtyChange;
  const listHash = kind === 'category' ? '#/posts/categories' : kind === 'tag' ? '#/posts/tags'
    : `#/taxonomy/${encodeURIComponent(kind)}/terms`;
  const routeHash = mode === 'new' ? `${listHash}/new` : `${listHash}/${termId}/edit`;
  const hierarchical = Boolean(definition?.hierarchical);
  const providerUnavailable = Boolean(definition &&
    (definition.provider_status !== 'active' || definition.declaration_state !== 'declared'));
  const dirty = form.name !== baseline.name || form.slug !== baseline.slug ||
    form.parentId !== baseline.parentId || JSON.stringify(extensions) !== JSON.stringify(extensionBaseline);
  const parentOptions = useMemo(() => allowedParents(allTerms, current?.id ?? null), [allTerms, current?.id]);

  useEffect(() => { reportDirtyRef.current(dirty); }, [dirty]);
  useEffect(() => () => reportDirtyRef.current(false), []);

  useEffect(() => {
    // Do not replace the snapshot with transient empty state during hydration.
    if (!scope || loading) return;
    if (dirty) termDrafts.set(scope, {
      form: { ...form }, baseline: { ...baseline }, version, manualSlug,
      extensions: cloneExtensions(extensions),
      extensionBaseline: cloneExtensions(extensionBaseline),
      schemaToken: extensionConfig?.schema_token ?? recoveredRef.current?.schemaToken ?? null,
      conflict,
    });
    else termDrafts.delete(scope);
  }, [scope, loading, dirty, conflict, form, baseline, version, manualSlug, extensions,
    extensionBaseline, extensionConfig?.schema_token]);

  useEffect(() => () => {
    if (scope && window.location.hash.split('?')[0] !== routeHash) termDrafts.delete(scope);
  }, [scope, routeHash]);

  useEffect(() => {
    const controller = new AbortController();
    const { signal } = controller;
    setLoading(true); setNotFound(false); setError(''); setNotice(''); setConflict(recoveredRef.current?.conflict ?? false);
    const remembered = recoveredRef.current;
    setCurrent(null); setLatest(null);
    setForm(remembered?.form ?? blank()); setBaseline(remembered?.baseline ?? blank());
    setVersion(remembered?.version ?? null);
    setDefinition(null); setAllTerms([]);
    setExtensionConfig(null); setExtensions(remembered?.extensions ?? {});
    setExtensionBaseline(remembered?.extensionBaseline ?? {});
    setManualSlug(remembered?.manualSlug ?? false);
    if (!user) { setLoading(false); return () => controller.abort(); }
    (async () => {
      try {
        if (mode === 'edit' && (!Number.isSafeInteger(termId) || (termId ?? 0) <= 0)) {
          if (!signal.aborted) setNotFound(true);
          return;
        }
        const [saved, descriptor, config] = await Promise.all([
          mode === 'edit' ? getTaxonomyTerm(termId!, signal) : Promise.resolve(null),
          getTaxonomyDefinition(kind, signal),
          mode === 'edit' ? getEffectiveTermConfig(termId!, signal) : Promise.resolve(null),
        ]);
        if (signal.aborted) return;
        if (saved && saved.kind !== kind) { setNotFound(true); return; }
        setDefinition(descriptor);
        if (descriptor.hierarchical) setAllTerms(await listAllTaxonomyTerms(kind, signal));
        if (signal.aborted) return;
        if (saved) {
          const next = fromTerm(saved);
          setCurrent(saved);
          setVersion(remembered?.version ?? saved.version);
          setBaseline(remembered?.baseline ?? next);
          setForm(remembered?.form ?? next);
          setManualSlug(remembered?.manualSlug ?? true);
          if (config) {
            setExtensionConfig(config);
            setExtensions(remembered?.extensions ?? config.extensions);
            setExtensionBaseline(remembered?.extensionBaseline ?? config.extensions);
          }
          if (remembered && (saved.version !== remembered.version ||
              (config?.schema_token ?? null) !== remembered.schemaToken)) {
            // Keep the old CAS and field values until the user inspects the
            // fresh record/schema; never submit an old draft with a new token.
            setConflict(true);
            setLatest(null);
            setError('Term hoặc schema đã đổi trong lúc đăng xuất. Nháp được giữ; hãy đối chiếu phiên bản mới.');
          }
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
      const [fresh, config] = await Promise.all([
        getTaxonomyTerm(termId), getEffectiveTermConfig(termId),
      ]);
      if (!mountedRef.current) return;
      setLatest(fresh.kind === kind ? fresh : null);
      if (fresh.kind === kind) {
        // Rebase extension data by per-field differences. Preserve only fields
        // actually edited locally; otherwise take the newest server value.
        // Withdrawn fields are not submitted or revived by a stale draft.
        setExtensionConfig(config);
        setExtensions(prev => Object.fromEntries(config.panels.map(panel => [
          panel.key,
          Object.fromEntries(panel.fields.map(field => [
            field.key,
            prev[panel.key]?.[field.key] !== extensionBaseline[panel.key]?.[field.key]
              ? (prev[panel.key]?.[field.key] ?? null)
              : (config.extensions[panel.key]?.[field.key] ?? null),
          ])),
        ])) as TermExtensionValues);
        setExtensionBaseline(config.extensions);
      }
      if (fresh.kind !== kind) setNotFound(true);
    } catch (cause) {
      if (!mountedRef.current) return;
      if (cause instanceof ApiClientError && cause.status === 404) setNotFound(true);
      else { setError(cause instanceof Error ? cause.message : 'Không thể đọc bản mới.');
        onErrorRef.current(cause, 'Không thể đọc bản mới.'); }
    } finally { setBusy(false); }
  };

  const continueWithLatest = () => {
    if (!latest || latest.kind !== kind) return;
    const currentServer = fromTerm(latest);
    const changed = (Object.keys(form) as Array<keyof FormValues>)
      .filter(field => form[field] !== baseline[field]);
    const overlaps = changed.filter(field => currentServer[field] !== baseline[field]);
    if (overlaps.length && !window.confirm(
      `Máy chủ cũng thay đổi ${overlaps.join(', ')}. Giữ giá trị đang nhập trên phiên bản mới?`,
    )) return;
    const merged = { ...currentServer };
    for (const field of changed) merged[field] = form[field];
    setBaseline(currentServer); setForm(merged);
    setVersion(latest.version); setConflict(false); setLatest(null); setError('');
    setNotice(`Đã đối chiếu v${latest.version}. Trường đã sửa vẫn được giữ; kiểm tra rồi Lưu.`);
    recoveredRef.current = null;
  };

  const discardAndBack = () => {
    if (dirty && !window.confirm('Bỏ bản nhập taxonomy chưa lưu và về danh sách?')) return;
    if (scope) termDrafts.delete(scope);
    recoveredRef.current = null;
    setForm(baseline); setExtensions(extensionBaseline);
    reportDirtyRef.current(false);
    onNavigateHash(listHash);
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!user || !csrfToken || savingRef.current || busy || loading || notFound || conflict ||
      (mode === 'new' && providerUnavailable) ||
      (mode === 'new' && user.role !== 'admin')) return;
    const canEditCore = user.role === 'admin';
    const name = form.name.trim();
    const slug = form.slug.trim();
    if (!name || [...name].length > 120 || /[\u0000-\u001f\u007f]/.test(name)
      || !/^[a-z][a-z0-9-]{0,63}$/.test(slug)) {
      setError('Tên hoặc slug không hợp lệ (tên 1–120 ký tự, slug kebab-case tối đa 64).');
      return;
    }
    const parentId = hierarchical && form.parentId ? Number(form.parentId) : null;
    if (canEditCore && parentId !== null &&
      (!Number.isSafeInteger(parentId) || !parentOptions.some(t => t.id === parentId))) {
      setError('Term cha không hợp lệ hoặc vượt quá 8 cấp.'); return;
    }
    savingRef.current = true;
    setBusy(true); setError(''); setNotice('');
    (async () => {
      try {
        let saved: TaxonomyTerm;
        if (mode === 'new') {
          saved = await createTaxonomyTerm({kind, name, slug,
            ...(hierarchical ? {parent_id: parentId} : {})}, csrfToken);
        } else {
          if (!current || version === null) return;
          const changes: TaxonomyTermChanges = {};
          if (canEditCore && name !== baseline.name) changes.name = name;
          if (canEditCore && slug !== baseline.slug) changes.slug = slug;
          const oldParent = baseline.parentId ? Number(baseline.parentId) : null;
          if (canEditCore && hierarchical && parentId !== oldParent) changes.parent_id = parentId;
          if (extensionConfig) {
            const altered: TermExtensionValues = {};
            for (const panel of extensionConfig.panels) {
              const patch: TermExtensionValues[string] = {};
              for (const field of panel.fields) {
                const currentValue = extensions[panel.key]?.[field.key];
                const oldValue = extensionBaseline[panel.key]?.[field.key];
                if (currentValue !== oldValue) patch[field.key] = currentValue ?? null;
              }
              if (Object.keys(patch).length) altered[panel.key] = patch;
            }
            if (Object.keys(altered).length) {
              changes.extensions = altered;
              changes.schema_token = extensionConfig.schema_token;
            }
          }
          if (!Object.keys(changes).length) { setNotice('Không có thay đổi cần lưu.'); return; }
          saved = await updateTaxonomyTerm(current.id, version, changes, csrfToken);
        }
        if (!mountedRef.current) return;
        const next = fromTerm(saved);
        setCurrent(saved); setVersion(saved.version); setBaseline(next); setForm(next);
        setExtensionBaseline(extensions);
        if (saved.schema_token && extensionConfig)
          setExtensionConfig({...extensionConfig, schema_token: saved.schema_token});
        setConflict(false); setLatest(null);
        if (scope) termDrafts.delete(scope);
        recoveredRef.current = null;
        reportDirtyRef.current(false);
        setNotice(`Đã lưu term ${definition?.label ?? kind} #${saved.id} (v${saved.version}).`);
        if (mode === 'new') onNavigateHash(`${listHash}/${saved.id}/edit`);
      } catch (cause) {
        if (!mountedRef.current) return;
        if (cause instanceof ApiClientError && (cause.status === 412 || cause.status === 409)) {
          setConflict(true);
          setError(cause.status === 412 ?
            'Phiên bản đã thay đổi (412). Bản nhập chưa lưu vẫn được giữ.' :
            'Cấu hình provider đã thay đổi (409). Bản nhập chưa lưu vẫn được giữ.');
        } else { setError(cause instanceof Error ? cause.message : 'Không thể lưu term.'); }
        onErrorRef.current(cause, 'Không thể lưu term.');
      } finally { savingRef.current = false; setBusy(false); }
    })();
  };

  if (!user) return null;
  if (notFound) return (
    <section data-testid="term-not-found" className="space-y-3">
      <h2 className="text-xl font-bold text-zinc-950">404 Không tìm thấy term {kind}</h2>
      <p className="text-sm text-zinc-600">ID không tồn tại hoặc thuộc loại taxonomy khác.</p>
      {recoveredRef.current && <AdminAlert tone="warning" className="space-y-2">
        <p role="status">Bản nháp từ phiên trước vẫn được giữ để sao chép, nhưng đích lưu hiện không còn khả dụng.</p>
        <pre className="max-w-full overflow-auto whitespace-pre-wrap break-words rounded bg-white/80 p-2 font-mono text-xs">{JSON.stringify({ form, extensions }, null, 2)}</pre>
      </AdminAlert>}
      <AdminButton variant="secondary" size="md" onClick={discardAndBack}>Bỏ bản nhập / Về danh sách</AdminButton>
    </section>
  );
  if (loading) return <p role="status" className="text-sm text-zinc-600">Đang tải màn hình chỉnh sửa taxonomy...</p>;

  return (
    <section className="mx-auto w-full min-w-0 max-w-4xl space-y-5" data-testid="term-edit-page"
      data-term-kind={kind} data-term-id={current?.id ?? ''}>
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-od-accent">Nội dung / {definition?.label ?? kind}</p>
          <h2 className="mt-1 text-xl font-bold tracking-tight text-zinc-950">
            {mode === 'new' ? `Thêm term ${definition?.label ?? kind}` : `Chỉnh sửa term ${definition?.label ?? kind} #${current?.id}`}
          </h2>
          {current && <p className="mt-0.5 text-xs text-zinc-500">ID #{current.id} · Phiên bản v{version} · {dirty ? 'Chưa lưu' : 'Đã đồng bộ'}</p>}
        </div>
        <AdminButton variant="secondary" size="sm" disabled={busy} onClick={discardAndBack}>
          Hủy / Về danh sách
        </AdminButton>
      </header>

      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {providerUnavailable && <AdminAlert tone="warning">
        Taxonomy này không còn hoạt động hoặc đã rút khai báo. Có thể giữ/chỉnh sửa term hiện có; không tạo term mới.
      </AdminAlert>}

      <div data-extension-slot="term-before-core" aria-hidden="true" />
      <form onSubmit={submit} className="min-w-0 space-y-4 rounded-xl border border-zinc-200 bg-white p-5"
        aria-label="Biểu mẫu chỉnh sửa taxonomy">
        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-name">
          <span>Tên *</span>
          <input id="term-name" className={fieldClass} maxLength={120} required disabled={busy || user.role !== 'admin'}
            value={form.name} onChange={event => setForm(previous => ({
              ...previous, name: event.target.value,
              slug: manualSlug ? previous.slug : slugFromName(event.target.value),
            }))} />
        </label>
        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-slug">
          <span>Slug *</span>
          <input id="term-slug" className={fieldClass} maxLength={64} required
            pattern="[a-z](?:[a-z0-9]|-)*" disabled={busy || user.role !== 'admin'} value={form.slug}
            onChange={event => { setManualSlug(true); setForm(previous => ({...previous, slug: event.target.value})); }} />
          <span className="text-xs font-normal text-zinc-500">Slug duy nhất trong loại, bắt đầu bằng a-z, tối đa 64 ký tự.</span>
        </label>
        {hierarchical && <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-parent">
          <span>Term cha</span>
          <select id="term-parent" className={selectClass} value={form.parentId} disabled={busy || user.role !== 'admin'}
            onChange={event => setForm(previous => ({...previous, parentId: event.target.value}))}>
            <option value="">Không có (term gốc)</option>
            {parentOptions.map(term => <option key={term.id} value={term.id}>{term.name} (#{term.id})</option>)}
            {form.parentId && !parentOptions.some(term => String(term.id) === form.parentId) &&
              <option value={form.parentId} disabled>Cha không còn hợp lệ (#{form.parentId})</option>}
          </select>
          <span className="text-xs font-normal text-zinc-500">Không thể chọn hậu duệ hoặc chuyển nhánh vượt 8 cấp.</span>
        </label>}

        {mode === 'edit' && extensionConfig && <AdminTermPanels config={extensionConfig}
          values={extensions} disabled={busy}
          onChange={(panelKey, key, value) => setExtensions(previous => ({
            ...previous,
            [panelKey]: { ...(previous[panelKey] ?? {}), [key]: value },
          }))} />}

        {conflict && <AdminAlert tone="warning" className="space-y-2">
          <p role="status">Dữ liệu máy chủ thay đổi. Các trường đã nhập vẫn được giữ.</p>
          {!latest ? <AdminButton variant="secondary" size="sm" disabled={busy}
            onClick={() => void inspectConflict()}>Tải bản mới để so sánh</AdminButton> : <>
            <p className="text-xs">Bản trên máy chủ: v{latest.version} · {latest.name} · /{latest.slug}</p>
            <AdminButton variant="secondary" size="sm" disabled={busy} onClick={continueWithLatest}>
              Tiếp tục với v{latest.version} (giữ trường đã sửa)
            </AdminButton>
          </>}
        </AdminAlert>}
        <div className="flex flex-wrap gap-2.5 pt-1">
          <AdminButton type="submit" variant="primary" size="md"
            disabled={busy || conflict || !csrfToken || (mode === 'new' && (user.role !== 'admin' || providerUnavailable))}>
            {busy ? 'Đang lưu...' : mode === 'new' ? 'Tạo mới' : 'Lưu thay đổi'}
          </AdminButton>
          <AdminButton variant="secondary" size="md" disabled={busy} onClick={discardAndBack}>
            Hủy sửa
          </AdminButton>
        </div>
      </form>
      <div data-extension-slot="term-after-core" aria-hidden="true" />
    </section>
  );
}
