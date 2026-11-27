import { useCallback, useEffect, useState, type MutableRefObject } from 'react';
import { __ } from '../../i18n/index.tsx';
import * as api from '../../api/index.ts';
import type * as builderApi from '../../api/builder.ts';
import {
  deleteEntryAutosave,
  getEntryAutosave,
  listEntryRevisions,
  type AutosaveSnapshotDto,
} from '../../api/revisions.ts';
import type { PatternRecord } from '../../api/patterns.ts';
import {
  visibleNodesInTree,
  type BuilderDocument,
  type StudioBuilderClipboard,
} from '../../builder/document.ts';
import type { StudioBuilderView } from '../../builder/BuilderEditorCore.tsx';
import type { StudioMenuRequest } from '../../studio/StudioBlockTree.tsx';
import type { AuthUser } from '../../types.ts';
import { editorDraftStore } from '../editor/EditorDraftStore.ts';
import type { RevisionModalInitialCompare } from './types.ts';
import { useEntryAutosave } from './useEntryAutosave.ts';
import { singleEditHashForEntry, useStudioMode2Actions } from './useStudioMode2Actions.ts';

export { singleEditHashForEntry };

export interface UseStudioMode2RevisionsOptions {
  active: boolean;
  isMode2: boolean;
  isPatternEntry: boolean;
  entryTypeKey: string;
  entryId: number | null;
  user: AuthUser | null;
  csrfToken: string | null;
  siteInstanceId: string;
  authGeneration: number;
  entryDirty: boolean;
  entryDocRevision: number;
  entryHasRetainedDraft: boolean;
  entryVersion: number | null;
  entryPublicationStatus: 'draft' | 'published' | string;
  entryPublicationPolicy: 'immediate' | 'draft_publish';
  entryHadExistingDoc: boolean;
  entryTrashedBlocked: boolean;
  entryDoc: BuilderDocument;
  entrySavedDoc: BuilderDocument;
  entryDiagnostics: builderApi.BlockDiagnostic[];
  patterns: PatternRecord[];
  patternsLoaded: boolean;
  savingRef: MutableRefObject<boolean>;
  loadSeqRef: MutableRefObject<number>;
  mode2LoadedKeyRef: MutableRefObject<string | null>;
  isTrashedConflict: (cause: unknown) => boolean;
  resetHistory: () => void;
  setLoading: (loading: boolean) => void;
  setSaving: (saving: boolean) => void;
  setConflict: (conflict: boolean) => void;
  setStatus: (text: string, failed?: boolean) => void;
  setBuilderView: (view: StudioBuilderView) => void;
  setAddTargetParentId: (id: string | null) => void;
  setClipboard: (clip: StudioBuilderClipboard | null) => void;
  setMenuRequest: (req: StudioMenuRequest | null) => void;
  setEntryTitle: (title: string) => void;
  setEntryVersion: (version: number | null) => void;
  setEntryPublicationStatus: (status: 'draft' | 'published' | string) => void;
  setEntryPublicationPolicy: (policy: 'immediate' | 'draft_publish') => void;
  setEntryDiagnostics: (diagnostics: builderApi.BlockDiagnostic[]) => void;
  setEntryHadFreeformWrap: (flag: boolean) => void;
  setEntryHadRecovery: (flag: boolean) => void;
  setEntrySavedDoc: (doc: BuilderDocument) => void;
  setEntryHadExistingDoc: (flag: boolean) => void;
  setEntryDoc: (doc: BuilderDocument) => void;
  setEntrySelectedNodeId: (id: string | null) => void;
  setEntryTrashedBlocked: (blocked: boolean) => void;
  setEntryDocRevision: (updater: (prev: number) => number) => void;
  setEntryHasRetainedDraft: (retained: boolean) => void;
  onDirtyChange: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
  onApiError: (error: unknown, fallback: string) => void;
}

