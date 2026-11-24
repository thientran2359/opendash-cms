import React, { useEffect, useMemo, useRef, useState } from 'react';
import {
  createEntryComposite,
  createEntryTranslation,
  contentEntryPreviewPath,
  deleteContentEntry,
  effectiveContentTypeSettings,
  getContentEntry,
  getEffectiveEditorConfig,
  listContentTypes,
  type ContentTypeSettings,
  type EffectiveEditorConfigWithSettings,
  updateEntryComposite,
} from '../../api/content.ts';
import { runPluginAction } from '../../api/plugins.ts';
import { getEntryRoute, getEntrySeo } from '../../api/seo.ts';
import { ApiClientError } from '../../api/client.ts';
import {
  getActiveThemeLayouts,
  type ThemeLayoutCatalog,
} from '../../api/themes.ts';
import {
  EMPTY_ADMIN_LOCALE_STATE,
  type AdminLocaleState,
} from '../../api/locales.ts';
import type { AuthUser } from '../../types.ts';
import type {
  ContentFieldValue,
  EffectiveEditorConfigProjection,
  EffectivePanelProjection,
  TranslationLinkSummary,
} from '../content/types.ts';
import { editorDraftStore, diffEditorPayload, type EditorDraftPayload } from './EditorDraftStore.ts';
import { PanelRegionRenderer } from './PanelRegionRenderer.tsx';
import { MediaPicker } from '../media/MediaPicker.tsx';
import { EntryListPage, entryTableHash, parseEntryTableHash, safePublicPath } from '../content/EntryListPage.tsx';

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

function supportAllowsField(typeKey: string, key: string, settings: ContentTypeSettings): boolean {
  if (key === 'featured_image' && typeKey !== 'section' && typeKey !== 'pattern') {
    return settings.supports.includes('featured_image');
  }
  if (typeKey !== 'post' && typeKey !== 'page') return true;
  if (key === 'body') return settings.supports.includes('editor') || settings.supports.includes('builder');
  return true;
}

