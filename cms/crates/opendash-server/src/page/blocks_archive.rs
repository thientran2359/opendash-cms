use std::collections::BTreeMap;

use opendash_core::{builder::BlockNode, field_schema::valid_safe_url, media::valid_media_render_url};
use serde_json::Value;

use super::{
    blocks_chrome::{align_justify_classes, align_text_classes, bool_prop, preview_empty_placeholder},
    blocks_entry::{aspect_ratio_class, radius_class},
    blocks_layout::{responsive_i64_cascade, responsive_str_cascade, str_prop},
    render::escape_html,
};
use crate::{
    content_render::{PaginationPageLink, PaginationView, TemplateRenderData, core_l10n},
    template_sec::RenderBudget,
};

pub(super) fn grid_cols_classes(props: &BTreeMap<String, Value>, default_cols: i64) -> Vec<String> {
    let (is_resp, d_r, t_r, m_r, _) = responsive_i64_cascade(props, "columns", default_cols);
    let (d, t, m) = (d_r.clamp(1, 4), t_r.clamp(1, 4), m_r.clamp(1, 4));
    let mut cls = vec!["grid".to_owned()];
    if !is_resp {
        cls.push("grid-cols-1".to_owned());
        if d >= 2 {
            cls.push(format!("md:grid-cols-{}", d.min(2)));
        }
        if d >= 3 {
            cls.push(format!("lg:grid-cols-{d}"));
        }
    } else {
        cls.push(format!("grid-cols-{m}"));
        if t != m {
            cls.push(format!("md:grid-cols-{t}"));
        }
        if d != t {
            cls.push(format!("lg:grid-cols-{d}"));
        }
    }
    cls
}

pub(super) fn node_extra_classes(node: &BlockNode) -> Vec<String> {
    node.meta
        .as_ref()
        .and_then(|m| m.class_name.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default()
}

fn render_archive_title(
    node: &BlockNode,
    td: Option<&TemplateRenderData<'_>>,
    locale: &str,
    is_preview: bool,
) -> Option<String> {
    let l10n = core_l10n(locale);
    let title = if let Some(term) = td.and_then(|d| d.term) {
        if bool_prop(&node.props, "show_prefix", false) && !term.taxonomy_label.trim().is_empty() {
            format!("{}: {}", term.taxonomy_label, term.name)
        } else {
            term.name.clone()
        }
    } else if let Some(ct) = td.and_then(|d| d.content_type) {
        ct.label.clone()
    } else {
        return preview_empty_placeholder("Archive Title", is_preview);
    };

    let tag = match str_prop(&node.props, "tag", "h1") {
        "h2" => "h2",
        _ => "h1",
    };
    let mut cls = vec![
        "od-archive-title".to_owned(),
        "font-od-heading".to_owned(),
        "font-bold".to_owned(),
        "tracking-tight".to_owned(),
    ];
    let (sz_resp, ds, ts, ms) = responsive_str_cascade(&node.props, "size", "3xl");
    let map_sz = |s: &str| match s {
        "xl" => "text-xl",
        "2xl" => "text-2xl",
        "4xl" => "text-4xl",
        _ => "text-3xl",
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

    let style_attr = node
        .props
        .get("color")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|_| " style=\"color:var(--od-at-color);\"")
        .unwrap_or("");

    let count_html = if bool_prop(&node.props, "show_total_count", false)
        && let Some(total) = td.and_then(|d| d.pagination).map(|p| p.total_items)
    {
        format!(
            " <span class=\"text-base font-normal text-od-muted\">({total} {})</span>",
            escape_html(l10n.posts_count_suffix.as_ref())
        )
    } else {
        String::new()
    };

    Some(format!(
        "<{tag} class=\"{}\"{style_attr}>{}{count_html}</{tag}>",
        cls.join(" "),
        escape_html(&title),
    ))
}

fn render_archive_description(
    node: &BlockNode,
    td: Option<&TemplateRenderData<'_>>,
    is_preview: bool,
) -> Option<String> {
    let raw = td.and_then(|d| {
        d.term
            .map(|t| t.description.as_str())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                d.content_type
                    .and_then(|ct| ct.description.as_deref())
                    .filter(|s| !s.trim().is_empty())
            })
    });
    let Some(desc) = raw else {
        return preview_empty_placeholder("Archive Description", is_preview);
    };

    let mut cls = vec!["od-archive-description".to_owned(), "text-od-muted".to_owned()];
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

    let style_attr = node
        .props
        .get("color")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|_| " style=\"color:var(--od-ad-color);\"")
        .unwrap_or("");

    Some(format!(
        "<p class=\"{}\"{style_attr}>{}</p>",
        cls.join(" "),
        escape_html(desc.trim()),
    ))
}

