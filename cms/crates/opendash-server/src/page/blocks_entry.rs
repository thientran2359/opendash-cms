use std::collections::BTreeMap;

use opendash_core::{
    builder::BlockNode,
    content::EffectiveContentSchema,
    field_schema::valid_safe_url,
    media::valid_media_render_url,
};
use serde_json::Value;

use super::{
    blocks_chrome::{align_justify_classes, align_text_classes, bool_prop, preview_empty_placeholder},
    blocks_entry_field::render_entry_field,
    blocks_entry_nav::{render_entry_content, render_entry_nav, render_entry_terms},
    blocks_layout::{responsive_str_cascade, str_prop},
    render::escape_html,
};
use crate::{
    content_render::{
        DateFormatPreset, EntryView, HydratedMedia, TemplateRenderData, core_l10n,
        format_date_with_preset,
    },
    error::{ApiError, ApiResult},
    template_sec::{RenderBudget, register_core_icon_in_budget},
};

pub(super) fn aspect_ratio_class(ratio: &str) -> &'static str {
    match ratio {
        "auto" => "aspect-auto",
        "4_3" => "aspect-[4/3]",
        "3_2" => "aspect-[3/2]",
        "1_1" => "aspect-square",
        "4_5" => "aspect-[4/5]",
        _ => "aspect-video",
    }
}

pub(super) fn radius_class(radius: &str) -> &'static str {
    match radius {
        "none" => "rounded-none",
        "sm" => "rounded-sm",
        "md" => "rounded-md",
        "xl" => "rounded-xl",
        "full" => "rounded-full",
        _ => "rounded-lg",
    }
}

fn clamp_max_lines_class(raw: &str) -> Option<&'static str> {
    match raw {
        "1" => Some("line-clamp-1"),
        "2" => Some("line-clamp-2"),
        "3" => Some("line-clamp-3"),
        "4" => Some("line-clamp-4"),
        _ => None,
    }
}

fn truncate_chars_boundary(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let prefix: String = text.chars().take(max_chars).collect();
    format!("{}…", prefix.trim_end())
}

pub(super) fn render_entry_title(
    node: &BlockNode,
    entry: Option<&EntryView>,
    in_loop: bool,
    is_preview: bool,
) -> Option<String> {
    let Some(entry) = entry.filter(|e| !e.title.trim().is_empty()) else {
        return preview_empty_placeholder("Entry Title", is_preview);
    };
    let mut tag = match str_prop(&node.props, "tag", "h1") {
        s @ ("h1" | "h2" | "h3" | "h4") => s,
        _ => "h1",
    };
    if in_loop && tag == "h1" {
        tag = "h2";
    }
    let mut cls = vec![
        "od-entry-title".to_owned(),
        "font-od-heading".to_owned(),
        "font-bold".to_owned(),
        "tracking-tight".to_owned(),
    ];
    let (size_resp, ds, ts, ms) = responsive_str_cascade(&node.props, "size", "3xl");
    let map_sz = |v: &str| match v {
        "sm" => "text-sm",
        "base" => "text-base",
        "lg" => "text-lg",
        "xl" => "text-xl",
        "2xl" => "text-2xl",
        "4xl" => "text-4xl",
        _ => "text-3xl",
    };
    let (mc, tc, dc) = (map_sz(ms), map_sz(ts), map_sz(ds));
    cls.push(mc.to_owned());
    if size_resp && tc != mc {
        cls.push(format!("md:{tc}"));
    }
    if size_resp && dc != tc {
        cls.push(format!("lg:{dc}"));
    }
    cls.extend(align_text_classes(&node.props, "left"));
    if let Some(lc) = clamp_max_lines_class(str_prop(&node.props, "max_lines", "none")) {
        cls.push(lc.to_owned());
    }

    let style_attr = node
        .props
        .get("color")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|_| " style=\"color:var(--od-et-color);\"")
        .unwrap_or("");

    let esc_title = escape_html(&entry.title);
    let inner = if bool_prop(&node.props, "link_to_entry", false)
        && let Some(u) = entry.url.as_deref().map(str::trim).filter(|u| valid_safe_url(u))
    {
        format!(
            "<a href=\"{}\" class=\"hover:text-od-primary transition-colors\">{esc_title}</a>",
            escape_html(u)
        )
    } else {
        esc_title
    };
    Some(format!(
        "<{tag} class=\"{}\"{style_attr}>{inner}</{tag}>",
        cls.join(" ")
    ))
}

