import { useI18n } from '../i18n/index.tsx';
import React, { useEffect, useMemo, useRef, useState } from 'react';
import * as api from '../api/index.ts';
import * as builderApi from '../api/builder.ts';
import { getContentEntry, queryContentEntryTable } from '../api/content.ts';
import { getMenuSlots, listMenus, type MenuRecord, type MenuSlotsSnapshot } from '../api/menus.ts';
import { EMPTY_ADMIN_LOCALE_STATE, type AdminLocaleState } from '../api/locales.ts';
import { listPatterns, type PatternRecord } from '../api/patterns.ts';
import {
  getGeneralSettings, getGlobalSiteOptions, getReadingSettings, getTimezoneCatalog,
  type GeneralSettings, type GeneralSettingsDraft, type GlobalSiteOptionsSnapshot,
  type ReadingSettings, type ReadingSettingsDraft,
} from '../api/settings.ts';
import { getStudioTargets, postStudioPreview, type StudioTargetItem, type TransientStudioOverrides } from '../api/studio.ts';
import { BuilderEditorCore, type StudioBuilderView } from '../builder/BuilderEditorCore.tsx';
import { BuilderPreviewFrame } from '../builder/BuilderPreviewFrame.tsx';
import {
  EMPTY_DOCUMENT, createStarterTemplateDocument, locate, visibleNodesInTree,
  type BuilderDocument, type BuilderViewport, type StudioBuilderClipboard,
} from '../builder/document.ts';
import { StudioMode2RevisionsOverlay } from '../features/revisions/StudioMode2RevisionsOverlay.tsx';
import { useStudioMode2Revisions } from '../features/revisions/useStudioMode2Revisions.ts';
import { isActiveDeclaredOptionsPage, type OptionsPageMenuItem } from '../shell/AdminShellMenu.tsx';
import type { ParsedRoute, StudioOptionsScope, StudioPanelId } from '../shell/router.ts';
import type { AuthUser } from '../types.ts';
import { StudioCustomizerNav } from './StudioCustomizerNav.tsx';
import { StudioSidebarHeader } from './StudioSidebarHeader.tsx';
import { StudioSidebarFooter } from './StudioSidebarFooter.tsx';
import type { StudioMenuRequest } from './StudioBlockTree.tsx';
import { buildStudioHash } from './studioOptionsSections.ts';
import { useStudioMode1Controller } from './useStudioMode1Controller.ts';
import { useStudioOptionsState } from './useStudioOptionsState.ts';

interface Props {
  route: ParsedRoute;
  active: boolean;
  user: AuthUser | null;
  siteInstanceId: string;
  authGeneration: number;
  csrfToken: string | null;
  resetToken: number;
  themeRevision: number;
  localeState?: AdminLocaleState;
  optionsPages?: OptionsPageMenuItem[];
  onNavigateHash: (hash: string) => void;
  onDirtyChange: (dirty: boolean) => void;
  onTemplateDirtyChange?: (dirty: boolean) => void;
  onApiError: (error: unknown, fallback: string) => void;
  onSavedSettingsOrTheme?: () => void;
}

interface HistorySnapshot {
  document: BuilderDocument;
  selectedId: string | null;
}

const MAX_UNDO_STEPS = 50;

function isTextInputFocused(): boolean {
  const el = document.activeElement as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable;
}

