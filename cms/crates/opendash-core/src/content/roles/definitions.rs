use super::*;

pub fn valid_content_type_key(value: &str) -> bool {
    valid_snake_key(value)
}

/// Core-managed fields controlled exclusively via `ContentTypeSettings.supports`.
pub const CORE_MANAGED_ENTRY_KEYS: &[&str] = &["body", "featured_image", "excerpt"];

/// Reserved entry-level keys that cannot be used as custom field keys in any Field Group.
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
    "body",
    "featured_image",
    "excerpt",
    "parent_id",
    "menu_order",
    "trashed_at",
    "published_at",
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

fn normalize_optional_singular_label(raw: Option<String>) -> Result<Option<String>, &'static str> {
    let Some(s) = raw else { return Ok(None) };
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if !valid_label(trimmed) {
        return Err("singular_label loại nội dung không hợp lệ.");
    }
    Ok(Some(trimmed.to_owned()))
}

fn normalize_optional_description(raw: Option<String>) -> Result<Option<String>, &'static str> {
    let Some(s) = raw else { return Ok(None) };
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if s.chars().count() > 400 || s.chars().any(|c| c.is_control() && c != '\n') {
        return Err("Mô tả loại nội dung tối đa 400 ký tự và không chứa ký tự điều khiển.");
    }
    Ok(Some(s))
}

impl ContentTypeDefinitionInput {
    pub fn into_canonical(self) -> Result<ContentTypeDefinition, &'static str> {
        if !valid_content_type_key(&self.key) {
            return Err("Khóa loại nội dung không hợp lệ.");
        }
        if !valid_label(&self.label) {
            return Err("Tên loại nội dung không hợp lệ.");
        }
        let singular_label = normalize_optional_singular_label(self.singular_label)?;
        let description = normalize_optional_description(self.description)?;
        if let Some(taxonomies) = &self.taxonomies {
            validate_provider_taxonomy_keys(taxonomies)?;
        }
        if let PatchField::Set(ref base) = self.rewrite_base
            && !base.trim().is_empty()
        {
            validate_provider_rewrite_base(Some(base.trim()))?;
        }
        let mut settings = self
            .settings
            .unwrap_or_else(|| ContentTypeSettings::default_for_type_key(&self.key));
        settings.validate_and_normalize()?;
        let def = ContentTypeDefinition {
            key: self.key,
            label: self.label,
            singular_label,
            description,
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
        if let Some(sl) = self.singular_label.as_deref()
            && !valid_label(sl)
        {
            return Err("singular_label loại nội dung không hợp lệ.");
        }
        if let Some(desc) = self.description.as_deref()
            && (desc.chars().count() > 400 || desc.chars().any(|c| c.is_control() && c != '\n'))
        {
            return Err("Mô tả loại nội dung tối đa 400 ký tự và không chứa ký tự điều khiển.");
        }
        let mut settings = self.settings.clone();
        settings.validate_and_normalize()?;
        if settings != self.settings {
            return Err("supports hoặc settings chưa được chuẩn hóa.");
        }
        if matches!(self.key.as_str(), "post" | "page")
            && self.settings.publication != ContentPublication::DraftPublish
        {
            return Err("post/page phải sử dụng cơ chế Lưu nháp và Xuất bản.");
        }
        if self.key == "post" && self.settings.hierarchical {
            return Err("post không hỗ trợ bật phân cấp cha-con.");
        }
        if self.key == "page" && self.settings.supports(ContentSupport::CustomTemplate) {
            return Err("page không hỗ trợ custom_template (sử dụng page_layout).");
        }
        if self.key == "pattern" && self.settings != ContentTypeSettings::private_pattern() {
            return Err("Pattern phải luôn riêng tư và chỉ hỗ trợ Builder.");
        }
        Ok(())
    }
}

