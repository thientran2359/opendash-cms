//! M3-S26: a session-scoped snapshot of built-in Post/Page publication counts.
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use sqlx::SqlitePool;

use crate::{auth::require_session, error::ApiResult, storage::dashboard};

pub async fn dashboard_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    // Editors intentionally see the same built-in entries as Admins per ADR-0004.
    require_session(&headers, &db).await?;
    let counts = dashboard::get_counts(&db).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(counts)).into_response())
}
