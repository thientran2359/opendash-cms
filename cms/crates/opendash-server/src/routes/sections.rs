//! Editor/Admin-only Section CRUD and exact-entry Builder AST.
use std::sync::Arc;

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::builder::{BuilderDocument, MAX_BUILDER_JSON_BYTES};
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin_with_csrf, require_session, require_session_with_csrf},
    error::{ApiError, ApiResult},
    media::MediaStore,
    storage::sections::{self, SectionInput},
};

fn expected_version(headers: &HeaderMap) -> ApiResult<i64> {
    let raw = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError::precondition_required("Section cần If-Match để sửa/xóa."))?
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    let digits = raw
        .strip_prefix("\"v")
        .and_then(|v| v.strip_suffix('"'))
        .ok_or_else(|| ApiError::bad_request("If-Match phải có dạng \"vN\"."))?;
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::bad_request("If-Match không hợp lệ."));
    }
    digits
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| ApiError::bad_request("If-Match không hợp lệ."))
}

fn private_headers(version: Option<i64>) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if let Some(v) = version {
        h.insert(
            header::ETAG,
            HeaderValue::from_str(&format!("\"v{v}\"")).expect("numeric etag"),
        );
    }
    h
}

fn document_from_request(headers: &HeaderMap, bytes: &Bytes) -> ApiResult<BuilderDocument> {
    let json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"));
    if !json {
        return Err(ApiError::unsupported_media_type(
            "Section Builder cần application/json.",
        ));
    }
    if bytes.len() > MAX_BUILDER_JSON_BYTES {
        return Err(ApiError::bad_request("Section Builder vượt 256 KiB."));
    }
    BuilderDocument::from_json(bytes).map_err(|e| ApiError::bad_request(e.to_string()))
}

pub async fn section_list(State(db): State<SqlitePool>, headers: HeaderMap) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    Ok((private_headers(None), Json(sections::list(&db).await?)).into_response())
}

pub async fn section_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Json(input): Json<SectionInput>,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    let created = sections::create(&db, &input).await?;
    Ok((
        StatusCode::CREATED,
        private_headers(Some(created.version)),
        Json(created),
    )
        .into_response())
}

pub async fn section_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let record = sections::get(&db, id).await?;
    Ok((private_headers(Some(record.version)), Json(record)).into_response())
}

pub async fn section_update(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(input): Json<SectionInput>,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    let record = sections::update(&db, id, expected_version(&headers)?, &input).await?;
    Ok((private_headers(Some(record.version)), Json(record)).into_response())
}

pub async fn section_delete(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    require_session_with_csrf(&headers, &db).await?;
    sections::delete(&db, id, expected_version(&headers)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn section_document_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let record = sections::read_document(&db, id).await?;
    Ok((private_headers(Some(record.version)), Json(record)).into_response())
}

pub async fn section_document_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    bytes: Bytes,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    let version = expected_version(&headers)?;
    let document = document_from_request(&headers, &bytes)?;
    let saved = sections::save_document(&db, id, version, &document).await?;
    Ok((private_headers(Some(saved.opened.version)), Json(saved)).into_response())
}

pub async fn section_document_preview(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    bytes: Bytes,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    let document = document_from_request(&headers, &bytes)?;
    let rendered = sections::preview(&db, id, &document).await?;
    Ok((private_headers(None), Json(rendered)).into_response())
}

/// The Section-specific private pin is NOT a public media grant. Check it in
/// the SAME writer transaction as the generic media tombstone, so concurrent
/// Section Save/Delete and Media Delete cannot race past each other. This
/// wrapper replaces only the registered DELETE handler; other media routes
/// and the existing lifecycle implementation remain unchanged.
pub async fn media_delete_section_aware(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let (expected_generation, expected_version) =
        crate::media_lifecycle::parse_media_if_match(&headers)?;
    let mut tx = db.begin().await?;
    // Check private pins and tombstone in one reserved writer transaction.
    if let Some(pins) =
        sections::media_pins_for_delete_in_tx(&mut tx, &id, expected_generation, expected_version)
            .await?
    {
        return Ok((StatusCode::CONFLICT, [(header::CACHE_CONTROL, "no-store")], Json(
            serde_json::json!({
                "error":"Media đang được Section riêng tư sử dụng; không thể xóa.",
                "pins": {"total_pins": pins.total_pins,
                         "pins": pins.entry_ids.into_iter().take(50).map(|entry_id|
                             serde_json::json!({"source_kind":"section_document","entity_id":entry_id.to_string()}))
                             .collect::<Vec<_>>()}
            }),
        )).into_response());
    }
    let job = match crate::media_lifecycle::tombstone_media_in_tx(
        &mut tx,
        &id,
        expected_generation,
        expected_version,
    )
    .await
    {
        Ok(job) => job,
        Err(crate::media_lifecycle::TombstoneError::Referenced(pins)) => {
            return Ok((StatusCode::CONFLICT, [(header::CACHE_CONTROL, "no-store")],
                Json(serde_json::json!({"error":"Media đang được sử dụng; không thể xóa.","pins":pins})))
                .into_response());
        }
        Err(crate::media_lifecycle::TombstoneError::Api(err)) => return Err(err),
    };
    tx.commit().await?;
    let final_job = crate::media_lifecycle::execute_delete_job(&db, &store, job, &lease).await?;
    let status = if final_job.phase == crate::media_lifecycle::MediaDeletePhase::Finalized {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok((
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(final_job),
    )
        .into_response())
}
