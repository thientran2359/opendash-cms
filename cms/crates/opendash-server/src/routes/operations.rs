//! Admin operations routes: reconcile plan/apply, quarantine purge, diagnostics (M8-S02-T02).

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::{require_admin, require_admin_with_csrf};
use crate::error::{ApiError, ApiResult};
use crate::media_lifecycle::{self, ApplyReconcileRequest};
use crate::operations::{MAX_MAINTENANCE_DRAIN, MaintenanceError};
use crate::storage::operations::OperationKind;

pub const MAX_OPERATIONS_JSON_BYTES: usize = 8 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeQuarantineRequest {
    pub confirm: bool,
}

fn map_maintenance_error(err: MaintenanceError) -> ApiError {
    match err {
        MaintenanceError::AlreadyActive => {
            ApiError::conflict("Đang có tác vụ bảo trì khác đang thực thi.")
        }
        MaintenanceError::DrainTimeout { .. } => {
            ApiError::service_unavailable("Hệ thống đang bận xử lý ghi; vui lòng thử lại sau.")
        }
    }
}

pub async fn diagnostics(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &state.db).await?;
    let secrets_dir = state.coordinator.paths().smtp_secrets_dir();
    let report = media_lifecycle::collect_operations_diagnostics(
        &state.db,
        &state.media_store,
        &secrets_dir,
    )
    .await?;
    Ok((StatusCode::OK, Json(report)))
}

pub async fn reconcile_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    let admin = require_admin_with_csrf(&headers, &state.db).await?;
    let maint = state
        .coordinator
        .begin_maintenance(OperationKind::Reconcile, MAX_MAINTENANCE_DRAIN)
        .await
        .map_err(map_maintenance_error)?;
    let secrets_dir = state.coordinator.paths().smtp_secrets_dir();
    let plan = media_lifecycle::plan_reconcile(
        &state.db,
        &state.media_store,
        &secrets_dir,
        Some(admin.user.id.to_string()),
        &maint,
    )
    .await?;
    Ok((StatusCode::OK, Json(plan)))
}

pub async fn reconcile_apply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ApplyReconcileRequest>,
) -> ApiResult<impl IntoResponse> {
    let admin = require_admin_with_csrf(&headers, &state.db).await?;
    let maint = state
        .coordinator
        .begin_maintenance(OperationKind::Reconcile, MAX_MAINTENANCE_DRAIN)
        .await
        .map_err(map_maintenance_error)?;
    let result = media_lifecycle::apply_reconcile(
        &state.db,
        &state.media_store,
        Some(admin.user.id.to_string()),
        req,
        &maint,
    )
    .await?;
    Ok((StatusCode::OK, Json(result)))
}

pub async fn quarantine_purge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PurgeQuarantineRequest>,
) -> ApiResult<impl IntoResponse> {
    let admin = require_admin_with_csrf(&headers, &state.db).await?;
    let maint = state
        .coordinator
        .begin_maintenance(OperationKind::Purge, MAX_MAINTENANCE_DRAIN)
        .await
        .map_err(map_maintenance_error)?;
    let secrets_dir = state.coordinator.paths().smtp_secrets_dir();
    let result = media_lifecycle::purge_quarantine(
        &state.db,
        &state.media_store,
        &secrets_dir,
        Some(admin.user.id.to_string()),
        req.confirm,
        &maint,
    )
    .await?;
    Ok((StatusCode::OK, Json(result)))
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

fn sha256_hex_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn count_existing_backup_artifacts(paths: &crate::paths::SitePaths) -> usize {
    let dir = paths.backups_artifacts_dir();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.flatten()
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.ends_with(".odbackup.age") && !n.starts_with('.'))
        })
        .count()
}

