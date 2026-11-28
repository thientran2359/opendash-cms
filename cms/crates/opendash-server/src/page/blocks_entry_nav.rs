use opendash_core::{
    builder::BlockNode,
    field_schema::valid_safe_url,
    media::valid_media_render_url,
};
use serde_json::Value;

use super::{
    blocks_chrome::{align_justify_classes, bool_prop, preview_empty_placeholder},
    blocks_layout::str_prop,
    render::escape_html,
};
use crate::{
    content_render::{EntryView, TemplateRenderData, core_l10n},
    template_sec::{RenderBudget, register_core_icon_in_budget},
};

pub(super) fn render_entry_terms(
    node: &BlockNode,
    entry: Option<&EntryView>,
    is_preview: bool,
) -> Option<String> {
    let Some(entry) = entry else {
        return preview_empty_placeholder("Entry Terms", is_preview);
    };
    let tax_key = str_prop(&node.props, "taxonomy_key", "");
    let max_items = usize::try_from(
        node.props
            .get("max_items")
            .and_then(Value::as_i64)
            .unwrap_or(6)
            .clamp(1, 12),
    )
    .unwrap_or(6);
    let terms: Vec<_> = if tax_key.is_empty() {
        entry.all_terms.iter().take(max_items).collect()
    } else {
        entry
            .terms
            .get(tax_key)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .take(max_items)
            .collect()
    };
    if terms.is_empty() {
        return preview_empty_placeholder("Entry Terms", is_preview);
    }

    let style = str_prop(&node.props, "style", "pill");
    let item_cls = match style {
        "outline" => "inline-block rounded-full border border-od-border px-2.5 py-0.5 text-xs hover:border-od-primary",
        "text" => "inline-block text-xs text-od-muted hover:text-od-primary",
        _ => "inline-block rounded-full bg-od-surface-alt px-2.5 py-0.5 text-xs font-medium hover:text-od-primary",
    };
    let link_to_term = bool_prop(&node.props, "link_to_term", true);
    let mut cls = vec![
        "od-entry-terms".to_owned(),
        "flex".to_owned(),
        "flex-wrap".to_owned(),
        "items-center".to_owned(),
        "gap-1.5".to_owned(),
    ];
    cls.extend(align_justify_classes(&node.props, "left"));

    let mut inner = String::new();
    let prefix = node
        .props
        .get("prefix_label")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if !prefix.is_empty() {
        inner.push_str(&format!(
            "<span class=\"text-xs font-medium text-od-muted\">{}</span>",
            escape_html(prefix),
        ));
    }
    for term in terms {
        let name = escape_html(&term.name);
        if link_to_term
            && let Some(u) = term.url.as_deref().map(str::trim).filter(|u| valid_safe_url(u))
        {
            inner.push_str(&format!(
                "<a href=\"{}\" class=\"{item_cls}\">{name}</a>",
                escape_html(u)
            ));
        } else {
            inner.push_str(&format!("<span class=\"{item_cls}\">{name}</span>"));
        }
    }
    Some(format!("<div class=\"{}\">{inner}</div>", cls.join(" ")))
}

pub(super) fn render_entry_content(
    node: &BlockNode,
    td: Option<&TemplateRenderData<'_>>,
    fallback_content_html: Option<&str>,
    is_preview: bool,
) -> (Option<String>, usize) {
    let raw_html = match td {
        Some(d) => d.entry_body_html,
        None => fallback_content_html,
    }
    .filter(|s| !s.trim().is_empty());
    let has_builder_body = td.is_some_and(|d| d.entry_has_builder_body);
    if let Some(body) = raw_html {
        let cls = if has_builder_body {
            "od-entry-content w-full"
        } else {
            match str_prop(&node.props, "prose_style", "default") {
                "wide" => "od-entry-content od-prose max-w-5xl",
                "none" => "od-entry-content",
                _ => "od-entry-content od-prose max-w-3xl",
            }
        };
        return (
            Some(format!("<div class=\"{cls}\">{body}</div>")),
            body.len(),
        );
    }

    if bool_prop(&node.props, "fallback_to_fields", false)
        && let Some(entry) = td.and_then(|d| d.entry)
    {
        let mut dl_items = String::new();
        for f in &entry.field_list {
            if f.text.trim().is_empty() {
                continue;
            }
            dl_items.push_str(&format!(
                "<div><dt class=\"text-xs font-medium text-od-muted\">{}</dt><dd class=\"text-sm\">{}</dd></div>",
                escape_html(&f.label),
                escape_html(&f.text),
            ));
        }
        if !dl_items.is_empty() {
            return (
                Some(format!(
                    "<div class=\"od-entry-content\"><dl class=\"grid gap-3 sm:grid-cols-2\">{dl_items}</dl></div>"
                )),
                0,
            );
        }
    }

    (
        preview_empty_placeholder("Nội dung bài (Entry Content)", is_preview),
        0,
    )
}

