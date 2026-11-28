//! M4 P09: Admin category/tag & custom taxonomy management. SQL remains in storage/taxonomy.

mod definitions;
mod terms;

pub use definitions::*;
pub use terms::*;

pub(super) use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};

pub(super) use opendash_core::content::{
    FieldRole, PatchField, TaxonomyDefinition, TaxonomyDefinitionInput, TaxonomyDefinitionPatch,
    TermSeoInput, deserialize_patch_field, valid_content_type_key,
};

pub(super) use serde::{Deserialize, Deserializer, de::DeserializeOwned};
pub(super) use serde_json::Value;
pub(super) use sqlx::SqlitePool;
pub(super) use std::collections::BTreeMap;

pub(super) use crate::{
    auth::{
        Role, require_admin, require_admin_with_csrf, require_session, require_session_with_csrf,
    },
    error::{ApiError, ApiResult},
    storage::taxonomy,
};

pub const MAX_TAXONOMY_JSON_BYTES: usize = 16 * 1024;

pub(super) fn parse_body<T: DeserializeOwned>(
    payload: Result<Json<T>, JsonRejection>,
) -> ApiResult<T> {
    payload
        .map(|Json(body)| body)
        .map_err(|_| ApiError::bad_request("Dữ liệu JSON taxonomy không hợp lệ."))
}

pub(super) fn valid_id(id: i64) -> ApiResult<()> {
    if id > 0 {
        Ok(())
    } else {
        Err(ApiError::bad_request("ID term phải là số nguyên dương."))
    }
}

pub(super) fn expected_version(headers: &HeaderMap) -> ApiResult<i64> {
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

#[cfg(test)]
mod tests;