pub async fn create_backup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateBackupRequest>,
) -> ApiResult<impl IntoResponse> {
    let admin = require_admin_with_csrf(&headers, &state.db).await?;
    if req.passphrase != req.confirm_passphrase {
        return Err(ApiError::bad_request(
            "Mật khẩu xác nhận không khớp với mật khẩu backup.",
        ));
    }
    let passphrase = age::secrecy::SecretString::from(req.passphrase);
    crate::backup_archive::validate_passphrase(&passphrase)?;

    let request_id = match req.request_id {
        Some(raw) => {
            if !crate::storage::operations::is_valid_job_id(&raw) {
                return Err(ApiError::bad_request("request_id phải là 64 ký tự hex."));
            }
            raw
        }
        None => crate::auth::generate_random_token(),
    };
    let actor_day = crate::backup::utc_day_string();
    let nonsecret_canonical = serde_json::json!({
        "kind": "backup",
        "reason": req.reason,
    });
    let nonsecret_digest = sha256_hex_bytes(nonsecret_canonical.to_string().as_bytes());
    let actor_id_str = admin.user.id.to_string();

    // Idempotency check by (actor, UTC day, request_id) without comparing passphrase text.
    let existing_jobs = crate::storage::operations::list_backup_jobs(&state.db, 200).await?;
    for job in existing_jobs {
        if job.actor_user_id.as_deref() != Some(actor_id_str.as_str()) {
            continue;
        }
        let Some(ref raw_sum) = job.summary_json else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<crate::backup::BackupSummaryMetadata>(raw_sum) else {
            continue;
        };
        if meta.request_id == request_id && meta.actor_day == actor_day {
            if matches!(
                job.state,
                crate::storage::operations::OperationState::Queued
                    | crate::storage::operations::OperationState::Running
                    | crate::storage::operations::OperationState::Succeeded
            ) {
                if meta.nonsecret_digest != nonsecret_digest {
                    return Err(ApiError::conflict(
                        "request_id đã được dùng với tham số cấu hình khác.",
                    ));
                }
                return Ok((StatusCode::ACCEPTED, Json(job)));
            }
        }
    }

    let paths = state.coordinator.paths().clone();
    if count_existing_backup_artifacts(&paths) >= crate::backup::MAX_BACKUP_ARTIFACTS {
        return Err(ApiError(
            StatusCode::INSUFFICIENT_STORAGE,
            "Đã đạt giới hạn tối đa 10 gói backup; vui lòng xóa bớt trước khi tạo mới.".into(),
        ));
    }

    let maint = state
        .coordinator
        .begin_maintenance(OperationKind::Backup, MAX_MAINTENANCE_DRAIN)
        .await
        .map_err(map_maintenance_error)?;

    let job_id = crate::auth::generate_random_token();
    let job = crate::storage::operations::create_job(
        &state.db,
        crate::storage::operations::JobCreateInput {
            id: job_id.clone(),
            kind: OperationKind::Backup,
            actor_user_id: Some(actor_id_str),
            reason: req.reason,
        },
    )
    .await?;

    let summary_meta = crate::backup::BackupSummaryMetadata {
        request_id,
        actor_day,
        nonsecret_digest,
        archive_filename: None,
        archive_size: None,
        archive_sha256: None,
        schema_version: None,
        file_count: None,
        deleted: false,
    };
    let running_job = crate::storage::operations::transition_job_cas(
        &state.db,
        &job_id,
        crate::storage::operations::JobTransitionInput {
            expected_version: job.version,
            next_state: crate::storage::operations::OperationState::Running,
            summary_json: Some(serde_json::to_string(&summary_meta).unwrap_or_default()),
            error_code: None,
            error_message: None,
        },
    )
    .await?;

    let cancel_flag = state.backup_registry.register_cancel_flag(&job_id);
    let db_clone = state.db.clone();
    let reg_clone = std::sync::Arc::clone(&state.backup_registry);
    let job_id_clone = job_id.clone();
    let actor_user_id = admin.user.id;
    let running_version = running_job.version;

    let handle = tokio::spawn(async move {
        crate::backup::run_backup_job_background(
            db_clone,
            paths,
            reg_clone,
            job_id_clone,
            running_version,
            actor_user_id,
            summary_meta,
            passphrase,
            maint,
            cancel_flag,
        )
        .await;
    });
    state
        .backup_registry
        .register_task_handle(&job_id, handle)
        .await;

    Ok((StatusCode::ACCEPTED, Json(running_job)))
}

