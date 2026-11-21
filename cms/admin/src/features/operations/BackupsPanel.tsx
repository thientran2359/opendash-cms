import { useCallback, useEffect, useState } from 'react';
import {
  cancelOperationJob,
  createBackup,
  deleteBackupArtifact,
  getBackupDownloadUrl,
  getOperationJob,
  listBackups,
  type BackupListResponse,
  type OperationJob,
} from '../../api/operations.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';

interface BackupsPanelProps {
  csrfToken: string | null;
  onApiError: (error: unknown) => void;
}

function generateRequestId64Hex(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;
}

export function BackupsPanel({ csrfToken, onApiError }: BackupsPanelProps) {
  const [data, setData] = useState<BackupListResponse | null>(null);
  const [passphrase, setPassphrase] = useState('');
  const [confirmPassphrase, setConfirmPassphrase] = useState('');
  const [reason, setReason] = useState('');
  const [activeJob, setActiveJob] = useState<OperationJob | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const res = await listBackups(20);
      setData(res);
      const running = res.jobs.find((j) => j.state === 'queued' || j.state === 'running') ?? null;
      setActiveJob(running);
    } catch (error) {
      onApiError(error);
    }
  }, [onApiError]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!activeJob || (activeJob.state !== 'queued' && activeJob.state !== 'running')) {
      return;
    }
    const timer = window.setInterval(async () => {
      try {
        const updated = await getOperationJob(activeJob.id);
        setActiveJob(updated);
        if (updated.state !== 'queued' && updated.state !== 'running') {
          await refresh();
        }
      } catch (error) {
        onApiError(error);
      }
    }, 1000);
    return () => window.clearInterval(timer);
  }, [activeJob, onApiError, refresh]);

  const handleCreateBackup = async (e: React.FormEvent) => {
    e.preventDefault();
    setNotice(null);
    if (!csrfToken) {
      setNotice('Thiếu CSRF token phiên đăng nhập.');
      return;
    }
    if (passphrase !== confirmPassphrase) {
      setNotice('Mật khẩu xác nhận không khớp.');
      return;
    }
    setBusy(true);
    try {
      const requestId = generateRequestId64Hex();
      const job = await createBackup(csrfToken, {
        passphrase,
        confirm_passphrase: confirmPassphrase,
        request_id: requestId,
        reason: reason.trim() || undefined,
      });
      setPassphrase('');
      setConfirmPassphrase('');
      setReason('');
      setActiveJob(job);
      setNotice(`Đã khởi tạo tác vụ sao lưu ${job.id.slice(0, 12)}…`);
      await refresh();
    } catch (error) {
      onApiError(error);
    } finally {
      setBusy(false);
    }
  };

  const handleCancel = async () => {
    if (!activeJob || !csrfToken) return;
    setBusy(true);
    try {
      const updated = await cancelOperationJob(csrfToken, activeJob.id, activeJob.version);
      setActiveJob(updated);
      setNotice(`Đã hủy tác vụ ${updated.id.slice(0, 12)}…`);
      await refresh();
    } catch (error) {
      onApiError(error);
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteArtifact = async (id: string) => {
    if (!csrfToken) return;
    setBusy(true);
    try {
      await deleteBackupArtifact(csrfToken, id);
      setNotice(`Đã xóa gói sao lưu ${id.slice(0, 12)}…`);
      await refresh();
    } catch (error) {
      onApiError(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="space-y-4" aria-label="Sao lưu mã hóa">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">Sao lưu mã hóa (.odbackup.age)</h2>
          <p className="mt-0.5 text-sm text-zinc-600">
            {data
              ? `Đang lưu ${data.active_artifact_count} / ${data.max_artifacts} gói sao lưu · tối đa ${data.max_receipts} biên nhận.`
              : 'Đang tải thông tin sao lưu…'}
          </p>
        </div>
        <AdminButton variant="secondary" size="sm" onClick={() => void refresh()} disabled={busy}>
          Làm mới
        </AdminButton>
      </div>

      <form onSubmit={handleCreateBackup} className="flex max-w-xl flex-col gap-3">
        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
          <span>Mật khẩu mã hóa (16–128 ký tự, không lưu trên máy chủ)</span>
          <input
            type="password"
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm font-normal text-zinc-900"
            value={passphrase}
            onChange={(e) => setPassphrase(e.target.value)}
            autoComplete="new-password"
            required
          />
        </label>
        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
          <span>Xác nhận mật khẩu mã hóa</span>
          <input
            type="password"
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm font-normal text-zinc-900"
            value={confirmPassphrase}
            onChange={(e) => setConfirmPassphrase(e.target.value)}
            autoComplete="new-password"
            required
          />
        </label>
        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
          <span>Ghi chú (tùy chọn)</span>
          <input
            type="text"
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm font-normal text-zinc-900"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            placeholder="Ví dụ: Trước khi nâng cấp theme"
          />
        </label>
        <div className="flex flex-wrap items-center gap-2 pt-1">
          <AdminButton type="submit" variant="primary" disabled={busy || Boolean(activeJob)}>
            Tạo bản sao lưu mã hóa
          </AdminButton>
          {activeJob && (activeJob.state === 'queued' || activeJob.state === 'running') && (
            <AdminButton variant="danger" onClick={handleCancel} disabled={busy}>
              Hủy tác vụ ({activeJob.state})
            </AdminButton>
          )}
        </div>
      </form>

      {notice && <AdminAlert tone="info">{notice}</AdminAlert>}

      {data && data.artifacts.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>Tệp</th>
              <th>Kích thước</th>
              <th>SHA-256</th>
              <th>Tạo lúc</th>
              <th>Thao tác</th>
            </tr>
          </thead>
          <tbody>
            {data.artifacts.map((art) => (
              <tr key={art.id}>
                <td>{art.filename}</td>
                <td>{formatBytes(art.size_bytes)}</td>
                <td><code>{art.sha256.slice(0, 16)}…</code></td>
                <td>{art.created_at}</td>
                <td className="row-gap">
                  <a className="btn-secondary" href={getBackupDownloadUrl(art.id)} download={art.filename}>
                    Tải xuống
                  </a>
                  <button
                    type="button"
                    className="btn-danger"
                    onClick={() => void handleDeleteArtifact(art.id)}
                    disabled={busy}
                  >
                    Xóa
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
