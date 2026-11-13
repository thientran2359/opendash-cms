use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::builder::{BuilderDocument, MAX_BUILDER_JSON_BYTES};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    app::AdminAssets,
    auth::{require_admin_with_csrf, require_session, require_session_with_csrf},
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    package::is_valid_zip_content_type,
    page::{self as page_usecase, render_block, save_page},
    routes::public::rendered,
    storage::{
        builder as builder_storage, overrides as overrides_storage,
        page::{insert_theme_block, list_theme_blocks},
    },
    theme::{Block, Fields, Theme},
};

pub async fn builder() -> Response {
    match AdminAssets::get("index.html") {
        Some(file) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            file.data,
        )
            .into_response(),
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Giao diện quản trị chưa được đóng gói.",
        )
            .into_response(),
    }
}

pub async fn serve_asset(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    let clean_path = path.trim_start_matches('/');
    let asset_path = format!("assets/{clean_path}");
    match AdminAssets::get(&asset_path) {
        Some(file) => {
            let mime = mime_guess::from_path(&asset_path).first_or_octet_stream();
            (
                [
                    (header::CONTENT_TYPE, mime.as_ref()),
                    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                ],
                file.data,
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Asset không tồn tại.",
        )
            .into_response(),
    }
}

pub async fn registry(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Json<BTreeMap<String, Fields>>> {
    require_session(&headers, &db).await?;
    Ok(Json(list_theme_blocks(&db).await?))
}

pub async fn install(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<StatusCode> {
    require_admin_with_csrf(&headers, &db).await?;

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    if !is_valid_zip_content_type(content_type, true) {
        return Err(ApiError::unsupported_media_type("Cần gói application/zip."));
    }
    let theme = Theme::from_zip(&bytes).map_err(ApiError::bad_request)?;
    insert_theme_block(&db, &theme).await?;
    Ok(StatusCode::CREATED)
}

pub async fn page(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Json<Option<Block>>> {
    require_session(&headers, &db).await?;
    Ok(Json(builder_storage::legacy_page(&db).await?))
}

pub async fn preview(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Json(block): Json<Block>,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    Ok(builder_rendered(render_block(&db, &block).await?))
}

pub async fn save(
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Json(block): Json<Block>,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    Ok(builder_rendered(save_page(&db, &hooks, &block).await?))
}

fn builder_rendered(result: crate::page::RenderedBlock) -> Response {
    let mut response = rendered(result.html);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        "X-OpenDash-Block-Visibility",
        axum::http::HeaderValue::from_static(result.visibility),
    );
    response
}

#[derive(Serialize)]
pub struct HomepageOpen {
    pub document: BuilderDocument,
    pub source: &'static str,
    pub version: i64,
}

#[derive(Serialize)]
pub struct HomepageSaved {
    pub document: BuilderDocument,
    pub source: &'static str,
    pub version: i64,
    pub html: String,
    pub diagnostics: Vec<page_usecase::BuilderDiagnostic>,
}

fn builder_headers(version: Option<i64>) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if let Some(version) = version {
        h.insert(
            header::ETAG,
            HeaderValue::from_str(&format!("\"v{version}\"")).expect("numeric ETag"),
        );
    }
    h
}

fn parse_builder_if_match(headers: &HeaderMap) -> ApiResult<i64> {
    let value = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError::precondition_required("Builder Save cần If-Match."))?
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    let Some(digits) = value
        .strip_prefix("\"v")
        .and_then(|tail| tail.strip_suffix('"'))
    else {
        return Err(ApiError::bad_request("If-Match phải có dạng \"vN\"."));
    };
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::bad_request("If-Match không hợp lệ."));
    }
    digits
        .parse::<i64>()
        .ok()
        .filter(|v| *v >= 1)
        .ok_or_else(|| ApiError::bad_request("If-Match không hợp lệ."))
}

