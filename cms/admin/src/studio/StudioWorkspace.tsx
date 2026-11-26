import { useI18n } from '../i18n/index.tsx';
import React, { useEffect, useRef, useState } from 'react';
import * as api from '../api/index.ts';
import * as builderApi from '../api/builder.ts';
import { changeContentEntryPublication, getContentEntry, queryContentEntryTable } from '../api/content.ts';
import { getMenuSlots, listMenus, putMenuSlots, type MenuRecord, type MenuSlotsSnapshot } from '../api/menus.ts';
import { EMPTY_ADMIN_LOCALE_STATE, type AdminLocaleState } from '../api/locales.ts';
import { listPatterns, type PatternRecord } from '../api/patterns.ts';
import {
  getGeneralSettings,
  getGlobalSiteOptions,
  getReadingSettings,
  getTimezoneCatalog,
  saveGeneralSettings,
  saveGlobalSiteOptions,
  saveReadingSettings,
  type GeneralSettings,
  type GeneralSettingsDraft,
  type GlobalSiteOptionsSnapshot,
  type ReadingSettings,
  type ReadingSettingsDraft,
} from '../api/settings.ts';
import { getStudioTargets, postStudioPreview, type StudioTargetItem, type TransientStudioOverrides } from '../api/studio.ts';
import { saveThemeOptions } from '../api/themes.ts';
import { BuilderEditorCore, type StudioBuilderView } from '../builder/BuilderEditorCore.tsx';
import { BuilderPreviewFrame } from '../builder/BuilderPreviewFrame.tsx';
import {
  EMPTY_DOCUMENT,
  VALID_TEMPLATE_PART_KEY,
  allNodes,
  canDeleteElement,
  countElementsByType,
  createStarterTemplateDocument,
  isElement,
  locate,
  visibleNodesInTree,
  type BuilderDocument,
  type BuilderElement,
  type BuilderViewport,
  type StudioBuilderClipboard,
} from '../builder/document.ts';
import { editorDraftStore } from '../features/editor/EditorDraftStore.ts';
import type { ParsedRoute, StudioPanelId } from '../shell/router.ts';
import type { AuthUser } from '../types.ts';
import { StudioCustomizerNav } from './StudioCustomizerNav.tsx';
import { StudioSidebarHeader } from './StudioSidebarHeader.tsx';
import { StudioSidebarFooter } from './StudioSidebarFooter.tsx';
import type { StudioMenuRequest } from './StudioBlockTree.tsx';

