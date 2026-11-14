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
use crate::media_lifecycle::{
    self, ApplyReconcileRequest,
};
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
    let report =
        media_lifecycle::collect_operations_diagnostics(&state.db, &state.media_store, &secrets_dir)
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
        if let (Some(filename), Some(size_bytes), Some(sha256), Some(schema_version), Some(file_count)) = (
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
                created_at: job.finished_at.clone().unwrap_or_else(|| job.requested_at.clone()),
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
        return Err(ApiError::not_found("Bản sao lưu chưa hoàn tất hoặc không tồn tại."));
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
                    if tx.send(Ok(axum::body::Bytes::copy_from_slice(&buf[..n]))).await.is_err() { break; }
                }
                Err(e) => { let _ = tx.send(Err(e)).await; break; }
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
        fn poll_frame(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
            match self.rx.poll_recv(cx) {
                std::task::Poll::Ready(Some(Ok(bytes))) => std::task::Poll::Ready(Some(Ok(http_body::Frame::data(bytes)))),
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
