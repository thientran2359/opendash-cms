use super::*;

impl PluginManager {
    #[allow(dead_code)]
    pub async fn filter_public_content_payload_for_plugin(
        &self,
        plugin_id: &str,
        type_key: &str,
        payload: &mut Value,
    ) {
        self.filter_public_content_payload_for_owner(
            &RuntimeOwner::plugin(plugin_id),
            type_key,
            payload,
        )
        .await;
    }
}

impl PluginManager {
    pub async fn filter_public_content_payload_for_owner(
        &self,
        owner: &RuntimeOwner,
        type_key: &str,
        payload: &mut Value,
    ) {
        let manifest_opt = self.cached_owner_manifest(owner).await;
        let schema_opt =
            crate::storage::content::load_effective_content_schema(&self.db_pool, type_key)
                .await
                .ok()
                .flatten();

        for field_map_key in ["candidate_values", "values"] {
            if let Some(Value::Object(map)) = payload.get_mut(field_map_key) {
                match (&manifest_opt, &schema_opt) {
                    (Some(manifest), Some(effective)) => {
                        map.retain(|k, _| {
                            effective.effective_fields.get(k).is_some_and(|fe| {
                                fe.definition.public_exposure
                                    == opendash_core::content::PublicExposure::Public
                                    && opendash_core::plugin::plugin_can_read_field_for_owner(
                                        owner.kind,
                                        manifest,
                                        None,
                                        opendash_core::content::FieldRole::Editor,
                                        type_key,
                                        fe,
                                    )
                            })
                        });
                    }
                    _ => {
                        map.clear();
                    }
                }
            }
        }
    }
}

impl PluginManager {
    #[allow(dead_code)]
    pub async fn hook_subscriptions(&self, event: HookEvent) -> Result<Vec<String>, String> {
        let cache = self.ensure_manifest_cache().await?;
        Ok(cache
            .into_iter()
            .filter_map(|(id, item)| item.manifest.hooks.contains(&event).then_some(id))
            .collect())
    }
}

impl PluginManager {
    pub async fn owner_hook_subscriptions(
        &self,
        event: HookEvent,
    ) -> Result<Vec<ExtensionSubscriber>, String> {
        let mut subs = Vec::new();
        let cache = self.ensure_manifest_cache().await?;
        for (id, item) in cache {
            if item.manifest.hooks.contains(&event) {
                subs.push(ExtensionSubscriber {
                    priority: item.manifest.hook_priority,
                    owner: RuntimeOwner::plugin(id),
                    theme_fence: None,
                });
            }
        }
        if let Some(theme) = self.ensure_active_theme_cache().await?
            && theme.manifest.hooks.contains(&event)
        {
            subs.push(ExtensionSubscriber {
                priority: theme.manifest.hook_priority,
                owner: RuntimeOwner::theme(theme.theme_id),
                theme_fence: Some((theme.revision_id, theme.selection_version)),
            });
        }
        subs.sort_by(|a, b| (a.priority, &a.owner).cmp(&(b.priority, &b.owner)));
        Ok(subs)
    }
}

impl PluginManager {
    #[allow(dead_code)]
    pub async fn content_hook_subscriptions(
        &self,
        event: HookEvent,
        type_key: &str,
    ) -> Result<Vec<(i32, String)>, String> {
        let cache = self.ensure_manifest_cache().await?;
        let mut subs: Vec<(i32, String)> = cache
            .into_iter()
            .filter_map(|(id, item)| {
                (item.manifest.hooks.contains(&event)
                    && item.manifest.can_read_content_type(type_key))
                .then_some((item.manifest.hook_priority, id))
            })
            .collect();
        subs.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        Ok(subs)
    }
}

