use super::*;

pub const MAX_TEXT_CHARS: usize = 2_000;

pub const MAX_TEXTAREA_CHARS: usize = 100_000;

pub const MIN_TEXTAREA_ROWS: u8 = 2;

pub const MAX_TEXTAREA_ROWS: u8 = 16;

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

pub(super) fn is_false(value: &bool) -> bool {
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

pub(super) fn parse_iso_date_parts(raw: &str) -> Option<(i64, i64, i64)> {
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

pub(super) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
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

pub(super) fn validate_optional_plain_text(
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

pub(super) fn is_non_empty_json_value(spec: &FieldSpec, val: &Value) -> bool {
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

pub(super) fn normalize_acl_recursive(
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

pub(super) fn build_scope_lookup_map(specs: &[FieldSpec]) -> BTreeMap<&str, &FieldSpec> {
    let mut map = BTreeMap::new();
    for spec in specs {
        map.entry(spec.key.as_str()).or_insert(spec);
    }
    map
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_scope_specs<'a>(
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

pub(super) fn lookup_condition_target<'a>(
    key: &str,
    sibling_map: &BTreeMap<&'a str, &'a FieldSpec>,
    top_map: &BTreeMap<&'a str, &'a FieldSpec>,
) -> Option<&'a FieldSpec> {
    sibling_map
        .get(key)
        .copied()
        .or_else(|| top_map.get(key).copied())
}

pub(super) fn validate_conditions<'a>(
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
pub(super) fn validate_dependency_cycles(specs: &[FieldSpec]) -> Result<(), &'static str> {
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

pub(super) fn find_spec_in_slice<'a>(specs: &'a [FieldSpec], key: &str) -> Option<&'a FieldSpec> {
    specs.iter().find(|s| s.key == key)
}

pub(super) fn resolve_effective_condition_value<'a>(
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

pub(super) fn effective_value_for_spec(spec: &FieldSpec, current: Option<&Value>) -> Value {
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

pub(super) fn is_json_value_empty(val: &Value) -> bool {
    match val {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(arr) => arr.is_empty(),
        Value::Object(obj) => obj.is_empty(),
        Value::Number(_) | Value::Bool(_) => false,
    }
}

pub(super) fn scalar_equal(a: &Value, b: &Value) -> bool {
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

pub(super) fn eval_single_condition(
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
pub(super) fn validate_map_against_specs(
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
pub(super) fn validate_field_value(
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

pub(super) fn check_compat4_recursive(specs: &[FieldSpec]) -> Result<(), &'static str> {
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

pub fn collect_media_ids(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
) -> CollectedMediaIds {
    let mut out = CollectedMediaIds::default();
    collect_media_recursive(specs, values, &mut out);
    out
}

pub(super) fn collect_media_recursive(
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

pub(super) fn collect_public_media_recursive(
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

pub(super) fn collect_relational_recursive(
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

pub(super) fn collect_options_source_values_recursive(
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
