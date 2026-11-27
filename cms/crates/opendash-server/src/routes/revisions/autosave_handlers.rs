use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use opendash_core::builder::MAX_BUILDER_JSON_BYTES;
use opendash_core::content::PatchField;
use sqlx::SqlitePool;

use crate::auth::{require_session, require_session_with_csrf};
use crate::error::{ApiError, ApiResult};
use crate::routes::settings::no_store_headers;
use crate::storage::revisions::{
    AutosaveUpsertInput, delete_entry_autosave, get_entry_autosave, upsert_entry_autosave,
};

use super::{guard_type_key_access, parse_positive_id, require_json_content_type};

pub(super) fn validate_autosave_input_syntax(input: &AutosaveUpsertInput) -> ApiResult<()> {
    if input.base_version < 1 || input.client_seq < 1 {
        return Err(ApiError::bad_request(
            "base_version và client_seq phải là số nguyên dương lớn hơn hoặc bằng 1.",
        ));
    }
    match &input.title {
        PatchField::Clear => {
            return Err(ApiError::bad_request(
                "Tiêu đề bản lưu tự động không được đặt là null.",
            ));
        }
        PatchField::Set(title) => {
            if title.chars().any(char::is_control) || title.chars().count() > 200 {
                return Err(ApiError::bad_request(
                    "Tiêu đề bản lưu tự động không được chứa ký tự điều khiển hoặc vượt quá 200 ký tự.",
                ));
            }
        }
        PatchField::Omitted => {}
    }
    if input.cleared_fields.len() > 128 {
        return Err(ApiError::bad_request(
            "Danh sách cleared_fields vượt quá giới hạn 128 trường.",
        ));
    }
    if input.term_ids.as_ref().is_some_and(|ids| ids.len() > 96) {
        return Err(ApiError::bad_request(
            "Danh sách term_ids vượt quá giới hạn 96 mục.",
        ));
    }
    if let PatchField::Set(ref seo) = input.seo {
        let bytes = serde_json::to_vec(seo)
            .map_err(|_| ApiError::bad_request("Dữ liệu SEO lưu tự động không hợp lệ."))?;
        if bytes.len() > 4096 {
            return Err(ApiError::bad_request(
                "Dữ liệu SEO lưu tự động vượt quá giới hạn 4096 byte.",
            ));
        }
    }
    Ok(())
}

/// Reads the current authenticated user's autosave draft for an entry, if any.
pub async fn autosave_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, raw_id)): Path<(String, String)>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    guard_type_key_access(&type_key, actor.user.role)?;
    let entry_id = parse_positive_id(&raw_id, "ID nội dung không hợp lệ.")?;
    let resp = get_entry_autosave(&db, &actor, &type_key, entry_id).await?;
    Ok((no_store_headers(None), Json(resp)).into_response())
}

/// Creates or updates the current user's isolated autosave draft with monotonic `client_seq`.
pub async fn autosave_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, raw_id)): Path<(String, String)>,
    bytes: Bytes,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    guard_type_key_access(&type_key, actor.user.role)?;
    let entry_id = parse_positive_id(&raw_id, "ID nội dung không hợp lệ.")?;
    require_json_content_type(&headers)?;
    if type_key == "pattern" && bytes.len() > MAX_BUILDER_JSON_BYTES + 16 * 1024 {
        return Err(ApiError::bad_request(
            "Dữ liệu lưu tự động Pattern vượt quá giới hạn kích thước cho phép.",
        ));
    }
    let input: AutosaveUpsertInput = serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::bad_request("Dữ liệu lưu tự động JSON không hợp lệ."))?;
    validate_autosave_input_syntax(&input)?;
    let resp = upsert_entry_autosave(&db, &actor, &type_key, entry_id, &input).await?;
    Ok((no_store_headers(None), Json(resp)).into_response())
}

/// Discards the current user's autosave draft for an entry (idempotent 204).
pub async fn autosave_delete(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, raw_id)): Path<(String, String)>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    guard_type_key_access(&type_key, actor.user.role)?;
    let entry_id = parse_positive_id(&raw_id, "ID nội dung không hợp lệ.")?;
    delete_entry_autosave(&db, &actor, &type_key, entry_id).await?;
    Ok((StatusCode::NO_CONTENT, no_store_headers(None)).into_response())
}
