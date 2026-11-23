//! Content-type and custom-field schema for the CMS content module (M3 / M7-S09).
//! Identifier rules come from `crate::id`; resource ownership is recorded by
//! wrapping a definition in `registry::OwnedDescriptor<ContentTypeDefinition>`.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
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

/// Fixed regions of the Single Edit screen (`M7-S09` / `M7-S10`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum EditorRegion {
    HeaderBar,
    MainTop,
    MainPrimary,
    #[default]
    MainBottom,
    Sidebar,
}

impl EditorRegion {
    pub const fn default_main_bottom() -> Self {
        Self::MainBottom
    }

    pub const fn default_sidebar() -> Self {
        Self::Sidebar
    }

    pub fn is_valid_for_field_group(self) -> bool {
        matches!(
            self,
            Self::MainTop | Self::MainPrimary | Self::MainBottom | Self::Sidebar
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::HeaderBar => "header_bar",
            Self::MainTop => "main_top",
            Self::MainPrimary => "main_primary",
            Self::MainBottom => "main_bottom",
            Self::Sidebar => "sidebar",
        }
    }

    pub fn from_db(raw: &str) -> Option<Self> {
        match raw {
            "header_bar" => Some(Self::HeaderBar),
            "main_top" => Some(Self::MainTop),
            "main_primary" => Some(Self::MainPrimary),
            "main_bottom" => Some(Self::MainBottom),
            "sidebar" => Some(Self::Sidebar),
            _ => None,
        }
    }
}

/// Lifecycle status of a `content_field_groups` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldGroupStatus {
    Active,
    ProviderInactive,
    Withdrawn,
}

impl FieldGroupStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::ProviderInactive => "provider_inactive",
            Self::Withdrawn => "withdrawn",
        }
    }

    pub fn from_db(raw: &str) -> Option<Self> {
        match raw {
            "active" => Some(Self::Active),
            "provider_inactive" => Some(Self::ProviderInactive),
            "withdrawn" => Some(Self::Withdrawn),
            _ => None,
        }
    }
}

/// Computed status of a field in `EffectiveContentSchema`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveFieldStatus {
    Active,
    ProviderInactive,
    Withdrawn,
}

/// The nine custom-field kinds supported by the V1 content editor.
/// Rendering of field values belongs to later M3/M4 slices. Values are
/// validated here before persistence; rich text is never rendered as HTML here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentFieldType {
    ShortText,
    LongText,
    RichText,
    Number,
    Boolean,
    Date,
    Select,
    Image,
    Gallery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

const fn default_declaration_declared() -> DeclarationState {
    DeclarationState::Declared
}

const fn default_panel_order_50() -> i32 {
    50
}

const fn default_true() -> bool {
    true
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

pub fn valid_default_value(field: &ContentFieldDefinition, value: &Value) -> bool {
    if value.is_null() {
        return false;
    }
    if matches!(
        field.field_type,
        ContentFieldType::Image | ContentFieldType::Gallery
    ) {
        return false;
    }
    if !valid_field_value(field, value) {
        return false;
    }
    if matches!(
        field.field_type,
        ContentFieldType::ShortText | ContentFieldType::LongText | ContentFieldType::RichText
    ) && value.as_str().is_none_or(|s| s.trim().is_empty())
    {
        return false;
    }
    true
}

/// Publication semantics are server-owned; the Admin UI must not infer them from
/// the content-type key. Legacy custom types use the immediate policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentPublication {
    Immediate,
    DraftPublish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentSupport {
    Editor,
    FeaturedImage,
    Builder,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentMode {
    #[default]
    Editor,
    Builder,
}

impl ContentMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Editor => "editor",
            Self::Builder => "builder",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentTypeSettings {
    pub public: bool,
    pub has_archive: bool,
    pub publication: ContentPublication,
    pub supports: Vec<ContentSupport>,
}

impl Default for ContentTypeSettings {
    fn default() -> Self {
        Self {
            public: true,
            has_archive: true,
            publication: ContentPublication::Immediate,
            supports: vec![ContentSupport::Editor, ContentSupport::FeaturedImage],
        }
    }
}

impl ContentTypeSettings {
    pub fn default_for_type_key(type_key: &str) -> Self {
        match type_key {
            "post" => Self::builtin_publication(),
            "page" => Self::builtin_page(),
            "pattern" => Self::private_pattern(),
            _ => Self::default(),
        }
    }

    pub fn builtin_publication() -> Self {
        Self {
            publication: ContentPublication::DraftPublish,
            ..Self::default()
        }
    }

    pub fn builtin_page() -> Self {
        let mut settings = Self::builtin_publication();
        // Page has no archive of its own; its Builder capability is an
        // entry-owned content surface, not a shared template or archive.
        settings.has_archive = false;
        settings.supports.push(ContentSupport::Builder);
        settings
    }

    pub fn private_pattern() -> Self {
        Self {
            public: false,
            has_archive: false,
            publication: ContentPublication::Immediate,
            supports: vec![ContentSupport::Builder],
        }
    }

    pub fn validate_and_normalize(&mut self) -> Result<(), &'static str> {
        let original_len = self.supports.len();
        self.supports.sort_unstable();
        self.supports.dedup();
        if self.supports.len() != original_len {
            return Err("supports không được chứa tính năng trùng lặp.");
        }
        Ok(())
    }

    pub fn supports(&self, feature: ContentSupport) -> bool {
        self.supports.contains(&feature)
    }
}

/// Wire input DTO for `POST /api/admin/content-types` and `PUT /api/admin/content-types/{type_key}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentTypeDefinitionInput {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub fields: Vec<FieldDefinitionInput>,
    #[serde(default)]
    pub settings: Option<ContentTypeSettings>,
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

/// Schema is independent of the theme/plugin that declared it. Ownership and
/// provider status are managed by the existing `RegistryCatalog` instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentTypeDefinition {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub fields: Vec<ContentFieldDefinition>,
    /// Legacy definitions without this property deserialize to public immediate.
    #[serde(default)]
    pub settings: ContentTypeSettings,
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

/// Wire DTO for `field_groups` in `theme.json` (v2) and `plugin.json` (`manifest_schema: 2`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFieldGroupInput {
    pub local_key: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub target_types: Vec<String>,
    #[serde(default = "EditorRegion::default_main_bottom")]
    pub default_region: EditorRegion,
    #[serde(default = "default_panel_order_50")]
    pub default_order: i32,
    #[serde(default = "default_true")]
    pub collapsible: bool,
    pub fields: Vec<FieldDefinitionInput>,
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

/// Wire DTO for User `POST /api/admin/content-field-groups` and `PUT /api/admin/content-field-groups/{group_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserFieldGroupInput {
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub local_key: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub target_types: Vec<String>,
    #[serde(default = "EditorRegion::default_main_bottom")]
    pub default_region: EditorRegion,
    #[serde(default = "default_panel_order_50")]
    pub default_order: i32,
    #[serde(default = "default_true")]
    pub collapsible: bool,
    pub fields: Vec<FieldDefinitionInput>,
}

