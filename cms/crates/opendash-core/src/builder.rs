//! Versioned, typed Builder document for M4-S01-T01.
//!
//! Storage, rendering, media permissions, and the M3 homepage projection are
//! implemented in later slices. Existing elements whose provider/descriptor is
//! unavailable can be round-tripped without letting clients introduce or edit
//! such elements.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::{
    block_schema::{
        MAX_NODE_VALUES_BYTES, MAX_NODE_VALUES_KEYS, RegisteredBlockSchema, upcast_legacy_catalog,
    },
    content::valid_content_type_key,
    id::{
        valid_kebab_segment_with_limit, valid_namespaced_block_id, valid_package_id,
        valid_snake_key,
    },
    registry::{DeclarationState, ProviderStatus, RegistryCatalog},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoxSpacing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<i16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<i16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<i16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<i16>,
}

impl BoxSpacing {
    pub fn is_empty(&self) -> bool {
        self.top.is_none() && self.right.is_none() && self.bottom.is_none() && self.left.is_none()
    }

    pub fn sides(&self) -> [Option<i16>; 4] {
        [self.top, self.right, self.bottom, self.left]
    }
}

pub const TW_PADDING_STEPS: &[i16] = &[0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48, 64];
pub const TW_MARGIN_STEPS: &[i16] = &[
    -32, -24, -16, -12, -8, -6, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32,
];
pub const TW_GAP_STEPS: &[u16] = &[0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16];

