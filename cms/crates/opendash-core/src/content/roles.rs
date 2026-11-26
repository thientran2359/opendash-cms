//! Content-type and custom-field schema for the CMS content module (M3 / M7-S09).
//! Identifier rules come from `crate::id`; resource ownership is recorded by
//! wrapping a definition in `registry::OwnedDescriptor<ContentTypeDefinition>`.

use super::*;

pub(super) use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use serde_json::Value;

pub(super) use crate::{
    id::{ScopedResourceId, valid_kebab_segment_with_limit, valid_snake_key},
    registry::{DeclarationState, OwnedDescriptor, ProviderStatus, ResourceOwner},
};

/// Valid identifiers start with a lowercase ASCII letter and contain only
/// lowercase letters, digits, or underscores (`1..=64` chars).
pub fn valid_content_type_key(value: &str) -> bool {
    valid_snake_key(value)
}

/// Reserved entry-level keys that cannot be used as custom field keys in extension field groups
/// (or as new system collisions). Note: `body` and `featured_image` are allowed on built-in `post`/`page`.
pub const RESERVED_ENTRY_KEYS: &[&str] = &[
    "id",
    "content_type_key",
    "type_key",
    "slug",
    "title",
    "publication_status",
    "version",
    "created_at",
    "updated_at",
    "term_ids",
    "custom_path",
    "seo",
    "locale",
    "translation_group_id",
];

/// Role hierarchy for field and panel access control (`Editor < Admin`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum FieldRole {
    #[default]
    Editor,
    Admin,
}

pub type Role = FieldRole;

impl FieldRole {
    pub const fn default_editor() -> Self {
        Self::Editor
    }
}

/// Controls whether a custom field is exposed on public routes, templates, preview, and public media.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum PublicExposure {
    #[default]
    Public,
    Private,
}

impl PublicExposure {
    pub const fn default_public() -> Self {
        Self::Public
    }

    pub const fn default_private() -> Self {
        Self::Private
    }
}

/// Distinguishes base CPT fields from extension field group fields during input normalization (`270-R7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldScopeKind {
    /// Fields in `content_types.definition_json.fields` (missing `public_exposure` -> `Public`).
    BaseCptField,
    /// Fields in `content_field_groups.fields_json` via User API or Theme/Plugin manifest (missing `public_exposure` -> `Private`).
    ExtensionGroupField,
}

/// Wire/manifest input DTO for field definitions (`270-R7`).
/// Keeps `public_exposure: Option<PublicExposure>` so `None` (missing) is normalized
/// according to `FieldScopeKind` (`BaseCptField` -> `Public`, `ExtensionGroupField` -> `Private`),
/// while explicit `Some(Public | Private)` is always preserved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDefinitionInput {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: ContentFieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SelectOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
    #[serde(default)]
    pub min_view_role: Option<FieldRole>,
    #[serde(default)]
    pub min_edit_role: Option<FieldRole>,
    #[serde(default)]
    pub public_exposure: Option<PublicExposure>,
    #[serde(default)]
    pub declaration_state: Option<DeclarationState>,
}

impl FieldDefinitionInput {
    pub fn into_canonical(
        self,
        scope: FieldScopeKind,
    ) -> Result<ContentFieldDefinition, &'static str> {
        let min_view_role = self.min_view_role.unwrap_or(FieldRole::Editor);
        let min_edit_role = self.min_edit_role.unwrap_or(FieldRole::Editor);
        if min_edit_role < min_view_role {
            return Err("invalid_field_acl: min_edit_role không được thấp hơn min_view_role.");
        }
        let public_exposure = match (self.public_exposure, scope) {
            (Some(explicit), _) => explicit,
            (None, FieldScopeKind::BaseCptField) => PublicExposure::Public,
            (None, FieldScopeKind::ExtensionGroupField) => PublicExposure::Private,
        };
        if public_exposure == PublicExposure::Public && min_view_role != FieldRole::Editor {
            return Err(
                "invalid_field_acl: Trường có min_view_role = admin không được đặt public_exposure = public.",
            );
        }
        if scope == FieldScopeKind::ExtensionGroupField
            && (RESERVED_ENTRY_KEYS.contains(&self.key.as_str()) || self.key == "status")
        {
            return Err(
                "field_key_collision: Khóa custom field trùng với trường hệ thống được bảo vệ.",
            );
        }
        if scope == FieldScopeKind::BaseCptField && RESERVED_ENTRY_KEYS.contains(&self.key.as_str())
        {
            return Err(
                "field_key_collision: Khóa custom field trùng với trường hệ thống được bảo vệ.",
            );
        }
        let def = ContentFieldDefinition {
            key: self.key,
            label: self.label,
            field_type: self.field_type,
            required: self.required,
            options: self.options,
            default_value: self.default_value,
            min_view_role,
            min_edit_role,
            public_exposure,
            declaration_state: self.declaration_state.unwrap_or(DeclarationState::Declared),
        };
        def.validate()?;
        Ok(def)
    }
}

/// Canonical persisted field definition stored in `content_types.definition_json`
/// and `content_field_groups.fields_json`.
/// Always serializes `min_view_role`, `min_edit_role`, `public_exposure`, and `declaration_state`
/// explicitly so DB reads after server restart never guess scope defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentFieldDefinition {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: ContentFieldType,
    #[serde(default)]
    pub required: bool,
    /// Used only for `select`. Other field kinds must not declare options.
    #[serde(default)]
    pub options: Vec<SelectOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
    #[serde(default = "FieldRole::default_editor")]
    pub min_view_role: FieldRole,
    #[serde(default = "FieldRole::default_editor")]
    pub min_edit_role: FieldRole,
    #[serde(default = "PublicExposure::default_public")]
    pub public_exposure: PublicExposure,
    #[serde(default = "default_declaration_declared")]
    pub declaration_state: DeclarationState,
}

impl Default for ContentFieldDefinition {
    fn default() -> Self {
        Self {
            key: String::new(),
            label: String::new(),
            field_type: ContentFieldType::ShortText,
            required: false,
            options: Vec::new(),
            default_value: None,
            min_view_role: FieldRole::Editor,
            min_edit_role: FieldRole::Editor,
            public_exposure: PublicExposure::Public,
            declaration_state: DeclarationState::Declared,
        }
    }
}

