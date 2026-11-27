import { __, useI18n } from '../../i18n/index.tsx';
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  createEntryComposite,
  contentEntryPreviewPath,
  effectiveContentTypeSettings,
  getContentEntry,
  getEffectiveEditorConfig,
  listContentEntries,
  listContentTypes,
  trashContentEntry,
  type EffectiveEditorConfigWithSettings,
  updateEntryComposite,
} from '../../api/content.ts';
import { getReadingSettings } from '../../api/settings.ts';
import { listAllTaxonomyTerms, resolveTaxonomyTermLabels } from '../../api/taxonomy.ts';
import { runPluginAction } from '../../api/plugins.ts';
import { isRestoreEntryOutcome, type RestoreRevisionResponse } from '../../api/revisions.ts';
import { getEntryRoute, getEntrySeo } from '../../api/seo.ts';
import { ApiClientError } from '../../api/client.ts';
import { getActiveThemeLayouts, type ThemeLayoutCatalog } from '../../api/themes.ts';
import { EMPTY_ADMIN_LOCALE_STATE, type AdminLocaleState } from '../../api/locales.ts';
import type { AuthUser, PluginLifecycleConfirmation as PluginLifecycleConfirmationDto } from '../../types.ts';
import type { ContentEntryRecord, ContentFieldValue, TranslationLinkSummary } from '../content/types.ts';
import { editorDraftStore, diffEditorPayload, type EditorDraftPayload } from './EditorDraftStore.ts';
import { PanelRegionRenderer } from './PanelRegionRenderer.tsx';
import { SingleEditBuilderPromptDialog } from './SingleEditBuilderPromptDialog.tsx';
import { SingleEditTranslationsPanel } from './SingleEditTranslationsPanel.tsx';
import {
  displayEditorConfigWithActiveKeys,
  matchesMediaPickerKind,
  supportAllowsField,
  validateRequiredEditorFields,
} from './editorFieldValidation.ts';
import { useSiteTimezone } from '../../fields/useSiteTimezone.ts';
import { MediaPicker } from '../media/MediaPicker.tsx';
import { EntryListPage, entryTableHash, parseEntryTableHash, safePublicPath } from '../content/EntryListPage.tsx';
import { PluginLifecycleConfirmation } from '../plugins/PluginLifecycleConfirmation.tsx';
import { PublishBoxRevisionsControl } from '../revisions/PublishBoxRevisionsControl.tsx';
import { SingleEditRevisionsOverlay } from '../revisions/SingleEditRevisionsOverlay.tsx';
import {
  buildRestoredEditorPayload,
  useSingleEditRevisions,
} from '../revisions/useSingleEditRevisions.ts';

interface FieldMediaPickerTarget {
  fieldKey: string;
  kind: 'image' | 'gallery';
  scopeKey: string;
  sessionEpoch: number;
  schemaToken: string;
}

interface SingleEditWorkspaceProps {
  typeKey: string;
  mode: 'list' | 'new' | 'edit';
  entryId: number | null;
  user: AuthUser | null;
  siteInstanceId: string;
  authGeneration: number;
  draftUuid?: string;
  routeHash?: string;
  isActive?: boolean;
  csrfToken: string | null;
  localeState?: AdminLocaleState;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange: (dirty: boolean) => void;
  onNavigateHash: (hash: string) => void;
}

function slugFromTitle(title: string): string {
  const s = title
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 64);
  return s && /^[a-z]/.test(s) ? s : `item-${Date.now().toString(36).slice(-4)}`;
}

