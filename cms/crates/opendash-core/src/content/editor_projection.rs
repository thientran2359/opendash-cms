use super::*;

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

pub(super) const fn default_declaration_declared() -> DeclarationState {
    DeclarationState::Declared
}

pub(super) const fn default_panel_order_50() -> i32 {
    50
}

pub(super) const fn default_true() -> bool {
    true
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

pub(super) fn classify_validation_error(err: &str) -> SchemaTransitionError {
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

pub(super) fn taxonomy_public_default() -> bool {
    true
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

pub(super) fn valid_iso_date(value: &str) -> bool {
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

pub(super) const fn default_panel_order_80() -> i32 {
    80
}

pub(super) fn default_action_allowed_regions() -> Vec<EditorRegion> {
    vec![
        EditorRegion::MainTop,
        EditorRegion::MainBottom,
        EditorRegion::Sidebar,
    ]
}

pub(super) fn default_all_content_regions() -> Vec<EditorRegion> {
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

pub(super) fn owner_rank(owner: &ResourceOwner) -> u8 {
    match owner {
        ResourceOwner::Core => 0,
        ResourceOwner::User => 2,
        ResourceOwner::Theme(_) => 3,
        ResourceOwner::Plugin(_) => 4,
    }
}

pub(super) fn panel_sort_key(
    def: &PanelDefinition,
    placement: &PanelPlacementConfig,
) -> (i32, u8, String) {
    let rank = match def.kind {
        PanelKind::BaseFieldsSlice { .. } => 1,
        _ => owner_rank(&def.owner),
    };
    (placement.order, rank, def.panel_id.clone())
}

pub(super) fn has_core_body_panel(effective: &EffectiveContentSchema) -> bool {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutValidationError {
    pub code: &'static str,
    pub message: String,
}

impl LayoutValidationError {
    pub(super) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: format!("{code}: {}", message.into()),
        }
    }
}