impl ContentFieldDefinition {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_snake_key(&self.key) {
            return Err("Khóa custom field không hợp lệ.");
        }
        if !valid_label(&self.label) {
            return Err("Tên custom field không hợp lệ.");
        }
        if self.min_edit_role < self.min_view_role {
            return Err("invalid_field_acl: min_edit_role không được thấp hơn min_view_role.");
        }
        if self.public_exposure == PublicExposure::Public && self.min_view_role != FieldRole::Editor
        {
            return Err(
                "invalid_field_acl: Trường có min_view_role = admin không được đặt public_exposure = public.",
            );
        }
        if self.field_type != ContentFieldType::Select {
            if !self.options.is_empty() {
                return Err("Chỉ field lựa chọn mới được khai báo options.");
            }
        } else {
            if self.options.is_empty() || self.options.len() > 100 {
                return Err("Field lựa chọn cần từ 1 đến 100 phương án.");
            }
            let mut values = HashSet::new();
            for option in &self.options {
                if option.value.trim() != option.value
                    || option.value.is_empty()
                    || option.value.chars().count() > 128
                    || option.value.chars().any(char::is_control)
                    || !valid_label(&option.label)
                    || !values.insert(option.value.as_str())
                {
                    return Err("Phương án lựa chọn không hợp lệ hoặc trùng giá trị.");
                }
            }
        }
        if let Some(ref def_val) = self.default_value {
            if !valid_default_value(self, def_val) {
                return Err(
                    "invalid_field_default: Giá trị default_value không hợp lệ hoặc bị cấm cho kiểu trường này.",
                );
            }
        }
        if self.required
            && self.declaration_state == DeclarationState::Declared
            && self.min_edit_role == FieldRole::Admin
            && self.default_value.is_none()
        {
            return Err(
                "admin_required_field_missing_default: Trường bắt buộc có min_edit_role = admin phải khai báo default_value hợp lệ.",
            );
        }
        Ok(())
    }

    pub fn to_input(&self) -> FieldDefinitionInput {
        FieldDefinitionInput {
            key: self.key.clone(),
            label: self.label.clone(),
            field_type: self.field_type,
            required: self.required,
            options: self.options.clone(),
            default_value: self.default_value.clone(),
            min_view_role: Some(self.min_view_role),
            min_edit_role: Some(self.min_edit_role),
            public_exposure: Some(self.public_exposure),
            declaration_state: Some(self.declaration_state),
        }
    }
}

/// Publication semantics are server-owned; the Admin UI must not infer them from
/// the content-type key. Legacy custom types use the immediate policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentPublication {
    Immediate,
    DraftPublish,
}

impl ContentTypeDefinitionInput {
    pub fn into_canonical(self) -> Result<ContentTypeDefinition, &'static str> {
        if !valid_content_type_key(&self.key) {
            return Err("Khóa loại nội dung không hợp lệ.");
        }
        if !valid_label(&self.label) {
            return Err("Tên loại nội dung không hợp lệ.");
        }
        if self.fields.len() > 64 {
            return Err("Mỗi loại nội dung hỗ trợ tối đa 64 custom fields.");
        }
        let mut canonical_fields = Vec::with_capacity(self.fields.len());
        let mut keys = HashSet::new();
        for f in self.fields {
            let cf = f.into_canonical(FieldScopeKind::BaseCptField)?;
            if !keys.insert(cf.key.clone()) {
                return Err("field_key_collision: Trùng khóa custom field trong loại nội dung.");
            }
            canonical_fields.push(cf);
        }
        let mut settings = self
            .settings
            .unwrap_or_else(|| ContentTypeSettings::default_for_type_key(&self.key));
        settings.validate_and_normalize()?;
        let def = ContentTypeDefinition {
            key: self.key,
            label: self.label,
            fields: canonical_fields,
            settings,
        };
        def.validate()?;
        Ok(def)
    }
}

impl ContentTypeDefinition {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_content_type_key(&self.key) {
            return Err("Khóa loại nội dung không hợp lệ.");
        }
        if self.key == "section" {
            return Err("Section là khóa Core đã được thay thế bằng Pattern.");
        }
        if !valid_label(&self.label) {
            return Err("Tên loại nội dung không hợp lệ.");
        }
        let mut settings = self.settings.clone();
        settings.validate_and_normalize()?;
        if settings != self.settings {
            return Err("supports chưa được chuẩn hóa.");
        }
        if matches!(self.key.as_str(), "post" | "page")
            && self.settings.publication != ContentPublication::DraftPublish
        {
            return Err("post/page phải sử dụng cơ chế Lưu nháp và Xuất bản.");
        }
        if self.key == "pattern" && self.settings != ContentTypeSettings::private_pattern() {
            return Err("Pattern phải luôn riêng tư và chỉ hỗ trợ Builder.");
        }
        if self.fields.len() > 64 {
            return Err("Mỗi loại nội dung hỗ trợ tối đa 64 custom fields.");
        }
        let is_builtin_post_or_page = matches!(self.key.as_str(), "post" | "page");
        let mut keys = HashSet::new();
        for field in &self.fields {
            if !is_builtin_post_or_page && RESERVED_ENTRY_KEYS.contains(&field.key.as_str()) {
                return Err(
                    "field_key_collision: Khóa custom field trùng với trường hệ thống được bảo vệ.",
                );
            }
            field.validate()?;
            if !keys.insert(field.key.as_str()) {
                return Err("field_key_collision: Trùng khóa custom field trong loại nội dung.");
            }
        }
        Ok(())
    }
}

