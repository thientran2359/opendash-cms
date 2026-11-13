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

pub const UI_SCHEMA_VERSION: u32 = 1;
pub const MAX_UI_SCREENS: usize = 20;
pub const MAX_UI_DEPTH: usize = 8;
pub const MAX_UI_NODES: usize = 200;
pub const MAX_UI_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeclarativeUiSchemaV1 {
    pub plugin_id: String,
    pub schema_version: u32,
    #[serde(default)]
    pub screens: Vec<UiScreen>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UiScreen {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub nodes: Vec<UiNode>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UiNode {
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub field_type: Option<String>,
    #[serde(default)]
    pub action_id: Option<String>,
    #[serde(default)]
    pub page_size: Option<usize>,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub children: Vec<UiNode>,
}

fn contains_disallowed_markup(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains('<') || lower.contains('>') || lower.contains("javascript:")
}

fn validate_ui_node(
    node: &UiNode,
    depth: usize,
    node_count: &mut usize,
    allowed_actions: &[String],
) -> Result<(), &'static str> {
    if depth > MAX_UI_DEPTH {
        return Err("Declarative UI schema vượt quá độ sâu tối đa (8).");
    }
    *node_count += 1;
    if *node_count > MAX_UI_NODES {
        return Err("Declarative UI schema vượt quá số lượng node tối đa (200).");
    }
    if node.id.trim().is_empty() || contains_disallowed_markup(&node.id) {
        return Err("Node id không hợp lệ.");
    }
    if let Some(ref lbl) = node.label {
        if contains_disallowed_markup(lbl) {
            return Err("Label không được chứa HTML/JS.");
        }
    }
    match node.kind.as_str() {
        "form" | "tabs" => {}
        "table" | "list" => {
            if let Some(ps) = node.page_size {
                if ps == 0 || ps > MAX_UI_PAGE_SIZE {
                    return Err("page_size vượt quá giới hạn 100.");
                }
            }
        }
        "button" => {
            let Some(ref act) = node.action_id else {
                return Err("Button bắt buộc phải gắn action_id.");
            };
            if !allowed_actions.iter().any(|a| a == act) {
                return Err("Button tham chiếu action_id không tồn tại trong manifest.");
            }
        }
        "field" => {
            let ft = node.field_type.as_deref().unwrap_or("text");
            if !matches!(ft, "text" | "textarea" | "number" | "checkbox" | "select") {
                return Err("Field type không được hỗ trợ hoặc bị cấm (không hỗ trợ secret/html/js/css).");
            }
        }
        _ => return Err("Node kind không thuộc schema v1."),
    }
    if let Some(ref act) = node.action_id {
        if !allowed_actions.iter().any(|a| a == act) {
            return Err("action_id không tồn tại trong manifest.");
        }
    }
    for child in &node.children {
        validate_ui_node(child, depth + 1, node_count, allowed_actions)?;
    }
    Ok(())
}

pub fn validate_declarative_ui_schema_v1(
    schema: &DeclarativeUiSchemaV1,
    allowed_actions: &[String],
) -> Result<(), &'static str> {
    if schema.schema_version != UI_SCHEMA_VERSION {
        return Err("Chỉ hỗ trợ declarative UI schema_version 1.");
    }
    if schema.screens.len() > MAX_UI_SCREENS {
        return Err("Số lượng screen vượt quá giới hạn 20.");
    }
    let mut node_count = 0usize;
    for screen in &schema.screens {
        if screen.id.trim().is_empty() || contains_disallowed_markup(&screen.title) {
            return Err("Screen không hợp lệ.");
        }
        for node in &screen.nodes {
            validate_ui_node(node, 1, &mut node_count, allowed_actions)?;
        }
    }
    Ok(())
}

