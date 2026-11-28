use opendash_core::{builder::BlockNode, field_schema::FieldSpec};
use serde_json::Value;

use super::{
    blocks_chrome::bool_prop,
    blocks_interactive::pad_attr,
    blocks_layout::{NativeParentInfo, spacing_classes_or_attr, str_prop},
    render::escape_html,
};
use crate::{
    content_render::core_l10n,
    error::{ApiError, ApiResult},
    template_sec::{RenderBudget, register_core_icon_in_budget},
};

pub(super) fn sanitize_popup_id(raw: &str) -> Option<&str> {
    let trimmed = raw.trim().strip_prefix('#').unwrap_or(raw.trim());
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() || bytes.len() > 48 || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    if bytes
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
    {
        Some(trimmed)
    } else {
        None
    }
}

pub(super) fn render_popup(
    node: &BlockNode,
    fields: &[FieldSpec],
    child_nonce: &str,
    node_scope: &str,
    parent: NativeParentInfo<'_>,
    is_preview: bool,
    budget: &mut RenderBudget,
) -> ApiResult<String> {
    if parent.in_popup {
        return Ok(String::new());
    }
    let l10n = core_l10n(&budget.locale);
    let raw_id = node
        .props
        .get("popup_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let Some(safe_id) = sanitize_popup_id(raw_id) else {
        if is_preview {
            return Ok(format!(
                "<div class=\"od-popup-host\" data-od-popup-host><span class=\"od-popup-preview-chip od-popup-preview-invalid\">{}</span></div>",
                escape_html(&l10n.popup_invalid_id_preview),
            ));
        }
        return Ok(String::new());
    };
    if !is_preview {
        budget.needs_blocks_js = true;
    }

    let position = match str_prop(&node.props, "position", "center") {
        p @ ("left" | "right" | "top" | "bottom") => p,
        _ => "center",
    };
    let full_height = bool_prop(&node.props, "full_height", false);
    let backdrop_style = match str_prop(&node.props, "backdrop_style", "blur") {
        b @ ("dark" | "light" | "none") => b,
        _ => "blur",
    };
    let radius = match str_prop(&node.props, "radius", "xl") {
        r @ ("none" | "md" | "lg" | "2xl") => r,
        _ => "xl",
    };
    let close_on_backdrop = bool_prop(&node.props, "close_on_backdrop", true);
    let show_close = bool_prop(&node.props, "show_close_button", true);
    let open_on_hash = bool_prop(&node.props, "open_on_hash", true);
    let effective_show_close = show_close || !close_on_backdrop;
    let closedby = if close_on_backdrop {
        "any"
    } else {
        "closerequest"
    };
    let aria_label = node
        .props
        .get("aria_label")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(l10n.popup_default_label.as_ref());

    let chip_html = if is_preview {
        format!(
            "<div class=\"od-popup-preview-chip\">Popup #{} \u{00B7} {position}</div>",
            escape_html(safe_id),
        )
    } else {
        String::new()
    };
    let close_btn = if effective_show_close {
        let icon_x = register_core_icon_in_budget(budget, "x", Some("w-4 h-4"))
            .map_err(ApiError::bad_request)?
            .unwrap_or_default();
        format!(
            "<button type=\"button\" class=\"od-popup-close\" data-od-popup-close aria-label=\"{}\">{icon_x}</button>",
            escape_html(&l10n.popup_close),
        )
    } else {
        String::new()
    };

    let (pad_tw, pad_fb) = spacing_classes_or_attr(fields, &node.props, "padding");
    let mut body_cls = vec!["od-popup-body".to_owned()];
    body_cls.extend(pad_tw);

    Ok(format!(
        "<div class=\"od-popup-host\" data-od-popup-host data-od-popup-id=\"{}\">{chip_html}<dialog id=\"od-popup-dlg-{node_scope}\" class=\"od-popup-dialog\" data-od-popup-dialog=\"{}\" data-od-popup-pos=\"{position}\" data-od-popup-full-h=\"{full_height}\" data-od-popup-backdrop=\"{backdrop_style}\" data-od-popup-radius=\"{radius}\" data-od-popup-backdrop-close=\"{close_on_backdrop}\" data-od-popup-hash=\"{open_on_hash}\" closedby=\"{closedby}\" aria-modal=\"true\" aria-label=\"{}\"><div class=\"od-popup-surface\">{close_btn}<div class=\"{}\"{}>{child_nonce}</div></div></dialog></div>",
        escape_html(safe_id),
        escape_html(safe_id),
        escape_html(aria_label),
        body_cls.join(" "),
        pad_attr(pad_fb),
    ))
}
