use super::*;

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
