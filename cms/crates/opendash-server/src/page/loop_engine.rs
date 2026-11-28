use std::collections::BTreeMap;

use opendash_core::{
    builder::{BlockNode, MAX_LOOP_ITEMS_PER_REQUEST},
    content::EffectiveContentSchema,
    field_schema::valid_safe_url,
    registry::{DeclarationState, ProviderStatus},
};
use serde_json::Value;

use super::{
    blocks_archive::{grid_cols_classes, node_extra_classes},
    blocks_chrome::bool_prop,
    blocks_entry::{radius_class, render_entry_block_with_context},
    blocks_layout::{NativeParentInfo, render_native_container, str_prop},
    blocks_leaf::render_native_leaf,
    cache_types::{BuilderDiagnostic, BuilderTemplateContext, ResolvedList},
    html_helpers::list_page_href,
    render::escape_html,
    root_wrapper::{RootAttrs, apply_single_root_wrapper},
    scoped_css::loop_node_scope,
};
use crate::{
    content_render::{EntryView, core_l10n},
    error::{ApiError, ApiResult},
    storage::{
        builder::{BuilderRenderContext, ListResults},
        page::RegisteredBlockDef,
    },
    template_sec::{BlockRenderInputs, LoopBlockContext, MAX_RENDERED_HTML_BYTES, RenderBudget},
};