pub async fn get_operation_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &state.db).await?;
    if !crate::storage::operations::is_valid_job_id(&id) {
        return Err(ApiError::bad_request("JobId không hợp lệ."));
    }
    let job = crate::storage::operations::get_job(&state.db, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy tác vụ vận hành."))?;
    Ok((StatusCode::OK, Json(job)))
}

pub async fn cancel_operation_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    req: Option<Json<CancelJobRequest>>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &state.db).await?;
    if !crate::storage::operations::is_valid_job_id(&id) {
        return Err(ApiError::bad_request("JobId không hợp lệ."));
    }
    let job = crate::storage::operations::get_job(&state.db, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy tác vụ vận hành."))?;
    if let Some(Json(body)) = req
        && let Some(expected) = body.expected_version
        && job.version != expected
    {
        return Err(ApiError::precondition_failed(
            "Phiên bản tác vụ đã thay đổi (CAS mismatch).",
        ));
    }
    if job.state.is_terminal() {
        return Err(ApiError::precondition_failed(
            "Tác vụ đã ở trạng thái kết thúc.",
        ));
    }

    let _signalled = state.backup_registry.request_cancel(&id);
    state.backup_registry.join_task(&id).await;

    let latest = crate::storage::operations::get_job(&state.db, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy tác vụ vận hành."))?;
    if !latest.state.is_terminal() {
        let updated = crate::storage::operations::transition_job_cas(
            &state.db,
            &id,
            crate::storage::operations::JobTransitionInput {
                expected_version: latest.version,
                next_state: crate::storage::operations::OperationState::Cancelled,
                summary_json: None,
                error_code: Some("cancelled_by_admin".to_string()),
                error_message: Some("Tác vụ đã bị hủy bởi Admin.".to_string()),
            },
        )
        .await?;
        return Ok((StatusCode::OK, Json(updated)));
    }
    Ok((StatusCode::OK, Json(latest)))
}

pub async fn list_backups(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<ListBackupsQuery>,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &state.db).await?;
    let limit = query.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("Tham số limit phải từ 1 đến 100."));
    }

    let jobs = crate::storage::operations::list_backup_jobs(&state.db, limit).await?;
    let paths = state.coordinator.paths();
    let mut artifacts = Vec::new();
    for job in &jobs {
        if job.state != crate::storage::operations::OperationState::Succeeded {
            continue;
        }
        let Some(ref raw_sum) = job.summary_json else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<crate::backup::BackupSummaryMetadata>(raw_sum) else {
            continue;
        };
        if meta.deleted {
            continue;
        }
        let path = crate::backup::artifact_path_for_job(paths, &job.id);
        if !path.is_file() {
            continue;
        }
        if let (
            Some(filename),
            Some(size_bytes),
            Some(sha256),
            Some(schema_version),
            Some(file_count),
        ) = (
            meta.archive_filename,
            meta.archive_size,
            meta.archive_sha256,
            meta.schema_version,
            meta.file_count,
        ) {
            artifacts.push(BackupArtifactView {
                id: job.id.clone(),
                filename,
                size_bytes,
                sha256,
                schema_version,
                file_count,
                created_at: job
                    .finished_at
                    .clone()
                    .unwrap_or_else(|| job.requested_at.clone()),
                version: job.version,
            });
        }
    }

    let active_artifact_count = count_existing_backup_artifacts(paths);
    Ok((
        StatusCode::OK,
        Json(BackupListResponse {
            artifacts,
            jobs,
            max_artifacts: crate::backup::MAX_BACKUP_ARTIFACTS,
            max_receipts: crate::storage::operations::MAX_RETAINED_OPERATION_RECEIPTS,
            active_artifact_count,
        }),
    ))
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

