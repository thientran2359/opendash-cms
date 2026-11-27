use super::*;

pub async fn theme_functions_get_settings(
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<crate::plugin::PluginManager>>,
    request_headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&request_headers, &db).await?;
    if let Some((theme_id, revision_id, _sel_ver, manifest)) = plugins.active_theme_manifest().await
    {
        let snapshot = crate::storage::settings::get_theme_functions_settings_redacted(
            &db,
            &theme_id,
            &manifest.settings_schema,
        )
        .await?;
        let resp = ThemeFunctionsSettingsResponse {
            theme_id: Some(theme_id),
            revision_id: Some(revision_id),
            has_functions: true,
            version: snapshot.version,
            fields: snapshot.fields,
            values: snapshot.values,
            actions: manifest.actions,
        };
        return Ok((
            crate::routes::settings::no_store_headers(Some(resp.version)),
            Json(resp),
        )
            .into_response());
    }
    let selection = theme::get_active_selection(&db).await?;
    let resp = ThemeFunctionsSettingsResponse {
        theme_id: selection.theme_id,
        revision_id: selection.revision_id,
        has_functions: false,
        version: 0,
        fields: Vec::new(),
        values: serde_json::Map::new(),
        actions: Vec::new(),
    };
    Ok((crate::routes::settings::no_store_headers(None), Json(resp)).into_response())
}

#[allow(clippy::too_many_arguments)]
pub async fn theme_functions_put_settings(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<crate::plugin::PluginManager>>,
    State(hooks): State<Arc<crate::hooks::HookDispatcher>>,
    State(coordinator): State<Arc<crate::operations::SiteCoordinator>>,
    Extension(recovery): Extension<crate::config::RecoveryState>,
    request_headers: HeaderMap,
    input: Result<Json<crate::routes::plugin::PluginSettingsPutInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&request_headers, &db).await?;
    let expected_version = crate::routes::settings::parse_if_match(&request_headers)?;
    let Json(input) = input.map_err(|err| {
        if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Dữ liệu cài đặt Theme Functions vượt quá 1 MiB.".into(),
            )
        } else {
            ApiError::bad_request("Dữ liệu cài đặt Theme Functions không hợp lệ.")
        }
    })?;
    let Some((theme_id, revision_id, sel_ver, manifest)) = plugins.active_theme_manifest().await
    else {
        return Err(ApiError::not_found(
            "Theme hiện hành không có functions.js.",
        ));
    };
    let has_projection_filters = manifest.filters.iter().any(|f| {
        matches!(
            f,
            opendash_core::hooks::HookEvent::FilterSeoHead
                | opendash_core::hooks::HookEvent::FilterEntryView
        )
    });
    let outcome = crate::storage::settings::put_theme_functions_settings(
        &db,
        plugins.secrets_root(),
        &theme_id,
        expected_version,
        &manifest.settings_schema,
        &input.values,
        has_projection_filters,
    )
    .await?;

    if !recovery.is_safe_mode() {
        if outcome.changed && outcome.has_projection_filters {
            let _handle =
                crate::storage::plugin_projections::spawn_recompute_owner_projections_batched(
                    db.clone(),
                    Arc::clone(&hooks),
                    Arc::clone(&coordinator),
                    opendash_core::hooks::ExtensionOwnerKind::Theme,
                    theme_id.clone(),
                    Some((revision_id.clone(), sel_ver)),
                    coordinator.maintenance_cancel_rx(),
                    coordinator.shutdown_rx(),
                )
                .await;
        }
        hooks.enqueue_async(crate::hooks::AsyncHookJob {
            event: opendash_core::hooks::HookEvent::SettingsAfterSave,
            principal: opendash_core::hooks::ExecutionPrincipal::Admin,
            locale: None,
            type_key: None,
            payload: serde_json::json!({
                "scope": "theme_functions",
                "theme_id": &theme_id,
                "version": outcome.snapshot.version,
            }),
            dispatch_depth: 0,
            lease,
        });
    }

    let resp = ThemeFunctionsSettingsResponse {
        theme_id: Some(theme_id),
        revision_id: Some(revision_id),
        has_functions: true,
        version: outcome.snapshot.version,
        fields: outcome.snapshot.fields,
        values: outcome.snapshot.values,
        actions: manifest.actions,
    };
    Ok((
        crate::routes::settings::no_store_headers(Some(resp.version)),
        Json(resp),
    )
        .into_response())
}

