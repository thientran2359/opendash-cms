//! Authenticated content entry CRUD for CPTs (M3.3 backend).

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use opendash_core::{
    content::{ContentEntryInput, valid_content_type_key},
};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;

use crate::{
    auth::{require_session, require_session_with_csrf},
    content_render,
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    storage::{
        content::load_content_type_definition, entries as entry_storage, media as media_storage,
        page::active_content_templates,
    },
};

pub const MAX_CONTENT_ENTRY_JSON_BYTES: usize = 512 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationInput {
    pub status: String,
}

fn is_builtin(key: &str) -> bool {
    matches!(key, "post" | "page")
}

#[derive(Deserialize, Default)]
pub struct EntryPagination {
    limit: Option<i64>,
    offset: Option<i64>,
    category_id: Option<i64>,
    tag_id: Option<i64>,
    include_descendants: Option<bool>,
}

fn valid_type_key(key: &str) -> ApiResult<()> {
    if valid_content_type_key(key) {
        Ok(())
    } else {
        Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."))
    }
}

async fn content_schema(
    db: &SqlitePool,
    type_key: &str,
) -> ApiResult<opendash_core::content::ContentTypeDefinition> {
    valid_type_key(type_key)?;
    load_content_type_definition(db, type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))
}

fn expected_version(headers: &HeaderMap) -> ApiResult<i64> {
    let raw = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError::precondition_required("Cần If-Match với phiên bản hiện tại."))?
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    let digits = raw
        .strip_prefix("\"v")
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| ApiError::bad_request("If-Match phải có dạng \"v1\"."))?;
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::bad_request("If-Match không hợp lệ."));
    }
    let parsed = digits
        .parse::<i64>()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    if parsed <= 0 {
        return Err(ApiError::bad_request("If-Match không hợp lệ."));
    }
    Ok(parsed)
}

fn etag(version: i64) -> String {
    format!("\"v{version}\"")
}

pub async fn entry_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Query(pagination): Query<EntryPagination>,
) -> ApiResult<Json<Vec<entry_storage::ContentEntryRecord>>> {
    require_session(&headers, &db).await?;
    content_schema(&db, &type_key).await?;
    let limit = pagination.limit.unwrap_or(50);
    let offset = pagination.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) || !(0..=1_000_000).contains(&offset) {
        return Err(ApiError::bad_request("Phân trang không hợp lệ."));
    }
    if pagination
        .category_id
        .into_iter()
        .chain(pagination.tag_id)
        .any(|id| id <= 0)
    {
        return Err(ApiError::bad_request("ID bộ lọc taxonomy phải dương."));
    }
    if pagination.include_descendants.unwrap_or(false) && pagination.category_id.is_none() {
        return Err(ApiError::bad_request(
            "include_descendants chỉ được dùng cùng category_id.",
        ));
    }
    let filters = entry_storage::EntryFilters {
        category_id: pagination.category_id,
        tag_id: pagination.tag_id,
        include_descendants: pagination.include_descendants.unwrap_or(false),
    };
    Ok(Json(
        entry_storage::list_entries(&db, &type_key, limit, offset, filters).await?,
    ))
}

pub async fn entry_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let entry = entry_storage::get_entry(&db, &type_key, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    Ok(([(header::ETAG, etag(entry.version))], Json(entry)).into_response())
}

