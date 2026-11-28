import { __, useI18n } from '../../i18n/index.tsx';
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
import { ConfirmDialog } from '../../components/ConfirmDialog.tsx';

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
  const { __ } = useI18n();
  const [data, setData] = useState<BackupListResponse | null>(null);
  const [passphrase, setPassphrase] = useState('');
  const [confirmPassphrase, setConfirmPassphrase] = useState('');
  const [reason, setReason] = useState('');
  const [activeJob, setActiveJob] = useState<OperationJob | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<{ id: string; filename: string } | null>(null);

  const reportError = useCallback((error: unknown, fallback: string) => {
    setErrorMessage(error instanceof Error ? error.message : fallback);
    onApiError(error);
  }, [onApiError]);

  const refresh = useCallback(async () => {
    try {
      setErrorMessage(null);
      const res = await listBackups(20);
      setData(res);
      const running = res.jobs.find((j) => j.state === 'queued' || j.state === 'running') ?? null;
      setActiveJob(running);
    } catch (error) {
      reportError(error, __("Could not load backups."));
    }
  }, [reportError]);

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
      setNotice(__("Missing session CSRF token."));
      return;
    }
    if (passphrase !== confirmPassphrase) {
      setNotice(__("The confirmation password does not match."));
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
      setNotice(__("Started backup job {{value1}}…", { value1: job.id.slice(0, 12) }));
      await refresh();
    } catch (error) {
      reportError(error, __("Could not create backup."));
    } finally {
      setBusy(false);
    }
  };

  const handleCancel = async () => {
    if (!activeJob || !csrfToken) return;
    setBusy(true);
    setErrorMessage(null);
    try {
      const updated = await cancelOperationJob(csrfToken, activeJob.id, activeJob.version);
      setActiveJob(updated);
      setNotice(__("Cancelled job {{value1}}…", { value1: updated.id.slice(0, 12) }));
      await refresh();
    } catch (error) {
      reportError(error, __("Could not cancel backup job."));
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteArtifact = async (id: string) => {
    if (!csrfToken) return;
    setBusy(true);
    setErrorMessage(null);
    try {
      await deleteBackupArtifact(csrfToken, id);
      setNotice(__("Deleted backup package {{value1}}…", { value1: id.slice(0, 12) }));
      await refresh();
    } catch (error) {
      reportError(error, __("Could not delete backup package."));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="space-y-4" aria-label={__("Encrypted backup")}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold text-zinc-950">{__("Encrypted backup (.odbackup.age)")}</h2>
          <p className="mt-0.5 text-sm text-zinc-600">
            {data
              ? __("Storing {{value1}} / {{value2}} backup packages · up to {{value3}} receipts.", { value1: data.active_artifact_count, value2: data.max_artifacts, value3: data.max_receipts })
              : __("Loading backup information…")}
          </p>
        </div>
        <AdminButton variant="secondary" size="sm" onClick={() => void refresh()} disabled={busy}>{__("Refresh")}</AdminButton>
      </div>

      <form onSubmit={handleCreateBackup} className="flex max-w-xl flex-col gap-3">
        <label className="flex flex-col gap-1 text-sm font-medium text-zinc-800">
          <span>{__("Encryption passphrase (16–128 characters, not stored on the server)")}</span>
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
          <span>{__("Confirm encryption passphrase")}</span>
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
          <span>{__("Note (optional)")}</span>
          <input
            type="text"
            className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm font-normal text-zinc-900"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            placeholder={__("Example: Before upgrading the theme")}
          />
        </label>
        <div className="flex flex-wrap items-center gap-2 pt-1">
          <AdminButton type="submit" variant="primary" disabled={busy || Boolean(activeJob)}>{__("Create encrypted backup")}</AdminButton>
          {activeJob && (activeJob.state === 'queued' || activeJob.state === 'running') && (
            <AdminButton variant="danger" onClick={handleCancel} disabled={busy}>
              {__("Cancel job ({{state}})", { state: activeJob.state })}
            </AdminButton>
          )}
        </div>
      </form>

      {errorMessage && <AdminAlert tone="error">{errorMessage}</AdminAlert>}
      {notice && <AdminAlert tone="info">{notice}</AdminAlert>}

      {data && data.artifacts.length > 0 && (
        <div className="overflow-x-auto rounded-lg border border-zinc-200 bg-white">
          <table className="min-w-full divide-y divide-zinc-200 text-left text-sm">
            <thead className="bg-zinc-50 text-xs font-semibold uppercase tracking-wide text-zinc-600">
              <tr>
                <th className="px-3 py-2.5">{__("File")}</th>
                <th className="px-3 py-2.5">{__("Size")}</th>
                <th className="px-3 py-2.5">SHA-256</th>
                <th className="px-3 py-2.5">{__("Created at")}</th>
                <th className="px-3 py-2.5 text-right">{__("Actions")}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-zinc-100 text-zinc-800">
              {data.artifacts.map((art) => (
                <tr key={art.id} className="hover:bg-zinc-50/70">
                  <td className="whitespace-nowrap px-3 py-2.5 font-medium text-zinc-900">{art.filename}</td>
                  <td className="whitespace-nowrap px-3 py-2.5 text-zinc-600">{formatMediaBytes(art.size_bytes)}</td>
                  <td className="whitespace-nowrap px-3 py-2.5">
                    <code className="rounded bg-zinc-100 px-1.5 py-0.5 font-mono text-xs text-zinc-700">{art.sha256.slice(0, 16)}…</code>
                  </td>
                  <td className="whitespace-nowrap px-3 py-2.5 text-zinc-600">{formatMediaDate(art.created_at)}</td>
                  <td className="whitespace-nowrap px-3 py-2.5 text-right">
                    <div className="inline-flex items-center justify-end gap-2">
                      <a
                        className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 no-underline transition-colors hover:bg-zinc-50"
                        href={getBackupDownloadUrl(art.id)}
                        download={art.filename}
                      >
                        {__("Download")}
                      </a>
                      <AdminButton
                        variant="danger"
                        size="sm"
                        onClick={() => setDeleteTarget({ id: art.id, filename: art.filename })}
                        disabled={busy}
                      >
                        {__("Delete")}
                      </AdminButton>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <ConfirmDialog
        open={Boolean(deleteTarget)}
        title={__("Delete backup package?")}
        description={__("Permanently delete backup file “{{filename}}”? This cannot be undone.", { filename: deleteTarget?.filename ?? '' })}
        confirmLabel={busy ? __("Deleting…") : __("Delete backup")}
        busy={busy}
        testId="confirm-delete-backup-dialog"
        onCancel={() => setDeleteTarget(null)}
        onConfirm={() => {
          if (!deleteTarget) return;
          const targetId = deleteTarget.id;
          setDeleteTarget(null);
          void handleDeleteArtifact(targetId);
        }}
      />
    </section>
  );
}
