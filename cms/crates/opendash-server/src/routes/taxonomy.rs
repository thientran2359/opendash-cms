//! M4 P09: Admin category/tag management. SQL remains in storage/taxonomy.

use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use std::collections::BTreeMap;
use serde_json::Value;
use sqlx::SqlitePool;
use opendash_core::content::{
    TaxonomyDefinition, TaxonomyDefinitionInput, TaxonomyDefinitionPatch,
    valid_content_type_key,
};

use crate::{
    auth::{require_admin_with_csrf, require_session, require_session_with_csrf},
    error::{ApiError, ApiResult},
    storage::taxonomy,
};

pub const MAX_TAXONOMY_JSON_BYTES: usize = 16 * 1024;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyPagination {
    kind: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewTaxonomyTerm {
    kind: String,
    name: String,
    slug: String,
    parent_id: Option<i64>,
}

// Missing parent_id means "keep current parent"; explicit null means root.
fn present_optional_parent<'de, D>(deserializer: D) -> Result<Option<Option<i64>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<i64>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchTaxonomyTerm {
    name: Option<String>,
    slug: Option<String>,
    #[serde(default, deserialize_with = "present_optional_parent")]
    parent_id: Option<Option<i64>>,
    #[serde(default)]
    extensions: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    schema_token: Option<String>,
}

fn parse_body<T: DeserializeOwned>(payload: Result<Json<T>, JsonRejection>) -> ApiResult<T> {
    payload
        .map(|Json(body)| body)
        .map_err(|_| ApiError::bad_request("Dữ liệu JSON taxonomy không hợp lệ."))
}

fn valid_id(id: i64) -> ApiResult<()> {
    if id > 0 {
        Ok(())
    } else {
        Err(ApiError::bad_request("ID term phải là số nguyên dương."))
    }
}

fn expected_version(headers: &HeaderMap) -> ApiResult<i64> {
    let raw = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError::precondition_required("Cần If-Match phiên bản term."))?
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    let digits = raw
        .strip_prefix("\"v")
        .and_then(|v| v.strip_suffix('"'))
        .ok_or_else(|| ApiError::bad_request("If-Match phải có dạng \"v1\"."))?;
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::bad_request("If-Match không hợp lệ."));
    }
    let version = digits
        .parse::<i64>()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    if version < 1 {
        return Err(ApiError::bad_request("If-Match không hợp lệ."));
    }
    Ok(version)
}

fn term_response(status: StatusCode, term: taxonomy::TaxonomyTerm) -> Response {
    (
        status,
        [
            (header::ETAG, format!("\"v{}\"", term.version)),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Json(term),
    )
        .into_response()
}

fn definition_response(status: StatusCode, definition: TaxonomyDefinition) -> Response {
    (
        status,
        [
            (header::ETAG, format!("\"v{}\"", definition.version)),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Json(definition),
    ).into_response()
}

pub async fn taxonomy_definitions_list(
    State(db): State<SqlitePool>, headers: HeaderMap,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")],
        Json(taxonomy::list_definitions(&db).await?)).into_response())
}

pub async fn taxonomy_definitions_get(
    State(db): State<SqlitePool>, headers: HeaderMap, Path(key): Path<String>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    if !valid_content_type_key(&key) {
        return Err(ApiError::bad_request("Khóa taxonomy không hợp lệ."));
    }
    let definition = taxonomy::get_definition(&db, &key).await?
        .ok_or_else(|| ApiError::not_found("Taxonomy không tồn tại."))?;
    Ok(definition_response(StatusCode::OK, definition))
}

pub async fn taxonomy_definitions_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>, headers: HeaderMap,
    payload: Result<Json<TaxonomyDefinitionInput>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    let input = parse_body(payload)?;
    let created = taxonomy::create_definition(&db, &actor, &input).await?;
    Ok(definition_response(StatusCode::CREATED, created))
}

pub async fn taxonomy_definitions_patch(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>, headers: HeaderMap, Path(key): Path<String>,
    payload: Result<Json<TaxonomyDefinitionPatch>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    if !valid_content_type_key(&key) {
        return Err(ApiError::bad_request("Khóa taxonomy không hợp lệ."));
    }
    let expected = expected_version(&headers)?;
    let input = parse_body(payload)?;
    let updated = taxonomy::patch_definition(&db, &actor, &key, expected, &input).await?;
    Ok(definition_response(StatusCode::OK, updated))
}

pub async fn taxonomy_definitions_delete(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>, headers: HeaderMap, Path(key): Path<String>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    if !valid_content_type_key(&key) {
        return Err(ApiError::bad_request("Khóa taxonomy không hợp lệ."));
    }
    let expected = expected_version(&headers)?;
    taxonomy::delete_definition(&db, &actor, &key, expected).await?;
    Ok((StatusCode::NO_CONTENT, [(header::CACHE_CONTROL, "no-store")]).into_response())
}

pub async fn taxonomy_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(query): Query<TaxonomyPagination>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    if let Some(key) = query.kind.as_deref() {
        if !valid_content_type_key(key)
            || taxonomy::get_definition(&db, key).await?.is_none()
        {
            return Err(ApiError::bad_request("Taxonomy không tồn tại."));
        }
    }
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) || offset < 0 {
        return Err(ApiError::bad_request("Phân trang taxonomy không hợp lệ."));
    }
    let page = taxonomy::list_terms(&db, query.kind.as_deref(), limit, offset).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(page)).into_response())
}

