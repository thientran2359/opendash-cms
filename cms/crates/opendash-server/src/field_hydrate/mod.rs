//! Pure zero-SQL template hydrator transforming raw `FieldSpec` values into
//! unified V1 MiniJinja objects per `docs/contracts/UNIFIED_FIELD_SCHEMA.md` §6.

mod media_and_links;

pub(crate) use media_and_links::build_media_urls_and_srcsets;
use media_and_links::{
    hydrate_entry_row, hydrate_link_value, hydrate_media_for_ctx, hydrate_spacing_value,
    hydrate_term_row, parse_i64_id, unwrap_responsive_desktop,
};
pub use media_and_links::{hydrate_media_row, hydrate_media_row_for_template};

pub(super) use std::collections::{BTreeMap, BTreeSet};

pub(super) use opendash_core::{
    field_schema::{
        FieldSpec, FieldType, LinkValue, MAX_REPEATER_ITEMS, SpacingProperty, SpacingValue,
        canonicalize_f64, collect_media_ids, collect_relational_refs, default_value_for_field_spec,
        valid_safe_url, valid_sha256_media_id,
    },
    icons::lookup_core_icon_paths,
    richtext::sanitize_richtext,
};

pub(super) use serde_json::{Map, Value, json};

pub(super) use crate::{
    storage::fields::{HydratedEntryRefRow, HydratedMediaRow, HydratedTermRefRow},
    template_sec::RichTextNonceTable,
};

pub use opendash_core::field_schema::HydrationVisibility;

pub struct HydrationContext<'a> {
    pub media: &'a BTreeMap<String, HydratedMediaRow>,
    pub entries: &'a BTreeMap<i64, HydratedEntryRefRow>,
    pub terms: &'a BTreeMap<i64, HydratedTermRefRow>,
    pub is_preview: bool,
    pub tag_media_provenance: bool,
}

fn filter_specs_for_visibility(
    specs: &[FieldSpec],
    visibility: HydrationVisibility,
) -> Vec<FieldSpec> {
    let mut out = Vec::new();
    for spec in specs {
        if !visibility.allows(spec) {
            continue;
        }
        let mut cloned = spec.clone();
        if matches!(cloned.field_type, FieldType::Group | FieldType::Repeater) {
            cloned.fields =
                filter_specs_for_visibility(&cloned.fields, visibility.child_visibility());
        }
        out.push(cloned);
    }
    out
}

/// Collects all referenced `media_ids`, `entry_ids`, and `term_ids` across a set of
/// specs + merged raw values so the caller can execute at most 3 Zero-N+1 batch queries.
pub fn collect_hydration_ids(
    specs: &[FieldSpec],
    raw_values: &BTreeMap<String, Value>,
    visibility: HydrationVisibility,
    media_ids: &mut BTreeSet<String>,
    entry_ids: &mut BTreeSet<i64>,
    term_ids: &mut BTreeSet<i64>,
) {
    let subset = filter_specs_for_visibility(specs, visibility);
    if subset.is_empty() {
        return;
    }
    let collected_media = collect_media_ids(&subset, raw_values);
    media_ids.extend(collected_media.all_ids());
    let collected_rels = collect_relational_refs(&subset, raw_values);
    entry_ids.extend(collected_rels.entry_ids);
    term_ids.extend(collected_rels.term_ids);
}

pub fn hydrate_field_value_stateless(
    spec: &FieldSpec,
    raw_val: Option<&Value>,
    visibility: HydrationVisibility,
) -> Result<Value, &'static str> {
    let media = BTreeMap::new();
    let entries = BTreeMap::new();
    let terms = BTreeMap::new();
    let ctx = HydrationContext {
        media: &media,
        entries: &entries,
        terms: &terms,
        is_preview: false,
        tag_media_provenance: false,
    };
    hydrate_field_value(spec, raw_val, &ctx, None, visibility)
}

pub fn hydrate_field_value(
    spec: &FieldSpec,
    raw_val: Option<&Value>,
    ctx: &HydrationContext<'_>,
    richtext_nonces: Option<&mut RichTextNonceTable>,
    visibility: HydrationVisibility,
) -> Result<Value, &'static str> {
    hydrate_field_value_with_richtext(spec, raw_val, ctx, richtext_nonces, visibility, None, None)
}