impl PluginManager {
    pub async fn owner_content_hook_subscriptions(
        &self,
        event: HookEvent,
        type_key: &str,
    ) -> Result<Vec<ExtensionSubscriber>, String> {
        let mut subs = Vec::new();
        let cache = self.ensure_manifest_cache().await?;
        for (id, item) in cache {
            if item.manifest.hooks.contains(&event) && item.manifest.can_read_content_type(type_key)
            {
                subs.push(ExtensionSubscriber {
                    priority: item.manifest.hook_priority,
                    owner: RuntimeOwner::plugin(id),
                    theme_fence: None,
                });
            }
        }
        if let Some(theme) = self.ensure_active_theme_cache().await?
            && theme.manifest.hooks.contains(&event)
            && theme.manifest.can_read_content_type(type_key)
        {
            subs.push(ExtensionSubscriber {
                priority: theme.manifest.hook_priority,
                owner: RuntimeOwner::theme(theme.theme_id),
                theme_fence: Some((theme.revision_id, theme.selection_version)),
            });
        }
        subs.sort_by(|a, b| (a.priority, &a.owner).cmp(&(b.priority, &b.owner)));
        Ok(subs)
    }
}

impl PluginManager {
    #[allow(dead_code)]
    pub async fn filter_subscriptions(
        &self,
        event: HookEvent,
        type_key: Option<&str>,
    ) -> Result<Vec<(i32, String)>, String> {
        let cache = self.ensure_manifest_cache().await?;
        let mut subs: Vec<(i32, String)> = cache
            .into_iter()
            .filter_map(|(id, item)| {
                let matches_type =
                    type_key.is_none_or(|tk| item.manifest.can_read_content_type(tk));
                (item.manifest.filters.contains(&event) && matches_type)
                    .then_some((item.manifest.hook_priority, id))
            })
            .collect();
        subs.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        Ok(subs)
    }
}

impl PluginManager {
    pub async fn owner_filter_subscriptions(
        &self,
        event: HookEvent,
        type_key: Option<&str>,
    ) -> Result<Vec<ExtensionSubscriber>, String> {
        let mut subs = Vec::new();
        let cache = self.ensure_manifest_cache().await?;
        for (id, item) in cache {
            let matches_type = type_key.is_none_or(|tk| item.manifest.can_read_content_type(tk));
            if item.manifest.filters.contains(&event) && matches_type {
                subs.push(ExtensionSubscriber {
                    priority: item.manifest.hook_priority,
                    owner: RuntimeOwner::plugin(id),
                    theme_fence: None,
                });
            }
        }
        if let Some(theme) = self.ensure_active_theme_cache().await? {
            let matches_type = type_key.is_none_or(|tk| theme.manifest.can_read_content_type(tk));
            if theme.manifest.filters.contains(&event) && matches_type {
                subs.push(ExtensionSubscriber {
                    priority: theme.manifest.hook_priority,
                    owner: RuntimeOwner::theme(theme.theme_id),
                    theme_fence: Some((theme.revision_id, theme.selection_version)),
                });
            }
        }
        subs.sort_by(|a, b| (a.priority, &a.owner).cmp(&(b.priority, &b.owner)));
        Ok(subs)
    }
}

impl PluginManager {
    pub fn is_safe_mode(&self) -> bool {
        self.recovery.is_safe_mode()
    }
}

impl PluginManager {
    pub async fn owner_render_filter_subscriptions(
        &self,
        event: HookEvent,
    ) -> Result<Vec<RenderFilterSubscriber>, String> {
        let mut subs = Vec::new();
        let cache = self.ensure_manifest_cache().await?;
        for (id, item) in cache {
            if item.manifest.filters.contains(&event) {
                let revision_key = item
                    .revision_digest
                    .unwrap_or_else(|| format!("gen:{}", item.revision_generation));
                subs.push(RenderFilterSubscriber {
                    priority: item.manifest.hook_priority,
                    owner: RuntimeOwner::plugin(id),
                    revision_key,
                    render_cache: item.manifest.render_cache,
                    render_trusted_html: item.manifest.permissions.render_trusted_html,
                });
            }
        }
        if let Some(theme) = self.ensure_active_theme_cache().await?
            && theme.manifest.filters.contains(&event)
        {
            subs.push(RenderFilterSubscriber {
                priority: theme.manifest.hook_priority,
                owner: RuntimeOwner::theme(theme.theme_id),
                revision_key: format!("{}:v{}", theme.revision_digest, theme.selection_version),
                render_cache: theme.manifest.render_cache,
                render_trusted_html: theme.manifest.permissions.render_trusted_html,
            });
        }
        subs.sort_by(|a, b| (a.priority, &a.owner).cmp(&(b.priority, &b.owner)));
        Ok(subs)
    }
}

