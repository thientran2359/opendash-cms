use super::*;

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

pub(super) fn validate_ui_node(
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
