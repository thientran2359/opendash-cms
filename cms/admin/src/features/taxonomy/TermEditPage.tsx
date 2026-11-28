import { __, _x, useI18n } from '../../i18n/index.tsx';
import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { mediaThumbnailUrl } from '../../api/media.ts';
import {
  createTaxonomyTerm,
  getEffectiveTermConfig,
  getNewTermEffectiveConfig,
  getTaxonomyDefinition,
  getTaxonomyTerm,
  listAllTaxonomyTerms,
  updateTaxonomyTerm,
  type EffectiveTermEditorConfig,
  type TaxonomyDefinition,
  type TaxonomyTerm,
  type TermSeoInput,
} from '../../api/taxonomy.ts';
import { EMPTY_ADMIN_LOCALE_STATE } from '../../api/locales.ts';
import { MediaPicker } from '../media/MediaPicker.tsx';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { useSiteTimezone } from '../../fields/useSiteTimezone.ts';
import { TermMetaFieldGroups } from './TermMetaFieldGroups.tsx';
import { TermSeoFields } from './TermSeoFields.tsx';
import {
  type TermEditProps,
  type FormValues,
  type TermDraft,
  termDrafts,
  termScope,
  cloneValues,
  defaultTermSeo,
  blank,
  fromTerm,
  rebaseTermValues,
  buildCreateTermPayload,
  buildPatchTermChanges,
  fieldClass,
  selectClass,
  slugFromName,
  allowedParents,
} from './termEditHelpers.ts';

