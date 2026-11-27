use std::collections::{BTreeMap, BTreeSet};

use opendash_core::builder::{
    BindingSource, BindingTarget, BuilderDocument, ElementContent, KnownElement, is_preserved,
};
use opendash_core::content::{FieldRole, PublicExposure};
use opendash_core::field_schema::{FieldType, SelectOptionsSource};
use opendash_core::registry::{DeclarationState, ProviderStatus};
use opendash_core::revisions::{
    ClearedBindingNode, MissingBlockTypeRef, MissingDynamicOptionRef, MissingListRef,
    MissingMediaRef, MissingRefsReport,
};
use serde_json::Value;
use sqlx::SqliteConnection;

use crate::error::ApiResult;
use crate::storage::builder::{pattern_ids_with_schemas, visible_builder_field};
use crate::storage::fields::collect_all_options_sources_with_paths;
use crate::storage::menu::extract_declared_menu_slots;

use super::custom_field_refs::inspect_field_map_refs;
use super::ref_cache::InspectionCaches;
use super::snapshot_queries::LiveEntryState;

#[allow(clippy::too_many_arguments)]
pub(super) async fn inspect_builder_doc_refs(
    conn: &mut SqliteConnection,
    cache: &mut InspectionCaches,
    live: &LiveEntryState,
    doc: &BuilderDocument,
    live_doc: Option<&BuilderDocument>,
    caller_role: FieldRole,
    allowed_context_fields: &[String],
    report: &mut MissingRefsReport,
) -> ApiResult<()> {
    let pattern_ids = pattern_ids_with_schemas(doc, &cache.schemas);
    for pid in pattern_ids {
        if !cache.pattern_ok(&mut *conn, pid).await? {
            report.missing_pattern_ids.push(pid);
        }
    }

    for (s_idx, section) in doc.sections.iter().enumerate() {
        if let Some(style) = &section.style
            && let Some(ref bg_id) = style.background_media_id
            && !bg_id.is_empty()
            && !cache.media_ok(&mut *conn, bg_id, Some("image")).await?
        {
            report.missing_media.push(MissingMediaRef {
                media_id: bg_id.clone(),
                location: format!("builder:{}", section.id),
                expected_kind: "image".to_owned(),
                required: false,
            });
        }
        for (r_idx, row) in section.children.iter().enumerate() {
            for (c_idx, col) in row.children.iter().enumerate() {
                for (e_idx, element) in col.children.iter().enumerate() {
                    let preserved = is_preserved(
                        live_doc,
                        (s_idx, r_idx, c_idx, e_idx),
                        section,
                        row,
                        col,
                        element,
                    );
                    let ElementContent::Known(known) = &element.content else {
                        if !preserved {
                            report.missing_block_types.push(MissingBlockTypeRef {
                                node_id: element.id.clone(),
                                block_id: "unresolved".to_owned(),
                                reason: "unresolved_block".to_owned(),
                            });
                        }
                        continue;
                    };
                    if let Some(binding) = known.binding() {
                        check_binding_access(
                            &mut *conn,
                            cache,
                            live,
                            &element.id,
                            binding,
                            caller_role,
                            report,
                        )
                        .await?;
                    }
                    inspect_known_element(
                        &mut *conn,
                        cache,
                        doc,
                        live_doc,
                        &element.id,
                        known,
                        preserved,
                        allowed_context_fields,
                        report,
                    )
                    .await?;
                }
            }
        }
    }
    Ok(())
}

