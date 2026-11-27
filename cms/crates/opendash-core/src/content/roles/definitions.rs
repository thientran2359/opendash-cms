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

pub use crate::field_schema::{FieldRole, PublicExposure};
pub type Role = FieldRole;

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
        let mut canonical_fields = self.fields;
        crate::field_schema::normalize_field_specs_for_scope(
            &mut canonical_fields,
            crate::field_schema::FieldContext::Content,
            crate::field_schema::FieldScopeKind::BaseCptField,
        )?;
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
        for field in &self.fields {
            if !is_builtin_post_or_page
                && !field.is_ui_only()
                && RESERVED_ENTRY_KEYS.contains(&field.key.as_str())
            {
                return Err(
                    "field_key_collision: Khóa custom field trùng với trường hệ thống được bảo vệ.",
                );
            }
        }
        crate::field_schema::validate_field_specs(
            &self.fields,
            crate::field_schema::FieldContext::Content,
        )?;
        Ok(())
    }
}

impl ManifestFieldGroupInput {
    pub fn validate_and_normalize(
        &self,
    ) -> Result<Vec<crate::field_schema::FieldSpec>, &'static str> {
        self.validate_and_normalize_for_scope(
            crate::field_schema::FieldScopeKind::ExtensionGroupField,
        )
    }

    pub fn validate_and_normalize_for_scope(
        &self,
        scope: crate::field_schema::FieldScopeKind,
    ) -> Result<Vec<crate::field_schema::FieldSpec>, &'static str> {
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
        let mut canonical_fields = self.fields.clone();
        crate::field_schema::normalize_field_specs_for_scope(
            &mut canonical_fields,
            crate::field_schema::FieldContext::Content,
            scope,
        )?;
        Ok(canonical_fields)
    }
}
