use super::*;

impl PluginManager {
    /// Resolves a host-owned single-use lifecycle confirmation (`confirm` or `cancel`)
    /// without ever re-invoking guest Wasm or accepting replacement targets.
    #[allow(clippy::too_many_arguments)]
    pub async fn resolve_plugin_lifecycle_confirmation(
        &self,
        id: &str,
        action_id: &str,
        actor: &crate::auth::SessionContext,
        confirmation_id: &str,
        decision: &str,
        hooks: &crate::hooks::HookDispatcher,
        correlation_id: String,
        maintenance_epoch: u64,
    ) -> Result<ActionCommitOutcome, (StatusCode, PluginRunResponse)> {
        let mk_err = |status: StatusCode, code: &str, msg: String| {
            (
                status,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(msg),
                    code: Some(code.into()),
                    correlation_id: correlation_id.clone(),
                    confirmation: None,
                },
            )
        };

        if self.recovery.is_safe_mode() {
            return Err(mk_err(
                StatusCode::SERVICE_UNAVAILABLE,
                "safe_mode",
                "Hệ thống đang ở safe mode sau khi khôi phục; plugin bị tạm khóa.".into(),
            ));
        }
        if !matches!(decision, "confirm" | "cancel") {
            return Err(mk_err(
                StatusCode::BAD_REQUEST,
                "invalid_decision",
                "Quyết định xác nhận chỉ chấp nhận 'confirm' hoặc 'cancel'.".into(),
            ));
        }

