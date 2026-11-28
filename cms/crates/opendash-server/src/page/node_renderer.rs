use std::collections::BTreeMap;

use opendash_core::{
    builder::{BlockNode, BuilderDocument},
    field_schema::{FieldSpec, FieldType},
    registry::{DeclarationState, ProviderStatus},
};
use serde_json::{Value, json};

use super::{
    blocks_layout::{NativeParentInfo, render_native_container},
    blocks_leaf::render_native_leaf,
    blocks_runtime::render_native_runtime,
    cache_types::{
        BlockTemplateCache, BuilderDiagnostic, BuilderTemplateContext, RenderAnnotationScope,
        RenderedLayout,
    },
    loop_engine::render_loop_node_wrapper,
    render::{ContactFormResults, PatternResults, escape_html},
    root_wrapper::{RootAttrs, apply_single_root_wrapper},
    scoped_css::{builder_visibility_css, loop_node_scope, serialize_scoped_block_css},
    slot_table::{HtmlSlotTable, assemble_slot_table, scan_child_nonce_in_shell},
};
use crate::{
    error::{ApiError, ApiResult},
    storage::builder::{BuilderRenderContext, ListResults},
    storage::page::RegisteredBlockDef,
    template_sec::{MAX_RENDERED_HTML_BYTES, RenderBudget},
};

struct RenderEnv<'a> {
    state: &'a BuilderRenderContext,
    is_preview: bool,
    lists: &'a ListResults,
    forms: &'a ContactFormResults,
    context: Option<&'a BuilderTemplateContext<'a>>,
    patterns: &'a PatternResults,
    pattern_document: bool,
    annotation_scope: RenderAnnotationScope,
    scope: &'a str,
    list_base_path: &'a str,
    cache: &'a BlockTemplateCache,
    budget: &'a mut RenderBudget,
    slot_table: HtmlSlotTable,
    diagnostics: Vec<BuilderDiagnostic>,
    has_active_contact_form: bool,
    used_entry_content: bool,
    has_popup: bool,
}

fn node_preview_kind(
    env: &RenderEnv<'_>,
    block_id: &str,
    supports_children: bool,
) -> Option<&'static str> {
    if !env.is_preview
        || env.pattern_document
        || env.annotation_scope == RenderAnnotationScope::None
    {
        return None;
    }
    Some(if block_id == "core/section" {
        "section"
    } else if supports_children {
        "container"
    } else {
        "block"
    })
}

fn root_attrs_for<'a>(
    node: &'a BlockNode,
    node_scope: &'a str,
    kind: Option<&'a str>,
    extra_classes: &'a [String],
) -> RootAttrs<'a> {
    RootAttrs {
        node_id: &node.id,
        block_id: &node.block_id,
        node_scope,
        html_id: node.meta.as_ref().and_then(|m| m.html_id.as_deref()),
        extra_classes,
        kind,
        is_loop_clone: false,
    }
}

fn extra_classes_for(node: &BlockNode) -> Vec<String> {
    let mut classes = Vec::new();
    if let Some(raw) = node
        .meta
        .as_ref()
        .and_then(|m| m.class_name.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        classes.extend(raw.split_whitespace().map(str::to_owned));
    }
    classes
}

fn active_child_nodes<'a>(
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