impl UserFieldGroupInput {
    pub fn resolve_user_scoped_id(
        &self,
        expected_group_id: Option<&str>,
    ) -> Result<ScopedResourceId, &'static str> {
        let candidate_raw = match (&self.group_id, &self.local_key, expected_group_id) {
            (Some(gid), Some(lkey), _) => {
                if lkey.starts_with("layout:") || gid.starts_with("layout:") {
                    return Err(
                        "reserved_panel_namespace: Tiền tố 'layout:' chỉ dành riêng cho cấu hình bố cục cấp site.",
                    );
                }
                let parsed = if gid.contains(':') {
                    ScopedResourceId::parse(gid)?
                } else {
                    ScopedResourceId::new(ResourceOwner::User, gid.clone())?
                };
                if parsed.owner != ResourceOwner::User || parsed.local_key != *lkey {
                    return Err("group_id và local_key của User Field Group không khớp nhau.");
                }
                parsed
            }
            (Some(gid), None, _) => {
                if gid.starts_with("layout:") {
                    return Err(
                        "reserved_panel_namespace: Tiền tố 'layout:' chỉ dành riêng cho cấu hình bố cục cấp site.",
                    );
                }
                let parsed = if gid.contains(':') {
                    ScopedResourceId::parse(gid)?
                } else {
                    ScopedResourceId::new(ResourceOwner::User, gid.clone())?
                };
                if parsed.owner != ResourceOwner::User {
                    return Err("User API chỉ được phép tạo hoặc sửa nhóm trường thuộc 'user:'.");
                }
                parsed
            }
            (None, Some(lkey), _) => {
                if lkey.starts_with("layout:") {
                    return Err(
                        "reserved_panel_namespace: Tiền tố 'layout:' chỉ dành riêng cho cấu hình bố cục cấp site.",
                    );
                }
                if lkey.contains(':') {
                    let parsed = ScopedResourceId::parse(lkey)?;
                    if parsed.owner != ResourceOwner::User {
                        return Err(
                            "User API chỉ được phép tạo hoặc sửa nhóm trường thuộc 'user:'.",
                        );
                    }
                    parsed
                } else {
                    ScopedResourceId::new(ResourceOwner::User, lkey.clone())?
                }
            }
            (None, None, Some(path_gid)) => {
                if path_gid.starts_with("layout:") {
                    return Err(
                        "reserved_panel_namespace: Tiền tố 'layout:' chỉ dành riêng cho cấu hình bố cục cấp site.",
                    );
                }
                let parsed = if path_gid.contains(':') {
                    ScopedResourceId::parse(path_gid)?
                } else {
                    ScopedResourceId::new(ResourceOwner::User, path_gid.to_owned())?
                };
                if parsed.owner != ResourceOwner::User {
                    return Err("User API chỉ được phép tạo hoặc sửa nhóm trường thuộc 'user:'.");
                }
                parsed
            }
            (None, None, None) => {
                return Err("Cần cung cấp local_key hoặc group_id cho nhóm trường.");
            }
        };