interface Props {
  route: ParsedRoute;
  active: boolean;
  user: AuthUser | null;
  csrfToken: string | null;
  resetToken: number;
  themeRevision: number;
  localeState?: AdminLocaleState;
  onNavigateHash: (hash: string) => void;
  onDirtyChange: (dirty: boolean) => void;
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
  csrfToken,
  resetToken,
  themeRevision,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
  onNavigateHash,
  onDirtyChange,
  onApiError,
  onSavedSettingsOrTheme,
}) => {
  const { t } = useI18n();
  const isMode2 = route.view === 'studio-entry';
  const entryTypeKey = isMode2 ? (route.typeKey ?? '') : '';
  const entryId = isMode2 ? (route.entryId ?? null) : null;
  const isPatternEntry = isMode2 && entryTypeKey === 'pattern';

  // Shared UI & preview state
  const [viewport, setViewport] = useState<BuilderViewport>('desktop');
  const [loading, setLoading] = useState(false);
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

  const [themeOptionsVersion, setThemeOptionsVersion] = useState<number | null>(null);
  const [themeOptionsDraft, setThemeOptionsDraft] = useState<Record<string, unknown> | null>(null);
  const [themeOptionsDirty, setThemeOptionsDirty] = useState(false);
  const [themeOptionsReloadToken, setThemeOptionsReloadToken] = useState(0);
  const [mode1ReloadSeq, setMode1ReloadSeq] = useState(0);
  const [menuPreviewSeq, setMenuPreviewSeq] = useState(0);

  const [overrideDocs, setOverrideDocs] = useState<Record<string, builderApi.TypedTemplateOverride>>({});
  const [templateEditingEnabled, setTemplateEditingEnabled] = useState(false);
  const [templateSavedVersion, setTemplateSavedVersion] = useState<number | null>(null);
  const [templateSavedDoc, setTemplateSavedDoc] = useState<BuilderDocument>(EMPTY_DOCUMENT);
  const [templateDoc, setTemplateDoc] = useState<BuilderDocument>(EMPTY_DOCUMENT);
  const [templateSelectedNodeId, setTemplateSelectedNodeId] = useState<string | null>(null);

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
    setThemeOptionsDraft(null);
    setThemeOptionsDirty(false);
    setThemeOptionsVersion(null);
    setTemplateEditingEnabled(false);
    setTemplateDoc(EMPTY_DOCUMENT);
    setTemplateSavedDoc(EMPTY_DOCUMENT);
    setEntryDoc(EMPTY_DOCUMENT);
    setEntrySavedDoc(EMPTY_DOCUMENT);
    setEntryVersion(null);
    setEntryHadExistingDoc(false);
    setEntryTrashedBlocked(false);
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
  const mode1Dirty = generalDirty || readingDirty || menuSlotsDirty || globalOptionsDirty || themeOptionsDirty || templateDirty;
  const entryDirty = Boolean(entryVersion !== null && !sameJson(entryDoc, entrySavedDoc));
  const totalDirty = isMode2 ? entryDirty : mode1Dirty;

  useEffect(() => {
    onDirtyChange(totalDirty);
  }, [totalDirty, onDirtyChange]);

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
      setStatus(t("studio.the_preview_locale_is_no_longer_available_drafts_are_retained_reload_to_switch_to_the_default_locale"), true);
    } else {
      mode1ForceReloadRef.current = true;
      mode1LoadedRef.current = false;
      setLoading(true);
      setMode1ReloadSeq(v => v + 1);
    }
  }, [isMode2, previewLocaleInvalid, localeBaselineMismatch, localeReloadPending, localeDraftConflict, hydratedPreviewLocale, previewLocale, saving, mode1Dirty, t]);

  // Keep route-driven Mode 1 panel/target synchronized cleanly on navigation.
  useEffect(() => {
    if (!isMode2 && active) {
      if (route.studioPanel) setPanel(route.studioPanel);
      if (route.studioTemplateKey) setSelectedTargetKey(route.studioTemplateKey);
    }
  }, [isMode2, active, route.studioPanel, route.studioTemplateKey]);

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
      const starter = createStarterTemplateDocument(Boolean(targetInfo?.requires_content_slot));
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
      setStatus(t("studio.loading_appearance_customization"));
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
            throw new Error(t("studio.reload_preview_locale_before_saving"));
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
            setThemeOptionsDraft(null);
            setThemeOptionsDirty(false);
            setThemeOptionsVersion(null);
            setThemeOptionsReloadToken(v => v + 1);
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
          setStatus(t("studio.ready_to_customize_the_website_and_build_templates"));
        })
        .catch(cause => {
          if (alive && seq === loadSeqRef.current && authorityRef.current === reqAuthority) {
            setStatus(t("studio.cannot_load_appearance_customization_data"), true);
            onApiError(cause, t("studio.cannot_open_appearance_customization"));
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
    setStatus(t("studio.loading_the_page_builder"));
    Promise.allSettled([
      builderApi.getEntryBuilder(entryTypeKey, entryId),
      getContentEntry(entryTypeKey, entryId),
      listPatterns(),
    ])
      .then(([builderRes, entryRes, patSettled]) => {
        if (!alive || seq !== loadSeqRef.current || authorityRef.current !== reqAuthority) return;
        if (builderRes.status === 'rejected' && isTrashedConflict(builderRes.reason)) {
          setEntryTrashedBlocked(true);
          setStatus(t("entry_lifecycle.studio_entry_trashed_blocked"), true);
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
          setStatus(t("studio.editing_title_v_version", { title: entryRec.title, version: opened.version }));
        } else {
          setConflict(entryVersion !== opened.version);
          setStatus(t("studio.restored_the_unsaved_page_builder_draft_from_the_previous_session"));
        }
        mode2LoadedKeyRef.current = entryIdentity;
      })
      .catch(cause => {
        if (alive && seq === loadSeqRef.current && authorityRef.current === reqAuthority) {
          if (isTrashedConflict(cause)) {
            setEntryTrashedBlocked(true);
            setStatus(t("entry_lifecycle.studio_entry_trashed_blocked"), true);
          } else {
            setStatus(t("studio.cannot_open_the_page_builder_for_this_content"), true);
            onApiError(cause, t("studio.cannot_open_the_page_builder"));
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
    setStatus(t("studio.there_are_unsaved_changes"));
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
              setStatus(t("entry_lifecycle.studio_entry_trashed_blocked"), true);
              return;
            }
            onApiError(cause, t("studio.cannot_create_a_page_builder_preview"));
          });
        return;
      }

      if (!previewTargetCandidate) return;
      const transient: TransientStudioOverrides = {};
      if (!localeDraftConflict && generalDirty && generalDraft) transient.general = generalDraft;
      if (!localeDraftConflict && globalOptionsDirty && globalOptionsDraft) transient.global_options = globalOptionsDraft;
      if (!localeDraftConflict && themeOptionsDirty && themeOptionsDraft) transient.theme_options = themeOptionsDraft;
      if (!localeDraftConflict && menuSlotsDirty && menuSlotsDraft) transient.menu_slots = menuSlotsDraft;

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
        })
        .catch(cause => {
          if (controller.signal.aborted || seq !== previewSeqRef.current) return;
          if (cause instanceof api.ApiClientError && cause.status === 412) {
            setConflict(true);
          } else {
            onApiError(cause, t("studio.cannot_preview_appearance_customization"));
          }
        });
    }, 250);

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
    themeOptionsDirty,
    themeOptionsDraft,
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
    if (templateDirty && nextKey !== selectedTargetKey) {
      if (!window.confirm(t("studio.the_current_template_has_unsaved_changes_discard_them_and_switch_targets"))) {
        return;
      }
    }
    setSelectedTargetKey(nextKey);
    hydrateTemplateTarget(nextKey, targets, overrideDocs);
  };

  const handlePanelChange = (nextPanel: StudioPanelId) => {
    setBuilderView('tree');
    setAddTargetParentId(null);
    setMenuRequest(null);
    setPanel(nextPanel);
    if (nextPanel !== 'templates' && templateDirty) {
      setStatus(t("studio.unsaved_template_changes_are_retained_in_the_current_session"));
    }
  };

  const handleSaveMode1 = async () => {
    if (!active || !user || !csrfToken || loading || savingRef.current || !mode1Dirty || localeDraftBlocked || !hydratedPreviewLocale || hydratedPreviewLocale !== effectivePreviewLocale) return;

    if (readingDirty && readingDraft && readingDraft.homepage_page_id !== null &&
        readingDraft.homepage_page_id === readingDraft.posts_page_id) {
      setStatus(t("studio.the_homepage_and_posts_page_cannot_be_the_same_page"), true);
      return;
    }

    if (templateDirty && selectedTarget) {
      if (selectedTarget.requires_content_slot && countElementsByType(templateDoc, 'core/content_slot') !== 1) {
        setStatus(t("studio.this_template_requires_exactly_one_core_content_slot"), true);
        return;
      }
      if (selectedTarget.forbids_content_slot && countElementsByType(templateDoc, 'core/content_slot') > 0) {
        setStatus(t("studio.this_template_must_not_contain_core_content_slot"), true);
        return;
      }
      const badPart = allNodes(templateDoc).some(
        n =>
          n.kind === 'element' &&
          (n.node as BuilderElement).type === 'core/template_part' &&
          !VALID_TEMPLATE_PART_KEY.test(String((n.node as BuilderElement).props.part_key ?? '')),
      );
      if (badPart) {
        setStatus(t("studio.a_template_part_has_an_invalid_part_key"), true);
        return;
      }
    }

    savingRef.current = true;
    setSaving(true);
    setConflict(false);
    const savedNames: string[] = [];
    try {
      if (generalDirty && generalSaved && generalDraft) {
        const res = await saveGeneralSettings(generalDraft, generalSaved.version, csrfToken, effectivePreviewLocale || undefined);
        setGeneralSaved(res);
        setGeneralDraft({ site_title: res.site_title, tagline: res.tagline, timezone: res.timezone });
        savedNames.push(t("studio.site_identity"));
      }
      if (readingDirty && readingSaved && readingDraft) {
        const res = await saveReadingSettings(readingDraft, readingSaved.version, csrfToken, effectivePreviewLocale || undefined);
        setReadingSaved(res);
        setReadingDraft({ homepage_page_id: res.homepage_page_id, posts_page_id: res.posts_page_id });
        savedNames.push(t("studio.homepage_and_reading"));
      }
      if (menuSlotsDirty && menuSlotsSaved && menuSlotsDraft) {
        const res = await putMenuSlots(menuSlotsSaved.theme_id, menuSlotsDraft, menuSlotsSaved.version, csrfToken, effectivePreviewLocale || undefined);
        setMenuSlotsSaved(res);
        setMenuSlotsDraft(buildSlotAssignmentsMap(res));
        savedNames.push(t("studio.menu_locations"));
      }
      if (globalOptionsDirty && globalOptionsSaved && globalOptionsDraft) {
        const res = await saveGlobalSiteOptions(globalOptionsDraft, globalOptionsSaved.version, csrfToken);
        setGlobalOptionsSaved(res);
        setGlobalOptionsDraft(res.values);
        savedNames.push(t("studio.global_site_options"));
      }
      if (themeOptionsDirty && activeThemeId && themeOptionsDraft && themeOptionsVersion !== null) {
        const res = await saveThemeOptions(activeThemeId, themeOptionsDraft, themeOptionsVersion, csrfToken, effectivePreviewLocale || undefined);
        setThemeOptionsVersion(res.version);
        setThemeOptionsDraft(res.values);
        setThemeOptionsDirty(false);
        setThemeOptionsReloadToken(v => v + 1);
        savedNames.push(t("studio.appearance_options"));
      }
      if (templateDirty && selectedTarget) {
        const res = await builderApi.putTemplateOverride(
          selectedTarget.target,
          templateDoc,
          csrfToken,
          templateSavedVersion !== null && templateSavedVersion > 0 ? templateSavedVersion : undefined,
        );
        setOverrideDocs(prev => ({ ...prev, [res.canonical_target_key]: res }));
        setTemplateSavedVersion(res.version);
        setTemplateSavedDoc(res.ast_json);
        setTemplateDoc(res.ast_json);
        const freshTargets = await getStudioTargets();
        setTargets(freshTargets.items);
        savedNames.push(`Template (${selectedTarget.canonical_target_key})`);
      }

      if (savedNames.length > 0) {
        onSavedSettingsOrTheme?.();
        setStatus(t("studio.saved_names", { names: savedNames.join(", ") }));
      }
    } catch (cause) {
      if (cause instanceof api.ApiClientError && cause.status === 412) {
        setConflict(true);
        setStatus(t("studio.data_changed_in_another_session_412_your_unsaved_changes_are_retained"), true);
      } else {
        setStatus(
          savedNames.length > 0
            ? t("studio.saved_names_but_the_next_step_failed_remaining_changes_are_retained", { names: savedNames.join(", ") })
            : t("studio.saving_failed_drafts_are_retained"),
          true,
        );
        onApiError(cause, t("studio.cannot_save_appearance_customization"));
      }
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const handleResetTemplate = async () => {
    if (!active || !csrfToken || savingRef.current || localeDraftBlocked || !selectedTarget?.override_id || !templateSavedVersion) return;
    if (!window.confirm(t("studio.reset_template_label_to_the_theme_core_default", { label: selectedTarget.label }))) return;
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
      setStatus(t("studio.template_reset_current_source_label", { label: res.next_effective_source.label }));
    } catch (cause) {
      if (cause instanceof api.ApiClientError && cause.status === 412) setConflict(true);
      onApiError(cause, t("studio.cannot_reset_template_override"));
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const handleSaveMode2 = async (intent?: 'draft' | 'published') => {
    if (!active || !user || !csrfToken || !isMode2 || entryId === null || entryVersion === null || savingRef.current || entryTrashedBlocked) return;

    const refs = allNodes(entryDoc)
      .filter(node => node.kind === 'element' && isElement(node.node) && node.node.type === 'core/pattern_ref')
      .map(node => Number((node.node as BuilderElement).props.pattern_id));
    if (refs.some(id => !Number.isSafeInteger(id) || id <= 0) || new Set(refs).size > 16) {
      setStatus(t("studio.each_reusable_pattern_requires_a_valid_pattern_up_to_16_patterns_per_page"), true);
      return;
    }
    if (patternsLoaded && refs.some(id => !patterns.some(p => p.id === id))) {
      setStatus(t("studio.a_referenced_pattern_was_deleted_select_another_pattern_before_saving"), true);
      return;
    }

    savingRef.current = true;
    setSaving(true);
    setConflict(false);
    try {
      let currentVersion = entryVersion;
      let currentStatus = entryPublicationStatus;
      let currentDoc = entrySavedDoc;
      let currentDiagnostics = entryDiagnostics;

      if (entryDirty || (intent !== 'published' && entryHadExistingDoc === false)) {
        const saved = await builderApi.saveEntryBuilder(entryTypeKey, entryId, entryDoc, currentVersion, csrfToken);
        editorDraftStore.clearDraft(entryTypeKey, entryId);
        currentVersion = saved.version;
        currentStatus = saved.publication_status;
        currentDoc = saved.document ?? entryDoc;
        currentDiagnostics = saved.diagnostics ?? [];
        setEntryVersion(currentVersion);
        setEntrySavedDoc(currentDoc);
        setEntryDoc(currentDoc);
        setEntryHadExistingDoc(true);
        setEntryHadFreeformWrap(saved.had_freeform_wrap);
        setEntryHadRecovery(saved.had_recovery);
        setEntryDiagnostics(currentDiagnostics);
      }

      if (intent === 'published' && entryPublicationPolicy === 'draft_publish' && currentStatus !== 'published') {
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

      setEntryPublicationStatus(currentStatus);
      setStatus(
        isPatternEntry
          ? t("studio.saved_pattern_v_version", { version: currentVersion })
          : entryPublicationPolicy === 'draft_publish' && currentStatus === 'draft'
            ? t("studio.saved_page_builder_draft_v_version_content_remains_a_draft", { version: currentVersion })
            : t("studio.saved_and_published_changes_v_version", { version: currentVersion }),
      );
    } catch (cause) {
      if (isTrashedConflict(cause)) {
        setEntryTrashedBlocked(true);
        setStatus(t("entry_lifecycle.studio_entry_trashed_blocked"), true);
      } else if (cause instanceof api.ApiClientError && cause.status === 412) {
        setConflict(true);
        setStatus(t("studio.content_changed_in_another_session_412_your_page_builder_draft_is_retained"), true);
      } else {
        setStatus(t("studio.could_not_save_the_page_builder_your_draft_is_retained"), true);
        onApiError(cause, t("studio.cannot_save_the_page_builder"));
      }
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const handleReloadMode2 = () => {
    if (!isMode2 || !entryTypeKey || entryId === null) return;
    if (entryDirty && !window.confirm(t("studio.reloading_the_server_version_discards_the_current_page_builder_draft_continue"))) return;
    mode2LoadedKeyRef.current = null;
    setConflict(false);
    const seq = ++loadSeqRef.current;
    setLoading(true);
    void builderApi.getEntryBuilder(entryTypeKey, entryId).then(opened => {
      if (seq !== loadSeqRef.current) return;
      const doc = opened.document ?? EMPTY_DOCUMENT;
      setEntryVersion(opened.version);
      setEntryPublicationStatus(opened.publication_status);
      setEntryPublicationPolicy(opened.publication_policy);
      setEntrySavedDoc(doc);
      setEntryDoc(doc);
      setEntrySelectedNodeId(visibleNodesInTree(doc)[0]?.node.id ?? null);
      setEntryDiagnostics(opened.diagnostics ?? []);
      setEntryHadFreeformWrap(opened.had_freeform_wrap);
      setEntryHadRecovery(opened.had_recovery);
      resetHistory();
      setBuilderView('tree');
      setAddTargetParentId(null);
      setClipboard(null);
      setMenuRequest(null);
      setStatus(t("studio.loaded_latest_version_v_version", { version: opened.version }));
    }).catch(cause => onApiError(cause, t("studio.cannot_reload_the_page_builder")))
      .finally(() => { if (seq === loadSeqRef.current) setLoading(false); });
  };

  const handleExit = () => {
    if (isMode2 && entryId !== null) {
      const target = entryTypeKey === 'page'
        ? `#/pages/${entryId}/edit`
        : entryTypeKey === 'post'
          ? `#/posts/${entryId}/edit`
          : entryTypeKey === 'pattern'
            ? `#/patterns/${entryId}/edit`
            : `#/cpt/${encodeURIComponent(entryTypeKey)}/${entryId}/edit`;
      onNavigateHash(target);
      return;
    }
    onNavigateHash('#/dashboard');
  };

  const refreshPatterns = () => {
    if (!user || !csrfToken) return;
    listPatterns()
      .then(res => { setPatterns(res.items); setPatternsLoaded(true); })
      .catch(cause => onApiError(cause, t("studio.cannot_reload_the_pattern_list")));
  };

  const handleMenusMutated = async () => {
    try {
      const res = await listMenus(100, 0);
      setMenusList(res.items);
      setMenuPreviewSeq(v => v + 1);
    } catch (cause) {
      onApiError(cause, t("studio.cannot_reload_the_menu_list"));
    }
  };

  const availablePartKeys = targets
    .filter(t => t.target.kind === 'part' && t.target.part_key)
    .map(t => t.target.part_key as string);

  void canDeleteElement;

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
  const contextTitle = isMode2
    ? entryTitle || `${entryTypeKey} #${entryId}`
    : selectedTarget && panel === 'templates'
      ? selectedTarget.label
      : ({ root: activeThemeName ?? activeThemeId ?? t("studio.default_theme"),
          identity: t("studio.site_identity"), 'theme-options': t("studio.appearance_options"),
          menus: t("studio.menus_and_locations"), reading: t("studio.homepage_and_reading"), templates: t("studio.build_templates") } as Record<string, string>)[panel];
  const contextBadge = isMode2
    ? (entryTypeKey === 'page'
        ? t("studio.page")
        : entryTypeKey === 'post'
          ? t("studio.post")
          : entryTypeKey === 'pattern'
            ? t("builder.document.pattern")
            : entryTypeKey)
    : selectedTarget && panel === 'templates' ? t("studio.template") : panel === 'root' ? t("studio.theme") : undefined;
  const contextBack = isMode2
    ? undefined
    : panel === 'root'
      ? undefined
      : panel === 'templates' && selectedTarget ? t("studio.template_list") : t("studio.customize");
  const handleContextBack = () => {
    if (panel === 'templates' && selectedTarget) handleSelectTarget(null);
    else handlePanelChange('root');
  };
  const handleReloadConflict = () => {
    if (isMode2) {
      handleReloadMode2();
      return;
    }
    if (loading || savingRef.current) return;
    if (mode1Dirty && !window.confirm(t("studio.reloading_from_the_server_discards_unsaved_studio_changes_continue"))) {
      return;
    }
    setLocaleDraftConflict(null);
    setLocaleReloadPending(true);
    setLoading(true);
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
      <div className="flex min-h-0 flex-1 flex-col md:flex-row">
        <aside
          className="flex max-h-[50vh] w-full shrink-0 flex-col overflow-hidden border-b border-zinc-200 bg-white md:max-h-none md:h-full md:w-[320px] md:border-r md:border-b-0"
          aria-label={isMode2 ? t("studio.page_builder_content_panel") : t("studio.studio_customization_panel")}
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
            resetLabel={t("studio.restore_theme_defaults")}
            onReset={() => void handleResetTemplate()}
            conflict={conflict || localeDraftBlocked}
            onReloadConflict={handleReloadConflict}
          />
          {!isMode2 && localeState.hasMultiplePublicLocales && (
            <div className="flex items-center justify-between gap-2 border-b border-zinc-200 bg-zinc-50 px-3 py-2" data-testid="studio-preview-locale-bar">
              <label htmlFor="studio-preview-locale" className="text-[11px] font-semibold uppercase tracking-wide text-zinc-600">
                {t("studio.preview_locale")}
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
                    if (!window.confirm(t("studio.there_are_unsaved_changes_in_the_current_language_discard_them_to_switch_preview_locale"))) {
                      return;
                    }
                    setGeneralDraft(generalSaved ? { site_title: generalSaved.site_title, tagline: generalSaved.tagline, timezone: generalSaved.timezone } : null);
                    setReadingDraft(readingSaved ? { homepage_page_id: readingSaved.homepage_page_id, posts_page_id: readingSaved.posts_page_id } : null);
                    setMenuSlotsDraft(menuSlotsSaved ? buildSlotAssignmentsMap(menuSlotsSaved) : null);
                    setGlobalOptionsDraft(globalOptionsSaved ? globalOptionsSaved.values : null);
                    setThemeOptionsDraft(null);
                    setThemeOptionsDirty(false);
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
              <span className="min-w-0 flex-1 truncate" title={entryTrashedBlocked ? t("entry_lifecycle.studio_entry_trashed_blocked") : localeDraftConflict || previewLocaleInvalid ? t("studio.the_preview_locale_is_no_longer_available_drafts_are_retained_reload_to_switch_to_the_default_locale") : localeReloadPending ? t("studio.reload_preview_locale_before_saving") : notice || t("studio.data_version_conflict")}>
                {entryTrashedBlocked ? t("entry_lifecycle.studio_entry_trashed_blocked") : localeDraftConflict || previewLocaleInvalid ? t("studio.the_preview_locale_is_no_longer_available_drafts_are_retained_reload_to_switch_to_the_default_locale") : localeReloadPending ? t("studio.reload_preview_locale_before_saving") : error && notice ? notice : t("studio.data_changed_on_the_server_412")}
              </span>
              {entryTrashedBlocked && !isPatternEntry ? (
                <button
                  type="button"
                  data-testid="studio-open-trash-link"
                  onClick={() => onNavigateHash(mode2TrashHash)}
                  className="shrink-0 rounded border border-rose-300 bg-white px-2 py-0.5 text-[11px] font-medium text-rose-800 hover:bg-rose-100"
                >
                  {t("entry_lifecycle.open_trash_view")}
                </button>
              ) : (conflict || localeDraftBlocked) && (
                <button
                  type="button"
                  onClick={handleReloadConflict}
                  disabled={loading || saving}
                  className="shrink-0 rounded border border-rose-300 bg-white px-2 py-0.5 text-[11px] font-medium text-rose-800 hover:bg-rose-100"
                >
                  {t("studio.reload")}
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
                themeRevision={themeRevision}
                patterns={patterns}
                patternsLoaded={patternsLoaded}
                onRefreshPatterns={refreshPatterns}
                allowRouteContext={!isPatternEntry}
                allowPatternRef={!isPatternEntry}
                allowList={!isPatternEntry}
                allowContentSlot={false}
                allowTemplatePart={false}
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
              onGlobalOptionsChange={next => setGlobalOptionsDraft(next)}
              themeOptionsDraft={themeOptionsDraft}
              themeOptionsDirty={themeOptionsDirty}
              themeOptionsReloadToken={themeOptionsReloadToken}
              onThemeOptionsChange={(vals, meta) => {
                if (localeDraftBlocked || !hydratedPreviewLocale || hydratedPreviewLocale !== effectivePreviewLocale) return;
                setThemeOptionsDraft(vals);
                setThemeOptionsVersion(meta.version);
                setThemeOptionsDirty(meta.dirty);
              }}
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
                setStatus(t("studio.initialized_the_page_builder_for_label_not_yet_saved_to_the_database", { label: selectedTarget?.label ?? "" }));
              }}
              templateDoc={templateDoc}
              templateSelectedNodeId={templateSelectedNodeId}
              onSelectTemplateNode={setTemplateSelectedNodeId}
              onChangeTemplateDoc={handleBuilderChange}
              templateDirty={templateDirty}
              viewport={viewport}
              themeRevision={themeRevision}
              patterns={patterns}
              patternsLoaded={patternsLoaded}
              onRefreshPatterns={refreshPatterns}
              availablePartKeys={availablePartKeys}
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
          />
        </aside>

        <main className={'flex min-w-0 flex-1 flex-col overflow-hidden ' + (viewport === 'desktop' ? 'bg-white' : 'bg-zinc-200/70')}>
          <div className={'min-h-0 flex-1 overflow-auto ' + (viewport === 'desktop' ? 'p-0' : 'p-4')}>
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
                title={isMode2 ? t("studio.content_preview") : t("studio.website_preview")}
                className="h-full w-full"
                frameClassName={
                  viewport === 'desktop'
                    ? '!h-full !w-full !border-0 !rounded-none !shadow-none'
                    : '!h-full min-h-[540px] !border border-zinc-300 rounded-lg shadow-sm'
                }
              />
            ) : (
              <div className="flex h-full min-h-[360px] items-center justify-center bg-white p-8 text-center text-sm text-zinc-500">
                {loading ? t("studio.preparing_live_preview") : t("studio.no_preview_content_for_this_target_yet")}
              </div>
            )}
          </div>
        </main>
      </div>
    </div>
  );
};