pub async fn delete_backup_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &state.db).await?;
    if !crate::storage::operations::is_valid_job_id(&id) {
        return Err(ApiError::bad_request("Backup ID không hợp lệ."));
    }
    let job = crate::storage::operations::get_job(&state.db, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy bản sao lưu."))?;
    if job.kind != OperationKind::Backup
        || job.state != crate::storage::operations::OperationState::Succeeded
    {
        return Err(ApiError::precondition_failed(
            "Chỉ có thể xóa bản sao lưu đã hoàn tất.",
        ));
    }
    let mut meta: crate::backup::BackupSummaryMetadata = job
        .summary_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .ok_or_else(|| ApiError::not_found("Thiếu metadata bản sao lưu."))?;
    if meta.deleted {
        return Err(ApiError::not_found("Bản sao lưu đã bị xóa trước đó."));
    }

    state
        .backup_registry
        .wait_for_download_leases_drain(&id, std::time::Duration::from_secs(10))
        .await?;

    meta.deleted = true;
    let updated_json = serde_json::to_string(&meta)
        .map_err(|_| ApiError::internal("Không tuần tự hóa được metadata xóa."))?;
    let updated_job = crate::storage::operations::mark_backup_artifact_deleted_cas(
        &state.db,
        &id,
        job.version,
        &updated_json,
    )
    .await?;

    let file_path = crate::backup::artifact_path_for_job(state.coordinator.paths(), &id);
    if file_path.exists() {
        let _ = tokio::fs::remove_file(&file_path).await;
    }

    Ok((StatusCode::OK, Json(updated_job)))
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

pub async fn upload_restore_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    body: axum::body::Body,
) -> ApiResult<impl IntoResponse> {
    upload_restore_archive_with_timeout(state, headers, body, std::time::Duration::from_secs(300))
        .await
}

pub async fn upload_restore_archive_with_timeout(
    state: AppState,
    headers: HeaderMap,
    mut body: axum::body::Body,
    upload_timeout: std::time::Duration,
) -> ApiResult<impl IntoResponse> {
    use sha2::{Digest, Sha256};
    use std::io::Write;

    require_admin_with_csrf(&headers, &state.db).await?;
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .eq_ignore_ascii_case("application/octet-stream")
    {
        return Err(ApiError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Chỉ chấp nhận application/octet-stream cho gói restore.".into(),
        ));
    }

    if let Some(len) = headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        && len > crate::backup_archive::MAX_ENCRYPTED_ARCHIVE_BYTES
    {
        return Err(ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Kích thước gói backup vượt quá giới hạn cho phép.".into(),
        ));
    }

    let upload_id = crate::auth::generate_random_token();
    let uploads_dir = state
        .coordinator
        .paths()
        .backups_staging_dir()
        .join("restore-uploads");
    crate::secrets::ensure_private_directory_tree(&uploads_dir)?;

    let part_path = uploads_dir.join(format!(".{upload_id}.part"));
    let final_path = uploads_dir.join(format!("{upload_id}.odbackup.age"));
    let mut file = crate::secrets::create_private_file(&part_path)?;
    let mut hasher = Sha256::new();
    let mut total_bytes: u64 = 0;

    let upload_deadline = tokio::time::Instant::now() + upload_timeout;
    loop {
        let frame_res = tokio::time::timeout_at(
            upload_deadline,
            std::future::poll_fn(|cx| {
                <axum::body::Body as http_body::Body>::poll_frame(std::pin::Pin::new(&mut body), cx)
            }),
        )
        .await;
        let frame_opt = match frame_res {
            Ok(opt) => opt,
            Err(_) => {
                drop(file);
                let _ = std::fs::remove_file(&part_path);
                return Err(ApiError(
                    StatusCode::REQUEST_TIMEOUT,
                    "Quá thời gian tải lên gói restore.".into(),
                ));
            }
        };
        match frame_opt {
            Some(Ok(frame)) => {
                if let Ok(data) = frame.into_data() {
                    let next_total = total_bytes.saturating_add(data.len() as u64);
                    if next_total > crate::backup_archive::MAX_ENCRYPTED_ARCHIVE_BYTES {
                        drop(file);
                        let _ = std::fs::remove_file(&part_path);
                        return Err(ApiError(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            "Kích thước gói backup vượt quá giới hạn cho phép.".into(),
                        ));
                    }
                    for chunk in data.chunks(64 * 1024) {
                        if file.write_all(chunk).is_err() {
                            drop(file);
                            let _ = std::fs::remove_file(&part_path);
                            return Err(ApiError::internal("Không ghi được tệp upload restore."));
                        }
                        hasher.update(chunk);
                    }
                    total_bytes = next_total;
                }
            }
            Some(Err(_)) => {
                drop(file);
                let _ = std::fs::remove_file(&part_path);
                return Err(ApiError::bad_request("Luồng upload bị gián đoạn."));
            }
            None => break,
        }
    }

    if total_bytes == 0 {
        drop(file);
        let _ = std::fs::remove_file(&part_path);
        return Err(ApiError::bad_request("Tệp backup rỗng."));
    }
    if file.sync_all().is_err() {
        drop(file);
        let _ = std::fs::remove_file(&part_path);
        return Err(ApiError::internal("Không đồng bộ được tệp upload restore."));
    }
    drop(file);
    std::fs::rename(&part_path, &final_path).map_err(|_| {
        let _ = std::fs::remove_file(&part_path);
        ApiError::internal("Không hoàn tất được tệp upload restore.")
    })?;

    let sha256: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((
        StatusCode::CREATED,
        Json(RestoreUploadResponse {
            upload_id,
            size_bytes: total_bytes,
            sha256,
        }),
    ))
}