export function TermEditPage({
  kind, mode, cptTypeKey, termId, user, siteInstanceId, csrfToken,
  localeState = EMPTY_ADMIN_LOCALE_STATE, embedded = false,
  onApiError, onDirtyChange, onNavigateHash, onSaved, onCancelEdit,
}: TermEditProps) {
  useI18n();
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
  const [sortOrderText, setSortOrderText] = useState<string>(() => String(form.sortOrder));
  const [baseline, setBaseline] = useState<FormValues>(recovered?.baseline ?? blank);
  const [version, setVersion] = useState<number | null>(recovered?.version ?? null);
  const [manualSlug, setManualSlug] = useState(recovered?.manualSlug ?? false);
  const [conflict, setConflict] = useState(recovered?.conflict ?? false);
  const [latest, setLatest] = useState<TaxonomyTerm | null>(null);
  const [editorConfig, setEditorConfig] = useState<EffectiveTermEditorConfig | null>(null);
  const [values, setValues] = useState<Record<string, unknown>>(recovered?.values ?? {});
  const [valuesBaseline, setValuesBaseline] = useState<Record<string, unknown>>(recovered?.valuesBaseline ?? {});
  const [seo, setSeo] = useState<TermSeoInput>(recovered?.seo ?? defaultTermSeo);
  const [seoBaseline, setSeoBaseline] = useState<TermSeoInput>(recovered?.seoBaseline ?? defaultTermSeo);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [pickerOpen, setPickerOpen] = useState(false);

  const onErrorRef = useRef(onApiError); onErrorRef.current = onApiError;
  const reportDirtyRef = useRef(onDirtyChange); reportDirtyRef.current = onDirtyChange;
  const onSavedRef = useRef(onSaved); onSavedRef.current = onSaved;
  const onCancelRef = useRef(onCancelEdit); onCancelRef.current = onCancelEdit;
  const savingRef = useRef(false);

  const listHash = cptTypeKey
    ? `#/cpt/${encodeURIComponent(cptTypeKey)}/taxonomy/${encodeURIComponent(kind)}`
    : kind === 'category' ? '#/posts/categories' : kind === 'tag' ? '#/posts/tags'
      : `#/taxonomy/${encodeURIComponent(kind)}/terms`;
  const routeHash = mode === 'new' ? `${listHash}/new` : `${listHash}/${termId}/edit`;
  const hierarchical = Boolean(definition?.hierarchical);
  const providerUnavailable = Boolean(definition && (definition.provider_status !== 'active' || definition.declaration_state !== 'declared'));
  const canCreate = Boolean(user && (user.role === 'admin' || Boolean(definition?.settings?.editor_can_create_terms)));
  const canEditCore = mode === 'new' ? canCreate : user?.role === 'admin';
  const defaultLocaleCode = localeState.defaultLocale?.code ?? '';
  const [selectedLocale, setSelectedLocale] = useState<string>(defaultLocaleCode);
  useEffect(() => { if (!selectedLocale && defaultLocaleCode) setSelectedLocale(defaultLocaleCode); }, [selectedLocale, defaultLocaleCode]);

  const activeTermLocale = mode === 'edit' ? (current?.locale ?? defaultLocaleCode) : selectedLocale;
  const siteTimezone = useSiteTimezone();
  const dirty =
    JSON.stringify(form) !== JSON.stringify(baseline) ||
    JSON.stringify(values) !== JSON.stringify(valuesBaseline) ||
    JSON.stringify(seo) !== JSON.stringify(seoBaseline);

  const parentOptions = useMemo(() => {
    const scoped = activeTermLocale ? allTerms.filter(t => t.locale === activeTermLocale) : allTerms;
    return hierarchical ? allowedParents(scoped, current?.id ?? null) : [];
  }, [hierarchical, allTerms, current?.id, activeTermLocale]);

  useEffect(() => { reportDirtyRef.current(dirty); }, [dirty]);
  useEffect(() => () => reportDirtyRef.current(false), []);

  useEffect(() => {
    if (!scope || loading) return;
    if (dirty) {
      termDrafts.set(scope, {
        form: { ...form }, baseline: { ...baseline }, version, manualSlug,
        values: cloneValues(values), valuesBaseline: cloneValues(valuesBaseline),
        seo: { ...seo }, seoBaseline: { ...seoBaseline },
        schemaToken: editorConfig?.schema_token ?? recoveredRef.current?.schemaToken ?? null, conflict,
      });
    } else termDrafts.delete(scope);
  }, [scope, loading, dirty, conflict, form, baseline, version, manualSlug, values, valuesBaseline, seo, seoBaseline, editorConfig?.schema_token]);

  useEffect(() => () => {
    const currentHash = window.location.hash.split('?')[0];
    const matchesEmbeddedCreate = embedded && mode === 'new' && (currentHash === listHash || currentHash === `${listHash}/new`);
    if (scope && !matchesEmbeddedCreate && currentHash !== routeHash) termDrafts.delete(scope);
  }, [scope, routeHash, embedded, mode, listHash]);

  useEffect(() => {
    const controller = new AbortController();
    const { signal } = controller;
    setLoading(true); setNotFound(false); setError(''); setNotice(''); setConflict(recoveredRef.current?.conflict ?? false);
    const remembered = recoveredRef.current;
    setCurrent(null); setLatest(null);
    setForm(remembered?.form ?? blank()); setBaseline(remembered?.baseline ?? blank());
    setVersion(remembered?.version ?? null); setDefinition(null); setAllTerms([]); setEditorConfig(null);
    setValues(remembered?.values ?? {}); setValuesBaseline(remembered?.valuesBaseline ?? {});
    setSeo(remembered?.seo ?? defaultTermSeo()); setSeoBaseline(remembered?.seoBaseline ?? defaultTermSeo());
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
          mode === 'edit' ? getEffectiveTermConfig(termId!, signal) : getNewTermEffectiveConfig(kind, signal),
        ]);
        if (signal.aborted) return;
        if (saved && saved.kind !== kind) { setNotFound(true); return; }
        setDefinition(descriptor);
        if (descriptor.hierarchical) setAllTerms(await listAllTaxonomyTerms(kind, signal));
        if (signal.aborted) return;

        setEditorConfig(config);
        setValues(remembered?.values ?? cloneValues(config.values));
        setValuesBaseline(remembered?.valuesBaseline ?? cloneValues(config.values));
        const initSeo = config.seo ?? defaultTermSeo();
        setSeo(remembered?.seo ?? { ...initSeo });
        setSeoBaseline(remembered?.seoBaseline ?? { ...initSeo });

        if (saved) {
          const next = fromTerm(saved);
          setCurrent(saved); setVersion(remembered?.version ?? saved.version);
          setBaseline(remembered?.baseline ?? next); setForm(remembered?.form ?? next);
          setManualSlug(remembered?.manualSlug ?? true);
          if (remembered && (saved.version !== remembered.version || config.schema_token !== remembered.schemaToken)) {
            setConflict(true); setLatest(null);
            setError(__("The term or schema changed while signed out. Your draft is retained; review the latest version."));
          }
        }
      } catch (cause) {
        if (signal.aborted) return;
        if (cause instanceof ApiClientError && cause.status === 404) setNotFound(true);
        else { setError(cause instanceof Error ? cause.message : __("Could not load term.")); onErrorRef.current(cause, __("Could not load term.")); }
      } finally { if (!signal.aborted) setLoading(false); }
    })();
    return () => controller.abort();
  }, [kind, mode, termId, user?.id, user?.role]);

  const inspectConflict = async () => {
    if (!termId || busy) return;
    setBusy(true);
    try {
      const [fresh, config] = await Promise.all([getTaxonomyTerm(termId), getEffectiveTermConfig(termId)]);
      if (!mountedRef.current) return;
      setLatest(fresh.kind === kind ? fresh : null);
      if (fresh.kind === kind) {
        setEditorConfig(config);
        setValues(rebaseTermValues(config, values, valuesBaseline));
        setValuesBaseline(cloneValues(config.values));
        const freshSeo = config.seo ?? defaultTermSeo();
        if (JSON.stringify(seo) === JSON.stringify(seoBaseline)) setSeo({ ...freshSeo });
        setSeoBaseline({ ...freshSeo });
      }
      if (fresh.kind !== kind) setNotFound(true);
    } catch (cause) {
      if (!mountedRef.current) return;
      if (cause instanceof ApiClientError && cause.status === 404) setNotFound(true);
      else { setError(cause instanceof Error ? cause.message : __("Could not read latest version.")); onErrorRef.current(cause, __("Could not read latest version.")); }
    } finally { if (mountedRef.current) setBusy(false); }
  };

  const continueWithLatest = () => {
    if (!latest || latest.kind !== kind) return;
    const currentServer = fromTerm(latest);
    const changed = (Object.keys(form) as Array<keyof FormValues>).filter(f => form[f] !== baseline[f]);
    const overlaps = changed.filter(f => currentServer[f] !== baseline[f]);
    if (overlaps.length && !window.confirm(__("Server version changed {{value1}}. Keep edited input and continue with v{{value2}}?", { value1: overlaps.join(', '), value2: latest.version }))) return;
    const merged: FormValues = { ...currentServer };
    for (const f of changed) Object.assign(merged, { [f]: form[f] });
    setCurrent(latest); setVersion(latest.version); setBaseline(currentServer); setForm(merged);
    setConflict(false); setLatest(null);
    setNotice(__("Updated baseline version v{{value1}}; edited input is retained.", { value1: latest.version }));
    recoveredRef.current = null;
  };

  const discardAndBack = () => {
    if (dirty && !window.confirm(__("Discard unsaved taxonomy input and return to list?"))) return;
    if (scope) termDrafts.delete(scope);
    recoveredRef.current = null;
    if (embedded && onCancelRef.current) { onCancelRef.current(); return; }
    onNavigateHash(listHash);
  };

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!user || !csrfToken || savingRef.current || busy || loading || notFound || conflict ||
      (mode === 'new' && (providerUnavailable || !canCreate))) return;
    const name = form.name.trim();
    const slug = form.slug.trim();
    if (!name || [...name].length > 120 || /[\u0000-\u001f\u007f]/.test(name) || !/^[a-z][a-z0-9-]{0,63}$/.test(slug)) {
      setError(__("Invalid name or slug (name 1–120 characters, kebab-case slug up to 64)."));
      return;
    }
    const parentId = hierarchical && form.parentId ? Number(form.parentId) : null;
    if (canEditCore && parentId !== null && (!Number.isSafeInteger(parentId) || !parentOptions.some(t => t.id === parentId))) {
      setError(__("Invalid parent term or hierarchy exceeds 8 levels."));
      return;
    }
    savingRef.current = true; setBusy(true); setError(''); setNotice('');
    (async () => {
      try {
        let saved: TaxonomyTerm;
        if (mode === 'new') {
          saved = await createTaxonomyTerm(buildCreateTermPayload({
            kind, name, slug, hierarchical, parentId, form, isAdmin: user.role === 'admin',
            hasMultipleLocales: localeState.hasMultipleLocales, selectedLocale, editorConfig, values, seo,
          }), csrfToken);
        } else {
          if (!current || version === null) return;
          const changes = buildPatchTermChanges({
            isAdmin: user.role === 'admin', hierarchical, name, slug, parentId, form, baseline,
            editorConfig, values, valuesBaseline, seo, seoBaseline,
          });
          if (!Object.keys(changes).length) { setNotice(__("No changes to save.")); return; }
          saved = await updateTaxonomyTerm(current.id, version, changes, csrfToken);
        }
        if (!mountedRef.current) return;
        if (embedded && mode === 'new') {
          const reset = blank();
          setCurrent(null); setVersion(null); setBaseline(reset); setForm(reset); setManualSlug(false);
          const freshDefaults = cloneValues(editorConfig?.values ?? {});
          setValues(cloneValues(freshDefaults)); setValuesBaseline(cloneValues(freshDefaults));
          setSeo(defaultTermSeo()); setSeoBaseline(defaultTermSeo());
          if (definition?.hierarchical) {
            void listAllTaxonomyTerms(kind).then(t => { if (mountedRef.current) setAllTerms(t); }).catch(() => {});
          }
        } else {
          const next = fromTerm(saved);
          setCurrent(saved); setVersion(saved.version); setBaseline(next); setForm(next);
          const nextValues = saved.values ? cloneValues(saved.values) : cloneValues(values);
          setValues(cloneValues(nextValues)); setValuesBaseline(cloneValues(nextValues));
          const nextSeo = saved.seo ? { ...saved.seo } : { ...seo };
          setSeo({ ...nextSeo }); setSeoBaseline({ ...nextSeo });
          if (saved.schema_token && editorConfig) setEditorConfig({ ...editorConfig, schema_token: saved.schema_token });
        }
        setConflict(false); setLatest(null);
        if (scope) termDrafts.delete(scope);
        recoveredRef.current = null; reportDirtyRef.current(false);
        setNotice(__("Saved term {{value1}} #{{value2}} (v{{value3}}).", { value1: definition?.label ?? kind, value2: saved.id, value3: saved.version }));
        onSavedRef.current?.(saved);
        if (!embedded && mode === 'new') onNavigateHash(`${listHash}/${saved.id}/edit`);
      } catch (cause) {
        if (!mountedRef.current) return;
        if (cause instanceof ApiClientError && (cause.status === 412 || cause.status === 409)) {
          setConflict(true);
          setError(cause.status === 412 ? __("Version changed (412). Unsaved input is retained.") : __("Provider configuration changed (409). Unsaved input is retained."));
        } else setError(cause instanceof Error ? cause.message : __("Could not save term."));
        onErrorRef.current(cause, __("Could not save term."));
      } finally { savingRef.current = false; if (mountedRef.current) setBusy(false); }
    })();
  };

  if (!user) return null;
  if (notFound) {
    return (
      <section data-testid="term-not-found" className="space-y-3">
        <h2 className="text-xl font-bold text-zinc-950">{__("404 Term not found")}{' '}{kind}</h2>
        <p className="text-sm text-zinc-600">{__("ID does not exist or belongs to another taxonomy.")}</p>
        {recoveredRef.current && (
          <AdminAlert tone="warning" className="space-y-2">
            <p role="status">{__("Your previous draft is retained for copying, but the save target is no longer available.")}</p>
            <pre className="max-w-full overflow-auto whitespace-pre-wrap break-words rounded bg-white/80 p-2 font-mono text-xs">{JSON.stringify({ form, values, seo }, null, 2)}</pre>
          </AdminAlert>
        )}
        <AdminButton variant="secondary" size="md" onClick={discardAndBack}>{__("Discard input / Return to list")}</AdminButton>
      </section>
    );
  }
  if (loading) return <p role="status" className="text-sm text-zinc-600">{__("Loading taxonomy editor...")}</p>;

  return (
    <section
      className={embedded ? 'w-full min-w-0 space-y-4' : 'mx-auto w-full min-w-0 max-w-4xl space-y-5'}
      data-testid="term-edit-page" data-term-kind={kind} data-term-id={current?.id ?? ''}
    >
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-od-accent">{__("Content /")}{' '}{definition?.label ?? kind}</p>
          <h2 className="mt-1 text-lg font-bold tracking-tight text-zinc-950">
            {mode === 'new' ? __("Add {{value1}} term", { value1: definition?.label ?? kind }) : __("Edit {{value1}} term #{{value2}}", { value1: definition?.label ?? kind, value2: String(current?.id) })}
          </h2>
          {current && <p className="mt-0.5 text-xs text-zinc-500">ID #{current.id} {__("· Version v")}{version} · {dirty ? __("Unsaved") : __("Synced")}</p>}
        </div>
        {(!embedded || mode === 'edit') && (
          <AdminButton variant="secondary" size="sm" disabled={busy} onClick={discardAndBack}>
            {embedded ? __("Back to create new") : __("Cancel / Return to list")}
          </AdminButton>
        )}
      </header>

      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {providerUnavailable && (
        <AdminAlert tone="warning">{__("This taxonomy is inactive or its declaration was withdrawn. Existing terms can be retained/edited; new terms cannot be created.")}</AdminAlert>
      )}

      <div data-extension-slot="term-before-core" aria-hidden="true" />
      <form onSubmit={submit} className="min-w-0 space-y-4 rounded-xl border border-zinc-200 bg-white p-5" aria-label={__("Taxonomy edit form")}>
        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-name">
          <span>{__("Name *")}</span>
          <input id="term-name" className={fieldClass} maxLength={120} required disabled={busy || !canEditCore}
            value={form.name} onChange={e => setForm(prev => ({ ...prev, name: e.target.value, slug: manualSlug ? prev.slug : slugFromName(e.target.value) }))} />
        </label>

        <div className="grid gap-3 sm:grid-cols-2">
          <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-slug">
            <span>{__("Slug *")}</span>
            <input id="term-slug" className={fieldClass} maxLength={64} required pattern="[a-z](?:[a-z0-9]|-)*"
              disabled={busy || !canEditCore} value={form.slug}
              onChange={e => { setManualSlug(true); setForm(prev => ({ ...prev, slug: e.target.value })); }} />
          </label>
          <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-sort-order">
            <span>{__("Sort order")}</span>
            <input id="term-sort-order" type="number" className={fieldClass} disabled={busy || !canEditCore}
              value={sortOrderText === '' ? '' : String(form.sortOrder) === sortOrderText ? sortOrderText : String(form.sortOrder)}
              onChange={e => {
                const raw = e.target.value;
                setSortOrderText(raw);
                if (raw.trim() !== '' && Number.isFinite(Number(raw))) {
                  setForm(prev => ({ ...prev, sortOrder: Math.trunc(Number(raw)) }));
                }
              }}
              onBlur={() => setSortOrderText(String(form.sortOrder))} />
          </label>
        </div>

        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-description">
          <span>{__("Description")}</span>
          <textarea id="term-description" rows={2} maxLength={400} disabled={busy || !canEditCore}
            className="w-full rounded-md border border-zinc-300 bg-white px-3 py-2 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50"
            value={form.description} onChange={e => setForm(prev => ({ ...prev, description: e.target.value }))} />
        </label>

        {localeState.hasMultipleLocales && (
          <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-locale">
            <span>{__("Language")}</span>
            <select id="term-locale" className={selectClass} value={activeTermLocale} disabled={mode !== 'new' || busy || !canEditCore}
              onChange={e => { setSelectedLocale(e.target.value); setForm(prev => ({ ...prev, parentId: '' })); }} data-testid="term-locale-select">
              {localeState.allLocales.map(loc => <option key={loc.code} value={loc.code}>{loc.label} ({loc.code})</option>)}
            </select>
          </label>
        )}

        {hierarchical && (
          <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-parent">
            <span>{__("Parent term")}</span>
            <select id="term-parent" className={selectClass} value={form.parentId} disabled={busy || !canEditCore}
              onChange={e => setForm(prev => ({ ...prev, parentId: e.target.value }))}>
              <option value="">{__("None (root term)")}</option>
              {parentOptions.map(term => <option key={term.id} value={term.id}>{term.name} (#{term.id})</option>)}
            </select>
            <span className="text-xs font-normal text-zinc-500">{__("Cannot select a descendant or move a branch beyond 8 levels.")}</span>
          </label>
        )}

        <div className="space-y-2" data-testid="term-featured-image-field">
          <div className="text-sm font-medium text-zinc-800">{__("Featured image")}</div>
          {form.featuredImageId ? (
            <div className="flex flex-wrap items-center gap-3 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3">
              <img src={mediaThumbnailUrl(form.featuredImageId)} alt={__("Featured image")}
                className="h-20 w-20 rounded-md border border-zinc-200 bg-white object-cover" />
              <div className="flex flex-wrap items-center gap-2">
                <AdminButton type="button" variant="secondary" size="sm" disabled={busy || !canEditCore}
                  onClick={() => setPickerOpen(true)}>{__("Change image")}</AdminButton>
                <AdminButton type="button" variant="secondary" size="sm" disabled={busy || !canEditCore}
                  onClick={() => setForm(prev => ({ ...prev, featuredImageId: '' }))}>{_x("Remove image", "taxonomy.remove_featured_image")}</AdminButton>
              </div>
            </div>
          ) : (
            <AdminButton type="button" variant="secondary" size="sm" disabled={busy || !canEditCore}
              onClick={() => setPickerOpen(true)}>{__("Choose or upload image")}</AdminButton>
          )}
        </div>

        {editorConfig && (
          <TermMetaFieldGroups config={editorConfig} values={values} savedValues={valuesBaseline}
            disabled={busy} csrfToken={csrfToken} locale={activeTermLocale || null}
            siteTimezone={siteTimezone} onChange={setValues} onApiError={onApiError} />
        )}

        <TermSeoFields value={seo} disabled={busy} csrfToken={csrfToken}
          editorRestricted={user.role !== 'admin'} onChange={setSeo} onApiError={onApiError} />

        {conflict && (
          <AdminAlert tone="warning" className="space-y-2">
            <p>{__("Server data changed (412/409). Your edits are retained.")}</p>
            {!latest ? (
              <AdminButton type="button" variant="secondary" size="sm" disabled={busy} onClick={() => void inspectConflict()}>
                {__("Load latest version to compare")}
              </AdminButton>
            ) : (
              <>
                <p className="text-xs">
                  {__("Server version: v{{version}} · {{name}} · /{{slug}}", { version: latest.version, name: latest.name, slug: latest.slug })}
                </p>
                <AdminButton type="button" variant="secondary" size="sm" disabled={busy} onClick={continueWithLatest}>
                  {__("Continue with v{{version}} (retain edited fields)", { version: latest.version })}
                </AdminButton>
              </>
            )}
          </AdminAlert>
        )}

        <div className="flex flex-wrap gap-2.5 pt-1">
          <AdminButton type="submit" variant="primary" size="md"
            disabled={busy || conflict || !csrfToken || (mode === 'new' && (!canCreate || providerUnavailable))}>
            {busy ? __("Saving...") : mode === 'new' ? __("Create new") : __("Save changes")}
          </AdminButton>
          {(!embedded || mode === 'edit' || dirty) && (
            <AdminButton type="button" variant="secondary" size="md" disabled={busy} onClick={discardAndBack}>
              {__("Cancel edits")}
            </AdminButton>
          )}
        </div>
      </form>
      {pickerOpen && (
        <MediaPicker mode="image" acceptKind="image" selectedIds={form.featuredImageId ? [form.featuredImageId] : []}
          csrfToken={csrfToken} maxItems={1}
          onApply={ids => { setForm(prev => ({ ...prev, featuredImageId: ids[0] ?? '' })); setPickerOpen(false); }}
          onClose={() => setPickerOpen(false)} onError={onApiError} />
      )}
    </section>
  );
}
