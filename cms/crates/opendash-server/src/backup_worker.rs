//! Isolated `--operations-worker snapshot` process with native 512MiB memory limit and lifeline (M8-S03-T01).

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use opendash_core::backup::{
    BACKUP_FORMAT_VERSION, BackupFileKind, BackupFileRecord, BackupManifestV1,
    MAX_BACKUP_MANIFEST_BYTES, PortableRuntimeConfigV1, RESTORE_POLICY_VERSION,
    format_flat_entry_name, is_valid_sha256_hex, validate_logical_key, validate_manifest_v1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqliteConnectOptions;

use crate::error::{ApiError, ApiResult};
use crate::paths::SitePaths;
use crate::secrets::{create_private_file, ensure_no_reparse_or_symlink, ensure_private_directory_tree};
use crate::storage::backup::{collect_backup_inventory, vacuum_into_snapshot};

pub const WORKER_MEMORY_LIMIT_BYTES: usize = 512 * 1024 * 1024;
const STREAM_BUF_BYTES: usize = 64 * 1024;
const MAX_WORKER_REQUEST_BYTES: usize = 64 * 1024;

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

#[cfg(windows)]
pub fn enforce_native_memory_limit_512mib() -> Result<(), String> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    unsafe {
        let job = CreateJobObjectW(null_mut(), null_mut());
        if job.is_null() || job == INVALID_HANDLE_VALUE {
            return Err("CreateJobObjectW failed".to_string());
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        info.ProcessMemoryLimit = WORKER_MEMORY_LIMIT_BYTES;

        let ok = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if ok == 0 {
            CloseHandle(job);
            return Err("SetInformationJobObject failed".to_string());
        }
        let assigned = AssignProcessToJobObject(job, GetCurrentProcess());
        if assigned == 0 {
            CloseHandle(job);
            return Err("AssignProcessToJobObject failed".to_string());
        }
        // Keep job handle open for process lifetime so KILL_ON_JOB_CLOSE stays bound.
        let _ = job;
    }
    Ok(())
}

#[cfg(unix)]
pub fn enforce_native_memory_limit_512mib() -> Result<(), String> {
    #[repr(C)]
    struct RLimit {
        rlim_cur: u64,
        rlim_max: u64,
    }
    #[cfg(target_os = "linux")]
    const RLIMIT_AS: i32 = 9;
    #[cfg(target_os = "macos")]
    const RLIMIT_AS: i32 = 5;
    unsafe extern "C" {
        fn setrlimit(resource: i32, rlim: *const RLimit) -> i32;
    }
    let lim = RLimit {
        rlim_cur: WORKER_MEMORY_LIMIT_BYTES as u64,
        rlim_max: WORKER_MEMORY_LIMIT_BYTES as u64,
    };
    let rc = unsafe { setrlimit(RLIMIT_AS, &lim) };
    if rc != 0 {
        return Err(format!("setrlimit(RLIMIT_AS) failed with {rc}"));
    }
    Ok(())
}

pub fn run_operations_worker_from_stdio() -> i32 {
    if let Err(err) = enforce_native_memory_limit_512mib() {
        let resp = SnapshotWorkerResponse {
            ok: false,
            manifest_path: None,
            error: Some(format!("memory_limit_failed: {err}")),
        };
        let _ = serde_json::to_writer(io::stdout(), &resp);
        return 2;
    }

    let mode = std::env::args()
        .skip_while(|a| a != "--operations-worker")
        .nth(1)
        .unwrap_or_else(|| "snapshot".to_string());
    match mode.as_str() {
        "snapshot" => run_snapshot_worker_from_stdio(),
        "archive" => run_archive_worker_from_stdio(),
        "verify" => run_verify_worker_from_stdio(),
        "restore" => run_restore_worker_from_stdio(),
        _ => {
            let resp = SnapshotWorkerResponse {
                ok: false,
                manifest_path: None,
                error: Some("unknown_operations_worker_mode".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            2
        }
    }
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

fn read_worker_line() -> Result<String, &'static str> {
    let mut req_line = String::new();
    let mut reader = io::BufReader::new(io::stdin().take((MAX_WORKER_REQUEST_BYTES + 1) as u64));
    if io::BufRead::read_line(&mut reader, &mut req_line).is_err()
        || req_line.is_empty()
        || req_line.len() > MAX_WORKER_REQUEST_BYTES
    {
        return Err("invalid_worker_request_size");
    }
    Ok(req_line)
}

fn spawn_stdin_lifeline() -> Arc<AtomicBool> {
    let done = Arc::new(AtomicBool::new(false));
    let done_clone = Arc::clone(&done);
    std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        loop {
            match io::stdin().read(&mut byte) {
                Ok(0) => {
                    if !done_clone.load(Ordering::SeqCst) {
                        std::process::exit(3);
                    }
                    break;
                }
                Ok(_) => {}
                Err(_) => {
                    if !done_clone.load(Ordering::SeqCst) {
                        std::process::exit(3);
                    }
                    break;
                }
            }
        }
    });
    done
}

fn run_archive_worker_from_stdio() -> i32 {
    let Ok(req_line) = read_worker_line() else {
        let resp = ArchiveWorkerResponse {
            ok: false,
            archive_path: None,
            archive_size: None,
            archive_sha256: None,
            error: Some("invalid_worker_request_size".to_string()),
        };
        let _ = serde_json::to_writer(io::stdout(), &resp);
        return 2;
    };
    let req: ArchiveWorkerRequest = match serde_json::from_str(req_line.trim_end()) {
        Ok(r) => r,
        Err(_) => {
            let resp = ArchiveWorkerResponse {
                ok: false,
                archive_path: None,
                archive_size: None,
                archive_sha256: None,
                error: Some("invalid_worker_request_json".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 2;
        }
    };
    let done = spawn_stdin_lifeline();
    let manifest_bytes = match fs::read(&req.manifest_path) {
        Ok(b) => b,
        Err(_) => {
            done.store(true, Ordering::SeqCst);
            let resp = ArchiveWorkerResponse {
                ok: false,
                archive_path: None,
                archive_size: None,
                archive_sha256: None,
                error: Some("manifest_read_failed".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 1;
        }
    };
    let manifest: BackupManifestV1 = match serde_json::from_slice(&manifest_bytes) {
        Ok(m) => m,
        Err(_) => {
            done.store(true, Ordering::SeqCst);
            let resp = ArchiveWorkerResponse {
                ok: false,
                archive_path: None,
                archive_size: None,
                archive_sha256: None,
                error: Some("manifest_parse_failed".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 1;
        }
    };
    let receipt = crate::backup::SnapshotReceipt {
        job_id: "worker".to_string(),
        snapshot_dir: req.snapshot_dir,
        manifest_path: req.manifest_path,
        manifest,
    };
    let secret = age::secrecy::SecretString::from(req.passphrase);
    let res = crate::backup_archive::write_encrypted_archive(&receipt, &secret, &req.out_path);
    done.store(true, Ordering::SeqCst);
    match res {
        Ok(art) => {
            let resp = ArchiveWorkerResponse {
                ok: true,
                archive_path: Some(art.archive_path),
                archive_size: Some(art.archive_size),
                archive_sha256: Some(art.archive_sha256),
                error: None,
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            0
        }
        Err(err) => {
            let resp = ArchiveWorkerResponse {
                ok: false,
                archive_path: None,
                archive_size: None,
                archive_sha256: None,
                error: Some(err.1.to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            1
        }
    }
}

fn run_verify_worker_from_stdio() -> i32 {
    let Ok(req_line) = read_worker_line() else {
        let resp = VerifyArchiveWorkerResponse {
            ok: false,
            zip_path: None,
            payload_dir: None,
            file_count: None,
            schema_version: None,
            error: Some("invalid_worker_request_size".to_string()),
        };
        let _ = serde_json::to_writer(io::stdout(), &resp);
        return 2;
    };
    let req: VerifyArchiveWorkerRequest = match serde_json::from_str(req_line.trim_end()) {
        Ok(r) => r,
        Err(_) => {
            let resp = VerifyArchiveWorkerResponse {
                ok: false,
                zip_path: None,
                payload_dir: None,
                file_count: None,
                schema_version: None,
                error: Some("invalid_worker_request_json".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 2;
        }
    };
    let done = spawn_stdin_lifeline();
    let secret = age::secrecy::SecretString::from(req.passphrase);
    let res = crate::backup_archive::decrypt_and_validate_archive(
        &req.input_path,
        &secret,
        &req.private_dir,
    );
    done.store(true, Ordering::SeqCst);
    match res {
        Ok(v) => {
            let resp = VerifyArchiveWorkerResponse {
                ok: true,
                zip_path: Some(v.zip_path),
                payload_dir: Some(v.payload_dir),
                file_count: Some(v.manifest.files.len()),
                schema_version: Some(v.manifest.schema_version),
                error: None,
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            0
        }
        Err(err) => {
            let resp = VerifyArchiveWorkerResponse {
                ok: false,
                zip_path: None,
                payload_dir: None,
                file_count: None,
                schema_version: None,
                error: Some(err.1.to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            1
        }
    }
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

fn run_restore_worker_from_stdio() -> i32 {
    let Ok(req_line) = read_worker_line() else {
        let resp = RestoreWorkerResponse {
            ok: false,
            target_root: None,
            schema_version: None,
            db_sha256: None,
            file_count: None,
            error: Some("invalid_worker_request_size".to_string()),
        };
        let _ = serde_json::to_writer(io::stdout(), &resp);
        return 2;
    };
    let req: RestoreWorkerRequest = match serde_json::from_str(req_line.trim_end()) {
        Ok(r) => r,
        Err(_) => {
            let resp = RestoreWorkerResponse {
                ok: false,
                target_root: None,
                schema_version: None,
                db_sha256: None,
                file_count: None,
                error: Some("invalid_worker_request_json".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 2;
        }
    };
    let done = spawn_stdin_lifeline();
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            done.store(true, Ordering::SeqCst);
            let resp = RestoreWorkerResponse {
                ok: false,
                target_root: None,
                schema_version: None,
                db_sha256: None,
                file_count: None,
                error: Some(format!("runtime_init_failed: {err}")),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 2;
        }
    };

    let failpoint = match req.failpoint.as_deref() {
        Some("after_extract") => Some(crate::restore::RestoreFailpoint::AfterExtract),
        Some("after_db_sanitize") => Some(crate::restore::RestoreFailpoint::AfterDbSanitize),
        Some("after_ready_marker") => Some(crate::restore::RestoreFailpoint::AfterReadyMarker),
        _ => None,
    };

    let secret = age::secrecy::SecretString::from(req.passphrase);
    let res = rt.block_on(async {
        let validated = crate::backup_archive::decrypt_and_validate_archive(
            &req.input_path,
            &secret,
            &req.private_dir,
        )?;
        let policy = crate::restore::RestorePolicy {
            job_id: req.job_id,
            source_site_root: req.source_site_root,
            source_db_path: req.source_db_path,
            failpoint,
        };
        crate::restore::restore_to_new_root(&validated, &policy, &req.generated_target).await
    });
    done.store(true, Ordering::SeqCst);
    match res {
        Ok(r) => {
            let resp = RestoreWorkerResponse {
                ok: true,
                target_root: Some(r.target_root),
                schema_version: Some(r.schema_version),
                db_sha256: Some(r.db_sha256),
                file_count: Some(r.file_count),
                error: None,
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            0
        }
        Err(err) => {
            let resp = RestoreWorkerResponse {
                ok: false,
                target_root: None,
                schema_version: None,
                db_sha256: None,
                file_count: None,
                error: Some(err.1.to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            1
        }
    }
}

fn run_snapshot_worker_from_stdio() -> i32 {

    let mut req_line = String::new();
    let mut reader = io::BufReader::new(io::stdin().take((MAX_WORKER_REQUEST_BYTES + 1) as u64));
    if io::BufRead::read_line(&mut reader, &mut req_line).is_err()
        || req_line.is_empty()
        || req_line.len() > MAX_WORKER_REQUEST_BYTES
    {
        let resp = SnapshotWorkerResponse {
            ok: false,
            manifest_path: None,
            error: Some("invalid_worker_request_size".to_string()),
        };
        let _ = serde_json::to_writer(io::stdout(), &resp);
        return 2;
    }

    let req: SnapshotWorkerRequest = match serde_json::from_str(req_line.trim_end()) {
        Ok(r) => r,
        Err(_) => {
            let resp = SnapshotWorkerResponse {
                ok: false,
                manifest_path: None,
                error: Some("invalid_worker_request_json".to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 2;
        }
    };

    // Lifeline watcher: if parent closes stdin pipe unexpectedly, terminate.
    let done = Arc::new(AtomicBool::new(false));
    let done_clone = Arc::clone(&done);
    std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        loop {
            match io::stdin().read(&mut byte) {
                Ok(0) => {
                    if !done_clone.load(Ordering::SeqCst) {
                        std::process::exit(3);
                    }
                    break;
                }
                Ok(_) => {}
                Err(_) => {
                    if !done_clone.load(Ordering::SeqCst) {
                        std::process::exit(3);
                    }
                    break;
                }
            }
        }
    });

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            let resp = SnapshotWorkerResponse {
                ok: false,
                manifest_path: None,
                error: Some(format!("runtime_init_failed: {err}")),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            return 2;
        }
    };

    let result = rt.block_on(execute_snapshot_worker(&req));
    done.store(true, Ordering::SeqCst);

    match result {
        Ok(manifest_path) => {
            let resp = SnapshotWorkerResponse {
                ok: true,
                manifest_path: Some(manifest_path),
                error: None,
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            let _ = io::stdout().flush();
            0
        }
        Err(err) => {
            let resp = SnapshotWorkerResponse {
                ok: false,
                manifest_path: None,
                error: Some(err.1.to_string()),
            };
            let _ = serde_json::to_writer(io::stdout(), &resp);
            let _ = io::stdout().flush();
            1
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn stream_copy_and_hash(src_path: &Path, dst_path: &Path) -> ApiResult<(u64, String)> {
    ensure_no_reparse_or_symlink(src_path)?;
    let meta = fs::symlink_metadata(src_path)
        .map_err(|_| ApiError::conflict(format!("Thiếu tệp tham chiếu: {}", src_path.display())))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(ApiError::conflict("Từ chối symlink hoặc đối tượng không phải tệp."));
    }

    let mut src = File::open(src_path)
        .map_err(|_| ApiError::conflict("Không thể mở tệp nguồn khi tạo snapshot."))?;
    let mut dst = create_private_file(dst_path)?;

    let mut buf = vec![0u8; STREAM_BUF_BYTES];
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;

    loop {
        let n = src
            .read(&mut buf)
            .map_err(|_| ApiError::internal("Lỗi đọc tệp khi tạo snapshot."))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        dst.write_all(&buf[..n])
            .map_err(|_| ApiError::internal("Lỗi ghi tệp snapshot (có thể đầy đĩa)."))?;
        total += n as u64;
    }

    dst.sync_all()
        .map_err(|_| ApiError::internal("Không thể fsync tệp snapshot."))?;
    Ok((total, hex_encode(&hasher.finalize())))
}

fn hash_existing_file_streaming(path: &Path) -> ApiResult<(u64, String)> {
    let mut f = File::open(path)
        .map_err(|_| ApiError::internal("Không mở được tệp DB snapshot để băm."))?;
    let mut buf = vec![0u8; STREAM_BUF_BYTES];
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|_| ApiError::internal("Lỗi đọc DB snapshot."))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    Ok((total, hex_encode(&hasher.finalize())))
}

struct PlannedCopy {
    kind: BackupFileKind,
    logical_key: String,
    source_path: Option<PathBuf>,
    inline_bytes: Option<Vec<u8>>,
}

async fn execute_snapshot_worker(req: &SnapshotWorkerRequest) -> ApiResult<PathBuf> {
    let paths = SitePaths::from_parts(req.site_root.clone(), req.db_path.clone());
    ensure_private_directory_tree(&req.snapshot_dir)?;

    let opts = SqliteConnectOptions::new()
        .filename(paths.db_path())
        .create_if_missing(false)
        .read_only(true);
    let db = sqlx::SqlitePool::connect_with(opts)
        .await
        .map_err(|_| ApiError::internal("Worker không mở được SQLite nguồn."))?;

    let inventory = collect_backup_inventory(&db).await?;
    db.close().await;

    let mut planned: Vec<PlannedCopy> = Vec::new();
    let mut known_media_ids: HashSet<String> = HashSet::new();

    for m in &inventory.media {
        known_media_ids.insert(m.id.clone());
        let shard = &m.id[..2];
        let orig_rel = format!("media/{shard}/{}.{}", m.id, m.extension);
        let orig_path = paths.uploads_dir().join(&orig_rel);
        planned.push(PlannedCopy {
            kind: BackupFileKind::MediaOriginal,
            logical_key: orig_rel,
            source_path: Some(orig_path),
            inline_bytes: None,
        });
        if m.kind == "image" {
            let thumb_rel = format!("media/{shard}/{}.thumb.png", m.id);
            let thumb_path = paths.uploads_dir().join(&thumb_rel);
            planned.push(PlannedCopy {
                kind: BackupFileKind::MediaThumbnail,
                logical_key: thumb_rel,
                source_path: Some(thumb_path),
                inline_bytes: None,
            });
        }
    }

    for q_job_id in &inventory.quarantine_job_ids {
        let q_dir = paths.uploads_dir().join(".quarantine").join(q_job_id);
        if q_dir.exists() {
            ensure_no_reparse_or_symlink(&q_dir)?;
            let rd = fs::read_dir(&q_dir)
                .map_err(|_| ApiError::conflict("Không đọc được thư mục quarantine."))?;
            for entry in rd {
                let entry = entry.map_err(|_| ApiError::conflict("Lỗi đọc tệp quarantine."))?;
                let fname = entry.file_name().to_string_lossy().to_string();
                let key = format!("quarantine/{q_job_id}/{fname}");
                planned.push(PlannedCopy {
                    kind: BackupFileKind::QuarantineObject,
                    logical_key: key,
                    source_path: Some(entry.path()),
                    inline_bytes: None,
                });
            }
        }
    }

    // Include canonical unreferenced media files under quarantine/unreferenced/
    let media_root = paths.uploads_dir().join("media");
    if media_root.exists() {
        ensure_no_reparse_or_symlink(&media_root)?;
        let shards = fs::read_dir(&media_root)
            .map_err(|_| ApiError::conflict("Không đọc được uploads/media."))?;
        for shard_ent in shards {
            let shard_ent =
                shard_ent.map_err(|_| ApiError::conflict("Lỗi đọc shard trong uploads/media."))?;
            let shard_name = shard_ent.file_name().to_string_lossy().to_string();
            let shard_path = shard_ent.path();
            ensure_no_reparse_or_symlink(&shard_path)?;
            if !shard_path.is_dir() || shard_name.len() != 2 {
                return Err(ApiError::conflict("Phát hiện đối tượng lạ trong uploads/media."));
            }
            let files = fs::read_dir(&shard_path)
                .map_err(|_| ApiError::conflict("Không đọc được shard uploads/media."))?;
            for f_ent in files {
                let f_ent =
                    f_ent.map_err(|_| ApiError::conflict("Lỗi đọc tệp trong uploads/media."))?;
                let fname = f_ent.file_name().to_string_lossy().to_string();
                let stem = fname
                    .strip_suffix(".thumb.png")
                    .or_else(|| fname.rsplit_once('.').map(|(s, _)| s))
                    .unwrap_or("");
                if !is_valid_sha256_hex(stem) || !stem.starts_with(&shard_name) {
                    return Err(ApiError::conflict(
                        "Phát hiện tệp không hợp lệ trong uploads/media; cần reconcile trước.",
                    ));
                }
                if !known_media_ids.contains(stem) {
                    planned.push(PlannedCopy {
                        kind: BackupFileKind::QuarantineObject,
                        logical_key: format!("quarantine/unreferenced/{fname}"),
                        source_path: Some(f_ent.path()),
                        inline_bytes: None,
                    });
                }
            }
        }
    }

    for rev in &inventory.theme_revisions {
        if rev.source_format == "v2" {
            let rev_dir = paths
                .themes_dir()
                .join(&rev.theme_id)
                .join(&rev.revision_digest);
            if !rev_dir.exists() {
                return Err(ApiError::conflict(format!(
                    "Thiếu thư mục theme revision {}/{}.",
                    rev.theme_id, rev.revision_digest
                )));
            }
            collect_dir_files_recursive(
                &rev_dir,
                &format!("themes/{}/{}", rev.theme_id, rev.revision_digest),
                BackupFileKind::ThemePackageFile,
                &mut planned,
            )?;
        }
    }

    for sref in &inventory.secret_refs {
        let json_path = paths.smtp_secrets_dir().join(format!("{sref}.json"));
        let bin_path = paths.smtp_secrets_dir().join(format!("{sref}.bin"));
        if json_path.exists() {
            planned.push(PlannedCopy {
                kind: BackupFileKind::SmtpSecret,
                logical_key: format!("secrets/smtp/{sref}.json"),
                source_path: Some(json_path),
                inline_bytes: None,
            });
        } else if bin_path.exists() {
            planned.push(PlannedCopy {
                kind: BackupFileKind::SmtpSecret,
                logical_key: format!("secrets/smtp/{sref}.bin"),
                source_path: Some(bin_path),
                inline_bytes: None,
            });
        } else {
            return Err(ApiError::conflict(
                "Thiếu tệp bí mật SMTP được tham chiếu; không thể tạo snapshot.",
            ));
        }
    }

    // Portable config/site.json
    let cfg_bytes = if paths.site_config_path().exists() {
        ensure_no_reparse_or_symlink(&paths.site_config_path())?;
        let raw = fs::read(&paths.site_config_path())
            .map_err(|_| ApiError::conflict("Không đọc được config/site.json."))?;
        let cfg: PortableRuntimeConfigV1 = serde_json::from_slice(&raw)
            .map_err(|_| ApiError::conflict("config/site.json không hợp lệ."))?;
        cfg.validate().map_err(ApiError::conflict)?;
        serde_json::to_vec_pretty(&cfg)
            .map_err(|_| ApiError::internal("Không thể tuần tự hóa config/site.json."))?
    } else {
        serde_json::to_vec_pretty(&PortableRuntimeConfigV1::default())
            .map_err(|_| ApiError::internal("Không thể tuần tự hóa config mặc định."))?
    };
    planned.push(PlannedCopy {
        kind: BackupFileKind::PortableConfig,
        logical_key: "config/site.json".to_string(),
        source_path: None,
        inline_bytes: Some(cfg_bytes),
    });

    // Entry 1: DB snapshot via VACUUM INTO
    let db_entry_name = format_flat_entry_name(1).map_err(ApiError::internal)?;
    let db_dest = req.snapshot_dir.join(&db_entry_name);
    let (schema_version, migration_checksums) =
        vacuum_into_snapshot(paths.db_path(), &db_dest).await?;
    let (db_size, db_sha) = hash_existing_file_streaming(&db_dest)?;

    planned.sort_by(|a, b| a.logical_key.cmp(&b.logical_key));

    let mut records = Vec::with_capacity(planned.len() + 1);
    records.push(BackupFileRecord {
        entry_name: db_entry_name,
        kind: BackupFileKind::Db,
        logical_key: "db/main.sqlite".to_string(),
        size: db_size,
        sha256: db_sha,
    });

    for (idx, item) in planned.into_iter().enumerate() {
        validate_logical_key(item.kind, &item.logical_key).map_err(ApiError::conflict)?;
        let entry_name = format_flat_entry_name(idx + 2).map_err(ApiError::conflict)?;
        let dst_path = req.snapshot_dir.join(&entry_name);

        let (size, sha256) = if let Some(src_path) = item.source_path {
            stream_copy_and_hash(&src_path, &dst_path)?
        } else if let Some(bytes) = item.inline_bytes {
            let mut dst = create_private_file(&dst_path)?;
            dst.write_all(&bytes)
                .map_err(|_| ApiError::internal("Lỗi ghi config snapshot."))?;
            dst.sync_all()
                .map_err(|_| ApiError::internal("Lỗi fsync config snapshot."))?;
            let digest = Sha256::digest(&bytes);
            (bytes.len() as u64, hex_encode(&digest))
        } else {
            return Err(ApiError::internal("Mục snapshot không có nguồn dữ liệu."));
        };

        records.push(BackupFileRecord {
            entry_name,
            kind: item.kind,
            logical_key: item.logical_key,
            size,
            sha256,
        });
    }

    records.sort_by(|a, b| a.logical_key.cmp(&b.logical_key));

    let manifest = BackupManifestV1 {
        format_version: BACKUP_FORMAT_VERSION,
        app_version: req.app_version.clone(),
        source_sha: req.source_sha.clone(),
        schema_version,
        migration_checksums,
        created_at: req.created_at.clone(),
        files: records,
        restore_policy_version: RESTORE_POLICY_VERSION,
    };

    validate_manifest_v1(&manifest).map_err(ApiError::conflict)?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|_| ApiError::internal("Không thể tuần tự hóa manifest.json."))?;
    if manifest_bytes.len() > MAX_BACKUP_MANIFEST_BYTES {
        return Err(ApiError::conflict("manifest.json vượt giới hạn 8 MiB."));
    }

    let manifest_path = req.snapshot_dir.join("manifest.json");
    let mut mf = create_private_file(&manifest_path)?;
    mf.write_all(&manifest_bytes)
        .map_err(|_| ApiError::internal("Lỗi ghi manifest.json."))?;
    mf.sync_all()
        .map_err(|_| ApiError::internal("Lỗi fsync manifest.json."))?;

    Ok(manifest_path)
}

fn collect_dir_files_recursive(
    dir: &Path,
    prefix_key: &str,
    kind: BackupFileKind,
    out: &mut Vec<PlannedCopy>,
) -> ApiResult<()> {
    ensure_no_reparse_or_symlink(dir)?;
    let rd = fs::read_dir(dir).map_err(|_| ApiError::conflict("Không đọc được thư mục gói."))?;
    for entry in rd {
        let entry = entry.map_err(|_| ApiError::conflict("Lỗi đọc mục trong thư mục gói."))?;
        let fname = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        ensure_no_reparse_or_symlink(&path)?;
        let key = format!("{prefix_key}/{fname}");
        if path.is_dir() {
            collect_dir_files_recursive(&path, &key, kind, out)?;
        } else {
            out.push(PlannedCopy {
                kind,
                logical_key: key,
                source_path: Some(path),
                inline_bytes: None,
            });
        }
    }
    Ok(())
}
