use super::*;

pub(super) fn map_maintenance_error(err: MaintenanceError) -> ApiError {
    match err {
        MaintenanceError::AlreadyActive => {
            ApiError::conflict("Đang có tác vụ bảo trì khác đang thực thi.")
        }
        MaintenanceError::DrainTimeout { .. } => {
            ApiError::service_unavailable("Hệ thống đang bận xử lý ghi; vui lòng thử lại sau.")
        }
        MaintenanceError::EpochOverflow => {
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
    let _ = state
        .catalog_registry
        .reload_site_with_active_packages(&state.db, paths)
        .await;
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
