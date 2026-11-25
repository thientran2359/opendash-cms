import { tStatic } from '../i18n/index.tsx';
import { requestJson, requestText } from './client.ts';
import type { EffectiveTermConfig, TermExtensionValues } from '../features/extensions/AdminContributionRenderer.tsx';

/** A taxonomy kind is the immutable key of its canonical definition. */
export type TaxonomyKind = string;

export interface TaxonomyDefinition {
  key: string;
  label: string;
  hierarchical: boolean;
  public: boolean;
  version: number;
  owner: { kind: 'core' | 'user' | 'plugin' | 'theme'; id?: string };
  provider_status: 'active' | 'provider_inactive' | 'missing' | 'invalid';
  declaration_state: 'declared' | 'withdrawn';
}

export type TaxonomyDefinitionInput = Pick<TaxonomyDefinition, 'key' | 'label' | 'hierarchical' | 'public'>;
export type TaxonomyDefinitionChanges = Partial<Omit<TaxonomyDefinitionInput, 'key'>>;

const definitionsPath = '/api/admin/taxonomy/definitions';
const definitionPath = (key: string): string => `${definitionsPath}/${encodeURIComponent(key)}`;

export function listTaxonomyDefinitions(signal?: AbortSignal): Promise<TaxonomyDefinition[]> {
  return requestJson(definitionsPath, { signal, defaultErrorMessage: tStatic('api.taxonomy.load_taxonomy_definitions') });
}

export function getTaxonomyDefinition(key: string, signal?: AbortSignal): Promise<TaxonomyDefinition> {
  return requestJson(definitionPath(key), { signal, defaultErrorMessage: tStatic('api.taxonomy.the_taxonomy_does_not_exist') });
}

export function createTaxonomyDefinition(input: TaxonomyDefinitionInput, csrfToken: string): Promise<TaxonomyDefinition> {
  return requestJson(definitionsPath, { method: 'POST', contentType: 'application/json', csrfToken,
    body: JSON.stringify(input), defaultErrorMessage: tStatic('api.taxonomy.create_the_taxonomy') });
}

export function updateTaxonomyDefinition(key: string, version: number, changes: TaxonomyDefinitionChanges,
  csrfToken: string): Promise<TaxonomyDefinition> {
  return requestJson(definitionPath(key), { method: 'PATCH', contentType: 'application/json', csrfToken,
    ifMatchVersion: version, body: JSON.stringify(changes), defaultErrorMessage: tStatic('api.taxonomy.update_the_taxonomy') });
}

export async function deleteTaxonomyDefinition(key: string, version: number, csrfToken: string): Promise<void> {
  await requestText(definitionPath(key), { method: 'DELETE', csrfToken, ifMatchVersion: version,
    defaultErrorMessage: tStatic('api.taxonomy.delete_the_taxonomy') });
}

/** The effective association resolves site overrides and inactive providers on the server. */
export function listEffectiveTaxonomies(typeKey: string, signal?: AbortSignal): Promise<string[]> {
  return requestJson(`/api/admin/taxonomy/eligibility/${encodeURIComponent(typeKey)}/effective`, {
    signal, defaultErrorMessage: tStatic('api.taxonomy.resolve_taxonomies_for_the_content_type'),
  });
}

export function updateTaxonomyEligibility(typeKey: string, version: number, taxonomies: string[],
  csrfToken: string): Promise<unknown> {
  return requestJson(`/api/admin/taxonomy/eligibility/${encodeURIComponent(typeKey)}`, {
    method: 'PUT', contentType: 'application/json', csrfToken, ifMatchVersion: version,
    body: JSON.stringify({ taxonomies: [...new Set(taxonomies)], expected_version: version }),
    defaultErrorMessage: tStatic('api.taxonomy.save_taxonomies_assigned_to_the_content_type'),
  });
}

/** Optional list filters; the server applies them before pagination. */
export interface EntryTermFilters {
  categoryId?: number | null;
  tagId?: number | null;
  includeDescendants?: boolean;
  taxonomyKey?: TaxonomyKind | null;
  termId?: number | null;
}

export function appendEntryTermFilters(query: URLSearchParams, filters: EntryTermFilters): void {
  if (filters.taxonomyKey && filters.termId != null) {
    query.set('taxonomy_key', filters.taxonomyKey);
    query.set('term_id', String(filters.termId));
    if (filters.includeDescendants) query.set('include_descendants', 'true');
    return;
  }
  if (filters.categoryId != null) {
    query.set('category_id', String(filters.categoryId));
    if (filters.includeDescendants) query.set('include_descendants', 'true');
  }
  if (filters.tagId != null) query.set('tag_id', String(filters.tagId));
}