fn resolve_location_input(
    location: Option<&LocationRuleGroups>,
    target_types: Option<&[String]>,
) -> Result<LocationRuleGroups, &'static str> {
    match (location, target_types) {
        (Some(_), Some(_)) | (None, None) => Err(
            "invalid_location_rules_domain: Cần cung cấp đúng một trong location hoặc target_types.",
        ),
        (Some(loc), None) => Ok(loc.clone()),
        (None, Some(types)) => {
            if types.is_empty() || types.len() > 16 {
                return Err(
                    "invalid_location_rules_domain: target_types phải có từ 1 đến 16 loại nội dung.",
                );
            }
            if types.iter().any(|t| t == "*") {
                if types.len() != 1 {
                    return Err(
                        "invalid_location_rules_domain: target_types wildcard '*' không được trộn với loại nội dung cụ thể.",
                    );
                }
                return Ok(vec![vec![LocationRule {
                    param: LocationParam::ContentType,
                    op: LocationOp::Eq,
                    value: "all".to_owned(),
                }]]);
            }
            let mut seen = HashSet::new();
            let mut groups = Vec::with_capacity(types.len());
            for t in types {
                if matches!(t.as_str(), "pattern" | "section")
                    || !valid_content_type_key(t)
                    || !seen.insert(t.as_str())
                {
                    return Err(
                        "invalid_location_rules_domain: target_types không hợp lệ hoặc bị trùng lặp.",
                    );
                }
                groups.push(vec![LocationRule {
                    param: LocationParam::ContentType,
                    op: LocationOp::Eq,
                    value: t.clone(),
                }]);
            }
            Ok(groups)
        }
    }
}

fn resolve_presentation_input(
    presentation: Option<FieldGroupPresentation>,
    default_region: Option<EditorRegion>,
    default_order: Option<i32>,
    collapsible: Option<bool>,
) -> Result<FieldGroupPresentation, &'static str> {
    if presentation.is_some()
        && (default_region.is_some() || default_order.is_some() || collapsible.is_some())
    {
        return Err(
            "invalid_field_group_presentation: Không được trộn lẫn presentation với default_region/default_order/collapsible.",
        );
    }
    let resolved = if let Some(pres) = presentation {
        pres
    } else {
        FieldGroupPresentation {
            region: default_region.unwrap_or(EditorRegion::MainBottom),
            order: default_order.unwrap_or(50),
            collapsible: collapsible.unwrap_or(true),
            ..FieldGroupPresentation::default()
        }
    };
    resolved.validate()?;
    Ok(resolved)
}

fn has_any_public_field(specs: &[crate::field_schema::FieldSpec]) -> bool {
    specs.iter().any(|spec| {
        !spec.is_ui_only()
            && (spec.public
                || spec.effective_acl().2 == PublicExposure::Public
                || has_any_public_field(&spec.fields))
    })
}

fn has_any_sync_entry_terms(specs: &[crate::field_schema::FieldSpec]) -> bool {
    specs.iter().any(|spec| spec.sync_entry_terms)
}