/** Restrict UI affordances without removing values from the saved entry/draft. */
function displayEditorConfig(
  config: EffectiveEditorConfigWithSettings,
  settings: ContentTypeSettings,
): EffectiveEditorConfigProjection {
  const typeKey = config.type_key;
  const projectPanels = (panels: EffectivePanelProjection[]): EffectivePanelProjection[] =>
    panels.filter(({ panel }) => {
      if (panel.kind.kind !== 'core_built_in') return true;
      if (panel.kind.core_key === 'body_editor') {
        return settings.supports.includes('editor') || settings.supports.includes('builder');
      }
      if (panel.kind.core_key === 'featured_image') return settings.supports.includes('featured_image');
      return true;
    }).map(panel => ({
      ...panel,
      field_keys: panel.field_keys.filter(key => supportAllowsField(typeKey, key, settings)),
      subgroups: panel.subgroups.map(group => ({
        ...group,
        field_keys: group.field_keys.filter(key => supportAllowsField(typeKey, key, settings)),
      })).filter(group => group.field_keys.length > 0),
    })).filter(panel => {
      if (panel.panel.kind.kind === 'base_fields_slice' || panel.panel.kind.kind === 'content_fields') {
        return panel.field_keys.length > 0;
      }
      if (panel.panel.kind.kind === 'core_built_in' && panel.panel.kind.core_key === 'base_fields') {
        return panel.field_keys.length > 0;
      }
      return true;
    });

  return {
    ...config,
    effective_fields: Object.fromEntries(
      Object.entries(config.effective_fields).filter(([key]) => supportAllowsField(typeKey, key, settings)),
    ),
    regions: {
      main_top: projectPanels(config.regions.main_top),
      main_primary: projectPanels(config.regions.main_primary),
      main_bottom: projectPanels(config.regions.main_bottom),
      sidebar: projectPanels(config.regions.sidebar),
    },
  };
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
  const defaultLocaleCode = localeState.defaultLocale?.code ?? '';
  const [entryLocale, setEntryLocale] = useState<string>(defaultLocaleCode);
  const [translations, setTranslations] = useState<TranslationLinkSummary[]>([]);
  const [translationModalLocale, setTranslationModalLocale] = useState<string | null>(null);
  const [translationTitle, setTranslationTitle] = useState('');
  const [translationSlug, setTranslationSlug] = useState('');
  const [creatingTranslation, setCreatingTranslation] = useState(false);
  const [translationError, setTranslationError] = useState<string | null>(null);
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
  const builderDialogRef = useRef<HTMLDialogElement | null>(null);
  const [values, setValues] = useState<Record<string, ContentFieldValue>>({});
  const [clearedFields, setClearedFields] = useState<string[]>([]);
  const [termIds, setTermIds] = useState<number[]>([]);
  const [primaryCategoryTermId, setPrimaryCategoryTermId] = useState<number | null>(null);
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
  const [saving, setSaving] = useState(false);
  const saveInFlightRef = useRef(false);
  const [dirty, setDirty] = useState(false);
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
  const editorConfig = useMemo(
    () => config && settings ? displayEditorConfig(config, settings) : null,
    [config],
  );

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
      || eff.definition.type !== target.kind
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
    setCustomPath(payload.custom_path ?? '');
    setSeoTitle(payload.seo?.title ?? '');
    setSeoDescription(payload.seo?.description ?? '');
    setSeoOgImageId(payload.seo?.og_image_id ?? '');
    setSeoNoindex(Boolean(payload.seo?.noindex));
    if (payload.locale) setEntryLocale(payload.locale);
  };

  const persistDraftPatch = (patch: Partial<EditorDraftPayload>) => {
    const previous = draftRef.current;
    const baseline = baselineRef.current;
    if (!user || !config || !previous || !baseline || mode === 'list') return;
    const updated = { ...previous, ...patch };
    draftRef.current = updated;
    editRevisionRef.current += 1;
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

  useEffect(() => {
    onDirtyChange(dirty);
  }, [dirty, onDirtyChange]);

  useEffect(() => {
    const dialog = builderDialogRef.current;
    if (!dialog) return;
    if (builderPromptOpen && !dialog.open) dialog.showModal();
    else if (!builderPromptOpen && dialog.open) dialog.close();
  }, [builderPromptOpen]);

  useEffect(() => {
    setConfirmedPublicLink(null);
    setThemeLayouts(null);
    if (!user) {
      setConfig(null);
      baselineRef.current = null;
      draftRef.current = null;
      setDirty(false);
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
      return;
    }

    (async () => {
      try {
        if (mode === 'list') {
          baselineRef.current = null;
          draftRef.current = null;
          setDirty(false);
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
          const themeKey = typeKey === 'page'
            ? (rec as typeof rec & { page_layout_key?: string | null }).page_layout_key ?? null
            : (rec as typeof rec & { custom_template_key?: string | null }).custom_template_key ?? null;
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
        applyPayload(draftRef.current);
        setDirty(Boolean(restored?.dirty_fields.length));
        setConfig(freshCfg);
        if (layoutUnavailable) {
          setStatusNote('Không tải được danh sách template từ theme. Bản nội dung và lựa chọn cũ vẫn được giữ; không thể chọn template mới cho đến khi tải lại.');
        } else if (restored && restored.schema_token_at_edit !== freshCfg.schema_token) {
          setStatusNote('Schema đã thay đổi. Nháp được giữ; Lưu sẽ kiểm tra schema token cũ để tránh ghi đè.');
        }
      } catch (err) {
        if (!active) return;
        if (err instanceof ApiClientError && err.status === 401) {
          editorDraftStore.quarantineAll();
        }
        // Fail closed: do not leave the previous account/entry form visible.
        setConfig(null);
        baselineRef.current = null;
        draftRef.current = null;
        setDirty(false);
        onApiError(err, 'Không thể tải dữ liệu màn hình chỉnh sửa.');
      }
    })();

    return () => {
      active = false;
    };
  }, [typeKey, mode, entryId, draftKey, siteInstanceId, user?.id, user?.role, authGeneration]);

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
    if (!snapshot.title.trim()) nextErrors.title = 'Tiêu đề không được để trống.';
    if (!snapshot.slug.trim()) nextErrors.slug = 'Slug không được để trống.';

    for (const [fk, eff] of Object.entries(config.effective_fields)) {
      if (eff.effective_required && eff.editable && supportAllowsField(typeKey, fk, effectiveSettings)) {
        const v = snapshot.values[fk];
        const empty = snapshot.cleared_fields.includes(fk) || v === undefined || v === null || (typeof v === 'string' && !v.trim());
        if (empty) {
          nextErrors[fk] = `Trường "${eff.definition.label}" là bắt buộc.`;
        }
      }
    }

    if (Object.keys(nextErrors).length > 0) {
      setFieldErrors(nextErrors);
      const firstKey = Object.keys(nextErrors)[0];
      expandPanelContainingField(firstKey);
      return null;
    }

    setFieldErrors({});
    saveInFlightRef.current = true;
    setSaving(true);
    setStatusNote(null);

    // Send ONLY editable fields (never send read-only fields -> Omitted = Preserve)
    const submitValues: Record<string, ContentFieldValue> = {};
    for (const [fk, eff] of Object.entries(config.effective_fields)) {
      if (!eff.editable || !supportAllowsField(typeKey, fk, effectiveSettings)) continue;
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
      const saved: EditorDraftPayload = {
        ...snapshot,
        id: res.entry.id, version: res.entry.version, title: res.entry.title, slug: res.entry.slug,
        slug_touched: true, locale: res.entry.locale, publication_status: res.entry.publication_status,
        content_mode: res.entry.content_mode,
        values: res.entry.values, cleared_fields: [], term_ids: res.entry.term_ids ?? snapshot.term_ids,
        primary_category_term_id: res.entry.primary_category_term_id ?? null,
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
      const savedSelection = typeKey === 'page'
        ? (res.entry as typeof res.entry & { page_layout_key?: string | null }).page_layout_key ?? null
        : (res.entry as typeof res.entry & { custom_template_key?: string | null }).custom_template_key ?? null;
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
        void getEntryRoute(typeKey, res.entry.id).then(routeSnap => {
          if (requestEpoch !== editorDraftStore.getSessionEpoch() ||
              activeDraftKeyRef.current !== draftKey ||
              activeMediaScopeRef.current !== mediaScopeKey) return;
          if (routeSnap.entry_version !== res.entry.version) return;
          setConfirmedPublicLink({
            scope: mediaScopeKey,
            version: res.entry.version,
            path: safePublicPath(routeSnap.public_url),
          });
        }).catch(error => {
          if (requestEpoch === editorDraftStore.getSessionEpoch() &&
              activeDraftKeyRef.current === draftKey &&
              activeMediaScopeRef.current === mediaScopeKey &&
              error instanceof ApiClientError && error.status === 401) {
            onApiError(error, 'Phiên đăng nhập hết hạn khi tải URL công khai.');
          }
        });
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
      setStatusNote(hasNewerChanges
        ? `Đã lưu snapshot v${res.entry.version}; chỉnh sửa mới hơn vẫn chưa lưu.`
        : `Đã lưu thành công (v${res.entry.version}).`);
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
        setStatusNote('Có xung đột version hoặc schema (412). Nháp vẫn được giữ để xử lý.');
      } else if (err instanceof ApiClientError && err.status === 409) {
        setStatusNote('Xung đột đường dẫn tĩnh hoặc slug (409). Hãy chọn đường dẫn khác hoặc xem chi tiết xung đột. Nháp vẫn được giữ.');
      }
      onApiError(err, 'Không thể lưu nội dung.');
      return null;
    } finally {
      saveInFlightRef.current = false;
      setSaving(false);
    }
  };

  if (isPattern) {
    if (!isActive || !user) return null;
    return <div className="od-admin-card space-y-3" role="status">
      <p className="text-sm text-slate-700">Pattern là nội dung riêng tư, dùng chung qua Visual Builder.</p>
      <button type="button" onClick={() => onNavigateHash(entryId === null
        ? '#/patterns' : `#/patterns/${entryId}/edit`)}>Mở quản lý Pattern</button>
    </div>;
  }

  if (mode === 'list') {
    if (!isActive || !user) return null;
    return <EntryListPage typeKey={typeKey} user={user} siteInstanceId={siteInstanceId}
      authGeneration={authGeneration} csrfToken={csrfToken} routeHash={routeHash ?? listHashPrefix}
      localeState={localeState} onApiError={onApiError} onNavigateHash={onNavigateHash} />;
  }

  if (!config) {
    return <p className="text-sm text-slate-600">Đang tải cấu hình trình chỉnh sửa...</p>;
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
        <span>{typeKey === 'page' ? 'Page Template / Layout' : 'Custom Single Template'}</span>
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
            if (differs) editRevisionRef.current++;
            setDirty(differs || !!(draftRef.current && baselineRef.current && diffEditorPayload(baselineRef.current, draftRef.current).length));
          }}
        >
          <option value="">Theo theme mặc định</option>
          {templateChoices.map(choice => (
            <option key={choice.key} value={choice.key}>{choice.label}</option>
          ))}
          {selectedThemeKey && !templateChoices.some(choice => choice.key === selectedThemeKey) && (
            <option value={selectedThemeKey}>{selectedThemeKey} (theme hiện tại không còn cung cấp — được bảo toàn)</option>
          )}
        </select>
      </label>
      {themeKeyDirty && <p className="text-xs font-medium text-amber-700">● Chưa lưu</p>}
    </div>
  ) : null;

  const missingTranslationLocales = localeState.allLocales.filter(
    loc => loc.code !== entryLocale && !translations.some(tr => tr.locale === loc.code),
  );

  const handleCreateTranslation = async () => {
    if (!csrfToken || entryId === null || !translationModalLocale) return;
    const expectedVersion = baselineRef.current?.version;
    if (expectedVersion === undefined || expectedVersion === null) return;
    setCreatingTranslation(true);
    setTranslationError(null);
    try {
      const created = await createEntryTranslation(
        typeKey,
        entryId,
        {
          target_locale: translationModalLocale,
          title: translationTitle.trim() || undefined,
          slug: translationSlug.trim() || slug || `${typeKey}-${translationModalLocale}`,
          auto_unique_slug: !translationSlug.trim(),
        },
        expectedVersion,
        csrfToken,
      );
      setTranslationModalLocale(null);
      onNavigateHash(`${listHashPrefix}/${created.id}`);
    } catch (error) {
      if (error instanceof ApiClientError) {
        setTranslationError(error.message);
      } else {
        setTranslationError('Không thể tạo bản dịch.');
      }
      onApiError(error, 'Không thể tạo bản dịch');
    } finally {
      setCreatingTranslation(false);
    }
  };

  const translationPanelNode = localeState.hasMultipleLocales && !isPattern ? (
    <div className="space-y-2.5" data-testid="entry-translations-panel">
      {entryId === null ? (
        <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700" htmlFor="entry-locale-select">
          <span>Ngôn ngữ nội dung</span>
          <select
            id="entry-locale-select"
            data-testid="entry-locale-select"
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
            value={entryLocale || defaultLocaleCode}
            disabled={saving || !csrfToken}
            onChange={event => {
              const nextLocale = event.target.value;
              setEntryLocale(nextLocale);
              persistDraftPatch({ locale: nextLocale });
            }}
          >
            {localeState.allLocales.map(loc => (
              <option key={loc.code} value={loc.code}>
                {loc.label} ({loc.code}){loc.is_default ? ' — Mặc định' : ''}
              </option>
            ))}
          </select>
        </label>
      ) : (
        <>
          <div className="flex items-center justify-between text-xs">
            <span className="font-medium text-zinc-700">Ngôn ngữ</span>
            <span className="rounded bg-zinc-100 px-2 py-0.5 font-mono text-zinc-800" data-testid="entry-locked-locale">
              {entryLocale || defaultLocaleCode}
            </span>
          </div>
          {translations.length > 0 && (
            <div className="space-y-1">
              <p className="text-xs font-medium text-zinc-600">Bản dịch liên kết</p>
              <ul className="space-y-1">
                {translations.map(tr => (
                  <li key={tr.id} className="flex items-center justify-between gap-2 text-xs">
                    <a
                      href={`${listHashPrefix}/${tr.id}`}
                      onClick={e => {
                        e.preventDefault();
                        onNavigateHash(`${listHashPrefix}/${tr.id}`);
                      }}
                      className="truncate font-medium text-od-accent hover:underline"
                    >
                      [{tr.locale}] {tr.title || `#${tr.id}`}
                    </a>
                    <span className="shrink-0 text-[11px] text-zinc-500">{tr.publication_status}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {missingTranslationLocales.length > 0 && (
            <div className="space-y-1 pt-1">
              <p className="text-xs font-medium text-zinc-600">Tạo bản dịch</p>
              <div className="flex flex-wrap gap-1.5">
                {missingTranslationLocales.map(loc => (
                  <button
                    key={loc.code}
                    type="button"
                    disabled={saving || !csrfToken}
                    onClick={() => {
                      setTranslationModalLocale(loc.code);
                      setTranslationTitle('');
                      setTranslationSlug('');
                      setTranslationError(null);
                    }}
                    data-testid={`create-translation-${loc.code}`}
                    className="inline-flex h-7 items-center rounded border border-zinc-300 bg-white px-2 text-xs font-medium text-zinc-800 hover:bg-zinc-50 disabled:opacity-50"
                  >
                    + Tạo bản dịch ({loc.code})
                  </button>
                ))}
              </div>
            </div>
          )}
        </>
      )}
    </div>
  ) : null;

  const handleDelete = async () => {
    const version = baselineRef.current?.version;
    if (entryId === null || version === null || version === undefined || !csrfToken || saving) return;
    if (!window.confirm('Xóa vĩnh viễn nội dung này? Hành động này không thể hoàn tác.')) return;
    setSaving(true);
    try {
      await deleteContentEntry(typeKey, entryId, version, csrfToken);
      editorDraftStore.delete(draftKey);
      setDirty(false);
      onDirtyChange(false);
      onNavigateHash(entryTableHash(listHashPrefix, parseEntryTableHash(routeHash ?? listHashPrefix)));
    } catch (err) {
      onApiError(err, 'Không thể xóa nội dung.');
    } finally {
      setSaving(false);
    }
  };

  const entryBuilderHash = entryId !== null
    ? `#/studio/entry/${encodeURIComponent(typeKey)}/${entryId}`
    : '';

  return (
    <div className="space-y-3">
      {statusNote && (
        <div className={statusNote.includes('(412)') || statusNote.includes('(409)')
          ? 'rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-800'
          : 'rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-800'}
          role={statusNote.includes('(412)') || statusNote.includes('(409)') ? 'alert' : 'status'}>
          {statusNote}
        </div>
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
        templatePanelNode={templatePanelNode}
        translationPanelNode={translationPanelNode}
        locale={localeState.hasMultipleLocales && !isPattern ? (entryLocale || defaultLocaleCode || undefined) : undefined}
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
            || eff.definition.type !== kind
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
            onApiError(err, 'Lỗi khi chạy plugin action.');
          } finally {
            setActionBusy(prev => ({ ...prev, [panelId]: false }));
          }
        }}
        onSave={intent => void handleSave(intent)}
        canOpenBuilder={!isPattern && Boolean(settings?.supports.includes('builder'))}
        onOpenBuilder={() => {
          if (isPattern || !settings?.supports.includes('builder')) {
            setStatusNote('Loại nội dung này chưa hỗ trợ Trình dựng trang.');
            return;
          }
          if (dirty) {
            setBuilderPromptOpen(true);
            return;
          }
          if (entryId === null) {
            setStatusNote('Vui lòng Lưu nội dung trước khi mở Trình dựng trang.');
            return;
          }
          onNavigateHash(entryBuilderHash);
        }}
      />
      </div>
      <dialog
        ref={builderDialogRef}
        aria-labelledby="builder-unsaved-title"
        onClose={() => setBuilderPromptOpen(false)}
        className="od-confirm-dialog m-auto max-w-md rounded-xl border border-zinc-200 bg-white p-5 text-zinc-900 shadow-xl"
      >
        <style>{'dialog.od-confirm-dialog:modal{overscroll-behavior:contain} dialog.od-confirm-dialog::backdrop{overflow:hidden;overscroll-behavior:contain;background:rgba(9,9,11,0.45)}'}</style>
        <h2 id="builder-unsaved-title" className="text-base font-semibold text-zinc-950">
          Có thay đổi chưa lưu trong trình soạn thảo
        </h2>
        <p className="mt-2 text-sm text-zinc-600">
          Bạn muốn lưu các chỉnh sửa hiện tại trước khi mở Tùy chỉnh giao diện (Trình dựng trang) không?
        </p>
        <div className="mt-4 flex flex-wrap justify-end gap-2">
          {entryId !== null && (
            <button
              type="button"
              disabled={saving}
              onClick={() => {
                setBuilderPromptOpen(false);
                editorDraftStore.delete(draftKey);
                setDirty(false);
                onDirtyChange(false);
                onNavigateHash(entryBuilderHash);
              }}
              data-testid="builder-prompt-discard"
              className="inline-flex h-9 items-center rounded-md border border-rose-200 bg-rose-50 px-3 text-xs font-medium text-rose-700 hover:bg-rose-100 disabled:opacity-50"
            >
              Bỏ thay đổi &amp; mở
            </button>
          )}
          <button
            type="button"
            autoFocus
            disabled={saving}
            onClick={() => setBuilderPromptOpen(false)}
            data-testid="builder-prompt-cancel"
            className="inline-flex h-9 items-center rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 hover:bg-zinc-50 disabled:opacity-50"
          >
            Hủy
          </button>
          <button
            type="button"
            disabled={saving}
            onClick={() => {
              void (async () => {
                const intent = publicationPolicy === 'draft_publish' ? publicationStatus : undefined;
                const savedId = await handleSave(intent, false);
                setBuilderPromptOpen(false);
                if (savedId !== null) {
                  onNavigateHash(`#/studio/entry/${encodeURIComponent(typeKey)}/${savedId}`);
                }
              })();
            }}
            data-testid="builder-prompt-save"
            className="inline-flex h-9 items-center rounded-md bg-od-accent px-3.5 text-xs font-medium text-white hover:bg-od-accent-hover disabled:opacity-50"
          >
            {saving ? 'Đang lưu…' : 'Lưu & mở Trình dựng trang'}
          </button>
        </div>
      </dialog>
      {mediaPicker && mediaPicker.scopeKey === mediaScopeKey && csrfToken && user && config
        && config.schema_token === mediaPicker.schemaToken
        && editorDraftStore.getSessionEpoch() === mediaPicker.sessionEpoch
        && config.effective_fields[mediaPicker.fieldKey]?.editable
        && config.effective_fields[mediaPicker.fieldKey]?.effective_status === 'active'
        && supportAllowsField(typeKey, mediaPicker.fieldKey, effectiveContentTypeSettings(typeKey, config.settings))
        && config.effective_fields[mediaPicker.fieldKey]?.definition.type === mediaPicker.kind
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
      {translationModalLocale && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
          data-testid="create-translation-modal"
        >
          <div className="w-full max-w-md rounded-lg bg-white p-5 shadow-xl space-y-4">
            <h3 className="text-base font-semibold text-zinc-900">
              Tạo bản dịch ({translationModalLocale})
            </h3>
            <p className="text-xs text-zinc-600">
              Hệ thống sẽ sao chép nội dung, custom fields, taxonomy terms cùng ngôn ngữ và media tham chiếu sang bản dịch mới ở trạng thái nháp.
            </p>
            <div className="space-y-3">
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                <span>Tiêu đề (tùy chọn)</span>
                <input
                  type="text"
                  value={translationTitle}
                  onChange={e => setTranslationTitle(e.target.value)}
                  placeholder={title || 'Để trống để sao chép tiêu đề gốc'}
                  className="h-9 rounded-md border border-zinc-300 px-2.5 text-sm text-zinc-900"
                />
              </label>
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                <span>Slug (tùy chọn)</span>
                <input
                  type="text"
                  value={translationSlug}
                  onChange={e => setTranslationSlug(e.target.value)}
                  placeholder="Để trống để tự sinh theo tiêu đề"
                  className="h-9 rounded-md border border-zinc-300 px-2.5 text-sm text-zinc-900"
                />
              </label>
              {translationError && (
                <p className="text-xs font-medium text-rose-700">{translationError}</p>
              )}
            </div>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                disabled={creatingTranslation}
                onClick={() => setTranslationModalLocale(null)}
                className="inline-flex h-9 items-center rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 hover:bg-zinc-50 disabled:opacity-50"
              >
                Hủy
              </button>
              <button
                type="button"
                disabled={creatingTranslation || !csrfToken}
                onClick={() => void handleCreateTranslation()}
                data-testid="confirm-create-translation"
                className="inline-flex h-9 items-center rounded-md bg-od-accent px-3.5 text-xs font-medium text-white hover:bg-od-accent-hover disabled:opacity-50"
              >
                {creatingTranslation ? 'Đang tạo…' : 'Tạo bản dịch'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
