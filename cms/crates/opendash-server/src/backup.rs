//! Parent-side snapshot orchestration under exclusive maintenance (M8-S03-T01).

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use opendash_core::backup::{BackupManifestV1, validate_manifest_v1};
use sqlx::SqlitePool;

use crate::auth::generate_random_token;
use crate::backup_worker::{SnapshotWorkerRequest, SnapshotWorkerResponse};
use crate::error::{ApiError, ApiResult};
use crate::operations::ExclusiveOpsContext;
use crate::paths::SitePaths;
use crate::secrets::ensure_private_directory_tree;

pub const MAX_SNAPSHOT_WALL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct SnapshotReceipt {
    pub job_id: String,
    pub snapshot_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: BackupManifestV1,
}

pub async fn create_snapshot_with_cancel(
    db: &SqlitePool,
    paths: &SitePaths,
    _maint: &ExclusiveOpsContext,
    cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> ApiResult<SnapshotReceipt> {
    create_snapshot_with_timeout_and_cancel(db, paths, MAX_SNAPSHOT_WALL_TIMEOUT, cancel_flag).await
}

async fn create_snapshot_with_timeout_and_cancel(
    db: &SqlitePool,
    paths: &SitePaths,
    wall_timeout: Duration,
    cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> ApiResult<SnapshotReceipt> {
    // Pre-check pending delete jobs before spawning worker.
    let _ = crate::storage::backup::collect_backup_inventory(db).await?;

    let job_id = generate_random_token();
    let staging_root = paths.backups_staging_dir();
    ensure_private_directory_tree(&paths.backups_dir())?;
    ensure_private_directory_tree(&staging_root)?;
    let job_dir = staging_root.join(&job_id);
    ensure_private_directory_tree(&job_dir)?;
    let snapshot_dir = job_dir.join("snapshot");
    ensure_private_directory_tree(&snapshot_dir)?;

    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();

    let req = SnapshotWorkerRequest {
        site_root: paths.site_root().to_path_buf(),
        db_path: paths.db_path().to_path_buf(),
        snapshot_dir: snapshot_dir.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        source_sha: option_env!("OPENDASH_SOURCE_SHA")
            .unwrap_or("dev")
            .to_string(),
        created_at,
    };
    let req_bytes = serde_json::to_vec(&req)
        .map_err(|_| ApiError::internal("Không thể tuần tự hóa yêu cầu snapshot worker."))?;

    let exe = std::env::current_exe()
        .map_err(|_| ApiError::internal("Không xác định được đường dẫn thực thi."))?;

    let mut child = crate::operations_worker_process::spawn(&exe, "snapshot")
        .map_err(|_| ApiError::internal("Không khởi chạy được operations worker."))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&req_bytes)
            .and_then(|_| stdin.write_all(b"\n"))
            .and_then(|_| stdin.flush())
            .map_err(|_| ApiError::internal("Không gửi được yêu cầu tới operations worker."))?;
        // Close stdin pipe to signal EOF of request (keep? Wait: lifeline reads stdin!).
        // Wait: if we drop stdin immediately, the lifeline thread in worker sees EOF!
        // So we keep `stdin` open until `child` finishes!
        let output = tokio::task::spawn_blocking(move || {
            let start = std::time::Instant::now();
            loop {
                if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    let _ = child.kill();
                    let _ = child.wait();
                    drop(stdin);
                    return Err(ApiError::conflict("Tác vụ backup đã bị hủy."));
                }
                match child.try_wait() {
                    Ok(Some(_status)) => {
                        drop(stdin);
                        return child
                            .wait_with_output()
                            .map_err(|_| ApiError::internal("Không đọc được kết quả worker."));
                    }
                    Ok(None) => {
                        if start.elapsed() >= wall_timeout {
                            let _ = child.kill();
                            let _ = child.wait();
                            drop(stdin);
                            return Err(ApiError::service_unavailable(
                                "Tác vụ snapshot vượt quá thời gian cho phép và đã bị dừng.",
                            ));
                        }
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    Err(_) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        drop(stdin);
                        return Err(ApiError::internal("Lỗi theo dõi tiến trình worker."));
                    }
                }
            }
        })
        .await
        .map_err(|_| ApiError::internal("Lỗi join tác vụ snapshot worker."))??;

        let resp: SnapshotWorkerResponse =
            serde_json::from_slice(&output.stdout).map_err(|_| {
                ApiError::internal("Phản hồi từ operations worker không hợp lệ hoặc bị ngắt.")
            })?;
        if !resp.ok || !output.status.success() {
            let msg = resp
                .error
                .unwrap_or_else(|| "Operations worker thất bại.".to_string());
            return Err(ApiError::conflict(msg));
        }
    }

    let manifest_path = snapshot_dir.join("manifest.json");
    let manifest_bytes = tokio::fs::read(&manifest_path)
        .await
        .map_err(|_| ApiError::internal("Không đọc được manifest.json sau snapshot."))?;
    let manifest: BackupManifestV1 = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| ApiError::internal("manifest.json không hợp lệ."))?;
    validate_manifest_v1(&manifest).map_err(ApiError::internal)?;

    Ok(SnapshotReceipt {
        job_id,
        snapshot_dir,
        manifest_path,
        manifest,
    })
}

