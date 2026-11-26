use super::*;

impl PluginManager {
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_plugin_action(
        &self,
        id: &str,
        action_id: &str,
        actor: &crate::auth::SessionContext,
        input: Value,
        content_registry: &std::sync::Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
        hooks: &crate::hooks::HookDispatcher,
        correlation_id: String,
        origin: LifecycleActionOrigin,
        maintenance_epoch: u64,
    ) -> Result<ActionCommitOutcome, (StatusCode, PluginRunResponse)> {
        self.execute_owner_action(
            ExtensionOwnerKind::Plugin,
            Some(id),
            action_id,
            actor,
            input,
            content_registry,
            hooks,
            correlation_id,
            origin,
            maintenance_epoch,
        )
        .await
    }
}

impl PluginManager {
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_owner_action(
        &self,
        owner_kind: ExtensionOwnerKind,
        id_opt: Option<&str>,
        action_id: &str,
        actor: &crate::auth::SessionContext,
        input: Value,
        content_registry: &std::sync::Arc<
            tokio::sync::RwLock<
                opendash_core::registry::RegistryCatalog<
                    opendash_core::content::ContentTypeDefinition,
                >,
            >,
        >,
        hooks: &crate::hooks::HookDispatcher,
        correlation_id: String,
        origin: LifecycleActionOrigin,
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
                "Hệ thống đang ở safe mode sau khi khôi phục; gói mở rộng bị tạm khóa.".into(),
            ));
        }

        let (owner_id, code_bytes, manifest, owner_fence, runtime_kind) = match owner_kind {
            ExtensionOwnerKind::Plugin => {
                let id = id_opt.ok_or_else(|| {
                    mk_err(
                        StatusCode::NOT_FOUND,
                        "not_found",
                        "Plugin không tồn tại.".into(),
                    )
                })?;
                if let Some(dev) = crate::dev_mode::DevOverlayConfig::global()
                    && dev.has_dev_plugin(id)
                {
                    let snap = dev
                        .load_dev_plugin_by_id(id, self.runtime.engine())
                        .map_err(|err| mk_err(StatusCode::CONFLICT, "dev_plugin_invalid", err))?
                        .ok_or_else(|| {
                            mk_err(
                                StatusCode::NOT_FOUND,
                                "not_found",
                                "Plugin không tồn tại.".into(),
                            )
                        })?;
                    let db_gen =
                        plugin_storage::load_plugin_executable_with_generation(&self.db_pool, id)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|(_, _, enabled, gen_val, _)| enabled.then_some(gen_val))
                            .unwrap_or(0);
                    let rt_kind = snap.manifest.runtime_kind();
                    (
                        id.to_owned(),
                        snap.code_bytes,
                        snap.manifest,
                        OwnerFence::Plugin {
                            plugin_id: id.to_owned(),
                            expected_generation: db_gen,
                        },
                        rt_kind,
                    )
                } else {
                    let row_opt =
                        plugin_storage::load_plugin_executable_with_generation(&self.db_pool, id)
                            .await
                            .map_err(|e| {
                                mk_err(
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    "db_error",
                                    format!("Lỗi cơ sở dữ liệu: {e}"),
                                )
                            })?;
                    let Some((
                        wasm_blob,
                        manifest_json,
                        enabled,
                        revision_generation,
                        revision_digest_opt,
                    )) = row_opt
                    else {
                        return Err(mk_err(
                            StatusCode::NOT_FOUND,
                            "not_found",
                            "Plugin không tồn tại.".into(),
                        ));
                    };
                    if !enabled {
                        return Err(mk_err(
                            StatusCode::BAD_REQUEST,
                            "disabled",
                            "Plugin chưa được bật hoặc đã bị vô hiệu hóa.".into(),
                        ));
                    }
                    if wasm_blob.is_empty() && revision_digest_opt.is_none() {
                        return Err(mk_err(
                            StatusCode::NOT_FOUND,
                            "dev_plugin_unmounted",
                            "Plugin chỉ có ở chế độ dev; hãy khởi chạy kèm --dev-plugin hoặc cài bản ZIP chính thức.".into(),
                        ));
                    }
                    let manifest =
                        serde_json::from_str::<PluginManifest>(&manifest_json).map_err(|e| {
                            mk_err(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "corrupt_manifest",
                                format!("Manifest không hợp lệ: {e}"),
                            )
                        })?;
                    if let Some(digest) = &revision_digest_opt
                        && let Err(err) = plugin_storage::verify_plugin_revision_durability(
                            &self.plugins_root,
                            digest,
                        )
                    {
                        return Err(mk_err(StatusCode::CONFLICT, "corrupt_revision", err));
                    }
                    let rt_kind = manifest.runtime_kind();
                    (
                        id.to_owned(),
                        wasm_blob,
                        manifest,
                        OwnerFence::Plugin {
                            plugin_id: id.to_owned(),
                            expected_generation: revision_generation,
                        },
                        rt_kind,
                    )
                }
            }
            ExtensionOwnerKind::Theme => {
                let cached = self
                    .ensure_active_theme_cache()
                    .await
                    .map_err(|e| mk_err(StatusCode::INTERNAL_SERVER_ERROR, "theme_cache_error", e))?
                    .ok_or_else(|| {
                        mk_err(
                            StatusCode::NOT_FOUND,
                            "theme_functions_not_found",
                            "Theme hiện hành không có functions.js.".into(),
                        )
                    })?;
                if let Some(expected_id) = id_opt
                    && cached.theme_id != expected_id
                {
                    return Err(mk_err(
                        StatusCode::NOT_FOUND,
                        "theme_not_active",
                        "Theme không còn đang được kích hoạt.".into(),
                    ));
                }
                let fence = OwnerFence::Theme {
                    theme_id: cached.theme_id.clone(),
                    revision_id: cached.revision_id.clone(),
                    selection_version: cached.selection_version,
                };
                (
                    cached.theme_id,
                    cached.js_bytes,
                    cached.manifest,
                    fence,
                    ExtensionRuntimeKind::Js,
                )
            }
        };

        let owner = RuntimeOwner {
            kind: owner_kind,
            id: owner_id.clone(),
        };
        let _lease_guard = {
            if self.recovery.is_safe_mode() {
                return Err(mk_err(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "safe_mode",
                    "Hệ thống đang ở safe mode sau khi khôi phục; gói mở rộng bị tạm khóa.".into(),
                ));
            }
            let mut leases = self.active_action_leases.lock().map_err(|_| {
                mk_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "lease_lock",
                    "Lỗi khóa action lease.".into(),
                )
            })?;
            if !leases.insert(owner.clone()) {
                return Err(mk_err(
                    StatusCode::CONFLICT,
                    "recursive_dispatch",
                    "Không cho phép gọi đệ quy action trên cùng gói mở rộng.".into(),
                ));
            }
            ActionLeaseGuard {
                leases: std::sync::Arc::clone(&self.active_action_leases),
                owner: owner.clone(),
            }
        };

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
        let actor_role = match actor.user.role {
            crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
            crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
        };
        let declares_http = action_decl.capabilities.iter().any(|c| c == "http");
        let declares_lifecycle_confirm_write = action_decl
            .capabilities
            .iter()
            .any(|c| c == "content_restore" || c == "content_purge");
        if declares_http && declares_lifecycle_confirm_write {
            return Err(mk_err(
                StatusCode::FORBIDDEN,
                "capability_denied",
                "Action khai báo content_restore hoặc content_purge không được kết hợp capability http.".into(),
            ));
        }
        for cap in &action_decl.capabilities {
            let allowed = match cap.as_str() {
                "kv_read" => manifest.has_sqlite_perm("kv_read"),
                "kv_list" => manifest.has_sqlite_perm("kv_read"),
                "kv_write" | "kv_delete" => manifest.has_sqlite_perm("kv_write"),
                "content_read" => {
                    !manifest.permissions.content_read.is_empty()
                        || manifest.permissions.content_read_all_public
                }
                "content_read_private" => !manifest.permissions.content_read_private.is_empty(),
                "content_write" => !manifest.permissions.content_write.is_empty(),
                "content_write_private" => !manifest.permissions.content_write_private.is_empty(),
                "content_delete" => !manifest.permissions.content_delete.is_empty(),
                "content_trash_read" => !manifest.permissions.content_trash_read.is_empty(),
                "content_restore" => !manifest.permissions.content_restore.is_empty(),
                "content_purge" => !manifest.permissions.content_purge.is_empty(),
                "http" => {
                    !manifest.permissions.http.is_empty()
                        || !manifest.permissions.network.is_empty()
                }
                "taxonomy_read" => manifest.permissions.taxonomy_read,
                "media_read" => manifest.permissions.media_read,
                "site_options_read" => manifest.permissions.site_options_read,
                "plugin_settings_read" => manifest.permissions.plugin_settings_read,
                "plugin_secrets_read" => !manifest.permissions.plugin_secrets_read.is_empty(),
                "log" => true,
                _ => false,
            };
            if !allowed {
                return Err(mk_err(
                    StatusCode::FORBIDDEN,
                    "capability_denied",
                    format!("Gói mở rộng thiếu quyền manifest cho capability '{cap}'."),
                ));
            }
        }

        let mut filtered_input = input;
        self.filter_action_input_for_owner(
            owner_kind,
            &manifest,
            &action_decl.capabilities,
            actor_role,
            &mut filtered_input,
        )
        .await;

        let principal = match actor.user.role {
            crate::auth::Role::Admin => ExecutionPrincipal::Admin,
            crate::auth::Role::Editor => ExecutionPrincipal::Editor,
        };
        let runtime_input = if manifest.manifest_schema >= 3 {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let locale = filtered_input
                .get("locale")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            serde_json::to_value(PluginEnvelopeV2::new(
                EnvelopeKind::Action,
                format!("action:{action_id}"),
                now_ms,
                correlation_id.clone(),
                locale,
                principal,
                0,
                filtered_input,
            ))
            .unwrap_or(Value::Null)
        } else {
            filtered_input
        };

        let action_res = self
            .runtime
            .run_owner_action_with_principal(
                owner_kind,
                runtime_kind,
                code_bytes,
                manifest.clone(),
                runtime_input,
                self.db_pool.clone(),
                correlation_id.clone(),
                self.allow_loopback,
                Some(owner_fence.clone()),
                HookExecutionMode::Standard,
                principal,
                None,
                Some(action_decl.capabilities.clone()),
                Some(crate::plugin::runtime::ActionActorFence {
                    user_id: actor.user.id,
                    required_role: action_decl.required_role.clone(),
                }),
            )
            .await;

        let exec_out = match action_res {
            Ok(out) => out,
            Err(err) => {
                let (status, code_str) = match &err {
                    PluginError::Disabled => (StatusCode::CONFLICT, "generation_changed"),
                    PluginError::Timeout => (StatusCode::GATEWAY_TIMEOUT, "timeout"),
                    PluginError::Internal(m) if m.starts_with("rbac_revoked:") => {
                        (StatusCode::FORBIDDEN, "rbac_revoked")
                    }
                    _ => (StatusCode::BAD_REQUEST, "action_failed"),
                };
                let msg = err.to_string();
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
                return Err(mk_err(status, code_str, msg));
            }
        };

        if let Some(cmd) = exec_out.staged_content_command {
            if !manifest.can_write_content_type(&cmd.type_key) {
                return Err(mk_err(
                    StatusCode::FORBIDDEN,
                    "content_write_denied",
                    "Gói mở rộng không có quyền ghi loại nội dung này.".into(),
                ));
            }
            let _ = &content_registry;
            let effective_schema = crate::storage::content::load_effective_content_schema(
                &self.db_pool,
                &cmd.type_key,
            )
            .await
            .map_err(|e| mk_err(e.0, "schema_load_failed", e.1.to_string()))?
            .ok_or_else(|| {
                mk_err(
                    StatusCode::NOT_FOUND,
                    "content_type_not_found",
                    "Không tìm thấy loại nội dung.".into(),
                )
            })?;

            for field_key in cmd.values.keys() {
                let Some(field_entry) = effective_schema.effective_fields.get(field_key) else {
                    return Err(mk_err(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "unknown_field_key",
                        format!(
                            "Trường '{field_key}' không tồn tại trong schema của '{}'.",
                            cmd.type_key
                        ),
                    ));
                };
                if !opendash_core::plugin::plugin_can_write_field_for_owner(
                    owner_kind,
                    &manifest,
                    Some(&action_decl.capabilities),
                    actor_role,
                    &cmd.type_key,
                    field_entry,
                ) {
                    return Err(mk_err(
                        StatusCode::FORBIDDEN,
                        "field_edit_forbidden",
                        format!(
                            "Gói mở rộng hoặc tài khoản không có quyền ghi trường '{field_key}'."
                        ),
                    ));
                }
            }

            let schema = opendash_core::content::ContentTypeDefinition {
                key: effective_schema.base.descriptor.key.clone(),
                label: effective_schema.base.descriptor.label.clone(),
                settings: effective_schema.base.descriptor.settings.clone(),
                fields: effective_schema
                    .effective_fields
                    .values()
                    .map(|e| {
                        let mut def = e.definition.clone();
                        if !e.effective_required() {
                            def.required = false;
                        }
                        def
                    })
                    .collect(),
            };

            let entry_input = opendash_core::content::ContentEntryInput {
                slug: cmd.slug,
                title: cmd.title,
                locale: cmd.locale.clone(),
                values: cmd.values,
                term_ids: cmd.term_ids,
            };
            if let Err(msg) = entry_input.validate_against_effective(&effective_schema) {
                return Err(mk_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_entry_input",
                    msg.to_string(),
                ));
            }
            if let Err(err) = crate::storage::media::require_existing_effective_references(
                &self.db_pool,
                &effective_schema,
                &entry_input.values,
            )
            .await
            {
                return Err(mk_err(err.0, "media_reference_error", err.1.to_string()));
            }

            let (hook_locale, hook_group_id) = match cmd.entry_id {
                Some(entry_id) => {
                    match crate::storage::entries::get_entry_locale_and_group(
                        &self.db_pool,
                        &cmd.type_key,
                        entry_id,
                    )
                    .await
                    {
                        Ok(Some((existing_locale, group_id))) => {
                            if let Some(raw) = cmd.locale.as_deref()
                                && raw.trim().to_ascii_lowercase() != existing_locale
                            {
                                return Err(mk_err(
                                    StatusCode::BAD_REQUEST,
                                    "locale_immutable",
                                    "Không thể đổi ngôn ngữ của nội dung đã tạo.".into(),
                                ));
                            }
                            (Some(existing_locale), Some(group_id))
                        }
                        Ok(None) => (cmd.locale.clone(), None),
                        Err(err) => {
                            return Err(mk_err(err.0, "entry_lookup_failed", err.1.to_string()));
                        }
                    }
                }
                None => {
                    let effective_locale = crate::storage::settings::resolve_requested_locale(
                        &self.db_pool,
                        cmd.locale.as_deref(),
                    )
                    .await
                    .map_err(|err| mk_err(err.0, "invalid_locale", err.1.to_string()))?;
                    (Some(effective_locale), None)
                }
            };

            let operation = if cmd.entry_id.is_some() {
                "update"
            } else {
                "create"
            };
            let candidate = serde_json::json!({
                "operation": operation,
                "type_key": &cmd.type_key,
                "entry_id": cmd.entry_id,
                "expected_version": cmd.expected_version,
                "locale": hook_locale,
                "translation_group_id": hook_group_id,
                "title": &entry_input.title,
                "slug": &entry_input.slug,
                "candidate_values": &entry_input.values,
                "term_ids": &entry_input.term_ids,
            });
            if let Err(veto) = hooks
                .dispatch_content_before_save_for_actor(&cmd.type_key, actor_role, candidate)
                .await
            {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    PluginRunResponse {
                        success: false,
                        data: serde_json::to_value(&veto).ok(),
                        error: Some(veto.message),
                        code: Some(veto.code),
                        correlation_id,
                        confirmation: None,
                    },
                ));
            }

            let fence = crate::storage::entries::PluginActionFence {
                owner_fence: owner_fence.clone(),
                required_role: action_decl.required_role.clone(),
            };
            let version_before = cmd.expected_version;
            let saved_res = match cmd.entry_id {
                Some(entry_id) => {
                    let expected_ver = cmd.expected_version.ok_or_else(|| {
                        mk_err(
                            StatusCode::PRECONDITION_REQUIRED,
                            "missing_expected_version",
                            "Cập nhật nội dung yêu cầu expected_version.".into(),
                        )
                    })?;
                    crate::storage::entries::update_entry_with_fence(
                        &self.db_pool,
                        actor,
                        &cmd.type_key,
                        &schema,
                        entry_id,
                        expected_ver,
                        &entry_input,
                        Some(&fence),
                    )
                    .await
                }
                None => {
                    crate::storage::entries::create_entry_with_fence(
                        &self.db_pool,
                        actor,
                        &cmd.type_key,
                        &schema,
                        &entry_input,
                        Some(&fence),
                    )
                    .await
                }
            };
            let record = match saved_res {
                Ok(r) => r,
                Err(err) => {
                    return Err(mk_err(err.0, "content_commit_fenced", err.1.to_string()));
                }
            };
            let projection = crate::storage::entry_lifecycle::recompute_projections_nonfatal(
                &self.db_pool,
                hooks,
                record.id,
                true,
            )
            .await;
            hooks
                .dispatch_content_after_save_for_actor(
                    &cmd.type_key,
                    actor_role,
                    serde_json::to_value(&record).unwrap_or_default(),
                )
                .await;
            plugin_storage::record_owner_execution_result(
                &self.db_pool,
                owner_kind,
                &owner_id,
                &correlation_id,
                "info",
                "Thực thi action thành công",
                None,
            )
            .await;
            let receipt = EntryCommitReceipt {
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
            };
            return Ok(ActionCommitOutcome {
                response: PluginRunResponse {
                    success: true,
                    data: Some(serde_json::json!({
                        "output": exec_out.output,
                        "entry": record,
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
            });
        }

        if let Some(staged) = exec_out.staged_lifecycle_command {
            // Exhaustive conversion from wire StagedLifecycleOperation to storage LifecycleOperation.
            let domain_op = match staged.operation {
                StagedLifecycleOperation::Trash => LifecycleOperation::Trash,
                StagedLifecycleOperation::Restore => LifecycleOperation::Restore,
                StagedLifecycleOperation::Purge => LifecycleOperation::Purge,
            };
            let (required_cap, has_manifest_grant) = match domain_op {
                LifecycleOperation::Trash => (
                    "content_delete",
                    manifest.can_delete_content_type(&staged.type_key),
                ),
                LifecycleOperation::Restore => (
                    "content_restore",
                    manifest.can_restore_content_type(&staged.type_key),
                ),
                LifecycleOperation::Purge => (
                    "content_purge",
                    manifest.can_purge_content_type(&staged.type_key),
                ),
            };
            if !action_decl.capabilities.iter().any(|c| c == required_cap) || !has_manifest_grant {
                return Err(mk_err(
                    StatusCode::FORBIDDEN,
                    "content_lifecycle_denied",
                    format!("Gói mở rộng không có quyền '{required_cap}' cho loại nội dung này."),
                ));
            }
            if domain_op == LifecycleOperation::Purge && actor.user.role != crate::auth::Role::Admin
            {
                return Err(mk_err(
                    StatusCode::FORBIDDEN,
                    "entry_purge_forbidden",
                    "Chỉ Quản trị viên được phép xóa vĩnh viễn nội dung.".into(),
                ));
            }

            let command = LifecycleCommand {
                operation: domain_op,
                type_key: staged.type_key.clone(),
                entry_id: staged.entry_id,
                expected_version: staged.expected_version,
            };
            let fence = crate::storage::entries::PluginActionFence {
                owner_fence: owner_fence.clone(),
                required_role: action_decl.required_role.clone(),
            };

            match domain_op {
                LifecycleOperation::Trash => {
                    let authority = LifecycleAuthority {
                        session: actor,
                        plugin_fence: Some(&fence),
                        confirmation: LifecycleConfirmationEvidence::None,
                        scope: LifecycleScope::Ordinary,
                    };
                    let receipt = crate::storage::entry_lifecycle::execute_lifecycle_transition(
                        &self.db_pool,
                        &command,
                        &authority,
                    )
                    .await
                    .map_err(|err| {
                        mk_err(err.0, map_lifecycle_api_error_code(&err), err.1.to_string())
                    })?;
                    plugin_storage::record_owner_execution_result(
                        &self.db_pool,
                        owner_kind,
                        &owner_id,
                        &correlation_id,
                        "info",
                        "Thực thi action thành công",
                        None,
                    )
                    .await;
                    return Ok(ActionCommitOutcome {
                        response: PluginRunResponse {
                            success: true,
                            data: Some(serde_json::json!({
                                "output": exec_out.output,
                                "deleted_entry_id": receipt.id,
                                "deleted_type_key": receipt.type_key,
                                "lifecycle": lifecycle_data_value(&receipt, "trash"),
                            })),
                            error: None,
                            code: None,
                            correlation_id,
                            confirmation: None,
                        },
                        committed: Some(receipt),
                        secondary: SecondaryOutcome::none(),
                    });
                }
                LifecycleOperation::Restore | LifecycleOperation::Purge => {
                    let preview =
                        crate::storage::entry_lifecycle::preflight_plugin_lifecycle_confirmation(
                            &self.db_pool,
                            actor,
                            &command,
                            &fence,
                        )
                        .await
                        .map_err(|err| {
                            mk_err(err.0, map_lifecycle_api_error_code(&err), err.1.to_string())
                        })?;

                    if !preview.needs_confirmation {
                        let authority = LifecycleAuthority {
                            session: actor,
                            plugin_fence: Some(&fence),
                            confirmation: LifecycleConfirmationEvidence::None,
                            scope: LifecycleScope::Ordinary,
                        };
                        let receipt =
                            crate::storage::entry_lifecycle::execute_lifecycle_transition(
                                &self.db_pool,
                                &command,
                                &authority,
                            )
                            .await
                            .map_err(|err| {
                                mk_err(err.0, map_lifecycle_api_error_code(&err), err.1.to_string())
                            })?;
                        let projection =
                            crate::storage::entry_lifecycle::recompute_projections_nonfatal(
                                &self.db_pool,
                                hooks,
                                receipt.id,
                                receipt.publication_after.as_deref() == Some("published"),
                            )
                            .await;
                        plugin_storage::record_owner_execution_result(
                            &self.db_pool,
                            owner_kind,
                            &owner_id,
                            &correlation_id,
                            "info",
                            "Thực thi action thành công",
                            None,
                        )
                        .await;
                        return Ok(ActionCommitOutcome {
                            response: PluginRunResponse {
                                success: true,
                                data: Some(serde_json::json!({
                                    "output": exec_out.output,
                                    "lifecycle": lifecycle_data_value(&receipt, "restore"),
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
                        });
                    }

                    // Purge or immediate-mode public Restore requires explicit host-owned confirmation
                    // (scoped to Plugin actions).
                    let OwnerFence::Plugin {
                        expected_generation: revision_generation,
                        ..
                    } = owner_fence
                    else {
                        return Err(mk_err(
                            StatusCode::FORBIDDEN,
                            "content_lifecycle_denied",
                            "Xác nhận thao tác vòng đời chỉ hỗ trợ cho plugin đã khai báo.".into(),
                        ));
                    };
                    let site_id = crate::storage::auth::compute_site_instance_id(&self.db_pool)
                        .await
                        .unwrap_or_else(|| "default-site".into());
                    let display = self
                        .confirmation_store
                        .insert(
                            maintenance_epoch,
                            NewPendingConfirmation {
                                command,
                                site_id,
                                session_token_hash: actor.token_hash.clone(),
                                actor_id: actor.user.id,
                                actor_role: actor.user.role,
                                plugin_id: owner_id.clone(),
                                action_id: action_id.to_string(),
                                expected_generation: revision_generation,
                                origin,
                                correlation_id: correlation_id.clone(),
                                initiating_maintenance_epoch: maintenance_epoch,
                                locale: preview.locale,
                                title: preview.title,
                                slug: preview.slug,
                            },
                        )
                        .map_err(|err| match err {
                            ConfirmationStoreError::ScopeAlreadyPending => mk_err(
                                StatusCode::CONFLICT,
                                "confirmation_pending_limit",
                                "Đang có một yêu cầu xác nhận chưa hoàn tất cho phiên và action này.".into(),
                            ),
                            ConfirmationStoreError::GlobalCapacityReached => mk_err(
                                StatusCode::TOO_MANY_REQUESTS,
                                "confirmation_capacity_reached",
                                "Quá nhiều yêu cầu xác nhận đang chờ trên hệ thống; hãy thử lại sau.".into(),
                            ),
                            ConfirmationStoreError::ItemTooLarge => mk_err(
                                StatusCode::PAYLOAD_TOO_LARGE,
                                "confirmation_too_large",
                                "Dữ liệu xác nhận vượt quá giới hạn cho phép.".into(),
                            ),
                            ConfirmationStoreError::LockPoisoned => mk_err(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "confirmation_store_error",
                                "Lỗi nội bộ bộ nhớ xác nhận.".into(),
                            ),
                        })?;

                    let prompt_msg = if domain_op == LifecycleOperation::Purge {
                        "Thao tác xóa vĩnh viễn nội dung cần xác nhận rõ ràng không thể hoàn tác từ Quản trị viên."
                    } else {
                        "Khôi phục nội dung chế độ lưu ngay sẽ hiển thị công khai ngay lập tức; cần xác nhận rõ ràng."
                    };
                    return Err((
                        StatusCode::CONFLICT,
                        PluginRunResponse {
                            success: false,
                            data: None,
                            error: Some(prompt_msg.into()),
                            code: Some("confirmation_required".into()),
                            correlation_id,
                            confirmation: Some(display),
                        },
                    ));
                }
            }
        }

        plugin_storage::record_owner_execution_result(
            &self.db_pool,
            owner_kind,
            &owner_id,
            &correlation_id,
            "info",
            "Thực thi action thành công",
            None,
        )
        .await;

        Ok(ActionCommitOutcome {
            response: PluginRunResponse {
                success: true,
                data: Some(exec_out.output),
                error: None,
                code: None,
                correlation_id,
                confirmation: None,
            },
            committed: None,
            secondary: SecondaryOutcome::none(),
        })
    }
}
