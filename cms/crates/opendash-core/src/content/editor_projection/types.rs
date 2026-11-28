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

pub(crate) const fn default_declaration_declared() -> DeclarationState {
    DeclarationState::Declared
}

pub(crate) const fn default_panel_order_50() -> i32 {
    50
}

pub(crate) const fn default_true() -> bool {
    true
}

pub(crate) fn is_false_bool(value: &bool) -> bool {
    !*value
}

pub(crate) fn is_true_bool(value: &bool) -> bool {
    *value
}

pub(crate) const fn default_archive_per_page_12() -> u16 {
    12
}

pub(crate) fn is_default_archive_per_page_12(value: &u16) -> bool {
    *value == 12
}

pub(crate) const fn default_menu_position_50() -> i32 {
    50
}

pub(crate) fn is_default_menu_position_50(value: &i32) -> bool {
    *value == 50
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
    Excerpt,
    MenuOrder,
    CustomTemplate,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentDefaultSort {
    #[default]
    PublishedAtDesc,
    CreatedAtDesc,
    UpdatedAtDesc,
    TitleAsc,
    MenuOrderAsc,
}

impl ContentDefaultSort {
    pub fn is_default(&self) -> bool {
        *self == Self::PublishedAtDesc
    }
}

pub const MAX_CPT_LIST_COLUMNS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentListColumnKind {
    Field,
    Taxonomy,
    MenuOrder,
    Parent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentListColumn {
    pub kind: ContentListColumnKind,
    #[serde(default)]
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
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
    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub hierarchical: bool,
    #[serde(
        default = "default_archive_per_page_12",
        skip_serializing_if = "is_default_archive_per_page_12"
    )]
    pub archive_per_page: u16,
    #[serde(default, skip_serializing_if = "ContentDefaultSort::is_default")]
    pub default_sort: ContentDefaultSort,
    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub exclude_from_sitemap: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_icon: Option<String>,
    #[serde(
        default = "default_menu_position_50",
        skip_serializing_if = "is_default_menu_position_50"
    )]
    pub menu_position: i32,
    #[serde(default = "default_true", skip_serializing_if = "is_true_bool")]
    pub show_in_menu: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub list_columns: Vec<ContentListColumn>,
}

impl Default for ContentTypeSettings {
    fn default() -> Self {
        Self {
            public: true,
            has_archive: true,
            publication: ContentPublication::Immediate,
            supports: vec![ContentSupport::Editor, ContentSupport::FeaturedImage],
            hierarchical: false,
            archive_per_page: 12,
            default_sort: ContentDefaultSort::PublishedAtDesc,
            exclude_from_sitemap: false,
            menu_name: None,
            menu_icon: None,
            menu_position: 50,
            show_in_menu: true,
            list_columns: Vec::new(),
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
            archive_per_page: 20,
            ..Self::default()
        }
    }

    pub fn builtin_page() -> Self {
        let mut settings = Self::builtin_publication();
        // Page has no archive of its own; its Builder capability is an
        // entry-owned content surface, not a shared template or archive.
        settings.has_archive = false;
        settings.archive_per_page = 12;
        settings.supports.push(ContentSupport::Builder);
        settings
    }

    pub fn private_pattern() -> Self {
        Self {
            public: false,
            has_archive: false,
            publication: ContentPublication::Immediate,
            supports: vec![ContentSupport::Builder],
            hierarchical: false,
            archive_per_page: 12,
            default_sort: ContentDefaultSort::PublishedAtDesc,
            exclude_from_sitemap: true,
            menu_name: None,
            menu_icon: None,
            menu_position: 50,
            show_in_menu: false,
            list_columns: Vec::new(),
        }
    }