impl PluginManager {
    #[allow(dead_code)]
    pub async fn filter_content_payload_for_plugin(
        &self,
        plugin_id: &str,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: &mut Value,
    ) {
        self.filter_content_payload_for_owner(
            &RuntimeOwner::plugin(plugin_id),
            type_key,
            actor_role,
            payload,
        )
        .await;
    }
}

impl PluginManager {
    pub async fn filter_content_payload_for_owner(
        &self,
        owner: &RuntimeOwner,
        type_key: &str,
        actor_role: opendash_core::content::FieldRole,
        payload: &mut Value,
    ) {
        let manifest_opt = self.cached_owner_manifest(owner).await;
        let schema_opt =
            crate::storage::content::load_effective_content_schema(&self.db_pool, type_key)
                .await
                .ok()
                .flatten();

        for field_map_key in ["candidate_values", "values"] {
            if let Some(Value::Object(map)) = payload.get_mut(field_map_key) {
                match (&manifest_opt, &schema_opt) {
                    (Some(manifest), Some(effective)) => {
                        map.retain(|k, _| {
                            effective.effective_fields.get(k).is_some_and(|fe| {
                                opendash_core::plugin::plugin_can_read_field_for_owner(
                                    owner.kind, manifest, None, actor_role, type_key, fe,
                                )
                            })
                        });
                    }
                    _ => {
                        map.clear();
                    }
                }
            }
        }
    }
}

impl PluginManager {
    pub(super) async fn filter_action_input_for_owner(
        &self,
        owner_kind: ExtensionOwnerKind,
        manifest: &PluginManifest,
        action_caps: &[String],
        actor_role: opendash_core::content::FieldRole,
        input: &mut Value,
    ) {
        let Some(type_key) = input
            .get("type_key")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            return;
        };
        let schema_opt =
            crate::storage::content::load_effective_content_schema(&self.db_pool, &type_key)
                .await
                .ok()
                .flatten();
        if let Some(Value::Object(map)) = input.get_mut("values") {
            match &schema_opt {
                Some(effective) => {
                    map.retain(|k, _| {
                        effective.effective_fields.get(k).is_some_and(|fe| {
                            opendash_core::plugin::plugin_can_read_field_for_owner(
                                owner_kind,
                                manifest,
                                Some(action_caps),
                                actor_role,
                                &type_key,
                                fe,
                            )
                        })
                    });
                }
                None => {
                    map.clear();
                }
            }
        }
    }
}

