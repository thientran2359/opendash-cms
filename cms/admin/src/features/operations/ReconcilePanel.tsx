import { tStatic, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useState } from 'react';
import {
  applyReconcile,
  getOperationsDiagnostics,
  planReconcile,
  purgeQuarantine,
  type OperationsDiagnostics,
  type ReconcilePlan,
} from '../../api/operations.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';

interface ReconcilePanelProps {
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
}

export const ReconcilePanel: React.FC<ReconcilePanelProps> = ({ csrfToken, onApiError }) => {
  const { t: translate } = useI18n();
  const [diagnostics, setDiagnostics] = useState<OperationsDiagnostics | null>(null);
  const [plan, setPlan] = useState<ReconcilePlan | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const loadDiagnostics = async () => {
    try {
      const data = await getOperationsDiagnostics();
      setDiagnostics(data);
    } catch (err) {
      onApiError(err);
    }
  };

  useEffect(() => {
    void loadDiagnostics();
  }, []);

  const handlePlan = async () => {
    if (!csrfToken) return;
    setBusy(true);
    setNotice(null);
    try {
      const next = await planReconcile(csrfToken);
      setPlan(next);
      setNotice(tStatic('feature.operations.reconcilepanel.reconciliation_plan_created_generation_value1_value2_orphaned_files_24h', { value1: next.generation, value2: next.orphan_candidates.length }));
      await loadDiagnostics();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleApply = async () => {
    if (!csrfToken || !plan) return;
    if (!window.confirm(tStatic('feature.operations.reconcilepanel.confirm_moving_value1_orphaned_files_to_quarantine', { value1: plan.orphan_candidates.length }))) {
      return;
    }
    setBusy(true);
    setNotice(null);
    try {
      const res = await applyReconcile(csrfToken, plan);
      setPlan(null);
      setNotice(tStatic('feature.operations.reconcilepanel.quarantined_value1_files_generation_value2', { value1: res.quarantined_files, value2: res.generation }));
      await loadDiagnostics();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handlePurge = async () => {
    if (!csrfToken) return;
    if (!window.confirm(tStatic('feature.operations.reconcilepanel.confirm_permanently_deleting_quarantine_directories_and_orphaned_secrets_older_t'))) {
      return;
    }
    setBusy(true);
    setNotice(null);
    try {
      const res = await purgeQuarantine(csrfToken);
      setNotice(
        tStatic('feature.operations.reconcilepanel.purged_value1_quarantine_directories_and_value2_orphaned_secrets_skipped_value3_', { value1: res.purged_quarantine_dirs, value2: res.purged_secret_blobs, value3: res.skipped_under_grace }),
      );
      await loadDiagnostics();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">{translate('feature.operations.reconcilepanel.storage_reconciliation_quarantine')}</h2>
          <p className="mt-0.5 text-sm text-zinc-600">{translate('feature.operations.reconcilepanel.manually_scan_orphaned_files_24_hours_missing_files_and_orphaned_secrets_purge_q')}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <AdminButton variant="secondary" size="sm" disabled={busy || !csrfToken} onClick={handlePlan}>{translate('feature.operations.reconcilepanel.scan_plan')}</AdminButton>
          <AdminButton
            variant="primary"
            size="sm"
            disabled={busy || !csrfToken || !plan || plan.orphan_candidates.length === 0}
            onClick={handleApply}
          >{translate('feature.operations.reconcilepanel.apply_quarantine')}</AdminButton>
          <AdminButton variant="danger" size="sm" disabled={busy || !csrfToken} onClick={handlePurge}>{translate('feature.operations.reconcilepanel.purge_quarantine_7_days')}</AdminButton>
        </div>
      </div>

      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}

      {diagnostics && (
        <div className="grid grid-cols-1 gap-3 text-sm sm:grid-cols-2 lg:grid-cols-4">
          <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3.5">
            <div className="text-xs font-medium text-zinc-500">{translate('feature.operations.reconcilepanel.generation')}</div>
            <div className="mt-1 text-lg font-semibold tabular-nums text-zinc-950">{diagnostics.maintenance_generation}</div>
          </div>
          <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3.5">
            <div className="text-xs font-medium text-zinc-500">{translate('feature.operations.reconcilepanel.pending_deletions')}</div>
            <div className="mt-1 text-lg font-semibold tabular-nums text-zinc-950">{diagnostics.pending_delete_jobs.length}</div>
          </div>
          <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3.5">
            <div className="text-xs font-medium text-zinc-500">{translate('feature.operations.reconcilepanel.quarantine_eligible_after_7d_retained')}</div>
            <div className="mt-1 text-lg font-semibold tabular-nums text-zinc-950">
              {diagnostics.purgeable_quarantine_dirs} / {diagnostics.retained_quarantine_dirs}
            </div>
          </div>
          <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3.5">
            <div className="text-xs font-medium text-zinc-500">{translate('feature.operations.reconcilepanel.orphaned_secrets_eligible_after_7d')}</div>
            <div className="mt-1 text-lg font-semibold tabular-nums text-zinc-950">
              {diagnostics.orphan_secret_blobs} ({diagnostics.purgeable_secret_blobs})
            </div>
          </div>
        </div>
      )}

      {plan && (
        <AdminAlert tone="warning">{translate('feature.operations.reconcilepanel.plan')}{' '}<code className="font-mono text-xs">{plan.plan_id.slice(0, 12)}...</code>{' '}{translate('feature.operations.reconcilepanel.scan')}{' '}{plan.scanned_records}{' '}{translate('feature.operations.reconcilepanel.records')}{' '}
          {plan.scanned_files}{' '}{translate('feature.operations.reconcilepanel.files_missing_files')}{' '}{plan.missing_canonical_files.length}{translate('feature.operations.reconcilepanel.orphaned_24h')}{' '}
          {plan.orphan_candidates.length}{translate('feature.operations.reconcilepanel.new_orphans_24h')}{' '}{plan.recent_orphans_under_grace}.
        </AdminAlert>
      )}
    </section>
  );
};
