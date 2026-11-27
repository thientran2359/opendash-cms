import { getEffectiveBcp47, __, _x, useI18n } from '../../i18n/index.tsx';
import { useEffect, useId, useMemo, useRef, useState } from 'react';
import {
  listAllTaxonomyTerms,
  listEffectiveTaxonomies,
  listTaxonomyDefinitions,
  type EntryTermFilters,
  type TaxonomyDefinition,
  type TaxonomyKind,
  type TaxonomyTerm,
} from '../../api/taxonomy.ts';

interface TermsProps {
  userId: number | null;
  /** Scope the picker to the server's current canonical eligibility for this CPT. */
  typeKey?: string;
  locale?: string | null;
  disabled?: boolean;
  onApiError: (error: unknown, fallback: string) => void;
}

interface TermPickerProps extends TermsProps {
  selectedIds: number[];
  onChange: (ids: number[]) => void;
  primaryCategoryTermId?: number | null;
  onPrimaryCategoryChange?: (id: number | null) => void;
  allowedKinds?: TaxonomyKind[];
  /** 'cards' renders each taxonomy as its own standalone sidebar card (WordPress-style). */
  variant?: 'default' | 'cards';
}

interface TermFiltersProps extends TermsProps {
  value: EntryTermFilters;
  onChange: (value: EntryTermFilters) => void;
}

const fieldClass = 'block w-full min-w-0 rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-600 disabled:opacity-50';
const maxTerms = (kind: TaxonomyKind): number => kind === 'category' ? 32 : 64;

function sortTerms(a: TaxonomyTerm, b: TaxonomyTerm): number {
  return a.name.localeCompare(b.name, getEffectiveBcp47()) || a.id - b.id;
}

/** Order hierarchical terms by parent without hiding an orphan if the tree changed mid-fetch. */
function termTree(terms: TaxonomyTerm[]): Array<{ term: TaxonomyTerm; depth: number }> {
  const byParent = new Map<number | null, TaxonomyTerm[]>();
  const known = new Set(terms.map(term => term.id));
  for (const term of terms) {
    const parent = term.parent_id !== null && known.has(term.parent_id) ? term.parent_id : null;
    byParent.set(parent, [...(byParent.get(parent) ?? []), term]);
  }
  for (const siblings of byParent.values()) siblings.sort(sortTerms);
  const result: Array<{ term: TaxonomyTerm; depth: number }> = [];
  const visited = new Set<number>();
  const visit = (parentId: number | null, depth: number) => {
    for (const term of byParent.get(parentId) ?? []) {
      if (visited.has(term.id)) continue;
      visited.add(term.id);
      result.push({ term, depth });
      visit(term.id, depth + 1);
    }
  };
  visit(null, 0);
  for (const term of [...terms].sort(sortTerms)) {
    if (visited.has(term.id)) continue;
    visited.add(term.id);
    result.push({ term, depth: 0 });
    visit(term.id, 1);
  }
  return result;
}

function useTermOptions(userId: number | null, typeKey: string | undefined,
  onApiError: TermsProps['onApiError'], allowedKinds?: TaxonomyKind[], locale?: string | null) {
  const [definitions, setDefinitions] = useState<TaxonomyDefinition[]>([]);
  const [byKind, setByKind] = useState<Record<string, TaxonomyTerm[]>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [revision, setRevision] = useState(0);
  const onErrorRef = useRef(onApiError);
  onErrorRef.current = onApiError;
  const kindsKey = allowedKinds?.join('\u0001') ?? '';

  useEffect(() => {
    if (userId === null) {
      setDefinitions([]);
      setByKind({});
      setLoading(false);
      return;
    }
    const controller = new AbortController();
    setLoading(true);
    setError('');
    setDefinitions([]);
    setByKind({});
    Promise.all([
      listTaxonomyDefinitions(controller.signal),
      typeKey ? listEffectiveTaxonomies(typeKey, controller.signal)
        : Promise.resolve(allowedKinds ?? ['category', 'tag']),
    ]).then(async ([knownDefinitions, eligible]) => {
      // The backend checks eligibility again on Save. Do not offer unknown,
      // withdrawn or inactive definitions, even if a stale UI has their keys.
      const permitted = [...new Set(eligible)];
      const available = knownDefinitions.filter(definition => permitted.includes(definition.key)
        && definition.provider_status === 'active' && definition.declaration_state === 'declared');
      const pages = await Promise.all(available.map(async definition =>
        [definition.key, await listAllTaxonomyTerms(definition.key, controller.signal, locale)] as const));
      return { available, pages };
    })
      .then(({ available, pages }) => {
        if (controller.signal.aborted) return;
        setDefinitions(available);
        setByKind(Object.fromEntries(pages));
      })
      .catch(cause => {
        if (controller.signal.aborted) return;
        setError(cause instanceof Error ? cause.message : __("Could not load applicable taxonomies."));
        onErrorRef.current(cause, __("Could not load applicable taxonomies."));
      })
      .finally(() => { if (!controller.signal.aborted) setLoading(false); });
    return () => controller.abort();
  }, [userId, typeKey, kindsKey, revision, locale]);

  return { definitions, byKind,
    categories: byKind.category ?? [], tags: byKind.tag ?? [], loading, error,
    reload: () => setRevision(value => value + 1) };
}

