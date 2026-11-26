//! Unified Clean-Slate V1 (`1.0.0`) field schema, validation, canonicalization,
//! DNF conditional logic, ACL invariants (`M7-S09`), schema compatibility
//! checks (`COMPAT-1..4`), and shared helper functions.
//!
//! Strictly pure Rust (`std`, `serde`, `serde_json`, `url`) with zero I/O, HTTP, or SQL.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::{
    builder::{BoxSpacing, compile_box_spacing_tailwind},
    builder_blocks::{MAX_BUILDER_BLOCK_BODY_BYTES, contains_blocks},
    icons::lookup_core_icon_paths,
    id::valid_snake_key,
    richtext::{MAX_RICHTEXT_OUTPUT_BYTES, sanitize_richtext, strip_richtext_to_plain},
};

pub const MAX_FIELD_KEY_LEN: usize = 64;
pub const MAX_LABEL_LEN: usize = 100;
pub const MAX_INSTRUCTIONS_LEN: usize = 400;
pub const MAX_PLACEHOLDER_LEN: usize = 120;
pub const MAX_AFFIX_LEN: usize = 32;
pub const DEFAULT_TEXT_MAX_CHARS: usize = 512;
pub const MAX_TEXT_CHARS: usize = 2_000;
pub const DEFAULT_TEXTAREA_MAX_CHARS: usize = 4_000;
pub const MAX_TEXTAREA_CHARS: usize = 100_000;
pub const MIN_TEXTAREA_ROWS: u8 = 2;
pub const MAX_TEXTAREA_ROWS: u8 = 16;
pub const DEFAULT_TEXTAREA_ROWS: u8 = 4;
pub const MAX_EMAIL_CHARS: usize = 254;
pub const MAX_URL_BYTES: usize = 2048;
pub const MAX_LINK_LABEL_CHARS: usize = 160;
pub const MAX_OPTION_VALUE_CHARS: usize = 64;
pub const MAX_SELECT_OPTIONS: usize = 100;
pub const MAX_RADIO_OPTIONS: usize = 12;
pub const MAX_GALLERY_ITEMS: usize = 50;
pub const MAX_ENTRY_REF_TYPES: usize = 16;
pub const MAX_ENTRY_REF_ITEMS: usize = 50;
pub const MAX_TERM_REF_ITEMS: usize = 50;
pub const MAX_REPEATER_ITEMS: usize = 50;
pub const MAX_BLOCK_REPEATER_ITEMS: usize = 32;
pub const MAX_BLOCK_REPEATER_TOTAL_ITEMS: usize = 64;
pub const MAX_SCHEMA_DATA_FIELDS: usize = 64;
pub const MAX_TERM_PANEL_DATA_FIELDS: usize = 16;
pub const MAX_BLOCK_TOP_DATA_FIELDS: usize = 32;
pub const MAX_BLOCK_LEAF_DATA_FIELDS: usize = 48;
pub const MAX_BLOCK_TOTAL_SPECS: usize = 64;
pub const MAX_CONDITION_GROUPS: usize = 8;
pub const MAX_CONDITION_RULES_PER_GROUP: usize = 8;
pub const MAX_MESSAGE_BODY_CHARS: usize = 1_000;
pub const MAX_BUTTON_LABEL_CHARS: usize = 64;
pub const MAX_ON_OFF_LABEL_CHARS: usize = 64;
pub const MAX_GROUP_DEPTH: usize = 2;

pub const ALLOWED_COLOR_TOKENS: [&str; 6] = [
    "var(--od-token-primary)",
    "var(--od-token-accent)",
    "var(--od-token-surface)",
    "var(--od-token-muted)",
    "var(--od-token-text)",
    "var(--od-token-border)",
];