pub fn hydrate_field_value_with_richtext(
    spec: &FieldSpec,
    raw_val: Option<&Value>,
    ctx: &HydrationContext<'_>,
    mut richtext_nonces: Option<&mut RichTextNonceTable>,
    visibility: HydrationVisibility,
    mut richtext_html: Option<&mut BTreeMap<String, String>>,
    key_prefix: Option<&str>,
) -> Result<Value, &'static str> {
    if !visibility.allows(spec) {
        return Ok(Value::Null);
    }
    match spec.field_type {
        FieldType::Tab | FieldType::Accordion | FieldType::Message | FieldType::Secret => {
            Ok(Value::Null)
        }
        FieldType::Text
        | FieldType::Textarea
        | FieldType::Email
        | FieldType::Color
        | FieldType::Date
        | FieldType::Datetime
        | FieldType::Time => match raw_val {
            Some(Value::String(s)) => Ok(Value::String(s.clone())),
            _ => Ok(default_value_for_field_spec(spec)),
        },
        FieldType::RadioButtons => {
            let effective = unwrap_responsive_desktop(spec, raw_val);
            match effective {
                Some(Value::String(s)) => Ok(Value::String(s.clone())),
                _ => Ok(default_value_for_field_spec(spec)),
            }
        }
        FieldType::Url => match raw_val {
            Some(Value::String(s)) => {
                if s.is_empty() || valid_safe_url(s) {
                    Ok(Value::String(s.clone()))
                } else {
                    Ok(default_value_for_field_spec(spec))
                }
            }
            _ => Ok(default_value_for_field_spec(spec)),
        },
        FieldType::Number | FieldType::Range => {
            let effective = unwrap_responsive_desktop(spec, raw_val);
            if let Some(n) = effective.and_then(Value::as_f64).filter(|n| n.is_finite()) {
                Ok(json!(canonicalize_f64(n)))
            } else {
                Ok(default_value_for_field_spec(spec))
            }
        }
        FieldType::Boolean => match raw_val.and_then(Value::as_bool) {
            Some(b) => Ok(Value::Bool(b)),
            None => Ok(default_value_for_field_spec(spec)),
        },
        FieldType::Select => {
            let effective = unwrap_responsive_desktop(spec, raw_val);
            if spec.multiple {
                if let Some(arr) = effective.and_then(Value::as_array) {
                    let items: Vec<Value> = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| Value::String(s.to_owned())))
                        .collect();
                    Ok(Value::Array(items))
                } else {
                    Ok(default_value_for_field_spec(spec))
                }
            } else {
                match effective {
                    Some(Value::String(s)) => Ok(Value::String(s.clone())),
                    Some(Value::Null) if spec.allow_null => Ok(Value::Null),
                    _ => Ok(default_value_for_field_spec(spec)),
                }
            }
        }
        FieldType::Checkbox => {
            if let Some(arr) = raw_val.and_then(Value::as_array) {
                let items: Vec<Value> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| Value::String(s.to_owned())))
                    .collect();
                Ok(Value::Array(items))
            } else {
                Ok(default_value_for_field_spec(spec))
            }
        }
        FieldType::Richtext => {
            let text = raw_val
                .and_then(Value::as_str)
                .or_else(|| spec.default.as_ref().and_then(Value::as_str))
                .unwrap_or("");
            if text.trim().is_empty() {
                return Ok(Value::String(String::new()));
            }
            let clean = sanitize_richtext(text)?;
            if clean.is_empty() {
                return Ok(Value::String(String::new()));
            }
            if let Some(rt_map) = richtext_html.as_deref_mut() {
                let full_key = match key_prefix {
                    Some(prefix) if !prefix.is_empty() => format!("{prefix}.{}", spec.key),
                    _ => spec.key.clone(),
                };
                rt_map.insert(full_key, clean.clone());
            }
            if let Some(table) = richtext_nonces.as_deref_mut() {
                let nonce = table.register_sanitized(clean)?;
                Ok(Value::String(nonce))
            } else {
                Ok(Value::String(clean))
            }
        }
        FieldType::Media => {
            // Unsaved defaults never auto-publicize media assets.
            let Some(media_id) = raw_val
                .and_then(Value::as_str)
                .filter(|id| valid_sha256_media_id(id))
            else {
                return Ok(Value::Null);
            };
            Ok(ctx
                .media
                .get(media_id)
                .map(|row| hydrate_media_for_ctx(row, ctx))
                .unwrap_or(Value::Null))
        }
        FieldType::Gallery => {
            let Some(arr) = raw_val.and_then(Value::as_array) else {
                return Ok(Value::Array(Vec::new()));
            };
            let items: Vec<Value> = arr
                .iter()
                .filter_map(Value::as_str)
                .filter(|id| valid_sha256_media_id(id))
                .filter_map(|id| ctx.media.get(id))
                .map(|row| hydrate_media_for_ctx(row, ctx))
                .collect();
            Ok(Value::Array(items))
        }
        FieldType::Link => Ok(hydrate_link_value(spec, raw_val)),
        FieldType::Icon => {
            let name = raw_val
                .and_then(Value::as_str)
                .or_else(|| spec.default.as_ref().and_then(Value::as_str))
                .map(str::trim)
                .filter(|n| !n.is_empty() && lookup_core_icon_paths(n).is_some());
            Ok(match name {
                Some(n) => json!({ "name": n }),
                None => Value::Null,
            })
        }
        FieldType::Spacing => Ok(hydrate_spacing_value(spec, raw_val)),
        FieldType::EntryRef => {
            if spec.multiple {
                let items: Vec<Value> = raw_val
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(parse_i64_id)
                    .filter_map(|id| ctx.entries.get(&id))
                    .map(hydrate_entry_row)
                    .collect();
                Ok(Value::Array(items))
            } else {
                let resolved = raw_val
                    .and_then(parse_i64_id)
                    .and_then(|id| ctx.entries.get(&id))
                    .map(hydrate_entry_row)
                    .unwrap_or(Value::Null);
                Ok(resolved)
            }
        }
        FieldType::TermRef => {
            if spec.multiple {
                let items: Vec<Value> = raw_val
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(parse_i64_id)
                    .filter_map(|id| ctx.terms.get(&id))
                    .map(|row| hydrate_term_row(row, ctx))
                    .collect();
                Ok(Value::Array(items))
            } else {
                let resolved = raw_val
                    .and_then(parse_i64_id)
                    .and_then(|id| ctx.terms.get(&id))
                    .map(|row| hydrate_term_row(row, ctx))
                    .unwrap_or(Value::Null);
                Ok(resolved)
            }
        }
        FieldType::Group => {
            let child_raw: BTreeMap<String, Value> = raw_val
                .and_then(Value::as_object)
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            let next_prefix = match key_prefix {
                Some(prefix) if !prefix.is_empty() => format!("{prefix}.{}", spec.key),
                _ => spec.key.clone(),
            };
            let hydrated = hydrate_field_values_with_richtext(
                &spec.fields,
                &child_raw,
                ctx,
                richtext_nonces,
                visibility.child_visibility(),
                richtext_html,
                Some(&next_prefix),
            )?;
            let mut obj = Map::with_capacity(hydrated.len());
            for (k, v) in hydrated {
                obj.insert(k, v);
            }
            Ok(Value::Object(obj))
        }
        FieldType::Repeater => {
            let Some(arr) = raw_val.and_then(Value::as_array) else {
                return Ok(Value::Array(Vec::new()));
            };
            let cap = spec
                .max_items
                .unwrap_or(MAX_REPEATER_ITEMS)
                .min(MAX_REPEATER_ITEMS);
            let mut out = Vec::with_capacity(arr.len().min(cap));
            for item in arr.iter().take(cap) {
                let item_raw: BTreeMap<String, Value> = item
                    .as_object()
                    .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();
                let hydrated = hydrate_field_values(
                    &spec.fields,
                    &item_raw,
                    ctx,
                    richtext_nonces.as_deref_mut(),
                    visibility.child_visibility(),
                )?;
                let mut obj = Map::with_capacity(hydrated.len());
                for (k, v) in hydrated {
                    obj.insert(k, v);
                }
                out.push(Value::Object(obj));
            }
            Ok(Value::Array(out))
        }
    }
}

