use super::*;
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
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Textarea => "textarea",
            Self::Number => "number",
            Self::Range => "range",
            Self::Email => "email",
            Self::Url => "url",
            Self::Secret => "secret",
            Self::Richtext => "richtext",
            Self::Media => "media",
            Self::Gallery => "gallery",
            Self::Link => "link",
            Self::Select => "select",
            Self::Checkbox => "checkbox",
            Self::RadioButtons => "radio_buttons",
            Self::Boolean => "boolean",
            Self::Color => "color",
            Self::Icon => "icon",
            Self::Date => "date",
            Self::Datetime => "datetime",
            Self::Time => "time",
            Self::Spacing => "spacing",
            Self::EntryRef => "entry_ref",
            Self::TermRef => "term_ref",
            Self::Group => "group",
            Self::Repeater => "repeater",
            Self::Tab => "tab",
            Self::Accordion => "accordion",
            Self::Message => "message",
        }
    }

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

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}
