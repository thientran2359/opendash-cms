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
      setNotice(`Đã lập kế hoạch đối soát (generation ${next.generation}, ${next.orphan_candidates.length} tệp mồ côi >= 24h).`);
      await loadDiagnostics();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleApply = async () => {
    if (!csrfToken || !plan) return;
    if (!window.confirm(`Xác nhận chuyển ${plan.orphan_candidates.length} tệp mồ côi vào vùng cách ly?`)) {
      return;
    }
    setBusy(true);
    setNotice(null);
    try {
      const res = await applyReconcile(csrfToken, plan);
      setPlan(null);
      setNotice(`Đã cách ly ${res.quarantined_files} tệp (generation ${res.generation}).`);
      await loadDiagnostics();
    } catch (err) {
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handlePurge = async () => {
    if (!csrfToken) return;
    if (!window.confirm('Xác nhận xóa vĩnh viễn các thư mục cách ly và secret mồ côi đã quá 7 ngày?')) {
      return;
    }
    setBusy(true);
    setNotice(null);
    try {
      const res = await purgeQuarantine(csrfToken);
      setNotice(
        `Đã dọn ${res.purged_quarantine_dirs} thư mục cách ly, ${res.purged_secret_blobs} secret mồ côi (bỏ qua ${res.skipped_under_grace} mục chưa đủ 7 ngày).`,
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
          <h2 className="text-base font-semibold text-zinc-950">Đối soát lưu trữ &amp; Vùng cách ly</h2>
          <p className="mt-0.5 text-sm text-zinc-600">
            Quét thủ công tệp mồ côi (&ge; 24 giờ), tệp thiếu, secret mồ côi và dọn vùng cách ly (&ge; 7 ngày).
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <AdminButton variant="secondary" size="sm" disabled={busy || !csrfToken} onClick={handlePlan}>
            Quét &amp; Lập kế hoạch
          </AdminButton>
          <AdminButton
            variant="primary"
            size="sm"
            disabled={busy || !csrfToken || !plan || plan.orphan_candidates.length === 0}
            onClick={handleApply}
          >
            Áp dụng cách ly
          </AdminButton>
          <AdminButton variant="danger" size="sm" disabled={busy || !csrfToken} onClick={handlePurge}>
            Dọn cách ly (&ge; 7 ngày)
          </AdminButton>
        </div>
      </div>

      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}

      {diagnostics && (
        <div className="grid grid-cols-1 gap-3 text-sm sm:grid-cols-2 lg:grid-cols-4">
          <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3.5">
            <div className="text-xs font-medium text-zinc-500">Generation</div>
            <div className="mt-1 text-lg font-semibold tabular-nums text-zinc-950">{diagnostics.maintenance_generation}</div>
          </div>
          <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3.5">
            <div className="text-xs font-medium text-zinc-500">Xóa đang chờ</div>
            <div className="mt-1 text-lg font-semibold tabular-nums text-zinc-950">{diagnostics.pending_delete_jobs.length}</div>
          </div>
          <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3.5">
            <div className="text-xs font-medium text-zinc-500">Cách ly (đủ 7d / giữ lại)</div>
            <div className="mt-1 text-lg font-semibold tabular-nums text-zinc-950">
              {diagnostics.purgeable_quarantine_dirs} / {diagnostics.retained_quarantine_dirs}
            </div>
          </div>
          <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3.5">
            <div className="text-xs font-medium text-zinc-500">Secret mồ côi (đủ 7d)</div>
            <div className="mt-1 text-lg font-semibold tabular-nums text-zinc-950">
              {diagnostics.orphan_secret_blobs} ({diagnostics.purgeable_secret_blobs})
            </div>
          </div>
        </div>
      )}

      {plan && (
        <AdminAlert tone="warning">
          Kế hoạch <code className="font-mono text-xs">{plan.plan_id.slice(0, 12)}...</code> — Quét {plan.scanned_records} bản ghi,{' '}
          {plan.scanned_files} tệp; thiếu file: {plan.missing_canonical_files.length}; mồ côi &ge; 24h:{' '}
          {plan.orphan_candidates.length}; mồ côi mới (&lt; 24h): {plan.recent_orphans_under_grace}.
        </AdminAlert>
      )}
    </section>
  );
};