export const SingleEditWorkspace: React.FC<SingleEditWorkspaceProps> = ({
  typeKey,
  mode,
  entryId,
  user,
  siteInstanceId,
  authGeneration,
  draftUuid,
  routeHash,
  isActive = true,
  csrfToken,
  localeState = EMPTY_ADMIN_LOCALE_STATE,
  onApiError,
  onDirtyChange,
  onNavigateHash,
}) => {
  useI18n();
  const siteTimezone = useSiteTimezone();
  const defaultLocaleCode = localeState.defaultLocale?.code ?? '';
  const [entryLocale, setEntryLocale] = useState<string>(defaultLocaleCode);
  const [translations, setTranslations] = useState<TranslationLinkSummary[]>([]);
  useEffect(() => {
    if (entryId === null && !entryLocale && defaultLocaleCode) {
      setEntryLocale(defaultLocaleCode);
    }
  }, [entryId, entryLocale, defaultLocaleCode]);
  // Pattern is a private, Builder-only CPT. Its metadata mutations go through
  // PatternsPage (/api/admin/patterns); generic entry editing must not expose
  // public URLs, publication controls or a second write path for Patterns.
  const isPattern = typeKey === 'pattern';
  const [config, setConfig] = useState<EffectiveEditorConfigWithSettings | null>(null);
  const [title, setTitle] = useState('');
  const [slug, setSlug] = useState('');
  const [publicationStatus, setPublicationStatus] = useState<'draft' | 'published'>('draft');
  const [builderPromptOpen, setBuilderPromptOpen] = useState(false);
  const [values, setValues] = useState<Record<string, ContentFieldValue>>({});
  const [clearedFields, setClearedFields] = useState<string[]>([]);
  const [termIds, setTermIds] = useState<number[]>([]);
  const [primaryCategoryTermId, setPrimaryCategoryTermId] = useState<number | null>(null);
  const [parentId, setParentId] = useState<number | null>(null);
  const [menuOrder, setMenuOrder] = useState<number>(0);
  const [parentCandidates, setParentCandidates] = useState<ContentEntryRecord[]>([]);
  const [readingSettings, setReadingSettings] = useState<{ homepage_page_id: number | null; posts_page_id: number | null } | null>(null);
  const [termKindById, setTermKindById] = useState<Map<number, string>>(() => new Map());
  const [customPath, setCustomPath] = useState('');
  const [themeLayouts, setThemeLayouts] = useState<ThemeLayoutCatalog | null>(null);
  const [selectedThemeKey, setSelectedThemeKey] = useState<string | null>(null);
  const [themeKeyDirty, setThemeKeyDirty] = useState(false);
  const [entryTimestamps, setEntryTimestamps] = useState<{ createdAt: string; updatedAt: string } | null>(null);
  const [customCreatedAt, setCustomCreatedAt] = useState<string | null>(null);
  const [customUpdatedAt, setCustomUpdatedAt] = useState<string | null>(null);
  const savedThemeKeyRef = useRef<string | null>(null);
  const selectedThemeKeyRef = useRef<string | null>(null);
  const themeKeyDirtyRef = useRef(false);
  const [seoTitle, setSeoTitle] = useState('');
  const [seoDescription, setSeoDescription] = useState('');
  const [seoOgImageId, setSeoOgImageId] = useState('');
  const [seoNoindex, setSeoNoindex] = useState(false);
  const [collapsedPanels, setCollapsedPanels] = useState<Record<string, boolean>>({});
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});
  const [actionOutputs, setActionOutputs] = useState<Record<string, string>>({});
  const [actionBusy, setActionBusy] = useState<Record<string, boolean>>({});
  const [pendingActionConfirmation, setPendingActionConfirmation] = useState<{
    pluginId: string;
    actionId: string;
    panelId: string;
    confirmation: PluginLifecycleConfirmationDto;
  } | null>(null);
  const [trashedBlockedReason, setTrashedBlockedReason] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const saveInFlightRef = useRef(false);
  const [dirty, setDirty] = useState(false);
  const [editRevision, setEditRevision] = useState(0);
  const [hasLocalRestoredDraft, setHasLocalRestoredDraft] = useState(false);
  const [statusNote, setStatusNote] = useState<string | null>(null);
  const [confirmedPublicLink, setConfirmedPublicLink] = useState<{
    scope: string; version: number; path: string | null;
  } | null>(null);
  const [mediaPicker, setMediaPicker] = useState<FieldMediaPickerTarget | null>(null);
  const baselineRef = useRef<EditorDraftPayload | null>(null);
  const draftRef = useRef<EditorDraftPayload | null>(null);
  const editRevisionRef = useRef(0);
  const schemaTokenRef = useRef('');
  const canWriteRouteRef = useRef(true);
  const canWriteSeoRef = useRef(true);

  const settings = config ? effectiveContentTypeSettings(typeKey, config.settings) : null;
  const entryTermPairs = useMemo(
    () =>
      termIds
        .map(id => {
          const kind = termKindById.get(id);
          return kind ? ([kind, id] as const) : null;
        })
        .filter((pair): pair is readonly [string, number] => pair !== null),
    [termIds, termKindById],
  );
  const editorProjection = useMemo(() => {
    if (!config || !settings) return null;
    const specialPage =
      typeKey === 'page' && entryId !== null
        ? entryId === readingSettings?.homepage_page_id
          ? 'homepage'
          : entryId === readingSettings?.posts_page_id
            ? 'posts_page'
            : null
        : null;
    return displayEditorConfigWithActiveKeys(config, settings, {
      content_type: typeKey,
      page_layout: typeKey === 'page' ? selectedThemeKey : null,
      custom_template: typeKey !== 'page' ? selectedThemeKey : null,
      special_page: specialPage,
      entry_term_pairs: entryTermPairs,
      entry_id: entryId,
      user_role: user?.role === 'admin' ? 'admin' : 'editor',
    });
  }, [config, settings, typeKey, entryId, readingSettings, selectedThemeKey, entryTermPairs, user?.role]);
  const editorConfig = editorProjection?.projected ?? null;
  const activeFieldKeys = editorProjection?.activeFieldKeys;

  const tabUuid = useRef(Math.random().toString(36).slice(2, 10));
  const draftKey = editorDraftStore.makeDraftKey(typeKey, entryId, draftUuid ?? tabUuid.current);
  const activeDraftKeyRef = useRef(draftKey);
  activeDraftKeyRef.current = draftKey;
  // Scope both modal visibility and eventual Apply to the complete editor
  // identity. A late callback from a prior entry/account may never mutate it.
  const mediaScopeKey = JSON.stringify([
    siteInstanceId, user?.id, user?.role, authGeneration,
    typeKey, mode, entryId, draftKey,
  ]);
  const activeMediaScopeRef = useRef(mediaScopeKey);
  activeMediaScopeRef.current = mediaScopeKey;
  const activeMediaSchemaRef = useRef(config?.schema_token);
  activeMediaSchemaRef.current = config?.schema_token;

  useEffect(() => {
    setMediaPicker(null);
    setPendingActionConfirmation(null);
    setTrashedBlockedReason(null);
  }, [mediaScopeKey]);

  const applyMediaSelection = (target: FieldMediaPickerTarget, ids: string[]) => {
    const current = draftRef.current;
    const eff = config?.effective_fields[target.fieldKey];
    if (
      !csrfToken || !user || !current || !config || mode === 'list'
      || target.scopeKey !== activeMediaScopeRef.current
      || target.sessionEpoch !== editorDraftStore.getSessionEpoch()
      || target.schemaToken !== activeMediaSchemaRef.current
      || !eff || !eff.editable || eff.effective_status !== 'active'
      || !supportAllowsField(typeKey, target.fieldKey, effectiveContentTypeSettings(typeKey, config.settings))
      || !matchesMediaPickerKind(eff.definition.type, target.kind)
    ) return;
    setMediaPicker(null);
    // Trust the picker-provided ordered selection, not any stale React values
    // snapshot; the draft ref is authoritative across fast field edits.
    const nextIds = [...new Set(ids)].slice(0, target.kind === 'image' ? 1 : 50);
    const nextValues: Record<string, ContentFieldValue> = {
      ...current.values,
      [target.fieldKey]: target.kind === 'image' ? (nextIds[0] ?? '') : nextIds,
    };
    const nextCleared = current.cleared_fields.filter(k => k !== target.fieldKey);
    setValues(nextValues);
    setClearedFields(nextCleared);
    persistDraftPatch({ values: nextValues, cleared_fields: nextCleared });
  };

  const listHashPrefix = typeKey === 'post' ? '#/posts' : typeKey === 'page' ? '#/pages'
    : isPattern ? '#/patterns' : `#/cpt/${typeKey}`;

  const applyPayload = (payload: EditorDraftPayload) => {
    setTitle(payload.title);
    setSlug(payload.slug);
    setPublicationStatus(payload.publication_status);
    setValues(payload.values);
    setClearedFields(payload.cleared_fields);
    setTermIds(payload.term_ids);
    setPrimaryCategoryTermId(payload.primary_category_term_id);
    setParentId(payload.parent_id ?? null);
    setMenuOrder(payload.menu_order ?? 0);
    setCustomPath(payload.custom_path ?? '');
    setSeoTitle(payload.seo?.title ?? '');
    setSeoDescription(payload.seo?.description ?? '');
    setSeoOgImageId(payload.seo?.og_image_id ?? '');
    setSeoNoindex(Boolean(payload.seo?.noindex));
    if (payload.locale) setEntryLocale(payload.locale);
    const nextThemeKey = typeKey === 'page'
      ? (payload.page_layout_key ?? null)
      : (payload.custom_template_key ?? null);
    selectedThemeKeyRef.current = nextThemeKey;
    setSelectedThemeKey(nextThemeKey);
    const themeDiffers = nextThemeKey !== savedThemeKeyRef.current;
    themeKeyDirtyRef.current = themeDiffers;
    setThemeKeyDirty(themeDiffers);
  };

  const persistDraftPatch = (patch: Partial<EditorDraftPayload>) => {
    const previous = draftRef.current;
    const baseline = baselineRef.current;
    if (!user || !config || !previous || !baseline || mode === 'list') return;
    const updated = { ...previous, ...patch };
    draftRef.current = updated;
    editRevisionRef.current += 1;
    setEditRevision(editRevisionRef.current);
    const changed = diffEditorPayload(baseline, updated);
    setDirty(changed.length > 0 || themeKeyDirtyRef.current);
    if (!changed.length) {
      editorDraftStore.delete(draftKey);
      return;
    }
    editorDraftStore.set({
      site_instance_id: siteInstanceId,
      user_id: user.id,
      auth_generation: authGeneration,
      draft_key: draftKey,
      type_key: typeKey,
      status: 'active',
      schema_token_at_edit: schemaTokenRef.current,
      dirty_fields: changed,
      server_values: { ...baseline.values },
      payload: {
        ...updated,
        custom_path_dirty: changed.includes('custom_path'),
        seo_dirty: changed.includes('seo'),
      },
    });
  };

  const handleTrashedConflict = useCallback((message: string) => {
    setTrashedBlockedReason(message);
    setStatusNote(
      __(
        "This entry was moved to Trash in another session (409). Your unsaved draft is retained; restore the entry from Trash before saving.",
      ),
    );
  }, []);

  const revisions = useSingleEditRevisions({
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
    onApplyMergedAutosave: (merged, nextSelectedThemeKey) => {
      selectedThemeKeyRef.current = nextSelectedThemeKey;
      setSelectedThemeKey(nextSelectedThemeKey);
      const themeDiffers = nextSelectedThemeKey !== savedThemeKeyRef.current;
      themeKeyDirtyRef.current = themeDiffers;
      setThemeKeyDirty(themeDiffers);
      persistDraftPatch(merged);
      applyPayload(merged);
    },
    onTrashedConflict: handleTrashedConflict,
    onStatusNote: setStatusNote,
    onApiError,
  });

  useEffect(() => {
    onDirtyChange(dirty);
  }, [dirty, onDirtyChange]);

  useEffect(() => {
    setConfirmedPublicLink(null);
    setThemeLayouts(null);
    if (!user) {
      setConfig(null);
      baselineRef.current = null;
      draftRef.current = null;
      setDirty(false);
      setHasLocalRestoredDraft(false);
      return;
    }
    let active = true;
    const reqEpoch = editorDraftStore.getSessionEpoch();
    setConfig(null);
    setStatusNote(null);
    if (isPattern) {
      baselineRef.current = null;
      draftRef.current = null;
      setDirty(false);
      setHasLocalRestoredDraft(false);
      return;
    }

    (async () => {
      try {
        if (mode === 'list') {
          baselineRef.current = null;
          draftRef.current = null;
          setDirty(false);
          setHasLocalRestoredDraft(false);
          return;
        }

        const effectiveConfig = await getEffectiveEditorConfig(typeKey);
        // Older effective-editor-config projections omit C1 settings. The
        // canonical content-type read DTO still carries the effective values.
        const readSettings = effectiveConfig.settings
          ?? (await listContentTypes()).find(item => item.descriptor.key === typeKey)?.descriptor.settings;
        const freshCfg: EffectiveEditorConfigWithSettings = {
          ...effectiveConfig,
          settings: effectiveContentTypeSettings(typeKey, readSettings),
        };
        if (!active || reqEpoch !== editorDraftStore.getSessionEpoch()) return;
        const emptySeo = { title: null, description: null, og_image_id: null, canonical_url: null, noindex: false };
        let baseline: EditorDraftPayload;
        let serverPublicPath: string | null = null;
        let routeReadable = true;
        // Optional theme metadata must not make the entire content editor
        // unusable during a theme reload/corrupt provider. Only authorization
        // failures remain fatal; all existing assignments stay in the entry
        // response and are never implicitly cleared on Save.
        let layoutCatalog: ThemeLayoutCatalog | null = null;
        let layoutUnavailable = false;
        try {
          layoutCatalog = await getActiveThemeLayouts();
        } catch (cause) {
          if (cause instanceof ApiClientError && (cause.status === 401 || cause.status === 403)) throw cause;
          layoutUnavailable = true;
        }
        const [readingSnap, ...taxLists] = await Promise.all([
          typeKey === 'page' && user?.role === 'admin' ? getReadingSettings().catch(() => null) : Promise.resolve(null),
          ...freshCfg.taxonomies.map(tax => listAllTaxonomyTerms(tax).catch(() => [])),
        ]);
        if (!active || reqEpoch !== editorDraftStore.getSessionEpoch()) return;
        setReadingSettings(readingSnap);
        const kindMap = new Map<number, string>();
        for (const list of taxLists) {
          for (const term of list) kindMap.set(term.id, term.kind);
        }
        setTermKindById(kindMap);
        let seoReadable = true;
        if (mode === 'edit' && entryId !== null) {
          // Route/SEO can be forbidden after same-user role downgrade. Never
          // rehydrate Admin-only metadata when the fresh endpoint denies it.
          const optional = async <T,>(request: Promise<T>): Promise<T | null> => {
            try { return await request; }
            catch (cause) {
              if (cause instanceof ApiClientError && (cause.status === 403 || cause.status === 404)) return null;
              throw cause;
            }
          };
          const [rec, routeSnap, seoSnap] = await Promise.all([
            getContentEntry(typeKey, entryId),
            optional(getEntryRoute(typeKey, entryId)),
            optional(getEntrySeo(typeKey, entryId)),
          ]);
          if (!active || reqEpoch !== editorDraftStore.getSessionEpoch()) return;
          setEntryTimestamps({ createdAt: rec.created_at, updatedAt: rec.updated_at });
          setCustomCreatedAt(null);
          setCustomUpdatedAt(null);
          const pageLayoutKey =
            (rec as typeof rec & { page_layout_key?: string | null }).page_layout_key ?? null;
          const customTemplateKey =
            (rec as typeof rec & { custom_template_key?: string | null }).custom_template_key ?? null;
          const themeKey = typeKey === 'page' ? pageLayoutKey : customTemplateKey;
          savedThemeKeyRef.current = themeKey;
          selectedThemeKeyRef.current = themeKey;
          themeKeyDirtyRef.current = false;
          setSelectedThemeKey(themeKey);
          setThemeKeyDirty(false);
          routeReadable = routeSnap !== null;
          seoReadable = seoSnap !== null;
          if (rec.publication_status === 'published' && routeSnap?.entry_version === rec.version) {
            serverPublicPath = safePublicPath(routeSnap.public_url);
          }
          const allowedValues = Object.fromEntries(
            Object.entries(rec.values).filter(([key]) => key in freshCfg.effective_fields),
          );
          setEntryLocale(rec.locale ?? defaultLocaleCode);
          setTranslations(rec.translations ?? []);
          baseline = {
            id: rec.id, version: rec.version, title: rec.title, slug: rec.slug,
            slug_touched: true, locale: rec.locale, publication_status: rec.publication_status,
            content_mode: rec.content_mode,
            values: allowedValues, cleared_fields: [], term_ids: rec.term_ids ?? [],
            primary_category_term_id: rec.primary_category_term_id ?? null,
            parent_id: rec.parent_id ?? null,
            menu_order: rec.menu_order ?? 0,
            page_layout_key: pageLayoutKey,
            custom_template_key: customTemplateKey,
            custom_path: routeSnap?.custom_path ?? '',
            seo: seoSnap ? {
              title: seoSnap.title, description: seoSnap.description,
              og_image_id: seoSnap.og_image_id, canonical_url: seoSnap.canonical_url,
              noindex: seoSnap.noindex,
            } : emptySeo,
          };
        } else {
          setEntryTimestamps(null);
          setCustomCreatedAt(null);
          setCustomUpdatedAt(null);
          savedThemeKeyRef.current = null;
          selectedThemeKeyRef.current = null;
          themeKeyDirtyRef.current = false;
          setSelectedThemeKey(null);
          setThemeKeyDirty(false);
          baseline = {
            id: null, version: null, title: '', slug: '', slug_touched: false,
            publication_status: effectiveContentTypeSettings(typeKey, freshCfg.settings).publication === 'draft_publish'
              ? 'draft' : 'published',
            content_mode: 'editor',
            values: {}, cleared_fields: [], term_ids: [], primary_category_term_id: null,
            parent_id: null, menu_order: 0,
            page_layout_key: null,
            custom_template_key: null,
            custom_path: '', seo: emptySeo,
          };
        }
        const restored = editorDraftStore.restoreAuthorizedDraft(
          draftKey, siteInstanceId, user.id, freshCfg, authGeneration, baseline,
          routeReadable, seoReadable,
        );
        if (!active || reqEpoch !== editorDraftStore.getSessionEpoch()) return;
        setThemeLayouts(layoutCatalog);
        baselineRef.current = baseline;
        setConfirmedPublicLink({
          scope: mediaScopeKey, version: baseline.version ?? 0, path: serverPublicPath,
        });
        draftRef.current = restored?.payload ?? baseline;
        schemaTokenRef.current = restored?.schema_token_at_edit ?? freshCfg.schema_token;
        canWriteRouteRef.current = routeReadable;
        canWriteSeoRef.current = seoReadable;
        editRevisionRef.current = 0;
        setEditRevision(0);
        setHasLocalRestoredDraft(Boolean(restored?.dirty_fields.length));
        applyPayload(draftRef.current);
        setDirty(Boolean(restored?.dirty_fields.length));
        setConfig(freshCfg);
        if (layoutUnavailable) {
          setStatusNote(__("Could not load theme templates. Content and the previous selection are retained; reload before selecting a new template."));
        } else if (restored && restored.schema_token_at_edit !== freshCfg.schema_token) {
          setStatusNote(__("Schema changed. Your draft is retained; saving checks the old schema token to prevent overwrites."));
        }
      } catch (err) {
        if (!active) return;
        if (err instanceof ApiClientError && err.status === 401) {
          editorDraftStore.quarantineAll();
        }
        // Direct URL load of a trashed or absent entry: block editing without discarding
        // any authorized RAM draft, and offer direct navigation to the Trash view.
        if (
          mode === 'edit' &&
          entryId !== null &&
          err instanceof ApiClientError &&
          (err.status === 404 || err.status === 409)
        ) {
          setConfig(null);
          baselineRef.current = null;
          draftRef.current = null;
          setHasLocalRestoredDraft(false);
          setDirty(false);
          setTrashedBlockedReason(err.message);
          return;
        }
        // Fail closed: do not leave the previous account/entry form visible.
        setConfig(null);
        baselineRef.current = null;
        draftRef.current = null;
        setHasLocalRestoredDraft(false);
        setDirty(false);
        onApiError(err, __("Could not load editor screen data."));
      }
    })();

    return () => {
      active = false;
    };
  }, [typeKey, mode, entryId, draftKey, siteInstanceId, user?.id, user?.role, authGeneration]);

  useEffect(() => {
    if (!user || isPattern || mode === 'list' || !settings?.hierarchical) {
      setParentCandidates([]);
      return;
    }
    let alive = true;
    void listContentEntries(typeKey, 100, 0, {}, entryLocale || undefined)
      .then(rows => { if (alive) setParentCandidates(rows); })
      .catch(() => { if (alive) setParentCandidates([]); });
    return () => { alive = false; };
  }, [typeKey, mode, entryLocale, settings?.hierarchical, isPattern, user?.id]);

  useEffect(() => {
    const missing = termIds.filter(id => !termKindById.has(id));
    if (missing.length === 0) return;
    let alive = true;
    void resolveTaxonomyTermLabels(missing)
      .then(items => {
        if (!alive) return;
        setTermKindById(prev => {
          const next = new Map(prev);
          for (const item of items) next.set(item.id, item.kind);
          return next;
        });
      })
      .catch(() => {});
    return () => { alive = false; };
  }, [termIds, termKindById]);

  const expandPanelContainingField = (fieldKey: string) => {
    if (!config) return;
    const allRegions = [
      ...config.regions.main_top,
      ...config.regions.main_primary,
      ...config.regions.main_bottom,
      ...config.regions.sidebar,
    ];
    for (const p of allRegions) {
      if (p.field_keys.includes(fieldKey)) {
        setCollapsedPanels(prev => ({ ...prev, [p.panel.panel_id]: false }));
      }
    }
    setTimeout(() => {
      const el = document.querySelector(`[data-field-key="${fieldKey}"] input, [data-field-key="${fieldKey}"] textarea, [data-field-key="${fieldKey}"] select`) as HTMLElement | null;
      el?.focus();
    }, 20);
  };

  const refreshPublishedPublicLink = (
    targetId: number,
    targetVersion: number,
    requestEpoch: number,
  ) => {
    void getEntryRoute(typeKey, targetId).then(routeSnap => {
      if (requestEpoch !== editorDraftStore.getSessionEpoch() ||
          activeDraftKeyRef.current !== draftKey ||
          activeMediaScopeRef.current !== mediaScopeKey) return;
      if (routeSnap.entry_version !== targetVersion) return;
      setConfirmedPublicLink({
        scope: mediaScopeKey,
        version: targetVersion,
        path: safePublicPath(routeSnap.public_url),
      });
    }).catch(error => {
      if (requestEpoch === editorDraftStore.getSessionEpoch() &&
          activeDraftKeyRef.current === draftKey &&
          activeMediaScopeRef.current === mediaScopeKey &&
          error instanceof ApiClientError && error.status === 401) {
        onApiError(error, __("Your session expired while loading the public URL."));
      }
    });
  };

  const handleSave = async (intent?: 'draft' | 'published', navigateOnCreate = true): Promise<number | null> => {
    if (isPattern || !config || !csrfToken || saving || saveInFlightRef.current || !draftRef.current || !user) return null;
    const isBuiltInContent = typeKey === 'post' || typeKey === 'page';
    if (isBuiltInContent && !intent) return null;
    const effectiveSettings = effectiveContentTypeSettings(typeKey, config.settings);
    const nextPublication = effectiveSettings.publication === 'draft_publish'
      ? (intent ?? draftRef.current.publication_status)
      : 'published';
    const snapshot = draftRef.current;
    const submittedRevision = editRevisionRef.current;
    const requestEpoch = editorDraftStore.getSessionEpoch();
    const expectedSchemaToken = schemaTokenRef.current;
    const nextErrors: Record<string, string> = {};
    if (!snapshot.title.trim()) nextErrors.title = __("Title cannot be empty.");
    if (!snapshot.slug.trim()) nextErrors.slug = __("Slug cannot be empty.");
    Object.assign(
      nextErrors,
      validateRequiredEditorFields(config, typeKey, effectiveSettings, snapshot, (label) =>
        __("Field \"{{value1}}\" is required.", { value1: label }), activeFieldKeys),
    );

    if (Object.keys(nextErrors).length > 0) {
      setFieldErrors(nextErrors);
      const firstKey = Object.keys(nextErrors)[0];
      expandPanelContainingField(firstKey);
      return null;
    }

    setFieldErrors({});
    revisions.abortAndClearInflight();
    saveInFlightRef.current = true;
    setSaving(true);
    setStatusNote(null);

    // Send ONLY active editable fields (hidden LocationRules/read-only fields stay omitted to preserve DB values).
    const submitValues: Record<string, ContentFieldValue> = {};
    for (const [fk, eff] of Object.entries(config.effective_fields)) {
      if ((activeFieldKeys && !activeFieldKeys.has(fk)) || !eff.editable || !supportAllowsField(typeKey, fk, effectiveSettings)) {
        continue;
      }
      if (snapshot.cleared_fields.includes(fk)) {
        submitValues[fk] = null;
      } else if (fk in snapshot.values) {
        submitValues[fk] = snapshot.values[fk];
      }
    }

    try {
      const payload = {
        ...(themeKeyDirtyRef.current ? {
          [typeKey === 'page' ? 'page_layout_key' : 'custom_template_key']: selectedThemeKeyRef.current,
        } : {}),
        expected_version: entryId === null ? undefined : (snapshot.version ?? undefined),
        expected_schema_token: expectedSchemaToken,
        title: snapshot.title.trim(),
        slug: snapshot.slug.trim(),
        publication_status: nextPublication,
        ...((effectiveSettings.supports.includes('builder') || snapshot.content_mode === 'editor') &&
            baselineRef.current?.content_mode !== snapshot.content_mode
          ? { content_mode: snapshot.content_mode } : {}),
        values: submitValues,
        // Omitted term_ids retains existing links; [] only clears by user edit.
        term_ids: baselineRef.current && diffEditorPayload(baselineRef.current, snapshot).includes('term_ids')
          ? snapshot.term_ids : undefined,
        // Primary Category is writable only while category remains eligible for post.
        // Omitted means preserve; explicit null means clear the previous choice.
        ...(typeKey === 'post' && config.taxonomies.includes('category')
          && baselineRef.current?.primary_category_term_id !== snapshot.primary_category_term_id
          ? { primary_category_term_id: snapshot.primary_category_term_id } : {}),
        ...(effectiveSettings.hierarchical && (baselineRef.current?.parent_id !== snapshot.parent_id || snapshot.parent_id != null)
          ? { parent_id: snapshot.parent_id ?? null } : {}),
        ...((effectiveSettings.hierarchical || effectiveSettings.supports.includes('menu_order')) && baselineRef.current?.menu_order !== snapshot.menu_order
          ? { menu_order: snapshot.menu_order ?? 0 } : {}),
        ...(entryId === null ? { auto_unique_slug: true } : {}),
        ...(entryId === null && localeState.hasMultipleLocales && entryLocale ? { locale: entryLocale } : {}),
        ...(canWriteRouteRef.current ? {
          custom_path: snapshot.custom_path?.trim() ? snapshot.custom_path.trim() : null,
        } : {}),
        ...(canWriteSeoRef.current ? { seo: {
          title: snapshot.seo?.title?.trim() || null,
          description: snapshot.seo?.description?.trim() || null,
          og_image_id: snapshot.seo?.og_image_id?.trim() || null,
          canonical_url: snapshot.seo?.canonical_url?.trim() || null,
          noindex: Boolean(snapshot.seo?.noindex),
        }} : {}),
        ...(customCreatedAt ? { created_at: customCreatedAt } : {}),
        ...(customUpdatedAt ? { updated_at: customUpdatedAt } : {}),
      };

      const res =
        entryId === null
          ? await createEntryComposite(typeKey, payload, csrfToken)
          : await updateEntryComposite(typeKey, entryId, payload, csrfToken);
      if (requestEpoch !== editorDraftStore.getSessionEpoch() || activeDraftKeyRef.current !== draftKey) return null;
      setEntryLocale(res.entry.locale ?? entryLocale);
      if (res.entry.translations) setTranslations(res.entry.translations);
      const pageLayoutKey =
        (res.entry as typeof res.entry & { page_layout_key?: string | null }).page_layout_key ?? null;
      const customTemplateKey =
        (res.entry as typeof res.entry & { custom_template_key?: string | null }).custom_template_key ?? null;
      const savedSelection = typeKey === 'page' ? pageLayoutKey : customTemplateKey;
      const saved: EditorDraftPayload = {
        ...snapshot,
        id: res.entry.id, version: res.entry.version, title: res.entry.title, slug: res.entry.slug,
        slug_touched: true, locale: res.entry.locale, publication_status: res.entry.publication_status,
        content_mode: res.entry.content_mode,
        values: res.entry.values, cleared_fields: [], term_ids: res.entry.term_ids ?? snapshot.term_ids,
        primary_category_term_id: res.entry.primary_category_term_id ?? null,
        parent_id: res.entry.parent_id ?? null,
        menu_order: res.entry.menu_order ?? 0,
        page_layout_key: pageLayoutKey,
        custom_template_key: customTemplateKey,
        custom_path: res.custom_path ?? '',
        seo: res.seo ? {
          title: res.seo.title, description: res.seo.description,
          og_image_id: res.seo.og_image_id, canonical_url: res.seo.canonical_url,
          noindex: res.seo.noindex,
        } : snapshot.seo,
      };
      baselineRef.current = saved;
      setEntryTimestamps({ createdAt: res.entry.created_at, updatedAt: res.entry.updated_at });
      setCustomCreatedAt(null);
      setCustomUpdatedAt(null);
      savedThemeKeyRef.current = savedSelection;
      if (editRevisionRef.current === submittedRevision) {
        selectedThemeKeyRef.current = savedSelection;
        setSelectedThemeKey(savedSelection);
        themeKeyDirtyRef.current = false;
        setThemeKeyDirty(false);
      } else {
        const stillDirty = selectedThemeKeyRef.current !== savedSelection;
        themeKeyDirtyRef.current = stillDirty;
        setThemeKeyDirty(stillDirty);
      }
      // A committed slug, route or publication change can invalidate the old
      // link. Require a matching server response before exposing another URL.
      setConfirmedPublicLink(null);
      if (entryId !== null && saved.publication_status === 'published') {
        refreshPublishedPublicLink(res.entry.id, res.entry.version, requestEpoch);
      }
      schemaTokenRef.current = config.schema_token;
      editorDraftStore.delete(draftKey);
      const hasNewerChanges = editRevisionRef.current !== submittedRevision;
      if (!hasNewerChanges) {
        draftRef.current = saved;
        applyPayload(saved);
        setDirty(themeKeyDirtyRef.current);
      } else if (draftRef.current) {
        const remaining = { ...draftRef.current, id: res.entry.id, version: res.entry.version,
          publication_status: draftRef.current.publication_status === snapshot.publication_status
            ? res.entry.publication_status : draftRef.current.publication_status,
          content_mode: draftRef.current.content_mode === snapshot.content_mode
            ? saved.content_mode : draftRef.current.content_mode };
        draftRef.current = remaining;
        const changed = diffEditorPayload(saved, remaining);
        const newKey = editorDraftStore.makeDraftKey(typeKey, res.entry.id);
        if (changed.length) {
          editorDraftStore.set({
            site_instance_id: siteInstanceId, user_id: user.id,
            auth_generation: authGeneration, draft_key: newKey,
            type_key: typeKey, status: 'active',
            schema_token_at_edit: config.schema_token,
            dirty_fields: changed, server_values: { ...saved.values },
            payload: { ...remaining, custom_path_dirty: changed.includes('custom_path'), seo_dirty: changed.includes('seo') },
          });
        }
        setDirty(changed.length > 0 || themeKeyDirtyRef.current);
      }
      setHasLocalRestoredDraft(false);
      revisions.clearServerAutosaveState();
      if (hasNewerChanges) {
        editRevisionRef.current += 1;
        setEditRevision(editRevisionRef.current);
      }
      void revisions.refreshRevisionCount();
      setStatusNote(hasNewerChanges
        ? __("Saved snapshot v{{value1}}; newer edits remain unsaved.", { value1: res.entry.version })
        : __("Saved successfully (v{{value1}}).", { value1: res.entry.version }));
      if (entryId === null && navigateOnCreate) {
        onNavigateHash(`${listHashPrefix}/${res.entry.id}/edit`);
      }
      return res.entry.id;
    } catch (err) {
      if (requestEpoch !== editorDraftStore.getSessionEpoch() || activeDraftKeyRef.current !== draftKey
          || activeMediaScopeRef.current !== mediaScopeKey) return null;
      if (err instanceof ApiClientError && err.status === 401) {
        editorDraftStore.quarantineAll();
      }
      if (err instanceof ApiClientError && err.status === 412) {
        setStatusNote(__("Version or schema conflict (412). Your draft is retained for resolution."));
      } else if (
        err instanceof ApiClientError &&
        err.status === 409 &&
        (err.errorCode === 'entry.trashed' || err.message.includes('entry_trashed'))
      ) {
        // Entry was moved to Trash in another session: retain dirty RAM draft and direct user to Trash.
        setTrashedBlockedReason(err.message);
        setStatusNote(__("This entry was moved to Trash in another session (409). Your unsaved draft is retained; restore the entry from Trash before saving."));
      } else if (err instanceof ApiClientError && err.status === 409) {
        setStatusNote(__("Static path or slug conflict (409). Choose another path or review conflict details. Your draft is retained."));
      }
      onApiError(err, __("Could not save the entry."));
      return null;
    } finally {
      saveInFlightRef.current = false;
      setSaving(false);
    }
  };

  const handleRevisionRestored = (res: RestoreRevisionResponse, restoredFromRevNum: number) => {
    if (!isRestoreEntryOutcome(res) || !config || !user) return;
    const { payload: restoredPayload, savedSelection } = buildRestoredEditorPayload(typeKey, res, config);
    savedThemeKeyRef.current = savedSelection;
    baselineRef.current = restoredPayload;
    draftRef.current = restoredPayload;
    schemaTokenRef.current = res.schema_token;
    setEntryLocale(res.entry.locale ?? entryLocale);
    if (res.entry.translations) setTranslations(res.entry.translations);
    setEntryTimestamps({ createdAt: res.entry.created_at, updatedAt: res.entry.updated_at });
    setCustomCreatedAt(null);
    setCustomUpdatedAt(null);
    setFieldErrors({});
    editRevisionRef.current += 1;
    setEditRevision(editRevisionRef.current);
    applyPayload(restoredPayload);
    editorDraftStore.delete(draftKey);
    setDirty(false);
    onDirtyChange(false);
    setHasLocalRestoredDraft(false);
    revisions.clearServerAutosaveState();
    revisions.handleCloseHistory();
    void revisions.refreshRevisionCount();
    setConfirmedPublicLink(null);
    if (restoredPayload.publication_status === 'published') {
      refreshPublishedPublicLink(res.entry.id, res.entry.version, editorDraftStore.getSessionEpoch());
    }
    setStatusNote(__("Restored snapshot #{{value1}} as new version v{{value2}}.", {
      value1: restoredFromRevNum,
      value2: res.entry.version,
    }));
  };

  if (isPattern) {
    if (!isActive || !user) return null;
    return <div className="od-admin-card space-y-3" role="status">
      <p className="text-sm text-slate-700">{__("Patterns are private content shared through Visual Builder.")}</p>
      <button type="button" onClick={() => onNavigateHash(entryId === null
        ? '#/patterns' : `#/patterns/${entryId}/edit`)}>{__("Open Pattern management")}</button>
    </div>;
  }

  if (mode === 'list') {
    if (!isActive || !user) return null;
    return <EntryListPage typeKey={typeKey} user={user} siteInstanceId={siteInstanceId}
      authGeneration={authGeneration} csrfToken={csrfToken} routeHash={routeHash ?? listHashPrefix}
      localeState={localeState} onApiError={onApiError} onNavigateHash={onNavigateHash} />;
  }

  if (!config) {
    if (trashedBlockedReason) {
      return (
        <div
          className="max-w-xl space-y-3 rounded-xl border border-amber-200 bg-amber-50 p-5 text-sm text-amber-950"
          role="alert"
          data-testid="single-edit-trashed-guard"
        >
          <p className="font-semibold">{__("This entry is not available for direct editing")}</p>
          <p className="text-xs leading-5 text-amber-900">{trashedBlockedReason}</p>
          <p className="text-xs text-amber-800">{__("If the entry is in Trash, restore it from the Trash view first. Any authorized unsaved draft in this session has been preserved.")}</p>
          <div className="flex flex-wrap gap-2 pt-1">
            <button
              type="button"
              data-testid="single-edit-open-trash-btn"
              onClick={() => onNavigateHash(`${listHashPrefix}?status=trash`)}
              className="inline-flex h-8 items-center rounded-md bg-od-accent px-3 text-xs font-medium text-white hover:bg-od-accent-hover"
            >
              {__("Open Trash")}
            </button>
            <button
              type="button"
              onClick={() => onNavigateHash(listHashPrefix)}
              className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 hover:bg-zinc-50"
            >
              {__("Back to list")}
            </button>
          </div>
        </div>
      );
    }
    return <p className="text-sm text-slate-600">{__("Loading editor configuration...")}</p>;
  }

  // Public URLs must come from the server route authority. Never derive a URL
  // from the slug/custom path or from an unsaved editor draft.
  const confirmed = baselineRef.current;
  const publicPath = settings?.public && !isPattern
    && confirmed?.id === entryId && confirmed?.publication_status === 'published'
    && confirmedPublicLink?.scope === mediaScopeKey
    && confirmedPublicLink.version === confirmed.version
    ? confirmedPublicLink.path
    : null;
  const draftPreviewPath = !isPattern && confirmed?.id === entryId
    && confirmed.publication_status === 'draft' && entryId !== null
    ? contentEntryPreviewPath(typeKey, entryId) : null;
  const isBuiltIn = typeKey === 'post' || typeKey === 'page';
  const publicationPolicy = settings?.publication ?? (isBuiltIn ? 'draft_publish' : 'immediate');
  const templateChoices = typeKey === 'page'
    ? (themeLayouts?.page_layouts.map(layout => ({ key: layout.key, label: layout.label })) ?? [])
    : (themeLayouts?.custom_templates
        .filter(tpl => tpl.allowed_types.includes(typeKey))
        .map(tpl => ({ key: tpl.custom_key, label: tpl.label })) ?? []);
  const showTemplateBox = typeKey === 'page' || templateChoices.length > 0 || Boolean(selectedThemeKey);
  const templatePanelNode = showTemplateBox ? (
    <div className="space-y-1.5" data-testid="theme-template-selection">
      <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700" htmlFor="entry-theme-selector">
        <span>{typeKey === 'page' ? __("Page Template / Layout") : __("Custom Single Template")}</span>
        <select
          id="entry-theme-selector"
          className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
          value={selectedThemeKey ?? ''}
          disabled={saving || !csrfToken || !themeLayouts}
          onChange={event => {
            const choice = event.target.value || null;
            selectedThemeKeyRef.current = choice;
            setSelectedThemeKey(choice);
            const differs = choice !== savedThemeKeyRef.current;
            themeKeyDirtyRef.current = differs;
            setThemeKeyDirty(differs);
            persistDraftPatch(
              typeKey === 'page'
                ? { page_layout_key: choice }
                : { custom_template_key: choice },
            );
          }}
        >
          <option value="">{__("Theme default")}</option>
          {templateChoices.map(choice => (
            <option key={choice.key} value={choice.key}>{choice.label}</option>
          ))}
          {selectedThemeKey && !templateChoices.some(choice => choice.key === selectedThemeKey) && (
            <option value={selectedThemeKey}>{selectedThemeKey} {__("(no longer provided by current theme — preserved)")}</option>
          )}
        </select>
      </label>
      {themeKeyDirty && <p className="text-xs font-medium text-amber-700">{__("● Unsaved")}</p>}
    </div>
  ) : null;

  const translationPanelNode = localeState.hasMultipleLocales && !isPattern ? (
    <SingleEditTranslationsPanel
      typeKey={typeKey}
      entryId={entryId}
      expectedVersion={baselineRef.current?.version ?? null}
      slug={slug}
      title={title}
      entryLocale={entryLocale}
      defaultLocaleCode={defaultLocaleCode}
      translations={translations}
      localeState={localeState}
      listHashPrefix={listHashPrefix}
      saving={saving}
      csrfToken={csrfToken}
      onLocaleChange={nextLocale => {
        setEntryLocale(nextLocale);
        if (settings?.hierarchical) {
          setParentId(null);
          persistDraftPatch({ locale: nextLocale, parent_id: null });
        } else {
          persistDraftPatch({ locale: nextLocale });
        }
      }}
      onApiError={onApiError}
      onNavigateHash={onNavigateHash}
    />
  ) : null;

  const handleDelete = async () => {
    const version = baselineRef.current?.version;
    if (entryId === null || version === null || version === undefined || !csrfToken || saving) return;
    if (!window.confirm(__("Move this entry to Trash? You can restore it later from Trash."))) return;
    revisions.abortAndClearInflight();
    setSaving(true);
    try {
      await trashContentEntry(typeKey, entryId, version, csrfToken);
      revisions.clearServerAutosaveState();
      setHasLocalRestoredDraft(false);
      editorDraftStore.delete(draftKey);
      setDirty(false);
      onDirtyChange(false);
      onNavigateHash(entryTableHash(listHashPrefix, parseEntryTableHash(routeHash ?? listHashPrefix)));
    } catch (err) {
      onApiError(err, __("Could not move the entry to Trash."));
    } finally {
      setSaving(false);
    }
  };

  const entryBuilderHash = entryId !== null
    ? `#/studio/entry/${encodeURIComponent(typeKey)}/${entryId}`
    : '';

  return (
    <div className="space-y-3">
      {trashedBlockedReason && (
        <div
          className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-950"
          role="alert"
          data-testid="single-edit-trashed-banner"
        >
          <span>{trashedBlockedReason}</span>
          <button
            type="button"
            data-testid="single-edit-banner-open-trash"
            onClick={() => onNavigateHash(`${listHashPrefix}?status=trash`)}
            className="inline-flex h-7 items-center rounded border border-amber-400 bg-white px-2.5 text-xs font-medium text-amber-950 hover:bg-amber-100"
          >
            {__("Open Trash")}
          </button>
        </div>
      )}
      {pendingActionConfirmation && (
        <PluginLifecycleConfirmation
          pluginId={pendingActionConfirmation.pluginId}
          actionId={pendingActionConfirmation.actionId}
          confirmation={pendingActionConfirmation.confirmation}
          csrfToken={csrfToken}
          onResolved={res => {
            const panelId = pendingActionConfirmation.panelId;
            setPendingActionConfirmation(null);
            setActionOutputs(prev => ({ ...prev, [panelId]: JSON.stringify(res, null, 2) }));
          }}
          onCancelled={res => {
            const panelId = pendingActionConfirmation.panelId;
            setPendingActionConfirmation(null);
            setActionOutputs(prev => ({
              ...prev,
              [panelId]: res ? JSON.stringify(res, null, 2) : __("Confirmation cancelled. No content was changed."),
            }));
          }}
          onFailed={message => {
            setPendingActionConfirmation(null);
            setStatusNote(message);
          }}
        />
      )}
      {statusNote && (
        <div className={statusNote.includes('(412)') || statusNote.includes('(409)')
          ? 'rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-800'
          : 'rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-800'}
          role={statusNote.includes('(412)') || statusNote.includes('(409)') ? 'alert' : 'status'}>
          {statusNote}
        </div>
      )}
      {mode === 'edit' && entryId !== null && !isPattern && (
        <SingleEditRevisionsOverlay
          typeKey={typeKey}
          entryId={entryId}
          csrfToken={csrfToken}
          currentVersion={baselineRef.current?.version ?? 0}
          serverAutosave={revisions.serverAutosave}
          hasNewerLocalDraft={revisions.hasNewerLocalDraft}
          discardingAutosave={revisions.discardingAutosave}
          historyOpen={revisions.historyOpen}
          initialCompare={revisions.initialCompare}
          onApplyAutosave={revisions.handleApplyServerAutosave}
          onCompareAutosave={() => revisions.handleOpenHistory({ from: 'current', to: 'autosave' })}
          onDiscardAutosave={() => void revisions.handleDiscardServerAutosave()}
          onCloseHistory={revisions.handleCloseHistory}
          onBeforeRestore={revisions.abortAndClearInflight}
          onRestored={handleRevisionRestored}
          onApiError={onApiError}
        />
      )}
      <div>
      <PanelRegionRenderer
        config={editorConfig ?? config}
        values={values}
        clearedFields={clearedFields}
        title={title}
        slug={slug}
        publicationStatus={publicationStatus}
        customPath={customPath}
        seoTitle={seoTitle}
        seoDescription={seoDescription}
        seoOgImageId={seoOgImageId}
        seoNoindex={seoNoindex}
        collapsedPanels={collapsedPanels}
        fieldErrors={fieldErrors}
        actionOutputs={actionOutputs}
        actionBusy={actionBusy}
        entryId={entryId}
        userId={user?.id ?? null}
        userRole={user?.role}
        termIds={termIds}
        primaryCategoryTermId={primaryCategoryTermId}
        publicPath={publicPath}
        draftPreviewPath={draftPreviewPath}
        isPublic={settings?.public ?? true}
        publicationPolicy={publicationPolicy}
        createdAt={customCreatedAt ?? entryTimestamps?.createdAt ?? null}
        updatedAt={customUpdatedAt ?? entryTimestamps?.updatedAt ?? null}
        onCreatedAtChange={val => {
          setCustomCreatedAt(val);
          setDirty(true);
        }}
        onUpdatedAtChange={val => {
          setCustomUpdatedAt(val);
          setDirty(true);
        }}
        revisionsControlNode={
          mode === 'edit' && entryId !== null && !isPattern ? (
            <PublishBoxRevisionsControl
              revisionCount={revisions.revisionCount}
              hasAutosave={revisions.hasServerAutosave}
              autosaveState={revisions.autosaveState}
              disabled={saving || !user}
              onOpenHistory={() => revisions.handleOpenHistory()}
            />
          ) : undefined
        }
        templatePanelNode={templatePanelNode}
        translationPanelNode={translationPanelNode}
        locale={localeState.hasMultipleLocales && !isPattern ? (entryLocale || defaultLocaleCode || undefined) : undefined}
        isHierarchical={Boolean(settings?.hierarchical)}
        supportsMenuOrder={Boolean(settings?.supports.includes('menu_order'))}
        parentId={parentId}
        menuOrder={menuOrder}
        parentOptions={parentCandidates}
        onParentIdChange={nextParentId => {
          setParentId(nextParentId);
          persistDraftPatch({ parent_id: nextParentId });
        }}
        onMenuOrderChange={nextOrder => {
          setMenuOrder(nextOrder);
          persistDraftPatch({ menu_order: nextOrder });
        }}
        csrfToken={csrfToken}
        siteTimezone={siteTimezone}
        saving={saving}
        dirty={dirty}
        onDelete={entryId !== null && baselineRef.current?.version !== null && baselineRef.current?.version !== undefined ? () => void handleDelete() : undefined}
        onToggleCollapse={pid => setCollapsedPanels(prev => ({ ...prev, [pid]: !prev[pid] }))}
        onTitleChange={nextTitle => {
          const nextSlug = draftRef.current?.slug_touched ? slug : slugFromTitle(nextTitle);
          setTitle(nextTitle);
          if (!draftRef.current?.slug_touched) setSlug(nextSlug);
          persistDraftPatch({ title: nextTitle, slug: nextSlug });
        }}
        onSlugChange={nextSlug => {
          setSlug(nextSlug);
          persistDraftPatch({ slug: nextSlug, slug_touched: true });
        }}
        onPublicationStatusChange={s => {
          setPublicationStatus(s);
          persistDraftPatch({ publication_status: s });
        }}
        onTermIdsChange={nextIds => {
          const sorted = [...nextIds].sort((a, b) => a - b);
          setTermIds(sorted);
          const currentPrimary = draftRef.current?.primary_category_term_id ?? null;
          const nextPrimary = currentPrimary !== null && !sorted.includes(currentPrimary)
            ? null : currentPrimary;
          if (nextPrimary !== currentPrimary) setPrimaryCategoryTermId(null);
          persistDraftPatch({ term_ids: sorted, primary_category_term_id: nextPrimary });
        }}
        onPrimaryCategoryChange={id => {
          if (typeKey !== 'post' || !config.taxonomies.includes('category')) return;
          if (id !== null && !draftRef.current?.term_ids.includes(id)) return;
          setPrimaryCategoryTermId(id);
          persistDraftPatch({ primary_category_term_id: id });
        }}
        onApiError={onApiError}
        onFieldChange={(fk, val) => {
          const nextVals = { ...(draftRef.current?.values ?? values), [fk]: val };
          const nextCleared = clearedFields.filter(k => k !== fk);
          setValues(nextVals);
          setClearedFields(nextCleared);
          persistDraftPatch({ values: nextVals, cleared_fields: nextCleared });
        }}
        onPickMedia={(fieldKey, kind) => {
          const eff = config.effective_fields[fieldKey];
          if (
            !csrfToken || !user || !draftRef.current
            || !eff || !eff.editable || eff.effective_status !== 'active'
            || !supportAllowsField(typeKey, fieldKey, effectiveContentTypeSettings(typeKey, config.settings))
            || !matchesMediaPickerKind(eff.definition.type, kind)
          ) return;
          setMediaPicker({
            fieldKey, kind, scopeKey: mediaScopeKey,
            sessionEpoch: editorDraftStore.getSessionEpoch(),
            schemaToken: config.schema_token,
          });
        }}
        onFieldClear={fk => {
          const nextVals = { ...values };
          delete nextVals[fk];
          const nextCleared = Array.from(new Set([...clearedFields, fk]));
          setValues(nextVals);
          setClearedFields(nextCleared);
          persistDraftPatch({ values: nextVals, cleared_fields: nextCleared });
        }}
        onCustomPathChange={p => {
          setCustomPath(p);
          persistDraftPatch({ custom_path: p });
        }}
        onSeoChange={patch => {
          if (patch.title !== undefined) setSeoTitle(patch.title);
          if (patch.description !== undefined) setSeoDescription(patch.description);
          if (patch.og_image_id !== undefined) setSeoOgImageId(patch.og_image_id);
          if (patch.noindex !== undefined) setSeoNoindex(patch.noindex);
          persistDraftPatch({ seo: { ...(draftRef.current?.seo ?? {}), ...patch } });
        }}
        onRunAction={async (pluginId, actionId, panelId) => {
          if (!csrfToken) return;
          setPendingActionConfirmation(null);
          setActionBusy(prev => ({ ...prev, [panelId]: true }));
          try {
            const res = await runPluginAction(
              pluginId,
              actionId,
              { type_key: typeKey, entry_id: entryId, values },
              csrfToken,
            );
            setActionOutputs(prev => ({ ...prev, [panelId]: JSON.stringify(res, null, 2) }));
          } catch (err) {
            if (err instanceof ApiClientError && err.confirmation) {
              setPendingActionConfirmation({
                pluginId,
                actionId,
                panelId,
                confirmation: err.confirmation,
              });
            } else {
              onApiError(err, __("Error running plugin action."));
            }
          } finally {
            setActionBusy(prev => ({ ...prev, [panelId]: false }));
          }
        }}
        onSave={intent => void handleSave(intent)}
        canOpenBuilder={!isPattern && Boolean(settings?.supports.includes('builder'))}
        onOpenBuilder={() => {
          if (isPattern || !settings?.supports.includes('builder')) {
            setStatusNote(__("This content type does not support Builder yet."));
            return;
          }
          if (dirty) {
            setBuilderPromptOpen(true);
            return;
          }
          if (entryId === null) {
            setStatusNote(__("Save the content before opening Builder."));
            return;
          }
          onNavigateHash(entryBuilderHash);
        }}
      />
      </div>
      <SingleEditBuilderPromptDialog
        open={builderPromptOpen}
        entryId={entryId}
        saving={saving}
        onClose={() => setBuilderPromptOpen(false)}
        onDiscardAndOpen={() => {
          revisions.abortAndClearInflight();
          setBuilderPromptOpen(false);
          editorDraftStore.delete(draftKey);
          setDirty(false);
          onDirtyChange(false);
          onNavigateHash(entryBuilderHash);
        }}
        onSaveAndOpen={() => {
          void (async () => {
            const intent = publicationPolicy === 'draft_publish' ? publicationStatus : undefined;
            const savedId = await handleSave(intent, false);
            setBuilderPromptOpen(false);
            if (savedId !== null) {
              onNavigateHash(`#/studio/entry/${encodeURIComponent(typeKey)}/${savedId}`);
            }
          })();
        }}
      />
      {mediaPicker && mediaPicker.scopeKey === mediaScopeKey && csrfToken && user && config
        && config.schema_token === mediaPicker.schemaToken
        && editorDraftStore.getSessionEpoch() === mediaPicker.sessionEpoch
        && config.effective_fields[mediaPicker.fieldKey]?.editable
        && config.effective_fields[mediaPicker.fieldKey]?.effective_status === 'active'
        && supportAllowsField(typeKey, mediaPicker.fieldKey, effectiveContentTypeSettings(typeKey, config.settings))
        && matchesMediaPickerKind(config.effective_fields[mediaPicker.fieldKey]?.definition.type, mediaPicker.kind)
        && (
          <MediaPicker
            key={`${mediaPicker.scopeKey}:${mediaPicker.fieldKey}`}
            mode={mediaPicker.kind}
            selectedIds={(() => {
              const raw = draftRef.current?.values[mediaPicker.fieldKey];
              if (Array.isArray(raw)) return raw.map(String);
              if (typeof raw === 'string') return mediaPicker.kind === 'gallery'
                ? raw.split(/[\n,]+/).map(id => id.trim()).filter(Boolean)
                : raw ? [raw] : [];
              return [];
            })()}
            csrfToken={csrfToken}
            onApply={ids => applyMediaSelection(mediaPicker, ids)}
            onClose={() => setMediaPicker(null)}
            onError={(error, fallback) => {
              if (error instanceof ApiClientError && error.status === 401) {
                editorDraftStore.quarantineAll();
                setMediaPicker(null);
              }
              onApiError(error, fallback);
            }}
          />
        )}
      </div>
  );
};
