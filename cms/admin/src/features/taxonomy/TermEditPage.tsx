import { __, _x, useI18n } from '../../i18n/index.tsx';
import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { mediaThumbnailUrl } from '../../api/media.ts';
import { createTaxonomyTerm, getTaxonomyTerm, getEffectiveTermConfig, getTaxonomyDefinition,
  listAllTaxonomyTerms, updateTaxonomyTerm,
  type TaxonomyDefinition, type TaxonomyTerm, type TaxonomyTermChanges } from '../../api/taxonomy.ts';
import {
  EMPTY_ADMIN_LOCALE_STATE,
} from '../../api/locales.ts';
import { MediaPicker } from '../media/MediaPicker.tsx';
import { AdminTermPanels, type EffectiveTermConfig, type TermExtensionValues } from '../extensions/AdminContributionRenderer.tsx';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { isUiOnlyFieldType } from '../../fields/helpers.ts';
import { useSiteTimezone } from '../../fields/useSiteTimezone.ts';

import {
  type TermEditProps, type FormValues, type TermDraft,
  termDrafts, termScope, cloneExtensions, blank, fromTerm,
  fieldClass, selectClass, slugFromName, allowedParents,
} from './termEditHelpers.ts';

export function TermEditPage({kind, mode, cptTypeKey, termId, user, siteInstanceId, csrfToken,
  localeState = EMPTY_ADMIN_LOCALE_STATE, embedded = false, onApiError, onDirtyChange, onNavigateHash,
  onSaved, onCancelEdit}: TermEditProps) {
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
  const [pickerOpen, setPickerOpen] = useState(false);
  const savingRef = useRef(false);
  const onErrorRef = useRef(onApiError);
  onErrorRef.current = onApiError;
  const onSavedRef = useRef(onSaved);
  onSavedRef.current = onSaved;
  const reportDirtyRef = useRef(onDirtyChange);
  reportDirtyRef.current = onDirtyChange;
  const listHash = cptTypeKey
    ? `#/cpt/${encodeURIComponent(cptTypeKey)}/taxonomy/${encodeURIComponent(kind)}`
    : kind === 'category' ? '#/posts/categories' : kind === 'tag' ? '#/posts/tags'
      : `#/taxonomy/${encodeURIComponent(kind)}/terms`;
  const routeHash = mode === 'new' ? `${listHash}/new` : `${listHash}/${termId}/edit`;
  const hierarchical = Boolean(definition?.hierarchical);
  const providerUnavailable = Boolean(definition &&
    (definition.provider_status !== 'active' || definition.declaration_state !== 'declared'));
  const defaultLocaleCode = localeState.defaultLocale?.code ?? '';
  const [selectedLocale, setSelectedLocale] = useState<string>(defaultLocaleCode);
  useEffect(() => {
    if (!selectedLocale && defaultLocaleCode) {
      setSelectedLocale(defaultLocaleCode);
    }
  }, [selectedLocale, defaultLocaleCode]);
  const activeTermLocale = mode === 'edit' ? (current?.locale ?? defaultLocaleCode) : selectedLocale;
  const siteTimezone = useSiteTimezone();
  const dirty = form.name !== baseline.name || form.slug !== baseline.slug ||
    form.parentId !== baseline.parentId || form.featuredImageId !== baseline.featuredImageId ||
    JSON.stringify(extensions) !== JSON.stringify(extensionBaseline);
  const parentOptions = useMemo(() => {
    const scoped = activeTermLocale
      ? allTerms.filter((t) => !t.locale || t.locale === activeTermLocale)
      : allTerms;
    return allowedParents(scoped, current?.id ?? null);
  }, [allTerms, current?.id, activeTermLocale]);

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
            setError(__("The term or schema changed while signed out. Your draft is retained; review the latest version."));
          }
        }
      } catch (cause) {
        if (signal.aborted) return;
        if (cause instanceof ApiClientError && cause.status === 404) setNotFound(true);
        else { setError(cause instanceof Error ? cause.message : __("Could not load term."));
          onErrorRef.current(cause, __("Could not load term.")); }
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
        setExtensions(prev => Object.fromEntries(config.panels.map(panel => {
          const readOnlySet = new Set(panel.read_only_keys ?? []);
          return [
            panel.key,
            Object.fromEntries(
              panel.fields
                .filter(field => !isUiOnlyFieldType(field.type) && !readOnlySet.has(field.key))
                .map(field => [
                  field.key,
                  prev[panel.key]?.[field.key] !== extensionBaseline[panel.key]?.[field.key]
                    ? (prev[panel.key]?.[field.key] ?? null)
                    : (config.extensions[panel.key]?.[field.key] ?? null),
                ]),
            ),
          ];
        })) as TermExtensionValues);
        setExtensionBaseline(config.extensions);
      }
      if (fresh.kind !== kind) setNotFound(true);
    } catch (cause) {
      if (!mountedRef.current) return;
      if (cause instanceof ApiClientError && cause.status === 404) setNotFound(true);
      else { setError(cause instanceof Error ? cause.message : __("Could not read latest version."));
        onErrorRef.current(cause, __("Could not read latest version.")); }
    } finally { setBusy(false); }
  };

  const continueWithLatest = () => {
    if (!latest || latest.kind !== kind) return;
    const currentServer = fromTerm(latest);
    const changed = (Object.keys(form) as Array<keyof FormValues>)
      .filter(field => form[field] !== baseline[field]);
    const overlaps = changed.filter(field => currentServer[field] !== baseline[field]);
    if (overlaps.length && !window.confirm(
      __("The server also changed {{value1}}. Keep current input on the latest version?", { value1: overlaps.join(', ') }),
    )) return;
    const merged = { ...currentServer };
    for (const field of changed) merged[field] = form[field];
    setBaseline(currentServer); setForm(merged);
    setVersion(latest.version); setConflict(false); setLatest(null); setError('');
    setNotice(__("Reviewed v{{value1}}. Edited fields are retained; review and save.", { value1: latest.version }));
    recoveredRef.current = null;
  };

  const discardAndBack = () => {
    if (dirty && !window.confirm(__("Discard unsaved taxonomy input and return to list?"))) return;
    if (scope) termDrafts.delete(scope);
    recoveredRef.current = null;
    setForm(baseline); setExtensions(extensionBaseline);
    setError(''); setNotice('');
    setManualSlug(mode === 'edit');
    reportDirtyRef.current(false);
    if (embedded) {
      if (mode === 'edit') {
        if (onCancelEdit) onCancelEdit();
        else onNavigateHash(listHash);
      }
      return;
    }
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
      setError(__("Invalid name or slug (name 1–120 characters, kebab-case slug up to 64)."));
      return;
    }
    const parentId = hierarchical && form.parentId ? Number(form.parentId) : null;
    if (canEditCore && parentId !== null &&
      (!Number.isSafeInteger(parentId) || !parentOptions.some(t => t.id === parentId))) {
      setError(__("Invalid parent term or hierarchy exceeds 8 levels.")); return;
    }
    savingRef.current = true;
    setBusy(true); setError(''); setNotice('');
    (async () => {
      try {
        let saved: TaxonomyTerm;
        if (mode === 'new') {
          saved = await createTaxonomyTerm({
            kind,
            name,
            slug,
            ...(hierarchical ? { parent_id: parentId } : {}),
            ...(localeState.hasMultipleLocales && selectedLocale ? { locale: selectedLocale } : {}),
            ...(form.featuredImageId ? { featured_image_media_id: form.featuredImageId } : {}),
          }, csrfToken);
        } else {
          if (!current || version === null) return;
          const changes: TaxonomyTermChanges = {};
          if (canEditCore && name !== baseline.name) changes.name = name;
          if (canEditCore && slug !== baseline.slug) changes.slug = slug;
          const oldParent = baseline.parentId ? Number(baseline.parentId) : null;
          if (canEditCore && hierarchical && parentId !== oldParent) changes.parent_id = parentId;
          if (canEditCore && form.featuredImageId !== baseline.featuredImageId) {
            changes.featured_image_media_id = form.featuredImageId || null;
          }
          if (extensionConfig) {
            const altered: TermExtensionValues = {};
            for (const panel of extensionConfig.panels) {
              const readOnlySet = new Set(panel.read_only_keys ?? []);
              const patch: TermExtensionValues[string] = {};
              for (const field of panel.fields) {
                if (isUiOnlyFieldType(field.type) || readOnlySet.has(field.key)) continue;
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
          if (!Object.keys(changes).length) { setNotice(__("No changes to save.")); return; }
          saved = await updateTaxonomyTerm(current.id, version, changes, csrfToken);
        }
        if (!mountedRef.current) return;
        if (embedded && mode === 'new') {
          const reset = blank();
          setCurrent(null); setVersion(null); setBaseline(reset); setForm(reset);
          setManualSlug(false);
          setExtensionBaseline({}); setExtensions({});
          if (definition?.hierarchical) {
            void listAllTaxonomyTerms(kind).then(terms => {
              if (mountedRef.current) setAllTerms(terms);
            }).catch(() => {});
          }
          setConflict(false); setLatest(null);
          if (scope) termDrafts.delete(scope);
          recoveredRef.current = null;
          reportDirtyRef.current(false);
          setNotice(__("Saved term {{value1}} #{{value2}} (v{{value3}}).", { value1: definition?.label ?? kind, value2: saved.id, value3: saved.version }));
          onSavedRef.current?.(saved);
        } else {
          const next = fromTerm(saved);
          setCurrent(saved); setVersion(saved.version); setBaseline(next); setForm(next);
          setExtensionBaseline(extensions);
          if (saved.schema_token && extensionConfig)
            setExtensionConfig({...extensionConfig, schema_token: saved.schema_token});
          setConflict(false); setLatest(null);
          if (scope) termDrafts.delete(scope);
          recoveredRef.current = null;
          reportDirtyRef.current(false);
          setNotice(__("Saved term {{value1}} #{{value2}} (v{{value3}}).", { value1: definition?.label ?? kind, value2: saved.id, value3: saved.version }));
          onSavedRef.current?.(saved);
          if (!embedded && mode === 'new') onNavigateHash(`${listHash}/${saved.id}/edit`);
        }
      } catch (cause) {
        if (!mountedRef.current) return;
        if (cause instanceof ApiClientError && (cause.status === 412 || cause.status === 409)) {
          setConflict(true);
          setError(cause.status === 412 ?
            __("Version changed (412). Unsaved input is retained.") :
            __("Provider configuration changed (409). Unsaved input is retained."));
        } else { setError(cause instanceof Error ? cause.message : __("Could not save term.")); }
        onErrorRef.current(cause, __("Could not save term."));
      } finally { savingRef.current = false; setBusy(false); }
    })();
  };

  if (!user) return null;
  if (notFound) return (
    <section data-testid="term-not-found" className="space-y-3">
      <h2 className="text-xl font-bold text-zinc-950">{__("404 Term not found")}{' '}{kind}</h2>
      <p className="text-sm text-zinc-600">{__("ID does not exist or belongs to another taxonomy.")}</p>
      {recoveredRef.current && <AdminAlert tone="warning" className="space-y-2">
        <p role="status">{__("Your previous draft is retained for copying, but the save target is no longer available.")}</p>
        <pre className="max-w-full overflow-auto whitespace-pre-wrap break-words rounded bg-white/80 p-2 font-mono text-xs">{JSON.stringify({ form, extensions }, null, 2)}</pre>
      </AdminAlert>}
      <AdminButton variant="secondary" size="md" onClick={discardAndBack}>{__("Discard input / Return to list")}</AdminButton>
    </section>
  );
  if (loading) return <p role="status" className="text-sm text-zinc-600">{__("Loading taxonomy editor...")}</p>;

  return (
    <section
      className={embedded ? 'w-full min-w-0 space-y-4' : 'mx-auto w-full min-w-0 max-w-4xl space-y-5'}
      data-testid="term-edit-page"
      data-term-kind={kind}
      data-term-id={current?.id ?? ''}
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
      {providerUnavailable && <AdminAlert tone="warning">
        {__("This taxonomy is inactive or its declaration was withdrawn. Existing terms can be retained/edited; new terms cannot be created.")}
      </AdminAlert>}

      <div data-extension-slot="term-before-core" aria-hidden="true" />
      <form onSubmit={submit} className="min-w-0 space-y-4 rounded-xl border border-zinc-200 bg-white p-5"
        aria-label={__("Taxonomy edit form")}>
        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-name">
          <span>{__("Name *")}</span>
          <input id="term-name" className={fieldClass} maxLength={120} required disabled={busy || user.role !== 'admin'}
            value={form.name} onChange={event => setForm(previous => ({
              ...previous, name: event.target.value,
              slug: manualSlug ? previous.slug : slugFromName(event.target.value),
            }))} />
        </label>
        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-slug">
          <span>{__("Slug *")}</span>
          <input id="term-slug" className={fieldClass} maxLength={64} required
            pattern="[a-z](?:[a-z0-9]|-)*" disabled={busy || user.role !== 'admin'} value={form.slug}
            onChange={event => { setManualSlug(true); setForm(previous => ({...previous, slug: event.target.value})); }} />
          <span className="text-xs font-normal text-zinc-500">{__("Slug unique within the type, starts with a-z, maximum 64 characters.")}</span>
        </label>
        {localeState.hasMultipleLocales && (
          <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-locale">
            <span>{__("Language")}</span>
            <select
              id="term-locale"
              className={selectClass}
              value={activeTermLocale}
              disabled={mode !== 'new' || busy || user.role !== 'admin'}
              onChange={(event) => {
                setSelectedLocale(event.target.value);
                setForm((previous) => ({ ...previous, parentId: '' }));
              }}
              data-testid="term-locale-select"
            >
              {localeState.allLocales.map((loc) => (
                <option key={loc.code} value={loc.code}>
                  {loc.label} ({loc.code})
                </option>
              ))}
            </select>
            <span className="text-xs font-normal text-zinc-500">
              {mode === 'new'
                ? __("Term language is fixed after creation.")
                : __("Language cannot be changed after creation.")}
            </span>
          </label>
        )}
        {hierarchical && <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-parent">
          <span>{__("Parent term")}</span>
          <select id="term-parent" className={selectClass} value={form.parentId} disabled={busy || user.role !== 'admin'}
            onChange={event => setForm(previous => ({...previous, parentId: event.target.value}))}>
            <option value="">{__("None (root term)")}</option>
            {parentOptions.map(term => <option key={term.id} value={term.id}>{term.name} (#{term.id})</option>)}
            {form.parentId && !parentOptions.some(term => String(term.id) === form.parentId) &&
              <option value={form.parentId} disabled>{__("Parent no longer valid (#")}{form.parentId})</option>}
          </select>
          <span className="text-xs font-normal text-zinc-500">{__("Cannot select a descendant or move a branch beyond 8 levels.")}</span>
        </label>}

        <div className="space-y-2" data-testid="term-featured-image-field">
          <div className="text-sm font-medium text-zinc-800">{__("Featured image")}</div>
          {form.featuredImageId ? (
            <div className="flex flex-wrap items-center gap-3 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3">
              <img
                src={mediaThumbnailUrl(form.featuredImageId)}
                alt={__("Featured image")}
                className="h-20 w-20 rounded-md border border-zinc-200 bg-white object-cover"
              />
              <div className="flex flex-wrap items-center gap-2">
                <AdminButton
                  variant="secondary"
                  size="sm"
                  disabled={busy || user.role !== 'admin'}
                  data-testid="term-change-featured-image"
                  onClick={() => setPickerOpen(true)}
                >
                  {__("Change image")}
                </AdminButton>
                <AdminButton
                  variant="danger"
                  size="sm"
                  disabled={busy || user.role !== 'admin'}
                  data-testid="term-remove-featured-image"
                  onClick={() => setForm(previous => ({ ...previous, featuredImageId: '' }))}
                >
                  {_x("Remove image", "taxonomy.remove_featured_image")}
                </AdminButton>
              </div>
            </div>
          ) : (
            <AdminButton
              variant="secondary"
              size="sm"
              disabled={busy || user.role !== 'admin'}
              data-testid="term-choose-featured-image"
              onClick={() => setPickerOpen(true)}
            >
              {__("Choose or upload image")}
            </AdminButton>
          )}
          <p className="text-xs font-normal text-zinc-500">{__("Optional thumbnail image for this term.")}</p>
        </div>

        {mode === 'edit' && extensionConfig && <AdminTermPanels config={extensionConfig}
          values={extensions} disabled={busy}
          csrfToken={csrfToken} locale={activeTermLocale || null} siteTimezone={siteTimezone}
          onApiError={onApiError}
          onChange={(panelKey, key, value) => setExtensions(previous => ({
            ...previous,
            [panelKey]: { ...(previous[panelKey] ?? {}), [key]: value },
          }))} />}

        {conflict && <AdminAlert tone="warning" className="space-y-2">
          <p role="status">{__("Server data changed. Your input is retained.")}</p>
          {!latest ? <AdminButton variant="secondary" size="sm" disabled={busy}
            onClick={() => void inspectConflict()}>{__("Load latest version to compare")}</AdminButton> : <>
            <p className="text-xs">{__("Server version: v")}{latest.version} · {latest.name} · /{latest.slug}</p>
            <AdminButton variant="secondary" size="sm" disabled={busy} onClick={continueWithLatest}>
              {__("Continue with v")}{latest.version} {__("(retain edited fields)")}
            </AdminButton>
          </>}
        </AdminAlert>}
        <div className="flex flex-wrap gap-2.5 pt-1">
          <AdminButton type="submit" variant="primary" size="md"
            disabled={busy || conflict || !csrfToken || (mode === 'new' && (user.role !== 'admin' || providerUnavailable))}>
            {busy ? __("Saving...") : mode === 'new' ? __("Create new") : __("Save changes")}
          </AdminButton>
          {(!embedded || mode === 'edit' || dirty) && (
            <AdminButton variant="secondary" size="md" disabled={busy} onClick={discardAndBack}>
              {__("Cancel edits")}
            </AdminButton>
          )}
        </div>
      </form>
      {pickerOpen && (
        <MediaPicker
          mode="image"
          acceptKind="image"
          selectedIds={form.featuredImageId ? [form.featuredImageId] : []}
          csrfToken={csrfToken}
          onApply={(ids) => {
            setForm(previous => ({ ...previous, featuredImageId: ids[0] ?? '' }));
            setPickerOpen(false);
          }}
          onClose={() => setPickerOpen(false)}
          onError={(err, fallback) => onErrorRef.current(err, fallback)}
        />
      )}
      <div data-extension-slot="term-after-core" aria-hidden="true" />
    </section>
  );
}