impl ManifestFieldGroupInput {
    pub fn validate_and_normalize(&self) -> Result<Vec<ContentFieldDefinition>, &'static str> {
        if self.local_key.starts_with("layout:") || self.local_key.starts_with("core:") {
            return Err(
                "reserved_panel_namespace: Không được khai báo tiền tố hệ thống 'core:' hoặc 'layout:'.",
            );
        }
        if !valid_snake_key(&self.local_key) {
            return Err("local_key của field_group phải là snake_case hợp lệ (1..=64 ký tự).");
        }
        if !valid_label(&self.title) {
            return Err("Tiêu đề của field_group không hợp lệ.");
        }
        if self.description.chars().count() > 500 || self.description.chars().any(char::is_control)
        {
            return Err("Mô tả của field_group vượt quá giới hạn hoặc chứa ký tự điều khiển.");
        }
        if self.target_types.is_empty() || self.target_types.len() > 16 {
            return Err("target_types của field_group phải có từ 1 đến 16 loại nội dung.");
        }
        let mut seen_targets = HashSet::new();
        for t in &self.target_types {
            if !valid_content_type_key(t) || !seen_targets.insert(t.as_str()) {
                return Err("target_types trong field_group không hợp lệ hoặc bị trùng lặp.");
            }
        }
        if !self.default_region.is_valid_for_field_group() {
            return Err(
                "invalid_panel_region: default_region của field_group không được đặt vào header_bar.",
            );
        }
        if !(-1000..=1000).contains(&self.default_order) {
            return Err("default_order của field_group phải nằm trong khoảng [-1000..=1000].");
        }
        if self.fields.is_empty() || self.fields.len() > 64 {
            return Err("Mỗi field_group phải khai báo từ 1 đến 64 custom fields.");
        }
        let mut canonical_fields = Vec::with_capacity(self.fields.len());
        let mut seen_keys = HashSet::new();
        for f in &self.fields {
            let cf = f
                .clone()
                .into_canonical(FieldScopeKind::ExtensionGroupField)?;
            if !seen_keys.insert(cf.key.clone()) {
                return Err("field_key_collision: Trùng khóa custom field trong cùng field_group.");
            }
            canonical_fields.push(cf);
        }
        Ok(canonical_fields)
    }
}

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
        let base_source_id = format!("base:{}", base.descriptor.key);

        for field in &base.descriptor.fields {
            if !is_builtin && RESERVED_ENTRY_KEYS.contains(&field.key.as_str()) {
                return Err(SchemaTransitionError::FieldKeyCollision(format!(
                    "Trường '{}' trùng với khóa hệ thống được bảo vệ.",
                    field.key
                )));
            }
            if let Err(e) = field.validate() {
                return Err(classify_validation_error(e));
            }
            let effective_status = if base.declaration_state == DeclarationState::Withdrawn
                || field.declaration_state == DeclarationState::Withdrawn
            {
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
            for field in &group.fields {
                if RESERVED_ENTRY_KEYS.contains(&field.key.as_str()) || field.key == "status" {
                    return Err(SchemaTransitionError::FieldKeyCollision(format!(
                        "Trường '{}' trong nhóm '{}' trùng với khóa hệ thống được bảo vệ.",
                        field.key, group.group_id
                    )));
                }
                if let Err(e) = field.validate() {
                    return Err(classify_validation_error(e));
                }
                if let Some(existing) = effective_fields.get(&field.key) {
                    return Err(SchemaTransitionError::FieldKeyCollision(format!(
                        "Khóa trường '{}' trong nhóm '{}' xung đột với trường đã tồn tại ở '{}'.",
                        field.key, group.group_id, existing.source_id
                    )));
                }
                let effective_status = if group.status == FieldGroupStatus::Withdrawn
                    || group.association_state == DeclarationState::Withdrawn
                    || field.declaration_state == DeclarationState::Withdrawn
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
            effective_fields.insert(
                "featured_image".into(),
                EffectiveFieldEntry {
                    definition: ContentFieldDefinition {
                        key: "featured_image".into(),
                        label: "Ảnh đại diện".into(),
                        field_type: ContentFieldType::Image,
                        required: false,
                        options: Vec::new(),
                        min_view_role: FieldRole::Editor,
                        min_edit_role: FieldRole::Editor,
                        public_exposure: PublicExposure::Public,
                        default_value: None,
                        declaration_state: DeclarationState::Declared,
                    },
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
                if existing_body.definition.field_type != ContentFieldType::RichText {
                    return Err(SchemaTransitionError::IncompatibleSchemaChange(format!(
                        "Trường 'body' trên loại nội dung '{}' phải có kiểu rich_text khi bật ContentSupport::Builder.",
                        base.descriptor.key
                    )));
                }
            } else {
                effective_fields.insert(
                    "body".into(),
                    EffectiveFieldEntry {
                        definition: ContentFieldDefinition {
                            key: "body".into(),
                            label: "Nội dung".into(),
                            field_type: ContentFieldType::RichText,
                            required: false,
                            options: Vec::new(),
                            min_view_role: FieldRole::Editor,
                            min_edit_role: FieldRole::Editor,
                            public_exposure: PublicExposure::Public,
                            default_value: None,
                            declaration_state: DeclarationState::Declared,
                        },
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
        for (old_key, old_entry) in &old.effective_fields {
            let Some(new_entry) = new_effective.effective_fields.get(old_key) else {
                // Allow removing the synthetic core-injected `body` field when a custom CPT disables Builder.
                if old_key == "body"
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
                if has_existing_entries {
                    return Err(SchemaTransitionError::CannotRemoveFieldWithData(format!(
                        "Không thể xóa hẳn trường '{old_key}' khi đã có dữ liệu; hãy đặt declaration_state = withdrawn."
                    )));
                }
                continue;
            };

            if old_entry.definition.field_type != new_entry.definition.field_type {
                return Err(SchemaTransitionError::IncompatibleSchemaChange(format!(
                    "Không thể đổi kiểu dữ liệu (type) của trường '{old_key}' đã tồn tại."
                )));
            }

            if old_entry.definition.field_type == ContentFieldType::Select && has_existing_entries {
                let new_opts: HashSet<&str> = new_entry
                    .definition
                    .options
                    .iter()
                    .map(|o| o.value.as_str())
                    .collect();
                for old_opt in &old_entry.definition.options {
                    if !new_opts.contains(old_opt.value.as_str()) {
                        return Err(SchemaTransitionError::IncompatibleSchemaChange(format!(
                            "Không thể xóa phương án lựa chọn '{}' của trường '{old_key}' khi đã có dữ liệu.",
                            old_opt.value
                        )));
                    }
                }
            }

            let old_req = old_entry.effective_required();
            let new_req = new_entry.effective_required();
            if !old_req && new_req && has_existing_entries {
                return Err(SchemaTransitionError::IncompatibleSchemaChange(format!(
                    "Không thể đổi trường '{old_key}' sang bắt buộc (required) khi đã có dữ liệu."
                )));
            }
        }

        for (new_key, new_entry) in &new_effective.effective_fields {
            if !old.effective_fields.contains_key(new_key)
                && new_entry.effective_required()
                && has_existing_entries
            {
                return Err(SchemaTransitionError::IncompatibleSchemaChange(format!(
                    "Không thể thêm trường bắt buộc (required) mới '{new_key}' khi đã có dữ liệu."
                )));
            }
        }
    }

    // Rule COMPAT-4 (270-R8): Unconditional Editor create-satisfiability & ACL/default checks on every transition
    for (key, entry) in &new_effective.effective_fields {
        if entry.definition.min_edit_role < entry.definition.min_view_role {
            return Err(SchemaTransitionError::InvalidFieldAcl(format!(
                "Trường '{key}' có min_edit_role thấp hơn min_view_role."
            )));
        }
        if entry.definition.public_exposure == PublicExposure::Public
            && entry.definition.min_view_role != FieldRole::Editor
        {
            return Err(SchemaTransitionError::InvalidFieldAcl(format!(
                "Trường '{key}' có min_view_role = admin không được đặt public_exposure = public."
            )));
        }
        if let Some(ref def_val) = entry.definition.default_value {
            if !valid_default_value(&entry.definition, def_val) {
                return Err(SchemaTransitionError::InvalidFieldDefault(format!(
                    "Trường '{key}' có default_value không hợp lệ hoặc bị cấm."
                )));
            }
        }
        if entry.effective_required()
            && entry.definition.min_edit_role == FieldRole::Admin
            && entry.definition.default_value.is_none()
        {
            return Err(SchemaTransitionError::AdminRequiredFieldMissingDefault(
                format!(
                    "Trường bắt buộc '{key}' có min_edit_role = admin phải khai báo default_value hợp lệ để Editor có thể tạo mới bản ghi."
                ),
            ));
        }
    }

    Ok(())
}

impl CntDeclarations {
    pub fn is_empty(&self) -> bool {
        self.content_types.is_empty()
            && self.taxonomy_definitions.is_empty()
            && self.taxonomy_eligibility.is_empty()
            && self.field_groups.is_empty()
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.taxonomy_definitions.len() > 64 {
            return Err("Mỗi gói hỗ trợ tối đa 64 taxonomy_definitions.");
        }
        let mut seen_taxonomies = HashSet::new();
        for taxonomy in &self.taxonomy_definitions {
            taxonomy.validate()?;
            if !seen_taxonomies.insert(taxonomy.key.as_str()) {
                return Err("Khóa taxonomy bị trùng lặp trong declarations.");
            }
        }
        let mut seen_keys = HashSet::new();
        for ct in &self.content_types {
            if !seen_keys.insert(ct.key.as_str()) {
                return Err("Khóa content_type bị trùng lặp trong declarations.");
            }
            if matches!(ct.key.as_str(), "post" | "page" | "pattern" | "section")
                && ct.rewrite_base.is_some()
            {
                return Err("Core Post/Page/Pattern không hỗ trợ rewrite_base riêng của provider.");
            }
            validate_provider_rewrite_base(ct.rewrite_base.as_deref())?;
            let mut settings = ct.settings.clone().unwrap_or_default();
            settings.validate_and_normalize()?;
            let def = ContentTypeDefinition {
                key: ct.key.clone(),
                label: ct.label.clone(),
                fields: ct.fields.clone(),
                settings,
            };
            def.validate()?;
            validate_provider_taxonomy_keys(&ct.taxonomies)?;
        }
        let mut seen_eligibility = HashSet::new();
        for elig in &self.taxonomy_eligibility {
            if !valid_content_type_key(&elig.content_type) {
                return Err("Khóa loại nội dung trong taxonomy_eligibility không hợp lệ.");
            }
            validate_provider_taxonomy_keys(&elig.taxonomies)?;
            for tax in &elig.taxonomies {
                if !seen_eligibility.insert((elig.content_type.as_str(), tax.as_str())) {
                    return Err("Trùng association content_type/taxonomy trong declarations.");
                }
            }
        }
        if self.field_groups.len() > 16 {
            return Err("Mỗi gói hỗ trợ tối đa 16 field_groups trong declarations.");
        }
        let mut seen_groups = HashSet::new();
        for fg in &self.field_groups {
            if !seen_groups.insert(fg.local_key.as_str()) {
                return Err("duplicate_panel_id: Trùng local_key trong danh sách field_groups.");
            }
            fg.validate_and_normalize()?;
        }
        Ok(())
    }
}

/// References may point to core, site-owned, or same-provider taxonomies;
/// canonical storage verifies existence, ownership, and provider activity.
pub(super) fn validate_provider_taxonomy_keys(keys: &[String]) -> Result<(), &'static str> {
    if keys.len() > 64 {
        return Err("Mỗi content type hỗ trợ tối đa 64 taxonomy associations.");
    }
    let mut seen = HashSet::new();
    for key in keys {
        if !valid_content_type_key(key) {
            return Err("Khóa taxonomy trong eligibility không hợp lệ.");
        }
        if !seen.insert(key.as_str()) {
            return Err("Khóa taxonomy trùng lặp trong eligibility.");
        }
    }
    Ok(())
}

impl TaxonomyDefinitionInput {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_content_type_key(&self.key) {
            return Err("Khóa taxonomy không hợp lệ.");
        }
        if matches!(
            self.key.as_str(),
            "category" | "tag" | "post" | "page" | "pattern" | "section"
        ) || RESERVED_ENTRY_KEYS.contains(&self.key.as_str())
        {
            return Err("Khóa taxonomy thuộc namespace Core được bảo vệ.");
        }
        if !valid_label(&self.label) {
            return Err("Tên taxonomy không hợp lệ (tối đa 100 ký tự).");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelDefinition {
    pub panel_id: String,
    pub owner: ResourceOwner,
    pub title: String,
    pub kind: PanelKind,
    pub applies_to: Vec<String>,
    pub default_region: EditorRegion,
    pub allowed_regions: Vec<EditorRegion>,
    pub default_order: i32,
    pub min_view_role: FieldRole,
    pub min_edit_role: FieldRole,
    pub collapsible: bool,
    pub required_core: bool,
    pub effective_status: EffectiveFieldStatus,
}

/// Wire DTO for `editor_panels` (`Action` panels) in `plugin.json` (`manifest_schema: 2`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestActionPanelInput {
    pub local_key: String,
    pub title: String,
    pub action_id: String,
    pub button_label: String,
    #[serde(default)]
    pub confirm_message: Option<String>,
    #[serde(default)]
    pub public_effect: bool,
    pub applies_to: Vec<String>,
    #[serde(default = "EditorRegion::default_sidebar")]
    pub default_region: EditorRegion,
    #[serde(default = "default_action_allowed_regions")]
    pub allowed_regions: Vec<EditorRegion>,
    #[serde(default = "default_panel_order_80")]
    pub default_order: i32,
    #[serde(default)]
    pub min_view_role: Option<FieldRole>,
    #[serde(default)]
    pub min_edit_role: Option<FieldRole>,
    #[serde(default = "default_true")]
    pub collapsible: bool,
}

impl ManifestActionPanelInput {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.local_key.starts_with("core:") || self.local_key.starts_with("layout:") {
            return Err(
                "reserved_panel_namespace: Không được khai báo tiền tố 'core:' hoặc 'layout:' trong editor_panels.",
            );
        }
        if !valid_snake_key(&self.local_key) {
            return Err("local_key của editor_panel phải là snake_case hợp lệ (1..=64 ký tự).");
        }
        if !valid_label(&self.title) || !valid_label(&self.button_label) {
            return Err("Tiêu đề hoặc nhãn nút của editor_panel không hợp lệ.");
        }
        if let Some(msg) = &self.confirm_message {
            if msg.chars().count() > 300 || msg.chars().any(char::is_control) {
                return Err("confirm_message của editor_panel không hợp lệ.");
            }
        }
        if self.applies_to.is_empty() || self.applies_to.len() > 16 {
            return Err("applies_to của editor_panel phải có từ 1 đến 16 loại nội dung.");
        }
        let mut seen_types = HashSet::new();
        for t in &self.applies_to {
            if !valid_content_type_key(t) || !seen_types.insert(t.as_str()) {
                return Err("applies_to của editor_panel không hợp lệ hoặc bị trùng lặp.");
            }
        }
        if self.allowed_regions.is_empty() {
            return Err("invalid_panel_region: allowed_regions không được rỗng.");
        }
        for r in &self.allowed_regions {
            if !matches!(
                r,
                EditorRegion::MainTop | EditorRegion::MainBottom | EditorRegion::Sidebar
            ) {
                return Err(
                    "invalid_panel_region: Action panel chỉ được phép đặt trong main_top, main_bottom hoặc sidebar.",
                );
            }
        }
        if !self.allowed_regions.contains(&self.default_region) {
            return Err("invalid_panel_region: default_region phải nằm trong allowed_regions.");
        }
        if !(-1000..=1000).contains(&self.default_order) {
            return Err("default_order của editor_panel phải nằm trong [-1000..=1000].");
        }
        let min_view = self.min_view_role.unwrap_or(FieldRole::Editor);
        let min_edit = self.min_edit_role.unwrap_or(FieldRole::Editor);
        if min_edit < min_view {
            return Err("invalid_panel_acl: min_edit_role không được thấp hơn min_view_role.");
        }
        Ok(())
    }
}

pub fn build_core_and_group_panels(
    effective: &EffectiveContentSchema,
    has_taxonomies: bool,
    plugin_action_panels: &[PanelDefinition],
) -> Vec<PanelDefinition> {
    let type_key = &effective.base.descriptor.key;
    let use_body_panel = has_core_body_panel(effective);
    let mut panels = Vec::new();
    let all_regions = default_all_content_regions();

    panels.push(PanelDefinition {
        panel_id: "core:header_bar".into(),
        owner: ResourceOwner::Core,
        title: "Thanh tiêu đề".into(),
        kind: PanelKind::CoreBuiltIn {
            core_key: "header_bar".into(),
        },
        applies_to: vec![type_key.clone()],
        default_region: EditorRegion::HeaderBar,
        allowed_regions: vec![EditorRegion::HeaderBar],
        default_order: 0,
        min_view_role: FieldRole::Editor,
        min_edit_role: FieldRole::Editor,
        collapsible: false,
        required_core: true,
        effective_status: EffectiveFieldStatus::Active,
    });

    panels.push(PanelDefinition {
        panel_id: "core:title_slug".into(),
        owner: ResourceOwner::Core,
        title: "Tiêu đề & Đường dẫn".into(),
        kind: PanelKind::CoreBuiltIn {
            core_key: "title_slug".into(),
        },
        applies_to: vec![type_key.clone()],
        default_region: EditorRegion::MainPrimary,
        allowed_regions: all_regions.clone(),
        default_order: 10,
        min_view_role: FieldRole::Editor,
        min_edit_role: FieldRole::Editor,
        collapsible: false,
        required_core: true,
        effective_status: EffectiveFieldStatus::Active,
    });

    if use_body_panel {
        panels.push(PanelDefinition {
            panel_id: "core:body_editor".into(),
            owner: ResourceOwner::Core,
            title: "Nội dung chính".into(),
            kind: PanelKind::CoreBuiltIn {
                core_key: "body_editor".into(),
            },
            applies_to: vec![type_key.clone()],
            default_region: EditorRegion::MainPrimary,
            allowed_regions: all_regions.clone(),
            default_order: 20,
            min_view_role: FieldRole::Editor,
            min_edit_role: FieldRole::Editor,
            collapsible: false,
            required_core: false,
            effective_status: EffectiveFieldStatus::Active,
        });
    }

    panels.push(PanelDefinition {
        panel_id: "core:base_fields".into(),
        owner: ResourceOwner::Core,
        title: "Trường cơ bản".into(),
        kind: PanelKind::CoreBuiltIn {
            core_key: "base_fields".into(),
        },
        applies_to: vec![type_key.clone()],
        default_region: EditorRegion::MainPrimary,
        allowed_regions: all_regions.clone(),
        default_order: 30,
        min_view_role: FieldRole::Editor,
        min_edit_role: FieldRole::Editor,
        collapsible: true,
        required_core: false,
        effective_status: EffectiveFieldStatus::Active,
    });

    panels.push(PanelDefinition {
        panel_id: "core:seo".into(),
        owner: ResourceOwner::Core,
        title: "Cấu hình SEO".into(),
        kind: PanelKind::CoreBuiltIn {
            core_key: "seo".into(),
        },
        applies_to: vec![type_key.clone()],
        default_region: EditorRegion::MainBottom,
        allowed_regions: all_regions.clone(),
        default_order: 90,
        min_view_role: FieldRole::Editor,
        min_edit_role: FieldRole::Editor,
        collapsible: true,
        required_core: false,
        effective_status: EffectiveFieldStatus::Active,
    });

    panels.push(PanelDefinition {
        panel_id: "core:publish_box".into(),
        owner: ResourceOwner::Core,
        title: "Xuất bản & Trạng thái".into(),
        kind: PanelKind::CoreBuiltIn {
            core_key: "publish_box".into(),
        },
        applies_to: vec![type_key.clone()],
        default_region: EditorRegion::Sidebar,
        allowed_regions: all_regions.clone(),
        default_order: 10,
        min_view_role: FieldRole::Editor,
        min_edit_role: FieldRole::Editor,
        collapsible: false,
        required_core: true,
        effective_status: EffectiveFieldStatus::Active,
    });

    if type_key == "page" {
        panels.push(PanelDefinition {
            panel_id: "core:page_attributes".into(),
            owner: ResourceOwner::Core,
            title: "Thuộc tính trang".into(),
            kind: PanelKind::CoreBuiltIn {
                core_key: "page_attributes".into(),
            },
            applies_to: vec![type_key.clone()],
            default_region: EditorRegion::Sidebar,
            allowed_regions: all_regions.clone(),
            default_order: 20,
            min_view_role: FieldRole::Editor,
            min_edit_role: FieldRole::Editor,
            collapsible: true,
            required_core: false,
            effective_status: EffectiveFieldStatus::Active,
        });
    }

    if !matches!(type_key.as_str(), "section" | "pattern")
        && effective.effective_fields.contains_key("featured_image")
    {
        panels.push(PanelDefinition {
            panel_id: "core:featured_image".into(),
            owner: ResourceOwner::Core,
            title: "Ảnh đại diện".into(),
            kind: PanelKind::CoreBuiltIn {
                core_key: "featured_image".into(),
            },
            applies_to: vec![type_key.clone()],
            default_region: EditorRegion::Sidebar,
            allowed_regions: all_regions.clone(),
            default_order: 30,
            min_view_role: FieldRole::Editor,
            min_edit_role: FieldRole::Editor,
            collapsible: true,
            required_core: false,
            effective_status: EffectiveFieldStatus::Active,
        });
    }

    if type_key == "post" || has_taxonomies {
        panels.push(PanelDefinition {
            panel_id: "core:taxonomies".into(),
            owner: ResourceOwner::Core,
            title: "Phân loại".into(),
            kind: PanelKind::CoreBuiltIn {
                core_key: "taxonomies".into(),
            },
            applies_to: vec![type_key.clone()],
            default_region: EditorRegion::Sidebar,
            allowed_regions: all_regions.clone(),
            default_order: 40,
            min_view_role: FieldRole::Editor,
            min_edit_role: FieldRole::Editor,
            collapsible: true,
            required_core: false,
            effective_status: EffectiveFieldStatus::Active,
        });
    }

    for group in &effective.attached_groups {
        let Ok(fg_id) = ScopedResourceId::parse(&group.group_id) else {
            continue;
        };
        let min_view_role = group
            .fields
            .iter()
            .map(|f| f.min_view_role)
            .min()
            .unwrap_or(FieldRole::Editor);
        let min_edit_role = group
            .fields
            .iter()
            .map(|f| f.min_edit_role)
            .min()
            .unwrap_or(FieldRole::Editor);
        let effective_status = if group.status == FieldGroupStatus::Withdrawn
            || group.association_state == DeclarationState::Withdrawn
        {
            EffectiveFieldStatus::Withdrawn
        } else if group.status == FieldGroupStatus::ProviderInactive {
            EffectiveFieldStatus::ProviderInactive
        } else {
            EffectiveFieldStatus::Active
        };
        panels.push(PanelDefinition {
            panel_id: group.group_id.clone(),
            owner: group.owner.clone(),
            title: group.title.clone(),
            kind: PanelKind::ContentFields {
                field_group_id: fg_id,
            },
            applies_to: vec![type_key.clone()],
            default_region: group.default_region,
            allowed_regions: all_regions.clone(),
            default_order: group.default_order,
            min_view_role,
            min_edit_role,
            collapsible: group.collapsible,
            required_core: false,
            effective_status,
        });
    }

    for ap in plugin_action_panels {
        if ap.applies_to.iter().any(|t| t == type_key) {
            panels.push(ap.clone());
        }
    }

    panels
}

/// Validates an `EditorLayoutConfigV1` against `Rule LAY-1`..`Rule LAY-6` and `Rule COMPAT-4`,
/// auto-appending any newly registered panels into their `default_region` and returning
/// the deterministically sorted layout alongside the complete `PanelDefinition` list.
pub fn resolve_and_validate_editor_layout(
    effective: &EffectiveContentSchema,
    base_panels: Vec<PanelDefinition>,
    raw_layout: Option<EditorLayoutConfigV1>,
    layout_version: u64,
) -> Result<(EditorLayoutConfigV1, Vec<PanelDefinition>), LayoutValidationError> {
    let type_key = &effective.base.descriptor.key;
    let eligible_base = eligible_base_field_keys(effective);
    let eligible_set: HashSet<&str> = eligible_base.iter().map(String::as_str).collect();

    let mut layout = raw_layout.unwrap_or(EditorLayoutConfigV1 {
        schema_version: 1,
        type_key: type_key.clone(),
        version: layout_version,
        base_field_panels: Vec::new(),
        regions: EditorRegionsConfig::default(),
    });

    if layout.schema_version != 1 {
        return Err(LayoutValidationError::new(
            "unsupported_layout_schema_version",
            "schema_version của EditorLayoutConfigV1 phải bằng 1.",
        ));
    }
    if layout.type_key != *type_key {
        return Err(LayoutValidationError::new(
            "mismatched_type_key",
            "type_key trong EditorLayoutConfigV1 không khớp với đường dẫn URL.",
        ));
    }
    if layout.base_field_panels.len() > 16 {
        return Err(LayoutValidationError::new(
            "layout_quota_exceeded",
            "Mỗi loại nội dung hỗ trợ tối đa 16 base_field_panels.",
        ));
    }

    let total_placements = layout.regions.main_top.len()
        + layout.regions.main_primary.len()
        + layout.regions.main_bottom.len()
        + layout.regions.sidebar.len();
    if total_placements > 64 {
        return Err(LayoutValidationError::new(
            "layout_quota_exceeded",
            "Tổng số panel placement trên 4 vùng vượt quá giới hạn 64.",
        ));
    }

    // Rule LAY-2: Build BaseFieldsSlice PanelDefinitions & check base_field_panels partitioning
    let mut all_panels = base_panels;
    let mut seen_slice_ids = HashSet::new();
    let mut assigned_base_fields = HashSet::new();
    let mut slice_field_map: HashMap<String, Vec<String>> = HashMap::new();

    for slice in &layout.base_field_panels {
        let Some(slice_key) = crate::id::valid_layout_slice_panel_id(&slice.panel_id) else {
            return Err(LayoutValidationError::new(
                "invalid_or_duplicate_slice_panel_id",
                format!(
                    "panel_id '{}' phải có dạng 'layout:<snake_key>'.",
                    slice.panel_id
                ),
            ));
        };
        if !seen_slice_ids.insert(slice.panel_id.as_str()) {
            return Err(LayoutValidationError::new(
                "invalid_or_duplicate_slice_panel_id",
                format!("Trùng lặp base_field_panels panel_id '{}'.", slice.panel_id),
            ));
        }
        if !valid_label(&slice.title) {
            return Err(LayoutValidationError::new(
                "invalid_panel_title",
                format!("Tiêu đề panel '{}' không hợp lệ.", slice.panel_id),
            ));
        }
        if slice.field_keys.is_empty() || slice.field_keys.len() > 64 {
            return Err(LayoutValidationError::new(
                "layout_quota_exceeded",
                format!(
                    "Panel '{}' phải chứa từ 1 đến 64 field_keys.",
                    slice.panel_id
                ),
            ));
        }
        let mut min_view = FieldRole::Admin;
        let mut min_edit = FieldRole::Admin;
        for fk in &slice.field_keys {
            if !eligible_set.contains(fk.as_str()) {
                return Err(LayoutValidationError::new(
                    "unknown_base_field_key",
                    format!(
                        "Trường '{fk}' không thuộc danh sách base fields hợp lệ của '{type_key}'."
                    ),
                ));
            }
            if !assigned_base_fields.insert(fk.as_str()) {
                return Err(LayoutValidationError::new(
                    "duplicate_base_field_placement",
                    format!(
                        "Base field '{fk}' chỉ được xuất hiện tối đa 1 lần trong base_field_panels."
                    ),
                ));
            }
            if let Some(eff) = effective.effective_fields.get(fk) {
                if eff.definition.min_view_role < min_view {
                    min_view = eff.definition.min_view_role;
                }
                if eff.definition.min_edit_role < min_edit {
                    min_edit = eff.definition.min_edit_role;
                }
            }
        }
        validate_subgroups_for_panel(&slice.subgroups, &slice.field_keys, &slice.panel_id)?;
        slice_field_map.insert(slice.panel_id.clone(), slice.field_keys.clone());
        all_panels.push(PanelDefinition {
            panel_id: slice.panel_id.clone(),
            owner: ResourceOwner::User,
            title: slice.title.clone(),
            kind: PanelKind::BaseFieldsSlice {
                slice_key: slice_key.to_owned(),
                field_keys: slice.field_keys.clone(),
            },
            applies_to: vec![type_key.clone()],
            default_region: EditorRegion::MainBottom,
            allowed_regions: default_all_content_regions(),
            default_order: 50,
            min_view_role: min_view,
            min_edit_role: min_edit,
            collapsible: slice.collapsible,
            required_core: false,
            effective_status: EffectiveFieldStatus::Active,
        });
    }

    let unassigned_base_fields: Vec<String> = eligible_base
        .into_iter()
        .filter(|fk| !assigned_base_fields.contains(fk.as_str()))
        .collect();

    let panels_by_id: HashMap<String, PanelDefinition> = all_panels
        .iter()
        .map(|p| (p.panel_id.clone(), p.clone()))
        .collect();

    // Map each panel_id -> list of field keys belonging to it (for Rule COMPAT-4 and Rule LAY-5)
    let mut panel_fields: HashMap<String, Vec<String>> = slice_field_map;
    panel_fields.insert("core:base_fields".into(), unassigned_base_fields.clone());
    for g in &effective.attached_groups {
        panel_fields.insert(
            g.group_id.clone(),
            g.fields.iter().map(|f| f.key.clone()).collect(),
        );
    }

    // Rule LAY-3, LAY-4, LAY-5: Validate every placement in regions
    let mut placed_ids: HashSet<String> = HashSet::new();
    let region_pairs = [
        (EditorRegion::MainTop, &mut layout.regions.main_top),
        (EditorRegion::MainPrimary, &mut layout.regions.main_primary),
        (EditorRegion::MainBottom, &mut layout.regions.main_bottom),
        (EditorRegion::Sidebar, &mut layout.regions.sidebar),
    ];

    for (region, placements) in region_pairs {
        for p in placements.iter() {
            let Some(def) = panels_by_id.get(&p.panel_id) else {
                return Err(LayoutValidationError::new(
                    "unknown_or_inapplicable_panel_id",
                    format!(
                        "panel_id '{}' không tồn tại hoặc không áp dụng cho '{type_key}'.",
                        p.panel_id
                    ),
                ));
            };
            if !placed_ids.insert(p.panel_id.clone()) {
                return Err(LayoutValidationError::new(
                    "duplicate_panel_placement",
                    format!(
                        "panel_id '{}' chỉ được xuất hiện tối đa 1 lần trong regions.",
                        p.panel_id
                    ),
                ));
            }
            if !def.allowed_regions.contains(&region) {
                return Err(LayoutValidationError::new(
                    "invalid_panel_region",
                    format!(
                        "Panel '{}' không được phép đặt vào vùng '{}'.",
                        p.panel_id,
                        region.as_str()
                    ),
                ));
            }
            if def.required_core && !p.enabled {
                return Err(LayoutValidationError::new(
                    "required_core_panel_disabled",
                    format!("Panel lõi '{}' không thể bị vô hiệu hóa.", p.panel_id),
                ));
            }
            if !p.subgroups.is_empty() {
                let allows_subgroups = matches!(
                    def.kind,
                    PanelKind::ContentFields { .. } | PanelKind::BaseFieldsSlice { .. }
                ) || p.panel_id == "core:base_fields";
                if !allows_subgroups {
                    return Err(LayoutValidationError::new(
                        "subgroups_not_allowed_on_panel_kind",
                        format!("Panel '{}' không hỗ trợ subgroups.", p.panel_id),
                    ));
                }
                let fkeys = panel_fields
                    .get(&p.panel_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                validate_subgroups_for_panel(&p.subgroups, fkeys, &p.panel_id)?;
            }
            // Rule COMPAT-4: panel containing effective_required == true && min_edit_role == Editor cannot be hidden/admin-only
            if let Some(fkeys) = panel_fields.get(&p.panel_id) {
                for fk in fkeys {
                    if let Some(eff) = effective.effective_fields.get(fk) {
                        if eff.effective_required()
                            && eff.definition.min_edit_role == FieldRole::Editor
                            && (!p.enabled
                                || def.min_view_role == FieldRole::Admin
                                || def.min_edit_role == FieldRole::Admin)
                        {
                            return Err(LayoutValidationError::new(
                                "required_field_hidden_or_inaccessible",
                                format!(
                                    "Không thể ẩn hoặc giới hạn quyền Admin cho panel '{}' vì chứa trường bắt buộc '{fk}'.",
                                    p.panel_id
                                ),
                            ));
                        }
                    }
                }
            }
        }
    }

    // Rule LAY-2: Every slice in base_field_panels must be placed in regions
    for slice in &layout.base_field_panels {
        if !placed_ids.contains(&slice.panel_id) {
            return Err(LayoutValidationError::new(
                "unplaced_base_field_slice",
                format!(
                    "Panel '{}' trong base_field_panels chưa được đặt vào vùng nào.",
                    slice.panel_id
                ),
            ));
        }
    }

    // Rule LAY-6: Auto-append any newly registered panels missing from saved regions
    for def in &all_panels {
        if def.default_region == EditorRegion::HeaderBar || placed_ids.contains(&def.panel_id) {
            continue;
        }
        let default_placement = PanelPlacementConfig {
            panel_id: def.panel_id.clone(),
            enabled: true,
            order: def.default_order,
            collapsed_by_default: false,
            subgroups: Vec::new(),
        };
        match def.default_region {
            EditorRegion::MainTop => layout.regions.main_top.push(default_placement),
            EditorRegion::MainPrimary => layout.regions.main_primary.push(default_placement),
            EditorRegion::MainBottom => layout.regions.main_bottom.push(default_placement),
            EditorRegion::Sidebar => layout.regions.sidebar.push(default_placement),
            EditorRegion::HeaderBar => {}
        }
        placed_ids.insert(def.panel_id.clone());
    }

    // Deterministic sorting in each region
    let sort_region = |list: &mut Vec<PanelPlacementConfig>| {
        list.sort_by(|a, b| {
            let ka = panels_by_id
                .get(&a.panel_id)
                .map(|d| panel_sort_key(d, a))
                .unwrap_or((a.order, 99, a.panel_id.clone()));
            let kb = panels_by_id
                .get(&b.panel_id)
                .map(|d| panel_sort_key(d, b))
                .unwrap_or((b.order, 99, b.panel_id.clone()));
            ka.cmp(&kb)
        });
    };
    sort_region(&mut layout.regions.main_top);
    sort_region(&mut layout.regions.main_primary);
    sort_region(&mut layout.regions.main_bottom);
    sort_region(&mut layout.regions.sidebar);

    layout.version = layout_version;
    Ok((layout, all_panels))
}

/// Projects `EffectiveEditorConfigProjection` for `caller_role` (`Editor` | `Admin`),
/// hiding fields/panels where `caller_role < min_view_role` and marking `editable: false`
/// where `caller_role < min_edit_role`.
pub fn project_effective_editor_config(
    effective: &EffectiveContentSchema,
    layout: &EditorLayoutConfigV1,
    all_panels: &[PanelDefinition],
    taxonomies: &[String],
    caller_role: FieldRole,
) -> EffectiveEditorConfigProjection {
    let panels_by_id: HashMap<&str, &PanelDefinition> = all_panels
        .iter()
        .map(|p| (p.panel_id.as_str(), p))
        .collect();

    let mut projected_fields = BTreeMap::new();
    for (k, entry) in &effective.effective_fields {
        if caller_role < entry.definition.min_view_role {
            continue;
        }
        projected_fields.insert(
            k.clone(),
            EffectiveFieldProjection {
                definition: entry.definition.clone(),
                owner: entry.owner.clone(),
                source_id: entry.source_id.clone(),
                effective_status: entry.effective_status,
                effective_required: entry.effective_required(),
                editable: caller_role >= entry.definition.min_edit_role,
            },
        );
    }

    let eligible_base = eligible_base_field_keys(effective);
    let assigned_base: HashSet<&str> = layout
        .base_field_panels
        .iter()
        .flat_map(|s| s.field_keys.iter().map(String::as_str))
        .collect();
    let unassigned_base: Vec<String> = eligible_base
        .into_iter()
        .filter(|fk| !assigned_base.contains(fk.as_str()))
        .collect();

    let group_fields_map: HashMap<&str, Vec<String>> = effective
        .attached_groups
        .iter()
        .map(|g| {
            (
                g.group_id.as_str(),
                g.fields.iter().map(|f| f.key.clone()).collect(),
            )
        })
        .collect();

    let project_region = |placements: &[PanelPlacementConfig]| -> Vec<EffectivePanelProjection> {
        let mut out = Vec::new();
        for p in placements {
            if !p.enabled {
                continue;
            }
            let Some(def) = panels_by_id.get(p.panel_id.as_str()).copied() else {
                continue;
            };
            if caller_role < def.min_view_role {
                continue;
            }
            let raw_keys: Vec<String> = match &def.kind {
                PanelKind::BaseFieldsSlice { field_keys, .. } => field_keys.clone(),
                PanelKind::CoreBuiltIn { core_key } if core_key == "base_fields" => {
                    if unassigned_base.is_empty() {
                        continue;
                    }
                    unassigned_base.clone()
                }
                PanelKind::ContentFields { field_group_id } => group_fields_map
                    .get(field_group_id.to_canonical().as_str())
                    .cloned()
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            let visible_keys: Vec<String> = raw_keys
                .into_iter()
                .filter(|fk| projected_fields.contains_key(fk))
                .collect();
            if matches!(
                def.kind,
                PanelKind::BaseFieldsSlice { .. } | PanelKind::ContentFields { .. }
            ) && visible_keys.is_empty()
            {
                continue;
            }
            if p.panel_id == "core:base_fields" && visible_keys.is_empty() {
                continue;
            }
            let visible_set: HashSet<&str> = visible_keys.iter().map(String::as_str).collect();
            let raw_subgroups = if !p.subgroups.is_empty() {
                &p.subgroups
            } else if let Some(slice) = layout
                .base_field_panels
                .iter()
                .find(|s| s.panel_id == p.panel_id)
            {
                &slice.subgroups
            } else {
                &[][..]
            };
            let subgroups: Vec<LayoutFieldSubgroup> = raw_subgroups
                .iter()
                .filter_map(|sg| {
                    let fks: Vec<String> = sg
                        .field_keys
                        .iter()
                        .filter(|fk| visible_set.contains(fk.as_str()))
                        .cloned()
                        .collect();
                    if fks.is_empty() {
                        None
                    } else {
                        Some(LayoutFieldSubgroup {
                            subgroup_id: sg.subgroup_id.clone(),
                            title: sg.title.clone(),
                            field_keys: fks,
                        })
                    }
                })
                .collect();

            out.push(EffectivePanelProjection {
                panel: def.clone(),
                placement: p.clone(),
                field_keys: visible_keys,
                subgroups,
            });
        }
        out
    };

    EffectiveEditorConfigProjection {
        type_key: effective.base.descriptor.key.clone(),
        label: effective.base.descriptor.label.clone(),
        settings: effective.base.descriptor.settings.clone(),
        owner: effective.base.owner.clone(),
        provider_status: effective.base.status,
        declaration_state: effective.base.declaration_state,
        base_version: effective.base_version,
        layout_token: format!("v{}", layout.version),
        schema_token: effective.schema_token.clone(),
        taxonomies: taxonomies.to_vec(),
        regions: EffectiveEditorRegionsProjection {
            main_top: project_region(&layout.regions.main_top),
            main_primary: project_region(&layout.regions.main_primary),
            main_bottom: project_region(&layout.regions.main_bottom),
            sidebar: project_region(&layout.regions.sidebar),
        },
        effective_fields: projected_fields,
    }
}