pub const GLOBAL_PUBLIC_URL_OPTION_KEYS: &[&str] = &[
    "social_facebook",
    "social_youtube",
    "social_tiktok",
    "social_instagram",
    "social_x",
    "social_linkedin",
    "social_email",
];
pub const GLOBAL_PUBLIC_MEDIA_OPTION_KEYS: &[&str] = &["site_logo"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldContext {
    Content,
    Term,
    Options,
    OwnerSettings,
    StudioBlock,
}

impl FieldContext {
    pub fn supports_type(self, field_type: FieldType) -> bool {
        if field_type.is_ui_only() {
            return true;
        }
        match self {
            Self::Content | Self::Term => {
                !matches!(field_type, FieldType::Secret | FieldType::Spacing)
            }
            Self::Options | Self::StudioBlock => field_type != FieldType::Secret,
            Self::OwnerSettings => matches!(
                field_type,
                FieldType::Text
                    | FieldType::Textarea
                    | FieldType::Number
                    | FieldType::Range
                    | FieldType::Boolean
                    | FieldType::Select
                    | FieldType::RadioButtons
                    | FieldType::Checkbox
                    | FieldType::Email
                    | FieldType::Url
                    | FieldType::Media
                    | FieldType::Secret
                    | FieldType::Group
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldScopeKind {
    BaseCptField,
    ExtensionGroupField,
    UserFieldGroup,
    TermPanelField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    Textarea,
    Number,
    Range,
    Email,
    Url,
    Secret,
    Richtext,
    Media,
    Gallery,
    Link,
    Select,
    Checkbox,
    RadioButtons,
    Boolean,
    Color,
    Icon,
    Date,
    Datetime,
    Time,
    Spacing,
    EntryRef,
    TermRef,
    Group,
    Repeater,
    Tab,
    Accordion,
    Message,
}

impl FieldType {
    pub fn is_ui_only(self) -> bool {
        matches!(self, Self::Tab | Self::Accordion | Self::Message)
    }

    pub fn is_container(self) -> bool {
        matches!(self, Self::Group | Self::Repeater)
    }

    pub fn is_scalar_listable(self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Textarea
                | Self::Number
                | Self::Range
                | Self::Email
                | Self::Url
                | Self::Media
                | Self::Link
                | Self::Select
                | Self::Checkbox
                | Self::RadioButtons
                | Self::Boolean
                | Self::Color
                | Self::Icon
                | Self::Date
                | Self::Datetime
                | Self::Time
                | Self::EntryRef
                | Self::TermRef
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpacingProperty {
    #[default]
    Padding,
    Margin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectOptionsSource {
    Menus,
    MenuSlots,
    ContactForms,
    ContentTypes,
    Taxonomies,
    ContextFields,
    TemplateParts,
    Patterns,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaFieldKind {
    #[default]
    Image,
    Video,
    File,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectDisplay {
    #[default]
    Dropdown,
    Buttons,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermPickerStyle {
    #[default]
    Auto,
    Checklist,
    Tags,
    Select,
    Radio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldRole {
    #[default]
    Editor,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicExposure {
    #[default]
    Public,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldDeclarationState {
    #[default]
    Declared,
    Withdrawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RichTextToolbar {
    Minimal,
    #[default]
    Standard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CssUnit {
    #[default]
    Px,
    Rem,
    Em,
    #[serde(rename = "%")]
    Percent,
    Vw,
    Vh,
}

impl CssUnit {
    pub fn as_css_suffix(self) -> &'static str {
        match self {
            Self::Px => "px",
            Self::Rem => "rem",
            Self::Em => "em",
            Self::Percent => "%",
            Self::Vw => "vw",
            Self::Vh => "vh",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckboxLayout {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RadioLayout {
    #[default]
    Buttons,
    Radio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeaterLayout {
    #[default]
    Accordion,
    Table,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldLayoutPlacement {
    #[default]
    Top,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageTone {
    #[default]
    Info,
    Warning,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionOp {
    Eq,
    Neq,
    Empty,
    NotEmpty,
    Contains,
    Gt,
    Lt,
    In,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionRule {
    pub field: String,
    pub op: ConditionOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

pub type ConditionGroups = Vec<Vec<ConditionRule>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkValue {
    pub url: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub new_tab: bool,
    #[serde(default)]
    pub nofollow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpacingValue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<f64>,
}

impl SpacingValue {
    pub fn sides(&self) -> [Option<f64>; 4] {
        [self.top, self.right, self.bottom, self.left]
    }

    pub fn canonicalize(&mut self) {
        self.top = self.top.map(canonicalize_f64);
        self.right = self.right.map(canonicalize_f64);
        self.bottom = self.bottom.map(canonicalize_f64);
        self.left = self.left.map(canonicalize_f64);
    }

    pub fn format_css(&self, unit: CssUnit) -> Option<String> {
        let sides = self.sides();
        if sides.iter().all(Option::is_none) {
            return None;
        }
        let suffix = unit.as_css_suffix();
        let mut parts = Vec::with_capacity(4);
        for side in sides {
            let n = canonicalize_f64(side.unwrap_or(0.0));
            if !n.is_finite() {
                return None;
            }
            if n.fract() == 0.0 && n.abs() < 1e15 {
                parts.push(format!("{}{suffix}", n as i64));
            } else {
                parts.push(format!("{n}{suffix}"));
            }
        }
        Some(parts.join(" "))
    }

    /// Generates minimal mobile-first Tailwind utility classes (`p-*`, `px-*`, `py-*`, `pt-*`,
    /// `m-*`, `-m-*`, `-mt-*`, ...) by delegating to `compile_box_spacing_tailwind`.
    pub fn format_classes(&self, property: SpacingProperty, unit: CssUnit) -> String {
        Self::format_responsive_classes(Some(*self), None, None, property, unit)
    }

    /// Generates responsive mobile-first Tailwind classes across Desktop / Tablet / Mobile.
    pub fn format_responsive_classes(
        desktop: Option<SpacingValue>,
        tablet: Option<SpacingValue>,
        mobile: Option<SpacingValue>,
        property: SpacingProperty,
        unit: CssUnit,
    ) -> String {
        if unit != CssUnit::Px {
            return String::new();
        }
        let to_box = |sp: SpacingValue| -> Option<BoxSpacing> {
            let conv = |v: Option<f64>| -> Result<Option<i16>, ()> {
                let Some(n) = v else { return Ok(None) };
                let c = canonicalize_f64(n);
                if !c.is_finite() || c.fract() != 0.0 || c < i16::MIN as f64 || c > i16::MAX as f64
                {
                    return Err(());
                }
                Ok(Some(c as i16))
            };
            Some(BoxSpacing {
                top: conv(sp.top).ok()?,
                right: conv(sp.right).ok()?,
                bottom: conv(sp.bottom).ok()?,
                left: conv(sp.left).ok()?,
            })
        };
        let d_box = match desktop {
            Some(sp) => match to_box(sp) {
                Some(b) => Some(b),
                None => return String::new(),
            },
            None => None,
        };
        let t_box = match tablet {
            Some(sp) => match to_box(sp) {
                Some(b) => Some(b),
                None => return String::new(),
            },
            None => None,
        };
        let m_box = match mobile {
            Some(sp) => match to_box(sp) {
                Some(b) => Some(b),
                None => return String::new(),
            },
            None => None,
        };
        let prefix = match property {
            SpacingProperty::Padding => "p",
            SpacingProperty::Margin => "m",
        };
        let allow_negative = property == SpacingProperty::Margin;
        compile_box_spacing_tailwind(prefix, d_box, t_box, m_box, allow_negative)
            .map(|classes| classes.join(" "))
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldAcl {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_view_role: Option<FieldRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_edit_role: Option<FieldRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_exposure: Option<PublicExposure>,
}

impl FieldAcl {
    pub fn canonical(
        min_view_role: FieldRole,
        min_edit_role: FieldRole,
        public_exposure: PublicExposure,
    ) -> Self {
        Self {
            min_view_role: Some(min_view_role),
            min_edit_role: Some(min_edit_role),
            public_exposure: Some(public_exposure),
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldSpec {
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub key: String,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<ConditionGroups>,

    // Context-specific capability flags
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acl: Option<FieldAcl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declaration_state: Option<FieldDeclarationState>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub show_in_list: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub public: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub translatable: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub responsive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub css_var: Option<String>,

    // Text / Textarea / Richtext / Number / Range / Date / Spacing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolbar: Option<RichTextToolbar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<CssUnit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property: Option<SpacingProperty>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_date: Option<String>,

    // Choice / Boolean / Color / Media / Gallery / Relational
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SelectOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options_source: Option<SelectOptionsSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<SelectDisplay>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub multiple: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_null: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off_label: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_alpha: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_kind: Option<MediaFieldKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_items: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taxonomy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taxonomy_from_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picker_style: Option<TermPickerStyle>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub sync_entry_terms: bool,

    // Group / Repeater
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_label_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button_label: Option<String>,

    // UI-only layout primitives (tab / accordion / message)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<FieldLayoutPlacement>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub open_by_default: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub multi_expand: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub endpoint: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<MessageTone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

impl FieldSpec {
    pub fn new(field_type: FieldType, key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            field_type,
            key: key.into(),
            label: label.into(),
            instructions: None,
            placeholder: None,
            required: false,
            default: None,
            width: None,
            conditions: None,
            acl: None,
            declaration_state: None,
            show_in_list: false,
            public: false,
            translatable: false,
            responsive: false,
            css_var: None,
            min_chars: None,
            max_chars: None,
            max_bytes: None,
            rows: None,
            prepend: None,
            append: None,
            toolbar: None,
            min: None,
            max: None,
            step: None,
            unit: None,
            property: None,
            min_date: None,
            max_date: None,
            options: Vec::new(),
            options_source: None,
            display: None,
            multiple: false,
            allow_null: false,
            on_label: None,
            off_label: None,
            allow_alpha: false,
            media_kind: None,
            min_items: None,
            max_items: None,
            target_types: Vec::new(),
            taxonomy: None,
            taxonomy_from_field: None,
            picker_style: None,
            sync_entry_terms: false,
            fields: Vec::new(),
            layout: None,
            item_label_field: None,
            button_label: None,
            placement: None,
            open_by_default: false,
            multi_expand: false,
            endpoint: false,
            tone: None,
            body: None,
        }
    }

    pub fn is_ui_only(&self) -> bool {
        self.field_type.is_ui_only()
    }

    pub fn is_withdrawn(&self) -> bool {
        self.declaration_state == Some(FieldDeclarationState::Withdrawn)
    }

    pub fn effective_required(&self) -> bool {
        self.required && !self.is_withdrawn() && !self.is_ui_only()
    }

    pub fn effective_media_kind(&self) -> MediaFieldKind {
        self.media_kind.unwrap_or(MediaFieldKind::Image)
    }

    pub fn effective_spacing_property(&self) -> SpacingProperty {
        self.property.unwrap_or(SpacingProperty::Padding)
    }

    pub fn effective_unit(&self) -> CssUnit {
        self.unit.unwrap_or(CssUnit::Px)
    }

    pub fn effective_acl(&self) -> (FieldRole, FieldRole, PublicExposure) {
        match &self.acl {
            Some(acl) => (
                acl.min_view_role.unwrap_or(FieldRole::Editor),
                acl.min_edit_role.unwrap_or(FieldRole::Editor),
                acl.public_exposure.unwrap_or(PublicExposure::Public),
            ),
            None => (FieldRole::Editor, FieldRole::Editor, PublicExposure::Public),
        }
    }

    pub fn checkbox_layout(&self) -> Result<Option<CheckboxLayout>, &'static str> {
        let Some(raw) = self.layout.as_deref() else {
            return Ok(None);
        };
        if self.field_type != FieldType::Checkbox {
            return Err("Thuộc tính layout 'checkbox' không khớp kiểu trường.");
        }
        match raw {
            "vertical" => Ok(Some(CheckboxLayout::Vertical)),
            "horizontal" => Ok(Some(CheckboxLayout::Horizontal)),
            _ => Err("layout của checkbox chỉ chấp nhận 'vertical' hoặc 'horizontal'."),
        }
    }

    pub fn radio_layout(&self) -> Result<Option<RadioLayout>, &'static str> {
        let Some(raw) = self.layout.as_deref() else {
            return Ok(None);
        };
        if self.field_type != FieldType::RadioButtons {
            return Err("Thuộc tính layout 'radio_buttons' không khớp kiểu trường.");
        }
        match raw {
            "buttons" => Ok(Some(RadioLayout::Buttons)),
            "radio" => Ok(Some(RadioLayout::Radio)),
            _ => Err("layout của radio_buttons chỉ chấp nhận 'buttons' hoặc 'radio'."),
        }
    }

    pub fn repeater_layout(&self) -> Result<Option<RepeaterLayout>, &'static str> {
        let Some(raw) = self.layout.as_deref() else {
            return Ok(None);
        };
        if self.field_type != FieldType::Repeater {
            return Err("Thuộc tính layout 'repeater' không khớp kiểu trường.");
        }
        match raw {
            "accordion" => Ok(Some(RepeaterLayout::Accordion)),
            "table" => Ok(Some(RepeaterLayout::Table)),
            "block" => Ok(Some(RepeaterLayout::Block)),
            _ => Err("layout của repeater chỉ chấp nhận 'accordion', 'table' hoặc 'block'."),
        }
    }
}

#[inline]
pub fn canonicalize_f64(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value }
}

pub fn canonicalize_value(value: &mut Value) {
    match value {
        Value::Number(num) => {
            if num.is_f64()
                && let Some(f) = num.as_f64()
                && f == 0.0
                && let Some(zero) = serde_json::Number::from_f64(0.0)
            {
                *num = zero;
            }
        }
        Value::Array(items) => {
            for item in items {
                canonicalize_value(item);
            }
        }
        Value::Object(map) => {
            for val in map.values_mut() {
                canonicalize_value(val);
            }
        }
        _ => {}
    }
}

pub fn canonicalize_field_specs(specs: &mut [FieldSpec]) {
    for spec in specs {
        spec.min = spec.min.map(canonicalize_f64);
        spec.max = spec.max.map(canonicalize_f64);
        spec.step = spec.step.map(canonicalize_f64);
        if matches!(spec.default, Some(Value::Null)) {
            spec.default = None;
        }
        if let Some(def) = spec.default.as_mut() {
            canonicalize_value(def);
        }
        if let Some(conds) = spec.conditions.as_mut() {
            for group in conds.iter_mut() {
                for rule in group.iter_mut() {
                    if let Some(val) = rule.value.as_mut() {
                        canonicalize_value(val);
                    }
                }
            }
        }
        if !spec.fields.is_empty() {
            canonicalize_field_specs(&mut spec.fields);
        }
    }
}

pub fn valid_safe_url(raw: &str) -> bool {
    if raw.is_empty() || raw.len() > MAX_URL_BYTES {
        return false;
    }
    if raw
        .bytes()
        .any(|b| b <= 0x1F || b == 0x7F || b.is_ascii_whitespace())
        || raw.chars().any(char::is_whitespace)
    {
        return false;
    }
    if raw == "#" {
        return true;
    }
    if let Some(fragment) = raw.strip_prefix('#') {
        return !fragment.is_empty()
            && fragment.len() <= 128
            && fragment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    }
    if raw.starts_with('/') {
        return !raw.starts_with("//") && !raw.contains('\\') && !raw.contains("..");
    }
    let lower_prefix_8 = raw.get(..8).map(str::to_ascii_lowercase);
    let lower_prefix_7 = raw.get(..7).map(str::to_ascii_lowercase);
    let lower_prefix_4 = raw.get(..4).map(str::to_ascii_lowercase);

    if lower_prefix_7.as_deref() == Some("mailto:") {
        let addr = &raw[7..];
        let email_part = addr.split('?').next().unwrap_or("");
        let Some((local, domain)) = email_part.split_once('@') else {
            return false;
        };
        return !local.is_empty()
            && !domain.is_empty()
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
            && !addr.contains('\\');
    }
    if lower_prefix_4.as_deref() == Some("tel:") {
        let number = &raw[4..];
        return !number.is_empty()
            && number.len() <= 64
            && number.bytes().all(|b| {
                b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'(' | b')' | b'*' | b'#')
            })
            && number.bytes().any(|b| b.is_ascii_digit());
    }
    if lower_prefix_7.as_deref() == Some("http://") || lower_prefix_8.as_deref() == Some("https://")
    {
        let Ok(parsed) = Url::parse(raw) else {
            return false;
        };
        return matches!(parsed.scheme(), "http" | "https")
            && parsed.has_host()
            && parsed.username().is_empty()
            && parsed.password().is_none();
    }
    false
}

pub fn valid_email_address(raw: &str) -> bool {
    if raw.is_empty() || raw.len() > MAX_EMAIL_CHARS || raw.chars().count() > MAX_EMAIL_CHARS {
        return false;
    }
    if raw
        .bytes()
        .any(|b| b <= 0x1F || b >= 0x7F || b.is_ascii_whitespace())
    {
        return false;
    }
    let Some((local, domain)) = raw.split_once('@') else {
        return false;
    };
    if local.is_empty() || local.len() > 64 || domain.is_empty() || domain.contains('@') {
        return false;
    }
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return false;
    }
    let valid_local = local.bytes().all(|b| {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'.' | b'!'
                    | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'/'
                    | b'='
                    | b'?'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'{'
                    | b'|'
                    | b'}'
                    | b'~'
            )
    });
    if !valid_local || !domain.contains('.') {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    for label in &labels {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        let bytes = label.as_bytes();
        if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
            return false;
        }
        if !bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
        {
            return false;
        }
    }
    let tld = labels[labels.len() - 1];
    tld.len() >= 2
}

pub fn valid_color_value(raw: &str) -> bool {
    if raw == "transparent" || raw == "currentColor" || ALLOWED_COLOR_TOKENS.contains(&raw) {
        return true;
    }
    let Some(hex) = raw.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn valid_css_var_name(raw: &str) -> bool {
    let Some(suffix) = raw.strip_prefix("--od-") else {
        return false;
    };
    !suffix.is_empty()
        && suffix.len() <= 48
        && suffix
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn valid_css_select_value(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 32
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

pub fn valid_sha256_media_id(raw: &str) -> bool {
    raw.len() == 64 && raw.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn valid_iso_date(raw: &str) -> bool {
    parse_iso_date_parts(raw).is_some()
}

fn parse_iso_date_parts(raw: &str) -> Option<(i64, i64, i64)> {
    let b = raw.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return None;
    }
    let year = raw[0..4].parse::<i64>().ok()?;
    let month = raw[5..7].parse::<i64>().ok()?;
    let day = raw[8..10].parse::<i64>().ok()?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year >= 1 && day >= 1 && day <= days_in_month {
        Some((year, month, day))
    } else {
        None
    }
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parses canonical ISO-8601 UTC datetimes (`YYYY-MM-DDTHH:MM[:SS[.fffffffff]]Z`)
/// into UTC epoch milliseconds so `.000Z` and `Z` compare identically.
pub fn parse_utc_datetime_millis(raw: &str) -> Option<i64> {
    let (date_part, rest) = raw.split_once('T')?;
    let (year, month, day) = parse_iso_date_parts(date_part)?;
    let time_part = rest.strip_suffix('Z')?;
    let (hms, frac_ms) = if let Some((main, frac)) = time_part.split_once('.') {
        if frac.is_empty() || frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut padded = [b'0'; 3];
        for (idx, b) in frac.as_bytes().iter().take(3).enumerate() {
            padded[idx] = *b;
        }
        let ms = std::str::from_utf8(&padded).ok()?.parse::<i64>().ok()?;
        (main, ms)
    } else {
        (time_part, 0)
    };
    let hms_bytes = hms.as_bytes();
    let (hour, min, sec) = match hms_bytes.len() {
        5 if hms_bytes[2] == b':'
            && hms_bytes[..2].iter().all(u8::is_ascii_digit)
            && hms_bytes[3..5].iter().all(u8::is_ascii_digit) =>
        {
            (
                hms[0..2].parse::<i64>().ok()?,
                hms[3..5].parse::<i64>().ok()?,
                0,
            )
        }
        8 if hms_bytes[2] == b':'
            && hms_bytes[5] == b':'
            && hms_bytes[0..2].iter().all(u8::is_ascii_digit)
            && hms_bytes[3..5].iter().all(u8::is_ascii_digit)
            && hms_bytes[6..8].iter().all(u8::is_ascii_digit) =>
        {
            (
                hms[0..2].parse::<i64>().ok()?,
                hms[3..5].parse::<i64>().ok()?,
                hms[6..8].parse::<i64>().ok()?,
            )
        }
        _ => return None,
    };
    if hour > 23 || min > 59 || sec > 59 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let total_secs = days
        .checked_mul(86_400)?
        .checked_add(hour * 3_600 + min * 60 + sec)?;
    total_secs.checked_mul(1_000)?.checked_add(frac_ms)
}

pub fn valid_iso_datetime_utc(raw: &str) -> bool {
    parse_utc_datetime_millis(raw).is_some()
}

/// Parses `HH:MM` or `HH:MM:SS` (24h) into seconds since midnight (`0..=86399`).
pub fn parse_time_seconds(raw: &str) -> Option<u32> {
    let b = raw.as_bytes();
    let (hour, min, sec) = match b.len() {
        5 if b[2] == b':'
            && b[0..2].iter().all(u8::is_ascii_digit)
            && b[3..5].iter().all(u8::is_ascii_digit) =>
        {
            (
                raw[0..2].parse::<u32>().ok()?,
                raw[3..5].parse::<u32>().ok()?,
                0,
            )
        }
        8 if b[2] == b':'
            && b[5] == b':'
            && b[0..2].iter().all(u8::is_ascii_digit)
            && b[3..5].iter().all(u8::is_ascii_digit)
            && b[6..8].iter().all(u8::is_ascii_digit) =>
        {
            (
                raw[0..2].parse::<u32>().ok()?,
                raw[3..5].parse::<u32>().ok()?,
                raw[6..8].parse::<u32>().ok()?,
            )
        }
        _ => return None,
    };
    if hour > 23 || min > 59 || sec > 59 {
        return None;
    }
    Some(hour * 3_600 + min * 60 + sec)
}

pub fn valid_iso_time(raw: &str) -> bool {
    parse_time_seconds(raw).is_some()
}

fn validate_label(label: &str) -> Result<(), &'static str> {
    if label.trim().is_empty()
        || label.chars().count() > MAX_LABEL_LEN
        || label.chars().any(char::is_control)
    {
        return Err("Nhãn field phải không rỗng, tối đa 100 ký tự và không chứa ký tự điều khiển.");
    }
    Ok(())
}

fn validate_optional_plain_text(
    raw: Option<&str>,
    max_chars: usize,
    allow_newlines: bool,
    err: &'static str,
) -> Result<(), &'static str> {
    let Some(s) = raw else { return Ok(()) };
    if s.chars().count() > max_chars
        || s.chars()
            .any(|c| c.is_control() && !(allow_newlines && c == '\n'))
    {
        return Err(err);
    }
    Ok(())
}

fn collect_css_var<'a>(
    css_var: Option<&'a str>,
    seen_css_vars: &mut BTreeSet<&'a str>,
) -> Result<(), &'static str> {
    if let Some(var) = css_var {
        if !valid_css_var_name(var) {
            return Err("Tên biến CSS phải khớp ^--od-[a-z0-9-]{1,48}$.");
        }
        if !seen_css_vars.insert(var) {
            return Err("Biến CSS không được trùng lặp trong cùng một block.");
        }
    }
    Ok(())
}

fn validate_option_list(
    options: &[SelectOption],
    max_len: usize,
    require_css_safe: bool,
    allow_icons: bool,
) -> Result<(), &'static str> {
    if options.is_empty() || options.len() > max_len {
        return Err("Số lượng lựa chọn vượt giới hạn hợp lệ.");
    }
    let mut seen = BTreeSet::new();
    for opt in options {
        if opt.value.is_empty()
            || opt.value.chars().count() > MAX_OPTION_VALUE_CHARS
            || opt.value.chars().any(char::is_control)
        {
            return Err("Giá trị option phải không rỗng và tối đa 64 ký tự.");
        }
        if !seen.insert(opt.value.as_str()) {
            return Err("Giá trị option không được trùng lặp.");
        }
        validate_label(&opt.label)?;
        if let Some(icon) = &opt.icon {
            if !allow_icons {
                return Err("Kiểu field này không hỗ trợ icon trên từng option.");
            }
            if lookup_core_icon_paths(icon).is_none() {
                return Err("Tên icon option không nằm trong danh mục CORE_ICONS.");
            }
        }
        if require_css_safe && !valid_css_select_value(&opt.value) {
            return Err("Option có css_var phải khớp ^[a-z0-9_-]{1,32}$.");
        }
    }
    Ok(())
}

fn is_non_empty_json_value(spec: &FieldSpec, val: &Value) -> bool {
    match val {
        Value::Null => false,
        Value::String(s) => !s.trim().is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => {
            if spec.field_type == FieldType::Link {
                map.get("url")
                    .and_then(Value::as_str)
                    .is_some_and(|u| !u.trim().is_empty())
            } else if spec.field_type == FieldType::Spacing {
                ["top", "right", "bottom", "left"]
                    .iter()
                    .any(|side| map.get(*side).is_some_and(|v| v.is_number()))
            } else {
                !map.is_empty()
            }
        }
        Value::Number(_) | Value::Bool(_) => true,
    }
}

pub fn normalize_field_specs_for_scope(
    specs: &mut [FieldSpec],
    ctx: FieldContext,
    scope: FieldScopeKind,
) -> Result<(), &'static str> {
    if !matches!(ctx, FieldContext::Content | FieldContext::Term) {
        return Err("normalize_field_specs_for_scope chỉ áp dụng cho Content và Term.");
    }
    if scope == FieldScopeKind::TermPanelField && ctx != FieldContext::Term {
        return Err("TermPanelField yêu cầu context FieldContext::Term.");
    }
    canonicalize_field_specs(specs);
    let (default_view, default_edit, default_pub) = match scope {
        FieldScopeKind::BaseCptField | FieldScopeKind::UserFieldGroup => {
            (FieldRole::Editor, FieldRole::Editor, PublicExposure::Public)
        }
        FieldScopeKind::ExtensionGroupField | FieldScopeKind::TermPanelField => (
            FieldRole::Editor,
            FieldRole::Editor,
            PublicExposure::Private,
        ),
    };
    normalize_acl_recursive(specs, scope, default_view, default_edit, default_pub)?;
    validate_field_specs(specs, ctx)
}

fn normalize_acl_recursive(
    specs: &mut [FieldSpec],
    scope: FieldScopeKind,
    default_view: FieldRole,
    default_edit: FieldRole,
    default_pub: PublicExposure,
) -> Result<(), &'static str> {
    for spec in specs {
        if spec.is_ui_only() {
            if spec.acl.is_some() {
                return Err("UI layout field không được khai báo acl.");
            }
            continue;
        }
        let existing = spec.acl.take().unwrap_or_default();
        let min_view_role = existing.min_view_role.unwrap_or(default_view);
        let min_edit_role = existing.min_edit_role.unwrap_or(default_edit);
        let public_exposure = existing.public_exposure.unwrap_or(default_pub);

        if min_edit_role < min_view_role {
            return Err("invalid_field_acl: min_edit_role không được thấp hơn min_view_role.");
        }
        if public_exposure == PublicExposure::Public && min_view_role != FieldRole::Editor {
            return Err(
                "invalid_field_acl: Trường có min_view_role = admin không được đặt public_exposure = public.",
            );
        }
        if scope == FieldScopeKind::TermPanelField && public_exposure == PublicExposure::Public {
            return Err("TermPanelField không hỗ trợ public_exposure = public ở Đợt 1.");
        }
        spec.acl = Some(FieldAcl::canonical(
            min_view_role,
            min_edit_role,
            public_exposure,
        ));
        if spec.effective_required() && min_edit_role == FieldRole::Admin {
            let valid = spec
                .default
                .as_ref()
                .is_some_and(|v| is_non_empty_json_value(spec, v));
            if !valid {
                return Err(
                    "Trường required với min_edit_role = admin bắt buộc có default hợp lệ, khác rỗng.",
                );
            }
        }
        if !spec.fields.is_empty() {
            normalize_acl_recursive(
                &mut spec.fields,
                scope,
                min_view_role,
                min_edit_role,
                public_exposure,
            )?;
        }
    }
    Ok(())
}

pub fn validate_field_specs(specs: &[FieldSpec], ctx: FieldContext) -> Result<(), &'static str> {
    let mut seen_css_vars = BTreeSet::new();
    let mut total_block_repeater_max = 0usize;
    let mut leaf_data_count = 0usize;
    let top_map = build_scope_lookup_map(specs);

    validate_scope_specs(
        specs,
        ctx,
        &top_map,
        0,
        false,
        &mut seen_css_vars,
        &mut total_block_repeater_max,
        &mut leaf_data_count,
    )?;

    if ctx == FieldContext::StudioBlock {
        let top_data_count = specs.iter().filter(|s| !s.is_ui_only()).count();
        if specs.len() > MAX_BLOCK_TOTAL_SPECS {
            return Err("StudioBlock tối đa 64 phần tử top-level.");
        }
        if top_data_count > MAX_BLOCK_TOP_DATA_FIELDS {
            return Err("StudioBlock tối đa 32 trường dữ liệu top-level.");
        }
        if leaf_data_count > MAX_BLOCK_LEAF_DATA_FIELDS {
            return Err("StudioBlock tối đa 48 trường dữ liệu lá.");
        }
        if total_block_repeater_max > MAX_BLOCK_REPEATER_TOTAL_ITEMS {
            return Err("Tổng max_items của các repeater trên 1 StudioBlock tối đa 64.");
        }
    } else if leaf_data_count > MAX_SCHEMA_DATA_FIELDS {
        return Err("Mỗi schema tối đa 64 trường dữ liệu.");
    }

    validate_dependency_cycles(specs)?;
    Ok(())
}

fn build_scope_lookup_map(specs: &[FieldSpec]) -> BTreeMap<&str, &FieldSpec> {
    let mut map = BTreeMap::new();
    for spec in specs {
        map.entry(spec.key.as_str()).or_insert(spec);
    }
    map
}

#[allow(clippy::too_many_arguments)]
fn validate_scope_specs<'a>(
    specs: &'a [FieldSpec],
    ctx: FieldContext,
    top_map: &BTreeMap<&'a str, &'a FieldSpec>,
    group_depth: usize,
    in_repeater: bool,
    seen_css_vars: &mut BTreeSet<&'a str>,
    total_block_repeater_max: &mut usize,
    leaf_data_count: &mut usize,
) -> Result<(), &'static str> {
    let mut seen_keys = BTreeSet::new();
    let sibling_map = build_scope_lookup_map(specs);
    let in_group = group_depth > 0;

    for spec in specs {
        if !valid_snake_key(&spec.key) {
            return Err("Khóa field không hợp lệ (snake_case 1..=64 ký tự).");
        }
        if !seen_keys.insert(spec.key.as_str()) {
            return Err("Khóa field không được trùng lặp trong cùng phạm vi.");
        }
        if in_group && spec.key == "secret_ref" {
            return Err("Cấm đặt khóa con 'secret_ref' bên trong group.");
        }
        if !ctx.supports_type(spec.field_type) {
            return Err("Kiểu field không được hỗ trợ trong ngữ cảnh này.");
        }

        validate_single_field_spec(
            spec,
            ctx,
            top_map,
            &sibling_map,
            group_depth,
            in_repeater,
            seen_css_vars,
            total_block_repeater_max,
            leaf_data_count,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_single_field_spec<'a>(
    spec: &'a FieldSpec,
    ctx: FieldContext,
    top_map: &BTreeMap<&'a str, &'a FieldSpec>,
    sibling_map: &BTreeMap<&'a str, &'a FieldSpec>,
    group_depth: usize,
    in_repeater: bool,
    seen_css_vars: &mut BTreeSet<&'a str>,
    total_block_repeater_max: &mut usize,
    leaf_data_count: &mut usize,
) -> Result<(), &'static str> {
    let ft = spec.field_type;
    let in_group = group_depth > 0;

    if ft == FieldType::Accordion && spec.endpoint {
        if !spec.label.is_empty() {
            validate_label(&spec.label)?;
        }
    } else {
        validate_label(&spec.label)?;
    }

    validate_optional_plain_text(
        spec.instructions.as_deref(),
        MAX_INSTRUCTIONS_LEN,
        true,
        "Hướng dẫn field tối đa 400 ký tự và không chứa ký tự điều khiển.",
    )?;
    validate_optional_plain_text(
        spec.placeholder.as_deref(),
        MAX_PLACEHOLDER_LEN,
        false,
        "Placeholder field tối đa 120 ký tự.",
    )?;
    validate_optional_plain_text(
        spec.prepend.as_deref(),
        MAX_AFFIX_LEN,
        false,
        "Tiền tố prepend tối đa 32 ký tự.",
    )?;
    validate_optional_plain_text(
        spec.append.as_deref(),
        MAX_AFFIX_LEN,
        false,
        "Hậu tố append tối đa 32 ký tự.",
    )?;

    if let Some(w) = spec.width
        && !matches!(w, 25 | 33 | 50 | 66 | 75 | 100)
    {
        return Err("width chỉ chấp nhận một trong {25, 33, 50, 66, 75, 100}.");
    }

    if (spec.prepend.is_some() || spec.append.is_some())
        && !matches!(
            ft,
            FieldType::Text | FieldType::Number | FieldType::Email | FieldType::Url
        )
    {
        return Err("prepend/append chỉ áp dụng cho text, number, email, url.");
    }

    // Context-only capability checks
    if (spec.acl.is_some() || spec.declaration_state.is_some())
        && !matches!(ctx, FieldContext::Content | FieldContext::Term)
    {
        return Err("acl và declaration_state chỉ hợp lệ trên Content và Term.");
    }
    if spec.show_in_list && (ctx != FieldContext::Content || !ft.is_scalar_listable()) {
        return Err("show_in_list chỉ hợp lệ trên trường scalar của Content.");
    }
    if spec.public && (ctx != FieldContext::Options || ft.is_ui_only()) {
        return Err("Thuộc tính public chỉ hợp lệ trên trường dữ liệu Options.");
    }
    if spec.translatable && (ctx != FieldContext::Options || ft.is_ui_only()) {
        return Err("Thuộc tính translatable chỉ hợp lệ trên trường dữ liệu Options ở Đợt 1.");
    }
    if spec.responsive {
        if ctx != FieldContext::StudioBlock || in_group || in_repeater {
            return Err("responsive chỉ hỗ trợ ở top-level của StudioBlock.");
        }
        let allowed_resp =
            matches!(
                ft,
                FieldType::Number | FieldType::Range | FieldType::RadioButtons | FieldType::Spacing
            ) || (ft == FieldType::Select && !spec.multiple && spec.options_source.is_none());
        if !allowed_resp {
            return Err("Kiểu trường này không hỗ trợ responsive.");
        }
    }
    if spec.css_var.is_some() {
        if ctx != FieldContext::StudioBlock || in_repeater {
            return Err("css_var chỉ hỗ trợ ở StudioBlock (không đặt trong repeater).");
        }
        let allowed_css = match ft {
            FieldType::Number
            | FieldType::Range
            | FieldType::RadioButtons
            | FieldType::Color
            | FieldType::Spacing => true,
            FieldType::Select => !spec.multiple && spec.options_source.is_none(),
            FieldType::Media => spec.effective_media_kind() == MediaFieldKind::Image,
            _ => false,
        };
        if !allowed_css {
            return Err("Kiểu trường hoặc cấu hình hiện tại không hỗ trợ css_var.");
        }
        collect_css_var(spec.css_var.as_deref(), seen_css_vars)?;
    }

    if spec.unit.is_some() {
        let allowed_unit = ft == FieldType::Spacing
            || (ctx == FieldContext::StudioBlock
                && matches!(ft, FieldType::Number | FieldType::Range));
        if !allowed_unit {
            return Err("unit chỉ áp dụng cho spacing hoặc number/range trên StudioBlock.");
        }
    }
    if spec.property.is_some() && ft != FieldType::Spacing {
        return Err("Thuộc tính property chỉ áp dụng cho spacing.");
    }
    if spec.options_source.is_some()
        && (ft != FieldType::Select || ctx != FieldContext::StudioBlock || in_repeater)
    {
        return Err("options_source chỉ áp dụng cho select ngoài repeater trên StudioBlock.");
    }
    if spec.taxonomy_from_field.is_some()
        && (ft != FieldType::TermRef || ctx != FieldContext::StudioBlock || in_repeater)
    {
        return Err(
            "taxonomy_from_field chỉ áp dụng cho term_ref ngoài repeater trên StudioBlock.",
        );
    }
    if spec.sync_entry_terms {
        return Err("sync_entry_terms được kích hoạt ở Đợt 2 (ACF Content Structure).");
    }

    // Strict unused attribute hygiene checks
    if spec.rows.is_some() && ft != FieldType::Textarea {
        return Err("Thuộc tính rows chỉ áp dụng cho textarea.");
    }
    if (spec.min_chars.is_some() || spec.max_chars.is_some())
        && !matches!(ft, FieldType::Text | FieldType::Textarea | FieldType::Email)
    {
        return Err("min_chars/max_chars chỉ áp dụng cho text, textarea, email.");
    }
    if (spec.max_bytes.is_some() || spec.toolbar.is_some()) && ft != FieldType::Richtext {
        return Err("max_bytes/toolbar chỉ áp dụng cho richtext.");
    }
    if (spec.min.is_some() || spec.max.is_some() || spec.step.is_some())
        && !matches!(
            ft,
            FieldType::Number | FieldType::Range | FieldType::Spacing
        )
    {
        return Err("min/max/step chỉ áp dụng cho number, range, spacing.");
    }
    if (spec.min_date.is_some() || spec.max_date.is_some()) && ft != FieldType::Date {
        return Err("min_date/max_date chỉ áp dụng cho date.");
    }
    if !spec.options.is_empty()
        && !matches!(
            ft,
            FieldType::Select | FieldType::Checkbox | FieldType::RadioButtons
        )
    {
        return Err("Chỉ select, checkbox, radio_buttons mới được khai báo options.");
    }
    if (spec.display.is_some() || spec.allow_null) && ft != FieldType::Select {
        return Err("display/allow_null chỉ áp dụng cho select.");
    }
    if spec.multiple
        && !matches!(
            ft,
            FieldType::Select | FieldType::EntryRef | FieldType::TermRef
        )
    {
        return Err("multiple chỉ áp dụng cho select, entry_ref, term_ref.");
    }
    if (spec.on_label.is_some() || spec.off_label.is_some()) && ft != FieldType::Boolean {
        return Err("on_label/off_label chỉ áp dụng cho boolean.");
    }
    if spec.allow_alpha && ft != FieldType::Color {
        return Err("allow_alpha chỉ áp dụng cho color.");
    }
    if spec.media_kind.is_some() && !matches!(ft, FieldType::Media | FieldType::Gallery) {
        return Err("media_kind chỉ áp dụng cho media hoặc gallery.");
    }
    if (spec.min_items.is_some() || spec.max_items.is_some())
        && !matches!(
            ft,
            FieldType::Gallery
                | FieldType::Checkbox
                | FieldType::EntryRef
                | FieldType::TermRef
                | FieldType::Repeater
        )
    {
        return Err("min_items/max_items không áp dụng cho kiểu field này.");
    }
    if !spec.target_types.is_empty() && ft != FieldType::EntryRef {
        return Err("target_types chỉ áp dụng cho entry_ref.");
    }
    if (spec.taxonomy.is_some() || spec.picker_style.is_some()) && ft != FieldType::TermRef {
        return Err("taxonomy/picker_style chỉ áp dụng cho term_ref.");
    }
    if !spec.fields.is_empty() && !ft.is_container() {
        return Err("Chỉ group và repeater mới được chứa fields con.");
    }
    if spec.layout.is_some()
        && !matches!(
            ft,
            FieldType::Checkbox | FieldType::RadioButtons | FieldType::Repeater
        )
    {
        return Err("layout chỉ áp dụng cho checkbox, radio_buttons hoặc repeater.");
    }
    if (spec.item_label_field.is_some() || spec.button_label.is_some()) && ft != FieldType::Repeater
    {
        return Err("item_label_field/button_label chỉ áp dụng cho repeater.");
    }
    if spec.placement.is_some() && ft != FieldType::Tab {
        return Err("placement chỉ áp dụng cho tab.");
    }
    if (spec.open_by_default || spec.multi_expand || spec.endpoint) && ft != FieldType::Accordion {
        return Err("open_by_default/multi_expand/endpoint chỉ áp dụng cho accordion.");
    }
    if (spec.tone.is_some() || spec.body.is_some()) && ft != FieldType::Message {
        return Err("tone/body chỉ áp dụng cho message.");
    }

    // M7-S09 #1, #2, #4 when acl explicitly provided
    if let Some(acl) = &spec.acl {
        let min_view = acl.min_view_role.unwrap_or(FieldRole::Editor);
        let min_edit = acl.min_edit_role.unwrap_or(FieldRole::Editor);
        let pub_exp = acl.public_exposure.unwrap_or(PublicExposure::Public);
        if min_edit < min_view {
            return Err("invalid_field_acl: min_edit_role không được thấp hơn min_view_role.");
        }
        if pub_exp == PublicExposure::Public && min_view != FieldRole::Editor {
            return Err(
                "invalid_field_acl: Trường có min_view_role = admin không được đặt public_exposure = public.",
            );
        }
        if spec.effective_required() && min_edit == FieldRole::Admin {
            let valid = spec
                .default
                .as_ref()
                .is_some_and(|v| is_non_empty_json_value(spec, v));
            if !valid {
                return Err(
                    "Trường required với min_edit_role = admin bắt buộc có default hợp lệ, khác rỗng.",
                );
            }
        }
    }

    // UI-only placement checks
    if ft.is_ui_only() {
        if in_repeater || group_depth >= MAX_GROUP_DEPTH {
            return Err("UI layout field không được đặt trong repeater hoặc group cấp 2.");
        }
        if spec.required {
            return Err("UI layout field không được đặt required.");
        }
        if matches!(spec.default, Some(ref v) if !v.is_null()) {
            return Err("UI layout field không được đặt default.");
        }
    }

    // Per-type structural & default checks
    match ft {
        FieldType::Tab | FieldType::Accordion => {}
        FieldType::Message => {
            let Some(body) = spec.body.as_deref() else {
                return Err("Field message bắt buộc có body.");
            };
            if body.trim().is_empty()
                || body.chars().count() > MAX_MESSAGE_BODY_CHARS
                || body.chars().any(|c| c.is_control() && c != '\n')
                || body.contains('<')
                || body.contains('>')
            {
                return Err(
                    "Nội dung message phải là văn bản thuần không rỗng, <= 1000 ký tự và không chứa HTML.",
                );
            }
        }
        FieldType::Text => {
            *leaf_data_count += 1;
            let max_c = spec.max_chars.unwrap_or(DEFAULT_TEXT_MAX_CHARS);
            if max_c == 0 || max_c > MAX_TEXT_CHARS {
                return Err("max_chars của text phải nằm trong 1..=2000.");
            }
            if let Some(min_c) = spec.min_chars
                && min_c > max_c
            {
                return Err("min_chars của text lớn hơn max_chars.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định text phải là chuỗi.");
                };
                if s.chars().any(char::is_control) {
                    return Err("Giá trị mặc định text không được chứa ký tự điều khiển.");
                }
                let len = s.chars().count();
                if len > max_c || (!s.is_empty() && spec.min_chars.is_some_and(|mn| len < mn)) {
                    return Err("Giá trị mặc định text vượt giới hạn độ dài.");
                }
            }
        }
        FieldType::Textarea => {
            *leaf_data_count += 1;
            if let Some(rows) = spec.rows
                && !(MIN_TEXTAREA_ROWS..=MAX_TEXTAREA_ROWS).contains(&rows)
            {
                return Err("rows của textarea phải nằm trong 2..=16.");
            }
            let max_c = spec.max_chars.unwrap_or(DEFAULT_TEXTAREA_MAX_CHARS);
            if max_c == 0 || max_c > MAX_TEXTAREA_CHARS {
                return Err("max_chars của textarea phải nằm trong 1..=100000.");
            }
            if let Some(min_c) = spec.min_chars
                && min_c > max_c
            {
                return Err("min_chars của textarea lớn hơn max_chars.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định textarea phải là chuỗi.");
                };
                let len = s.chars().count();
                if len > max_c || (!s.is_empty() && spec.min_chars.is_some_and(|mn| len < mn)) {
                    return Err("Giá trị mặc định textarea vượt giới hạn độ dài.");
                }
            }
        }
        FieldType::Number => {
            *leaf_data_count += 1;
            if let Some(mn) = spec.min
                && !mn.is_finite()
            {
                return Err("min của number phải hữu hạn.");
            }
            if let Some(mx) = spec.max
                && !mx.is_finite()
            {
                return Err("max của number phải hữu hạn.");
            }
            if let (Some(mn), Some(mx)) = (spec.min, spec.max)
                && mn > mx
            {
                return Err("number có min lớn hơn max.");
            }
            if let Some(st) = spec.step
                && (!st.is_finite() || st <= 0.0)
            {
                return Err("step của number phải hữu hạn và > 0.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(n) = def.as_f64().filter(|f| f.is_finite()) else {
                    return Err("Giá trị mặc định number phải là số hữu hạn.");
                };
                if spec.min.is_some_and(|mn| n < mn) || spec.max.is_some_and(|mx| n > mx) {
                    return Err("Giá trị mặc định number nằm ngoài [min, max].");
                }
            }
        }
        FieldType::Range => {
            *leaf_data_count += 1;
            let (Some(mn), Some(mx)) = (spec.min, spec.max) else {
                return Err("Field range bắt buộc khai báo cả min và max.");
            };
            if !mn.is_finite() || !mx.is_finite() || mn > mx {
                return Err("Khoảng min/max của range không hợp lệ.");
            }
            if let Some(st) = spec.step
                && (!st.is_finite() || st <= 0.0)
            {
                return Err("step của range phải hữu hạn và > 0.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(n) = def.as_f64().filter(|f| f.is_finite()) else {
                    return Err("Giá trị mặc định range phải là số hữu hạn.");
                };
                if n < mn || n > mx {
                    return Err("Giá trị mặc định range nằm ngoài [min, max].");
                }
            }
        }
        FieldType::Email => {
            *leaf_data_count += 1;
            let max_c = spec.max_chars.unwrap_or(MAX_EMAIL_CHARS);
            if max_c == 0 || max_c > MAX_EMAIL_CHARS {
                return Err("max_chars của email phải nằm trong 1..=254.");
            }
            if let Some(min_c) = spec.min_chars
                && min_c > max_c
            {
                return Err("min_chars của email lớn hơn max_chars.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định email phải là chuỗi.");
                };
                if !s.is_empty() && (!valid_email_address(s) || s.chars().count() > max_c) {
                    return Err("Email mặc định không hợp lệ.");
                }
            }
        }
        FieldType::Url => {
            *leaf_data_count += 1;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định url phải là chuỗi.");
                };
                if !s.is_empty() && !valid_safe_url(s) {
                    return Err("URL mặc định không hợp lệ.");
                }
            }
        }
        FieldType::Secret => {
            *leaf_data_count += 1;
            if in_group || in_repeater {
                return Err("Field secret chỉ được đặt ở top-level của OwnerSettings.");
            }
            if matches!(spec.default, Some(ref v) if !v.is_null()) {
                return Err("Field secret không được đặt default khác null.");
            }
        }
        FieldType::Richtext => {
            *leaf_data_count += 1;
            let is_core_entry_body =
                ctx == FieldContext::Content && !in_group && !in_repeater && spec.key == "body";
            let cap = if is_core_entry_body {
                MAX_BUILDER_BLOCK_BODY_BYTES
            } else {
                MAX_RICHTEXT_OUTPUT_BYTES
            };
            let limit = spec.max_bytes.unwrap_or(cap);
            if limit == 0 || limit > cap {
                return Err("max_bytes của richtext vượt giới hạn cho phép.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định richtext phải là chuỗi.");
                };
                if !s.is_empty() {
                    let cleaned = sanitize_richtext(s)?;
                    if cleaned.len() > limit {
                        return Err("Giá trị mặc định richtext vượt quá max_bytes sau làm sạch.");
                    }
                }
            }
        }
        FieldType::Media => {
            *leaf_data_count += 1;
            if matches!(spec.default, Some(ref v) if !v.is_null()) {
                return Err("Field media không được phép khai báo default.");
            }
        }
        FieldType::Gallery => {
            *leaf_data_count += 1;
            if spec.effective_media_kind() == MediaFieldKind::File {
                return Err("Field gallery chỉ hỗ trợ media_kind: image | video | any.");
            }
            let max_i = spec.max_items.unwrap_or(MAX_GALLERY_ITEMS);
            if max_i == 0 || max_i > MAX_GALLERY_ITEMS {
                return Err("max_items của gallery phải nằm trong 1..=50.");
            }
            if let Some(min_i) = spec.min_items
                && min_i > max_i
            {
                return Err("min_items của gallery lớn hơn max_items.");
            }
            if matches!(spec.default, Some(ref v) if !v.is_null()) {
                return Err("Field gallery không được phép khai báo default.");
            }
        }
        FieldType::Link => {
            *leaf_data_count += 1;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let link: LinkValue = serde_json::from_value(def.clone())
                    .map_err(|_| "Giá trị mặc định link không đúng cấu trúc.")?;
                if link.label.chars().count() > MAX_LINK_LABEL_CHARS
                    || link.label.chars().any(char::is_control)
                {
                    return Err(
                        "Nhãn link mặc định vượt quá 160 ký tự hoặc chứa ký tự điều khiển.",
                    );
                }
                if !link.url.is_empty() && !valid_safe_url(&link.url) {
                    return Err("URL mặc định trong field link không hợp lệ.");
                }
            }
        }
        FieldType::Select => {
            *leaf_data_count += 1;
            if spec.options_source.is_some() {
                if !spec.options.is_empty() {
                    return Err("options và options_source trên select loại trừ lẫn nhau.");
                }
                if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                    if spec.multiple {
                        let Some(arr) = def.as_array() else {
                            return Err("Giá trị mặc định select multiple phải là mảng.");
                        };
                        for item in arr {
                            let Some(s) = item.as_str() else {
                                return Err("Phần tử mặc định select multiple phải là chuỗi.");
                            };
                            if s.is_empty() || s.len() > 128 || s.chars().any(char::is_control) {
                                return Err("Phần tử mặc định options_source không hợp lệ.");
                            }
                        }
                    } else {
                        let Some(s) = def.as_str() else {
                            return Err("Giá trị mặc định select đơn phải là chuỗi.");
                        };
                        if s.len() > 128 || s.chars().any(char::is_control) {
                            return Err("Giá trị mặc định options_source không hợp lệ.");
                        }
                    }
                }
            } else {
                validate_option_list(
                    &spec.options,
                    MAX_SELECT_OPTIONS,
                    spec.css_var.is_some(),
                    true,
                )?;
                let opt_vals: BTreeSet<&str> =
                    spec.options.iter().map(|o| o.value.as_str()).collect();
                if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                    if spec.multiple {
                        let Some(arr) = def.as_array() else {
                            return Err("Giá trị mặc định select multiple phải là mảng.");
                        };
                        let mut seen = BTreeSet::new();
                        for item in arr {
                            let Some(s) = item.as_str() else {
                                return Err("Phần tử mặc định select phải là chuỗi.");
                            };
                            if !opt_vals.contains(s) || !seen.insert(s) {
                                return Err("Giá trị mặc định select multiple không hợp lệ.");
                            }
                        }
                    } else {
                        let Some(s) = def.as_str() else {
                            return Err("Giá trị mặc định select phải là chuỗi.");
                        };
                        if !(s.is_empty() && (spec.allow_null || !spec.required))
                            && !opt_vals.contains(s)
                        {
                            return Err("Giá trị mặc định select không nằm trong options.");
                        }
                    }
                }
            }
        }
        FieldType::Checkbox => {
            *leaf_data_count += 1;
            let _ = spec.checkbox_layout()?;
            validate_option_list(&spec.options, MAX_SELECT_OPTIONS, false, false)?;
            let max_i = spec.max_items.unwrap_or(spec.options.len());
            if max_i == 0 || max_i > spec.options.len() {
                return Err("max_items của checkbox vượt quá số options.");
            }
            if let Some(min_i) = spec.min_items
                && min_i > max_i
            {
                return Err("min_items của checkbox lớn hơn max_items.");
            }
            let opt_vals: BTreeSet<&str> = spec.options.iter().map(|o| o.value.as_str()).collect();
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(arr) = def.as_array() else {
                    return Err("Giá trị mặc định checkbox phải là mảng.");
                };
                let mut seen = BTreeSet::new();
                for item in arr {
                    let Some(s) = item.as_str() else {
                        return Err("Phần tử mặc định checkbox phải là chuỗi.");
                    };
                    if !opt_vals.contains(s) || !seen.insert(s) {
                        return Err(
                            "Giá trị mặc định checkbox không có trong options hoặc bị trùng.",
                        );
                    }
                }
                if arr.len() > max_i {
                    return Err("Giá trị mặc định checkbox vượt max_items.");
                }
                if !arr.is_empty() && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                    return Err("Giá trị mặc định checkbox ít hơn min_items.");
                }
            }
        }
        FieldType::RadioButtons => {
            *leaf_data_count += 1;
            let _ = spec.radio_layout()?;
            validate_option_list(
                &spec.options,
                MAX_RADIO_OPTIONS,
                spec.css_var.is_some(),
                true,
            )?;
            let opt_vals: BTreeSet<&str> = spec.options.iter().map(|o| o.value.as_str()).collect();
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định radio_buttons phải là chuỗi.");
                };
                if !(s.is_empty() && !spec.required) && !opt_vals.contains(s) {
                    return Err("Giá trị mặc định radio_buttons không có trong options.");
                }
            }
        }
        FieldType::Boolean => {
            *leaf_data_count += 1;
            validate_optional_plain_text(
                spec.on_label.as_deref(),
                MAX_ON_OFF_LABEL_CHARS,
                false,
                "on_label tối đa 64 ký tự.",
            )?;
            validate_optional_plain_text(
                spec.off_label.as_deref(),
                MAX_ON_OFF_LABEL_CHARS,
                false,
                "off_label tối đa 64 ký tự.",
            )?;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null())
                && !def.is_boolean()
            {
                return Err("Giá trị mặc định boolean phải là true hoặc false.");
            }
        }
        FieldType::Color => {
            *leaf_data_count += 1;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định color phải là chuỗi.");
                };
                if !s.is_empty() && !valid_color_value(s) {
                    return Err("Màu mặc định không hợp lệ.");
                }
            }
        }
        FieldType::Icon => {
            *leaf_data_count += 1;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định icon phải là chuỗi.");
                };
                if !s.is_empty() && lookup_core_icon_paths(s).is_none() {
                    return Err("Icon mặc định không thuộc danh mục CORE_ICONS.");
                }
            }
        }
        FieldType::Date => {
            *leaf_data_count += 1;
            if let Some(mn) = spec.min_date.as_deref()
                && !valid_iso_date(mn)
            {
                return Err("min_date phải là ngày YYYY-MM-DD hợp lệ.");
            }
            if let Some(mx) = spec.max_date.as_deref()
                && !valid_iso_date(mx)
            {
                return Err("max_date phải là ngày YYYY-MM-DD hợp lệ.");
            }
            if let (Some(mn), Some(mx)) = (spec.min_date.as_deref(), spec.max_date.as_deref())
                && mn > mx
            {
                return Err("min_date không được lớn hơn max_date.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định date phải là chuỗi.");
                };
                if !s.is_empty() {
                    if !valid_iso_date(s) {
                        return Err("Giá trị mặc định date phải có định dạng YYYY-MM-DD.");
                    }
                    if spec.min_date.as_deref().is_some_and(|mn| s < mn)
                        || spec.max_date.as_deref().is_some_and(|mx| s > mx)
                    {
                        return Err("Giá trị mặc định date nằm ngoài [min_date, max_date].");
                    }
                }
            }
        }
        FieldType::Datetime => {
            *leaf_data_count += 1;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định datetime phải là chuỗi.");
                };
                if !s.is_empty() && !valid_iso_datetime_utc(s) {
                    return Err("Giá trị mặc định datetime phải là ISO-8601 UTC hợp lệ.");
                }
            }
        }
        FieldType::Time => {
            *leaf_data_count += 1;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(s) = def.as_str() else {
                    return Err("Giá trị mặc định time phải là chuỗi.");
                };
                if !s.is_empty() && !valid_iso_time(s) {
                    return Err("Giá trị mặc định time phải là HH:MM hoặc HH:MM:SS.");
                }
            }
        }
        FieldType::Spacing => {
            *leaf_data_count += 1;
            let prop = spec.effective_spacing_property();
            let lo = spec.min.unwrap_or(match prop {
                SpacingProperty::Padding => 0.0,
                SpacingProperty::Margin => -128.0,
            });
            let hi = spec.max.unwrap_or(256.0);
            let min_bound = match prop {
                SpacingProperty::Padding => 0.0,
                SpacingProperty::Margin => -256.0,
            };
            if !lo.is_finite() || !hi.is_finite() || lo < min_bound || hi > 256.0 || lo > hi {
                return Err("Khoảng min/max của spacing không hợp lệ.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let val: SpacingValue = serde_json::from_value(def.clone())
                    .map_err(|_| "Giá trị mặc định spacing không hợp lệ.")?;
                for side in val.sides().into_iter().flatten() {
                    if !side.is_finite() || side < lo || side > hi {
                        return Err("Giá trị mặc định spacing nằm ngoài [min, max].");
                    }
                }
            }
        }
        FieldType::EntryRef => {
            *leaf_data_count += 1;
            if spec.target_types.is_empty() || spec.target_types.len() > MAX_ENTRY_REF_TYPES {
                return Err("entry_ref yêu cầu target_types có từ 1..=16 phần tử.");
            }
            if spec.target_types.iter().any(|t| t == "*") {
                if ctx != FieldContext::StudioBlock || spec.target_types.len() != 1 {
                    return Err(
                        "target_types wildcard [\"*\"] chỉ hợp lệ ở StudioBlock và cấm trộn cùng CPT cụ thể.",
                    );
                }
            } else {
                let mut seen = BTreeSet::new();
                for t in &spec.target_types {
                    if t == "pattern" || !valid_snake_key(t) || !seen.insert(t.as_str()) {
                        return Err("target_types chứa CPT không hợp lệ hoặc bị trùng.");
                    }
                }
            }
            let max_i = spec.max_items.unwrap_or(MAX_ENTRY_REF_ITEMS);
            if max_i == 0 || max_i > MAX_ENTRY_REF_ITEMS {
                return Err("max_items của entry_ref phải nằm trong 1..=50.");
            }
            if let Some(min_i) = spec.min_items
                && min_i > max_i
            {
                return Err("min_items của entry_ref lớn hơn max_items.");
            }
            if !spec.multiple
                && (spec.min_items.is_some_and(|n| n > 1) || spec.max_items.is_some_and(|n| n > 1))
            {
                return Err("entry_ref đơn không cho min_items/max_items > 1.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                if spec.multiple {
                    let Some(arr) = def.as_array() else {
                        return Err("Giá trị mặc định entry_ref multiple phải là mảng số nguyên.");
                    };
                    if arr.len() > max_i {
                        return Err("Giá trị mặc định entry_ref vượt max_items.");
                    }
                    let mut seen = BTreeSet::new();
                    for item in arr {
                        let Some(id) = item.as_i64().filter(|n| *n > 0) else {
                            return Err("ID mặc định entry_ref phải là số nguyên dương.");
                        };
                        if !seen.insert(id) {
                            return Err("ID mặc định entry_ref không được trùng lặp.");
                        }
                    }
                } else if def.as_i64().is_none_or(|n| n <= 0) {
                    return Err("Giá trị mặc định entry_ref đơn phải là số nguyên dương.");
                }
            }
        }
        FieldType::TermRef => {
            *leaf_data_count += 1;
            if ctx == FieldContext::StudioBlock {
                match (
                    spec.taxonomy.as_deref(),
                    spec.taxonomy_from_field.as_deref(),
                ) {
                    (Some(tax), None) => {
                        if !valid_snake_key(tax) {
                            return Err("Khóa taxonomy của term_ref không hợp lệ.");
                        }
                    }
                    (None, Some(src_field)) => {
                        if in_repeater || !valid_snake_key(src_field) || src_field == spec.key {
                            return Err("taxonomy_from_field không hợp lệ hoặc tự tham chiếu.");
                        }
                        let target = lookup_condition_target(src_field, sibling_map, top_map)
                            .ok_or(
                                "taxonomy_from_field phải trỏ tới một field select đơn tồn tại.",
                            )?;
                        if target.field_type != FieldType::Select
                            || target.multiple
                            || target.responsive
                        {
                            return Err(
                                "taxonomy_from_field bắt buộc trỏ tới field select đơn không-responsive.",
                            );
                        }
                    }
                    (Some(_), Some(_)) => {
                        return Err(
                            "taxonomy và taxonomy_from_field trên term_ref loại trừ lẫn nhau.",
                        );
                    }
                    (None, None) => {
                        return Err(
                            "term_ref trên StudioBlock bắt buộc khai báo taxonomy hoặc taxonomy_from_field.",
                        );
                    }
                }
            } else {
                if spec.taxonomy_from_field.is_some() {
                    return Err("taxonomy_from_field chỉ hợp lệ ở StudioBlock.");
                }
                let Some(tax) = spec.taxonomy.as_deref() else {
                    return Err("term_ref bắt buộc khai báo taxonomy.");
                };
                if !valid_snake_key(tax) {
                    return Err("Khóa taxonomy của term_ref không hợp lệ.");
                }
            }
            let max_i = spec.max_items.unwrap_or(MAX_TERM_REF_ITEMS);
            if max_i == 0 || max_i > MAX_TERM_REF_ITEMS {
                return Err("max_items của term_ref phải nằm trong 1..=50.");
            }
            if let Some(min_i) = spec.min_items
                && min_i > max_i
            {
                return Err("min_items của term_ref lớn hơn max_items.");
            }
            if !spec.multiple
                && (spec.min_items.is_some_and(|n| n > 1) || spec.max_items.is_some_and(|n| n > 1))
            {
                return Err("term_ref đơn không cho min_items/max_items > 1.");
            }
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                if spec.multiple {
                    let Some(arr) = def.as_array() else {
                        return Err("Giá trị mặc định term_ref multiple phải là mảng số nguyên.");
                    };
                    if arr.len() > max_i {
                        return Err("Giá trị mặc định term_ref vượt max_items.");
                    }
                    let mut seen = BTreeSet::new();
                    for item in arr {
                        let Some(id) = item.as_i64().filter(|n| *n > 0) else {
                            return Err("ID mặc định term_ref phải là số nguyên dương.");
                        };
                        if !seen.insert(id) {
                            return Err("ID mặc định term_ref không được trùng lặp.");
                        }
                    }
                } else if def.as_i64().is_none_or(|n| n <= 0) {
                    return Err("Giá trị mặc định term_ref đơn phải là số nguyên dương.");
                }
            }
        }
        FieldType::Group => {
            if spec.fields.is_empty() {
                return Err("Field group phải có ít nhất 1 field con.");
            }
            let next_depth = group_depth + 1;
            if next_depth > MAX_GROUP_DEPTH {
                return Err("group chỉ hỗ trợ độ sâu tối đa 2 cấp.");
            }
            if in_repeater && next_depth > 1 {
                return Err("Trong repeater chỉ cho phép group phẳng 1 cấp.");
            }
            validate_scope_specs(
                &spec.fields,
                ctx,
                top_map,
                next_depth,
                in_repeater,
                seen_css_vars,
                total_block_repeater_max,
                leaf_data_count,
            )?;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(obj) = def.as_object() else {
                    return Err("Giá trị mặc định group phải là JSON object.");
                };
                let map: BTreeMap<String, Value> =
                    obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                validate_map_against_specs(
                    &spec.fields,
                    ctx,
                    &map,
                    None,
                    &spec.fields,
                    &map,
                    None,
                    true,
                    true,
                )?;
            }
        }
        FieldType::Repeater => {
            if spec.fields.is_empty() {
                return Err("Field repeater phải có ít nhất 1 field con.");
            }
            if in_repeater {
                return Err("V1 không hỗ trợ repeater lồng bên trong repeater.");
            }
            if group_depth >= MAX_GROUP_DEPTH {
                return Err("Không được đặt repeater bên trong group cấp 2.");
            }
            let _ = spec.repeater_layout()?;
            validate_optional_plain_text(
                spec.button_label.as_deref(),
                MAX_BUTTON_LABEL_CHARS,
                false,
                "button_label tối đa 64 ký tự.",
            )?;
            let max_allowed = if ctx == FieldContext::StudioBlock {
                MAX_BLOCK_REPEATER_ITEMS
            } else {
                MAX_REPEATER_ITEMS
            };
            let max_items = spec.max_items.unwrap_or(max_allowed);
            if max_items == 0 || max_items > max_allowed {
                return Err("max_items của repeater vượt giới hạn cho phép.");
            }
            if let Some(min_i) = spec.min_items
                && min_i > max_items
            {
                return Err("min_items của repeater lớn hơn max_items.");
            }
            if ctx == FieldContext::StudioBlock {
                *total_block_repeater_max += max_items;
            }
            if let Some(item_label) = spec.item_label_field.as_deref() {
                let has_target = spec.fields.iter().any(|sub| {
                    sub.key == item_label && !sub.is_ui_only() && !sub.field_type.is_container()
                });
                if !has_target {
                    return Err(
                        "item_label_field phải trỏ tới một trường nguyên thủy tồn tại trong repeater.",
                    );
                }
            }
            validate_scope_specs(
                &spec.fields,
                ctx,
                top_map,
                group_depth,
                true,
                seen_css_vars,
                total_block_repeater_max,
                leaf_data_count,
            )?;
            if let Some(def) = spec.default.as_ref().filter(|v| !v.is_null()) {
                let Some(arr) = def.as_array() else {
                    return Err("Giá trị mặc định repeater phải là mảng.");
                };
                if arr.len() > max_items {
                    return Err("Giá trị mặc định repeater vượt max_items.");
                }
                if !arr.is_empty() && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                    return Err("Giá trị mặc định repeater ít hơn min_items.");
                }
                for item in arr {
                    let Some(obj) = item.as_object() else {
                        return Err("Phần tử mặc định repeater phải là JSON object.");
                    };
                    let map: BTreeMap<String, Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    validate_map_against_specs(
                        &spec.fields,
                        ctx,
                        &map,
                        None,
                        &spec.fields,
                        &map,
                        None,
                        true,
                        true,
                    )?;
                }
            }
        }
    }

    if let Some(conds) = spec.conditions.as_ref() {
        validate_conditions(conds, spec, ctx, sibling_map, top_map)?;
    }

    Ok(())
}

