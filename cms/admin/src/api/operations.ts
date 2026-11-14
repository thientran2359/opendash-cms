import { requestJson } from './client.ts';

export interface MissingCanonicalDiagnostic {
  media_id: string;
  logical_key: string;
  lifecycle_state: string;
}

export interface ReconcileOrphanCandidate {
  media_id: string;
  logical_key: string;
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
  kind: 'backup' | 'restore' | 'reconcile' | 'purge' | 'media_delete';
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
    defaultErrorMessage: 'Không tải được chẩn đoán vận hành.',
  });
}

export function planReconcile(csrfToken: string): Promise<ReconcilePlan> {
  return requestJson<ReconcilePlan>('/api/admin/operations/reconcile/plan', {
    method: 'POST',
    csrfToken,
    defaultErrorMessage: 'Không thể lập kế hoạch đối soát.',
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
    defaultErrorMessage: 'Không thể áp dụng kế hoạch đối soát.',
  });
}

export function purgeQuarantine(csrfToken: string): Promise<PurgeQuarantineResult> {
  return requestJson<PurgeQuarantineResult>('/api/admin/operations/quarantine/purge', {
    method: 'POST',
    csrfToken,
    contentType: 'application/json',
    body: JSON.stringify({ confirm: true }),
    defaultErrorMessage: 'Không thể dọn vùng cách ly.',
  });
}