/** Native checkboxes preserve keyboard navigation and leave unknown saved IDs removable. */
export function TermPicker({ userId, typeKey, locale, disabled, selectedIds, onChange,
  primaryCategoryTermId = null, onPrimaryCategoryChange, onApiError, allowedKinds, variant = 'default' }: TermPickerProps) {
  useI18n();
  const { definitions, byKind, loading, error, reload } = useTermOptions(userId, typeKey, onApiError, allowedKinds, locale);
  const searchId = useId();
  const [search, setSearch] = useState('');
  const [searchByKind, setSearchByKind] = useState<Record<string, string>>({});
  const [collapsedKinds, setCollapsedKinds] = useState<Record<string, boolean>>({});
  const [selectionError, setSelectionError] = useState('');
  const allTerms = useMemo(() => Object.values(byKind).flat(), [byKind]);
  const known = useMemo(() => new Map(allTerms.map(term => [term.id, term])), [allTerms]);
  const selected = useMemo(() => new Set(selectedIds), [selectedIds]);
  const counts = useMemo(() => {
    const result: Record<string, number> = {};
    for (const id of selectedIds) {
      const kind = known.get(id)?.kind;
      if (kind) result[kind] = (result[kind] ?? 0) + 1;
    }
    return result;
  }, [selectedIds, known]);
  const unknownIds = selectedIds.filter(id => !known.has(id));
  const bcp47 = getEffectiveBcp47();
  const normalizedSearch = search.trim().toLocaleLowerCase(bcp47);

  const toggle = (id: number, checked: boolean) => {
    const term = known.get(id);
    if (checked && term) {
      if (unknownIds.length > 0) {
        setSelectionError(__("Some saved terms are unavailable. Remove those IDs before selecting additional terms."));
        return;
      }
      if (selected.size >= 96) {
        setSelectionError(__("Content supports up to 96 terms in total."));
        return;
      }
      const count = counts[term.kind] ?? 0;
      if (count >= maxTerms(term.kind)) {
        setSelectionError(__("Up to {{value1}} terms of taxonomy {{value2}} per content item.", { value1: maxTerms(term.kind), value2: term.kind }));
        return;
      }
    }
    setSelectionError('');
    if (!checked && id === primaryCategoryTermId) onPrimaryCategoryChange?.(null);
    onChange([...new Set(checked ? [...selectedIds, id] : selectedIds.filter(value => value !== id))]
      .sort((a, b) => a - b));
  };

  const groups = definitions.map(definition => ({
    kind: definition.key,
    title: definition.label,
    hierarchical: definition.hierarchical,
    terms: definition.hierarchical ? termTree(byKind[definition.key] ?? [])
      : [...(byKind[definition.key] ?? [])].sort(sortTerms).map(term => ({ term, depth: 0 })),
    count: counts[definition.key] ?? 0,
  }));
  const selectedTags = (byKind.tag ?? []).filter(term => selected.has(term.id));

  if (variant === 'cards') {
    return (
      <div className="min-w-0 space-y-4" aria-label={__("Assign taxonomies to content")}>
        {loading && (
          <div className="rounded-xl border border-zinc-200 bg-white p-4 shadow-2xs">
            <p role="status" className="text-xs text-zinc-500">{__("Loading classifications…")}</p>
          </div>
        )}
        {error && (
          <div role="alert" className="space-y-2 rounded-xl border border-rose-200 bg-rose-50 p-4 text-xs text-rose-800">
            <p>{error} {__("Selected IDs are retained in the draft.")}</p>
            <button type="button" className="inline-flex h-7 items-center rounded-md border border-rose-300 bg-white px-2.5 text-xs font-medium text-rose-800 hover:bg-rose-50" onClick={reload} disabled={disabled}>
              {__("Try reloading")}
            </button>
          </div>
        )}
        {selectionError && (
          <p role="alert" className="rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-xs text-rose-800">{selectionError}</p>
        )}
        {groups.map(group => {
          const isCollapsed = Boolean(collapsedKinds[group.kind]);
          const q = (searchByKind[group.kind] ?? '').trim().toLocaleLowerCase(bcp47);
          const visible = group.terms.filter(({ term }) =>
            !q || term.name.toLocaleLowerCase(bcp47).includes(q) || term.slug.toLocaleLowerCase(bcp47).includes(q));
          const pickedInGroup = group.terms.filter(({ term }) => selected.has(term.id));
          return (
            <section
              key={group.kind}
              data-taxonomy-box={group.kind}
              className="rounded-xl border border-zinc-200 bg-white p-4 shadow-2xs"
            >
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-1.5">
                  <h3 className="text-sm font-semibold text-zinc-900">{group.title}</h3>
                  <span className="rounded-full bg-zinc-100 px-1.5 py-0.5 text-[11px] font-medium text-zinc-600">
                    {group.count}/{maxTerms(group.kind)}
                  </span>
                </div>
                <button
                  type="button"
                  aria-expanded={!isCollapsed}
                  onClick={() => setCollapsedKinds(prev => ({ ...prev, [group.kind]: !prev[group.kind] }))}
                  className="rounded border-0 bg-transparent px-2 py-1 text-xs font-medium text-zinc-500 hover:bg-zinc-100 hover:text-zinc-900"
                >
                  {isCollapsed ? __("Expand") : __("Collapse")}
                </button>
              </div>
              {!isCollapsed && (
                <div className="mt-3 space-y-2.5">
                  {group.terms.length > 5 && (
                    <input
                      type="search"
                      aria-label={__("Search in {{value1}}", { value1: group.title })}
                      value={searchByKind[group.kind] ?? ''}
                      onChange={event => setSearchByKind(prev => ({ ...prev, [group.kind]: event.target.value }))}
                      placeholder={__("Search {{value1}}...", { value1: group.title.toLowerCase() })}
                      disabled={disabled}
                      className="h-8 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2.5 text-xs text-zinc-900 placeholder:text-zinc-400 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50"
                    />
                  )}
                  {!group.hierarchical && pickedInGroup.length > 0 && (
                    <div className="flex flex-wrap gap-1.5" role="list" aria-label={__("{{value1}} selected", { value1: group.title })}>
                      {pickedInGroup.map(({ term }) => (
                        <span key={term.id} role="listitem" className="inline-flex items-center gap-1 rounded-full bg-od-accent-soft px-2 py-0.5 text-xs font-medium text-od-accent">
                          <span>{term.name}</span>
                          <button
                            type="button"
                            aria-label={__("Remove {{value1}}", { value1: term.name })}
                            disabled={disabled}
                            className="rounded-full px-0.5 font-bold hover:bg-white/60 disabled:opacity-50"
                            onClick={() => toggle(term.id, false)}
                          >
                            ×
                          </button>
                        </span>
                      ))}
                    </div>
                  )}
                  <div className="max-h-52 space-y-1 overflow-y-auto pr-1">
                    {visible.map(({ term, depth }) => (
                      <div key={term.id} className="flex min-w-0 items-center justify-between gap-2 rounded-md px-1.5 py-1 text-sm text-zinc-800 hover:bg-zinc-50">
                        <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-2">
                          <input
                            type="checkbox"
                            className="shrink-0"
                            checked={selected.has(term.id)}
                            onChange={event => toggle(term.id, event.target.checked)}
                            disabled={disabled || loading || Boolean(error) || (!selected.has(term.id) && (group.count >= maxTerms(group.kind) || selected.size >= 96))}
                          />
                          <span className="min-w-0 truncate" style={{ paddingLeft: `${Math.min(depth, 6) * 12}px` }} title={`${term.name} (${term.slug})`}>
                            {term.name}
                          </span>
                        </label>
                        {typeKey === 'post' && group.kind === 'category' && selected.has(term.id) && onPrimaryCategoryChange && (
                          <button
                            type="button"
                            data-testid={`set-primary-term-${term.id}`}
                            aria-pressed={primaryCategoryTermId === term.id}
                            disabled={disabled || loading || Boolean(error)}
                            onClick={() => onPrimaryCategoryChange(term.id)}
                            className="shrink-0 rounded-md border border-zinc-200 px-2 py-0.5 text-[11px] font-medium text-od-accent hover:bg-od-accent-soft disabled:opacity-50"
                          >
                            {primaryCategoryTermId === term.id ? _x("Primary", "feature.taxonomy") : __("Set as primary")}
                          </button>
                        )}
                      </div>
                    ))}
                    {!loading && visible.length === 0 && (
                      <p className="py-1 text-xs text-zinc-500">{q ? _x("No matching items.", "feature.taxonomy") : __("No items yet.")}</p>
                    )}
                  </div>
                </div>
              )}
            </section>
          );
        })}
        {unknownIds.length > 0 && (
          <section className="rounded-xl border border-amber-300 bg-amber-50/70 p-4 text-xs text-amber-950">
            <h3 className="font-semibold">
              {loading ? __("Selected ID (loading list)") : __("Classification ID not found")}
            </h3>
            <p className="mt-1">
              {loading ? __("Your selection is retained while loading.") : __("The item may have been deleted or become inaccessible. Clear selection to remove it.")}
            </p>
            <div className="mt-2 flex flex-wrap gap-2">
              {unknownIds.map(id => (
                <label key={id} className="inline-flex items-center gap-1.5">
                  <input type="checkbox" checked onChange={() => toggle(id, false)} disabled={disabled} />
                  ID #{id}
                </label>
              ))}
            </div>
          </section>
        )}
      </div>
    );
  }

  return (
    <section className="min-w-0 space-y-3" aria-label={__("Assign taxonomies to content")}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-xs text-slate-600">{__("Select up to 32 categories or 64 terms for each other permitted taxonomy.")}</p>
        <button type="button" className="btn-secondary" onClick={reload} disabled={disabled || loading}>
          {__("Reload taxonomies")}
        </button>
      </div>
      <div>
        <label htmlFor={searchId} className="mb-1 block text-sm font-medium text-slate-700">{__("Search terms")}</label>
        <input id={searchId} type="search" value={search} onChange={event => setSearch(event.target.value)}
          placeholder={__("Search by name or slug")} disabled={disabled} className={fieldClass} />
      </div>
      {loading && <p role="status" className="text-sm text-slate-600">{_x("Loading applicable taxonomies…", "feature.taxonomy")}</p>}
      {error && (
        <div role="alert" className="space-y-2 text-sm text-red-700">
          <p>{error} {__("Selected IDs are retained in the draft.")}</p>
          <button type="button" className="btn-secondary" onClick={reload} disabled={disabled}>{__("Try reloading")}</button>
        </div>
      )}
      {selectionError && <p role="alert" className="text-sm text-red-700">{selectionError}</p>}
      {selectedTags.length > 0 && (
        <div className="flex flex-wrap gap-2" role="list" aria-label={__("Selected tags")}>
          {selectedTags.map(tag => (
            <div key={tag.id} role="listitem" className="inline-flex items-center gap-1 rounded-full bg-sky-50 px-2.5 py-1 text-xs text-sky-900">
              <span>{tag.name}</span>
              <button type="button" aria-label={__("Remove tag {{value1}}", { value1: tag.name })} disabled={disabled}
                className="rounded-full px-1 font-bold hover:bg-sky-100 disabled:opacity-50"
                onClick={() => toggle(tag.id, false)}>×</button>
            </div>
          ))}
        </div>
      )}
      <div className="grid min-w-0 grid-cols-1 gap-3 lg:grid-cols-2">
        {groups.map(group => {
          const visible = group.terms.filter(({ term }) =>
            !normalizedSearch || term.name.toLocaleLowerCase(bcp47).includes(normalizedSearch) ||
            term.slug.toLocaleLowerCase(bcp47).includes(normalizedSearch));
          return (
            <fieldset key={group.kind} disabled={disabled} className="min-w-0 rounded-lg border border-slate-200 p-3">
              <legend className="px-1 text-sm font-semibold text-slate-800">
                {group.title} ({group.count}/{maxTerms(group.kind)})
              </legend>
              <div className="max-h-56 space-y-1 overflow-y-auto">
                {visible.map(({ term, depth }) => (
                  <label key={term.id} className="flex min-w-0 cursor-pointer items-start gap-2 rounded px-1 py-1 text-sm hover:bg-slate-50">
                    <input type="checkbox" className="mt-1 accent-blue-600" checked={selected.has(term.id)}
                      onChange={event => toggle(term.id, event.target.checked)}
                      disabled={disabled || loading || Boolean(error) || (!selected.has(term.id) && (group.count >= maxTerms(group.kind) || selected.size >= 96))} />
                    <span className="min-w-0 break-words" style={{ paddingLeft: `${Math.min(depth, 8) * 10}px` }}>
                      <span>{term.name}</span> <span className="text-slate-500">({term.slug})</span>
                    </span>
                  </label>
                ))}
                {!loading && visible.length === 0 && (
                  <p className="text-sm text-slate-500">{normalizedSearch ? _x("No matching items.", "feature.taxonomy") : __("No items yet.")}</p>
                )}
              </div>
            </fieldset>
          );
        })}
      </div>
      {unknownIds.length > 0 && (
        <fieldset disabled={disabled} className="min-w-0 rounded-lg border border-amber-300 p-3">
          <legend className="px-1 text-sm font-medium text-amber-900">
            {loading ? __("Selected ID (loading list)") : __("Saved ID not found")}
          </legend>
          <p className="mb-2 text-xs text-amber-900">
            {loading ? __("Your selection is retained while loading.") : __("The item may have been deleted or become inaccessible. Clear selection if you no longer need this association.")}
          </p>
          <div className="flex flex-wrap gap-3">
            {unknownIds.map(id => (
              <label key={id} className="flex items-center gap-2 text-sm">
                <input type="checkbox" checked onChange={() => toggle(id, false)} disabled={disabled} />
                ID #{id}
              </label>
            ))}
          </div>
        </fieldset>
      )}
      <p className="text-xs text-slate-600">{__("Only existing items can be associated. Unsaved selections do not change site data until you save.")}</p>
    </section>
  );
}

