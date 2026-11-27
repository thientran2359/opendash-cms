use super::*;

impl EffectiveContentSchema {
    /// Assembles the unified `EffectiveContentSchema` for a `type_key` and enforces `Rule COMPAT-1`
    /// (global `field_key` uniqueness across base fields and all attached groups, including
    /// `provider_inactive` and `withdrawn` groups, targets, and fields).
    pub fn assemble(
        base: OwnedDescriptor<ContentTypeDefinition>,
        base_version: i64,
        mut attached_groups: Vec<AttachedFieldGroupSnapshot>,
    ) -> Result<Self, SchemaTransitionError> {
        attached_groups.sort_by(|a, b| a.group_id.cmp(&b.group_id));
        let is_builtin = matches!(base.descriptor.key.as_str(), "post" | "page");
        let mut effective_fields = BTreeMap::new();
        let mut ui_fields = Vec::new();
        let base_source_id = format!("base:{}", base.descriptor.key);

        if let Err(e) = crate::field_schema::validate_field_specs(
            &base.descriptor.fields,
            crate::field_schema::FieldContext::Content,
        ) {
            return Err(classify_validation_error(e));
        }
        for field in &base.descriptor.fields {
            if field.is_ui_only() {
                ui_fields.push(field.clone());
                continue;
            }
            if !is_builtin && RESERVED_ENTRY_KEYS.contains(&field.key.as_str()) {
                return Err(SchemaTransitionError::FieldKeyCollision(format!(
                    "Trường '{}' trùng với khóa hệ thống được bảo vệ.",
                    field.key
                )));
            }
            let effective_status =
                if base.declaration_state == DeclarationState::Withdrawn || field.is_withdrawn() {
                    EffectiveFieldStatus::Withdrawn
                } else if base.status != ProviderStatus::Active {
                    EffectiveFieldStatus::ProviderInactive
                } else {
                    EffectiveFieldStatus::Active
                };
            if effective_fields
                .insert(
                    field.key.clone(),
                    EffectiveFieldEntry {
                        definition: field.clone(),
                        owner: base.owner.clone(),
                        source_id: base_source_id.clone(),
                        effective_status,
                    },
                )
                .is_some()
            {
                return Err(SchemaTransitionError::FieldKeyCollision(format!(
                    "Trùng khóa custom field '{}' trong loại nội dung '{}'.",
                    field.key, base.descriptor.key
                )));
            }
        }

        for group in &attached_groups {
            if let Err(e) = crate::field_schema::validate_field_specs(
                &group.fields,
                crate::field_schema::FieldContext::Content,
            ) {
                return Err(classify_validation_error(e));
            }
            for field in &group.fields {
                if field.is_ui_only() {
                    ui_fields.push(field.clone());
                    continue;
                }
                if RESERVED_ENTRY_KEYS.contains(&field.key.as_str()) || field.key == "status" {
                    return Err(SchemaTransitionError::FieldKeyCollision(format!(
                        "Trường '{}' trong nhóm '{}' trùng với khóa hệ thống được bảo vệ.",
                        field.key, group.group_id
                    )));
                }
                if let Some(existing) = effective_fields.get(&field.key) {
                    return Err(SchemaTransitionError::FieldKeyCollision(format!(
                        "Khóa trường '{}' trong nhóm '{}' xung đột với trường đã tồn tại ở '{}'.",
                        field.key, group.group_id, existing.source_id
                    )));
                }
                let effective_status = if group.status == FieldGroupStatus::Withdrawn
                    || group.association_state == DeclarationState::Withdrawn
                    || field.is_withdrawn()
                {
                    EffectiveFieldStatus::Withdrawn
                } else if group.status == FieldGroupStatus::ProviderInactive {
                    EffectiveFieldStatus::ProviderInactive
                } else {
                    EffectiveFieldStatus::Active
                };
                effective_fields.insert(
                    field.key.clone(),
                    EffectiveFieldEntry {
                        definition: field.clone(),
                        owner: group.owner.clone(),
                        source_id: group.group_id.clone(),
                        effective_status,
                    },
                );
            }
        }

        if !is_builtin
            && !matches!(base.descriptor.key.as_str(), "section" | "pattern")
            && !effective_fields.contains_key("featured_image")
        {
            let mut fi_spec = crate::field_schema::FieldSpec::new(
                crate::field_schema::FieldType::Media,
                "featured_image",
                "Ảnh đại diện",
            );
            fi_spec.media_kind = Some(crate::field_schema::MediaFieldKind::Image);
            effective_fields.insert(
                "featured_image".into(),
                EffectiveFieldEntry {
                    definition: fi_spec,
                    owner: ResourceOwner::Core,
                    source_id: base_source_id.clone(),
                    effective_status: EffectiveFieldStatus::Active,
                },
            );
        }

        if !is_builtin
            && !matches!(base.descriptor.key.as_str(), "section" | "pattern")
            && base.descriptor.settings.supports(ContentSupport::Builder)
        {
            if let Some(existing_body) = effective_fields.get("body") {
                if existing_body.definition.field_type != crate::field_schema::FieldType::Richtext {
                    return Err(SchemaTransitionError::IncompatibleSchemaChange(format!(
                        "Trường 'body' trên loại nội dung '{}' phải có kiểu richtext khi bật ContentSupport::Builder.",
                        base.descriptor.key
                    )));
                }
            } else {
                let body_spec = crate::field_schema::FieldSpec::new(
                    crate::field_schema::FieldType::Richtext,
                    "body",
                    "Nội dung",
                );
                effective_fields.insert(
                    "body".into(),
                    EffectiveFieldEntry {
                        definition: body_spec,
                        owner: ResourceOwner::Core,
                        source_id: base_source_id.clone(),
                        effective_status: EffectiveFieldStatus::Active,
                    },
                );
            }
        }

        Ok(Self {
            base,
            base_version,
            attached_groups,
            effective_fields,
            ui_fields,
            schema_token: String::new(),
        })
    }

