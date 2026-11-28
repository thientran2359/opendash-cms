import { __, useI18n } from '../../i18n/index.tsx';
import { formatNumber } from '../../i18n/format.ts';
import React, { useEffect, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { getDashboard, type DashboardCounts } from '../../api/dashboard.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';

interface DashboardPageProps {
  onApiError: (error: unknown, fallback: string) => void;
}

export const DashboardPage: React.FC<DashboardPageProps> = ({ onApiError }) => {
  const { __, _x } = useI18n();
  const metrics = [
    { kind: 'post', status: 'published', label: __("Published posts") },
    { kind: 'post', status: 'draft', label: __("Draft posts") },
    { kind: 'page', status: 'published', label: __("Published pages") },
    { kind: 'page', status: 'draft', label: __("Draft pages") },
  ] as const;
  const [counts, setCounts] = useState<DashboardCounts | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [retryVersion, setRetryVersion] = useState(0);

  // The page is mounted only while active: returning from Posts/Pages always
  // fetches again. Abort stale requests when switching sections or logging out.
  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    setCounts(null);
    setError('');
    void getDashboard(controller.signal)
      .then(data => {
        if (controller.signal.aborted) return;
        setCounts(data);
      })
      .catch(cause => {
        if (controller.signal.aborted) return;
        setError(cause instanceof Error ? cause.message : __("Could not load statistics."));
        if (cause instanceof ApiClientError && cause.status === 401) {
          onApiError(cause, __("The session is invalid."));
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [retryVersion, onApiError]);

  return (
    <section className="space-y-3" aria-labelledby="dashboard-stats-heading">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 id="dashboard-stats-heading" className="text-base font-semibold text-zinc-950">{__("Content statistics")}</h2>
          <p className="mt-0.5 text-sm text-zinc-600">{__("Number of posts and pages on the website.")}</p>
        </div>
        <AdminButton
          variant="secondary"
          size="sm"
          onClick={() => setRetryVersion(value => value + 1)}
          disabled={loading}
        >
          {loading ? __("Loading...") : __("Refresh statistics")}
        </AdminButton>
      </div>

      {error && (
        <AdminAlert tone="error" className="space-y-2.5">
          <p>{__("Could not load statistics:")}{' '}{error}</p>
          <AdminButton
            variant="secondary"
            size="sm"
            onClick={() => setRetryVersion(value => value + 1)}
          >{_x("Retry", "feature.dashboard")}</AdminButton>
        </AdminAlert>
      )}

      <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-busy={loading}>
        {metrics.map(metric => (
          <article
            key={metric.kind + '-' + metric.status}
            className="min-w-0 rounded-xl border border-zinc-200 bg-white p-4"
            data-dashboard-metric={metric.kind + '-' + metric.status}
          >
            <p className="text-xs font-medium text-zinc-500">{metric.label}</p>
            <p className="mt-2 text-2xl font-semibold tracking-tight tabular-nums text-zinc-950">
              {loading ? <span role="status" className="text-sm font-medium text-zinc-400">{__("Loading…")}</span>
                : counts ? formatNumber(counts[metric.kind][metric.status])
                  : <span aria-label={__("No data available")}>—</span>}
            </p>
          </article>
        ))}
      </div>
    </section>
  );
};
