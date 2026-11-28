import type {
  ContentFieldValue,
  ContentMode,
  EntryTableRow,
  TaxonomyDeltaOp,
} from '../content/types.ts';
import type { EffectiveEditorConfigWithSettings } from '../../api/content.ts';

export type DraftStatus = 'active' | 'quarantined';

export interface QuickEditOriginalMetadata {
  title: string;
  slug: string;
  publication_status: 'draft' | 'published';
  published_at: string | null;
  published_at_local: string;
  timezone: string;
  term_ids: number[];
  locale: string;
}

export interface QuickEditTaxonomyDraft {
  key: string;
  operation: TaxonomyDeltaOp;
  term_ids: number[];
}

export interface QuickEditDraftPatch {
  title?: string;
  slug?: string;
  publication_status?: 'draft' | 'published';
  published_at_local?: string;
  timezone?: string;
  taxonomy?: QuickEditTaxonomyDraft;
}

/**
 * Distinct in-memory Quick Edit draft stored under `quick_edit:<typeKey>:<entryId>`.
 * Shares session epoch and quarantine/clear lifecycle with Single Edit drafts,
 * but never overwrites or clears a full Single Edit draft for the same entry.
 */
export interface QuickEditDraftEntry {
  kind: 'quick_edit';
  draft_key: string;
  site_instance_id: string;
  user_id: number;
  auth_generation: number;
  type_key: string;
  entry_id: number;
  status: DraftStatus;
  base_version: number;
  schema_token: string;
  original: QuickEditOriginalMetadata;
  patch: QuickEditDraftPatch;
}

export function isQuickEditPatchDirty(patch: QuickEditDraftPatch): boolean {
  return (
    patch.title !== undefined ||
    patch.slug !== undefined ||
    patch.publication_status !== undefined ||
    patch.published_at_local !== undefined ||
    patch.taxonomy !== undefined
  );
}

export interface EditorDraftPayload {
  id: number | null;
  version: number | null;
  title: string;
  slug: string;
  /** Keep manual slug edits from being replaced by title auto-slug after return. */
  slug_touched?: boolean;
  locale?: string;
  publication_status: 'draft' | 'published';
  content_mode?: ContentMode;
  values: Record<string, ContentFieldValue>;
  cleared_fields: string[];
  term_ids: number[];
  primary_category_term_id: number | null;
  parent_id?: number | null;
  menu_order?: number;
  page_layout_key?: string | null;
  custom_template_key?: string | null;
  custom_path?: string | null;
  custom_path_dirty?: boolean;
  seo?: {
    title?: string | null;
    description?: string | null;
    og_image_id?: string | null;
    canonical_url?: string | null;
    noindex?: boolean;
  } | null;
  seo_dirty?: boolean;
}

export interface EditorDraftEntry {
  site_instance_id: string;
  user_id: number;
  auth_generation: number;
  draft_key: string;
  type_key: string;
  status: DraftStatus;
  schema_token_at_edit: string;
  dirty_fields: string[];
  server_values: Record<string, ContentFieldValue>;
  payload: EditorDraftPayload;
}

/** Dirty means actual divergence from the last server snapshot, including
 * metadata-only edits. Version and schema token are CAS guards, not user edits. */
export function diffEditorPayload(
  baseline: EditorDraftPayload,
  draft: EditorDraftPayload,
): string[] {
  const changed: string[] = [];
  for (const key of [
    'title',
    'slug',
    'publication_status',
    'content_mode',
    'term_ids',
    'primary_category_term_id',
    'parent_id',
    'menu_order',
    'page_layout_key',
    'custom_template_key',
    'custom_path',
    'seo',
  ] as const) {
    if (JSON.stringify(baseline[key]) !== JSON.stringify(draft[key])) changed.push(key);
  }
  const fieldKeys = new Set([
    ...Object.keys(baseline.values), ...Object.keys(draft.values),
    ...baseline.cleared_fields, ...draft.cleared_fields,
  ]);
  for (const key of fieldKeys) {
    const hadClear = baseline.cleared_fields.includes(key);
    const hasClear = draft.cleared_fields.includes(key);
    if (hadClear !== hasClear || JSON.stringify(baseline.values[key]) !== JSON.stringify(draft.values[key])) {
      changed.push(key);
    }
  }
  return changed;
}

/**
 * In-memory-only draft store (`R5` / `AC-11`).
 * Never writes to `localStorage` or `sessionStorage`.
 */
export class EditorDraftStore {
  private drafts = new Map<string, EditorDraftEntry>();
  private quickEditDrafts = new Map<string, QuickEditDraftEntry>();
  private sessionEpoch = 1;
  private listeners = new Set<() => void>();

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  getSessionEpoch(): number {
    return this.sessionEpoch;
  }

