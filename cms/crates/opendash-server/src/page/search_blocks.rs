use super::*;
use crate::content_render::{
    EntryView, PaginationView, SearchView, core_l10n, render_snippet_segments_html,
};
use opendash_core::builder::BlockNode;
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub struct BuilderSearchData<'a> {
    pub view: &'a SearchView,
    pub entries: &'a [EntryView],
    pub pagination: &'a PaginationView,
}

pub(super) fn render_search_form_block(
    state: &BuilderRenderContext,
    node: &BlockNode,
    context: Option<&BuilderTemplateContext<'_>>,
    is_preview: bool,
    locale: &str,
) -> String {
    let l10n = core_l10n(locale);
    let search_view = context.and_then(|ctx| ctx.search.as_ref()).map(|s| s.view);
    let placeholder = node
        .props
        .get("placeholder")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(l10n.search_placeholder.as_ref());
    let submit_raw = node
        .props
        .get("submit_label")
        .and_then(Value::as_str)
        .unwrap_or("Tìm kiếm")
        .trim();
    let submit_label = if submit_raw.is_empty() {
        l10n.search_submit.as_ref()
    } else {
        submit_raw
    };
    let show_type_filter = node
        .props
        .get("show_type_filter")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let preserve_type_filter = node
        .props
        .get("preserve_type_filter")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let action_url = search_view
        .map(|s| s.action_url.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            if state.locale_search_url.trim().is_empty() {
                "/"
            } else {
                state.locale_search_url.as_str()
            }
        });
    let query = search_view.map(|s| s.query.as_str()).unwrap_or("");
    let active_type = search_view
        .and_then(|s| s.type_filter.as_deref())
        .filter(|s| !s.is_empty());

    if is_preview {
        let mut html = format!(
            r#"<div class="od-search-form od-search-preview"><input type="search" disabled value="{}" placeholder="{}" aria-label="{}">"#,
            escape_html(query),
            escape_html(placeholder),
            escape_html(&l10n.search_title),
        );
        if show_type_filter {
            html.push_str(&format!(
                r#"<select disabled aria-label="{}"><option value="">{}</option>"#,
                escape_html(&l10n.search_title),
                escape_html(&l10n.search_all_types),
            ));
            if let Some(view) = search_view {
                for facet in &view.facets {
                    let selected = if facet.is_active { " selected" } else { "" };
                    html.push_str(&format!(
                        r#"<option value="{}"{selected}>{}</option>"#,
                        escape_html(&facet.type_key),
                        escape_html(&facet.label),
                    ));
                }
            }
            html.push_str("</select>");
        }
        html.push_str(&format!(
            r#"<button type="button" disabled>{}</button></div>"#,
            escape_html(submit_label),
        ));
        return html;
    }

    let mut html = format!(
        r#"<search class="od-search-box"><form method="get" action="{}" class="od-search-form" role="search"><input type="search" name="s" value="{}" placeholder="{}" aria-label="{}" maxlength="200" autocorrect="off" autocapitalize="none" spellcheck="false">"#,
        escape_html(action_url),
        escape_html(query),
        escape_html(placeholder),
        escape_html(&l10n.search_title),
    );
    let mut emitted_select = false;
    if show_type_filter
        && let Some(view) = search_view
        && !view.facets.is_empty()
    {
        emitted_select = true;
        html.push_str(&format!(
            r#"<select name="type" aria-label="{}"><option value="">{}</option>"#,
            escape_html(&l10n.search_title),
            escape_html(&l10n.search_all_types),
        ));
        for facet in &view.facets {
            let selected = if facet.is_active { " selected" } else { "" };
            html.push_str(&format!(
                r#"<option value="{}"{selected}>{}</option>"#,
                escape_html(&facet.type_key),
                escape_html(&facet.label),
            ));
        }
        html.push_str("</select>");
    }
    if !emitted_select
        && preserve_type_filter
        && let Some(type_key) = active_type
    {
        html.push_str(&format!(
            r#"<input type="hidden" name="type" value="{}">"#,
            escape_html(type_key),
        ));
    }
    html.push_str(&format!(
        r#"<button type="submit">{}</button></form></search>"#,
        escape_html(submit_label),
    ));
    html
}