pub const MAX_BACKUP_ARTIFACTS: usize = 10;
pub const MAX_ARCHIVE_WALL_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackupSummaryMetadata {
    pub request_id: String,
    pub actor_day: String,
    pub nonsecret_digest: String,
    pub archive_filename: Option<String>,
    pub archive_size: Option<u64>,
    pub archive_sha256: Option<String>,
    pub schema_version: Option<i64>,
    pub file_count: Option<usize>,
    pub deleted: bool,
}

#[derive(Default)]
pub struct BackupRuntimeRegistry {
    active_cancels: std::sync::Mutex<
        std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>,
    >,
    active_handles:
        tokio::sync::Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>,
    download_leases: std::sync::Mutex<std::collections::HashMap<String, usize>>,
    lease_notify: tokio::sync::Notify,
}

pub struct DownloadLeaseGuard {
    job_id: String,
    registry: std::sync::Arc<BackupRuntimeRegistry>,
}

impl Drop for DownloadLeaseGuard {
    fn drop(&mut self) {
        let mut map = self.registry.download_leases.lock().expect("lease mutex");
        if let Some(cnt) = map.get_mut(&self.job_id) {
            if *cnt <= 1 {
                map.remove(&self.job_id);
            } else {
                *cnt -= 1;
            }
        }
        self.registry.lease_notify.notify_waiters();
    }
}

