//! Admin-only selective sample import dry-run route (M5-S05-T01).

use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
    http::HeaderMap,
};
use opendash_core::id::valid_kebab_segment_with_limit;
use sqlx::SqlitePool;

use crate::{
    auth::require_admin_with_csrf,
    error::{ApiError, ApiResult},
    storage::import::{DryRunImportInput, ImportPlan, plan_sample_import},
};

pub async fn import_dry_run(
    State(db): State<SqlitePool>,
    Path(theme_id): Path<String>,
    request_headers: HeaderMap,
    input: Result<Json<DryRunImportInput>, JsonRejection>,
) -> ApiResult<Json<ImportPlan>> {
    require_admin_with_csrf(&request_headers, &db).await?;
    if !valid_kebab_segment_with_limit(&theme_id, 64) {
        return Err(ApiError::bad_request("theme_id không hợp lệ."));
    }
    let Json(input) = input.map_err(|_| ApiError::bad_request("Dữ liệu dry-run không hợp lệ."))?;
    let plan = plan_sample_import(&db, &theme_id, &input).await?;
    Ok(Json(plan))
}
