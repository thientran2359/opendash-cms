import { useCallback, useEffect, useState, type MutableRefObject } from 'react';
import { __ } from '../../i18n/index.tsx';
import { ApiClientError } from '../../api/client.ts';
import {
  effectiveContentTypeSettings,
  type ContentTypeSettings,
  type EffectiveEditorConfigWithSettings,
} from '../../api/content.ts';
import {
  deleteEntryAutosave,
  getEntryAutosave,
  listEntryRevisions,
  type AutosaveSnapshotDto,
  type AutosaveUpsertInput,
  type RestoreEntryRevisionResponse,
} from '../../api/revisions.ts';
import type { AuthUser } from '../../types.ts';
import type { ContentFieldValue } from '../content/types.ts';
import {
  diffEditorPayload,
  editorDraftStore,
  type EditorDraftPayload,
} from '../editor/EditorDraftStore.ts';
import type { AutosaveState, RevisionModalInitialCompare } from './types.ts';
import { useEntryAutosave } from './useEntryAutosave.ts';

export interface UseSingleEditRevisionsOptions {
  typeKey: string;
  mode: 'list' | 'new' | 'edit';
  entryId: number | null;
  isPattern: boolean;
  user: AuthUser | null;
  config: EffectiveEditorConfigWithSettings | null;
  trashedBlockedReason: string | null;
  csrfToken: string | null;
  siteInstanceId: string;
  authGeneration: number;
  dirty: boolean;
  editRevision: number;
  hasLocalRestoredDraft: boolean;
  supportAllowsField: (
    typeKey: string,
    key: string,
    settings: ContentTypeSettings,
  ) => boolean;
  baselineRef: MutableRefObject<EditorDraftPayload | null>;
  draftRef: MutableRefObject<EditorDraftPayload | null>;
  savedThemeKeyRef: MutableRefObject<string | null>;
  selectedThemeKeyRef: MutableRefObject<string | null>;
  canWriteSeoRef: MutableRefObject<boolean>;
  onApplyMergedAutosave: (
    merged: EditorDraftPayload,
    nextSelectedThemeKey: string | null,
  ) => void;
  onTrashedConflict: (message: string) => void;
  onStatusNote: (note: string | null) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

export interface UseSingleEditRevisionsResult {
  revisionCount: number | null;
  hasServerAutosave: boolean;
  serverAutosave: AutosaveSnapshotDto | null;
  hasNewerLocalDraft: boolean;
  bannerPendingDecision: boolean;
  discardingAutosave: boolean;
  historyOpen: boolean;
  initialCompare: RevisionModalInitialCompare;
  autosaveState: AutosaveState;
  abortAndClearInflight: () => void;
  handleApplyServerAutosave: () => void;
  handleDiscardServerAutosave: () => Promise<void>;
  handleOpenHistory: (compare?: RevisionModalInitialCompare) => void;
  handleCloseHistory: () => void;
  clearServerAutosaveState: () => void;
  refreshRevisionCount: () => Promise<void>;
}

const EMPTY_SEO = {
  title: null,
  description: null,
  og_image_id: null,
  canonical_url: null,
  noindex: false,
};

export function buildRestoredEditorPayload(
  typeKey: string,
  res: RestoreEntryRevisionResponse,
  config: EffectiveEditorConfigWithSettings,
): { payload: EditorDraftPayload; savedSelection: string | null } {
  const pageLayoutKey = res.entry.page_layout_key ?? null;
  const customTemplateKey = res.entry.custom_template_key ?? null;
  const savedSelection = typeKey === 'page' ? pageLayoutKey : customTemplateKey;
  const allowedValues = Object.fromEntries(
    Object.entries(res.entry.values).filter(([key]) => key in config.effective_fields),
  );
  return {
    savedSelection,
    payload: {
      id: res.entry.id,
      version: res.entry.version,
      title: res.entry.title,
      slug: res.entry.slug,
      slug_touched: true,
      locale: res.entry.locale,
      publication_status: res.entry.publication_status,
      content_mode: res.entry.content_mode,
      values: allowedValues,
      cleared_fields: [],
      term_ids: res.entry.term_ids ?? [],
      primary_category_term_id: res.entry.primary_category_term_id ?? null,
      parent_id: res.entry.parent_id ?? null,
      menu_order: res.entry.menu_order ?? 0,
      page_layout_key: pageLayoutKey,
      custom_template_key: customTemplateKey,
      custom_path: res.custom_path ?? '',
      seo: res.seo
        ? {
            title: res.seo.title,
            description: res.seo.description,
            og_image_id: res.seo.og_image_id,
            canonical_url: res.seo.canonical_url,
            noindex: res.seo.noindex,
          }
        : EMPTY_SEO,
    },
  };
}

export function useSingleEditRevisions(
  options: UseSingleEditRevisionsOptions,
): UseSingleEditRevisionsResult {
  const {
    typeKey,
    mode,
    entryId,
    isPattern,
    user,
    config,
    trashedBlockedReason,
    csrfToken,
    siteInstanceId,
    authGeneration,
    dirty,
    editRevision,
    hasLocalRestoredDraft,
    supportAllowsField,
    baselineRef,
    draftRef,
    savedThemeKeyRef,
    selectedThemeKeyRef,
    canWriteSeoRef,
    onApplyMergedAutosave,
    onTrashedConflict,
    onStatusNote,
    onApiError,
  } = options;

  const [revisionCount, setRevisionCount] = useState<number | null>(null);
  const [hasServerAutosave, setHasServerAutosave] = useState(false);
  const [serverAutosave, setServerAutosave] = useState<AutosaveSnapshotDto | null>(null);
  const [discardingAutosave, setDiscardingAutosave] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [initialCompare, setInitialCompare] =
    useState<RevisionModalInitialCompare>(null);

  const hasConfig = Boolean(config);
  const isBlocked = Boolean(trashedBlockedReason);

  useEffect(() => {
    if (
      !user ||
      isPattern ||
      mode !== 'edit' ||
      entryId === null ||
      !hasConfig ||
      isBlocked
    ) {
      setRevisionCount(null);
      setHasServerAutosave(false);
      setServerAutosave(null);
      setHistoryOpen(false);
      setInitialCompare(null);
      return;
    }

    const controller = new AbortController();
    const reqEpoch = editorDraftStore.getSessionEpoch();
    let active = true;

    void Promise.all([
      listEntryRevisions(typeKey, entryId, {
        limit: 1,
        offset: 0,
        signal: controller.signal,
      }),
      getEntryAutosave(typeKey, entryId, controller.signal),
    ])
      .then(([revRes, asRes]) => {
        if (
          !active ||
          controller.signal.aborted ||
          reqEpoch !== editorDraftStore.getSessionEpoch()
        ) {
          return;
        }
        setRevisionCount(revRes.total);
        setHasServerAutosave(revRes.has_autosave || asRes.autosave !== null);
        setServerAutosave(asRes.autosave);
      })
      .catch(err => {
        if (
          !active ||
          controller.signal.aborted ||
          (err instanceof DOMException && err.name === 'AbortError')
        ) {
          return;
        }
        if (err instanceof ApiClientError && err.status === 401) {
          onApiError(err, __("Your session expired while checking revision history."));
        }
      });

    return () => {
      active = false;
      controller.abort();
    };
  }, [
    user,
    user?.id,
    user?.role,
    isPattern,
    mode,
    entryId,
    typeKey,
    hasConfig,
    isBlocked,
    siteInstanceId,
    authGeneration,
    onApiError,
  ]);

  const hasNewerLocalDraft = Boolean(serverAutosave && hasLocalRestoredDraft);
  const bannerPendingDecision = Boolean(serverAutosave && !hasNewerLocalDraft);

  const buildPayload = useCallback((): Omit<
    AutosaveUpsertInput,
    'source_surface' | 'client_seq'
  > | null => {
    const draft = draftRef.current;
    const baseline = baselineRef.current;
    if (!draft || !baseline || !config || baseline.version === null) {
      return null;
    }
    if ([...draft.title].length > 200) {
      return null;
    }

    const effectiveSettings = effectiveContentTypeSettings(typeKey, config.settings);
    const submitValues: Record<string, ContentFieldValue> = {};
    const submitCleared: string[] = [];

    for (const [fk, eff] of Object.entries(config.effective_fields)) {
      if (!eff.editable || !supportAllowsField(typeKey, fk, effectiveSettings)) {
        continue;
      }
      if (draft.cleared_fields.includes(fk)) {
        submitCleared.push(fk);
      } else if (fk in draft.values) {
        submitValues[fk] = draft.values[fk];
      }
    }

    const payload: Omit<AutosaveUpsertInput, 'source_surface' | 'client_seq'> = {
      base_version: baseline.version,
      title: draft.title,
      content_mode: draft.content_mode,
      values: submitValues,
      cleared_fields: submitCleared,
      term_ids: draft.term_ids,
    };

    if (
      typeKey === 'post' &&
      config.taxonomies.includes('category') &&
      draft.primary_category_term_id !== baseline.primary_category_term_id
    ) {
      payload.primary_category_term_id = draft.primary_category_term_id;
    }

    if (typeKey === 'page' && selectedThemeKeyRef.current !== savedThemeKeyRef.current) {
      payload.page_layout_key = selectedThemeKeyRef.current;
    } else if (
      typeKey !== 'page' &&
      selectedThemeKeyRef.current !== savedThemeKeyRef.current
    ) {
      payload.custom_template_key = selectedThemeKeyRef.current;
    }

    if (
      canWriteSeoRef.current &&
      diffEditorPayload(baseline, draft).includes('seo')
    ) {
      const normalizedSeo = {
        title: draft.seo?.title?.trim() || null,
        description: draft.seo?.description?.trim() || null,
        og_image_id: draft.seo?.og_image_id?.trim() || null,
        canonical_url: draft.seo?.canonical_url?.trim() || null,
        noindex: Boolean(draft.seo?.noindex),
      };
      const isEmptySeo =
        normalizedSeo.title === null &&
        normalizedSeo.description === null &&
        normalizedSeo.og_image_id === null &&
        normalizedSeo.canonical_url === null &&
        !normalizedSeo.noindex;
      payload.seo = isEmptySeo ? null : normalizedSeo;
    }

    return payload;
  }, [
    baselineRef,
    draftRef,
    config,
    typeKey,
    supportAllowsField,
    savedThemeKeyRef,
    selectedThemeKeyRef,
    canWriteSeoRef,
  ]);

  const { autosaveState, abortAndClearInflight, resetAutosaveState } =
    useEntryAutosave({
      enabled:
        Boolean(user) &&
        !isPattern &&
        mode === 'edit' &&
        entryId !== null &&
        hasConfig &&
        !isBlocked,
      typeKey,
      entryId,
      sourceSurface: 'editor',
      csrfToken,
      siteInstanceId,
      authGeneration,
      dirty,
      dirtyRevision: editRevision,
      bannerPendingDecision,
      initialServerClientSeq: serverAutosave?.client_seq,
      buildPayload,
      onApiError,
      onTrashedConflict,
      onSaved: () => setHasServerAutosave(true),
    });

  const handleApplyServerAutosave = useCallback(() => {
    const as = serverAutosave;
    const baseline = baselineRef.current;
    if (!as || !baseline || !config) return;

    const effectiveSettings = effectiveContentTypeSettings(typeKey, config.settings);
    const nextValues: Record<string, ContentFieldValue> = { ...baseline.values };
    const nextCleared: string[] = [];

    for (const [fk, eff] of Object.entries(config.effective_fields)) {
      if (!eff.editable || !supportAllowsField(typeKey, fk, effectiveSettings)) {
        continue;
      }
      if (as.cleared_fields.includes(fk)) {
        delete nextValues[fk];
        nextCleared.push(fk);
      } else if (fk in as.values) {
        nextValues[fk] = as.values[fk];
      }
    }

    const nextTermIds = [...as.term_ids].sort((a, b) => a - b);
    let nextPrimary = baseline.primary_category_term_id;
    if (typeKey === 'post' && config.taxonomies.includes('category')) {
      const candidatePrimary = as.primary_category_touched
        ? as.primary_category_term_id
        : baseline.primary_category_term_id;
      nextPrimary =
        candidatePrimary !== null && nextTermIds.includes(candidatePrimary)
          ? candidatePrimary
          : null;
    }

    const nextPageLayoutKey = as.page_layout_touched
      ? as.page_layout_key
      : (baseline.page_layout_key ?? null);
    const nextCustomTemplateKey = as.custom_template_touched
      ? as.custom_template_key
      : (baseline.custom_template_key ?? null);
    const nextSelectedThemeKey =
      typeKey === 'page' ? nextPageLayoutKey : nextCustomTemplateKey;

    const nextSeo =
      canWriteSeoRef.current && as.seo_touched
        ? as.seo
          ? { ...as.seo }
          : EMPTY_SEO
        : baseline.seo;

    const merged: EditorDraftPayload = {
      ...baseline,
      version: baseline.version,
      title: as.title,
      content_mode: as.content_mode,
      values: nextValues,
      cleared_fields: nextCleared,
      term_ids: nextTermIds,
      primary_category_term_id: nextPrimary,
      page_layout_key: nextPageLayoutKey,
      custom_template_key: nextCustomTemplateKey,
      seo: nextSeo,
    };

    onApplyMergedAutosave(merged, nextSelectedThemeKey ?? null);
    setServerAutosave(null);
    onStatusNote(
      __(
        "Autosaved changes were loaded into your working draft. Review and save to create a new revision.",
      ),
    );
  }, [
    serverAutosave,
    baselineRef,
    config,
    typeKey,
    supportAllowsField,
    canWriteSeoRef,
    onApplyMergedAutosave,
    onStatusNote,
  ]);

  const handleDiscardServerAutosave = useCallback(async () => {
    if (!csrfToken || entryId === null || discardingAutosave) return;
    abortAndClearInflight();
    setDiscardingAutosave(true);
    try {
      await deleteEntryAutosave(typeKey, entryId, csrfToken);
      setServerAutosave(null);
      setHasServerAutosave(false);
      resetAutosaveState();
      onStatusNote(__("Server autosave discarded."));
    } catch (err) {
      onApiError(err, __("Could not discard the autosaved draft."));
    } finally {
      setDiscardingAutosave(false);
    }
  }, [
    csrfToken,
    entryId,
    discardingAutosave,
    abortAndClearInflight,
    typeKey,
    resetAutosaveState,
    onStatusNote,
    onApiError,
  ]);

  const handleOpenHistory = useCallback(
    (compare: RevisionModalInitialCompare = null) => {
      setInitialCompare(compare);
      setHistoryOpen(true);
    },
    [],
  );

  const handleCloseHistory = useCallback(() => {
    setHistoryOpen(false);
    setInitialCompare(null);
  }, []);

  const clearServerAutosaveState = useCallback(() => {
    setServerAutosave(null);
    setHasServerAutosave(false);
    resetAutosaveState();
  }, [resetAutosaveState]);

  const refreshRevisionCount = useCallback(async () => {
    if (entryId === null || isPattern || mode !== 'edit') return;
    try {
      const res = await listEntryRevisions(typeKey, entryId, {
        limit: 1,
        offset: 0,
      });
      setRevisionCount(res.total);
      setHasServerAutosave(res.has_autosave);
    } catch {
      // Non-fatal badge refresh after save/restore.
    }
  }, [typeKey, entryId, isPattern, mode]);

  return {
    revisionCount,
    hasServerAutosave,
    serverAutosave,
    hasNewerLocalDraft,
    bannerPendingDecision,
    discardingAutosave,
    historyOpen,
    initialCompare,
    autosaveState,
    abortAndClearInflight,
    handleApplyServerAutosave,
    handleDiscardServerAutosave,
    handleOpenHistory,
    handleCloseHistory,
    clearServerAutosaveState,
    refreshRevisionCount,
  };
}
