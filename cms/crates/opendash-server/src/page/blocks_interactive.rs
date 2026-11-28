use std::collections::BTreeMap;

use opendash_core::{
    builder::BlockNode,
    field_schema::{FieldSpec, FieldType},
    icons::lookup_core_icon_paths,
    registry::{DeclarationState, ProviderStatus},
};
use serde_json::{Value, json};

use super::{
    blocks_chrome::bool_prop,
    blocks_layout::{
        NativeParentInfo, responsive_str_cascade, spacing_classes_or_attr, str_prop,
    },
    render::escape_html,
};
use crate::{
    content_render::core_l10n,
    error::{ApiError, ApiResult},
    storage::page::RegisteredBlockDef,
    template_sec::{RenderBudget, register_core_icon_in_budget},
};

pub(super) fn active_child_nodes<'a>(
    children: &'a [BlockNode],
    declared: &'a BTreeMap<String, RegisteredBlockDef>,
) -> Vec<(&'a BlockNode, &'a RegisteredBlockDef)> {
    children
        .iter()
        .filter_map(|child| {
            let def = declared.get(&child.block_id)?;
            (def.status == ProviderStatus::Active
                && def.declaration_state == DeclarationState::Declared)
                .then_some((child, def))
        })
        .collect()
}

pub(super) fn build_children_meta(active: &[(&BlockNode, &RegisteredBlockDef)]) -> Vec<Value> {
    active
        .iter()
        .enumerate()
        .map(|(idx, (child, def))| {
            let defaulted = def.schema.schema.apply_field_defaults(&child.props);
            let title = defaulted.get("title").and_then(Value::as_str).unwrap_or("");
            let open_by_default = defaulted
                .get("open_by_default")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let icon = defaulted
                .get("icon")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| lookup_core_icon_paths(name).is_some())
                .map(|name| json!({ "name": name }))
                .unwrap_or(Value::Null);
            json!({
                "id": child.id,
                "block_id": child.block_id,
                "index": idx,
                "title": title,
                "icon": icon,
                "open_by_default": open_by_default,
            })
        })
        .collect()
}

pub(super) fn scalar_parent_props(
    fields: &[FieldSpec],
    defaulted: &BTreeMap<String, Value>,
) -> Value {
    let mut map = serde_json::Map::new();
    for spec in fields {
        if spec.is_ui_only() {
            continue;
        }
        let allowed_scalar = matches!(
            spec.field_type,
            FieldType::Boolean
                | FieldType::Checkbox
                | FieldType::Text
                | FieldType::Textarea
                | FieldType::Number
                | FieldType::Range
                | FieldType::Email
                | FieldType::Select
                | FieldType::RadioButtons
                | FieldType::Color
                | FieldType::Date
                | FieldType::Datetime
                | FieldType::Time
        );
        if !allowed_scalar {
            continue;
        }
        if let Some(val) = defaulted.get(&spec.key)
            && matches!(val, Value::Bool(_) | Value::String(_) | Value::Number(_))
        {
            map.insert(spec.key.clone(), val.clone());
        }
    }
    Value::Object(map)
}

pub(super) fn compute_child_native_parent<'a>(
    parent_node: &BlockNode,
    defaulted_parent: &BTreeMap<String, Value>,
    active_children: &[(&BlockNode, &RegisteredBlockDef)],
    child: &BlockNode,
    child_idx_fallback: usize,
    node_scope: &'a str,
    grandparent: NativeParentInfo<'a>,
) -> NativeParentInfo<'a> {
    let active_count = active_children.len();
    let child_index = active_children
        .iter()
        .position(|(ac, _)| ac.id == child.id)
        .unwrap_or(child_idx_fallback);
    let layout_mode = if parent_node.block_id == "core/row" {
        Some(
            match defaulted_parent.get("layout_mode").and_then(Value::as_str) {
                Some("grid_12") => "grid_12",
                _ => "equal",
            },
        )
    } else {
        None
    };
    let initial_active_index = if parent_node.block_id == "core/tabs" && active_count > 0 {
        let def_tab = defaulted_parent
            .get("default_tab")
            .and_then(Value::as_i64)
            .unwrap_or(1)
            .max(1) as usize;
        def_tab.clamp(1, active_count) - 1
    } else {
        0
    };
    let in_deferred_media = grandparent.in_deferred_media
        || (parent_node.block_id == "core/slider" && child_index > 0)
        || parent_node.block_id == "core/popup";
    let in_popup = grandparent.in_popup || parent_node.block_id == "core/popup";

    NativeParentInfo {
        parent_node_scope: Some(node_scope),
        layout_mode,
        active_child_count: active_count,
        child_index,
        initial_active_index,
        in_deferred_media,
        in_popup,
    }
}