pub async fn taxonomy_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    valid_id(id)?;
    let result = taxonomy::get_term(&db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy term."))?;
    let result = taxonomy::term_with_effective_extensions(
        &db, result, if actor.user.role == crate::auth::Role::Admin { "admin" } else { "editor" },
    ).await?;
    Ok(term_response(StatusCode::OK, result))
}

pub async fn taxonomy_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    payload: Result<Json<NewTaxonomyTerm>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    let input = parse_body(payload)?;
    let created = taxonomy::create_term(
        &db,
        &actor,
        &input.kind,
        &input.name,
        &input.slug,
        input.parent_id,
    )
    .await?;
    let created = taxonomy::term_with_effective_extensions(&db, created, "admin").await?;
    Ok(term_response(StatusCode::CREATED, created))
}

pub async fn taxonomy_patch(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    payload: Result<Json<PatchTaxonomyTerm>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    valid_id(id)?;
    let version = expected_version(&headers)?;
    let input = parse_body(payload)?;
    if input.name.is_none() && input.slug.is_none() && input.parent_id.is_none()
        && input.extensions.is_none() {
        return Err(ApiError::bad_request(
            "Cần name, slug, parent_id hoặc extensions để cập nhật.",
        ));
    }
    let updated = taxonomy::patch_term(
        &db,
        &actor,
        id,
        version,
        input.name.as_deref(),
        input.slug.as_deref(),
        input.parent_id,
        input.extensions.as_ref(),
        input.schema_token.as_deref(),
    )
    .await?;
    let role = if actor.user.role == crate::auth::Role::Admin { "admin" } else { "editor" };
    let updated = taxonomy::term_with_effective_extensions(&db, updated, role).await?;
    Ok(term_response(StatusCode::OK, updated))
}

/// Separate authenticated projection of readable panel definitions. This is
/// never a write grant: the PATCH transaction recomputes role+schema token.
pub async fn taxonomy_effective_editor_config(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    valid_id(id)?;
    let term = taxonomy::get_term(&db, id).await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy term."))?;
    let role = if actor.user.role == crate::auth::Role::Admin { "admin" } else { "editor" };
    let config = crate::storage::admin_extensions::effective_term(&db, id, &term.kind, role).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(config)).into_response())
}

pub async fn taxonomy_delete(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    valid_id(id)?;
    let version = expected_version(&headers)?;
    taxonomy::delete_term(&db, &actor, id, version).await?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response())
}

pub async fn taxonomy_eligibility_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(content_type_key): Path<String>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let rows =
        crate::storage::content::list_content_type_taxonomy_eligibility(&db, &content_type_key)
            .await?;
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(rows),
    )
        .into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyEligibilityPut {
    taxonomies: Vec<String>,
    expected_version: i64,
}

pub async fn taxonomy_eligibility_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(content_type_key): Path<String>,
    payload: Result<Json<TaxonomyEligibilityPut>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    let input = parse_body(payload)?;
    let etag_version = crate::routes::settings::parse_if_match(&headers)?;
    if input.expected_version != etag_version {
        return Err(ApiError::bad_request("expected_version không khớp If-Match."));
    }
    let updated = crate::storage::content::put_site_taxonomy_eligibility(
        &db, &actor, &content_type_key, etag_version, &input.taxonomies,
    ).await?;
    Ok((
        StatusCode::OK,
        [
            (header::CACHE_CONTROL, "no-store".to_string()),
            (header::ETAG, format!("\"v{}\"", updated.version)),
        ],
        Json(updated),
    ).into_response())
}

pub async fn taxonomy_eligibility_effective(
    State(db): State<SqlitePool>, headers: HeaderMap,
    Path(content_type_key): Path<String>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    if !valid_content_type_key(&content_type_key) {
        return Err(ApiError::bad_request("Khóa CPT không hợp lệ."));
    }
    let keys = crate::storage::content::load_effective_taxonomies(&db, &content_type_key).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(keys)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_match_requires_exact_version() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            expected_version(&headers).unwrap_err().0,
            StatusCode::PRECONDITION_REQUIRED
        );
        for invalid in ["*", "\"v0\"", "\"v01\"", "v1", "W/\"v1\"", "\"v-1\""] {
            headers.insert(header::IF_MATCH, invalid.parse().unwrap());
            assert_eq!(
                expected_version(&headers).unwrap_err().0,
                StatusCode::BAD_REQUEST
            );
        }
        headers.insert(header::IF_MATCH, "\"v17\"".parse().unwrap());
        assert_eq!(expected_version(&headers).unwrap(), 17);
    }

    #[test]
    fn null_parent_is_distinct_from_omitted_parent() {
        let omitted: PatchTaxonomyTerm = serde_json::from_str(r#"{"name":"Changed"}"#).unwrap();
        assert_eq!(omitted.parent_id, None);
        let root: PatchTaxonomyTerm = serde_json::from_str(r#"{"parent_id":null}"#).unwrap();
        assert_eq!(root.parent_id, Some(None));
        let child: PatchTaxonomyTerm = serde_json::from_str(r#"{"parent_id":42}"#).unwrap();
        assert_eq!(child.parent_id, Some(Some(42)));
        assert!(serde_json::from_str::<PatchTaxonomyTerm>(r#"{"kind":"tag"}"#).is_err());
    }
}
