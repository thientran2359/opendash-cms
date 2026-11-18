import React, { useEffect, useRef, useState } from 'react';
import {
  createEntryComposite,
  getContentEntry,
  getEffectiveEditorConfig,
  updateEntryComposite,
} from '../../api/content.ts';
import { runPluginAction } from '../../api/plugins.ts';
import { getEntryRoute, getEntrySeo } from '../../api/seo.ts';
import { ApiClientError } from '../../api/client.ts';
import type { AuthUser } from '../../types.ts';
import type {
  ContentFieldValue,
  EffectiveEditorConfigProjection,
} from '../content/types.ts';
import { editorDraftStore, diffEditorPayload, type EditorDraftPayload } from './EditorDraftStore.ts';
import { PanelRegionRenderer } from './PanelRegionRenderer.tsx';
import { MediaPicker } from '../media/MediaPicker.tsx';
import { EntryListPage, entryTableHash, parseEntryTableHash } from '../content/EntryListPage.tsx';

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
  const [config, setConfig] = useState<EffectiveEditorConfigProjection | null>(null);
  const [title, setTitle] = useState('');
  const [slug, setSlug] = useState('');
  const [publicationStatus, setPublicationStatus] = useState<'draft' | 'published'>('draft');
  const [values, setValues] = useState<Record<string, ContentFieldValue>>({});
  const [clearedFields, setClearedFields] = useState<string[]>([]);
  const [termIds, setTermIds] = useState<number[]>([]);
  const [customPath, setCustomPath] = useState('');
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
  const [mediaPicker, setMediaPicker] = useState<FieldMediaPickerTarget | null>(null);
  const baselineRef = useRef<EditorDraftPayload | null>(null);
  const draftRef = useRef<EditorDraftPayload | null>(null);
  const editRevisionRef = useRef(0);
  const schemaTokenRef = useRef('');
  const canWriteRouteRef = useRef(true);
  const canWriteSeoRef = useRef(true);

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
    setDirty(changed.length > 0);
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

        const freshCfg = await getEffectiveEditorConfig(typeKey);
        if (!active || reqEpoch !== editorDraftStore.getSessionEpoch()) return;
        const emptySeo = { title: null, description: null, og_image_id: null, canonical_url: null, noindex: false };
        let baseline: EditorDraftPayload;
        let routeReadable = true;
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
          routeReadable = routeSnap !== null;
          seoReadable = seoSnap !== null;
          const allowedValues = Object.fromEntries(
            Object.entries(rec.values).filter(([key]) => key in freshCfg.effective_fields),
          );
          baseline = {
            id: rec.id, version: rec.version, title: rec.title, slug: rec.slug,
            slug_touched: true, publication_status: rec.publication_status,
            values: allowedValues, cleared_fields: [], term_ids: rec.term_ids ?? [],
            custom_path: routeSnap?.custom_path ?? '',
            seo: seoSnap ? {
              title: seoSnap.title, description: seoSnap.description,
              og_image_id: seoSnap.og_image_id, canonical_url: seoSnap.canonical_url,
              noindex: seoSnap.noindex,
            } : emptySeo,
          };
        } else {
          baseline = {
            id: null, version: null, title: '', slug: '', slug_touched: false,
            publication_status: typeKey === 'post' || typeKey === 'page' ? 'draft' : 'published',
            values: {}, cleared_fields: [], term_ids: [], custom_path: '', seo: emptySeo,
          };
        }
        const restored = editorDraftStore.restoreAuthorizedDraft(
          draftKey, siteInstanceId, user.id, freshCfg, authGeneration, baseline,
          routeReadable, seoReadable,
        );
        if (!active || reqEpoch !== editorDraftStore.getSessionEpoch()) return;
        baselineRef.current = baseline;
        draftRef.current = restored?.payload ?? baseline;
        schemaTokenRef.current = restored?.schema_token_at_edit ?? freshCfg.schema_token;
        canWriteRouteRef.current = routeReadable;
        canWriteSeoRef.current = seoReadable;
        editRevisionRef.current = 0;
        applyPayload(draftRef.current);
        setDirty(Boolean(restored?.dirty_fields.length));
        setConfig(freshCfg);
        if (restored && restored.schema_token_at_edit !== freshCfg.schema_token) {
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
    const snapshot = draftRef.current;
    const submittedRevision = editRevisionRef.current;
    const requestEpoch = editorDraftStore.getSessionEpoch();
    const expectedSchemaToken = schemaTokenRef.current;
    const nextErrors: Record<string, string> = {};
    if (!snapshot.title.trim()) nextErrors.title = 'Tiêu đề không được để trống.';
    if (!snapshot.slug.trim()) nextErrors.slug = 'Slug không được để trống.';

    for (const [fk, eff] of Object.entries(config.effective_fields)) {
      if (eff.effective_required && eff.editable) {
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
      if (!eff.editable) continue;
      if (snapshot.cleared_fields.includes(fk)) {
        submitValues[fk] = null;
      } else if (fk in snapshot.values) {
        submitValues[fk] = snapshot.values[fk];
      }
    }

    try {
      const payload = {
        expected_version: entryId === null ? undefined : (snapshot.version ?? undefined),
        expected_schema_token: expectedSchemaToken,
        title: snapshot.title.trim(),
        slug: snapshot.slug.trim(),
        publication_status: isBuiltInContent ? intent : snapshot.publication_status,
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
        values: res.entry.values, cleared_fields: [], term_ids: res.entry.term_ids ?? snapshot.term_ids,
        custom_path: res.custom_path ?? '',
        seo: res.seo ? {
          title: res.seo.title, description: res.seo.description,
          og_image_id: res.seo.og_image_id, canonical_url: res.seo.canonical_url,
          noindex: res.seo.noindex,
        } : snapshot.seo,
      };
      baselineRef.current = saved;
      schemaTokenRef.current = config.schema_token;
      editorDraftStore.delete(draftKey);
      const hasNewerChanges = editRevisionRef.current !== submittedRevision;
      if (!hasNewerChanges) {
        draftRef.current = saved;
        applyPayload(saved);
        setDirty(false);
      } else if (draftRef.current) {
        const remaining = { ...draftRef.current, id: res.entry.id, version: res.entry.version,
          publication_status: isBuiltInContent ? res.entry.publication_status : draftRef.current.publication_status };
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
        setDirty(changed.length > 0);
      }
      setStatusNote(hasNewerChanges
        ? `Đã lưu snapshot v${res.entry.version}; chỉnh sửa mới hơn vẫn chưa lưu.`
        : `Đã lưu thành công (v${res.entry.version}).`);
      if (entryId === null) {
        onNavigateHash(`${listHashPrefix}/${res.entry.id}/edit`);
      }
    } catch (err) {
      if (err instanceof ApiClientError && err.status === 401) {
        editorDraftStore.quarantineAll();
      }
      if (err instanceof ApiClientError && err.status === 412) {
        setStatusNote('Có xung đột version hoặc schema (412). Nháp vẫn được giữ để xử lý.');
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

  // Link from the last server-confirmed version, never a dirty custom-path
  // draft. A draft has no public URL and must not get a public View link.
  const confirmed = baselineRef.current;
  const confirmedCustomPath = confirmed?.custom_path;
  const publicPath = confirmed?.id === entryId && confirmed?.publication_status === 'published'
    ? confirmedCustomPath?.startsWith('/') && !confirmedCustomPath.startsWith('//')
      ? confirmedCustomPath
      : `/content/${encodeURIComponent(typeKey)}/${encodeURIComponent(confirmed.slug)}`
    : null;

  return (
    <div className="space-y-4">
      <button type="button" className="rounded-lg border border-slate-300 px-3 py-2 text-sm font-medium text-slate-700 hover:bg-slate-50"
        onClick={() => onNavigateHash(entryTableHash(listHashPrefix, parseEntryTableHash(routeHash ?? listHashPrefix)))}>
        ← Về danh sách
      </button>
      {statusNote && (
        <div className={statusNote.includes('(412)')
          ? 'rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-800'
          : 'rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-800'}
          role={statusNote.includes('(412)') ? 'alert' : 'status'}>
          {statusNote}
        </div>
      )}
      <PanelRegionRenderer
        config={config}
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
        onOpenBuilder={() => {
          if (entryId === null) {
            setStatusNote('Vui lòng Lưu nội dung trước khi mở Visual Builder.');
            return;
          }
          onNavigateHash(`#/builder?template=single&type=${encodeURIComponent(typeKey)}&entry=${entryId}`);
        }}
      />
      {mediaPicker && mediaPicker.scopeKey === mediaScopeKey && csrfToken && user && config
        && config.schema_token === mediaPicker.schemaToken
        && editorDraftStore.getSessionEpoch() === mediaPicker.sessionEpoch
        && config.effective_fields[mediaPicker.fieldKey]?.editable
        && config.effective_fields[mediaPicker.fieldKey]?.effective_status === 'active'
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