async fn check_binding_access(
    conn: &mut SqliteConnection,
    cache: &mut InspectionCaches,
    live: &LiveEntryState,
    node_id: &str,
    binding: &opendash_core::builder::ElementBinding,
    caller_role: FieldRole,
    report: &mut MissingRefsReport,
) -> ApiResult<()> {
    match &binding.source {
        BindingSource::Title => {}
        BindingSource::RouteContext { field_key } => {
            let clear = match field_key.as_str() {
                "title" | "slug" | "url" => false,
                "cover_url" => live
                    .effective
                    .effective_fields
                    .values()
                    .find(|entry| {
                        entry.effective_status
                            == opendash_core::content::EffectiveFieldStatus::Active
                            && entry.definition.field_type == FieldType::Media
                            && entry.definition.effective_acl().2 == PublicExposure::Public
                            && (entry.owner == opendash_core::registry::ResourceOwner::Core
                                || live
                                    .effective
                                    .attached_groups
                                    .iter()
                                    .find(|g| g.group_id == entry.source_id)
                                    .is_some_and(|g| g.is_runtime_active()))
                    })
                    .is_some_and(|entry| {
                        !visible_builder_field(&live.effective, &entry.definition.key, caller_role)
                    }),
                other => !visible_builder_field(&live.effective, other, caller_role),
            };
            if clear {
                report.cleared_binding_nodes.push(ClearedBindingNode {
                    node_id: node_id.to_owned(),
                    field_key: field_key.clone(),
                });
            }
        }
        BindingSource::Field { key } => {
            if let Some(schema) = cache.schema_for(&mut *conn, &binding.type_key).await?
                && schema.effective_fields.contains_key(key)
                && !visible_builder_field(schema, key, caller_role)
            {
                report.cleared_binding_nodes.push(ClearedBindingNode {
                    node_id: node_id.to_owned(),
                    field_key: key.clone(),
                });
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn inspect_known_element(
    conn: &mut SqliteConnection,
    cache: &mut InspectionCaches,
    _doc: &BuilderDocument,
    live_doc: Option<&BuilderDocument>,
    node_id: &str,
    known: &KnownElement,
    preserved: bool,
    allowed_context_fields: &[String],
    report: &mut MissingRefsReport,
) -> ApiResult<()> {
    match known {
        KnownElement::Image(props) => {
            let bound_media = props
                .binding
                .as_ref()
                .is_some_and(|b| b.target == BindingTarget::MediaId);
            if !bound_media
                && !props.media_id.is_empty()
                && !cache
                    .media_ok(&mut *conn, &props.media_id, Some("image"))
                    .await?
            {
                report.missing_media.push(MissingMediaRef {
                    media_id: props.media_id.clone(),
                    location: format!("builder:{node_id}"),
                    expected_kind: "image".to_owned(),
                    required: true,
                });
            }
        }
        KnownElement::Video(props) => {
            if !cache
                .media_ok(&mut *conn, &props.media_id, Some("video"))
                .await?
            {
                report.missing_media.push(MissingMediaRef {
                    media_id: props.media_id.clone(),
                    location: format!("builder:{node_id}"),
                    expected_kind: "video".to_owned(),
                    required: true,
                });
            }
            if let Some(ref pid) = props.poster_image_id
                && !pid.is_empty()
                && !cache.media_ok(&mut *conn, pid, Some("image")).await?
            {
                report.missing_media.push(MissingMediaRef {
                    media_id: pid.clone(),
                    location: format!("builder:{node_id}"),
                    expected_kind: "image".to_owned(),
                    required: false,
                });
            }
        }
        KnownElement::ContactForm(props) => {
            if !cache.form_ok(&mut *conn, props.form_id, false).await? {
                report.missing_contact_form_ids.push(props.form_id);
            }
        }
        KnownElement::List(props) => {
            if !cache.cpt_ok(&mut *conn, &props.type_key).await? {
                report.missing_list_refs.push(MissingListRef {
                    node_id: node_id.to_owned(),
                    reason: format!("inactive_cpt:{}", props.type_key),
                });
            }
            for term_opt in [props.category_id, props.tag_id, props.term_id] {
                if let Some(tid) = term_opt {
                    let exists: i64 = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM taxonomy_terms WHERE id = ?)",
                    )
                    .bind(tid)
                    .fetch_one(&mut *conn)
                    .await?;
                    if exists == 0 {
                        report.missing_list_refs.push(MissingListRef {
                            node_id: node_id.to_owned(),
                            reason: format!("missing_term:{tid}"),
                        });
                    }
                }
            }
        }
        KnownElement::Legacy(props) => {
            if !preserved {
                report.missing_block_types.push(MissingBlockTypeRef {
                    node_id: node_id.to_owned(),
                    block_id: props.block_id.clone(),
                    reason: "legacy_block".to_owned(),
                });
            }
        }
        KnownElement::PluginBlock(props) => {
            let Some(entry) = cache.schemas.get(&props.block_id).cloned() else {
                if !preserved {
                    report.missing_block_types.push(MissingBlockTypeRef {
                        node_id: node_id.to_owned(),
                        block_id: props.block_id.clone(),
                        reason: "unknown_block_type".to_owned(),
                    });
                }
                return Ok(());
            };
            if entry.status != ProviderStatus::Active
                || entry.declaration_state != DeclarationState::Declared
            {
                if !preserved {
                    report.missing_block_types.push(MissingBlockTypeRef {
                        node_id: node_id.to_owned(),
                        block_id: props.block_id.clone(),
                        reason: "provider_inactive".to_owned(),
                    });
                }
                return Ok(());
            }
            let block_specs = &entry.descriptor.schema.fields;
            let prev_block_vals =
                live_doc.and_then(|d| find_live_plugin_values(d, node_id, &props.block_id));
            inspect_field_map_refs(
                &mut *conn,
                cache,
                block_specs,
                &props.values,
                block_specs,
                &props.values,
                true,
                None,
                Some((node_id, &props.block_id)),
                None,
                prev_block_vals,
                report,
            )
            .await?;

            if !preserved {
                let mut opts = Vec::new();
                collect_all_options_sources_with_paths(block_specs, &props.values, None, &mut opts);
                inspect_dynamic_options(&mut *conn, cache, &opts, allowed_context_fields, report)
                    .await?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn find_live_plugin_values<'a>(
    doc: &'a BuilderDocument,
    element_id: &str,
    block_id: &str,
) -> Option<&'a BTreeMap<String, Value>> {
    for section in &doc.sections {
        for row in &section.children {
            for col in &row.children {
                for el in &col.children {
                    if el.id == element_id
                        && let ElementContent::Known(KnownElement::PluginBlock(props)) = &el.content
                        && props.block_id == block_id
                    {
                        return Some(&props.values);
                    }
                }
            }
        }
    }
    None
}

async fn inspect_dynamic_options(
    conn: &mut SqliteConnection,
    cache: &mut InspectionCaches,
    opts: &[(String, SelectOptionsSource, Vec<String>)],
    allowed_context_fields: &[String],
    report: &mut MissingRefsReport,
) -> ApiResult<()> {
    const CORE_CONTEXT_FIELDS: &[&str] = &["title", "slug", "url", "cover_url"];
    for (field_path, source, selected) in opts {
        match source {
            SelectOptionsSource::Patterns => {}
            SelectOptionsSource::ContactForms => {
                for raw in selected {
                    if let Ok(fid) = raw.parse::<i64>() {
                        if !cache.form_ok(&mut *conn, fid, true).await? {
                            report.missing_contact_form_ids.push(fid);
                        }
                    } else {
                        report
                            .missing_dynamic_options
                            .push(MissingDynamicOptionRef {
                                source: "contact_forms".to_owned(),
                                value: raw.clone(),
                                field_path: field_path.clone(),
                            });
                    }
                }
            }
            SelectOptionsSource::Menus => {
                for val in selected {
                    if !cache.menu_ok(&mut *conn, val).await? {
                        report
                            .missing_dynamic_options
                            .push(MissingDynamicOptionRef {
                                source: "menus".to_owned(),
                                value: val.clone(),
                                field_path: field_path.clone(),
                            });
                    }
                }
            }
            SelectOptionsSource::MenuSlots => {
                let slots: BTreeSet<String> = match cache.ensure_manifest(&mut *conn).await? {
                    Some(m) => serde_json::to_string(m)
                        .ok()
                        .map(|raw| extract_declared_menu_slots(&raw))
                        .unwrap_or_default()
                        .into_iter()
                        .collect(),
                    None => BTreeSet::new(),
                };
                for val in selected {
                    if !slots.contains(val) {
                        report
                            .missing_dynamic_options
                            .push(MissingDynamicOptionRef {
                                source: "menu_slots".to_owned(),
                                value: val.clone(),
                                field_path: field_path.clone(),
                            });
                    }
                }
            }
            SelectOptionsSource::ContentTypes => {
                for val in selected {
                    if !cache.cpt_ok(&mut *conn, val).await? {
                        report
                            .missing_dynamic_options
                            .push(MissingDynamicOptionRef {
                                source: "content_types".to_owned(),
                                value: val.clone(),
                                field_path: field_path.clone(),
                            });
                    }
                }
            }
            SelectOptionsSource::Taxonomies => {
                for val in selected {
                    if !cache.tax_declared(&mut *conn, val).await? {
                        report
                            .missing_dynamic_options
                            .push(MissingDynamicOptionRef {
                                source: "taxonomies".to_owned(),
                                value: val.clone(),
                                field_path: field_path.clone(),
                            });
                    }
                }
            }
            SelectOptionsSource::ContextFields => {
                for val in selected {
                    let ok = CORE_CONTEXT_FIELDS.contains(&val.as_str())
                        || allowed_context_fields.iter().any(|k| k == val);
                    if !ok {
                        report
                            .missing_dynamic_options
                            .push(MissingDynamicOptionRef {
                                source: "context_fields".to_owned(),
                                value: val.clone(),
                                field_path: field_path.clone(),
                            });
                    }
                }
            }
            SelectOptionsSource::TemplateParts => {
                for val in selected {
                    if !cache.template_part_ok(&mut *conn, val).await? {
                        report
                            .missing_dynamic_options
                            .push(MissingDynamicOptionRef {
                                source: "template_parts".to_owned(),
                                value: val.clone(),
                                field_path: field_path.clone(),
                            });
                    }
                }
            }
        }
    }
    Ok(())
}
