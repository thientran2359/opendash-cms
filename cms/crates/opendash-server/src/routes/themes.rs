//! Admin-only theme install/list/CAS activation, with legacy /theme preserved.
use axum::{
    Json,
    body::Bytes,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin, require_admin_with_csrf},
    error::{ApiError, ApiResult},
    package::is_valid_zip_content_type,
    storage::{
        page::insert_theme_block,
        theme::{self, ActivationOutcome, ThemeItem, ThemeList},
    },
    theme::Theme,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivateInput {
    id: String,
}

fn headers(version: Option<i64>) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(v) = version {
        h.insert(
            header::ETAG,
            HeaderValue::from_str(&format!("\"v{v}\"")).expect("version is integer"),
        );
    }
    h
}

fn response(value: ThemeList) -> Response {
    (headers(Some(value.version)), Json(value)).into_response()
}

fn parse_version(headers: &HeaderMap) -> ApiResult<i64> {
    let val = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError::precondition_required("Cần If-Match để kích hoạt theme."))?
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    val.strip_prefix("\"v")
        .and_then(|s| s.strip_suffix('"'))
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|v| *v >= 1)
        .ok_or_else(|| ApiError::bad_request("If-Match phải có định dạng \"vN\"."))
}

pub async fn theme_list(
    State(db): State<SqlitePool>,
    request_headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&request_headers, &db).await?;
    Ok(response(theme::list(&db).await?))
}

pub async fn theme_install(
    State(db): State<SqlitePool>,
    request_headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<Response> {
    require_admin_with_csrf(&request_headers, &db).await?;
    let content_type = request_headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok());
    if !is_valid_zip_content_type(content_type, true) {
        return Err(ApiError::unsupported_media_type("Cần gói application/zip."));
    }
    let data = Theme::from_zip(&bytes).map_err(ApiError::bad_request)?;
    let item = ThemeItem {
        id: data.descriptor.id.clone(),
        compatible_block_id: data.compatible_block_id.clone(),
    };
    insert_theme_block(&db, &data).await?;
    Ok((StatusCode::CREATED, headers(None), Json(item)).into_response())
}

pub async fn theme_activate(
    State(db): State<SqlitePool>,
    request_headers: HeaderMap,
    input: Result<Json<ActivateInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&request_headers, &db).await?;
    let version = parse_version(&request_headers)?;
    let Json(input) =
        input.map_err(|_| ApiError::bad_request("Dữ liệu kích hoạt không hợp lệ."))?;
    match theme::activate(&db, &input.id, version).await? {
        ActivationOutcome::Activated(value) => Ok(response(value)),
        ActivationOutcome::NotFound => Err(ApiError::not_found("Theme chưa được cài đặt.")),
        ActivationOutcome::Stale => Err(ApiError::precondition_failed(
            "Theme đã thay đổi ở phiên khác (412). Tải lại trạng thái trước khi thử tiếp.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_canonical_etag() {
        let mut h = HeaderMap::new();
        assert_eq!(
            parse_version(&h).unwrap_err().0,
            StatusCode::PRECONDITION_REQUIRED
        );
        for s in ["v1", "W/\"v1\"", "\"v0\"", "\"v1\",\"v2\""] {
            h.insert(header::IF_MATCH, HeaderValue::from_str(s).unwrap());
            assert_eq!(parse_version(&h).unwrap_err().0, StatusCode::BAD_REQUEST);
        }
        h.insert(header::IF_MATCH, HeaderValue::from_static("\"v42\""));
        assert_eq!(parse_version(&h).unwrap(), 42);
    }
}
