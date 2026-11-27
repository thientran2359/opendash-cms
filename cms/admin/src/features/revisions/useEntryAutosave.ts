import { useCallback, useEffect, useRef, useState } from 'react';
import { __ } from '../../i18n/index.tsx';
import { ApiClientError } from '../../api/client.ts';
import {
  upsertEntryAutosave,
  type AutosaveSurface,
  type AutosaveUpsertInput,
  type AutosaveUpsertResponse,
} from '../../api/revisions.ts';
import { editorDraftStore } from '../editor/EditorDraftStore.ts';
import type { AutosaveState } from './types.ts';

export const AUTOSAVE_DEBOUNCE_MS = 15_000;
export const AUTOSAVE_MAX_WAIT_MS = 60_000;
export const PAGEHIDE_KEEPALIVE_MAX_BYTES = 56 * 1024;

export interface UseEntryAutosaveOptions {
  enabled: boolean;
  typeKey: string;
  entryId: number | null;
  sourceSurface: AutosaveSurface;
  csrfToken: string | null;
  siteInstanceId: string;
  authGeneration: number;
  dirty: boolean;
  dirtyRevision: number;
  bannerPendingDecision: boolean;
  initialServerClientSeq?: number | null;
  buildPayload: () => Omit<AutosaveUpsertInput, 'source_surface' | 'client_seq'> | null;
  onApiError: (error: unknown, fallback: string) => void;
  onTrashedConflict: (message: string) => void;
  onSaved?: (response: AutosaveUpsertResponse) => void;
}

export interface UseEntryAutosaveResult {
  autosaveState: AutosaveState;
  abortAndClearInflight: () => void;
  resetAutosaveState: () => void;
}

const INITIAL_AUTOSAVE_STATE: AutosaveState = {
  status: 'idle',
  savedAt: null,
  isStaleBase: false,
  errorMessage: null,
};

