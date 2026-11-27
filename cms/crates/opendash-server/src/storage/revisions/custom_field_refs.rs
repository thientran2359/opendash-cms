use std::collections::{BTreeMap, BTreeSet};

use opendash_core::field_schema::{FieldSpec, FieldType, MediaFieldKind, evaluate_conditions};
use opendash_core::revisions::{
    DroppedBlockFieldKey, MissingMediaRef, MissingRefsReport, canonical_json_value,
};
use serde_json::Value;
use sqlx::SqliteConnection;

use crate::error::ApiResult;
use crate::storage::fields::extract_ids_from_value;

use super::ref_cache::InspectionCaches;

#[allow(clippy::too_many_arguments)]
fn box_inspect_fields<'a>(
    conn: &'a mut SqliteConnection,
    cache: &'a mut InspectionCaches,
    specs: &'a [FieldSpec],
    values: &'a BTreeMap<String, Value>,
    root_specs: &'a [FieldSpec],
    root_values: &'a BTreeMap<String, Value>,
    parent_visible: bool,
    required_locale: Option<&'a str>,
    block_ctx: Option<(&'a str, &'a str)>,
    path_prefix: Option<&'a str>,
    prev_block_values: Option<&'a BTreeMap<String, Value>>,
    report: &'a mut MissingRefsReport,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ApiResult<()>> + Send + 'a>> {
    Box::pin(inspect_field_map_refs(
        conn,
        cache,
        specs,
        values,
        root_specs,
        root_values,
        parent_visible,
        required_locale,
        block_ctx,
        path_prefix,
        prev_block_values,
        report,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn inspect_field_map_refs(
    conn: &mut SqliteConnection,
    cache: &mut InspectionCaches,
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
    parent_visible: bool,
    required_locale: Option<&str>,
    block_ctx: Option<(&str, &str)>,
    path_prefix: Option<&str>,
    prev_block_values: Option<&BTreeMap<String, Value>>,
    report: &mut MissingRefsReport,
) -> ApiResult<()> {
    if let Some((node_id, block_type)) = block_ctx {
        let valid_keys: BTreeSet<&str> = specs
            .iter()
            .filter(|s| !s.is_ui_only())
            .map(|s| s.key.as_str())
            .collect();
        for (k, val) in values {
            if !valid_keys.contains(k.as_str()) {
                let preserved_key = prev_block_values
                    .and_then(|pv| pv.get(k.as_str()))
                    .is_some_and(|pv| canonical_json_value(pv) == canonical_json_value(val));
                if !preserved_key {
                    let fp = match path_prefix {
                        Some(p) => format!("{p}.{k}"),
                        None => k.clone(),
                    };
                    report.dropped_block_field_keys.push(DroppedBlockFieldKey {
                        node_id: node_id.to_owned(),
                        block_type: block_type.to_owned(),
                        field_path: fp,
                    });
                }
            }
        }
    }

    for spec in specs {
        if spec.is_ui_only() || spec.field_type == FieldType::Secret {
            continue;
        }
        let field_path = match path_prefix {
            Some(p) => format!("{p}.{}", spec.key),
            None => spec.key.clone(),
        };
        let loc_prefix = match block_ctx {
            Some((node_id, _)) => format!("builder:{node_id}.{field_path}"),
            None => format!("field:{field_path}"),
        };
        let visible = parent_visible
            && evaluate_conditions(
                spec.conditions.as_ref(),
                specs,
                values,
                root_specs,
                root_values,
            );
        let req_flag = visible && spec.effective_required();
        let Some(val) = values.get(spec.key.as_str()).filter(|v| !v.is_null()) else {
            continue;
        };

        match spec.field_type {
            FieldType::Media => {
                if let Some(id) = val.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    let (expected_label, query_kind) = match spec.effective_media_kind() {
                        MediaFieldKind::Image => ("image", Some("image")),
                        MediaFieldKind::Video => ("video", Some("video")),
                        MediaFieldKind::File => ("file", None),
                        MediaFieldKind::Any => ("any", None),
                    };
                    if !cache.media_ok(&mut *conn, id, query_kind).await? {
                        report.missing_media.push(MissingMediaRef {
                            media_id: id.to_owned(),
                            location: loc_prefix,
                            expected_kind: expected_label.to_owned(),
                            required: req_flag,
                        });
                    }
                }
            }
            FieldType::Gallery => {
                if let Some(arr) = val.as_array() {
                    for (idx, item) in arr.iter().enumerate() {
                        if let Some(id) = item.as_str().map(str::trim).filter(|s| !s.is_empty())
                            && !cache.media_ok(&mut *conn, id, Some("image")).await?
                        {
                            report.missing_media.push(MissingMediaRef {
                                media_id: id.to_owned(),
                                location: format!("{loc_prefix}[{idx}]"),
                                expected_kind: "image".to_owned(),
                                required: req_flag,
                            });
                        }
                    }
                }
            }
            FieldType::EntryRef => {
                let ids = extract_ids_from_value(val);
                let is_wildcard = spec.target_types.len() == 1 && spec.target_types[0] == "*";
                for id in ids {
                    let ok = match cache.entry_info(&mut *conn, id).await? {
                        Some((tk, loc)) => {
                            (is_wildcard || spec.target_types.iter().any(|t| t == &tk))
                                && required_locale.is_none_or(|req| req == loc)
                        }
                        None => false,
                    };
                    if !ok {
                        report.missing_entry_ids.push(id);
                    }
                }
            }
            FieldType::TermRef => {
                let ids = extract_ids_from_value(val);
                if !ids.is_empty() {
                    let expected_tax =
                        resolve_term_ref_taxonomy(spec, specs, values, root_specs, root_values);
                    let tax_ok = match expected_tax.as_deref() {
                        Some(tax) => cache.tax_declared(&mut *conn, tax).await?,
                        None => false,
                    };
                    for id in ids {
                        let ok = if tax_ok {
                            matches!(
                                cache.term_info(&mut *conn, id).await?,
                                Some((ref kind, ref loc))
                                    if expected_tax.as_deref() == Some(kind.as_str())
                                        && required_locale.is_none_or(|req| req == loc)
                            )
                        } else {
                            false
                        };
                        if !ok {
                            report.missing_term_ids.push(id);
                        }
                    }
                }
            }
            FieldType::Group => {
                if let Some(obj) = val.as_object() {
                    let sub_map: BTreeMap<String, Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    let prev_sub = prev_block_values
                        .and_then(|pv| pv.get(spec.key.as_str()))
                        .and_then(Value::as_object)
                        .map(|o| {
                            o.iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect::<BTreeMap<_, _>>()
                        });
                    box_inspect_fields(
                        &mut *conn,
                        cache,
                        &spec.fields,
                        &sub_map,
                        root_specs,
                        root_values,
                        visible,
                        required_locale,
                        block_ctx,
                        Some(&field_path),
                        prev_sub.as_ref(),
                        report,
                    )
                    .await?;
                }
            }
            FieldType::Repeater => {
                if let Some(arr) = val.as_array() {
                    for (idx, item) in arr.iter().enumerate() {
                        if let Some(obj) = item.as_object() {
                            let sub_map: BTreeMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            let item_path = format!("{field_path}[{idx}]");
                            let prev_item = prev_block_values
                                .and_then(|pv| pv.get(spec.key.as_str()))
                                .and_then(Value::as_array)
                                .and_then(|a| a.get(idx))
                                .and_then(Value::as_object)
                                .map(|o| {
                                    o.iter()
                                        .map(|(k, v)| (k.clone(), v.clone()))
                                        .collect::<BTreeMap<_, _>>()
                                });
                            box_inspect_fields(
                                &mut *conn,
                                cache,
                                &spec.fields,
                                &sub_map,
                                root_specs,
                                root_values,
                                visible,
                                required_locale,
                                block_ctx,
                                Some(&item_path),
                                prev_item.as_ref(),
                                report,
                            )
                            .await?;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn resolve_term_ref_taxonomy(
    spec: &FieldSpec,
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
) -> Option<String> {
    if let Some(ref field_ref) = spec.taxonomy_from_field {
        values
            .get(field_ref)
            .or_else(|| root_values.get(field_ref))
            .or_else(|| {
                specs
                    .iter()
                    .chain(root_specs.iter())
                    .find(|s| &s.key == field_ref)
                    .and_then(|s| s.default.as_ref())
            })
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    } else {
        spec.taxonomy.clone()
    }
}
