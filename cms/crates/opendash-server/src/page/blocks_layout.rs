use std::collections::BTreeMap;

use opendash_core::{
    builder::{BlockNode, gap_px_to_tw_step},
    field_schema::{FieldSpec, SpacingValue},
};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct NativeParentInfo {
    pub layout_mode: Option<&'static str>,
    pub active_child_count: usize,
}

pub(super) fn str_prop<'a>(
    props: &'a BTreeMap<String, Value>,
    key: &str,
    default: &'a str,
) -> &'a str {
    props
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
}

fn bool_prop(props: &BTreeMap<String, Value>, key: &str, default: bool) -> bool {
    props.get(key).and_then(Value::as_bool).unwrap_or(default)
}

pub(super) fn responsive_str_cascade<'a>(
    props: &'a BTreeMap<String, Value>,
    key: &str,
    default: &'a str,
) -> (bool, &'a str, &'a str, &'a str) {
    match props.get(key) {
        Some(Value::Object(obj)) => {
            let d = obj
                .get("desktop")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(default);
            let t = obj
                .get("tablet")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(d);
            let m = obj
                .get("mobile")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(t);
            (true, d, t, m)
        }
        _ => {
            let v = str_prop(props, key, default);
            (false, v, v, v)
        }
    }
}

pub(super) fn responsive_i64_cascade(
    props: &BTreeMap<String, Value>,
    key: &str,
    default: i64,
) -> (bool, i64, i64, i64, bool) {
    match props.get(key) {
        Some(Value::Object(obj)) => {
            let d = obj
                .get("desktop")
                .and_then(Value::as_i64)
                .unwrap_or(default);
            let t = obj.get("tablet").and_then(Value::as_i64).unwrap_or(d);
            let has_m = obj.get("mobile").and_then(Value::as_i64).is_some();
            let m = obj.get("mobile").and_then(Value::as_i64).unwrap_or(t);
            (true, d, t, m, has_m)
        }
        Some(val) => {
            let v = val.as_i64().unwrap_or(default);
            (false, v, v, v, false)
        }
        None => (false, default, default, default, false),
    }
}

pub(super) fn spacing_classes_or_attr(
    fields: &[FieldSpec],
    props: &BTreeMap<String, Value>,
    key: &str,
) -> (Vec<String>, bool) {
    let Some(spec) = fields.iter().find(|f| f.key == key) else {
        return (Vec::new(), false);
    };
    let default_sv = spec
        .default
        .as_ref()
        .and_then(|v| serde_json::from_value::<SpacingValue>(v.clone()).ok());
    let Some(classes) = crate::template_sec::spacing_field_tailwind_classes(
        props.get(key),
        default_sv,
        spec.responsive,
        spec.effective_unit(),
        spec.effective_spacing_property(),
    ) else {
        return (Vec::new(), true);
    };
    (classes, false)
}

fn gap_classes_and_fallback(
    props: &BTreeMap<String, Value>,
    key: &str,
    default_px: i64,
) -> (Vec<String>, Option<i64>) {
    let (_, d_px, t_px, m_px, _) = responsive_i64_cascade(props, key, default_px);
    let to_step = |px: i64| u8::try_from(px).ok().and_then(gap_px_to_tw_step);
    if let (Some(gm), Some(gt), Some(gd)) = (to_step(m_px), to_step(t_px), to_step(d_px)) {
        let mut out = vec![format!("gap-{gm}")];
        if gt != gm {
            out.push(format!("md:gap-{gt}"));
        }
        if gd != gt {
            out.push(format!("lg:gap-{gd}"));
        }
        (out, None)
    } else {
        (Vec::new(), Some(d_px.max(0)))
    }
}