        if let Some(path_gid) = expected_group_id {
            let expected_parsed = if path_gid.contains(':') {
                ScopedResourceId::parse(path_gid)?
            } else {
                ScopedResourceId::new(ResourceOwner::User, path_gid.to_owned())?
            };
            if candidate_raw != expected_parsed {
                return Err("group_id trong body không khớp với đường dẫn URL.");
            }
        }
        Ok(candidate_raw)
    }

    pub fn validate_and_normalize(
        &self,
        expected_group_id: Option<&str>,
    ) -> Result<(ScopedResourceId, Vec<ContentFieldDefinition>), &'static str> {
        let scoped_id = self.resolve_user_scoped_id(expected_group_id)?;
        let as_manifest = ManifestFieldGroupInput {
            local_key: scoped_id.local_key.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            target_types: self.target_types.clone(),
            default_region: self.default_region,
            default_order: self.default_order,
            collapsible: self.collapsible,
            fields: self.fields.clone(),
        };
        let fields = as_manifest.validate_and_normalize()?;
        Ok((scoped_id, fields))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldGroupTargetBinding {
    pub type_key: String,
    pub association_state: DeclarationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldGroupRecord {
    pub group_id: String,
    pub owner: ResourceOwner,
    pub local_key: String,
    pub title: String,
    pub description: String,
    pub fields: Vec<ContentFieldDefinition>,
    pub targets: Vec<FieldGroupTargetBinding>,
    pub default_region: EditorRegion,
    pub default_order: i32,
    pub collapsible: bool,
    pub status: FieldGroupStatus,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachedFieldGroupSnapshot {
    pub group_id: String,
    pub owner: ResourceOwner,
    pub local_key: String,
    pub title: String,
    pub description: String,
    pub fields: Vec<ContentFieldDefinition>,
    pub default_region: EditorRegion,
    pub default_order: i32,
    pub collapsible: bool,
    pub status: FieldGroupStatus,
    pub association_state: DeclarationState,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveFieldEntry {
    pub definition: ContentFieldDefinition,
    pub owner: ResourceOwner,
    pub source_id: String,
    pub effective_status: EffectiveFieldStatus,
}

impl EffectiveFieldEntry {
    /// A field is effectively required when `definition.required == true` AND
    /// its effective status is NOT `Withdrawn` (both `Active` and `ProviderInactive` stay required).
    pub fn effective_required(&self) -> bool {
        self.definition.required && self.effective_status != EffectiveFieldStatus::Withdrawn
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveContentSchema {
    pub base: OwnedDescriptor<ContentTypeDefinition>,
    pub base_version: i64,
    pub attached_groups: Vec<AttachedFieldGroupSnapshot>,
    pub effective_fields: BTreeMap<String, EffectiveFieldEntry>,
    pub schema_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaTransitionError {
    FieldKeyCollision(String),
    CannotRemoveFieldWithData(String),
    IncompatibleSchemaChange(String),
    InvalidFieldAcl(String),
    InvalidFieldDefault(String),
    AdminRequiredFieldMissingDefault(String),
    RequiredFieldHiddenOrInaccessible(String),
}

impl SchemaTransitionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::FieldKeyCollision(_) => "field_key_collision",
            Self::CannotRemoveFieldWithData(_) => "cannot_remove_field_with_data",
            Self::IncompatibleSchemaChange(_) => "incompatible_schema_change",
            Self::InvalidFieldAcl(_) => "invalid_field_acl",
            Self::InvalidFieldDefault(_) => "invalid_field_default",
            Self::AdminRequiredFieldMissingDefault(_) => "admin_required_field_missing_default",
            Self::RequiredFieldHiddenOrInaccessible(_) => "required_field_hidden_or_inaccessible",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::FieldKeyCollision(m)
            | Self::CannotRemoveFieldWithData(m)
            | Self::IncompatibleSchemaChange(m)
            | Self::InvalidFieldAcl(m)
            | Self::InvalidFieldDefault(m)
            | Self::AdminRequiredFieldMissingDefault(m)
            | Self::RequiredFieldHiddenOrInaccessible(m) => format!("{}: {m}", self.code()),
        }
    }
}

fn classify_validation_error(err: &str) -> SchemaTransitionError {
    if let Some(rest) = err.strip_prefix("invalid_field_acl:") {
        SchemaTransitionError::InvalidFieldAcl(rest.trim().to_owned())
    } else if let Some(rest) = err.strip_prefix("invalid_field_default:") {
        SchemaTransitionError::InvalidFieldDefault(rest.trim().to_owned())
    } else if let Some(rest) = err.strip_prefix("admin_required_field_missing_default:") {
        SchemaTransitionError::AdminRequiredFieldMissingDefault(rest.trim().to_owned())
    } else if let Some(rest) = err.strip_prefix("required_field_hidden_or_inaccessible:") {
        SchemaTransitionError::RequiredFieldHiddenOrInaccessible(rest.trim().to_owned())
    } else if let Some(rest) = err.strip_prefix("field_key_collision:") {
        SchemaTransitionError::FieldKeyCollision(rest.trim().to_owned())
    } else {
        SchemaTransitionError::IncompatibleSchemaChange(err.to_owned())
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

/// Retains omitted fields from `existing_fields` as `DeclarationState::Withdrawn` when a package
/// re-declares a CPT or field group (`270-R1`). Preserves declared order of incoming fields and
/// appends withdrawn fields deterministically.
pub fn merge_package_redeclared_fields(
    existing_fields: &[ContentFieldDefinition],
    incoming_fields: &[ContentFieldDefinition],
) -> Vec<ContentFieldDefinition> {
    let incoming_keys: HashSet<&str> = incoming_fields.iter().map(|f| f.key.as_str()).collect();
    let mut merged: Vec<ContentFieldDefinition> = incoming_fields.to_vec();
    for old in existing_fields {
        if !incoming_keys.contains(old.key.as_str()) {
            let mut withdrawn = old.clone();
            withdrawn.declaration_state = DeclarationState::Withdrawn;
            merged.push(withdrawn);
        }
    }
    merged
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CntDeclarations {
    #[serde(default)]
    pub content_types: Vec<ContentTypeDeclaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub taxonomy_definitions: Vec<TaxonomyDeclaration>,
    #[serde(default)]
    pub taxonomy_eligibility: Vec<TaxonomyEligibilityDeclaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_groups: Vec<ManifestFieldGroupInput>,
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

/// Syntax shared by CPT and taxonomy rewrite defaults. Route root reservations,
/// namespace collisions, and site overrides belong to the C3 storage resolver.
fn validate_provider_rewrite_base(base: Option<&str>) -> Result<(), &'static str> {
    if let Some(base) = base {
        let segments: Vec<&str> = base.split('/').collect();
        if segments.len() > 3
            || segments
                .iter()
                .any(|segment| !valid_kebab_segment_with_limit(segment, 64))
        {
            return Err("rewrite_base phải có từ 1 đến 3 đoạn kebab-case hợp lệ.");
        }
    }
    Ok(())
}

/// References may point to core, site-owned, or same-provider taxonomies;
/// canonical storage verifies existence, ownership, and provider activity.
fn validate_provider_taxonomy_keys(keys: &[String]) -> Result<(), &'static str> {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentTypeDeclaration {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub fields: Vec<ContentFieldDefinition>,
    #[serde(default)]
    pub taxonomies: Vec<String>,
    /// Omission preserves existing provider-owned settings on package re-install.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ContentTypeSettings>,
    /// Provider default only. The site's explicit C3 override takes precedence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewrite_base: Option<String>,
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
    pub rewrite_base: Option<String>,
}

impl TaxonomyDeclaration {
    pub fn validate(&self) -> Result<(), &'static str> {
        TaxonomyDefinitionInput {
            key: self.key.clone(),
            label: self.label.clone(),
            hierarchical: self.hierarchical,
            public: self.public,
        }
        .validate()?;
        validate_provider_rewrite_base(self.rewrite_base.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyEligibilityDeclaration {
    pub content_type: String,
    #[serde(default)]
    pub taxonomies: Vec<String>,
}

/// Canonical site taxonomy descriptor (C2). Ownership is the same registry
/// primitive used by content types, not a second competing ownership system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyDefinition {
    pub key: String,
    pub label: String,
    pub hierarchical: bool,
    pub public: bool,
    pub version: i64,
    pub owner: ResourceOwner,
    pub provider_status: ProviderStatus,
    pub declaration_state: DeclarationState,
}

fn taxonomy_public_default() -> bool {
    true
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
#[serde(deny_unknown_fields)]
pub struct TaxonomyDefinitionPatch {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub hierarchical: Option<bool>,
    #[serde(default)]
    pub public: Option<bool>,
}

impl TaxonomyDefinitionPatch {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.label.is_none() && self.hierarchical.is_none() && self.public.is_none() {
            return Err("Cần ít nhất một thuộc tính taxonomy để cập nhật.");
        }
        if self.label.as_ref().is_some_and(|label| !valid_label(label)) {
            return Err("Tên taxonomy không hợp lệ (tối đa 100 ký tự).");
        }
        Ok(())
    }
}

pub fn valid_label(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 100 && !value.chars().any(char::is_control)
}

/// A complete content entry write, with a stable slug scoped to its CPT.
/// Missing optional fields and explicit JSON null are both permitted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentEntryInput {
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    /// Omitted on update: preserve existing membership (legacy client).
    /// Explicit []: unlink all. Kind/count validation lives in storage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term_ids: Option<Vec<i64>>,
}

/// Three-state patch field distinguishing omitted key (`Omitted`), JSON `null` (`Clear`), and value (`Set(T)`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PatchField<T> {
    #[default]
    Omitted,
    Clear,
    Set(T),
}

pub fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<PatchField<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    match Option::<T>::deserialize(deserializer)? {
        None => Ok(PatchField::Clear),
        Some(val) => Ok(PatchField::Set(val)),
    }
}

impl ContentEntryInput {
    pub fn validate_against(&self, schema: &ContentTypeDefinition) -> Result<(), &'static str> {
        if self.title.trim().is_empty()
            || self.title.chars().count() > 200
            || self.title.chars().any(char::is_control)
        {
            return Err("Tiêu đề nội dung không hợp lệ.");
        }
        if !valid_kebab_segment_with_limit(&self.slug, 64) {
            return Err("Slug nội dung không hợp lệ.");
        }
        if let Some(ids) = &self.term_ids {
            if ids.len() > 96
                || ids.iter().any(|id| *id <= 0)
                || ids.iter().copied().collect::<HashSet<_>>().len() != ids.len()
            {
                return Err("term_ids phải là danh sách ID dương, duy nhất (tối đa 96).");
            }
        }
        for key in self.values.keys() {
            if !schema.fields.iter().any(|field| field.key == *key) {
                return Err("Giá trị chứa custom field chưa được khai báo.");
            }
        }
        for field in &schema.fields {
            let value = self.values.get(&field.key).unwrap_or(&Value::Null);
            if value.is_null() {
                if field.required && field.declaration_state != DeclarationState::Withdrawn {
                    return Err("Thiếu custom field bắt buộc.");
                }
                continue;
            }
            if !valid_field_value(field, value) {
                return Err("Giá trị custom field không đúng kiểu hoặc vượt giới hạn.");
            }
        }
        Ok(())
    }

    pub fn validate_against_effective(
        &self,
        effective: &EffectiveContentSchema,
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
        if let Some(ids) = &self.term_ids {
            if ids.len() > 96
                || ids.iter().any(|id| *id <= 0)
                || ids.iter().copied().collect::<HashSet<_>>().len() != ids.len()
            {
                return Err("term_ids phải là danh sách ID dương, duy nhất (tối đa 96).");
            }
        }
        for key in self.values.keys() {
            if !effective.effective_fields.contains_key(key) {
                return Err("Giá trị chứa custom field chưa được khai báo.");
            }
        }
        for (key, entry) in &effective.effective_fields {
            let value = self.values.get(key).unwrap_or(&Value::Null);
            if value.is_null() {
                if entry.effective_required() {
                    return Err("Thiếu custom field bắt buộc.");
                }
                continue;
            }
            if !valid_field_value(&entry.definition, value) {
                return Err("Giá trị custom field không đúng kiểu hoặc vượt giới hạn.");
            }
        }
        Ok(())
    }
}

pub fn valid_field_value(field: &ContentFieldDefinition, value: &Value) -> bool {
    let text = value.as_str();
    match field.field_type {
        ContentFieldType::ShortText => text.is_some_and(|s| {
            s.chars().count() <= 512
                && s.chars().all(|c| !c.is_control())
                && (!field.required || !s.trim().is_empty())
        }),
        ContentFieldType::LongText => text.is_some_and(|s| {
            s.chars().count() <= 100_000 && (!field.required || !s.trim().is_empty())
        }),
        ContentFieldType::RichText => text.is_some_and(|s| {
            let len_ok = if crate::builder_blocks::contains_blocks(s) {
                s.len() <= crate::builder_blocks::MAX_BUILDER_BLOCK_BODY_BYTES
                    && s.chars().count() <= crate::builder_blocks::MAX_BUILDER_BLOCK_BODY_BYTES
            } else {
                s.chars().count() <= 200_000
            };
            len_ok && (!field.required || !s.trim().is_empty())
        }),
        ContentFieldType::Number => value.is_number(),
        ContentFieldType::Boolean => value.is_boolean(),
        ContentFieldType::Date => text.is_some_and(valid_iso_date),
        ContentFieldType::Select => {
            text.is_some_and(|selected| field.options.iter().any(|option| option.value == selected))
        }
        ContentFieldType::Image => text.is_some_and(valid_media_id),
        ContentFieldType::Gallery => value.as_array().is_some_and(|items| {
            (!field.required || !items.is_empty()) && items.len() <= 50 && {
                let mut seen = HashSet::new();
                items.iter().all(|item| {
                    item.as_str()
                        .is_some_and(|id| valid_media_id(id) && seen.insert(id))
                })
            }
        }),
    }
}

/// The domain checks opaque media identifier syntax; the server storage layer
/// additionally verifies each image/gallery ID exists before saving entries.
pub fn valid_media_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.bytes().all(|c| {
            c.is_ascii_lowercase()
                || c.is_ascii_uppercase()
                || c.is_ascii_digit()
                || c == b'-'
                || c == b'_'
        })
}

fn valid_iso_date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..10].parse::<u32>().unwrap_or(0);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    year > 0 && day > 0 && day <= days_in_month
}

