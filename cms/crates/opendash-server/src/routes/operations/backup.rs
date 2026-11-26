use super::*;

pub(super) fn count_existing_backup_artifacts(paths: &crate::paths::SitePaths) -> usize {
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
        dev_mode_active: crate::dev_mode::DevOverlayConfig::global().is_some(),
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