fn lookup_condition_target<'a>(
    key: &str,
    sibling_map: &BTreeMap<&'a str, &'a FieldSpec>,
    top_map: &BTreeMap<&'a str, &'a FieldSpec>,
) -> Option<&'a FieldSpec> {
    sibling_map
        .get(key)
        .copied()
        .or_else(|| top_map.get(key).copied())
}

fn validate_conditions<'a>(
    conds: &ConditionGroups,
    owner: &FieldSpec,
    ctx: FieldContext,
    sibling_map: &BTreeMap<&'a str, &'a FieldSpec>,
    top_map: &BTreeMap<&'a str, &'a FieldSpec>,
) -> Result<(), &'static str> {
    if conds.is_empty() || conds.len() > MAX_CONDITION_GROUPS {
        return Err("conditions phải có từ 1..=8 nhóm OR.");
    }
    for group in conds {
        if group.is_empty() || group.len() > MAX_CONDITION_RULES_PER_GROUP {
            return Err("Mỗi nhóm AND trong conditions phải có từ 1..=8 điều kiện.");
        }
        for rule in group {
            if rule.field == owner.key {
                return Err("conditions không được tự tham chiếu chính mình.");
            }
            let Some(target) = lookup_condition_target(&rule.field, sibling_map, top_map) else {
                return Err("conditions phải trỏ tới field cùng phạm vi hoặc top-level tồn tại.");
            };
            if target.is_ui_only()
                || matches!(
                    target.field_type,
                    FieldType::Secret | FieldType::Spacing | FieldType::Group | FieldType::Repeater
                )
                || target.responsive
            {
                return Err(
                    "conditions không được tham chiếu field UI layout, secret, spacing, group, repeater hoặc responsive.",
                );
            }
            if matches!(ctx, FieldContext::Content | FieldContext::Term) {
                let (target_view, _, _) = target.effective_acl();
                let (owner_view, _, _) = owner.effective_acl();
                if target_view > owner_view {
                    return Err(
                        "Vi phạm ACL Oracle: field có thể xem bởi Editor không được phụ thuộc điều kiện vào field Admin-only.",
                    );
                }
            }
            match rule.op {
                ConditionOp::Empty | ConditionOp::NotEmpty => {
                    if matches!(rule.value, Some(ref v) if !v.is_null()) {
                        return Err("Toán tử empty/not_empty không nhận giá trị value.");
                    }
                }
                ConditionOp::In => {
                    let Some(Value::Array(arr)) = rule.value.as_ref() else {
                        return Err("Toán tử in yêu cầu mảng giá trị.");
                    };
                    if arr.is_empty() || arr.len() > 64 {
                        return Err("Mảng giá trị cho toán tử in phải có 1..=64 phần tử.");
                    }
                    if !arr
                        .iter()
                        .all(|v| v.is_string() || v.is_number() || v.is_boolean())
                    {
                        return Err("Phần tử toán tử in phải là giá trị vô hướng.");
                    }
                }
                ConditionOp::Eq | ConditionOp::Neq => {
                    let Some(val) = rule.value.as_ref() else {
                        return Err("Toán tử eq/neq yêu cầu giá trị value.");
                    };
                    if !(val.is_string() || val.is_number() || val.is_boolean() || val.is_null()) {
                        return Err("Giá trị toán tử eq/neq phải là giá trị vô hướng hoặc null.");
                    }
                }
                ConditionOp::Contains => {
                    let Some(val) = rule.value.as_ref() else {
                        return Err("Toán tử contains yêu cầu giá trị value.");
                    };
                    if !(val.is_string() || val.is_number() || val.is_boolean()) {
                        return Err("Giá trị toán tử contains phải là chuỗi hoặc số.");
                    }
                }
                ConditionOp::Gt | ConditionOp::Lt => {
                    let Some(val) = rule.value.as_ref() else {
                        return Err("Toán tử gt/lt yêu cầu giá trị so sánh.");
                    };
                    match target.field_type {
                        FieldType::Number | FieldType::Range => {
                            if val.as_f64().is_none_or(|f| !f.is_finite()) {
                                return Err("Toán tử gt/lt cho number/range yêu cầu số hữu hạn.");
                            }
                        }
                        FieldType::Date => {
                            if val.as_str().is_none_or(|s| !valid_iso_date(s)) {
                                return Err("Toán tử gt/lt cho date yêu cầu YYYY-MM-DD.");
                            }
                        }
                        FieldType::Datetime => {
                            if val.as_str().is_none_or(|s| !valid_iso_datetime_utc(s)) {
                                return Err("Toán tử gt/lt cho datetime yêu cầu ISO-8601 UTC.");
                            }
                        }
                        FieldType::Time => {
                            if val.as_str().is_none_or(|s| !valid_iso_time(s)) {
                                return Err("Toán tử gt/lt cho time yêu cầu HH:MM hoặc HH:MM:SS.");
                            }
                        }
                        _ => {
                            return Err(
                                "Toán tử gt/lt chỉ hỗ trợ number, range, date, datetime, time.",
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// 3-color DFS cycle detection across condition & `taxonomy_from_field` dependency graphs.
fn validate_dependency_cycles(specs: &[FieldSpec]) -> Result<(), &'static str> {
    let mut nodes: Vec<&FieldSpec> = Vec::new();
    let mut key_to_id_by_scope: Vec<BTreeMap<&str, usize>> = Vec::new();
    let mut top_lookup: BTreeMap<&str, usize> = BTreeMap::new();

    fn index_scope<'a>(
        specs: &'a [FieldSpec],
        nodes: &mut Vec<&'a FieldSpec>,
        scope_map: &mut BTreeMap<&'a str, usize>,
        all_edges_meta: &mut Vec<(usize, Vec<String>, usize)>,
        scope_maps: &mut Vec<BTreeMap<&'a str, usize>>,
    ) {
        let scope_idx = scope_maps.len();
        let mut local = BTreeMap::new();
        for spec in specs {
            let id = nodes.len();
            nodes.push(spec);
            local.insert(spec.key.as_str(), id);
            scope_map.insert(spec.key.as_str(), id);
            let mut deps = Vec::new();
            if let Some(conds) = spec.conditions.as_ref() {
                for g in conds {
                    for r in g {
                        deps.push(r.field.clone());
                    }
                }
            }
            if let Some(tf) = spec.taxonomy_from_field.as_ref() {
                deps.push(tf.clone());
            }
            all_edges_meta.push((id, deps, scope_idx));
        }
        scope_maps.push(local);
        for spec in specs {
            if !spec.fields.is_empty() {
                let mut child_map = BTreeMap::new();
                index_scope(
                    &spec.fields,
                    nodes,
                    &mut child_map,
                    all_edges_meta,
                    scope_maps,
                );
            }
        }
    }

    let mut edges_meta: Vec<(usize, Vec<String>, usize)> = Vec::new();
    index_scope(
        specs,
        &mut nodes,
        &mut top_lookup,
        &mut edges_meta,
        &mut key_to_id_by_scope,
    );

    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (node_id, dep_keys, scope_idx) in edges_meta {
        for dep in dep_keys {
            if let Some(&target_id) = key_to_id_by_scope[scope_idx]
                .get(dep.as_str())
                .or_else(|| top_lookup.get(dep.as_str()))
            {
                adj[node_id].push(target_id);
            }
        }
    }

    // 0 = unvisited, 1 = visiting, 2 = visited
    let mut color = vec![0u8; nodes.len()];
    fn dfs(u: usize, adj: &[Vec<usize>], color: &mut [u8]) -> bool {
        color[u] = 1;
        for &v in &adj[u] {
            if color[v] == 1 {
                return true;
            }
            if color[v] == 0 && dfs(v, adj, color) {
                return true;
            }
        }
        color[u] = 2;
        false
    }

    for i in 0..nodes.len() {
        if color[i] == 0 && dfs(i, &adj, &mut color) {
            return Err("Phát hiện chu trình phụ thuộc trong conditions/taxonomy_from_field.");
        }
    }
    Ok(())
}

pub fn evaluate_conditions(
    conds: Option<&ConditionGroups>,
    scope_specs: &[FieldSpec],
    scope_values: &BTreeMap<String, Value>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
) -> bool {
    let Some(or_groups) = conds else {
        return true;
    };
    if or_groups.is_empty() {
        return true;
    }
    or_groups.iter().any(|and_rules| {
        and_rules.iter().all(|rule| {
            eval_single_condition(rule, scope_specs, scope_values, root_specs, root_values)
        })
    })
}

fn find_spec_in_slice<'a>(specs: &'a [FieldSpec], key: &str) -> Option<&'a FieldSpec> {
    specs.iter().find(|s| s.key == key)
}

fn resolve_effective_condition_value<'a>(
    key: &str,
    scope_specs: &'a [FieldSpec],
    scope_values: &'a BTreeMap<String, Value>,
    root_specs: &'a [FieldSpec],
    root_values: &'a BTreeMap<String, Value>,
) -> (Option<&'a FieldSpec>, Value) {
    if let Some(spec) = find_spec_in_slice(scope_specs, key) {
        let val = effective_value_for_spec(spec, scope_values.get(key));
        return (Some(spec), val);
    }
    if let Some(spec) = find_spec_in_slice(root_specs, key) {
        let val = effective_value_for_spec(spec, root_values.get(key));
        return (Some(spec), val);
    }
    (None, Value::Null)
}

fn effective_value_for_spec(spec: &FieldSpec, current: Option<&Value>) -> Value {
    match current {
        Some(v) if !v.is_null() => v.clone(),
        _ => spec
            .default
            .as_ref()
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or_else(|| canonical_empty_for_spec(spec)),
    }
}

pub fn canonical_empty_for_spec(spec: &FieldSpec) -> Value {
    match spec.field_type {
        FieldType::Text
        | FieldType::Textarea
        | FieldType::Email
        | FieldType::Url
        | FieldType::Richtext
        | FieldType::Color
        | FieldType::Icon
        | FieldType::Date
        | FieldType::Datetime
        | FieldType::Time
        | FieldType::RadioButtons => Value::String(String::new()),
        FieldType::Select => {
            if spec.multiple {
                Value::Array(Vec::new())
            } else {
                Value::String(String::new())
            }
        }
        FieldType::EntryRef | FieldType::TermRef => {
            if spec.multiple {
                Value::Array(Vec::new())
            } else {
                Value::Null
            }
        }
        FieldType::Checkbox | FieldType::Gallery | FieldType::Repeater => Value::Array(Vec::new()),
        FieldType::Boolean => Value::Bool(false),
        FieldType::Group => Value::Object(serde_json::Map::new()),
        FieldType::Number
        | FieldType::Range
        | FieldType::Media
        | FieldType::Link
        | FieldType::Spacing
        | FieldType::Secret
        | FieldType::Tab
        | FieldType::Accordion
        | FieldType::Message => Value::Null,
    }
}

fn is_json_value_empty(val: &Value) -> bool {
    match val {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(arr) => arr.is_empty(),
        Value::Object(obj) => obj.is_empty(),
        Value::Number(_) | Value::Bool(_) => false,
    }
}

fn scalar_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => {
            x.as_f64() == y.as_f64() && x.as_f64().is_some_and(f64::is_finite)
        }
        _ => false,
    }
}