  bumpSessionEpoch(): number {
    this.sessionEpoch += 1;
    for (const listener of Array.from(this.listeners)) {
      try {
        listener();
      } catch {
        // Ignore subscriber errors during session reset.
      }
    }
    return this.sessionEpoch;
  }

  makeDraftKey(typeKey: string, entryId: number | null, draftUuid?: string): string {
    if (entryId !== null) return `${typeKey}:${entryId}`;
    return `${typeKey}:new:${draftUuid ?? 'default'}`;
  }

  makeQuickEditKey(typeKey: string, entryId: number): string {
    return `quick_edit:${typeKey}:${entryId}`;
  }

  getQuickEdit(typeKey: string, entryId: number): QuickEditDraftEntry | undefined {
    return this.quickEditDrafts.get(this.makeQuickEditKey(typeKey, entryId));
  }

  setQuickEdit(entry: QuickEditDraftEntry): void {
    if (!isQuickEditPatchDirty(entry.patch)) {
      this.quickEditDrafts.delete(entry.draft_key);
      return;
    }
    this.quickEditDrafts.set(entry.draft_key, entry);
  }

  deleteQuickEdit(typeKey: string, entryId: number): void {
    this.quickEditDrafts.delete(this.makeQuickEditKey(typeKey, entryId));
  }

  findQuickEditForType(
    siteInstanceId: string,
    userId: number,
    typeKey: string,
  ): QuickEditDraftEntry | undefined {
    for (const draft of this.quickEditDrafts.values()) {
      if (
        draft.site_instance_id === siteInstanceId &&
        draft.user_id === userId &&
        draft.type_key === typeKey &&
        isQuickEditPatchDirty(draft.patch)
      ) {
        return draft;
      }
    }
    return undefined;
  }

  /**
   * Restores a Quick Edit draft only after verifying the same authorized site, actor,
   * type, and entry ID against a freshly fetched authoritative row. Keeps base_version
   * and schema_token intact so stale saves trigger explicit 412 reconciliation.
   */
  restoreAuthorizedQuickEditDraft(
    siteInstanceId: string,
    userId: number,
    authGeneration: number,
    typeKey: string,
    freshRow: EntryTableRow,
  ): QuickEditDraftEntry | undefined {
    const key = this.makeQuickEditKey(typeKey, freshRow.id);
    const saved = this.quickEditDrafts.get(key);
    if (!saved) return undefined;
    if (
      saved.site_instance_id !== siteInstanceId ||
      saved.user_id !== userId ||
      saved.type_key !== typeKey ||
      saved.entry_id !== freshRow.id
    ) {
      this.quickEditDrafts.delete(key);
      return undefined;
    }
    if (!isQuickEditPatchDirty(saved.patch)) {
      this.quickEditDrafts.delete(key);
      return undefined;
    }
    const restored: QuickEditDraftEntry = {
      ...saved,
      status: 'active',
      auth_generation: authGeneration,
    };
    this.quickEditDrafts.set(key, restored);
    return restored;
  }

  get(draftKey: string): EditorDraftEntry | undefined {
    return this.drafts.get(draftKey);
  }

  hasDirtyKey(draftKey: string): boolean {
    const entry = this.drafts.get(draftKey);
    return Boolean(entry && (entry.dirty_fields.length || entry.payload.custom_path_dirty || entry.payload.seo_dirty));
  }

  set(entry: EditorDraftEntry): void {
    this.drafts.set(entry.draft_key, entry);
  }

  delete(draftKey: string): void {
    this.drafts.delete(draftKey);
  }

  clearDraft(typeKey: string, entryId: number): void {
    this.drafts.delete(this.makeDraftKey(typeKey, entryId));
  }

  hasAnyDirty(): boolean {
    for (const d of this.drafts.values()) {
      if (d.dirty_fields.length > 0 || d.payload.custom_path_dirty || d.payload.seo_dirty) {
        return true;
      }
    }
    for (const q of this.quickEditDrafts.values()) {
      if (isQuickEditPatchDirty(q.patch)) {
        return true;
      }
    }
    return false;
  }

  quarantineAll(): void {
    this.bumpSessionEpoch();
    for (const [k, d] of this.drafts.entries()) {
      this.drafts.set(k, { ...d, status: 'quarantined' });
    }
    for (const [k, q] of this.quickEditDrafts.entries()) {
      this.quickEditDrafts.set(k, { ...q, status: 'quarantined' });
    }
  }

  clear(): void {
    this.bumpSessionEpoch();
    this.drafts.clear();
    this.quickEditDrafts.clear();
  }

