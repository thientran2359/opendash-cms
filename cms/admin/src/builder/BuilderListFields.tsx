import { __, _x, useI18n } from '../i18n/index.tsx';
import { useEffect, useState } from 'react';
import { ApiClientError } from '../api/client.ts';
import { getEffectiveContentTypeTaxonomies, listContentTypes } from '../api/content.ts';
import { listAllTaxonomyTerms, listTaxonomyDefinitions, type TaxonomyDefinition, type TaxonomyTerm } from '../api/taxonomy.ts';
import type { OwnedContentType } from '../features/content/types.ts';
import type { BuilderElement, ListProps, ListSort } from './document.ts';

interface Props {
  element: BuilderElement;
  onChange: (id: string, config: ListProps) => void;
  onError: (cause: unknown, fallback: string) => void;
}

type TaxonomyListProps = ListProps & { taxonomy_key?: string; term_id?: number };

function readConfig(element: BuilderElement): TaxonomyListProps {
  const p = element.props;
  return {
    type_key: String(p.type_key ?? 'post'),
    ...(typeof p.category_id === 'number' ? { category_id: p.category_id } : {}),
    ...(typeof p.tag_id === 'number' ? { tag_id: p.tag_id } : {}),
    ...(typeof p.taxonomy_key === 'string' ? { taxonomy_key: p.taxonomy_key } : {}),
    ...(typeof p.term_id === 'number' ? { term_id: p.term_id } : {}),
    include_descendants: p.include_descendants === true,
    sort: p.sort === 'id_asc' ? 'id_asc' : 'id_desc',
    page_size: typeof p.page_size === 'number' ? p.page_size : 6,
  };
}

/** G3: selects existing CPT and taxonomy through the authenticated Admin API.
 * Missing schema/terms and failed lookups must never clear stored configuration.
 */
