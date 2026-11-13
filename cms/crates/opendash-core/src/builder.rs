//! Versioned, typed Builder document for M4-S01-T01.
//!
//! Storage, rendering, media permissions, and the M3 homepage projection are
//! implemented in later slices. Existing elements whose provider/descriptor is
//! unavailable can be round-tripped without letting clients introduce or edit
//! such elements.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::{
    content::valid_content_type_key,
    id::{
        valid_kebab_segment_with_limit, valid_namespaced_block_id, valid_package_id,
        valid_snake_key,
    },
    registry::{ProviderStatus, RegistryCatalog},
    theme::{Block, BlockDescriptor, Fields},
};

pub const BUILDER_SCHEMA_VERSION: u32 = 1;
pub const MAX_BUILDER_NODES: usize = 512;
pub const MAX_BUILDER_JSON_BYTES: usize = 256 * 1024;
pub const MAX_BUILDER_LISTS: usize = 8;

/// Sparse, viewport-specific layout metadata on existing v1 container nodes.
/// No theme CSS or arbitrary strings can enter the renderer from this payload.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Responsive {
    #[serde(default, skip_serializing_if = "empty_style_option")]
    pub desktop: Option<ResponsiveStyle>,
    #[serde(default, skip_serializing_if = "empty_style_option")]
    pub tablet: Option<ResponsiveStyle>,
    #[serde(default, skip_serializing_if = "empty_style_option")]
    pub mobile: Option<ResponsiveStyle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponsiveStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding_px: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap_px: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns_per_row: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_align: Option<TextAlign>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Start,
    Center,
    End,
}

impl ResponsiveStyle {
    pub fn is_empty(&self) -> bool {
        self.padding_px.is_none()
            && self.gap_px.is_none()
            && self.columns_per_row.is_none()
            && self.text_align.is_none()
    }
}

impl Responsive {
    pub fn is_empty(&self) -> bool {
        [&self.desktop, &self.tablet, &self.mobile]
            .iter()
            .all(|style| empty_style_option(style))
    }

    pub fn opts_into_grid(&self) -> bool {
        [&self.desktop, &self.tablet, &self.mobile]
            .into_iter()
            .flatten()
            .any(|style| style.gap_px.is_some() || style.columns_per_row.is_some())
    }
}

fn empty_style_option(value: &Option<ResponsiveStyle>) -> bool {
    value.as_ref().is_none_or(ResponsiveStyle::is_empty)
}

fn empty_responsive_option(value: &Option<Responsive>) -> bool {
    value.as_ref().is_none_or(Responsive::is_empty)
}

#[derive(Clone, Copy)]
enum LayoutKind {
    Section,
    Row,
    Column,
}

fn validate_responsive(
    responsive: Option<&Responsive>,
    kind: LayoutKind,
) -> Result<(), BuilderError> {
    let Some(responsive) = responsive else {
        return Ok(());
    };
    for style in [&responsive.desktop, &responsive.tablet, &responsive.mobile]
        .into_iter()
        .flatten()
    {
        if style.padding_px.is_some_and(|n| n > 64)
            || style.gap_px.is_some_and(|n| n > 64)
            || style.columns_per_row.is_some_and(|n| !(1..=6).contains(&n))
        {
            return Err(BuilderError::Invalid(
                "Builder responsive value out of range",
            ));
        }
        let wrong_context = match kind {
            LayoutKind::Section => {
                style.gap_px.is_some()
                    || style.columns_per_row.is_some()
                    || style.text_align.is_some()
            }
            LayoutKind::Row => style.text_align.is_some(),
            LayoutKind::Column => style.gap_px.is_some() || style.columns_per_row.is_some(),
        };
        if wrong_context {
            return Err(BuilderError::Invalid(
                "Builder responsive field not allowed on node",
            ));
        }
    }
    Ok(())
}