        let site_id = crate::storage::auth::compute_site_instance_id(&self.db_pool)
            .await
            .unwrap_or_else(|| "default-site".into());
        let pending_opt = self
            .confirmation_store
            .consume(
                confirmation_id,
                &site_id,
                &actor.token_hash,
                actor.user.id,
                id,
                action_id,
                maintenance_epoch,
            )
            .map_err(|_| {
                mk_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "confirmation_store_error",
                    "Lỗi nội bộ bộ nhớ xác nhận.".into(),
                )
            })?;

        let Some(pending) = pending_opt else {
            return Err(mk_err(
                StatusCode::CONFLICT,
                "confirmation_invalid",
                "Yêu cầu xác nhận không hợp lệ, đã hết hạn hoặc đã được sử dụng.".into(),
            ));
        };

        if decision == "cancel" {
            return Ok(ActionCommitOutcome {
                response: PluginRunResponse {
                    success: false,
                    data: None,
                    error: None,
                    code: Some("confirmation_cancelled".into()),
                    correlation_id,
                    confirmation: None,
                },
                committed: None,
                secondary: SecondaryOutcome::none(),
            });
        }

        // Reauthorize installed plugin, manifest, generation, capability, and origin before commit.
        if actor.user.role != pending.actor_role {
            return Err(mk_err(
                StatusCode::FORBIDDEN,
                "rbac_revoked",
                "Quyền của tài khoản đã thay đổi kể từ khi tạo yêu cầu xác nhận.".into(),
            ));
        }
        let row_opt = plugin_storage::load_plugin_executable_with_generation(&self.db_pool, id)
            .await
            .map_err(|e| {
                mk_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "db_error",
                    format!("Lỗi cơ sở dữ liệu: {e}"),
                )
            })?;
        let Some((_, manifest_json, enabled, revision_generation, _)) = row_opt else {
            return Err(mk_err(
                StatusCode::CONFLICT,
                "generation_changed",
                "Plugin đã bị gỡ hoặc thay đổi phiên bản.".into(),
            ));
        };
        if !enabled || revision_generation != pending.expected_generation {
            return Err(mk_err(
                StatusCode::CONFLICT,
                "generation_changed",
                "Plugin đã bị vô hiệu hóa hoặc thay đổi phiên bản trong lúc chờ xác nhận.".into(),
            ));
        }
        let manifest = serde_json::from_str::<PluginManifest>(&manifest_json).map_err(|e| {
            mk_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "corrupt_manifest",
                format!("Manifest không hợp lệ: {e}"),
            )
        })?;
        let Some(action_decl) = manifest.action(action_id).cloned() else {
            return Err(mk_err(
                StatusCode::NOT_FOUND,
                "action_not_found",
                "Action không tồn tại trong manifest.".into(),
            ));
        };
        if action_decl.required_role == "admin" && actor.user.role != crate::auth::Role::Admin {
            return Err(mk_err(
                StatusCode::FORBIDDEN,
                "rbac_denied",
                "Tài khoản không đủ quyền thực thi action này.".into(),
            ));
        }
        let (required_cap, has_manifest_grant, evidence, op_label) = match pending.command.operation
        {
            LifecycleOperation::Purge => (
                "content_purge",
                manifest.can_purge_content_type(&pending.command.type_key),
                LifecycleConfirmationEvidence::ExplicitPurge {
                    type_key: pending.command.type_key.clone(),
                    entry_id: pending.command.entry_id,
                    expected_version: pending.command.expected_version,
                    session_token_hash: actor.token_hash.clone(),
                },
                "purge",
            ),
            LifecycleOperation::Restore => (
                "content_restore",
                manifest.can_restore_content_type(&pending.command.type_key),
                LifecycleConfirmationEvidence::PublicRestore {
                    type_key: pending.command.type_key.clone(),
                    entry_id: pending.command.entry_id,
                    expected_version: pending.command.expected_version,
                    session_token_hash: actor.token_hash.clone(),
                },
                "restore",
            ),
            LifecycleOperation::Trash => {
                return Err(mk_err(
                    StatusCode::CONFLICT,
                    "confirmation_invalid",
                    "Yêu cầu xác nhận không hợp lệ, đã hết hạn hoặc đã được sử dụng.".into(),
                ));
            }
        };
        if !action_decl.capabilities.iter().any(|c| c == required_cap)
            || !has_manifest_grant
            || action_decl.capabilities.iter().any(|c| c == "http")
        {
            return Err(mk_err(
                StatusCode::FORBIDDEN,
                "capability_denied",
                format!("Plugin không còn quyền '{required_cap}' hợp lệ."),
            ));
        }

        if let LifecycleActionOrigin::ProviderRow { type_key, entry_id } = &pending.origin {
            if type_key != &pending.command.type_key || *entry_id != pending.command.entry_id {
                return Err(mk_err(
                    StatusCode::CONFLICT,
                    "confirmation_invalid",
                    "Yêu cầu xác nhận không khớp phạm vi hàng dữ liệu ban đầu.".into(),
                ));
            }
            let actor_role = match actor.user.role {
                crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
                crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
            };
            let list_cfg = crate::storage::admin_extensions::effective_list(
                &self.db_pool,
                type_key,
                actor_role,
            )
            .await
            .map_err(|err| mk_err(err.0, "provider_config_error", err.1.to_string()))?;
            if !list_cfg.columns.iter().any(|col| {
                col.owner_kind == "plugin"
                    && col.owner_id == id
                    && col.action_id.as_deref() == Some(action_id)
            }) {
                return Err(mk_err(
                    StatusCode::FORBIDDEN,
                    "provider_action_revoked",
                    "Action không còn thuộc cấu hình cột hiệu lực.".into(),
                ));
            }
        }

        let fence = crate::storage::entries::PluginActionFence {
            owner_fence: OwnerFence::Plugin {
                plugin_id: id.to_string(),
                expected_generation: pending.expected_generation,
            },
            required_role: action_decl.required_role.clone(),
        };
        let authority = LifecycleAuthority {
            session: actor,
            plugin_fence: Some(&fence),
            confirmation: evidence,
            scope: LifecycleScope::Ordinary,
        };
        let receipt = crate::storage::entry_lifecycle::execute_lifecycle_transition(
            &self.db_pool,
            &pending.command,
            &authority,
        )
        .await
        .map_err(|err| mk_err(err.0, map_lifecycle_api_error_code(&err), err.1.to_string()))?;

        let projection = crate::storage::entry_lifecycle::recompute_projections_nonfatal(
            &self.db_pool,
            hooks,
            receipt.id,
            pending.command.operation == LifecycleOperation::Restore
                && receipt.publication_after.as_deref() == Some("published"),
        )
        .await;
        plugin_storage::record_plugin_execution_result(
            &self.db_pool,
            id,
            &correlation_id,
            "info",
            "Xác nhận và hoàn tất thao tác vòng đời thành công",
            None,
        )
        .await;

        Ok(ActionCommitOutcome {
            response: PluginRunResponse {
                success: true,
                data: Some(serde_json::json!({
                    "lifecycle": lifecycle_data_value(&receipt, op_label),
                })),
                error: None,
                code: None,
                correlation_id,
                confirmation: None,
            },
            committed: Some(receipt),
            secondary: SecondaryOutcome {
                projection,
                hook_enqueued: 0,
                hook_dropped: 0,
            },
        })
    }
}

impl PluginManager {
    pub async fn install_plugin(&self, zip_bytes: &[u8]) -> Result<PluginSummary, String> {
        self.install_plugin_checked(zip_bytes)
            .await
            .map_err(|e| e.to_string())
    }
}

impl PluginManager {
    pub async fn install_plugin_checked(
        &self,
        zip_bytes: &[u8],
    ) -> Result<PluginSummary, plugin_storage::PluginStorageError> {
        let package = PluginPackage::from_zip(zip_bytes, self.runtime.engine())
            .map_err(|e| plugin_storage::PluginStorageError::BadRequest(e.to_string()))?;

        let manifest_json = serde_json::to_string(&package.manifest).map_err(|e| {
            plugin_storage::PluginStorageError::Internal(format!("Lỗi serialize manifest: {e}"))
        })?;

        plugin_storage::install_plugin_revision(
            &self.db_pool,
            &self.plugins_root,
            &package.manifest,
            &manifest_json,
            &package.wasm_bytes,
            package.admin_extensions.as_ref(),
            &package.blocks,
        )
        .await?;
        self.invalidate_cache().await;

        self.get_plugin(&package.manifest.id)
            .await
            .map_err(plugin_storage::PluginStorageError::Internal)?
            .ok_or_else(|| {
                plugin_storage::PluginStorageError::Internal(
                    "Không tìm thấy plugin vừa cài đặt.".to_string(),
                )
            })
    }
}