impl PluginManager {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn run_theme_with_principal(
        &self,
        expected_theme_id: &str,
        v2_input: Value,
        correlation_id: String,
        execution_mode: HookExecutionMode,
        principal: ExecutionPrincipal,
        timeout_override: Option<std::time::Duration>,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        if self.recovery.is_safe_mode() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(
                        "Hệ thống đang ở safe mode sau khi khôi phục; theme functions bị tạm khóa."
                            .into(),
                    ),
                    code: Some("safe_mode".into()),
                    correlation_id,
                    confirmation: None,
                },
            ));
        }
        let cached = match self.ensure_active_theme_cache().await {
            Ok(Some(c)) if c.theme_id == expected_theme_id => c,
            Ok(_) => {
                return Err((
                    StatusCode::NOT_FOUND,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some("Theme functions không tồn tại hoặc đã đổi theme.".into()),
                        code: Some("not_found".into()),
                        correlation_id,
                        confirmation: None,
                    },
                ));
            }
            Err(err) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some(err),
                        code: Some("theme_cache_error".into()),
                        correlation_id,
                        confirmation: None,
                    },
                ));
            }
        };

        let fence = OwnerFence::Theme {
            theme_id: cached.theme_id.clone(),
            revision_id: cached.revision_id.clone(),
            selection_version: cached.selection_version,
        };
        let run_res = self
            .runtime
            .run_owner_action_with_principal(
                ExtensionOwnerKind::Theme,
                ExtensionRuntimeKind::Js,
                cached.js_bytes,
                cached.manifest,
                v2_input,
                self.db_pool.clone(),
                correlation_id.clone(),
                self.allow_loopback,
                Some(fence),
                execution_mode,
                principal,
                timeout_override,
                None,
                None,
            )
            .await
            .map(|r| r.output);

        match run_res {
            Ok(val) => {
                if !matches!(
                    execution_mode,
                    HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
                ) {
                    plugin_storage::record_owner_execution_result(
                        &self.db_pool,
                        ExtensionOwnerKind::Theme,
                        &cached.theme_id,
                        &correlation_id,
                        "info",
                        "Thực thi thành công",
                        None,
                    )
                    .await;
                }
                Ok(PluginRunResponse {
                    success: true,
                    data: Some(val),
                    error: None,
                    code: None,
                    correlation_id,
                    confirmation: None,
                })
            }
            Err(err) => {
                let (code_str, err_msg) = match &err {
                    PluginError::Disabled => ("disabled", err.to_string()),
                    PluginError::NotFound => ("not_found", err.to_string()),
                    PluginError::OutOfFuel => ("out_of_fuel", err.to_string()),
                    PluginError::Timeout => ("timeout", err.to_string()),
                    PluginError::BadPointer => ("bad_pointer", err.to_string()),
                    PluginError::GuestFailed(_) => ("guest_failed", err.to_string()),
                    PluginError::Trap(_) => ("trap", err.to_string()),
                    PluginError::InvalidOutput(_) => ("invalid_output", err.to_string()),
                    PluginError::ConcurrencyLimit => ("concurrency_limit", err.to_string()),
                    PluginError::ModuleCompilation(_) => ("module_compilation", err.to_string()),
                    PluginError::Internal(_) => ("internal_error", err.to_string()),
                };
                if !matches!(
                    execution_mode,
                    HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
                ) {
                    plugin_storage::record_owner_execution_result(
                        &self.db_pool,
                        ExtensionOwnerKind::Theme,
                        &cached.theme_id,
                        &correlation_id,
                        "error",
                        &err_msg,
                        Some(&err_msg),
                    )
                    .await;
                }
                Err((
                    StatusCode::BAD_REQUEST,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some(err_msg),
                        code: Some(code_str.to_string()),
                        correlation_id,
                        confirmation: None,
                    },
                ))
            }
        }
    }
}