pub(super) fn render_search_results_block(
    node: &BlockNode,
    context: Option<&BuilderTemplateContext<'_>>,
    is_preview: bool,
    locale: &str,
    diagnostics: &mut Vec<BuilderDiagnostic>,
) -> Option<String> {
    let node_id = node.id.as_str();
    let show_facets = node
        .props
        .get("show_facets")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let show_excerpt = node
        .props
        .get("show_excerpt")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let empty_message = node
        .props
        .get("empty_message")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    let Some(search_ctx) = context
        .and_then(|ctx| ctx.search)
        .filter(|s| s.view.is_search_route)
    else {
        if is_preview {
            diagnostics.push(BuilderDiagnostic {
                node_id: node_id.to_owned(),
                code: "search_results_outside_search_route",
            });
            let notice = if locale.starts_with("en") {
                "Search Results block only displays items on the search route."
            } else {
                "Khối Kết quả tìm kiếm chỉ hiển thị nội dung trên trang tìm kiếm."
            };
            return Some(format!(
                r#"<div class="od-search-results-unavailable" data-diagnostic-code="search_results_outside_search_route">{}</div>"#,
                escape_html(notice),
            ));
        }
        return None;
    };

    let l10n = core_l10n(locale);
    let view = search_ctx.view;
    if !view.has_query {
        let prompt = if empty_message.is_empty() {
            l10n.search_empty_prompt.as_ref()
        } else {
            empty_message
        };
        return Some(format!(
            r#"<div class="od-search-results-block"><p class="od-search-empty-prompt">{}</p></div>"#,
            escape_html(prompt),
        ));
    }

    let mut html = String::from(r#"<div class="od-search-results-block">"#);
    if show_facets && !view.facets.is_empty() {
        let all_active = if view.type_filter.is_none() {
            r#" aria-current="page""#
        } else {
            ""
        };
        let all_href = if is_preview {
            "#"
        } else {
            &view.clear_type_url
        };
        html.push_str(&format!(
            r#"<nav class="od-search-facets-nav" aria-label="{}"><ul class="od-search-facets"><li><a href="{}"{all_active}>{}</a></li>"#,
            escape_html(&l10n.search_title),
            escape_html(all_href),
            escape_html(&l10n.search_all_types),
        ));
        for facet in &view.facets {
            let active_attr = if facet.is_active {
                r#" aria-current="page""#
            } else {
                ""
            };
            let facet_href = if is_preview { "#" } else { &facet.url };
            html.push_str(&format!(
                r#"<li><a href="{}"{active_attr}>{} ({})</a></li>"#,
                escape_html(facet_href),
                escape_html(&facet.label),
                facet.count,
            ));
        }
        html.push_str("</ul></nav>");
    }

    html.push_str(&format!(
        r#"<p class="od-search-summary">{} {}</p>"#,
        escape_html(&l10n.search_results_summary),
        view.total_items,
    ));

    if search_ctx.entries.is_empty() {
        let empty = if empty_message.is_empty() {
            l10n.search_no_results.as_ref()
        } else {
            empty_message
        };
        html.push_str(&format!(
            r#"<p class="od-search-no-results">{}</p>"#,
            escape_html(empty),
        ));
    } else {
        html.push_str(r#"<div class="od-search-results">"#);
        for entry in search_ctx.entries {
            let href = if is_preview {
                "#"
            } else {
                entry.url.as_deref().unwrap_or("#")
            };
            html.push_str(r#"<article class="od-search-result">"#);
            if !entry.type_label.is_empty() {
                html.push_str(&format!(
                    r#"<span class="od-search-result-type">{}</span>"#,
                    escape_html(&entry.type_label),
                ));
            }
            html.push_str(&format!(
                r#"<h2 class="od-search-result-title"><a href="{}">{}</a></h2>"#,
                escape_html(href),
                escape_html(&entry.title),
            ));
            if show_excerpt {
                let snippet = render_snippet_segments_html(&entry.snippet_segments, &entry.excerpt);
                html.push_str(&format!(r#"<p class="od-search-snippet">{snippet}</p>"#));
            }
            html.push_str("</article>");
        }
        html.push_str("</div>");
    }

    let pagination = search_ctx.pagination;
    if pagination.has_prev || pagination.has_next {
        let page_nav_label = if locale.starts_with("en") {
            "Pagination"
        } else {
            "Phân trang"
        };
        html.push_str(&format!(
            r#"<nav class="od-search-pagination" aria-label="{}">"#,
            escape_html(page_nav_label),
        ));
        if let Some(prev) = pagination.previous_url.as_deref() {
            let href = if is_preview { "#" } else { prev };
            html.push_str(&format!(
                r#"<a href="{}">{}</a>"#,
                escape_html(href),
                escape_html(&l10n.list_prev),
            ));
        }
        if let Some(next) = pagination.next_url.as_deref() {
            let href = if is_preview { "#" } else { next };
            html.push_str(&format!(
                r#"<a href="{}">{}</a>"#,
                escape_html(href),
                escape_html(&l10n.list_next),
            ));
        }
        html.push_str("</nav>");
    }

    html.push_str("</div>");
    Some(html)
}
