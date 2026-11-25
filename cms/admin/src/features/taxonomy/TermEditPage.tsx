import { getEffectiveBcp47, tStatic, useI18n } from '../../i18n/index.tsx';
import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { mediaThumbnailUrl } from '../../api/media.ts';
import { createTaxonomyTerm, getTaxonomyTerm, getEffectiveTermConfig, getTaxonomyDefinition,
  listAllTaxonomyTerms, updateTaxonomyTerm,
  type TaxonomyDefinition, type TaxonomyKind, type TaxonomyTerm, type TaxonomyTermChanges } from '../../api/taxonomy.ts';
import type { AuthUser } from '../../types.ts';
import {
  EMPTY_ADMIN_LOCALE_STATE,
  type AdminLocaleState,
} from '../../api/locales.ts';
import { MediaPicker } from '../media/MediaPicker.tsx';
import { AdminTermPanels, type EffectiveTermConfig, type TermExtensionValues } from '../extensions/AdminContributionRenderer.tsx';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';

interface TermEditProps {
  kind: TaxonomyKind;
  mode: 'new' | 'edit';
  cptTypeKey?: string;
  termId: number | null;
  user: AuthUser | null;
  /** Site instance from App, required to safely recover a draft after a 401. */
  siteInstanceId?: string;
  csrfToken: string | null;
  localeState?: AdminLocaleState;
  embedded?: boolean;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
  onSaved?: (saved: TaxonomyTerm) => void;
  onCancelEdit?: () => void;
}

interface FormValues { name: string; slug: string; parentId: string; featuredImageId: string }
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

const blank = (): FormValues => ({ name: '', slug: '', parentId: '', featuredImageId: '' });
const fromTerm = (term: TaxonomyTerm): FormValues => ({
  name: term.name,
  slug: term.slug,
  parentId: term.parent_id === null ? '' : String(term.parent_id),
  featuredImageId: term.featured_image_media_id ?? '',
});
const fieldClass = 'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';
const selectClass = 'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';

function slugFromName(name: string): string {
  return name.replace(/[\u0111\u0110]/g, 'd').normalize('NFD').replace(/[\u0300-\u036f]/g, '')
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
  }).sort((a,b) => a.name.localeCompare(b.name, getEffectiveBcp47()) || a.id-b.id);
}