export interface TaxonomyTerm {
  id: number;
  kind: TaxonomyKind;
  name: string;
  slug: string;
  parent_id: number | null;
  version: number;
  locale?: string;
  translation_group_id?: string;
  featured_image_media_id?: string | null;
  extensions?: TermExtensionValues;
  schema_token?: string;
}

export interface TaxonomyTermPage {
  items: TaxonomyTerm[];
  total: number;
}

/** Label-only projection for term IDs referenced by the visible table rows. */
export interface TaxonomyTermLabel {
  id: number;
  kind: TaxonomyKind;
  name: string;
}

export interface TaxonomyTermInput {
  kind: TaxonomyKind;
  name: string;
  slug: string;
  parent_id?: number | null;
  locale?: string;
  source_term_id?: number | null;
  featured_image_media_id?: string | null;
}

export type TaxonomyTermChanges = Partial<Pick<TaxonomyTerm, 'name' | 'slug' | 'parent_id' | 'featured_image_media_id'>> & {
  extensions?: TermExtensionValues;
  schema_token?: string;
};

const termsPath = '/api/admin/taxonomy/terms';
const termPath = (id: number): string => `${termsPath}/${id}`;

/** A single request never resolves more than 100 IDs. Missing IDs are omitted. */
export function resolveTaxonomyTermLabels(
  ids: number[], signal?: AbortSignal,
): Promise<TaxonomyTermLabel[]> {
  if (!ids.length || ids.length > 100 ||
    new Set(ids).size !== ids.length || ids.some(id => !Number.isSafeInteger(id) || id <= 0)) {
    return Promise.reject(new Error(tStatic('api.taxonomy.the_term_id_list_must_contain_1_100_unique_positive_integers')));
  }
  return requestJson<TaxonomyTermLabel[]>(`${termsPath}/resolve?ids=${ids.join(',')}`, {
    signal, defaultErrorMessage: tStatic('api.taxonomy.load_term_names_on_this_page'),
  });
}

export function getTaxonomyTerm(id: number, signal?: AbortSignal): Promise<TaxonomyTerm> {
  return requestJson<TaxonomyTerm>(termPath(id), {
    signal,
    defaultErrorMessage: tStatic('api.taxonomy.the_term_was_not_found'),
  });
}

export function getEffectiveTermConfig(id: number, signal?: AbortSignal): Promise<EffectiveTermConfig> {
  return requestJson(`${termPath(id)}/effective-editor-config`, {
    signal,
    defaultErrorMessage: tStatic('api.taxonomy.load_the_extension_panel_configuration'),
  });
}

export function listTaxonomyTerms(
  kind: TaxonomyKind,
  limit = 100,
  offset = 0,
  signal?: AbortSignal,
  locale?: string | null,
): Promise<TaxonomyTermPage> {
  const query = new URLSearchParams({ kind, limit: String(limit), offset: String(offset) });
  if (locale) query.set('locale', locale);
  return requestJson<TaxonomyTermPage>(`${termsPath}?${query}`, {
    signal,
    defaultErrorMessage: tStatic('api.taxonomy.load_terms'),
  });
}

/** Iterate the paged term endpoint so editors can choose every existing term. */
export async function listAllTaxonomyTerms(
  kind: TaxonomyKind,
  signal?: AbortSignal,
  locale?: string | null,
): Promise<TaxonomyTerm[]> {
  const unique = new Map<number, TaxonomyTerm>();
  let offset = 0;
  while (true) {
    const page = await listTaxonomyTerms(kind, 100, offset, signal, locale);
    for (const term of page.items) unique.set(term.id, term);
    offset += page.items.length;
    if (page.items.length === 0 || offset >= page.total) break;
  }
  return [...unique.values()];
}

export function createTaxonomyTerm(input: TaxonomyTermInput, csrfToken: string): Promise<TaxonomyTerm> {
  return requestJson<TaxonomyTerm>(termsPath, {
    method: 'POST',
    contentType: 'application/json',
    csrfToken,
    body: JSON.stringify(input),
    defaultErrorMessage: tStatic('api.taxonomy.create_the_term'),
  });
}

export function updateTaxonomyTerm(
  id: number,
  version: number,
  changes: TaxonomyTermChanges,
  csrfToken: string,
): Promise<TaxonomyTerm> {
  return requestJson<TaxonomyTerm>(termPath(id), {
    method: 'PATCH',
    contentType: 'application/json',
    csrfToken,
    ifMatchVersion: version,
    body: JSON.stringify(changes),
    defaultErrorMessage: tStatic('api.taxonomy.update_the_term'),
  });
}

export async function deleteTaxonomyTerm(id: number, version: number, csrfToken: string): Promise<void> {
  await requestText(termPath(id), {
    method: 'DELETE',
    csrfToken,
    ifMatchVersion: version,
    defaultErrorMessage: tStatic('api.taxonomy.delete_the_term'),
  });
}