impl PluginManager {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn run_plugin_with_principal(
        &self,
        id: &str,
        v2_input: Option<Value>,
        legacy_input: Value,
        correlation_id: String,
        execution_mode: HookExecutionMode,
        principal: ExecutionPrincipal,
        timeout_override: Option<std::time::Duration>,
    ) -> Result<PluginRunResponse, (StatusCode, PluginRunResponse)> {
        if self.recovery.is_safe_mode() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(
                        "Hệ thống đang ở safe mode sau khi khôi phục; plugin bị tạm khóa.".into(),
                    ),
                    code: Some("safe_mode".into()),
                    correlation_id,
                    confirmation: None,
                },
            ));
        }

        let (wasm_blob, manifest, revision_generation) = if let Some(dev) =
            crate::dev_mode::DevOverlayConfig::global()
            && dev.has_dev_plugin(id)
        {
            let snap = dev
                .load_dev_plugin_by_id(id, self.runtime.engine())
                .map_err(|err| {
                    (
                        StatusCode::CONFLICT,
                        PluginRunResponse {
                            success: false,
                            data: None,
                            error: Some(err),
                            code: Some("dev_plugin_invalid".into()),
                            correlation_id: correlation_id.clone(),
                            confirmation: None,
                        },
                    )
                })?
                .ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        PluginRunResponse {
                            success: false,
                            data: None,
                            error: Some("Plugin không tồn tại.".into()),
                            code: Some("not_found".into()),
                            correlation_id: correlation_id.clone(),
                            confirmation: None,
                        },
                    )
                })?;
            let db_gen = plugin_storage::load_plugin_executable_with_generation(&self.db_pool, id)
                .await
                .ok()
                .flatten()
                .and_then(|(_, _, enabled, gen_val, _)| enabled.then_some(gen_val))
                .unwrap_or(0);
            (snap.code_bytes, snap.manifest, db_gen)
        } else {
            let row_opt = plugin_storage::load_plugin_executable_with_generation(&self.db_pool, id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        PluginRunResponse {
                            success: false,
                            data: None,
                            error: Some(format!("Lỗi cơ sở dữ liệu: {e}")),
                            code: Some("db_error".into()),
                            correlation_id: correlation_id.clone(),
                            confirmation: None,
                        },
                    )
                })?;
            let Some((wasm_blob, manifest_json, enabled, revision_generation, revision_digest_opt)) =
                row_opt
            else {
                return Err((
                    StatusCode::NOT_FOUND,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some("Plugin không tồn tại.".into()),
                        code: Some("not_found".into()),
                        correlation_id,
                        confirmation: None,
                    },
                ));
            };

            if !enabled {
                return Err((
                    StatusCode::BAD_REQUEST,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some("Plugin chưa được bật hoặc đã bị vô hiệu hóa.".into()),
                        code: Some("disabled".into()),
                        correlation_id,
                        confirmation: None,
                    },
                ));
            }
            if wasm_blob.is_empty() && revision_digest_opt.is_none() {
                return Err((
                    StatusCode::NOT_FOUND,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some(
                            "Plugin chỉ có ở chế độ dev; hãy khởi chạy kèm --dev-plugin hoặc cài bản ZIP chính thức."
                                .into(),
                        ),
                        code: Some("dev_plugin_unmounted".into()),
                        correlation_id,
                        confirmation: None,
                    },
                ));
            }

            let manifest = serde_json::from_str::<PluginManifest>(&manifest_json).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Manifest trong database không hợp lệ: {e}")),
                        code: Some("corrupt_manifest".into()),
                        correlation_id: correlation_id.clone(),
                        confirmation: None,
                    },
                )
            })?;

            if let Some(digest) = &revision_digest_opt {
                if let Err(err) =
                    plugin_storage::verify_plugin_revision_durability(&self.plugins_root, digest)
                {
                    return Err((
                        StatusCode::CONFLICT,
                        PluginRunResponse {
                            success: false,
                            data: None,
                            error: Some(err),
                            code: Some("corrupt_revision".into()),
                            correlation_id,
                            confirmation: None,
                        },
                    ));
                }
            }
            (wasm_blob, manifest, revision_generation)
        };

        let input = if manifest.manifest_schema >= 3 {
            v2_input.unwrap_or(legacy_input)
        } else {
            legacy_input
        };

        let run_res = self
            .runtime
            .run_plugin_action_with_principal(
                wasm_blob,
                manifest,
                input,
                self.db_pool.clone(),
                correlation_id.clone(),
                self.allow_loopback,
                Some(revision_generation),
                execution_mode,
                principal,
                timeout_override,
                None,
                None,
            )
            .await
            .map(|r| r.output);

        match run_res {
            Ok(val) => {
                if !matches!(
                    execution_mode,
                    HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
                ) {
                    plugin_storage::record_plugin_execution_result(
                        &self.db_pool,
                        id,
                        &correlation_id,
                        "info",
                        "Thực thi thành công",
                        None,
                    )
                    .await;
                }

                Ok(PluginRunResponse {
                    success: true,
                    data: Some(val),
                    error: None,
                    code: None,
                    correlation_id,
                    confirmation: None,
                })
            }
            Err(err) => {
                let (code_str, err_msg) = match &err {
                    PluginError::Disabled => ("disabled", err.to_string()),
                    PluginError::NotFound => ("not_found", err.to_string()),
                    PluginError::OutOfFuel => ("out_of_fuel", err.to_string()),
                    PluginError::Timeout => ("timeout", err.to_string()),
                    PluginError::BadPointer => ("bad_pointer", err.to_string()),
                    PluginError::GuestFailed(_) => ("guest_failed", err.to_string()),
                    PluginError::Trap(_) => ("trap", err.to_string()),
                    PluginError::InvalidOutput(_) => ("invalid_output", err.to_string()),
                    PluginError::ConcurrencyLimit => ("concurrency_limit", err.to_string()),
                    PluginError::ModuleCompilation(_) => ("module_compilation", err.to_string()),
                    PluginError::Internal(_) => ("internal_error", err.to_string()),
                };

                if !matches!(
                    execution_mode,
                    HookExecutionMode::FilterRender | HookExecutionMode::RouteGet
                ) {
                    plugin_storage::record_plugin_execution_result(
                        &self.db_pool,
                        id,
                        &correlation_id,
                        "error",
                        &err_msg,
                        Some(&err_msg),
                    )
                    .await;
                }

                Err((
                    StatusCode::BAD_REQUEST,
                    PluginRunResponse {
                        success: false,
                        data: None,
                        error: Some(err_msg),
                        code: Some(code_str.to_string()),
                        correlation_id,
                        confirmation: None,
                    },
                ))
            }
        }
    }
}

