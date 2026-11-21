import { useEffect, useState } from 'react';
import { getHealth, type HealthResponse } from '../../api/client.ts';
import { AdminAlert, AdminBadge } from '../../components/AdminPrimitives.tsx';

export function SystemStatus() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    getHealth(controller.signal)
      .then((result) => {
        setHealth(result);
        setError(null);
      })
      .catch((reason: unknown) => {
        if (!controller.signal.aborted) {
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      });
    return () => controller.abort();
  }, []);

  return (
    <section className="space-y-4" aria-labelledby="system-status-heading">
      <div>
        <h2 id="system-status-heading" className="text-base font-semibold text-zinc-950">
          Trạng thái hệ thống
        </h2>
        <p className="mt-0.5 text-sm text-zinc-600">Kiểm tra kết nối HTTP backend và cơ sở dữ liệu SQLite.</p>
      </div>
      {health ? (
        <div className="flex flex-wrap items-center gap-3 rounded-xl border border-zinc-200 bg-zinc-50 px-4 py-3 text-sm">
          <AdminBadge tone="success">Đang hoạt động</AdminBadge>
          <span className="text-zinc-700">
            HTTP: <strong className="text-zinc-950">{health.status}</strong> · SQLite:{' '}
            <strong className="text-zinc-950">{health.database}</strong>
          </span>
        </div>
      ) : error ? (
        <AdminAlert tone="error">Không kết nối được: {error}</AdminAlert>
      ) : (
        <p role="status" className="text-sm text-zinc-500">Đang kiểm tra /api/health…</p>
      )}
    </section>
  );
}
