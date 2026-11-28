import React, { useEffect, useState } from 'react';
import { useI18n } from '../../i18n/index.tsx';
import { ApiClientError } from '../../api/client.ts';
import {
  getSearchIndexStatus,
  reindexSearchIndex,
  type SearchIndexStatus,
  type SearchReindexResult,
} from '../../api/operations.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';

const PANEL_CARD_CLASS = 'rounded-xl border border-zinc-200 bg-white p-5 shadow-2xs';

interface SearchIndexToolsPanelProps {
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
}

export const SearchIndexToolsPanel: React.FC<SearchIndexToolsPanelProps> = ({
  csrfToken,
  onApiError,
}) => {
  const { __ } = useI18n();
  const [status, setStatus] = useState<SearchIndexStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [reindexing, setReindexing] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [errorBanner, setErrorBanner] = useState<string | null>(null);
  const [lastResult, setLastResult] = useState<SearchReindexResult | null>(null);

  const loadStatus = async () => {
    setLoading(true);
    try {
      const next = await getSearchIndexStatus();
      setStatus(next);
      setErrorBanner(null);
    } catch (err) {
      if (err instanceof ApiClientError && err.status === 403) {
        setErrorBanner(__("Admin permission is required for search index tools."));
      } else {
        onApiError(err);
      }
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadStatus();
  }, []);

  const handleReindex = async () => {
    if (!csrfToken || reindexing) return;
    setReindexing(true);
    setNotice(null);
    setErrorBanner(null);
    try {
      const res = await reindexSearchIndex(csrfToken);
      setLastResult(res);
      setNotice(
        __("Search index rebuilt: {{count}} indexed entries in {{ms}} ms.", {
          count: res.indexed_documents,
          ms: res.duration_ms,
        }),
      );
      await loadStatus();
    } catch (err) {
      if (err instanceof ApiClientError) {
        if (err.status === 409) {
          setErrorBanner(__("Another maintenance or backup task is currently active. Try again after it completes."));
          return;
        }
        if (err.status === 503) {
          setErrorBanner(__("Server is busy processing active writes (503). Please retry shortly."));
          return;
        }
        if (err.status === 403) {
          setErrorBanner(__("Admin permission is required for search index tools."));
          return;
        }
      }
      onApiError(err);
    } finally {
      setReindexing(false);
    }
  };

  return (
    <div className="min-w-0 space-y-5" data-testid="search-index-tools-panel">
      <section className={PANEL_CARD_CLASS}>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-base font-semibold text-zinc-900">{__("Search index")}</h2>
            <p className="mt-1 text-sm text-zinc-600">
              {__("Inspect SQLite FTS5 coverage across published public entries and rebuild the index on demand.")}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <AdminButton
              variant="secondary"
              size="sm"
              disabled={loading || reindexing}
              onClick={() => { void loadStatus(); }}
              data-testid="search-index-refresh-btn"
            >
              {__("Refresh")}
            </AdminButton>
            <AdminButton
              variant="primary"
              size="sm"
              disabled={reindexing || !csrfToken || Boolean(status?.maintenance_active)}
              onClick={() => { void handleReindex(); }}
              data-testid="search-index-rebuild-btn"
            >
              {reindexing ? __("Rebuilding search index...") : __("Rebuild search index")}
            </AdminButton>
          </div>
        </div>

        {notice && (
          <div className="mt-4" role="status" data-testid="search-index-notice">
            <AdminAlert tone="success">{notice}</AdminAlert>
          </div>
        )}
        {errorBanner && (
          <div className="mt-4" role="alert" data-testid="search-index-error">
            <AdminAlert tone="error">{errorBanner}</AdminAlert>
          </div>
        )}
        {status?.maintenance_active && (
          <div className="mt-4" role="status" data-testid="search-index-maintenance-warning">
            <AdminAlert tone="warning">
              {__("Maintenance mode is active on this site. Reindex will stay disabled until it finishes.")}
            </AdminAlert>
          </div>
        )}

        {status && (
          <dl className="mt-5 grid grid-cols-1 gap-4 sm:grid-cols-3">
            <div className="rounded-lg border border-zinc-200 bg-zinc-50 p-3.5">
              <dt className="text-xs font-medium text-zinc-600">{__("Indexed documents")}</dt>
              <dd className="ml-0 mt-1 text-2xl font-semibold text-zinc-900" data-testid="search-indexed-count">
                {status.indexed_documents}
              </dd>
            </div>
            <div className="rounded-lg border border-zinc-200 bg-zinc-50 p-3.5">
              <dt className="text-xs font-medium text-zinc-600">{__("Eligible published entries")}</dt>
              <dd className="ml-0 mt-1 text-2xl font-semibold text-zinc-900" data-testid="search-eligible-count">
                {status.eligible_published_entries}
              </dd>
            </div>
            <div className="rounded-lg border border-zinc-200 bg-zinc-50 p-3.5">
              <dt className="text-xs font-medium text-zinc-600">{__("Status")}</dt>
              <dd className="ml-0 mt-2" data-testid="search-sync-badge">
                <AdminBadge tone={status.in_sync ? 'success' : 'warning'}>
                  {status.in_sync ? __("In sync") : __("Out of sync")}
                </AdminBadge>
              </dd>
            </div>
          </dl>
        )}
      </section>

      <section className={PANEL_CARD_CLASS}>
        <h3 className="text-sm font-semibold text-zinc-900">{__("Last reindex job")}</h3>
        {status?.last_job ? (
          <div className="mt-3 space-y-1.5 text-sm text-zinc-700" data-testid="search-last-job">
            <div className="flex flex-wrap items-center gap-2">
              <code className="rounded bg-zinc-100 px-1.5 py-0.5 text-xs">{status.last_job.id}</code>
              <AdminBadge tone={status.last_job.state === 'succeeded' ? 'success' : status.last_job.state === 'failed' ? 'danger' : 'neutral'}>
                {status.last_job.state}
              </AdminBadge>
              {lastResult && lastResult.job.id === status.last_job.id && (
                <span className="text-xs text-zinc-500">({lastResult.duration_ms} ms)</span>
              )}
            </div>
            {status.last_job.finished_at && (
              <p className="text-xs text-zinc-500">
                {__("Finished at: {{time}}", { time: status.last_job.finished_at })}
              </p>
            )}
            {status.last_job.error_message && (
              <p className="text-xs text-rose-700">{status.last_job.error_message}</p>
            )}
          </div>
        ) : (
          <p className="mt-2 text-sm text-zinc-500">{__("No reindex jobs recorded yet.")}</p>
        )}
      </section>
    </div>
  );
};
