use super::*;

pub const TW_PADDING_STEPS: &[i16] = &[0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48, 64];

pub const TW_MARGIN_STEPS: &[i16] = &[
    -32, -24, -16, -12, -8, -6, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32,
];

pub const TW_GAP_STEPS: &[u16] = &[0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16];

pub(super) fn px_to_tw_step(px: i16, steps: &[i16], allow_negative: bool) -> Option<i16> {
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

pub(super) fn fmt_tw_spacing(bp: &str, prop: &str, axis: &str, step: i16) -> String {
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

pub(super) fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Copy)]
pub(super) enum LayoutKind {
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

pub(super) fn validate_section_style(
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

pub(super) fn default_list_size() -> u8 {
    6
}

pub(super) fn default_true() -> bool {
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

pub(super) fn default_submit_label() -> String {
    "Gửi liên hệ".to_string()
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

#[derive(Debug)]
pub enum BuilderError {
    Json(serde_json::Error),
    Invalid(&'static str),
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(f, "Invalid Builder JSON: {error}"),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for BuilderError {}

impl From<serde_json::Error> for BuilderError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub(super) fn validate_known_with_schemas(
    element_id: &str,
    element: &KnownElement,
    catalog: &RegistryCatalog<RegisteredBlockSchema>,
    prev_plugin_values: &HashMap<(&str, &str), &BTreeMap<String, Value>>,
) -> Result<(), &'static str> {
    match element {
        KnownElement::Heading(props) => {
            if !(1..=6).contains(&props.level)
                || !valid_text(&props.text, 512, props.binding.is_none())
                || !valid_binding(props.binding.as_ref(), &[BindingTarget::Text])
            {
                return Err("Invalid Heading level or text");
            }
        }
        KnownElement::Text(props) => {
            if !valid_text(&props.text, 16_384, false)
                || !valid_binding(props.binding.as_ref(), &[BindingTarget::Text])
            {
                return Err("Invalid Text content");
            }
            if props.rich {
                crate::richtext::sanitize_richtext(&props.text)
                    .map_err(|_| "Invalid RichText HTML in core/text")?;
            }
        }
        KnownElement::Image(props) => {
            if !(valid_media_id(&props.media_id)
                || (props.media_id.is_empty()
                    && props
                        .binding
                        .as_ref()
                        .is_some_and(|b| b.target == BindingTarget::MediaId)))
                || !valid_text(&props.alt, 512, false)
                || !valid_binding(
                    props.binding.as_ref(),
                    &[BindingTarget::MediaId, BindingTarget::Alt],
                )
            {
                return Err("Invalid Image media ID or alt text");
            }
        }
        KnownElement::Button(props) => {
            if !valid_text(
                &props.label,
                200,
                !props
                    .binding
                    .as_ref()
                    .is_some_and(|b| b.target == BindingTarget::Label),
            ) || !(valid_button_url(&props.url)
                || (props.url.is_empty()
                    && props
                        .binding
                        .as_ref()
                        .is_some_and(|b| b.target == BindingTarget::Url)))
                || !valid_binding(
                    props.binding.as_ref(),
                    &[BindingTarget::Label, BindingTarget::Url],
                )
            {
                return Err("Invalid Button label or URL");
            }
        }
        KnownElement::Legacy(_) => {
            return Err("Legacy block descriptor unavailable");
        }
        KnownElement::PluginBlock(props) => {
            if !valid_namespaced_block_id(&props.block_id) {
                return Err("Invalid plugin block id");
            }
            if props.values.len() > MAX_NODE_VALUES_KEYS {
                return Err("Plugin block props exceed limit");
            }
            let entry = catalog
                .get(&props.block_id)
                .ok_or("Unknown plugin block descriptor")?;
            if entry.status != ProviderStatus::Active
                || entry.declaration_state != DeclarationState::Declared
            {
                return Err("Plugin block provider is not active");
            }
            let prev_values = prev_plugin_values
                .get(&(element_id, props.block_id.as_str()))
                .copied();
            entry
                .descriptor
                .schema
                .validate_values(&props.values, prev_values)?;
        }
        KnownElement::Video(props) => {
            if !valid_media_id(&props.media_id) {
                return Err("Invalid Video media ID");
            }
            if let Some(poster) = props.poster_image_id.as_deref()
                && !valid_media_id(poster)
            {
                return Err("Invalid Video poster image ID");
            }
        }
        KnownElement::ContactForm(props) => {
            let label = props.submit_label.trim();
            if props.form_id <= 0
                || label.is_empty()
                || props.submit_label.chars().count() > 80
                || props.submit_label.chars().any(char::is_control)
            {
                return Err("Invalid Builder contact form configuration");
            }
        }
        KnownElement::ContentSlot(_) => {}
        KnownElement::PatternRef(props) => {
            if props.pattern_id <= 0 {
                return Err("Invalid Pattern reference ID");
            }
        }
        KnownElement::TemplatePart(props) => {
            if !valid_kebab_segment_with_limit(&props.part_key, 64) {
                return Err("Invalid Builder template part key");
            }
        }
        KnownElement::List(props) => {
            if !valid_content_type_key(&props.type_key)
                || !(1..=20).contains(&props.page_size)
                || props.category_id.is_some_and(|id| id <= 0)
                || props.tag_id.is_some_and(|id| id <= 0)
                || props.term_id.is_some_and(|id| id <= 0)
                || props
                    .taxonomy_key
                    .as_deref()
                    .is_some_and(|key| !valid_content_type_key(key))
                || (props.taxonomy_key.is_some() != props.term_id.is_some())
                || (props.taxonomy_key.is_some()
                    && (props.category_id.is_some() || props.tag_id.is_some()))
                || (props.include_descendants
                    && props.category_id.is_none()
                    && props.taxonomy_key.is_none())
            {
                return Err("Invalid Builder list configuration");
            }
        }
    }
    Ok(())
}

pub(super) fn valid_binding(binding: Option<&ElementBinding>, targets: &[BindingTarget]) -> bool {
    let Some(binding) = binding else { return true };
    if !targets.contains(&binding.target) {
        return false;
    }
    if let Some(locale) = &binding.entry_locale
        && !crate::id::valid_lowercase_locale_code(locale)
    {
        return false;
    }
    let fixed_entry_valid = || {
        valid_content_type_key(&binding.type_key)
            && valid_kebab_segment_with_limit(&binding.entry_slug, 64)
    };
    match &binding.source {
        BindingSource::Title => {
            fixed_entry_valid()
                && !matches!(binding.target, BindingTarget::MediaId | BindingTarget::Url)
        }
        BindingSource::Field { key } => fixed_entry_valid() && valid_snake_key(key),
        BindingSource::RouteContext { field_key } => {
            let valid_key = matches!(field_key.as_str(), "title" | "slug" | "url" | "cover_url")
                || valid_snake_key(field_key);
            let target_ok = match field_key.as_str() {
                "title" | "slug" => {
                    !matches!(binding.target, BindingTarget::MediaId | BindingTarget::Url)
                }
                "url" => matches!(
                    binding.target,
                    BindingTarget::Url | BindingTarget::Text | BindingTarget::Label
                ),
                "cover_url" => matches!(
                    binding.target,
                    BindingTarget::Url | BindingTarget::MediaId | BindingTarget::Text
                ),
                _ => true,
            };
            let scope_ok = (binding.type_key.is_empty()
                || valid_content_type_key(&binding.type_key))
                && (binding.entry_slug.is_empty()
                    || valid_kebab_segment_with_limit(&binding.entry_slug, 64));
            valid_key && target_ok && scope_ok
        }
    }
}

pub(super) fn valid_media_id(value: &str) -> bool {
    // Matches the M3 media ID *syntax*; P02 checks actual existence and ACL.
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

pub(super) fn valid_text(value: &str, max_chars: usize, required: bool) -> bool {
    value.chars().count() <= max_chars
        && (!required || !value.trim().is_empty())
        && !value
            .chars()
            .any(|ch| ch == '\0' || (ch.is_control() && ch != '\n' && ch != '\t'))
}

/// URL policy from ADR-0007: local path or absolute HTTP(S), no credentials.
pub fn valid_button_url(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 2048
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
        || value
            .chars()
            .any(|ch| matches!(ch, '\\' | '<' | '>' | '"' | '\''))
    {
        return false;
    }
    if value.starts_with('/') {
        return !value.starts_with("//")
            && Url::parse(&format!("https://builder.invalid{value}")).is_ok();
    }
    Url::parse(value).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}