#[allow(clippy::too_many_arguments)]
pub async fn theme_active_action(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<crate::plugin::PluginManager>>,
    State(content_registry): State<
        Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
    >,
    State(hooks): State<Arc<crate::hooks::HookDispatcher>>,
    request_headers: HeaderMap,
    Path(action_id): Path<String>,
    bytes: Bytes,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&request_headers, &db).await?;
    let input_val: serde_json::Value = if bytes.is_empty() {
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
        "tact-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let maint_epoch = lease.maintenance_epoch();
    match plugins
        .execute_owner_action(
            opendash_core::hooks::ExtensionOwnerKind::Theme,
            None,
            &action_id,
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
                Role::Admin => opendash_core::hooks::ExecutionPrincipal::Admin,
                Role::Editor => opendash_core::hooks::ExecutionPrincipal::Editor,
            };
            hooks.enqueue_plugin_action_hooks(principal, outcome.committed.as_ref(), lease);
            Ok((StatusCode::OK, Json(outcome.response)).into_response())
        }
        Err((status, resp)) => Ok((status, Json(resp)).into_response()),
    }
}

pub async fn theme_functions_logs(
    State(db): State<SqlitePool>,
    request_headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<ThemeFunctionsLogsQuery>,
) -> ApiResult<Response> {
    require_admin(&request_headers, &db).await?;
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("Tham số limit phải từ 1 đến 100."));
    }
    let active_theme_id = crate::dev_mode::DevOverlayConfig::global()
        .and_then(|d| d.dev_theme_id().map(str::to_owned))
        .or(theme::get_active_selection(&db).await?.theme_id);

    let items = match active_theme_id.as_deref() {
        Some(theme_id) => {
            crate::storage::plugin::list_owner_logs(
                &db,
                opendash_core::hooks::ExtensionOwnerKind::Theme,
                theme_id,
                limit,
            )
            .await?
        }
        None => Vec::new(),
    };
    Ok((
        crate::routes::settings::no_store_headers(None),
        Json(serde_json::json!({
            "theme_id": active_theme_id,
            "items": items,
        })),
    )
        .into_response())
}

pub async fn dev_sync(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(plugins): State<Arc<crate::plugin::PluginManager>>,
    State(content_registry): State<
        Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
    >,
    Extension(recovery): Extension<crate::config::RecoveryState>,
    request_headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin_with_csrf(&request_headers, &db).await?;
    if recovery.is_safe_mode() {
        return Err(ApiError::service_unavailable(
            "Hệ thống đang ở safe mode; không thể đồng bộ Dev Manifest.",
        ));
    }
    let Some(dev) = crate::dev_mode::DevOverlayConfig::global() else {
        return Err(ApiError::not_found("Dev Mode hiện không được bật."));
    };

    let mut synced_theme: Option<String> = None;
    if let Some(theme_snap) = dev.load_dev_theme().map_err(ApiError::bad_request)? {
        theme::sync_dev_theme_in_db(&db, &theme_snap).await?;
        synced_theme = Some(theme_snap.manifest.theme_id);
    }

    let mut synced_plugins: Vec<String> = Vec::new();
    let plugin_snaps = dev
        .load_all_dev_plugins(plugins.engine())
        .map_err(ApiError::bad_request)?;
    for snap in plugin_snaps {
        crate::storage::plugin::sync_dev_plugin_in_db(&db, &snap).await?;
        synced_plugins.push(snap.manifest.id);
    }

    *content_registry.write().await =
        crate::storage::content::load_content_type_catalog(&db).await?;
    plugins.invalidate_active_theme_cache().await;
    plugins.invalidate_cache().await;
    let _ = crate::l10n::CatalogRegistry::global()
        .reload_site_with_active_packages(
            &db,
            &crate::paths::SitePaths::from_env().unwrap_or_else(|_| {
                crate::paths::SitePaths::resolve_from(None, None, std::path::Path::new("."))
            }),
        )
        .await;
    crate::page::BlockTemplateCache::global().clear();
    crate::cache::SiteCacheEpoch::global().bump();

    Ok((
        crate::routes::settings::no_store_headers(None),
        Json(serde_json::json!({
            "synced_theme": synced_theme,
            "synced_plugins": synced_plugins,
        })),
    )
        .into_response())
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let mut out = String::with_capacity(bytes.as_ref().len() * 2);
        for b in bytes.as_ref() {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }
}
