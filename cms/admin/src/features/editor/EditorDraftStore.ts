import type { ContentFieldValue, EffectiveEditorConfigProjection } from '../content/types.ts';

export type DraftStatus = 'active' | 'quarantined';

export interface EditorDraftPayload {
  id: number | null;
  version: number | null;
  title: string;
  slug: string;
  /** Keep manual slug edits from being replaced by title auto-slug after return. */
  slug_touched?: boolean;
  publication_status: 'draft' | 'published';
  values: Record<string, ContentFieldValue>;
  cleared_fields: string[];
  term_ids: number[];
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
  for (const key of ['title', 'slug', 'publication_status', 'term_ids', 'custom_path', 'seo'] as const) {
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
  private sessionEpoch = 1;

  getSessionEpoch(): number {
    return this.sessionEpoch;
  }

  bumpSessionEpoch(): number {
    this.sessionEpoch += 1;
    return this.sessionEpoch;
  }

  makeDraftKey(typeKey: string, entryId: number | null, draftUuid?: string): string {
    if (entryId !== null) return `${typeKey}:${entryId}`;
    return `${typeKey}:new:${draftUuid ?? 'default'}`;
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

  hasAnyDirty(): boolean {
    for (const d of this.drafts.values()) {
      if (d.dirty_fields.length > 0 || d.payload.custom_path_dirty || d.payload.seo_dirty) {
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
  }

  clear(): void {
    this.bumpSessionEpoch();
    this.drafts.clear();
  }

  /** Rehydration only after a fresh server config and entry read. Never allow
   * a prior account/site's RAM draft into a new session, even with the same ID. */
  restoreAuthorizedDraft(
    draftKey: string,
    siteInstanceId: string,
    userId: number,
    freshConfig: EffectiveEditorConfigProjection,
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
      term_ids: savedDirty.has('term_ids') ? [...saved.payload.term_ids] : [...fresh.term_ids],
      values,
      cleared_fields: clearedFields,
      custom_path: routeReadable && saved.payload.custom_path_dirty
        ? saved.payload.custom_path : fresh.custom_path,
      seo: seoReadable && saved.payload.seo_dirty ? saved.payload.seo : fresh.seo,
    };
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
