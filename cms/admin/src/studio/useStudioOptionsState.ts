import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ApiClientError } from '../api/client.ts';
import {
  getOptionsPageValues,
  putOptionsPageValues,
  type OptionsPageValuesSnapshot,
} from '../api/optionsPages.ts';
import {
  getThemeOptions,
  saveThemeOptions,
  type ThemeOptionsSnapshot,
} from '../api/themes.ts';
import { isUiOnlyFieldType } from '../fields/helpers.ts';
import {
  buildOwnerOptionsSubmitPayload,
  normalizeOwnerValuesForFields,
  sameOwnerOptionValues,
} from '../features/shared/OwnerOptionsForm.tsx';
import { __ } from '../i18n/index.tsx';
import type { AuthUser } from '../types.ts';
import { stripNonPublicPreviewValues } from './studioOptionsSections.ts';

function sameJson(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

interface Params {
  active: boolean;
  isMode2: boolean;
  user: AuthUser | null;
  csrfToken: string | null;
  activeThemeId: string | null;
  effectivePreviewLocale: string;
  defaultLocaleCode: string;
  hydratedPreviewLocale: string | null;
  localeDraftBlocked: boolean;
  mode1ReloadSeq: number;
  themeRevision: number;
  onApiError: (error: unknown, fallback: string) => void;
}

export interface StudioOptionsState {
  themeSnapshot: ThemeOptionsSnapshot | null;
  themeDraft: Record<string, unknown> | null;
  themeLoading: boolean;
  themeError: string;
  themeDirty: boolean;
  updateThemeDraft: (partial: Record<string, unknown>) => void;
  snapshotsByRef: Record<string, OptionsPageValuesSnapshot>;
  draftsByRef: Record<string, Record<string, unknown>>;
  loadingRefs: Record<string, boolean>;
  errorsByRef: Record<string, string>;
  notFoundRefs: Record<string, boolean>;
  ensurePageLoaded: (pageRef: string) => void;
  updateOptionsPageDraft: (pageRef: string, partial: Record<string, unknown>) => void;
  isOptionsPageDirty: (pageRef: string) => boolean;
  isOptionsGroupDirty: (pageRef: string, groupId: string) => boolean;
  anyOptionsPagesDirty: boolean;
  buildTransientOptionsPages: () => Record<string, Record<string, unknown>> | undefined;
  saveThemeOptionsIfDirty: (csrfToken: string, locale?: string) => Promise<boolean>;
  saveDirtyOptionsPages: (csrfToken: string) => Promise<string[]>;
  resetAll: () => void;
}

function isFieldChangedInPage(
  snap: OptionsPageValuesSnapshot,
  draft: Record<string, unknown>,
  readOnly: Set<string>,
  spec: { key: string; type: string; translatable?: boolean },
): boolean {
  if (isUiOnlyFieldType(spec.type as never) || readOnly.has(spec.key)) return false;
  if (!snap.is_default_locale && !spec.translatable) return false;
  if (spec.type === 'secret') {
    const val = draft[spec.key];
    return (typeof val === 'string' && val.length > 0)
      || Boolean(val && typeof val === 'object' && (val as { clear?: unknown }).clear === true);
  }
  return !sameJson(draft[spec.key], snap.values[spec.key]);
}

export function useStudioOptionsState({
  active,
  isMode2,
  user,
  csrfToken,
  activeThemeId,
  effectivePreviewLocale,
  defaultLocaleCode,
  hydratedPreviewLocale,
  localeDraftBlocked,
  mode1ReloadSeq,
  themeRevision,
  onApiError,
}: Params): StudioOptionsState {
  const [themeSnapshot, setThemeSnapshot] = useState<ThemeOptionsSnapshot | null>(null);
  const [themeDraft, setThemeDraft] = useState<Record<string, unknown> | null>(null);
  const [themeLoading, setThemeLoading] = useState(false);
  const [themeError, setThemeError] = useState('');

  const [snapshotsByRef, setSnapshotsByRef] = useState<Record<string, OptionsPageValuesSnapshot>>({});
  const [draftsByRef, setDraftsByRef] = useState<Record<string, Record<string, unknown>>>({});
  const [loadingRefs, setLoadingRefs] = useState<Record<string, boolean>>({});
  const [errorsByRef, setErrorsByRef] = useState<Record<string, string>>({});
  const [notFoundRefs, setNotFoundRefs] = useState<Record<string, boolean>>({});

  const seqRef = useRef(0);
  const fetchedRefsRef = useRef<Set<string>>(new Set());
  const onApiErrorRef = useRef(onApiError);
  onApiErrorRef.current = onApiError;

  const canLoad = Boolean(
    active && !isMode2 && user && csrfToken && effectivePreviewLocale
    && hydratedPreviewLocale === effectivePreviewLocale && !localeDraftBlocked,
  );

  const resetAll = useCallback(() => {
    seqRef.current += 1;
    fetchedRefsRef.current = new Set();
    setThemeSnapshot(null);
    setThemeDraft(null);
    setThemeLoading(false);
    setThemeError('');
    setSnapshotsByRef({});
    setDraftsByRef({});
    setLoadingRefs({});
    setErrorsByRef({});
    setNotFoundRefs({});
  }, []);

  const loadPageRef = useCallback((pageRef: string, force = false) => {
    if (!canLoad || !pageRef) return;
    if (!force && fetchedRefsRef.current.has(pageRef)) return;
    fetchedRefsRef.current.add(pageRef);
    const reqSeq = seqRef.current;
    setLoadingRefs(prev => ({ ...prev, [pageRef]: true }));
    setErrorsByRef(prev => ({ ...prev, [pageRef]: '' }));
    setNotFoundRefs(prev => ({ ...prev, [pageRef]: false }));
    void getOptionsPageValues(pageRef, effectivePreviewLocale || undefined)
      .then(snap => {
        if (seqRef.current !== reqSeq) return;
        setSnapshotsByRef(prev => ({ ...prev, [pageRef]: snap }));
        setDraftsByRef(prev => ({ ...prev, [pageRef]: { ...snap.values } }));
      })
      .catch(cause => {
        if (seqRef.current !== reqSeq) return;
        if (cause instanceof ApiClientError && cause.status === 404) {
          setNotFoundRefs(prev => ({ ...prev, [pageRef]: true }));
          return;
        }
        fetchedRefsRef.current.delete(pageRef);
        const msg = cause instanceof Error ? cause.message : __("Could not load options page values.");
        setErrorsByRef(prev => ({ ...prev, [pageRef]: msg }));
        onApiErrorRef.current(cause, __("Could not load options page values."));
      })
      .finally(() => {
        if (seqRef.current !== reqSeq) return;
        setLoadingRefs(prev => ({ ...prev, [pageRef]: false }));
      });
  }, [canLoad, effectivePreviewLocale]);

  // Hydrate theme options and extra attached site_global/theme:<id> field groups with Mode 1.
  useEffect(() => {
    if (!canLoad) return;
    const reqSeq = ++seqRef.current;
    fetchedRefsRef.current = new Set();
    setSnapshotsByRef({});
    setDraftsByRef({});
    setErrorsByRef({});
    setNotFoundRefs({});

    if (!activeThemeId) {
      setThemeSnapshot(null);
      setThemeDraft(null);
      setThemeLoading(false);
      setThemeError('');
    } else {
      setThemeLoading(true);
      setThemeError('');
      void getThemeOptions(activeThemeId, effectivePreviewLocale || undefined)
        .then(res => {
          if (seqRef.current !== reqSeq) return;
          setThemeSnapshot(res);
          setThemeDraft({ ...res.values });
        })
        .catch(cause => {
          if (seqRef.current !== reqSeq) return;
          if (cause instanceof ApiClientError && cause.status === 404) {
            setThemeSnapshot(null);
            setThemeDraft(null);
            return;
          }
          setThemeError(cause instanceof Error ? cause.message : __("Could not load theme options."));
          onApiErrorRef.current(cause, __("Could not load theme options."));
        })
        .finally(() => {
          if (seqRef.current === reqSeq) setThemeLoading(false);
        });
    }

    const coreRefs = activeThemeId ? ['site_global', `theme:${activeThemeId}`] : ['site_global'];
    for (const ref of coreRefs) {
      fetchedRefsRef.current.add(ref);
      setLoadingRefs(prev => ({ ...prev, [ref]: true }));
      void getOptionsPageValues(ref, effectivePreviewLocale || undefined)
        .then(snap => {
          if (seqRef.current !== reqSeq) return;
          setSnapshotsByRef(prev => ({ ...prev, [ref]: snap }));
          setDraftsByRef(prev => ({ ...prev, [ref]: { ...snap.values } }));
        })
        .catch(() => {
          // Non-fatal: site_global/theme:<id> may simply have no attached extra groups.
        })
        .finally(() => {
          if (seqRef.current === reqSeq) setLoadingRefs(prev => ({ ...prev, [ref]: false }));
        });
    }
  }, [canLoad, activeThemeId, effectivePreviewLocale, mode1ReloadSeq, themeRevision]);

  const updateThemeDraft = useCallback((partial: Record<string, unknown>) => {
    if (localeDraftBlocked || !hydratedPreviewLocale || hydratedPreviewLocale !== effectivePreviewLocale) return;
    setThemeDraft(prev => {
      const merged = { ...(prev ?? themeSnapshot?.values ?? {}), ...partial };
      return themeSnapshot
        ? normalizeOwnerValuesForFields(merged, themeSnapshot.fields, themeSnapshot.values)
        : merged;
    });
  }, [localeDraftBlocked, hydratedPreviewLocale, effectivePreviewLocale, themeSnapshot]);

  const updateOptionsPageDraft = useCallback((pageRef: string, partial: Record<string, unknown>) => {
    if (localeDraftBlocked || !hydratedPreviewLocale || hydratedPreviewLocale !== effectivePreviewLocale) return;
    setDraftsByRef(prev => ({
      ...prev,
      [pageRef]: { ...(prev[pageRef] ?? snapshotsByRef[pageRef]?.values ?? {}), ...partial },
    }));
  }, [localeDraftBlocked, hydratedPreviewLocale, effectivePreviewLocale, snapshotsByRef]);

  const themeDirty = useMemo(() => {
    if (!themeSnapshot || !themeDraft) return false;
    return !sameOwnerOptionValues(themeDraft, themeSnapshot.values);
  }, [themeSnapshot, themeDraft]);

  const isOptionsGroupDirty = useCallback((pageRef: string, groupId: string): boolean => {
    const snap = snapshotsByRef[pageRef];
    const draft = draftsByRef[pageRef];
    if (!snap || !draft) return false;
    const grp = snap.groups.find(g => g.group_id === groupId);
    if (!grp) return false;
    const readOnly = new Set(grp.read_only_keys);
    return grp.fields.some(spec => isFieldChangedInPage(snap, draft, readOnly, spec));
  }, [snapshotsByRef, draftsByRef]);

  const isOptionsPageDirty = useCallback((pageRef: string): boolean => {
    const snap = snapshotsByRef[pageRef];
    const draft = draftsByRef[pageRef];
    if (!snap || !draft) return false;
    return snap.groups.some(grp => {
      const readOnly = new Set(grp.read_only_keys);
      return grp.fields.some(spec => isFieldChangedInPage(snap, draft, readOnly, spec));
    });
  }, [snapshotsByRef, draftsByRef]);

  const anyOptionsPagesDirty = useMemo(
    () => Object.keys(snapshotsByRef).some(pageRef => isOptionsPageDirty(pageRef)),
    [snapshotsByRef, isOptionsPageDirty],
  );

  const buildTransientOptionsPages = useCallback((): Record<string, Record<string, unknown>> | undefined => {
    const result: Record<string, Record<string, unknown>> = {};
    for (const [pageRef, snap] of Object.entries(snapshotsByRef)) {
      const draft = draftsByRef[pageRef];
      if (!draft) continue;
      if (
        pageRef.startsWith('custom:') &&
        (snap.page?.provider_status !== 'active' || snap.page?.declaration_state !== 'declared')
      ) {
        continue;
      }
      const pageChanges: Record<string, unknown> = {};
      for (const grp of snap.groups) {
        if (grp.public_previewable === false) continue;
        const readOnly = new Set(grp.read_only_keys);
        const changedRaw: Record<string, unknown> = {};
        for (const spec of grp.fields) {
          if (isUiOnlyFieldType(spec.type) || readOnly.has(spec.key) || spec.type === 'secret') continue;
          if (!snap.is_default_locale && !spec.translatable) continue;
          if (!sameJson(draft[spec.key], snap.values[spec.key])) {
            changedRaw[spec.key] = draft[spec.key];
          }
        }
        const safe = stripNonPublicPreviewValues(grp.fields, changedRaw, true);
        Object.assign(pageChanges, safe);
      }
      if (Object.keys(pageChanges).length > 0) {
        result[pageRef] = pageChanges;
      }
    }
    return Object.keys(result).length > 0 ? result : undefined;
  }, [snapshotsByRef, draftsByRef]);

  const saveThemeOptionsIfDirty = useCallback(async (token: string, locale?: string): Promise<boolean> => {
    if (!themeDirty || !activeThemeId || !themeSnapshot || !themeDraft) return false;
    const isSecondary = Boolean(locale && defaultLocaleCode && locale !== defaultLocaleCode);
    const keysFilter = isSecondary
      ? new Set(themeSnapshot.fields.filter(f => !isUiOnlyFieldType(f.type) && Boolean(f.translatable)).map(f => f.key))
      : undefined;
    const payload = buildOwnerOptionsSubmitPayload(themeSnapshot.fields, themeDraft, keysFilter);
    const saved = await saveThemeOptions(activeThemeId, payload, themeSnapshot.version, token, locale);
    setThemeSnapshot(saved);
    setThemeDraft({ ...saved.values });
    return true;
  }, [themeDirty, activeThemeId, themeSnapshot, themeDraft, defaultLocaleCode]);

  const saveDirtyOptionsPages = useCallback(async (token: string): Promise<string[]> => {
    const savedLabels: string[] = [];
    for (const [pageRef, snap] of Object.entries(snapshotsByRef)) {
      const draft = draftsByRef[pageRef];
      if (!draft) continue;
      const patch: Record<string, unknown> = {};
      for (const grp of snap.groups) {
        const readOnly = new Set(grp.read_only_keys);
        for (const spec of grp.fields) {
          if (isUiOnlyFieldType(spec.type) || readOnly.has(spec.key)) continue;
          if (!snap.is_default_locale && !spec.translatable) continue;
          if (spec.type === 'secret') {
            if (typeof draft[spec.key] === 'string' && (draft[spec.key] as string).length > 0) {
              patch[spec.key] = draft[spec.key];
            } else if (
              draft[spec.key] && typeof draft[spec.key] === 'object'
              && (draft[spec.key] as { clear?: unknown }).clear === true
            ) {
              patch[spec.key] = { clear: true };
            }
            continue;
          }
          if (!sameJson(draft[spec.key], snap.values[spec.key])) {
            patch[spec.key] = draft[spec.key] ?? null;
          }
        }
      }
      if (Object.keys(patch).length === 0) continue;
      const saved = await putOptionsPageValues(
        pageRef,
        snap.version,
        { values: patch, expected_schema_token: snap.schema_token },
        token,
        snap.locale,
      );
      setSnapshotsByRef(prev => ({ ...prev, [pageRef]: saved }));
      setDraftsByRef(prev => ({ ...prev, [pageRef]: { ...saved.values } }));
      savedLabels.push(saved.page?.title ?? (pageRef === 'site_global' ? __("Global site options") : __("Theme options")));
    }
    return savedLabels;
  }, [snapshotsByRef, draftsByRef]);

  return {
    themeSnapshot,
    themeDraft,
    themeLoading,
    themeError,
    themeDirty,
    updateThemeDraft,
    snapshotsByRef,
    draftsByRef,
    loadingRefs,
    errorsByRef,
    notFoundRefs,
    ensurePageLoaded: loadPageRef,
    updateOptionsPageDraft,
    isOptionsPageDirty,
    isOptionsGroupDirty,
    anyOptionsPagesDirty,
    buildTransientOptionsPages,
    saveThemeOptionsIfDirty,
    saveDirtyOptionsPages,
    resetAll,
  };
}
