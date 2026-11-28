use std::collections::BTreeMap;

use opendash_core::{
    builder::BlockNode,
    field_schema::{FieldSpec, valid_safe_url, valid_sha256_media_id},
};
use serde_json::Value;

use super::{
    blocks_layout::{NativeParentInfo, responsive_str_cascade, spacing_classes_or_attr},
    render::escape_html,
};
use crate::{error::ApiError, error::ApiResult, template_sec::RenderBudget};

fn str_prop<'a>(props: &'a BTreeMap<String, Value>, key: &str, default: &'a str) -> &'a str {
    props
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
}

fn link_rel(new_tab: bool, nofollow: bool) -> &'static str {
    match (new_tab, nofollow) {
        (true, true) => "noopener noreferrer nofollow",
        (false, true) => "nofollow",
        _ => "noopener noreferrer",
    }
}

fn link_props<'a>(
    props: &'a BTreeMap<String, Value>,
    key: &str,
) -> Option<(&'a str, &'a str, bool, bool)> {
    let obj = props.get(key)?.as_object()?;
    let url = obj.get("url").and_then(Value::as_str).map(str::trim)?;
    if !valid_safe_url(url) {
        return None;
    }
    let label = obj
        .get("label")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    let new_tab = obj.get("new_tab").and_then(Value::as_bool).unwrap_or(false);
    let nofollow = obj
        .get("nofollow")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some((url, label, new_tab, nofollow))
}

fn render_heading(node: &BlockNode) -> String {
    let level = match str_prop(&node.props, "level", "h2") {
        s @ ("h1" | "h2" | "h3" | "h4" | "h5" | "h6") => s,
        _ => "h2",
    };
    let text = node
        .props
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("Tiêu đề");
    let mut cls = vec![
        "od-heading".to_owned(),
        "font-od-heading".to_owned(),
        "font-bold".to_owned(),
        "tracking-tight".to_owned(),
    ];
    let (size_resp, ds, ts, ms) = responsive_str_cascade(&node.props, "size", "2xl");
    let map_size = |s: &str| match s {
        "xs" => "text-xs",
        "sm" => "text-sm",
        "md" => "text-base",
        "lg" => "text-lg",
        "xl" => "text-xl",
        "3xl" => "text-3xl",
        "4xl" => "text-4xl",
        _ => "text-2xl",
    };
    let (sm_c, md_c, lg_c) = (map_size(ms), map_size(ts), map_size(ds));
    cls.push(sm_c.to_owned());
    if size_resp && md_c != sm_c {
        cls.push(format!("md:{md_c}"));
    }
    if size_resp && lg_c != md_c {
        cls.push(format!("lg:{lg_c}"));
    }
    if node.props.contains_key("align") {
        let (al_resp, da, ta, ma) = responsive_str_cascade(&node.props, "align", "left");
        let map_align = |s: &str| match s {
            "center" => "text-center",
            "right" => "text-right",
            _ => "text-left",
        };
        let (ac_m, ac_t, ac_d) = (map_align(ma), map_align(ta), map_align(da));
        cls.push(ac_m.to_owned());
        if al_resp && ac_t != ac_m {
            cls.push(format!("md:{ac_t}"));
        }
        if al_resp && ac_d != ac_t {
            cls.push(format!("lg:{ac_d}"));
        }
    }
    match str_prop(&node.props, "decoration", "none") {
        "underline" => cls.extend(["underline", "underline-offset-4"].map(str::to_owned)),
        "line_side" => cls.extend(
            [
                "od-heading-line-side",
                "border-l-4",
                "border-od-primary",
                "pl-3",
            ]
            .map(str::to_owned),
        ),
        _ => {}
    }
    let style_attr = node
        .props
        .get("color")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|_| " style=\"color:var(--od-heading-color);\"")
        .unwrap_or("");
    let escaped_text = escape_html(text);
    let inner = match link_props(&node.props, "link") {
        Some((url, _, new_tab, nofollow)) => {
            let target = if new_tab { " target=\"_blank\"" } else { "" };
            let rel = link_rel(new_tab, nofollow);
            format!(
                "<a href=\"{}\"{target} rel=\"{rel}\" class=\"hover:underline\">{escaped_text}</a>",
                escape_html(url),
            )
        }
        None => escaped_text,
    };
    format!(
        "<{level} class=\"{}\"{style_attr}>{inner}</{level}>",
        cls.join(" ")
    )
}

