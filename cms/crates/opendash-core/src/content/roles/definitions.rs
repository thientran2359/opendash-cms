use super::*;

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
