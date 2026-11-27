use std::sync::Arc;

use axum::{
    Json,
    extract::{
        Extension, Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::{
    map_json_rejection, map_not_found_storage_error, map_query_rejection, map_storage_error,
};
use crate::{
    auth::{require_admin, require_admin_with_csrf},
    error::ApiResult,
    operations::AdmittedMutation,
    redirects::RedirectTelemetryBuffer,
    routes::settings::no_store_headers,
    storage::redirects::{
        NotFoundSort, bulk_delete_not_found_logs, clear_not_found_logs, delete_not_found_log,
        list_not_found_logs,
    },
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListNotFoundLogsQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub q: Option<String>,
    pub sort: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BulkDeleteNotFoundRequest {
    pub ids: Vec<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClearNotFoundRequest {
    pub older_than_days: Option<u32>,
}

#[derive(Serialize)]
pub struct DeletedCountResponse {
    pub deleted: usize,
}

pub async fn list_not_found_logs_handler(
    State(db): State<SqlitePool>,
    telemetry: Option<Extension<Arc<RedirectTelemetryBuffer>>>,
    headers: HeaderMap,
    query: Result<Query<ListNotFoundLogsQuery>, QueryRejection>,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &db).await?;
    let Query(q) = query.map_err(map_query_rejection)?;
    if let Some(Extension(buf)) = telemetry {
        let _ = buf.flush_now().await;
    }
    let sort = NotFoundSort::parse(q.sort.as_deref()).map_err(map_storage_error)?;
    let page = list_not_found_logs(
        &db,
        q.page.unwrap_or(1),
        q.per_page.unwrap_or(50),
        q.q.as_deref(),
        sort,
    )
    .await
    .map_err(map_storage_error)?;
    Ok((no_store_headers(None), Json(page)))
}

pub async fn delete_not_found_log_handler(
    AdmittedMutation(_lease): AdmittedMutation,
    State(db): State<SqlitePool>,
    telemetry: Option<Extension<Arc<RedirectTelemetryBuffer>>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    if let Some(Extension(buf)) = telemetry {
        let flush_guard = buf.lock_flush().await;
        let path = delete_not_found_log(&db, id)
            .await
            .map_err(map_not_found_storage_error)?;
        buf.evict_not_found_paths_locked(&flush_guard, &[path]);
    } else {
        delete_not_found_log(&db, id)
            .await
            .map_err(map_not_found_storage_error)?;
    }
    Ok((StatusCode::NO_CONTENT, no_store_headers(None)))
}

pub async fn bulk_delete_not_found_logs_handler(
    AdmittedMutation(_lease): AdmittedMutation,
    State(db): State<SqlitePool>,
    telemetry: Option<Extension<Arc<RedirectTelemetryBuffer>>>,
    headers: HeaderMap,
    payload: Result<Json<BulkDeleteNotFoundRequest>, JsonRejection>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let Json(req) = payload.map_err(map_json_rejection)?;
    let deleted = if let Some(Extension(buf)) = telemetry {
        let flush_guard = buf.lock_flush().await;
        let paths = bulk_delete_not_found_logs(&db, &req.ids)
            .await
            .map_err(map_storage_error)?;
        buf.evict_not_found_paths_locked(&flush_guard, &paths);
        paths.len()
    } else {
        bulk_delete_not_found_logs(&db, &req.ids)
            .await
            .map_err(map_storage_error)?
            .len()
    };
    Ok((
        no_store_headers(None),
        Json(DeletedCountResponse { deleted }),
    ))
}

pub async fn clear_not_found_logs_handler(
    AdmittedMutation(_lease): AdmittedMutation,
    State(db): State<SqlitePool>,
    telemetry: Option<Extension<Arc<RedirectTelemetryBuffer>>>,
    headers: HeaderMap,
    payload: Result<Json<ClearNotFoundRequest>, JsonRejection>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let Json(req) = payload.map_err(map_json_rejection)?;
    if let Some(days) = req.older_than_days {
        if days == 0 || days > 3650 {
            return Err(map_storage_error(
                crate::storage::redirects::RedirectStorageError::InvalidClearDays,
            ));
        }
    }
    let deleted = if let Some(Extension(buf)) = telemetry {
        let flush_guard = buf.lock_flush().await;
        match req.older_than_days {
            None => {
                let count = clear_not_found_logs(&db, None, None)
                    .await
                    .map_err(map_storage_error)?;
                buf.clear_pending_404s_locked(&flush_guard);
                count
            }
            Some(days) => {
                let pending = buf.drain_snapshot();
                clear_not_found_logs(&db, Some(days), Some(&pending))
                    .await
                    .map_err(map_storage_error)?
            }
        }
    } else {
        clear_not_found_logs(&db, req.older_than_days, None)
            .await
            .map_err(map_storage_error)?
    };
    Ok((
        no_store_headers(None),
        Json(DeletedCountResponse { deleted }),
    ))
}