fn decode_document(headers: &HeaderMap, bytes: &Bytes) -> ApiResult<BuilderDocument> {
    let is_json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"));
    if !is_json {
        return Err(ApiError::unsupported_media_type(
            "Builder cần application/json.",
        ));
    }
    if bytes.len() > MAX_BUILDER_JSON_BYTES {
        return Err(ApiError::bad_request("Builder JSON vượt giới hạn 256 KiB."));
    }
    BuilderDocument::from_json(bytes).map_err(|error| ApiError::bad_request(error.to_string()))
}

pub async fn homepage_get(State(db): State<SqlitePool>, headers: HeaderMap) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let current = builder_storage::read(&db).await?;
    Ok((
        builder_headers(Some(current.version)),
        Json(HomepageOpen {
            document: current.document(),
            source: current.source(),
            version: current.version,
        }),
    )
        .into_response())
}

pub async fn homepage_preview(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    let document = decode_document(&headers, &bytes)?;
    let current = builder_storage::read(&db).await?;
    current.validate(&document)?;
    builder_storage::check_preview_images(&db, &document).await?;
    let (bindings, lists) = builder_storage::resolve_preview(&db, &document).await?;
    let rendered =
        page_usecase::render_builder_with_lists(&current, &document, true, &bindings, &lists)?;
    Ok((builder_headers(None), Json(rendered)).into_response())
}

pub async fn homepage_put(
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    let version = parse_builder_if_match(&headers)?;
    let document = decode_document(&headers, &bytes)?;
    let (version, rendered) = builder_storage::save_and_render(
        &db,
        &document,
        version,
        |current, draft, bindings, lists| {
            page_usecase::render_builder_with_lists(current, draft, false, bindings, lists)
        },
    )
    .await?;
    page_usecase::dispatch_saved_hook(
        &hooks,
        page_usecase::layout_hook_payload(&document, version),
    )
    .await;
    Ok((
        builder_headers(Some(version)),
        Json(HomepageSaved {
            document,
            source: "layout",
            version,
            html: rendered.html,
            diagnostics: rendered.diagnostics,
        }),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct OverrideScopeQuery {
    pub type_key: Option<String>,
    pub entry_id: Option<i64>,
    pub preview_type_key: Option<String>,
    pub preview_entry_slug: Option<String>,
}

#[derive(Serialize)]
pub struct OverrideOpen {
    pub exists: bool,
    pub role: String,
    pub scope_type_key: Option<String>,
    pub scope_entry_id: Option<i64>,
    pub document: BuilderDocument,
    pub version: i64,
    pub base_theme_id: Option<String>,
    pub base_revision_id: Option<String>,
}

#[derive(Serialize)]
pub struct OverrideSaved {
    pub role: String,
    pub scope_type_key: Option<String>,
    pub scope_entry_id: Option<i64>,
    pub document: BuilderDocument,
    pub version: i64,
    pub base_theme_id: Option<String>,
    pub base_revision_id: Option<String>,
    pub html: String,
    pub diagnostics: Vec<page_usecase::BuilderDiagnostic>,
}

#[derive(Deserialize)]
pub struct OverrideResetRequest {
    pub confirm: bool,
}

fn parse_optional_builder_if_match(headers: &HeaderMap) -> ApiResult<Option<i64>> {
    let Some(raw) = headers.get(header::IF_MATCH) else {
        return Ok(None);
    };
    let value = raw
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    let Some(digits) = value
        .strip_prefix("\"v")
        .and_then(|tail| tail.strip_suffix('"'))
    else {
        return Err(ApiError::bad_request("If-Match phải có dạng \"vN\"."));
    };
    if digits == "0" {
        return Ok(Some(0));
    }
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::bad_request("If-Match không hợp lệ."));
    }
    digits
        .parse::<i64>()
        .ok()
        .filter(|v| *v >= 1)
        .map(Some)
        .ok_or_else(|| ApiError::bad_request("If-Match không hợp lệ."))
}

pub async fn overrides_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let items = overrides_storage::list_overrides(&db).await?;
    Ok((builder_headers(None), Json(items)).into_response())
}

