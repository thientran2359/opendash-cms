//! Admin-only selective sample import dry-run and G-RECOVERY execute routes (M5-S05-T01/T02).

use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::{Path, State, rejection::JsonRejection},
    http::HeaderMap,
};
use opendash_core::id::valid_kebab_segment_with_limit;
use sqlx::SqlitePool;

use crate::{
    auth::require_admin_with_csrf,
    error::{ApiError, ApiResult},
    media::MediaStore,
    routes::themes::ThemeStoreRoot,
    storage::import::{
        DryRunImportInput, ExecuteImportInput, ImportExecutionOutcome, ImportPlan,
        execute_sample_import, plan_sample_import,
    },
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

pub async fn import_execute(
    State(db): State<SqlitePool>,
    State(media_store): State<Arc<MediaStore>>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    Path(theme_id): Path<String>,
    request_headers: HeaderMap,
    input: Result<Json<ExecuteImportInput>, JsonRejection>,
) -> ApiResult<Json<ImportExecutionOutcome>> {
    require_admin_with_csrf(&request_headers, &db).await?;
    if !valid_kebab_segment_with_limit(&theme_id, 64) {
        return Err(ApiError::bad_request("theme_id không hợp lệ."));
    }
    let Json(input) = input.map_err(|_| ApiError::bad_request("Dữ liệu execute không hợp lệ."))?;
    let outcome = execute_sample_import(
        &db,
        &themes_root.0,
        &media_store.uploads_root(),
        &theme_id,
        &input,
    )
    .await?;
    Ok(Json(outcome))
}
