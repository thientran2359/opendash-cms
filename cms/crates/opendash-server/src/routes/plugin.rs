use std::sync::Arc;

use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::Value;
use sqlx::SqlitePool;

use crate::{
    auth::{require_admin, require_admin_with_csrf, require_session_with_csrf},
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    package::is_valid_zip_content_type,
    plugin::PluginManager,
};

pub const MAX_PLUGIN_ZIP_BYTES: usize = 5 * 1024 * 1024; // 5 MiB
pub const MAX_PLUGIN_JSON_BYTES: usize = 1024 * 1024; // 1 MiB

pub async fn plugin_install(
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    State(content_registry): State<Arc<tokio::sync::RwLock<opendash_core::registry::RegistryCatalog<opendash_core::content::ContentTypeDefinition>>>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok());
    if !is_valid_zip_content_type(content_type, false) {
        return Err(ApiError::bad_request("Content-Type phải là file ZIP."));
    }

    let summary = plugins
        .install_plugin_checked(&bytes)
        .await
        .map_err(map_plugin_storage_err)?;
    *content_registry.write().await = crate::storage::content::load_content_type_catalog(&db).await?;
    Ok((StatusCode::CREATED, Json(summary)).into_response())
}

pub async fn plugin_list(
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let _session = require_admin(&headers, &db).await?;
    let list = plugins.list_plugins().await.map_err(ApiError::internal)?;
    Ok(Json(list).into_response())
}

pub async fn plugin_run(
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    bytes: Bytes,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;

    let input_val: Value = if bytes.is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "success": false,
                        "data": null,
                        "error": format!("Dữ liệu đầu vào không phải JSON hợp lệ: {e}"),
                        "code": "invalid_input",
                        "correlation_id": ""
                    })),
                )
                    .into_response());
            }
        }
    };

    match hooks.dispatch_plugin_run(&id, input_val).await {
        Ok(resp) => Ok((StatusCode::OK, Json(resp)).into_response()),
        Err((status, resp)) => Ok((status, Json(resp)).into_response()),
    }
}

pub async fn plugin_action(
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    State(content_registry): State<Arc<tokio::sync::RwLock<opendash_core::registry::RegistryCatalog<opendash_core::content::ContentTypeDefinition>>>>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    axum::extract::Path((id, action)): axum::extract::Path<(String, String)>,
    bytes: Bytes,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let input_val: Value = if bytes.is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(e) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "success": false,
                        "data": null,
                        "error": format!("Dữ liệu đầu vào không phải JSON hợp lệ: {e}"),
                        "code": "invalid_input",
                        "correlation_id": ""
                    })),
                )
                    .into_response());
            }
        }
    };
    let correlation_id = format!(
        "act-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    match plugins
        .execute_plugin_action(
            &id,
            &action,
            &actor,
            input_val,
            &content_registry,
            &hooks,
            correlation_id,
        )
        .await
    {
        Ok(resp) => Ok((StatusCode::OK, Json(resp)).into_response()),
        Err((status, resp)) => Ok((status, Json(resp)).into_response()),
    }
}

pub async fn plugin_disable(
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    State(content_registry): State<Arc<tokio::sync::RwLock<opendash_core::registry::RegistryCatalog<opendash_core::content::ContentTypeDefinition>>>>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;

    let expected_cas = parse_optional_if_match(&headers)?;
    let next_cas = plugins
        .disable_plugin_cas(&id, expected_cas)
        .await
        .map_err(map_plugin_storage_err)?;
    *content_registry.write().await = crate::storage::content::load_content_type_catalog(&db).await?;
    Ok(Json(serde_json::json!({ "message": "Đã vô hiệu hóa plugin.", "cas_version": next_cas })).into_response())
}

pub async fn plugin_enable(
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    State(content_registry): State<Arc<tokio::sync::RwLock<opendash_core::registry::RegistryCatalog<opendash_core::content::ContentTypeDefinition>>>>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;

    let expected_cas = parse_optional_if_match(&headers)?;
    let next_cas = plugins
        .enable_plugin_cas(&id, expected_cas)
        .await
        .map_err(map_plugin_storage_err)?;
    *content_registry.write().await = crate::storage::content::load_content_type_catalog(&db).await?;
    Ok(Json(serde_json::json!({ "message": "Đã kích hoạt plugin.", "cas_version": next_cas })).into_response())
}

fn parse_optional_if_match(headers: &HeaderMap) -> ApiResult<Option<i64>> {
    let Some(raw) = headers.get(header::IF_MATCH) else {
        return Ok(None);
    };
    let s = raw
        .to_str()
        .map_err(|_| ApiError::bad_request("Header If-Match không hợp lệ."))?
        .trim()
        .trim_matches('"');
    let parsed = s
        .parse::<i64>()
        .map_err(|_| ApiError::bad_request("Header If-Match phải là số nguyên."))?;
    Ok(Some(parsed))
}

fn map_plugin_storage_err(err: crate::storage::plugin::PluginStorageError) -> ApiError {
    use crate::storage::plugin::PluginStorageError;
    match err {
        PluginStorageError::BadRequest(m) => ApiError::bad_request(m),
        PluginStorageError::NotFound(m) => ApiError::not_found(m),
        PluginStorageError::Conflict(m) => ApiError::conflict(m),
        PluginStorageError::PreconditionRequired(m) => ApiError::precondition_required(m),
        PluginStorageError::PreconditionFailed(m) => ApiError::precondition_failed(m),
        PluginStorageError::Internal(m) => ApiError::internal(m),
    }
}
