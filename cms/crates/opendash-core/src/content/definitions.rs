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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub singular_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub taxonomies: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_field",
        skip_serializing_if = "PatchField::is_omitted"
    )]
    pub rewrite_base: PatchField<String>,
    #[serde(default)]
    pub settings: Option<ContentTypeSettings>,
}

/// Clean-Slate V1 CPT descriptor (no embedded `fields`; custom fields live in Field Groups).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentTypeDefinition {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub singular_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub settings: ContentTypeSettings,
}

/// Wire DTO for `field_groups` in `theme.json` and `plugin.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFieldGroupInput {
    pub local_key: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationRuleGroups>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_types: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<FieldGroupPresentation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_region: Option<EditorRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_order: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapsible: Option<bool>,
    #[serde(default = "default_true", skip_serializing_if = "is_true_bool")]
    pub active: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationRuleGroups>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_types: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<FieldGroupPresentation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_region: Option<EditorRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_order: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapsible: Option<bool>,
    #[serde(default = "default_true", skip_serializing_if = "is_true_bool")]
    pub active: bool,
    pub fields: Vec<crate::field_schema::FieldSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedFieldGroup {
    pub domain: FieldGroupDomain,
    pub location: LocationRuleGroups,
    pub presentation: FieldGroupPresentation,
    pub active: bool,
    pub fields: Vec<crate::field_schema::FieldSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldGroupTargetBinding {
    pub target_kind: String,
    pub target_key: String,
    pub association_state: DeclarationState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldGroupRecord {
    pub group_id: String,
    pub owner: ResourceOwner,
    pub local_key: String,
    pub title: String,
    pub description: String,
    pub domain: FieldGroupDomain,
    pub location: LocationRuleGroups,
    pub presentation: FieldGroupPresentation,
    pub active: bool,
    pub fields: Vec<crate::field_schema::FieldSpec>,
    pub targets: Vec<FieldGroupTargetBinding>,
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
    pub domain: FieldGroupDomain,
    pub location: LocationRuleGroups,
    pub presentation: FieldGroupPresentation,
    pub active: bool,
    pub fields: Vec<crate::field_schema::FieldSpec>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options_pages: Vec<CustomOptionsPageDeclaration>,
}
