import React, { useEffect, useMemo, useRef, useState } from 'react';
import {
  createEntryComposite,
  contentEntryPreviewPath,
  effectiveContentTypeSettings,
  getContentEntry,
  getEffectiveEditorConfig,
  listContentTypes,
  type ContentTypeSettings,
  type EffectiveEditorConfigWithSettings,
  updateEntryComposite,
} from '../../api/content.ts';
import { getEntryBuilder } from '../../api/builder.ts';
import { runPluginAction } from '../../api/plugins.ts';
import { getEntryRoute, getEntrySeo } from '../../api/seo.ts';
import { ApiClientError } from '../../api/client.ts';
import { getActiveThemeLayouts, type ThemeLayoutCatalog } from '../../api/themes.ts';
import type { AuthUser } from '../../types.ts';
import type {
  ContentFieldValue,
  ContentMode,
  EffectiveEditorConfigProjection,
  EffectivePanelProjection,
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
  if (typeKey !== 'post' && typeKey !== 'page') return true;
  if (key === 'body') return settings.supports.includes('editor');
  if (key === 'featured_image') return settings.supports.includes('featured_image');
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
      if (panel.kind.core_key === 'body_editor') return settings.supports.includes('editor');
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
  onApiError,
  onDirtyChange,
  onNavigateHash,
}) => {
  const [config, setConfig] = useState<EffectiveEditorConfigWithSettings | null>(null);
  const [title, setTitle] = useState('');
  const [slug, setSlug] = useState('');
  const [publicationStatus, setPublicationStatus] = useState<'draft' | 'published'>('draft');
  const [contentMode, setContentMode] = useState<ContentMode>('editor');
  const [hasBuilderDocument, setHasBuilderDocument] = useState(false);
  const [values, setValues] = useState<Record<string, ContentFieldValue>>({});
  const [clearedFields, setClearedFields] = useState<string[]>([]);
  const [termIds, setTermIds] = useState<number[]>([]);
  const [customPath, setCustomPath] = useState('');
  const [themeLayouts, setThemeLayouts] = useState<ThemeLayoutCatalog | null>(null);
  const [selectedThemeKey, setSelectedThemeKey] = useState<string | null>(null);
  const [themeKeyDirty, setThemeKeyDirty] = useState(false);
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

  const listHashPrefix = typeKey === 'post' ? '#/posts' : typeKey === 'page' ? '#/pages' : `#/cpt/${typeKey}`;

  const applyPayload = (payload: EditorDraftPayload) => {
    setTitle(payload.title);
    setSlug(payload.slug);
    setPublicationStatus(payload.publication_status);
    setContentMode(payload.content_mode ?? 'editor');
    setValues(payload.values);
    setClearedFields(payload.cleared_fields);
    setTermIds(payload.term_ids);
    setCustomPath(payload.custom_path ?? '');
    setSeoTitle(payload.seo?.title ?? '');
    setSeoDescription(payload.seo?.description ?? '');
    setSeoOgImageId(payload.seo?.og_image_id ?? '');
    setSeoNoindex(Boolean(payload.seo?.noindex));
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
    setConfirmedPublicLink(null);
    setThemeLayouts(null);
    setHasBuilderDocument(false);
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
          const [rec, routeSnap, seoSnap, builderSnap] = await Promise.all([
            getContentEntry(typeKey, entryId),
            optional(getEntryRoute(typeKey, entryId)),
            optional(getEntrySeo(typeKey, entryId)),
            typeKey !== 'section' && freshCfg.settings?.supports.includes('builder')
              ? optional(getEntryBuilder(typeKey, entryId)) : Promise.resolve(null),
          ]);
          if (!active || reqEpoch !== editorDraftStore.getSessionEpoch()) return;
          setHasBuilderDocument(Boolean(builderSnap?.document));
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
          baseline = {
            id: rec.id, version: rec.version, title: rec.title, slug: rec.slug,
            slug_touched: true, publication_status: rec.publication_status,
            content_mode: rec.content_mode,
            values: allowedValues, cleared_fields: [], term_ids: rec.term_ids ?? [],
            custom_path: routeSnap?.custom_path ?? '',
            seo: seoSnap ? {
              title: seoSnap.title, description: seoSnap.description,
              og_image_id: seoSnap.og_image_id, canonical_url: seoSnap.canonical_url,
              noindex: seoSnap.noindex,
            } : emptySeo,
          };
        } else {
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
            values: {}, cleared_fields: [], term_ids: [], custom_path: '', seo: emptySeo,
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

  const handleSave = async (intent?: 'draft' | 'published') => {
    if (!config || !csrfToken || saving || saveInFlightRef.current || !draftRef.current || !user) return;
    const isBuiltInContent = typeKey === 'post' || typeKey === 'page';
    if (isBuiltInContent && !intent) return;
    const effectiveSettings = effectiveContentTypeSettings(typeKey, config.settings);
    if (typeKey === 'section') return; // Section uses its dedicated private editor.
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
      return;
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
      };

      const res =
        entryId === null
          ? await createEntryComposite(typeKey, payload, csrfToken)
          : await updateEntryComposite(typeKey, entryId, payload, csrfToken);
      if (requestEpoch !== editorDraftStore.getSessionEpoch() || activeDraftKeyRef.current !== draftKey) return;
      const saved: EditorDraftPayload = {
        ...snapshot,
        id: res.entry.id, version: res.entry.version, title: res.entry.title, slug: res.entry.slug,
        slug_touched: true, publication_status: res.entry.publication_status,
        content_mode: res.entry.content_mode,
        values: res.entry.values, cleared_fields: [], term_ids: res.entry.term_ids ?? snapshot.term_ids,
        custom_path: res.custom_path ?? '',
        seo: res.seo ? {
          title: res.seo.title, description: res.seo.description,
          og_image_id: res.seo.og_image_id, canonical_url: res.seo.canonical_url,
          noindex: res.seo.noindex,
        } : snapshot.seo,
      };
      baselineRef.current = saved;
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
      if (entryId === null) {
        onNavigateHash(`${listHashPrefix}/${res.entry.id}/edit`);
      }
    } catch (err) {
      if (requestEpoch !== editorDraftStore.getSessionEpoch() || activeDraftKeyRef.current !== draftKey
          || activeMediaScopeRef.current !== mediaScopeKey) return;
      if (err instanceof ApiClientError && err.status === 401) {
        editorDraftStore.quarantineAll();
      }
      if (err instanceof ApiClientError && err.status === 412) {
        setStatusNote('Có xung đột version hoặc schema (412). Nháp vẫn được giữ để xử lý.');
      } else if (err instanceof ApiClientError && err.status === 409) {
        setStatusNote('Thao tác bị từ chối vì xung đột chính sách xuất bản (409). Nháp vẫn được giữ.');
      }
      onApiError(err, 'Không thể lưu nội dung.');
    } finally {
      saveInFlightRef.current = false;
      setSaving(false);
    }
  };

  if (mode === 'list') {
    if (!isActive || !user) return null;
    return <EntryListPage typeKey={typeKey} user={user} siteInstanceId={siteInstanceId}
      authGeneration={authGeneration} csrfToken={csrfToken} routeHash={routeHash ?? listHashPrefix}
      onApiError={onApiError} onNavigateHash={onNavigateHash} />;
  }

  if (!config) {
    return <p className="text-sm text-slate-600">Đang tải cấu hình trình chỉnh sửa...</p>;
  }

  // Public URLs must come from the server route authority. Never derive a URL
  // from the slug/custom path or from an unsaved editor draft.
  const confirmed = baselineRef.current;
  const publicPath = settings?.public && typeKey !== 'section'
    && confirmed?.id === entryId && confirmed?.publication_status === 'published'
    && confirmedPublicLink?.scope === mediaScopeKey
    && confirmedPublicLink.version === confirmed.version
    ? confirmedPublicLink.path
    : null;
  const draftPreviewPath = typeKey !== 'section' && confirmed?.id === entryId
    && confirmed.publication_status === 'draft' && entryId !== null
    ? contentEntryPreviewPath(typeKey, entryId) : null;
  const isBuiltIn = typeKey === 'post' || typeKey === 'page';
  const isDraftPublishCpt = !isBuiltIn && typeKey !== 'section'
    && settings?.publication === 'draft_publish';

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <button type="button" className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-700 hover:bg-zinc-50"
          onClick={() => onNavigateHash(entryTableHash(listHashPrefix, parseEntryTableHash(routeHash ?? listHashPrefix)))}>
          ← Về danh sách
        </button>
        <div className="flex flex-wrap items-center gap-2" data-testid="theme-template-selection">
          <label className="text-xs font-medium text-zinc-600" htmlFor="entry-theme-selector">
            {typeKey === 'page' ? 'Page Layout' : 'Template'}
          </label>
          <select id="entry-theme-selector" className="h-8 rounded-md border border-zinc-300 bg-white px-2.5 text-xs text-zinc-900"
            value={selectedThemeKey ?? ''} disabled={saving || !csrfToken || !themeLayouts}
            onChange={event => {
              const choice = event.target.value || null;
              selectedThemeKeyRef.current = choice;
              setSelectedThemeKey(choice);
              const differs = choice !== savedThemeKeyRef.current;
              themeKeyDirtyRef.current = differs;
              setThemeKeyDirty(differs);
              if (differs) editRevisionRef.current++;
              setDirty(differs || !!(draftRef.current && baselineRef.current && diffEditorPayload(baselineRef.current, draftRef.current).length));
            }}>
            <option value="">Theo theme mặc định</option>
            {(typeKey === 'page' ? themeLayouts?.page_layouts.map(layout => ({ key: layout.key, label: layout.label }))
              : themeLayouts?.custom_templates.filter(tpl => tpl.allowed_types.includes(typeKey))
                  .map(tpl => ({ key: tpl.custom_key, label: tpl.label })))?.map(choice =>
                <option key={choice.key} value={choice.key}>{choice.label}</option>)}
            {selectedThemeKey && !(typeKey === 'page'
              ? themeLayouts?.page_layouts.some(layout => layout.key === selectedThemeKey)
              : themeLayouts?.custom_templates.some(tpl => tpl.custom_key === selectedThemeKey && tpl.allowed_types.includes(typeKey))) &&
              <option value={selectedThemeKey}>{selectedThemeKey} (theme hiện tại không còn cung cấp — được bảo toàn)</option>}
          </select>
          {themeKeyDirty && <span className="text-xs text-amber-700">Chưa lưu</span>}
        </div>
      </div>
      {statusNote && (
        <div className={statusNote.includes('(412)') || statusNote.includes('(409)')
          ? 'rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-800'
          : 'rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-800'}
          role={statusNote.includes('(412)') || statusNote.includes('(409)') ? 'alert' : 'status'}>
          {statusNote}
        </div>
      )}
      {draftPreviewPath && (
        <a href={draftPreviewPath} target="_blank" rel="noopener noreferrer"
          className="inline-flex rounded-lg border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-800">
          Xem trước bản nháp đã lưu (yêu cầu đăng nhập)
        </a>
      )}
      {typeKey !== 'section' && entryId !== null &&
        (settings?.supports.includes('builder') || contentMode === 'builder' ||
          baselineRef.current?.content_mode === 'builder') && (
        <div className="rounded-xl border border-slate-200 bg-white p-3 space-y-2">
          <label htmlFor={typeKey === 'page' ? 'page-content-mode' : 'entry-content-mode'}
            className="block text-sm font-semibold">Nguồn nội dung {typeKey === 'page' ? 'Page' : typeKey}</label>
          <select id={typeKey === 'page' ? 'page-content-mode' : 'entry-content-mode'}
            value={contentMode} disabled={saving || !csrfToken}
            onChange={event => {
              const next = event.target.value as ContentMode;
              if (next === 'builder' &&
                  (!settings?.supports.includes('builder') || (!hasBuilderDocument && contentMode !== 'builder'))) return;
              setContentMode(next);
              persistDraftPatch({ content_mode: next });
            }}>
            <option value="editor">Văn bản</option>
            <option value="builder" disabled={!settings?.supports.includes('builder') ||
              (!hasBuilderDocument && contentMode !== 'builder')}>
              Visual Builder{!settings?.supports.includes('builder') ? ' (đã tắt)' : ''}
            </option>
          </select>
          <p className="text-xs text-slate-600">
            {!settings?.supports.includes('builder')
              ? 'Builder đã tắt; hệ thống hiển thị văn bản. Chọn Văn bản rồi lưu để cập nhật nguồn, thiết kế cũ vẫn được giữ.'
              : 'Chuyển nguồn khi lưu giữ nguyên văn bản và thiết kế đã lưu.'}
          </p>
        </div>
      )}
      {isDraftPublishCpt && (
        <div className="flex flex-wrap items-center gap-3 rounded-xl border border-slate-200 bg-white p-3"
          role="group" aria-label="Lưu nháp và xuất bản">
          <span className="text-sm text-slate-700">
            {publicationStatus === 'published' ? 'Đã xuất bản' : 'Bản nháp'}
          </span>
          <button type="button" disabled={saving || !csrfToken}
            onClick={() => void handleSave('draft')}
            className="rounded-lg border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-800 disabled:opacity-50">
            {publicationStatus === 'published' ? 'Chuyển về nháp' : 'Lưu nháp'}
          </button>
          <button type="button" disabled={saving || !csrfToken}
            onClick={() => void handleSave('published')}
            className="rounded-lg bg-slate-900 px-3 py-2 text-sm font-semibold text-white disabled:opacity-50">
            {publicationStatus === 'published' ? 'Cập nhật' : 'Đăng bài'}
          </button>
          {publicPath && <a href={publicPath} target="_blank" rel="noopener noreferrer"
            className="rounded-lg border border-slate-300 px-3 py-2 text-sm font-semibold text-slate-800">
            Xem nội dung
          </a>}
        </div>
      )}
      {!isBuiltIn && !isDraftPublishCpt && publicPath && (
        <a href={publicPath} target="_blank" rel="noopener noreferrer"
          className="inline-flex rounded-md border border-zinc-300 bg-white px-3 py-1.5 text-xs font-medium text-zinc-800 hover:bg-zinc-50">
          Xem nội dung
        </a>
      )}
      {/* Legacy renderer has a fixed CPT status select and generic Save button.
          Keep its Builder action and layout while showing policy-aware actions above. */}
      <style>{`
        .c1-immediate [data-panel-id="core:publish_box"] label { display: none; }
        .c1-draft-publish [data-panel-id="core:publish_box"] label { display: none; }
        .c1-draft-publish [data-region="header_bar"] > div:last-child > button { display: none; }
        .c1-draft-publish [data-panel-id="core:publish_box"] div.flex.flex-wrap.items-center.gap-2 > button { display: none; }
      `}</style>
      <div className={isDraftPublishCpt ? 'c1-draft-publish' : !isBuiltIn ? 'c1-immediate' : ''}>
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
        publicPath={publicPath}
        saving={saving}
        dirty={dirty}
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
          persistDraftPatch({ term_ids: sorted });
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
        canOpenBuilder={typeKey !== 'section' && Boolean(settings?.supports.includes('builder'))}
        onOpenBuilder={() => {
          if (typeKey === 'section' || !settings?.supports.includes('builder')) {
            setStatusNote('Loại nội dung này chưa hỗ trợ Visual Builder.');
            return;
          }
          if (entryId === null) {
            setStatusNote('Vui lòng Lưu nội dung trước khi mở Visual Builder.');
            return;
          }
          onNavigateHash(`#/builder?template=entry&type=${encodeURIComponent(typeKey)}&entry=${entryId}`);
        }}
      />
      </div>
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
    </div>
  );
};
