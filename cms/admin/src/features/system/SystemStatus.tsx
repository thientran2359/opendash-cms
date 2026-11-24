import { useEffect, useState } from 'react';
import { useI18n } from '../../i18n/index.tsx';
import { getHealth, type HealthResponse } from '../../api/client.ts';
import { AdminAlert, AdminBadge } from '../../components/AdminPrimitives.tsx';

export function SystemStatus() {
  const { t } = useI18n();
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
          {t('system.heading')}
        </h2>
        <p className="mt-0.5 text-sm text-zinc-600">{t('system.description')}</p>
      </div>
      {health ? (
        <div className="flex flex-wrap items-center gap-3 rounded-xl border border-zinc-200 bg-zinc-50 px-4 py-3 text-sm">
          <AdminBadge tone="success">{t('system.running')}</AdminBadge>
          <span className="text-zinc-700">
            {t('system.health', { status: health.status, database: health.database })}
          </span>
        </div>
      ) : error ? (
        <AdminAlert tone="error">{t('system.unreachable', { error })}</AdminAlert>
      ) : (
        <p role="status" className="text-sm text-zinc-500">{t('system.checking')}</p>
      )}
    </section>
  );
}