fn px_to_tw_step(px: i16, steps: &[i16], allow_negative: bool) -> Option<i16> {
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

fn fmt_tw_spacing(bp: &str, prop: &str, axis: &str, step: i16) -> String {
    if step < 0 {
        format!("{bp}-{prop}{axis}-{}", step.unsigned_abs())
    } else {
        format!("{bp}{prop}{axis}-{step}")
    }
}

/// Compiles desktop-first `BoxSpacing` cascades across Desktop / Tablet / Mobile into
/// minimal mobile-first Tailwind classes (`base`, `md:`, `lg:`).
/// Returns `None` if any specified pixel value does not align to the allowed Tailwind scale.
pub fn compile_box_spacing_tailwind(
    prop: &str,
    desktop: Option<BoxSpacing>,
    tablet: Option<BoxSpacing>,
    mobile: Option<BoxSpacing>,
    allow_negative: bool,
) -> Option<Vec<String>> {
    let d_raw = desktop.map(|b| b.sides()).unwrap_or([None; 4]);
    let t_raw = tablet.map(|b| b.sides()).unwrap_or([None; 4]);
    let m_raw = mobile.map(|b| b.sides()).unwrap_or([None; 4]);

    let d_eff: [Option<i16>; 4] = d_raw;
    let t_eff: [Option<i16>; 4] = std::array::from_fn(|i| t_raw[i].or(d_eff[i]));
    let m_eff: [Option<i16>; 4] = std::array::from_fn(|i| m_raw[i].or(t_eff[i]));

    let active = |i: usize| d_eff[i].is_some() || t_eff[i].is_some() || m_eff[i].is_some();
    if !(0..4).any(active) {
        return Some(Vec::new());
    }

    let steps = if allow_negative {
        TW_MARGIN_STEPS
    } else {
        TW_PADDING_STEPS
    };
    let normalize = |arr: [Option<i16>; 4]| -> Option<[Option<i16>; 4]> {
        let mut out = [None; 4];
        for i in 0..4 {
            if active(i) {
                let px = arr[i].unwrap_or(0);
                out[i] = Some(px_to_tw_step(px, steps, allow_negative)?);
            }
        }
        Some(out)
    };

    let m = normalize(m_eff)?;
    let t = normalize(t_eff)?;
    let d = normalize(d_eff)?;

    let mut classes = Vec::new();
    let emit_bp =
        |bp: &str, cur: [Option<i16>; 4], prev: Option<[Option<i16>; 4]>, out: &mut Vec<String>| {
            if let Some(p) = prev
                && cur == p
            {
                return;
            }
            if let (Some(v0), Some(v1), Some(v2), Some(v3)) = (cur[0], cur[1], cur[2], cur[3])
                && v0 == v1
                && v1 == v2
                && v2 == v3
            {
                out.push(fmt_tw_spacing(bp, prop, "", v0));
                return;
            }
            match prev {
                None => {
                    if cur[1].is_some() && cur[1] == cur[3] {
                        out.push(fmt_tw_spacing(bp, prop, "x", cur[1].unwrap()));
                    } else {
                        if let Some(vr) = cur[1] {
                            out.push(fmt_tw_spacing(bp, prop, "r", vr));
                        }
                        if let Some(vl) = cur[3] {
                            out.push(fmt_tw_spacing(bp, prop, "l", vl));
                        }
                    }
                    if cur[0].is_some() && cur[0] == cur[2] {
                        out.push(fmt_tw_spacing(bp, prop, "y", cur[0].unwrap()));
                    } else {
                        if let Some(vt) = cur[0] {
                            out.push(fmt_tw_spacing(bp, prop, "t", vt));
                        }
                        if let Some(vb) = cur[2] {
                            out.push(fmt_tw_spacing(bp, prop, "b", vb));
                        }
                    }
                }
                Some(p) => {
                    if cur[1] != p[1] || cur[3] != p[3] {
                        if cur[1] == cur[3]
                            && let Some(vx) = cur[1]
                        {
                            out.push(fmt_tw_spacing(bp, prop, "x", vx));
                        } else {
                            if cur[1] != p[1]
                                && let Some(vr) = cur[1]
                            {
                                out.push(fmt_tw_spacing(bp, prop, "r", vr));
                            }
                            if cur[3] != p[3]
                                && let Some(vl) = cur[3]
                            {
                                out.push(fmt_tw_spacing(bp, prop, "l", vl));
                            }
                        }
                    }
                    if cur[0] != p[0] || cur[2] != p[2] {
                        if cur[0] == cur[2]
                            && let Some(vy) = cur[0]
                        {
                            out.push(fmt_tw_spacing(bp, prop, "y", vy));
                        } else {
                            if cur[0] != p[0]
                                && let Some(vt) = cur[0]
                            {
                                out.push(fmt_tw_spacing(bp, prop, "t", vt));
                            }
                            if cur[2] != p[2]
                                && let Some(vb) = cur[2]
                            {
                                out.push(fmt_tw_spacing(bp, prop, "b", vb));
                            }
                        }
                    }
                }
            }
        };

    emit_bp("", m, None, &mut classes);
    emit_bp("md:", t, Some(m), &mut classes);
    emit_bp("lg:", d, Some(t), &mut classes);
    Some(classes)
}

fn empty_box_spacing_option(value: &Option<BoxSpacing>) -> bool {
    value.as_ref().is_none_or(BoxSpacing::is_empty)
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponsiveStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding_px: Option<u8>,
    #[serde(default, skip_serializing_if = "empty_box_spacing_option")]
    pub padding: Option<BoxSpacing>,
    #[serde(default, skip_serializing_if = "empty_box_spacing_option")]
    pub margin: Option<BoxSpacing>,
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
    Justify,
}

impl ResponsiveStyle {
    pub fn is_empty(&self) -> bool {
        self.padding_px.is_none()
            && empty_box_spacing_option(&self.padding)
            && empty_box_spacing_option(&self.margin)
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

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeVisibility {
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide_desktop: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide_tablet: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide_mobile: bool,
}

impl NodeVisibility {
    pub fn is_empty(&self) -> bool {
        !self.hide_desktop && !self.hide_tablet && !self.hide_mobile
    }
}

fn empty_visibility_option(value: &Option<NodeVisibility>) -> bool {
    value.as_ref().is_none_or(NodeVisibility::is_empty)
}

/// Optional common UX Builder metadata on any Section, Row, Column, or Element.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    #[serde(default, skip_serializing_if = "empty_visibility_option")]
    pub visibility: Option<NodeVisibility>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub synthetic: bool,
}

impl NodeMeta {
    pub fn is_empty(&self) -> bool {
        self.name.as_deref().is_none_or(str::is_empty)
            && self.html_id.as_deref().is_none_or(str::is_empty)
            && self.class_name.as_deref().is_none_or(str::is_empty)
            && empty_visibility_option(&self.visibility)
            && !self.synthetic
    }
}

pub fn empty_node_meta_option(value: &Option<NodeMeta>) -> bool {
    value.as_ref().is_none_or(NodeMeta::is_empty)
}

fn validate_node_meta(
    meta: Option<&NodeMeta>,
    has_responsive: bool,
    is_element: bool,
) -> Result<(), BuilderError> {
    let Some(meta) = meta else {
        return Ok(());
    };
    if is_element && meta.synthetic {
        return Err(BuilderError::Invalid("Element cannot be synthetic"));
    }
    if meta.synthetic
        && (has_responsive
            || meta.html_id.as_deref().is_some_and(|s| !s.is_empty())
            || meta.class_name.as_deref().is_some_and(|s| !s.is_empty())
            || !empty_visibility_option(&meta.visibility))
    {
        return Err(BuilderError::Invalid(
            "Synthetic wrapper cannot carry responsive or custom HTML attributes",
        ));
    }
    if let Some(name) = meta.name.as_deref()
        && !name.is_empty()
    {
        if name.trim().is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control)
        {
            return Err(BuilderError::Invalid("Invalid Builder node name"));
        }
    }
    if let Some(html_id) = meta.html_id.as_deref()
        && !html_id.is_empty()
    {
        let valid = html_id.len() <= 64
            && html_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
        if !valid {
            return Err(BuilderError::Invalid("Invalid Builder node HTML ID"));
        }
    }
    if let Some(class_name) = meta.class_name.as_deref()
        && !class_name.is_empty()
    {
        let valid = class_name.len() <= 256
            && class_name.bytes().all(|b| {
                b.is_ascii_alphanumeric()
                    || matches!(
                        b,
                        b' ' | b'-' | b'_' | b':' | b'/' | b'.' | b'[' | b']' | b'%' | b'#'
                    )
            });
        if !valid {
            return Err(BuilderError::Invalid("Invalid Builder node CSS class"));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum LayoutKind {
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

fn validate_section_style(
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
        && !crate::block_schema::valid_block_color(color)
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
        if let Some(pad) = style.padding {
            for side in pad.sides().into_iter().flatten() {
                if !(0..=256).contains(&side) {
                    return Err(BuilderError::Invalid(
                        "Builder responsive padding out of range",
                    ));
                }
            }
        }
        if let Some(mar) = style.margin {
            for side in mar.sides().into_iter().flatten() {
                if !(-128..=256).contains(&side) {
                    return Err(BuilderError::Invalid(
                        "Builder responsive margin out of range",
                    ));
                }
            }
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionNode {
    pub id: String,
    #[serde(default, skip_serializing_if = "empty_responsive_option")]
    pub responsive: Option<Responsive>,
    #[serde(default, skip_serializing_if = "empty_section_style_option")]
    pub style: Option<SectionStyle>,
    #[serde(default, skip_serializing_if = "empty_node_meta_option")]
    pub meta: Option<NodeMeta>,
    pub children: Vec<RowNode>,
}

impl SectionNode {
    pub fn is_synthetic(&self) -> bool {
        self.meta.as_ref().is_some_and(|m| m.synthetic)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowNode {
    pub id: String,
    #[serde(default, skip_serializing_if = "empty_responsive_option")]
    pub responsive: Option<Responsive>,
    #[serde(default, skip_serializing_if = "empty_node_meta_option")]
    pub meta: Option<NodeMeta>,
    pub children: Vec<ColumnNode>,
}

impl RowNode {
    pub fn is_synthetic(&self) -> bool {
        self.meta.as_ref().is_some_and(|m| m.synthetic)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnNode {
    pub id: String,
    #[serde(default, skip_serializing_if = "empty_responsive_option")]
    pub responsive: Option<Responsive>,
    #[serde(default, skip_serializing_if = "empty_node_meta_option")]
    pub meta: Option<NodeMeta>,
    pub children: Vec<ElementNode>,
}

impl ColumnNode {
    pub fn is_synthetic(&self) -> bool {
        self.meta.as_ref().is_some_and(|m| m.synthetic)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementNode {
    pub id: String,
    #[serde(default, skip_serializing_if = "empty_node_meta_option")]
    pub meta: Option<NodeMeta>,
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

fn default_list_size() -> u8 {
    6
}

fn default_true() -> bool {
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

fn default_submit_label() -> String {
    "Gửi liên hệ".to_string()
}

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
        let schemas = upcast_legacy_catalog(catalog);
        self.validate_for_save_with_schemas(&schemas, previous)
    }

    /// Validates this document against rich `RegisteredBlockSchema` declarations (`block.json` v1
    /// and upcast legacy `theme_blocks`).
    pub fn validate_for_save_with_schemas(
        &self,
        catalog: &RegistryCatalog<RegisteredBlockSchema>,
        previous: Option<&BuilderDocument>,
    ) -> Result<(), BuilderError> {
        if self.schema_version != BUILDER_SCHEMA_VERSION {
            return Err(BuilderError::Invalid("Unsupported Builder schema version"));
        }
        let prev_plugin_values = index_previous_plugin_values(previous);
        let mut ids = HashSet::new();
        let mut count = 0;
        let mut sources = HashSet::new();
        let mut lists = 0_usize;
        for (s, section) in self.sections.iter().enumerate() {
            check_node_id(&section.id, &mut ids, &mut count)?;
            validate_responsive(section.responsive.as_ref(), LayoutKind::Section)?;
            validate_section_style(section.style.as_ref(), section.is_synthetic())?;
            validate_node_meta(
                section.meta.as_ref(),
                section.responsive.as_ref().is_some_and(|r| !r.is_empty()),
                false,
            )?;
            for (r, row) in section.children.iter().enumerate() {
                check_node_id(&row.id, &mut ids, &mut count)?;
                validate_responsive(row.responsive.as_ref(), LayoutKind::Row)?;
                validate_node_meta(
                    row.meta.as_ref(),
                    row.responsive.as_ref().is_some_and(|r| !r.is_empty()),
                    false,
                )?;
                for (c, column) in row.children.iter().enumerate() {
                    check_node_id(&column.id, &mut ids, &mut count)?;
                    validate_responsive(column.responsive.as_ref(), LayoutKind::Column)?;
                    validate_node_meta(
                        column.meta.as_ref(),
                        column.responsive.as_ref().is_some_and(|r| !r.is_empty()),
                        false,
                    )?;
                    for (e, element) in column.children.iter().enumerate() {
                        check_node_id(&element.id, &mut ids, &mut count)?;
                        validate_node_meta(element.meta.as_ref(), false, true)?;
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
                                validate_known_with_schemas(
                                    &element.id,
                                    known,
                                    catalog,
                                    &prev_plugin_values,
                                )
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

fn index_previous_plugin_values<'a>(
    previous: Option<&'a BuilderDocument>,
) -> HashMap<(&'a str, &'a str), &'a BTreeMap<String, Value>> {
    let mut map = HashMap::new();
    let Some(prev) = previous else {
        return map;
    };
    for section in &prev.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if let ElementContent::Known(KnownElement::PluginBlock(props)) =
                        &element.content
                    {
                        map.insert(
                            (element.id.as_str(), props.block_id.as_str()),
                            &props.values,
                        );
                    }
                }
            }
        }
    }
    map
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

fn validate_known_with_schemas(
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
        KnownElement::Legacy(props) => {
            if !valid_namespaced_block_id(&props.block_id) {
                return Err("Invalid legacy block ID");
            }
            let entry = catalog
                .get(&props.block_id)
                .filter(|entry| {
                    entry.status == ProviderStatus::Active
                        && entry.declaration_state == DeclarationState::Declared
                })
                .ok_or("Legacy block descriptor unavailable")?;
            let legacy_fields: Fields = entry
                .descriptor
                .schema
                .fields
                .iter()
                .map(|field| (field.key().to_owned(), field.label().to_owned()))
                .collect();
            let block = Block {
                kind: props.block_id.clone(),
                values: props.values.clone(),
            };
            block
                .validate_against_fields(&legacy_fields)
                .map_err(|_| "Legacy field map does not match descriptor")?;
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
            if entry.descriptor.legacy_mode {
                let bytes = serde_json::to_vec(&props.values)
                    .map_err(|_| "Plugin block props invalid JSON")?;
                if bytes.len() > MAX_NODE_VALUES_BYTES {
                    return Err("Plugin block props exceed 64 KiB");
                }
            } else {
                let prev_values = prev_plugin_values
                    .get(&(element_id, props.block_id.as_str()))
                    .copied();
                entry
                    .descriptor
                    .schema
                    .validate_values(&props.values, prev_values)?;
            }
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
            meta: None,
            content: ElementContent::Known(content),
        }
    }

    fn fixture() -> BuilderDocument {
        BuilderDocument {
            schema_version: BUILDER_SCHEMA_VERSION,
            sections: vec![SectionNode {
                id: "section-1".into(),
                meta: None,
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "row-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "column-1".into(),
                        meta: None,
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
                                    rich: false,
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
                                    new_tab: false,
                                    nofollow: false,
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
    fn v3_template_composition_blocks_roundtrip_and_validate() {
        let mut document = fixture();
        let children = &mut document.sections[0].children[0].children[0].children;
        children.push(element(
            "content-slot-1",
            KnownElement::ContentSlot(ContentSlotProps {}),
        ));
        children.push(element(
            "template-part-1",
            KnownElement::TemplatePart(TemplatePartProps {
                part_key: "post-card".into(),
            }),
        ));
        document.validate_for_save(&registry(), None).unwrap();

        let encoded = serde_json::to_value(&document).unwrap();
        assert_eq!(
            encoded["sections"][0]["children"][0]["children"][0]["children"][5],
            json!({"id": "content-slot-1", "type": "core/content_slot", "props": {}})
        );
        assert_eq!(
            encoded["sections"][0]["children"][0]["children"][0]["children"][6],
            json!({"id": "template-part-1", "type": "core/template_part",
                   "props": {"part_key": "post-card"}})
        );
        let decoded = BuilderDocument::from_json(&document.to_json().unwrap()).unwrap();
        assert_eq!(decoded, document);
        decoded.validate_for_save(&registry(), None).unwrap();
    }

    #[test]
    fn template_part_rejects_invalid_key_and_unknown_properties() {
        let mut document = fixture();
        document.sections[0].children[0].children[0]
            .children
            .push(element(
                "template-part-1",
                KnownElement::TemplatePart(TemplatePartProps {
                    part_key: "header".into(),
                }),
            ));
        let valid = serde_json::to_value(&document).unwrap();
        let overly_long = "a".repeat(65);
        for bad_key in [
            "",
            "../header",
            "header:footer",
            "Header",
            overly_long.as_str(),
        ] {
            let mut changed = document.clone();
            let ElementContent::Known(KnownElement::TemplatePart(props)) =
                &mut changed.sections[0].children[0].children[0].children[5].content
            else {
                panic!("expected template part");
            };
            props.part_key = bad_key.into();
            assert!(
                changed.validate_for_save(&registry(), None).is_err(),
                "{bad_key}"
            );
        }
        let mut bad = valid.clone();
        bad["sections"][0]["children"][0]["children"][0]["children"][5]["props"]["extra"] =
            json!("forbidden");
        let decoded = BuilderDocument::from_json(bad.to_string().as_bytes()).unwrap();
        assert!(decoded.validate_for_save(&registry(), None).is_err());

        let mut content_slot = valid;
        content_slot["sections"][0]["children"][0]["children"][0]["children"][5] = json!({"id": "content-slot-1", "type": "core/content_slot",
                   "props": {"html": "<script>unsafe</script>"}});
        let decoded = BuilderDocument::from_json(content_slot.to_string().as_bytes()).unwrap();
        assert!(decoded.validate_for_save(&registry(), None).is_err());
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
                        ..Default::default()
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
                meta: None,
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: format!("row-{s}"),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: format!("column-{s}"),
                        meta: None,
                        responsive: None,
                        children: vec![element(
                            &format!("text-{s}"),
                            KnownElement::Text(TextProps {
                                text: "x".repeat(5000),
                                ..Default::default()
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
                ..Default::default()
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
                meta: None,
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
                    ..Default::default()
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
                    ..Default::default()
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
            json!({"text_align": "diagonal"}),
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
        assert_eq!(props.taxonomy_key, None);
        assert_eq!(props.term_id, None);
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
            json!({"taxonomy_key": "genre"}),
            json!({"term_id": 42}),
            json!({"taxonomy_key": "bad-key", "term_id": 42}),
            json!({"taxonomy_key": "genre", "term_id": 0}),
            json!({"taxonomy_key": "genre", "term_id": 42, "tag_id": 6}),
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

        let mut generic = original.clone();
        generic["sections"][0]["children"][0]["children"][0]["children"][0]["props"] = json!({"type_key": "post", "taxonomy_key": "genre", "term_id": 37,
                   "include_descendants": true});
        let generic = BuilderDocument::from_json(generic.to_string().as_bytes()).unwrap();
        generic.validate_for_save(&registry(), None).unwrap();
        let roundtrip = BuilderDocument::from_json(&generic.to_json().unwrap()).unwrap();
        assert_eq!(roundtrip, generic);

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
        assert!(
            doc.validate_route_context_for_role(Some("single"), None)
                .is_ok()
        );
        assert!(
            doc.validate_route_context_for_role(Some("page"), None)
                .is_ok()
        );
        assert!(
            doc.validate_route_context_for_role(Some("home"), None)
                .is_err()
        );
        assert!(
            doc.validate_route_context_for_role(Some("archive"), None)
                .is_err()
        );
        assert!(
            doc.validate_route_context_for_role(Some("single"), Some(&[]))
                .is_ok()
        );

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
        assert!(
            custom_doc
                .validate_route_context_for_role(Some("single"), Some(&["summary".to_string()]))
                .is_ok()
        );
        assert!(
            custom_doc
                .validate_route_context_for_role(Some("single"), Some(&["other".to_string()]))
                .is_err()
        );
    }

    #[test]
    fn validate_for_save_with_schemas_enforces_status_declaration_and_schema_evolution() {
        use crate::{
            block_schema::BlockSchemaV1,
            registry::{OwnedDescriptor, ResourceOwner},
        };

        let media_hex = "a".repeat(64);
        let schema_v1: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/faq",
            "title": "FAQ",
            "category": "interactive",
            "render": "render.html",
            "fields": [
                { "type": "text", "key": "heading", "label": "Heading", "required": true },
                { "type": "media", "key": "icon_img", "label": "Icon", "media_kind": "image" },
                {
                    "type": "repeater",
                    "key": "items",
                    "label": "Items",
                    "fields": [
                        { "type": "text", "key": "question", "label": "Question", "required": true }
                    ]
                }
            ]
        }))
        .unwrap();
        schema_v1.validate("core").unwrap();

        let doc_raw = json!({
            "schema_version": 1,
            "sections": [{
                "id": "sec-1",
                "children": [{
                    "id": "row-1",
                    "children": [
                        {
                            "id": "col-1",
                            "children": [{
                                "id": "faq-1",
                                "type": "plugin/block",
                                "props": {
                                    "block_id": "core/faq",
                                    "values": {
                                        "heading": "Hỏi đáp",
                                        "icon_img": media_hex,
                                        "items": [{ "question": "Câu 1" }]
                                    }
                                }
                            }]
                        },
                        {
                            "id": "col-2",
                            "children": []
                        }
                    ]
                }]
            }]
        });
        let doc_v1 = BuilderDocument::from_json(doc_raw.to_string().as_bytes()).unwrap();

        let mut catalog = RegistryCatalog::new();
        catalog
            .register(
                "core/faq",
                ResourceOwner::Core,
                RegisteredBlockSchema {
                    schema: schema_v1.clone(),
                    legacy_mode: false,
                },
            )
            .unwrap();
        doc_v1
            .validate_for_save_with_schemas(&catalog, None)
            .unwrap();

        // Reject unsafe URL / color / richtext in active schema
        let mut bad_doc = doc_v1.clone();
        let bad_schema: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/faq",
            "title": "FAQ",
            "category": "interactive",
            "render": "render.html",
            "fields": [
                { "type": "url", "key": "cta_url", "label": "URL" }
            ]
        }))
        .unwrap();
        catalog
            .redeclare(
                "core/faq",
                ResourceOwner::Core,
                ProviderStatus::Active,
                RegisteredBlockSchema {
                    schema: bad_schema,
                    legacy_mode: false,
                },
            )
            .unwrap();
        if let ElementContent::Known(KnownElement::PluginBlock(ref mut p)) =
            bad_doc.sections[0].children[0].children[0].children[0].content
        {
            p.values
                .insert("cta_url".into(), json!("javascript:alert(1)"));
        }
        assert!(
            bad_doc
                .validate_for_save_with_schemas(&catalog, None)
                .is_err()
        );

        // ProviderInactive & Withdrawn: preserved identically, rejected on edit/move/fresh insert
        let mut inactive_catalog = RegistryCatalog::new();
        inactive_catalog.insert_loaded(
            "core/faq",
            OwnedDescriptor {
                owner: ResourceOwner::Core,
                status: ProviderStatus::ProviderInactive,
                declaration_state: DeclarationState::Declared,
                descriptor: RegisteredBlockSchema {
                    schema: schema_v1.clone(),
                    legacy_mode: false,
                },
            },
        );
        doc_v1
            .validate_for_save_with_schemas(&inactive_catalog, Some(&doc_v1))
            .unwrap();
        assert!(
            doc_v1
                .validate_for_save_with_schemas(&inactive_catalog, None)
                .is_err()
        );

        let mut withdrawn_catalog = RegistryCatalog::new();
        withdrawn_catalog.insert_loaded(
            "core/faq",
            OwnedDescriptor {
                owner: ResourceOwner::Core,
                status: ProviderStatus::Active,
                declaration_state: DeclarationState::Withdrawn,
                descriptor: RegisteredBlockSchema {
                    schema: schema_v1,
                    legacy_mode: false,
                },
            },
        );
        doc_v1
            .validate_for_save_with_schemas(&withdrawn_catalog, Some(&doc_v1))
            .unwrap();
        assert!(
            doc_v1
                .validate_for_save_with_schemas(&withdrawn_catalog, None)
                .is_err()
        );

        // Schema evolution on Active block (new required top-level + repeater field) passes even when moved to col-2
        let schema_v2: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/faq",
            "title": "FAQ",
            "category": "interactive",
            "render": "render.html",
            "fields": [
                { "type": "text", "key": "heading", "label": "Heading", "required": true },
                { "type": "text", "key": "newly_required_subtitle", "label": "Subtitle", "required": true },
                {
                    "type": "repeater",
                    "key": "items",
                    "label": "Items",
                    "fields": [
                        { "type": "text", "key": "question", "label": "Question", "required": true },
                        { "type": "text", "key": "newly_required_answer", "label": "Answer", "required": true }
                    ]
                }
            ]
        }))
        .unwrap();
        let mut v2_catalog = RegistryCatalog::new();
        v2_catalog
            .register(
                "core/faq",
                ResourceOwner::Core,
                RegisteredBlockSchema {
                    schema: schema_v2,
                    legacy_mode: false,
                },
            )
            .unwrap();
        let mut moved_doc = doc_v1.clone();
        let node = moved_doc.sections[0].children[0].children[0]
            .children
            .remove(0);
        moved_doc.sections[0].children[0].children[1]
            .children
            .push(node);
        moved_doc
            .validate_for_save_with_schemas(&v2_catalog, Some(&doc_v1))
            .unwrap();
        assert!(
            moved_doc
                .validate_for_save_with_schemas(&v2_catalog, None)
                .is_err()
        );
    }

    #[test]
    fn section_style_box_spacing_rich_text_and_button_flags_validate() {
        let mut doc = fixture();
        doc.sections[0].style = Some(SectionStyle {
            background_color: Some("#f8fafc".into()),
            background_media_id: Some("b".repeat(64)),
            content_width: Some(SectionWidth::Boxed),
        });
        doc.sections[0].responsive = Some(Responsive {
            desktop: Some(ResponsiveStyle {
                padding: Some(BoxSpacing {
                    top: Some(48),
                    right: Some(16),
                    bottom: Some(48),
                    left: Some(16),
                }),
                margin: Some(BoxSpacing {
                    top: Some(-12),
                    right: None,
                    bottom: Some(24),
                    left: None,
                }),
                ..Default::default()
            }),
            ..Default::default()
        });
        doc.sections[0].children[0].children[0].responsive = Some(Responsive {
            desktop: Some(ResponsiveStyle {
                text_align: Some(TextAlign::Justify),
                ..Default::default()
            }),
            ..Default::default()
        });
        doc.sections[0].children[0].children[0].children[1].content =
            ElementContent::Known(KnownElement::Text(TextProps {
                text: "<p>Xin chào <strong>OpenDash</strong></p>".into(),
                rich: true,
                binding: None,
            }));
        doc.sections[0].children[0].children[0].children[3].content =
            ElementContent::Known(KnownElement::Button(ButtonProps {
                label: "Đọc thêm".into(),
                url: "https://opendash.vn".into(),
                new_tab: true,
                nofollow: true,
                binding: None,
            }));

        doc.validate_for_save(&registry(), None).unwrap();
        let encoded = doc.to_json().unwrap();
        let decoded = BuilderDocument::from_json(&encoded).unwrap();
        assert_eq!(decoded, doc);

        // Synthetic section cannot carry style
        let mut syn = doc.clone();
        syn.sections[0].meta = Some(NodeMeta {
            synthetic: true,
            ..Default::default()
        });
        assert!(syn.validate_for_save(&registry(), None).is_err());

        // Bad section background color fails
        let mut bad_color = doc.clone();
        bad_color.sections[0]
            .style
            .as_mut()
            .unwrap()
            .background_color = Some("expression(alert(1))".into());
        assert!(bad_color.validate_for_save(&registry(), None).is_err());
    }

    #[test]
    fn compiles_box_spacing_to_minimal_mobile_first_tailwind_classes() {
        let uniform = BoxSpacing {
            top: Some(32),
            right: Some(32),
            bottom: Some(32),
            left: Some(32),
        };
        assert_eq!(
            compile_box_spacing_tailwind("p", Some(uniform), None, None, false),
            Some(vec!["p-8".into()])
        );

        let d_xy = BoxSpacing {
            top: Some(48),
            right: Some(24),
            bottom: Some(48),
            left: Some(24),
        };
        let m_xy = BoxSpacing {
            top: Some(32),
            right: Some(16),
            bottom: Some(32),
            left: Some(16),
        };
        assert_eq!(
            compile_box_spacing_tailwind("p", Some(d_xy), None, Some(m_xy), false),
            Some(vec![
                "px-4".into(),
                "py-8".into(),
                "md:px-6".into(),
                "md:py-12".into(),
            ])
        );

        let mobile_only = BoxSpacing {
            top: Some(16),
            right: Some(16),
            bottom: Some(16),
            left: Some(16),
        };
        assert_eq!(
            compile_box_spacing_tailwind("p", None, None, Some(mobile_only), false),
            Some(vec!["p-4".into(), "md:p-0".into()])
        );

        let neg_margin = BoxSpacing {
            top: Some(-24),
            right: None,
            bottom: None,
            left: None,
        };
        assert_eq!(
            compile_box_spacing_tailwind("m", None, None, Some(neg_margin), true),
            Some(vec!["-mt-6".into(), "md:mt-0".into()])
        );

        let custom_px = BoxSpacing {
            top: Some(37),
            right: None,
            bottom: None,
            left: None,
        };
        assert_eq!(
            compile_box_spacing_tailwind("p", Some(custom_px), None, None, false),
            None
        );
    }
}
