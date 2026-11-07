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
    auth::{require_admin, require_admin_with_csrf},
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
        .install_plugin(&bytes)
        .await
        .map_err(ApiError::bad_request)?;
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

pub async fn plugin_disable(
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;

    plugins
        .disable_plugin(&id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(serde_json::json!({ "message": "Đã vô hiệu hóa plugin." })).into_response())
}

pub async fn plugin_enable(
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;

    plugins
        .enable_plugin(&id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(serde_json::json!({ "message": "Đã kích hoạt plugin." })).into_response())
}
