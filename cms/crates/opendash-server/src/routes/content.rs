//! Admin API for CPT definitions and extension field groups (`M3` / `M7-S09-T01`).

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use opendash_core::{
    content::{
        ContentTypeDefinition, ContentTypeDefinitionInput, FieldGroupRecord, UserFieldGroupInput,
        valid_content_type_key,
    },
    registry::RegistryCatalog,
};
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin, require_admin_with_csrf, require_session},
    error::{ApiError, ApiResult},
    routes::settings::{no_store_headers, parse_if_match},
    storage::content::{self as content_storage, ContentTypeApiItem},
};

pub const MAX_CONTENT_TYPE_JSON_BYTES: usize = 512 * 1024;

pub async fn content_type_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<ContentTypeApiItem>>> {
    require_session(&headers, &db).await?;
    Ok(Json(content_storage::list_content_types_with_meta(&db).await?))
}

pub async fn content_type_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(content_registry): State<Arc<tokio::sync::RwLock<RegistryCatalog<ContentTypeDefinition>>>>,
    headers: HeaderMap,
    Json(input): Json<ContentTypeDefinitionInput>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let result = content_storage::insert_user_content_type(&db, &input).await?;
    *content_registry.write().await = content_storage::load_content_type_catalog(&db).await?;
    Ok((
        StatusCode::CREATED,
        no_store_headers(Some(result.version)),
        Json(result),
    ))
}

pub async fn content_type_update(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(content_registry): State<Arc<tokio::sync::RwLock<RegistryCatalog<ContentTypeDefinition>>>>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Json(input): Json<ContentTypeDefinitionInput>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    if !valid_content_type_key(&type_key) {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    }
    let expected_version = parse_if_match(&headers)?;
    let result =
        content_storage::update_user_content_type(&db, &type_key, expected_version, &input).await?;
    *content_registry.write().await = content_storage::load_content_type_catalog(&db).await?;
    Ok((no_store_headers(Some(result.version)), Json(result)))
}

pub async fn field_group_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    require_admin(&headers, &db).await?;
    let groups: Vec<FieldGroupRecord> = content_storage::list_field_groups(&db).await?;
    Ok((no_store_headers(None), Json(groups)))
}

pub async fn field_group_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Json(input): Json<UserFieldGroupInput>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let created = content_storage::create_user_field_group(&db, &input).await?;
    Ok((
        StatusCode::CREATED,
        no_store_headers(Some(created.version)),
        Json(created),
    ))
}

pub async fn field_group_update(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Json(input): Json<UserFieldGroupInput>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let expected_version = parse_if_match(&headers)?;
    let updated =
        content_storage::update_user_field_group(&db, &group_id, expected_version, &input).await?;
    Ok((no_store_headers(Some(updated.version)), Json(updated)))
}

pub async fn field_group_delete(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    require_admin_with_csrf(&headers, &db).await?;
    let expected_version = parse_if_match(&headers)?;
    let withdrawn =
        content_storage::withdraw_user_field_group(&db, &group_id, expected_version).await?;
    Ok((no_store_headers(Some(withdrawn.version)), Json(withdrawn)))
}
