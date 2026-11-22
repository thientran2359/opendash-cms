import React, { useEffect, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { getDashboard, type DashboardCounts } from '../../api/dashboard.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';

interface DashboardPageProps {
  onApiError: (error: unknown, fallback: string) => void;
}

const metrics = [
  { kind: 'post', status: 'published', label: 'Bài viết đã xuất bản' },
  { kind: 'post', status: 'draft', label: 'Bài viết nháp' },
  { kind: 'page', status: 'published', label: 'Trang đã xuất bản' },
  { kind: 'page', status: 'draft', label: 'Trang nháp' },
] as const;

export const DashboardPage: React.FC<DashboardPageProps> = ({ onApiError }) => {
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
        setError(cause instanceof Error ? cause.message : 'Không thể tải thống kê.');
        if (cause instanceof ApiClientError && cause.status === 401) {
          onApiError(cause, 'Phiên đăng nhập không hợp lệ.');
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
          <h2 id="dashboard-stats-heading" className="text-base font-semibold text-zinc-950">
            Thống kê nội dung
          </h2>
          <p className="mt-0.5 text-sm text-zinc-600">Số bài viết và trang hiện có trên website.</p>
        </div>
        <AdminButton
          variant="secondary"
          size="sm"
          onClick={() => setRetryVersion(value => value + 1)}
          disabled={loading}
        >
          {loading ? 'Đang tải...' : 'Làm mới thống kê'}
        </AdminButton>
      </div>

      {error && (
        <AdminAlert tone="error" className="space-y-2.5">
          <p>Không tải được thống kê: {error}</p>
          <AdminButton
            variant="secondary"
            size="sm"
            onClick={() => setRetryVersion(value => value + 1)}
          >
            Thử lại
          </AdminButton>
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
              {loading ? <span role="status" className="text-sm font-medium text-zinc-400">Đang tải…</span>
                : counts ? counts[metric.kind][metric.status].toLocaleString('vi-VN')
                  : <span aria-label="Chưa có số liệu">—</span>}
            </p>
          </article>
        ))}
      </div>
    </section>
  );
};