impl PluginManager {
    pub async fn resolve_extension_route(
        &self,
        pkg: &str,
        method: opendash_core::plugin::RouteMethod,
        subpath: &str,
    ) -> Result<Option<ResolvedExtensionRoute>, (StatusCode, String)> {
        if self.recovery.is_safe_mode() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "Hệ thống đang ở safe mode sau khi khôi phục; route mở rộng tạm khóa.".into(),
            ));
        }
        if opendash_core::id::validate_package_id(pkg).is_err() {
            return Ok(None);
        }

        // 1. Check mounted dev plugin first, then enabled plugin in DB
        if let Some(dev) = crate::dev_mode::DevOverlayConfig::global()
            && dev.has_dev_plugin(pkg)
        {
            if let Ok(Some(snap)) = dev.load_dev_plugin_by_id(pkg, self.runtime.engine()) {
                let db_gen =
                    plugin_storage::load_plugin_executable_with_generation(&self.db_pool, pkg)
                        .await
                        .ok()
                        .flatten()
                        .and_then(|(_, _, enabled, gen_val, _)| enabled.then_some(gen_val))
                        .unwrap_or(0);
                for route in &snap.manifest.routes {
                    if route.method == method
                        && let Some(params) =
                            opendash_core::plugin::match_route_path(&route.path, subpath)
                    {
                        let runtime_kind =
                            ExtensionRuntimeKind::from_entrypoint(&snap.manifest.entrypoint);
                        return Ok(Some(ResolvedExtensionRoute {
                            owner: RuntimeOwner::plugin(snap.manifest.id.clone()),
                            route: route.clone(),
                            params,
                            code_bytes: snap.code_bytes,
                            runtime_kind,
                            owner_fence: OwnerFence::Plugin {
                                plugin_id: snap.manifest.id.clone(),
                                expected_generation: db_gen,
                            },
                            manifest: snap.manifest,
                        }));
                    }
                }
            }
        } else if let Ok(Some((
            wasm_blob,
            manifest_json,
            enabled,
            revision_generation,
            revision_digest,
        ))) =
            plugin_storage::load_plugin_executable_with_generation(&self.db_pool, pkg).await
            && enabled
            && !(wasm_blob.is_empty() && revision_digest.is_none())
            && let Ok(manifest) = serde_json::from_str::<PluginManifest>(&manifest_json)
        {
            for route in &manifest.routes {
                if route.method == method
                    && let Some(params) =
                        opendash_core::plugin::match_route_path(&route.path, subpath)
                {
                    if let Some(digest) = &revision_digest
                        && let Err(err) = plugin_storage::verify_plugin_revision_durability(
                            &self.plugins_root,
                            digest,
                        )
                    {
                        return Err((StatusCode::CONFLICT, err));
                    }
                    let runtime_kind = ExtensionRuntimeKind::from_entrypoint(&manifest.entrypoint);
                    return Ok(Some(ResolvedExtensionRoute {
                        owner: RuntimeOwner::plugin(manifest.id.clone()),
                        route: route.clone(),
                        params,
                        code_bytes: wasm_blob,
                        runtime_kind,
                        owner_fence: OwnerFence::Plugin {
                            plugin_id: manifest.id.clone(),
                            expected_generation: revision_generation,
                        },
                        manifest,
                    }));
                }
            }
        }

        // 2. Check active theme functions with matching theme ID and route
        if let Ok(Some(cached)) = self.ensure_active_theme_cache().await
            && cached.theme_id == pkg
        {
            for route in &cached.manifest.routes {
                if route.method == method
                    && let Some(params) =
                        opendash_core::plugin::match_route_path(&route.path, subpath)
                {
                    return Ok(Some(ResolvedExtensionRoute {
                        owner: RuntimeOwner::theme(cached.theme_id.clone()),
                        route: route.clone(),
                        params,
                        code_bytes: cached.js_bytes,
                        runtime_kind: ExtensionRuntimeKind::Js,
                        owner_fence: OwnerFence::Theme {
                            theme_id: cached.theme_id,
                            revision_id: cached.revision_id,
                            selection_version: cached.selection_version,
                        },
                        manifest: cached.manifest,
                    }));
                }
            }
        }

        Ok(None)
    }
}

