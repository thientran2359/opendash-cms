import type { ContentFieldValue, EffectiveEditorConfigProjection } from '../content/types.ts';

export type DraftStatus = 'active' | 'quarantined';

export interface EditorDraftPayload {
  id: number | null;
  version: number | null;
  title: string;
  slug: string;
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

  /**
   * Re-checks permissions of a quarantined draft against the newly fetched
   * `effective-editor-config` after re-login (`AC-11`):
   * - Fields absent from `freshConfig.effective_fields` (`caller.role < min_view_role`)
   *   are permanently removed from `payload.values` and `dirty_fields`.
   * - Fields with `editable === false` (`caller.role < min_edit_role`) are reverted
   *   to `server_values` and removed from `dirty_fields` / `cleared_fields`.
   */
  scrubQuarantinedDraft(
    draftKey: string,
    freshConfig: EffectiveEditorConfigProjection,
    authGeneration: number,
  ): EditorDraftEntry | undefined {
    const existing = this.drafts.get(draftKey);
    if (!existing) return undefined;

    const nextValues: Record<string, ContentFieldValue> = {};
    const nextDirty = new Set<string>();
    const nextCleared: string[] = [];

    for (const [fk, val] of Object.entries(existing.payload.values)) {
      const eff = freshConfig.effective_fields[fk];
      if (!eff) {
        // Hidden field (`caller.role < min_view_role`): strip permanently
        continue;
      }
      if (!eff.editable) {
        // Read-only field (`caller.role < min_edit_role`): revert to server value
        if (fk in existing.server_values) {
          nextValues[fk] = existing.server_values[fk];
        }
        continue;
      }
      nextValues[fk] = val;
      if (existing.dirty_fields.includes(fk)) {
        nextDirty.add(fk);
      }
    }

    for (const fk of existing.payload.cleared_fields) {
      const eff = freshConfig.effective_fields[fk];
      if (eff && eff.editable) {
        nextCleared.push(fk);
        if (existing.dirty_fields.includes(fk)) {
          nextDirty.add(fk);
        }
      }
    }

    for (const df of existing.dirty_fields) {
      if (df === 'title' || df === 'slug' || df === 'term_ids' || df === 'publication_status') {
        nextDirty.add(df);
      }
    }

    const scrubbed: EditorDraftEntry = {
      ...existing,
      auth_generation: authGeneration,
      status: 'active',
      schema_token_at_edit: freshConfig.schema_token,
      dirty_fields: Array.from(nextDirty),
      payload: {
        ...existing.payload,
        values: nextValues,
        cleared_fields: nextCleared,
      },
    };
    this.drafts.set(draftKey, scrubbed);
    return scrubbed;
  }
}

export const editorDraftStore = new EditorDraftStore();
