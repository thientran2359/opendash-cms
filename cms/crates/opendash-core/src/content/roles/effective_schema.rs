use super::*;

impl EffectiveContentSchema {
    /// Assembles the unified `EffectiveContentSchema` for a `type_key` and enforces `Rule COMPAT-1`
    /// (global `field_key` uniqueness across Core-managed keys and all attached groups, including
    /// `active == false`, `provider_inactive` and `withdrawn` groups/fields).
    pub fn assemble(
        base: OwnedDescriptor<ContentTypeDefinition>,
        base_version: i64,
        mut attached_groups: Vec<AttachedFieldGroupSnapshot>,
    ) -> Result<Self, SchemaTransitionError> {
        attached_groups.sort_by(|a, b| a.group_id.cmp(&b.group_id));
        let is_private_reusable = matches!(base.descriptor.key.as_str(), "section" | "pattern");
        let mut effective_fields: BTreeMap<String, EffectiveFieldEntry> = BTreeMap::new();
        let mut ui_fields = Vec::new();
        let mut seen_sync_taxonomies: HashSet<String> = HashSet::new();
        let base_source_id = format!("base:{}", base.descriptor.key);

        for group in &attached_groups {
            if let Err(e) = crate::field_schema::validate_field_specs(
                &group.fields,
                crate::field_schema::FieldContext::Content,
            ) {
                return Err(classify_validation_error(e));
            }
            if group.fields.iter().any(|f| f.sync_entry_terms)
                && location_rules_have_param(&group.location, LocationParam::EntryTaxonomy)
            {
                return Err(SchemaTransitionError::SyncEntryTermsLocationCycle(format!(
                    "Nhóm '{}' không thể bật sync_entry_terms khi dùng rule entry_taxonomy.",
                    group.group_id
                )));
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
                if effective_status != EffectiveFieldStatus::Withdrawn
                    && field.sync_entry_terms
                    && let Some(tax) = field.taxonomy.as_deref()
                    && !seen_sync_taxonomies.insert(tax.to_owned())
                {
                    return Err(SchemaTransitionError::DuplicateSyncEntryTermsTaxonomy(
                        format!(
                            "Phân loại '{tax}' chỉ được đồng bộ bởi tối đa một trường term_ref trên '{}'.",
                            base.descriptor.key
                        ),
                    ));
                }
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

        if !is_private_reusable {
            let settings = &base.descriptor.settings;
            if settings.supports(ContentSupport::Editor)
                || settings.supports(ContentSupport::Builder)
            {
                let mut body_spec = crate::field_schema::FieldSpec::new(
                    crate::field_schema::FieldType::Richtext,
                    "body",
                    "Nội dung",
                );
                body_spec.acl = Some(crate::field_schema::FieldAcl::canonical(
                    FieldRole::Editor,
                    FieldRole::Editor,
                    PublicExposure::Public,
                ));
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
            if settings.supports(ContentSupport::FeaturedImage) {
                let mut fi_spec = crate::field_schema::FieldSpec::new(
                    crate::field_schema::FieldType::Media,
                    "featured_image",
                    "Ảnh đại diện",
                );
                fi_spec.media_kind = Some(crate::field_schema::MediaFieldKind::Image);
                fi_spec.acl = Some(crate::field_schema::FieldAcl::canonical(
                    FieldRole::Editor,
                    FieldRole::Editor,
                    PublicExposure::Public,
                ));
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
            if settings.supports(ContentSupport::Excerpt) {
                let mut exc_spec = crate::field_schema::FieldSpec::new(
                    crate::field_schema::FieldType::Textarea,
                    "excerpt",
                    "Tóm tắt",
                );
                exc_spec.max_chars = Some(1000);
                exc_spec.acl = Some(crate::field_schema::FieldAcl::canonical(
                    FieldRole::Editor,
                    FieldRole::Editor,
                    PublicExposure::Public,
                ));
                effective_fields.insert(
                    "excerpt".into(),
                    EffectiveFieldEntry {
                        definition: exc_spec,
                        owner: ResourceOwner::Core,
                        source_id: base_source_id,
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

    pub fn is_field_active_in_context(
        &self,
        entry: &EffectiveFieldEntry,
        loc_ctx: &LocationEvalContext,
    ) -> bool {
        if entry.owner == ResourceOwner::Core {
            return true;
        }
        self.attached_groups
            .iter()
            .find(|g| g.group_id == entry.source_id)
            .is_some_and(|g| g.active && evaluate_location_rules(&g.location, loc_ctx))
    }

    pub fn is_group_admin_only(&self, source_id: &str) -> bool {
        self.attached_groups
            .iter()
            .find(|g| g.group_id == source_id)
            .is_some_and(|g| {
                !group_allows_role_for_target(
                    &g.location,
                    "content_type",
                    &self.base.descriptor.key,
                    FieldRole::Editor,
                )
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
                "singular_label": self.base.descriptor.singular_label,
                "description": self.base.descriptor.description,
                "owner": self.base.owner,
                "provider_status": self.base.status,
                "declaration_state": self.base.declaration_state,
                "version": self.base_version,
                "taxonomies": sorted_tax,
                "settings": self.base.descriptor.settings,
            },
            "attached_groups": self.attached_groups.iter().map(|g| {
                serde_json::json!({
                    "group_id": g.group_id,
                    "owner": g.owner,
                    "status": g.status,
                    "association_state": g.association_state,
                    "active": g.active,
                    "domain": g.domain,
                    "location": g.location,
                    "presentation": g.presentation,
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
            // Core-managed `supports` fields (`body`, `featured_image`, `excerpt`) may be toggled off freely.
            if !new_effective.effective_fields.contains_key(old_key)
                && old_entry.owner == ResourceOwner::Core
                && CORE_MANAGED_ENTRY_KEYS.contains(&old_key.as_str())
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
        if let Some(ref def_val) = entry.definition.default
            && !valid_default_value(&entry.definition, def_val)
        {
            return Err(SchemaTransitionError::InvalidFieldDefault(format!(
                "Trường '{key}' có default không hợp lệ hoặc bị cấm."
            )));
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

impl ContentEntryInput {
    pub fn validate_against_effective(
        &self,
        effective: &EffectiveContentSchema,
        loc_ctx: &LocationEvalContext,
    ) -> Result<(), &'static str> {
        if self.title.trim().is_empty()
            || self.title.chars().count() > 200
            || self.title.chars().any(char::is_control)
        {
            return Err("Tiêu đề nội dung không hợp lệ.");
        }
        if !valid_kebab_segment_with_limit(&self.slug, 64) {
            return Err("Slug nội dung không hợp lệ.");
        }
        if let Some(ids) = &self.term_ids
            && (ids.len() > 96
                || ids.iter().any(|id| *id <= 0)
                || ids.iter().copied().collect::<HashSet<_>>().len() != ids.len())
        {
            return Err("term_ids phải là danh sách ID dương, duy nhất (tối đa 96).");
        }
        for key in self.values.keys() {
            if !effective.effective_fields.contains_key(key) {
                return Err("Giá trị chứa custom field chưa được khai báo.");
            }
        }
        let root_specs: Vec<crate::field_schema::FieldSpec> = effective
            .effective_fields
            .values()
            .map(|e| e.definition.clone())
            .chain(effective.ui_fields.iter().cloned())
            .collect();
        for (key, entry) in &effective.effective_fields {
            let group_active = effective.is_field_active_in_context(entry, loc_ctx);
            let visible = group_active
                && crate::field_schema::evaluate_conditions(
                    entry.definition.conditions.as_ref(),
                    &root_specs,
                    &self.values,
                    &root_specs,
                    &self.values,
                );
            let value = self.values.get(key).unwrap_or(&Value::Null);
            if value.is_null() {
                if visible && entry.effective_required() {
                    return Err("Thiếu custom field bắt buộc.");
                }
                continue;
            }
            if !valid_field_value_in_context(
                &entry.definition,
                value,
                &root_specs,
                &self.values,
                visible,
            ) {
                return Err("Giá trị custom field không đúng kiểu hoặc vượt giới hạn.");
            }
        }
        Ok(())
    }
}