impl PluginManager {
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_resolved_route(
        &self,
        resolved: ResolvedExtensionRoute,
        payload: Value,
        execution_mode: HookExecutionMode,
        principal: ExecutionPrincipal,
        timeout: std::time::Duration,
        actor: Option<&crate::auth::SessionContext>,
        hooks: &crate::hooks::HookDispatcher,
    ) -> Result<ExtensionRouteExecOutput, (StatusCode, PluginRunResponse)> {
        let correlation_id = format!("rte_{}", &crate::auth::generate_random_token()[..16]);
        let mk_err = |status: StatusCode, code: &str, msg: String| {
            (
                status,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(msg),
                    code: Some(code.to_string()),
                    correlation_id: correlation_id.clone(),
                    confirmation: None,
                },
            )
        };

        if self.recovery.is_safe_mode() {
            return Err(mk_err(
                StatusCode::SERVICE_UNAVAILABLE,
                "safe_mode",
                "Hệ thống đang ở safe mode sau khi khôi phục; route mở rộng tạm khóa.".into(),
            ));
        }

        let owner_kind = resolved.owner.kind;
        let owner_id = resolved.owner.id.clone();
        let manifest = resolved.manifest;
        let route_decl = resolved.route;
        let owner_fence = resolved.owner_fence;

        let occurred_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let event_str = format!("route:{}", route_decl.id);
        let runtime_input = serde_json::to_value(PluginEnvelopeV2::new(
            EnvelopeKind::Route,
            &event_str,
            occurred_at_ms,
            correlation_id.clone(),
            None,
            principal,
            0,
            payload,
        ))
        .unwrap_or(Value::Null);

        let caps_opt = if route_decl.capabilities.is_empty() {
            None
        } else {
            Some(route_decl.capabilities.clone())
        };
        let required_role = if route_decl.access == opendash_core::plugin::RouteAccess::Admin {
            "admin".to_string()
        } else {
            "editor".to_string()
        };
        let actor_fence = actor.map(|a| crate::plugin::runtime::ActionActorFence {
            user_id: a.user.id,
            required_role: required_role.clone(),
        });

        let exec_res = self
            .runtime
            .run_owner_action_with_principal(
                owner_kind,
                resolved.runtime_kind,
                resolved.code_bytes,
                manifest.clone(),
                runtime_input,
                self.db_pool.clone(),
                correlation_id.clone(),
                self.allow_loopback,
                Some(owner_fence.clone()),
                execution_mode,
                principal,
                Some(timeout),
                caps_opt.clone(),
                actor_fence,
            )
            .await;

