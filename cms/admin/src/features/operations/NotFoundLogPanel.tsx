import React from 'react';
import type { AuthUser } from '../../types.ts';
import { useI18n } from '../../i18n/index.tsx';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import {
  bulkDeleteNotFoundLogs,
  clearNotFoundLogs,
  createManualRedirect,
  deleteNotFoundLog,
  listNotFoundLogs,
  type NotFoundLogItem,
  type NotFoundLogPage,
  type NotFoundSort,
} from '../../api/redirects.ts';
import { RedirectFormModal } from './RedirectFormModal.tsx';
import {
  discardRedirectsDraft,
  getRedirectsDraft,
  setRedirectsDraft,
  type RedirectsDraftRecord,
} from './redirectsDraftStore.ts';

export interface NotFoundLogPanelProps {
  user: AuthUser | null;
  siteInstanceId: string;
  csrfToken: string | null;
  refreshRevision: number;
  initialTab: 'redirects' | 'not-found';
  onCreatedRedirect: () => void;
  onNavigateHash: (hash: string) => void;
  onDirtyChange: (dirty: boolean) => void;
  onApiError: (err: unknown) => void;
}

export function NotFoundLogPanel({
  user,
  siteInstanceId,
  csrfToken,
  refreshRevision,
  initialTab,
  onCreatedRedirect,
  onNavigateHash,
  onDirtyChange,
  onApiError,
}: NotFoundLogPanelProps) {
  const { __, _x } = useI18n();
  const [data, setData] = React.useState<NotFoundLogPage | null>(null);
  const [page, setPage] = React.useState(1);
  const [q, setQ] = React.useState('');
  const [sort, setSort] = React.useState<NotFoundSort>('hits_desc');
  const [selectedIds, setSelectedIds] = React.useState<number[]>([]);
  const [clearOpen, setClearOpen] = React.useState(false);
  const clearMenuRef = React.useRef<HTMLDivElement | null>(null);
  const [notice, setNotice] = React.useState<{ text: string; linkToRedirects?: boolean } | null>(null);
  const [formError, setFormError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);

  React.useEffect(() => {
    if (!clearOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        setClearOpen(false);
      }
    };
    const onMouseDown = (event: MouseEvent) => {
      if (clearMenuRef.current && !clearMenuRef.current.contains(event.target as Node)) {
        setClearOpen(false);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('mousedown', onMouseDown);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('mousedown', onMouseDown);
    };
  }, [clearOpen]);

  const restored = user ? getRedirectsDraft(siteInstanceId, user.id) : undefined;
  const restoredFor404 =
    restored && restored.modalMode === 'from_404' ? restored : undefined;
  const [draft, setDraft] = React.useState<RedirectsDraftRecord | null>(restoredFor404 ?? null);

  React.useEffect(() => {
    if (!user) return;
    const saved = getRedirectsDraft(siteInstanceId, user.id);
    if (saved && saved.modalMode === 'from_404') {
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

  const loadLogs = React.useCallback(async () => {
    try {
      const res = await listNotFoundLogs({ page, per_page: 50, q, sort });
      setData(res);
      setSelectedIds([]);
    } catch (err) {
      onApiErrorRef.current(err);
    }
  }, [page, q, sort]);

  React.useEffect(() => {
    if (initialTab === 'not-found') {
      void loadLogs();
    }
  }, [initialTab, loadLogs, refreshRevision]);

  const handleOpenCreateFrom404 = (item: NotFoundLogItem) => {
    setClearOpen(false);
    setFormError(null);
    persistDraft({
      modalMode: 'from_404',
      editingId: null,
      baselineVersion: null,
      sourcePath: item.path,
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

  const handleSubmitFrom404 = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!csrfToken || !draft) return;
    setBusy(true);
    setFormError(null);
    try {
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
      setNotice({
        text: __("Redirect created from '{{source}}' and removed from 404 log.", { source: created.source_path }),
        linkToRedirects: true,
      });
      onCreatedRedirect();
      await loadLogs();
    } catch (err) {
      setFormError(err instanceof Error ? err.message : __('Could not save manual redirect.'));
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteOne = async (item: NotFoundLogItem) => {
    if (!csrfToken) return;
    if (!window.confirm(__("Delete 404 log entry '{{path}}'?", { path: item.path }))) return;
    setBusy(true);
    try {
      await deleteNotFoundLog(item.id, csrfToken);
      setNotice({ text: __('404 log entry deleted.') });
      await loadLogs();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleBulkDelete = async () => {
    if (!csrfToken || selectedIds.length === 0) return;
    if (!window.confirm(__('Delete {{count}} selected 404 log entry(s)?', { count: selectedIds.length }))) return;
    setBusy(true);
    try {
      const res = await bulkDeleteNotFoundLogs(selectedIds, csrfToken);
      setNotice({ text: __('Deleted {{count}} 404 log entry(s).', { count: res.deleted }) });
      await loadLogs();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleClear = async (olderThanDays: number | null) => {
    if (!csrfToken) return;
    setClearOpen(false);
    const confirmMsg =
      olderThanDays === null
        ? __('Delete all recorded 404 log entries?')
        : __('Flush pending telemetry and delete 404 logs older than 30 days?');
    if (!window.confirm(confirmMsg)) return;
    setBusy(true);
    try {
      const res = await clearNotFoundLogs(olderThanDays, csrfToken);
      setNotice({ text: __('Cleared {{count}} 404 log entry(s).', { count: res.deleted }) });
      await loadLogs();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const allOnPageSelected =
    Boolean(data && data.items.length > 0) &&
    data!.items.every((item) => selectedIds.includes(item.id));
  const totalPages = data ? Math.max(1, Math.ceil(data.total / data.per_page)) : 1;

  return (
    <div className="space-y-4 pt-4" data-testid="not-found-log-panel">
      {notice && (
        <AdminAlert tone="success" className="flex flex-wrap items-center justify-between gap-2">
          <span>{notice.text}</span>
          {notice.linkToRedirects && (
            <AdminButton
              variant="secondary"
              size="sm"
              onClick={() => onNavigateHash('#/tools/redirects')}
            >
              {__('View Redirects')}
            </AdminButton>
          )}
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
            placeholder={__('Search 404 path or referrer…')}
            data-testid="not-found-search-input"
            className="w-64 rounded-md border border-zinc-300 px-3 py-1.5 text-sm text-zinc-900 focus:border-od-accent focus:outline-none"
          />
          <select
            value={sort}
            onChange={(e) => {
              setPage(1);
              setSort(e.target.value as NotFoundSort);
            }}
            data-testid="not-found-sort-select"
            className="rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-sm text-zinc-900"
          >
            <option value="hits_desc">{__('Most hits')}</option>
            <option value="recent_desc">{__('Most recent')}</option>
            <option value="oldest_asc">{__('Oldest first')}</option>
          </select>
        </div>

        <div className="relative flex flex-wrap items-center gap-2">
          <AdminButton
            variant="secondary"
            size="sm"
            onClick={() => {
              void loadLogs();
            }}
            data-testid="not-found-refresh-btn"
          >
            {__('Refresh')}
          </AdminButton>
          {selectedIds.length > 0 && (
            <AdminButton
              variant="danger"
              size="sm"
              disabled={busy}
              onClick={() => {
                void handleBulkDelete();
              }}
              data-testid="not-found-bulk-delete-btn"
            >
              {__('Delete selected ({{count}})', { count: selectedIds.length })}
            </AdminButton>
          )}
          <div className="relative" ref={clearMenuRef}>
            <AdminButton
              variant="secondary"
              size="sm"
              disabled={busy}
              aria-expanded={clearOpen}
              aria-haspopup="menu"
              onClick={() => setClearOpen((v) => !v)}
              data-testid="not-found-clear-btn"
            >
              {__('Clear log')}
            </AdminButton>
            {clearOpen && (
              <div role="menu" className="absolute right-0 z-20 mt-1 w-56 rounded-lg border border-zinc-200 bg-white p-1 shadow-lg">
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    void handleClear(30);
                  }}
                  className="block w-full rounded px-3 py-1.5 text-left text-xs text-zinc-800 hover:bg-zinc-100"
                >
                  {__('Clear older than 30 days')}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    void handleClear(null);
                  }}
                  className="block w-full rounded px-3 py-1.5 text-left text-xs text-rose-700 hover:bg-rose-50"
                >
                  {__('Clear all 404 logs')}
                </button>
              </div>
            )}
          </div>
        </div>
      </div>

      <div className="overflow-x-auto rounded-lg border border-zinc-200 bg-white">
        <table className="w-full border-collapse text-left text-sm">
          <thead className="border-b border-zinc-200 bg-zinc-50 text-xs font-medium text-zinc-600">
            <tr>
              <th className="w-8 px-3 py-2.5">
                <input
                  type="checkbox"
                  checked={allOnPageSelected}
                  aria-label={__('Select all 404 logs on this page')}
                  onChange={(e) => {
                    if (!data) return;
                    setSelectedIds(e.target.checked ? data.items.map((i) => i.id) : []);
                  }}
                />
              </th>
              <th className="px-3 py-2.5">{__('Requested Path')}</th>
              <th className="px-3 py-2.5">{__('Hits')}</th>
              <th className="px-3 py-2.5">{__('Last Seen')}</th>
              <th className="px-3 py-2.5">{__('First Seen')}</th>
              <th className="px-3 py-2.5">{__('Referrer / User-Agent')}</th>
              <th className="px-3 py-2.5 text-right">{__('Actions')}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-zinc-100">
            {!data || data.items.length === 0 ? (
              <tr>
                <td colSpan={7} className="px-3 py-6 text-center text-sm text-zinc-500">
                  {__('No 404 logs recorded yet.')}
                </td>
              </tr>
            ) : (
              data.items.map((item) => (
                <tr key={item.id} className="hover:bg-zinc-50/70">
                  <td className="px-3 py-2.5">
                    <input
                      type="checkbox"
                      checked={selectedIds.includes(item.id)}
                      aria-label={__('Select 404 log {{path}}', { path: item.path })}
                      onChange={(e) => {
                        setSelectedIds((prev) =>
                          e.target.checked ? [...prev, item.id] : prev.filter((id) => id !== item.id),
                        );
                      }}
                    />
                  </td>
                  <td className="px-3 py-2.5 font-mono text-xs text-zinc-900">{item.path}</td>
                  <td className="px-3 py-2.5 text-xs font-medium text-zinc-800">{item.hit_count}</td>
                  <td className="px-3 py-2.5 text-xs text-zinc-600">{item.last_seen_at}</td>
                  <td className="px-3 py-2.5 text-xs text-zinc-500">{item.first_seen_at}</td>
                  <td className="max-w-xs truncate px-3 py-2.5 text-xs text-zinc-500">
                    {[item.last_referrer, item.last_user_agent].filter(Boolean).join(' · ') || '—'}
                  </td>
                  <td className="px-3 py-2.5 text-right">
                    <div className="inline-flex items-center gap-1.5">
                      <AdminButton
                        variant="primary"
                        size="sm"
                        onClick={() => handleOpenCreateFrom404(item)}
                        data-testid="not-found-create-redirect-btn"
                      >
                        {__('Create Redirect')}
                      </AdminButton>
                      <AdminButton
                        variant="danger"
                        size="sm"
                        disabled={busy}
                        onClick={() => {
                          void handleDeleteOne(item);
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

      {draft && draft.modalMode === 'from_404' && (
        <RedirectFormModal
          mode="from_404"
          editingId={null}
          sourcePath={draft.sourcePath}
          targetUrl={draft.targetUrl}
          statusCode={draft.statusCode}
          enabled={draft.enabled}
          note={draft.note}
          resolveNotFound={draft.resolveNotFound}
          staleConflict={false}
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
          onRebaseLatest={() => {}}
          onKeepDraft={() => {}}
          onError={onApiError}
          onSubmit={handleSubmitFrom404}
          onClose={() => {
            setFormError(null);
            persistDraft(null);
          }}
        />
      )}
    </div>
  );
}