pub async fn override_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(role): Path<String>,
    Query(query): Query<OverrideScopeQuery>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let role = overrides_storage::OverrideRole::parse(&role)?;
    let scope = overrides_storage::OverrideScope::new(role, query.type_key, query.entry_id)?;
    let existing = overrides_storage::get_override(&db, &scope).await?;
    let (exists, document, version, base_theme_id, base_revision_id) = match existing {
        Some(record) => (
            true,
            record.document,
            record.version,
            record.base_theme_id,
            record.base_revision_id,
        ),
        None => (false, BuilderDocument::empty(), 0, None, None),
    };
    Ok((
        builder_headers((version > 0).then_some(version)),
        Json(OverrideOpen {
            exists,
            role: scope.role.as_str().to_string(),
            scope_type_key: scope.scope_type_key,
            scope_entry_id: scope.scope_entry_id,
            document,
            version,
            base_theme_id,
            base_revision_id,
        }),
    )
        .into_response())
}

pub async fn override_preview(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(role): Path<String>,
    Query(query): Query<OverrideScopeQuery>,
    bytes: Bytes,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    let role = overrides_storage::OverrideRole::parse(&role)?;
    let scope = overrides_storage::OverrideScope::new(role, query.type_key, query.entry_id)?;
    let document = decode_document(&headers, &bytes)?;
    let preview_entry = match (
        query.preview_type_key.as_deref(),
        query.preview_entry_slug.as_deref(),
    ) {
        (Some(tk), Some(slug)) => Some((tk, slug)),
        _ => None,
    };
    let rendered =
        overrides_storage::preview_override(&db, &scope, &document, preview_entry).await?;
    Ok((builder_headers(None), Json(rendered)).into_response())
}

pub async fn override_put(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(role): Path<String>,
    Query(query): Query<OverrideScopeQuery>,
    bytes: Bytes,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    let role = overrides_storage::OverrideRole::parse(&role)?;
    let scope = overrides_storage::OverrideScope::new(role, query.type_key, query.entry_id)?;
    let expected_version = parse_optional_builder_if_match(&headers)?;
    let document = decode_document(&headers, &bytes)?;
    let (saved, rendered) =
        overrides_storage::save_override_and_render(&db, &scope, &document, expected_version)
            .await?;
    Ok((
        builder_headers(Some(saved.version)),
        Json(OverrideSaved {
            role: saved.role,
            scope_type_key: saved.scope_type_key,
            scope_entry_id: saved.scope_entry_id,
            document: saved.document,
            version: saved.version,
            base_theme_id: saved.base_theme_id,
            base_revision_id: saved.base_revision_id,
            html: rendered.html,
            diagnostics: rendered.diagnostics,
        }),
    )
        .into_response())
}

pub async fn override_reset(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(role): Path<String>,
    Query(query): Query<OverrideScopeQuery>,
    Json(body): Json<OverrideResetRequest>,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    let role = overrides_storage::OverrideRole::parse(&role)?;
    let scope = overrides_storage::OverrideScope::new(role, query.type_key, query.entry_id)?;
    let expected_version = parse_builder_if_match(&headers)?;
    overrides_storage::reset_override(&db, &scope, expected_version, body.confirm).await?;
    Ok((
        builder_headers(None),
        Json(serde_json::json!({ "reset": true, "role": scope.role.as_str() })),
    )
        .into_response())
}

#[cfg(test)]
mod homepage_tests {
    use super::*;

    #[test]
    fn builder_version_requires_canonical_etag() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            parse_builder_if_match(&headers).unwrap_err().0,
            StatusCode::PRECONDITION_REQUIRED
        );
        for value in [
            "v1",
            "W/\"v1\"",
            "\"v0\"",
            "\"v01\"",
            "\"",
            "\"v1\",\"v2\"",
            "*",
        ] {
            headers.insert(header::IF_MATCH, value.parse().unwrap());
            assert_eq!(
                parse_builder_if_match(&headers).unwrap_err().0,
                StatusCode::BAD_REQUEST
            );
        }
        headers.insert(header::IF_MATCH, HeaderValue::from_static("\"v8\""));
        assert_eq!(parse_builder_if_match(&headers).unwrap(), 8);
    }
}
