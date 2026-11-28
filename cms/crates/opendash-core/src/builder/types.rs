pub(super) use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) use serde::{Deserialize, Serialize};
pub(super) use serde_json::Value;
pub(super) use url::Url;

pub(super) use crate::{
    block_schema::{RegisteredBlockSchema, matches_pattern_list},
    content::valid_content_type_key,
    id::{valid_kebab_segment_with_limit, valid_namespaced_block_id, valid_package_id},
    registry::{DeclarationState, ProviderStatus, RegistryCatalog},
};

pub const BUILDER_SCHEMA_VERSION: u32 = 1;
pub const MAX_BUILDER_NODES: usize = 512;
pub const MAX_BUILDER_DEPTH: usize = 8;
pub const MAX_BUILDER_JSON_BYTES: usize = 256 * 1024;
pub const MAX_BUILDER_LISTS_PER_REQUEST: usize = 8;
pub const MAX_BUILDER_LISTS: usize = MAX_BUILDER_LISTS_PER_REQUEST;
pub const MAX_BUILDER_PATTERN_INSTANCES: usize = 16;
pub const MAX_EXPANDED_BUILDER_NODES: usize = 1024;

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

/// Parses a positive `i64` ID from either a JSON integer or a numeric wire string
/// (matching `<select options_source>` wire representation).
pub fn parse_positive_i64_prop(val: Option<&Value>) -> Option<i64> {
    match val? {
        Value::Number(n) => n.as_i64().filter(|&id| id > 0),
        Value::String(s) => s.trim().parse::<i64>().ok().filter(|&id| id > 0),
        _ => None,
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
        super::TW_MARGIN_STEPS
    } else {
        super::TW_PADDING_STEPS
    };
    let normalize = |arr: [Option<i16>; 4]| -> Option<[Option<i16>; 4]> {
        let mut out = [None; 4];
        for i in 0..4 {
            if active(i) {
                let px = arr[i].unwrap_or(0);
                out[i] = Some(super::px_to_tw_step(px, steps, allow_negative)?);
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
                out.push(super::fmt_tw_spacing(bp, prop, "", v0));
                return;
            }
            match prev {
                None => {
                    if cur[1].is_some() && cur[1] == cur[3] {
                        out.push(super::fmt_tw_spacing(bp, prop, "x", cur[1].unwrap()));
                    } else {
                        if let Some(vr) = cur[1] {
                            out.push(super::fmt_tw_spacing(bp, prop, "r", vr));
                        }
                        if let Some(vl) = cur[3] {
                            out.push(super::fmt_tw_spacing(bp, prop, "l", vl));
                        }
                    }
                    if cur[0].is_some() && cur[0] == cur[2] {
                        out.push(super::fmt_tw_spacing(bp, prop, "y", cur[0].unwrap()));
                    } else {
                        if let Some(vt) = cur[0] {
                            out.push(super::fmt_tw_spacing(bp, prop, "t", vt));
                        }
                        if let Some(vb) = cur[2] {
                            out.push(super::fmt_tw_spacing(bp, prop, "b", vb));
                        }
                    }
                }
                Some(p) => {
                    if cur[1] != p[1] || cur[3] != p[3] {
                        if cur[1] == cur[3]
                            && let Some(vx) = cur[1]
                        {
                            out.push(super::fmt_tw_spacing(bp, prop, "x", vx));
                        } else {
                            if cur[1] != p[1]
                                && let Some(vr) = cur[1]
                            {
                                out.push(super::fmt_tw_spacing(bp, prop, "r", vr));
                            }
                            if cur[3] != p[3]
                                && let Some(vl) = cur[3]
                            {
                                out.push(super::fmt_tw_spacing(bp, prop, "l", vl));
                            }
                        }
                    }
                    if cur[0] != p[0] || cur[2] != p[2] {
                        if cur[0] == cur[2]
                            && let Some(vy) = cur[0]
                        {
                            out.push(super::fmt_tw_spacing(bp, prop, "y", vy));
                        } else {
                            if cur[0] != p[0]
                                && let Some(vt) = cur[0]
                            {
                                out.push(super::fmt_tw_spacing(bp, prop, "t", vt));
                            }
                            if cur[2] != p[2]
                                && let Some(vb) = cur[2]
                            {
                                out.push(super::fmt_tw_spacing(bp, prop, "b", vb));
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
