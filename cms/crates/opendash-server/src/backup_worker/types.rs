//! Isolated `--operations-worker snapshot` process with native 512MiB memory limit and lifeline (M8-S03-T01).

use super::*;

pub(super) use std::collections::HashSet;

pub(super) use std::fs::{self, File};

pub(super) use std::io::{self, Read, Write};

pub(super) use std::path::{Path, PathBuf};

pub(super) use std::sync::Arc;

pub(super) use std::sync::atomic::{AtomicBool, Ordering};

pub(super) use opendash_core::backup::{
    BACKUP_FORMAT_VERSION, BackupFileKind, BackupFileRecord, BackupManifestV1,
    MAX_BACKUP_MANIFEST_BYTES, PortableRuntimeConfigV1, RESTORE_POLICY_VERSION,
    format_flat_entry_name, is_valid_sha256_hex, validate_logical_key, validate_manifest_v1,
};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use sha2::{Digest, Sha256};

pub(super) use sqlx::sqlite::SqliteConnectOptions;

pub(super) use crate::error::{ApiError, ApiResult};

pub(super) use crate::paths::SitePaths;

pub(super) use crate::secrets::{
    create_private_file, ensure_no_reparse_or_symlink, ensure_private_directory_tree,
};

pub(super) use crate::storage::backup::{collect_backup_inventory, vacuum_into_snapshot};

pub const WORKER_MEMORY_LIMIT_BYTES: usize =
    crate::operations_worker_process::WORKER_MEMORY_LIMIT_BYTES;

pub(super) const STREAM_BUF_BYTES: usize = 64 * 1024;

pub(super) const MAX_WORKER_REQUEST_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotWorkerRequest {
    pub site_root: PathBuf,
    pub db_path: PathBuf,
    pub snapshot_dir: PathBuf,
    pub app_version: String,
    pub source_sha: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotWorkerResponse {
    pub ok: bool,
    pub manifest_path: Option<PathBuf>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveWorkerRequest {
    pub snapshot_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub out_path: PathBuf,
    pub passphrase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveWorkerResponse {
    pub ok: bool,
    pub archive_path: Option<PathBuf>,
    pub archive_size: Option<u64>,
    pub archive_sha256: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyArchiveWorkerRequest {
    pub input_path: PathBuf,
    pub private_dir: PathBuf,
    pub passphrase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyArchiveWorkerResponse {
    pub ok: bool,
    pub zip_path: Option<PathBuf>,
    pub payload_dir: Option<PathBuf>,
    pub file_count: Option<usize>,
    pub schema_version: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreWorkerRequest {
    pub input_path: PathBuf,
    pub private_dir: PathBuf,
    pub generated_target: PathBuf,
    pub source_site_root: PathBuf,
    pub source_db_path: PathBuf,
    pub job_id: String,
    pub passphrase: String,
    pub failpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreWorkerResponse {
    pub ok: bool,
    pub target_root: Option<PathBuf>,
    pub schema_version: Option<i64>,
    pub db_sha256: Option<String>,
    pub file_count: Option<usize>,
    pub error: Option<String>,
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

pub(super) struct PlannedCopy {
    pub(super) kind: BackupFileKind,
    pub(super) logical_key: String,
    pub(super) source_path: Option<PathBuf>,
    pub(super) inline_bytes: Option<Vec<u8>>,
}
