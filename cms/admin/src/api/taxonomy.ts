import { __ } from '../i18n/index.tsx';
import type { FieldSpec, TermPickerStyle } from '../fields/types.ts';
import type {
  FieldGroupPresentation,
  ResourceOwner,
} from '../features/content/types.ts';
import { requestJson, requestText } from './client.ts';

/** A taxonomy kind is the immutable key of its canonical definition. */
export type TaxonomyKind = string;

export type TaxonomySelectionMode = 'multiple' | 'single';

export interface TaxonomySettings {
  singular_label?: string | null;
  description?: string;
  selection_mode?: TaxonomySelectionMode;
  picker_style?: TermPickerStyle;
  default_term_id?: number | null;
  required_on_publish?: boolean;
  editor_can_create_terms?: boolean;
  show_in_menu?: boolean;
  show_admin_column?: boolean;
}

export interface TaxonomyDefinition {
  key: string;
  label: string;
  hierarchical: boolean;
  public: boolean;
  settings?: TaxonomySettings;
  rewrite_base?: string | null;
  content_types?: string[];
  version: number;
  owner: ResourceOwner;
  provider_status: 'active' | 'provider_inactive' | 'missing' | 'invalid';
  declaration_state: 'declared' | 'withdrawn';
}

export interface TaxonomyDefinitionInput {
  key: string;
  label: string;
  hierarchical: boolean;
  public: boolean;
  settings?: TaxonomySettings;
  content_types?: string[];
  rewrite_base?: string | null;
}

export interface TaxonomyDefinitionChanges {
  label?: string;
  hierarchical?: boolean;
  public?: boolean;
  settings?: TaxonomySettings;
  content_types?: string[];
  rewrite_base?: string | null;
}

const definitionsPath = '/api/admin/taxonomy/definitions';
const definitionPath = (key: string): string => `${definitionsPath}/${encodeURIComponent(key)}`;

export function listTaxonomyDefinitions(signal?: AbortSignal): Promise<TaxonomyDefinition[]> {
  return requestJson(definitionsPath, { signal, defaultErrorMessage: __("Could not load taxonomy definitions.") });
}

export function getTaxonomyDefinition(key: string, signal?: AbortSignal): Promise<TaxonomyDefinition> {
  return requestJson(definitionPath(key), { signal, defaultErrorMessage: __("The taxonomy does not exist.") });
}

export function createTaxonomyDefinition(input: TaxonomyDefinitionInput, csrfToken: string): Promise<TaxonomyDefinition> {
  return requestJson(definitionsPath, { method: 'POST', contentType: 'application/json', csrfToken,
    body: JSON.stringify(input), defaultErrorMessage: __("Could not create the taxonomy.") });
}

export function updateTaxonomyDefinition(key: string, version: number, changes: TaxonomyDefinitionChanges,
  csrfToken: string): Promise<TaxonomyDefinition> {
  return requestJson(definitionPath(key), { method: 'PATCH', contentType: 'application/json', csrfToken,
    ifMatchVersion: version, body: JSON.stringify(changes), defaultErrorMessage: __("Could not update the taxonomy.") });
}

export async function deleteTaxonomyDefinition(key: string, version: number, csrfToken: string): Promise<void> {
  await requestText(definitionPath(key), { method: 'DELETE', csrfToken, ifMatchVersion: version,
    defaultErrorMessage: __("Could not delete the taxonomy.") });
}

/** The effective association resolves site overrides and inactive providers on the server. */
export function listEffectiveTaxonomies(typeKey: string, signal?: AbortSignal): Promise<string[]> {
  return requestJson(`/api/admin/taxonomy/eligibility/${encodeURIComponent(typeKey)}/effective`, {
    signal, defaultErrorMessage: __("Could not resolve taxonomies for the content type."),
  });
}