fn render_archive_featured_image(
    node: &BlockNode,
    td: Option<&TemplateRenderData<'_>>,
    is_preview: bool,
) -> Option<String> {
    let term = td.and_then(|d| d.term)?;
    let Some(media) = &term.featured_image else {
        return preview_empty_placeholder("Archive Image", is_preview);
    };
    let ratio = str_prop(&node.props, "aspect_ratio", "16_9");
    let fit = if str_prop(&node.props, "object_fit", "cover") == "contain" {
        "object-contain"
    } else {
        "object-cover"
    };
    let radius = radius_class(str_prop(&node.props, "radius", "lg"));
    let img_cls = format!("w-full h-full {fit}");
    let input = media.as_picture_input(
        "(max-width: 768px) 100vw, 1024px",
        &img_cls,
        Some(&term.name),
        "eager",
        "",
    );
    let media_html = opendash_core::media::render_responsive_picture_html(&input, is_preview)
        .or_else(|| {
            valid_media_render_url(media.url.trim(), is_preview).then(|| {
                format!(
                    "<img src=\"{}\" alt=\"{}\" class=\"{img_cls}\" loading=\"eager\" decoding=\"async\">",
                    escape_html(media.url.trim()),
                    escape_html(&term.name),
                )
            })
        })?;
    Some(format!(
        "<figure class=\"od-archive-featured-image overflow-hidden {} {radius}\">{media_html}</figure>",
        aspect_ratio_class(ratio),
    ))
}

fn render_archive_pagination(
    node: &BlockNode,
    td: Option<&TemplateRenderData<'_>>,
    locale: &str,
    is_preview: bool,
) -> Option<String> {
    let l10n = core_l10n(locale);
    let real_pag = td.and_then(|d| d.pagination);
    let preview_stub;
    let pag = match real_pag {
        Some(p) if p.total_pages > 1 => p,
        _ if is_preview => {
            preview_stub = PaginationView {
                page: 1,
                per_page: 12,
                total_items: 36,
                total_pages: 3,
                has_prev: false,
                has_next: true,
                previous_url: None,
                next_url: Some("?page=2".to_owned()),
                pages: (1..=3)
                    .map(|n| PaginationPageLink {
                        number: Some(n),
                        url: Some(format!("?page={n}")),
                        is_current: n == 1,
                        is_ellipsis: false,
                    })
                    .collect(),
            };
            &preview_stub
        }
        _ => return None,
    };

    let style = str_prop(&node.props, "style", "numbers");
    let prev_label = str_prop(&node.props, "prev_label", l10n.previous_page.as_ref());
    let next_label = str_prop(&node.props, "next_label", l10n.next_page.as_ref());
    let mut cls = vec![
        "od-archive-pagination".to_owned(),
        "flex".to_owned(),
        "flex-wrap".to_owned(),
        "items-center".to_owned(),
        "gap-2".to_owned(),
        "text-sm".to_owned(),
    ];
    cls.extend(align_justify_classes(&node.props, "center"));

    let mut inner = String::new();
    if let Some(u) = pag.previous_url.as_deref().map(str::trim).filter(|u| valid_safe_url(u)) {
        inner.push_str(&format!(
            "<a href=\"{}\" class=\"rounded border border-od-border px-3 py-1.5 hover:border-od-primary\">{}</a>",
            escape_html(u),
            escape_html(prev_label),
        ));
    }

    if style == "numbers" {
        for link in &pag.pages {
            if link.is_ellipsis {
                inner.push_str("<span class=\"px-2 text-od-muted\" aria-hidden=\"true\">…</span>");
            } else if let Some(num) = link.number {
                if link.is_current {
                    inner.push_str(&format!(
                        "<span aria-current=\"page\" class=\"rounded bg-od-primary px-3 py-1.5 font-semibold text-white\">{num}</span>"
                    ));
                } else if let Some(u) = link.url.as_deref().map(str::trim).filter(|u| valid_safe_url(u)) {
                    inner.push_str(&format!(
                        "<a href=\"{}\" class=\"rounded border border-od-border px-3 py-1.5 hover:border-od-primary\">{num}</a>",
                        escape_html(u)
                    ));
                }
            }
        }
    } else if style == "compact" {
        inner.push_str(&format!(
            "<span class=\"px-2 text-od-muted\">{} / {}</span>",
            pag.page, pag.total_pages
        ));
    }

    if let Some(u) = pag.next_url.as_deref().map(str::trim).filter(|u| valid_safe_url(u)) {
        inner.push_str(&format!(
            "<a href=\"{}\" class=\"rounded border border-od-border px-3 py-1.5 hover:border-od-primary\">{}</a>",
            escape_html(u),
            escape_html(next_label),
        ));
    }

    if inner.is_empty() {
        return None;
    }
    Some(format!(
        "<nav class=\"{}\" aria-label=\"Pagination\">{inner}</nav>",
        cls.join(" "),
    ))
}

pub(super) fn render_archive_block(
    node: &BlockNode,
    td: Option<&TemplateRenderData<'_>>,
    is_preview: bool,
    budget: &RenderBudget,
) -> Option<Option<String>> {
    match node.block_id.as_str() {
        "core/archive-title" => Some(render_archive_title(node, td, &budget.locale, is_preview)),
        "core/archive-description" => Some(render_archive_description(node, td, is_preview)),
        "core/archive-featured-image" => {
            Some(render_archive_featured_image(node, td, is_preview))
        }
        "core/archive-pagination" => {
            Some(render_archive_pagination(node, td, &budget.locale, is_preview))
        }
        _ => None,
    }
}
