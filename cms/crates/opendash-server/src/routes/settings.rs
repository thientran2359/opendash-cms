//! Admin-only SITE-01 API, with ETag/CAS and no-store responses.
use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin, require_admin_with_csrf},
    error::{ApiError, ApiResult},
    storage::settings::{self, GeneralSettings, GeneralSettingsInput, SaveOutcome},
};

fn no_store_headers(version: Option<i64>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(version) = version {
        let etag = HeaderValue::from_str(&format!("\"v{version}\""))
            .expect("integer ETag header is always valid");
        headers.insert(header::ETAG, etag);
    }
    headers
}

fn settings_response(settings: GeneralSettings) -> Response {
    (no_store_headers(Some(settings.version)), Json(settings)).into_response()
}

fn parse_if_match(headers: &HeaderMap) -> ApiResult<i64> {
    let value = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError::precondition_required("Cần If-Match để lưu cài đặt."))?
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    value
        .strip_prefix("\"v")
        .and_then(|value| value.strip_suffix('"'))
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|version| *version >= 1)
        .ok_or_else(|| ApiError::bad_request("If-Match phải có định dạng \"vN\"."))
}

fn map_storage_error(error: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref db_error) = error {
        if matches!(db_error.code().as_deref(), Some("5" | "6" | "517")) {
            return ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "SQLite đang bận, vui lòng thử lại.".into(),
            );
        }
    }
    error.into()
}

pub async fn general_get(State(db): State<SqlitePool>, headers: HeaderMap) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let current = settings::get_general(&db).await?;
    Ok(settings_response(current))
}

pub async fn timezones_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    Ok((
        no_store_headers(None),
        Json(json!({"version": settings::TZDB_VERSION, "items": settings::timezone_catalog()})),
    )
        .into_response())
}

pub async fn general_put(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    input: Result<Json<GeneralSettingsInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let version = parse_if_match(&headers)?;
    let Json(input) = input.map_err(|_| ApiError::bad_request("Dữ liệu Settings không hợp lệ."))?;
    let input = input.validate().map_err(ApiError::bad_request)?;
    match settings::save_general(&db, input, version)
        .await
        .map_err(map_storage_error)?
    {
        SaveOutcome::Saved(saved) => Ok(settings_response(saved)),
        SaveOutcome::Stale => Err(ApiError::precondition_failed(
            "Dữ liệu đã thay đổi ở một tab khác. Form chưa lưu vẫn được giữ lại.",
        )),
        SaveOutcome::InvalidTimezone => Err(ApiError::bad_request(
            "Múi giờ không có trong danh mục IANA tzdb 2026f.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn etag_rejects_missing_weak_and_malformed_versions() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            parse_if_match(&headers).unwrap_err().0,
            StatusCode::PRECONDITION_REQUIRED
        );
        for value in ["W/\"v1\"", "v1", "\"v0\"", "\"v-1\"", "\"v1\",\"v2\""] {
            headers.insert(header::IF_MATCH, HeaderValue::from_str(value).unwrap());
            assert_eq!(
                parse_if_match(&headers).unwrap_err().0,
                StatusCode::BAD_REQUEST
            );
        }
        headers.insert(header::IF_MATCH, HeaderValue::from_static("\"v42\""));
        assert_eq!(parse_if_match(&headers).unwrap(), 42);
    }
}
