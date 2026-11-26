import { useEffect, useMemo, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { patchEntryMetadata } from '../../api/content.ts';
import { getGeneralSettings } from '../../api/settings.ts';
import {
  listAllTaxonomyTerms,
  type TaxonomyDefinition,
  type TaxonomyTerm,
} from '../../api/taxonomy.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { formatNumber } from '../../i18n/format.ts';
import { tStatic, useI18n } from '../../i18n/index.tsx';
import type { AuthUser } from '../../types.ts';
import {
  editorDraftStore,
  isQuickEditPatchDirty,
  type QuickEditDraftPatch,
  type QuickEditOriginalMetadata,
} from '../editor/EditorDraftStore.ts';
import type {
  EntryTableRow,
  MetadataPatch,
  MetadataPatchResponse,
  TaxonomyDeltaOp,
} from './types.ts';

export interface QuickEditFormProps {
  typeKey: string;
  row: EntryTableRow;
  schemaToken: string;
  taxonomies: TaxonomyDefinition[];
  user: AuthUser;
  siteInstanceId: string;
  authGeneration: number;
  csrfToken: string | null;
  editHref: string;
  trashHref: string;
  onSaved: (updated: MetadataPatchResponse) => void;
  onCancel: () => void;
  onRefreshServerRow: () => Promise<{ row: EntryTableRow | null; schemaToken: string }>;
  onNavigateHash: (hash: string) => void;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange: (dirty: boolean) => void;
}

const inputClass =
  'h-9 w-full min-w-0 rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 ' +
  'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-od-accent disabled:bg-zinc-100 disabled:opacity-60';

/** Formats SQLite canonical UTC `"YYYY-MM-DD HH:MM:SS"` into `YYYY-MM-DDTHH:mm:ss` in `timeZone`. */
export function formatUtcToSiteLocal(
  utcSql: string | null | undefined,
  timeZone: string,
): string {
  if (!utcSql) return '';
  const trimmed = utcSql.trim();
  if (!trimmed) return '';
  const iso = trimmed.includes('T')
    ? (trimmed.endsWith('Z') ? trimmed : `${trimmed}Z`)
    : `${trimmed.replace(' ', 'T')}Z`;
  const parsed = new Date(iso);
  if (Number.isNaN(parsed.getTime())) return '';
  try {
    const parts = new Intl.DateTimeFormat('en-CA', {
      timeZone,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hourCycle: 'h23',
    }).formatToParts(parsed);
    const get = (type: string) => parts.find(p => p.type === type)?.value ?? '';
    const y = get('year');
    const m = get('month');
    const d = get('day');
    const rawH = get('hour');
    const h = rawH === '24' ? '00' : rawH;
    const min = get('minute');
    const sec = get('second');
    if (!y || !m || !d || !h || !min || !sec) return '';
    return `${y}-${m}-${d}T${h}:${min}:${sec}`;
  } catch {
    return '';
  }
}

function isTrashedError(cause: unknown): boolean {
  return (
    cause instanceof ApiClientError &&
    cause.status === 409 &&
    (cause.errorCode === 'entry.trashed' || cause.message.includes('entry_trashed'))
  );
}

function isHookScopeError(cause: unknown): boolean {
  return (
    cause instanceof ApiClientError &&
    cause.status === 422 &&
    (cause.errorCode === 'entry.metadata_hook_scope_conflict' ||
      cause.message.includes('metadata_hook_scope_conflict'))
  );
}

interface ConflictDiff {
  field: string;
  server: string;
  yours: string;
}

export function QuickEditForm({
  typeKey,
  row,
  schemaToken,
  taxonomies,
  user,
  siteInstanceId,
  authGeneration,
  csrfToken,
  editHref,
  trashHref,
  onSaved,
  onCancel,
  onRefreshServerRow,
  onNavigateHash,
  onApiError,
  onDirtyChange,
}: QuickEditFormProps) {
  useI18n();
  const titleInputRef = useRef<HTMLInputElement | null>(null);
  const supportsDraftPublish = Boolean(row.capabilities?.publish || row.capabilities?.unpublish);

  // Restore any authorized in-memory Quick Edit draft from distinct `quick_edit:` namespace.
  const initialDraft = useMemo(() => {
    const existing = editorDraftStore.getQuickEdit(typeKey, row.id);
    const wasQuarantined = existing?.status === 'quarantined';
    const restored = editorDraftStore.restoreAuthorizedQuickEditDraft(
      siteInstanceId,
      user.id,
      authGeneration,
      typeKey,
      row,
    );
    return { restored, wasQuarantined };
  }, [typeKey, row.id, siteInstanceId, user.id, authGeneration]);

  const [baseVersion, setBaseVersion] = useState<number>(
    () => initialDraft.restored?.base_version ?? row.version,
  );
  const [activeSchemaToken, setActiveSchemaToken] = useState<string>(
    () => initialDraft.restored?.schema_token ?? schemaToken,
  );
  const [originalMeta, setOriginalMeta] = useState<QuickEditOriginalMetadata>(() => {
    if (initialDraft.restored) return initialDraft.restored.original;
    return {
      title: row.title,
      slug: row.slug,
      publication_status: row.publication_status,
      published_at: row.published_at ?? null,
      published_at_local: '',
      timezone: '',
      term_ids: [...row.term_ids],
      locale: row.locale ?? '',
    };
  });

  const [title, setTitle] = useState<string>(
    () => initialDraft.restored?.patch.title ?? initialDraft.restored?.original.title ?? row.title,
  );
  const [slug, setSlug] = useState<string>(
    () => initialDraft.restored?.patch.slug ?? initialDraft.restored?.original.slug ?? row.slug,
  );
  const [status, setStatus] = useState<'draft' | 'published'>(
    () =>
      initialDraft.restored?.patch.publication_status ??
      initialDraft.restored?.original.publication_status ??
      row.publication_status,
  );
  const [siteTimezone, setSiteTimezone] = useState<string>(
    () => initialDraft.restored?.patch.timezone ?? initialDraft.restored?.original.timezone ?? '',
  );
  const [timezoneLoading, setTimezoneLoading] = useState<boolean>(!siteTimezone);
  const [timezoneError, setTimezoneError] = useState<string>('');
  const [publishedAtLocal, setPublishedAtLocal] = useState<string>(
    () =>
      initialDraft.restored?.patch.published_at_local ??
      initialDraft.restored?.original.published_at_local ??
      '',
  );
  const [taxonomyKey, setTaxonomyKey] = useState<string>(
    () => initialDraft.restored?.patch.taxonomy?.key ?? '',
  );
  const [taxonomyOp, setTaxonomyOp] = useState<TaxonomyDeltaOp>(
    () => initialDraft.restored?.patch.taxonomy?.operation ?? 'add',
  );
  const [selectedTermIds, setSelectedTermIds] = useState<number[]>(
    () => initialDraft.restored?.patch.taxonomy?.term_ids ?? [],
  );
  const [termCatalogByKind, setTermCatalogByKind] = useState<Record<string, TaxonomyTerm[]>>({});
  const [termCatalogLoading, setTermCatalogLoading] = useState<boolean>(false);
  const [termCatalogError, setTermCatalogError] = useState<string>('');

  const [saving, setSaving] = useState<boolean>(false);
  const [errorMessage, setErrorMessage] = useState<string>('');
  const [hookScopeConflict, setHookScopeConflict] = useState<boolean>(false);
  const [restoredNotice, setRestoredNotice] = useState<boolean>(
    () => Boolean(initialDraft.restored && initialDraft.wasQuarantined),
  );
  const [conflictState, setConflictState] = useState<{
    serverRow: EntryTableRow;
    serverSchemaToken: string;
    diffs: ConflictDiff[];
  } | null>(null);
  const [trashedBlocked, setTrashedBlocked] = useState<boolean>(false);

  // Focus title input on open for keyboard accessibility.
  useEffect(() => {
    titleInputRef.current?.focus();
  }, []);

  // Lazily fetch current site timezone once Quick Edit opens.
  useEffect(() => {
    const controller = new AbortController();
    setTimezoneLoading(true);
    setTimezoneError('');
    void getGeneralSettings(controller.signal)
      .then(general => {
        if (controller.signal.aborted) return;
        const tz = general.timezone;
        const computedLocal = formatUtcToSiteLocal(originalMeta.published_at, tz);
        setSiteTimezone(prev => prev || tz);
        setOriginalMeta(prev => {
          if (prev.timezone) return prev;
          return {
            ...prev,
            timezone: tz,
            published_at_local: computedLocal,
          };
        });
        setPublishedAtLocal(prev => {
          if (initialDraft.restored?.patch.published_at_local !== undefined) return prev;
          return prev || computedLocal;
        });
        setTimezoneLoading(false);
      })
      .catch(cause => {
        if (controller.signal.aborted) return;
        setTimezoneLoading(false);
        setTimezoneError(tStatic('quick_edit.timezone_error'));
        if (cause instanceof ApiClientError && cause.status === 401) {
          onApiError(cause, tStatic('quick_edit.timezone_error'));
        }
      });
    return () => controller.abort();
  }, [row.id, authGeneration]);

  // Lazily load terms for the chosen taxonomy filtered by this entry's exact locale.
  useEffect(() => {
    if (!taxonomyKey) {
      setTermCatalogError('');
      setTermCatalogLoading(false);
      return;
    }
    if (Object.prototype.hasOwnProperty.call(termCatalogByKind, taxonomyKey)) return;
    const controller = new AbortController();
    setTermCatalogLoading(true);
    setTermCatalogError('');
    void listAllTaxonomyTerms(taxonomyKey, controller.signal, row.locale ?? null)
      .then(terms => {
        if (controller.signal.aborted) return;
        setTermCatalogByKind(prev => ({ ...prev, [taxonomyKey]: terms }));
        setTermCatalogLoading(false);
      })
      .catch(cause => {
        if (controller.signal.aborted) return;
        setTermCatalogLoading(false);
        setTermCatalogError(cause instanceof Error ? cause.message : tStatic('api.taxonomy.load_terms'));
        if (cause instanceof ApiClientError && cause.status === 401) {
          onApiError(cause, tStatic('api.taxonomy.load_terms'));
        }
      });
    return () => controller.abort();
  }, [taxonomyKey, row.locale, authGeneration]);

  const currentDraftPatch = useMemo<QuickEditDraftPatch>(() => {
    const patch: QuickEditDraftPatch = {};
    if (title.trim() !== originalMeta.title) patch.title = title;
    if (slug.trim() !== originalMeta.slug) patch.slug = slug;
    if (supportsDraftPublish && status !== originalMeta.publication_status) {
      patch.publication_status = status;
    }
    if (
      originalMeta.timezone &&
      publishedAtLocal.trim() !== originalMeta.published_at_local
    ) {
      patch.published_at_local = publishedAtLocal;
      patch.timezone = siteTimezone || originalMeta.timezone;
    }
    if (taxonomyKey) {
      patch.taxonomy = {
        key: taxonomyKey,
        operation: taxonomyOp,
        term_ids: [...selectedTermIds],
      };
    }
    return patch;
  }, [
    title,
    slug,
    status,
    supportsDraftPublish,
    publishedAtLocal,
    siteTimezone,
    taxonomyKey,
    taxonomyOp,
    selectedTermIds,
    originalMeta,
  ]);

  const isDirty = isQuickEditPatchDirty(currentDraftPatch);

  useEffect(() => {
    onDirtyChange(isDirty);
    if (isDirty) {
      editorDraftStore.setQuickEdit({
        kind: 'quick_edit',
        draft_key: editorDraftStore.makeQuickEditKey(typeKey, row.id),
        site_instance_id: siteInstanceId,
        user_id: user.id,
        auth_generation: authGeneration,
        type_key: typeKey,
        entry_id: row.id,
        status: 'active',
        base_version: baseVersion,
        schema_token: activeSchemaToken,
        original: originalMeta,
        patch: currentDraftPatch,
      });
    } else {
      editorDraftStore.deleteQuickEdit(typeKey, row.id);
    }
  }, [
    isDirty,
    currentDraftPatch,
    typeKey,
    row.id,
    siteInstanceId,
    user.id,
    authGeneration,
    baseVersion,
    activeSchemaToken,
    originalMeta,
    onDirtyChange,
  ]);

  const canEditPublishedAt =
    Boolean(siteTimezone) &&
    !timezoneError &&
    (status === 'published' || Boolean(originalMeta.published_at));

  const buildPayload = (): MetadataPatch | null => {
    const payload: MetadataPatch = {
      expected_schema_token: activeSchemaToken,
    };
    let hasField = false;
    if (title.trim() !== originalMeta.title) {
      payload.title = title.trim();
      hasField = true;
    }
    if (slug.trim() !== originalMeta.slug) {
      payload.slug = slug.trim();
      hasField = true;
    }
    if (supportsDraftPublish && status !== originalMeta.publication_status) {
      payload.publication_status = status;
      hasField = true;
    }
    if (
      originalMeta.timezone &&
      publishedAtLocal.trim() !== '' &&
      publishedAtLocal.trim() !== originalMeta.published_at_local
    ) {
      payload.published_at = {
        local_datetime: publishedAtLocal.trim(),
        timezone: siteTimezone || originalMeta.timezone,
      };
      hasField = true;
    }
    if (taxonomyKey) {
      payload.taxonomy = {
        key: taxonomyKey,
        operation: taxonomyOp,
        term_ids: [...selectedTermIds],
      };
      hasField = true;
    }
    return hasField ? payload : null;
  };

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (saving || !csrfToken || trashedBlocked) return;
    setErrorMessage('');
    setHookScopeConflict(false);
    setRestoredNotice(false);

    const payload = buildPayload();
    if (!payload) {
      setErrorMessage(tStatic('quick_edit.no_changes'));
      return;
    }
    if (payload.taxonomy && payload.taxonomy.operation === 'replace') {
      const defLabel = taxonomies.find(t => t.key === payload.taxonomy?.key)?.label ?? payload.taxonomy.key;
      const prompt =
        payload.taxonomy.term_ids.length === 0
          ? tStatic('quick_edit.confirm_clear_terms', { value1: defLabel, value2: originalMeta.title })
          : tStatic('quick_edit.confirm_replace_terms', {
              value1: defLabel,
              value2: originalMeta.title,
              value3: formatNumber(payload.taxonomy.term_ids.length),
            });
      if (!window.confirm(prompt)) return;
    }

    setSaving(true);
    try {
      const updated = await patchEntryMetadata(
        typeKey,
        row.id,
        baseVersion,
        payload,
        csrfToken,
      );
      editorDraftStore.deleteQuickEdit(typeKey, row.id);
      onDirtyChange(false);
      onSaved(updated);
    } catch (cause) {
      if (cause instanceof ApiClientError && cause.status === 401) {
        // Quick Edit draft stays quarantined in `quick_edit:` namespace via App's onApiError.
        onApiError(cause, tStatic('api.content.patch_entry_metadata'));
        return;
      }
      if (cause instanceof ApiClientError && cause.status === 412) {
        setErrorMessage(cause.message);
        const refreshed = await onRefreshServerRow();
        if (!refreshed.row) {
          setTrashedBlocked(true);
          return;
        }
        const sr = refreshed.row;
        const diffs: ConflictDiff[] = [];
        if (sr.title !== originalMeta.title) {
          diffs.push({ field: tStatic('quick_edit.title_label'), server: sr.title, yours: title });
        }
        if (sr.slug !== originalMeta.slug) {
          diffs.push({ field: tStatic('quick_edit.slug_label'), server: sr.slug, yours: slug });
        }
        if (sr.publication_status !== originalMeta.publication_status) {
          diffs.push({
            field: tStatic('quick_edit.status_label'),
            server: sr.publication_status,
            yours: status,
          });
        }
        const serverLocal = siteTimezone
          ? formatUtcToSiteLocal(sr.published_at, siteTimezone)
          : (sr.published_at ?? '');
        if ((sr.published_at ?? null) !== originalMeta.published_at) {
          diffs.push({
            field: tStatic('quick_edit.published_at_label', { value1: siteTimezone || 'UTC' }),
            server: serverLocal || '—',
            yours: publishedAtLocal || '—',
          });
        }
        if (JSON.stringify(sr.term_ids) !== JSON.stringify(originalMeta.term_ids)) {
          diffs.push({
            field: tStatic('quick_edit.taxonomy_select_label'),
            server: sr.term_ids.join(', ') || '—',
            yours: selectedTermIds.join(', ') || '—',
          });
        }
        setConflictState({
          serverRow: sr,
          serverSchemaToken: refreshed.schemaToken || activeSchemaToken,
          diffs,
        });
        return;
      }
      if (isTrashedError(cause)) {
        setTrashedBlocked(true);
        setErrorMessage(cause instanceof Error ? cause.message : '');
        return;
      }
      if (isHookScopeError(cause)) {
        setHookScopeConflict(true);
      }
      setErrorMessage(
        cause instanceof Error ? cause.message : tStatic('api.content.patch_entry_metadata'),
      );
    } finally {
      setSaving(false);
    }
  };

  const handleExplicitRebase = () => {
    if (!conflictState) return;
    const sr = conflictState.serverRow;
    const tz = siteTimezone || originalMeta.timezone;
    const serverLocal = tz ? formatUtcToSiteLocal(sr.published_at, tz) : '';
    const nextOriginal: QuickEditOriginalMetadata = {
      title: sr.title,
      slug: sr.slug,
      publication_status: sr.publication_status,
      published_at: sr.published_at ?? null,
      published_at_local: serverLocal,
      timezone: tz,
      term_ids: [...sr.term_ids],
      locale: sr.locale ?? originalMeta.locale,
    };
    // Preserve user's dirty inputs while adopting fresh untouched server fields + version/schema.
    if (title.trim() === originalMeta.title) setTitle(sr.title);
    if (slug.trim() === originalMeta.slug) setSlug(sr.slug);
    if (status === originalMeta.publication_status) setStatus(sr.publication_status);
    if (publishedAtLocal.trim() === originalMeta.published_at_local) setPublishedAtLocal(serverLocal);
    setOriginalMeta(nextOriginal);
    setBaseVersion(sr.version);
    setActiveSchemaToken(conflictState.serverSchemaToken);
    setConflictState(null);
    setErrorMessage('');
  };

  const handleCancelClick = () => {
    if (isDirty && !window.confirm(tStatic('quick_edit.confirm_discard'))) return;
    editorDraftStore.deleteQuickEdit(typeKey, row.id);
    onDirtyChange(false);
    onCancel();
  };

  const availableTerms = taxonomyKey ? (termCatalogByKind[taxonomyKey] ?? []) : [];
  const recoverableSummary = JSON.stringify(
    {
      title,
      slug,
      publication_status: supportsDraftPublish ? status : undefined,
      published_at_local: publishedAtLocal || undefined,
      timezone: siteTimezone || undefined,
      taxonomy: taxonomyKey
        ? { key: taxonomyKey, operation: taxonomyOp, term_ids: selectedTermIds }
        : undefined,
    },
    null,
    2,
  );

  return (
    <form
      onSubmit={handleSubmit}
      data-testid={`quick-edit-form-${row.id}`}
      aria-label={tStatic('quick_edit.heading', { value1: row.title, value2: row.id })}
      className="space-y-3 rounded-lg border border-od-accent/30 bg-zinc-50/90 p-3.5 text-sm"
    >
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-zinc-200 pb-2">
        <div className="font-semibold text-zinc-900">
          {tStatic('quick_edit.heading', { value1: row.title, value2: row.id })}
        </div>
        <span className="text-xs text-zinc-500">
          {tStatic('quick_edit.updated_at_note', { value1: row.updated_at })}
        </span>
      </div>

      {restoredNotice && (
        <AdminAlert tone="info" data-testid="quick-edit-restored-alert">
          {tStatic('quick_edit.restored_after_relogin')}
        </AdminAlert>
      )}

      {trashedBlocked && (
        <div
          role="alert"
          data-testid="quick-edit-trashed-guard"
          className="space-y-2 rounded-lg border border-amber-300 bg-amber-50 p-3 text-xs text-amber-950"
        >
          <p className="font-semibold">{tStatic('quick_edit.trashed_title')}</p>
          <p>{tStatic('quick_edit.trashed_hint')}</p>
          <label className="block space-y-1">
            <span className="font-medium">{tStatic('quick_edit.recoverable_draft_label')}</span>
            <textarea
              readOnly
              rows={5}
              data-testid="quick-edit-recoverable-text"
              value={recoverableSummary}
              className="w-full rounded border border-amber-300 bg-white p-2 font-mono text-xs text-zinc-900"
            />
          </label>
          <div className="flex flex-wrap gap-2">
            <AdminButton
              type="button"
              size="sm"
              variant="primary"
              onClick={() => onNavigateHash(trashHref)}
            >
              {tStatic('entry_lifecycle.open_trash_view')}
            </AdminButton>
            <AdminButton type="button" size="sm" variant="secondary" onClick={handleCancelClick}>
              {tStatic('quick_edit.cancel')}
            </AdminButton>
          </div>
        </div>
      )}

      {conflictState && !trashedBlocked && (
        <div
          role="alert"
          data-testid="quick-edit-conflict-412"
          className="space-y-2 rounded-lg border border-amber-300 bg-amber-50 p-3 text-xs text-amber-950"
        >
          <p className="font-semibold">{tStatic('quick_edit.conflict_412_title')}</p>
          <p>{tStatic('quick_edit.conflict_412_hint')}</p>
          {conflictState.diffs.length > 0 && (
            <ul className="list-disc space-y-0.5 pl-4" data-testid="quick-edit-conflict-diffs">
              {conflictState.diffs.map(diff => (
                <li key={diff.field}>
                  {tStatic('quick_edit.conflict_diff_item', {
                    field: diff.field,
                    server: diff.server,
                    yours: diff.yours,
                  })}
                </li>
              ))}
            </ul>
          )}
          <div className="flex flex-wrap gap-2">
            <AdminButton
              type="button"
              size="sm"
              variant="primary"
              data-testid="quick-edit-rebase-btn"
              onClick={handleExplicitRebase}
            >
              {tStatic('quick_edit.rebase_button')}
            </AdminButton>
          </div>
        </div>
      )}

      {errorMessage && !conflictState && !trashedBlocked && (
        <AdminAlert tone="error" role="alert" data-testid="quick-edit-error">
          <div className="space-y-1.5">
            <p>{errorMessage}</p>
            {hookScopeConflict && (
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-xs">{tStatic('quick_edit.hook_scope_conflict_hint')}</span>
                <AdminButton
                  type="button"
                  size="sm"
                  variant="secondary"
                  onClick={() => onNavigateHash(editHref)}
                >
                  {tStatic('quick_edit.open_single_edit')}
                </AdminButton>
              </div>
            )}
          </div>
        </AdminAlert>
      )}

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
          <span>{tStatic('quick_edit.title_label')}</span>
          <input
            ref={titleInputRef}
            type="text"
            data-testid="quick-edit-title"
            className={inputClass}
            value={title}
            maxLength={200}
            disabled={saving || trashedBlocked}
            onChange={e => setTitle(e.target.value)}
          />
        </label>

        <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
          <span>{tStatic('quick_edit.slug_label')}</span>
          <input
            type="text"
            data-testid="quick-edit-slug"
            className={`${inputClass} font-mono`}
            value={slug}
            maxLength={64}
            disabled={saving || trashedBlocked}
            onChange={e => setSlug(e.target.value)}
          />
        </label>

        {supportsDraftPublish && (
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{tStatic('quick_edit.status_label')}</span>
            <select
              data-testid="quick-edit-status"
              className={inputClass}
              value={status}
              disabled={saving || trashedBlocked}
              onChange={e => setStatus(e.target.value as 'draft' | 'published')}
            >
              <option value="draft">{tStatic('feature.content.entrylistpage.draft')}</option>
              <option value="published">{tStatic('reading.published')}</option>
            </select>
          </label>
        )}

        <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
          <span>
            {tStatic('quick_edit.published_at_label', {
              value1: siteTimezone || 'UTC',
            })}
          </span>
          <input
            type="text"
            data-testid="quick-edit-published-at"
            className={`${inputClass} font-mono`}
            placeholder="YYYY-MM-DDTHH:mm:ss"
            value={publishedAtLocal}
            disabled={saving || trashedBlocked || timezoneLoading || !canEditPublishedAt}
            onChange={e => setPublishedAtLocal(e.target.value)}
          />
          {timezoneLoading ? (
            <span className="text-[11px] font-normal text-zinc-500">
              {tStatic('quick_edit.timezone_loading')}
            </span>
          ) : timezoneError ? (
            <span className="text-[11px] font-normal text-rose-600">{timezoneError}</span>
          ) : !canEditPublishedAt ? (
            <span className="text-[11px] font-normal text-zinc-500">
              {tStatic('quick_edit.published_at_draft_hint')}
            </span>
          ) : (
            <span className="text-[11px] font-normal text-zinc-500">
              {tStatic('quick_edit.published_at_format_hint', { value1: siteTimezone })}
            </span>
          )}
        </label>
      </div>

      {taxonomies.length > 0 && (
        <fieldset className="space-y-2 rounded-md border border-zinc-200 bg-white p-3">
          <legend className="px-1 text-xs font-semibold text-zinc-700">
            {tStatic('quick_edit.taxonomy_section')}
          </legend>
          <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
            <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>{tStatic('quick_edit.taxonomy_select_label')}</span>
              <select
                data-testid="quick-edit-taxonomy-key"
                className={inputClass}
                value={taxonomyKey}
                disabled={saving || trashedBlocked}
                onChange={e => {
                  setTaxonomyKey(e.target.value);
                  setSelectedTermIds([]);
                }}
              >
                <option value="">{tStatic('quick_edit.taxonomy_none')}</option>
                {taxonomies.map(def => (
                  <option key={def.key} value={def.key}>
                    {def.label}
                  </option>
                ))}
              </select>
            </label>

            {taxonomyKey && (
              <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
                <span>{tStatic('quick_edit.taxonomy_op_label')}</span>
                <select
                  data-testid="quick-edit-taxonomy-op"
                  className={inputClass}
                  value={taxonomyOp}
                  disabled={saving || trashedBlocked}
                  onChange={e => setTaxonomyOp(e.target.value as TaxonomyDeltaOp)}
                >
                  <option value="add">{tStatic('quick_edit.taxonomy_op_add')}</option>
                  <option value="remove">{tStatic('quick_edit.taxonomy_op_remove')}</option>
                  <option value="replace">{tStatic('quick_edit.taxonomy_op_replace')}</option>
                </select>
              </label>
            )}
          </div>

          {taxonomyKey && (
            <div className="space-y-1.5">
              <p className="text-xs font-medium text-zinc-600">
                {tStatic('quick_edit.taxonomy_terms_label', { value1: row.locale ?? 'default' })}
              </p>
              {termCatalogLoading ? (
                <p className="text-xs text-zinc-500">{tStatic('quick_edit.taxonomy_loading')}</p>
              ) : termCatalogError ? (
                <p className="text-xs text-rose-600">{termCatalogError}</p>
              ) : availableTerms.length === 0 ? (
                <p className="text-xs text-zinc-500">
                  {tStatic('quick_edit.taxonomy_empty_catalog', { value1: row.locale ?? 'default' })}
                </p>
              ) : (
                <div
                  className="flex max-h-32 flex-wrap gap-2 overflow-y-auto rounded border border-zinc-200 p-2"
                  data-testid="quick-edit-taxonomy-terms"
                >
                  {availableTerms.map(term => {
                    const checked = selectedTermIds.includes(term.id);
                    return (
                      <label
                        key={term.id}
                        className="inline-flex items-center gap-1.5 rounded border border-zinc-200 bg-zinc-50 px-2 py-1 text-xs text-zinc-800"
                      >
                        <input
                          type="checkbox"
                          data-testid={`quick-edit-term-${term.id}`}
                          checked={checked}
                          disabled={saving || trashedBlocked}
                          onChange={e => {
                            setSelectedTermIds(prev =>
                              e.target.checked
                                ? [...prev, term.id]
                                : prev.filter(id => id !== term.id),
                            );
                          }}
                        />
                        <span>{term.name}</span>
                      </label>
                    );
                  })}
                </div>
              )}
              {taxonomyOp === 'replace' && (
                <p className="text-[11px] text-zinc-500">
                  {tStatic('quick_edit.taxonomy_clear_hint')}
                </p>
              )}
            </div>
          )}
        </fieldset>
      )}

      <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
        <div className="flex flex-wrap items-center gap-2">
          <AdminButton
            type="submit"
            variant="primary"
            size="sm"
            data-testid="quick-edit-save-btn"
            disabled={saving || !csrfToken || trashedBlocked || Boolean(conflictState)}
          >
            {saving ? tStatic('quick_edit.saving') : tStatic('quick_edit.save')}
          </AdminButton>
          <AdminButton
            type="button"
            variant="secondary"
            size="sm"
            data-testid="quick-edit-cancel-btn"
            disabled={saving}
            onClick={handleCancelClick}
          >
            {tStatic('quick_edit.cancel')}
          </AdminButton>
        </div>
        <button
          type="button"
          className="text-xs font-medium text-od-accent hover:underline"
          onClick={() => onNavigateHash(editHref)}
        >
          {tStatic('quick_edit.open_single_edit')}
        </button>
      </div>
    </form>
  );
}
