use super::*;

pub(super) async fn maybe_spawn_projection_recompute(
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
    let _ = crate::l10n::CatalogRegistry::global()
        .reload_site_with_active_packages(&db, coordinator.paths())
        .await;
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
    State(coordinator): State<Arc<crate::operations::SiteCoordinator>>,
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
    let _ = crate::l10n::CatalogRegistry::global()
        .reload_site_with_active_packages(&db, coordinator.paths())
        .await;
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
    let _ = crate::l10n::CatalogRegistry::global()
        .reload_site_with_active_packages(&db, coordinator.paths())
        .await;
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

pub(super) fn parse_optional_if_match(headers: &HeaderMap) -> ApiResult<Option<i64>> {
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

pub(super) fn contains_disallowed_markup(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains('<') || lower.contains('>') || lower.contains("javascript:")
}