pub async fn entry_create(
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Json(input): Json<ContentEntryInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let schema = content_schema(&db, &type_key).await?;
    input
        .validate_against(&schema)
        .map_err(ApiError::bad_request)?;
    media_storage::require_existing_references(&db, &schema, &input).await?;
    let candidate = json!({
        "operation": "create",
        "type_key": &type_key,
        "entry_id": serde_json::Value::Null,
        "expected_version": serde_json::Value::Null,
        "title": &input.title,
        "slug": &input.slug,
        "candidate_values": &input.values,
        "term_ids": &input.term_ids,
    });
    if let Err(veto) = hooks.dispatch_content_before_save(&type_key, candidate).await {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let entry = entry_storage::create_entry(&db, &actor, &type_key, &schema, &input).await?;
    let _ = hooks
        .dispatch_content_after_save(
            &type_key,
            json!({ "type_key": &type_key, "id": entry.id, "operation": "create", "version": entry.version }),
        )
        .await;
    Ok((
        StatusCode::CREATED,
        [
            (header::ETAG, etag(entry.version)),
            (
                header::LOCATION,
                format!("/api/admin/content-types/{type_key}/entries/{}", entry.id),
            ),
        ],
        Json(entry),
    )
        .into_response())
}

pub async fn entry_update(
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(input): Json<ContentEntryInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let version = expected_version(&headers)?;
    let schema = content_schema(&db, &type_key).await?;
    input
        .validate_against(&schema)
        .map_err(ApiError::bad_request)?;
    media_storage::require_existing_references(&db, &schema, &input).await?;
    let candidate = json!({
        "operation": "update",
        "type_key": &type_key,
        "entry_id": id,
        "expected_version": version,
        "title": &input.title,
        "slug": &input.slug,
        "candidate_values": &input.values,
        "term_ids": &input.term_ids,
    });
    if let Err(veto) = hooks.dispatch_content_before_save(&type_key, candidate).await {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let entry =
        entry_storage::update_entry(&db, &actor, &type_key, &schema, id, version, &input).await?;
    let _ = hooks
        .dispatch_content_after_save(
            &type_key,
            json!({ "type_key": &type_key, "id": entry.id, "operation": "update", "version": entry.version }),
        )
        .await;
    Ok(([(header::ETAG, etag(entry.version))], Json(entry)).into_response())
}

pub async fn entry_delete(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<StatusCode> {
    require_session_with_csrf(&headers, &db).await?;
    let version = expected_version(&headers)?;
    valid_type_key(&type_key)?;
    entry_storage::delete_entry(&db, &type_key, id, version).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Publish/Unpublish is a separate, authenticated optimistic-CAS operation.
/// Ordinary PUT/Save deliberately never alters the current status.
pub async fn entry_publication(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(input): Json<PublicationInput>,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    valid_type_key(&type_key)?;
    if !is_builtin(&type_key) || !matches!(input.status.as_str(), "draft" | "published") {
        return Err(ApiError::bad_request(
            "Chỉ Post/Page hỗ trợ draft/published.",
        ));
    }
    let version = expected_version(&headers)?;
    let record =
        entry_storage::set_publication_status(&db, &type_key, id, version, &input.status).await?;
    // No success hooks are emitted until state is durable; on CAS failures
    // storage returns 404/412 without any external side effects.
    Ok(([(header::ETAG, etag(record.version))], Json(record)).into_response())
}

/// Persisted Post/Page preview is private and never shares the public reader.
pub async fn entry_preview(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    valid_type_key(&type_key)?;
    if !is_builtin(&type_key) {
        return Err(ApiError::bad_request("Preview chỉ hỗ trợ Post/Page."));
    }
    let schema = content_schema(&db, &type_key).await?;
    let entry = entry_storage::get_entry(&db, &type_key, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    let view = content_render::authenticated_preview_view(entry, &schema);
    let (_, selected_template) = active_content_templates(&db).await?;
    let raw_html = content_render::render_single(&schema, &view, selected_template.as_deref())?;
    let pub_settings = crate::storage::settings::get_public(&db).await?;
    let seo_view = crate::seo::SeoView::for_noindex_page(&pub_settings.content_language, &view.title);
    let html = content_render::compose_document_with_seo(None, &raw_html, None, &seo_view)?;
    Ok((
        [
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; script-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        Html(html),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_match_requires_canonical_positive_version() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            expected_version(&headers).unwrap_err().0,
            StatusCode::PRECONDITION_REQUIRED
        );
        for invalid in ["*", "\"v0\"", "\"v01\"", "v1", "\"v-1\"", "W/\"v1\""] {
            headers.insert(header::IF_MATCH, invalid.parse().unwrap());
            assert_eq!(
                expected_version(&headers).unwrap_err().0,
                StatusCode::BAD_REQUEST
            );
        }
        headers.insert(header::IF_MATCH, "\"v42\"".parse().unwrap());
        assert_eq!(expected_version(&headers).unwrap(), 42);
    }
}