fn render_section(node: &BlockNode, fields: &[FieldSpec], child_nonce: &str) -> String {
    let width_cls = match str_prop(&node.props, "content_width", "boxed") {
        "full" => "max-w-none",
        "wide" => "max-w-screen-2xl mx-auto",
        _ => "max-w-7xl mx-auto",
    };
    let scheme_cls = match str_prop(&node.props, "text_scheme", "auto") {
        "light" => Some("text-white"),
        "dark" => Some("text-od-text"),
        _ => None,
    };
    let has_overlay = node
        .props
        .get("bg_overlay")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|s| !s.is_empty() && s != "transparent");
    let (pad_tw, pad_fb) = spacing_classes_or_attr(fields, &node.props, "padding");
    let (mar_tw, mar_fb) = spacing_classes_or_attr(fields, &node.props, "margin");

    let mut outer_cls = vec![
        "od-section".to_owned(),
        "relative".to_owned(),
        "w-full".to_owned(),
    ];
    if let Some(s) = scheme_cls {
        outer_cls.push(s.to_owned());
    }
    outer_cls.extend(mar_tw);

    let mut inner_cls = vec![
        "od-section-inner".to_owned(),
        "relative".to_owned(),
        "z-10".to_owned(),
        "w-full".to_owned(),
        width_cls.to_owned(),
    ];
    inner_cls.extend(pad_tw);

    let mar_attr = if mar_fb {
        " data-od-spacing=\"margin\""
    } else {
        ""
    };
    let pad_attr = if pad_fb {
        " data-od-spacing=\"padding\""
    } else {
        ""
    };
    let overlay_html = if has_overlay {
        "<div class=\"od-section-overlay pointer-events-none absolute inset-0\" style=\"background-color:var(--od-sec-overlay,transparent);\"></div>"
    } else {
        ""
    };
    format!(
        "<section class=\"{}\"{mar_attr} style=\"min-height:var(--od-sec-min-h,0px);background-color:var(--od-sec-bg,transparent);background-image:var(--od-sec-bg-img,none);background-size:cover;background-position:var(--od-sec-fx,50%) var(--od-sec-fy,50%);\">{overlay_html}<div class=\"{}\"{pad_attr}>{child_nonce}</div></section>",
        outer_cls.join(" "),
        inner_cls.join(" "),
    )
}

fn render_row(node: &BlockNode, fields: &[FieldSpec], child_nonce: &str) -> String {
    let layout_mode = str_prop(&node.props, "layout_mode", "equal");
    let reverse_mobile = bool_prop(&node.props, "reverse_mobile", false);
    let mut grid_cls: Vec<String> = vec![
        if reverse_mobile {
            "flex flex-col-reverse md:grid"
        } else {
            "grid"
        }
        .to_owned(),
    ];
    if layout_mode == "grid_12" {
        grid_cls.push(if reverse_mobile {
            "md:grid-cols-12".to_owned()
        } else {
            "grid-cols-12".to_owned()
        });
    } else {
        let (is_resp, d_raw, t_raw, m_raw, _) = responsive_i64_cascade(&node.props, "columns", 2);
        let (d, t, m) = (d_raw.clamp(1, 6), t_raw.clamp(1, 6), m_raw.clamp(1, 6));
        if !is_resp {
            if reverse_mobile {
                grid_cls.push(format!("md:grid-cols-{d}"));
            } else if d > 1 {
                grid_cls.push("grid-cols-1".to_owned());
                grid_cls.push(format!("md:grid-cols-{d}"));
            } else {
                grid_cls.push("grid-cols-1".to_owned());
            }
        } else {
            if !reverse_mobile {
                grid_cls.push(format!("grid-cols-{m}"));
            }
            if t != m || reverse_mobile {
                grid_cls.push(format!("md:grid-cols-{t}"));
            }
            if d != t {
                grid_cls.push(format!("lg:grid-cols-{d}"));
            }
        }
    }
    grid_cls.push(
        match str_prop(&node.props, "v_align", "stretch") {
            "start" => "items-start",
            "center" => "items-center",
            "end" => "items-end",
            _ => "items-stretch",
        }
        .to_owned(),
    );
    grid_cls.push(
        match str_prop(&node.props, "h_align", "start") {
            "center" => "justify-center",
            "end" => "justify-end",
            "between" => "justify-between",
            _ => "justify-start",
        }
        .to_owned(),
    );
    let (gap_cls, gap_fb) = gap_classes_and_fallback(&node.props, "gap", 24);
    grid_cls.extend(gap_cls);
    let (pad_tw, pad_fb) = spacing_classes_or_attr(fields, &node.props, "padding");
    let (mar_tw, mar_fb) = spacing_classes_or_attr(fields, &node.props, "margin");
    grid_cls.extend(pad_tw);

    let style_attr = gap_fb
        .map(|px| format!(" style=\"gap:{px}px;\""))
        .unwrap_or_default();
    let pad_attr = if pad_fb {
        " data-od-spacing=\"padding\""
    } else {
        ""
    };
    if mar_fb {
        format!(
            "<div class=\"od-row builder-row\" data-od-spacing=\"margin\"><div class=\"{}\"{pad_attr}{style_attr}>{child_nonce}</div></div>",
            grid_cls.join(" "),
        )
    } else {
        let mut all_cls = vec!["od-row".to_owned(), "builder-row".to_owned()];
        all_cls.extend(grid_cls);
        all_cls.extend(mar_tw);
        format!(
            "<div class=\"{}\"{pad_attr}{style_attr}>{child_nonce}</div>",
            all_cls.join(" "),
        )
    }
}

