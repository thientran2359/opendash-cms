use opendash_core::search::SnippetSegment;
use serde::{Deserialize, Serialize};

use crate::{
    content_render::{
        BreadcrumbItem, EntryView, PaginationView, SiteContext, core_l10n,
        document_compose::escape_html_attr,
    },
    public_context::build_search_url,
    storage::search::PublicSearchOutput,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchFacetView {
    pub type_key: String,
    pub label: String,
    pub count: i64,
    pub url: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchView {
    pub is_search_route: bool,
    pub has_query: bool,
    pub query: String,
    pub type_filter: Option<String>,
    pub action_url: String,
    pub clear_type_url: String,
    pub total_items: i64,
    pub facets: Vec<SearchFacetView>,
}

impl SearchView {
    pub fn inactive(search_url: &str) -> Self {
        let base = if search_url.trim().is_empty() {
            "/"
        } else {
            search_url.trim()
        };
        Self {
            is_search_route: false,
            has_query: false,
            query: String::new(),
            type_filter: None,
            action_url: base.to_owned(),
            clear_type_url: base.to_owned(),
            total_items: 0,
            facets: Vec::new(),
        }
    }

    pub fn inactive_for_locale_ctx(locale_ctx: &serde_json::Value) -> Self {
        let search_url = locale_ctx
            .get("search_url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("/");
        Self::inactive(search_url)
    }

    pub fn preview_sample(search_url: &str, locale: &str) -> Self {
        let base = if search_url.trim().is_empty() {
            "/"
        } else {
            search_url.trim()
        };
        let is_en = locale.starts_with("en");
        let sample_query = if is_en {
            "architecture"
        } else {
            "kiến trúc"
        };
        let (post_label, page_label) = if is_en {
            ("Posts", "Pages")
        } else {
            ("Bài viết", "Trang")
        };
        Self {
            is_search_route: true,
            has_query: true,
            query: sample_query.to_owned(),
            type_filter: None,
            action_url: base.to_owned(),
            clear_type_url: build_search_url(base, Some(sample_query), None, 1),
            total_items: 2,
            facets: vec![
                SearchFacetView {
                    type_key: "post".to_owned(),
                    label: post_label.to_owned(),
                    count: 1,
                    url: build_search_url(base, Some(sample_query), Some("post"), 1),
                    is_active: false,
                },
                SearchFacetView {
                    type_key: "page".to_owned(),
                    label: page_label.to_owned(),
                    count: 1,
                    url: build_search_url(base, Some(sample_query), Some("page"), 1),
                    is_active: false,
                },
            ],
        }
    }

    pub fn preview_sample_entries(locale: &str) -> Vec<EntryView> {
        let is_en = locale.starts_with("en");
        let (post_label, post_title, post_pre, post_match, post_post) = if is_en {
            (
                "Post",
                "OpenDash Search Architecture",
                "Lightweight SQLite FTS5 ",
                "architecture",
                " for instant full-site search.",
            )
        } else {
            (
                "Bài viết",
                "Kiến trúc tìm kiếm OpenDash",
                "Tìm kiếm toàn site với SQLite FTS5 và ",
                "kiến trúc",
                " lõi gọn nhẹ.",
            )
        };
        let (page_label, page_title, page_pre, page_match, page_post) = if is_en {
            (
                "Page",
                "Documentation Overview",
                "Modular system ",
                "architecture",
                " with zero external daemon.",
            )
        } else {
            (
                "Trang",
                "Tổng quan tài liệu",
                "Tài liệu hướng dẫn ",
                "kiến trúc",
                " không cần dịch vụ ngoài.",
            )
        };
        vec![
            EntryView {
                id: 0,
                type_key: "post".to_owned(),
                title: post_title.to_owned(),
                slug: "sample-post".to_owned(),
                url: Some("/posts/sample-post".to_owned()),
                parent: None,
                menu_order: 0,
                created_at: String::new(),
                updated_at: String::new(),
                published_at: None,
                excerpt: format!("{post_pre}{post_match}{post_post}"),
                reading_time_minutes: 2,
                fields: Default::default(),
                field_list: Vec::new(),
                cover_url: None,
                cover_media: None,
                cover: None,
                richtext_html: Default::default(),
                terms: Default::default(),
                all_terms: Vec::new(),
                primary_category: None,
                ext: Default::default(),
                theme_ext: Default::default(),
                snippet_segments: vec![
                    SnippetSegment {
                        text: post_pre.to_owned(),
                        matched: false,
                    },
                    SnippetSegment {
                        text: post_match.to_owned(),
                        matched: true,
                    },
                    SnippetSegment {
                        text: post_post.to_owned(),
                        matched: false,
                    },
                ],
                type_label: post_label.to_owned(),
            },
            EntryView {
                id: 0,
                type_key: "page".to_owned(),
                title: page_title.to_owned(),
                slug: "sample-page".to_owned(),
                url: Some("/sample-page".to_owned()),
                parent: None,
                menu_order: 0,
                created_at: String::new(),
                updated_at: String::new(),
                published_at: None,
                excerpt: format!("{page_pre}{page_match}{page_post}"),
                reading_time_minutes: 1,
                fields: Default::default(),
                field_list: Vec::new(),
                cover_url: None,
                cover_media: None,
                cover: None,
                richtext_html: Default::default(),
                terms: Default::default(),
                all_terms: Vec::new(),
                primary_category: None,
                ext: Default::default(),
                theme_ext: Default::default(),
                snippet_segments: vec![
                    SnippetSegment {
                        text: page_pre.to_owned(),
                        matched: false,
                    },
                    SnippetSegment {
                        text: page_match.to_owned(),
                        matched: true,
                    },
                    SnippetSegment {
                        text: page_post.to_owned(),
                        matched: false,
                    },
                ],
                type_label: page_label.to_owned(),
            },
        ]
    }

    pub fn from_public_output(output: &PublicSearchOutput, base_search_url: &str) -> Self {
        let base = if base_search_url.trim().is_empty() {
            "/"
        } else {
            base_search_url.trim()
        };
        let query_arg = if output.has_query {
            output.query.as_str()
        } else {
            ""
        };
        Self {
            is_search_route: true,
            has_query: output.has_query,
            query: output.query.clone(),
            type_filter: output.content_type.clone(),
            action_url: base.to_owned(),
            clear_type_url: build_search_url(base, Some(query_arg), None, 1),
            total_items: output.total_items,
            facets: output
                .facets
                .iter()
                .map(|facet| SearchFacetView {
                    type_key: facet.type_key.clone(),
                    label: facet.label.clone(),
                    count: facet.count,
                    url: build_search_url(base, Some(query_arg), Some(&facet.type_key), 1),
                    is_active: facet.is_active,
                })
                .collect(),
        }
    }
}

pub(crate) fn render_snippet_segments_html(
    segments: &[SnippetSegment],
    fallback_excerpt: &str,
) -> String {
    if segments.is_empty() {
        return escape_html_attr(fallback_excerpt);
    }
    let mut out = String::new();
    for segment in segments {
        let escaped = escape_html_attr(&segment.text);
        if segment.matched {
            out.push_str("<mark>");
            out.push_str(&escaped);
            out.push_str("</mark>");
        } else {
            out.push_str(&escaped);
        }
    }
    out
}

/// Core HTML fallback when neither the active theme nor a template override
/// supplies `search.html` or `index.html`.
pub fn render_default_search_html(
    _site: &SiteContext<'_>,
    search: &SearchView,
    entries: &[EntryView],
    pagination: &PaginationView,
    _breadcrumbs: &[BreadcrumbItem],
    locale: &str,
) -> String {
    let l10n = core_l10n(locale);
    let action = escape_html_attr(&search.action_url);
    let query_val = escape_html_attr(&search.query);
    let title = escape_html_attr(&l10n.search_title);
    let placeholder = escape_html_attr(&l10n.search_placeholder);
    let submit = escape_html_attr(&l10n.search_submit);

    let mut html = format!(
        r#"<main class="od-search-main" data-route="search"><section class="od-search-section"><h1>{title}</h1><search><form method="get" action="{action}" class="od-search-form"><input type="search" name="s" value="{query_val}" placeholder="{placeholder}" aria-label="{title}" autocorrect="off" autocapitalize="none" spellcheck="false">"#,
    );
    if let Some(type_filter) = search.type_filter.as_deref().filter(|t| !t.is_empty()) {
        html.push_str(&format!(
            r#"<input type="hidden" name="type" value="{}">"#,
            escape_html_attr(type_filter)
        ));
    }
    html.push_str(&format!(
        r#"<button type="submit">{submit}</button></form></search>"#
    ));

    if !search.has_query {
        html.push_str(&format!(
            r#"<p class="od-search-empty-prompt">{}</p></section></main>"#,
            escape_html_attr(&l10n.search_empty_prompt)
        ));
        return html;
    }

    if !search.facets.is_empty() {
        let all_label = escape_html_attr(&l10n.search_all_types);
        let clear_url = escape_html_attr(&search.clear_type_url);
        let all_active = if search.type_filter.is_none() {
            r#" aria-current="page""#
        } else {
            ""
        };
        html.push_str(&format!(
            r#"<nav aria-label="{title}"><ul class="od-search-facets"><li><a href="{clear_url}"{all_active}>{all_label}</a></li>"#
        ));
        for facet in &search.facets {
            let furl = escape_html_attr(&facet.url);
            let flabel = escape_html_attr(&facet.label);
            let active_attr = if facet.is_active {
                r#" aria-current="page""#
            } else {
                ""
            };
            html.push_str(&format!(
                r#"<li><a href="{furl}"{active_attr}>{flabel} ({})</a></li>"#,
                facet.count
            ));
        }
        html.push_str("</ul></nav>");
    }

    html.push_str(&format!(
        r#"<p class="od-search-summary">{} {}</p>"#,
        escape_html_attr(&l10n.search_results_summary),
        search.total_items
    ));

    if entries.is_empty() {
        html.push_str(&format!(
            r#"<p class="od-search-no-results">{}</p>"#,
            escape_html_attr(&l10n.search_no_results)
        ));
    } else {
        html.push_str(r#"<div class="od-search-results">"#);
        for entry in entries {
            let entry_url = escape_html_attr(entry.url.as_deref().unwrap_or("#"));
            let entry_title = escape_html_attr(&entry.title);
            let snippet = render_snippet_segments_html(&entry.snippet_segments, &entry.excerpt);
            html.push_str("<article class=\"od-search-result\">");
            if !entry.type_label.is_empty() {
                html.push_str(&format!(
                    r#"<span class="od-search-result-type">{}</span>"#,
                    escape_html_attr(&entry.type_label)
                ));
            }
            html.push_str(&format!(
                r#"<h2><a href="{entry_url}">{entry_title}</a></h2><p class="od-search-snippet">{snippet}</p></article>"#
            ));
        }
        html.push_str("</div>");
    }

    if pagination.has_prev || pagination.has_next {
        let nav_label = if locale.starts_with("en") {
            "Pagination"
        } else {
            "Phân trang"
        };
        html.push_str(&format!(r#"<nav aria-label="{nav_label}">"#));
        if let Some(prev) = pagination.previous_url.as_deref() {
            html.push_str(&format!(
                r#"<a href="{}">{}</a>"#,
                escape_html_attr(prev),
                escape_html_attr(&l10n.list_prev)
            ));
        }
        if let Some(next) = pagination.next_url.as_deref() {
            html.push_str(&format!(
                r#"<a href="{}">{}</a>"#,
                escape_html_attr(next),
                escape_html_attr(&l10n.list_next)
            ));
        }
        html.push_str("</nav>");
    }

    html.push_str("</section></main>");
    html
}

/// Synchronizes `theme_context["locale"]` and `theme_context["search"]` URLs
/// with the resolved preview locale's `search_url` without duplicating JSON glue.
pub fn sync_preview_locale_and_search(
    theme_context: &mut serde_json::Value,
    locale_ctx: &serde_json::Value,
) {
    let search_url = locale_ctx
        .get("search_url")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("/");
    let Some(root) = theme_context.as_object_mut() else {
        return;
    };
    root.insert("locale".to_owned(), locale_ctx.clone());
    let Some(search_obj) = root
        .get_mut("search")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    search_obj.insert(
        "action_url".to_owned(),
        serde_json::Value::String(search_url.to_owned()),
    );
    let has_query = search_obj
        .get("has_query")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let query = search_obj
        .get("query")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned();
    let clear_url = if has_query {
        build_search_url(search_url, Some(&query), None, 1)
    } else {
        search_url.to_owned()
    };
    search_obj.insert(
        "clear_type_url".to_owned(),
        serde_json::Value::String(clear_url),
    );
    if let Some(facets) = search_obj
        .get_mut("facets")
        .and_then(serde_json::Value::as_array_mut)
    {
        for facet in facets {
            if let Some(fobj) = facet.as_object_mut()
                && let Some(type_key) = fobj
                    .get("type_key")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            {
                let facet_url = build_search_url(
                    search_url,
                    has_query.then_some(query.as_str()),
                    Some(&type_key),
                    1,
                );
                fobj.insert("url".to_owned(), serde_json::Value::String(facet_url));
            }
        }
    }
}