fn eval_single_condition(
    rule: &ConditionRule,
    scope_specs: &[FieldSpec],
    scope_values: &BTreeMap<String, Value>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
) -> bool {
    let (target_spec, cur_val) = resolve_effective_condition_value(
        &rule.field,
        scope_specs,
        scope_values,
        root_specs,
        root_values,
    );
    match rule.op {
        ConditionOp::Empty => is_json_value_empty(&cur_val),
        ConditionOp::NotEmpty => !is_json_value_empty(&cur_val),
        ConditionOp::Eq => rule
            .value
            .as_ref()
            .is_some_and(|expected| scalar_equal(&cur_val, expected)),
        ConditionOp::Neq => rule
            .value
            .as_ref()
            .is_some_and(|expected| !scalar_equal(&cur_val, expected)),
        ConditionOp::In => rule
            .value
            .as_ref()
            .and_then(Value::as_array)
            .is_some_and(|list| match &cur_val {
                Value::Array(arr) => arr
                    .iter()
                    .any(|item| list.iter().any(|cand| scalar_equal(item, cand))),
                _ => list.iter().any(|cand| scalar_equal(&cur_val, cand)),
            }),
        ConditionOp::Contains => {
            let Some(needle) = rule.value.as_ref() else {
                return false;
            };
            match (&cur_val, needle) {
                (Value::String(hay), Value::String(n)) => hay.contains(n.as_str()),
                (Value::Array(items), _) => items.iter().any(|item| scalar_equal(item, needle)),
                _ => false,
            }
        }
        ConditionOp::Gt | ConditionOp::Lt => {
            let Some(expected) = rule.value.as_ref() else {
                return false;
            };
            let is_gt = rule.op == ConditionOp::Gt;
            let ord = match target_spec.map(|s| s.field_type) {
                Some(FieldType::Datetime) => {
                    match (
                        cur_val.as_str().and_then(parse_utc_datetime_millis),
                        expected.as_str().and_then(parse_utc_datetime_millis),
                    ) {
                        (Some(a), Some(b)) => Some(a.cmp(&b)),
                        _ => None,
                    }
                }
                Some(FieldType::Time) => {
                    match (
                        cur_val.as_str().and_then(parse_time_seconds),
                        expected.as_str().and_then(parse_time_seconds),
                    ) {
                        (Some(a), Some(b)) => Some(a.cmp(&b)),
                        _ => None,
                    }
                }
                Some(FieldType::Date) => {
                    match (
                        cur_val.as_str().filter(|s| valid_iso_date(s)),
                        expected.as_str().filter(|s| valid_iso_date(s)),
                    ) {
                        (Some(a), Some(b)) => Some(a.cmp(b)),
                        _ => None,
                    }
                }
                Some(FieldType::Number | FieldType::Range) | None => {
                    match (
                        cur_val.as_f64().filter(|f| f.is_finite()),
                        expected.as_f64().filter(|f| f.is_finite()),
                    ) {
                        (Some(a), Some(b)) => a.partial_cmp(&b),
                        _ => None,
                    }
                }
                _ => None,
            };
            ord.is_some_and(|o| {
                if is_gt {
                    o == std::cmp::Ordering::Greater
                } else {
                    o == std::cmp::Ordering::Less
                }
            })
        }
    }
}

pub fn value_equals_previous(current: Option<&Value>, previous: Option<&Value>) -> bool {
    match (current, previous) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let mut ca = a.clone();
            let mut cb = b.clone();
            canonicalize_value(&mut ca);
            canonicalize_value(&mut cb);
            ca == cb
        }
        _ => false,
    }
}

