import { tStatic } from '../i18n/index.tsx';
import { requestJson, requestText } from './client.ts';
import type { BuilderDocument } from '../builder/document.ts';

export interface PatternRecord {
  id: number;
  title: string;
  slug: string;
  version: number;
  referencing_count: number;
  created_at: string;
  updated_at: string;
}

export interface PatternInput { title: string; slug: string }
export interface PatternList { items: PatternRecord[] }
export interface PatternReferencingEntry { id: number; type_key: string; title: string }
export interface PatternDeleteConflict {
  error: string;
  referencing_count: number;
  referencing_entries: PatternReferencingEntry[];
}

const root = '/api/admin/patterns';
const itemUrl = (id: number) => `${root}/${id}`;

export const listPatterns = () => requestJson<PatternList>(root, {
  defaultErrorMessage: tStatic('api.patterns.load_patterns'),
});
export const getPattern = (id: number) => requestJson<PatternRecord>(itemUrl(id), {
  defaultErrorMessage: tStatic('api.patterns.load_the_pattern'),
});
export const createPattern = (input: PatternInput, csrfToken: string) => requestJson<PatternRecord>(root, {
  method: 'POST', csrfToken, contentType: 'application/json', body: JSON.stringify(input),
  defaultErrorMessage: tStatic('api.patterns.create_the_pattern'),
});
export const updatePattern = (id: number, input: PatternInput, version: number, csrfToken: string) =>
  requestJson<PatternRecord>(itemUrl(id), {
    method: 'PUT', csrfToken, contentType: 'application/json', ifMatchVersion: version,
    body: JSON.stringify(input), defaultErrorMessage: tStatic('api.patterns.update_the_pattern'),
  });
/** requestText preserves the conflict JSON in ApiClientError.message for a
 * precise confirmation of all pages affected by a forced deletion. */
export const deletePattern = (id: number, version: number, csrfToken: string, force = false) =>
  requestText(itemUrl(id) + (force ? '?force=true' : ''), {
    method: 'DELETE', csrfToken, ifMatchVersion: version,
    defaultErrorMessage: tStatic('api.patterns.delete_the_pattern'),
  });

/** Theme patterns are immutable COPY sources; they are distinct from the
 * database-backed, synchronized Pattern CPT records above. */
export interface ThemePatternItem {
  source: 'theme';
  id: string;
  key: string;
  title: string;
  category: string;
  description: string | null;
  preview_url: string | null;
}
export interface ThemePatternCatalog { categories: string[]; items: ThemePatternItem[] }
export interface ThemePatternDocument {
  source: 'theme';
  key: string;
  title: string;
  document: BuilderDocument;
}

export function listThemePatterns(category = 'all', q = '', signal?: AbortSignal): Promise<ThemePatternCatalog> {
  const params = new URLSearchParams();
  if (category !== 'all') params.set('category', category);
  if (q.trim()) params.set('q', q.trim());
  const query = params.toString();
  return requestJson(root + '/library' + (query ? '?' + query : ''), {
    signal, defaultErrorMessage: tStatic('api.patterns.load_the_theme_pattern_library'),
  });
}

export function getThemePattern(key: string, signal?: AbortSignal): Promise<ThemePatternDocument> {
  return requestJson(root + '/library/theme/' + encodeURIComponent(key), {
    signal, defaultErrorMessage: tStatic('api.patterns.load_the_theme_pattern_content'),
  });
}
