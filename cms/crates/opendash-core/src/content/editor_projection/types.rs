use super::*;

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

/// Computed status of a field in `EffectiveContentSchema`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveFieldStatus {
    Active,
    ProviderInactive,
    Withdrawn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

pub(crate) const fn default_declaration_declared() -> DeclarationState {
    DeclarationState::Declared
}

pub(crate) const fn default_panel_order_50() -> i32 {
    50
}

pub(crate) const fn default_true() -> bool {
    true
}

pub fn valid_default_value(field: &crate::field_schema::FieldSpec, value: &Value) -> bool {
    if value.is_null() {
        return false;
    }
    valid_field_value(field, value)
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveFieldEntry {
    pub definition: crate::field_schema::FieldSpec,
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

pub(crate) fn classify_validation_error(err: &str) -> SchemaTransitionError {
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