export function useStudioMode2Revisions(options: UseStudioMode2RevisionsOptions) {
  const {
    active,
    isMode2,
    isPatternEntry,
    entryTypeKey,
    entryId,
    user,
    csrfToken,
    siteInstanceId,
    authGeneration,
    entryDirty,
    entryDocRevision,
    entryHasRetainedDraft,
    entryVersion,
    entryPublicationStatus,
    entryPublicationPolicy,
    entryHadExistingDoc,
    entryTrashedBlocked,
    entryDoc,
    entrySavedDoc,
    entryDiagnostics,
    patterns,
    patternsLoaded,
    savingRef,
    loadSeqRef,
    mode2LoadedKeyRef,
    isTrashedConflict,
    resetHistory,
    setLoading,
    setSaving,
    setConflict,
    setStatus,
    setBuilderView,
    setAddTargetParentId,
    setClipboard,
    setMenuRequest,
    setEntryTitle,
    setEntryVersion,
    setEntryPublicationStatus,
    setEntryPublicationPolicy,
    setEntryDiagnostics,
    setEntryHadFreeformWrap,
    setEntryHadRecovery,
    setEntrySavedDoc,
    setEntryHadExistingDoc,
    setEntryDoc,
    setEntrySelectedNodeId,
    setEntryTrashedBlocked,
    setEntryDocRevision,
    setEntryHasRetainedDraft,
    onDirtyChange,
    onNavigateHash,
    onApiError,
  } = options;

  const [revisionCount, setRevisionCount] = useState<number | null>(null);
  const [serverAutosave, setServerAutosave] = useState<AutosaveSnapshotDto | null>(null);
  const [discardingAutosave, setDiscardingAutosave] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [initialCompare, setInitialCompare] =
    useState<RevisionModalInitialCompare>(null);
  const [restoredEditorModeNotice, setRestoredEditorModeNotice] = useState(false);

  const enabled =
    isMode2 &&
    active &&
    Boolean(user) &&
    Boolean(csrfToken) &&
    entryId !== null &&
    entryVersion !== null &&
    !entryTrashedBlocked;

  useEffect(() => {
    if (!enabled || entryId === null) {
      setRevisionCount(null);
      setServerAutosave(null);
      setHistoryOpen(false);
      setInitialCompare(null);
      if (!isMode2) setRestoredEditorModeNotice(false);
      return;
    }

    const controller = new AbortController();
    const reqEpoch = editorDraftStore.getSessionEpoch();
    let alive = true;

    void Promise.all([
      listEntryRevisions(entryTypeKey, entryId, {
        limit: 1,
        offset: 0,
        signal: controller.signal,
      }),
      getEntryAutosave(entryTypeKey, entryId, controller.signal),
    ])
      .then(([revRes, asRes]) => {
        if (
          !alive ||
          controller.signal.aborted ||
          reqEpoch !== editorDraftStore.getSessionEpoch()
        ) {
          return;
        }
        setRevisionCount(revRes.total);
        setServerAutosave(asRes.autosave);
      })
      .catch(cause => {
        if (
          !alive ||
          controller.signal.aborted ||
          (cause instanceof DOMException && cause.name === 'AbortError')
        ) {
          return;
        }
        if (cause instanceof api.ApiClientError && cause.status === 401) {
          onApiError(cause, __("Your session expired while checking revision history."));
        }
      });

    return () => {
      alive = false;
      controller.abort();
    };
  }, [
    enabled,
    isMode2,
    entryTypeKey,
    entryId,
    user?.id,
    user?.role,
    siteInstanceId,
    authGeneration,
    onApiError,
  ]);

  const hasNewerLocalDraft = Boolean(serverAutosave && entryHasRetainedDraft);
  const bannerPendingDecision = Boolean(serverAutosave && !hasNewerLocalDraft);

  const buildPayload = useCallback(() => {
    if (entryVersion === null) return null;
    return {
      base_version: entryVersion,
      builder_document: entryDoc,
    };
  }, [entryVersion, entryDoc]);

  const handleAutosaveTrashed = useCallback(
    (_message: string) => {
      setEntryTrashedBlocked(true);
      setStatus(
        __(
          "This entry is in Trash (409). Restore it from Trash before editing or saving in Studio.",
        ),
        true,
      );
    },
    [setEntryTrashedBlocked, setStatus],
  );

  const { autosaveState, abortAndClearInflight, resetAutosaveState } =
    useEntryAutosave({
      enabled,
      typeKey: entryTypeKey,
      entryId,
      sourceSurface: 'studio',
      csrfToken,
      siteInstanceId,
      authGeneration,
      dirty: entryDirty,
      dirtyRevision: entryDocRevision,
      bannerPendingDecision,
      initialServerClientSeq: serverAutosave?.client_seq,
      buildPayload,
      onApiError,
      onTrashedConflict: handleAutosaveTrashed,
    });

  const clearServerAutosaveState = useCallback(() => {
    setServerAutosave(null);
    resetAutosaveState();
  }, [resetAutosaveState]);

  const refreshRevisionCount = useCallback(async () => {
    if (!isMode2 || entryId === null) return;
    try {
      const res = await listEntryRevisions(entryTypeKey, entryId, {
        limit: 1,
        offset: 0,
      });
      setRevisionCount(res.total);
    } catch {
      // Non-fatal revision count refresh after save/restore.
    }
  }, [isMode2, entryTypeKey, entryId]);

  const handleApplyServerAutosave = useCallback(() => {
    const doc = serverAutosave?.builder_document;
    if (!doc) return;
    setEntryDoc(doc);
    setEntrySelectedNodeId(visibleNodesInTree(doc)[0]?.node.id ?? null);
    resetHistory();
    setEntryDocRevision(v => v + 1);
    setServerAutosave(null);
    setStatus(
      __(
        "Loaded autosaved Visual Builder layout. Review and save to create a new revision.",
      ),
    );
  }, [
    serverAutosave,
    setEntryDoc,
    setEntrySelectedNodeId,
    resetHistory,
    setEntryDocRevision,
    setStatus,
  ]);

  const discardServerAutosaveSafe = useCallback(async (): Promise<boolean> => {
    if (!csrfToken || entryId === null) return true;
    try {
      await deleteEntryAutosave(entryTypeKey, entryId, csrfToken);
      return true;
    } catch (cause) {
      if (cause instanceof api.ApiClientError && cause.status === 404) {
        return true;
      }
      if (isTrashedConflict(cause)) {
        setEntryTrashedBlocked(true);
        setStatus(
          __(
            "This entry is in Trash (409). Restore it from Trash before editing or saving in Studio.",
          ),
          true,
        );
        return false;
      }
      onApiError(cause, __("Could not discard the autosaved draft."));
      return !(cause instanceof api.ApiClientError && cause.status === 401);
    }
  }, [
    csrfToken,
    entryTypeKey,
    entryId,
    isTrashedConflict,
    setEntryTrashedBlocked,
    setStatus,
    onApiError,
  ]);

  const handleDiscardServerAutosave = useCallback(async () => {
    if (!csrfToken || entryId === null || discardingAutosave) return;
    abortAndClearInflight();
    setDiscardingAutosave(true);
    try {
      const ok = await discardServerAutosaveSafe();
      if (ok) {
        clearServerAutosaveState();
        setStatus(__("Server autosave discarded."));
      }
    } finally {
      setDiscardingAutosave(false);
    }
  }, [
    csrfToken,
    entryId,
    discardingAutosave,
    abortAndClearInflight,
    discardServerAutosaveSafe,
    clearServerAutosaveState,
    setStatus,
  ]);

  const actions = useStudioMode2Actions({
    active,
    isMode2,
    isPatternEntry,
    entryTypeKey,
    entryId,
    user,
    csrfToken,
    entryDirty,
    entryVersion,
    entryPublicationStatus,
    entryPublicationPolicy,
    entryHadExistingDoc,
    entryTrashedBlocked,
    entryDoc,
    entrySavedDoc,
    entryDiagnostics,
    patterns,
    patternsLoaded,
    savingRef,
    loadSeqRef,
    mode2LoadedKeyRef,
    isTrashedConflict,
    resetHistory,
    setLoading,
    setSaving,
    setConflict,
    setStatus,
    setBuilderView,
    setAddTargetParentId,
    setClipboard,
    setMenuRequest,
    setEntryTitle,
    setEntryVersion,
    setEntryPublicationStatus,
    setEntryPublicationPolicy,
    setEntryDiagnostics,
    setEntryHadFreeformWrap,
    setEntryHadRecovery,
    setEntrySavedDoc,
    setEntryHadExistingDoc,
    setEntryDoc,
    setEntrySelectedNodeId,
    setEntryTrashedBlocked,
    setEntryHasRetainedDraft,
    onDirtyChange,
    onNavigateHash,
    onApiError,
    abortAndClearInflight,
    discardServerAutosaveSafe,
    clearServerAutosaveState,
    refreshRevisionCount,
    setRestoredEditorModeNotice,
    setHistoryOpen,
    setInitialCompare,
  });

  return {
    autosaveState,
    revisionCount,
    serverAutosave,
    hasNewerLocalDraft,
    discardingAutosave,
    historyOpen,
    initialCompare,
    restoredEditorModeNotice,
    singleEditHash:
      isMode2 && entryId !== null ? singleEditHashForEntry(entryTypeKey, entryId) : '#/dashboard',
    dismissEditorModeNotice: () => setRestoredEditorModeNotice(false),
    abortAndClearInflight,
    handleApplyServerAutosave,
    handleDiscardServerAutosave: () => void handleDiscardServerAutosave(),
    handleOpenHistory: (compare: RevisionModalInitialCompare = null) => {
      setInitialCompare(compare);
      setHistoryOpen(true);
    },
    handleCloseHistory: () => {
      setHistoryOpen(false);
      setInitialCompare(null);
    },
    ...actions,
  };
}
