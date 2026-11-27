import { __ } from '../i18n/index.tsx';
import { requestJson } from './client.ts';

export interface MissingCanonicalDiagnostic {
  media_id: string;
  logical_key: string;
  lifecycle_state: string;
}

export type ReconcileOrphanKind = 'canonical' | 'variant';

export interface ReconcileOrphanCandidate {
  media_id: string;
  logical_key: string;
  kind?: ReconcileOrphanKind;
  size_bytes: number;
  modified_unix_secs: number;
  sha256: string;
}

export interface ReconcilePlan {
  plan_id: string;
  generation: number;
  version: number;
  digest: string;
  created_at_unix: number;
  scanned_records: number;
  scanned_files: number;
  missing_canonical_files: MissingCanonicalDiagnostic[];
  orphan_candidates: ReconcileOrphanCandidate[];
  recent_orphans_under_grace: number;
  pending_delete_jobs: number;
  purgeable_quarantine_count: number;
  retained_quarantine_count: number;
  orphan_secret_count: number;
  purgeable_secret_count: number;
}

export interface OperationJob {
  id: string;
  kind:
    | 'backup'
    | 'restore'
    | 'reconcile'
    | 'purge'
    | 'media_delete'
    | 'media_regenerate'
    | 'search_reindex';
  state: 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled' | 'interrupted';
  version: number;
  actor_user_id: string | null;
  reason: string | null;
  summary_json: string | null;
  error_code: string | null;
  error_message: string | null;
  requested_at: string;
  started_at: string | null;
  finished_at: string | null;
  updated_at: string;
}

export interface ApplyReconcileResult {
  job: OperationJob;
  quarantined_files: number;
  generation: number;
}

export interface PurgeQuarantineResult {
  job: OperationJob;
  purged_quarantine_dirs: number;
  purged_secret_blobs: number;
  skipped_under_grace: number;
  generation: number;
}

export interface RedactedSecretDiagnostic {
  ref_prefix: string;
  purgeable: boolean;
  age_hours: number;
}

export interface OperationsDiagnostics {
  maintenance_generation: number;
  pending_delete_jobs: Array<{
    job_id: string;
    media_id: string;
    phase: string;
    error_code: string | null;
  }>;
  total_quarantine_dirs: number;
  purgeable_quarantine_dirs: number;
  retained_quarantine_dirs: number;
  total_secret_blobs: number;
  orphan_secret_blobs: number;
  purgeable_secret_blobs: number;
  orphan_secrets_redacted: RedactedSecretDiagnostic[];
  recent_jobs: OperationJob[];
}

export function getOperationsDiagnostics(): Promise<OperationsDiagnostics> {
  return requestJson<OperationsDiagnostics>('/api/admin/operations/diagnostics', {
    method: 'GET',
    defaultErrorMessage: __("Could not load operational diagnostics."),
  });
}

export function planReconcile(csrfToken: string): Promise<ReconcilePlan> {
  return requestJson<ReconcilePlan>('/api/admin/operations/reconcile/plan', {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: __("Could not plan reconciliation."),
  });
}

export function applyReconcile(
  csrfToken: string,
  plan: ReconcilePlan,
): Promise<ApplyReconcileResult> {
  return requestJson<ApplyReconcileResult>('/api/admin/operations/reconcile/apply', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify({
      plan_id: plan.plan_id,
      expected_generation: plan.generation,
      expected_version: plan.version,
      digest: plan.digest,
      confirm: true,
    }),
    defaultErrorMessage: __("Could not apply the reconciliation plan."),
  });
}

export function purgeQuarantine(csrfToken: string): Promise<PurgeQuarantineResult> {
  return requestJson<PurgeQuarantineResult>('/api/admin/operations/quarantine/purge', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify({ confirm: true }),
    defaultErrorMessage: __("Could not purge quarantine."),
  });
}

export interface BackupArtifactView {
  id: string;
  filename: string;
  size_bytes: number;
  sha256: string;
  schema_version: number;
  file_count: number;
  created_at: string;
  version: number;
}

export interface BackupListResponse {
  artifacts: BackupArtifactView[];
  jobs: OperationJob[];
  max_artifacts: number;
  max_receipts: number;
  active_artifact_count: number;
}