fn render_text(node: &BlockNode) -> String {
    let raw = node
        .props
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("<p>Nhập nội dung văn bản...</p>");
    let clean =
        opendash_core::richtext::sanitize_richtext(raw).unwrap_or_else(|_| escape_html(raw));
    let mut cls = vec!["od-rich-text".to_owned(), "od-prose".to_owned()];
    let (sz_resp, ds, ts, ms) = responsive_str_cascade(&node.props, "font_size", "base");
    let map_sz = |s: &str| match s {
        "sm" => "text-sm",
        "lg" => "text-lg",
        "xl" => "text-xl",
        _ => "text-base",
    };
    let (sm_c, md_c, lg_c) = (map_sz(ms), map_sz(ts), map_sz(ds));
    cls.push(sm_c.to_owned());
    if sz_resp && md_c != sm_c {
        cls.push(format!("md:{md_c}"));
    }
    if sz_resp && lg_c != md_c {
        cls.push(format!("lg:{lg_c}"));
    }
    if node.props.contains_key("align") {
        let (al_resp, da, ta, ma) = responsive_str_cascade(&node.props, "align", "left");
        let map_al = |s: &str| match s {
            "center" => "text-center",
            "right" => "text-right",
            "justify" => "text-justify",
            _ => "text-left",
        };
        let (ac_m, ac_t, ac_d) = (map_al(ma), map_al(ta), map_al(da));
        cls.push(ac_m.to_owned());
        if al_resp && ac_t != ac_m {
            cls.push(format!("md:{ac_t}"));
        }
        if al_resp && ac_d != ac_t {
            cls.push(format!("lg:{ac_d}"));
        }
    }
    match str_prop(&node.props, "max_width", "none") {
        "prose" => cls.push("max-w-prose".to_owned()),
        "2xl" => cls.push("max-w-2xl".to_owned()),
        "4xl" => cls.push("max-w-4xl".to_owned()),
        _ => {}
    }
    let style_attr = node
        .props
        .get("color")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|_| " style=\"color:var(--od-text-color);\"")
        .unwrap_or("");
    format!("<div class=\"{}\"{style_attr}>{clean}</div>", cls.join(" "))
}

fn render_image(
    node: &BlockNode,
    hydration_media: &BTreeMap<String, crate::storage::fields::HydratedMediaRow>,
    is_preview: bool,
    native_parent: NativeParentInfo<'_>,
) -> String {
    let Some(id) = node
        .props
        .get("image")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| valid_sha256_media_id(id))
    else {
        return String::new();
    };
    let alt = node.props.get("alt").and_then(Value::as_str).unwrap_or("");
    let caption = node
        .props
        .get("caption")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let mut img_cls = vec!["w-full".to_owned()];
    match str_prop(&node.props, "aspect_ratio", "auto") {
        "16_9" => img_cls.push("aspect-video".to_owned()),
        "4_3" => img_cls.push("aspect-[4/3]".to_owned()),
        "1_1" => img_cls.push("aspect-square".to_owned()),
        "3_4" => img_cls.push("aspect-[3/4]".to_owned()),
        _ => {}
    }
    img_cls.push(
        if str_prop(&node.props, "object_fit", "cover") == "contain" {
            "object-contain"
        } else {
            "object-cover"
        }
        .to_owned(),
    );
    match str_prop(&node.props, "radius", "lg") {
        "none" => {}
        r @ ("sm" | "md" | "lg" | "xl" | "2xl" | "full") => img_cls.push(format!("rounded-{r}")),
        _ => {}
    }
    let fetchpriority = if native_parent.in_deferred_media {
        "low"
    } else {
        ""
    };
    let img_html = super::media_render::render_responsive_image_html_with_priority(
        id,
        alt,
        &img_cls.join(" "),
        "100vw",
        hydration_media,
        is_preview,
        "lazy",
        fetchpriority,
    );
    let media_html = match link_props(&node.props, "link") {
        Some((url, _, new_tab, nofollow)) => {
            let target = if new_tab { " target=\"_blank\"" } else { "" };
            let rel = link_rel(new_tab, nofollow);
            format!(
                "<a href=\"{}\"{target} rel=\"{rel}\" class=\"block\">{img_html}</a>",
                escape_html(url),
            )
        }
        None => img_html,
    };
    let cap_html = caption
        .map(|c| {
            format!(
                "<figcaption class=\"mt-2 text-sm text-od-muted\">{}</figcaption>",
                escape_html(c)
            )
        })
        .unwrap_or_default();
    format!("<figure class=\"od-image\">{media_html}{cap_html}</figure>")
}

fn render_video(
    node: &BlockNode,
    hydration_media: &BTreeMap<String, crate::storage::fields::HydratedMediaRow>,
    is_preview: bool,
) -> String {
    let Some(id) = node
        .props
        .get("video")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| valid_sha256_media_id(id))
    else {
        return String::new();
    };
    let video_url = super::media_render::versioned_media_file_url(id, hydration_media, is_preview);
    let mut attrs = format!(
        "src=\"{}\" preload=\"metadata\" playsinline class=\"w-full rounded-lg\"",
        escape_html(&video_url),
    );
    if let Some(poster) = node
        .props
        .get("poster")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| valid_sha256_media_id(id))
    {
        let poster_url =
            super::media_render::versioned_media_file_url(poster, hydration_media, is_preview);
        attrs.push_str(&format!(" poster=\"{}\"", escape_html(&poster_url)));
    }
    if node
        .props
        .get("controls")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        attrs.push_str(" controls");
    }
    if node
        .props
        .get("muted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        attrs.push_str(" muted");
    }
    if node
        .props
        .get("loop")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        attrs.push_str(" loop");
    }
    format!("<figure class=\"od-video\"><video {attrs}></video></figure>")
}