fn render_column(
    node: &BlockNode,
    fields: &[FieldSpec],
    child_nonce: &str,
    parent: NativeParentInfo,
) -> String {
    let mut cls = vec![
        "od-column".to_owned(),
        "builder-column".to_owned(),
        "flex".to_owned(),
        "flex-col".to_owned(),
    ];
    if parent.layout_mode == Some("grid_12") {
        let def_span = if parent.active_child_count > 0 && 12 % parent.active_child_count == 0 {
            12 / parent.active_child_count as i64
        } else {
            6
        };
        let (is_resp, d_r, t_r, m_r, has_m) = responsive_i64_cascade(&node.props, "span", def_span);
        let (d, t, m) = (d_r.clamp(1, 12), t_r.clamp(1, 12), m_r.clamp(1, 12));
        if !is_resp {
            if d < 12 {
                cls.push("col-span-12".to_owned());
                cls.push(format!("md:col-span-{d}"));
            } else {
                cls.push("col-span-12".to_owned());
            }
        } else {
            let m_eff = if has_m || d == 12 { m } else { 12 };
            cls.push(format!("col-span-{m_eff}"));
            if t != m_eff {
                cls.push(format!("md:col-span-{t}"));
            }
            if d != t {
                cls.push(format!("lg:col-span-{d}"));
            }
        }
    }
    if node.props.contains_key("text_align") {
        let (is_resp, d, t, m) = responsive_str_cascade(&node.props, "text_align", "start");
        let map_align = |s: &str| match s {
            "center" => "text-center",
            "end" => "text-right",
            "justify" => "text-justify",
            _ => "text-left",
        };
        let (mc, tc, dc) = (map_align(m), map_align(t), map_align(d));
        cls.push(mc.to_owned());
        if is_resp && tc != mc {
            cls.push(format!("md:{tc}"));
        }
        if is_resp && dc != tc {
            cls.push(format!("lg:{dc}"));
        }
    }
    cls.push(
        match str_prop(&node.props, "v_align", "start") {
            "center" => "justify-center",
            "end" => "justify-end",
            "stretch" => "justify-stretch",
            _ => "justify-start",
        }
        .to_owned(),
    );
    match str_prop(&node.props, "radius", "none") {
        "sm" => cls.push("rounded-sm".to_owned()),
        "md" => cls.push("rounded-md".to_owned()),
        "lg" => cls.push("rounded-lg".to_owned()),
        "xl" => cls.push("rounded-xl".to_owned()),
        "2xl" => cls.push("rounded-2xl".to_owned()),
        _ => {}
    }
    if bool_prop(&node.props, "sticky", false) {
        cls.extend(["sticky", "top-4", "self-start"].map(str::to_owned));
    }
    let has_bg = node
        .props
        .get("bg_color")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|s| !s.is_empty());
    let style_attr = if has_bg {
        " style=\"background-color:var(--od-col-bg);\""
    } else {
        ""
    };
    let (pad_tw, pad_fb) = spacing_classes_or_attr(fields, &node.props, "padding");
    let (mar_tw, mar_fb) = spacing_classes_or_attr(fields, &node.props, "margin");
    cls.extend(pad_tw);
    if mar_fb {
        let pad_attr = if pad_fb {
            " data-od-spacing=\"padding\""
        } else {
            ""
        };
        format!(
            "<div data-od-spacing=\"margin\"><div class=\"{}\"{pad_attr}{style_attr}>{child_nonce}</div></div>",
            cls.join(" "),
        )
    } else {
        cls.extend(mar_tw);
        let pad_attr = if pad_fb {
            " data-od-spacing=\"padding\""
        } else {
            ""
        };
        format!(
            "<div class=\"{}\"{pad_attr}{style_attr}>{child_nonce}</div>",
            cls.join(" "),
        )
    }
}

