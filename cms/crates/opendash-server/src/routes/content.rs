//! Admin API to create/list CPT definitions. Entry editing follows in M3.3.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use opendash_core::{content::ContentTypeDefinition, registry::OwnedDescriptor};
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin_with_csrf, require_session},
    error::{ApiError, ApiResult},
    storage::content as content_storage,
};

pub const MAX_CONTENT_TYPE_JSON_BYTES: usize = 512 * 1024;

pub async fn content_type_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<OwnedDescriptor<ContentTypeDefinition>>>> {
    require_session(&headers, &db).await?;
    Ok(Json(content_storage::list_content_types(&db).await?))
}

pub async fn content_type_create(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Json(definition): Json<ContentTypeDefinition>,
) -> ApiResult<(StatusCode, Json<OwnedDescriptor<ContentTypeDefinition>>)> {
    require_admin_with_csrf(&headers, &db).await?;
    definition.validate().map_err(ApiError::bad_request)?;
    let result = content_storage::insert_user_content_type(&db, &definition).await?;
    Ok((StatusCode::CREATED, Json(result)))
}
