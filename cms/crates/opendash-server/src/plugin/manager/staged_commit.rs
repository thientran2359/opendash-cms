use super::*;

impl PluginManager {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn commit_staged_content_with_actor(
        &self,
        owner_kind: ExtensionOwnerKind,
        _owner_id: &str,
        manifest: &PluginManifest,
        caps_opt: Option<&[String]>,
        owner_fence: OwnerFence,
        required_role: &str,
        actor: &crate::auth::SessionContext,
        cmd: crate::plugin::host_ops::StagedContentCommand,
        hooks: &crate::hooks::HookDispatcher,
        correlation_id: &str,
    ) -> Result<crate::storage::entries::ContentEntryRecord, (StatusCode, PluginRunResponse)> {
        let mk_err = |status: StatusCode, code: &str, msg: String| {
            (
                status,
                PluginRunResponse {
                    success: false,
                    data: None,
                    error: Some(msg),
                    code: Some(code.to_string()),
                    correlation_id: correlation_id.to_owned(),
                    confirmation: None,
                },
            )
        };
        if !manifest.can_write_content_type(&cmd.type_key) {
            return Err(mk_err(
                StatusCode::FORBIDDEN,
                "content_write_denied",
                "Gói mở rộng không có quyền ghi loại nội dung này.".into(),
            ));
        }
        let actor_role = match actor.user.role {
            crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
            crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
        };
        let effective_schema =
            crate::storage::content::load_effective_content_schema(&self.db_pool, &cmd.type_key)
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
                manifest,
                caps_opt,
                actor_role,
                &cmd.type_key,
                field_entry,
            ) {
                return Err(mk_err(
                    StatusCode::FORBIDDEN,
                    "field_edit_forbidden",
                    format!("Gói mở rộng hoặc tài khoản không có quyền ghi trường '{field_key}'."),
                ));
            }
        }

        let schema = effective_schema.base.descriptor.clone();

        let entry_input = opendash_core::content::ContentEntryInput {
            slug: cmd.slug,
            title: cmd.title,
            locale: cmd.locale.clone(),
            values: cmd.values,
            term_ids: cmd.term_ids,
        };
        let loc_ctx = opendash_core::content::LocationEvalContext {
            content_type: Some(cmd.type_key.clone()),
            entry_id: cmd.entry_id,
            user_role: actor_role,
            ..Default::default()
        };
        if let Err(msg) = entry_input.validate_against_effective(&effective_schema, &loc_ctx) {
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
                    correlation_id: correlation_id.to_owned(),
                    confirmation: None,
                },
            ));
        }

        let fence = crate::storage::entries::PluginActionFence {
            owner_fence,
            required_role: required_role.to_owned(),
        };
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
        let _ = crate::storage::plugin_projections::recompute_entry_projections(
            &self.db_pool,
            hooks,
            record.id,
        )
        .await;
        hooks
            .dispatch_content_after_save_for_actor(
                &cmd.type_key,
                actor_role,
                serde_json::to_value(&record).unwrap_or_default(),
            )
            .await;
        Ok(record)
    }
}
