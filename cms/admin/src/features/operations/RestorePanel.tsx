import React, { useCallback, useEffect, useState } from 'react';
import {
  activateRecovery,
  createRestoreJob,
  getOperationJob,
  getRecoveryStatus,
  uploadRestoreArchive,
  type OperationJob,
  type RecoveryStatus,
} from '../../api/operations.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';

interface RestorePanelProps {
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
}

function makeHexRequestId(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

export const RestorePanel: React.FC<RestorePanelProps> = ({ csrfToken, onApiError }) => {
  const [recovery, setRecovery] = useState<RecoveryStatus | null>(null);
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [passphrase, setPassphrase] = useState('');
  const [reason, setReason] = useState('');
  const [activeJob, setActiveJob] = useState<OperationJob | null>(null);
  const [adminPassword, setAdminPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const loadRecoveryStatus = useCallback(async () => {
    try {
      const status = await getRecoveryStatus();
      setRecovery(status);
    } catch (err) {
      onApiError(err);
    }
  }, [onApiError]);

  useEffect(() => {
    void loadRecoveryStatus();
  }, [loadRecoveryStatus]);

  useEffect(() => {
    if (!activeJob || activeJob.state === 'succeeded' || activeJob.state === 'failed' || activeJob.state === 'cancelled') {
      return;
    }
    const timer = window.setInterval(() => {
      void getOperationJob(activeJob.id)
        .then((job) => {
          setActiveJob(job);
          if (job.state === 'succeeded') {
            setNotice('Đã giải mã và tái dựng site khôi phục vào thư mục riêng thành công.');
          } else if (job.state === 'failed') {
            setErrorMsg(job.error_message || 'Khôi phục thất bại.');
          }
        })
        .catch(() => {});
    }, 1000);
    return () => window.clearInterval(timer);
  }, [activeJob]);

  const handleRestoreUploadAndRun = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!csrfToken) return;
    if (!selectedFile) {
      setErrorMsg('Vui lòng chọn tệp .odbackup.age để khôi phục.');
      return;
    }
    if (!passphrase) {
      setErrorMsg('Vui lòng nhập passphrase giải mã backup.');
      return;
    }
    setBusy(true);
    setErrorMsg(null);
    setNotice(null);
    try {
      const uploaded = await uploadRestoreArchive(csrfToken, selectedFile);
      const job = await createRestoreJob(csrfToken, {
        upload_id: uploaded.upload_id,
        passphrase,
        request_id: makeHexRequestId(),
        reason: reason.trim() || undefined,
      });
      setActiveJob(job);
      setPassphrase('');
      setNotice(`Đã tiếp nhận tác vụ khôi phục ${job.id.slice(0, 12)}...`);
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : 'Lỗi khởi chạy khôi phục.');
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleActivate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!csrfToken || !recovery?.expected_generation) return;
    if (!adminPassword) {
      setErrorMsg('Vui lòng nhập mật khẩu Admin để xác nhận mở khóa.');
      return;
    }
    setBusy(true);
    setErrorMsg(null);
    setNotice(null);
    try {
      const res = await activateRecovery(csrfToken, {
        password: adminPassword,
        expected_generation: recovery.expected_generation,
      });
      setAdminPassword('');
      setNotice(
        `Đã mở khóa safe mode cho generation ${res.generation.slice(0, 12)}. SMTP vẫn giữ ${res.smtp_enabled ? 'bật' : 'tắt'} cho tới khi cấu hình lại.`,
      );
      await loadRecoveryStatus();
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : 'Không thể kích hoạt site.');
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="mt-8 space-y-4 border-t border-zinc-200 pt-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-base font-semibold text-zinc-950">Khôi phục &amp; Safe Mode</h3>
          <p className="mt-0.5 text-sm text-zinc-600">
            Tải lên gói <code className="text-xs">.odbackup.age</code> để dựng vào thư mục riêng, hoặc mở khóa safe mode sau khi khởi chạy root mới.
          </p>
        </div>
        <AdminButton variant="secondary" size="sm" onClick={() => void loadRecoveryStatus()}>
          Làm mới
        </AdminButton>
      </div>

      {recovery && (
        <div className={`space-y-1.5 rounded-xl border p-3.5 text-xs ${recovery.safe_mode ? 'border-amber-200 bg-amber-50 text-amber-900' : 'border-zinc-200 bg-zinc-50 text-zinc-700'}`}>
          <div className="flex flex-wrap items-center gap-2 font-semibold">
            <AdminBadge tone={recovery.safe_mode ? 'warning' : 'success'}>
              {recovery.safe_mode ? 'SAFE MODE' : 'Hoạt động bình thường'}
            </AdminBadge>
            {recovery.safe_mode && <span>Công khai noindex, contact/plugin/SMTP tạm khóa</span>}
          </div>
          <div>Site root: <code className="font-mono">{recovery.site_root}</code></div>
          {recovery.ready_marker && (
            <div>
              READY.json generation: <code className="font-mono">{recovery.ready_marker.job_id}</code> (files: {recovery.ready_marker.file_count})
            </div>
          )}
        </div>
      )}

      {errorMsg && <AdminAlert tone="error">{errorMsg}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}

      {recovery?.safe_mode && recovery.expected_generation && (
        <form onSubmit={handleActivate} className="max-w-xl space-y-3 rounded-xl border border-amber-200 bg-white p-4">
          <div className="text-sm font-semibold text-zinc-900">Kích hoạt vận hành sau khôi phục</div>
          <div className="flex flex-wrap items-end gap-2.5">
            <label className="flex min-w-[220px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>Mật khẩu Admin</span>
              <input
                type="password"
                value={adminPassword}
                onChange={(e) => setAdminPassword(e.target.value)}
                className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
                placeholder="Nhập mật khẩu Admin hiện tại"
              />
            </label>
            <AdminButton type="submit" variant="primary" disabled={busy}>
              Mở khóa Safe Mode
            </AdminButton>
          </div>
        </form>
      )}

      <form onSubmit={handleRestoreUploadAndRun} className="max-w-2xl space-y-3 rounded-xl border border-zinc-200 bg-white p-4">
        <div className="text-sm font-semibold text-zinc-900">Khôi phục từ gói .odbackup.age</div>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>Tệp backup (.odbackup.age)</span>
            <input
              type="file"
              accept=".age,.odbackup.age"
              onChange={(e) => setSelectedFile(e.target.files?.[0] ?? null)}
              className="w-full rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-xs text-zinc-800"
            />
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>Passphrase giải mã</span>
            <input
              type="password"
              value={passphrase}
              onChange={(e) => setPassphrase(e.target.value)}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
              placeholder="Passphrase của gói backup"
            />
          </label>
        </div>
        <div className="flex flex-wrap items-end gap-2.5">
          <label className="flex min-w-[220px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>Ghi chú (tùy chọn)</span>
            <input
              type="text"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
              placeholder="Ví dụ: Khôi phục kiểm thử sang thư mục mới"
            />
          </label>
          <AdminButton type="submit" variant="primary" disabled={busy || !csrfToken}>
            {busy ? 'Đang xử lý...' : 'Tải lên & Khôi phục'}
          </AdminButton>
        </div>
      </form>

      {activeJob && (
        <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3 text-xs text-zinc-700">
          <div>Tác vụ: <code className="font-mono">{activeJob.id}</code> — Trạng thái: <strong>{activeJob.state}</strong></div>
          {activeJob.summary_json && (
            <pre className="mt-1.5 overflow-x-auto text-[11px] text-zinc-600">{activeJob.summary_json}</pre>
          )}
        </div>
      )}
    </section>
  );
};