pub(super) fn render_entry_nav(
    node: &BlockNode,
    td: Option<&TemplateRenderData<'_>>,
    is_preview: bool,
    budget: &mut RenderBudget,
) -> Result<Option<String>, &'static str> {
    let same_tax = bool_prop(&node.props, "same_taxonomy", false);
    let adjacent = td.and_then(|d| {
        if same_tax {
            d.adjacent_entries_same_taxonomy
                .or(d.adjacent_entries_global)
        } else {
            d.adjacent_entries_global
        }
    });
    let Some(adj) = adjacent.filter(|a| a.prev.is_some() || a.next.is_some()) else {
        return Ok(preview_empty_placeholder("Entry Navigation", is_preview));
    };

    let l10n = core_l10n(&budget.locale);
    let show_thumb = bool_prop(&node.props, "show_thumbnail", true);
    let cards_style = str_prop(&node.props, "style", "cards") == "cards";
    let prev_lbl = str_prop(&node.props, "prev_label", l10n.prev_entry.as_ref());
    let next_lbl = str_prop(&node.props, "next_label", l10n.next_entry.as_ref());
    let base_link_cls = if cards_style {
        "flex items-center gap-3 rounded-lg border border-od-border p-4 hover:border-od-primary transition-colors"
    } else {
        "flex items-center gap-2 py-2 hover:text-od-primary"
    };

    let mut cols = String::new();
    if let Some(prev) = &adj.prev
        && let Some(href) = prev.url.as_deref().map(str::trim).filter(|u| valid_safe_url(u))
    {
        let arrow = register_core_icon_in_budget(budget, "arrow-left", Some("w-4 h-4 shrink-0"))?
            .unwrap_or_default();
        let thumb = if show_thumb
            && let Some(u) = prev
                .cover_url
                .as_deref()
                .map(str::trim)
                .filter(|u| valid_media_render_url(u, is_preview))
        {
            format!(
                "<img src=\"{}\" alt=\"{}\" class=\"h-12 w-12 shrink-0 rounded object-cover\" loading=\"lazy\" decoding=\"async\">",
                escape_html(u),
                escape_html(&prev.title),
            )
        } else {
            String::new()
        };
        cols.push_str(&format!(
            "<a href=\"{}\" class=\"{base_link_cls}\">{arrow}{thumb}<span class=\"min-w-0\"><span class=\"block text-xs text-od-muted\">{}</span><span class=\"block truncate font-medium\">{}</span></span></a>",
            escape_html(href),
            escape_html(prev_lbl),
            escape_html(&prev.title),
        ));
    }

    if let Some(next) = &adj.next
        && let Some(href) = next.url.as_deref().map(str::trim).filter(|u| valid_safe_url(u))
    {
        let arrow = register_core_icon_in_budget(budget, "arrow-right", Some("w-4 h-4 shrink-0"))?
            .unwrap_or_default();
        let thumb = if show_thumb
            && let Some(u) = next
                .cover_url
                .as_deref()
                .map(str::trim)
                .filter(|u| valid_media_render_url(u, is_preview))
        {
            format!(
                "<img src=\"{}\" alt=\"{}\" class=\"h-12 w-12 shrink-0 rounded object-cover\" loading=\"lazy\" decoding=\"async\">",
                escape_html(u),
                escape_html(&next.title),
            )
        } else {
            String::new()
        };
        cols.push_str(&format!(
            "<a href=\"{}\" class=\"md:col-start-2 justify-end text-right {base_link_cls}\"><span class=\"min-w-0\"><span class=\"block text-xs text-od-muted\">{}</span><span class=\"block truncate font-medium\">{}</span></span>{thumb}{arrow}</a>",
            escape_html(href),
            escape_html(next_lbl),
            escape_html(&next.title),
        ));
    }

    if cols.is_empty() {
        return Ok(preview_empty_placeholder("Entry Navigation", is_preview));
    }
    Ok(Some(format!(
        "<nav class=\"od-entry-nav grid grid-cols-1 md:grid-cols-2 gap-4\" aria-label=\"{}\">{cols}</nav>",
        escape_html(prev_lbl),
    )))
}
