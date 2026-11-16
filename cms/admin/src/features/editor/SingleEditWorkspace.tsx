import React, { useEffect, useRef, useState } from 'react';
import {
  createEntryComposite,
  getContentEntry,
  getEffectiveEditorConfig,
  listContentEntries,
  updateEntryComposite,
} from '../../api/content.ts';
import { runPluginAction } from '../../api/plugins.ts';
import { getEntryRoute, getEntrySeo } from '../../api/seo.ts';
import { ApiClientError } from '../../api/client.ts';
import type { AuthUser } from '../../types.ts';
import type {
  ContentEntryRecord,
  ContentFieldValue,
  EffectiveEditorConfigProjection,
} from '../content/types.ts';
import { editorDraftStore } from './EditorDraftStore.ts';
import { PanelRegionRenderer } from './PanelRegionRenderer.tsx';

interface SingleEditWorkspaceProps {
  typeKey: string;
  mode: 'list' | 'new' | 'edit';
  entryId: number | null;
  user: AuthUser | null;
  siteInstanceId: string;
  authGeneration: number;
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
  csrfToken,
  onApiError,
  onDirtyChange,
  onNavigateHash,
}) => {
  const [config, setConfig] = useState<EffectiveEditorConfigProjection | null>(null);
  const [entries, setEntries] = useState<ContentEntryRecord[]>([]);
  const [title, setTitle] = useState('');
  const [slug, setSlug] = useState('');
  const [slugTouched, setSlugTouched] = useState(false);
  const [publicationStatus, setPublicationStatus] = useState<'draft' | 'published'>('draft');
  const [values, setValues] = useState<Record<string, ContentFieldValue>>({});
  const [clearedFields, setClearedFields] = useState<string[]>([]);
  const [serverValues, setServerValues] = useState<Record<string, ContentFieldValue>>({});
  const [version, setVersion] = useState<number | null>(null);
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
  const [dirty, setDirty] = useState(false);
  const [statusNote, setStatusNote] = useState<string | null>(null);

  const tabUuid = useRef(Math.random().toString(36).slice(2, 10));
  const draftKey = editorDraftStore.makeDraftKey(typeKey, entryId, tabUuid.current);

  const listHashPrefix = typeKey === 'post' ? '#/posts' : typeKey === 'page' ? '#/pages' : `#/cpt/${typeKey}`;

  useEffect(() => {
    onDirtyChange(dirty);
  }, [dirty, onDirtyChange]);

  useEffect(() => {
    if (!user) return;
    let active = true;
    const reqEpoch = editorDraftStore.getSessionEpoch();

    (async () => {
      try {
        if (mode === 'list') {
          const list = await listContentEntries(typeKey, 50, 0);
          if (!active || reqEpoch !== editorDraftStore.getSessionEpoch()) return;
          setEntries(list);
          return;
        }

        const freshCfg = await getEffectiveEditorConfig(typeKey);
        if (!active || reqEpoch !== editorDraftStore.getSessionEpoch()) return;
        setConfig(freshCfg);

        const existingDraft = editorDraftStore.get(draftKey);
        if (existingDraft && existingDraft.status === 'quarantined') {
          const scrubbed = editorDraftStore.scrubQuarantinedDraft(draftKey, freshCfg, authGeneration);
          if (scrubbed) {
            setTitle(scrubbed.payload.title);
            setSlug(scrubbed.payload.slug);
            setPublicationStatus(scrubbed.payload.publication_status);
            setValues(scrubbed.payload.values);
            setClearedFields(scrubbed.payload.cleared_fields);
            setVersion(scrubbed.payload.version);
            setCustomPath(scrubbed.payload.custom_path ?? '');
            setSeoTitle(scrubbed.payload.seo?.title ?? '');
            setSeoDescription(scrubbed.payload.seo?.description ?? '');
            setSeoOgImageId(scrubbed.payload.seo?.og_image_id ?? '');
            setSeoNoindex(Boolean(scrubbed.payload.seo?.noindex));
            setDirty(scrubbed.dirty_fields.length > 0);
            return;
          }
        }

        if (mode === 'edit' && entryId !== null) {
          const [rec, routeSnap, seoSnap] = await Promise.all([
            getContentEntry(typeKey, entryId),
            getEntryRoute(typeKey, entryId).catch(() => null),
            getEntrySeo(typeKey, entryId).catch(() => null),
          ]);
          if (!active || reqEpoch !== editorDraftStore.getSessionEpoch()) return;
          setVersion(rec.version);
          setTitle(rec.title);
          setSlug(rec.slug);
          setSlugTouched(true);
          setPublicationStatus(rec.publication_status);
          setValues(rec.values);
          setServerValues(rec.values);
          setClearedFields([]);
          setCustomPath(routeSnap?.custom_path ?? '');
          setSeoTitle(seoSnap?.title ?? '');
          setSeoDescription(seoSnap?.description ?? '');
          setSeoOgImageId(seoSnap?.og_image_id ?? '');
          setSeoNoindex(Boolean(seoSnap?.noindex));
          setDirty(false);
        } else {
          setVersion(null);
          setTitle('');
          setSlug('');
          setSlugTouched(false);
          setPublicationStatus(typeKey === 'post' || typeKey === 'page' ? 'draft' : 'published');
          setValues({});
          setServerValues({});
          setClearedFields([]);
          setCustomPath('');
          setSeoTitle('');
          setSeoDescription('');
          setSeoOgImageId('');
          setSeoNoindex(false);
          setDirty(false);
        }
      } catch (err) {
        if (err instanceof ApiClientError && err.status === 401) {
          editorDraftStore.quarantineAll();
        }
        onApiError(err, 'Không thể tải dữ liệu màn hình chỉnh sửa.');
      }
    })();

    return () => {
      active = false;
    };
  }, [typeKey, mode, entryId, user?.id, authGeneration]);

  // Sync active edits into RAM-only EditorDraftStore
  const persistDraftInRam = (nextValues: Record<string, ContentFieldValue>, nextCleared: string[], nextTitle: string, nextSlug: string) => {
    if (!user || !config || mode === 'list') return;
    editorDraftStore.set({
      site_instance_id: siteInstanceId,
      user_id: user.id,
      auth_generation: authGeneration,
      draft_key: draftKey,
      type_key: typeKey,
      status: 'active',
      schema_token_at_edit: config.schema_token,
      dirty_fields: Array.from(new Set([...Object.keys(nextValues), ...nextCleared, 'title', 'slug'])),
      server_values: serverValues,
      payload: {
        id: entryId,
        version,
        title: nextTitle,
        slug: nextSlug,
        publication_status: publicationStatus,
        values: nextValues,
        cleared_fields: nextCleared,
        term_ids: [],
        custom_path: customPath,
        seo: {
          title: seoTitle || null,
          description: seoDescription || null,
          og_image_id: seoOgImageId || null,
          noindex: seoNoindex,
        },
      },
    });
  };

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

  const handleSave = async () => {
    if (!config || !csrfToken) return;
    const nextErrors: Record<string, string> = {};
    if (!title.trim()) nextErrors.title = 'Tiêu đề không được để trống.';
    if (!slug.trim()) nextErrors.slug = 'Slug không được để trống.';

    for (const [fk, eff] of Object.entries(config.effective_fields)) {
      if (eff.effective_required && eff.editable) {
        const v = values[fk];
        const empty = clearedFields.includes(fk) || v === undefined || v === null || (typeof v === 'string' && !v.trim());
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
    setSaving(true);
    setStatusNote(null);

    // Send ONLY editable fields (never send read-only fields -> Omitted = Preserve)
    const submitValues: Record<string, ContentFieldValue> = {};
    for (const [fk, eff] of Object.entries(config.effective_fields)) {
      if (!eff.editable) continue;
      if (clearedFields.includes(fk)) {
        submitValues[fk] = null;
      } else if (fk in values) {
        submitValues[fk] = values[fk];
      }
    }

    try {
      const payload = {
        expected_version: entryId === null ? undefined : (version ?? undefined),
        expected_schema_token: config.schema_token,
        title: title.trim(),
        slug: slug.trim(),
        publication_status: publicationStatus,
        values: submitValues,
        custom_path: customPath.trim() ? customPath.trim() : null,
        seo: {
          title: seoTitle.trim() || null,
          description: seoDescription.trim() || null,
          og_image_id: seoOgImageId.trim() || null,
          noindex: seoNoindex,
        },
      };

      const res =
        entryId === null
          ? await createEntryComposite(typeKey, payload, csrfToken)
          : await updateEntryComposite(typeKey, entryId, payload, csrfToken);

      setVersion(res.entry.version);
      setValues(res.entry.values);
      setServerValues(res.entry.values);
      setClearedFields([]);
      setDirty(false);
      editorDraftStore.delete(draftKey);
      setStatusNote(`Đã lưu thành công (v${res.entry.version}).`);
      if (entryId === null) {
        onNavigateHash(`${listHashPrefix}/${res.entry.id}/edit`);
      }
    } catch (err) {
      if (err instanceof ApiClientError && err.status === 401) {
        editorDraftStore.quarantineAll();
      }
      onApiError(err, 'Không thể lưu nội dung.');
    } finally {
      setSaving(false);
    }
  };

  if (mode === 'list') {
    return (
      <section className="space-y-4" data-testid={`entry-list-${typeKey}`}>
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold text-slate-900">Danh sách ({typeKey})</h2>
          <button
            type="button"
            onClick={() => onNavigateHash(`${listHashPrefix}/new`)}
            className="rounded-lg bg-slate-900 px-3.5 py-2 text-sm font-semibold text-white hover:bg-slate-800"
          >
            + Thêm mới
          </button>
        </div>
        {entries.length === 0 ? (
          <p className="text-sm text-slate-500">Chưa có nội dung nào.</p>
        ) : (
          <ul className="divide-y divide-slate-200 rounded-xl border border-slate-200 bg-white">
            {entries.map(e => (
              <li key={e.id} className="flex items-center justify-between px-4 py-3 text-sm">
                <div>
                  <div className="font-semibold text-slate-900">{e.title}</div>
                  <div className="text-xs text-slate-500">
                    /{e.slug} · {e.publication_status} · v{e.version}
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => onNavigateHash(`${listHashPrefix}/${e.id}/edit`)}
                  className="rounded border border-slate-300 px-3 py-1 text-xs font-medium text-slate-700 hover:bg-slate-50"
                >
                  Chỉnh sửa
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    );
  }

  if (!config) {
    return <p className="text-sm text-slate-600">Đang tải cấu hình trình chỉnh sửa...</p>;
  }

  return (
    <div className="space-y-4">
      {statusNote && (
        <div className="rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-800" role="status">
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
        saving={saving}
        dirty={dirty}
        onToggleCollapse={pid => setCollapsedPanels(prev => ({ ...prev, [pid]: !prev[pid] }))}
        onTitleChange={nextTitle => {
          const nextSlug = slugTouched ? slug : slugFromTitle(nextTitle);
          setTitle(nextTitle);
          if (!slugTouched) setSlug(nextSlug);
          setDirty(true);
          persistDraftInRam(values, clearedFields, nextTitle, nextSlug);
        }}
        onSlugChange={nextSlug => {
          setSlug(nextSlug);
          setSlugTouched(true);
          setDirty(true);
          persistDraftInRam(values, clearedFields, title, nextSlug);
        }}
        onPublicationStatusChange={s => {
          setPublicationStatus(s);
          setDirty(true);
        }}
        onFieldChange={(fk, val) => {
          const nextVals = { ...values, [fk]: val };
          const nextCleared = clearedFields.filter(k => k !== fk);
          setValues(nextVals);
          setClearedFields(nextCleared);
          setDirty(true);
          persistDraftInRam(nextVals, nextCleared, title, slug);
        }}
        onFieldClear={fk => {
          const nextVals = { ...values };
          delete nextVals[fk];
          const nextCleared = Array.from(new Set([...clearedFields, fk]));
          setValues(nextVals);
          setClearedFields(nextCleared);
          setDirty(true);
          persistDraftInRam(nextVals, nextCleared, title, slug);
        }}
        onCustomPathChange={p => {
          setCustomPath(p);
          setDirty(true);
        }}
        onSeoChange={patch => {
          if (patch.title !== undefined) setSeoTitle(patch.title);
          if (patch.description !== undefined) setSeoDescription(patch.description);
          if (patch.og_image_id !== undefined) setSeoOgImageId(patch.og_image_id);
          if (patch.noindex !== undefined) setSeoNoindex(patch.noindex);
          setDirty(true);
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
        onSave={() => void handleSave()}
        onOpenBuilder={() => {
          if (entryId === null) {
            setStatusNote('Vui lòng Lưu nội dung trước khi mở Visual Builder.');
            return;
          }
          onNavigateHash(`#/builder?template=single&type=${encodeURIComponent(typeKey)}&entry=${entryId}`);
        }}
      />
    </div>
  );
};