    pub fn validate_and_normalize(&mut self) -> Result<(), &'static str> {
        let original_len = self.supports.len();
        self.supports.sort_unstable();
        self.supports.dedup();
        if self.supports.len() != original_len {
            return Err("supports không được chứa tính năng trùng lặp.");
        }
        if !(1..=100).contains(&self.archive_per_page) {
            return Err("archive_per_page phải nằm trong khoảng 1..=100.");
        }
        if !(0..=1000).contains(&self.menu_position) {
            return Err("menu_position phải nằm trong khoảng 0..=1000.");
        }
        if let Some(raw) = self.menu_name.as_deref() {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                self.menu_name = None;
            } else if !valid_label(trimmed) {
                return Err("menu_name không hợp lệ (tối đa 100 ký tự).");
            } else {
                self.menu_name = Some(trimmed.to_owned());
            }
        }
        if let Some(raw) = self.menu_icon.as_deref() {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                self.menu_icon = None;
            } else if crate::icons::lookup_core_icon_paths(trimmed).is_none() {
                return Err("menu_icon phải thuộc danh mục CORE_ICONS.");
            } else {
                self.menu_icon = Some(trimmed.to_owned());
            }
        }
        if self.list_columns.len() > MAX_CPT_LIST_COLUMNS {
            return Err("Mỗi loại nội dung tối đa 8 list_columns.");
        }
        let mut seen_cols = HashSet::new();
        for col in &mut self.list_columns {
            match col.kind {
                ContentListColumnKind::Field | ContentListColumnKind::Taxonomy => {
                    if !valid_snake_key(&col.key) {
                        return Err("Khóa cột trong list_columns phải là snake_case hợp lệ.");
                    }
                }
                ContentListColumnKind::MenuOrder | ContentListColumnKind::Parent => {
                    if !col.key.is_empty() {
                        return Err("Cột menu_order/parent trong list_columns không nhận key.");
                    }
                }
            }
            if let Some(raw_lbl) = col.label.as_deref() {
                let trimmed = raw_lbl.trim();
                if trimmed.is_empty() {
                    col.label = None;
                } else if !valid_label(trimmed) {
                    return Err("Nhãn cột trong list_columns không hợp lệ.");
                } else {
                    col.label = Some(trimmed.to_owned());
                }
            }
            if !seen_cols.insert((col.kind, col.key.clone())) {
                return Err("list_columns không được chứa cột trùng lặp.");
            }
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
    InvalidLocationRulesDomain(String),
    SyncEntryTermsLocationCycle(String),
    DuplicateSyncEntryTermsTaxonomy(String),
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
            Self::InvalidLocationRulesDomain(_) => "invalid_location_rules_domain",
            Self::SyncEntryTermsLocationCycle(_) => "sync_entry_terms_location_cycle",
            Self::DuplicateSyncEntryTermsTaxonomy(_) => "duplicate_sync_entry_terms_taxonomy",
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
            | Self::RequiredFieldHiddenOrInaccessible(m)
            | Self::InvalidLocationRulesDomain(m)
            | Self::SyncEntryTermsLocationCycle(m)
            | Self::DuplicateSyncEntryTermsTaxonomy(m) => format!("{}: {m}", self.code()),
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
    } else if let Some(rest) = err.strip_prefix("cannot_remove_field_with_data:") {
        SchemaTransitionError::CannotRemoveFieldWithData(rest.trim().to_owned())
    } else if let Some(rest) = err.strip_prefix("invalid_location_rules_domain:") {
        SchemaTransitionError::InvalidLocationRulesDomain(rest.trim().to_owned())
    } else if let Some(rest) = err.strip_prefix("sync_entry_terms_location_cycle:") {
        SchemaTransitionError::SyncEntryTermsLocationCycle(rest.trim().to_owned())
    } else if let Some(rest) = err.strip_prefix("duplicate_sync_entry_terms_taxonomy:") {
        SchemaTransitionError::DuplicateSyncEntryTermsTaxonomy(rest.trim().to_owned())
    } else {
        SchemaTransitionError::IncompatibleSchemaChange(err.to_owned())
    }
}