/** Same filter controls for Post, Page and CPT; the API applies them before LIMIT/OFFSET. */
export function TermFilters({ userId, typeKey, disabled, value, onChange, onApiError }: TermFiltersProps) {
  useI18n();
  const { categories, tags, definitions, byKind, loading, error, reload } = useTermOptions(userId, typeKey, onApiError);
  const categoryId = useId();
  const tagId = useId();
  const descendantId = useId();
  const genericTaxonomyId = useId();
  const genericTermId = useId();
  const ordered = useMemo(() => termTree(categories), [categories]);
  const missingCategory = value.categoryId != null && !categories.some(term => term.id === value.categoryId);
  const missingTag = value.tagId != null && !tags.some(term => term.id === value.tagId);
  const genericDefinition = definitions.find(item => item.key === value.taxonomyKey);
  const genericTerms = value.taxonomyKey ? byKind[value.taxonomyKey] ?? [] : [];
  const genericRows = genericDefinition?.hierarchical
    ? termTree(genericTerms) : [...genericTerms].sort(sortTerms).map(term => ({term, depth: 0}));
  return (
    <div className="min-w-0 space-y-2" aria-label={__("Category and tag filters")}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="text-xs text-slate-600">{__("Filter results on the server before pagination.")}</span>
        <button type="button" className="btn-secondary" onClick={reload} disabled={disabled || loading}>
          {__("Reload taxonomies")}
        </button>
      </div>
      <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">
        <label htmlFor={categoryId} className="block min-w-0 text-sm font-medium text-slate-700">
          {_x("Category", "feature.content")}
          <select id={categoryId} className={`mt-1 ${fieldClass}`} value={value.categoryId ?? ''}
            disabled={disabled || loading} onChange={event => onChange({
              ...value, taxonomyKey: null, termId: null,
              categoryId: event.target.value ? Number(event.target.value) : null,
              includeDescendants: event.target.value ? value.includeDescendants : false,
            })}>
            <option value="">{_x("All categories", "feature.taxonomy")}</option>
            {missingCategory && <option value={value.categoryId!}>ID #{value.categoryId} {__("(no longer in list)")}</option>}
            {ordered.map(({ term, depth }) => (
              <option key={term.id} value={term.id}>{'— '.repeat(Math.min(depth, 8))}{term.name} ({term.slug})</option>
            ))}
          </select>
        </label>
        <label htmlFor={tagId} className="block min-w-0 text-sm font-medium text-slate-700">
          {__("Tags")}
          <select id={tagId} className={`mt-1 ${fieldClass}`} value={value.tagId ?? ''}
            disabled={disabled || loading} onChange={event => onChange({
              ...value, taxonomyKey: null, termId: null,
              tagId: event.target.value ? Number(event.target.value) : null,
            })}>
            <option value="">{_x("All tags", "feature.taxonomy")}</option>
            {missingTag && <option value={value.tagId!}>ID #{value.tagId} {__("(no longer in list)")}</option>}
            {[...tags].sort(sortTerms).map(term => <option key={term.id} value={term.id}>{term.name} ({term.slug})</option>)}
          </select>
        </label>
      </div>
      <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">
        <label htmlFor={genericTaxonomyId} className="block min-w-0 text-sm font-medium text-slate-700">
          {__("Custom taxonomies")}
          <select id={genericTaxonomyId} className={`mt-1 ${fieldClass}`}
            value={value.taxonomyKey ?? ''} disabled={disabled || loading}
            onChange={event => onChange({ taxonomyKey: event.target.value || null, termId: null, includeDescendants: false })}>
            <option value="">{__("No custom filter")}</option>
            {definitions.filter(item => item.key !== 'category' && item.key !== 'tag').map(item =>
              <option key={item.key} value={item.key}>{item.label} ({item.key})</option>)}
            {value.taxonomyKey && !genericDefinition && <option value={value.taxonomyKey}>{__("Taxonomy no longer active (")}{value.taxonomyKey})</option>}
          </select>
        </label>
        {value.taxonomyKey && <label htmlFor={genericTermId} className="block min-w-0 text-sm font-medium text-slate-700">
          {__("Term")}
          <select id={genericTermId} className={`mt-1 ${fieldClass}`}
            value={value.termId ?? ''} disabled={disabled || loading || !genericDefinition}
            onChange={event => onChange({ ...value, termId: event.target.value ? Number(event.target.value) : null })}>
            <option value="">{__("Choose term")}</option>
            {value.termId != null && !genericTerms.some(term => term.id === value.termId) &&
              <option value={value.termId}>ID #{value.termId} {__("(no longer available)")}</option>}
            {genericRows.map(({ term, depth }) =>
              <option key={term.id} value={term.id}>{'— '.repeat(Math.min(depth, 8))}{term.name} ({term.slug})</option>)}
          </select>
        </label>}
      </div>
      <label htmlFor={descendantId} className="flex cursor-pointer items-center gap-2 text-sm text-slate-700">
        <input id={descendantId} type="checkbox"
          checked={Boolean(value.includeDescendants && (value.categoryId != null || (value.termId != null && genericDefinition?.hierarchical)))}
          disabled={disabled || (value.categoryId == null && !(value.termId != null && genericDefinition?.hierarchical))}
          onChange={event => onChange({ ...value, includeDescendants: event.target.checked })} />
        {__("Include child terms (hierarchical taxonomy)")}
      </label>
      {loading && <p role="status" className="text-xs text-slate-500">{__("Loading filters…")}</p>}
      {error && (
        <div role="alert" className="flex flex-wrap items-center gap-2 text-xs text-red-700">
          {error} <button type="button" className="btn-secondary" onClick={reload} disabled={disabled}>{__("Try reloading")}</button>
        </div>
      )}
    </div>
  );
}