async fn spawn_restore_worker_process(
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

pub async fn create_restore(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateRestoreRequest>,
) -> ApiResult<impl IntoResponse> {
    let admin = require_admin_with_csrf(&headers, &state.db).await?;
    if !crate::storage::operations::is_valid_job_id(&req.upload_id) {
        return Err(ApiError::bad_request("upload_id phải là 64 ký tự hex."));
    }
    if req.passphrase.is_empty() {
        return Err(ApiError::bad_request("Passphrase không được để trống."));
    }
    let request_id = match req.request_id {
        Some(raw) => {
            if !crate::storage::operations::is_valid_job_id(&raw) {
                return Err(ApiError::bad_request("request_id phải là 64 ký tự hex."));
            }
            raw
        }
        None => crate::auth::generate_random_token(),
    };

    let paths = state.coordinator.paths().clone();
    let upload_path = paths
        .backups_staging_dir()
        .join("restore-uploads")
        .join(format!("{}.odbackup.age", req.upload_id));
    crate::secrets::ensure_no_reparse_or_symlink(&upload_path)?;
    if !upload_path.is_file() {
        return Err(ApiError::not_found("Không tìm thấy gói backup đã tải lên."));
    }

    let job_id = crate::auth::generate_random_token();
    let job = crate::storage::operations::create_job(
        &state.db,
        crate::storage::operations::JobCreateInput {
            id: job_id.clone(),
            kind: OperationKind::Restore,
            actor_user_id: Some(admin.user.id.to_string()),
            reason: req.reason,
        },
    )
    .await?;

    let running_job = crate::storage::operations::transition_job_cas(
        &state.db,
        &job_id,
        crate::storage::operations::JobTransitionInput {
            expected_version: job.version,
            next_state: crate::storage::operations::OperationState::Running,
            summary_json: None,
            error_code: None,
            error_message: None,
        },
    )
    .await?;

    let target_root = paths.backups_restored_dir().join(&job_id);
    let private_dir = paths
        .backups_staging_dir()
        .join(format!("restore-{job_id}"));
    let db_clone = state.db.clone();
    let reg_clone = std::sync::Arc::clone(&state.backup_registry);
    let job_id_clone = job_id.clone();
    let upload_id_clone = req.upload_id.clone();
    let running_version = running_job.version;

    let worker_req = crate::backup_worker::RestoreWorkerRequest {
        input_path: upload_path.clone(),
        private_dir: private_dir.clone(),
        generated_target: target_root.clone(),
        source_site_root: paths.site_root().to_path_buf(),
        source_db_path: paths.db_path().to_path_buf(),
        job_id: job_id.clone(),
        passphrase: req.passphrase,
        failpoint: None,
    };

    let cancel_flag = state.backup_registry.register_cancel_flag(&job_id);
    let cancel_flag_task = std::sync::Arc::clone(&cancel_flag);
    let handle = tokio::spawn(async move {
        let res = spawn_restore_worker_process(
            worker_req,
            std::time::Duration::from_secs(300),
            std::sync::Arc::clone(&cancel_flag_task),
        )
        .await;
        let _ = std::fs::remove_dir_all(&private_dir);
        let _ = std::fs::remove_file(&upload_path);
        if cancel_flag_task.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = std::fs::remove_dir_all(&target_root);
            let _ = crate::storage::operations::transition_job_cas(
                &db_clone,
                &job_id_clone,
                crate::storage::operations::JobTransitionInput {
                    expected_version: running_version,
                    next_state: crate::storage::operations::OperationState::Cancelled,
                    summary_json: None,
                    error_code: Some("cancelled_by_admin".into()),
                    error_message: Some("Tác vụ đã bị hủy bởi Admin.".into()),
                },
            )
            .await;
        } else {
            match res {
                Ok(resp) => {
                    let summary = RestoreSummaryMetadata {
                        upload_id: upload_id_clone,
                        request_id,
                        target_root: resp
                            .target_root
                            .unwrap_or(target_root)
                            .to_string_lossy()
                            .into_owned(),
                        schema_version: resp.schema_version.unwrap_or(0),
                        db_sha256: resp.db_sha256.unwrap_or_default(),
                        file_count: resp.file_count.unwrap_or(0),
                    };
                    let _ = crate::storage::operations::transition_job_cas(
                        &db_clone,
                        &job_id_clone,
                        crate::storage::operations::JobTransitionInput {
                            expected_version: running_version,
                            next_state: crate::storage::operations::OperationState::Succeeded,
                            summary_json: Some(serde_json::to_string(&summary).unwrap_or_default()),
                            error_code: None,
                            error_message: None,
                        },
                    )
                    .await;
                }
                Err(err) => {
                    let _ = crate::storage::operations::transition_job_cas(
                        &db_clone,
                        &job_id_clone,
                        crate::storage::operations::JobTransitionInput {
                            expected_version: running_version,
                            next_state: crate::storage::operations::OperationState::Failed,
                            summary_json: None,
                            error_code: Some("restore_failed".into()),
                            error_message: Some(err.1.into_owned()),
                        },
                    )
                    .await;
                }
            }
        }
        reg_clone.clear_job(&job_id_clone);
    });
    state
        .backup_registry
        .register_task_handle(&job_id, handle)
        .await;

    Ok((StatusCode::ACCEPTED, Json(running_job)))
}