fn build_children_meta(active: &[(&BlockNode, &RegisteredBlockDef)]) -> Vec<Value> {
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
                .filter(|name| opendash_core::icons::lookup_core_icon_paths(name).is_some())
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

fn scalar_parent_props(fields: &[FieldSpec], defaulted: &BTreeMap<String, Value>) -> Value {
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

fn render_csec_node(
    env: &mut RenderEnv<'_>,
    node: &BlockNode,
    def: &RegisteredBlockDef,
    children_html: Option<&str>,
    children_meta: Option<&[Value]>,
    parent_ctx: Option<&Value>,
) -> ApiResult<Result<String, ()>> {
    let hydration_ctx = crate::field_hydrate::HydrationContext {
        media: &env.state.hydration_media,
        entries: &env.state.hydration_entries,
        terms: &env.state.hydration_terms,
        is_preview: env.is_preview,
        tag_media_provenance: true,
    };
    let defaulted = def.schema.schema.apply_field_defaults(&node.props);
    let mut local_rt = crate::template_sec::RichTextNonceTable::default();
    let res = env
        .cache
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
                timezone: env.state.timezone.clone(),
                ..Default::default()
            };
            let menus_opt = (!env.state.menus.is_empty()).then_some(&env.state.menus);
            crate::template_sec::render_block_csec_compiled(
                &compiled.block_id,
                &compiled.base_env,
                crate::template_sec::BlockRenderInputs {
                    hydrated_props: &hydrated,
                    local_richtext_nonces: Some(local_rt),
                    merged_options: Some(&env.state.options),
                    site_options: Some(&env.state.site_options),
                    menus: menus_opt,
                    children_html,
                    children_meta,
                    parent_ctx,
                    is_preview: env.is_preview,
                    fmt_cfg: &fmt_cfg,
                    loop_context: None,
                },
                env.budget,
            )
        });
    match res {
        Ok(rendered) => {
            if !env.is_preview && def.schema.schema.has_active_contact_form(&defaulted) {
                env.has_active_contact_form = true;
            }
            Ok(Ok(rendered))
        }
        Err(_) => {
            if env.budget.html_bytes_used > MAX_RENDERED_HTML_BYTES {
                return Err(ApiError::bad_request(
                    "Builder template composition exceeds 512 KiB.",
                ));
            }
            Ok(Err(()))
        }
    }
}

fn push_unavailable_placeholder(
    env: &mut RenderEnv<'_>,
    node: &BlockNode,
    node_scope: &str,
    extra_classes: &[String],
    code: &'static str,
    out: &mut String,
) -> ApiResult<()> {
    env.diagnostics.push(BuilderDiagnostic {
        node_id: node.id.clone(),
        code,
    });
    let raw = format!(
        "<div class=\"builder-block-unavailable\" data-block-id=\"{}\"></div>",
        escape_html(&node.block_id),
    );
    let attrs = root_attrs_for(node, node_scope, None, extra_classes);
    let wrapped = apply_single_root_wrapper(&raw, &attrs, env.is_preview);
    env.budget
        .charge_html_bytes(wrapped.len())
        .map_err(ApiError::bad_request)?;
    out.push_str(&wrapped);
    Ok(())
}

