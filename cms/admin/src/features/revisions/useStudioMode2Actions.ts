import { useCallback, type MutableRefObject } from 'react';
import { __ } from '../../i18n/index.tsx';
import * as api from '../../api/index.ts';
import * as builderApi from '../../api/builder.ts';
import { changeContentEntryPublication } from '../../api/content.ts';
import { isRestoreEntryOutcome, type RestoreRevisionResponse } from '../../api/revisions.ts';
import type { PatternRecord } from '../../api/patterns.ts';
import {
  EMPTY_DOCUMENT,
  allNodes,
  isElement,
  visibleNodesInTree,
  type BuilderDocument,
  type BuilderElement,
  type StudioBuilderClipboard,
} from '../../builder/document.ts';
import type { StudioBuilderView } from '../../builder/BuilderEditorCore.tsx';
import type { StudioMenuRequest } from '../../studio/StudioBlockTree.tsx';
import type { AuthUser } from '../../types.ts';
import { editorDraftStore } from '../editor/EditorDraftStore.ts';
import type { RevisionModalInitialCompare } from './types.ts';

export function singleEditHashForEntry(entryTypeKey: string, entryId: number): string {
  if (entryTypeKey === 'page') return `#/pages/${entryId}/edit`;
  if (entryTypeKey === 'post') return `#/posts/${entryId}/edit`;
  if (entryTypeKey === 'pattern') return `#/patterns/${entryId}/edit`;
  return `#/cpt/${encodeURIComponent(entryTypeKey)}/${entryId}/edit`;
}

export interface UseStudioMode2ActionsOptions {
  active: boolean;
  isMode2: boolean;
  isPatternEntry: boolean;
  entryTypeKey: string;
  entryId: number | null;
  user: AuthUser | null;
  csrfToken: string | null;
  entryDirty: boolean;
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
  setEntryHasRetainedDraft: (retained: boolean) => void;
  onDirtyChange: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
  onApiError: (error: unknown, fallback: string) => void;
  abortAndClearInflight: () => void;
  discardServerAutosaveSafe: () => Promise<boolean>;
  clearServerAutosaveState: () => void;
  refreshRevisionCount: () => Promise<void>;
  setRestoredEditorModeNotice: (open: boolean) => void;
  setHistoryOpen: (open: boolean) => void;
  setInitialCompare: (compare: RevisionModalInitialCompare) => void;
}

interface RestoredSnapshotPayload {
  title: string;
  version: number;
  publicationStatus: 'draft' | 'published' | string;
  publicationPolicy: 'immediate' | 'draft_publish';
  diagnostics: builderApi.BlockDiagnostic[];
  hadFreeformWrap: boolean;
  hadRecovery: boolean;
  hadExistingDoc: boolean;
  doc: BuilderDocument;
  editorModeNotice: boolean;
  restoredFromRevNum: number;
}

