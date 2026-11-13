//! M4 P09: Admin category/tag management. SQL remains in storage/taxonomy.

use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin_with_csrf, require_session},
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

pub async fn taxonomy_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(query): Query<TaxonomyPagination>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    if query
        .kind
        .as_deref()
        .is_some_and(|kind| !taxonomy::valid_kind(kind))
    {
        return Err(ApiError::bad_request(
            "Loại term phải là category hoặc tag.",
        ));
    }
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) || offset < 0 {
        return Err(ApiError::bad_request("Phân trang taxonomy không hợp lệ."));
    }
    let page = taxonomy::list_terms(&db, query.kind.as_deref(), limit, offset).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(page)).into_response())
}

pub async fn taxonomy_create(
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
    Ok(term_response(StatusCode::CREATED, created))
}

pub async fn taxonomy_patch(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    payload: Result<Json<PatchTaxonomyTerm>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    valid_id(id)?;
    let version = expected_version(&headers)?;
    let input = parse_body(payload)?;
    if input.name.is_none() && input.slug.is_none() && input.parent_id.is_none() {
        return Err(ApiError::bad_request(
            "Cần name, slug hoặc parent_id để cập nhật.",
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
    )
    .await?;
    Ok(term_response(StatusCode::OK, updated))
}

pub async fn taxonomy_delete(
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