fn render_loop_csec_leaf(
    node: &BlockNode,
    def: &RegisteredBlockDef,
    state: &BuilderRenderContext,
    entry: &EntryView,
    entry_json: &Value,
    loop_index: usize,
    loop_count: usize,
    is_preview: bool,
    cache: &super::BlockTemplateCache,
    budget: &mut RenderBudget,
) -> ApiResult<String> {
    if !def.schema.schema.is_loop_schema_compatible() {
        return Ok(String::new());
    }
    let hydration_ctx = crate::field_hydrate::HydrationContext {
        media: &state.hydration_media,
        entries: &state.hydration_entries,
        terms: &state.hydration_terms,
        is_preview,
        tag_media_provenance: true,
    };
    let defaulted = def.schema.schema.apply_field_defaults(&node.props);
    let mut local_rt = crate::template_sec::RichTextNonceTable::default();
    let res = cache
        .get_or_validate(&node.block_id, &def.schema.schema, &def.template)
        .and_then(|compiled| {
            let hydrated = crate::field_hydrate::hydrate_field_values(
                &def.schema.schema.fields,
                &defaulted,
                &hydration_ctx,
                Some(&mut local_rt),
                crate::field_hydrate::HydrationVisibility::All,
            )?;
            let fmt_cfg = crate::template_sec::RenderFormattingConfig {
                timezone: state.timezone.clone(),
                ..Default::default()
            };
            let menus_opt = (!state.menus.is_empty()).then_some(&state.menus);
            crate::template_sec::render_block_csec_compiled(
                &compiled.block_id,
                &compiled.base_env,
                BlockRenderInputs {
                    hydrated_props: &hydrated,
                    local_richtext_nonces: Some(local_rt),
                    merged_options: Some(&state.options),
                    site_options: Some(&state.site_options),
                    menus: menus_opt,
                    children_html: None,
                    children_meta: None,
                    parent_ctx: None,
                    is_preview,
                    fmt_cfg: &fmt_cfg,
                    loop_context: Some(LoopBlockContext {
                        entry: entry_json,
                        loop_index,
                        loop_count,
                    }),
                },
                budget,
            )
        });
    match res {
        Ok(html) => Ok(html),
        Err(_) if budget.html_bytes_used > MAX_RENDERED_HTML_BYTES => Err(ApiError::bad_request(
            "Builder template composition exceeds 512 KiB.",
        )),
        Err(_) => {
            let _ = entry;
            Ok(String::new())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_loop_subtree_node(
    node: &BlockNode,
    loop_item_id: &str,
    scope: &str,
    state: &BuilderRenderContext,
    entry: &EntryView,
    entry_schema: Option<&EffectiveContentSchema>,
    entry_json: &Value,
    loop_index: usize,
    loop_count: usize,
    is_clone: bool,
    is_preview: bool,
    allow_editable: bool,
    native_parent: NativeParentInfo<'_>,
    cache: &super::BlockTemplateCache,
    budget: &mut RenderBudget,
    context: Option<&BuilderTemplateContext<'_>>,
) -> ApiResult<String> {
    let Some(def) = state.declared_blocks.get(&node.block_id) else {
        return Ok(String::new());
    };
    if def.status != ProviderStatus::Active || def.declaration_state != DeclarationState::Declared {
        return Ok(String::new());
    }
    let node_scope = loop_node_scope(scope, Some(loop_item_id), &node.id);
    let extra_classes = node_extra_classes(node);
    let kind = if is_preview && !is_clone && allow_editable {
        Some(if def.schema.schema.supports_children {
            "container"
        } else {
            "block"
        })
    } else {
        None
    };
    let attrs = RootAttrs {
        node_id: &node.id,
        block_id: &node.block_id,
        node_scope: &node_scope,
        html_id: None,
        extra_classes: &extra_classes,
        kind,
        is_loop_clone: is_clone,
    };

    if def.schema.schema.supports_children {
        if !node.block_id.starts_with("core/") {
            return Ok(String::new());
        }
        let nonce =
            crate::template_sec::generate_children_nonce().map_err(ApiError::bad_request)?;
        let Some(shell) = render_native_container(
            node,
            &def.schema.schema.fields,
            &[],
            &nonce,
            &node_scope,
            native_parent,
            is_preview,
            budget,
        )?
        else {
            return Ok(String::new());
        };
        let active_cnt = node
            .children
            .iter()
            .filter(|c| state.declared_blocks.contains_key(&c.block_id))
            .count();
        let child_parent = if node.block_id == "core/row" {
            let defaulted = def.schema.schema.apply_field_defaults(&node.props);
            let mode = match defaulted.get("layout_mode").and_then(Value::as_str) {
                Some("grid_12") => "grid_12",
                _ => "equal",
            };
            NativeParentInfo {
                layout_mode: Some(mode),
                active_child_count: active_cnt,
                ..Default::default()
            }
        } else {
            NativeParentInfo {
                active_child_count: active_cnt,
                ..Default::default()
            }
        };
        let mut children_html = String::new();
        for child in &node.children {
            children_html.push_str(&render_loop_subtree_node(
                child,
                loop_item_id,
                scope,
                state,
                entry,
                entry_schema,
                entry_json,
                loop_index,
                loop_count,
                is_clone,
                is_preview,
                allow_editable,
                child_parent,
                cache,
                budget,
                context,
            )?);
        }
        let replaced = shell.replace(&nonce, &children_html);
        let wrapped = apply_single_root_wrapper(&replaced, &attrs, is_preview);
        return Ok(wrapped);
    }

    let td = context.and_then(|c| c.template_data);
    let raw = if let Some((maybe_entry_html, _)) = render_entry_block_with_context(
        node,
        Some(entry),
        entry_schema,
        &state.content_schemas,
        true,
        td,
        None,
        is_preview,
        &state.timezone,
        budget,
    )? {
        maybe_entry_html
    } else if def.schema.schema.render == "native" {
        render_native_leaf(
            node,
            &def.schema.schema.fields,
            &state.hydration_media,
            is_preview,
            native_parent,
            budget,
        )?
    } else {
        Some(render_loop_csec_leaf(
            node, def, state, entry, entry_json, loop_index, loop_count, is_preview, cache, budget,
        )?)
    };

    let Some(frag) = raw else {
        return Ok(String::new());
    };
    Ok(apply_single_root_wrapper(&frag, &attrs, is_preview))
}

#[allow(clippy::too_many_arguments)]
fn render_loop_item_iterations(
    loop_item: &BlockNode,
    entries: &[EntryView],
    entry_schema: Option<&EffectiveContentSchema>,
    state: &BuilderRenderContext,
    scope: &str,
    is_preview: bool,
    allow_editable: bool,
    cache: &super::BlockTemplateCache,
    budget: &mut RenderBudget,
    context: Option<&BuilderTemplateContext<'_>>,
    diagnostics: &mut Vec<BuilderDiagnostic>,
) -> ApiResult<String> {
    let layout = str_prop(&loop_item.props, "layout", "vertical");
    let surface = str_prop(&loop_item.props, "surface_style", "bordered");
    let radius = radius_class(str_prop(&loop_item.props, "radius", "lg"));
    let card_link = bool_prop(&loop_item.props, "card_link_to_entry", false);
    let has_bg = loop_item
        .props
        .get("bg_color")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|s| !s.is_empty());
    let padding_attr = " data-od-spacing=\"padding\"";
    let surface_cls = match surface {
        "flat" => "bg-transparent",
        "elevated" => "bg-od-surface shadow-sm border border-od-border",
        _ => "bg-od-surface border border-od-border",
    };

    let mut out = String::new();
    let loop_count = entries.len();
    let item_scope = loop_node_scope(scope, Some(&loop_item.id), &loop_item.id);
    let item_extra = node_extra_classes(loop_item);

    for (idx, entry) in entries.iter().enumerate() {
        if budget.loop_items_rendered >= MAX_LOOP_ITEMS_PER_REQUEST {
            diagnostics.push(BuilderDiagnostic {
                node_id: loop_item.id.clone(),
                code: "loop_item_limit_reached",
            });
            break;
        }
        budget.loop_items_rendered += 1;

        let is_clone = idx > 0;
        let entry_json = serde_json::to_value(entry).unwrap_or(Value::Null);
        let mut children_html = String::new();
        for child in &loop_item.children {
            children_html.push_str(&render_loop_subtree_node(
                child,
                &loop_item.id,
                scope,
                state,
                entry,
                entry_schema,
                &entry_json,
                idx,
                loop_count,
                is_clone,
                is_preview,
                allow_editable,
                NativeParentInfo::default(),
                cache,
                budget,
                context,
            )?);
        }

        let mut cls = vec![
            "od-loop-item".to_owned(),
            surface_cls.to_owned(),
            radius.to_owned(),
            "flex".to_owned(),
            "flex-col".to_owned(),
            "gap-3".to_owned(),
        ];
        if layout == "horizontal" {
            cls.push("od-loop-item-horizontal".to_owned());
        }
        let mut stretched = String::new();
        if card_link
            && !is_preview
            && let Some(u) = entry
                .url
                .as_deref()
                .map(str::trim)
                .filter(|u| valid_safe_url(u))
        {
            cls.push("relative".to_owned());
            cls.push("od-loop-card-linked".to_owned());
            stretched = format!(
                "<a class=\"od-loop-stretched-link after:absolute after:inset-0 after:z-10\" href=\"{}\" aria-hidden=\"true\" tabindex=\"-1\"></a>",
                escape_html(u)
            );
        }
        let style_attr = if has_bg {
            " style=\"background-color:var(--od-li-bg);\""
        } else {
            ""
        };
        let article = format!(
            "<article class=\"{}\"{padding_attr}{style_attr}>{stretched}{children_html}</article>",
            cls.join(" "),
        );
        let attrs = RootAttrs {
            node_id: &loop_item.id,
            block_id: &loop_item.block_id,
            node_scope: &item_scope,
            html_id: None,
            extra_classes: &item_extra,
            kind: (is_preview && !is_clone && allow_editable).then_some("container"),
            is_loop_clone: is_clone,
        };
        let wrapped = apply_single_root_wrapper(&article, &attrs, is_preview);
        out.push_str(&wrapped);
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_loop_node_wrapper(
    node: &BlockNode,
    attrs: &RootAttrs<'_>,
    state: &BuilderRenderContext,
    scope: &str,
    lists: &ListResults,
    list_base_path: &str,
    context: Option<&BuilderTemplateContext<'_>>,
    is_preview: bool,
    allow_editable: bool,
    cache: &super::BlockTemplateCache,
    budget: &mut RenderBudget,
    diagnostics: &mut Vec<BuilderDiagnostic>,
    out: &mut String,
) -> ApiResult<()> {
    let rendered_opt = render_loop_container_block(
        node,
        state,
        scope,
        lists,
        list_base_path,
        context,
        is_preview,
        allow_editable,
        cache,
        budget,
        diagnostics,
    )?;
    if let Some(frag) = rendered_opt {
        let wrapped = apply_single_root_wrapper(&frag, attrs, is_preview);
        budget
            .charge_html_bytes(wrapped.len())
            .map_err(ApiError::bad_request)?;
        out.push_str(&wrapped);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_loop_container_block(
    node: &BlockNode,
    state: &BuilderRenderContext,
    scope: &str,
    lists: &ListResults,
    list_base_path: &str,
    context: Option<&BuilderTemplateContext<'_>>,
    is_preview: bool,
    allow_editable: bool,
    cache: &super::BlockTemplateCache,
    budget: &mut RenderBudget,
    diagnostics: &mut Vec<BuilderDiagnostic>,
) -> ApiResult<Option<String>> {
    let Some(loop_item) = node
        .children
        .iter()
        .find(|c| c.block_id == "core/loop-item")
    else {
        return Ok(None);
    };
    let l10n = core_l10n(&budget.locale);
    let td = context.and_then(|c| c.template_data);

    match node.block_id.as_str() {
        "core/archive-loop" => {
            let entries = td.map(|d| d.archive_entries).unwrap_or(&[]);
            let capped = &entries[..entries.len().min(24)];
            if capped.is_empty() {
                let empty_msg = str_prop(&node.props, "empty_message", l10n.empty_content.as_ref());
                return Ok(Some(format!(
                    "<div class=\"od-archive-loop\"><p class=\"od-archive-empty text-sm text-od-muted\">{}</p></div>",
                    escape_html(empty_msg),
                )));
            }
            let cards = render_loop_item_iterations(
                loop_item,
                capped,
                None,
                state,
                scope,
                is_preview,
                allow_editable,
                cache,
                budget,
                context,
                diagnostics,
            )?;
            let mut grid_cls = vec!["od-archive-loop".to_owned()];
            grid_cls.extend(grid_cols_classes(&node.props, 3));
            Ok(Some(format!(
                "<div class=\"{}\" style=\"gap:var(--od-loop-gap,24px);\">{cards}</div>",
                grid_cls.join(" "),
            )))
        }
        "core/entry-related" => {
            let limit = usize::try_from(
                node.props
                    .get("limit")
                    .and_then(Value::as_i64)
                    .unwrap_or(4)
                    .clamp(2, 8),
            )
            .unwrap_or(4);
            let related = td.map(|d| d.related_entries).unwrap_or(&[]);
            let capped = &related[..related.len().min(limit)];
            if capped.is_empty() {
                return Ok(is_preview.then(|| {
                    "<div class=\"od-tpl-placeholder rounded border border-dashed border-od-border px-3 py-2 text-xs text-od-muted\">Related Entries</div>".to_owned()
                }));
            }
            let heading = str_prop(&node.props, "heading", l10n.related_entries.as_ref());
            let cards = render_loop_item_iterations(
                loop_item,
                capped,
                None,
                state,
                scope,
                is_preview,
                allow_editable,
                cache,
                budget,
                context,
                diagnostics,
            )?;
            let mut grid_cls = grid_cols_classes(&node.props, 4);
            grid_cls.insert(0, "od-related-grid".to_owned());
            Ok(Some(format!(
                "<section class=\"od-entry-related space-y-4\"><h2 class=\"text-xl font-bold tracking-tight\">{}</h2><div class=\"{}\" style=\"gap:var(--od-rel-gap,24px);\">{cards}</div></section>",
                escape_html(heading),
                grid_cls.join(" "),
            )))
        }
        "core/list" => {
            let Some(ResolvedList::Page {
                page,
                has_next,
                out_of_range,
                hydrated_entries,
                entry_schema,
                ..
            }) = lists.get(&node.id)
            else {
                diagnostics.push(BuilderDiagnostic {
                    node_id: node.id.clone(),
                    code: "list_unavailable",
                });
                return Ok(Some(format!(
                    "<p class=\"builder-list-empty\">{}</p>",
                    escape_html(l10n.list_unavailable.as_ref())
                )));
            };
            let entries = hydrated_entries.as_deref().unwrap_or(&[]);
            let mut out = String::from("<div class=\"builder-list builder-list-loop space-y-4\">");
            if *out_of_range {
                out.push_str(&format!("<p>{}</p>", escape_html(l10n.page_empty.as_ref())));
                out.push_str(&format!(
                    "<a class=\"builder-list-first\" href=\"{}\">{}</a>",
                    escape_html(&list_page_href(list_base_path, lists, &node.id, 1)),
                    escape_html(l10n.back_to_first_page.as_ref()),
                ));
            } else if entries.is_empty() {
                out.push_str(&format!(
                    "<p class=\"builder-list-empty\">{}</p>",
                    escape_html(l10n.empty_content.as_ref()),
                ));
            } else {
                let cards = render_loop_item_iterations(
                    loop_item,
                    entries,
                    entry_schema.as_ref(),
                    state,
                    scope,
                    is_preview,
                    allow_editable,
                    cache,
                    budget,
                    context,
                    diagnostics,
                )?;
                let grid_cls = grid_cols_classes(&node.props, 3);
                out.push_str(&format!(
                    "<div class=\"{}\" style=\"gap:16px;\">{cards}</div>",
                    grid_cls.join(" ")
                ));
                if *page > 1 || *has_next {
                    out.push_str("<div class=\"builder-list-nav flex gap-4 text-sm\">");
                    if *page > 1 {
                        out.push_str(&format!(
                            "<a href=\"{}\">{}</a>",
                            escape_html(&list_page_href(list_base_path, lists, &node.id, page - 1)),
                            escape_html(l10n.list_prev.as_ref()),
                        ));
                    }
                    if *has_next {
                        out.push_str(&format!(
                            "<a href=\"{}\">{}</a>",
                            escape_html(&list_page_href(list_base_path, lists, &node.id, page + 1)),
                            escape_html(l10n.list_next.as_ref()),
                        ));
                    }
                    out.push_str("</div>");
                }
            }
            out.push_str("</div>");
            Ok(Some(out))
        }
        _ => Ok(None),
    }
}
