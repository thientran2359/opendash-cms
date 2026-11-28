import type { AuthUser } from '../../types.ts';
import type { AdminLocaleState } from '../../api/locales.ts';
import type {
  EffectiveTermEditorConfig,
  TaxonomyKind,
  TaxonomyTerm,
  TaxonomyTermChanges,
  TaxonomyTermInput,
  TermSeoInput,
} from '../../api/taxonomy.ts';
import { isUiOnlyFieldType } from '../../fields/helpers.ts';
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

export interface FormValues {
  name: string;
  slug: string;
  parentId: string;
  featuredImageId: string;
  description: string;
  sortOrder: number;
}

export interface TermDraft {
  form: FormValues;
  baseline: FormValues;
  version: number | null;
  manualSlug: boolean;
  values: Record<string, unknown>;
  valuesBaseline: Record<string, unknown>;
  seo: TermSeoInput;
  seoBaseline: TermSeoInput;
  schemaToken: string | null;
  conflict: boolean;
}

export const termDrafts = new Map<string, TermDraft>();
export const termScope = (
  siteId: string | undefined,
  actor: AuthUser | null,
  kind: TaxonomyKind,
  mode: 'new' | 'edit',
  id: number | null,
): string | null =>
  siteId && siteId !== 'default-site' && actor
    ? JSON.stringify([siteId, actor.id, actor.role, kind, mode, id]) : null;

export const defaultTermSeo = (): TermSeoInput => ({
  title: null,
  description: null,
  og_image_id: null,
  canonical_url: null,
  noindex: false,
});

export function cloneValues(source: Record<string, unknown>): Record<string, unknown> {
  return JSON.parse(JSON.stringify(source ?? {})) as Record<string, unknown>;
}

export const blank = (): FormValues => ({
  name: '',
  slug: '',
  parentId: '',
  featuredImageId: '',
  description: '',
  sortOrder: 0,
});

export const fromTerm = (term: TaxonomyTerm): FormValues => ({
  name: term.name,
  slug: term.slug,
  parentId: term.parent_id === null ? '' : String(term.parent_id),
  featuredImageId: term.featured_image_media_id ?? '',
  description: term.description ?? '',
  sortOrder: term.sort_order ?? 0,
});

export function editableTermFieldKeys(config: EffectiveTermEditorConfig | null): Set<string> {
  const keys = new Set<string>();
  if (!config) return keys;
  for (const grp of config.groups) {
    const ro = new Set(grp.read_only_keys);
    for (const f of grp.fields) {
      if (!isUiOnlyFieldType(f.type) && !ro.has(f.key)) keys.add(f.key);
    }
  }
  return keys;
}

export function rebaseTermValues(
  config: EffectiveTermEditorConfig,
  draftValues: Record<string, unknown>,
  baselineValues: Record<string, unknown>,
): Record<string, unknown> {
  const editable = editableTermFieldKeys(config);
  const rebased: Record<string, unknown> = { ...config.values };
  for (const k of editable) {
    if (JSON.stringify(draftValues[k]) !== JSON.stringify(baselineValues[k])) {
      rebased[k] = draftValues[k];
    }
  }
  return rebased;
}

export function buildCreateTermPayload(opts: {
  kind: string;
  name: string;
  slug: string;
  hierarchical: boolean;
  parentId: number | null;
  form: FormValues;
  isAdmin: boolean;
  hasMultipleLocales: boolean;
  selectedLocale: string;
  editorConfig: EffectiveTermEditorConfig | null;
  values: Record<string, unknown>;
  seo: TermSeoInput;
}): TaxonomyTermInput {
  const { kind, name, slug, hierarchical, parentId, form, isAdmin, hasMultipleLocales, selectedLocale, editorConfig, values, seo } = opts;
  const initialValues: Record<string, unknown> = {};
  for (const k of editableTermFieldKeys(editorConfig)) {
    if (k in values) initialValues[k] = values[k] ?? null;
  }
  const hasSeo = isAdmin && JSON.stringify(seo) !== JSON.stringify(defaultTermSeo());
  return {
    kind,
    name,
    slug,
    ...(hierarchical ? { parent_id: parentId } : {}),
    ...(form.description ? { description: form.description } : {}),
    ...(form.sortOrder !== 0 ? { sort_order: Math.trunc(form.sortOrder) } : {}),
    ...(hasMultipleLocales && selectedLocale ? { locale: selectedLocale } : {}),
    ...(form.featuredImageId ? { featured_image_media_id: form.featuredImageId } : {}),
    ...(editorConfig && Object.keys(initialValues).length > 0
      ? { values: initialValues, schema_token: editorConfig.schema_token }
      : {}),
    ...(hasSeo ? { seo } : {}),
  };
}

export function buildPatchTermChanges(opts: {
  isAdmin: boolean;
  hierarchical: boolean;
  name: string;
  slug: string;
  parentId: number | null;
  form: FormValues;
  baseline: FormValues;
  editorConfig: EffectiveTermEditorConfig | null;
  values: Record<string, unknown>;
  valuesBaseline: Record<string, unknown>;
  seo: TermSeoInput;
  seoBaseline: TermSeoInput;
}): TaxonomyTermChanges {
  const { isAdmin, hierarchical, name, slug, parentId, form, baseline, editorConfig, values, valuesBaseline, seo, seoBaseline } = opts;
  const changes: TaxonomyTermChanges = {};
  if (isAdmin && name !== baseline.name) changes.name = name;
  if (isAdmin && slug !== baseline.slug) changes.slug = slug;
  const oldParent = baseline.parentId ? Number(baseline.parentId) : null;
  if (isAdmin && hierarchical && parentId !== oldParent) changes.parent_id = parentId;
  if (isAdmin && form.description !== baseline.description) changes.description = form.description;
  if (isAdmin && form.sortOrder !== baseline.sortOrder) changes.sort_order = Math.trunc(form.sortOrder);
  if (isAdmin && form.featuredImageId !== baseline.featuredImageId) {
    changes.featured_image_media_id = form.featuredImageId || null;
  }
  if (editorConfig) {
    const changedValues: Record<string, unknown> = {};
    for (const k of editableTermFieldKeys(editorConfig)) {
      if (JSON.stringify(values[k]) !== JSON.stringify(valuesBaseline[k])) {
        changedValues[k] = values[k] ?? null;
      }
    }
    if (Object.keys(changedValues).length > 0) {
      changes.values = changedValues;
      changes.schema_token = editorConfig.schema_token;
    }
  }
  if (isAdmin && JSON.stringify(seo) !== JSON.stringify(seoBaseline)) {
    changes.seo = seo;
  }
  return changes;
}

export const fieldClass =
  'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';
export const selectClass =
  'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent disabled:opacity-50';

export function slugFromName(name: string): string {
  return name.replace(/[\u0111\u0110]/g, 'd').normalize('NFD').replace(/[\u0300-\u036f]/g, '')
    .toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 64).replace(/-+$/g, '');
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
  }).sort((a, b) => a.name.localeCompare(b.name, getEffectiveBcp47()) || a.id - b.id);
}
