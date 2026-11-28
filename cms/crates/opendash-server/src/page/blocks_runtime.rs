use opendash_core::builder::{BlockNode, parse_positive_i64_prop};
use serde_json::Value;

use super::{
    blocks_archive::render_archive_block,
    blocks_chrome::render_chrome_block,
    blocks_entry::render_entry_block_with_context,
    blocks_layout::responsive_i64_cascade,
    cache_types::{BuilderDiagnostic, BuilderTemplateContext, ResolvedList},
    html_helpers::list_page_href,
    render::{ContactFormResults, PatternResults, escape_html},
    render_contact_form_html, render_search_form_block, render_search_results_block,
};
use crate::{
    error::ApiResult,
    storage::builder::{BuilderRenderContext, ListResults},
    template_sec::RenderBudget,
};

pub(super) struct RuntimeBlockOutput {
    pub html: Option<String>,
    pub already_charged: usize,
    pub used_entry_content: bool,
    pub has_popup: bool,
}

fn render_list(
    node: &BlockNode,
    is_preview: bool,
    lists: &ListResults,
    list_base_path: &str,
    budget: &RenderBudget,
    diagnostics: &mut Vec<BuilderDiagnostic>,
) -> Option<String> {
    let l10n = crate::content_render::core_l10n(&budget.locale);
    let type_key = node
        .props
        .get("type_key")
        .and_then(Value::as_str)
        .unwrap_or("post");
    match lists.get(&node.id) {
        Some(ResolvedList::MissingConfig) => {
            diagnostics.push(BuilderDiagnostic {
                node_id: node.id.clone(),
                code: "list_config_unavailable",
            });
            Some(format!(
                "<p class=\"builder-list-empty\">{}</p>",
                l10n.list_unavailable
            ))
        }
        Some(ResolvedList::Page {
            items,
            page,
            has_next,
            out_of_range,
            ..
        }) => {
            let mut out = format!(
                "<div class=\"builder-list\" data-list-type=\"{}\">",
                escape_html(type_key),
            );
            if *out_of_range {
                out.push_str(&format!("<p>{}</p>", l10n.page_empty));
                out.push_str(&format!(
                    "<a class=\"builder-list-first\" href=\"{}\">{}</a>",
                    escape_html(&list_page_href(list_base_path, lists, &node.id, 1)),
                    l10n.back_to_first_page,
                ));
            } else if items.is_empty() {
                out.push_str(&format!(
                    "<p class=\"builder-list-empty\">{}</p>",
                    l10n.empty_content,
                ));
            } else {
                let layout = node
                    .props
                    .get("layout")
                    .and_then(Value::as_str)
                    .unwrap_or("grid");
                let list_cls = if layout == "list" {
                    "flex flex-col list-none p-0 gap-4".to_owned()
                } else {
                    let (is_resp, d_r, t_r, m_r, _) =
                        responsive_i64_cascade(&node.props, "columns", 3);
                    let (d, t, m) = (d_r.clamp(1, 4), t_r.clamp(1, 4), m_r.clamp(1, 4));
                    let mut cls = vec![
                        "grid".to_owned(),
                        "list-none".to_owned(),
                        "p-0".to_owned(),
                        "gap-4".to_owned(),
                    ];
                    if !is_resp {
                        if d > 1 {
                            cls.push("grid-cols-1".to_owned());
                            cls.push(format!("md:grid-cols-{d}"));
                        } else {
                            cls.push("grid-cols-1".to_owned());
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
                    cls.join(" ")
                };
                let show_img = node
                    .props
                    .get("show_image")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let show_date = node
                    .props
                    .get("show_date")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let show_excerpt = node
                    .props
                    .get("show_excerpt")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let show_taxonomy = node
                    .props
                    .get("show_taxonomy")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let readmore = node
                    .props
                    .get("readmore_label")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or("Đọc tiếp");

                out.push_str(&format!("<ul class=\"{list_cls}\">"));
                for item in items {
                    out.push_str(
                        "<li class=\"rounded-xl border border-od-border bg-od-surface p-4\">",
                    );
                    if show_img && let Some(url) = &item.cover_url {
                        let cover = if is_preview {
                            url.replacen("/media/", "/api/admin/media/", 1)
                        } else {
                            url.clone()
                        };
                        out.push_str(&format!(
                            "<img src=\"{}\" alt=\"{}\" loading=\"lazy\" class=\"w-full aspect-video object-cover rounded-lg mb-3\">",
                            escape_html(&cover),
                            escape_html(&item.title),
                        ));
                    }
                    if show_taxonomy && !item.terms.is_empty() {
                        out.push_str(
                            "<div class=\"flex flex-wrap gap-1.5 mb-2 text-xs text-od-muted\">",
                        );
                        for term in &item.terms {
                            if let Some(turl) = &term.url {
                                out.push_str(&format!(
                                    "<a class=\"hover:underline\" href=\"{}\">{}</a>",
                                    escape_html(turl),
                                    escape_html(&term.name),
                                ));
                            } else {
                                out.push_str(&format!("<span>{}</span>", escape_html(&term.name)));
                            }
                        }
                        out.push_str("</div>");
                    }
                    if let Some(path) = &item.href {
                        out.push_str(&format!(
                            "<a class=\"font-semibold text-od-text hover:text-od-primary\" href=\"{}\">{}</a>",
                            escape_html(path),
                            escape_html(&item.title),
                        ));
                    } else {
                        out.push_str(&format!(
                            "<span class=\"font-semibold text-od-text\">{}</span>",
                            escape_html(&item.title),
                        ));
                    }
                    if show_date && let Some(dt) = &item.published_at {
                        out.push_str(&format!(
                            "<time class=\"block text-xs text-od-muted mt-1\">{}</time>",
                            escape_html(dt),
                        ));
                    }
                    if show_excerpt && let Some(excerpt) = &item.excerpt {
                        out.push_str(&format!(
                            "<p class=\"text-sm text-od-muted mt-2\">{}</p>",
                            escape_html(excerpt),
                        ));
                    }
                    if !readmore.is_empty()
                        && let Some(path) = &item.href
                    {
                        out.push_str(&format!(
                            "<a class=\"inline-block text-sm font-medium text-od-primary mt-2 hover:underline\" href=\"{}\">{}</a>",
                            escape_html(path),
                            escape_html(readmore),
                        ));
                    }
                    out.push_str("</li>");
                }
                out.push_str("</ul>");
                if !is_preview && *page > 1 {
                    out.push_str(&format!(
                        "<a class=\"builder-list-prev\" href=\"{}\">{}</a>",
                        escape_html(&list_page_href(list_base_path, lists, &node.id, page - 1)),
                        l10n.list_prev,
                    ));
                }
                if !is_preview && *has_next && *page < 100 {
                    out.push_str(&format!(
                        "<a class=\"builder-list-next\" href=\"{}\">{}</a>",
                        escape_html(&list_page_href(list_base_path, lists, &node.id, page + 1)),
                        l10n.list_next,
                    ));
                }
            }
            out.push_str("</div>");
            Some(out)
        }
        None => {
            diagnostics.push(BuilderDiagnostic {
                node_id: node.id.clone(),
                code: "list_unavailable",
            });
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_native_runtime(
    state: &BuilderRenderContext,
    node: &BlockNode,
    scope: &str,
    is_preview: bool,
    lists: &ListResults,
    forms: &ContactFormResults,
    context: Option<&BuilderTemplateContext<'_>>,
    patterns: &PatternResults,
    list_base_path: &str,
    in_popup: bool,
    budget: &mut RenderBudget,
    has_active_contact_form: &mut bool,
    diagnostics: &mut Vec<BuilderDiagnostic>,
) -> ApiResult<Option<RuntimeBlockOutput>> {
    let td = context.and_then(|c| c.template_data);
    if let Some(html) = render_chrome_block(node, scope, td, is_preview, budget)? {
        return Ok(Some(RuntimeBlockOutput {
            html,
            already_charged: 0,
            used_entry_content: false,
            has_popup: false,
        }));
    }
    if let Some((html, already_charged)) = render_entry_block_with_context(
        node,
        None,
        None,
        &state.content_schemas,
        false,
        td,
        context.and_then(|c| c.content_html),
        is_preview,
        &state.timezone,
        budget,
    )? {
        return Ok(Some(RuntimeBlockOutput {
            used_entry_content: node.block_id == "core/entry-content" && html.is_some(),
            has_popup: false,
            html,
            already_charged,
        }));
    }
    if let Some(html) = render_archive_block(node, td, is_preview, budget) {
        return Ok(Some(RuntimeBlockOutput {
            html,
            already_charged: 0,
            used_entry_content: false,
            has_popup: false,
        }));
    }

    match node.block_id.as_str() {
        "core/list" => Ok(Some(RuntimeBlockOutput {
            html: render_list(node, is_preview, lists, list_base_path, budget, diagnostics),
            already_charged: 0,
            used_entry_content: false,
            has_popup: false,
        })),
        "core/search-form" => Ok(Some(RuntimeBlockOutput {
            html: Some(render_search_form_block(
                state,
                node,
                context,
                is_preview,
                &budget.locale,
            )),
            already_charged: 0,
            used_entry_content: false,
            has_popup: false,
        })),
        "core/search-results" => Ok(Some(RuntimeBlockOutput {
            html: render_search_results_block(
                node,
                context,
                is_preview,
                &budget.locale,
                diagnostics,
            ),
            already_charged: 0,
            used_entry_content: false,
            has_popup: false,
        })),
        "core/contact-form" => {
            let l10n = crate::content_render::core_l10n(&budget.locale);
            let submit_label = node
                .props
                .get("submit_label")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("Gửi liên hệ");
            let compact = node.props.get("style").and_then(Value::as_str) == Some("compact");
            let html = match forms.get(&node.id).and_then(|opt| opt.as_ref()) {
                Some(form) if form.status == "active" => {
                    let rendered =
                        render_contact_form_html(&node.id, form, submit_label, is_preview, &l10n);
                    if !is_preview {
                        *has_active_contact_form = true;
                    }
                    if compact {
                        format!("<div class=\"od-contact-compact\">{rendered}</div>")
                    } else {
                        rendered
                    }
                }
                _ => {
                    diagnostics.push(BuilderDiagnostic {
                        node_id: node.id.clone(),
                        code: "contact_form_unavailable",
                    });
                    format!(
                        "<p class=\"builder-contact-unavailable\">{}</p>",
                        l10n.contact_unavailable
                    )
                }
            };
            Ok(Some(RuntimeBlockOutput {
                html: Some(html),
                already_charged: 0,
                used_entry_content: false,
                has_popup: false,
            }))
        }
        "core/pattern-ref" => {
            let pid = parse_positive_i64_prop(node.props.get("pattern_id")).unwrap_or(0);
            match patterns.get(&node.id) {
                Some(pattern) => {
                    if in_popup && pattern.has_popup {
                        diagnostics.push(BuilderDiagnostic {
                            node_id: node.id.clone(),
                            code: "nested_popup_not_allowed",
                        });
                        return Ok(Some(RuntimeBlockOutput {
                            html: None,
                            already_charged: 0,
                            used_entry_content: false,
                            has_popup: false,
                        }));
                    }
                    *has_active_contact_form |= pattern.has_active_contact_form;
                    budget.needs_blocks_js |= pattern.needs_blocks_js;
                    Ok(Some(RuntimeBlockOutput {
                        html: Some(format!(
                            "<div class=\"od-pattern-ref\" data-pattern-id=\"{pid}\">{}</div>",
                            pattern.html
                        )),
                        already_charged: pattern.html.len(),
                        used_entry_content: pattern.used_entry_content,
                        has_popup: pattern.has_popup,
                    }))
                }
                None => {
                    diagnostics.push(BuilderDiagnostic {
                        node_id: node.id.clone(),
                        code: "pattern_missing",
                    });
                    let html = is_preview.then(|| {
                        let l10n = crate::content_render::core_l10n(&budget.locale);
                        format!(
                            "<div class=\"od-pattern-missing\" role=\"status\">{}</div>",
                            l10n.pattern_missing
                        )
                    });
                    Ok(Some(RuntimeBlockOutput {
                        html,
                        already_charged: 0,
                        used_entry_content: false,
                        has_popup: false,
                    }))
                }
            }
        }
        "core/template-part" => {
            let key = node
                .props
                .get("part_key")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            match context.and_then(|ctx| ctx.parts.get(key)) {
                Some(part_html) => Ok(Some(RuntimeBlockOutput {
                    html: Some(format!(
                        "<div class=\"od-template-part\" data-part-key=\"{}\">{}</div>",
                        escape_html(key),
                        part_html
                    )),
                    already_charged: part_html.len(),
                    used_entry_content: false,
                    has_popup: false,
                })),
                None => {
                    diagnostics.push(BuilderDiagnostic {
                        node_id: node.id.clone(),
                        code: "template_part_unavailable",
                    });
                    Ok(Some(RuntimeBlockOutput {
                        html: None,
                        already_charged: 0,
                        used_entry_content: false,
                        has_popup: false,
                    }))
                }
            }
        }
        _ => Ok(None),
    }
}