export function updateTaxonomyEligibility(typeKey: string, version: number, taxonomies: string[],
  csrfToken: string): Promise<unknown> {
  return requestJson(`/api/admin/taxonomy/eligibility/${encodeURIComponent(typeKey)}`, {
    method: 'PUT', contentType: 'application/json', csrfToken, ifMatchVersion: version,
    body: JSON.stringify({ taxonomies: [...new Set(taxonomies)], expected_version: version }),
    defaultErrorMessage: __("Could not save taxonomies assigned to the content type."),
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

export interface TermSeoInput {
  title?: string | null;
  description?: string | null;
  og_image_id?: string | null;
  canonical_url?: string | null;
  noindex: boolean;
}

export interface TermFieldGroupView {
  group_id: string;
  title: string;
  description: string;
  presentation: FieldGroupPresentation;
  fields: FieldSpec[];
  read_only_keys: string[];
}

export interface EffectiveTermEditorConfig {
  schema_version: 2;
  schema_token: string;
  taxonomy: TaxonomyDefinition;
  groups: TermFieldGroupView[];
  values: Record<string, unknown>;
  seo: TermSeoInput;
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
  description?: string;
  sort_order?: number;
  translations?: Array<{ id: number; locale: string; name: string; slug: string }>;
  values?: Record<string, unknown>;
  seo?: TermSeoInput;
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
  description?: string;
  sort_order?: number;
  locale?: string;
  source_term_id?: number | null;
  featured_image_media_id?: string | null;
  values?: Record<string, unknown>;
  schema_token?: string;
  seo?: TermSeoInput;
}

export type TaxonomyTermChanges = Partial<Pick<TaxonomyTerm, 'name' | 'slug' | 'parent_id' | 'featured_image_media_id' | 'description' | 'sort_order'>> & {
  values?: Record<string, unknown>;
  schema_token?: string;
  seo?: TermSeoInput | null;
};

const termsPath = '/api/admin/taxonomy/terms';
const termPath = (id: number): string => `${termsPath}/${id}`;

/** A single request never resolves more than 100 IDs. Missing IDs are omitted. */
export function resolveTaxonomyTermLabels(
  ids: number[], signal?: AbortSignal,
): Promise<TaxonomyTermLabel[]> {
  if (!ids.length || ids.length > 100 ||
    new Set(ids).size !== ids.length || ids.some(id => !Number.isSafeInteger(id) || id <= 0)) {
    return Promise.reject(new Error(__("The term ID list must contain 1–100 unique positive integers.")));
  }
  return requestJson<TaxonomyTermLabel[]>(`${termsPath}/resolve?ids=${ids.join(',')}`, {
    signal, defaultErrorMessage: __("Could not load term names on this page."),
  });
}

export function getTaxonomyTerm(id: number, signal?: AbortSignal): Promise<TaxonomyTerm> {
  return requestJson<TaxonomyTerm>(termPath(id), {
    signal,
    defaultErrorMessage: __("The term was not found."),
  });
}

export function getNewTermEffectiveConfig(kind: TaxonomyKind, signal?: AbortSignal): Promise<EffectiveTermEditorConfig> {
  return requestJson<EffectiveTermEditorConfig>(`${definitionPath(kind)}/effective-editor-config`, {
    signal,
    defaultErrorMessage: __("Could not load the extension panel configuration."),
  });
}

export function getEffectiveTermConfig(id: number, signal?: AbortSignal): Promise<EffectiveTermEditorConfig> {
  return requestJson<EffectiveTermEditorConfig>(`${termPath(id)}/effective-editor-config`, {
    signal,
    defaultErrorMessage: __("Could not load the extension panel configuration."),
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
    defaultErrorMessage: __("Could not load terms."),
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
    defaultErrorMessage: __("Could not create the term."),
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
    defaultErrorMessage: __("Could not update the term."),
  });
}

export async function deleteTaxonomyTerm(id: number, version: number, csrfToken: string): Promise<void> {
  await requestText(termPath(id), {
    method: 'DELETE',
    csrfToken,
    ifMatchVersion: version,
    defaultErrorMessage: __("Could not delete the term."),
  });
}
