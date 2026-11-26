//! Admin operations routes: reconcile plan/apply, quarantine purge, diagnostics (M8-S02-T02).

use super::*;

pub(super) use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};

pub(super) use serde::Deserialize;

pub(super) use crate::app::AppState;

pub(super) use crate::auth::{require_admin, require_admin_with_csrf};

pub(super) use crate::error::{ApiError, ApiResult};

pub(super) use crate::media_lifecycle::{self, ApplyReconcileRequest};

pub(super) use crate::operations::{MAX_MAINTENANCE_DRAIN, MaintenanceError};

pub(super) use crate::storage::operations::OperationKind;

pub const MAX_OPERATIONS_JSON_BYTES: usize = 8 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeQuarantineRequest {
    pub confirm: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBackupRequest {
    pub passphrase: String,
    pub confirm_passphrase: String,
    pub request_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelJobRequest {
    pub expected_version: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ListBackupsQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct BackupArtifactView {
    pub id: String,
    pub filename: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub schema_version: i64,
    pub file_count: usize,
    pub created_at: String,
    pub version: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct BackupListResponse {
    pub artifacts: Vec<BackupArtifactView>,
    pub jobs: Vec<crate::storage::operations::OperationJob>,
    pub max_artifacts: usize,
    pub max_receipts: i64,
    pub active_artifact_count: usize,
}

pub(super) fn sha256_hex_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn download_backup_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: axum::http::Method,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &state.db).await?;
    if !crate::storage::operations::is_valid_job_id(&id) {
        return Err(ApiError::bad_request("Backup ID không hợp lệ."));
    }
    let job = crate::storage::operations::get_job(&state.db, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy bản sao lưu."))?;
    if job.kind != OperationKind::Backup
        || job.state != crate::storage::operations::OperationState::Succeeded
    {
        return Err(ApiError::not_found(
            "Bản sao lưu chưa hoàn tất hoặc không tồn tại.",
        ));
    }
    let meta: crate::backup::BackupSummaryMetadata = job
        .summary_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .ok_or_else(|| ApiError::not_found("Thiếu metadata bản sao lưu."))?;
    if meta.deleted {
        return Err(ApiError::not_found("Bản sao lưu đã bị xóa."));
    }

    let paths = state.coordinator.paths();
    let file_path = crate::backup::artifact_path_for_job(paths, &id);
    crate::secrets::ensure_no_reparse_or_symlink(&file_path)?;
    let file_meta = tokio::fs::metadata(&file_path)
        .await
        .map_err(|_| ApiError::not_found("Không tìm thấy tệp backup trên đĩa."))?;
    let total_len = file_meta.len();
    let filename = meta
        .archive_filename
        .unwrap_or_else(|| format!("opendash-backup-{}.odbackup.age", &id[..12]));

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/octet-stream"),
    );
    resp_headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    resp_headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    resp_headers.insert(
        axum::http::header::CONTENT_LENGTH,
        axum::http::HeaderValue::from_str(&total_len.to_string())
            .map_err(|_| ApiError::internal("Invalid content-length"))?,
    );
    let disp = format!("attachment; filename=\"{filename}\"");
    resp_headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_str(&disp)
            .map_err(|_| ApiError::internal("Invalid content-disposition"))?,
    );

    if method == axum::http::Method::HEAD {
        return Ok((StatusCode::OK, resp_headers, axum::body::Body::empty()));
    }

    let lease = state.backup_registry.acquire_download_lease(&id);
    let file = tokio::fs::File::open(&file_path)
        .await
        .map_err(|_| ApiError::not_found("Không mở được tệp backup."))?;

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(4);
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut file = file;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match file.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if tx
                        .send(Ok(axum::body::Bytes::copy_from_slice(&buf[..n])))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                    break;
                }
            }
        }
    });
    struct FileStreamBody {
        rx: tokio::sync::mpsc::Receiver<Result<axum::body::Bytes, std::io::Error>>,
        _lease: crate::backup::DownloadLeaseGuard,
    }
    impl http_body::Body for FileStreamBody {
        type Data = axum::body::Bytes;
        type Error = std::io::Error;
        fn poll_frame(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
            match self.rx.poll_recv(cx) {
                std::task::Poll::Ready(Some(Ok(bytes))) => {
                    std::task::Poll::Ready(Some(Ok(http_body::Frame::data(bytes))))
                }
                std::task::Poll::Ready(Some(Err(e))) => std::task::Poll::Ready(Some(Err(e))),
                std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
                std::task::Poll::Pending => std::task::Poll::Pending,
            }
        }
    }
    Ok((
        StatusCode::OK,
        resp_headers,
        axum::body::Body::new(FileStreamBody { rx, _lease: lease }),
    ))
}