pub(super) fn render_entry_featured_image(
    node: &BlockNode,
    entry: Option<&EntryView>,
    in_loop: bool,
    is_preview: bool,
) -> Option<String> {
    let Some(entry) = entry else {
        return preview_empty_placeholder("Featured Image", is_preview);
    };
    let ratio = str_prop(&node.props, "aspect_ratio", "16_9");
    let fit = if str_prop(&node.props, "object_fit", "cover") == "contain" {
        "object-contain"
    } else {
        "object-cover"
    };
    let radius = radius_class(str_prop(&node.props, "radius", "lg"));
    let img_cls = format!("w-full h-full {fit}");

    let media_opt = entry
        .cover
        .clone()
        .or_else(|| entry.cover_media.as_ref().and_then(HydratedMedia::from_value));
    let sizes = if in_loop {
        "(max-width: 768px) 100vw, 480px"
    } else {
        "(max-width: 768px) 100vw, 1024px"
    };
    let img_html = media_opt
        .and_then(|media| {
            let input = media.as_picture_input(
                sizes,
                &img_cls,
                Some(&entry.title),
                if in_loop { "lazy" } else { "eager" },
                "",
            );
            opendash_core::media::render_responsive_picture_html(&input, is_preview).or_else(|| {
                valid_media_render_url(media.url.trim(), is_preview).then(|| {
                    format!(
                        "<img src=\"{}\" alt=\"{}\" class=\"{img_cls}\" loading=\"{}\" decoding=\"async\">",
                        escape_html(media.url.trim()),
                        escape_html(&entry.title),
                        if in_loop { "lazy" } else { "eager" },
                    )
                })
            })
        })
        .or_else(|| {
            entry
                .cover_url
                .as_deref()
                .map(str::trim)
                .filter(|u| valid_media_render_url(u, is_preview))
                .map(|url| {
                    format!(
                        "<img src=\"{}\" alt=\"{}\" class=\"{img_cls}\" loading=\"{}\" decoding=\"async\">",
                        escape_html(url),
                        escape_html(&entry.title),
                        if in_loop { "lazy" } else { "eager" },
                    )
                })
        });

    let Some(media_markup) = img_html else {
        return preview_empty_placeholder("Featured Image", is_preview);
    };

    let body = if bool_prop(&node.props, "link_to_entry", false)
        && let Some(u) = entry.url.as_deref().map(str::trim).filter(|u| valid_safe_url(u))
    {
        format!(
            "<a href=\"{}\" class=\"block w-full h-full\">{media_markup}</a>",
            escape_html(u)
        )
    } else {
        media_markup
    };
    let fig_cls = format!(
        "od-entry-featured-image overflow-hidden {} {}",
        aspect_ratio_class(ratio),
        radius
    );
    Some(format!("<figure class=\"{fig_cls}\">{body}</figure>"))
}

