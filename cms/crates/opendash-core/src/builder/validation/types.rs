use super::*;

pub const TW_PADDING_STEPS: &[i16] = &[0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48, 64];

pub const TW_MARGIN_STEPS: &[i16] = &[
    -32, -24, -16, -12, -8, -6, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32,
];

pub const TW_GAP_STEPS: &[u16] = &[0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16];

pub(crate) fn px_to_tw_step(px: i16, steps: &[i16], allow_negative: bool) -> Option<i16> {
    if (!allow_negative && px < 0) || px % 4 != 0 {
        return None;
    }
    let step = px / 4;
    if steps.contains(&step) {
        Some(step)
    } else {
        None
    }
}

pub fn gap_px_to_tw_step(gap_px: u8) -> Option<u16> {
    if gap_px % 4 != 0 {
        return None;
    }
    let step = u16::from(gap_px / 4);
    TW_GAP_STEPS.contains(&step).then_some(step)
}

pub(crate) fn fmt_tw_spacing(bp: &str, prop: &str, axis: &str, step: i16) -> String {
    if step < 0 {
        format!("{bp}-{prop}{axis}-{}", step.unsigned_abs())
    } else {
        format!("{bp}{prop}{axis}-{step}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Start,
    Center,
    End,
    Justify,
}

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Copy)]
pub(crate) enum LayoutKind {
    Section,
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionWidth {
    Full,
    Boxed,
    Wide,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_media_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_width: Option<SectionWidth>,
}

impl SectionStyle {
    pub fn is_empty(&self) -> bool {
        self.background_color.as_deref().is_none_or(str::is_empty)
            && self
                .background_media_id
                .as_deref()
                .is_none_or(str::is_empty)
            && self.content_width.is_none()
    }
}

pub fn empty_section_style_option(value: &Option<SectionStyle>) -> bool {
    value.as_ref().is_none_or(SectionStyle::is_empty)
}

pub(crate) fn validate_section_style(
    style: Option<&SectionStyle>,
    is_synthetic: bool,
) -> Result<(), BuilderError> {
    let Some(style) = style else {
        return Ok(());
    };
    if style.is_empty() {
        return Ok(());
    }
    if is_synthetic {
        return Err(BuilderError::Invalid(
            "Synthetic section cannot carry custom styling",
        ));
    }
    if let Some(color) = style.background_color.as_deref()
        && !color.is_empty()
        && !crate::field_schema::valid_color_value(color)
    {
        return Err(BuilderError::Invalid("Invalid Section background color"));
    }
    if let Some(media_id) = style.background_media_id.as_deref()
        && !media_id.is_empty()
        && !valid_media_id(media_id)
    {
        return Err(BuilderError::Invalid("Invalid Section background media ID"));
    }
    Ok(())
}

/// Known v1 elements have typed props. Unknown or malformed saved elements
/// retain their original JSON map, but are only accepted by validation when
/// proven to be unchanged from the trusted previous saved document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ElementContent {
    Known(KnownElement),
    Unresolved(BTreeMap<String, Value>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "props", deny_unknown_fields)]
pub enum KnownElement {
    #[serde(rename = "core/heading")]
    Heading(HeadingProps),
    #[serde(rename = "core/text")]
    Text(TextProps),
    #[serde(rename = "core/image")]
    Image(ImageProps),
    #[serde(rename = "core/video")]
    Video(VideoProps),
    #[serde(rename = "core/button")]
    Button(ButtonProps),
    #[serde(rename = "core/list")]
    List(ListProps),
    #[serde(rename = "core/contact_form")]
    ContactForm(ContactFormProps),
    #[serde(rename = "core/search_form")]
    SearchForm(SearchFormProps),
    #[serde(rename = "core/search_results")]
    SearchResults(SearchResultsProps),
    #[serde(rename = "core/content_slot")]
    ContentSlot(ContentSlotProps),
    #[serde(rename = "core/pattern_ref")]
    PatternRef(PatternRefProps),
    #[serde(rename = "core/template_part")]
    TemplatePart(TemplatePartProps),
    #[serde(rename = "legacy/block")]
    Legacy(LegacyBlockProps),
    #[serde(rename = "plugin/block")]
    PluginBlock(PluginBlockProps),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginBlockProps {
    pub block_id: String,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
}

/// Typed insertion point for the current entry's sanitized content.
/// Context-specific slot-count rules belong to the template override layer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentSlotProps {}

/// A one-hop reference to a private Pattern CPT entry. The server resolves
/// this ID from a trusted SQLite snapshot; client-provided markup is ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternRefProps {
    pub pattern_id: i64,
}