#[derive(Debug, serde::Serialize)]
pub struct RestoreUploadResponse {
    pub upload_id: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRestoreRequest {
    pub upload_id: String,
    pub passphrase: String,
    pub request_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, serde::Serialize, Deserialize)]
pub struct RestoreSummaryMetadata {
    pub upload_id: String,
    pub request_id: String,
    pub target_root: String,
    pub schema_version: i64,
    pub db_sha256: String,
    pub file_count: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct RecoveryStatusResponse {
    pub safe_mode: bool,
    pub smtp_enabled: bool,
    pub ready_marker: Option<crate::restore::RestoreReadyMarker>,
    pub site_root: String,
    pub expected_generation: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivateRecoveryRequest {
    pub password: String,
    pub expected_generation: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ActivateRecoveryResponse {
    pub activated: bool,
    pub safe_mode: bool,
    pub generation: String,
    pub smtp_enabled: bool,
}

pub(super) async fn spawn_restore_worker_process(
    req: crate::backup_worker::RestoreWorkerRequest,
    wall_timeout: std::time::Duration,
    cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> ApiResult<crate::backup_worker::RestoreWorkerResponse> {
    use std::io::Write;

    let req_bytes = serde_json::to_vec(&req)
        .map_err(|_| ApiError::internal("Không tuần tự hóa được yêu cầu restore worker."))?;
    let exe = std::env::current_exe()
        .map_err(|_| ApiError::internal("Không xác định được đường dẫn thực thi."))?;

    let mut child = crate::operations_worker_process::spawn(&exe, "restore")
        .map_err(|_| ApiError::internal("Không khởi chạy được restore worker."))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| ApiError::internal("Không mở được stdin restore worker."))?;
    stdin
        .write_all(&req_bytes)
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|_| ApiError::internal("Không gửi được yêu cầu tới restore worker."))?;

    let output = tokio::task::spawn_blocking(move || {
        let start = std::time::Instant::now();
        loop {
            if cancel_flag.load(std::sync::atomic::Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                drop(stdin);
                return Err(ApiError::conflict("Tác vụ restore đã bị hủy bởi Admin."));
            }
            match child.try_wait() {
                Ok(Some(_)) => {
                    drop(stdin);
                    return child
                        .wait_with_output()
                        .map_err(|_| ApiError::internal("Không đọc được kết quả restore worker."));
                }
                Ok(None) => {
                    if start.elapsed() >= wall_timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        drop(stdin);
                        return Err(ApiError::service_unavailable(
                            "Tác vụ restore vượt quá thời gian cho phép.",
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    drop(stdin);
                    return Err(ApiError::internal(
                        "Lỗi theo dõi tiến trình restore worker.",
                    ));
                }
            }
        }
    })
    .await
    .map_err(|_| ApiError::internal("Lỗi đồng bộ restore worker."))??;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().last().unwrap_or("");
    let resp: crate::backup_worker::RestoreWorkerResponse = serde_json::from_str(line)
        .map_err(|_| ApiError::bad_request("Restore worker trả về phản hồi không hợp lệ."))?;
    if !resp.ok {
        return Err(ApiError::bad_request(
            resp.error
                .unwrap_or_else(|| "Restore thất bại hoặc sai passphrase.".into()),
        ));
    }
    Ok(resp)
}
