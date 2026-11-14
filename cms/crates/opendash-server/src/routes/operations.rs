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