pub fn validate_field_values(
    specs: &[FieldSpec],
    ctx: FieldContext,
    values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
) -> Result<(), &'static str> {
    validate_map_against_specs(
        specs,
        ctx,
        values,
        previous_values,
        specs,
        values,
        previous_values,
        true,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_map_against_specs(
    specs: &[FieldSpec],
    ctx: FieldContext,
    values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
    prev_root_values: Option<&BTreeMap<String, Value>>,
    parent_visible: bool,
    parent_prev_visible: bool,
) -> Result<(), &'static str> {
    let in_root =
        std::ptr::eq(specs.as_ptr(), root_specs.as_ptr()) && std::ptr::eq(values, root_values);
    let data_specs: BTreeMap<&str, &FieldSpec> = specs
        .iter()
        .filter(|s| !s.is_ui_only())
        .map(|s| (s.key.as_str(), s))
        .collect();

    for (key, val) in values {
        if !data_specs.contains_key(key.as_str()) {
            if ctx == FieldContext::StudioBlock
                && previous_values
                    .and_then(|p| p.get(key.as_str()))
                    .is_some_and(|pv| value_equals_previous(Some(val), Some(pv)))
            {
                continue;
            }
            return Err("Payload chứa khóa field không tồn tại trong schema.");
        }
    }

    for spec in specs {
        if spec.is_ui_only() {
            continue;
        }
        let visible = parent_visible
            && evaluate_conditions(
                spec.conditions.as_ref(),
                specs,
                values,
                root_specs,
                root_values,
            );
        let prev_visible = parent_prev_visible
            && previous_values.is_some_and(|prev| {
                evaluate_conditions(
                    spec.conditions.as_ref(),
                    specs,
                    prev,
                    root_specs,
                    prev_root_values.unwrap_or(prev),
                )
            });
        let cur_val = values.get(spec.key.as_str());
        let prev_val = previous_values.and_then(|p| p.get(spec.key.as_str()));
        let upgrade_exempt = ctx == FieldContext::StudioBlock
            && previous_values
                .is_some_and(|prev| prev_visible && !prev.contains_key(spec.key.as_str()));

        let Some(raw_val) = cur_val.filter(|v| !v.is_null()) else {
            if visible && !upgrade_exempt && spec.field_type != FieldType::Secret {
                let missing_required = spec.effective_required()
                    || (ctx == FieldContext::StudioBlock
                        && spec.field_type == FieldType::Repeater
                        && spec.min_items.is_some_and(|m| m > 0));
                if missing_required {
                    return Err("Thiếu giá trị cho trường bắt buộc.");
                }
            }
            continue;
        };

        // Even when hidden, validate non-null values strictly against schema/XSS/bounds.
        validate_field_value(
            spec,
            ctx,
            raw_val,
            visible,
            prev_visible,
            upgrade_exempt,
            prev_val,
            root_specs,
            root_values,
            prev_root_values,
            in_root,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_field_value(
    spec: &FieldSpec,
    ctx: FieldContext,
    raw_val: &Value,
    visible: bool,
    prev_visible: bool,
    upgrade_exempt: bool,
    prev_val: Option<&Value>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
    prev_root_values: Option<&BTreeMap<String, Value>>,
    in_root: bool,
) -> Result<(), &'static str> {
    if spec.responsive && ctx == FieldContext::StudioBlock {
        if let Some(obj) = raw_val.as_object() {
            for k in obj.keys() {
                if !matches!(k.as_str(), "desktop" | "tablet" | "mobile") {
                    return Err("Giá trị responsive chỉ chấp nhận desktop, tablet, mobile.");
                }
            }
            let mut has_non_empty = false;
            for vp in obj.values() {
                if !vp.is_null() {
                    if !vp.as_str().is_some_and(|s| s.trim().is_empty()) {
                        has_non_empty = true;
                    }
                    validate_non_responsive_value(
                        spec,
                        ctx,
                        vp,
                        false,
                        prev_visible,
                        false,
                        prev_val,
                        root_specs,
                        root_values,
                        prev_root_values,
                        in_root,
                    )?;
                }
            }
            if visible && !upgrade_exempt && spec.effective_required() && !has_non_empty {
                return Err("Thiếu giá trị cho trường bắt buộc.");
            }
            return Ok(());
        }
    }
    validate_non_responsive_value(
        spec,
        ctx,
        raw_val,
        visible,
        prev_visible,
        upgrade_exempt,
        prev_val,
        root_specs,
        root_values,
        prev_root_values,
        in_root,
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_non_responsive_value(
    spec: &FieldSpec,
    ctx: FieldContext,
    val: &Value,
    visible: bool,
    prev_visible: bool,
    upgrade_exempt: bool,
    prev_val: Option<&Value>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
    prev_root_values: Option<&BTreeMap<String, Value>>,
    in_root: bool,
) -> Result<(), &'static str> {
    let req = visible && !upgrade_exempt && spec.effective_required();
    match spec.field_type {
        FieldType::Text => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị text phải là chuỗi.");
            };
            if s.chars().any(char::is_control) {
                return Err("Giá trị text không được chứa ký tự điều khiển.");
            }
            let max_c = spec.max_chars.unwrap_or(DEFAULT_TEXT_MAX_CHARS);
            let len = s.chars().count();
            if len > max_c {
                return Err("Giá trị text vượt quá max_chars.");
            }
            if req && s.trim().is_empty() {
                return Err("Trường text bắt buộc không được để trống.");
            }
            if !s.is_empty() && spec.min_chars.is_some_and(|mn| len < mn) {
                return Err("Giá trị text ngắn hơn min_chars.");
            }
        }
        FieldType::Textarea => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị textarea phải là chuỗi.");
            };
            let max_c = spec.max_chars.unwrap_or(DEFAULT_TEXTAREA_MAX_CHARS);
            let len = s.chars().count();
            if len > max_c {
                return Err("Giá trị textarea vượt quá max_chars.");
            }
            if req && s.trim().is_empty() {
                return Err("Trường textarea bắt buộc không được để trống.");
            }
            if !s.is_empty() && spec.min_chars.is_some_and(|mn| len < mn) {
                return Err("Giá trị textarea ngắn hơn min_chars.");
            }
        }
        FieldType::Number | FieldType::Range => {
            let Some(n) = val.as_f64().filter(|f| f.is_finite()) else {
                return Err("Giá trị number/range phải là số hữu hạn.");
            };
            if spec.min.is_some_and(|mn| n < mn) || spec.max.is_some_and(|mx| n > mx) {
                return Err("Giá trị number/range nằm ngoài [min, max].");
            }
        }
        FieldType::Email => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị email phải là chuỗi.");
            };
            if s.trim().is_empty() {
                if req {
                    return Err("Trường email bắt buộc không được để trống.");
                }
            } else {
                let max_c = spec.max_chars.unwrap_or(MAX_EMAIL_CHARS);
                if s.chars().count() > max_c || !valid_email_address(s) {
                    return Err("Địa chỉ email không hợp lệ.");
                }
            }
        }
        FieldType::Url => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị url phải là chuỗi.");
            };
            if s.trim().is_empty() {
                if req {
                    return Err("Trường url bắt buộc không được để trống.");
                }
            } else if !valid_safe_url(s) {
                return Err("URL không hợp lệ hoặc dùng scheme không an toàn.");
            }
        }
        FieldType::Secret => {
            // Wire representations allow write-only updates or null/sentinel placeholders;
            // exact SecretStore handling occurs in storage/settings.rs.
        }
        FieldType::Richtext => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị richtext phải là chuỗi.");
            };
            if req && strip_richtext_to_plain(s).trim().is_empty() {
                return Err("Trường richtext bắt buộc không được để trống.");
            }
            if ctx == FieldContext::Content && in_root && spec.key == "body" {
                if contains_blocks(s) {
                    if s.len() > MAX_BUILDER_BLOCK_BODY_BYTES {
                        return Err("Nội dung block body vượt giới hạn 384 KiB.");
                    }
                } else if s.chars().count() > 200_000 {
                    return Err("Nội dung body vượt giới hạn 200000 ký tự.");
                }
            } else if !s.is_empty() {
                let cleaned = sanitize_richtext(s)?;
                if req && strip_richtext_to_plain(&cleaned).trim().is_empty() {
                    return Err("Trường richtext bắt buộc không được để trống.");
                }
                let cap = spec.max_bytes.unwrap_or(MAX_RICHTEXT_OUTPUT_BYTES);
                if cleaned.len() > cap {
                    return Err("Nội dung richtext sau làm sạch vượt quá max_bytes.");
                }
            }
        }
        FieldType::Media => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị media phải là chuỗi media_id.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường media bắt buộc chưa chọn tệp.");
                }
            } else if !valid_sha256_media_id(s) {
                return Err("media_id phải là chuỗi SHA-256 64 ký tự hex.");
            }
        }
        FieldType::Gallery => {
            let Some(arr) = val.as_array() else {
                return Err("Giá trị gallery phải là mảng media_id.");
            };
            let max_i = spec.max_items.unwrap_or(MAX_GALLERY_ITEMS);
            if arr.len() > max_i {
                return Err("Số lượng phần tử gallery vượt quá max_items.");
            }
            if req && arr.is_empty() {
                return Err("Trường gallery bắt buộc không được để trống.");
            }
            if !arr.is_empty() && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                return Err("Số lượng phần tử gallery ít hơn min_items.");
            }
            let mut seen = BTreeSet::new();
            for item in arr {
                let Some(id) = item.as_str() else {
                    return Err("Phần tử gallery phải là chuỗi media_id.");
                };
                if !valid_sha256_media_id(id) || !seen.insert(id) {
                    return Err("media_id trong gallery không hợp lệ hoặc bị trùng.");
                }
            }
        }
        FieldType::Link => {
            let link: LinkValue = serde_json::from_value(val.clone())
                .map_err(|_| "Giá trị link không đúng cấu trúc.")?;
            if link.label.chars().count() > MAX_LINK_LABEL_CHARS
                || link.label.chars().any(char::is_control)
            {
                return Err("Nhãn link tối đa 160 ký tự và không chứa ký tự điều khiển.");
            }
            if link.url.is_empty() {
                if req {
                    return Err("Trường link bắt buộc chưa có URL.");
                }
            } else if !valid_safe_url(&link.url) {
                return Err("URL trong field link không hợp lệ.");
            }
        }
        FieldType::Select => {
            let opt_vals: BTreeSet<&str> = spec.options.iter().map(|o| o.value.as_str()).collect();
            if spec.multiple {
                let Some(arr) = val.as_array() else {
                    return Err("Giá trị select multiple phải là mảng.");
                };
                if req && arr.is_empty() {
                    return Err("Trường select bắt buộc phải chọn ít nhất 1 mục.");
                }
                let mut seen = BTreeSet::new();
                for item in arr {
                    let Some(s) = item.as_str() else {
                        return Err("Phần tử select multiple phải là chuỗi.");
                    };
                    if !seen.insert(s) {
                        return Err("Giá trị select multiple không được trùng.");
                    }
                    if spec.options_source.is_none() && !opt_vals.contains(s) {
                        return Err("Lựa chọn không có trong danh sách options.");
                    }
                }
            } else {
                let Some(s) = val.as_str() else {
                    return Err("Giá trị select phải là chuỗi.");
                };
                if s.is_empty() {
                    if req {
                        return Err("Trường select bắt buộc chưa được chọn.");
                    }
                } else if spec.options_source.is_none() && !opt_vals.contains(s) {
                    return Err("Lựa chọn không có trong danh sách options.");
                }
            }
        }
        FieldType::Checkbox => {
            let Some(arr) = val.as_array() else {
                return Err("Giá trị checkbox phải là mảng.");
            };
            let max_i = spec.max_items.unwrap_or(spec.options.len());
            if arr.len() > max_i {
                return Err("Số lượng checkbox vượt quá max_items.");
            }
            if req && arr.is_empty() {
                return Err("Trường checkbox bắt buộc phải chọn ít nhất 1 mục.");
            }
            if !arr.is_empty() && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                return Err("Số lượng checkbox ít hơn min_items.");
            }
            let opt_vals: BTreeSet<&str> = spec.options.iter().map(|o| o.value.as_str()).collect();
            let mut seen = BTreeSet::new();
            for item in arr {
                let Some(s) = item.as_str() else {
                    return Err("Phần tử checkbox phải là chuỗi.");
                };
                if !opt_vals.contains(s) || !seen.insert(s) {
                    return Err("Giá trị checkbox không hợp lệ hoặc bị trùng.");
                }
            }
        }
        FieldType::RadioButtons => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị radio_buttons phải là chuỗi.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường radio_buttons bắt buộc chưa được chọn.");
                }
            } else if !spec.options.iter().any(|o| o.value == s) {
                return Err("Giá trị radio_buttons không có trong options.");
            }
        }
        FieldType::Boolean => {
            if !val.is_boolean() {
                return Err("Giá trị boolean phải là true hoặc false.");
            }
        }
        FieldType::Color => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị color phải là chuỗi.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường color bắt buộc chưa chọn màu.");
                }
            } else if !valid_color_value(s) {
                return Err("Mã màu không hợp lệ.");
            }
        }
        FieldType::Icon => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị icon phải là chuỗi.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường icon bắt buộc chưa chọn biểu tượng.");
                }
            } else if lookup_core_icon_paths(s).is_none() {
                return Err("Tên icon không hợp lệ trong CORE_ICONS.");
            }
        }
        FieldType::Date => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị date phải là chuỗi.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường date bắt buộc chưa có ngày.");
                }
            } else {
                if !valid_iso_date(s) {
                    return Err("Ngày phải có định dạng YYYY-MM-DD hợp lệ.");
                }
                if spec.min_date.as_deref().is_some_and(|mn| s < mn)
                    || spec.max_date.as_deref().is_some_and(|mx| s > mx)
                {
                    return Err("Ngày nằm ngoài khoảng min_date..=max_date.");
                }
            }
        }
        FieldType::Datetime => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị datetime phải là chuỗi.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường datetime bắt buộc chưa có thời gian.");
                }
            } else if !valid_iso_datetime_utc(s) {
                return Err("datetime phải là ISO-8601 UTC hợp lệ.");
            }
        }
        FieldType::Time => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị time phải là chuỗi.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường time bắt buộc chưa có giờ.");
                }
            } else if !valid_iso_time(s) {
                return Err("time phải có định dạng HH:MM hoặc HH:MM:SS.");
            }
        }
        FieldType::Spacing => {
            let sp: SpacingValue = serde_json::from_value(val.clone())
                .map_err(|_| "Giá trị spacing không đúng cấu trúc.")?;
            let prop = spec.effective_spacing_property();
            let lo = spec.min.unwrap_or(match prop {
                SpacingProperty::Padding => 0.0,
                SpacingProperty::Margin => -128.0,
            });
            let hi = spec.max.unwrap_or(256.0);
            let sides = sp.sides();
            if req && sides.iter().all(Option::is_none) {
                return Err("Trường spacing bắt buộc phải có ít nhất 1 cạnh.");
            }
            for s in sides.into_iter().flatten() {
                if !s.is_finite() || s < lo || s > hi {
                    return Err("Giá trị spacing nằm ngoài [min, max].");
                }
            }
        }
        FieldType::EntryRef | FieldType::TermRef => {
            let default_max = if spec.field_type == FieldType::EntryRef {
                MAX_ENTRY_REF_ITEMS
            } else {
                MAX_TERM_REF_ITEMS
            };
            if spec.multiple {
                let Some(arr) = val.as_array() else {
                    return Err("Trường quan hệ multiple phải là mảng số nguyên.");
                };
                let max_i = spec.max_items.unwrap_or(default_max);
                if arr.len() > max_i {
                    return Err("Số lượng tham chiếu vượt max_items.");
                }
                if req && arr.is_empty() {
                    return Err("Trường quan hệ bắt buộc phải chọn ít nhất 1 mục.");
                }
                if !arr.is_empty() && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                    return Err("Số lượng tham chiếu ít hơn min_items.");
                }
                let mut seen = BTreeSet::new();
                for item in arr {
                    let Some(id) = item.as_i64().filter(|n| *n > 0) else {
                        return Err("ID tham chiếu phải là số nguyên dương.");
                    };
                    if !seen.insert(id) {
                        return Err("ID tham chiếu không được trùng lặp.");
                    }
                }
            } else if val.as_i64().is_none_or(|n| n <= 0) {
                return Err("ID tham chiếu đơn phải là số nguyên dương.");
            }
        }
        FieldType::Group => {
            let Some(obj) = val.as_object() else {
                return Err("Giá trị group phải là JSON object.");
            };
            let map: BTreeMap<String, Value> =
                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let prev_map: Option<BTreeMap<String, Value>> = prev_val
                .and_then(Value::as_object)
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
            validate_map_against_specs(
                &spec.fields,
                ctx,
                &map,
                prev_map.as_ref(),
                root_specs,
                root_values,
                prev_root_values,
                visible,
                prev_visible,
            )?;
        }
        FieldType::Repeater => {
            let Some(arr) = val.as_array() else {
                return Err("Giá trị repeater phải là mảng.");
            };
            let cap = if ctx == FieldContext::StudioBlock {
                MAX_BLOCK_REPEATER_ITEMS
            } else {
                MAX_REPEATER_ITEMS
            };
            let max_items = spec.max_items.unwrap_or(cap);
            if arr.len() > max_items {
                return Err("Số phần tử repeater vượt quá max_items.");
            }
            if req && arr.is_empty() {
                return Err("Trường repeater bắt buộc phải có ít nhất 1 phần tử.");
            }
            if ctx == FieldContext::StudioBlock {
                if visible && !upgrade_exempt && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                    return Err("Số phần tử repeater ít hơn min_items.");
                }
            } else if !arr.is_empty() && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                return Err("Số phần tử repeater ít hơn min_items.");
            }
            for (idx, item) in arr.iter().enumerate() {
                let Some(obj) = item.as_object() else {
                    return Err("Phần tử repeater phải là JSON object.");
                };
                if ctx == FieldContext::StudioBlock
                    && obj.len() > crate::block_schema::MAX_NODE_VALUES_KEYS
                {
                    return Err("Phần tử Repeater vượt quá 64 khóa.");
                }
                let map: BTreeMap<String, Value> =
                    obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                let prev_map: Option<BTreeMap<String, Value>> = prev_val
                    .and_then(Value::as_array)
                    .and_then(|rows| rows.get(idx))
                    .and_then(Value::as_object)
                    .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
                validate_map_against_specs(
                    &spec.fields,
                    ctx,
                    &map,
                    prev_map.as_ref(),
                    root_specs,
                    root_values,
                    prev_root_values,
                    visible,
                    prev_visible,
                )?;
            }
        }
        FieldType::Tab | FieldType::Accordion | FieldType::Message => {}
    }
    Ok(())
}

/// Checks `COMPAT-1..4` transitions between existing and incoming `FieldSpec` trees.
pub fn check_schema_compatibility(
    existing: &[FieldSpec],
    incoming: &[FieldSpec],
    has_existing_data: bool,
) -> Result<(), &'static str> {
    let old_data: BTreeMap<&str, &FieldSpec> = existing
        .iter()
        .filter(|s| !s.is_ui_only())
        .map(|s| (s.key.as_str(), s))
        .collect();
    let new_data: BTreeMap<&str, &FieldSpec> = incoming
        .iter()
        .filter(|s| !s.is_ui_only())
        .map(|s| (s.key.as_str(), s))
        .collect();

    // Unconditional COMPAT-4 check on incoming ACL/default invariants
    check_compat4_recursive(incoming)?;

    if !has_existing_data {
        return Ok(());
    }

    for (&old_key, &old_spec) in &old_data {
        let Some(&new_spec) = new_data.get(old_key) else {
            return Err(
                "cannot_remove_field_with_data: Không thể xóa hoặc di chuyển khóa field khi đã có dữ liệu.",
            );
        };
        let compatible_type = old_spec.field_type == new_spec.field_type
            || (old_spec.field_type == FieldType::Text
                && new_spec.field_type == FieldType::Textarea);
        if !compatible_type {
            return Err(
                "incompatible_schema_change: Không thể đổi kiểu dữ liệu (chỉ cho phép nới text -> textarea).",
            );
        }
        if matches!(
            old_spec.field_type,
            FieldType::Select | FieldType::EntryRef | FieldType::TermRef
        ) && old_spec.multiple != new_spec.multiple
        {
            return Err("incompatible_schema_change: Không thể lật multiple khi đã có dữ liệu.");
        }
        if matches!(old_spec.field_type, FieldType::Media | FieldType::Gallery) {
            let old_k = old_spec.effective_media_kind();
            let new_k = new_spec.effective_media_kind();
            let kind_ok = old_k == new_k || new_k == MediaFieldKind::Any;
            if !kind_ok {
                return Err(
                    "incompatible_schema_change: Không thể thu hẹp media_kind khi đã có dữ liệu.",
                );
            }
        }
        if matches!(
            old_spec.field_type,
            FieldType::Select | FieldType::Checkbox | FieldType::RadioButtons
        ) && old_spec.options_source.is_none()
        {
            let new_vals: BTreeSet<&str> =
                new_spec.options.iter().map(|o| o.value.as_str()).collect();
            for old_opt in &old_spec.options {
                if !new_vals.contains(old_opt.value.as_str()) {
                    return Err(
                        "incompatible_schema_change: Không thể xóa option đang khai báo khi đã có dữ liệu.",
                    );
                }
            }
        }
        if !old_spec.effective_required() && new_spec.effective_required() {
            return Err(
                "incompatible_schema_change: Không thể đổi trường tùy chọn thành required khi đã có dữ liệu.",
            );
        }
        if !old_spec.fields.is_empty() && !new_spec.fields.is_empty() {
            check_schema_compatibility(&old_spec.fields, &new_spec.fields, true)?;
        }
    }

    for (&new_key, &new_spec) in &new_data {
        if !old_data.contains_key(new_key) && new_spec.effective_required() {
            return Err(
                "incompatible_schema_change: Không thể thêm trường required mới khi đã có dữ liệu.",
            );
        }
    }

    Ok(())
}

