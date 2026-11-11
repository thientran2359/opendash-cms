import { requestJson, requestText } from './client.ts';

export type TaxonomyKind = 'category' | 'tag';

/** Optional list filters; the server applies them before pagination. */
export interface EntryTermFilters {
  categoryId?: number | null;
  tagId?: number | null;
  includeDescendants?: boolean;
}

export function appendEntryTermFilters(query: URLSearchParams, filters: EntryTermFilters): void {
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
}

export interface TaxonomyTermPage {
  items: TaxonomyTerm[];
  total: number;
}

export interface TaxonomyTermInput {
  kind: TaxonomyKind;
  name: string;
  slug: string;
  parent_id?: number | null;
}

export type TaxonomyTermChanges = Partial<Pick<TaxonomyTerm, 'name' | 'slug' | 'parent_id'>>;

const termsPath = '/api/admin/taxonomy/terms';
const termPath = (id: number): string => `${termsPath}/${id}`;

export function listTaxonomyTerms(
  kind: TaxonomyKind,
  limit = 100,
  offset = 0,
  signal?: AbortSignal,
): Promise<TaxonomyTermPage> {
  const query = new URLSearchParams({ kind, limit: String(limit), offset: String(offset) });
  return requestJson<TaxonomyTermPage>(`${termsPath}?${query}`, {
    signal,
    defaultErrorMessage: 'Không thể tải danh sách category/tag.',
  });
}

/** Iterate the paged term endpoint so editors can choose every existing term. */
export async function listAllTaxonomyTerms(kind: TaxonomyKind, signal?: AbortSignal): Promise<TaxonomyTerm[]> {
  const unique = new Map<number, TaxonomyTerm>();
  let offset = 0;
  while (true) {
    const page = await listTaxonomyTerms(kind, 100, offset, signal);
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
    defaultErrorMessage: 'Không thể tạo category/tag.',
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
    defaultErrorMessage: 'Không thể cập nhật category/tag.',
  });
}

export async function deleteTaxonomyTerm(id: number, version: number, csrfToken: string): Promise<void> {
  await requestText(termPath(id), {
    method: 'DELETE',
    csrfToken,
    ifMatchVersion: version,
    defaultErrorMessage: 'Không thể xóa category/tag.',
  });
}
