import { tStatic, useI18n } from '../../i18n/index.tsx';
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
  const { t: translate } = useI18n();
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
            setNotice(tStatic('feature.operations.restorepanel.successfully_decrypted_and_rebuilt_the_restored_site_in_a_separate_directory'));
          } else if (job.state === 'failed') {
            setErrorMsg(job.error_message || tStatic('feature.operations.restorepanel.restore_failed'));
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
      setErrorMsg(tStatic('feature.operations.restorepanel.select_an_odbackup_age_file_to_restore'));
      return;
    }
    if (!passphrase) {
      setErrorMsg(tStatic('feature.operations.restorepanel.enter_the_backup_decryption_passphrase'));
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
      setNotice(tStatic('feature.operations.restorepanel.restore_job_value1_accepted', { value1: job.id.slice(0, 12) }));
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : tStatic('feature.operations.restorepanel.could_not_start_the_restore'));
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  const handleActivate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!csrfToken || !recovery?.expected_generation) return;
    if (!adminPassword) {
      setErrorMsg(tStatic('feature.operations.restorepanel.enter_the_administrator_password_to_confirm_unlocking'));
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
        tStatic('feature.operations.restorepanel.unlocked_safe_mode_for_generation_value1_smtp_remains_value2_until_reconfigured', { value1: res.generation.slice(0, 12), value2: res.smtp_enabled ? tStatic('feature.operations.restorepanel.enabled') : tStatic('feature.operations.restorepanel.disabled') }),
      );
      await loadRecoveryStatus();
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : tStatic('feature.operations.restorepanel.could_not_activate_the_site'));
      onApiError(err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-base font-semibold text-zinc-950">{translate('feature.operations.restorepanel.restore_safe_mode')}</h3>
          <p className="mt-0.5 text-sm text-zinc-600">{translate('feature.operations.restorepanel.upload_the_package')}{' '}<code className="text-xs">.odbackup.age</code>{' '}{translate('feature.operations.restorepanel.to_rebuild_in_a_separate_directory_or_unlock_safe_mode_after_starting_the_new_ro')}</p>
        </div>
        <AdminButton variant="secondary" size="sm" onClick={() => void loadRecoveryStatus()}>{translate('feature.users.userspanel.refresh')}</AdminButton>
      </div>

      {recovery && (
        <div className={`space-y-1.5 rounded-xl border p-3.5 text-xs ${recovery.safe_mode ? 'border-amber-200 bg-amber-50 text-amber-900' : 'border-zinc-200 bg-zinc-50 text-zinc-700'}`}>
          <div className="flex flex-wrap items-center gap-2 font-semibold">
            <AdminBadge tone={recovery.safe_mode ? 'warning' : 'success'}>
              {recovery.safe_mode ? translate('feature.operations.restorepanel.safe_mode') : translate('feature.operations.restorepanel.normal_operation')}
            </AdminBadge>
            {recovery.safe_mode && <span>{translate('feature.operations.restorepanel.public_pages_are_noindex_contact_plugins_and_smtp_are_temporarily_disabled')}</span>}
          </div>
          <div>{translate('feature.operations.restorepanel.site_root')}{' '}<code className="font-mono">{recovery.site_root}</code></div>
          {recovery.ready_marker && (
            <div>{translate('feature.operations.restorepanel.ready_json_generation')}{' '}<code className="font-mono">{recovery.ready_marker.job_id}</code>{' '}{translate('feature.operations.restorepanel.files')}{' '}{recovery.ready_marker.file_count})
            </div>
          )}
        </div>
      )}

      {errorMsg && <AdminAlert tone="error">{errorMsg}</AdminAlert>}
      {notice && <AdminAlert tone="success">{notice}</AdminAlert>}

      {recovery?.safe_mode && recovery.expected_generation && (
        <form onSubmit={handleActivate} className="max-w-xl space-y-3 rounded-xl border border-amber-200 bg-white p-4">
          <div className="text-sm font-semibold text-zinc-900">{translate('feature.operations.restorepanel.activate_operation_after_restore')}</div>
          <div className="flex flex-wrap items-end gap-2.5">
            <label className="flex min-w-[220px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-700">
              <span>{translate('feature.operations.restorepanel.administrator_password')}</span>
              <input
                type="password"
                value={adminPassword}
                onChange={(e) => setAdminPassword(e.target.value)}
                className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
                placeholder={translate('feature.operations.restorepanel.enter_the_current_administrator_password')}
              />
            </label>
            <AdminButton type="submit" variant="primary" disabled={busy}>{translate('feature.operations.restorepanel.unlock_safe_mode')}</AdminButton>
          </div>
        </form>
      )}

      <form onSubmit={handleRestoreUploadAndRun} className="max-w-2xl space-y-3 rounded-xl border border-zinc-200 bg-white p-4">
        <div className="text-sm font-semibold text-zinc-900">{translate('feature.operations.restorepanel.restore_from_odbackup_age_package')}</div>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{translate('feature.operations.restorepanel.backup_file_odbackup_age')}</span>
            <input
              type="file"
              accept=".age,.odbackup.age"
              onChange={(e) => setSelectedFile(e.target.files?.[0] ?? null)}
              className="w-full rounded-md border border-zinc-300 bg-white px-2.5 py-1.5 text-xs text-zinc-800"
            />
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{translate('feature.operations.restorepanel.decryption_passphrase')}</span>
            <input
              type="password"
              value={passphrase}
              onChange={(e) => setPassphrase(e.target.value)}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
              placeholder={translate('feature.operations.restorepanel.backup_package_passphrase')}
            />
          </label>
        </div>
        <div className="flex flex-wrap items-end gap-2.5">
          <label className="flex min-w-[220px] flex-1 flex-col gap-1 text-xs font-medium text-zinc-700">
            <span>{translate('feature.operations.restorepanel.note_optional')}</span>
            <input
              type="text"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900"
              placeholder={translate('feature.operations.restorepanel.example_test_restore_to_a_new_directory')}
            />
          </label>
          <AdminButton type="submit" variant="primary" disabled={busy || !csrfToken}>
            {busy ? translate('feature.operations.restorepanel.processing') : translate('feature.operations.restorepanel.upload_restore')}
          </AdminButton>
        </div>
      </form>

      {activeJob && (
        <div className="rounded-xl border border-zinc-200 bg-zinc-50 p-3 text-xs text-zinc-700">
          <div>{translate('feature.operations.restorepanel.job')}{' '}<code className="font-mono">{activeJob.id}</code>{' '}{translate('feature.operations.restorepanel.status')}{' '}<strong>{activeJob.state}</strong></div>
          {activeJob.summary_json && (
            <pre className="mt-1.5 overflow-x-auto text-[11px] text-zinc-600">{activeJob.summary_json}</pre>
          )}
        </div>
      )}
    </section>
  );
};