fn render_node(
    env: &mut RenderEnv<'_>,
    node: &BlockNode,
    parent_ctx: Option<&Value>,
    native_parent: NativeParentInfo,
    out: &mut String,
) -> ApiResult<()> {
    let node_scope = loop_node_scope(env.scope, None, &node.id);
    let extra_classes = extra_classes_for(node);
    let Some(def) = env.state.declared_blocks.get(&node.block_id).cloned() else {
        return push_unavailable_placeholder(
            env,
            node,
            &node_scope,
            &extra_classes,
            "plugin_block_unavailable",
            out,
        );
    };
    if def.status != ProviderStatus::Active || def.declaration_state != DeclarationState::Declared {
        return push_unavailable_placeholder(
            env,
            node,
            &node_scope,
            &extra_classes,
            "plugin_block_unavailable",
            out,
        );
    }

    let kind = node_preview_kind(env, &node.block_id, def.schema.schema.supports_children);
    let attrs = root_attrs_for(node, &node_scope, kind, &extra_classes);
    let is_native = def.schema.schema.render == "native";

    if matches!(
        node.block_id.as_str(),
        "core/archive-loop" | "core/entry-related"
    ) || (node.block_id == "core/list"
        && node.children.iter().any(|c| c.block_id == "core/loop-item"))
    {
        let allow_editable = env.is_preview
            && !env.pattern_document
            && env.annotation_scope != RenderAnnotationScope::None;
        return render_loop_node_wrapper(
            node,
            &attrs,
            env.state,
            env.scope,
            env.lists,
            env.list_base_path,
            env.context,
            env.is_preview,
            allow_editable,
            env.cache,
            env.budget,
            &mut env.diagnostics,
            out,
        );
    }

    if def.schema.schema.supports_children && node.block_id != "core/list" {
        let active = active_child_nodes(&node.children, &env.state.declared_blocks);
        let active_count = active.len();
        let children_meta = if is_native {
            Vec::new()
        } else {
            build_children_meta(&active)
        };
        let child_nonce =
            crate::template_sec::generate_children_nonce().map_err(ApiError::bad_request)?;
        let raw_shell = if is_native {
            match render_native_container(
                node,
                &def.schema.schema.fields,
                &child_nonce,
                native_parent,
            ) {
                Some(html) => html,
                None => {
                    return push_unavailable_placeholder(
                        env,
                        node,
                        &node_scope,
                        &extra_classes,
                        "descriptor_missing",
                        out,
                    );
                }
            }
        } else {
            match render_csec_node(
                env,
                node,
                &def,
                Some(&child_nonce),
                Some(&children_meta),
                parent_ctx,
            )? {
                Ok(html) => html,
                Err(()) => {
                    return push_unavailable_placeholder(
                        env,
                        node,
                        &node_scope,
                        &extra_classes,
                        "plugin_block_render_error",
                        out,
                    );
                }
            }
        };
        let wrapped = apply_single_root_wrapper(&raw_shell, &attrs, env.is_preview);
        let emitted_nonce = match scan_child_nonce_in_shell(&wrapped, &child_nonce) {
            Ok(flag) => flag,
            Err(_) => {
                return push_unavailable_placeholder(
                    env,
                    node,
                    &node_scope,
                    &extra_classes,
                    "plugin_block_render_error",
                    out,
                );
            }
        };
        let charge_delta = if is_native {
            wrapped
                .len()
                .saturating_sub(if emitted_nonce { child_nonce.len() } else { 0 })
        } else {
            wrapped.len().saturating_sub(raw_shell.len())
        };
        env.budget
            .charge_html_bytes(charge_delta)
            .map_err(ApiError::bad_request)?;
        out.push_str(&wrapped);

        if emitted_nonce {
            let defaulted_parent = def.schema.schema.apply_field_defaults(&node.props);
            let p_props = scalar_parent_props(&def.schema.schema.fields, &defaulted_parent);
            let child_native_parent = if node.block_id == "core/row" {
                let mode = match defaulted_parent.get("layout_mode").and_then(Value::as_str) {
                    Some("grid_12") => "grid_12",
                    _ => "equal",
                };
                NativeParentInfo {
                    layout_mode: Some(mode),
                    active_child_count: active_count,
                }
            } else {
                NativeParentInfo {
                    layout_mode: None,
                    active_child_count: active_count,
                }
            };
            let mut children_buf = String::new();
            for (idx, child) in node.children.iter().enumerate() {
                let child_meta_idx = active
                    .iter()
                    .position(|(ac, _)| ac.id == child.id)
                    .unwrap_or(idx);
                let child_parent_ctx = json!({
                    "parent_id": node.id,
                    "parent_block_id": node.block_id,
                    "index": child_meta_idx,
                    "sibling_count": active_count,
                    "parent_props": &p_props,
                });
                render_node(
                    env,
                    child,
                    Some(&child_parent_ctx),
                    child_native_parent,
                    &mut children_buf,
                )?;
            }
            env.slot_table.insert(child_nonce, children_buf);
        }
        return Ok(());
    }

    if is_native {
        let mut diag_buf = Vec::new();
        let (maybe_html, already_charged) = if let Some(rt) = render_native_runtime(
            env.state,
            node,
            env.scope,
            env.is_preview,
            env.lists,
            env.forms,
            env.context,
            env.patterns,
            env.list_base_path,
            env.budget,
            &mut env.has_active_contact_form,
            &mut diag_buf,
        )? {
            env.used_entry_content |= rt.used_entry_content;
            env.has_popup |= rt.has_popup;
            (rt.html, rt.already_charged)
        } else if let Some(leaf) = render_native_leaf(
            node,
            &def.schema.schema.fields,
            &env.state.hydration_media,
            env.is_preview,
            env.budget,
        )? {
            (Some(leaf), 0)
        } else {
            diag_buf.push(BuilderDiagnostic {
                node_id: node.id.clone(),
                code: "descriptor_missing",
            });
            (None, 0)
        };
        env.diagnostics.extend(diag_buf);
        let Some(fragment) = maybe_html else {
            return Ok(());
        };
        let wrapped = apply_single_root_wrapper(&fragment, &attrs, env.is_preview);
        let own_bytes = wrapped.len().saturating_sub(already_charged);
        env.budget
            .charge_html_bytes(own_bytes)
            .map_err(ApiError::bad_request)?;
        out.push_str(&wrapped);
        return Ok(());
    }

    match render_csec_node(env, node, &def, None, None, parent_ctx)? {
        Ok(raw_html) => {
            let wrapped = apply_single_root_wrapper(&raw_html, &attrs, env.is_preview);
            let wrapper_delta = wrapped.len().saturating_sub(raw_html.len());
            env.budget
                .charge_html_bytes(wrapper_delta)
                .map_err(ApiError::bad_request)?;
            out.push_str(&wrapped);
        }
        Err(()) => {
            push_unavailable_placeholder(
                env,
                node,
                &node_scope,
                &extra_classes,
                "plugin_block_render_error",
                out,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_document(
    state: &BuilderRenderContext,
    document: &BuilderDocument,
    is_preview: bool,
    lists: &ListResults,
    forms: &ContactFormResults,
    context: Option<&BuilderTemplateContext<'_>>,
    patterns: &PatternResults,
    pattern_document: bool,
    annotation_scope: RenderAnnotationScope,
    scope: &str,
    list_base_path: &str,
    budget: &mut RenderBudget,
    cache: &BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    let vis_css = builder_visibility_css(document, scope);
    let block_css = serialize_scoped_block_css(state, document, scope, is_preview)?;
    let root_open = if pattern_document {
        "<div class=\"od-pattern-content\" data-builder-version=\"1\">"
    } else {
        "<main data-builder-version=\"1\">"
    };
    let root_close = if pattern_document {
        "</div>"
    } else {
        "</main>"
    };
    let chrome_bytes = vis_css.len() + block_css.len() + root_open.len() + root_close.len();
    budget
        .charge_html_bytes(chrome_bytes)
        .map_err(ApiError::bad_request)?;

    let mut inner_html = String::new();
    let mut env = RenderEnv {
        state,
        is_preview,
        lists,
        forms,
        context,
        patterns,
        pattern_document,
        annotation_scope,
        scope,
        list_base_path,
        cache,
        budget,
        slot_table: HtmlSlotTable::new(),
        diagnostics: Vec::new(),
        has_active_contact_form: false,
        used_entry_content: false,
        has_popup: false,
    };
    for node in &document.blocks {
        render_node(
            &mut env,
            node,
            None,
            NativeParentInfo::default(),
            &mut inner_html,
        )?;
    }
    let has_active_contact_form = env.has_active_contact_form;
    let used_entry_content = env.used_entry_content;
    let has_popup = env.has_popup;
    let diagnostics = env.diagnostics;
    let mut slot_table = env.slot_table;

    let root_html = format!("{vis_css}{block_css}{root_open}{inner_html}{root_close}");
    let html = assemble_slot_table(&root_html, &mut slot_table).map_err(ApiError::bad_request)?;
    Ok(RenderedLayout {
        has_active_contact_form,
        needs_blocks_js: env.budget.needs_blocks_js,
        used_entry_content,
        has_popup,
        html,
        diagnostics,
    })
}
