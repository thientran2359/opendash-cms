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
use serde::Deserialize;
use sqlx::SqlitePool;

use super::{
    extract_public_origin_from_headers, map_json_rejection, map_query_rejection, map_storage_error,
};
use crate::{
    auth::{require_admin, require_admin_with_csrf},
    error::ApiResult,
    operations::AdmittedMutation,
    redirects::RedirectTelemetryBuffer,
    routes::settings::{no_store_headers, parse_if_match},
    storage::redirects::{
        RedirectListFilter, create_manual_redirect, delete_manual_redirect,
        get_manual_redirect_by_id, list_manual_redirects, update_manual_redirect_cas,
    },
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRedirectsQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub q: Option<String>,
    pub status: Option<u16>,
    pub enabled: Option<bool>,
    pub shadowed: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateManualRedirectRequest {
    pub source_path: String,
    pub target_url: String,
    pub status_code: u16,
    pub enabled: Option<bool>,
    pub note: Option<String>,
    pub resolve_not_found: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateManualRedirectRequest {
    pub source_path: String,
    pub target_url: String,
    pub status_code: u16,
    pub enabled: bool,
    pub note: Option<String>,
}

pub async fn list_redirects(
    State(db): State<SqlitePool>,
    telemetry: Option<Extension<Arc<RedirectTelemetryBuffer>>>,
    headers: HeaderMap,
    query: Result<Query<ListRedirectsQuery>, QueryRejection>,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &db).await?;
    let Query(q) = query.map_err(map_query_rejection)?;
    if let Some(Extension(buf)) = telemetry {
        let _ = buf.flush_now().await;
    }
    let filter = RedirectListFilter {
        page: q.page.unwrap_or(1),
        per_page: q.per_page.unwrap_or(50),
        q: q.q,
        status: q.status,
        enabled: q.enabled,
        shadowed: q.shadowed,
    };
    let page = list_manual_redirects(&db, &filter)
        .await
        .map_err(map_storage_error)?;
    Ok((no_store_headers(None), Json(page)))
}

pub async fn get_redirect(
    State(db): State<SqlitePool>,
    telemetry: Option<Extension<Arc<RedirectTelemetryBuffer>>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &db).await?;
    if let Some(Extension(buf)) = telemetry {
        let _ = buf.flush_now().await;
    }
    let item = get_manual_redirect_by_id(&db, id)
        .await
        .map_err(map_storage_error)?;
    Ok((no_store_headers(Some(item.version)), Json(item)))
}

pub async fn create_redirect(
    AdmittedMutation(_lease): AdmittedMutation,
    State(db): State<SqlitePool>,
    telemetry: Option<Extension<Arc<RedirectTelemetryBuffer>>>,
    headers: HeaderMap,
    payload: Result<Json<CreateManualRedirectRequest>, JsonRejection>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let Json(input) = payload.map_err(map_json_rejection)?;
    let resolve_not_found = input.resolve_not_found.unwrap_or(true);
    let enabled = input.enabled.unwrap_or(true);
    let public_origin = extract_public_origin_from_headers(&headers);

    let item = if let Some(Extension(buf)) = telemetry {
        let flush_guard = buf.lock_flush().await;
        let created = create_manual_redirect(
            &db,
            &input.source_path,
            &input.target_url,
            input.status_code,
            enabled,
            input.note.as_deref(),
            resolve_not_found,
            public_origin.as_deref(),
        )
        .await
        .map_err(map_storage_error)?;
        if resolve_not_found {
            buf.evict_not_found_paths_locked(
                &flush_guard,
                std::slice::from_ref(&created.source_path),
            );
        }
        created
    } else {
        create_manual_redirect(
            &db,
            &input.source_path,
            &input.target_url,
            input.status_code,
            enabled,
            input.note.as_deref(),
            resolve_not_found,
            public_origin.as_deref(),
        )
        .await
        .map_err(map_storage_error)?
    };
    Ok((
        StatusCode::CREATED,
        no_store_headers(Some(item.version)),
        Json(item),
    ))
}

pub async fn update_redirect(
    AdmittedMutation(_lease): AdmittedMutation,
    State(db): State<SqlitePool>,
    telemetry: Option<Extension<Arc<RedirectTelemetryBuffer>>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    payload: Result<Json<UpdateManualRedirectRequest>, JsonRejection>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let expected_version = parse_if_match(&headers)?;
    let Json(input) = payload.map_err(map_json_rejection)?;
    let public_origin = extract_public_origin_from_headers(&headers);

    let item = if let Some(Extension(buf)) = telemetry {
        let flush_guard = buf.lock_flush().await;
        let outcome = update_manual_redirect_cas(
            &db,
            id,
            expected_version,
            &input.source_path,
            &input.target_url,
            input.status_code,
            input.enabled,
            input.note.as_deref(),
            public_origin.as_deref(),
        )
        .await
        .map_err(map_storage_error)?;
        if outcome.should_evict_hits {
            buf.evict_redirect_id_locked(&flush_guard, id);
        }
        outcome.item
    } else {
        update_manual_redirect_cas(
            &db,
            id,
            expected_version,
            &input.source_path,
            &input.target_url,
            input.status_code,
            input.enabled,
            input.note.as_deref(),
            public_origin.as_deref(),
        )
        .await
        .map_err(map_storage_error)?
        .item
    };
    Ok((no_store_headers(Some(item.version)), Json(item)))
}

pub async fn delete_redirect(
    AdmittedMutation(_lease): AdmittedMutation,
    State(db): State<SqlitePool>,
    telemetry: Option<Extension<Arc<RedirectTelemetryBuffer>>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    if let Some(Extension(buf)) = telemetry {
        let flush_guard = buf.lock_flush().await;
        delete_manual_redirect(&db, id)
            .await
            .map_err(map_storage_error)?;
        buf.evict_redirect_id_locked(&flush_guard, id);
    } else {
        delete_manual_redirect(&db, id)
            .await
            .map_err(map_storage_error)?;
    }
    Ok((StatusCode::NO_CONTENT, no_store_headers(None)))
}
