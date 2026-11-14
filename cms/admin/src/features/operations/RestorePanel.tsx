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
    <section className="mt-8 border-t border-slate-200 pt-6">
      <div className="mb-4 flex items-center justify-between">
        <div>
          <h3 className="text-base font-semibold text-slate-900">Khôi phục & Safe Mode</h3>
          <p className="text-xs text-slate-600">
            Tải lên gói <code>.odbackup.age</code> để dựng vào thư mục riêng, hoặc mở khóa safe mode sau khi khởi chạy root mới.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void loadRecoveryStatus()}
          className="rounded border border-slate-300 bg-white px-2.5 py-1 text-xs font-medium text-slate-700 hover:bg-slate-50"
        >
          Làm mới
        </button>
      </div>

      {recovery && (
        <div className={`mb-4 rounded border p-3 text-xs ${recovery.safe_mode ? 'border-amber-300 bg-amber-50 text-amber-900' : 'border-slate-200 bg-slate-50 text-slate-700'}`}>
          <div className="font-semibold">
            Trạng thái hiện tại: {recovery.safe_mode ? 'SAFE MODE (công khai noindex, contact/plugin/SMTP tạm khóa)' : 'Hoạt động bình thường'}
          </div>
          <div className="mt-1">Site root: <code>{recovery.site_root}</code></div>
          {recovery.ready_marker && (
            <div className="mt-1">
              READY.json generation: <code>{recovery.ready_marker.job_id}</code> (files: {recovery.ready_marker.file_count})
            </div>
          )}
        </div>
      )}

      {errorMsg && (
        <div className="mb-3 rounded border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700">
          {errorMsg}
        </div>
      )}
      {notice && (
        <div className="mb-3 rounded border border-emerald-200 bg-emerald-50 px-3 py-2 text-xs text-emerald-700">
          {notice}
        </div>
      )}

      {recovery?.safe_mode && recovery.expected_generation && (
        <form onSubmit={handleActivate} className="mb-5 rounded border border-amber-300 bg-white p-3">
          <div className="mb-2 text-xs font-semibold text-slate-800">Kích hoạt vận hành sau khôi phục</div>
          <div className="flex flex-wrap items-end gap-2">
            <label className="flex-1 text-xs text-slate-700">
              Mật khẩu Admin
              <input
                type="password"
                value={adminPassword}
                onChange={(e) => setAdminPassword(e.target.value)}
                className="mt-1 block w-full rounded border border-slate-300 px-2 py-1 text-xs"
                placeholder="Nhập mật khẩu Admin hiện tại"
              />
            </label>
            <button
              type="submit"
              disabled={busy}
              className="rounded bg-amber-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-amber-700 disabled:opacity-50"
            >
              Mở khóa Safe Mode
            </button>
          </div>
        </form>
      )}

      <form onSubmit={handleRestoreUploadAndRun} className="rounded border border-slate-200 bg-white p-3">
        <div className="mb-2 text-xs font-semibold text-slate-800">Khôi phục từ gói .odbackup.age</div>
        <div className="grid gap-2 sm:grid-cols-2">
          <label className="text-xs text-slate-700">
            Tệp backup (.odbackup.age)
            <input
              type="file"
              accept=".age,.odbackup.age"
              onChange={(e) => setSelectedFile(e.target.files?.[0] ?? null)}
              className="mt-1 block w-full text-xs"
            />
          </label>
          <label className="text-xs text-slate-700">
            Passphrase giải mã
            <input
              type="password"
              value={passphrase}
              onChange={(e) => setPassphrase(e.target.value)}
              className="mt-1 block w-full rounded border border-slate-300 px-2 py-1 text-xs"
              placeholder="Passphrase của gói backup"
            />
          </label>
        </div>
        <div className="mt-2 flex flex-wrap items-end gap-2">
          <label className="flex-1 text-xs text-slate-700">
            Ghi chú (tùy chọn)
            <input
              type="text"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              className="mt-1 block w-full rounded border border-slate-300 px-2 py-1 text-xs"
              placeholder="Ví dụ: Khôi phục kiểm thử sang thư mục mới"
            />
          </label>
          <button
            type="submit"
            disabled={busy || !csrfToken}
            className="rounded bg-slate-900 px-3 py-1.5 text-xs font-medium text-white hover:bg-slate-800 disabled:opacity-50"
          >
            {busy ? 'Đang xử lý...' : 'Tải lên & Khôi phục'}
          </button>
        </div>
      </form>

      {activeJob && (
        <div className="mt-3 rounded border border-slate-200 bg-slate-50 p-2.5 text-xs text-slate-700">
          <div>Tác vụ: <code>{activeJob.id}</code> — Trạng thái: <strong>{activeJob.state}</strong></div>
          {activeJob.summary_json && (
            <pre className="mt-1 overflow-x-auto text-[11px] text-slate-600">{activeJob.summary_json}</pre>
          )}
        </div>
      )}
    </section>
  );
};