  /** Rehydration only after a fresh server config and entry read. Never allow
   * a prior account/site's RAM draft into a new session, even with the same ID. */
  restoreAuthorizedDraft(
    draftKey: string,
    siteInstanceId: string,
    userId: number,
    freshConfig: EffectiveEditorConfigWithSettings,
    authGeneration: number,
    fresh: EditorDraftPayload,
    routeReadable: boolean,
    seoReadable: boolean,
  ): EditorDraftEntry | undefined {
    const saved = this.drafts.get(draftKey);
    if (!saved) return undefined;
    if (saved.site_instance_id !== siteInstanceId || saved.user_id !== userId ||
        saved.type_key !== freshConfig.type_key || saved.payload.id !== fresh.id) {
      this.drafts.delete(draftKey);
      return undefined;
    }

    const values: Record<string, ContentFieldValue> = {};
    const clearedFields: string[] = [];
    for (const [field, access] of Object.entries(freshConfig.effective_fields)) {
      if (!access.editable) {
        // Read-only (or demoted): use freshly authorized server value only.
        if (field in fresh.values) values[field] = fresh.values[field];
      } else if (saved.payload.cleared_fields.includes(field)) {
        clearedFields.push(field);
      } else if (field in saved.payload.values) {
        values[field] = saved.payload.values[field];
      } else if (field in fresh.values) {
        values[field] = fresh.values[field];
      }
      // Unknown/hidden fields are permanently omitted, including clear markers.
    }

    const savedDirty = new Set(saved.dirty_fields);
    const next: EditorDraftPayload = {
      ...fresh,
      version: saved.payload.version, // preserve stale If-Match for 412, never silently rebase
      title: savedDirty.has('title') ? saved.payload.title : fresh.title,
      slug: savedDirty.has('slug') ? saved.payload.slug : fresh.slug,
      slug_touched: saved.payload.slug_touched ?? fresh.slug_touched,
      publication_status: savedDirty.has('publication_status')
        ? saved.payload.publication_status : fresh.publication_status,
      // Switching back to Editor remains authorized after Builder is disabled;
      // do not restore a pending switch *to* Builder without the capability.
      content_mode: savedDirty.has('content_mode') &&
        (freshConfig.settings?.supports.includes('builder') || saved.payload.content_mode === 'editor')
        ? saved.payload.content_mode : fresh.content_mode,
      term_ids: savedDirty.has('term_ids') ? [...saved.payload.term_ids] : [...fresh.term_ids],
      primary_category_term_id: savedDirty.has('primary_category_term_id')
        && freshConfig.taxonomies.includes('category')
        ? (saved.payload.primary_category_term_id ?? null) : fresh.primary_category_term_id,
      parent_id: savedDirty.has('parent_id')
        ? (saved.payload.parent_id ?? null)
        : (fresh.parent_id ?? null),
      menu_order: savedDirty.has('menu_order')
        ? (saved.payload.menu_order ?? 0)
        : (fresh.menu_order ?? 0),
      page_layout_key: savedDirty.has('page_layout_key')
        ? (saved.payload.page_layout_key ?? null)
        : (fresh.page_layout_key ?? null),
      custom_template_key: savedDirty.has('custom_template_key')
        ? (saved.payload.custom_template_key ?? null)
        : (fresh.custom_template_key ?? null),
      values,
      cleared_fields: clearedFields,
      custom_path: routeReadable && saved.payload.custom_path_dirty
        ? saved.payload.custom_path : fresh.custom_path,
      seo: seoReadable && saved.payload.seo_dirty ? saved.payload.seo : fresh.seo,
    };
    // A term removed by the server must not survive as the draft's primary.
    if (next.primary_category_term_id !== null && !next.term_ids.includes(next.primary_category_term_id)) {
      next.primary_category_term_id = null;
    }
    next.custom_path_dirty = routeReadable && diffEditorPayload(fresh, next).includes('custom_path');
    next.seo_dirty = seoReadable && diffEditorPayload(fresh, next).includes('seo');
    const restored = {
      ...saved,
      status: 'active' as const,
      auth_generation: authGeneration,
      // Keep the versioned schema from the edit, even if a new effective
      // schema has since loaded. The server must reject a stale token (412).
      schema_token_at_edit: saved.schema_token_at_edit,
      payload: next,
      server_values: { ...fresh.values },
      dirty_fields: diffEditorPayload(fresh, next),
    };
    // No externally supplied HTML, clipboard, URL query, or storage is trusted.
    if (!restored.dirty_fields.length) this.drafts.delete(draftKey);
    else this.drafts.set(draftKey, restored);
    return restored;
  }

}

export const editorDraftStore = new EditorDraftStore();