export function BuilderListFields({ element, onChange, onError }: Props) {
  const { __, _x } = useI18n();
  const config = readConfig(element);
  const [types, setTypes] = useState<OwnedContentType[]>([]);
  const [categories, setCategories] = useState<TaxonomyTerm[]>([]);
  const [tags, setTags] = useState<TaxonomyTerm[]>([]);
  const [definitions, setDefinitions] = useState<TaxonomyDefinition[]>([]);
  const [eligibleKeys, setEligibleKeys] = useState<string[]>([]);
  const [genericTerms, setGenericTerms] = useState<TaxonomyTerm[]>([]);
  const [filterMode, setFilterMode] = useState<'legacy' | 'generic'>(config.taxonomy_key ? 'generic' : 'legacy');
  const [selectedTaxonomy, setSelectedTaxonomy] = useState(config.taxonomy_key ?? '');
  const [termsBusy, setTermsBusy] = useState(false);
  const [lookupError, setLookupError] = useState('');
  const [reload, setReload] = useState(0);
  const [busy, setBusy] = useState(true);

  useEffect(() => {
    setFilterMode(config.taxonomy_key ? 'generic' : 'legacy');
    setSelectedTaxonomy(config.taxonomy_key ?? '');
  }, [element.id]);

  useEffect(() => {
    let alive = true;
    const controller = new AbortController();
    setBusy(true);
    Promise.all([
      listContentTypes(),
      listAllTaxonomyTerms('category', controller.signal),
      listAllTaxonomyTerms('tag', controller.signal),
      listTaxonomyDefinitions(controller.signal),
      getEffectiveContentTypeTaxonomies(config.type_key, controller.signal),
    ]).then(([available, categoryTerms, tagTerms, definitionsData, eligible]) => {
      if (!alive) return;
      setTypes(available);
      setCategories(categoryTerms);
      setTags(tagTerms);
      setDefinitions(definitionsData);
      setEligibleKeys(eligible);
      setLookupError('');
    }).catch(cause => {
      if (!alive) return;
      setLookupError(__("Could not load CPTs or taxonomies. Saved list configuration is preserved."));
      if (cause instanceof ApiClientError && cause.status === 401)
        onError(cause, __("Your session expired while loading the List block."));
    }).finally(() => { if (alive) setBusy(false); });
    return () => { alive = false; controller.abort(); };
  }, [element.id, config.type_key, reload]);

  useEffect(() => {
    if (!selectedTaxonomy || filterMode !== 'generic') {
      setGenericTerms([]);
      setTermsBusy(false);
      return;
    }
    let alive = true;
    const controller = new AbortController();
    setTermsBusy(true);
    listAllTaxonomyTerms(selectedTaxonomy, controller.signal)
      .then(terms => { if (alive) setGenericTerms(terms); })
      .catch(cause => {
        if (!alive) return;
        setGenericTerms([]);
        setLookupError(__("Could not load terms for the selected taxonomy. Previous configuration is preserved."));
        if (cause instanceof ApiClientError && cause.status === 401)
          onError(cause, __("Your session expired while loading the List block."));
      })
      .finally(() => { if (alive) setTermsBusy(false); });
    return () => { alive = false; controller.abort(); };
  }, [selectedTaxonomy, filterMode, element.id, reload]);

  const update = (changes: Partial<TaxonomyListProps>) => {
    const next = { ...config, ...changes };
    if (!next.taxonomy_key || !next.term_id) {
      delete next.taxonomy_key;
      delete next.term_id;
    } else {
      delete next.category_id;
      delete next.tag_id;
    }
    if (!next.category_id) {
      delete next.category_id;
    }
    if (!next.tag_id) delete next.tag_id;
    if (!next.category_id && !next.taxonomy_key) next.include_descendants = false;
    onChange(element.id, next);
  };
  const availableDefinitions = definitions.filter(def =>
    def.public && def.provider_status === 'active' && def.declaration_state === 'declared'
      && eligibleKeys.includes(def.key));
  const selectedDefinition = availableDefinitions.find(def => def.key === selectedTaxonomy);
  const missingType = !busy && !types.some(item =>
    item.descriptor.key === config.type_key && item.status === 'active');
  const missingCategory = !busy && config.category_id !== undefined
    && !categories.some(term => term.id === config.category_id);
  const missingTag = !busy && config.tag_id !== undefined
    && !tags.some(term => term.id === config.tag_id);
  const missingGeneric = !busy && !!config.taxonomy_key && (
    !availableDefinitions.some(def => def.key === config.taxonomy_key)
      || (!termsBusy && !genericTerms.some(term => term.id === config.term_id)));

  return (
    <fieldset className="space-y-3 rounded-lg border border-sky-200 p-3" aria-label={__("List Block")}>
      <legend className="px-1 text-sm font-semibold text-sky-900">{__("Content list")}</legend>
      <p className="text-xs text-slate-600">{__("Only queries published content; Preview/Save shows the first page. Public pages paginate each node independently.")}
      </p>
      <label className="block text-xs font-semibold">{__("Content type")}
        <select className="mt-1 w-full" aria-label={__("List content type")} value={config.type_key}
          onChange={event => update({ type_key: event.target.value })}>
          {config.type_key && !types.some(type => type.descriptor.key === config.type_key)
            && <option value={config.type_key}>{config.type_key}{__(" (no longer exists)")}</option>}
          {types.map(type => <option key={type.descriptor.key} value={type.descriptor.key}
            disabled={type.status !== 'active'}>
            {type.descriptor.label} ({type.descriptor.key})
            {type.status !== 'active' ? _x(" — inactive", "builder.builder_list_fields") : ''}
          </option>)}
        </select>
      </label>
      <label className="block text-xs font-semibold">{__("Taxonomy filter")}
        <select className="mt-1 w-full" aria-label={__("List taxonomy filter mode")} value={filterMode}
          onChange={event => {
            const mode = event.target.value as 'legacy' | 'generic';
            setFilterMode(mode);
            if (mode === 'legacy') setSelectedTaxonomy('');
            update({ category_id: undefined, tag_id: undefined,
              taxonomy_key: undefined, term_id: undefined, include_descendants: false });
          }}>
          <option value="legacy">{__("Category / Tag (legacy configuration)")}</option>
          <option value="generic">{__("Custom taxonomy")}</option>
        </select>
      </label>
      {filterMode === 'legacy' ? <>
        <label className="block text-xs font-semibold">
          {_x("Category", "feature.common")}
          <select className="mt-1 w-full" aria-label={__("List category")} value={config.category_id ?? ''}
            onChange={event => update({ category_id: event.target.value ? Number(event.target.value) : undefined })}>
            <option value="">{_x("All categories", "builder.builder_list_fields")}</option>
            {config.category_id !== undefined && !categories.some(term => term.id === config.category_id)
              && <option value={config.category_id}>ID {config.category_id}{__(" (missing)")}</option>}
            {categories.map(term => <option key={term.id} value={term.id}>{term.name} (#{term.id})</option>)}
          </select>
        </label>
        <label className="block text-xs font-semibold">
          {__("Tag")}
          <select className="mt-1 w-full" aria-label={__("List tag")} value={config.tag_id ?? ''}
            onChange={event => update({ tag_id: event.target.value ? Number(event.target.value) : undefined })}>
            <option value="">{_x("All tags", "builder.builder_list_fields")}</option>
            {config.tag_id !== undefined && !tags.some(term => term.id === config.tag_id)
              && <option value={config.tag_id}>ID {config.tag_id}{__(" (missing)")}</option>}
            {tags.map(term => <option key={term.id} value={term.id}>{term.name} (#{term.id})</option>)}
          </select>
        </label>
      </> : <>
        <label className="block text-xs font-semibold">
          {__("Taxonomy")}
          <select className="mt-1 w-full" aria-label={__("List taxonomy")} value={selectedTaxonomy}
            onChange={event => {
              setSelectedTaxonomy(event.target.value);
              update({ category_id: undefined, tag_id: undefined, taxonomy_key: undefined,
                term_id: undefined, include_descendants: false });
            }}>
            <option value="">{__("Choose taxonomy")}</option>
            {selectedTaxonomy && !availableDefinitions.some(def => def.key === selectedTaxonomy)
              && <option value={selectedTaxonomy}>{selectedTaxonomy}{__(" (unavailable)")}</option>}
            {availableDefinitions.map(def =>
              <option key={def.key} value={def.key}>{def.label} ({def.key})</option>)}
          </select>
        </label>
        <label className="block text-xs font-semibold">
          {__("Term")}
          <select className="mt-1 w-full" aria-label={__("List taxonomy term")}
            disabled={!selectedTaxonomy || termsBusy || !selectedDefinition}
            value={selectedTaxonomy === config.taxonomy_key ? (config.term_id ?? '') : ''}
            onChange={event => update({ category_id: undefined, tag_id: undefined,
              taxonomy_key: event.target.value ? selectedTaxonomy : undefined,
              term_id: event.target.value ? Number(event.target.value) : undefined,
              include_descendants: false })}>
            <option value="">{termsBusy ? __("Loading terms…") : __("All terms")}</option>
            {config.taxonomy_key === selectedTaxonomy && config.term_id !== undefined
              && !genericTerms.some(term => term.id === config.term_id)
              && <option value={config.term_id}>ID {config.term_id}{__(" (unavailable)")}</option>}
            {genericTerms.map(term =>
              <option key={term.id} value={term.id}>{term.name} (#{term.id})</option>)}
          </select>
        </label>
      </>}
      <label className="flex items-center gap-2 text-xs font-semibold">
        <input aria-label={__("List include descendants")} type="checkbox"
          disabled={filterMode === 'legacy' ? !config.category_id
            : !selectedDefinition?.hierarchical || !config.term_id}
          checked={config.include_descendants}
          onChange={event => update({ include_descendants: event.target.checked })} />{__("Include child terms")}
      </label>
      <label className="block text-xs font-semibold">{__("Order")}
        <select className="mt-1 w-full" aria-label={__("List sort")} value={config.sort}
          onChange={event => update({ sort: event.target.value as ListSort })}>
          <option value="id_desc">{__("Newest first (id_desc)")}</option>
          <option value="id_asc">{__("Oldest first (id_asc)")}</option>
        </select>
      </label>
      <label className="block text-xs font-semibold">{__("Items per page (1–20)")}
        <input className="mt-1 w-full" aria-label={__("List page size")} type="number"
          min={1} max={20} step={1} value={config.page_size}
          onChange={event => {
            const value = Number(event.target.value);
            if (Number.isInteger(value) && (1 <= value && value <= 20)) update({ page_size: value });
          }} />
      </label>
      {(missingType || missingCategory || missingTag || missingGeneric) &&
        <p role="status" className="text-xs text-amber-900">{__("The referenced CPT or taxonomy/term is unavailable. The list is empty; configuration is preserved on save.")}
        </p>}
      {lookupError && <div className="space-y-2">
        <p role="alert" className="text-xs text-rose-700">{lookupError}</p>
        <button type="button" className="btn-secondary !py-1.5 text-xs"
          onClick={() => { setLookupError(''); setReload(value => value + 1); }}>{__("Reload source list")}
        </button>
      </div>}
    </fieldset>
  );
}
