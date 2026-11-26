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

async fn maybe_spawn_projection_recompute(
    db: &SqlitePool,
    plugins: &PluginManager,
    hooks: &Arc<HookDispatcher>,
    coordinator: &Arc<crate::operations::SiteCoordinator>,
    plugin_id: &str,
) {
    let Some(manifest) = plugins.cached_plugin_manifest(plugin_id).await else {
        return;
    };
    if manifest
        .filters
        .iter()
        .chain(manifest.hooks.iter())
        .any(|h| {
            matches!(
                h,
                opendash_core::hooks::HookEvent::FilterSeoHead
                    | opendash_core::hooks::HookEvent::FilterEntryView
            )
        })
    {
        let _handle =
            crate::storage::plugin_projections::spawn_recompute_plugin_projections_batched(
                db.clone(),
                Arc::clone(hooks),
                Arc::clone(coordinator),
                plugin_id.to_owned(),
                coordinator.maintenance_cancel_rx(),
                coordinator.shutdown_rx(),
            )
            .await;
    }
}

pub async fn plugin_install(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    State(content_registry): State<
        Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
    >,
    State(hooks): State<Arc<HookDispatcher>>,
    State(coordinator): State<Arc<crate::operations::SiteCoordinator>>,
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
    *content_registry.write().await =
        crate::storage::content::load_content_type_catalog(&db).await?;
    if summary.enabled {
        maybe_spawn_projection_recompute(&db, &plugins, &hooks, &coordinator, &summary.id).await;
    }
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
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
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
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    State(content_registry): State<
        Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
    >,
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
    let maint_epoch = lease.maintenance_epoch();
    match plugins
        .execute_plugin_action(
            &id,
            &action,
            &actor,
            input_val,
            &content_registry,
            &hooks,
            correlation_id,
            crate::plugin::lifecycle_confirmation::LifecycleActionOrigin::Generic,
            maint_epoch,
        )
        .await
    {
        Ok(outcome) => {
            let principal = match actor.user.role {
                crate::auth::Role::Admin => opendash_core::hooks::ExecutionPrincipal::Admin,
                crate::auth::Role::Editor => opendash_core::hooks::ExecutionPrincipal::Editor,
            };
            hooks.enqueue_plugin_action_hooks(principal, outcome.committed.as_ref(), lease);
            Ok((
                StatusCode::OK,
                [(header::CACHE_CONTROL, "no-store")],
                Json(outcome.response),
            )
                .into_response())
        }
        Err((status, resp)) => {
            Ok((status, [(header::CACHE_CONTROL, "no-store")], Json(resp)).into_response())
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmPluginLifecycleInput {
    pub confirmation_id: String,
    pub decision: String,
}

pub async fn plugin_action_confirm_lifecycle(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    axum::extract::Path((id, action)): axum::extract::Path<(String, String)>,
    Json(input): Json<ConfirmPluginLifecycleInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let correlation_id = format!(
        "plc-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let maint_epoch = lease.maintenance_epoch();
    match plugins
        .resolve_plugin_lifecycle_confirmation(
            &id,
            &action,
            &actor,
            &input.confirmation_id,
            &input.decision,
            &hooks,
            correlation_id,
            maint_epoch,
        )
        .await
    {
        Ok(outcome) => {
            let principal = match actor.user.role {
                crate::auth::Role::Admin => opendash_core::hooks::ExecutionPrincipal::Admin,
                crate::auth::Role::Editor => opendash_core::hooks::ExecutionPrincipal::Editor,
            };
            hooks.enqueue_plugin_action_hooks(principal, outcome.committed.as_ref(), lease);
            Ok((
                StatusCode::OK,
                [(header::CACHE_CONTROL, "no-store")],
                Json(outcome.response),
            )
                .into_response())
        }
        Err((status, resp)) => {
            Ok((status, [(header::CACHE_CONTROL, "no-store")], Json(resp)).into_response())
        }
    }
}

pub async fn plugin_disable(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    State(content_registry): State<
        Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
    >,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;

    let expected_cas = parse_optional_if_match(&headers)?;
    let next_cas = plugins
        .disable_plugin_cas(&id, expected_cas)
        .await
        .map_err(map_plugin_storage_err)?;
    *content_registry.write().await =
        crate::storage::content::load_content_type_catalog(&db).await?;
    hooks.enqueue_async(crate::hooks::AsyncHookJob {
        event: opendash_core::hooks::HookEvent::PluginAfterDisable,
        principal: opendash_core::hooks::ExecutionPrincipal::Admin,
        locale: None,
        type_key: None,
        payload: serde_json::json!({
            "plugin_id": &id,
            "cas_version": next_cas,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok(
        Json(serde_json::json!({ "message": "Đã vô hiệu hóa plugin.", "cas_version": next_cas }))
            .into_response(),
    )
}

pub async fn plugin_enable(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    State(content_registry): State<
        Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
    >,
    State(hooks): State<Arc<HookDispatcher>>,
    State(coordinator): State<Arc<crate::operations::SiteCoordinator>>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;

    let expected_cas = parse_optional_if_match(&headers)?;
    let next_cas = plugins
        .enable_plugin_cas(&id, expected_cas)
        .await
        .map_err(map_plugin_storage_err)?;
    *content_registry.write().await =
        crate::storage::content::load_content_type_catalog(&db).await?;
    maybe_spawn_projection_recompute(&db, &plugins, &hooks, &coordinator, &id).await;
    hooks.enqueue_async(crate::hooks::AsyncHookJob {
        event: opendash_core::hooks::HookEvent::PluginAfterEnable,
        principal: opendash_core::hooks::ExecutionPrincipal::Admin,
        locale: None,
        type_key: None,
        payload: serde_json::json!({
            "plugin_id": &id,
            "cas_version": next_cas,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok(
        Json(serde_json::json!({ "message": "Đã kích hoạt plugin.", "cas_version": next_cas }))
            .into_response(),
    )
}

pub const MAX_PLUGIN_SETTINGS_JSON_BYTES: usize = 64 * 1024;

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSettingsPutInput {
    pub values: serde_json::Map<String, Value>,
}

pub async fn plugin_get_settings(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    if !opendash_core::plugin::valid_plugin_id(&id) {
        return Err(ApiError::not_found("Không tìm thấy plugin."));
    }
    let Some(manifest) =
        crate::storage::settings::get_plugin_manifest_for_settings(&db, &id).await?
    else {
        return Err(ApiError::not_found("Không tìm thấy plugin."));
    };
    let snapshot =
        crate::storage::settings::get_plugin_settings_redacted(&db, &id, &manifest.settings_schema)
            .await?;
    Ok((
        crate::routes::settings::no_store_headers(Some(snapshot.version)),
        Json(snapshot),
    )
        .into_response())
}

pub async fn plugin_put_settings(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<PluginManager>>,
    State(hooks): State<Arc<HookDispatcher>>,
    State(coordinator): State<Arc<crate::operations::SiteCoordinator>>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    input: Result<Json<PluginSettingsPutInput>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let expected_version = crate::routes::settings::parse_if_match(&headers)?;
    let Json(input) = input.map_err(|err| {
        if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Dữ liệu cài đặt plugin vượt quá 64 KiB.".into(),
            )
        } else {
            ApiError::bad_request("Dữ liệu cài đặt plugin không hợp lệ.")
        }
    })?;
    if !opendash_core::plugin::valid_plugin_id(&id) {
        return Err(ApiError::not_found("Không tìm thấy plugin."));
    }
    let Some(manifest) =
        crate::storage::settings::get_plugin_manifest_for_settings(&db, &id).await?
    else {
        return Err(ApiError::not_found("Không tìm thấy plugin."));
    };
    let outcome = crate::storage::settings::put_plugin_settings(
        &db,
        plugins.secrets_root(),
        &id,
        expected_version,
        &manifest.settings_schema,
        &input.values,
    )
    .await?;
    if outcome.changed {
        maybe_spawn_projection_recompute(&db, &plugins, &hooks, &coordinator, &id).await;
    }
    hooks.enqueue_async(crate::hooks::AsyncHookJob {
        event: opendash_core::hooks::HookEvent::SettingsAfterSave,
        principal: opendash_core::hooks::ExecutionPrincipal::Admin,
        locale: None,
        type_key: None,
        payload: serde_json::json!({
            "scope": "plugin",
            "plugin_id": &id,
            "version": outcome.snapshot.version,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok((
        crate::routes::settings::no_store_headers(Some(outcome.snapshot.version)),
        Json(outcome.snapshot),
    )
        .into_response())
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
                return Err(
                    "Field type không được hỗ trợ hoặc bị cấm (không hỗ trợ secret/html/js/css).",
                );
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
    let Some((_, manifest_json, enabled)) =
        crate::storage::plugin::load_plugin_executable(&db, &id).await?
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
        .filter(|a| {
            matches!(actor.user.role, crate::auth::Role::Admin) || a.required_role == "editor"
        })
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

    #[tokio::test]
    async fn plugin_settings_http_routes_enforce_cas_no_store_redaction_and_secret_lifecycle() {
        use std::io::Write as _;

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!(
            "s02_http_plugin_settings_{}_{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&temp_root).unwrap();

        let paths = crate::paths::SitePaths::resolve_from(
            Some(temp_root.as_os_str().to_os_string()),
            None,
            &temp_root,
        );
        let coordinator =
            Arc::new(crate::operations::SiteCoordinator::acquire(paths.clone()).unwrap());
        let db = crate::storage::init_db(paths.db_path()).await.unwrap();
        let plugin_manager = Arc::new(PluginManager::with_roots(
            db.clone(),
            false,
            paths.plugins_dir(),
            paths.secrets_dir(),
        ));
        let hooks = Arc::new(HookDispatcher::new(plugin_manager.clone()));
        let content_registry = Arc::new(tokio::sync::RwLock::new(
            crate::storage::content::load_content_type_catalog(&db)
                .await
                .unwrap(),
        ));
        let runtime_config = crate::config::RuntimeConfigV1::load(&paths).unwrap();
        let state = crate::app::AppState {
            db: db.clone(),
            ui_locale: Arc::new(std::sync::atomic::AtomicU8::new(0)),
            plugin_manager: plugin_manager.clone(),
            hooks,
            media_store: Arc::new(crate::media::MediaStore::new(paths.uploads_dir())),
            content_registry,
            coordinator,
            backup_registry: Arc::new(crate::backup::BackupRuntimeRegistry::new()),
            block_template_cache: crate::page::BlockTemplateCache::global().clone(),
            site_cache_epoch: Arc::clone(crate::cache::SiteCacheEpoch::global()),
            render_cache: Arc::new(crate::cache::ExtensionRenderCache::new()),
            recovery: crate::config::RecoveryState::new(false),
            runtime_config,
        };

        let wasm_bytes = wat::parse_str(
            r#"(module
                (memory (export "memory") 1)
                (func (export "opendash_run") (result i32) i32.const 0)
            )"#,
        )
        .unwrap();
        let manifest_json = serde_json::json!({
            "id": "settings-e2e",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": opendash_core::plugin::CURRENT_HOST_VERSION,
            "name": "Settings E2E",
            "settings_schema": [
                {
                    "key": "api_key",
                    "label": "API Key",
                    "type": "secret",
                    "required": true
                },
                {
                    "key": "opt_secret",
                    "label": "Optional Secret",
                    "type": "secret",
                    "required": false
                },
                {
                    "key": "endpoint",
                    "label": "Endpoint",
                    "type": "url",
                    "required": true,
                    "default": "https://example.com"
                }
            ]
        })
        .to_string();

        let mut zip_buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut zip_buf);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("manifest.json", opts).unwrap();
            zip.write_all(manifest_json.as_bytes()).unwrap();
            zip.start_file("plugin.wasm", opts).unwrap();
            zip.write_all(&wasm_bytes).unwrap();
            zip.finish().unwrap();
        }
        plugin_manager
            .install_plugin_checked(&zip_buf.into_inner())
            .await
            .unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let expected_host = format!("127.0.0.1:{port}");
        let origin = format!("http://{expected_host}");
        let router = crate::app::build_router_with_themes_root(
            state,
            expected_host.clone(),
            paths.themes_dir(),
        );
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let client = reqwest::Client::new();
        let setup_resp = client
            .post(format!("{origin}/auth/setup"))
            .header("Origin", &origin)
            .json(&serde_json::json!({
                "username": "admin",
                "password": "AdminPassword123!"
            }))
            .send()
            .await
            .unwrap();
        assert!(setup_resp.status().is_success());

        let login_resp = client
            .post(format!("{origin}/auth/login"))
            .header("Origin", &origin)
            .json(&serde_json::json!({
                "username": "admin",
                "password": "AdminPassword123!"
            }))
            .send()
            .await
            .unwrap();
        assert!(login_resp.status().is_success());
        let cookie = login_resp
            .headers()
            .get(reqwest::header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let me: Value = client
            .get(format!("{origin}/auth/me"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let csrf = me["csrf_token"].as_str().unwrap().to_string();

        // 404 on invalid or missing plugin id.
        let not_found_resp = client
            .get(format!(
                "{origin}/api/admin/plugins/missing-plugin/settings"
            ))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(not_found_resp.status(), reqwest::StatusCode::NOT_FOUND);

        // Disable plugin and verify GET/PUT settings remain accessible while disabled.
        let disable_resp = client
            .post(format!("{origin}/api/admin/plugins/settings-e2e/disable"))
            .header("Origin", &origin)
            .header("Cookie", &cookie)
            .header("X-CSRF-Token", &csrf)
            .send()
            .await
            .unwrap();
        assert!(disable_resp.status().is_success());

        // Initial GET -> v1, no-store, ETag "v1", redacted unconfigured.
        let get1 = client
            .get(format!("{origin}/api/admin/plugins/settings-e2e/settings"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(get1.status(), reqwest::StatusCode::OK);
        assert_eq!(
            get1.headers().get(reqwest::header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert_eq!(get1.headers().get(reqwest::header::ETAG).unwrap(), "\"v1\"");
        let snap1: Value = get1.json().await.unwrap();
        assert_eq!(snap1["version"], 1);
        assert_eq!(
            snap1["values"]["api_key"],
            serde_json::json!({ "configured": false })
        );

        // Missing If-Match -> 428.
        let r428 = client
            .put(format!("{origin}/api/admin/plugins/settings-e2e/settings"))
            .header("Origin", &origin)
            .header("Cookie", &cookie)
            .header("X-CSRF-Token", &csrf)
            .json(&serde_json::json!({
                "values": {
                    "api_key": "http_secret_v1",
                    "endpoint": "https://one.example.com"
                }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r428.status(), reqwest::StatusCode::PRECONDITION_REQUIRED);

        let sec_dir = paths.plugin_secrets_dir("settings-e2e");
        let file_count = |dir: &std::path::Path| -> usize {
            if !dir.exists() {
                0
            } else {
                std::fs::read_dir(dir)
                    .unwrap()
                    .filter_map(Result::ok)
                    .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                    .count()
            }
        };

        // Stale If-Match -> 412 with 0 orphan secret files.
        let r412 = client
            .put(format!("{origin}/api/admin/plugins/settings-e2e/settings"))
            .header("Origin", &origin)
            .header("Cookie", &cookie)
            .header("X-CSRF-Token", &csrf)
            .header("If-Match", "\"v99\"")
            .json(&serde_json::json!({
                "values": {
                    "api_key": "http_secret_orphan",
                    "endpoint": "https://one.example.com"
                }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(r412.status(), reqwest::StatusCode::PRECONDITION_FAILED);
        assert_eq!(file_count(&sec_dir), 0);

        // Save v1 -> v2 with two secrets; verify redaction & SecretStore readback.
        let put1 = client
            .put(format!("{origin}/api/admin/plugins/settings-e2e/settings"))
            .header("Origin", &origin)
            .header("Cookie", &cookie)
            .header("X-CSRF-Token", &csrf)
            .header("If-Match", "\"v1\"")
            .json(&serde_json::json!({
                "values": {
                    "api_key": "http_secret_v1",
                    "opt_secret": "opt_v1",
                    "endpoint": "https://one.example.com"
                }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(put1.status(), reqwest::StatusCode::OK);
        assert_eq!(put1.headers().get(reqwest::header::ETAG).unwrap(), "\"v2\"");
        let put1_text = put1.text().await.unwrap();
        assert!(!put1_text.contains("http_secret_v1"));
        assert!(!put1_text.contains("secret_ref"));
        let put1_json: Value = serde_json::from_str(&put1_text).unwrap();
        assert_eq!(
            put1_json["values"]["api_key"],
            serde_json::json!({ "configured": true })
        );
        assert_eq!(
            put1_json["values"]["opt_secret"],
            serde_json::json!({ "configured": true })
        );
        assert_eq!(file_count(&sec_dir), 2);

        // Clear opt_secret & null-preserve api_key across endpoint change -> v3.
        let put2 = client
            .put(format!("{origin}/api/admin/plugins/settings-e2e/settings"))
            .header("Origin", &origin)
            .header("Cookie", &cookie)
            .header("X-CSRF-Token", &csrf)
            .header("If-Match", "\"v2\"")
            .json(&serde_json::json!({
                "values": {
                    "api_key": null,
                    "opt_secret": { "clear": true },
                    "endpoint": "https://two.example.com"
                }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(put2.status(), reqwest::StatusCode::OK);
        assert_eq!(put2.headers().get(reqwest::header::ETAG).unwrap(), "\"v3\"");
        let put2_json: Value = put2.json().await.unwrap();
        assert_eq!(
            put2_json["values"]["api_key"],
            serde_json::json!({ "configured": true })
        );
        assert_eq!(
            put2_json["values"]["opt_secret"],
            serde_json::json!({ "configured": false })
        );
        assert_eq!(put2_json["values"]["endpoint"], "https://two.example.com");
        assert_eq!(file_count(&sec_dir), 1);

        let raw_opts = crate::storage::plugin::read_raw_plugin_option_map(&db, "settings-e2e")
            .await
            .unwrap();
        let stored_ref = raw_opts["api_key"]["secret_ref"].as_str().unwrap();
        assert_eq!(
            crate::secrets::SecretStore::for_plugin_dir(sec_dir.clone())
                .read_plugin_secret(stored_ref)
                .unwrap(),
            "http_secret_v1"
        );

        server_task.abort();
        drop(db);
        let _ = std::fs::remove_dir_all(&temp_root);
    }
}