pub(super) fn wrap_margin_padding(
    wrap_cls: &str,
    mar_tw: Vec<String>,
    mar_fb: bool,
    inner_html: String,
) -> String {
    if mar_tw.is_empty() && !mar_fb {
        return inner_html;
    }
    let mut cls = vec![wrap_cls.to_owned()];
    cls.extend(mar_tw);
    let mar_attr = if mar_fb {
        " data-od-spacing=\"margin\""
    } else {
        ""
    };
    format!("<div class=\"{}\"{mar_attr}>{inner_html}</div>", cls.join(" "))
}

pub(super) fn pad_attr(pad_fb: bool) -> &'static str {
    if pad_fb {
        " data-od-spacing=\"padding\""
    } else {
        ""
    }
}

pub(super) fn render_tabs(
    node: &BlockNode,
    fields: &[FieldSpec],
    active_children: &[(&BlockNode, &RegisteredBlockDef)],
    child_nonce: &str,
    node_scope: &str,
    is_preview: bool,
    budget: &mut RenderBudget,
) -> ApiResult<String> {
    let l10n = core_l10n(&budget.locale);
    let active_count = active_children.len();
    if active_count == 0 {
        if is_preview {
            return Ok(format!(
                "<div class=\"od-tabs od-tabs-empty rounded border border-dashed border-od-border px-3 py-2 text-xs text-od-muted\" data-od-tabs-root>{}</div>",
                escape_html(&l10n.tabs_empty_preview),
            ));
        }
        return Ok(String::new());
    }
    if active_count >= 2 && !is_preview {
        budget.needs_blocks_js = true;
    }

    let style = match str_prop(&node.props, "style", "underline") {
        s @ ("pills" | "boxed") => s,
        _ => "underline",
    };
    let default_tab = node
        .props
        .get("default_tab")
        .and_then(Value::as_i64)
        .unwrap_or(1)
        .max(1) as usize;
    let initial_idx = default_tab.clamp(1, active_count) - 1;
    let aria_label = node
        .props
        .get("aria_label")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(l10n.tabs_default_label.as_ref());

    let list_cls = match style {
        "pills" => "od-tablist flex flex-nowrap gap-2 overflow-x-auto pb-2",
        "boxed" => {
            "od-tablist flex flex-nowrap gap-1 overflow-x-auto rounded-xl border border-od-border bg-od-surface-alt p-1"
        }
        _ => "od-tablist flex flex-nowrap overflow-x-auto border-b border-od-border",
    };

    let mut buttons = String::new();
    for (idx, (child, def)) in active_children.iter().enumerate() {
        let cprops = def.schema.schema.apply_field_defaults(&child.props);
        let title = cprops
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Tab");
        let icon_html = if let Some(icon_name) = cprops
            .get("icon")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|n| !n.is_empty() && lookup_core_icon_paths(n).is_some())
        {
            register_core_icon_in_budget(budget, icon_name, Some("w-4 h-4 shrink-0"))
                .map_err(ApiError::bad_request)?
                .unwrap_or_default()
        } else {
            String::new()
        };
        let is_active = idx == initial_idx;
        let selected = if is_active { "true" } else { "false" };
        let tabindex = if is_active { "0" } else { "-1" };
        let focusgroup_attr = if is_active { " focusgroupstart" } else { "" };
        buttons.push_str(&format!(
            "<button type=\"button\" role=\"tab\" id=\"od-tab-{node_scope}-{idx}\" class=\"od-tab min-h-[24px] shrink-0 relative inline-flex items-center gap-2 whitespace-nowrap px-4 py-2.5 text-sm font-medium\" aria-selected=\"{selected}\" tabindex=\"{tabindex}\" data-od-tab-index=\"{idx}\"{focusgroup_attr}>{icon_html}<span>{}</span></button>",
            escape_html(title),
        ));
    }

    let (pad_tw, pad_fb) = spacing_classes_or_attr(fields, &node.props, "padding");
    let (mar_tw, mar_fb) = spacing_classes_or_attr(fields, &node.props, "margin");
    let mut inner_cls = vec!["od-tabs".to_owned(), format!("od-tabs-{style}")];
    inner_cls.extend(pad_tw);
    let inner = format!(
        "<div class=\"{}\" data-od-tabs-root{}><div class=\"{list_cls}\" role=\"tablist\" focusgroup=\"tablist nomemory\" aria-label=\"{}\">{buttons}</div><div class=\"od-tab-panels\">{child_nonce}</div></div>",
        inner_cls.join(" "),
        pad_attr(pad_fb),
        escape_html(aria_label),
    );
    Ok(wrap_margin_padding("od-tabs-wrap", mar_tw, mar_fb, inner))
}

