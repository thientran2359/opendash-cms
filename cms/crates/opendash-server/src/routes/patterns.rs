//! Pattern CPT metadata CRUD and authenticated, read-only Theme Pattern Library.
//! Builder AST is served by the shared /api/admin/entries/pattern/{id}/builder.

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin_with_csrf, require_session},
    error::{ApiError, ApiResult},
    routes::themes::ThemeStoreRoot,
    storage::patterns::{self, DeleteOutcome, PatternInput},
};

fn version_from_if_match(headers: &HeaderMap) -> ApiResult<i64> {
    let text = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError::precondition_required("Pattern cần If-Match để sửa/xóa."))?
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    let digits = text
        .strip_prefix("\"v")
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| ApiError::bad_request("If-Match phải có dạng \"vN\"."))?;
    if digits.is_empty()
        || (digits.len() > 1 && digits.starts_with('0'))
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(ApiError::bad_request("If-Match không hợp lệ."));
    }
    digits
        .parse::<i64>()
        .ok()
        .filter(|version| *version > 0)
        .ok_or_else(|| ApiError::bad_request("If-Match không hợp lệ."))
}

fn private_headers(version: Option<i64>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if let Some(version) = version {
        headers.insert(
            header::ETAG,
            HeaderValue::from_str(&format!("\"v{version}\"")).expect("numeric ETag"),
        );
    }
    headers
}

pub async fn pattern_list(State(db): State<SqlitePool>, headers: HeaderMap) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let items = patterns::list(&db).await?;
    Ok((
        private_headers(None),
        Json(serde_json::json!({ "items": items })),
    )
        .into_response())
}

pub async fn pattern_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let record = patterns::get(&db, id).await?;
    Ok((private_headers(Some(record.version)), Json(record)).into_response())
}

pub async fn pattern_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Json(input): Json<PatternInput>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let record = patterns::create(&db, &input).await?;
    Ok((
        StatusCode::CREATED,
        private_headers(Some(record.version)),
        Json(record),
    )
        .into_response())
}

pub async fn pattern_update(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(input): Json<PatternInput>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let record = patterns::update(&db, id, version_from_if_match(&headers)?, &input).await?;
    Ok((private_headers(Some(record.version)), Json(record)).into_response())
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternDeleteQuery {
    #[serde(default)]
    pub force: bool,
}

pub async fn pattern_delete(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Query(query): Query<PatternDeleteQuery>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let outcome = patterns::delete(&db, id, version_from_if_match(&headers)?, query.force).await?;
    match outcome {
        DeleteOutcome::Deleted => {
            Ok((StatusCode::NO_CONTENT, private_headers(None)).into_response())
        }
        DeleteOutcome::Referenced {
            referencing_count,
            referencing_entries,
        } => Ok((
            StatusCode::CONFLICT,
            private_headers(None),
            Json(serde_json::json!({
                "error": "Pattern đang được tham chiếu. Xác nhận force=true để xóa.",
                "referencing_count": referencing_count,
                "referencing_entries": referencing_entries,
            })),
        )
            .into_response()),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemePatternQuery {
    pub category: Option<String>,
    pub q: Option<String>,
}

pub async fn pattern_library_list(
    State(db): State<SqlitePool>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    headers: HeaderMap,
    Query(query): Query<ThemePatternQuery>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let catalog = patterns::list_theme_patterns(
        &db,
        &themes_root.0,
        query.category.as_deref(),
        query.q.as_deref(),
    )
    .await?;
    Ok((private_headers(None), Json(catalog)).into_response())
}

pub async fn pattern_library_theme_get(
    State(db): State<SqlitePool>,
    Extension(themes_root): Extension<ThemeStoreRoot>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let record = patterns::get_theme_pattern(&db, &themes_root.0, &key).await?;
    Ok((private_headers(None), Json(record)).into_response())
}
