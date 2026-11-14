//! Parent-side snapshot orchestration under exclusive maintenance (M8-S03-T01).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
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

pub async fn create_snapshot(
    db: &SqlitePool,
    paths: &SitePaths,
    _maint: &ExclusiveOpsContext,
) -> ApiResult<SnapshotReceipt> {
    create_snapshot_with_timeout(db, paths, MAX_SNAPSHOT_WALL_TIMEOUT).await
}

pub async fn create_snapshot_with_timeout(
    db: &SqlitePool,
    paths: &SitePaths,
    wall_timeout: Duration,
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

    let mut child = Command::new(exe)
        .arg("--operations-worker")
        .arg("snapshot")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
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

        let resp: SnapshotWorkerResponse = serde_json::from_slice(&output.stdout).map_err(|_| {
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