/// A named theme/template part resolved by the server in the render context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplatePartProps {
    pub part_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadingProps {
    pub level: u8,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<ElementBinding>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextProps {
    pub text: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub rich: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<ElementBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageProps {
    pub media_id: String,
    pub alt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<ElementBinding>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonProps {
    pub label: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub new_tab: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub nofollow: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<ElementBinding>,
}

/// Published-only collection reference; never embeds entry values in the AST.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListProps {
    pub type_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_id: Option<i64>,
    /// C2 generic taxonomy filter; both fields must be present together.
    /// Legacy category/tag aliases remain readable for existing documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taxonomy_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term_id: Option<i64>,
    #[serde(default)]
    pub include_descendants: bool,
    #[serde(default)]
    pub sort: ListSort,
    #[serde(default = "default_list_size")]
    pub page_size: u8,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListSort {
    #[default]
    IdDesc,
    IdAsc,
}

pub(crate) fn default_list_size() -> u8 {
    6
}

pub(crate) fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoProps {
    pub media_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster_image_id: Option<String>,
    #[serde(default = "default_true")]
    pub controls: bool,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactFormProps {
    pub form_id: i64,
    #[serde(default = "default_submit_label")]
    pub submit_label: String,
}

pub(crate) fn default_submit_label() -> String {
    "Gửi liên hệ".to_string()
}

pub(crate) fn default_search_submit_label() -> String {
    "Tìm kiếm".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchFormProps {
    #[serde(default)]
    pub placeholder: String,
    #[serde(default = "default_search_submit_label")]
    pub submit_label: String,
    #[serde(default)]
    pub show_type_filter: bool,
    #[serde(default)]
    pub preserve_type_filter: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchResultsProps {
    #[serde(default = "default_true")]
    pub show_facets: bool,
    #[serde(default = "default_true")]
    pub show_excerpt: bool,
    #[serde(default)]
    pub empty_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementBinding {
    #[serde(default)]
    pub type_key: String,
    #[serde(default)]
    pub entry_slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_locale: Option<String>,
    pub source: BindingSource,
    pub target: BindingTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BindingSource {
    Title,
    Field { key: String },
    RouteContext { field_key: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingTarget {
    Text,
    MediaId,
    Alt,
    Label,
    Url,
}

impl KnownElement {
    pub fn binding(&self) -> Option<&ElementBinding> {
        match self {
            Self::Heading(p) => p.binding.as_ref(),
            Self::Text(p) => p.binding.as_ref(),
            Self::Image(p) => p.binding.as_ref(),
            Self::Button(p) => p.binding.as_ref(),
            Self::Legacy(_)
            | Self::List(_)
            | Self::ContactForm(_)
            | Self::SearchForm(_)
            | Self::SearchResults(_)
            | Self::ContentSlot(_)
            | Self::PatternRef(_)
            | Self::TemplatePart(_)
            | Self::Video(_)
            | Self::PluginBlock(_) => None,
        }
    }
}

/// Preserves the exact legacy M3 block ID and field-value map for pre-existing
/// saved documents. Per Phase 2 Decision D3, new inserts or edits are rejected;
/// existing nodes survive only when unchanged at their exact tree path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyBlockProps {
    pub block_id: String,
    pub values: BTreeMap<String, String>,
}
