use super::*;

pub const MAX_TAXONOMY_DESCRIPTION_CHARS: usize = 400;

pub const RESERVED_TERM_KEYS: &[&str] = &[
    "id",
    "kind",
    "name",
    "slug",
    "parent_id",
    "description",
    "sort_order",
    "featured_image_media_id",
    "featured_image",
    "values",
    "seo",
    "locale",
    "translation_group_id",
    "version",
    "created_at",
    "updated_at",
    "url",
    "fields",
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxonomySelectionMode {
    #[default]
    Multiple,
    Single,
}

impl TaxonomySelectionMode {
    pub fn is_default(&self) -> bool {
        *self == Self::Multiple
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TaxonomySettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub singular_label: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "TaxonomySelectionMode::is_default")]
    pub selection_mode: TaxonomySelectionMode,
    #[serde(skip_serializing_if = "crate::field_schema::TermPickerStyle::is_default")]
    pub picker_style: crate::field_schema::TermPickerStyle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_term_id: Option<i64>,
    #[serde(skip_serializing_if = "is_false_bool")]
    pub required_on_publish: bool,
    #[serde(skip_serializing_if = "is_false_bool")]
    pub editor_can_create_terms: bool,
    #[serde(default = "default_true", skip_serializing_if = "is_true_bool")]
    pub show_in_menu: bool,
    #[serde(skip_serializing_if = "is_false_bool")]
    pub show_admin_column: bool,
}

impl Default for TaxonomySettings {
    fn default() -> Self {
        Self {
            singular_label: None,
            description: String::new(),
            selection_mode: TaxonomySelectionMode::Multiple,
            picker_style: crate::field_schema::TermPickerStyle::Auto,
            default_term_id: None,
            required_on_publish: false,
            editor_can_create_terms: false,
            show_in_menu: true,
            show_admin_column: false,
        }
    }
}

impl TaxonomySettings {
    pub fn validate_and_normalize(&mut self) -> Result<(), &'static str> {
        if let Some(raw) = self.singular_label.as_deref() {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                self.singular_label = None;
            } else {
                if !valid_label(trimmed) {
                    return Err("singular_label của taxonomy không hợp lệ (tối đa 100 ký tự).");
                }
                self.singular_label = Some(trimmed.to_owned());
            }
        }
        if self.description.chars().count() > MAX_TAXONOMY_DESCRIPTION_CHARS
            || self
                .description
                .chars()
                .any(|c| c.is_control() && c != '\n')
        {
            return Err("Mô tả taxonomy tối đa 400 ký tự và không chứa ký tự điều khiển.");
        }
        if self.default_term_id.is_some_and(|id| id <= 0) {
            return Err("default_term_id phải là số nguyên dương.");
        }
        Ok(())
    }
}

/// Package-declared taxonomy defaults. Owner, version and provider status are
/// assigned and checked by the canonical registry/storage layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyDeclaration {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub hierarchical: bool,
    #[serde(default = "taxonomy_public_default")]
    pub public: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<TaxonomySettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewrite_base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyEligibilityDeclaration {
    pub content_type: String,
    #[serde(default)]
    pub taxonomies: Vec<String>,
}

pub(crate) fn taxonomy_public_default() -> bool {
    true
}

impl TaxonomyDeclaration {
    pub fn validate(&self) -> Result<(), &'static str> {
        TaxonomyDefinitionInput {
            key: self.key.clone(),
            label: self.label.clone(),
            hierarchical: self.hierarchical,
            public: self.public,
            settings: self.settings.clone(),
            content_types: None,
            rewrite_base: PatchField::Omitted,
        }
        .validate()?;
        validate_provider_rewrite_base(self.rewrite_base.as_deref())
    }
}

/// Canonical site taxonomy descriptor (C2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyDefinition {
    pub key: String,
    pub label: String,
    pub hierarchical: bool,
    pub public: bool,
    #[serde(default)]
    pub settings: TaxonomySettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewrite_base: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_types: Vec<String>,
    pub version: i64,
    pub owner: ResourceOwner,
    pub provider_status: ProviderStatus,
    pub declaration_state: DeclarationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyDefinitionInput {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub hierarchical: bool,
    #[serde(default = "taxonomy_public_default")]
    pub public: bool,
    #[serde(default)]
    pub settings: Option<TaxonomySettings>,
    #[serde(default)]
    pub content_types: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_field",
        skip_serializing_if = "PatchField::is_omitted"
    )]
    pub rewrite_base: PatchField<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyDefinitionPatch {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub hierarchical: Option<bool>,
    #[serde(default)]
    pub public: Option<bool>,
    #[serde(default)]
    pub settings: Option<TaxonomySettings>,
    #[serde(default)]
    pub content_types: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_field",
        skip_serializing_if = "PatchField::is_omitted"
    )]
    pub rewrite_base: PatchField<String>,
}

impl TaxonomyDefinitionPatch {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.label.is_none()
            && self.hierarchical.is_none()
            && self.public.is_none()
            && self.settings.is_none()
            && self.content_types.is_none()
            && self.rewrite_base.is_omitted()
        {
            return Err("Cần ít nhất một thuộc tính taxonomy để cập nhật.");
        }
        if self.label.as_ref().is_some_and(|label| !valid_label(label)) {
            return Err("Tên taxonomy không hợp lệ (tối đa 100 ký tự).");
        }
        if let Some(mut settings) = self.settings.clone() {
            settings.validate_and_normalize()?;
        }
        if let Some(cts) = &self.content_types {
            validate_taxonomy_target_cpts(cts)?;
        }
        if let PatchField::Set(base) = &self.rewrite_base
            && !base.trim().is_empty()
        {
            validate_provider_rewrite_base(Some(base.trim()))?;
        }
        Ok(())
    }
}