fn render_button(node: &BlockNode, budget: &mut RenderBudget) -> ApiResult<String> {
    let Some((url, label, new_tab, nofollow)) = link_props(&node.props, "cta") else {
        return Ok(String::new());
    };
    if label.is_empty() {
        return Ok(String::new());
    }
    let mut cls = vec![
        "od-btn",
        "inline-flex",
        "items-center",
        "justify-center",
        "gap-2",
        "rounded-lg",
        "font-medium",
        "transition",
    ];
    match str_prop(&node.props, "variant", "primary") {
        "secondary" => cls.extend(["bg-od-surface-alt", "text-od-text", "hover:opacity-90"]),
        "outline" => cls.extend([
            "border",
            "border-od-border",
            "text-od-text",
            "bg-transparent",
            "hover:bg-od-surface-alt",
        ]),
        "ghost" => cls.extend(["text-od-text", "bg-transparent", "hover:bg-od-surface-alt"]),
        _ => cls.extend(["bg-od-primary", "text-white", "hover:opacity-90"]),
    }
    match str_prop(&node.props, "size", "md") {
        "sm" => cls.extend(["px-3", "py-1.5", "text-sm"]),
        "lg" => cls.extend(["px-6", "py-3", "text-base"]),
        _ => cls.extend(["px-4", "py-2", "text-sm"]),
    }
    if node
        .props
        .get("full_width")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        cls.push("w-full");
    }
    let icon_nonce = if let Some(icon_name) = node
        .props
        .get("icon")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        crate::template_sec::register_core_icon_in_budget(
            budget,
            icon_name,
            Some("od-btn-icon w-4 h-4 shrink-0"),
        )
        .map_err(ApiError::bad_request)?
    } else {
        None
    };
    let icon_right = str_prop(&node.props, "icon_pos", "left") == "right";
    let text_html = escape_html(label);
    let inner = match icon_nonce {
        Some(nonce) if icon_right => format!("{text_html}{nonce}"),
        Some(nonce) => format!("{nonce}{text_html}"),
        None => text_html,
    };
    let target_attr = if new_tab { " target=\"_blank\"" } else { "" };
    let rel_attr = link_rel(new_tab, nofollow);
    Ok(format!(
        "<a href=\"{}\"{target_attr} rel=\"{rel_attr}\" class=\"{}\">{inner}</a>",
        escape_html(url),
        cls.join(" "),
    ))
}

fn render_divider(node: &BlockNode, fields: &[FieldSpec]) -> String {
    let align_cls = match str_prop(&node.props, "align", "center") {
        "left" => "justify-start",
        "right" => "justify-end",
        _ => "justify-center",
    };
    let (mar_tw, mar_fb) = spacing_classes_or_attr(fields, &node.props, "margin");
    let mut cls = vec![
        "od-divider".to_owned(),
        "flex".to_owned(),
        "w-full".to_owned(),
        align_cls.to_owned(),
    ];
    cls.extend(mar_tw);
    let mar_attr = if mar_fb {
        " data-od-spacing=\"margin\""
    } else {
        ""
    };
    format!(
        "<div role=\"separator\" class=\"{}\"{mar_attr}><hr class=\"od-divider-line border-0\" style=\"width:var(--od-divider-w,100%);border-top-width:var(--od-divider-t,1px);border-top-style:var(--od-divider-style,solid);border-top-color:var(--od-divider-c,#e4e4e7);\"></div>",
        cls.join(" "),
    )
}

pub(super) fn render_native_leaf(
    node: &BlockNode,
    fields: &[FieldSpec],
    hydration_media: &BTreeMap<String, crate::storage::fields::HydratedMediaRow>,
    is_preview: bool,
    native_parent: NativeParentInfo<'_>,
    budget: &mut RenderBudget,
) -> ApiResult<Option<String>> {
    match node.block_id.as_str() {
        "core/heading" => Ok(Some(render_heading(node))),
        "core/text" => Ok(Some(render_text(node))),
        "core/image" => Ok(Some(render_image(
            node,
            hydration_media,
            is_preview,
            native_parent,
        ))),
        "core/video" => Ok(Some(render_video(node, hydration_media, is_preview))),
        "core/button" => Ok(Some(render_button(node, budget)?)),
        "core/divider" => Ok(Some(render_divider(node, fields))),
        "core/spacer" => Ok(Some(
            "<div class=\"od-spacer w-full\" aria-hidden=\"true\" style=\"height:var(--od-spacer-h,32px);\"></div>"
                .to_owned(),
        )),
        "core/gallery" => super::blocks_media_share::render_gallery(
            node,
            fields,
            hydration_media,
            is_preview,
            native_parent,
            budget,
        )
        .map(Some),
        "core/social-share" => {
            super::blocks_media_share::render_social_share(node, fields, is_preview, budget)
                .map(Some)
        }
        _ => Ok(None),
    }
}