pub(super) fn render_entry_meta(
    node: &BlockNode,
    entry: Option<&EntryView>,
    is_preview: bool,
    timezone: &str,
    budget: &mut RenderBudget,
) -> Result<Option<String>, &'static str> {
    let Some(entry) = entry else {
        return Ok(preview_empty_placeholder("Entry Meta", is_preview));
    };
    let items: Vec<&str> = match node.props.get("items").and_then(Value::as_array) {
        Some(arr) => arr.iter().filter_map(Value::as_str).collect(),
        None => vec!["published_date", "reading_time"],
    };
    let preset = DateFormatPreset::parse(node.props.get("date_format").and_then(Value::as_str));
    let sep = match str_prop(&node.props, "separator", "dot") {
        "slash" => "/",
        "pipe" => "|",
        _ => "·",
    };
    let show_icons = bool_prop(&node.props, "show_icons", true);
    let l10n = core_l10n(&budget.locale);
    let mut parts: Vec<String> = Vec::new();

    for key in items {
        match key {
            "published_date" => {
                let raw_and_draft = if let Some(pub_ts) = entry.published_at.as_deref() {
                    Some((pub_ts, false))
                } else if is_preview && !entry.updated_at.trim().is_empty() {
                    Some((entry.updated_at.as_str(), true))
                } else {
                    None
                };
                if let Some((ts, is_draft_fb)) = raw_and_draft
                    && let Some(date_str) =
                        format_date_with_preset(ts, preset, timezone, &budget.locale)
                {
                    let icon = if show_icons {
                        register_core_icon_in_budget(budget, "calendar", Some("w-3.5 h-3.5"))?
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let suffix = if is_draft_fb {
                        format!(" {}", escape_html(l10n.preview_draft_date_suffix.as_ref()))
                    } else {
                        String::new()
                    };
                    parts.push(format!(
                        "<span class=\"inline-flex items-center gap-1\">{icon}<time>{}</time>{suffix}</span>",
                        escape_html(&date_str),
                    ));
                }
            }
            "updated_date" if !entry.updated_at.trim().is_empty() => {
                if let Some(date_str) =
                    format_date_with_preset(&entry.updated_at, preset, timezone, &budget.locale)
                {
                    let icon = if show_icons {
                        register_core_icon_in_budget(budget, "calendar", Some("w-3.5 h-3.5"))?
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    parts.push(format!(
                        "<span class=\"inline-flex items-center gap-1\">{icon}<time>{}</time></span>",
                        escape_html(&date_str),
                    ));
                }
            }
            "reading_time" if entry.reading_time_minutes > 0 => {
                let icon = if show_icons {
                    register_core_icon_in_budget(budget, "clock", Some("w-3.5 h-3.5"))?
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                parts.push(format!(
                    "<span class=\"inline-flex items-center gap-1\">{icon}{} {}</span>",
                    entry.reading_time_minutes,
                    escape_html(l10n.reading_time_suffix.as_ref()),
                ));
            }
            "primary_category" => {
                if let Some(cat) = &entry.primary_category {
                    let icon = if show_icons {
                        register_core_icon_in_budget(budget, "tag", Some("w-3.5 h-3.5"))?
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let name_html = escape_html(&cat.name);
                    let inner = match cat.url.as_deref().map(str::trim).filter(|u| valid_safe_url(u)) {
                        Some(u) => format!(
                            "<a href=\"{}\" class=\"hover:text-od-primary\">{name_html}</a>",
                            escape_html(u)
                        ),
                        None => name_html,
                    };
                    parts.push(format!(
                        "<span class=\"inline-flex items-center gap-1\">{icon}{inner}</span>"
                    ));
                }
            }
            _ => {}
        }
    }

    if parts.is_empty() {
        return Ok(preview_empty_placeholder("Entry Meta", is_preview));
    }
    let mut cls = vec![
        "od-entry-meta".to_owned(),
        "flex".to_owned(),
        "flex-wrap".to_owned(),
        "items-center".to_owned(),
        "gap-2".to_owned(),
        "text-xs".to_owned(),
        "text-od-muted".to_owned(),
    ];
    cls.extend(align_justify_classes(&node.props, "left"));
    let sep_html = format!("<span aria-hidden=\"true\">{sep}</span>");
    Ok(Some(format!(
        "<div class=\"{}\">{}</div>",
        cls.join(" "),
        parts.join(&sep_html),
    )))
}

pub(super) fn render_entry_excerpt(
    node: &BlockNode,
    entry: Option<&EntryView>,
    is_preview: bool,
) -> Option<String> {
    let raw = entry.map(|e| e.excerpt.trim()).filter(|s| !s.is_empty());
    let Some(excerpt) = raw else {
        return preview_empty_placeholder("Entry Excerpt", is_preview);
    };
    let max_chars = usize::try_from(
        node.props
            .get("max_chars")
            .and_then(Value::as_i64)
            .unwrap_or(180)
            .clamp(40, 500),
    )
    .unwrap_or(180);
    let text = truncate_chars_boundary(excerpt, max_chars);
    let mut cls = vec!["od-entry-excerpt".to_owned(), "text-od-muted".to_owned()];
    let (sz_resp, ds, ts, ms) = responsive_str_cascade(&node.props, "font_size", "base");
    let map_sz = |s: &str| match s {
        "sm" => "text-sm",
        "lg" => "text-lg",
        _ => "text-base",
    };
    let (mc, tc, dc) = (map_sz(ms), map_sz(ts), map_sz(ds));
    cls.push(mc.to_owned());
    if sz_resp && tc != mc {
        cls.push(format!("md:{tc}"));
    }
    if sz_resp && dc != tc {
        cls.push(format!("lg:{dc}"));
    }
    cls.extend(align_text_classes(&node.props, "left"));
    if let Some(lc) = clamp_max_lines_class(str_prop(&node.props, "max_lines", "3")) {
        cls.push(lc.to_owned());
    }
    let style_attr = node
        .props
        .get("color")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|_| " style=\"color:var(--od-ex-color);\"")
        .unwrap_or("");
    Some(format!(
        "<p class=\"{}\"{style_attr}>{}</p>",
        cls.join(" "),
        escape_html(&text)
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_entry_block_with_context(
    node: &BlockNode,
    active_entry: Option<&EntryView>,
    entry_schema: Option<&EffectiveContentSchema>,
    state_schemas: &BTreeMap<String, EffectiveContentSchema>,
    in_loop: bool,
    td: Option<&TemplateRenderData<'_>>,
    fallback_content_html: Option<&str>,
    is_preview: bool,
    timezone: &str,
    budget: &mut RenderBudget,
) -> ApiResult<Option<(Option<String>, usize)>> {
    let entry = active_entry.or_else(|| td.and_then(|d| d.entry));
    let schema = entry.and_then(|e| {
        entry_schema
            .filter(|s| s.base.descriptor.key == e.type_key)
            .or_else(|| {
                td.and_then(|d| d.entry_schema)
                    .filter(|s| s.base.descriptor.key == e.type_key)
            })
            .or_else(|| state_schemas.get(&e.type_key))
    });
    match node.block_id.as_str() {
        "core/entry-title" => Ok(Some((render_entry_title(node, entry, in_loop, is_preview), 0))),
        "core/entry-featured-image" => Ok(Some((
            render_entry_featured_image(node, entry, in_loop, is_preview),
            0,
        ))),
        "core/entry-meta" => render_entry_meta(node, entry, is_preview, timezone, budget)
            .map(|h| Some((h, 0)))
            .map_err(ApiError::bad_request),
        "core/entry-excerpt" => Ok(Some((render_entry_excerpt(node, entry, is_preview), 0))),
        "core/entry-content" => Ok(Some(render_entry_content(
            node,
            td,
            fallback_content_html,
            is_preview,
        ))),
        "core/entry-terms" => Ok(Some((render_entry_terms(node, entry, is_preview), 0))),
        "core/entry-field" => render_entry_field(node, entry, schema, in_loop, is_preview, timezone, budget)
            .map(|h| Some((h, 0)))
            .map_err(ApiError::bad_request),
        "core/entry-nav" => render_entry_nav(node, td, is_preview, budget)
            .map(|h| Some((h, 0)))
            .map_err(ApiError::bad_request),
        _ => Ok(None),
    }
}
