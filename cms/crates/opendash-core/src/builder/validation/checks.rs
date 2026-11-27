use super::*;

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

pub(crate) fn validate_known_with_schemas(
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
        KnownElement::SearchForm(props) => {
            let label = props.submit_label.trim();
            if !valid_text(&props.placeholder, 120, false)
                || props.placeholder.chars().any(char::is_control)
                || label.is_empty()
                || props.submit_label.chars().count() > 64
                || props.submit_label.chars().any(char::is_control)
            {
                return Err("Invalid Builder search form configuration");
            }
        }
        KnownElement::SearchResults(props) => {
            if !valid_text(&props.empty_message, 240, false)
                || props.empty_message.chars().any(char::is_control)
            {
                return Err("Invalid Builder search results configuration");
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

pub(crate) fn valid_binding(binding: Option<&ElementBinding>, targets: &[BindingTarget]) -> bool {
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

pub(crate) fn valid_media_id(value: &str) -> bool {
    // Matches the M3 media ID *syntax*; P02 checks actual existence and ACL.
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

pub(crate) fn valid_text(value: &str, max_chars: usize, required: bool) -> bool {
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