export function listBackups(limit = 20): Promise<BackupListResponse> {
  return requestJson<BackupListResponse>(`/api/admin/operations/backups?limit=${limit}`, {
    method: 'GET',
    defaultErrorMessage: __("Could not load backups."),
  });
}

export function createBackup(
  csrfToken: string,
  input: { passphrase: string; confirm_passphrase: string; request_id: string; reason?: string },
): Promise<OperationJob> {
  return requestJson<OperationJob>('/api/admin/operations/backups', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: __("Could not start the backup job."),
  });
}

export function getOperationJob(id: string): Promise<OperationJob> {
  return requestJson<OperationJob>(`/api/admin/operations/jobs/${encodeURIComponent(id)}`, {
    method: 'GET',
    defaultErrorMessage: __("Could not load the job status."),
  });
}

export function cancelOperationJob(
  csrfToken: string,
  id: string,
  expectedVersion?: number,
): Promise<OperationJob> {
  return requestJson<OperationJob>(`/api/admin/operations/jobs/${encodeURIComponent(id)}/cancel`, {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(expectedVersion != null ? { expected_version: expectedVersion } : {}),
    defaultErrorMessage: __("Could not cancel the job."),
  });
}

export function deleteBackupArtifact(csrfToken: string, id: string): Promise<OperationJob> {
  return requestJson<OperationJob>(`/api/admin/operations/backups/${encodeURIComponent(id)}`, {
    method: 'DELETE',
    csrfToken,
    defaultErrorMessage: __("Could not delete the backup."),
  });
}

export function getBackupDownloadUrl(id: string): string {
  return `/api/admin/operations/backups/${encodeURIComponent(id)}/file`;
}


export interface RestoreUploadResult {
  upload_id: string;
  size_bytes: number;
  sha256: string;
}

export interface RecoveryStatus {
  safe_mode: boolean;
  smtp_enabled: boolean;
  ready_marker: {
    version: number;
    job_id: string;
    schema_version: number;
    db_sha256: string;
    file_count: number;
    recovery_safe_mode: boolean;
  } | null;
  site_root: string;
  expected_generation: string | null;
}

export interface ActivateRecoveryResult {
  activated: boolean;
  safe_mode: boolean;
  generation: string;
  smtp_enabled: boolean;
}

export function uploadRestoreArchive(
  csrfToken: string,
  file: File,
): Promise<RestoreUploadResult> {
  return requestJson<RestoreUploadResult>('/api/admin/operations/restore-uploads', {
    method: 'POST',
    csrfToken,
    contentType: 'application/octet-stream',
    body: file,
    defaultErrorMessage: __("Could not upload the backup package."),
  });
}

export function createRestoreJob(
  csrfToken: string,
  input: { upload_id: string; passphrase: string; request_id: string; reason?: string },
): Promise<OperationJob> {
  return requestJson<OperationJob>('/api/admin/operations/restores', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: __("Could not start the restore job."),
  });
}

export function getRecoveryStatus(): Promise<RecoveryStatus> {
  return requestJson<RecoveryStatus>('/api/admin/operations/recovery/status', {
    method: 'GET',
    defaultErrorMessage: __("Could not load safe mode status."),
  });
}

export function activateRecovery(
  csrfToken: string,
  input: { password: string; expected_generation: string },
): Promise<ActivateRecoveryResult> {
  return requestJson<ActivateRecoveryResult>('/api/admin/operations/recovery/activate', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify(input),
    defaultErrorMessage: __("Could not activate the site after restore."),
  });
}

export interface SearchIndexStatus {
  indexed_documents: number;
  eligible_published_entries: number;
  in_sync: boolean;
  maintenance_active: boolean;
  last_job: OperationJob | null;
}

export interface SearchReindexResult {
  job: OperationJob;
  indexed_documents: number;
  eligible_published_entries: number;
  in_sync: boolean;
  duration_ms: number;
}

export function getSearchIndexStatus(): Promise<SearchIndexStatus> {
  return requestJson<SearchIndexStatus>('/api/admin/tools/search/status', {
    method: 'GET',
    defaultErrorMessage: __("Could not load search index status."),
  });
}

export function reindexSearchIndex(csrfToken: string): Promise<SearchReindexResult> {
  return requestJson<SearchReindexResult>('/api/admin/tools/search/reindex', {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: __("Could not rebuild search index."),
  });
}