/// Compatibility helper used by legacy callers.
pub fn check_base_fields_compatibility(
    existing: &[ContentFieldDefinition],
    incoming: &[ContentFieldDefinition],
    has_existing_entries: bool,
) -> Result<(), &'static str> {
    let incoming_by_key: HashMap<&str, &ContentFieldDefinition> =
        incoming.iter().map(|f| (f.key.as_str(), f)).collect();
    let existing_by_key: HashMap<&str, &ContentFieldDefinition> =
        existing.iter().map(|f| (f.key.as_str(), f)).collect();

    for old in existing {
        let Some(new) = incoming_by_key.get(old.key.as_str()) else {
            if has_existing_entries {
                return Err(
                    "cannot_remove_field_with_data: Không thể xóa hoặc đổi key field khi đã có dữ liệu cũ.",
                );
            }
            continue;
        };
        if old.field_type != new.field_type {
            return Err(
                "incompatible_schema_change: Không thể đổi kiểu dữ liệu (type) của custom field đã tồn tại.",
            );
        }
        let old_req = old.required && old.declaration_state != DeclarationState::Withdrawn;
        let new_req = new.required && new.declaration_state != DeclarationState::Withdrawn;
        if !old_req && new_req && has_existing_entries {
            return Err(
                "incompatible_schema_change: Không thể đổi field tùy chọn thành bắt buộc (required) khi đã có dữ liệu.",
            );
        }
        if old.field_type == ContentFieldType::Select && has_existing_entries {
            let new_values: HashSet<&str> = new.options.iter().map(|o| o.value.as_str()).collect();
            for old_opt in &old.options {
                if !new_values.contains(old_opt.value.as_str()) {
                    return Err(
                        "incompatible_schema_change: Không thể xóa lựa chọn (select option) khi đã có dữ liệu cũ.",
                    );
                }
            }
        }
    }

    for new in incoming {
        let new_req = new.required && new.declaration_state != DeclarationState::Withdrawn;
        if !existing_by_key.contains_key(new.key.as_str()) && new_req && has_existing_entries {
            return Err(
                "incompatible_schema_change: Không thể thêm field bắt buộc (required) mới khi đã có dữ liệu.",
            );
        }
    }

    Ok(())
}

