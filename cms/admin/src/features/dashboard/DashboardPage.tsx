import React, { useEffect, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { getDashboard, type DashboardCounts } from '../../api/dashboard.ts';

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
    <section className="mb-8 space-y-4" aria-labelledby="dashboard-stats-heading">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 id="dashboard-stats-heading" className="text-lg font-semibold text-slate-900">
            Thống kê nội dung
          </h2>
          <p className="mt-1 text-sm text-slate-600">Số bài viết và trang hiện có trên website.</p>
        </div>
        <button
          type="button"
          className="btn-secondary"
          onClick={() => setRetryVersion(value => value + 1)}
          disabled={loading}
        >
          {loading ? 'Đang tải...' : 'Làm mới thống kê'}
        </button>
      </div>

      {error && (
        <div role="alert" className="rounded-xl border border-rose-200 bg-rose-50 p-4 text-sm text-rose-800">
          <p>Không tải được thống kê: {error}</p>
          <button
            type="button"
            className="btn-secondary mt-3"
            onClick={() => setRetryVersion(value => value + 1)}
          >
            Thử lại
          </button>
        </div>
      )}

      <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-busy={loading}>
        {metrics.map(metric => (
          <article
            key={metric.kind + '-' + metric.status}
            className="min-w-0 rounded-xl border border-slate-200 bg-white p-5 shadow-sm"
            data-dashboard-metric={metric.kind + '-' + metric.status}
          >
            <p className="text-sm font-medium text-slate-600">{metric.label}</p>
            <p className="mt-3 text-3xl font-bold tabular-nums text-slate-900">
              {loading ? <span role="status" className="text-lg font-medium text-slate-500">Đang tải…</span>
                : counts ? counts[metric.kind][metric.status].toLocaleString('vi-VN')
                  : <span aria-label="Chưa có số liệu">—</span>}
            </p>
          </article>
        ))}
      </div>
    </section>
  );
};