pub(crate) fn validate_taxonomy_target_cpts(keys: &[String]) -> Result<(), &'static str> {
    if keys.len() > 64 {
        return Err("Mỗi taxonomy hỗ trợ tối đa 64 loại nội dung.");
    }
    let mut seen = HashSet::new();
    for key in keys {
        if matches!(key.as_str(), "pattern" | "section") || !valid_content_type_key(key) {
            return Err("Khóa loại nội dung gán cho taxonomy không hợp lệ.");
        }
        if !seen.insert(key.as_str()) {
            return Err("Khóa loại nội dung gán cho taxonomy bị trùng lặp.");
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermSeoInput {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub og_image_id: Option<String>,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub noindex: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveTermSchema {
    pub taxonomy: TaxonomyDefinition,
    pub attached_groups: Vec<AttachedFieldGroupSnapshot>,
    pub effective_fields: BTreeMap<String, EffectiveFieldEntry>,
    #[serde(default)]
    pub ui_fields: Vec<crate::field_schema::FieldSpec>,
    pub schema_token: String,
}

impl EffectiveTermSchema {
    pub fn assemble(
        taxonomy: TaxonomyDefinition,
        mut attached_groups: Vec<AttachedFieldGroupSnapshot>,
    ) -> Result<Self, SchemaTransitionError> {
        attached_groups.sort_by(|a, b| a.group_id.cmp(&b.group_id));
        let mut effective_fields: BTreeMap<String, EffectiveFieldEntry> = BTreeMap::new();
        let mut ui_fields = Vec::new();

        for group in &attached_groups {
            if let Err(e) = crate::field_schema::validate_field_specs(
                &group.fields,
                crate::field_schema::FieldContext::Term,
            ) {
                return Err(classify_validation_error(e));
            }
            for field in &group.fields {
                if field.is_ui_only() {
                    ui_fields.push(field.clone());
                    continue;
                }
                if RESERVED_TERM_KEYS.contains(&field.key.as_str()) {
                    return Err(SchemaTransitionError::FieldKeyCollision(format!(
                        "Trường '{}' trong nhóm '{}' trùng với khóa term hệ thống được bảo vệ.",
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

        let all_specs: Vec<crate::field_schema::FieldSpec> = effective_fields
            .values()
            .map(|e| e.definition.clone())
            .collect();
        if let Err(msg) = crate::field_schema::check_schema_compatibility(&[], &all_specs, false) {
            return Err(classify_validation_error(msg));
        }

        Ok(Self {
            taxonomy,
            attached_groups,
            effective_fields,
            ui_fields,
            schema_token: String::new(),
        })
    }

    pub fn canonical_token_payload_json(
        &self,
        active_theme_revision_id: Option<&str>,
        enabled_plugins: &[(String, i64)],
        role_scope: Option<FieldRole>,
    ) -> String {
        let mut sorted_plugins = enabled_plugins.to_vec();
        sorted_plugins.sort_by(|a, b| a.0.cmp(&b.0));

        let visible_groups: Vec<serde_json::Value> = self
            .attached_groups
            .iter()
            .filter(|g| {
                role_scope.is_none_or(|role| {
                    group_allows_role_for_target(
                        &g.location,
                        "taxonomy_term",
                        &self.taxonomy.key,
                        role,
                    )
                })
            })
            .map(|g| {
                let fields: Vec<&crate::field_schema::FieldSpec> = g
                    .fields
                    .iter()
                    .filter(|f| {
                        role_scope.is_none_or(|role| f.is_ui_only() || f.effective_acl().0 <= role)
                    })
                    .collect();
                serde_json::json!({
                    "group_id": g.group_id,
                    "owner": g.owner,
                    "status": g.status,
                    "association_state": g.association_state,
                    "active": g.active,
                    "location": g.location,
                    "presentation": g.presentation,
                    "version": g.version,
                    "fields": fields,
                })
            })
            .collect();

        let payload = serde_json::json!({
            "taxonomy": {
                "key": self.taxonomy.key,
                "label": self.taxonomy.label,
                "hierarchical": self.taxonomy.hierarchical,
                "public": self.taxonomy.public,
                "settings": self.taxonomy.settings,
                "version": self.taxonomy.version,
                "owner": self.taxonomy.owner,
                "provider_status": self.taxonomy.provider_status,
                "declaration_state": self.taxonomy.declaration_state,
            },
            "attached_groups": visible_groups,
            "provider_generations": {
                "active_theme_revision_id": active_theme_revision_id,
                "enabled_plugins": sorted_plugins,
            }
        });
        serde_json::to_string(&payload).expect("canonical term schema token payload serializes")
    }
}

pub fn verify_effective_term_schema_transition(
    old_effective: Option<&EffectiveTermSchema>,
    new_effective: &EffectiveTermSchema,
    has_existing_terms: bool,
) -> Result<(), SchemaTransitionError> {
    let old_specs: Vec<crate::field_schema::FieldSpec> = old_effective
        .map(|old| {
            old.effective_fields
                .values()
                .map(|entry| entry.definition.clone())
                .collect()
        })
        .unwrap_or_default();
    let new_specs: Vec<crate::field_schema::FieldSpec> = new_effective
        .effective_fields
        .values()
        .map(|entry| entry.definition.clone())
        .collect();
    if let Err(msg) =
        crate::field_schema::check_schema_compatibility(&old_specs, &new_specs, has_existing_terms)
    {
        return Err(classify_validation_error(msg));
    }
    Ok(())
}