        let exec_out = match exec_res {
            Ok(out) => out,
            Err(err) => {
                let (status, code_str) = match &err {
                    PluginError::Disabled => (StatusCode::CONFLICT, "generation_changed"),
                    PluginError::Timeout => {
                        if execution_mode == HookExecutionMode::RouteGet {
                            (StatusCode::SERVICE_UNAVAILABLE, "timeout")
                        } else {
                            (StatusCode::GATEWAY_TIMEOUT, "timeout")
                        }
                    }
                    PluginError::ConcurrencyLimit => {
                        (StatusCode::SERVICE_UNAVAILABLE, "concurrency_limit")
                    }
                    PluginError::Internal(m) if m.starts_with("rbac_revoked:") => {
                        (StatusCode::FORBIDDEN, "rbac_revoked")
                    }
                    _ => (StatusCode::BAD_REQUEST, "route_failed"),
                };
                let msg = err.to_string();
                if execution_mode != HookExecutionMode::RouteGet {
                    plugin_storage::record_owner_execution_result(
                        &self.db_pool,
                        owner_kind,
                        &owner_id,
                        &correlation_id,
                        "error",
                        &msg,
                        Some(&msg),
                    )
                    .await;
                }
                return Err(mk_err(status, code_str, msg));
            }
        };

        let render_trusted_html = manifest.permissions.render_trusted_html;

        let mut committed_receipt: Option<EntryCommitReceipt> = None;
        if execution_mode == HookExecutionMode::RoutePost {
            if let Some(cmd) = exec_out.staged_content_command {
                let Some(actor) = actor else {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "content_write_denied",
                        "Ghi nội dung bị cấm trên public route.".into(),
                    ));
                };
                if principal == ExecutionPrincipal::Public {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "content_write_denied",
                        "Ghi nội dung bị cấm trên public route.".into(),
                    ));
                }
                let version_before = cmd.expected_version;
                let record = self
                    .commit_staged_content_with_actor(
                        owner_kind,
                        &owner_id,
                        &manifest,
                        caps_opt.as_deref(),
                        owner_fence,
                        &required_role,
                        actor,
                        cmd,
                        hooks,
                        &correlation_id,
                    )
                    .await?;
                committed_receipt = Some(EntryCommitReceipt {
                    operation: EntryMutationKind::Save,
                    id: record.id,
                    type_key: record.content_type_key.clone(),
                    locale: record.locale.clone(),
                    translation_group_id: record.translation_group_id.clone(),
                    version_before,
                    version_after: Some(record.version),
                    publication_before: None,
                    publication_after: Some(record.publication_status.clone()),
                    trashed_at: None,
                    published_at: record.published_at.clone(),
                });
            } else if let Some(del) = exec_out.staged_lifecycle_command {
                if del.operation != StagedLifecycleOperation::Trash {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "content_delete_denied",
                        "Chỉ content_delete được phép trên custom route; restore/purge yêu cầu action xác nhận.".into(),
                    ));
                }
                let Some(actor) = actor else {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "content_delete_denied",
                        "Xóa nội dung bị cấm trên public route.".into(),
                    ));
                };
                if principal == ExecutionPrincipal::Public {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "content_delete_denied",
                        "Xóa nội dung bị cấm trên public route.".into(),
                    ));
                }
                if !manifest.can_delete_content_type(&del.type_key) {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "content_delete_denied",
                        "Gói mở rộng không có quyền xóa loại nội dung này.".into(),
                    ));
                }
                let fence = crate::storage::entries::PluginActionFence {
                    owner_fence,
                    required_role,
                };
                match plugin_storage::delete_entry_with_plugin_fence(
                    &self.db_pool,
                    actor,
                    &del.type_key,
                    del.entry_id,
                    del.expected_version,
                    &fence,
                )
                .await
                {
                    Ok(receipt) => {
                        committed_receipt = Some(receipt);
                    }
                    Err(err) => {
                        return Err(mk_err(err.0, "content_delete_fenced", err.1.to_string()));
                    }
                }
            }
            plugin_storage::record_owner_execution_result(
                &self.db_pool,
                owner_kind,
                &owner_id,
                &correlation_id,
                "info",
                "Thực thi route thành công",
                None,
            )
            .await;
        }

        Ok(ExtensionRouteExecOutput {
            output: exec_out.output,
            render_trusted_html,
            correlation_id,
            committed: committed_receipt,
        })
    }
}