export function useStudioMode2Actions(options: UseStudioMode2ActionsOptions) {
  const {
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
  } = options;

  const handleSaveMode2 = async (intent?: 'draft' | 'published') => {
    if (
      !active ||
      !user ||
      !csrfToken ||
      !isMode2 ||
      entryId === null ||
      entryVersion === null ||
      savingRef.current ||
      entryTrashedBlocked
    ) {
      return;
    }

    const refs = allNodes(entryDoc)
      .filter(
        node =>
          node.kind === 'element' &&
          isElement(node.node) &&
          node.node.type === 'core/pattern_ref',
      )
      .map(node => Number((node.node as BuilderElement).props.pattern_id));
    if (refs.some(id => !Number.isSafeInteger(id) || id <= 0) || new Set(refs).size > 16) {
      setStatus(
        __("Each reusable pattern requires a valid pattern (up to 16 patterns per page)."),
        true,
      );
      return;
    }
    if (patternsLoaded && refs.some(id => !patterns.some(p => p.id === id))) {
      setStatus(
        __("A referenced pattern was deleted. Select another pattern before saving."),
        true,
      );
      return;
    }

    abortAndClearInflight();
    savingRef.current = true;
    setSaving(true);
    setConflict(false);
    let builderSaved = false;
    try {
      let currentVersion = entryVersion;
      let currentStatus = entryPublicationStatus;

      if (entryDirty || (intent !== 'published' && entryHadExistingDoc === false)) {
        const saved = await builderApi.saveEntryBuilder(
          entryTypeKey,
          entryId,
          entryDoc,
          currentVersion,
          csrfToken,
        );
        builderSaved = true;
        editorDraftStore.clearDraft(entryTypeKey, entryId);
        currentVersion = saved.version;
        currentStatus = saved.publication_status;
        const currentDoc = saved.document ?? entryDoc;
        setEntryVersion(currentVersion);
        setEntrySavedDoc(currentDoc);
        setEntryDoc(currentDoc);
        setEntryHadExistingDoc(true);
        setEntryHadFreeformWrap(saved.had_freeform_wrap);
        setEntryHadRecovery(saved.had_recovery);
        setEntryDiagnostics(saved.diagnostics ?? entryDiagnostics);
      }

      if (
        intent === 'published' &&
        entryPublicationPolicy === 'draft_publish' &&
        currentStatus !== 'published'
      ) {
        const published = await changeContentEntryPublication(
          entryTypeKey,
          entryId,
          currentVersion,
          'published',
          csrfToken,
        );
        editorDraftStore.clearDraft(entryTypeKey, entryId);
        currentVersion = published.version;
        currentStatus = published.publication_status;
        setEntryVersion(currentVersion);
      }

      if (builderSaved) {
        setEntryHasRetainedDraft(false);
        setRestoredEditorModeNotice(false);
        clearServerAutosaveState();
        void refreshRevisionCount();
      }
      setEntryPublicationStatus(currentStatus);
      setStatus(
        isPatternEntry
          ? __("Saved pattern (v{{version}}).", { version: currentVersion })
          : entryPublicationPolicy === 'draft_publish' && currentStatus === 'draft'
            ? __(
                "Saved page builder draft (v{{version}}). Content remains a draft.",
                { version: currentVersion },
              )
            : __("Saved and published changes (v{{version}}).", {
                version: currentVersion,
              }),
      );
    } catch (cause) {
      if (isTrashedConflict(cause)) {
        setEntryTrashedBlocked(true);
        setStatus(
          __(
            "This entry is in Trash (409). Restore it from Trash before editing or saving in Studio.",
          ),
          true,
        );
      } else if (cause instanceof api.ApiClientError && cause.status === 412) {
        setConflict(true);
        setStatus(
          __("Content changed in another session (412). Your page builder draft is retained."),
          true,
        );
      } else {
        setStatus(__("Could not save the page builder; your draft is retained."), true);
        onApiError(cause, __("Cannot save the page builder."));
      }
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const handleReloadMode2 = async () => {
    if (!isMode2 || !entryTypeKey || entryId === null) return;
    if (
      entryDirty &&
      !window.confirm(
        __("Reloading the server version discards the current page builder draft. Continue?"),
      )
    ) {
      return;
    }
    abortAndClearInflight();
    if (entryDirty) {
      const ok = await discardServerAutosaveSafe();
      if (!ok) return;
    }
    mode2LoadedKeyRef.current = null;
    setConflict(false);
    const seq = ++loadSeqRef.current;
    setLoading(true);
    try {
      const opened = await builderApi.getEntryBuilder(entryTypeKey, entryId);
      if (seq !== loadSeqRef.current) return;
      const doc = opened.document ?? EMPTY_DOCUMENT;
      setEntryVersion(opened.version);
      setEntryPublicationStatus(opened.publication_status);
      setEntryPublicationPolicy(opened.publication_policy);
      setEntrySavedDoc(doc);
      setEntryHadExistingDoc(opened.document !== null);
      setEntryDoc(doc);
      setEntrySelectedNodeId(visibleNodesInTree(doc)[0]?.node.id ?? null);
      setEntryDiagnostics(opened.diagnostics ?? []);
      setEntryHadFreeformWrap(opened.had_freeform_wrap);
      setEntryHadRecovery(opened.had_recovery);
      setEntryHasRetainedDraft(false);
      setRestoredEditorModeNotice(false);
      if (entryDirty) clearServerAutosaveState();
      void refreshRevisionCount();
      resetHistory();
      setBuilderView('tree');
      setAddTargetParentId(null);
      setClipboard(null);
      setMenuRequest(null);
      setStatus(__("Loaded latest version v{{version}}.", { version: opened.version }));
    } catch (cause) {
      onApiError(cause, __("Cannot reload the page builder."));
    } finally {
      if (seq === loadSeqRef.current) setLoading(false);
    }
  };

  const handleExit = async () => {
    if (!isMode2 || entryId === null) {
      onNavigateHash('#/dashboard');
      return;
    }
    const target = singleEditHashForEntry(entryTypeKey, entryId);
    if (!entryDirty) {
      abortAndClearInflight();
      onNavigateHash(target);
      return;
    }
    if (
      !window.confirm(
        __("Studio customization has unsaved changes. Discard changes and exit?"),
      )
    ) {
      return;
    }
    abortAndClearInflight();
    const ok = await discardServerAutosaveSafe();
    if (!ok) return;
    setEntryDoc(entrySavedDoc);
    setEntryHasRetainedDraft(false);
    clearServerAutosaveState();
    onDirtyChange(false);
    onNavigateHash(target);
  };

  const applyRestoredSnapshot = useCallback(
    (payload: RestoredSnapshotPayload) => {
      if (entryId !== null) {
        editorDraftStore.clearDraft(entryTypeKey, entryId);
      }
      setEntryTitle(payload.title);
      setEntryVersion(payload.version);
      setEntryPublicationStatus(payload.publicationStatus);
      setEntryPublicationPolicy(payload.publicationPolicy);
      setEntryDiagnostics(payload.diagnostics);
      setEntryHadFreeformWrap(payload.hadFreeformWrap);
      setEntryHadRecovery(payload.hadRecovery);
      setEntryHadExistingDoc(payload.hadExistingDoc);
      setEntrySavedDoc(payload.doc);
      setEntryDoc(payload.doc);
      setEntrySelectedNodeId(visibleNodesInTree(payload.doc)[0]?.node.id ?? null);
      setConflict(false);
      setBuilderView('tree');
      setAddTargetParentId(null);
      setMenuRequest(null);
      resetHistory();
      setEntryHasRetainedDraft(false);
      setRestoredEditorModeNotice(payload.editorModeNotice);
      clearServerAutosaveState();
      setHistoryOpen(false);
      setInitialCompare(null);
      onDirtyChange(false);
      void refreshRevisionCount();
      setStatus(
        __("Restored snapshot #{{value1}} as new version v{{value2}}.", {
          value1: payload.restoredFromRevNum,
          value2: payload.version,
        }),
      );
    },
    [
      entryTypeKey,
      entryId,
      setEntryTitle,
      setEntryVersion,
      setEntryPublicationStatus,
      setEntryPublicationPolicy,
      setEntryDiagnostics,
      setEntryHadFreeformWrap,
      setEntryHadRecovery,
      setEntryHadExistingDoc,
      setEntrySavedDoc,
      setEntryDoc,
      setEntrySelectedNodeId,
      setConflict,
      setBuilderView,
      setAddTargetParentId,
      setMenuRequest,
      resetHistory,
      setEntryHasRetainedDraft,
      setRestoredEditorModeNotice,
      clearServerAutosaveState,
      setHistoryOpen,
      setInitialCompare,
      onDirtyChange,
      refreshRevisionCount,
      setStatus,
    ],
  );

  const handleRevisionRestored = async (
    res: RestoreRevisionResponse,
    restoredFromRevNum: number,
  ) => {
    if (!isMode2 || entryId === null) return;
    if (!isRestoreEntryOutcome(res)) {
      applyRestoredSnapshot({
        title: res.title,
        version: res.version,
        publicationStatus: res.publication_status,
        publicationPolicy: res.publication_policy,
        diagnostics: res.diagnostics ?? [],
        hadFreeformWrap: Boolean(res.had_freeform_wrap),
        hadRecovery: Boolean(res.had_recovery),
        hadExistingDoc: res.document !== null,
        doc: res.document ?? EMPTY_DOCUMENT,
        editorModeNotice: false,
        restoredFromRevNum,
      });
      return;
    }

    const seq = ++loadSeqRef.current;
    try {
      const opened = await builderApi.getEntryBuilder(entryTypeKey, entryId);
      if (seq !== loadSeqRef.current) return;
      applyRestoredSnapshot({
        title: res.entry.title,
        version: opened.version,
        publicationStatus: opened.publication_status,
        publicationPolicy: opened.publication_policy,
        diagnostics: opened.diagnostics ?? [],
        hadFreeformWrap: Boolean(opened.had_freeform_wrap),
        hadRecovery: Boolean(opened.had_recovery),
        hadExistingDoc: opened.document !== null,
        doc: opened.document ?? EMPTY_DOCUMENT,
        editorModeNotice: res.entry.content_mode === 'editor',
        restoredFromRevNum,
      });
    } catch (cause) {
      onApiError(cause, __("Cannot reload the page builder."));
    }
  };

  return {
    handleSaveMode2,
    handleReloadMode2: () => void handleReloadMode2(),
    handleExit: () => void handleExit(),
    handleRevisionRestored,
  };
}