pub(super) fn render_tab_panel(
    node: &BlockNode,
    fields: &[FieldSpec],
    child_nonce: &str,
    parent: NativeParentInfo<'_>,
) -> String {
    let parent_scope = parent.parent_node_scope.unwrap_or("root");
    let idx = parent.child_index;
    let state_attr = if idx == parent.initial_active_index {
        " tabindex=\"0\""
    } else {
        " hidden=\"until-found\""
    };
    let (pad_tw, pad_fb) = spacing_classes_or_attr(fields, &node.props, "padding");
    let mut inner_cls = vec!["od-tab-panel-inner".to_owned()];
    inner_cls.extend(pad_tw);
    format!(
        "<div role=\"tabpanel\" class=\"od-tab-panel\" aria-labelledby=\"od-tab-{parent_scope}-{idx}\" data-od-panel-index=\"{idx}\"{state_attr}><div class=\"{}\"{}>{child_nonce}</div></div>",
        inner_cls.join(" "),
        pad_attr(pad_fb),
    )
}

pub(super) fn render_slider(
    node: &BlockNode,
    fields: &[FieldSpec],
    active_children: &[(&BlockNode, &RegisteredBlockDef)],
    child_nonce: &str,
    is_preview: bool,
    budget: &mut RenderBudget,
) -> ApiResult<String> {
    let l10n = core_l10n(&budget.locale);
    let active_count = active_children.len();
    if active_count == 0 {
        if is_preview {
            return Ok(format!(
                "<div class=\"od-slider od-slider-empty rounded border border-dashed border-od-border px-3 py-2 text-xs text-od-muted\" data-od-slider-root>{}</div>",
                escape_html(&l10n.slider_empty_preview),
            ));
        }
        return Ok(String::new());
    }
    if active_count >= 2 && !is_preview {
        budget.needs_blocks_js = true;
    }

    let clamp_spv = |s: &str| match s {
        "2" => "2",
        "3" => "3",
        _ => "1",
    };
    let (is_resp, d_raw, t_raw, m_raw) =
        responsive_str_cascade(&node.props, "slides_per_view", "1");
    let (d, t, m) = (clamp_spv(d_raw), clamp_spv(t_raw), clamp_spv(m_raw));
    let single_spv = d == "1" && t == "1" && m == "1";

    let mut cls = vec![
        "od-slider".to_owned(),
        "relative".to_owned(),
        format!("od-slider-cols-{m}"),
    ];
    if is_resp && t != m {
        cls.push(format!("md:od-slider-cols-{t}"));
    }
    if is_resp && d != t {
        cls.push(format!("lg:od-slider-cols-{d}"));
    }
    let (mar_tw, mar_fb) = spacing_classes_or_attr(fields, &node.props, "margin");
    cls.extend(mar_tw);

    let loop_on = bool_prop(&node.props, "loop", true);
    let show_arrows = bool_prop(&node.props, "show_arrows", true);
    let show_dots = bool_prop(&node.props, "show_dots", true);
    let autoplay = node
        .props
        .get("autoplay_seconds")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .clamp(0, 12);
    let aria_label = node
        .props
        .get("aria_label")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(l10n.slider_default_label.as_ref());

    let controls_html = if show_arrows && active_count >= 2 {
        let prev_ic = register_core_icon_in_budget(budget, "chevron-left", Some("w-5 h-5"))
            .map_err(ApiError::bad_request)?
            .unwrap_or_default();
        let next_ic = register_core_icon_in_budget(budget, "chevron-right", Some("w-5 h-5"))
            .map_err(ApiError::bad_request)?
            .unwrap_or_default();
        format!(
            "<div class=\"od-slider-controls pointer-events-none\"><button type=\"button\" class=\"od-slider-btn pointer-events-auto\" data-od-slide-dir=\"prev\" aria-label=\"{}\">{prev_ic}</button><button type=\"button\" class=\"od-slider-btn pointer-events-auto\" data-od-slide-dir=\"next\" aria-label=\"{}\">{next_ic}</button></div>",
            escape_html(&l10n.slider_prev),
            escape_html(&l10n.slider_next),
        )
    } else {
        String::new()
    };

    let dots_html = if show_dots && active_count >= 2 {
        let mut dots = String::new();
        for idx in 0..active_count {
            let cur = if idx == 0 { " aria-current=\"true\"" } else { "" };
            dots.push_str(&format!(
                "<button type=\"button\" class=\"od-slider-dot\" data-od-slide-to=\"{idx}\" aria-label=\"{} {}\"{cur}></button>",
                escape_html(&l10n.slider_dot_item),
                idx + 1,
            ));
        }
        format!(
            "<nav class=\"od-slider-dots flex items-center justify-center gap-2 mt-4\" aria-label=\"{}\">{dots}</nav>",
            escape_html(&l10n.slider_dots_label),
        )
    } else {
        String::new()
    };

    let toggle_html = if autoplay > 0 && active_count >= 2 {
        format!(
            "<button type=\"button\" class=\"od-slider-autoplay-toggle\" data-od-slider-toggle aria-pressed=\"false\" aria-label=\"{}\"><span class=\"od-pause-glyph\" aria-hidden=\"true\">\u{275A}\u{275A}</span><span class=\"od-play-glyph hidden\" aria-hidden=\"true\">\u{25B6}</span></button>",
            escape_html(&l10n.slider_pause),
        )
    } else {
        String::new()
    };

    let mar_attr = if mar_fb {
        " data-od-spacing=\"margin\""
    } else {
        ""
    };
    Ok(format!(
        "<section class=\"{}\" data-od-slider-root data-od-slider-spv=\"{}\" data-od-slider-loop=\"{}\" data-od-slider-autoplay=\"{autoplay}\" data-od-slider-count=\"{active_count}\" data-od-label-slide=\"{}\" data-od-label-pause=\"{}\" data-od-label-play=\"{}\"{mar_attr} aria-roledescription=\"carousel\" aria-label=\"{}\">{controls_html}<div class=\"od-slider-track flex overflow-x-auto\" tabindex=\"0\" role=\"group\" aria-label=\"{}\">{child_nonce}</div><p class=\"sr-only\" role=\"status\" aria-live=\"polite\" data-od-slider-status></p>{dots_html}{toggle_html}</section>",
        cls.join(" "),
        if single_spv { "1" } else { "multi" },
        loop_on,
        escape_html(&l10n.slider_slide_position_template),
        escape_html(&l10n.slider_pause),
        escape_html(&l10n.slider_play),
        escape_html(aria_label),
        escape_html(&l10n.slider_slides_group),
    ))
}

pub(super) fn render_slide(
    node: &BlockNode,
    fields: &[FieldSpec],
    child_nonce: &str,
    parent: NativeParentInfo<'_>,
    budget: &RenderBudget,
) -> String {
    let l10n = core_l10n(&budget.locale);
    let idx = parent.child_index;
    let total = parent.active_child_count.max(1);
    let pos_label = l10n
        .slider_slide_position_template
        .replace("{n}", &(idx + 1).to_string())
        .replace("{total}", &total.to_string());
    let (pad_tw, pad_fb) = spacing_classes_or_attr(fields, &node.props, "padding");
    let mut cls = vec!["od-slide".to_owned(), "min-w-0".to_owned(), "shrink-0".to_owned()];
    cls.extend(pad_tw);
    format!(
        "<div class=\"{}\" data-od-slide-index=\"{idx}\"{} role=\"group\" aria-roledescription=\"slide\" aria-label=\"{}\">{child_nonce}</div>",
        cls.join(" "),
        pad_attr(pad_fb),
        escape_html(&pos_label),
    )
}