pub async fn plugin_ui_schema(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<Response> {
    let actor = crate::auth::require_session(&headers, &db).await?;
    let Some((_, manifest_json, enabled)) = crate::storage::plugin::load_plugin_executable(&db, &id)
        .await?
    else {
        return Err(ApiError::not_found("Không tìm thấy plugin đang hoạt động."));
    };
    if !enabled {
        return Err(ApiError::not_found("Không tìm thấy plugin đang hoạt động."));
    }
    let manifest = opendash_core::plugin::PluginManifest::from_json_str(&manifest_json)
        .map_err(ApiError::internal)?;

    let allowed_actions: Vec<String> = manifest
        .actions
        .iter()
        .filter(|a| matches!(actor.user.role, crate::auth::Role::Admin) || a.required_role == "editor")
        .map(|a| a.id.clone())
        .collect();

    let mut nodes = Vec::new();
    for action in &manifest.actions {
        if !allowed_actions.iter().any(|id| id == &action.id) {
            continue;
        }
        nodes.push(UiNode {
            kind: "form".into(),
            id: format!("form-{}", action.id),
            label: Some(if action.title.is_empty() {
                action.id.clone()
            } else {
                action.title.clone()
            }),
            field_type: None,
            action_id: Some(action.id.clone()),
            page_size: None,
            columns: vec![],
            options: vec![],
            children: vec![
                UiNode {
                    kind: "field".into(),
                    id: "payload".into(),
                    label: Some("Dữ liệu".into()),
                    field_type: Some("text".into()),
                    action_id: None,
                    page_size: None,
                    columns: vec![],
                    options: vec![],
                    children: vec![],
                },
                UiNode {
                    kind: "button".into(),
                    id: format!("submit-{}", action.id),
                    label: Some("Thực thi".into()),
                    field_type: None,
                    action_id: Some(action.id.clone()),
                    page_size: None,
                    columns: vec![],
                    options: vec![],
                    children: vec![],
                },
            ],
        });
    }

    let schema = DeclarativeUiSchemaV1 {
        plugin_id: manifest.id.clone(),
        schema_version: UI_SCHEMA_VERSION,
        screens: vec![UiScreen {
            id: "main".into(),
            title: manifest.name.clone(),
            nodes,
        }],
    };
    validate_declarative_ui_schema_v1(&schema, &allowed_actions).map_err(ApiError::bad_request)?;
    Ok(Json(schema).into_response())
}

#[cfg(test)]
mod declarative_ui_tests {
    use super::*;

    #[test]
    fn schema_v1_enforces_quotas_forbids_secrets_and_rejects_forged_actions() {
        let allowed = vec!["sync".to_string()];

        // 1. Secret field rejected
        let secret_schema = DeclarativeUiSchemaV1 {
            plugin_id: "p1".into(),
            schema_version: 1,
            screens: vec![UiScreen {
                id: "s1".into(),
                title: "Main".into(),
                nodes: vec![UiNode {
                    kind: "field".into(),
                    id: "api_key".into(),
                    label: Some("API Key".into()),
                    field_type: Some("secret".into()),
                    action_id: None,
                    page_size: None,
                    columns: vec![],
                    options: vec![],
                    children: vec![],
                }],
            }],
        };
        assert!(validate_declarative_ui_schema_v1(&secret_schema, &allowed).is_err());

        // 2. Depth > 8 rejected
        let mut deep = UiNode {
            kind: "tabs".into(),
            id: "d9".into(),
            label: None,
            field_type: None,
            action_id: None,
            page_size: None,
            columns: vec![],
            options: vec![],
            children: vec![],
        };
        for i in (1..9).rev() {
            deep = UiNode {
                kind: "tabs".into(),
                id: format!("d{i}"),
                label: None,
                field_type: None,
                action_id: None,
                page_size: None,
                columns: vec![],
                options: vec![],
                children: vec![deep],
            };
        }
        let deep_schema = DeclarativeUiSchemaV1 {
            plugin_id: "p1".into(),
            schema_version: 1,
            screens: vec![UiScreen {
                id: "s1".into(),
                title: "Main".into(),
                nodes: vec![deep],
            }],
        };
        assert!(validate_declarative_ui_schema_v1(&deep_schema, &allowed).is_err());

        // 3. Forged action rejected
        let forged_schema = DeclarativeUiSchemaV1 {
            plugin_id: "p1".into(),
            schema_version: 1,
            screens: vec![UiScreen {
                id: "s1".into(),
                title: "Main".into(),
                nodes: vec![UiNode {
                    kind: "button".into(),
                    id: "b1".into(),
                    label: Some("Hack".into()),
                    field_type: None,
                    action_id: Some("forged_action".into()),
                    page_size: None,
                    columns: vec![],
                    options: vec![],
                    children: vec![],
                }],
            }],
        };
        assert!(validate_declarative_ui_schema_v1(&forged_schema, &allowed).is_err());
    }
}