impl ManifestFieldGroupInput {
    pub fn validate_and_normalize(
        &self,
    ) -> Result<Vec<crate::field_schema::FieldSpec>, &'static str> {
        self.validate_and_normalize_full(crate::field_schema::FieldScopeKind::ExtensionGroupField)
            .map(|n| n.fields)
    }

    pub fn validate_and_normalize_for_scope(
        &self,
        scope: crate::field_schema::FieldScopeKind,
    ) -> Result<Vec<crate::field_schema::FieldSpec>, &'static str> {
        self.validate_and_normalize_full(scope).map(|n| n.fields)
    }

    pub fn validate_and_normalize_full(
        &self,
        scope: crate::field_schema::FieldScopeKind,
    ) -> Result<NormalizedFieldGroup, &'static str> {
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
        let location =
            resolve_location_input(self.location.as_ref(), self.target_types.as_deref())?;
        let domain = validate_location_rules_and_infer_domain(&location).map_err(|e| match e {
            SchemaTransitionError::InvalidLocationRulesDomain(_) => {
                "invalid_location_rules_domain: Quy tắc hiển thị nhóm trường không hợp lệ hoặc trộn lẫn phạm vi."
            }
            _ => "invalid_location_rules_domain: Quy tắc hiển thị nhóm trường không hợp lệ.",
        })?;
        let presentation = resolve_presentation_input(
            self.presentation,
            self.default_region,
            self.default_order,
            self.collapsible,
        )?;
        let data_count = self.fields.iter().filter(|f| !f.is_ui_only()).count();
        if data_count == 0 || data_count > 64 {
            return Err("Mỗi field_group phải khai báo từ 1 đến 64 custom fields.");
        }

        let field_ctx = match domain {
            FieldGroupDomain::Content => crate::field_schema::FieldContext::Content,
            FieldGroupDomain::Term => crate::field_schema::FieldContext::Term,
            FieldGroupDomain::OptionsPage => crate::field_schema::FieldContext::Options,
        };
        let mut canonical_fields = self.fields.clone();
        crate::field_schema::normalize_field_specs_for_scope(
            &mut canonical_fields,
            field_ctx,
            scope,
        )?;

        if is_restricted_to_admin_only(&location) && has_any_public_field(&canonical_fields) {
            return Err(
                "invalid_field_acl: Nhóm trường chỉ dành cho Admin không được chứa trường công khai (public).",
            );
        }
        if has_any_sync_entry_terms(&canonical_fields)
            && location_rules_have_param(&location, LocationParam::EntryTaxonomy)
        {
            return Err(
                "sync_entry_terms_location_cycle: Không thể bật đồng bộ term_ref bên trong nhóm trường dùng điều kiện entry_taxonomy.",
            );
        }
        if domain == FieldGroupDomain::Content {
            for spec in &canonical_fields {
                if !spec.is_ui_only()
                    && (RESERVED_ENTRY_KEYS.contains(&spec.key.as_str()) || spec.key == "status")
                {
                    return Err(
                        "field_key_collision: Khóa trường trùng với khóa hệ thống được bảo vệ.",
                    );
                }
            }
        } else if domain == FieldGroupDomain::Term {
            for spec in &canonical_fields {
                if !spec.is_ui_only() && RESERVED_TERM_KEYS.contains(&spec.key.as_str()) {
                    return Err(
                        "field_key_collision: Khóa trường trùng với khóa term hệ thống được bảo vệ.",
                    );
                }
            }
        } else if domain == FieldGroupDomain::OptionsPage
            && location
                .iter()
                .flatten()
                .any(|r| r.param == LocationParam::OptionsPage && r.value == "site_global")
        {
            let global_keys: HashSet<String> = crate::field_schema::global_site_option_fields()
                .into_iter()
                .filter(|s| !s.is_ui_only())
                .map(|s| s.key)
                .collect();
            for spec in &canonical_fields {
                if !spec.is_ui_only() && global_keys.contains(&spec.key) {
                    return Err(
                        "field_key_collision: Khóa trường trùng với khóa cấu hình site_global mặc định.",
                    );
                }
            }
        }

        Ok(NormalizedFieldGroup {
            domain,
            location,
            presentation,
            active: self.active,
            fields: canonical_fields,
        })
    }
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
    ) -> Result<(ScopedResourceId, NormalizedFieldGroup), &'static str> {
        let scoped_id = self.resolve_user_scoped_id(expected_group_id)?;
        let as_manifest = ManifestFieldGroupInput {
            local_key: scoped_id.local_key.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            location: self.location.clone(),
            target_types: self.target_types.clone(),
            presentation: self.presentation,
            default_region: self.default_region,
            default_order: self.default_order,
            collapsible: self.collapsible,
            active: self.active,
            fields: self.fields.clone(),
        };
        let normalized = as_manifest
            .validate_and_normalize_full(crate::field_schema::FieldScopeKind::UserFieldGroup)?;
        Ok((scoped_id, normalized))
    }
}
