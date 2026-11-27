use super::*;

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

/// Wire input DTO for `POST /api/admin/content-types` and `PUT /api/admin/content-types/{type_key}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentTypeDefinitionInput {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub fields: Vec<crate::field_schema::FieldSpec>,
    #[serde(default)]
    pub settings: Option<ContentTypeSettings>,
}

/// Schema is independent of the theme/plugin that declared it. Ownership and
/// provider status are managed by the existing `RegistryCatalog` instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentTypeDefinition {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub fields: Vec<crate::field_schema::FieldSpec>,
    /// Legacy definitions without this property deserialize to public immediate.
    #[serde(default)]
    pub settings: ContentTypeSettings,
}

/// Wire DTO for `field_groups` in `theme.json` (v2) and `plugin.json` (`manifest_schema: 2`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub fields: Vec<crate::field_schema::FieldSpec>,
}

/// Wire DTO for User `POST /api/admin/content-field-groups` and `PUT /api/admin/content-field-groups/{group_id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub fields: Vec<crate::field_schema::FieldSpec>,
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
    ) -> Result<(ScopedResourceId, Vec<crate::field_schema::FieldSpec>), &'static str> {
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
        let fields = as_manifest.validate_and_normalize_for_scope(
            crate::field_schema::FieldScopeKind::UserFieldGroup,
        )?;
        Ok((scoped_id, fields))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldGroupTargetBinding {
    pub type_key: String,
    pub association_state: DeclarationState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldGroupRecord {
    pub group_id: String,
    pub owner: ResourceOwner,
    pub local_key: String,
    pub title: String,
    pub description: String,
    pub fields: Vec<crate::field_schema::FieldSpec>,
    pub targets: Vec<FieldGroupTargetBinding>,
    pub default_region: EditorRegion,
    pub default_order: i32,
    pub collapsible: bool,
    pub status: FieldGroupStatus,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachedFieldGroupSnapshot {
    pub group_id: String,
    pub owner: ResourceOwner,
    pub local_key: String,
    pub title: String,
    pub description: String,
    pub fields: Vec<crate::field_schema::FieldSpec>,
    pub default_region: EditorRegion,
    pub default_order: i32,
    pub collapsible: bool,
    pub status: FieldGroupStatus,
    pub association_state: DeclarationState,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveContentSchema {
    pub base: OwnedDescriptor<ContentTypeDefinition>,
    pub base_version: i64,
    pub attached_groups: Vec<AttachedFieldGroupSnapshot>,
    pub effective_fields: BTreeMap<String, EffectiveFieldEntry>,
    #[serde(default)]
    pub ui_fields: Vec<crate::field_schema::FieldSpec>,
    pub schema_token: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
            if !schema
                .fields
                .iter()
                .any(|field| !field.is_ui_only() && field.key == *key)
            {
                return Err("Giá trị chứa custom field chưa được khai báo.");
            }
        }
        for field in &schema.fields {
            if field.is_ui_only() {
                continue;
            }
            let value = self.values.get(&field.key).unwrap_or(&Value::Null);
            if value.is_null() {
                if field.effective_required() {
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
