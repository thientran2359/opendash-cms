import React, { useEffect, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import {
  getMediaVariantsStatus,
  regenerateMediaVariants,
  type MediaVariantsStatus,
  type RegenerateCumulativeStats,
} from '../../api/media.ts';
import { cancelOperationJob } from '../../api/operations.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { useI18n } from '../../i18n/index.tsx';
import { formatMediaBytes } from './library/format.ts';

export interface MediaVariantsRegenerateCardProps {
  csrfToken: string | null;
  onApiError: (error: unknown, fallback?: string) => void;
  onCompleted?: () => void;
  compact?: boolean;
}

const REGENERATE_BATCH_SIZE = 15;

export const MediaVariantsRegenerateCard: React.FC<MediaVariantsRegenerateCardProps> = ({
  csrfToken,
  onApiError,
  onCompleted,
  compact = false,
}) => {
  const { __ } = useI18n();
  const [status, setStatus] = useState<MediaVariantsStatus | null>(null);
  const [running, setRunning] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const [progress, setProgress] = useState<{
    cumulative: RegenerateCumulativeStats;
    remaining: number;
    failedIds: string[];
  } | null>(null);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const stopRequestedRef = useRef(false);
  const mountedRef = useRef(true);

  const loadStatus = async () => {
    try {
      const next = await getMediaVariantsStatus();
      if (mountedRef.current) setStatus(next);
    } catch (cause) {
      if (mountedRef.current) {
        onApiError(cause, __("Could not load responsive variant status."));
      }
    }
  };

  useEffect(() => {
    mountedRef.current = true;
    void loadStatus();
    return () => {
      mountedRef.current = false;
      stopRequestedRef.current = true;
    };
  }, []);

  const handleCancelRun = async (runId: string | null) => {
    stopRequestedRef.current = true;
    if (!csrfToken || !runId || cancelling) return;
    setCancelling(true);
    try {
      await cancelOperationJob(csrfToken, runId);
      if (mountedRef.current) {
        setNotice(__("Requested stopping variant regeneration ({{value1}}…).", { value1: runId.slice(0, 8) }));
        setActiveRunId(null);
      }
    } catch (cause) {
      if (mountedRef.current && cause instanceof ApiClientError && cause.status === 412) {
        setNotice(__("The variant regeneration job already finished."));
        setActiveRunId(null);
      } else if (mountedRef.current) {
        onApiError(cause, __("Could not stop the variant regeneration job."));
      }
    } finally {
      if (mountedRef.current) {
        setCancelling(false);
        void loadStatus();
      }
    }
  };

  const runRegenerate = async (force: boolean) => {
    if (!csrfToken || running) return;
    if (force && !window.confirm(__("Force regenerate responsive variants for all existing images? This re-encodes every active image in batches."))) {
      return;
    }

    stopRequestedRef.current = false;
    setRunning(true);
    setNotice('');
    setError('');
    setProgress(null);

    let runId: string | null = null;
    let afterId: string | null = null;
    const failedIds: string[] = [];

    try {
      while (!stopRequestedRef.current) {
        const res = await regenerateMediaVariants(csrfToken, {
          batch_size: REGENERATE_BATCH_SIZE,
          force,
          ...(runId ? { run_id: runId } : {}),
          ...(afterId ? { after_id: afterId } : {}),
        });
        runId = res.run_id;
        afterId = res.next_after_id;
        for (const id of res.failed_ids) {
          if (!failedIds.includes(id)) failedIds.push(id);
        }
        if (mountedRef.current) {
          setActiveRunId(res.done ? null : res.run_id);
          setProgress({
            cumulative: res.cumulative,
            remaining: res.remaining,
            failedIds: [...failedIds],
          });
        }

        if (res.done) {
          if (mountedRef.current) {
            setNotice(
              __("Variant regeneration completed: {{value1}} succeeded, {{value2}} failed, {{value3}} skipped.", {
                value1: res.cumulative.total_succeeded,
                value2: res.cumulative.total_failed,
                value3: res.cumulative.total_skipped_concurrent,
              }),
            );
          }
          break;
        }

        if (stopRequestedRef.current) {
          try {
            await cancelOperationJob(csrfToken, res.run_id);
          } catch {
            // Job may already be cancelled cleanly.
          }
          if (mountedRef.current) {
            setActiveRunId(null);
            setNotice(
              __("Variant regeneration paused at {{value1}} processed ({{value2}} remaining).", {
                value1: res.cumulative.total_processed,
                value2: res.remaining,
              }),
            );
          }
          break;
        }
      }
    } catch (cause) {
      if (!mountedRef.current) return;
      if (cause instanceof ApiClientError && cause.status === 409) {
        if (stopRequestedRef.current || cause.message.includes('regenerate_run_not_active')) {
          setNotice(__("Variant regeneration was stopped."));
          setActiveRunId(null);
        } else {
          setError(__("Another variant regeneration run is already in progress."));
        }
      } else {
        setError(cause instanceof Error ? cause.message : __("Could not regenerate responsive media variants."));
        onApiError(cause, __("Could not regenerate responsive media variants."));
      }
    } finally {
      if (mountedRef.current) {
        setRunning(false);
        void loadStatus();
        onCompleted?.();
      }
    }
  };

  const trackedActiveRunId = activeRunId ?? status?.active_run_id ?? null;

  return (
    <section
      className={compact
        ? 'rounded-xl border border-zinc-200 bg-white p-3.5 space-y-3'
        : 'rounded-xl border border-zinc-200 bg-white p-4 space-y-3'}
      aria-label={__("Responsive image variant regeneration")}
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold text-zinc-950">{__("Responsive image variants")}</h3>
          {status ? (
            <p className="mt-0.5 text-xs text-zinc-600">
              {__("Ready {{value1}}/{{value2}} images · Pending {{value3}} · Variant disk usage {{value4}}", {
                value1: status.ready_images,
                value2: status.total_images,
                value3: status.pending_images,
                value4: formatMediaBytes(status.variant_files_bytes),
              })}
            </p>
          ) : (
            <p className="mt-0.5 text-xs text-zinc-500">{__("Loading variant status…")}</p>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <AdminButton
            variant="secondary"
            size="sm"
            disabled={!csrfToken || running || (status?.pending_images === 0)}
            onClick={() => void runRegenerate(false)}
          >
            {__("Generate missing variants")}
          </AdminButton>
          <AdminButton
            variant="secondary"
            size="sm"
            disabled={!csrfToken || running || (status?.total_images === 0)}
            onClick={() => void runRegenerate(true)}
          >
            {__("Force regenerate all")}
          </AdminButton>
          {(running || trackedActiveRunId) && (
            <AdminButton
              variant="danger"
              size="sm"
              disabled={!csrfToken || cancelling}
              onClick={() => void handleCancelRun(trackedActiveRunId)}
            >
              {cancelling ? __("Stopping…") : __("Stop")}
            </AdminButton>
          )}
        </div>
      </div>

      {progress && (
        <p className="text-xs text-zinc-700" role="status">
          {__("Processed {{value1}} · Succeeded {{value2}} · Failed {{value3}} · Skipped {{value4}} · Remaining {{value5}}", {
            value1: progress.cumulative.total_processed,
            value2: progress.cumulative.total_succeeded,
            value3: progress.cumulative.total_failed,
            value4: progress.cumulative.total_skipped_concurrent,
            value5: progress.remaining,
          })}
        </p>
      )}
      {progress && progress.failedIds.length > 0 && (
        <AdminAlert tone="warning">
          {__("Failed media IDs:")}{' '}
          <code className="font-mono text-xs">{progress.failedIds.slice(0, 10).join(', ')}</code>
        </AdminAlert>
      )}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}
      {error && <AdminAlert tone="error">{error}</AdminAlert>}
    </section>
  );
};
