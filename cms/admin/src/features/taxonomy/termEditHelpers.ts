import type { AuthUser } from '../../types.ts';
import type { AdminLocaleState } from '../../api/locales.ts';
import type { TaxonomyKind, TaxonomyTerm } from '../../api/taxonomy.ts';
import type { TermExtensionValues } from '../extensions/AdminContributionRenderer.tsx';
import { getEffectiveBcp47 } from '../../i18n/index.tsx';

export interface TermEditProps {
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

export interface FormValues { name: string; slug: string; parentId: string; featuredImageId: string }
export interface TermDraft {
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
export const termDrafts = new Map<string, TermDraft>();
export const termScope = (siteId: string | undefined, actor: AuthUser | null,
  kind: TaxonomyKind, mode: 'new' | 'edit', id: number | null): string | null =>
  siteId && siteId !== 'default-site' && actor
    ? JSON.stringify([siteId, actor.id, actor.role, kind, mode, id]) : null;
export const cloneExtensions = (source: TermExtensionValues): TermExtensionValues =>
  Object.fromEntries(Object.entries(source).map(([panel, values]) => [panel, { ...values }]));

export const blank = (): FormValues => ({ name: '', slug: '', parentId: '', featuredImageId: '' });
export const fromTerm = (term: TaxonomyTerm): FormValues => ({
  name: term.name,
  slug: term.slug,
  parentId: term.parent_id === null ? '' : String(term.parent_id),
  featuredImageId: term.featured_image_media_id ?? '',
});
export const fieldClass = 'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';
export const selectClass = 'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';

export function slugFromName(name: string): string {
  return name.replace(/[\u0111\u0110]/g, 'd').normalize('NFD').replace(/[\u0300-\u036f]/g, '')
    .toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0,64).replace(/-+$/g,'');
}

export function allowedParents(terms: TaxonomyTerm[], targetId: number | null): TaxonomyTerm[] {
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
