use std::collections::{BTreeSet, HashMap};

use opendash_core::builder::{BlockNode, BuilderDocument};
use opendash_core::content::FieldRole;
use opendash_core::field_schema::SelectOptionsSource;
use opendash_core::registry::{DeclarationState, ProviderStatus};
use opendash_core::revisions::{
    MissingBlockTypeRef, MissingDynamicOptionRef, MissingListRef, MissingRefsReport,
};
use serde_json::Value;
use sqlx::SqliteConnection;

use crate::error::ApiResult;
use crate::storage::builder::pattern_ids_with_schemas;
use crate::storage::fields::collect_all_options_sources_with_paths;
use crate::storage::menu::extract_declared_menu_slots;

use super::custom_field_refs::inspect_field_map_refs;
use super::ref_cache::InspectionCaches;
use super::snapshot_queries::LiveEntryState;

fn parse_positive_i64(val: Option<&Value>) -> Option<i64> {
    val.and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
    })
    .filter(|&id| id > 0)
}

fn collect_live_nodes<'a>(nodes: &'a [BlockNode], out: &mut HashMap<&'a str, &'a BlockNode>) {
    for node in nodes {
        out.insert(node.id.as_str(), node);
        collect_live_nodes(&node.children, out);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn inspect_builder_doc_refs(
    conn: &mut SqliteConnection,
    cache: &mut InspectionCaches,
    _live: &LiveEntryState,
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

    let mut live_nodes: HashMap<&str, &BlockNode> = HashMap::new();
    if let Some(live) = live_doc {
        collect_live_nodes(&live.blocks, &mut live_nodes);
    }

    let mut stack: Vec<(&BlockNode, Option<String>)> =
        doc.blocks.iter().rev().map(|n| (n, None)).collect();
    while let Some((node, enclosing_list_type)) = stack.pop() {
        let prev_node = live_nodes.get(node.id.as_str()).copied();
        let preserved = prev_node
            .is_some_and(|prev| prev.block_id == node.block_id && prev.props == node.props);
        let Some(entry) = cache.schemas.get(&node.block_id).cloned() else {
            if !preserved {
                report.missing_block_types.push(MissingBlockTypeRef {
                    node_id: node.id.clone(),
                    block_id: node.block_id.clone(),
                    reason: "unknown_block_type".to_owned(),
                });
            }
            continue;
        };
        if entry.status != ProviderStatus::Active
            || entry.declaration_state != DeclarationState::Declared
        {
            if !preserved {
                report.missing_block_types.push(MissingBlockTypeRef {
                    node_id: node.id.clone(),
                    block_id: node.block_id.clone(),
                    reason: "provider_inactive".to_owned(),
                });
            }
            continue;
        }
        let child_list_type = if node.block_id == "core/list" {
            node.props
                .get("type_key")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        } else {
            enclosing_list_type.clone()
        };
        for child in node.children.iter().rev() {
            stack.push((child, child_list_type.clone()));
        }

        if node.block_id == "core/contact-form"
            && let Some(fid) = parse_positive_i64(node.props.get("form_id"))
            && !cache.form_ok(&mut *conn, fid, false).await?
        {
            report.missing_contact_form_ids.push(fid);
        }

        if node.block_id == "core/list" {
            if let Some(type_key) = node
                .props
                .get("type_key")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                && !cache.cpt_ok(&mut *conn, type_key).await?
            {
                report.missing_list_refs.push(MissingListRef {
                    node_id: node.id.clone(),
                    reason: format!("inactive_cpt:{type_key}"),
                });
            }
            for key in ["category_id", "tag_id", "term_id"] {
                if let Some(tid) = parse_positive_i64(node.props.get(key)) {
                    let exists: i64 = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM taxonomy_terms WHERE id = ?)",
                    )
                    .bind(tid)
                    .fetch_one(&mut *conn)
                    .await?;
                    if exists == 0 {
                        report.missing_list_refs.push(MissingListRef {
                            node_id: node.id.clone(),
                            reason: format!("missing_term:{tid}"),
                        });
                    }
                }
            }
        }

        let block_specs = &entry.descriptor.schema.fields;
        let prev_block_vals = prev_node
            .filter(|prev| prev.block_id == node.block_id)
            .map(|prev| &prev.props);
        inspect_field_map_refs(
            &mut *conn,
            cache,
            block_specs,
            &node.props,
            block_specs,
            &node.props,
            true,
            None,
            Some((node.id.as_str(), node.block_id.as_str())),
            None,
            prev_block_vals,
            report,
        )
        .await?;

        if !preserved && node.block_id != "core/template-part" {
            let mut opts = Vec::new();
            collect_all_options_sources_with_paths(block_specs, &node.props, None, &mut opts);
            let effective_fields_override = match enclosing_list_type.as_deref() {
                Some(type_key) => {
                    cache
                        .public_context_fields_for_cpt(&mut *conn, type_key, caller_role)
                        .await?
                }
                None => None,
            };
            let effective_fields = effective_fields_override
                .as_deref()
                .unwrap_or(allowed_context_fields);
            inspect_dynamic_options(&mut *conn, cache, &opts, effective_fields, report).await?;
        }
    }
    Ok(())
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
