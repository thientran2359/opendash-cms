//! Versioned, typed Builder document for M4-S01-T01.
//!
//! Storage, rendering, media permissions, and the M3 homepage projection are
//! implemented in later slices. Existing elements whose provider/descriptor is
//! unavailable can be round-tripped without letting clients introduce or edit
//! such elements.

use super::*;

pub(super) use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use serde_json::Value;

pub(super) use url::Url;

pub(super) use crate::{
    block_schema::{MAX_NODE_VALUES_KEYS, RegisteredBlockSchema},
    content::valid_content_type_key,
    id::{
        valid_kebab_segment_with_limit, valid_namespaced_block_id, valid_package_id,
        valid_snake_key,
    },
    registry::{DeclarationState, ProviderStatus, RegistryCatalog},
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

pub(super) fn empty_box_spacing_option(value: &Option<BoxSpacing>) -> bool {
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

pub(super) fn empty_style_option(value: &Option<ResponsiveStyle>) -> bool {
    value.as_ref().is_none_or(ResponsiveStyle::is_empty)
}

pub(super) fn empty_responsive_option(value: &Option<Responsive>) -> bool {
    value.as_ref().is_none_or(Responsive::is_empty)
}

pub(super) fn validate_responsive(
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

impl BuilderDocument {
    pub fn empty() -> Self {
        Self {
            schema_version: BUILDER_SCHEMA_VERSION,
            sections: Vec::new(),
        }
    }

    /// Reject oversized input before JSON decoding; call
    /// `validate_for_save_with_schemas` afterwards, as deserialization alone
    /// does not validate the tree.
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

    /// Validates this document against rich `RegisteredBlockSchema` declarations (`block.json` v1).
    /// `previous` MUST come from trusted persisted state, never the client.
    /// Unsupported or legacy payloads may survive only at their original position under
    /// the same ancestor IDs and with identical content, including the node ID.
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
                                    sources.insert((
                                        &binding.type_key,
                                        &binding.entry_slug,
                                        binding.entry_locale.as_deref(),
                                    ));
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

pub(super) fn check_node_id<'a>(
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