const fn default_panel_order_80() -> i32 {
    80
}

fn default_action_allowed_regions() -> Vec<EditorRegion> {
    vec![
        EditorRegion::MainTop,
        EditorRegion::MainBottom,
        EditorRegion::Sidebar,
    ]
}

fn default_all_content_regions() -> Vec<EditorRegion> {
    vec![
        EditorRegion::MainTop,
        EditorRegion::MainPrimary,
        EditorRegion::MainBottom,
        EditorRegion::Sidebar,
    ]
}

/// Panel kinds in the Single Edit Panel Registry (`270-R9`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PanelKind {
    CoreBuiltIn {
        core_key: String,
    },
    BaseFieldsSlice {
        slice_key: String,
        field_keys: Vec<String>,
    },
    ContentFields {
        field_group_id: ScopedResourceId,
    },
    Action {
        action_id: String,
        button_label: String,
        confirm_message: Option<String>,
        public_effect: bool,
    },
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutFieldSubgroup {
    pub subgroup_id: String,
    pub title: String,
    pub field_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaseFieldSlicePanelConfig {
    pub panel_id: String,
    pub title: String,
    #[serde(default = "default_true")]
    pub collapsible: bool,
    pub field_keys: Vec<String>,
    #[serde(default)]
    pub subgroups: Vec<LayoutFieldSubgroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelPlacementConfig {
    pub panel_id: String,
    pub enabled: bool,
    pub order: i32,
    #[serde(default)]
    pub collapsed_by_default: bool,
    #[serde(default)]
    pub subgroups: Vec<LayoutFieldSubgroup>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorRegionsConfig {
    #[serde(default)]
    pub main_top: Vec<PanelPlacementConfig>,
    #[serde(default)]
    pub main_primary: Vec<PanelPlacementConfig>,
    #[serde(default)]
    pub main_bottom: Vec<PanelPlacementConfig>,
    #[serde(default)]
    pub sidebar: Vec<PanelPlacementConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorLayoutConfigV1 {
    pub schema_version: u32,
    pub type_key: String,
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub base_field_panels: Vec<BaseFieldSlicePanelConfig>,
    pub regions: EditorRegionsConfig,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveFieldProjection {
    pub definition: ContentFieldDefinition,
    pub owner: ResourceOwner,
    pub source_id: String,
    pub effective_status: EffectiveFieldStatus,
    pub effective_required: bool,
    pub editable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectivePanelProjection {
    pub panel: PanelDefinition,
    pub placement: PanelPlacementConfig,
    pub field_keys: Vec<String>,
    pub subgroups: Vec<LayoutFieldSubgroup>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveEditorRegionsProjection {
    pub main_top: Vec<EffectivePanelProjection>,
    pub main_primary: Vec<EffectivePanelProjection>,
    pub main_bottom: Vec<EffectivePanelProjection>,
    pub sidebar: Vec<EffectivePanelProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveEditorConfigProjection {
    pub type_key: String,
    pub label: String,
    pub settings: ContentTypeSettings,
    pub owner: ResourceOwner,
    pub provider_status: ProviderStatus,
    pub declaration_state: DeclarationState,
    pub base_version: i64,
    pub layout_token: String,
    pub schema_token: String,
    pub taxonomies: Vec<String>,
    pub regions: EffectiveEditorRegionsProjection,
    pub effective_fields: BTreeMap<String, EffectiveFieldProjection>,
}

fn owner_rank(owner: &ResourceOwner) -> u8 {
    match owner {
        ResourceOwner::Core => 0,
        ResourceOwner::User => 2,
        ResourceOwner::Theme(_) => 3,
        ResourceOwner::Plugin(_) => 4,
    }
}

fn panel_sort_key(def: &PanelDefinition, placement: &PanelPlacementConfig) -> (i32, u8, String) {
    let rank = match def.kind {
        PanelKind::BaseFieldsSlice { .. } => 1,
        _ => owner_rank(&def.owner),
    };
    (placement.order, rank, def.panel_id.clone())
}

fn has_core_body_panel(effective: &EffectiveContentSchema) -> bool {
    let type_key = effective.base.descriptor.key.as_str();
    let is_builtin = matches!(type_key, "post" | "page");
    let settings = &effective.base.descriptor.settings;
    !matches!(type_key, "section" | "pattern")
        && (is_builtin
            || settings.supports(ContentSupport::Editor)
            || settings.supports(ContentSupport::Builder))
        && effective
            .effective_fields
            .get("body")
            .is_some_and(|fe| fe.definition.field_type == ContentFieldType::RichText)
}

pub fn eligible_base_field_keys(effective: &EffectiveContentSchema) -> Vec<String> {
    let type_key = effective.base.descriptor.key.as_str();
    let use_body_panel = has_core_body_panel(effective);
    let is_private_reusable = matches!(type_key, "section" | "pattern");
    effective
        .base
        .descriptor
        .fields
        .iter()
        .filter(|f| {
            !(use_body_panel && f.key == "body")
                && !(!is_private_reusable && f.key == "featured_image")
        })
        .map(|f| f.key.clone())
        .collect()
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutValidationError {
    pub code: &'static str,
    pub message: String,
}

impl LayoutValidationError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: format!("{code}: {}", message.into()),
        }
    }
}

fn validate_subgroups_for_panel(
    subgroups: &[LayoutFieldSubgroup],
    panel_field_keys: &[String],
    panel_id: &str,
) -> Result<(), LayoutValidationError> {
    if subgroups.len() > 12 {
        return Err(LayoutValidationError::new(
            "layout_quota_exceeded",
            format!("Panel '{panel_id}' vượt quá tối đa 12 subgroups."),
        ));
    }
    let allowed_set: HashSet<&str> = panel_field_keys.iter().map(String::as_str).collect();
    let mut seen_sg_ids = HashSet::new();
    let mut seen_fields = HashSet::new();
    for sg in subgroups {
        if !valid_snake_key(&sg.subgroup_id) || !seen_sg_ids.insert(sg.subgroup_id.as_str()) {
            return Err(LayoutValidationError::new(
                "invalid_subgroup",
                format!(
                    "subgroup_id '{}' không hợp lệ hoặc bị trùng.",
                    sg.subgroup_id
                ),
            ));
        }
        if !valid_label(&sg.title) {
            return Err(LayoutValidationError::new(
                "invalid_subgroup",
                format!("Tiêu đề subgroup '{}' không hợp lệ.", sg.subgroup_id),
            ));
        }
        for fk in &sg.field_keys {
            if !allowed_set.contains(fk.as_str()) {
                return Err(LayoutValidationError::new(
                    "unknown_field_in_subgroup",
                    format!(
                        "Trường '{fk}' trong subgroup '{}' không thuộc panel '{panel_id}'.",
                        sg.subgroup_id
                    ),
                ));
            }
            if !seen_fields.insert(fk.as_str()) {
                return Err(LayoutValidationError::new(
                    "duplicate_field_in_subgroups",
                    format!(
                        "Trường '{fk}' bị lặp lại trong nhiều subgroup của panel '{panel_id}'."
                    ),
                ));
            }
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn c4_provider_legacy_manifest_remains_compatible() {
        let legacy: CntDeclarations = serde_json::from_value(json!({
            "content_types": [{"key": "review", "label": "Reviews", "taxonomies": ["category", "tag"]}]
        }))
        .unwrap();
        legacy.validate().unwrap();
        let serialized = serde_json::to_value(&legacy).unwrap();
        assert!(serialized.get("taxonomy_definitions").is_none());
        assert!(serialized["content_types"][0].get("rewrite_base").is_none());
        assert!(serialized["content_types"][0].get("settings").is_none());
        assert!(CntDeclarations::default().is_empty());
    }

    #[test]
    fn c4_provider_can_declare_taxonomy_and_rewrite_defaults() {
        let payload = json!({
            "taxonomy_definitions": [
                {"key":"genre","label":"Genre","hierarchical":true,"public":false,"rewrite_base":"library/genres"},
                {"key":"series","label":"Series"}
            ],
            "content_types": [{
                "key":"review","label":"Reviews","taxonomies":["genre","category","site_taxonomy"],
                "rewrite_base":"library/reviews",
                "settings":{"public":true,"has_archive":true,"publication":"draft_publish","supports":["editor"]}
            }],
            "taxonomy_eligibility": [{"content_type":"review","taxonomies":["series","tag"]}]
        });
        let input: CntDeclarations = serde_json::from_value(payload).unwrap();
        input.validate().unwrap();
        let actual = serde_json::to_value(&input).unwrap();
        assert_eq!(actual["taxonomy_definitions"][0]["public"], false);
        assert_eq!(actual["taxonomy_definitions"][1]["hierarchical"], false);
        assert_eq!(actual["taxonomy_definitions"][1]["public"], true);
        assert!(
            actual["taxonomy_definitions"][1]
                .get("rewrite_base")
                .is_none()
        );
        assert_eq!(
            actual["content_types"][0]["rewrite_base"],
            "library/reviews"
        );
        assert_eq!(
            serde_json::from_value::<CntDeclarations>(actual).unwrap(),
            input
        );

        let tax_only: CntDeclarations = serde_json::from_value(json!({
            "taxonomy_definitions":[{"key":"genre","label":"Genre"}]
        }))
        .unwrap();
        assert!(!tax_only.is_empty());
    }

    #[test]
    fn c4_provider_rejects_duplicate_or_core_owned_taxonomy_declarations() {
        for key in [
            "category",
            "tag",
            "post",
            "page",
            "section",
            "seo",
            "Invalid-Key",
        ] {
            let declarations: CntDeclarations = serde_json::from_value(json!({
                "taxonomy_definitions":[{"key":key,"label":"A"}]
            }))
            .unwrap();
            assert!(
                declarations.validate().is_err(),
                "reserved or invalid taxonomy: {key}"
            );
        }
        let duplicate: CntDeclarations = serde_json::from_value(json!({
            "taxonomy_definitions":[{"key":"genre","label":"Genre"}, {"key":"genre","label":"Other"}]
        })).unwrap();
        assert!(duplicate.validate().is_err());

        for path in [
            json!({"taxonomy_definitions":[{"key":"genre","label":"Genre","surprise":3}]}),
            json!({"taxonomy_definitions":[{"key":"genre","label":"Genre","owner":{"kind":"plugin","id":"other"}}]}),
            json!({"content_types":[{"key":"review","label":"Reviews","rewrite_base":"a","surprise":3}]}),
            json!({"taxonomy_definitions":[{"key":"genre","label":"Genre","rewrite_base":7}]}),
        ] {
            assert!(serde_json::from_value::<CntDeclarations>(path).is_err());
        }
    }

    #[test]
    fn c4_provider_rejects_duplicate_eligibility_rows_claiming_same_taxonomy() {
        let duplicate: CntDeclarations = serde_json::from_value(json!({
            "taxonomy_eligibility": [
                {"content_type":"review","taxonomies":["genre"]},
                {"content_type":"review","taxonomies":["genre"]}
            ]
        }))
        .unwrap();
        assert!(duplicate.validate().is_err());
    }

    #[test]
    fn c4_provider_rejects_malformed_rewrite_bases_and_associations() {
        for base in [
            "",
            "/reviews",
            "reviews/",
            "a//b",
            "a/b/c/d",
            "../x",
            "X",
            "review_name",
            "news?x=1",
            "a/%term%",
        ] {
            let cpt: CntDeclarations = serde_json::from_value(json!({
                "content_types":[{"key":"review","label":"Reviews","rewrite_base":base}]
            }))
            .unwrap();
            assert!(cpt.validate().is_err(), "invalid CPT rewrite base: {base}");
            let tax: CntDeclarations = serde_json::from_value(json!({
                "taxonomy_definitions":[{"key":"genre","label":"Genre","rewrite_base":base}]
            }))
            .unwrap();
            assert!(
                tax.validate().is_err(),
                "invalid taxonomy rewrite base: {base}"
            );
        }
        for taxonomies in [
            json!(["genre", "genre"]),
            json!(["bad-key"]),
            json!([""]),
            json!(["category", "category"]),
        ] {
            let cpt: CntDeclarations = serde_json::from_value(json!({
                "content_types":[{"key":"review","label":"Reviews","taxonomies":taxonomies}]
            }))
            .unwrap();
            assert!(
                cpt.validate().is_err(),
                "invalid CPT association: {taxonomies}"
            );
            let eligibility: CntDeclarations = serde_json::from_value(json!({
                "taxonomy_eligibility":[{"content_type":"review","taxonomies":taxonomies}]
            }))
            .unwrap();
            assert!(
                eligibility.validate().is_err(),
                "invalid eligibility association: {taxonomies}"
            );
        }
        let builtin: CntDeclarations = serde_json::from_value(json!({
            "content_types":[{
                "key":"post","label":"Post","rewrite_base":"blog",
                "settings":{"public":true,"has_archive":true,"publication":"draft_publish","supports":["editor"]}
            }]
        })).unwrap();
        assert!(
            builtin.validate().is_err(),
            "post URLs belong to global C3 policy"
        );
    }

    #[test]
    fn accepts_custom_post_type_keys() {
        assert!(valid_content_type_key("post"));
        assert!(valid_content_type_key("portfolio_item2"));
    }

    #[test]
    fn rejects_unstable_or_invalid_keys() {
        assert!(!valid_content_type_key(""));
        assert!(!valid_content_type_key("2post"));
        assert!(!valid_content_type_key("Post"));
        assert!(!valid_content_type_key("post/type"));
    }

    fn sample_schema() -> ContentTypeDefinition {
        ContentTypeDefinition {
            key: "portfolio_item".into(),
            label: "Dự án".into(),
            fields: vec![
                ContentFieldDefinition {
                    key: "summary".into(),
                    label: "Tóm tắt".into(),
                    field_type: ContentFieldType::LongText,
                    required: false,
                    options: vec![],
                    ..Default::default()
                },
                ContentFieldDefinition {
                    key: "status".into(),
                    label: "Trạng thái".into(),
                    field_type: ContentFieldType::Select,
                    required: true,
                    options: vec![
                        SelectOption {
                            value: "planned".into(),
                            label: "Dự kiến".into(),
                        },
                        SelectOption {
                            value: "published".into(),
                            label: "Đã đăng".into(),
                        },
                    ],
                    ..Default::default()
                },
            ],
            settings: ContentTypeSettings::default(),
        }
    }

    #[test]
    fn content_schema_round_trips_and_validates() {
        let schema = sample_schema();
        assert!(schema.validate().is_ok());
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("\"type\":\"long_text\""));
        assert!(json.contains("\"public_exposure\":\"public\""));
        let decoded: ContentTypeDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, schema);
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn context_aware_field_definition_input_normalization() {
        // Case 1: Base CPT field without public_exposure -> Public
        let base_missing: FieldDefinitionInput = serde_json::from_value(json!({
            "key": "subtitle",
            "label": "Phụ đề",
            "type": "short_text"
        }))
        .unwrap();
        let canonical_base = base_missing
            .into_canonical(FieldScopeKind::BaseCptField)
            .unwrap();
        assert_eq!(canonical_base.public_exposure, PublicExposure::Public);

        // Case 2: Extension group field without public_exposure -> Private
        let ext_missing: FieldDefinitionInput = serde_json::from_value(json!({
            "key": "internal_note",
            "label": "Ghi chú nội bộ",
            "type": "long_text"
        }))
        .unwrap();
        let canonical_ext_priv = ext_missing
            .into_canonical(FieldScopeKind::ExtensionGroupField)
            .unwrap();
        assert_eq!(canonical_ext_priv.public_exposure, PublicExposure::Private);

        // Case 3: Extension group field with explicit "public_exposure": "public" -> Public
        let ext_explicit_pub: FieldDefinitionInput = serde_json::from_value(json!({
            "key": "hero_badge",
            "label": "Nhãn Hero",
            "type": "short_text",
            "public_exposure": "public"
        }))
        .unwrap();
        let canonical_ext_pub = ext_explicit_pub
            .into_canonical(FieldScopeKind::ExtensionGroupField)
            .unwrap();
        assert_eq!(canonical_ext_pub.public_exposure, PublicExposure::Public);
    }

    #[test]
    fn supports_all_nine_v1_field_kinds() {
        let kinds = [
            ContentFieldType::ShortText,
            ContentFieldType::LongText,
            ContentFieldType::RichText,
            ContentFieldType::Number,
            ContentFieldType::Boolean,
            ContentFieldType::Date,
            ContentFieldType::Select,
            ContentFieldType::Image,
            ContentFieldType::Gallery,
        ];
        for kind in kinds {
            let mut field = sample_schema().fields[0].clone();
            field.field_type = kind;
            if kind == ContentFieldType::Select {
                field.options = sample_schema().fields[1].options.clone();
            }
            assert!(field.validate().is_ok());
        }
    }

    #[test]
    fn rejects_duplicate_keys_and_invalid_select_options() {
        let mut schema = sample_schema();
        schema.fields.push(schema.fields[0].clone());
        assert!(schema.validate().is_err());

        let mut schema = sample_schema();
        schema.fields[1].options[1].value = "planned".into();
        assert!(schema.validate().is_err());

        schema.fields[1].options.clear();
        assert!(schema.validate().is_err());

        schema.fields[0].options.push(SelectOption {
            value: "wrong".into(),
            label: "Sai loại".into(),
        });
        assert!(schema.validate().is_err());
    }

    #[test]
    fn rejects_invalid_identifiers_and_labels() {
        let mut schema = sample_schema();
        schema.key = "Invalid-Type".into();
        assert!(schema.validate().is_err());

        schema.key = "post".into();
        schema.label = "   ".into();
        assert!(schema.validate().is_err());

        schema.label = "Bài viết".into();
        schema.fields[0].key = "bad-name".into();
        assert!(schema.validate().is_err());
    }

    fn entry() -> ContentEntryInput {
        ContentEntryInput {
            title: "Dự án đầu tiên".into(),
            slug: "du-an-dau-tien".into(),
            values: BTreeMap::from([
                ("summary".into(), json!("Nội dung")),
                ("status".into(), json!("planned")),
            ]),
            term_ids: None,
        }
    }

    #[test]
    fn validates_entry_and_all_nine_field_types() {
        let mut schema = sample_schema();
        let cases = [
            ("headline", ContentFieldType::ShortText, json!("Hello")),
            ("body", ContentFieldType::RichText, json!("<b>Text</b>")),
            ("budget", ContentFieldType::Number, json!(18.5)),
            ("featured", ContentFieldType::Boolean, json!(false)),
            ("due_date", ContentFieldType::Date, json!("2024-02-29")),
            ("photo", ContentFieldType::Image, json!("asset_1")),
            (
                "album",
                ContentFieldType::Gallery,
                json!(["asset_1", "asset_2"]),
            ),
        ];
        let mut entry = entry();
        for (key, kind, value) in cases {
            schema.fields.push(ContentFieldDefinition {
                key: key.into(),
                label: "Trường thử".into(),
                field_type: kind,
                required: false,
                options: vec![],
                ..Default::default()
            });
            entry.values.insert(key.into(), value);
        }
        assert!(entry.validate_against(&schema).is_ok());
        let encoded = serde_json::to_string(&entry).unwrap();
        assert_eq!(
            serde_json::from_str::<ContentEntryInput>(&encoded).unwrap(),
            entry
        );
    }

    #[test]
    fn rejects_invalid_custom_field_values_and_missing_required() {
        let schema = sample_schema();
        let mut entry = entry();
        entry.values.remove("status");
        assert!(entry.validate_against(&schema).is_err());
        entry.values.insert("status".into(), json!("unknown"));
        assert!(entry.validate_against(&schema).is_err());
        entry.values.insert("status".into(), json!("planned"));
        entry.values.insert("unregistered".into(), json!("bad"));
        assert!(entry.validate_against(&schema).is_err());
        entry.values.remove("unregistered");
        entry.slug = "Invalid_Slug".into();
        assert!(entry.validate_against(&schema).is_err());
        entry.slug = "valid-slug".into();
        entry.values.insert("summary".into(), json!(true));
        assert!(entry.validate_against(&schema).is_err());
    }

    #[test]
    fn rejects_invalid_dates_galleries_and_media_ids() {
        let mut schema = sample_schema();
        schema.fields.push(ContentFieldDefinition {
            key: "date".into(),
            label: "Ngày".into(),
            field_type: ContentFieldType::Date,
            required: true,
            options: vec![],
            ..Default::default()
        });
        schema.fields.push(ContentFieldDefinition {
            key: "photos".into(),
            label: "Album".into(),
            field_type: ContentFieldType::Gallery,
            required: false,
            options: vec![],
            ..Default::default()
        });
        let mut entry = entry();
        for bad in ["2025-02-29", "2024-13-01", "2024-02-30", "0000-01-01"] {
            entry.values.insert("date".into(), json!(bad));
            assert!(entry.validate_against(&schema).is_err(), "{bad}");
        }
        entry.values.insert("date".into(), json!("2000-02-29"));
        entry
            .values
            .insert("photos".into(), json!(["asset1", "asset1"]));
        assert!(entry.validate_against(&schema).is_err());
        entry.values.insert("photos".into(), json!(["../asset"]));
        assert!(entry.validate_against(&schema).is_err());
        entry
            .values
            .insert("photos".into(), json!(["asset1", "asset2"]));
        assert!(entry.validate_against(&schema).is_ok());
    }

    #[test]
    fn patch_field_distinguishes_omitted_clear_and_set() {
        #[derive(Debug, Deserialize)]
        struct Probe {
            #[serde(default, deserialize_with = "deserialize_patch_field")]
            path: PatchField<String>,
        }
        let omitted: Probe = serde_json::from_str("{}").unwrap();
        assert_eq!(omitted.path, PatchField::Omitted);

        let cleared: Probe = serde_json::from_str(r#"{"path": null}"#).unwrap();
        assert_eq!(cleared.path, PatchField::Clear);

        let set: Probe = serde_json::from_str(r#"{"path": "/hello"}"#).unwrap();
        assert_eq!(set.path, PatchField::Set("/hello".to_string()));
    }

    #[test]
    fn richtext_allows_384kib_when_containing_blocks_and_keeps_200k_for_plain() {
        let field = ContentFieldDefinition {
            key: "body".into(),
            label: "Nội dung".into(),
            field_type: ContentFieldType::RichText,
            required: false,
            options: vec![],
            ..Default::default()
        };
        let plain_over_200k = "a".repeat(200_001);
        assert!(!valid_field_value(&field, &json!(plain_over_200k)));

        let block_250k = format!(
            "<!-- od:text {{\"id\":\"t1\"}} -->\n{}\n<!-- /od:text -->",
            "a".repeat(250_000)
        );
        assert!(valid_field_value(&field, &json!(block_250k)));

        let block_over_384k = format!(
            "<!-- od:text {{\"id\":\"t1\"}} -->\n{}\n<!-- /od:text -->",
            "a".repeat(crate::builder_blocks::MAX_BUILDER_BLOCK_BODY_BYTES)
        );
        assert!(!valid_field_value(&field, &json!(block_over_384k)));
    }

    #[test]
    fn cpt_builder_auto_injects_body_rejects_non_richtext_and_emits_body_panel() {
        let builder_settings = ContentTypeSettings {
            public: true,
            has_archive: true,
            publication: ContentPublication::DraftPublish,
            supports: vec![ContentSupport::Builder, ContentSupport::FeaturedImage],
        };
        let base_no_body = OwnedDescriptor {
            owner: ResourceOwner::User,
            status: ProviderStatus::Active,
            declaration_state: DeclarationState::Declared,
            descriptor: ContentTypeDefinition {
                key: "service".into(),
                label: "Dịch vụ".into(),
                fields: vec![],
                settings: builder_settings.clone(),
            },
        };
        let eff = EffectiveContentSchema::assemble(base_no_body.clone(), 1, vec![]).unwrap();
        let injected = eff.effective_fields.get("body").expect("body injected");
        assert_eq!(injected.definition.field_type, ContentFieldType::RichText);
        assert_eq!(injected.owner, ResourceOwner::Core);
        assert!(
            build_core_and_group_panels(&eff, false, &[])
                .iter()
                .any(|p| p.panel_id == "core:body_editor")
        );

        // Disabling Builder on the custom CPT allows removing the synthetic Core `body`
        // even when entries already exist.
        let mut disabled_base = base_no_body;
        disabled_base.descriptor.settings.supports = vec![ContentSupport::FeaturedImage];
        let eff_disabled = EffectiveContentSchema::assemble(disabled_base, 2, vec![]).unwrap();
        assert!(!eff_disabled.effective_fields.contains_key("body"));
        verify_effective_schema_transition(Some(&eff), &eff_disabled, true).unwrap();

        // Non-RichText `body` is rejected when ContentSupport::Builder is enabled.
        let bad_base = OwnedDescriptor {
            owner: ResourceOwner::User,
            status: ProviderStatus::Active,
            declaration_state: DeclarationState::Declared,
            descriptor: ContentTypeDefinition {
                key: "bad_cpt".into(),
                label: "Bad".into(),
                fields: vec![ContentFieldDefinition {
                    key: "body".into(),
                    label: "Short body".into(),
                    field_type: ContentFieldType::ShortText,
                    required: false,
                    options: vec![],
                    ..Default::default()
                }],
                settings: builder_settings,
            },
        };
        assert!(matches!(
            EffectiveContentSchema::assemble(bad_base, 1, vec![]),
            Err(SchemaTransitionError::IncompatibleSchemaChange(_))
        ));
    }
}
