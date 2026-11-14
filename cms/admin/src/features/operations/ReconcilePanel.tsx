import React, { useEffect, useState } from 'react';
import {
  applyReconcile,
  getOperationsDiagnostics,
  planReconcile,
  purgeQuarantine,
  type OperationsDiagnostics,
  type ReconcilePlan,
} from '../../api/operations.ts';

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
    <section className="mt-8 border-t border-slate-200 pt-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-base font-semibold text-slate-900">Đối soát lưu trữ &amp; Vùng cách ly</h3>
          <p className="text-sm text-slate-500">
            Quét thủ công tệp mồ côi (&ge; 24 giờ), tệp thiếu, secret mồ côi và dọn vùng cách ly (&ge; 7 ngày).
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            disabled={busy || !csrfToken}
            onClick={handlePlan}
            className="rounded-lg border border-slate-300 bg-white px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50"
          >
            Quét &amp; Lập kế hoạch
          </button>
          <button
            type="button"
            disabled={busy || !csrfToken || !plan || plan.orphan_candidates.length === 0}
            onClick={handleApply}
            className="rounded-lg bg-amber-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-amber-700 disabled:opacity-50"
          >
            Áp dụng cách ly
          </button>
          <button
            type="button"
            disabled={busy || !csrfToken}
            onClick={handlePurge}
            className="rounded-lg bg-rose-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-rose-700 disabled:opacity-50"
          >
            Dọn cách ly (&ge; 7 ngày)
          </button>
        </div>
      </div>

      {notice && (
        <div className="mt-3 rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-800">
          {notice}
        </div>
      )}

      {diagnostics && (
        <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-4 text-sm">
          <div className="rounded-lg border border-slate-200 bg-slate-50 p-3">
            <div className="text-slate-500">Generation</div>
            <div className="font-semibold text-slate-900">{diagnostics.maintenance_generation}</div>
          </div>
          <div className="rounded-lg border border-slate-200 bg-slate-50 p-3">
            <div className="text-slate-500">Xóa đang chờ</div>
            <div className="font-semibold text-slate-900">{diagnostics.pending_delete_jobs.length}</div>
          </div>
          <div className="rounded-lg border border-slate-200 bg-slate-50 p-3">
            <div className="text-slate-500">Cách ly (đủ 7d / giữ lại)</div>
            <div className="font-semibold text-slate-900">
              {diagnostics.purgeable_quarantine_dirs} / {diagnostics.retained_quarantine_dirs}
            </div>
          </div>
          <div className="rounded-lg border border-slate-200 bg-slate-50 p-3">
            <div className="text-slate-500">Secret mồ côi (đủ 7d)</div>
            <div className="font-semibold text-slate-900">
              {diagnostics.orphan_secret_blobs} ({diagnostics.purgeable_secret_blobs})
            </div>
          </div>
        </div>
      )}

      {plan && (
        <div className="mt-4 rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm text-amber-900">
          <div>
            Kế hoạch <code>{plan.plan_id.slice(0, 12)}...</code> — Quét {plan.scanned_records} bản ghi,{' '}
            {plan.scanned_files} tệp; thiếu file: {plan.missing_canonical_files.length}; mồ côi &ge; 24h:{' '}
            {plan.orphan_candidates.length}; mồ côi mới (&lt; 24h): {plan.recent_orphans_under_grace}.
          </div>
        </div>
      )}
    </section>
  );
};
