import { __, useI18n } from '../../i18n/index.tsx';
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
import { formatMediaBytes } from '../media/library/format.ts';
import { MediaVariantsRegenerateCard } from '../media/MediaVariantsRegenerateCard.tsx';

interface ReconcilePanelProps {
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
}

export const ReconcilePanel: React.FC<ReconcilePanelProps> = ({ csrfToken, onApiError }) => {
  const { __ } = useI18n();
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
      setNotice(__("Reconciliation plan created (generation {{value1}}, {{value2}} orphaned files >= 24h).", { value1: next.generation, value2: next.orphan_candidates.length }));
      await loadDiagnostics();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleApply = async () => {
    if (!csrfToken || !plan) return;
    if (!window.confirm(__("Confirm moving {{value1}} orphaned files to quarantine?", { value1: plan.orphan_candidates.length }))) {
      return;
    }
    setBusy(true);
    setNotice(null);
    try {
      const res = await applyReconcile(csrfToken, plan);
      setPlan(null);
      setNotice(__("Quarantined {{value1}} files (generation {{value2}}).", { value1: res.quarantined_files, value2: res.generation }));
      await loadDiagnostics();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handlePurge = async () => {
    if (!csrfToken) return;
    if (!window.confirm(__("Confirm permanently deleting quarantine directories and orphaned secrets older than 7 days?"))) {
      return;
    }
    setBusy(true);
    setNotice(null);
    try {
      const res = await purgeQuarantine(csrfToken);
      setNotice(
        __("Purged {{value1}} quarantine directories and {{value2}} orphaned secrets (skipped {{value3}} items younger than 7 days).", { value1: res.purged_quarantine_dirs, value2: res.purged_secret_blobs, value3: res.skipped_under_grace }),
      );
      await loadDiagnostics();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const canonicalOrphans = plan?.orphan_candidates.filter(candidate => (candidate.kind ?? 'canonical') === 'canonical') ?? [];
  const variantOrphans = plan?.orphan_candidates.filter(candidate => candidate.kind === 'variant') ?? [];

  return (
    <div className="space-y-6">
      <section className="space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-base font-semibold text-zinc-950">{__("Storage reconciliation & quarantine")}</h2>
            <p className="mt-0.5 text-sm text-zinc-600">{__("Manually scan orphaned files (≥ 24 hours), missing files and orphaned secrets; purge quarantine (≥ 7 days).")}</p>
          </div>
          <div className="flex flex-wrap gap-2">
            <AdminButton variant="secondary" size="sm" disabled={busy || !csrfToken} onClick={handlePlan}>{__("Scan & plan")}</AdminButton>
            <AdminButton
              variant="primary"
              size="sm"
              disabled={busy || !csrfToken || !plan || plan.orphan_candidates.length === 0}
              onClick={handleApply}
            >{__("Apply quarantine")}</AdminButton>
            <AdminButton variant="danger" size="sm" disabled={busy || !csrfToken} onClick={handlePurge}>{__("Purge quarantine (≥ 7 days)")}</AdminButton>
          </div>
        </div>

        {notice && <AdminAlert tone="success">{notice}</AdminAlert>}

        {diagnostics && (
          <div className="grid grid-cols-1 gap-3 text-sm sm:grid-cols-2 lg:grid-cols-4">
            <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3.5">
              <div className="text-xs font-medium text-zinc-500">{__("Generation")}</div>
              <div className="mt-1 text-lg font-semibold tabular-nums text-zinc-950">{diagnostics.maintenance_generation}</div>
            </div>
            <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3.5">
              <div className="text-xs font-medium text-zinc-500">{__("Pending deletions")}</div>
              <div className="mt-1 text-lg font-semibold tabular-nums text-zinc-950">{diagnostics.pending_delete_jobs.length}</div>
            </div>
            <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3.5">
              <div className="text-xs font-medium text-zinc-500">{__("Quarantine (eligible after 7d / retained)")}</div>
              <div className="mt-1 text-lg font-semibold tabular-nums text-zinc-950">
                {diagnostics.purgeable_quarantine_dirs} / {diagnostics.retained_quarantine_dirs}
              </div>
            </div>
            <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3.5">
              <div className="text-xs font-medium text-zinc-500">{__("Orphaned secrets (eligible after 7d)")}</div>
              <div className="mt-1 text-lg font-semibold tabular-nums text-zinc-950">
                {diagnostics.orphan_secret_blobs} ({diagnostics.purgeable_secret_blobs})
              </div>
            </div>
          </div>
        )}

        {plan && (
          <div className="space-y-3">
            <AdminAlert tone="warning">{__("Plan")}{' '}<code className="font-mono text-xs">{plan.plan_id.slice(0, 12)}...</code>{' '}{__("— Scan")}{' '}{plan.scanned_records}{' '}{__("records,")}{' '}
              {plan.scanned_files}{' '}{__("files; missing files:")}{' '}{plan.missing_canonical_files.length}{__("; orphaned ≥ 24h:")}{' '}
              {plan.orphan_candidates.length}{' '}
              {__("(canonical: {{value1}}, variants: {{value2}})", { value1: canonicalOrphans.length, value2: variantOrphans.length })}
              {__("; new orphans (< 24h):")}{' '}{plan.recent_orphans_under_grace}.
            </AdminAlert>
            {plan.orphan_candidates.length > 0 && (
              <div className="overflow-x-auto rounded-xl border border-zinc-200 bg-white">
                <table className="w-full border-collapse text-left text-xs">
                  <thead>
                    <tr className="border-b border-zinc-200 bg-zinc-50 text-zinc-600">
                      <th className="px-3 py-2 font-medium">{__("Type")}</th>
                      <th className="px-3 py-2 font-medium">{__("Logical path")}</th>
                      <th className="px-3 py-2 font-medium">{__("Size")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {plan.orphan_candidates.slice(0, 50).map(candidate => {
                      const isVariant = candidate.kind === 'variant';
                      return (
                        <tr key={`${candidate.kind ?? 'canonical'}-${candidate.logical_key}`} className="border-b border-zinc-100 last:border-b-0">
                          <td className="px-3 py-1.5">
                            <span className={`inline-flex rounded-full px-2 py-0.5 text-[11px] font-medium ${isVariant ? 'bg-amber-100 text-amber-800' : 'bg-zinc-200 text-zinc-800'}`}>
                              {isVariant ? __("Variant") : __("Canonical")}
                            </span>
                          </td>
                          <td className="px-3 py-1.5 font-mono text-zinc-800">{candidate.logical_key}</td>
                          <td className="px-3 py-1.5 tabular-nums text-zinc-600">{formatMediaBytes(candidate.size_bytes)}</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
                {plan.orphan_candidates.length > 50 && (
                  <p className="border-t border-zinc-100 px-3 py-2 text-xs text-zinc-500">
                    {__("Showing first 50 of {{value1}} orphaned candidates.", { value1: plan.orphan_candidates.length })}
                  </p>
                )}
              </div>
            )}
          </div>
        )}
      </section>

      <MediaVariantsRegenerateCard csrfToken={csrfToken} onApiError={onApiError} />
    </div>
  );
};
