import { tStatic, useI18n } from '../../i18n/index.tsx';
import { formatNumber } from '../../i18n/format.ts';
import React, { useEffect, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { getDashboard, type DashboardCounts } from '../../api/dashboard.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';

interface DashboardPageProps {
  onApiError: (error: unknown, fallback: string) => void;
}

const metrics = [
  { kind: 'post', status: 'published', label: 'feature.dashboard.dashboardpage.published_posts' },
  { kind: 'post', status: 'draft', label: 'feature.dashboard.dashboardpage.draft_posts' },
  { kind: 'page', status: 'published', label: 'feature.dashboard.dashboardpage.published_pages' },
  { kind: 'page', status: 'draft', label: 'feature.dashboard.dashboardpage.draft_pages' },
] as const;

export const DashboardPage: React.FC<DashboardPageProps> = ({ onApiError }) => {
  const { t: translate } = useI18n();
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
        setError(cause instanceof Error ? cause.message : tStatic('feature.dashboard.dashboardpage.could_not_load_statistics'));
        if (cause instanceof ApiClientError && cause.status === 401) {
          onApiError(cause, tStatic('feature.dashboard.dashboardpage.the_session_is_invalid'));
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
          <h2 id="dashboard-stats-heading" className="text-base font-semibold text-zinc-950">{translate('feature.dashboard.dashboardpage.content_statistics')}</h2>
          <p className="mt-0.5 text-sm text-zinc-600">{translate('feature.dashboard.dashboardpage.number_of_posts_and_pages_on_the_website')}</p>
        </div>
        <AdminButton
          variant="secondary"
          size="sm"
          onClick={() => setRetryVersion(value => value + 1)}
          disabled={loading}
        >
          {loading ? translate('feature.dashboard.dashboardpage.loading') : translate('feature.dashboard.dashboardpage.refresh_statistics')}
        </AdminButton>
      </div>

      {error && (
        <AdminAlert tone="error" className="space-y-2.5">
          <p>{translate('feature.dashboard.dashboardpage.could_not_load_statistics_2')}{' '}{error}</p>
          <AdminButton
            variant="secondary"
            size="sm"
            onClick={() => setRetryVersion(value => value + 1)}
          >{translate('feature.dashboard.dashboardpage.retry')}</AdminButton>
        </AdminAlert>
      )}

      <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-busy={loading}>
        {metrics.map(metric => (
          <article
            key={metric.kind + '-' + metric.status}
            className="min-w-0 rounded-xl border border-zinc-200 bg-white p-4"
            data-dashboard-metric={metric.kind + '-' + metric.status}
          >
            <p className="text-xs font-medium text-zinc-500">{translate(metric.label)}</p>
            <p className="mt-2 text-2xl font-semibold tracking-tight tabular-nums text-zinc-950">
              {loading ? <span role="status" className="text-sm font-medium text-zinc-400">{translate('reading.loading')}</span>
                : counts ? formatNumber(counts[metric.kind][metric.status])
                  : <span aria-label={translate('feature.dashboard.dashboardpage.no_data_available')}>—</span>}
            </p>
          </article>
        ))}
      </div>
    </section>
  );
};