export function useEntryAutosave(options: UseEntryAutosaveOptions): UseEntryAutosaveResult {
  const {
    enabled,
    typeKey,
    entryId,
    sourceSurface,
    csrfToken,
    siteInstanceId,
    authGeneration,
    dirty,
    dirtyRevision,
    bannerPendingDecision,
    initialServerClientSeq,
    buildPayload,
    onApiError,
    onTrashedConflict,
    onSaved,
  } = options;

  const [autosaveState, setAutosaveState] = useState<AutosaveState>(INITIAL_AUTOSAVE_STATE);

  const mountedRef = useRef(true);
  const enabledRef = useRef(enabled);
  const dirtyRef = useRef(dirty);
  const bannerPendingRef = useRef(bannerPendingDecision);
  const typeKeyRef = useRef(typeKey);
  const entryIdRef = useRef(entryId);
  const sourceSurfaceRef = useRef(sourceSurface);
  const csrfTokenRef = useRef(csrfToken);
  const buildPayloadRef = useRef(buildPayload);
  const onApiErrorRef = useRef(onApiError);
  const onTrashedConflictRef = useRef(onTrashedConflict);
  const onSavedRef = useRef(onSaved);

  enabledRef.current = enabled;
  dirtyRef.current = dirty;
  bannerPendingRef.current = bannerPendingDecision;
  typeKeyRef.current = typeKey;
  entryIdRef.current = entryId;
  sourceSurfaceRef.current = sourceSurface;
  csrfTokenRef.current = csrfToken;
  buildPayloadRef.current = buildPayload;
  onApiErrorRef.current = onApiError;
  onTrashedConflictRef.current = onTrashedConflict;
  onSavedRef.current = onSaved;

  const lastSeqRef = useRef(initialServerClientSeq ?? 0);
  if (typeof initialServerClientSeq === 'number' && initialServerClientSeq > lastSeqRef.current) {
    lastSeqRef.current = initialServerClientSeq;
  }

  const nextClientSeq = useCallback(() => {
    const next = Math.max(Date.now() * 1000, lastSeqRef.current + 1);
    lastSeqRef.current = next;
    return next;
  }, []);

  const debounceTimerRef = useRef<number | null>(null);
  const maxWaitTimerRef = useRef<number | null>(null);
  const inFlightControllerRef = useRef<AbortController | null>(null);
  const requestEpochRef = useRef(0);

  const clearTimers = useCallback(() => {
    if (debounceTimerRef.current !== null) {
      window.clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = null;
    }
    if (maxWaitTimerRef.current !== null) {
      window.clearTimeout(maxWaitTimerRef.current);
      maxWaitTimerRef.current = null;
    }
  }, []);

  const abortAndClearInflight = useCallback(() => {
    clearTimers();
    requestEpochRef.current += 1;
    if (inFlightControllerRef.current) {
      inFlightControllerRef.current.abort();
      inFlightControllerRef.current = null;
    }
    if (mountedRef.current) {
      setAutosaveState(prev =>
        prev.status === 'saving'
          ? { ...prev, status: prev.savedAt ? 'saved' : 'idle' }
          : prev,
      );
    }
  }, [clearTimers]);

  const resetAutosaveState = useCallback(() => {
    abortAndClearInflight();
    if (mountedRef.current) {
      setAutosaveState(INITIAL_AUTOSAVE_STATE);
    }
  }, [abortAndClearInflight]);

  const runAutosaveRef = useRef<() => Promise<void>>(async () => undefined);
  runAutosaveRef.current = async () => {
    clearTimers();
    if (
      !enabledRef.current ||
      !dirtyRef.current ||
      bannerPendingRef.current ||
      !csrfTokenRef.current ||
      entryIdRef.current === null
    ) {
      return;
    }
    const payload = buildPayloadRef.current();
    if (!payload) return;

    if (inFlightControllerRef.current) {
      inFlightControllerRef.current.abort();
    }
    const controller = new AbortController();
    inFlightControllerRef.current = controller;
    const myEpoch = ++requestEpochRef.current;
    const sessionEpoch = editorDraftStore.getSessionEpoch();
    const targetTypeKey = typeKeyRef.current;
    const targetEntryId = entryIdRef.current;
    const token = csrfTokenRef.current;
    const clientSeq = nextClientSeq();

    setAutosaveState(prev => ({ ...prev, status: 'saving', errorMessage: null }));

    try {
      const res = await upsertEntryAutosave(
        targetTypeKey,
        targetEntryId,
        {
          ...payload,
          source_surface: sourceSurfaceRef.current,
          client_seq: clientSeq,
        },
        token,
        { signal: controller.signal },
      );
      if (inFlightControllerRef.current === controller) {
        inFlightControllerRef.current = null;
      }
      if (
        !mountedRef.current ||
        myEpoch !== requestEpochRef.current ||
        sessionEpoch !== editorDraftStore.getSessionEpoch()
      ) {
        return;
      }
      if (res.applied) {
        lastSeqRef.current = Math.max(lastSeqRef.current, res.client_seq);
        setAutosaveState({
          status: 'saved',
          savedAt: res.saved_at,
          isStaleBase: res.is_stale_base,
          errorMessage: null,
        });
        onSavedRef.current?.(res);
      } else {
        setAutosaveState(prev => ({
          ...prev,
          status: prev.savedAt ? 'saved' : 'idle',
        }));
      }
    } catch (err) {
      if (inFlightControllerRef.current === controller) {
        inFlightControllerRef.current = null;
      }
      if (
        controller.signal.aborted ||
        (err instanceof DOMException && err.name === 'AbortError')
      ) {
        return;
      }
      if (
        !mountedRef.current ||
        myEpoch !== requestEpochRef.current ||
        sessionEpoch !== editorDraftStore.getSessionEpoch()
      ) {
        return;
      }
      if (err instanceof ApiClientError && err.status === 401) {
        setAutosaveState(prev => ({
          ...prev,
          status: 'error',
          errorMessage: err.message,
        }));
        onApiErrorRef.current(err, __("Your session expired while autosaving."));
        return;
      }
      if (
        err instanceof ApiClientError &&
        err.status === 409 &&
        (err.errorCode === 'entry.trashed' ||
          err.bodyCode === 'entry_trashed' ||
          err.message.includes('entry_trashed'))
      ) {
        setAutosaveState(prev => ({
          ...prev,
          status: 'error',
          errorMessage: err.message,
        }));
        onTrashedConflictRef.current(err.message);
        return;
      }
      setAutosaveState(prev => ({
        ...prev,
        status: 'error',
        errorMessage: err instanceof Error ? err.message : __("Autosave failed."),
      }));
    }
  };

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      abortAndClearInflight();
    };
  }, [abortAndClearInflight]);

  useEffect(() => {
    return editorDraftStore.subscribe(() => {
      abortAndClearInflight();
    });
  }, [abortAndClearInflight]);

  useEffect(() => {
    abortAndClearInflight();
    setAutosaveState(INITIAL_AUTOSAVE_STATE);
    return () => {
      abortAndClearInflight();
    };
  }, [typeKey, entryId, siteInstanceId, authGeneration, csrfToken, abortAndClearInflight]);

  useEffect(() => {
    if (!enabled || !dirty || bannerPendingDecision || !csrfToken || entryId === null) {
      clearTimers();
      return;
    }
    if (debounceTimerRef.current !== null) {
      window.clearTimeout(debounceTimerRef.current);
    }
    debounceTimerRef.current = window.setTimeout(() => {
      void runAutosaveRef.current();
    }, AUTOSAVE_DEBOUNCE_MS);
    if (maxWaitTimerRef.current === null) {
      maxWaitTimerRef.current = window.setTimeout(() => {
        void runAutosaveRef.current();
      }, AUTOSAVE_MAX_WAIT_MS);
    }
  }, [enabled, dirty, dirtyRevision, bannerPendingDecision, csrfToken, entryId, clearTimers]);

  useEffect(() => {
    const onPageHide = () => {
      if (
        !enabledRef.current ||
        !dirtyRef.current ||
        bannerPendingRef.current ||
        !csrfTokenRef.current ||
        entryIdRef.current === null ||
        inFlightControllerRef.current !== null
      ) {
        return;
      }
      const payload = buildPayloadRef.current();
      if (!payload) return;
      clearTimers();
      const input: AutosaveUpsertInput = {
        ...payload,
        source_surface: sourceSurfaceRef.current,
        client_seq: nextClientSeq(),
      };
      const bytes = new TextEncoder().encode(JSON.stringify(input)).byteLength;
      if (bytes > PAGEHIDE_KEEPALIVE_MAX_BYTES) return;
      void upsertEntryAutosave(
        typeKeyRef.current,
        entryIdRef.current,
        input,
        csrfTokenRef.current,
        { keepalive: true },
      ).catch(() => undefined);
    };
    window.addEventListener('pagehide', onPageHide);
    return () => {
      window.removeEventListener('pagehide', onPageHide);
    };
  }, [clearTimers, nextClientSeq]);

  return {
    autosaveState,
    abortAndClearInflight,
    resetAutosaveState,
  };
}