function sameJson(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function buildSlotAssignmentsMap(snapshot: MenuSlotsSnapshot): Record<string, number | null> {
  const map: Record<string, number | null> = {};
  for (const slot of snapshot.available_slots) {
    map[slot] = snapshot.assignments[slot] ?? null;
  }
  return map;
}

export const StudioWorkspace: React.FC<Props> = ({
  route,
  active,
  user,
  siteInstanceId,
  authGeneration,
  csrfToken,
  resetToken,
  themeRevision,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
  optionsPages = [],
  onNavigateHash,
  onDirtyChange,
  onTemplateDirtyChange,
  onApiError,
  onSavedSettingsOrTheme,
}) => {
  const { __ } = useI18n();
  const isMode2 = route.view === 'studio-entry';
  const entryTypeKey = isMode2 ? (route.typeKey ?? '') : '';
  const entryId = isMode2 ? (route.entryId ?? null) : null;
  const isPatternEntry = isMode2 && entryTypeKey === 'pattern';

  // Shared UI & preview state
  const [viewport, setViewport] = useState<BuilderViewport>(() =>
    typeof window !== 'undefined' && window.innerWidth < 768 ? 'mobile' : 'desktop',
  );
  const [mobilePane, setMobilePane] = useState<'controls' | 'preview'>('controls');
  const [loading, setLoading] = useState(false);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const savingRef = useRef(false);
  const [previewHtml, setPreviewHtml] = useState('');
  const [notice, setNotice] = useState('');
  const [error, setError] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [patterns, setPatterns] = useState<PatternRecord[]>([]);
  const [patternsLoaded, setPatternsLoaded] = useState(false);
  const [builderView, setBuilderView] = useState<StudioBuilderView>('tree');
  const [addTargetParentId, setAddTargetParentId] = useState<string | null>(null);
  const [clipboard, setClipboard] = useState<StudioBuilderClipboard | null>(null);
  const [menuRequest, setMenuRequest] = useState<StudioMenuRequest | null>(null);
  const menuRequestSeqRef = useRef(0);
  const [canvasPickSeq, setCanvasPickSeq] = useState(0);
  const [patternRequestSeq, setPatternRequestSeq] = useState(0);

  // Mode 1 state
  const [panel, setPanel] = useState<StudioPanelId>(route.studioPanel ?? 'root');
  const [optionsRoute, setOptionsRoute] = useState<StudioOptionsScope>(route.studioOptionsRoute ?? { kind: 'index' });
  const [previewLocale, setPreviewLocale] = useState<string>('');
  const previewLocaleInvalid = Boolean(previewLocale && localeState.allLocales.length > 0
    && previewLocale !== localeState.defaultLocale?.code
    && (!localeState.multilingualEnabled || !localeState.enabledLocales.some(loc => loc.code === previewLocale)));
  const effectivePreviewLocale = (previewLocaleInvalid ? '' : previewLocale) || localeState.defaultLocale?.code || '';
  const [localeDraftConflict, setLocaleDraftConflict] = useState<string | null>(null);
  const [localeReloadPending, setLocaleReloadPending] = useState(false);
  const [hydratedPreviewLocale, setHydratedPreviewLocale] = useState<string | null>(null);
  const localeBaselineMismatch = Boolean(hydratedPreviewLocale && effectivePreviewLocale
    && hydratedPreviewLocale !== effectivePreviewLocale);
  // A failed locale load must never leave the old draft eligible for a new locale's CAS.
  const localeDraftBlocked = !isMode2 && Boolean(localeDraftConflict || previewLocaleInvalid || localeReloadPending || localeBaselineMismatch);
  const [activeThemeId, setActiveThemeId] = useState<string | null>(null);
  const [activeThemeName, setActiveThemeName] = useState<string | null>(null);
  const [targets, setTargets] = useState<StudioTargetItem[]>([]);
  const [selectedTargetKey, setSelectedTargetKey] = useState<string | null>(route.studioTemplateKey ?? null);

  const [generalSaved, setGeneralSaved] = useState<GeneralSettings | null>(null);
  const [generalDraft, setGeneralDraft] = useState<GeneralSettingsDraft | null>(null);
  const [timezones, setTimezones] = useState<string[]>([]);

  const [readingSaved, setReadingSaved] = useState<ReadingSettings | null>(null);
  const [readingDraft, setReadingDraft] = useState<ReadingSettingsDraft | null>(null);
  const [publishedPages, setPublishedPages] = useState<Array<{ id: number; title: string }>>([]);

  const [menuSlotsSaved, setMenuSlotsSaved] = useState<MenuSlotsSnapshot | null>(null);
  const [menuSlotsDraft, setMenuSlotsDraft] = useState<Record<string, number | null> | null>(null);
  const [menusList, setMenusList] = useState<MenuRecord[]>([]);

  const [globalOptionsSaved, setGlobalOptionsSaved] = useState<GlobalSiteOptionsSnapshot | null>(null);
  const [globalOptionsDraft, setGlobalOptionsDraft] = useState<Record<string, unknown> | null>(null);
  const [mode1ReloadSeq, setMode1ReloadSeq] = useState(0);
  const [menuPreviewSeq, setMenuPreviewSeq] = useState(0);

  const appearanceOptionsPages = useMemo(
    () =>
      optionsPages
        .filter(p => p.parent_menu === 'appearance' && isActiveDeclaredOptionsPage(p))
        .slice()
        .sort((a, b) => a.menu_position - b.menu_position || a.menu_label.localeCompare(b.menu_label)),
    [optionsPages],
  );

  const optionsState = useStudioOptionsState({
    active,
    isMode2,
    user,
    csrfToken,
    activeThemeId,
    activeCustomPageRef: optionsRoute.kind === 'custom' ? `custom:${optionsRoute.pageKey}` : null,
    effectivePreviewLocale,
    defaultLocaleCode: localeState.defaultLocale?.code ?? '',
    hydratedPreviewLocale,
    localeDraftBlocked,
    mode1ReloadSeq,
    themeRevision,
    onApiError,
  });

  const [overrideDocs, setOverrideDocs] = useState<Record<string, builderApi.TypedTemplateOverride>>({});
  const [templateEditingEnabled, setTemplateEditingEnabled] = useState(false);
  const [templateSavedVersion, setTemplateSavedVersion] = useState<number | null>(null);
  const [templateSavedDoc, setTemplateSavedDoc] = useState<BuilderDocument>(EMPTY_DOCUMENT);
  const [templateDoc, setTemplateDoc] = useState<BuilderDocument>(EMPTY_DOCUMENT);
  const [templateSelectedNodeId, setTemplateSelectedNodeId] = useState<string | null>(null);
  const [templateDiagnostics, setTemplateDiagnostics] = useState<builderApi.BuilderDiagnostic[]>([]);

  // Mode 2 state
  const [entryTitle, setEntryTitle] = useState('');
  const [entryVersion, setEntryVersion] = useState<number | null>(null);
  const [entryPublicationStatus, setEntryPublicationStatus] = useState<'draft' | 'published' | string>('draft');
  const [entryPublicationPolicy, setEntryPublicationPolicy] = useState<'immediate' | 'draft_publish'>('draft_publish');
  const [entryDiagnostics, setEntryDiagnostics] = useState<builderApi.BlockDiagnostic[]>([]);
  const [entryHadFreeformWrap, setEntryHadFreeformWrap] = useState(false);
  const [entryHadRecovery, setEntryHadRecovery] = useState(false);
  const [entrySavedDoc, setEntrySavedDoc] = useState<BuilderDocument>(EMPTY_DOCUMENT);
  const [entryHadExistingDoc, setEntryHadExistingDoc] = useState(false);
  const [entryDoc, setEntryDoc] = useState<BuilderDocument>(EMPTY_DOCUMENT);
  const [entrySelectedNodeId, setEntrySelectedNodeId] = useState<string | null>(null);
  const [entryTrashedBlocked, setEntryTrashedBlocked] = useState(false);
  const [entryLocale, setEntryLocale] = useState<string | null>(null);
  const [entryDocRevision, setEntryDocRevision] = useState(0);
  const [entryHasRetainedDraft, setEntryHasRetainedDraft] = useState(false);

  const isTrashedConflict = (cause: unknown): boolean =>
    !isPatternEntry &&
    cause instanceof api.ApiClientError &&
    cause.status === 409 &&
    (cause.errorCode === 'entry.trashed' || cause.message.includes('entry_trashed'));

  const mode2TrashHash = entryTypeKey === 'page'
    ? '#/pages?status=trash'
    : entryTypeKey === 'post'
      ? '#/posts?status=trash'
      : `#/cpt/${encodeURIComponent(entryTypeKey)}?status=trash`;

  // Undo / Redo stack for active BuilderDocument
  const undoStackRef = useRef<HistorySnapshot[]>([]);
  const redoStackRef = useRef<HistorySnapshot[]>([]);
  const [historyCounts, setHistoryCounts] = useState({ undo: 0, redo: 0 });

  // Session & request guards
  const ownerRef = useRef<number | null>(null);
  const ownerRoleRef = useRef<AuthUser['role'] | null>(null);
  const resetRef = useRef(resetToken);
  const mode1LoadedRef = useRef(false);
  const mode1ForceReloadRef = useRef(false);
  const mode2LoadedKeyRef = useRef<string | null>(null);
  const loadSeqRef = useRef(0);
  const previewSeqRef = useRef(0);
  const previewAbortRef = useRef<AbortController | null>(null);

  const authority = JSON.stringify([route.view, entryTypeKey, entryId, user?.id, user?.role, csrfToken, resetToken, active, isMode2 ? null : effectivePreviewLocale]);
  const authorityRef = useRef(authority);
  authorityRef.current = authority;

  const setStatus = (text: string, failed = false) => {
    setNotice(text);
    setError(failed);
  };

  const resetHistory = () => {
    undoStackRef.current = [];
    redoStackRef.current = [];
    setHistoryCounts({ undo: 0, redo: 0 });
  };

  const clearAllDrafts = () => {
    setBuilderView('tree');
    setAddTargetParentId(null);
    setClipboard(null);
    setMenuRequest(null);
    loadSeqRef.current += 1;
    previewAbortRef.current?.abort();
    previewAbortRef.current = null;
    setPreviewHtml('');
    setConflict(false);
    setLocaleDraftConflict(null);
    setLocaleReloadPending(false);
    setHydratedPreviewLocale(null);
    resetHistory();
    mode1LoadedRef.current = false;
    mode2LoadedKeyRef.current = null;
    setGeneralSaved(null);
    setGeneralDraft(null);
    setReadingSaved(null);
    setReadingDraft(null);
    setMenuSlotsSaved(null);
    setMenuSlotsDraft(null);
    setGlobalOptionsSaved(null);
    setGlobalOptionsDraft(null);
    optionsState.resetAll();
    setTemplateEditingEnabled(false);
    setTemplateDoc(EMPTY_DOCUMENT);
    setTemplateSavedDoc(EMPTY_DOCUMENT);
    setEntryDoc(EMPTY_DOCUMENT);
    setEntrySavedDoc(EMPTY_DOCUMENT);
    setEntryVersion(null);
    setEntryHadExistingDoc(false);
    setEntryTrashedBlocked(false);
    setEntryLocale(null);
    setEntryDocRevision(0);
    setEntryHasRetainedDraft(false);
  };

  // Dirty flags
  const generalDirty = Boolean(
    generalSaved && generalDraft && (
      generalDraft.site_title !== generalSaved.site_title ||
      generalDraft.tagline !== generalSaved.tagline ||
      generalDraft.timezone !== generalSaved.timezone
    ),
  );
  const readingDirty = Boolean(
    readingSaved && readingDraft && (
      readingDraft.homepage_page_id !== readingSaved.homepage_page_id ||
      readingDraft.posts_page_id !== readingSaved.posts_page_id
    ),
  );
  const menuSlotsDirty = Boolean(
    menuSlotsSaved && menuSlotsDraft &&
    !sameJson(buildSlotAssignmentsMap(menuSlotsSaved), menuSlotsDraft),
  );
  const selectedTarget = selectedTargetKey
    ? targets.find(item => item.canonical_target_key === selectedTargetKey) ?? null
    : null;
  const templateDirty = Boolean(
    selectedTarget &&
    templateEditingEnabled && (
      selectedTarget.override_id === null ||
      !sameJson(templateDoc, templateSavedDoc)
    ),
  );
  const globalOptionsDirty = Boolean(
    globalOptionsSaved && globalOptionsDraft &&
    !sameJson(globalOptionsSaved.values, globalOptionsDraft),
  );
  const mode1Dirty =
    generalDirty || readingDirty || menuSlotsDirty || globalOptionsDirty
    || optionsState.themeDirty || optionsState.anyOptionsPagesDirty || templateDirty;
  const entryDirty = Boolean(entryVersion !== null && !sameJson(entryDoc, entrySavedDoc));
  const totalDirty = isMode2 ? entryDirty : mode1Dirty;

  useEffect(() => {
    onDirtyChange(totalDirty);
  }, [totalDirty, onDirtyChange]);
  useEffect(() => {
    onTemplateDirtyChange?.(templateDirty);
  }, [templateDirty, onTemplateDirtyChange]);

  // A disabled/deleted preview locale or a changed default invalidates its loaded CAS baseline.
  useEffect(() => {
    if (isMode2 || savingRef.current || localeDraftConflict) return;
    if (!previewLocaleInvalid && (!localeBaselineMismatch || localeReloadPending)) return;
    loadSeqRef.current += 1;
    previewSeqRef.current += 1;
    previewAbortRef.current?.abort();
    if (previewLocaleInvalid) setPreviewLocale('');
    setPreviewHtml('');
    setLocaleReloadPending(true);
    setLoading(false);
    if (mode1Dirty && !localeReloadPending) {
      setLocaleDraftConflict(hydratedPreviewLocale || previewLocale);
      setStatus(__("The preview language changed or is no longer available. Drafts are retained; reload to switch languages."), true);
    } else {
      mode1ForceReloadRef.current = true;
      mode1LoadedRef.current = false;
      setLoading(true);
      setMode1ReloadSeq(v => v + 1);
    }
  }, [isMode2, previewLocaleInvalid, localeBaselineMismatch, localeReloadPending, localeDraftConflict, hydratedPreviewLocale, previewLocale, saving, mode1Dirty]);

  // Keep route-driven Mode 1 panel, options sub-route, and template target synchronized cleanly (including Back/Forward).
  useEffect(() => {
    if (!isMode2 && active) {
      setPanel(route.studioPanel ?? 'root');
      setOptionsRoute(route.studioOptionsRoute ?? { kind: 'index' });
      const nextTarget = route.studioTemplateKey ?? null;
      if (nextTarget !== selectedTargetKey) {
        setSelectedTargetKey(nextTarget);
        setTemplateDiagnostics([]);
        if (mode1LoadedRef.current) {
          hydrateTemplateTarget(nextTarget, targets, overrideDocs);
        }
      }
    }
  }, [isMode2, active, route.studioPanel, route.studioOptionsRoute, route.studioTemplateKey]);

  const hydrateTemplateTarget = (
    targetKey: string | null,
    items: StudioTargetItem[],
    overridesByKey: Record<string, builderApi.TypedTemplateOverride>,
  ) => {
    setBuilderView('tree');
    setAddTargetParentId(null);
    setMenuRequest(null);
    resetHistory();
    if (!targetKey) {
      setTemplateEditingEnabled(false);
      setTemplateSavedVersion(null);
      setTemplateSavedDoc(EMPTY_DOCUMENT);
      setTemplateDoc(EMPTY_DOCUMENT);
      setTemplateSelectedNodeId(null);
      return;
    }
    const existing = overridesByKey[targetKey];
    if (existing) {
      setTemplateEditingEnabled(true);
      setTemplateSavedVersion(existing.version);
      setTemplateSavedDoc(existing.ast_json);
      setTemplateDoc(existing.ast_json);
      setTemplateSelectedNodeId(visibleNodesInTree(existing.ast_json)[0]?.node.id ?? null);
    } else {
      const targetInfo = items.find(t => t.canonical_target_key === targetKey);
      const starter = createStarterTemplateDocument({
        includeContentSlot: Boolean(targetInfo?.requires_content_slot),
      });
      setTemplateEditingEnabled(false);
      setTemplateSavedVersion(null);
      setTemplateSavedDoc(starter);
      setTemplateDoc(starter);
      setTemplateSelectedNodeId(visibleNodesInTree(starter)[0]?.node.id ?? null);
    }
  };

  // Load Mode 1 or Mode 2 initial state
  useEffect(() => {
    if (resetRef.current !== resetToken) {
      resetRef.current = resetToken;
      clearAllDrafts();
      ownerRef.current = null;
      ownerRoleRef.current = null;
    }
    if (!user || !csrfToken) {
      // Quarantine drafts across 401: keep local drafts in state, revoke in-flight loads.
      loadSeqRef.current += 1;
      previewAbortRef.current?.abort();
      setLoading(false);
      return;
    }
    if ((ownerRef.current !== null && ownerRef.current !== user.id)
        || (ownerRoleRef.current !== null && ownerRoleRef.current !== user.role)) {
      clearAllDrafts();
    }
    ownerRef.current = user.id;
    ownerRoleRef.current = user.role;
    if (!active) return;

    const seq = ++loadSeqRef.current;
    const reqAuthority = authority;

    if (!isMode2) {
      if (!effectivePreviewLocale || previewLocaleInvalid || localeDraftConflict || (localeBaselineMismatch && !localeReloadPending)) return;
      if (mode1LoadedRef.current && !conflict) {
        return;
      }
      let alive = true;
      setLoading(true);
      setStatus(__("Loading appearance customization..."));
      Promise.all([
        getStudioTargets(),
        getGeneralSettings(undefined, effectivePreviewLocale || undefined),
        getTimezoneCatalog(),
        getReadingSettings(undefined, effectivePreviewLocale || undefined),
        queryContentEntryTable('page', {
          status: 'published',
          sort: 'title',
          order: 'asc',
          limit: 100,
          offset: 0,
          locale: effectivePreviewLocale || undefined,
        }),
        builderApi.getTemplateOverrides(),
        listMenus(100, 0),
        listPatterns().catch(() => ({ items: [] as PatternRecord[], total: 0, limit: 50, offset: 0 })),
        getGlobalSiteOptions(),
      ])
        .then(async ([
          targetsRes,
          generalRes,
          tzRes,
          readingRes,
          pagesRes,
          overridesRes,
          menusRes,
          patRes,
          globalOptsRes,
        ]) => {
          if (!alive || seq !== loadSeqRef.current || authorityRef.current !== reqAuthority) return;
          if ((generalRes.locale && generalRes.locale !== effectivePreviewLocale)
              || (readingRes.locale && readingRes.locale !== effectivePreviewLocale)) {
            throw new Error(__("Reload the preview language successfully before saving. Your draft is retained until then."));
          }
          let slotsRes: MenuSlotsSnapshot | null = null;
          if (targetsRes.active_theme_id) {
            slotsRes = await getMenuSlots(targetsRes.active_theme_id, undefined, effectivePreviewLocale || undefined).catch(() => null);
            if (!alive || seq !== loadSeqRef.current || authorityRef.current !== reqAuthority) return;
          }
          const byKey: Record<string, builderApi.TypedTemplateOverride> = {};
          for (const row of overridesRes.items) byKey[row.canonical_target_key] = row;

          const forceReload = mode1ForceReloadRef.current;
          mode1ForceReloadRef.current = false;

          setActiveThemeId(targetsRes.active_theme_id);
          setActiveThemeName(targetsRes.active_theme_name);
          setTargets(targetsRes.items);
          setOverrideDocs(byKey);

          setGeneralSaved(generalRes);
          setGeneralDraft(prev => (!forceReload && generalDirty && prev ? prev : {
            site_title: generalRes.site_title,
            tagline: generalRes.tagline,
            timezone: generalRes.timezone,
          }));
          setTimezones(tzRes.items);

          setReadingSaved(readingRes);
          setReadingDraft(prev => (!forceReload && readingDirty && prev ? prev : {
            homepage_page_id: readingRes.homepage_page_id,
            posts_page_id: readingRes.posts_page_id,
          }));
          setPublishedPages(pagesRes.items.map(({ id, title }) => ({ id, title })));

          setMenusList(menusRes.items);
          setMenuSlotsSaved(slotsRes);
          setMenuSlotsDraft(prev => (!forceReload && menuSlotsDirty && prev ? prev : (slotsRes ? buildSlotAssignmentsMap(slotsRes) : null)));

          setGlobalOptionsSaved(globalOptsRes);
          setGlobalOptionsDraft(prev => (!forceReload && globalOptionsDirty && prev ? prev : globalOptsRes.values));

          if (forceReload) {
            optionsState.resetAll();
          }

          setPatterns(patRes.items);
          setPatternsLoaded(true);

          if (!mode1LoadedRef.current || forceReload || !templateDirty) {
            const initialKey = route.studioTemplateKey ?? selectedTargetKey;
            hydrateTemplateTarget(initialKey, targetsRes.items, byKey);
          }
          mode1LoadedRef.current = true;
          setHydratedPreviewLocale(effectivePreviewLocale);
          setLocaleReloadPending(false);
          setConflict(false);
          setStatus(__("Ready to customize the website and build templates."));
        })
        .catch(cause => {
          if (alive && seq === loadSeqRef.current && authorityRef.current === reqAuthority) {
            setStatus(__("Cannot load appearance customization data."), true);
            onApiError(cause, __("Cannot open appearance customization."));
          }
        })
        .finally(() => {
          if (alive && seq === loadSeqRef.current) setLoading(false);
        });
      return () => { alive = false; };
    }

    if (!entryTypeKey || entryId === null) return;
    const entryIdentity = `${entryTypeKey}:${entryId}`;
    const retainingEntryDraft = mode2LoadedKeyRef.current === entryIdentity && entryDirty;
    let alive = true;
    setLoading(true);
    setStatus(__("Loading the page builder..."));
    Promise.allSettled([
      builderApi.getEntryBuilder(entryTypeKey, entryId),
      getContentEntry(entryTypeKey, entryId),
      listPatterns(),
    ])
      .then(([builderRes, entryRes, patSettled]) => {
        if (!alive || seq !== loadSeqRef.current || authorityRef.current !== reqAuthority) return;
        if (builderRes.status === 'rejected' && isTrashedConflict(builderRes.reason)) {
          setEntryTrashedBlocked(true);
          setStatus(__("This entry is in Trash (409). Restore it from Trash before editing or saving in Studio."), true);
          return;
        }
        if (builderRes.status === 'rejected') throw builderRes.reason;
        if (entryRes.status === 'rejected') throw entryRes.reason;
        if (patSettled.status === 'rejected') throw patSettled.reason;
        const opened = builderRes.value;
        const entryRec = entryRes.value;
        const patRes = patSettled.value;
        setPatterns(patRes.items);
        setPatternsLoaded(true);
        setEntryTrashedBlocked(false);
        setEntryTitle(entryRec.title);
        setEntryLocale(entryRec.locale ?? null);
        setEntryPublicationStatus(opened.publication_status);
        setEntryPublicationPolicy(opened.publication_policy);
        setEntryDiagnostics(opened.diagnostics ?? []);
        setEntryHadFreeformWrap(Boolean(opened.had_freeform_wrap));
        setEntryHadRecovery(Boolean(opened.had_recovery));
        const serverDoc = opened.document ?? EMPTY_DOCUMENT;
        setEntrySavedDoc(serverDoc);
        setEntryHadExistingDoc(opened.document !== null);
        if (!retainingEntryDraft) {
          setEntryVersion(opened.version);
          setEntryDoc(serverDoc);
          setEntrySelectedNodeId(visibleNodesInTree(serverDoc)[0]?.node.id ?? null);
          resetHistory();
          setConflict(false);
          setEntryHasRetainedDraft(false);
          setStatus(__("Editing {{title}} (v{{version}}).", { title: entryRec.title, version: opened.version }));
        } else {
          setConflict(entryVersion !== opened.version);
          setEntryHasRetainedDraft(true);
          setStatus(__("Restored the unsaved page builder draft from the previous session."));
        }
        mode2LoadedKeyRef.current = entryIdentity;
      })
      .catch(cause => {
        if (alive && seq === loadSeqRef.current && authorityRef.current === reqAuthority) {
          if (isTrashedConflict(cause)) {
            setEntryTrashedBlocked(true);
            setStatus(__("This entry is in Trash (409). Restore it from Trash before editing or saving in Studio."), true);
          } else {
            setStatus(__("Cannot open the page builder for this content."), true);
            onApiError(cause, __("Cannot open the page builder."));
          }
        }
      })
      .finally(() => {
        if (alive && seq === loadSeqRef.current) setLoading(false);
      });
    return () => { alive = false; };
  }, [active, isMode2, entryTypeKey, entryId, user?.id, user?.role, csrfToken, resetToken, themeRevision, mode1ReloadSeq, isMode2 ? null : effectivePreviewLocale, previewLocaleInvalid, localeDraftConflict]);

  const activeDoc = isMode2 ? entryDoc : templateDoc;
  const activeSelectedId = isMode2 ? entrySelectedNodeId : templateSelectedNodeId;

  useEffect(() => {
    if ((builderView === 'properties' && !locate(activeDoc, activeSelectedId))
        || (builderView === 'add' && addTargetParentId !== null && !locate(activeDoc, addTargetParentId))) {
      setBuilderView('tree');
      setAddTargetParentId(null);
    }
  }, [builderView, activeDoc, activeSelectedId, addTargetParentId]);

  const setActiveDocAndSelection = (nextDoc: BuilderDocument, nextSel?: string | null) => {
    if (isMode2) {
      setEntryDoc(nextDoc);
      setEntryDocRevision(v => v + 1);
      if (nextSel !== undefined) setEntrySelectedNodeId(nextSel);
    } else {
      setTemplateDoc(nextDoc);
      if (nextSel !== undefined) setTemplateSelectedNodeId(nextSel);
    }
  };

  const handleBuilderChange = (
    nextDoc: BuilderDocument,
    nextSelected?: string | null,
    _removedId?: string,
  ): boolean => {
    undoStackRef.current = [
      ...undoStackRef.current.slice(-(MAX_UNDO_STEPS - 1)),
      { document: activeDoc, selectedId: activeSelectedId },
    ];
    redoStackRef.current = [];
    setHistoryCounts({ undo: undoStackRef.current.length, redo: 0 });
    setActiveDocAndSelection(nextDoc, nextSelected);
    setStatus(__("There are unsaved changes."));
    return true;
  };

  const handleUndo = () => {
    if (savingRef.current || undoStackRef.current.length === 0) return;
    const prev = undoStackRef.current[undoStackRef.current.length - 1];
    undoStackRef.current = undoStackRef.current.slice(0, -1);
    redoStackRef.current = [
      ...redoStackRef.current.slice(-(MAX_UNDO_STEPS - 1)),
      { document: activeDoc, selectedId: activeSelectedId },
    ];
    setHistoryCounts({ undo: undoStackRef.current.length, redo: redoStackRef.current.length });
    setActiveDocAndSelection(prev.document, prev.selectedId);
  };

  const handleRedo = () => {
    if (savingRef.current || redoStackRef.current.length === 0) return;
    const next = redoStackRef.current[redoStackRef.current.length - 1];
    redoStackRef.current = redoStackRef.current.slice(0, -1);
    undoStackRef.current = [
      ...undoStackRef.current.slice(-(MAX_UNDO_STEPS - 1)),
      { document: activeDoc, selectedId: activeSelectedId },
    ];
    setHistoryCounts({ undo: undoStackRef.current.length, redo: redoStackRef.current.length });
    setActiveDocAndSelection(next.document, next.selectedId);
  };

  // Keyboard shortcuts Ctrl/Cmd+Z and Ctrl/Cmd+Shift+Z / Ctrl+Y (ignoring text inputs)
  const canShowUndo = isMode2 || Boolean(panel === 'templates' && selectedTarget && templateEditingEnabled);
  useEffect(() => {
    if (!active || !canShowUndo) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey) return;
      if (isTextInputFocused()) return;
      const key = event.key.toLowerCase();
      if (key === 'z' && !event.shiftKey) {
        if (undoStackRef.current.length > 0) {
          event.preventDefault();
          handleUndo();
        }
      } else if ((key === 'z' && event.shiftKey) || key === 'y') {
        if (redoStackRef.current.length > 0) {
          event.preventDefault();
          handleRedo();
        }
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [active, canShowUndo, isMode2, activeDoc, activeSelectedId]);

  // Debounced 250ms Live Preview (read-only)
  const previewTargetCandidate = isMode2
    ? null
    : (selectedTarget?.target ?? targets.find(t => t.canonical_target_key === 'home')?.target ?? targets[0]?.target ?? null);

  useEffect(() => {
    if (!active || !user || !csrfToken) return;
    if (!isMode2 && (!previewTargetCandidate || !effectivePreviewLocale || previewLocaleInvalid || ((localeReloadPending || localeBaselineMismatch) && !localeDraftConflict))) return;
    if (isMode2 && (!entryTypeKey || entryId === null || entryVersion === null || entryTrashedBlocked)) return;

    const seq = ++previewSeqRef.current;
    const delay = previewHtml ? 250 : 0;
    setPreviewLoading(true);
    const timer = window.setTimeout(() => {
      previewAbortRef.current?.abort();
      const controller = new AbortController();
      previewAbortRef.current = controller;

      if (isMode2 && entryId !== null) {
        void builderApi.previewEntryBuilder(entryTypeKey, entryId, entryDoc, csrfToken, controller.signal)
          .then(res => {
            if (controller.signal.aborted || seq !== previewSeqRef.current) return;
            setPreviewHtml(res.html);
            setEntryDiagnostics(res.diagnostics.map(d => ({ ...d, message: d.code })));
          })
          .catch(cause => {
            if (controller.signal.aborted || seq !== previewSeqRef.current) return;
            if (isTrashedConflict(cause)) {
              setEntryTrashedBlocked(true);
              setStatus(__("This entry is in Trash (409). Restore it from Trash before editing or saving in Studio."), true);
              return;
            }
            onApiError(cause, __("Cannot create a page builder preview."));
          })
          .finally(() => {
            if (!controller.signal.aborted && seq === previewSeqRef.current) setPreviewLoading(false);
          });
        return;
      }

      if (!previewTargetCandidate) {
        setPreviewLoading(false);
        return;
      }
      const transient: TransientStudioOverrides = {};
      if (!localeDraftConflict && generalDirty && generalDraft) transient.general = generalDraft;
      if (!localeDraftConflict && globalOptionsDirty && globalOptionsDraft) transient.global_options = globalOptionsDraft;
      if (!localeDraftConflict && optionsState.themeDirty && optionsState.themeDraft) transient.theme_options = optionsState.themeDraft;
      if (!localeDraftConflict && menuSlotsDirty && menuSlotsDraft) transient.menu_slots = menuSlotsDraft;
      const transientOptPages = !localeDraftConflict ? optionsState.buildTransientOptionsPages() : undefined;
      if (transientOptPages) transient.options_pages = transientOptPages;

      const editingSelectedTemplate = Boolean(panel === 'templates' && selectedTarget && templateEditingEnabled);
      void postStudioPreview(
        {
          target: previewTargetCandidate,
          document: editingSelectedTemplate ? templateDoc : null,
          preview_annotations: editingSelectedTemplate,
          preview_locale: effectivePreviewLocale || undefined,
          transient,
        },
        csrfToken,
        controller.signal,
      )
        .then(res => {
          if (controller.signal.aborted || seq !== previewSeqRef.current) return;
          setPreviewHtml(res.html);
          setTemplateDiagnostics(res.diagnostics ?? []);
        })
        .catch(cause => {
          if (controller.signal.aborted || seq !== previewSeqRef.current) return;
          if (cause instanceof api.ApiClientError && (cause.status === 412 || cause.status === 409)) {
            setConflict(true);
          } else {
            onApiError(cause, __("Cannot preview appearance customization."));
          }
        })
        .finally(() => {
          if (!controller.signal.aborted && seq === previewSeqRef.current) setPreviewLoading(false);
        });
    }, delay);

    return () => {
      window.clearTimeout(timer);
    };
  }, [
    active,
    user?.id,
    csrfToken,
    isMode2,
    entryTypeKey,
    entryId,
    entryVersion,
    entryDoc,
    panel,
    previewTargetCandidate,
    templateEditingEnabled,
    templateDoc,
    generalDirty,
    generalDraft,
    globalOptionsDirty,
    globalOptionsDraft,
    optionsState.themeDirty,
    optionsState.themeDraft,
    optionsState.draftsByRef,
    optionsState.snapshotsByRef,
    menuSlotsDirty,
    menuSlotsDraft,
    menuPreviewSeq,
    effectivePreviewLocale,
    previewLocaleInvalid,
    localeDraftConflict,
    localeReloadPending,
    localeBaselineMismatch,
  ]);

  const handleSelectTarget = (nextKey: string | null) => {
    onNavigateHash(buildStudioHash('templates', undefined, nextKey));
  };

  const mode1Controller = useStudioMode1Controller({
    active,
    isMode2,
    user,
    csrfToken,
    loading,
    savingRef,
    setSaving,
    setConflict,
    setStatus,
    onApiError,
    onNavigateHash,
    onSavedSettingsOrTheme,
    panel,
    optionsRoute,
    selectedTarget,
    activeThemeId,
    activeThemeName,
    appearanceOptionsPages,
    mode1Dirty,
    localeDraftBlocked,
    hydratedPreviewLocale,
    effectivePreviewLocale,
    generalDirty,
    generalSaved,
    generalDraft,
    setGeneralSaved,
    setGeneralDraft,
    readingDirty,
    readingSaved,
    readingDraft,
    setReadingSaved,
    setReadingDraft,
    menuSlotsDirty,
    menuSlotsSaved,
    menuSlotsDraft,
    setMenuSlotsSaved,
    setMenuSlotsDraft,
    globalOptionsDirty,
    globalOptionsSaved,
    globalOptionsDraft,
    setGlobalOptionsSaved,
    setGlobalOptionsDraft,
    optionsState,
    templateDirty,
    templateDoc,
    templateSavedVersion,
    setOverrideDocs,
    setTemplateSavedVersion,
    setTemplateSavedDoc,
    setTemplateDoc,
    setTargets,
    entryTitle,
    entryTypeKey,
    entryId,
    resetBuilderPaneUi: () => {
      setBuilderView('tree');
      setAddTargetParentId(null);
      setMenuRequest(null);
    },
  });
  const {
    navigatePanel: handlePanelChange,
    navigateOptionsRoute: handleOptionsRouteChange,
    contextTitle,
    contextBadge,
    contextBack,
    handleContextBack,
    handleSaveMode1,
  } = mode1Controller;

  const handleResetTemplate = async () => {
    if (!active || !csrfToken || savingRef.current || localeDraftBlocked || !selectedTarget?.override_id || !templateSavedVersion) return;
    if (!window.confirm(__("Reset template \"{{label}}\" to the theme/core default?", { label: selectedTarget.label }))) return;
    savingRef.current = true;
    setSaving(true);
    try {
      const res = await builderApi.resetTemplateOverride(selectedTarget.override_id, templateSavedVersion, csrfToken);
      const freshTargets = await getStudioTargets();
      const freshOverrides = await builderApi.getTemplateOverrides();
      const byKey: Record<string, builderApi.TypedTemplateOverride> = {};
      for (const row of freshOverrides.items) byKey[row.canonical_target_key] = row;
      setTargets(freshTargets.items);
      setOverrideDocs(byKey);
      hydrateTemplateTarget(selectedTarget.canonical_target_key, freshTargets.items, byKey);
      setClipboard(null);
      onSavedSettingsOrTheme?.();
      setStatus(__("Template reset. Current source: {{label}}", { label: res.next_effective_source.label }));
    } catch (cause) {
      if (cause instanceof api.ApiClientError && cause.status === 412) setConflict(true);
      onApiError(cause, __("Cannot reset template override."));
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const mode2Revisions = useStudioMode2Revisions({
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
  });
  const { handleSaveMode2, handleReloadMode2, handleExit } = mode2Revisions;

  const handleMenusMutated = async () => {
    try {
      const res = await listMenus(100, 0);
      setMenusList(res.items);
      setMenuPreviewSeq(v => v + 1);
    } catch (cause) {
      onApiError(cause, __("Cannot reload the menu list."));
    }
  };

  const canSaveMode2 =
    !entryTrashedBlocked &&
    (entryDirty ||
      (entryPublicationPolicy === 'draft_publish' && entryPublicationStatus === 'draft'));

  const builderActive = isMode2 || Boolean(panel === 'templates' && selectedTarget && templateEditingEnabled);
  const handleCanvasMenu = (
    id: string,
    mode: StudioMenuRequest['mode'],
    anchor?: { x: number; y: number } | null,
  ) => {
    if (loading || saving || localeDraftBlocked || !csrfToken || !builderActive || !locate(activeDoc, id)) return;
    if (isMode2) setEntrySelectedNodeId(id);
    else setTemplateSelectedNodeId(id);
    setAddTargetParentId(null);
    setBuilderView('tree');
    setMenuRequest({ nodeId: id, mode, seq: ++menuRequestSeqRef.current, anchor });
  };
  const handleCanvasEdit = (id: string) => {
    if (loading || saving || localeDraftBlocked || !csrfToken || !builderActive || !locate(activeDoc, id)) return;
    if (isMode2) setEntrySelectedNodeId(id);
    else setTemplateSelectedNodeId(id);
    setAddTargetParentId(null);
    setMenuRequest(null);
    setBuilderView('properties');
  };
  const handleCanvasPick = (id: string) => {
    setCanvasPickSeq(seq => seq + 1);
    setMenuRequest(null);
    if (isMode2) setEntrySelectedNodeId(id);
    else if (panel === 'templates' && templateEditingEnabled) setTemplateSelectedNodeId(id);
  };
  const handleReloadConflict = () => {
    if (isMode2) {
      handleReloadMode2();
      return;
    }
    if (loading || savingRef.current) return;
    if (mode1Dirty && !window.confirm(__("Reloading from the server discards unsaved Studio changes. Continue?"))) {
      return;
    }
    setLocaleDraftConflict(null);
    setLocaleReloadPending(true);
    setLoading(true);
    optionsState.resetAll();
    mode1ForceReloadRef.current = true;
    mode1LoadedRef.current = false;
    setConflict(false);
    setMode1ReloadSeq(v => v + 1);
  };
  const canResetStudio = isMode2
    ? false
    : Boolean(panel === 'templates' && selectedTarget?.override_id && (templateSavedVersion ?? 0) > 0);
  const templateCanShowPattern = Boolean(panel === 'templates' && selectedTarget && templateEditingEnabled);

  return (
    <div className="flex h-full min-h-0 w-full flex-col bg-zinc-100 text-zinc-900" data-testid="studio-workspace">
      <div className="flex items-center gap-1 border-b border-zinc-200 bg-white px-3 py-1.5 md:hidden" role="tablist" aria-label={__("Studio mobile view")}>
        <button
          type="button"
          role="tab"
          aria-selected={mobilePane === 'controls'}
          onClick={() => setMobilePane('controls')}
          className={`flex-1 rounded-md px-3 py-1.5 text-xs font-semibold transition-colors ${mobilePane === 'controls' ? 'bg-zinc-900 text-white' : 'bg-zinc-100 text-zinc-700'}`}
        >
          {__("Controls")}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mobilePane === 'preview'}
          onClick={() => setMobilePane('preview')}
          className={`flex-1 rounded-md px-3 py-1.5 text-xs font-semibold transition-colors ${mobilePane === 'preview' ? 'bg-zinc-900 text-white' : 'bg-zinc-100 text-zinc-700'}`}
        >
          {__("Preview")}
        </button>
      </div>
      <div className="flex min-h-0 flex-1 flex-col md:flex-row">
        <aside
          className={`min-h-0 w-full shrink-0 flex-col overflow-hidden border-b border-zinc-200 bg-white md:flex md:h-full md:w-[320px] md:border-r md:border-b-0 ${mobilePane === 'controls' ? 'flex flex-1 md:flex-none' : 'hidden'}`}
          aria-label={isMode2 ? __("Page builder content panel") : __("Studio customization panel")}
        >
          <StudioSidebarHeader
            mode={isMode2 ? 'entry' : 'customizer'}
            title={contextTitle}
            badge={contextBadge}
            dirty={totalDirty}
            saving={saving}
            onExit={handleExit}
            backLabel={contextBack}
            onBack={contextBack ? handleContextBack : undefined}
            canReset={canResetStudio && builderActive}
            resetLabel={__("Restore theme defaults")}
            onReset={() => void handleResetTemplate()}
            conflict={conflict || localeDraftBlocked}
            onReloadConflict={handleReloadConflict}
            onOpenRevisions={
              isMode2 && entryId !== null
                ? () => mode2Revisions.handleOpenHistory()
                : undefined
            }
            revisionCount={isMode2 ? mode2Revisions.revisionCount : null}
          />
          {isMode2 && entryId !== null && (
            <StudioMode2RevisionsOverlay
              entryTypeKey={entryTypeKey}
              entryId={entryId}
              isPatternEntry={isPatternEntry}
              csrfToken={csrfToken}
              currentVersion={entryVersion ?? 0}
              serverAutosave={mode2Revisions.serverAutosave}
              hasNewerLocalDraft={mode2Revisions.hasNewerLocalDraft}
              discardingAutosave={mode2Revisions.discardingAutosave}
              historyOpen={mode2Revisions.historyOpen}
              initialCompare={mode2Revisions.initialCompare}
              restoredEditorModeNotice={mode2Revisions.restoredEditorModeNotice}
              singleEditHash={mode2Revisions.singleEditHash}
              onDismissEditorModeNotice={mode2Revisions.dismissEditorModeNotice}
              onNavigateHash={onNavigateHash}
              onApplyAutosave={mode2Revisions.handleApplyServerAutosave}
              onCompareAutosave={() =>
                mode2Revisions.handleOpenHistory({ from: 'current', to: 'autosave' })
              }
              onDiscardAutosave={mode2Revisions.handleDiscardServerAutosave}
              onCloseHistory={mode2Revisions.handleCloseHistory}
              onBeforeRestore={mode2Revisions.abortAndClearInflight}
              onRestored={mode2Revisions.handleRevisionRestored}
              onApiError={onApiError}
            />
          )}
          {!isMode2 && localeState.hasMultiplePublicLocales && (
            <div className="flex items-center justify-between gap-2 border-b border-zinc-200 bg-zinc-50 px-3 py-2" data-testid="studio-preview-locale-bar">
              <label htmlFor="studio-preview-locale" className="text-[11px] font-semibold uppercase tracking-wide text-zinc-600">
                {__("Preview locale")}
              </label>
              <select
                id="studio-preview-locale"
                data-testid="studio-preview-locale-select"
                className="h-7 rounded border border-zinc-300 bg-white px-2 text-xs text-zinc-900"
                value={effectivePreviewLocale}
                disabled={saving || loading || localeDraftBlocked}
                onChange={e => {
                  const next = e.target.value;
                  if (next === effectivePreviewLocale) return;
                  const hasTransient = mode1Dirty;
                  if (hasTransient) {
                    if (!window.confirm(__("There are unsaved changes in the current language. Discard them to switch preview locale?"))) {
                      return;
                    }
                    setGeneralDraft(generalSaved ? { site_title: generalSaved.site_title, tagline: generalSaved.tagline, timezone: generalSaved.timezone } : null);
                    setReadingDraft(readingSaved ? { homepage_page_id: readingSaved.homepage_page_id, posts_page_id: readingSaved.posts_page_id } : null);
                    setMenuSlotsDraft(menuSlotsSaved ? buildSlotAssignmentsMap(menuSlotsSaved) : null);
                    setGlobalOptionsDraft(globalOptionsSaved ? globalOptionsSaved.values : null);
                    optionsState.resetAll();
                  }
                  setPreviewLocale(next);
                  setLocaleReloadPending(true);
                  setLoading(true);
                  mode1ForceReloadRef.current = true;
                  mode1LoadedRef.current = false;
                  setMode1ReloadSeq(v => v + 1);
                }}
              >
                {localeState.enabledLocales.map(loc => (
                  <option key={loc.code} value={loc.code}>
                    {loc.label} ({loc.code}){loc.is_default ? ' ★' : ''}
                  </option>
                ))}
              </select>
            </div>
          )}
          {(error || conflict || localeDraftBlocked || entryTrashedBlocked) && (
            <div
              role="alert"
              data-testid="studio-sidebar-alert"
              className="flex items-center justify-between gap-2 border-b border-rose-200 bg-rose-50 px-3 py-1.5 text-xs text-rose-800"
            >
              <span className="min-w-0 flex-1 break-words leading-snug">
                {entryTrashedBlocked ? __("This entry is in Trash (409). Restore it from Trash before editing or saving in Studio.") : localeDraftConflict || previewLocaleInvalid ? __("The preview language changed or is no longer available. Drafts are retained; reload to switch languages.") : localeReloadPending ? __("Reload the preview language successfully before saving. Your draft is retained until then.") : error && notice ? notice : __("Data changed on the server (412).")}
              </span>
              {entryTrashedBlocked && !isPatternEntry ? (
                <button
                  type="button"
                  data-testid="studio-open-trash-link"
                  onClick={() => onNavigateHash(mode2TrashHash)}
                  className="shrink-0 rounded border border-rose-300 bg-white px-2 py-0.5 text-[11px] font-medium text-rose-800 hover:bg-rose-100"
                >
                  {__("Open Trash")}
                </button>
              ) : (conflict || localeDraftBlocked) && (
                <button
                  type="button"
                  onClick={handleReloadConflict}
                  disabled={loading || saving}
                  className="shrink-0 rounded border border-rose-300 bg-white px-2 py-0.5 text-[11px] font-medium text-rose-800 hover:bg-rose-100"
                >
                  {__("Reload")}
                </button>
              )}
            </div>
          )}
          <div className="min-h-0 flex-1 overflow-y-auto">
          {isMode2 ? (
            <div className="px-1.5 py-2" data-testid="studio-mode2-sidebar">
              <BuilderEditorCore
                key={`${entryTypeKey}:${entryId}`}
                builderView={builderView}
                onBuilderViewChange={setBuilderView}
                addTargetParentId={addTargetParentId}
                onAddTargetParentIdChange={setAddTargetParentId}
                clipboard={clipboard}
                onClipboardChange={setClipboard}
                menuRequest={menuRequest}
                onMenuRequestHandled={() => setMenuRequest(null)}
                canvasPickSeq={canvasPickSeq}
                openPatternSeq={patternRequestSeq}
                document={entryDoc}
                selectedId={entrySelectedNodeId}
                onSelect={setEntrySelectedNodeId}
                onChangeDocument={handleBuilderChange}
                csrfToken={csrfToken}
                viewport={viewport}
                contextTarget={isPatternEntry ? { kind: 'pattern' } : { kind: 'entry_body', type_key: entryTypeKey }}
                contextTypeKey={isPatternEntry ? null : entryTypeKey}
                locale={entryLocale ?? effectivePreviewLocale ?? null}
                siteTimezone={generalDraft?.timezone ?? generalSaved?.timezone}
                themeRevision={themeRevision}
                requiresContentSlot={false}
                diagnostics={entryDiagnostics}
                hadFreeformWrap={entryHadFreeformWrap}
                hadRecovery={entryHadRecovery}
                disabled={loading || saving || !csrfToken}
                onMessage={(msg, failed) => setStatus(msg, Boolean(failed))}
                onApiError={onApiError}
              />
            </div>
          ) : (
            <StudioCustomizerNav
              builderView={builderView}
              onBuilderViewChange={setBuilderView}
              addTargetParentId={addTargetParentId}
              onAddTargetParentIdChange={setAddTargetParentId}
              clipboard={clipboard}
              onClipboardChange={setClipboard}
              menuRequest={menuRequest}
              onMenuRequestHandled={() => setMenuRequest(null)}
              canvasPickSeq={canvasPickSeq}
              openPatternSeq={patternRequestSeq}
              panel={panel}
              onPanelChange={handlePanelChange}
              optionsRoute={optionsRoute}
              onOptionsRouteChange={handleOptionsRouteChange}
              onNavigateHash={onNavigateHash}
              localeState={localeState}
              previewLocale={localeDraftConflict ?? (localeBaselineMismatch ? hydratedPreviewLocale! : previewLocaleInvalid ? previewLocale : effectivePreviewLocale)}
              activeThemeId={activeThemeId}
              generalDraft={generalDraft}
              generalDirty={generalDirty}
              timezones={timezones}
              onGeneralChange={next => setGeneralDraft(next)}
              csrfToken={csrfToken}
              globalOptionsSnapshot={globalOptionsSaved}
              globalOptionsDraft={globalOptionsDraft}
              globalOptionsDirty={globalOptionsDirty}
              onGlobalOptionsChange={partial =>
                setGlobalOptionsDraft(prev => ({ ...(prev ?? globalOptionsSaved?.values ?? {}), ...partial }))
              }
              optionsState={optionsState}
              appearanceOptionsPages={appearanceOptionsPages}
              menuSlots={menuSlotsSaved}
              menuSlotsDraft={menuSlotsDraft}
              menuSlotsDirty={menuSlotsDirty}
              menusList={menusList}
              onMenuSlotChange={(slot, menuId) =>
                setMenuSlotsDraft(prev => ({ ...(prev ?? {}), [slot]: menuId }))
              }
              onMenusMutated={handleMenusMutated}
              readingDraft={readingDraft}
              readingDirty={readingDirty}
              readingSettings={readingSaved}
              publishedPages={publishedPages}
              onReadingChange={next => setReadingDraft(next)}
              targets={targets}
              selectedTargetKey={selectedTargetKey}
              onSelectTarget={handleSelectTarget}
              templateEditingEnabled={templateEditingEnabled}
              onInitializeTemplateOverride={() => {
                setTemplateEditingEnabled(true);
                setStatus(__("Initialized the page builder for \"{{label}}\". Not yet saved to the database.", { label: selectedTarget?.label ?? "" }));
              }}
              templateDoc={templateDoc}
              templateSelectedNodeId={templateSelectedNodeId}
              onSelectTemplateNode={setTemplateSelectedNodeId}
              onChangeTemplateDoc={handleBuilderChange}
              templateDirty={templateDirty}
              viewport={viewport}
              themeRevision={themeRevision}
              templateDiagnostics={templateDiagnostics}
              onBuilderMessage={(msg, failed) => setStatus(msg, Boolean(failed))}
              onApiError={onApiError}
              disabled={loading || saving || !csrfToken || localeDraftBlocked}
            />
          )}
          </div>
          <StudioSidebarFooter
            saving={saving}
            busy={loading}
            viewport={viewport}
            onViewportChange={setViewport}
            canShowUndo={canShowUndo}
            canUndo={historyCounts.undo > 0}
            canRedo={historyCounts.redo > 0}
            onUndo={handleUndo}
            onRedo={handleRedo}
            savePolicy={isMode2
              ? {
                  mode: 'entry',
                  isPrivatePattern: isPatternEntry,
                  publicationPolicy: entryPublicationPolicy,
                  publicationStatus: entryPublicationStatus,
                }
              : { mode: 'customizer' }}
            canSave={isMode2 ? canSaveMode2 : mode1Dirty && !localeDraftBlocked}
            onSaveCustomizer={() => void handleSaveMode1()}
            onSaveEntry={intent => void handleSaveMode2(intent)}
            showPatternButton={isMode2 || templateCanShowPattern}
            onOpenPatternLibrary={() => setPatternRequestSeq(s => s + 1)}
            patternDisabled={!csrfToken}
            autosaveState={isMode2 ? mode2Revisions.autosaveState : undefined}
          />
        </aside>

        {(() => {
          const PreviewTag: 'main' | 'div' = active ? 'main' : 'div';
          return (
        <PreviewTag className={`min-w-0 flex-1 flex-col overflow-hidden md:flex ${mobilePane === 'preview' ? 'flex' : 'hidden'} ${viewport === 'desktop' ? 'bg-white' : 'bg-zinc-200/70'}`}>
          <div className={'min-h-0 flex-1 overflow-auto ' + (viewport === 'desktop' ? 'p-0' : 'p-0 sm:p-4')}>
            {previewHtml ? (
              <BuilderPreviewFrame
                html={previewHtml}
                viewport={viewport}
                interactive={isMode2 || Boolean(panel === 'templates' && selectedTarget && templateEditingEnabled)}
                selectedId={activeSelectedId}
                document={activeDoc}
                onSelectNode={handleCanvasPick}
                onEditNode={handleCanvasEdit}
                onOpenNodeMenu={handleCanvasMenu}
                onApiError={onApiError}
                title={isMode2 ? __("Content preview") : __("Website preview")}
                className="h-full w-full"
                frameClassName={
                  viewport === 'desktop'
                    ? '!h-full !w-full !border-0 !rounded-none !shadow-none'
                    : '!h-full min-h-[540px] !border border-zinc-300 rounded-lg shadow-sm'
                }
              />
            ) : (
              <div className="flex h-full min-h-[360px] items-center justify-center bg-white p-8 text-center text-sm text-zinc-500">
                {loading || previewLoading ? __("Preparing live preview…") : __("No preview content for this target yet.")}
              </div>
            )}
          </div>
        </PreviewTag>
          );
        })()}
      </div>
    </div>
  );
};