/// Children preserve render order, while IDs remain independent of position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderDocument {
    pub schema_version: u32,
    pub sections: Vec<SectionNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionNode {
    pub id: String,
    #[serde(default, skip_serializing_if = "empty_responsive_option")]
    pub responsive: Option<Responsive>,
    pub children: Vec<RowNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowNode {
    pub id: String,
    #[serde(default, skip_serializing_if = "empty_responsive_option")]
    pub responsive: Option<Responsive>,
    pub children: Vec<ColumnNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnNode {
    pub id: String,
    #[serde(default, skip_serializing_if = "empty_responsive_option")]
    pub responsive: Option<Responsive>,
    pub children: Vec<ElementNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementNode {
    pub id: String,
    #[serde(flatten)]
    pub content: ElementContent,
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
    #[serde(rename = "core/button")]
    Button(ButtonProps),
    #[serde(rename = "core/list")]
    List(ListProps),
    #[serde(rename = "legacy/block")]
    Legacy(LegacyBlockProps),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadingProps {
    pub level: u8,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<ElementBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextProps {
    pub text: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonProps {
    pub label: String,
    pub url: String,
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

fn default_list_size() -> u8 {
    6
}

/// An allowlisted, exact-entry field reference. No arbitrary queries or JSON paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementBinding {
    #[serde(default)]
    pub type_key: String,
    #[serde(default)]
    pub entry_slug: String,
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
            Self::Legacy(_) | Self::List(_) => None,
        }
    }
}

/// Preserves the exact M3 block ID and field-value map, including after a
/// theme switch. An active descriptor is required to add or change a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyBlockProps {
    pub block_id: String,
    pub values: Fields,
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

impl BuilderDocument {
    pub fn empty() -> Self {
        Self {
            schema_version: BUILDER_SCHEMA_VERSION,
            sections: Vec::new(),
        }
    }

    /// Reject oversized input before JSON decoding; call validate_for_save
    /// afterwards, as deserialization alone does not validate the tree.
    pub fn from_json(bytes: &[u8]) -> Result<Self, BuilderError> {
        if bytes.len() > MAX_BUILDER_JSON_BYTES {
            return Err(BuilderError::Invalid("Builder JSON exceeds 256 KiB"));
        }
        Ok(serde_json::from_slice(bytes)?)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, BuilderError> {
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_BUILDER_JSON_BYTES {
            return Err(BuilderError::Invalid("Builder JSON exceeds 256 KiB"));
        }
        Ok(bytes)
    }

    /// `previous` MUST come from trusted persisted state, never the client.
    /// Unsupported payloads may survive only at their original position under
    /// the same ancestor IDs and with identical content, including the node ID.
    pub fn validate_for_save(
        &self,
        catalog: &RegistryCatalog<BlockDescriptor>,
        previous: Option<&BuilderDocument>,
    ) -> Result<(), BuilderError> {
        if self.schema_version != BUILDER_SCHEMA_VERSION {
            return Err(BuilderError::Invalid("Unsupported Builder schema version"));
        }
        let mut ids = HashSet::new();
        let mut count = 0;
        let mut sources = HashSet::new();
        let mut lists = 0_usize;
        for (s, section) in self.sections.iter().enumerate() {
            check_node_id(&section.id, &mut ids, &mut count)?;
            validate_responsive(section.responsive.as_ref(), LayoutKind::Section)?;
            for (r, row) in section.children.iter().enumerate() {
                check_node_id(&row.id, &mut ids, &mut count)?;
                validate_responsive(row.responsive.as_ref(), LayoutKind::Row)?;
                for (c, column) in row.children.iter().enumerate() {
                    check_node_id(&column.id, &mut ids, &mut count)?;
                    validate_responsive(column.responsive.as_ref(), LayoutKind::Column)?;
                    for (e, element) in column.children.iter().enumerate() {
                        check_node_id(&element.id, &mut ids, &mut count)?;
                        let result = match &element.content {
                            ElementContent::Known(known) => {
                                if matches!(known, KnownElement::List(_)) {
                                    lists += 1;
                                    if lists > MAX_BUILDER_LISTS {
                                        return Err(BuilderError::Invalid(
                                            "Builder exceeds 8 list blocks",
                                        ));
                                    }
                                }
                                if let Some(binding) = known.binding() {
                                    sources.insert((&binding.type_key, &binding.entry_slug));
                                    if sources.len() > 32 {
                                        return Err(BuilderError::Invalid(
                                            "Too many Builder binding sources",
                                        ));
                                    }
                                }
                                validate_known(known, catalog)
                            }
                            ElementContent::Unresolved(_) => {
                                Err("Unknown or malformed Builder element")
                            }
                        };
                        if let Err(message) = result {
                            if !is_preserved(previous, (s, r, c, e), section, row, column, element)
                            {
                                return Err(BuilderError::Invalid(message));
                            }
                        }
                    }
                }
            }
        }
        // Enforce the encoded limit for Rust-constructed documents as well.
        self.to_json()?;
        Ok(())
    }
}

fn check_node_id<'a>(
    id: &'a str,
    ids: &mut HashSet<&'a str>,
    count: &mut usize,
) -> Result<(), BuilderError> {
    *count += 1;
    if *count > MAX_BUILDER_NODES {
        return Err(BuilderError::Invalid("Builder exceeds 512 nodes"));
    }
    if !valid_package_id(id) {
        return Err(BuilderError::Invalid("Invalid Builder node ID"));
    }
    if !ids.insert(id) {
        return Err(BuilderError::Invalid("Duplicate Builder node ID"));
    }
    Ok(())
}

fn is_preserved(
    previous: Option<&BuilderDocument>,
    (s, r, c, e): (usize, usize, usize, usize),
    section: &SectionNode,
    row: &RowNode,
    column: &ColumnNode,
    element: &ElementNode,
) -> bool {
    previous
        .and_then(|old| old.sections.get(s))
        .filter(|old| old.id == section.id)
        .and_then(|old| old.children.get(r))
        .filter(|old| old.id == row.id)
        .and_then(|old| old.children.get(c))
        .filter(|old| old.id == column.id)
        .and_then(|old| old.children.get(e))
        .is_some_and(|old| old == element)
}

fn validate_known(
    element: &KnownElement,
    catalog: &RegistryCatalog<BlockDescriptor>,
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
        KnownElement::Legacy(props) => {
            if !valid_namespaced_block_id(&props.block_id) {
                return Err("Invalid legacy block ID");
            }
            let descriptor = catalog
                .get(&props.block_id)
                .filter(|entry| entry.status == ProviderStatus::Active)
                .ok_or("Legacy block descriptor unavailable")?;
            let block = Block {
                kind: props.block_id.clone(),
                values: props.values.clone(),
            };
            block
                .validate_against_fields(&descriptor.descriptor.fields)
                .map_err(|_| "Legacy field map does not match descriptor")?;
        }
        KnownElement::List(props) => {
            if !valid_content_type_key(&props.type_key)
                || !(1..=20).contains(&props.page_size)
                || props.category_id.is_some_and(|id| id <= 0)
                || props.tag_id.is_some_and(|id| id <= 0)
                || (props.include_descendants && props.category_id.is_none())
            {
                return Err("Invalid Builder list configuration");
            }
        }
    }
    Ok(())
}

fn valid_binding(binding: Option<&ElementBinding>, targets: &[BindingTarget]) -> bool {
    let Some(binding) = binding else { return true };
    if !targets.contains(&binding.target) {
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

impl BuilderDocument {
    /// RouteContext bindings are valid only on `single` or `page` role overrides,
    /// and custom field keys must belong to the scoped CPT when provided.
    pub fn validate_route_context_for_role(
        &self,
        role: Option<&str>,
        scoped_cpt_fields: Option<&[String]>,
    ) -> Result<(), BuilderError> {
        let role_allows_route_context = matches!(role, Some("single") | Some("page"));
        for section in &self.sections {
            for row in &section.children {
                for column in &row.children {
                    for element in &column.children {
                        let ElementContent::Known(known) = &element.content else {
                            continue;
                        };
                        let Some(binding) = known.binding() else {
                            continue;
                        };
                        if let BindingSource::RouteContext { field_key } = &binding.source {
                            if !role_allows_route_context {
                                return Err(BuilderError::Invalid(
                                    "RouteContext binding is only valid on single/page role overrides",
                                ));
                            }
                            if !matches!(field_key.as_str(), "title" | "slug" | "url" | "cover_url")
                                && let Some(allowed) = scoped_cpt_fields
                                && !allowed.iter().any(|k| k == field_key)
                            {
                                return Err(BuilderError::Invalid(
                                    "RouteContext field_key is not in scoped CPT fields",
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn valid_media_id(value: &str) -> bool {
    // Matches the M3 media ID *syntax*; P02 checks actual existence and ACL.
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn valid_text(value: &str, max_chars: usize, required: bool) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ResourceOwner;
    use serde_json::json;

    fn registry() -> RegistryCatalog<BlockDescriptor> {
        let mut catalog = RegistryCatalog::new();
        catalog
            .register(
                "ocean/hero",
                ResourceOwner::Theme("ocean".into()),
                BlockDescriptor {
                    id: "ocean/hero".into(),
                    fields: Fields::from([("title".into(), "Tiêu đề".into())]),
                },
            )
            .unwrap();
        catalog
    }

    fn element(id: &str, content: KnownElement) -> ElementNode {
        ElementNode {
            id: id.into(),
            content: ElementContent::Known(content),
        }
    }

    fn fixture() -> BuilderDocument {
        BuilderDocument {
            schema_version: BUILDER_SCHEMA_VERSION,
            sections: vec![SectionNode {
                id: "section-1".into(),
                responsive: None,
                children: vec![RowNode {
                    id: "row-1".into(),
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "column-1".into(),
                        responsive: None,
                        children: vec![
                            element(
                                "heading-1",
                                KnownElement::Heading(HeadingProps {
                                    level: 2,
                                    text: "Ocean".into(),
                                    binding: None,
                                }),
                            ),
                            element(
                                "text-1",
                                KnownElement::Text(TextProps {
                                    text: "<script>plain text</script>".into(),
                                    binding: None,
                                }),
                            ),
                            element(
                                "image-1",
                                KnownElement::Image(ImageProps {
                                    media_id: "a".repeat(64),
                                    alt: "Anemone".into(),
                                    binding: None,
                                }),
                            ),
                            element(
                                "button-1",
                                KnownElement::Button(ButtonProps {
                                    label: "Explore".into(),
                                    url: "/species?order=latest".into(),
                                    binding: None,
                                }),
                            ),
                            element(
                                "legacy-1",
                                KnownElement::Legacy(LegacyBlockProps {
                                    block_id: "ocean/hero".into(),
                                    values: Fields::from([("title".into(), "Sea".into())]),
                                }),
                            ),
                        ],
                    }],
                }],
            }],
        }
    }

    #[test]
    fn round_trip_typed_elements_and_reorder_keep_node_ids() {
        let before = fixture();
        before.validate_for_save(&registry(), None).unwrap();
        let after = BuilderDocument::from_json(&before.to_json().unwrap()).unwrap();
        assert_eq!(after, before);
        let mut reordered = after;
        reordered.sections[0].children[0].children[0]
            .children
            .swap(0, 4);
        reordered
            .validate_for_save(&registry(), Some(&before))
            .unwrap();
        assert_eq!(
            reordered.sections[0].children[0].children[0].children[0].id,
            "legacy-1"
        );
    }

    #[test]
    fn rejects_unknown_schema_parent_shapes_duplicate_ids_and_limits() {
        let mut wrong_version = fixture();
        wrong_version.schema_version = 2;
        assert!(wrong_version.validate_for_save(&registry(), None).is_err());

        let mut duplicate = fixture();
        duplicate.sections[0].children[0].id = "section-1".into();
        assert!(duplicate.validate_for_save(&registry(), None).is_err());

        let mut invalid_id = fixture();
        invalid_id.sections[0].id = "../../etc".into();
        assert!(invalid_id.validate_for_save(&registry(), None).is_err());

        let wrong_nesting = json!({
            "schema_version": 1,
            "sections": [{"id": "section-1", "children": [{
                "id": "element-1", "type": "core/text", "props": {"text": "oops"}
            }]}]
        });
        assert!(BuilderDocument::from_json(wrong_nesting.to_string().as_bytes()).is_err());

        let mut too_many = fixture();
        too_many.sections[0].children[0].children[0].children = (0..510)
            .map(|index| {
                element(
                    &format!("node-{index}"),
                    KnownElement::Text(TextProps {
                        text: "x".into(),
                        binding: None,
                    }),
                )
            })
            .collect();
        assert!(too_many.validate_for_save(&registry(), None).is_err());
        assert!(BuilderDocument::from_json(&vec![b' '; MAX_BUILDER_JSON_BYTES + 1]).is_err());

        let mut oversized = fixture();
        oversized.sections = (0..100)
            .map(|s| SectionNode {
                id: format!("section-{s}"),
                responsive: None,
                children: vec![RowNode {
                    id: format!("row-{s}"),
                    responsive: None,
                    children: vec![ColumnNode {
                        id: format!("column-{s}"),
                        responsive: None,
                        children: vec![element(
                            &format!("text-{s}"),
                            KnownElement::Text(TextProps {
                                text: "x".repeat(5000),
                                binding: None,
                            }),
                        )],
                    }],
                }],
            })
            .collect();
        assert!(oversized.validate_for_save(&registry(), None).is_err());
    }

    #[test]
    fn typed_props_and_url_policy_reject_invalid_values() {
        let registry = registry();
        for invalid in [
            KnownElement::Heading(HeadingProps {
                level: 0,
                text: "Title".into(),
                binding: None,
            }),
            KnownElement::Heading(HeadingProps {
                level: 7,
                text: "Title".into(),
                binding: None,
            }),
            KnownElement::Image(ImageProps {
                media_id: "../file".into(),
                alt: "x".into(),
                binding: None,
            }),
            KnownElement::Button(ButtonProps {
                label: "Go".into(),
                url: "javascript:alert(1)".into(),
                binding: None,
            }),
        ] {
            let mut doc = fixture();
            doc.sections[0].children[0].children[0].children[0].content =
                ElementContent::Known(invalid);
            assert!(doc.validate_for_save(&registry, None).is_err());
        }
        for invalid in [
            "//evil.test",
            "https://user:pass@example.test",
            "data:text/html,hi",
            "http://x.test/a\\b",
            "https://x.test/space here",
            "#anchor",
            "ftp://host/path",
        ] {
            assert!(!valid_button_url(invalid), "{invalid}");
        }
        for allowed in [
            "/",
            "/page?q=1#anchor",
            "https://example.test/path",
            "http://localhost:8080/",
        ] {
            assert!(valid_button_url(allowed), "{allowed}");
        }
        let malformed = json!({
            "schema_version": 1,
            "sections": [{"id": "section-1", "children": [{"id": "row-1", "children": [{
                "id": "column-1", "children": [{"id": "heading-1", "type": "core/heading", "props": {"level": "not-a-number", "text": "x"}}]
            }]}]}]
        });
        let decoded = BuilderDocument::from_json(malformed.to_string().as_bytes()).unwrap();
        assert!(decoded.validate_for_save(&registry, None).is_err());
    }

    #[test]
    fn unavailable_legacy_and_unknown_saved_payload_preserved_only_unchanged() {
        let existing = fixture();
        let mut unavailable = registry();
        unavailable.deactivate_owner(&ResourceOwner::Theme("ocean".into()));
        assert!(
            existing
                .validate_for_save(&unavailable, Some(&existing))
                .is_ok()
        );
        assert!(existing.validate_for_save(&unavailable, None).is_err());

        let mut edited = existing.clone();
        let ElementContent::Known(KnownElement::Legacy(props)) =
            &mut edited.sections[0].children[0].children[0].children[4].content
        else {
            panic!()
        };
        props.values.insert("title".into(), "Changed".into());
        assert!(
            edited
                .validate_for_save(&unavailable, Some(&existing))
                .is_err()
        );

        let mut moved = existing.clone();
        moved.sections[0].children[0].children[0]
            .children
            .swap(3, 4);
        assert!(
            moved
                .validate_for_save(&unavailable, Some(&existing))
                .is_err()
        );

        let unknown = json!({"type": "theme/missing", "props": {"opaque": [1, {"a": true}]}, "original": "retained"});
        let mut persisted = fixture();
        persisted.sections[0].children[0].children[0]
            .children
            .push(ElementNode {
                id: "unknown-1".into(),
                content: ElementContent::Unresolved(serde_json::from_value(unknown).unwrap()),
            });
        assert!(persisted.validate_for_save(&registry(), None).is_err());
        let decoded = BuilderDocument::from_json(&persisted.to_json().unwrap()).unwrap();
        assert_eq!(decoded, persisted);
        assert!(
            decoded
                .validate_for_save(&registry(), Some(&persisted))
                .is_ok()
        );
        let mut updated = decoded;
        updated.sections[0].children[0].children[0].children[5].id = "unknown-2".into();
        assert!(
            updated
                .validate_for_save(&registry(), Some(&persisted))
                .is_err()
        );
    }

    #[test]
    fn validates_field_binding_protocol_and_distinct_source_cap() {
        let mut doc = fixture();
        let elements = &mut doc.sections[0].children[0].children[0].children;
        elements.clear();
        for index in 0..32 {
            elements.push(element(
                &format!("binding-{index}"),
                KnownElement::Text(TextProps {
                    text: String::new(),
                    binding: Some(ElementBinding {
                        type_key: "post".into(),
                        entry_slug: format!("post-{index}"),
                        source: BindingSource::Title,
                        target: BindingTarget::Text,
                    }),
                }),
            ));
        }
        doc.validate_for_save(&registry(), None).unwrap();
        assert_eq!(
            BuilderDocument::from_json(&doc.to_json().unwrap()).unwrap(),
            doc
        );
        let bound = &mut doc.sections[0].children[0].children[0].children[0].content;
        let ElementContent::Known(KnownElement::Text(props)) = bound else {
            panic!()
        };
        let binding = props.binding.as_mut().unwrap();
        binding.target = BindingTarget::Url;
        assert!(doc.validate_for_save(&registry(), None).is_err());
        let ElementContent::Known(KnownElement::Text(props)) =
            &mut doc.sections[0].children[0].children[0].children[0].content
        else {
            panic!()
        };
        props.binding.as_mut().unwrap().target = BindingTarget::Text;
        props.binding.as_mut().unwrap().entry_slug = "../broken".into();
        assert!(doc.validate_for_save(&registry(), None).is_err());
        let ElementContent::Known(KnownElement::Text(props)) =
            &mut doc.sections[0].children[0].children[0].children[0].content
        else {
            panic!()
        };
        props.binding.as_mut().unwrap().entry_slug = "post-0".into();
        doc.sections[0].children[0].children[0]
            .children
            .push(element(
                "binding-33",
                KnownElement::Text(TextProps {
                    text: String::new(),
                    binding: Some(ElementBinding {
                        type_key: "post".into(),
                        entry_slug: "extra".into(),
                        source: BindingSource::Title,
                        target: BindingTarget::Text,
                    }),
                }),
            ));
        assert!(doc.validate_for_save(&registry(), None).is_err());
    }

    #[test]
    fn legacy_descriptor_matches_m3_block_schema() {
        let mut invalid = fixture();
        let ElementContent::Known(KnownElement::Legacy(props)) =
            &mut invalid.sections[0].children[0].children[0].children[4].content
        else {
            panic!()
        };
        props.values.insert("unknown".into(), "value".into());
        assert!(invalid.validate_for_save(&registry(), None).is_err());
    }

    #[test]
    fn unknown_known_element_fields_cannot_be_silently_dropped() {
        let mut doc = serde_json::to_value(fixture()).unwrap();
        doc["sections"][0]["children"][0]["children"][0]["children"][0]["extra"] =
            json!({"must_keep": 3});
        let restored = BuilderDocument::from_json(doc.to_string().as_bytes()).unwrap();
        assert!(restored.validate_for_save(&registry(), None).is_err());
        // A historical node is losslessly preserved while the type is unavailable.
        let raw = restored.to_json().unwrap();
        assert!(String::from_utf8(raw).unwrap().contains("must_keep"));
    }

    #[test]
    fn responsive_sparse_zero_round_trip_and_existing_v1_compatibility() {
        let before = fixture();
        let old_json = before.to_json().unwrap();
        assert!(!String::from_utf8_lossy(&old_json).contains("responsive"));
        assert_eq!(BuilderDocument::from_json(&old_json).unwrap(), before);

        let mut current = before.clone();
        current.sections[0].responsive = Some(Responsive {
            desktop: Some(ResponsiveStyle {
                padding_px: Some(0),
                ..Default::default()
            }),
            ..Default::default()
        });
        let row = &mut current.sections[0].children[0];
        row.responsive = Some(Responsive {
            desktop: Some(ResponsiveStyle {
                gap_px: Some(0),
                columns_per_row: Some(3),
                ..Default::default()
            }),
            tablet: Some(ResponsiveStyle {
                columns_per_row: Some(2),
                ..Default::default()
            }),
            mobile: Some(ResponsiveStyle {
                columns_per_row: Some(1),
                ..Default::default()
            }),
        });
        current.sections[0].children[0].children[0].responsive = Some(Responsive {
            desktop: Some(ResponsiveStyle {
                text_align: Some(TextAlign::Start),
                ..Default::default()
            }),
            mobile: Some(ResponsiveStyle {
                text_align: Some(TextAlign::End),
                ..Default::default()
            }),
            ..Default::default()
        });
        current
            .validate_for_save(&registry(), Some(&before))
            .unwrap();
        assert!(
            current.sections[0].children[0]
                .responsive
                .as_ref()
                .unwrap()
                .opts_into_grid()
        );
        assert_eq!(
            BuilderDocument::from_json(&current.to_json().unwrap()).unwrap(),
            current
        );
        let mut blank = current.clone();
        blank.sections[0].responsive = Some(Responsive {
            desktop: Some(ResponsiveStyle::default()),
            ..Default::default()
        });
        let encoded = serde_json::to_value(&blank).unwrap();
        assert!(encoded["sections"][0].get("responsive").is_none());
    }

    #[test]
    fn responsive_rejects_wrong_types_contexts_and_limits() {
        let original = fixture();
        for (node, style) in [
            ("section", json!({"gap_px": 2})),
            ("section", json!({"text_align": "center"})),
            ("column", json!({"columns_per_row": 2})),
            ("row", json!({"text_align": "end"})),
            ("row", json!({"columns_per_row": 0})),
            ("row", json!({"columns_per_row": 7})),
            ("row", json!({"gap_px": 65})),
            ("column", json!({"padding_px": 65})),
        ] {
            let mut value = serde_json::to_value(&original).unwrap();
            let target = match node {
                "section" => &mut value["sections"][0],
                "row" => &mut value["sections"][0]["children"][0],
                _ => &mut value["sections"][0]["children"][0]["children"][0],
            };
            target["responsive"] = json!({"tablet": style});
            let doc = BuilderDocument::from_json(value.to_string().as_bytes()).unwrap();
            assert!(
                doc.validate_for_save(&registry(), Some(&original)).is_err(),
                "{node}: {style}"
            );
        }
        for bad in [
            json!({"padding_px": -1}),
            json!({"padding_px": 1.5}),
            json!({"padding_px": "10"}),
            json!({"padding_px": true}),
            json!({"text_align": "justify"}),
            json!({"gap_px": 0, "color": "red"}),
        ] {
            let mut value = serde_json::to_value(&original).unwrap();
            value["sections"][0]["children"][0]["responsive"] = json!({"mobile": bad});
            assert!(
                BuilderDocument::from_json(value.to_string().as_bytes()).is_err(),
                "{bad}"
            );
        }
        let mut invalid = serde_json::to_value(&original).unwrap();
        invalid["sections"][0]["responsive"] = json!({"phone": {"padding_px": 3}});
        assert!(BuilderDocument::from_json(invalid.to_string().as_bytes()).is_err());
    }

    #[test]
    fn list_defaults_limits_and_preserved_reference_contract() {
        let original = json!({
            "schema_version": 1,
            "sections": [{"id": "section-1", "children": [{
                "id": "row-1", "children": [{"id": "column-1", "children": [{
                    "id": "list-1", "type": "core/list", "props": {"type_key": "post"}
                }]}]
            }]}]
        });
        let document = BuilderDocument::from_json(original.to_string().as_bytes()).unwrap();
        document.validate_for_save(&registry(), None).unwrap();
        let ElementContent::Known(KnownElement::List(props)) =
            &document.sections[0].children[0].children[0].children[0].content
        else {
            panic!("valid List must decode into typed core/list")
        };
        assert_eq!(props.page_size, 6);
        assert_eq!(props.sort, ListSort::IdDesc);
        assert!(!props.include_descendants);
        assert_eq!(props.category_id, None);
        assert_eq!(
            BuilderDocument::from_json(&document.to_json().unwrap()).unwrap(),
            document
        );

        for bad in [
            json!({"page_size": 0}),
            json!({"page_size": 21}),
            json!({"page_size": "6"}),
            json!({"category_id": 0}),
            json!({"tag_id": -1}),
            json!({"include_descendants": true}),
            json!({"sort": "id_random"}),
            json!({"type_key": "invalid-key"}),
            json!({"extra": "injection"}),
        ] {
            let mut encoded = original.clone();
            for (key, value) in bad.as_object().unwrap() {
                encoded["sections"][0]["children"][0]["children"][0]["children"][0]["props"][key] =
                    value.clone();
            }
            let candidate = BuilderDocument::from_json(encoded.to_string().as_bytes()).unwrap();
            assert!(
                candidate.validate_for_save(&registry(), None).is_err(),
                "{bad}"
            );
        }
        // Correctly typed descendant filter with valid category and tag IDs.
        let mut filtered = original.clone();
        filtered["sections"][0]["children"][0]["children"][0]["children"][0]["props"] = json!({"type_key": "post", "category_id": 12, "tag_id": 22,
                   "include_descendants": true, "sort": "id_asc", "page_size": 20});
        let filtered = BuilderDocument::from_json(filtered.to_string().as_bytes()).unwrap();
        filtered.validate_for_save(&registry(), None).unwrap();

        let mut many = original.clone();
        let children = many["sections"][0]["children"][0]["children"][0]["children"]
            .as_array_mut()
            .unwrap();
        for index in 2..=8 {
            children.push(json!({"id": format!("list-{index}"), "type": "core/list",
                                 "props": {"type_key": "post", "page_size": 1}}));
        }
        // End the mutable borrow before serializing the document.
        let _ = children;
        BuilderDocument::from_json(many.to_string().as_bytes())
            .unwrap()
            .validate_for_save(&registry(), None)
            .unwrap();
        many["sections"][0]["children"][0]["children"][0]["children"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": "list-9", "type": "core/list", "props": {"type_key": "post"}}));
        assert!(
            BuilderDocument::from_json(many.to_string().as_bytes())
                .unwrap()
                .validate_for_save(&registry(), None)
                .is_err()
        );
    }

    #[test]
    fn route_context_allowed_only_on_single_or_page_and_checks_fields() {
        let raw = json!({
            "schema_version": 1,
            "sections": [{
                "id": "sec-1",
                "children": [{
                    "id": "row-1",
                    "children": [{
                        "id": "col-1",
                        "children": [{
                            "id": "heading-1",
                            "type": "core/heading",
                            "props": {
                                "text": "Fallback",
                                "level": 1,
                                "binding": {
                                    "target": "text",
                                    "source": { "kind": "route_context", "field_key": "title" }
                                }
                            }
                        }]
                    }]
                }]
            }]
        });
        let doc = BuilderDocument::from_json(raw.to_string().as_bytes()).unwrap();
        doc.validate_for_save(&registry(), None).unwrap();
        assert!(doc.validate_route_context_for_role(Some("single"), None).is_ok());
        assert!(doc.validate_route_context_for_role(Some("page"), None).is_ok());
        assert!(doc.validate_route_context_for_role(Some("home"), None).is_err());
        assert!(doc.validate_route_context_for_role(Some("archive"), None).is_err());
        assert!(doc.validate_route_context_for_role(Some("single"), Some(&[])).is_ok());

        let custom_field = json!({
            "schema_version": 1,
            "sections": [{
                "id": "sec-1",
                "children": [{
                    "id": "row-1",
                    "children": [{
                        "id": "col-1",
                        "children": [{
                            "id": "text-1",
                            "type": "core/text",
                            "props": {
                                "text": "Fallback",
                                "binding": {
                                    "target": "text",
                                    "source": { "kind": "route_context", "field_key": "summary" }
                                }
                            }
                        }]
                    }]
                }]
            }]
        });
        let custom_doc = BuilderDocument::from_json(custom_field.to_string().as_bytes()).unwrap();
        assert!(custom_doc.validate_route_context_for_role(Some("single"), Some(&["summary".to_string()])).is_ok());
        assert!(custom_doc.validate_route_context_for_role(Some("single"), Some(&["other".to_string()])).is_err());
    }
}