    /// Produces a deterministic canonical JSON string for computing `schema_token` (`sha256:<64-hex>`).
    pub fn canonical_token_payload_json(
        &self,
        taxonomies: &[String],
        active_theme_revision_id: Option<&str>,
        enabled_plugins: &[(String, i64)],
    ) -> String {
        let mut sorted_tax = taxonomies.to_vec();
        sorted_tax.sort();
        sorted_tax.dedup();

        let mut sorted_plugins = enabled_plugins.to_vec();
        sorted_plugins.sort_by(|a, b| a.0.cmp(&b.0));

        let payload = serde_json::json!({
            "base_type": {
                "key": self.base.descriptor.key,
                "label": self.base.descriptor.label,
                "owner": self.base.owner,
                "provider_status": self.base.status,
                "declaration_state": self.base.declaration_state,
                "version": self.base_version,
                "taxonomies": sorted_tax,
                "fields": self.base.descriptor.fields,
                "settings": self.base.descriptor.settings,
            },
            "attached_groups": self.attached_groups.iter().map(|g| {
                serde_json::json!({
                    "group_id": g.group_id,
                    "owner": g.owner,
                    "status": g.status,
                    "association_state": g.association_state,
                    "version": g.version,
                    "fields": g.fields,
                })
            }).collect::<Vec<_>>(),
            "provider_generations": {
                "active_theme_revision_id": active_theme_revision_id,
                "enabled_plugins": sorted_plugins,
            }
        });
        serde_json::to_string(&payload).expect("canonical schema token payload serializes")
    }
}

/// Symmetric compatibility and Editor create-satisfiability verification (`Rule COMPAT-1`..`Rule COMPAT-4`, `270-R1`, `270-R8`).
/// Runs unconditionally on every schema/group/package transition inside `BEGIN IMMEDIATE`.
pub fn verify_effective_schema_transition(
    old_effective: Option<&EffectiveContentSchema>,
    new_effective: &EffectiveContentSchema,
    has_existing_entries: bool,
) -> Result<(), SchemaTransitionError> {
    if let Some(old) = old_effective {
        let mut old_specs = Vec::new();
        for (old_key, old_entry) in &old.effective_fields {
            // Allow removing the synthetic core-injected `body` field when a custom CPT disables Builder.
            if !new_effective.effective_fields.contains_key(old_key)
                && old_key == "body"
                && old_entry.owner == ResourceOwner::Core
                && !matches!(old.base.descriptor.key.as_str(), "post" | "page")
                && !new_effective
                    .base
                    .descriptor
                    .settings
                    .supports(ContentSupport::Builder)
            {
                continue;
            }
            old_specs.push(old_entry.definition.clone());
        }
        let new_specs: Vec<crate::field_schema::FieldSpec> = new_effective
            .effective_fields
            .values()
            .map(|entry| entry.definition.clone())
            .collect();
        if let Err(msg) = crate::field_schema::check_schema_compatibility(
            &old_specs,
            &new_specs,
            has_existing_entries,
        ) {
            return Err(classify_validation_error(msg));
        }
    }

    // Rule COMPAT-4 (270-R8): Unconditional Editor create-satisfiability & ACL/default checks on every transition
    for (key, entry) in &new_effective.effective_fields {
        let (min_view_role, min_edit_role, public_exposure) = entry.definition.effective_acl();
        if min_edit_role < min_view_role {
            return Err(SchemaTransitionError::InvalidFieldAcl(format!(
                "Trường '{key}' có min_edit_role thấp hơn min_view_role."
            )));
        }
        if public_exposure == PublicExposure::Public && min_view_role != FieldRole::Editor {
            return Err(SchemaTransitionError::InvalidFieldAcl(format!(
                "Trường '{key}' có min_view_role = admin không được đặt public_exposure = public."
            )));
        }
        if let Some(ref def_val) = entry.definition.default {
            if !valid_default_value(&entry.definition, def_val) {
                return Err(SchemaTransitionError::InvalidFieldDefault(format!(
                    "Trường '{key}' có default không hợp lệ hoặc bị cấm."
                )));
            }
        }
        if entry.effective_required()
            && min_edit_role == FieldRole::Admin
            && entry.definition.default.is_none()
        {
            return Err(SchemaTransitionError::AdminRequiredFieldMissingDefault(
                format!(
                    "Trường bắt buộc '{key}' có min_edit_role = admin phải khai báo default hợp lệ để Editor có thể tạo mới bản ghi."
                ),
            ));
        }
    }

    Ok(())
}
