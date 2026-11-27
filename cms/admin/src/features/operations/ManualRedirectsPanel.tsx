import React from 'react';
import type { AuthUser } from '../../types.ts';
import { useI18n } from '../../i18n/index.tsx';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import {
  createManualRedirect,
  deleteManualRedirect,
  exportRedirectsCsvBlob,
  listManualRedirects,
  updateManualRedirect,
  type ConflictPolicy,
  type CsvImportResponse,
  type ManualRedirectItem,
  type RedirectListPage,
} from '../../api/redirects.ts';
import { ApiClientError } from '../../api/client.ts';
import { CsvImportModal } from './CsvImportModal.tsx';
import { RedirectFormModal } from './RedirectFormModal.tsx';
import {
  discardRedirectsDraft,
  getRedirectsDraft,
  setRedirectsDraft,
  type RedirectsDraftRecord,
} from './redirectsDraftStore.ts';

export interface ManualRedirectsPanelProps {
  user: AuthUser | null;
  siteInstanceId: string;
  csrfToken: string | null;
  refreshRevision: number;
  initialTab: 'redirects' | 'not-found';
  onResolved404: () => void;
  onDirtyChange: (dirty: boolean) => void;
  onApiError: (err: unknown) => void;
}

export function ManualRedirectsPanel({
  user,
  siteInstanceId,
  csrfToken,
  refreshRevision,
  initialTab,
  onResolved404,
  onDirtyChange,
  onApiError,
}: ManualRedirectsPanelProps) {
  const { __, _x } = useI18n();
  const [data, setData] = React.useState<RedirectListPage | null>(null);
  const [page, setPage] = React.useState(1);
  const [q, setQ] = React.useState('');
  const [statusFilter, setStatusFilter] = React.useState<'301' | '302' | ''>('');
  const [stateFilter, setStateFilter] = React.useState<'' | 'enabled' | 'disabled' | 'shadowed'>('');
  const [notice, setNotice] = React.useState<string | null>(null);
  const [formError, setFormError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);

  const restored = user ? getRedirectsDraft(siteInstanceId, user.id) : undefined;
  const restoredForManual =
    restored && restored.modalMode !== 'from_404' ? restored : undefined;
  const [draft, setDraft] = React.useState<RedirectsDraftRecord | null>(
    restoredForManual ?? null,
  );

  React.useEffect(() => {
    if (!user) return;
    const saved = getRedirectsDraft(siteInstanceId, user.id);
    if (saved && saved.modalMode !== 'from_404') {
      setDraft(saved);
    } else if (!saved) {
      setDraft(null);
    }
  }, [initialTab, siteInstanceId, user]);

  React.useEffect(() => {
    onDirtyChange(Boolean(draft?.dirty));
  }, [draft?.dirty, onDirtyChange]);

  const persistDraft = React.useCallback(
    (next: RedirectsDraftRecord | null) => {
      setDraft(next);
      if (!user) return;
      if (next && next.dirty) {
        setRedirectsDraft(siteInstanceId, user.id, next);
      } else {
        discardRedirectsDraft(siteInstanceId, user.id);
      }
    },
    [siteInstanceId, user],
  );

  const onApiErrorRef = React.useRef(onApiError);
  onApiErrorRef.current = onApiError;

  const loadList = React.useCallback(async () => {
    try {
      const res = await listManualRedirects({
        page,
        per_page: 50,
        q,
        status: statusFilter,
        enabled: stateFilter === 'enabled' ? 'true' : stateFilter === 'disabled' ? 'false' : '',
        shadowed: stateFilter === 'shadowed' ? 'true' : '',
      });
      setData(res);
    } catch (err) {
      onApiErrorRef.current(err);
    }
  }, [page, q, stateFilter, statusFilter]);

  React.useEffect(() => {
    if (initialTab === 'redirects') {
      void loadList();
    }
  }, [initialTab, loadList, refreshRevision]);

  const openCreate = () => {
    setFormError(null);
    persistDraft({
      modalMode: 'create',
      editingId: null,
      baselineVersion: null,
      sourcePath: '',
      targetUrl: '',
      statusCode: 301,
      enabled: true,
      note: '',
      resolveNotFound: true,
      csvText: '',
      conflictPolicy: 'skip',
      staleConflict: false,
      dirty: false,
    });
  };

  const openEdit = (item: ManualRedirectItem) => {
    setFormError(null);
    persistDraft({
      modalMode: 'edit',
      editingId: item.id,
      baselineVersion: item.version,
      sourcePath: item.source_path,
      targetUrl: item.target_url,
      statusCode: item.status_code,
      enabled: item.enabled,
      note: item.note,
      resolveNotFound: false,
      csvText: '',
      conflictPolicy: 'skip',
      staleConflict: false,
      dirty: false,
    });
  };

  const openCsvModal = () => {
    setFormError(null);
    persistDraft({
      modalMode: 'csv_import',
      editingId: null,
      baselineVersion: null,
      sourcePath: '',
      targetUrl: '',
      statusCode: 301,
      enabled: true,
      note: '',
      resolveNotFound: false,
      csvText: '',
      conflictPolicy: 'skip',
      staleConflict: false,
      dirty: false,
    });
  };

  const handleSaveRule = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!csrfToken || !draft || draft.staleConflict) return;
    setBusy(true);
    setFormError(null);
    try {
      if (draft.modalMode === 'edit' && draft.editingId !== null && draft.baselineVersion !== null) {
        const saved = await updateManualRedirect(
          draft.editingId,
          draft.baselineVersion,
          {
            source_path: draft.sourcePath,
            target_url: draft.targetUrl,
            status_code: draft.statusCode,
            enabled: draft.enabled,
            note: draft.note,
          },
          csrfToken,
        );
        persistDraft(null);
        setNotice(__("Redirect '{{source}}' updated.", { source: saved.source_path }));
      } else {
        const created = await createManualRedirect(
          {
            source_path: draft.sourcePath,
            target_url: draft.targetUrl,
            status_code: draft.statusCode,
            enabled: draft.enabled,
            note: draft.note,
            resolve_not_found: draft.resolveNotFound,
          },
          csrfToken,
        );
        persistDraft(null);
        setNotice(__("Redirect '{{source}}' created.", { source: created.source_path }));
        if (draft.resolveNotFound) {
          onResolved404();
        }
      }
      await loadList();
    } catch (err) {
      if (err instanceof ApiClientError && err.status === 412 && draft) {
        persistDraft({ ...draft, staleConflict: true, dirty: true });
      }
      setFormError(err instanceof Error ? err.message : __('Could not save manual redirect.'));
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleToggleEnabled = async (item: ManualRedirectItem) => {
    if (!csrfToken) return;
    setBusy(true);
    try {
      const updated = await updateManualRedirect(
        item.id,
        item.version,
        {
          source_path: item.source_path,
          target_url: item.target_url,
          status_code: item.status_code,
          enabled: !item.enabled,
          note: item.note,
        },
        csrfToken,
      );
      setNotice(__("Redirect '{{source}}' updated.", { source: updated.source_path }));
      await loadList();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async (item: ManualRedirectItem) => {
    if (!csrfToken) return;
    if (!window.confirm(__("Delete manual redirect '{{source}}'?", { source: item.source_path }))) return;
    setBusy(true);
    try {
      await deleteManualRedirect(item.id, csrfToken);
      setNotice(__('Redirect deleted.'));
      await loadList();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleExport = async () => {
    try {
      const blob = await exportRedirectsCsvBlob();
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = 'opendash-redirects.csv';
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(url);
      setNotice(__('CSV exported.'));
    } catch (err) {
      onApiError(err);
    }
  };

  const totalPages = data ? Math.max(1, Math.ceil(data.total / data.per_page)) : 1;

  return (
    <div className="space-y-4 pt-4" data-testid="manual-redirects-panel">
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {data && data.shadowed_count > 0 && (
        <AdminAlert tone="warning" data-testid="redirects-shadowed-banner">
          {__('{{count}} active redirect(s) are currently shadowed by live published routes. Live content always takes priority.', { count: data.shadowed_count })}
        </AdminAlert>
      )}

      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <input
            type="search"
            value={q}
            onChange={(e) => {
              setPage(1);
              setQ(e.target.value);
            }}
            placeholder={__('Search source path, target URL, or note…')}
            data-testid="redirects-search-input"
            className="w-64 rounded-md border border-zinc-300 px-3 py-1.5 text-sm text-zinc-900 focus:border-od-accent focus:outline-none"
          />
          <select
            value={statusFilter}
            onChange={(e) => {
              setPage(1);
              setStatusFilter(e.target.value as '301' | '302' | '');
            }}
            data-testid="redirects-status-filter"
            className="rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-sm text-zinc-900"
          >
            <option value="">{__('All status codes')}</option>
            <option value="301">301</option>
            <option value="302">302</option>
          </select>
          <select
            value={stateFilter}
            onChange={(e) => {
              setPage(1);
              setStateFilter(e.target.value as '' | 'enabled' | 'disabled' | 'shadowed');
            }}
            data-testid="redirects-state-filter"
            className="rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-sm text-zinc-900"
          >
            <option value="">{__('All states')}</option>
            <option value="enabled">{__('Enabled')}</option>
            <option value="disabled">{_x('Disabled', 'redirects')}</option>
            <option value="shadowed">{__('Shadowed by live page')}</option>
          </select>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <AdminButton variant="secondary" size="sm" onClick={handleExport} data-testid="redirect-export-btn">
            {__('Export CSV')}
          </AdminButton>
          <AdminButton variant="secondary" size="sm" onClick={openCsvModal} data-testid="redirect-import-btn">
            {__('Import CSV')}
          </AdminButton>
          <AdminButton variant="primary" size="sm" onClick={openCreate} data-testid="redirect-add-btn">
            {__('+ Add Redirect')}
          </AdminButton>
        </div>
      </div>

      <div className="overflow-x-auto rounded-lg border border-zinc-200 bg-white">
        <table className="w-full border-collapse text-left text-sm">
          <thead className="border-b border-zinc-200 bg-zinc-50 text-xs font-medium text-zinc-600">
            <tr>
              <th className="px-3 py-2.5">{__('Source Path')}</th>
              <th className="px-3 py-2.5">{__('Target URL')}</th>
              <th className="px-3 py-2.5">{__('Code')}</th>
              <th className="px-3 py-2.5">{__('Status')}</th>
              <th className="px-3 py-2.5">{__('Hits')}</th>
              <th className="px-3 py-2.5">{__('Last Hit')}</th>
              <th className="px-3 py-2.5 text-right">{__('Actions')}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-zinc-100">
            {!data || data.items.length === 0 ? (
              <tr>
                <td colSpan={7} className="px-3 py-6 text-center text-sm text-zinc-500">
                  {__('No manual redirects found.')}
                </td>
              </tr>
            ) : (
              data.items.map((item) => (
                <tr key={item.id} className="hover:bg-zinc-50/70">
                  <td className="px-3 py-2.5 font-mono text-xs text-zinc-900">
                    <div>{item.source_path}</div>
                    {item.shadowed_by_live_route && (
                      <AdminBadge tone="warning" className="mt-1">
                        {__('Shadowed by live route')}
                      </AdminBadge>
                    )}
                    {item.note && <div className="mt-0.5 font-sans text-zinc-500">{item.note}</div>}
                  </td>
                  <td className="max-w-xs truncate px-3 py-2.5 font-mono text-xs text-zinc-700">
                    {item.target_url}
                  </td>
                  <td className="px-3 py-2.5">
                    <AdminBadge tone={item.status_code === 301 ? 'neutral' : 'warning'}>
                      {item.status_code}
                    </AdminBadge>
                  </td>
                  <td className="px-3 py-2.5">
                    <AdminBadge tone={item.enabled ? 'success' : 'neutral'}>
                      {item.enabled ? _x('Active', 'redirects') : _x('Disabled', 'redirects')}
                    </AdminBadge>
                  </td>
                  <td className="px-3 py-2.5 text-xs text-zinc-700">{item.hit_count}</td>
                  <td className="px-3 py-2.5 text-xs text-zinc-500">{item.last_hit_at ?? '—'}</td>
                  <td className="px-3 py-2.5 text-right">
                    <div className="inline-flex items-center gap-1.5">
                      <AdminButton
                        variant="ghost"
                        size="sm"
                        disabled={busy}
                        onClick={() => {
                          void handleToggleEnabled(item);
                        }}
                      >
                        {item.enabled ? __('Disable') : _x('Enable', 'redirects')}
                      </AdminButton>
                      <AdminButton variant="secondary" size="sm" onClick={() => openEdit(item)}>
                        {_x('Edit', 'redirects')}
                      </AdminButton>
                      <AdminButton
                        variant="danger"
                        size="sm"
                        disabled={busy}
                        onClick={() => {
                          void handleDelete(item);
                        }}
                      >
                        {__('Delete')}
                      </AdminButton>
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {data && data.total > 0 && (
        <div className="flex items-center justify-between text-xs text-zinc-600">
          <span>
            {__('Page {{page}} / {{pages}} ({{total}} items)', { page: data.page, pages: totalPages, total: data.total })}
          </span>
          <div className="flex gap-1.5">
            <AdminButton
              variant="secondary"
              size="sm"
              disabled={page <= 1}
              onClick={() => setPage((p) => Math.max(1, p - 1))}
            >
              {_x('Previous', 'redirects')}
            </AdminButton>
            <AdminButton
              variant="secondary"
              size="sm"
              disabled={page >= totalPages}
              onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
            >
              {_x('Next', 'redirects')}
            </AdminButton>
          </div>
        </div>
      )}

      {draft && draft.modalMode === 'csv_import' && (
        <CsvImportModal
          csrfToken={csrfToken}
          csvText={draft.csvText}
          conflictPolicy={draft.conflictPolicy}
          onChange={(patch: { csvText?: string; conflictPolicy?: ConflictPolicy }) => {
            persistDraft({
              ...draft,
              csvText: patch.csvText ?? draft.csvText,
              conflictPolicy: patch.conflictPolicy ?? draft.conflictPolicy,
              dirty: Boolean((patch.csvText ?? draft.csvText).trim()),
            });
          }}
          onCommitted={(res: CsvImportResponse) => {
            persistDraft(null);
            setNotice(
              __('CSV import complete: {{create}} created, {{update}} updated, {{skip}} skipped.', {
                create: res.create_count,
                update: res.update_count,
                skip: res.skip_count,
              }),
            );
            onResolved404();
            void loadList();
          }}
          onApiError={onApiError}
          onClose={() => persistDraft(null)}
        />
      )}

      {draft && (draft.modalMode === 'create' || draft.modalMode === 'edit') && (
        <RedirectFormModal
          mode={draft.modalMode}
          editingId={draft.editingId}
          sourcePath={draft.sourcePath}
          targetUrl={draft.targetUrl}
          statusCode={draft.statusCode}
          enabled={draft.enabled}
          note={draft.note}
          resolveNotFound={draft.resolveNotFound}
          staleConflict={draft.staleConflict}
          busy={busy}
          formError={formError}
          onChange={(patch) => {
            persistDraft({
              ...draft,
              sourcePath: patch.sourcePath ?? draft.sourcePath,
              targetUrl: patch.targetUrl ?? draft.targetUrl,
              statusCode: patch.statusCode ?? draft.statusCode,
              enabled: patch.enabled ?? draft.enabled,
              note: patch.note ?? draft.note,
              resolveNotFound: patch.resolveNotFound ?? draft.resolveNotFound,
              dirty: true,
            });
          }}
          onRebaseLatest={(latest) => {
            setFormError(null);
            persistDraft({
              ...draft,
              editingId: latest.id,
              baselineVersion: latest.version,
              sourcePath: latest.source_path,
              targetUrl: latest.target_url,
              statusCode: latest.status_code,
              enabled: latest.enabled,
              note: latest.note,
              staleConflict: false,
              dirty: false,
            });
          }}
          onKeepDraft={() => {
            persistDraft({ ...draft, staleConflict: true, dirty: true });
          }}
          onError={(err) => {
            setFormError(err instanceof Error ? err.message : __('Could not load the latest redirect rule.'));
            onApiError(err);
          }}
          onSubmit={handleSaveRule}
          onClose={() => {
            setFormError(null);
            persistDraft(null);
          }}
        />
      )}
    </div>
  );
}