/** A full-route editor; extension mount slots intentionally contain no fake fields. */
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
            setError(tStatic("feature.taxonomy.termeditpage.the_term_or_schema_changed_while_signed_out_your"));
          }
        }
      } catch (cause) {
        if (signal.aborted) return;
        if (cause instanceof ApiClientError && cause.status === 404) setNotFound(true);
        else { setError(cause instanceof Error ? cause.message : tStatic("feature.taxonomy.termeditpage.could_not_load_term"));
          onErrorRef.current(cause, tStatic("feature.taxonomy.termeditpage.could_not_load_term")); }
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
      else { setError(cause instanceof Error ? cause.message : tStatic("feature.taxonomy.termeditpage.could_not_read_latest_version"));
        onErrorRef.current(cause, tStatic("feature.taxonomy.termeditpage.could_not_read_latest_version")); }
    } finally { setBusy(false); }
  };

  const continueWithLatest = () => {
    if (!latest || latest.kind !== kind) return;
    const currentServer = fromTerm(latest);
    const changed = (Object.keys(form) as Array<keyof FormValues>)
      .filter(field => form[field] !== baseline[field]);
    const overlaps = changed.filter(field => currentServer[field] !== baseline[field]);
    if (overlaps.length && !window.confirm(
      tStatic("feature.taxonomy.termeditpage.the_server_also_changed_value_keep_current_input_on", { value1: overlaps.join(', ') }),
    )) return;
    const merged = { ...currentServer };
    for (const field of changed) merged[field] = form[field];
    setBaseline(currentServer); setForm(merged);
    setVersion(latest.version); setConflict(false); setLatest(null); setError('');
    setNotice(tStatic("feature.taxonomy.termeditpage.reviewed_vvalue_edited_fields_are_retained_review_and_save", { value1: latest.version }));
    recoveredRef.current = null;
  };

  const discardAndBack = () => {
    if (dirty && !window.confirm(tStatic("feature.taxonomy.termeditpage.discard_unsaved_taxonomy_input_and_return_to_list"))) return;
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
      setError(tStatic("feature.taxonomy.termeditpage.invalid_name_or_slug_name_1_120_characters_kebab"));
      return;
    }
    const parentId = hierarchical && form.parentId ? Number(form.parentId) : null;
    if (canEditCore && parentId !== null &&
      (!Number.isSafeInteger(parentId) || !parentOptions.some(t => t.id === parentId))) {
      setError(tStatic("feature.taxonomy.termeditpage.invalid_parent_term_or_hierarchy_exceeds_8_levels")); return;
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
          if (!Object.keys(changes).length) { setNotice(tStatic("feature.taxonomy.taxonomydefinitionspage.no_changes_to_save")); return; }
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
          setNotice(tStatic("feature.taxonomy.termeditpage.saved_term_value_value_vvalue", { value1: definition?.label ?? kind, value2: saved.id, value3: saved.version }));
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
          setNotice(tStatic("feature.taxonomy.termeditpage.saved_term_value_value_vvalue", { value1: definition?.label ?? kind, value2: saved.id, value3: saved.version }));
          onSavedRef.current?.(saved);
          if (!embedded && mode === 'new') onNavigateHash(`${listHash}/${saved.id}/edit`);
        }
      } catch (cause) {
        if (!mountedRef.current) return;
        if (cause instanceof ApiClientError && (cause.status === 412 || cause.status === 409)) {
          setConflict(true);
          setError(cause.status === 412 ?
            tStatic("feature.taxonomy.termeditpage.version_changed_412_unsaved_input_is_retained") :
            tStatic("feature.taxonomy.termeditpage.provider_configuration_changed_409_unsaved_input_is_retained"));
        } else { setError(cause instanceof Error ? cause.message : tStatic("feature.taxonomy.termeditpage.could_not_save_term")); }
        onErrorRef.current(cause, tStatic("feature.taxonomy.termeditpage.could_not_save_term"));
      } finally { savingRef.current = false; setBusy(false); }
    })();
  };

  if (!user) return null;
  if (notFound) return (
    <section data-testid="term-not-found" className="space-y-3">
      <h2 className="text-xl font-bold text-zinc-950">{tStatic("feature.taxonomy.termeditpage.404_term_not_found")}{' '}{kind}</h2>
      <p className="text-sm text-zinc-600">{tStatic("feature.taxonomy.termeditpage.id_does_not_exist_or_belongs_to_another_taxonomy")}</p>
      {recoveredRef.current && <AdminAlert tone="warning" className="space-y-2">
        <p role="status">{tStatic("feature.taxonomy.termeditpage.your_previous_draft_is_retained_for_copying_but_the")}</p>
        <pre className="max-w-full overflow-auto whitespace-pre-wrap break-words rounded bg-white/80 p-2 font-mono text-xs">{JSON.stringify({ form, extensions }, null, 2)}</pre>
      </AdminAlert>}
      <AdminButton variant="secondary" size="md" onClick={discardAndBack}>{tStatic("feature.taxonomy.termeditpage.discard_input_return_to_list")}</AdminButton>
    </section>
  );
  if (loading) return <p role="status" className="text-sm text-zinc-600">{tStatic("feature.taxonomy.termeditpage.loading_taxonomy_editor")}</p>;

  return (
    <section
      className={embedded ? 'w-full min-w-0 space-y-4' : 'mx-auto w-full min-w-0 max-w-4xl space-y-5'}
      data-testid="term-edit-page"
      data-term-kind={kind}
      data-term-id={current?.id ?? ''}
    >
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-od-accent">{tStatic("feature.posts.postpageworkspace.content")}{' '}{definition?.label ?? kind}</p>
          <h2 className="mt-1 text-lg font-bold tracking-tight text-zinc-950">
            {mode === 'new' ? tStatic("feature.taxonomy.termeditpage.add_value_term", { value1: definition?.label ?? kind }) : tStatic("feature.taxonomy.termeditpage.edit_value_term_value", { value1: definition?.label ?? kind, value2: String(current?.id) })}
          </h2>
          {current && <p className="mt-0.5 text-xs text-zinc-500">ID #{current.id} {tStatic("feature.taxonomy.termeditpage.version_v")}{version} · {dirty ? tStatic("feature.taxonomy.termeditpage.unsaved") : tStatic("feature.taxonomy.termeditpage.synced")}</p>}
        </div>
        {(!embedded || mode === 'edit') && (
          <AdminButton variant="secondary" size="sm" disabled={busy} onClick={discardAndBack}>
            {embedded ? tStatic("taxonomy.back_to_create") : tStatic("feature.taxonomy.termeditpage.cancel_return_to_list")}
          </AdminButton>
        )}
      </header>

      {error && <AdminAlert tone="error">{error}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {providerUnavailable && <AdminAlert tone="warning">
        {tStatic("feature.taxonomy.termeditpage.this_taxonomy_is_inactive_or_its_declaration_was_withdrawn")}
      </AdminAlert>}

      <div data-extension-slot="term-before-core" aria-hidden="true" />
      <form onSubmit={submit} className="min-w-0 space-y-4 rounded-xl border border-zinc-200 bg-white p-5"
        aria-label={tStatic("feature.taxonomy.termeditpage.taxonomy_edit_form")}>
        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-name">
          <span>{tStatic("feature.taxonomy.termeditpage.name")}</span>
          <input id="term-name" className={fieldClass} maxLength={120} required disabled={busy || user.role !== 'admin'}
            value={form.name} onChange={event => setForm(previous => ({
              ...previous, name: event.target.value,
              slug: manualSlug ? previous.slug : slugFromName(event.target.value),
            }))} />
        </label>
        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-slug">
          <span>{tStatic("feature.common.ui_slug")}</span>
          <input id="term-slug" className={fieldClass} maxLength={64} required
            pattern="[a-z](?:[a-z0-9]|-)*" disabled={busy || user.role !== 'admin'} value={form.slug}
            onChange={event => { setManualSlug(true); setForm(previous => ({...previous, slug: event.target.value})); }} />
          <span className="text-xs font-normal text-zinc-500">{tStatic("feature.taxonomy.termeditpage.slug_unique_within_the_type_starts_with_a_z")}</span>
        </label>
        {localeState.hasMultipleLocales && (
          <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-locale">
            <span>{tStatic("feature.content.entrylistpage.language")}</span>
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
                ? tStatic("feature.taxonomy.termeditpage.term_language_is_fixed_after_creation")
                : tStatic("feature.taxonomy.termeditpage.language_cannot_be_changed_after_creation")}
            </span>
          </label>
        )}
        {hierarchical && <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800" htmlFor="term-parent">
          <span>{tStatic("feature.common.parent_term")}</span>
          <select id="term-parent" className={selectClass} value={form.parentId} disabled={busy || user.role !== 'admin'}
            onChange={event => setForm(previous => ({...previous, parentId: event.target.value}))}>
            <option value="">{tStatic("feature.taxonomy.termeditpage.none_root_term")}</option>
            {parentOptions.map(term => <option key={term.id} value={term.id}>{term.name} (#{term.id})</option>)}
            {form.parentId && !parentOptions.some(term => String(term.id) === form.parentId) &&
              <option value={form.parentId} disabled>{tStatic("feature.taxonomy.termeditpage.parent_no_longer_valid")}{form.parentId})</option>}
          </select>
          <span className="text-xs font-normal text-zinc-500">{tStatic("feature.taxonomy.termeditpage.cannot_select_a_descendant_or_move_a_branch_beyond")}</span>
        </label>}

        <div className="space-y-2" data-testid="term-featured-image-field">
          <div className="text-sm font-medium text-zinc-800">{tStatic("taxonomy.featured_image")}</div>
          {form.featuredImageId ? (
            <div className="flex flex-wrap items-center gap-3 rounded-lg border border-zinc-200 bg-zinc-50/50 p-3">
              <img
                src={mediaThumbnailUrl(form.featuredImageId)}
                alt={tStatic("taxonomy.featured_image")}
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
                  {tStatic("taxonomy.change_featured_image")}
                </AdminButton>
                <AdminButton
                  variant="danger"
                  size="sm"
                  disabled={busy || user.role !== 'admin'}
                  data-testid="term-remove-featured-image"
                  onClick={() => setForm(previous => ({ ...previous, featuredImageId: '' }))}
                >
                  {tStatic("taxonomy.remove_featured_image")}
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
              {tStatic("taxonomy.choose_featured_image")}
            </AdminButton>
          )}
          <p className="text-xs font-normal text-zinc-500">{tStatic("taxonomy.featured_image_help")}</p>
        </div>

        {mode === 'edit' && extensionConfig && <AdminTermPanels config={extensionConfig}
          values={extensions} disabled={busy}
          onChange={(panelKey, key, value) => setExtensions(previous => ({
            ...previous,
            [panelKey]: { ...(previous[panelKey] ?? {}), [key]: value },
          }))} />}

        {conflict && <AdminAlert tone="warning" className="space-y-2">
          <p role="status">{tStatic("feature.taxonomy.termeditpage.server_data_changed_your_input_is_retained")}</p>
          {!latest ? <AdminButton variant="secondary" size="sm" disabled={busy}
            onClick={() => void inspectConflict()}>{tStatic("feature.taxonomy.termeditpage.load_latest_version_to_compare")}</AdminButton> : <>
            <p className="text-xs">{tStatic("feature.taxonomy.termeditpage.server_version_v")}{latest.version} · {latest.name} · /{latest.slug}</p>
            <AdminButton variant="secondary" size="sm" disabled={busy} onClick={continueWithLatest}>
              {tStatic("feature.taxonomy.termeditpage.continue_with_v")}{latest.version} {tStatic("feature.taxonomy.termeditpage.retain_edited_fields")}
            </AdminButton>
          </>}
        </AdminAlert>}
        <div className="flex flex-wrap gap-2.5 pt-1">
          <AdminButton type="submit" variant="primary" size="md"
            disabled={busy || conflict || !csrfToken || (mode === 'new' && (user.role !== 'admin' || providerUnavailable))}>
            {busy ? tStatic("feature.users.userprofilepanel.saving") : mode === 'new' ? tStatic("feature.editor.panelregionrenderer.create_new") : tStatic("feature.users.userspanel.save_changes")}
          </AdminButton>
          {(!embedded || mode === 'edit' || dirty) && (
            <AdminButton variant="secondary" size="md" disabled={busy} onClick={discardAndBack}>
              {tStatic("feature.taxonomy.termeditpage.cancel_edits")}
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