pub async fn get_recovery_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &state.db).await?;
    let paths = state.coordinator.paths();
    let ready_marker = crate::config::load_ready_marker(paths)?;
    let smtp = crate::storage::smtp::get_smtp_settings(&state.db).await?;
    let expected_generation = ready_marker.as_ref().map(|m| m.job_id.clone());

    Ok((
        StatusCode::OK,
        Json(RecoveryStatusResponse {
            safe_mode: state.recovery.is_safe_mode(),
            smtp_enabled: smtp.enabled,
            ready_marker,
            site_root: paths.site_root().to_string_lossy().into_owned(),
            expected_generation,
        }),
    ))
}

pub async fn activate_recovery(
    State(state): State<AppState>,
    headers: HeaderMap,
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    Json(req): Json<ActivateRecoveryRequest>,
) -> ApiResult<impl IntoResponse> {
    let admin = require_admin_with_csrf(&headers, &state.db).await?;
    let paths = state.coordinator.paths();
    let marker = crate::config::load_ready_marker(paths)?.ok_or_else(|| {
        ApiError::precondition_failed("Không tìm thấy READY.json trong site root để kích hoạt.")
    })?;
    if marker.job_id != req.expected_generation {
        return Err(ApiError::precondition_failed(
            "Generation READY.json không khớp (CAS mismatch).",
        ));
    }

    let user_row = crate::storage::auth::find_user_by_username(&state.db, &admin.user.username)
        .await?
        .ok_or_else(|| ApiError::unauthorized("Không tìm thấy tài khoản Admin hiện tại."))?;
    if !crate::auth::verify_password(&req.password, &user_row.2) {
        return Err(ApiError::unauthorized("Mật khẩu Admin không chính xác."));
    }

    crate::config::persist_clear_safe_mode(paths)?;
    state.recovery.set_safe_mode(false);
    let smtp = crate::storage::smtp::get_smtp_settings(&state.db).await?;

    Ok((
        StatusCode::OK,
        Json(ActivateRecoveryResponse {
            activated: true,
            safe_mode: false,
            generation: marker.job_id,
            smtp_enabled: smtp.enabled,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    async fn test_app_state(label: &str, safe_mode: bool) -> (std::path::PathBuf, AppState) {
        let root =
            std::env::temp_dir().join(format!("opendash-ops-unit-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let site_root = root.join("site");
        let db_path = root.join("cms.sqlite");
        std::fs::create_dir_all(&site_root).expect("site_root");

        let paths = crate::paths::SitePaths::from_parts(site_root, db_path);
        std::fs::create_dir_all(paths.uploads_dir()).expect("uploads dir");
        std::fs::create_dir_all(paths.plugins_dir()).expect("plugins dir");
        std::fs::create_dir_all(paths.runtime_dir()).expect("runtime dir");
        let coordinator = Arc::new(
            crate::operations::SiteCoordinator::acquire(paths.clone()).expect("acquire coord"),
        );
        let db = crate::storage::init_db(paths.db_path())
            .await
            .expect("init_db");
        let runtime_config =
            crate::config::RuntimeConfigV1::load(&paths).expect("load runtime config");
        let recovery = crate::config::RecoveryState::new(safe_mode);
        let plugin_manager = Arc::new(
            crate::plugin::PluginManager::new(db.clone(), false)
                .with_recovery_state(recovery.clone()),
        );
        let hooks = Arc::new(crate::hooks::HookDispatcher::new(plugin_manager.clone()));
        let content_registry = Arc::new(tokio::sync::RwLock::new(
            crate::storage::content::load_content_type_catalog(&db)
                .await
                .expect("load content catalog"),
        ));

        let state = AppState {
            db,
            plugin_manager,
            hooks,
            media_store: Arc::new(crate::media::MediaStore::new(paths.uploads_dir())),
            content_registry,
            coordinator,
            backup_registry: Arc::new(crate::backup::BackupRuntimeRegistry::new()),
            block_template_cache: crate::page::BlockTemplateCache::global().clone(),
            recovery,
            runtime_config,
        };
        (root, state)
    }

    #[tokio::test]
    async fn r1_activate_recovery_blocked_during_maintenance_returns_503_and_preserves_safe_mode() {
        let (root, state) = test_app_state("r1-barrier", true).await;
        let paths = state.coordinator.paths().clone();

        // Write READY.json and config/site.json with recovery_safe_mode = true
        let ready = crate::restore::RestoreReadyMarker {
            version: 1,
            job_id: "a".repeat(64),
            schema_version: 1,
            db_sha256: "b".repeat(64),
            file_count: 1,
            recovery_safe_mode: true,
        };
        std::fs::write(
            paths.site_root().join("READY.json"),
            serde_json::to_vec(&ready).expect("ready json"),
        )
        .expect("write READY.json");
        std::fs::create_dir_all(paths.site_config_path().parent().unwrap()).expect("config dir");
        let mut portable = opendash_core::backup::PortableRuntimeConfigV1::default();
        portable.recovery_safe_mode = true;
        std::fs::write(
            paths.site_config_path(),
            serde_json::to_vec(&portable).expect("site config"),
        )
        .expect("write site.json");

        // Hold exclusive maintenance barrier
        let maint = state
            .coordinator
            .begin_maintenance(OperationKind::Backup, Duration::from_secs(5))
            .await
            .expect("enter maintenance");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let router = crate::app::build_router(state.clone(), addr.to_string());
        let srv = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await;
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(format!(
                "http://{addr}/api/admin/operations/recovery/activate"
            ))
            .json(&serde_json::json!({
                "expected_generation": "a".repeat(64),
                "password": "Password123!"
            }))
            .send()
            .await
            .expect("activate request");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            resp.headers().get("retry-after").is_some(),
            "503 response must include Retry-After header"
        );

        // Verify safe_mode and config/site.json were NOT modified
        assert!(state.recovery.is_safe_mode());
        let on_disk: opendash_core::backup::PortableRuntimeConfigV1 = serde_json::from_slice(
            &std::fs::read(paths.site_config_path()).expect("read site.json"),
        )
        .expect("parse site.json");
        assert!(on_disk.recovery_safe_mode);

        drop(maint);
        srv.abort();
        state.db.close().await;
        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }

    struct StalledBody;

    impl http_body::Body for StalledBody {
        type Data = axum::body::Bytes;
        type Error = std::io::Error;

        fn poll_frame(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
            std::task::Poll::Pending
        }
    }

    #[tokio::test]
    async fn r3_upload_restore_archive_stalled_stream_times_out_and_cleans_part_file() {
        let (root, state) = test_app_state("r3-upload-timeout", false).await;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let router = crate::app::build_router(state.clone(), addr.to_string());
        let srv = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await;
        });

        let client = reqwest::Client::new();
        let setup = client
            .post(format!("http://{addr}/auth/setup"))
            .json(&serde_json::json!({ "username": "admin", "password": "Password123!" }))
            .send()
            .await
            .expect("setup");
        assert_eq!(setup.status(), StatusCode::CREATED);

        let login = client
            .post(format!("http://{addr}/auth/login"))
            .json(&serde_json::json!({ "username": "admin", "password": "Password123!" }))
            .send()
            .await
            .expect("login");
        assert_eq!(login.status(), StatusCode::OK);
        let cookie = login
            .headers()
            .get(reqwest::header::SET_COOKIE)
            .expect("set-cookie")
            .to_str()
            .expect("cookie str")
            .split(';')
            .next()
            .expect("cookie pair")
            .to_string();
        let login_json: serde_json::Value = login.json().await.expect("login json");
        let csrf = login_json["csrf_token"]
            .as_str()
            .expect("csrf_token")
            .to_string();
        srv.abort();

        let mut headers = HeaderMap::new();
        headers.insert(axum::http::header::COOKIE, cookie.parse().unwrap());
        headers.insert("X-CSRF-Token", csrf.parse().unwrap());
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/octet-stream".parse().unwrap(),
        );

        let body = axum::body::Body::new(StalledBody);
        let res = upload_restore_archive_with_timeout(
            state.clone(),
            headers,
            body,
            Duration::from_millis(50),
        )
        .await;
        match res {
            Err(err) => assert_eq!(err.0, StatusCode::REQUEST_TIMEOUT),
            Ok(_) => panic!("stalled upload must time out"),
        }

        let uploads_dir = state
            .coordinator
            .paths()
            .backups_staging_dir()
            .join("restore-uploads");
        let remaining: Vec<_> = std::fs::read_dir(&uploads_dir)
            .expect("read uploads dir")
            .collect();
        assert!(
            remaining.is_empty(),
            "timed out upload must clean up .part file"
        );

        state.db.close().await;
        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }
}
