import { tStatic, useI18n } from '../../i18n/index.tsx';
import { formatMediaBytes, formatMediaDate } from '../media/library/format.ts';
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

export function BackupsPanel({ csrfToken, onApiError }: BackupsPanelProps) {
  const { t: translate } = useI18n();
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
      setNotice(tStatic('feature.operations.backupspanel.missing_session_csrf_token'));
      return;
    }
    if (passphrase !== confirmPassphrase) {
      setNotice(tStatic('feature.operations.backupspanel.the_confirmation_password_does_not_match'));
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
      setNotice(tStatic('feature.operations.backupspanel.started_backup_job_value1', { value1: job.id.slice(0, 12) }));
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
      setNotice(tStatic('feature.operations.backupspanel.cancelled_job_value1', { value1: updated.id.slice(0, 12) }));
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
      setNotice(tStatic('feature.operations.backupspanel.deleted_backup_package_value1', { value1: id.slice(0, 12) }));
      await refresh();
    } catch (error) {
      onApiError(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="space-y-4" aria-label={translate('feature.operations.backupspanel.encrypted_backup')}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">{translate('feature.operations.backupspanel.encrypted_backup_odbackup_age')}</h2>
          <p className="mt-0.5 text-sm text-zinc-600">
            {data
              ? translate('feature.operations.backupspanel.storing_value1_value2_backup_packages_up_to_value3_receipts', { value1: data.active_artifact_count, value2: data.max_artifacts, value3: data.max_receipts })
              : translate('feature.operations.backupspanel.loading_backup_information')}
          </p>
        </div>
        <AdminButton variant="secondary" size="sm" onClick={() => void refresh()} disabled={busy}>{translate('feature.users.userspanel.refresh')}</AdminButton>
      </div>

      <form onSubmit={handleCreateBackup} className="flex max-w-xl flex-col gap-3">
        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
          <span>{translate('feature.operations.backupspanel.encryption_passphrase_16_128_characters_not_stored_on_the_server')}</span>
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
          <span>{translate('feature.operations.backupspanel.confirm_encryption_passphrase')}</span>
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
          <span>{translate('feature.operations.backupspanel.note_optional')}</span>
          <input
            type="text"
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm font-normal text-zinc-900"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            placeholder={translate('feature.operations.backupspanel.example_before_upgrading_the_theme')}
          />
        </label>
        <div className="flex flex-wrap items-center gap-2 pt-1">
          <AdminButton type="submit" variant="primary" disabled={busy || Boolean(activeJob)}>{translate('feature.operations.backupspanel.create_encrypted_backup')}</AdminButton>
          {activeJob && (activeJob.state === 'queued' || activeJob.state === 'running') && (
            <AdminButton variant="danger" onClick={handleCancel} disabled={busy}>{translate('feature.operations.backupspanel.cancel_job')}{activeJob.state})
            </AdminButton>
          )}
        </div>
      </form>

      {notice && <AdminAlert tone="info">{notice}</AdminAlert>}

      {data && data.artifacts.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>{translate('feature.operations.backupspanel.file')}</th>
              <th>{translate('feature.operations.backupspanel.size')}</th>
              <th>SHA-256</th>
              <th>{translate('feature.operations.backupspanel.created_at')}</th>
              <th>{translate('multilingual.actions')}</th>
            </tr>
          </thead>
          <tbody>
            {data.artifacts.map((art) => (
              <tr key={art.id}>
                <td>{art.filename}</td>
                <td>{formatMediaBytes(art.size_bytes)}</td>
                <td><code>{art.sha256.slice(0, 16)}…</code></td>
                <td>{formatMediaDate(art.created_at)}</td>
                <td className="row-gap">
                  <a className="btn-secondary" href={getBackupDownloadUrl(art.id)} download={art.filename}>{translate('feature.operations.backupspanel.download')}</a>
                  <button
                    type="button"
                    className="btn-danger"
                    onClick={() => void handleDeleteArtifact(art.id)}
                    disabled={busy}
                  >{translate('multilingual.delete')}</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
