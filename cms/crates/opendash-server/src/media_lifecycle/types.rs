//! Journaled media deletion, quarantine moves, and crash recovery (M8-S02-T01).

use super::*;

pub(super) use std::collections::{HashMap, HashSet};

pub(super) use std::path::{Path, PathBuf};

pub(super) use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) use axum::http::{HeaderMap, header};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use sha2::{Digest, Sha256};

pub(super) use sqlx::SqlitePool;

pub(super) use tokio::fs;

pub(super) use crate::error::{ApiError, ApiResult};

pub(super) use crate::media::MediaStore;

pub(super) use crate::operations::{ExclusiveOpsContext, MutationLease};

pub use crate::storage::media_lifecycle::{
    MediaDeleteJob, MediaDeletePhase, TombstoneError, get_delete_job, tombstone_media_in_tx,
};

pub(super) use crate::storage::media_refs::{
    collect_referenced_secret_refs, count_reconcile_records_bounded, is_media_unreferenced,
    load_media_inventory_bounded,
};

pub(super) use crate::storage::operations::{
    JobCreateInput, JobTransitionInput, MAX_RETAINED_OPERATION_RECEIPTS, OperationJob,
    OperationKind, OperationState, bump_maintenance_generation, create_job, get_job,
    get_maintenance_generation, list_jobs, prune_old_receipts, transition_job_cas,
};

pub const MAX_RECONCILE_RECORDS: usize = 10_000;

pub const MAX_RECONCILE_FILES: usize = 20_000;

pub const ORPHAN_GRACE_SECS: u64 = 24 * 3600;

pub const PURGE_GRACE_SECS: u64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingCanonicalDiagnostic {
    pub media_id: String,
    pub logical_key: String,
    pub lifecycle_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileOrphanCandidate {
    pub media_id: String,
    pub logical_key: String,
    pub size_bytes: u64,
    pub modified_unix_secs: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcilePlan {
    pub plan_id: String,
    pub generation: i64,
    pub version: i64,
    pub digest: String,
    pub created_at_unix: u64,
    pub scanned_records: usize,
    pub scanned_files: usize,
    pub missing_canonical_files: Vec<MissingCanonicalDiagnostic>,
    pub orphan_candidates: Vec<ReconcileOrphanCandidate>,
    pub recent_orphans_under_grace: usize,
    pub pending_delete_jobs: usize,
    pub purgeable_quarantine_count: usize,
    pub retained_quarantine_count: usize,
    pub orphan_secret_count: usize,
    pub purgeable_secret_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyReconcileRequest {
    pub plan_id: String,
    pub expected_generation: i64,
    pub expected_version: i64,
    pub digest: String,
    pub confirm: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyReconcileResult {
    pub job: OperationJob,
    pub quarantined_files: usize,
    pub generation: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PurgeQuarantineResult {
    pub job: OperationJob,
    pub purged_quarantine_dirs: usize,
    pub purged_secret_blobs: usize,
    pub skipped_under_grace: usize,
    pub generation: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RedactedSecretDiagnostic {
    pub ref_prefix: String,
    pub purgeable: bool,
    pub age_hours: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationsDiagnostics {
    pub maintenance_generation: i64,
    pub pending_delete_jobs: Vec<MediaDeleteJob>,
    pub total_quarantine_dirs: usize,
    pub purgeable_quarantine_dirs: usize,
    pub retained_quarantine_dirs: usize,
    pub total_secret_blobs: usize,
    pub orphan_secret_blobs: usize,
    pub purgeable_secret_blobs: usize,
    pub orphan_secrets_redacted: Vec<RedactedSecretDiagnostic>,
    pub recent_jobs: Vec<OperationJob>,
}

pub(super) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

pub(super) struct ScannedFile {
    pub(super) logical_key: String,
    pub(super) abs_path: PathBuf,
    pub(super) media_id: String,
    pub(super) size_bytes: u64,
    pub(super) modified_unix_secs: u64,
}

pub(super) struct ScannedSecretBlob {
    pub(super) secret_ref: String,
    pub(super) abs_path: PathBuf,
    pub(super) modified_unix_secs: u64,
}
