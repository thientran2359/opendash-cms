import React, { useEffect, useRef, useState } from 'react';
import * as api from '../api/index.ts';
import * as builderApi from '../api/builder.ts';
import { changeContentEntryPublication, getContentEntry, queryContentEntryTable } from '../api/content.ts';
import { getMenuSlots, listMenus, putMenuSlots, type MenuRecord, type MenuSlotsSnapshot } from '../api/menus.ts';
import { listPatterns, type PatternRecord } from '../api/patterns.ts';
import {
  getGeneralSettings,
  getReadingSettings,
  getTimezoneCatalog,
  saveGeneralSettings,
  saveReadingSettings,
  type GeneralSettings,
  type GeneralSettingsDraft,
  type ReadingSettings,
  type ReadingSettingsDraft,
} from '../api/settings.ts';
import { getStudioTargets, postStudioPreview, type StudioTargetItem, type TransientStudioOverrides } from '../api/studio.ts';
import { saveThemeOptions } from '../api/themes.ts';
import { BuilderWorkspace, BuilderEditorCore, type StudioBuilderView } from '../builder/BuilderWorkspace.tsx';
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
  nodeTitle,
  type BuilderDocument,
  type BuilderElement,
  type BuilderViewport,
  type StudioBuilderClipboard,
} from '../builder/document.ts';
import { editorDraftStore } from '../features/editor/EditorDraftStore.ts';
import type { ParsedRoute, StudioPanelId } from '../shell/router.ts';
import type { AuthUser, BlockRegistry } from '../types.ts';
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
  onNavigateHash,
  onDirtyChange,
  onApiError,
  onSavedSettingsOrTheme,
}) => {
  const isMode2 = route.view === 'studio-entry';
  const entryTypeKey = isMode2 ? (route.typeKey ?? '') : '';
  const entryId = isMode2 ? (route.entryId ?? null) : null;

  // Shared UI & preview state
  const [viewport, setViewport] = useState<BuilderViewport>('desktop');
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const savingRef = useRef(false);
  const [previewHtml, setPreviewHtml] = useState('');
  const [previewSkipped, setPreviewSkipped] = useState(0);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [registry, setRegistry] = useState<BlockRegistry>({});
  const [patterns, setPatterns] = useState<PatternRecord[]>([]);
  const [patternsLoaded, setPatternsLoaded] = useState(false);
  const [builderView, setBuilderView] = useState<StudioBuilderView>('tree');
  const [addTargetParentId, setAddTargetParentId] = useState<string | null>(null);
  const [clipboard, setClipboard] = useState<StudioBuilderClipboard | null>(null);
  const [menuRequest, setMenuRequest] = useState<StudioMenuRequest | null>(null);
  const menuRequestSeqRef = useRef(0);
  const [canvasPickSeq, setCanvasPickSeq] = useState(0);

  // Mode 1 state
  const [panel, setPanel] = useState<StudioPanelId>(route.studioPanel ?? 'root');
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

  const [themeOptionsVersion, setThemeOptionsVersion] = useState<number | null>(null);
  const [themeOptionsDraft, setThemeOptionsDraft] = useState<Record<string, unknown> | null>(null);
  const [themeOptionsDirty, setThemeOptionsDirty] = useState(false);
  const [themeOptionsReloadToken, setThemeOptionsReloadToken] = useState(0);
  const [mode1ReloadSeq, setMode1ReloadSeq] = useState(0);

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

  const authority = JSON.stringify([route.view, entryTypeKey, entryId, user?.id, user?.role, csrfToken, resetToken, active]);
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
    setPreviewSkipped(0);
    setConflict(false);
    resetHistory();
    mode1LoadedRef.current = false;
    mode2LoadedKeyRef.current = null;
    setGeneralSaved(null);
    setGeneralDraft(null);
    setReadingSaved(null);
    setReadingDraft(null);
    setMenuSlotsSaved(null);
    setMenuSlotsDraft(null);
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
  const mode1Dirty = generalDirty || readingDirty || menuSlotsDirty || themeOptionsDirty || templateDirty;
  const entryDirty = Boolean(entryVersion !== null && !sameJson(entryDoc, entrySavedDoc));
  const totalDirty = isMode2 ? entryDirty : mode1Dirty;

  useEffect(() => {
    onDirtyChange(totalDirty);
  }, [totalDirty, onDirtyChange]);

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
      setTemplateSelectedNodeId(allNodes(existing.ast_json)[0]?.node.id ?? null);
    } else {
      const targetInfo = items.find(t => t.canonical_target_key === targetKey);
      const starter = createStarterTemplateDocument(Boolean(targetInfo?.requires_content_slot));
      setTemplateEditingEnabled(false);
      setTemplateSavedVersion(null);
      setTemplateSavedDoc(starter);
      setTemplateDoc(starter);
      setTemplateSelectedNodeId(allNodes(starter)[0]?.node.id ?? null);
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
      if (mode1LoadedRef.current && !conflict) {
        return;
      }
      let alive = true;
      setLoading(true);
      setStatus('Đang tải không gian OpenDash Studio...');
      Promise.all([
        getStudioTargets(),
        getGeneralSettings(),
        getTimezoneCatalog(),
        getReadingSettings(),
        queryContentEntryTable('page', { status: 'published', sort: 'title', order: 'asc', limit: 100, offset: 0 }),
        builderApi.getTemplateOverrides(),
        listMenus(100, 0),
        api.getRegistry(),
        listPatterns().catch(() => ({ items: [] as PatternRecord[], total: 0, limit: 50, offset: 0 })),
      ])
        .then(async ([
          targetsRes,
          generalRes,
          tzRes,
          readingRes,
          pagesRes,
          overridesRes,
          menusRes,
          regRes,
          patRes,
        ]) => {
          if (!alive || seq !== loadSeqRef.current || authorityRef.current !== reqAuthority) return;
          let slotsRes: MenuSlotsSnapshot | null = null;
          if (targetsRes.active_theme_id) {
            slotsRes = await getMenuSlots(targetsRes.active_theme_id).catch(() => null);
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

          if (forceReload) {
            setThemeOptionsDraft(null);
            setThemeOptionsDirty(false);
            setThemeOptionsReloadToken(v => v + 1);
          }

          setRegistry(regRes);
          setPatterns(patRes.items);
          setPatternsLoaded(true);

          if (!mode1LoadedRef.current || forceReload || !templateDirty) {
            const initialKey = route.studioTemplateKey ?? selectedTargetKey;
            hydrateTemplateTarget(initialKey, targetsRes.items, byKey);
          }
          mode1LoadedRef.current = true;
          setConflict(false);
          setStatus('Sẵn sàng tùy biến Website & dựng Template.');
        })
        .catch(cause => {
          if (alive && seq === loadSeqRef.current && authorityRef.current === reqAuthority) {
            setStatus('Không thể tải dữ liệu OpenDash Studio.', true);
            onApiError(cause, 'Không thể mở OpenDash Studio.');
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
    setStatus('Đang tải trình dựng nội dung Trình dựng trang...');
    Promise.all([
      builderApi.getEntryBuilder(entryTypeKey, entryId),
      getContentEntry(entryTypeKey, entryId),
      api.getRegistry(),
      listPatterns(),
    ])
      .then(([opened, entryRec, regRes, patRes]) => {
        if (!alive || seq !== loadSeqRef.current || authorityRef.current !== reqAuthority) return;
        setRegistry(regRes);
        setPatterns(patRes.items);
        setPatternsLoaded(true);
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
          setEntrySelectedNodeId(allNodes(serverDoc)[0]?.node.id ?? null);
          resetHistory();
          setConflict(false);
          setStatus(`Đang sửa ${entryRec.title} (v${opened.version}).`);
        } else {
          setConflict(entryVersion !== opened.version);
          setStatus('Đã khôi phục nháp Trình dựng trang chưa lưu từ phiên trước.');
        }
        mode2LoadedKeyRef.current = entryIdentity;
      })
      .catch(cause => {
        if (alive && seq === loadSeqRef.current && authorityRef.current === reqAuthority) {
          setStatus('Không thể mở trình dựng Trình dựng trang cho nội dung này.', true);
          onApiError(cause, 'Không thể mở Trình dựng trang.');
        }
      })
      .finally(() => {
        if (alive && seq === loadSeqRef.current) setLoading(false);
      });
    return () => { alive = false; };
  }, [active, isMode2, entryTypeKey, entryId, user?.id, user?.role, csrfToken, resetToken, themeRevision, mode1ReloadSeq]);

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
    setStatus('Có thay đổi chưa lưu.');
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
    if (!isMode2 && !previewTargetCandidate) return;
    if (isMode2 && (!entryTypeKey || entryId === null || entryVersion === null)) return;

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
            onApiError(cause, 'Không thể tạo bản xem trước Trình dựng trang.');
          });
        return;
      }

      if (!previewTargetCandidate) return;
      const transient: TransientStudioOverrides = {};
      if (generalDirty && generalDraft) transient.general = generalDraft;
      if (themeOptionsDirty && themeOptionsDraft) transient.theme_options = themeOptionsDraft;
      if (menuSlotsDirty && menuSlotsDraft) transient.menu_slots = menuSlotsDraft;

      const editingSelectedTemplate = Boolean(panel === 'templates' && selectedTarget && templateEditingEnabled);
      void postStudioPreview(
        {
          target: previewTargetCandidate,
          document: editingSelectedTemplate ? templateDoc : null,
          preview_annotations: editingSelectedTemplate,
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
            onApiError(cause, 'Không thể xem trước OpenDash Studio.');
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
    themeOptionsDirty,
    themeOptionsDraft,
    menuSlotsDirty,
    menuSlotsDraft,
  ]);

  const handleSelectTarget = (nextKey: string | null) => {
    if (templateDirty && nextKey !== selectedTargetKey) {
      if (!window.confirm('Template đang chỉnh có thay đổi chưa lưu. Bỏ thay đổi và chuyển target khác?')) {
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
      setStatus('Các thay đổi Template chưa lưu vẫn được giữ trong phiên hiện tại.');
    }
  };

  const handleSaveMode1 = async () => {
    if (!active || !user || !csrfToken || savingRef.current || !mode1Dirty) return;

    if (readingDirty && readingDraft && readingDraft.homepage_page_id !== null &&
        readingDraft.homepage_page_id === readingDraft.posts_page_id) {
      setStatus('Trang chủ và Trang bài viết không thể trùng cùng một Page.', true);
      return;
    }

    if (templateDirty && selectedTarget) {
      if (selectedTarget.requires_content_slot && countElementsByType(templateDoc, 'core/content_slot') !== 1) {
        setStatus('Template này bắt buộc có đúng 1 core/content_slot.', true);
        return;
      }
      if (selectedTarget.forbids_content_slot && countElementsByType(templateDoc, 'core/content_slot') > 0) {
        setStatus('Template này không được chứa core/content_slot.', true);
        return;
      }
      const badPart = allNodes(templateDoc).some(
        n =>
          n.kind === 'element' &&
          (n.node as BuilderElement).type === 'core/template_part' &&
          !VALID_TEMPLATE_PART_KEY.test(String((n.node as BuilderElement).props.part_key ?? '')),
      );
      if (badPart) {
        setStatus('Có Template Part chứa part_key không hợp lệ.', true);
        return;
      }
    }

    savingRef.current = true;
    setSaving(true);
    setConflict(false);
    const savedNames: string[] = [];
    try {
      if (generalDirty && generalSaved && generalDraft) {
        const res = await saveGeneralSettings(generalDraft, generalSaved.version, csrfToken);
        setGeneralSaved(res);
        setGeneralDraft({ site_title: res.site_title, tagline: res.tagline, timezone: res.timezone });
        savedNames.push('Nhận diện Site');
      }
      if (readingDirty && readingSaved && readingDraft) {
        const res = await saveReadingSettings(readingDraft, readingSaved.version, csrfToken);
        setReadingSaved(res);
        setReadingDraft({ homepage_page_id: res.homepage_page_id, posts_page_id: res.posts_page_id });
        savedNames.push('Trang chủ & Đọc');
      }
      if (menuSlotsDirty && menuSlotsSaved && menuSlotsDraft) {
        const res = await putMenuSlots(menuSlotsSaved.theme_id, menuSlotsDraft, menuSlotsSaved.version, csrfToken);
        setMenuSlotsSaved(res);
        setMenuSlotsDraft(buildSlotAssignmentsMap(res));
        savedNames.push('Vị trí Menu');
      }
      if (themeOptionsDirty && activeThemeId && themeOptionsDraft && themeOptionsVersion !== null) {
        const res = await saveThemeOptions(activeThemeId, themeOptionsDraft, themeOptionsVersion, csrfToken);
        setThemeOptionsVersion(res.version);
        setThemeOptionsDraft(res.values);
        setThemeOptionsDirty(false);
        setThemeOptionsReloadToken(v => v + 1);
        savedNames.push('Tùy chọn Giao diện');
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
        setStatus(`Đã lưu: ${savedNames.join(', ')}.`);
      }
    } catch (cause) {
      if (cause instanceof api.ApiClientError && cause.status === 412) {
        setConflict(true);
        setStatus('Dữ liệu đã thay đổi ở phiên khác (412). Các thay đổi chưa lưu của bạn vẫn được giữ.', true);
      } else {
        setStatus(
          savedNames.length > 0
            ? `Đã lưu ${savedNames.join(', ')}, nhưng bước kế tiếp thất bại; phần còn lại được giữ nguyên.`
            : 'Lưu thay đổi thất bại; nháp vẫn được giữ.',
          true,
        );
        onApiError(cause, 'Không thể lưu OpenDash Studio.');
      }
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const handleResetTemplate = async () => {
    if (!active || !csrfToken || savingRef.current || !selectedTarget?.override_id || !templateSavedVersion) return;
    if (!window.confirm(`Reset template "${selectedTarget.label}" về mẫu mặc định của theme/core?`)) return;
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
      setStatus(`Đã reset template. Nguồn hiện tại: ${res.next_effective_source.label}`);
    } catch (cause) {
      if (cause instanceof api.ApiClientError && cause.status === 412) setConflict(true);
      onApiError(cause, 'Không thể reset template override.');
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const handleSaveMode2 = async (intent?: 'draft' | 'published') => {
    if (!active || !user || !csrfToken || !isMode2 || entryId === null || entryVersion === null || savingRef.current) return;

    const refs = allNodes(entryDoc)
      .filter(node => node.kind === 'element' && isElement(node.node) && node.node.type === 'core/pattern_ref')
      .map(node => Number((node.node as BuilderElement).props.pattern_id));
    if (refs.some(id => !Number.isSafeInteger(id) || id <= 0) || new Set(refs).size > 16) {
      setStatus('Mỗi Reusable Pattern cần chọn một Pattern hợp lệ (tối đa 16 Pattern/trang).', true);
      return;
    }
    if (patternsLoaded && refs.some(id => !patterns.some(p => p.id === id))) {
      setStatus('Một Pattern tham chiếu đã bị xóa. Hãy chọn Pattern khác trước khi lưu.', true);
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
        entryPublicationPolicy === 'draft_publish' && currentStatus === 'draft'
          ? `Đã lưu bản nháp Trình dựng trang (v${currentVersion}). Nội dung vẫn ở trạng thái draft.`
          : `Đã lưu và cập nhật công khai (v${currentVersion}).`,
      );
    } catch (cause) {
      if (cause instanceof api.ApiClientError && cause.status === 412) {
        setConflict(true);
        setStatus('Nội dung đã thay đổi ở phiên khác (412). Nháp Trình dựng trang của bạn vẫn còn.', true);
      } else {
        setStatus('Lưu Trình dựng trang thất bại; nháp của bạn vẫn được giữ.', true);
        onApiError(cause, 'Không thể lưu Trình dựng trang.');
      }
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const handleResetMode2 = async () => {
    if (!active || !csrfToken || !isMode2 || entryId === null || entryVersion === null || !entryHadExistingDoc || savingRef.current) return;
    if (!window.confirm('Chuyển nội dung về văn bản thuần (rút gọn comment blocks)?')) return;
    savingRef.current = true;
    setSaving(true);
    try {
      const res = await builderApi.resetEntryBuilder(entryTypeKey, entryId, entryVersion, csrfToken);
      editorDraftStore.clearDraft(entryTypeKey, entryId);
      const nextDoc = res.document ?? EMPTY_DOCUMENT;
      setEntryVersion(res.version);
      setEntrySavedDoc(nextDoc);
      setEntryDoc(nextDoc);
      setEntrySelectedNodeId(allNodes(nextDoc)[0]?.node.id ?? null);
      setEntryDiagnostics(res.diagnostics ?? []);
      setEntryHadFreeformWrap(res.had_freeform_wrap);
      setEntryHadRecovery(res.had_recovery);
      resetHistory();
      setBuilderView('tree');
      setAddTargetParentId(null);
      setClipboard(null);
      setMenuRequest(null);
      setStatus(`Đã đưa nội dung về văn bản thuần (v${res.version}).`);
    } catch (cause) {
      if (cause instanceof api.ApiClientError && cause.status === 412) setConflict(true);
      onApiError(cause, 'Không thể reset nội dung Trình dựng trang.');
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const handleReloadMode2 = () => {
    if (!isMode2 || !entryTypeKey || entryId === null) return;
    if (entryDirty && !window.confirm('Tải lại bản trên server sẽ bỏ nháp Trình dựng trang hiện tại. Tiếp tục?')) return;
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
      setEntrySelectedNodeId(allNodes(doc)[0]?.node.id ?? null);
      setEntryDiagnostics(opened.diagnostics ?? []);
      setEntryHadFreeformWrap(opened.had_freeform_wrap);
      setEntryHadRecovery(opened.had_recovery);
      resetHistory();
      setBuilderView('tree');
      setAddTargetParentId(null);
      setClipboard(null);
      setMenuRequest(null);
      setStatus(`Đã tải bản mới nhất v${opened.version}.`);
    }).catch(cause => onApiError(cause, 'Không thể tải lại Trình dựng trang.'))
      .finally(() => { if (seq === loadSeqRef.current) setLoading(false); });
  };

  const handleExit = () => {
    if (isMode2 && entryId !== null) {
      const target = entryTypeKey === 'page'
        ? `#/pages/${entryId}/edit`
        : entryTypeKey === 'post'
          ? `#/posts/${entryId}/edit`
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
      .catch(cause => onApiError(cause, 'Không thể tải lại danh sách Pattern.'));
  };

  const availablePartKeys = targets
    .filter(t => t.target.kind === 'part' && t.target.part_key)
    .map(t => t.target.part_key as string);

  // Reference BuilderWorkspace helper check keeps legacy Pattern Builder export tree alive cleanly.
  void BuilderWorkspace;
  void canDeleteElement;

  const canSaveMode2 =
    entryDirty ||
    (entryPublicationPolicy === 'draft_publish' && entryPublicationStatus === 'draft');

  const builderActive = isMode2 || Boolean(panel === 'templates' && selectedTarget && templateEditingEnabled);
  const handleCanvasMenu = (id: string, mode: StudioMenuRequest['mode']) => {
    if (!builderActive || !locate(activeDoc, id)) return;
    if (isMode2) setEntrySelectedNodeId(id);
    else setTemplateSelectedNodeId(id);
    setAddTargetParentId(null);
    setBuilderView('tree');
    setMenuRequest({ nodeId: id, mode, seq: ++menuRequestSeqRef.current });
  };
  const handleCanvasPick = (id: string) => {
    setCanvasPickSeq(seq => seq + 1);
    setMenuRequest(null);
    if (isMode2) setEntrySelectedNodeId(id);
    else if (panel === 'templates' && templateEditingEnabled) setTemplateSelectedNodeId(id);
  };
  const contextualNode = locate(activeDoc, activeSelectedId);
  const addParent = locate(activeDoc, addTargetParentId);
  const contextualSubView = builderActive && builderView !== 'tree';
  const contextTitle = contextualSubView
    ? builderView === 'properties' && contextualNode
      ? nodeTitle(contextualNode)
      : addParent ? `Thêm vào ${nodeTitle(addParent)}` : 'Thêm Section'
    : isMode2 ? entryTitle || `${entryTypeKey} #${entryId}`
      : selectedTarget && panel === 'templates' ? selectedTarget.label
        : ({ root: activeThemeName ?? activeThemeId ?? 'Theme mặc định',
            identity: 'Nhận diện Site', 'theme-options': 'Tùy chọn Giao diện',
            menus: 'Vị trí Menu', reading: 'Trang chủ & Đọc', templates: 'Dựng Template' } as Record<string, string>)[panel];
  const contextBadge = contextualSubView ? undefined : isMode2
    ? entryTypeKey === 'page' ? 'Page' : entryTypeKey === 'post' ? 'Post' : entryTypeKey
    : selectedTarget && panel === 'templates' ? 'Template' : panel === 'root' ? 'Theme' : undefined;
  const contextBack = contextualSubView ? 'Cấu trúc trang' : isMode2
    ? entryTypeKey === 'page' ? 'Về danh sách trang'
      : entryTypeKey === 'post' ? 'Về danh sách bài viết' : `Về danh sách ${entryTypeKey}`
    : panel === 'root' ? undefined : panel === 'templates' && selectedTarget
      ? 'Danh sách Template' : 'Tùy biến';
  const handleContextBack = () => {
    if (contextualSubView) {
      setBuilderView('tree');
      setAddTargetParentId(null);
    } else if (isMode2) {
      onNavigateHash(entryTypeKey === 'page' ? '#/pages'
        : entryTypeKey === 'post' ? '#/posts' : `#/cpt/${encodeURIComponent(entryTypeKey)}`);
    } else if (panel === 'templates' && selectedTarget) handleSelectTarget(null);
    else handlePanelChange('root');
  };
  const canResetStudio = isMode2
    ? entryHadExistingDoc && (entryVersion ?? 0) > 0
    : Boolean(panel === 'templates' && selectedTarget?.override_id && (templateSavedVersion ?? 0) > 0);

  return (
    <div className="flex h-full min-h-0 w-full flex-col bg-zinc-100 text-zinc-900" data-testid="studio-workspace">
      <div className="flex min-h-0 flex-1 flex-col md:flex-row">
        <aside
          className="flex max-h-[50vh] w-full shrink-0 flex-col overflow-hidden border-b border-zinc-200 bg-white md:max-h-none md:h-full md:w-[320px] md:border-r md:border-b-0"
          aria-label={isMode2 ? 'Bảng dựng nội dung Trình dựng trang' : 'Bảng tùy biến Studio'}
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
            canReset={canResetStudio && builderView === 'tree' && builderActive}
            resetLabel={isMode2 ? 'Về văn bản thuần' : 'Reset mặc định'}
            onReset={isMode2 ? () => void handleResetMode2() : () => void handleResetTemplate()}
          />
          <div className="min-h-0 flex-1 overflow-y-auto">
          {isMode2 ? (
            <div className="p-3.5" data-testid="studio-mode2-sidebar">
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
                document={entryDoc}
                selectedId={entrySelectedNodeId}
                onSelect={setEntrySelectedNodeId}
                onChangeDocument={handleBuilderChange}
                registry={registry}
                csrfToken={csrfToken}
                viewport={viewport}
                themeRevision={themeRevision}
                patterns={patterns}
                patternsLoaded={patternsLoaded}
                onRefreshPatterns={refreshPatterns}
                allowRouteContext={true}
                allowPatternRef={true}
                allowList={true}
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
              panel={panel}
              onPanelChange={handlePanelChange}
              activeThemeId={activeThemeId}
              generalDraft={generalDraft}
              generalDirty={generalDirty}
              timezones={timezones}
              onGeneralChange={next => setGeneralDraft(next)}
              csrfToken={csrfToken}
              themeOptionsDraft={themeOptionsDraft}
              themeOptionsDirty={themeOptionsDirty}
              themeOptionsReloadToken={themeOptionsReloadToken}
              onThemeOptionsChange={(vals, meta) => {
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
              onNavigateHash={onNavigateHash}
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
                setStatus(`Đã khởi tạo bản dựng Trình dựng trang cho "${selectedTarget?.label ?? ''}". Chưa lưu lên CSDL.`);
              }}
              templateDoc={templateDoc}
              templateSelectedNodeId={templateSelectedNodeId}
              onSelectTemplateNode={setTemplateSelectedNodeId}
              onChangeTemplateDoc={handleBuilderChange}
              templateDirty={templateDirty}
              registry={registry}
              viewport={viewport}
              themeRevision={themeRevision}
              patterns={patterns}
              patternsLoaded={patternsLoaded}
              onRefreshPatterns={refreshPatterns}
              availablePartKeys={availablePartKeys}
              onBuilderMessage={(msg, failed) => setStatus(msg, Boolean(failed))}
              onApiError={onApiError}
              disabled={loading || saving || !csrfToken}
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
              ? { mode: 'entry', publicationPolicy: entryPublicationPolicy, publicationStatus: entryPublicationStatus }
              : { mode: 'customizer' }}
            canSave={isMode2 ? canSaveMode2 : mode1Dirty}
            onSaveCustomizer={() => void handleSaveMode1()}
            onSaveEntry={intent => void handleSaveMode2(intent)}
          />
        </aside>

        <main className="flex min-w-0 flex-1 flex-col overflow-hidden bg-zinc-200/70">
          {(notice || error || conflict || previewSkipped > 0) && (
            <div
              role={error ? 'alert' : 'status'}
              data-testid="studio-status-bar"
              className={
                'flex flex-wrap items-center justify-between gap-2 border-b px-4 py-2 text-xs ' +
                (error
                  ? 'border-rose-200 bg-rose-50 text-rose-800'
                  : 'border-zinc-200 bg-white/90 text-zinc-700')
              }
            >
              <span>
                {loading ? 'Đang tải… ' : ''}
                {notice}
                {previewSkipped > 0 && ` (${previewSkipped} ảnh riêng tư bỏ qua trong preview)`}
              </span>
              {conflict && (
                <button
                  type="button"
                  onClick={() => {
                    if (isMode2) handleReloadMode2();
                    else {
                      if (mode1Dirty && !window.confirm('Tải lại từ server sẽ bỏ các thay đổi chưa lưu trong Studio. Tiếp tục?')) {
                        return;
                      }
                      mode1ForceReloadRef.current = true;
                      mode1LoadedRef.current = false;
                      setConflict(false);
                      setMode1ReloadSeq(v => v + 1);
                    }
                  }}
                  className="rounded border border-zinc-300 bg-white px-2.5 py-1 text-xs font-medium text-zinc-800 hover:bg-zinc-50"
                >
                  Tải lại từ server
                </button>
              )}
            </div>
          )}

          <div className="min-h-0 flex-1 overflow-auto p-3 sm:p-5">
            {previewHtml ? (
              <BuilderPreviewFrame
                html={previewHtml}
                viewport={viewport}
                interactive={isMode2 || Boolean(panel === 'templates' && selectedTarget && templateEditingEnabled)}
                selectedId={activeSelectedId}
                document={activeDoc}
                onSelectNode={handleCanvasPick}
                onOpenNodeMenu={handleCanvasMenu}
                onApiError={onApiError}
                title={isMode2 ? 'Xem trước nội dung OpenDash Studio' : 'Xem trước Website OpenDash Studio'}
                className="h-full"
                frameClassName="h-full min-h-[540px] !border border-zinc-300 rounded-lg"
                onPrepared={({ skipped }) => setPreviewSkipped(skipped)}
              />
            ) : (
              <div className="flex h-full min-h-[360px] items-center justify-center rounded-xl border border-dashed border-zinc-300 bg-white/70 p-8 text-center text-sm text-zinc-500">
                {loading ? 'Đang chuẩn bị bản xem trước trực tiếp…' : 'Chưa có nội dung xem trước cho target này.'}
              </div>
            )}
          </div>
        </main>
      </div>
    </div>
  );
};