impl BackupRuntimeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_cancel_flag(
        &self,
        job_id: &str,
    ) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.active_cancels
            .lock()
            .expect("cancel mutex")
            .insert(job_id.to_string(), flag.clone());
        flag
    }

    pub async fn register_task_handle(&self, job_id: &str, handle: tokio::task::JoinHandle<()>) {
        self.active_handles
            .lock()
            .await
            .insert(job_id.to_string(), handle);
    }

    pub fn request_cancel(&self, job_id: &str) -> bool {
        if let Some(flag) = self
            .active_cancels
            .lock()
            .expect("cancel mutex")
            .get(job_id)
        {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    pub async fn join_task(&self, job_id: &str) {
        let handle = self.active_handles.lock().await.remove(job_id);
        if let Some(h) = handle {
            let _ = h.await;
        }
        self.active_cancels
            .lock()
            .expect("cancel mutex")
            .remove(job_id);
    }

    pub fn clear_job(&self, job_id: &str) {
        self.active_cancels
            .lock()
            .expect("cancel mutex")
            .remove(job_id);
    }

    pub async fn cancel_and_drain_all(&self, timeout: Duration) -> DrainReport {
        {
            let map = self.active_cancels.lock().expect("cancel mutex");
            for flag in map.values() {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let handles: Vec<(String, tokio::task::JoinHandle<()>)> = {
            let mut guard = self.active_handles.lock().await;
            guard.drain().collect()
        };
        let deadline = tokio::time::Instant::now() + timeout;
        let mut timed_out_jobs = Vec::new();
        for (id, h) in handles {
            if tokio::time::Instant::now() >= deadline {
                if !h.is_finished() {
                    timed_out_jobs.push(id);
                } else {
                    let _ = h.await;
                }
                continue;
            }
            if tokio::time::timeout_at(deadline, h).await.is_err() {
                timed_out_jobs.push(id);
            }
        }
        DrainReport {
            drained_cleanly: timed_out_jobs.is_empty(),
            timed_out_jobs,
        }
    }

    pub fn acquire_download_lease(self: &std::sync::Arc<Self>, job_id: &str) -> DownloadLeaseGuard {
        let mut map = self.download_leases.lock().expect("lease mutex");
        *map.entry(job_id.to_string()).or_insert(0) += 1;
        DownloadLeaseGuard {
            job_id: job_id.to_string(),
            registry: std::sync::Arc::clone(self),
        }
    }

    pub async fn wait_for_download_leases_drain(
        &self,
        job_id: &str,
        timeout: Duration,
    ) -> ApiResult<()> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.lease_notify.notified();
            let active = {
                let map = self.download_leases.lock().expect("lease mutex");
                map.get(job_id).copied().unwrap_or(0)
            };
            if active == 0 {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(ApiError::conflict(
                    "Bản sao lưu đang được tải xuống; vui lòng thử lại sau.",
                ));
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let _ = tokio::time::timeout(remaining, notified).await;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrainReport {
    pub drained_cleanly: bool,
    pub timed_out_jobs: Vec<String>,
}

pub fn utc_day_string() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86_400;
    format!("day-{days}")
}

pub fn artifact_path_for_job(paths: &SitePaths, job_id: &str) -> PathBuf {
    paths
        .backups_artifacts_dir()
        .join(format!("{job_id}.odbackup.age"))
}

pub async fn archive_snapshot_via_worker(
    snapshot_dir: &PathBuf,
    manifest_path: &PathBuf,
    out_path: &PathBuf,
    passphrase: &age::secrecy::SecretString,
    wall_timeout: Duration,
    cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> ApiResult<(u64, String)> {
    use crate::backup_worker::{ArchiveWorkerRequest, ArchiveWorkerResponse};
    use age::secrecy::ExposeSecret;

    let req = ArchiveWorkerRequest {
        snapshot_dir: snapshot_dir.clone(),
        manifest_path: manifest_path.clone(),
        out_path: out_path.clone(),
        passphrase: passphrase.expose_secret().to_string(),
    };
    let req_bytes = serde_json::to_vec(&req)
        .map_err(|_| ApiError::internal("Không thể tuần tự hóa yêu cầu archive worker."))?;
    drop(req);

    let exe = std::env::current_exe()
        .map_err(|_| ApiError::internal("Không xác định được đường dẫn thực thi."))?;

    let mut child = crate::operations_worker_process::spawn(&exe, "archive")
        .map_err(|_| ApiError::internal("Không khởi chạy được archive worker."))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| ApiError::internal("Thiếu stdin pipe cho archive worker."))?;
    stdin
        .write_all(&req_bytes)
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|_| ApiError::internal("Không gửi được yêu cầu tới archive worker."))?;

    let output = tokio::task::spawn_blocking(move || {
        let start = std::time::Instant::now();
        loop {
            if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                drop(stdin);
                return Err(ApiError::conflict("Tác vụ backup đã bị hủy."));
            }
            match child.try_wait() {
                Ok(Some(_)) => {
                    drop(stdin);
                    return child
                        .wait_with_output()
                        .map_err(|_| ApiError::internal("Không đọc được kết quả archive worker."));
                }
                Ok(None) => {
                    if start.elapsed() >= wall_timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        drop(stdin);
                        return Err(ApiError::service_unavailable(
                            "Tác vụ mã hóa backup vượt quá thời gian cho phép và đã bị dừng.",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    drop(stdin);
                    return Err(ApiError::internal(
                        "Lỗi theo dõi tiến trình archive worker.",
                    ));
                }
            }
        }
    })
    .await
    .map_err(|_| ApiError::internal("Lỗi join tác vụ archive worker."))??;

    let resp: ArchiveWorkerResponse = serde_json::from_slice(&output.stdout)
        .map_err(|_| ApiError::internal("Phản hồi từ archive worker không hợp lệ hoặc bị ngắt."))?;
    if !resp.ok || !output.status.success() {
        let msg = resp
            .error
            .unwrap_or_else(|| "Archive worker thất bại.".to_string());
        return Err(ApiError::conflict(msg));
    }
    let size = resp
        .archive_size
        .ok_or_else(|| ApiError::internal("Archive worker thiếu archive_size."))?;
    let sha = resp
        .archive_sha256
        .ok_or_else(|| ApiError::internal("Archive worker thiếu archive_sha256."))?;
    Ok((size, sha))
}

#[allow(clippy::too_many_arguments)]
pub async fn run_backup_job_background(
    db: SqlitePool,
    paths: SitePaths,
    registry: std::sync::Arc<BackupRuntimeRegistry>,
    job_id: String,
    running_version: i64,
    actor_user_id: i64,
    mut summary_meta: BackupSummaryMetadata,
    passphrase: age::secrecy::SecretString,
    maint: ExclusiveOpsContext,
    cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use crate::storage::operations::{JobTransitionInput, OperationState, transition_job_cas};

    let mut current_version = running_version;

    // Step 1: Take consistent snapshot while holding exclusive maintenance barrier.
    let snapshot_res = if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
        Err(ApiError::conflict("Tác vụ backup đã bị hủy."))
    } else {
        create_snapshot_with_cancel(&db, &paths, &maint, cancel_flag.clone()).await
    };
    // Release exclusive maintenance barrier immediately after immutable snapshot cut.
    drop(maint);

    let snapshot = match snapshot_res {
        Ok(s) => s,
        Err(err) => {
            let next_state = if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                OperationState::Cancelled
            } else {
                OperationState::Failed
            };
            let _ = transition_job_cas(
                &db,
                &job_id,
                JobTransitionInput {
                    expected_version: current_version,
                    next_state,
                    summary_json: Some(serde_json::to_string(&summary_meta).unwrap_or_default()),
                    error_code: Some("snapshot_failed".to_string()),
                    error_message: Some(err.1.to_string()),
                },
            )
            .await;
            registry.clear_job(&job_id);
            return;
        }
    };

    let out_path = artifact_path_for_job(&paths, &job_id);
    let archive_res = archive_snapshot_via_worker(
        &snapshot.snapshot_dir,
        &snapshot.manifest_path,
        &out_path,
        &passphrase,
        MAX_ARCHIVE_WALL_TIMEOUT,
        cancel_flag.clone(),
    )
    .await;
    drop(passphrase);

    // Always remove plaintext snapshot staging directory after encryption attempt.
    if let Some(job_stage_dir) = snapshot.snapshot_dir.parent() {
        let _ = std::fs::remove_dir_all(job_stage_dir);
    }

    let (archive_size, archive_sha256) = match archive_res {
        Ok(res) => res,
        Err(err) => {
            let _ = std::fs::remove_file(&out_path);
            let next_state = if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                OperationState::Cancelled
            } else {
                OperationState::Failed
            };
            let _ = transition_job_cas(
                &db,
                &job_id,
                JobTransitionInput {
                    expected_version: current_version,
                    next_state,
                    summary_json: Some(serde_json::to_string(&summary_meta).unwrap_or_default()),
                    error_code: Some("archive_failed".to_string()),
                    error_message: Some(err.1.to_string()),
                },
            )
            .await;
            registry.clear_job(&job_id);
            return;
        }
    };

    // Re-check creator Admin role/account before publishing succeeded receipt.
    let still_admin = crate::storage::operations::is_active_admin_user(&db, actor_user_id)
        .await
        .unwrap_or(false);
    if !still_admin || cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
        let _ = std::fs::remove_file(&out_path);
        let next_state = if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
            OperationState::Cancelled
        } else {
            OperationState::Failed
        };
        let _ = transition_job_cas(
            &db,
            &job_id,
            JobTransitionInput {
                expected_version: current_version,
                next_state,
                summary_json: Some(serde_json::to_string(&summary_meta).unwrap_or_default()),
                error_code: Some("actor_revoked_or_cancelled".to_string()),
                error_message: Some(
                    "Tài khoản Admin không còn hợp lệ hoặc tác vụ đã bị hủy.".to_string(),
                ),
            },
        )
        .await;
        registry.clear_job(&job_id);
        return;
    }

    summary_meta.archive_filename = Some(format!("opendash-backup-{}.odbackup.age", &job_id[..12]));
    summary_meta.archive_size = Some(archive_size);
    summary_meta.archive_sha256 = Some(archive_sha256);
    summary_meta.schema_version = Some(snapshot.manifest.schema_version);
    summary_meta.file_count = Some(snapshot.manifest.files.len());
    summary_meta.deleted = false;

    if let Ok(updated) = transition_job_cas(
        &db,
        &job_id,
        JobTransitionInput {
            expected_version: current_version,
            next_state: OperationState::Succeeded,
            summary_json: Some(serde_json::to_string(&summary_meta).unwrap_or_default()),
            error_code: None,
            error_message: None,
        },
    )
    .await
    {
        current_version = updated.version;
        let _ = current_version;
    }
    registry.clear_job(&job_id);
}