fn check_compat4_recursive(specs: &[FieldSpec]) -> Result<(), &'static str> {
    for spec in specs {
        if spec.is_ui_only() {
            continue;
        }
        if let Some(acl) = &spec.acl {
            let min_view = acl.min_view_role.unwrap_or(FieldRole::Editor);
            let min_edit = acl.min_edit_role.unwrap_or(FieldRole::Editor);
            let pub_exp = acl.public_exposure.unwrap_or(PublicExposure::Public);
            if min_edit < min_view {
                return Err("invalid_field_acl: min_edit_role không được thấp hơn min_view_role.");
            }
            if pub_exp == PublicExposure::Public && min_view != FieldRole::Editor {
                return Err(
                    "invalid_field_acl: Trường có min_view_role = admin không được đặt public_exposure = public.",
                );
            }
            if spec.effective_required() && min_edit == FieldRole::Admin {
                let valid = spec
                    .default
                    .as_ref()
                    .is_some_and(|v| is_non_empty_json_value(spec, v));
                if !valid {
                    return Err(
                        "Trường required với min_edit_role = admin bắt buộc có default hợp lệ, khác rỗng.",
                    );
                }
            }
        }
        if !spec.fields.is_empty() {
            check_compat4_recursive(&spec.fields)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectedMediaIds {
    pub image_ids: BTreeSet<String>,
    pub video_ids: BTreeSet<String>,
    pub file_ids: BTreeSet<String>,
    pub any_ids: BTreeSet<String>,
}

impl CollectedMediaIds {
    pub fn all_ids(&self) -> BTreeSet<String> {
        let mut all = BTreeSet::new();
        all.extend(self.image_ids.iter().cloned());
        all.extend(self.video_ids.iter().cloned());
        all.extend(self.file_ids.iter().cloned());
        all.extend(self.any_ids.iter().cloned());
        all
    }
}

fn push_collected_media(out: &mut CollectedMediaIds, kind: MediaFieldKind, id: &str) {
    match kind {
        MediaFieldKind::Image => {
            out.image_ids.insert(id.to_owned());
        }
        MediaFieldKind::Video => {
            out.video_ids.insert(id.to_owned());
        }
        MediaFieldKind::File => {
            out.file_ids.insert(id.to_owned());
        }
        MediaFieldKind::Any => {
            out.any_ids.insert(id.to_owned());
        }
    }
}

pub fn collect_media_ids(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
) -> CollectedMediaIds {
    let mut out = CollectedMediaIds::default();
    collect_media_recursive(specs, values, &mut out);
    out
}

fn collect_media_recursive(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    out: &mut CollectedMediaIds,
) {
    for spec in specs {
        let Some(val) = values.get(spec.key.as_str()) else {
            continue;
        };
        match spec.field_type {
            FieldType::Media => {
                if let Some(id) = val.as_str()
                    && valid_sha256_media_id(id)
                {
                    push_collected_media(out, spec.effective_media_kind(), id);
                }
            }
            FieldType::Gallery => {
                if let Some(arr) = val.as_array() {
                    let kind = spec.effective_media_kind();
                    for item in arr.iter().take(MAX_GALLERY_ITEMS) {
                        if let Some(id) = item.as_str()
                            && valid_sha256_media_id(id)
                        {
                            push_collected_media(out, kind, id);
                        }
                    }
                }
            }
            FieldType::Group => {
                if let Some(obj) = val.as_object() {
                    let map: BTreeMap<String, Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    collect_media_recursive(&spec.fields, &map, out);
                }
            }
            FieldType::Repeater => {
                if let Some(arr) = val.as_array() {
                    for item in arr.iter().take(MAX_REPEATER_ITEMS) {
                        if let Some(obj) = item.as_object() {
                            let map: BTreeMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            collect_media_recursive(&spec.fields, &map, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Collects media IDs only from fields with `public_exposure == Public`
/// (recursing strictly into nested `Group` and `Repeater` trees so private nested fields are never leaked,
/// while preserving access for withdrawn fields when `include_withdrawn_public == true`).
pub fn collect_public_media_ids(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    include_withdrawn_public: bool,
) -> CollectedMediaIds {
    let mut out = CollectedMediaIds::default();
    collect_public_media_recursive(specs, values, include_withdrawn_public, &mut out);
    out
}

fn collect_public_media_recursive(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    include_withdrawn_public: bool,
    out: &mut CollectedMediaIds,
) {
    for spec in specs {
        if spec.is_ui_only() {
            continue;
        }
        if spec.is_withdrawn() && !include_withdrawn_public {
            continue;
        }
        let (_, _, exposure) = spec.effective_acl();
        if exposure != PublicExposure::Public {
            continue;
        }
        let Some(val) = values.get(spec.key.as_str()) else {
            continue;
        };
        match spec.field_type {
            FieldType::Media => {
                if let Some(id) = val.as_str()
                    && valid_sha256_media_id(id)
                {
                    push_collected_media(out, spec.effective_media_kind(), id);
                }
            }
            FieldType::Gallery => {
                if let Some(arr) = val.as_array() {
                    let kind = spec.effective_media_kind();
                    for item in arr.iter().take(MAX_GALLERY_ITEMS) {
                        if let Some(id) = item.as_str()
                            && valid_sha256_media_id(id)
                        {
                            push_collected_media(out, kind, id);
                        }
                    }
                }
            }
            FieldType::Group => {
                if let Some(obj) = val.as_object() {
                    let map: BTreeMap<String, Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    collect_public_media_recursive(
                        &spec.fields,
                        &map,
                        include_withdrawn_public,
                        out,
                    );
                }
            }
            FieldType::Repeater => {
                if let Some(arr) = val.as_array() {
                    for item in arr.iter().take(MAX_REPEATER_ITEMS) {
                        if let Some(obj) = item.as_object() {
                            let map: BTreeMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            collect_public_media_recursive(
                                &spec.fields,
                                &map,
                                include_withdrawn_public,
                                out,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectedRelationalRefs {
    pub entry_ids: BTreeSet<i64>,
    pub term_ids: BTreeSet<i64>,
}

pub fn collect_relational_refs(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
) -> CollectedRelationalRefs {
    let mut out = CollectedRelationalRefs::default();
    collect_relational_recursive(specs, values, &mut out);
    out
}

fn collect_relational_recursive(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    out: &mut CollectedRelationalRefs,
) {
    for spec in specs {
        let Some(val) = values.get(spec.key.as_str()) else {
            continue;
        };
        match spec.field_type {
            FieldType::EntryRef => {
                if spec.multiple {
                    if let Some(arr) = val.as_array() {
                        for id in arr
                            .iter()
                            .take(MAX_ENTRY_REF_ITEMS)
                            .filter_map(Value::as_i64)
                        {
                            if id > 0 {
                                out.entry_ids.insert(id);
                            }
                        }
                    }
                } else if let Some(id) = val.as_i64().filter(|n| *n > 0) {
                    out.entry_ids.insert(id);
                }
            }
            FieldType::TermRef => {
                if spec.multiple {
                    if let Some(arr) = val.as_array() {
                        for id in arr
                            .iter()
                            .take(MAX_TERM_REF_ITEMS)
                            .filter_map(Value::as_i64)
                        {
                            if id > 0 {
                                out.term_ids.insert(id);
                            }
                        }
                    }
                } else if let Some(id) = val.as_i64().filter(|n| *n > 0) {
                    out.term_ids.insert(id);
                }
            }
            FieldType::Group => {
                if let Some(obj) = val.as_object() {
                    let map: BTreeMap<String, Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    collect_relational_recursive(&spec.fields, &map, out);
                }
            }
            FieldType::Repeater => {
                if let Some(arr) = val.as_array() {
                    for item in arr.iter().take(MAX_REPEATER_ITEMS) {
                        if let Some(obj) = item.as_object() {
                            let map: BTreeMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            collect_relational_recursive(&spec.fields, &map, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn collect_options_source_values(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    source: SelectOptionsSource,
) -> Vec<String> {
    let mut out = Vec::new();
    collect_options_source_values_recursive(specs, values, source, &mut out);
    out
}

fn collect_options_source_values_recursive(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    source: SelectOptionsSource,
    out: &mut Vec<String>,
) {
    for spec in specs {
        if spec.is_ui_only() || spec.field_type == FieldType::Secret {
            continue;
        }
        match spec.field_type {
            FieldType::Select if spec.options_source == Some(source) => {
                let candidate = values.get(spec.key.as_str()).or(spec.default.as_ref());
                match candidate {
                    Some(Value::String(s)) => {
                        let trimmed = s.trim();
                        if !trimmed.is_empty() {
                            out.push(trimmed.to_owned());
                        }
                    }
                    Some(Value::Array(arr)) if spec.multiple => {
                        for item in arr {
                            if let Some(s) = item.as_str() {
                                let trimmed = s.trim();
                                if !trimmed.is_empty() {
                                    out.push(trimmed.to_owned());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            FieldType::Group => {
                let child_map: BTreeMap<String, Value> = values
                    .get(spec.key.as_str())
                    .and_then(Value::as_object)
                    .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();
                collect_options_source_values_recursive(&spec.fields, &child_map, source, out);
            }
            _ => {}
        }
    }
}

pub fn extract_visible_text(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    out: &mut Vec<String>,
) {
    for spec in specs {
        if spec.is_ui_only() || spec.field_type == FieldType::Secret {
            continue;
        }
        let Some(val) = values.get(spec.key.as_str()) else {
            continue;
        };
        match spec.field_type {
            FieldType::Text | FieldType::Textarea => {
                if let Some(s) = val.as_str() {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        out.push(trimmed.to_owned());
                    }
                }
            }
            FieldType::Richtext => {
                if let Some(s) = val.as_str() {
                    let plain = strip_richtext_to_plain(s);
                    let trimmed = plain.trim();
                    if !trimmed.is_empty() {
                        out.push(trimmed.to_owned());
                    }
                }
            }
            FieldType::Link => {
                if let Ok(link) = serde_json::from_value::<LinkValue>(val.clone()) {
                    let trimmed = link.label.trim();
                    if !trimmed.is_empty() {
                        out.push(trimmed.to_owned());
                    }
                }
            }
            FieldType::Group => {
                if let Some(obj) = val.as_object() {
                    let map: BTreeMap<String, Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    extract_visible_text(&spec.fields, &map, out);
                }
            }
            FieldType::Repeater => {
                if let Some(arr) = val.as_array() {
                    for item in arr.iter().take(MAX_REPEATER_ITEMS) {
                        if let Some(obj) = item.as_object() {
                            let map: BTreeMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            extract_visible_text(&spec.fields, &map, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn default_value_for_field_spec(spec: &FieldSpec) -> Value {
    if let Some(v) = spec.default.as_ref().filter(|v| !v.is_null()) {
        return v.clone();
    }
    match spec.field_type {
        FieldType::Text
        | FieldType::Textarea
        | FieldType::Email
        | FieldType::Url
        | FieldType::Richtext
        | FieldType::Color
        | FieldType::Icon
        | FieldType::Date
        | FieldType::Datetime
        | FieldType::Time => Value::String(String::new()),
        FieldType::Number => serde_json::json!(0),
        FieldType::Range => spec
            .min
            .map(canonicalize_f64)
            .map(|n| serde_json::json!(n))
            .unwrap_or_else(|| serde_json::json!(0)),
        FieldType::Boolean => Value::Bool(false),
        FieldType::Select => {
            if spec.multiple {
                Value::Array(Vec::new())
            } else if spec.allow_null || spec.options_source.is_some() {
                Value::String(String::new())
            } else {
                spec.options
                    .first()
                    .map(|o| Value::String(o.value.clone()))
                    .unwrap_or_else(|| Value::String(String::new()))
            }
        }
        FieldType::RadioButtons => spec
            .options
            .first()
            .map(|o| Value::String(o.value.clone()))
            .unwrap_or_else(|| Value::String(String::new())),
        FieldType::Checkbox | FieldType::Gallery | FieldType::Repeater => Value::Array(Vec::new()),
        FieldType::Spacing => serde_json::json!({}),
        FieldType::Link => {
            serde_json::json!({ "url": "", "label": "", "new_tab": false, "nofollow": false })
        }
        FieldType::Group => {
            let mut map = serde_json::Map::new();
            for sub in &spec.fields {
                if !sub.is_ui_only() {
                    map.insert(sub.key.clone(), default_value_for_field_spec(sub));
                }
            }
            Value::Object(map)
        }
        FieldType::Media
        | FieldType::EntryRef
        | FieldType::TermRef
        | FieldType::Secret
        | FieldType::Tab
        | FieldType::Accordion
        | FieldType::Message => Value::Null,
    }
}

/// Global Site Options using UI-only `Accordion` primitives so all 12 data option keys
/// live flat at the root (`social_facebook`, `site_logo`, `brand_primary_color`, etc.).
pub fn global_site_option_fields() -> Vec<FieldSpec> {
    let acc_social = {
        let mut s = FieldSpec::new(FieldType::Accordion, "acc_social_links", "Mạng xã hội");
        s.open_by_default = true;
        s.multi_expand = true;
        s
    };
    let acc_brand = {
        let mut s = FieldSpec::new(
            FieldType::Accordion,
            "acc_brand_contact",
            "Thương hiệu & Liên hệ chung",
        );
        s.open_by_default = true;
        s.multi_expand = true;
        s
    };
    let url_spec = |key: &str, label: &str, ph: &str| {
        let mut s = FieldSpec::new(FieldType::Url, key, label);
        s.public = true;
        s.placeholder = Some(ph.to_owned());
        s
    };

    let mut logo = FieldSpec::new(FieldType::Media, "site_logo", "Logo website");
    logo.public = true;
    logo.media_kind = Some(MediaFieldKind::Image);

    let mut brand_color = FieldSpec::new(FieldType::Color, "brand_primary_color", "Màu chủ đạo");
    brand_color.public = true;
    brand_color.default = Some(Value::String("#2563eb".to_owned()));

    let mut phone = FieldSpec::new(FieldType::Text, "contact_phone", "Số điện thoại");
    phone.public = true;
    phone.max_chars = Some(40);
    phone.placeholder = Some("0901 234 567".to_owned());

    let mut addr = FieldSpec::new(FieldType::Textarea, "contact_address", "Địa chỉ");
    addr.public = true;
    addr.max_chars = Some(300);

    let mut footer = FieldSpec::new(FieldType::Text, "footer_note", "Ghi chú chân trang");
    footer.public = true;
    footer.max_chars = Some(240);
    footer.placeholder = Some("© 2026 OpenDash".to_owned());

    vec![
        acc_social,
        url_spec(
            "social_facebook",
            "Facebook",
            "https://facebook.com/yourpage",
        ),
        url_spec(
            "social_youtube",
            "YouTube",
            "https://youtube.com/@yourchannel",
        ),
        url_spec("social_tiktok", "TikTok", "https://tiktok.com/@yourhandle"),
        url_spec(
            "social_instagram",
            "Instagram",
            "https://instagram.com/yourhandle",
        ),
        url_spec("social_x", "X (Twitter)", "https://x.com/yourhandle"),
        url_spec(
            "social_linkedin",
            "LinkedIn",
            "https://linkedin.com/in/yourprofile",
        ),
        url_spec("social_email", "Email liên hệ", "mailto:hello@example.com"),
        acc_brand,
        logo,
        brand_color,
        phone,
        addr,
        footer,
    ]
}

pub fn global_site_option_defaults() -> BTreeMap<String, Value> {
    let fields = global_site_option_fields();
    let mut out = BTreeMap::new();
    for spec in &fields {
        if !spec.is_ui_only() {
            out.insert(spec.key.clone(), default_value_for_field_spec(spec));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(value: &str, label: &str) -> SelectOption {
        SelectOption {
            value: value.to_owned(),
            label: label.to_owned(),
            icon: None,
        }
    }

    fn rule(field: &str, op: ConditionOp, value: Option<Value>) -> ConditionRule {
        ConditionRule {
            field: field.to_owned(),
            op,
            value,
        }
    }

    #[test]
    fn validates_all_25_data_types_and_3_layouts_in_studio_block() {
        let mut tab1 = FieldSpec::new(FieldType::Tab, "tab_content", "Nội dung");
        tab1.placement = Some(FieldLayoutPlacement::Top);
        let mut acc1 = FieldSpec::new(FieldType::Accordion, "acc_meta", "Chi tiết");
        acc1.open_by_default = true;
        acc1.multi_expand = true;
        let mut msg = FieldSpec::new(FieldType::Message, "msg_hint", "Lưu ý");
        msg.body = Some("Điền đầy đủ trước khi lưu.".to_owned());
        msg.tone = Some(MessageTone::Info);

        let text = FieldSpec::new(FieldType::Text, "title", "Tiêu đề");
        let textarea = FieldSpec::new(FieldType::Textarea, "summary", "Tóm tắt");
        let num = FieldSpec::new(FieldType::Number, "cols", "Số cột");
        let mut range = FieldSpec::new(FieldType::Range, "opacity", "Độ mờ");
        range.min = Some(0.0);
        range.max = Some(100.0);
        range.default = Some(serde_json::json!(80));
        let email = FieldSpec::new(FieldType::Email, "contact_email", "Email");
        let url = FieldSpec::new(FieldType::Url, "website", "Website");
        let rt = FieldSpec::new(FieldType::Richtext, "body_copy", "Nội dung chi tiết");
        let media = FieldSpec::new(FieldType::Media, "hero_image", "Ảnh chính");
        let gallery = FieldSpec::new(FieldType::Gallery, "photos", "Thư viện ảnh");
        let link = FieldSpec::new(FieldType::Link, "cta_link", "Nút CTA");

        let mut sel = FieldSpec::new(FieldType::Select, "tone", "Tông màu");
        sel.options = vec![opt("brand", "Brand"), opt("neutral", "Neutral")];
        let mut sel_dyn = FieldSpec::new(FieldType::Select, "nav_menu", "Menu");
        sel_dyn.options_source = Some(SelectOptionsSource::Menus);

        let mut chk = FieldSpec::new(FieldType::Checkbox, "features", "Tính năng");
        chk.options = vec![opt("fast", "Nhanh"), opt("safe", "An toàn")];
        chk.layout = Some("horizontal".to_owned());

        let mut radio = FieldSpec::new(FieldType::RadioButtons, "align", "Căn lề");
        radio.options = vec![opt("left", "Trái"), opt("right", "Phải")];
        radio.layout = Some("buttons".to_owned());

        let boolean = FieldSpec::new(FieldType::Boolean, "show_badge", "Hiện nhãn");
        let color = FieldSpec::new(FieldType::Color, "accent_color", "Màu nhấn");
        let icon = FieldSpec::new(FieldType::Icon, "badge_icon", "Biểu tượng");
        let date = FieldSpec::new(FieldType::Date, "launch_date", "Ngày ra mắt");
        let datetime = FieldSpec::new(FieldType::Datetime, "publish_at", "Thời điểm");
        let time = FieldSpec::new(FieldType::Time, "open_time", "Giờ mở");
        let mut spacing = FieldSpec::new(FieldType::Spacing, "outer_margin", "Lề ngoài");
        spacing.property = Some(SpacingProperty::Margin);

        let mut entry_ref = FieldSpec::new(FieldType::EntryRef, "related_posts", "Bài liên quan");
        entry_ref.target_types = vec!["*".to_owned()];
        entry_ref.multiple = true;

        let mut term_tax = FieldSpec::new(FieldType::Select, "tax_kind", "Loại phân loại");
        term_tax.options_source = Some(SelectOptionsSource::Taxonomies);
        let mut term_ref = FieldSpec::new(FieldType::TermRef, "chosen_terms", "Thẻ chọn");
        term_ref.taxonomy_from_field = Some("tax_kind".to_owned());
        term_ref.multiple = true;

        let mut group = FieldSpec::new(FieldType::Group, "card_group", "Nhóm thẻ");
        group.fields = vec![FieldSpec::new(FieldType::Text, "heading", "Tiêu đề thẻ")];

        let mut rep = FieldSpec::new(FieldType::Repeater, "items", "Danh sách");
        rep.fields = vec![FieldSpec::new(FieldType::Text, "item_title", "Tên mục")];
        rep.item_label_field = Some("item_title".to_owned());

        let endpoint = {
            let mut a = FieldSpec::new(FieldType::Accordion, "acc_end", "");
            a.endpoint = true;
            a
        };

        let specs = vec![
            tab1, acc1, msg, text, textarea, num, range, email, url, rt, media, gallery, link, sel,
            sel_dyn, chk, radio, boolean, color, icon, date, datetime, time, spacing, entry_ref,
            term_tax, term_ref, group, rep, endpoint,
        ];
        assert!(validate_field_specs(&specs, FieldContext::StudioBlock).is_ok());
    }

    #[test]
    fn owner_settings_allows_exact_13_data_types_plus_secret_and_rejects_others() {
        let mut secret = FieldSpec::new(FieldType::Secret, "api_key", "API Key");
        secret.required = true;
        let specs = vec![
            FieldSpec::new(FieldType::Text, "a", "A"),
            FieldSpec::new(FieldType::Textarea, "b", "B"),
            FieldSpec::new(FieldType::Number, "c", "C"),
            {
                let mut r = FieldSpec::new(FieldType::Range, "d", "D");
                r.min = Some(1.0);
                r.max = Some(10.0);
                r
            },
            FieldSpec::new(FieldType::Boolean, "e", "E"),
            {
                let mut s = FieldSpec::new(FieldType::Select, "f", "F");
                s.options = vec![opt("one", "One")];
                s
            },
            {
                let mut rb = FieldSpec::new(FieldType::RadioButtons, "g", "G");
                rb.options = vec![opt("r1", "R1")];
                rb
            },
            {
                let mut cb = FieldSpec::new(FieldType::Checkbox, "h", "H");
                cb.options = vec![opt("c1", "C1")];
                cb
            },
            FieldSpec::new(FieldType::Email, "i", "I"),
            FieldSpec::new(FieldType::Url, "j", "J"),
            FieldSpec::new(FieldType::Media, "k", "K"),
            secret,
            {
                let mut g = FieldSpec::new(FieldType::Group, "l", "L");
                g.fields = vec![FieldSpec::new(FieldType::Text, "sub", "Sub")];
                g
            },
        ];
        assert!(validate_field_specs(&specs, FieldContext::OwnerSettings).is_ok());

        for forbidden in [
            FieldType::Richtext,
            FieldType::Gallery,
            FieldType::Link,
            FieldType::Color,
            FieldType::Icon,
            FieldType::Date,
            FieldType::Datetime,
            FieldType::Time,
            FieldType::Spacing,
            FieldType::EntryRef,
            FieldType::TermRef,
            FieldType::Repeater,
        ] {
            let mut bad = FieldSpec::new(forbidden, "bad_field", "Bad");
            if matches!(forbidden, FieldType::Repeater) {
                bad.fields = vec![FieldSpec::new(FieldType::Text, "s", "S")];
            }
            if matches!(forbidden, FieldType::EntryRef) {
                bad.target_types = vec!["post".to_owned()];
            }
            if matches!(forbidden, FieldType::TermRef) {
                bad.taxonomy = Some("category".to_owned());
            }
            assert!(
                validate_field_specs(&[bad], FieldContext::OwnerSettings).is_err(),
                "OwnerSettings must reject {forbidden:?}"
            );
        }
    }

    #[test]
    fn forbids_secret_in_other_contexts_or_inside_group_or_with_default() {
        let mut s = FieldSpec::new(FieldType::Secret, "tok", "Token");
        assert!(validate_field_specs(std::slice::from_ref(&s), FieldContext::Options).is_err());
        assert!(validate_field_specs(std::slice::from_ref(&s), FieldContext::Content).is_err());

        s.default = Some(Value::String("plaintext".to_owned()));
        assert!(
            validate_field_specs(std::slice::from_ref(&s), FieldContext::OwnerSettings).is_err()
        );

        let mut g = FieldSpec::new(FieldType::Group, "grp", "Group");
        g.fields = vec![FieldSpec::new(FieldType::Secret, "tok", "Token")];
        assert!(validate_field_specs(&[g], FieldContext::OwnerSettings).is_err());
    }

    #[test]
    fn forbids_secret_ref_subkey_inside_any_group() {
        let mut g = FieldSpec::new(FieldType::Group, "grp", "Group");
        g.fields = vec![FieldSpec::new(FieldType::Text, "secret_ref", "Bad Subkey")];
        assert!(validate_field_specs(&[g], FieldContext::Options).is_err());
    }

    #[test]
    fn forbids_sync_entry_terms_in_wave_1() {
        let mut tr = FieldSpec::new(FieldType::TermRef, "cats", "Categories");
        tr.taxonomy = Some("category".to_owned());
        tr.sync_entry_terms = true;
        let err = validate_field_specs(&[tr], FieldContext::Content).unwrap_err();
        assert!(err.contains("Đợt 2"));
    }

    #[test]
    fn forbids_bindable_unknown_json_property() {
        let json = r#"{"type":"text","key":"title","label":"Title","bindable":true}"#;
        assert!(serde_json::from_str::<FieldSpec>(json).is_err());
    }

    #[test]
    fn spacing_padding_vs_margin_bounds_and_tailwind_classes() {
        let mut pad = FieldSpec::new(FieldType::Spacing, "pad", "Padding");
        pad.min = Some(-4.0);
        assert!(validate_field_specs(&[pad], FieldContext::StudioBlock).is_err());

        let mut margin = FieldSpec::new(FieldType::Spacing, "mar", "Margin");
        margin.property = Some(SpacingProperty::Margin);
        margin.min = Some(-64.0);
        margin.max = Some(128.0);
        assert!(validate_field_specs(&[margin], FieldContext::StudioBlock).is_ok());

        let mut margin_oob = FieldSpec::new(FieldType::Spacing, "mar", "Margin");
        margin_oob.property = Some(SpacingProperty::Margin);
        margin_oob.min = Some(-300.0);
        assert!(validate_field_specs(&[margin_oob], FieldContext::StudioBlock).is_err());

        let val = SpacingValue {
            top: Some(-16.0),
            right: Some(8.0),
            bottom: Some(24.0),
            left: Some(0.0),
        };
        assert_eq!(
            val.format_classes(SpacingProperty::Margin, CssUnit::Px),
            "mr-2 ml-0 -mt-4 mb-6"
        );
        let pad_val = SpacingValue {
            top: Some(16.0),
            right: Some(8.0),
            bottom: Some(24.0),
            left: Some(0.0),
        };
        assert_eq!(
            pad_val.format_classes(SpacingProperty::Padding, CssUnit::Px),
            "pr-2 pl-0 pt-4 pb-6"
        );
        assert_eq!(
            pad_val.format_classes(SpacingProperty::Padding, CssUnit::Rem),
            ""
        );
    }

    #[test]
    fn gallery_forbids_file_media_kind() {
        let mut g = FieldSpec::new(FieldType::Gallery, "docs", "Docs");
        g.media_kind = Some(MediaFieldKind::File);
        assert!(validate_field_specs(&[g], FieldContext::Content).is_err());
    }

    #[test]
    fn select_options_source_constraints() {
        let mut s = FieldSpec::new(FieldType::Select, "menu", "Menu");
        s.options_source = Some(SelectOptionsSource::Menus);
        assert!(validate_field_specs(std::slice::from_ref(&s), FieldContext::StudioBlock).is_ok());
        assert!(validate_field_specs(std::slice::from_ref(&s), FieldContext::Content).is_err());

        let mut with_opts = s.clone();
        with_opts.options = vec![opt("a", "A")];
        assert!(validate_field_specs(&[with_opts], FieldContext::StudioBlock).is_err());

        let mut with_resp = s.clone();
        with_resp.responsive = true;
        assert!(validate_field_specs(&[with_resp], FieldContext::StudioBlock).is_err());

        let mut with_css = s.clone();
        with_css.css_var = Some("--od-menu".to_owned());
        assert!(validate_field_specs(&[with_css], FieldContext::StudioBlock).is_err());

        let mut rep = FieldSpec::new(FieldType::Repeater, "rep", "Repeater");
        rep.fields = vec![s];
        assert!(validate_field_specs(&[rep], FieldContext::StudioBlock).is_err());
    }

    #[test]
    fn entry_ref_wildcard_rules() {
        let mut e = FieldSpec::new(FieldType::EntryRef, "items", "Items");
        e.target_types = vec!["*".to_owned()];
        assert!(validate_field_specs(std::slice::from_ref(&e), FieldContext::StudioBlock).is_ok());
        assert!(validate_field_specs(std::slice::from_ref(&e), FieldContext::Content).is_err());
        assert!(validate_field_specs(std::slice::from_ref(&e), FieldContext::Options).is_err());

        e.target_types = vec!["*".to_owned(), "post".to_owned()];
        assert!(validate_field_specs(std::slice::from_ref(&e), FieldContext::StudioBlock).is_err());

        e.target_types = vec!["pattern".to_owned()];
        assert!(validate_field_specs(&[e], FieldContext::StudioBlock).is_err());
    }

    #[test]
    fn term_ref_taxonomy_from_field_rules() {
        let mut tax_sel = FieldSpec::new(FieldType::Select, "tax_field", "Taxonomy");
        tax_sel.options_source = Some(SelectOptionsSource::Taxonomies);
        let mut tr = FieldSpec::new(FieldType::TermRef, "terms", "Terms");
        tr.taxonomy_from_field = Some("tax_field".to_owned());
        assert!(
            validate_field_specs(&[tax_sel.clone(), tr.clone()], FieldContext::StudioBlock).is_ok()
        );
        assert!(
            validate_field_specs(&[tax_sel.clone(), tr.clone()], FieldContext::Content).is_err()
        );

        // mutually exclusive with taxonomy
        let mut both = tr.clone();
        both.taxonomy = Some("category".to_owned());
        assert!(validate_field_specs(&[tax_sel.clone(), both], FieldContext::StudioBlock).is_err());

        // target must not be responsive
        let mut resp_sel = FieldSpec::new(FieldType::Select, "tax_field", "Taxonomy");
        resp_sel.options = vec![opt("category", "Cat")];
        resp_sel.responsive = true;
        assert!(validate_field_specs(&[resp_sel, tr.clone()], FieldContext::StudioBlock).is_err());

        // target must not be multiple
        let mut mult_sel = tax_sel.clone();
        mult_sel.multiple = true;
        assert!(validate_field_specs(&[mult_sel, tr.clone()], FieldContext::StudioBlock).is_err());

        // forbidden inside repeater
        let mut rep = FieldSpec::new(FieldType::Repeater, "rep", "Rep");
        rep.fields = vec![tr];
        assert!(validate_field_specs(&[tax_sel, rep], FieldContext::StudioBlock).is_err());
    }

    #[test]
    fn normalizes_acl_and_enforces_m7_s09_invariants() {
        let f = FieldSpec::new(FieldType::Text, "k", "L");
        let mut user_specs = vec![f.clone()];
        normalize_field_specs_for_scope(
            &mut user_specs,
            FieldContext::Content,
            FieldScopeKind::UserFieldGroup,
        )
        .unwrap();
        let acl = user_specs[0].acl.as_ref().unwrap();
        assert_eq!(acl.public_exposure, Some(PublicExposure::Public));
        assert_eq!(acl.min_view_role, Some(FieldRole::Editor));
        assert_eq!(acl.min_edit_role, Some(FieldRole::Editor));

        let mut term_panel = vec![f.clone()];
        normalize_field_specs_for_scope(
            &mut term_panel,
            FieldContext::Term,
            FieldScopeKind::TermPanelField,
        )
        .unwrap();
        assert_eq!(
            term_panel[0].acl.as_ref().unwrap().public_exposure,
            Some(PublicExposure::Private)
        );

        let mut pub_term_panel = vec![f.clone()];
        pub_term_panel[0].acl = Some(FieldAcl::canonical(
            FieldRole::Editor,
            FieldRole::Editor,
            PublicExposure::Public,
        ));
        assert!(
            normalize_field_specs_for_scope(
                &mut pub_term_panel,
                FieldContext::Term,
                FieldScopeKind::TermPanelField
            )
            .is_err()
        );

        // M7-S09 #1: min_edit_role < min_view_role rejected
        let mut bad1 = vec![f.clone()];
        bad1[0].acl = Some(FieldAcl::canonical(
            FieldRole::Admin,
            FieldRole::Editor,
            PublicExposure::Private,
        ));
        assert!(
            normalize_field_specs_for_scope(
                &mut bad1,
                FieldContext::Content,
                FieldScopeKind::UserFieldGroup
            )
            .is_err()
        );

        // M7-S09 #2: Public with Admin view rejected
        let mut bad2 = vec![f.clone()];
        bad2[0].acl = Some(FieldAcl::canonical(
            FieldRole::Admin,
            FieldRole::Admin,
            PublicExposure::Public,
        ));
        assert!(
            normalize_field_specs_for_scope(
                &mut bad2,
                FieldContext::Content,
                FieldScopeKind::UserFieldGroup
            )
            .is_err()
        );

        // M7-S09 #4: required + Admin edit requires non-empty default
        let mut bad4 = vec![f.clone()];
        bad4[0].required = true;
        bad4[0].acl = Some(FieldAcl::canonical(
            FieldRole::Editor,
            FieldRole::Admin,
            PublicExposure::Public,
        ));
        assert!(
            normalize_field_specs_for_scope(
                &mut bad4,
                FieldContext::Content,
                FieldScopeKind::UserFieldGroup
            )
            .is_err()
        );

        let mut ok4 = vec![f];
        ok4[0].required = true;
        ok4[0].default = Some(Value::String("approved".to_owned()));
        ok4[0].acl = Some(FieldAcl::canonical(
            FieldRole::Editor,
            FieldRole::Admin,
            PublicExposure::Public,
        ));
        assert!(
            normalize_field_specs_for_scope(
                &mut ok4,
                FieldContext::Content,
                FieldScopeKind::UserFieldGroup
            )
            .is_ok()
        );
    }

    #[test]
    fn conditions_dfs_cycle_and_acl_oracle_rejected() {
        let mut a = FieldSpec::new(FieldType::Text, "a", "A");
        let mut b = FieldSpec::new(FieldType::Text, "b", "B");
        a.conditions = Some(vec![vec![rule("b", ConditionOp::NotEmpty, None)]]);
        b.conditions = Some(vec![vec![rule("a", ConditionOp::NotEmpty, None)]]);
        assert!(validate_field_specs(&[a, b], FieldContext::Content).is_err());

        let mut secret_admin = FieldSpec::new(FieldType::Text, "admin_flag", "Admin Flag");
        secret_admin.acl = Some(FieldAcl::canonical(
            FieldRole::Admin,
            FieldRole::Admin,
            PublicExposure::Private,
        ));
        let mut editor_dep = FieldSpec::new(FieldType::Text, "public_hint", "Hint");
        editor_dep.acl = Some(FieldAcl::canonical(
            FieldRole::Editor,
            FieldRole::Editor,
            PublicExposure::Public,
        ));
        editor_dep.conditions = Some(vec![vec![rule(
            "admin_flag",
            ConditionOp::Eq,
            Some(Value::String("yes".to_owned())),
        )]]);
        assert!(validate_field_specs(&[secret_admin, editor_dep], FieldContext::Content).is_err());
    }

    #[test]
    fn conditions_evaluate_dnf_and_datetime_time_and_cascade() {
        let mut ts = FieldSpec::new(FieldType::Datetime, "publish_at", "Publish At");
        ts.default = Some(Value::String("2026-11-26T10:00:00.000Z".to_owned()));
        let mut tm = FieldSpec::new(FieldType::Time, "start_time", "Start Time");
        tm.default = Some(Value::String("09:30".to_owned()));
        let mut flag_a = FieldSpec::new(FieldType::Boolean, "a", "A");
        flag_a.default = Some(Value::Bool(false));
        let mut b = FieldSpec::new(FieldType::Text, "b", "B");
        b.conditions = Some(vec![vec![rule(
            "a",
            ConditionOp::Eq,
            Some(Value::Bool(true)),
        )]]);
        let mut c = FieldSpec::new(FieldType::Text, "c", "C");
        c.conditions = Some(vec![vec![
            rule(
                "b",
                ConditionOp::Contains,
                Some(Value::String("keep".to_owned())),
            ),
            rule(
                "publish_at",
                ConditionOp::Gt,
                Some(Value::String("2026-11-26T09:59:59Z".to_owned())),
            ),
        ]]);
        let mut d = FieldSpec::new(FieldType::Text, "d", "D");
        d.conditions = Some(vec![vec![rule(
            "start_time",
            ConditionOp::Lt,
            Some(Value::String("09:30:01".to_owned())),
        )]]);
        let specs = vec![
            ts.clone(),
            tm.clone(),
            flag_a.clone(),
            b.clone(),
            c.clone(),
            d.clone(),
        ];
        assert!(validate_field_specs(&specs, FieldContext::Options).is_ok());

        let mut vals = BTreeMap::new();
        vals.insert("a".to_owned(), Value::Bool(false));
        vals.insert("b".to_owned(), Value::String("keep_value".to_owned()));
        vals.insert(
            "publish_at".to_owned(),
            Value::String("2026-11-26T10:00:00Z".to_owned()),
        );
        vals.insert(
            "start_time".to_owned(),
            Value::String("09:30:00".to_owned()),
        );

        // B is hidden because a == false, yet B's preserved value is still read by C!
        assert!(!evaluate_conditions(
            b.conditions.as_ref(),
            &specs,
            &vals,
            &specs,
            &vals
        ));
        assert!(evaluate_conditions(
            c.conditions.as_ref(),
            &specs,
            &vals,
            &specs,
            &vals
        ));
        assert!(evaluate_conditions(
            d.conditions.as_ref(),
            &specs,
            &vals,
            &specs,
            &vals
        ));

        // `.000Z` vs `Z` equal -> Gt must be false
        let eq_cond = vec![vec![rule(
            "publish_at",
            ConditionOp::Gt,
            Some(Value::String("2026-11-26T10:00:00.000Z".to_owned())),
        )]];
        assert!(!evaluate_conditions(
            Some(&eq_cond),
            &specs,
            &vals,
            &specs,
            &vals
        ));
    }

    #[test]
    fn hidden_required_skipped_on_save_but_xss_validated() {
        let toggle = FieldSpec::new(FieldType::Boolean, "show", "Show");
        let mut detail = FieldSpec::new(FieldType::Url, "detail_url", "Detail URL");
        detail.required = true;
        detail.conditions = Some(vec![vec![rule(
            "show",
            ConditionOp::Eq,
            Some(Value::Bool(true)),
        )]]);
        let specs = vec![toggle, detail];

        let mut vals = BTreeMap::new();
        vals.insert("show".to_owned(), Value::Bool(false));
        assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_ok());

        // Hidden field updated with unsafe javascript: URL must still fail!
        vals.insert(
            "detail_url".to_owned(),
            Value::String("javascript:alert(1)".to_owned()),
        );
        assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_err());
    }

    #[test]
    fn canonicalizes_negative_zero_and_compat_matrix() {
        let mut n = FieldSpec::new(FieldType::Number, "n", "Number");
        n.min = Some(-0.0);
        n.default = Some(serde_json::json!(-0.0));
        let mut list = vec![n];
        canonicalize_field_specs(&mut list);
        assert!(list[0].min.unwrap().is_sign_positive());
        assert_eq!(
            serde_json::to_value(list[0].default.as_ref()).unwrap(),
            serde_json::json!(0.0)
        );

        let mut f_text = FieldSpec::new(FieldType::Text, "title", "Title");
        f_text.acl = Some(FieldAcl::canonical(
            FieldRole::Editor,
            FieldRole::Editor,
            PublicExposure::Public,
        ));
        let mut f_textarea = FieldSpec::new(FieldType::Textarea, "title", "Title");
        f_textarea.acl = f_text.acl.clone();
        assert!(
            check_schema_compatibility(
                std::slice::from_ref(&f_text),
                std::slice::from_ref(&f_textarea),
                true
            )
            .is_ok()
        );
        assert!(
            check_schema_compatibility(
                std::slice::from_ref(&f_textarea),
                std::slice::from_ref(&f_text),
                true
            )
            .is_err()
        );

        // Removing key with existing data fails
        assert!(check_schema_compatibility(std::slice::from_ref(&f_text), &[], true).is_err());

        // UI-only changes are compatible even with existing data
        let msg = {
            let mut m = FieldSpec::new(FieldType::Message, "m", "Note");
            m.body = Some("Info".to_owned());
            m
        };
        assert!(
            check_schema_compatibility(&[f_text.clone()], &[msg, f_text.clone()], true).is_ok()
        );

        // Media narrowing blocked, widening allowed
        let mut m_img = FieldSpec::new(FieldType::Media, "cov", "Cover");
        m_img.media_kind = Some(MediaFieldKind::Image);
        let mut m_any = FieldSpec::new(FieldType::Media, "cov", "Cover");
        m_any.media_kind = Some(MediaFieldKind::Any);
        assert!(check_schema_compatibility(&[m_img.clone()], &[m_any.clone()], true).is_ok());
        assert!(check_schema_compatibility(&[m_any], &[m_img], true).is_err());
    }

    #[test]
    fn collects_media_relational_visible_text_and_global_site_options() {
        let globals = global_site_option_fields();
        assert!(validate_field_specs(&globals, FieldContext::Options).is_ok());
        let defaults = global_site_option_defaults();
        assert!(defaults.contains_key("social_facebook"));
        assert!(defaults.contains_key("site_logo"));
        assert!(!defaults.contains_key("acc_social_links"));

        let id1 = "a".repeat(64);
        let id2 = "b".repeat(64);
        let mut m_pub = FieldSpec::new(FieldType::Media, "cover", "Cover");
        m_pub.acl = Some(FieldAcl::canonical(
            FieldRole::Editor,
            FieldRole::Editor,
            PublicExposure::Public,
        ));
        let mut m_priv = FieldSpec::new(FieldType::Media, "priv_doc", "Private");
        m_priv.media_kind = Some(MediaFieldKind::File);
        m_priv.acl = Some(FieldAcl::canonical(
            FieldRole::Editor,
            FieldRole::Editor,
            PublicExposure::Private,
        ));
        let mut er = FieldSpec::new(FieldType::EntryRef, "rel", "Rel");
        er.target_types = vec!["post".to_owned()];
        er.multiple = true;
        let mut grp = FieldSpec::new(FieldType::Group, "g", "G");
        grp.fields = vec![FieldSpec::new(FieldType::Text, "note", "Note")];
        let specs = vec![m_pub, m_priv, er, grp];

        let mut vals = BTreeMap::new();
        vals.insert("cover".to_owned(), Value::String(id1.clone()));
        vals.insert("priv_doc".to_owned(), Value::String(id2.clone()));
        vals.insert("rel".to_owned(), serde_json::json!([11, 22]));
        vals.insert("g".to_owned(), serde_json::json!({"note": "Xin chào"}));

        let all_media = collect_media_ids(&specs, &vals);
        assert!(all_media.image_ids.contains(&id1));
        assert!(all_media.file_ids.contains(&id2));

        let pub_media = collect_public_media_ids(&specs, &vals, true);
        assert!(pub_media.image_ids.contains(&id1));
        assert!(!pub_media.file_ids.contains(&id2));

        let rels = collect_relational_refs(&specs, &vals);
        assert_eq!(rels.entry_ids, BTreeSet::from([11, 22]));

        let mut text = Vec::new();
        extract_visible_text(&specs, &vals, &mut text);
        assert_eq!(text, vec!["Xin chào".to_owned()]);
    }

    #[test]
    fn validates_email_url_date_time_color_safely() {
        assert!(valid_email_address("alice@example.com"));
        assert!(!valid_email_address("alice@localhost"));
        assert!(!valid_email_address("alice@@example.com"));
        assert!(!valid_email_address("alice @example.com"));

        assert!(valid_safe_url("https://example.com/path?a=1"));
        assert!(valid_safe_url("/gioi-thieu"));
        assert!(valid_safe_url("mailto:hello@example.com"));
        assert!(!valid_safe_url("javascript:alert(1)"));
        assert!(!valid_safe_url("//evil.example"));

        assert!(valid_iso_date("2024-02-29"));
        assert!(!valid_iso_date("2025-02-29"));
        assert_eq!(
            parse_utc_datetime_millis("2026-11-26T00:00:00Z"),
            parse_utc_datetime_millis("2026-11-26T00:00:00.000Z")
        );
        assert_eq!(parse_time_seconds("01:02"), parse_time_seconds("01:02:00"));
        assert!(valid_color_value("var(--od-token-primary)"));
        assert!(valid_color_value("#123456aa"));
        assert!(!valid_color_value("rgb(1,2,3)"));
    }

    #[test]
    fn nested_child_condition_reads_top_level_and_hidden_parent_group_skips_required() {
        let toggle = FieldSpec::new(FieldType::Boolean, "enable_grp", "Enable Group");
        let show_sub = FieldSpec::new(FieldType::Boolean, "show_sub", "Show Sub");
        let mut child_req = FieldSpec::new(FieldType::Text, "headline", "Headline");
        child_req.required = true;
        child_req.conditions = Some(vec![vec![rule(
            "show_sub",
            ConditionOp::Eq,
            Some(Value::Bool(true)),
        )]]);
        let mut grp = FieldSpec::new(FieldType::Group, "card", "Card");
        grp.conditions = Some(vec![vec![rule(
            "enable_grp",
            ConditionOp::Eq,
            Some(Value::Bool(true)),
        )]]);
        grp.fields = vec![child_req];
        let specs = vec![toggle, show_sub, grp];
        assert!(validate_field_specs(&specs, FieldContext::StudioBlock).is_ok());

        // Parent group hidden even if show_sub=true -> required child omitted should pass
        let mut vals = BTreeMap::new();
        vals.insert("enable_grp".to_owned(), Value::Bool(false));
        vals.insert("show_sub".to_owned(), Value::Bool(true));
        vals.insert("card".to_owned(), serde_json::json!({}));
        assert!(validate_field_values(&specs, FieldContext::StudioBlock, &vals, None).is_ok());

        // Parent group visible & show_sub=true -> required child missing must fail
        vals.insert("enable_grp".to_owned(), Value::Bool(true));
        assert!(validate_field_values(&specs, FieldContext::StudioBlock, &vals, None).is_err());

        // Parent group visible & show_sub=false -> child hidden by top-level condition -> ok
        vals.insert("show_sub".to_owned(), Value::Bool(false));
        assert!(validate_field_values(&specs, FieldContext::StudioBlock, &vals, None).is_ok());
    }

    #[test]
    fn public_group_does_not_leak_private_nested_media_ids() {
        let pub_id = "a".repeat(64);
        let priv_id = "b".repeat(64);
        let mut pub_m = FieldSpec::new(FieldType::Media, "pub_img", "Public Img");
        pub_m.acl = Some(FieldAcl::canonical(
            FieldRole::Editor,
            FieldRole::Editor,
            PublicExposure::Public,
        ));
        let mut priv_m = FieldSpec::new(FieldType::Media, "priv_img", "Private Img");
        priv_m.acl = Some(FieldAcl::canonical(
            FieldRole::Editor,
            FieldRole::Editor,
            PublicExposure::Private,
        ));
        let mut grp = FieldSpec::new(FieldType::Group, "hero", "Hero");
        grp.acl = Some(FieldAcl::canonical(
            FieldRole::Editor,
            FieldRole::Editor,
            PublicExposure::Public,
        ));
        grp.fields = vec![pub_m, priv_m];
        let specs = vec![grp];

        let mut vals = BTreeMap::new();
        vals.insert(
            "hero".to_owned(),
            serde_json::json!({ "pub_img": pub_id, "priv_img": priv_id }),
        );
        let all = collect_media_ids(&specs, &vals);
        assert_eq!(all.image_ids.len(), 2);
        let public_only = collect_public_media_ids(&specs, &vals, true);
        assert!(public_only.image_ids.contains(&pub_id));
        assert!(!public_only.image_ids.contains(&priv_id));
    }

    #[test]
    fn group_repeater_nesting_rejects_three_levels() {
        let leaf = FieldSpec::new(FieldType::Text, "item", "Item");
        let mut inner_grp = FieldSpec::new(FieldType::Group, "inner_g", "Inner");
        inner_grp.fields = vec![leaf.clone()];
        let mut mid_rep = FieldSpec::new(FieldType::Repeater, "mid_r", "Mid");
        mid_rep.fields = vec![inner_grp.clone()];
        let mut outer_grp = FieldSpec::new(FieldType::Group, "outer_g", "Outer");
        outer_grp.fields = vec![mid_rep.clone()];
        assert!(validate_field_specs(&[outer_grp], FieldContext::Content).is_err());

        // Two levels: top-level Repeater -> flat Group is valid
        assert!(validate_field_specs(&[mid_rep], FieldContext::Content).is_ok());

        // Two levels: top-level Group -> Repeater (primitives) is valid
        let mut ok_rep = FieldSpec::new(FieldType::Repeater, "rows", "Rows");
        ok_rep.fields = vec![leaf];
        let mut ok_outer = FieldSpec::new(FieldType::Group, "outer", "Outer");
        ok_outer.fields = vec![ok_rep];
        assert!(validate_field_specs(&[ok_outer], FieldContext::Content).is_ok());
    }

    #[test]
    fn core_entry_body_accepts_large_plain_text_and_blocks() {
        let body = FieldSpec::new(FieldType::Richtext, "body", "Nội dung");
        let specs = vec![body];
        let big_plain = "<p>".to_owned() + &"a".repeat(70_000) + "</p>";
        let mut vals = BTreeMap::new();
        vals.insert("body".to_owned(), Value::String(big_plain));
        assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_ok());

        // Too huge (> 200k chars) rejected
        let too_big = "a".repeat(200_005);
        vals.insert("body".to_owned(), Value::String(too_big));
        assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_err());

        // Non-body richtext remains capped at 32 KiB
        let other = FieldSpec::new(FieldType::Richtext, "notes", "Notes");
        let mut other_vals = BTreeMap::new();
        other_vals.insert(
            "notes".to_owned(),
            Value::String("<p>".to_owned() + &"a".repeat(40_000) + "</p>"),
        );
        assert!(validate_field_values(&[other], FieldContext::Content, &other_vals, None).is_err());
    }

    #[test]
    fn spacing_format_classes_delegates_to_shorthand_tailwind() {
        let eq_pad = SpacingValue {
            top: Some(16.0),
            right: Some(16.0),
            bottom: Some(16.0),
            left: Some(16.0),
        };
        assert_eq!(
            eq_pad.format_classes(SpacingProperty::Padding, CssUnit::Px),
            "p-4"
        );

        let sym_mar = SpacingValue {
            top: Some(-16.0),
            right: Some(8.0),
            bottom: Some(-16.0),
            left: Some(8.0),
        };
        assert_eq!(
            sym_mar.format_classes(SpacingProperty::Margin, CssUnit::Px),
            "mx-2 -my-4"
        );
    }

    #[test]
    fn rejects_invalid_snake_keys_and_duplicate_keys() {
        let bad = FieldSpec::new(FieldType::Text, "1bad", "Bad");
        assert!(validate_field_specs(&[bad], FieldContext::Content).is_err());
        let f1 = FieldSpec::new(FieldType::Text, "dup", "One");
        let f2 = FieldSpec::new(FieldType::Text, "dup", "Two");
        assert!(validate_field_specs(&[f1, f2], FieldContext::Content).is_err());
    }

    #[test]
    fn rejects_empty_and_control_and_overlong_labels() {
        let empty = FieldSpec::new(FieldType::Text, "k", "   ");
        assert!(validate_field_specs(&[empty], FieldContext::Content).is_err());
        let ctrl = FieldSpec::new(FieldType::Text, "k", "Hi\x07");
        assert!(validate_field_specs(&[ctrl], FieldContext::Content).is_err());
        let long = FieldSpec::new(FieldType::Text, "k", "a".repeat(101));
        assert!(validate_field_specs(&[long], FieldContext::Content).is_err());
    }

    #[test]
    fn accordion_endpoint_allows_empty_label_only_outside_repeater() {
        let mut acc = FieldSpec::new(FieldType::Accordion, "acc_end", "");
        acc.endpoint = true;
        assert!(
            validate_field_specs(std::slice::from_ref(&acc), FieldContext::StudioBlock).is_ok()
        );

        let mut rep = FieldSpec::new(FieldType::Repeater, "rep", "Rep");
        rep.fields = vec![acc];
        assert!(validate_field_specs(&[rep], FieldContext::StudioBlock).is_err());
    }

    #[test]
    fn message_requires_plain_non_empty_no_html_body() {
        let mut msg = FieldSpec::new(FieldType::Message, "m", "Note");
        assert!(validate_field_specs(std::slice::from_ref(&msg), FieldContext::Options).is_err());
        msg.body = Some("<b>No</b>".to_owned());
        assert!(validate_field_specs(std::slice::from_ref(&msg), FieldContext::Options).is_err());
        msg.body = Some("Read docs carefully.".to_owned());
        assert!(validate_field_specs(&[msg], FieldContext::Options).is_ok());
    }

    #[test]
    fn validates_width_affixes_and_instructions_limits() {
        let mut f = FieldSpec::new(FieldType::Text, "k", "L");
        f.width = Some(50);
        f.prepend = Some("https://".to_owned());
        f.instructions = Some("Line 1\nLine 2".to_owned());
        assert!(validate_field_specs(std::slice::from_ref(&f), FieldContext::Content).is_ok());

        f.width = Some(40);
        assert!(validate_field_specs(std::slice::from_ref(&f), FieldContext::Content).is_err());
        f.width = Some(50);
        f.prepend = Some("x".repeat(33));
        assert!(validate_field_specs(std::slice::from_ref(&f), FieldContext::Content).is_err());
    }

    #[test]
    fn prepend_and_append_rejected_on_non_affix_types() {
        let mut b = FieldSpec::new(FieldType::Boolean, "flag", "Flag");
        b.prepend = Some("pre".to_owned());
        assert!(validate_field_specs(&[b], FieldContext::Content).is_err());
    }

    #[test]
    fn validates_text_and_textarea_character_bounds() {
        let mut t = FieldSpec::new(FieldType::Text, "t", "T");
        t.max_chars = Some(2001);
        assert!(validate_field_specs(&[t], FieldContext::Content).is_err());

        let mut ta = FieldSpec::new(FieldType::Textarea, "ta", "TA");
        ta.rows = Some(17);
        assert!(validate_field_specs(&[ta], FieldContext::Content).is_err());
    }

    #[test]
    fn validates_number_and_range_finiteness_and_steps() {
        let mut r = FieldSpec::new(FieldType::Range, "r", "R");
        r.min = Some(10.0);
        assert!(validate_field_specs(std::slice::from_ref(&r), FieldContext::Content).is_err());
        r.max = Some(5.0);
        assert!(validate_field_specs(std::slice::from_ref(&r), FieldContext::Content).is_err());
        r.max = Some(20.0);
        r.step = Some(0.0);
        assert!(validate_field_specs(&[r], FieldContext::Content).is_err());
    }

    #[test]
    fn rejects_default_on_media_gallery_and_ui_only() {
        let mut m = FieldSpec::new(FieldType::Media, "m", "M");
        m.default = Some(Value::String("a".repeat(64)));
        assert!(validate_field_specs(&[m], FieldContext::Content).is_err());

        let mut g = FieldSpec::new(FieldType::Gallery, "g", "G");
        g.default = Some(serde_json::json!([]));
        assert!(validate_field_specs(&[g], FieldContext::Content).is_err());

        let mut t = FieldSpec::new(FieldType::Tab, "tab", "Tab");
        t.default = Some(Value::String("x".to_owned()));
        assert!(validate_field_specs(&[t], FieldContext::Content).is_err());
    }

    #[test]
    fn link_default_sanitizes_url_and_label_length() {
        let mut l = FieldSpec::new(FieldType::Link, "cta", "CTA");
        l.default = Some(serde_json::json!({"url": "javascript:evil()", "label": "Click"}));
        assert!(validate_field_specs(std::slice::from_ref(&l), FieldContext::Content).is_err());

        l.default = Some(serde_json::json!({"url": "/ok", "label": "x".repeat(161)}));
        assert!(validate_field_specs(&[l], FieldContext::Content).is_err());
    }

    #[test]
    fn checkbox_and_radio_layouts_and_option_caps() {
        let mut cb = FieldSpec::new(FieldType::Checkbox, "cb", "CB");
        cb.options = vec![opt("a", "A")];
        cb.layout = Some("table".to_owned());
        assert!(validate_field_specs(&[cb], FieldContext::Content).is_err());

        let mut rb = FieldSpec::new(FieldType::RadioButtons, "rb", "RB");
        rb.options = (0..13)
            .map(|i| opt(&format!("v{i}"), &format!("V{i}")))
            .collect();
        assert!(validate_field_specs(&[rb], FieldContext::Content).is_err());
    }

    #[test]
    fn icon_requires_valid_core_icon_key() {
        let mut ic = FieldSpec::new(FieldType::Icon, "ic", "Icon");
        ic.default = Some(Value::String("nonexistent-icon".to_owned()));
        assert!(validate_field_specs(std::slice::from_ref(&ic), FieldContext::Content).is_err());
        ic.default = Some(Value::String("star".to_owned()));
        assert!(validate_field_specs(&[ic], FieldContext::Content).is_ok());
    }

    #[test]
    fn date_min_max_bounds_checked_in_schema_and_values() {
        let mut d = FieldSpec::new(FieldType::Date, "ev", "Event");
        d.min_date = Some("2026-05-01".to_owned());
        d.max_date = Some("2026-04-01".to_owned());
        assert!(validate_field_specs(std::slice::from_ref(&d), FieldContext::Content).is_err());

        d.max_date = Some("2026-12-31".to_owned());
        assert!(validate_field_specs(std::slice::from_ref(&d), FieldContext::Content).is_ok());

        let mut vals = BTreeMap::new();
        vals.insert("ev".to_owned(), Value::String("2026-01-01".to_owned()));
        assert!(validate_field_values(&[d], FieldContext::Content, &vals, None).is_err());
    }

    #[test]
    fn spacing_rejected_in_content_term_and_owner_settings() {
        let sp = FieldSpec::new(FieldType::Spacing, "sp", "Spacing");
        assert!(validate_field_specs(std::slice::from_ref(&sp), FieldContext::Content).is_err());
        assert!(validate_field_specs(std::slice::from_ref(&sp), FieldContext::Term).is_err());
        assert!(
            validate_field_specs(std::slice::from_ref(&sp), FieldContext::OwnerSettings).is_err()
        );
        assert!(validate_field_specs(std::slice::from_ref(&sp), FieldContext::Options).is_ok());
        assert!(validate_field_specs(&[sp], FieldContext::StudioBlock).is_ok());
    }

    #[test]
    fn context_attribute_isolation_blocks_misplaced_flags() {
        let mut f_acl = FieldSpec::new(FieldType::Text, "a", "A");
        f_acl.acl = Some(FieldAcl::canonical(
            FieldRole::Editor,
            FieldRole::Editor,
            PublicExposure::Private,
        ));
        assert!(validate_field_specs(&[f_acl], FieldContext::Options).is_err());

        let mut f_pub = FieldSpec::new(FieldType::Text, "p", "P");
        f_pub.public = true;
        assert!(validate_field_specs(&[f_pub], FieldContext::Content).is_err());

        let mut f_trans = FieldSpec::new(FieldType::Text, "tr", "TR");
        f_trans.translatable = true;
        assert!(
            validate_field_specs(std::slice::from_ref(&f_trans), FieldContext::Content).is_err()
        );
        assert!(validate_field_specs(&[f_trans], FieldContext::Options).is_ok());

        let mut f_list = FieldSpec::new(FieldType::Richtext, "rt", "RT");
        f_list.show_in_list = true;
        assert!(validate_field_specs(&[f_list], FieldContext::Content).is_err());
    }

    #[test]
    fn studio_block_css_var_rules_and_uniqueness() {
        let mut a = FieldSpec::new(FieldType::Color, "c1", "Color 1");
        a.css_var = Some("--od-brand-bg".to_owned());
        let mut b = FieldSpec::new(FieldType::Color, "c2", "Color 2");
        b.css_var = Some("--od-brand-bg".to_owned());
        assert!(validate_field_specs(&[a.clone(), b], FieldContext::StudioBlock).is_err());

        let mut bad_media = FieldSpec::new(FieldType::Media, "vid", "Video");
        bad_media.media_kind = Some(MediaFieldKind::Video);
        bad_media.css_var = Some("--od-video-url".to_owned());
        assert!(validate_field_specs(&[bad_media], FieldContext::StudioBlock).is_err());

        let mut bad_sel = FieldSpec::new(FieldType::Select, "s", "Select");
        bad_sel.options = vec![opt("Bad Value!", "Bad")];
        bad_sel.css_var = Some("--od-sel".to_owned());
        assert!(validate_field_specs(&[bad_sel], FieldContext::StudioBlock).is_err());
    }

    #[test]
    fn responsive_flag_restricted_to_top_level_allowed_types() {
        let mut ok_n = FieldSpec::new(FieldType::Number, "cols", "Cols");
        ok_n.responsive = true;
        assert!(
            validate_field_specs(std::slice::from_ref(&ok_n), FieldContext::StudioBlock).is_ok()
        );

        let mut nested_g = FieldSpec::new(FieldType::Group, "g", "G");
        nested_g.fields = vec![ok_n];
        assert!(validate_field_specs(&[nested_g], FieldContext::StudioBlock).is_err());

        let mut bad_text = FieldSpec::new(FieldType::Text, "t", "T");
        bad_text.responsive = true;
        assert!(validate_field_specs(&[bad_text], FieldContext::StudioBlock).is_err());
    }

    #[test]
    fn responsive_value_validation_checks_desktop_tablet_mobile() {
        let mut cols = FieldSpec::new(FieldType::Number, "cols", "Cols");
        cols.responsive = true;
        cols.min = Some(1.0);
        cols.max = Some(6.0);
        let specs = vec![cols];

        let mut vals = BTreeMap::new();
        vals.insert(
            "cols".to_owned(),
            serde_json::json!({"desktop": 4, "mobile": 1}),
        );
        assert!(validate_field_values(&specs, FieldContext::StudioBlock, &vals, None).is_ok());

        vals.insert("cols".to_owned(), serde_json::json!({"desktop": 12}));
        assert!(validate_field_values(&specs, FieldContext::StudioBlock, &vals, None).is_err());

        vals.insert("cols".to_owned(), serde_json::json!({"ultrawide": 4}));
        assert!(validate_field_values(&specs, FieldContext::StudioBlock, &vals, None).is_err());
    }

    #[test]
    fn studio_block_field_count_and_repeater_budget_caps() {
        let too_many_top: Vec<FieldSpec> = (0..33)
            .map(|i| FieldSpec::new(FieldType::Text, format!("f_{i}"), format!("F{i}")))
            .collect();
        assert!(validate_field_specs(&too_many_top, FieldContext::StudioBlock).is_err());

        let mut r1 = FieldSpec::new(FieldType::Repeater, "r1", "R1");
        r1.max_items = Some(32);
        r1.fields = vec![FieldSpec::new(FieldType::Text, "a", "A")];
        let mut r2 = FieldSpec::new(FieldType::Repeater, "r2", "R2");
        r2.max_items = Some(32);
        r2.fields = vec![FieldSpec::new(FieldType::Text, "b", "B")];
        let mut r3 = FieldSpec::new(FieldType::Repeater, "r3", "R3");
        r3.max_items = Some(1);
        r3.fields = vec![FieldSpec::new(FieldType::Text, "c", "C")];
        assert!(validate_field_specs(&[r1.clone(), r2.clone()], FieldContext::StudioBlock).is_ok());
        assert!(validate_field_specs(&[r1, r2, r3], FieldContext::StudioBlock).is_err());
    }

    #[test]
    fn studio_block_leaf_field_cap_48_enforced_across_groups() {
        let mut groups = Vec::new();
        for g_idx in 0..3 {
            let mut g = FieldSpec::new(FieldType::Group, format!("g_{g_idx}"), format!("G{g_idx}"));
            g.fields = (0..17)
                .map(|i| FieldSpec::new(FieldType::Text, format!("leaf_{i}"), format!("L{i}")))
                .collect();
            groups.push(g);
        }
        // 3 * 17 = 51 leaves > 48
        assert!(validate_field_specs(&groups, FieldContext::StudioBlock).is_err());
    }

    #[test]
    fn non_block_schema_cap_64_enforced() {
        let too_many: Vec<FieldSpec> = (0..65)
            .map(|i| FieldSpec::new(FieldType::Text, format!("k_{i}"), format!("K{i}")))
            .collect();
        assert!(validate_field_specs(&too_many, FieldContext::Content).is_err());
    }

    #[test]
    fn dependency_dfs_detects_transitive_3_node_cycle() {
        let mut a = FieldSpec::new(FieldType::Text, "a", "A");
        let mut b = FieldSpec::new(FieldType::Text, "b", "B");
        let mut c = FieldSpec::new(FieldType::Text, "c", "C");
        a.conditions = Some(vec![vec![rule("b", ConditionOp::NotEmpty, None)]]);
        b.conditions = Some(vec![vec![rule("c", ConditionOp::NotEmpty, None)]]);
        c.conditions = Some(vec![vec![rule("a", ConditionOp::NotEmpty, None)]]);
        assert!(validate_field_specs(&[a, b, c], FieldContext::Options).is_err());
    }

    #[test]
    fn conditions_rejects_self_ref_group_spacing_secret_and_responsive_targets() {
        let mut self_f = FieldSpec::new(FieldType::Text, "s", "S");
        self_f.conditions = Some(vec![vec![rule("s", ConditionOp::NotEmpty, None)]]);
        assert!(validate_field_specs(&[self_f], FieldContext::Content).is_err());

        let mut sp = FieldSpec::new(FieldType::Spacing, "pad", "Pad");
        sp.key = "pad".to_owned();
        let mut dep = FieldSpec::new(FieldType::Text, "dep", "Dep");
        dep.conditions = Some(vec![vec![rule("pad", ConditionOp::Empty, None)]]);
        assert!(validate_field_specs(&[sp, dep], FieldContext::StudioBlock).is_err());
    }

    #[test]
    fn conditions_evaluate_all_8_operators() {
        let mut sel = FieldSpec::new(FieldType::Select, "mode", "Mode");
        sel.options = vec![opt("a", "A"), opt("b", "B"), opt("c", "C")];
        let num = FieldSpec::new(FieldType::Number, "count", "Count");
        let txt = FieldSpec::new(FieldType::Text, "note", "Note");
        let mut chk = FieldSpec::new(FieldType::Checkbox, "tags", "Tags");
        chk.options = vec![opt("x", "X"), opt("y", "Y")];
        let specs = vec![sel, num, txt, chk];

        let mut vals = BTreeMap::new();
        vals.insert("mode".to_owned(), Value::String("b".to_owned()));
        vals.insert("count".to_owned(), serde_json::json!(15));
        vals.insert("note".to_owned(), Value::String("hello world".to_owned()));
        vals.insert("tags".to_owned(), serde_json::json!(["x"]));

        assert!(evaluate_conditions(
            Some(&vec![vec![rule(
                "mode",
                ConditionOp::Eq,
                Some(Value::String("b".into()))
            )]]),
            &specs,
            &vals,
            &specs,
            &vals
        ));
        assert!(evaluate_conditions(
            Some(&vec![vec![rule(
                "mode",
                ConditionOp::Neq,
                Some(Value::String("a".into()))
            )]]),
            &specs,
            &vals,
            &specs,
            &vals
        ));
        assert!(evaluate_conditions(
            Some(&vec![vec![rule(
                "mode",
                ConditionOp::In,
                Some(serde_json::json!(["b", "c"]))
            )]]),
            &specs,
            &vals,
            &specs,
            &vals
        ));
        assert!(evaluate_conditions(
            Some(&vec![vec![rule(
                "tags",
                ConditionOp::In,
                Some(serde_json::json!(["x", "z"]))
            )]]),
            &specs,
            &vals,
            &specs,
            &vals
        ));
        assert!(evaluate_conditions(
            Some(&vec![vec![rule(
                "tags",
                ConditionOp::Contains,
                Some(Value::String("x".into()))
            )]]),
            &specs,
            &vals,
            &specs,
            &vals
        ));
        assert!(evaluate_conditions(
            Some(&vec![vec![rule("note", ConditionOp::NotEmpty, None)]]),
            &specs,
            &vals,
            &specs,
            &vals
        ));
        assert!(evaluate_conditions(
            Some(&vec![vec![rule(
                "count",
                ConditionOp::Gt,
                Some(serde_json::json!(10))
            )]]),
            &specs,
            &vals,
            &specs,
            &vals
        ));
        assert!(evaluate_conditions(
            Some(&vec![vec![rule(
                "count",
                ConditionOp::Lt,
                Some(serde_json::json!(20))
            )]]),
            &specs,
            &vals,
            &specs,
            &vals
        ));

        vals.insert("note".to_owned(), Value::String("   ".to_owned()));
        assert!(evaluate_conditions(
            Some(&vec![vec![rule("note", ConditionOp::Empty, None)]]),
            &specs,
            &vals,
            &specs,
            &vals
        ));
    }

    #[test]
    fn compat_blocks_flipping_multiple_and_deleting_options_with_data() {
        let mut s_single = FieldSpec::new(FieldType::Select, "cat", "Cat");
        s_single.options = vec![opt("a", "A"), opt("b", "B")];
        let mut s_multi = s_single.clone();
        s_multi.multiple = true;
        assert!(
            check_schema_compatibility(
                std::slice::from_ref(&s_single),
                std::slice::from_ref(&s_multi),
                true
            )
            .is_err()
        );

        let mut s_drop = s_single.clone();
        s_drop.options = vec![opt("a", "A")];
        assert!(check_schema_compatibility(&[s_single], &[s_drop], true).is_err());
    }

    #[test]
    fn compat_blocks_optional_to_required_and_new_required_with_data() {
        let f_opt = FieldSpec::new(FieldType::Text, "a", "A");
        let mut f_req = f_opt.clone();
        f_req.required = true;
        assert!(
            check_schema_compatibility(
                std::slice::from_ref(&f_opt),
                std::slice::from_ref(&f_req),
                true
            )
            .is_err()
        );
        assert!(check_schema_compatibility(&[], std::slice::from_ref(&f_req), true).is_err());

        // Withdrawn required is allowed (effective_required == false)
        f_req.declaration_state = Some(FieldDeclarationState::Withdrawn);
        assert!(check_schema_compatibility(&[f_opt], &[f_req], true).is_ok());
    }

    #[test]
    fn compat_allows_moving_fields_between_tabs_and_accordions() {
        let tab = FieldSpec::new(FieldType::Tab, "t1", "Tab 1");
        let f = FieldSpec::new(FieldType::Text, "title", "Title");
        assert!(check_schema_compatibility(&[f.clone()], &[tab, f], true).is_ok());
    }

    #[test]
    fn compat_recurses_into_nested_group_subfields() {
        let mut g1 = FieldSpec::new(FieldType::Group, "g", "G");
        g1.fields = vec![FieldSpec::new(FieldType::Text, "sub", "Sub")];
        let mut g2 = FieldSpec::new(FieldType::Group, "g", "G");
        g2.fields = vec![FieldSpec::new(FieldType::Number, "sub", "Sub")];
        assert!(check_schema_compatibility(&[g1], &[g2], true).is_err());
    }

    #[test]
    fn collect_media_recurses_through_repeater_and_nested_group() {
        let img1 = "c".repeat(64);
        let vid1 = "d".repeat(64);
        let mut m_img = FieldSpec::new(FieldType::Media, "thumb", "Thumb");
        m_img.media_kind = Some(MediaFieldKind::Image);
        let mut gal = FieldSpec::new(FieldType::Gallery, "vids", "Videos");
        gal.media_kind = Some(MediaFieldKind::Video);
        let mut rep = FieldSpec::new(FieldType::Repeater, "cards", "Cards");
        rep.fields = vec![m_img, gal];
        let specs = vec![rep];

        let mut vals = BTreeMap::new();
        vals.insert(
            "cards".to_owned(),
            serde_json::json!([{"thumb": img1, "vids": [vid1]}]),
        );
        let collected = collect_media_ids(&specs, &vals);
        assert!(collected.image_ids.contains(&img1));
        assert!(collected.video_ids.contains(&vid1));
    }

    #[test]
    fn collect_relational_refs_recurses_through_group_and_repeater() {
        let mut er = FieldSpec::new(FieldType::EntryRef, "rel", "Rel");
        er.target_types = vec!["post".to_owned()];
        let mut tr = FieldSpec::new(FieldType::TermRef, "cat", "Cat");
        tr.taxonomy = Some("category".to_owned());
        tr.multiple = true;
        let mut grp = FieldSpec::new(FieldType::Group, "meta", "Meta");
        grp.fields = vec![er];
        let mut rep = FieldSpec::new(FieldType::Repeater, "sections", "Sections");
        rep.fields = vec![tr];
        let specs = vec![grp, rep];

        let mut vals = BTreeMap::new();
        vals.insert("meta".to_owned(), serde_json::json!({"rel": 42}));
        vals.insert("sections".to_owned(), serde_json::json!([{"cat": [7, 9]}]));
        let refs = collect_relational_refs(&specs, &vals);
        assert_eq!(refs.entry_ids, BTreeSet::from([42]));
        assert_eq!(refs.term_ids, BTreeSet::from([7, 9]));
    }

    #[test]
    fn extract_visible_text_ignores_secrets_and_reads_links() {
        let s = FieldSpec::new(FieldType::Secret, "api_key", "API Key");
        let l = FieldSpec::new(FieldType::Link, "cta", "CTA");
        let mut rep = FieldSpec::new(FieldType::Repeater, "items", "Items");
        rep.fields = vec![FieldSpec::new(FieldType::Text, "t", "T")];
        let specs = vec![s, l, rep];

        let mut vals = BTreeMap::new();
        vals.insert(
            "api_key".to_owned(),
            Value::String("super-secret-token".to_owned()),
        );
        vals.insert(
            "cta".to_owned(),
            serde_json::json!({"url": "/go", "label": "Learn More"}),
        );
        vals.insert("items".to_owned(), serde_json::json!([{"t": "Row 1"}]));
        let mut out = Vec::new();
        extract_visible_text(&specs, &vals, &mut out);
        assert_eq!(out, vec!["Learn More".to_owned(), "Row 1".to_owned()]);
    }

    #[test]
    fn value_equals_previous_canonicalizes_negative_zero() {
        let a = serde_json::json!(-0.0);
        let b = serde_json::json!(0.0);
        assert!(value_equals_previous(Some(&a), Some(&b)));
        assert!(!value_equals_previous(Some(&a), None));
    }

    #[test]
    fn entry_ref_and_term_ref_reject_multiple_when_configured_single() {
        let mut er = FieldSpec::new(FieldType::EntryRef, "rel", "Rel");
        er.target_types = vec!["post".to_owned()];
        er.max_items = Some(3);
        assert!(validate_field_specs(&[er], FieldContext::Content).is_err());

        let mut tr = FieldSpec::new(FieldType::TermRef, "t", "T");
        tr.taxonomy = Some("category".to_owned());
        tr.min_items = Some(2);
        assert!(validate_field_specs(&[tr], FieldContext::Content).is_err());
    }

    #[test]
    fn entry_ref_and_term_ref_value_validation_checks_positive_unique_ids() {
        let mut er = FieldSpec::new(FieldType::EntryRef, "rel", "Rel");
        er.target_types = vec!["post".to_owned()];
        er.multiple = true;
        er.max_items = Some(2);
        let specs = vec![er];

        let mut vals = BTreeMap::new();
        vals.insert("rel".to_owned(), serde_json::json!([1, 1]));
        assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_err());

        vals.insert("rel".to_owned(), serde_json::json!([0]));
        assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_err());

        vals.insert("rel".to_owned(), serde_json::json!([1, 2]));
        assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_ok());
    }

    #[test]
    fn rejects_unknown_fields_in_submitted_values() {
        let specs = vec![FieldSpec::new(FieldType::Text, "title", "Title")];
        let mut vals = BTreeMap::new();
        vals.insert("injected".to_owned(), Value::String("bad".to_owned()));
        assert!(validate_field_values(&specs, FieldContext::Content, &vals, None).is_err());
    }

    #[test]
    fn default_value_for_field_spec_covers_containers_and_primitives() {
        let mut grp = FieldSpec::new(FieldType::Group, "g", "G");
        grp.fields = vec![FieldSpec::new(FieldType::Boolean, "flag", "Flag")];
        assert_eq!(
            default_value_for_field_spec(&grp),
            serde_json::json!({"flag": false})
        );

        let mut rng = FieldSpec::new(FieldType::Range, "r", "R");
        rng.min = Some(5.0);
        rng.max = Some(10.0);
        assert_eq!(default_value_for_field_spec(&rng), serde_json::json!(5.0));
    }
}