pub fn hydrate_field_values(
    specs: &[FieldSpec],
    raw_values: &BTreeMap<String, Value>,
    ctx: &HydrationContext<'_>,
    richtext_nonces: Option<&mut RichTextNonceTable>,
    visibility: HydrationVisibility,
) -> Result<BTreeMap<String, Value>, &'static str> {
    hydrate_field_values_with_richtext(
        specs,
        raw_values,
        ctx,
        richtext_nonces,
        visibility,
        None,
        None,
    )
}

pub fn hydrate_field_values_with_richtext(
    specs: &[FieldSpec],
    raw_values: &BTreeMap<String, Value>,
    ctx: &HydrationContext<'_>,
    mut richtext_nonces: Option<&mut RichTextNonceTable>,
    visibility: HydrationVisibility,
    mut richtext_html: Option<&mut BTreeMap<String, String>>,
    key_prefix: Option<&str>,
) -> Result<BTreeMap<String, Value>, &'static str> {
    let mut out = BTreeMap::new();
    for spec in specs {
        if !visibility.allows(spec) {
            continue;
        }
        let val = hydrate_field_value_with_richtext(
            spec,
            raw_values.get(&spec.key),
            ctx,
            richtext_nonces.as_deref_mut(),
            visibility,
            richtext_html.as_deref_mut(),
            key_prefix,
        )?;
        out.insert(spec.key.clone(), val);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../field_hydrate_tests.rs"]
mod tests;
