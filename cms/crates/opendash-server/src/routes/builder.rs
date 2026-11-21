use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, RawQuery, State},
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
    page::{self as page_usecase, render_block, save_page},
    routes::public::rendered,
    storage::{builder as builder_storage, page::list_theme_blocks},
    theme::{Block, Fields},
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
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
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

fn require_builder_json(headers: &HeaderMap, bytes: &Bytes) -> ApiResult<()> {
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
    Ok(())
}

fn decode_document(headers: &HeaderMap, bytes: &Bytes) -> ApiResult<BuilderDocument> {
    require_builder_json(headers, bytes)?;
    BuilderDocument::from_json(bytes).map_err(|error| ApiError::bad_request(error.to_string()))
}

pub async fn homepage_get(State(db): State<SqlitePool>, headers: HeaderMap) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    if actor.user.role != crate::auth::Role::Admin {
        return Err(ApiError::forbidden("Chỉ Admin mới được chỉnh homepage."));
    }
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
    require_admin_with_csrf(&headers, &db).await?;
    let document = decode_document(&headers, &bytes)?;
    let current = builder_storage::read(&db).await?;
    current.validate(&document)?;
    builder_storage::check_preview_images(&db, &document).await?;
    let (bindings, lists) = builder_storage::resolve_preview(&db, &document).await?;
    let forms = crate::storage::builder::resolve_contact_forms(&db, &document).await?;
    let rendered = page_usecase::render_builder_with_lists_and_forms(
        &current, &document, true, &bindings, &lists, &forms,
    )?;
    Ok((builder_headers(None), Json(rendered)).into_response())
}

pub async fn homepage_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
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

fn require_page_builder_type(type_key: &str) -> ApiResult<()> {
    if type_key != "page" {
        return Err(ApiError::forbidden(
            "Builder is available only for Page entries.",
        ));
    }
    Ok(())
}

fn entry_builder_response(result: builder_storage::EntryBuilderOpen) -> Response {
    (builder_headers(Some(result.version)), Json(result)).into_response()
}

fn entry_builder_mutation_response(
    result: Result<builder_storage::EntryBuilderOpen, builder_storage::EntryBuilderMutationError>,
) -> ApiResult<Response> {
    match result {
        Ok(open) => Ok(entry_builder_response(open)),
        Err(builder_storage::EntryBuilderMutationError::Api(error)) => Err(error),
        Err(builder_storage::EntryBuilderMutationError::Veto(veto)) => Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            builder_headers(None),
            Json(veto),
        )
            .into_response()),
    }
}

pub async fn entry_builder_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    require_page_builder_type(&type_key)?;
    let result = builder_storage::read_entry_builder(&db, &actor, &type_key, id).await?;
    Ok(entry_builder_response(result))
}

pub async fn entry_builder_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    bytes: Bytes,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    require_page_builder_type(&type_key)?;
    let expected_version = parse_builder_if_match(&headers)?;
    let document = decode_document(&headers, &bytes)?;
    entry_builder_mutation_response(
        builder_storage::save_entry_builder(
            &db,
            &actor,
            &hooks,
            &type_key,
            id,
            expected_version,
            &document,
        )
        .await,
    )
}

pub async fn entry_builder_preview(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    RawQuery(raw): RawQuery,
    bytes: Bytes,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    require_page_builder_type(&type_key)?;
    if raw.as_deref().is_some_and(|query| !query.is_empty()) {
        return Err(ApiError::bad_request(
            "Page Builder preview target is fixed by the path.",
        ));
    }
    let document = decode_document(&headers, &bytes)?;
    let rendered =
        builder_storage::preview_entry_builder(&db, &actor, &type_key, id, &document).await?;
    let mut response_headers = builder_headers(None);
    // Reuse the site's strict layout CSP, including the contact form flag.
    let layout_response = super::public::rendered_layout_with_form_csp(
        rendered.html.clone(),
        rendered.has_active_contact_form,
    );
    for name in [header::CONTENT_SECURITY_POLICY, header::REFERRER_POLICY] {
        if let Some(value) = layout_response.headers().get(&name) {
            response_headers.insert(name, value.clone());
        }
    }
    Ok((response_headers, Json(rendered)).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryBuilderReset {
    confirm: bool,
}

pub async fn entry_builder_reset(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    bytes: Bytes,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    require_page_builder_type(&type_key)?;
    let expected_version = parse_builder_if_match(&headers)?;
    require_builder_json(&headers, &bytes)?;
    let input: EntryBuilderReset =
        serde_json::from_slice(&bytes).map_err(|error| ApiError::bad_request(error.to_string()))?;
    entry_builder_mutation_response(
        builder_storage::reset_entry_builder(
            &db,
            &actor,
            &hooks,
            &type_key,
            id,
            expected_version,
            input.confirm,
        )
        .await,
    )
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