fn render_stack(node: &BlockNode, fields: &[FieldSpec], child_nonce: &str) -> String {
    let mut cls = vec!["od-stack".to_owned(), "flex".to_owned()];
    let (is_resp, d, t, m) = responsive_str_cascade(&node.props, "direction", "column");
    let map_dir = |s: &str| if s == "row" { "flex-row" } else { "flex-col" };
    let (mc, tc, dc) = (map_dir(m), map_dir(t), map_dir(d));
    cls.push(mc.to_owned());
    if is_resp && tc != mc {
        cls.push(format!("md:{tc}"));
    }
    if is_resp && dc != tc {
        cls.push(format!("lg:{dc}"));
    }
    if bool_prop(&node.props, "wrap", true) {
        cls.push("flex-wrap".to_owned());
    }
    cls.push(
        match str_prop(&node.props, "justify", "start") {
            "center" => "justify-center",
            "end" => "justify-end",
            "between" => "justify-between",
            _ => "justify-start",
        }
        .to_owned(),
    );
    cls.push(
        match str_prop(&node.props, "align", "stretch") {
            "start" => "items-start",
            "center" => "items-center",
            "end" => "items-end",
            _ => "items-stretch",
        }
        .to_owned(),
    );
    let (gap_cls, gap_fb) = gap_classes_and_fallback(&node.props, "gap", 16);
    cls.extend(gap_cls);
    let (pad_tw, pad_fb) = spacing_classes_or_attr(fields, &node.props, "padding");
    let (mar_tw, mar_fb) = spacing_classes_or_attr(fields, &node.props, "margin");
    cls.extend(pad_tw);
    let style_attr = gap_fb
        .map(|px| format!(" style=\"gap:{px}px;\""))
        .unwrap_or_default();
    if mar_fb {
        let pad_attr = if pad_fb {
            " data-od-spacing=\"padding\""
        } else {
            ""
        };
        format!(
            "<div data-od-spacing=\"margin\"><div class=\"{}\"{pad_attr}{style_attr}>{child_nonce}</div></div>",
            cls.join(" "),
        )
    } else {
        cls.extend(mar_tw);
        let pad_attr = if pad_fb {
            " data-od-spacing=\"padding\""
        } else {
            ""
        };
        format!(
            "<div class=\"{}\"{pad_attr}{style_attr}>{child_nonce}</div>",
            cls.join(" "),
        )
    }
}

pub(super) fn render_native_container(
    node: &BlockNode,
    fields: &[FieldSpec],
    child_nonce: &str,
    parent: NativeParentInfo,
) -> Option<String> {
    match node.block_id.as_str() {
        "core/section" => Some(render_section(node, fields, child_nonce)),
        "core/row" => Some(render_row(node, fields, child_nonce)),
        "core/column" => Some(render_column(node, fields, child_nonce, parent)),
        "core/stack" => Some(render_stack(node, fields, child_nonce)),
        _ => None,
    }
}
