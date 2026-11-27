//! Pure zero-SQL template hydrator transforming raw `FieldSpec` values into
//! unified V1 MiniJinja objects per `docs/contracts/UNIFIED_FIELD_SCHEMA.md` §6.

use std::collections::{BTreeMap, BTreeSet};

use opendash_core::{
    field_schema::{
        FieldSpec, FieldType, LinkValue, MAX_REPEATER_ITEMS, SpacingProperty, SpacingValue,
        canonicalize_f64, collect_media_ids, collect_relational_refs, default_value_for_field_spec,
        valid_safe_url, valid_sha256_media_id,
    },
    icons::lookup_core_icon_paths,
    richtext::sanitize_richtext,
};

use serde_json::{Map, Value, json};

use crate::{
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

pub(crate) fn build_media_urls_and_srcsets(
    row: &HydratedMediaRow,
    is_preview: bool,
) -> (
    String,
    String,
    Vec<opendash_core::media::MediaVariantView>,
    Option<String>,
    Option<String>,
) {
    let prefix = if is_preview {
        "/api/admin/media"
    } else {
        "/media"
    };
    let version = row.version.max(1);
    let file_url = format!("{prefix}/{}/file?v={version}", row.id);
    let thumbnail_url = if row.kind == "image" {
        format!("{prefix}/{}/thumbnail?v={version}", row.id)
    } else {
        String::new()
    };
    let variants: Vec<opendash_core::media::MediaVariantView> = row
        .variants
        .iter()
        .map(|v| opendash_core::media::MediaVariantView {
            spec: v.spec,
            format: v.format,
            width: v.width,
            height: v.height,
            byte_size: v.byte_size,
            url: format!(
                "{prefix}/{}/variant/{}.{}?v={version}",
                row.id,
                v.spec.as_str(),
                v.format.extension()
            ),
        })
        .collect();
    let (webp_srcset, fallback_srcset) = if row.kind == "image" {
        let orig_w = row.width.and_then(|w| u32::try_from(w).ok()).unwrap_or(0);
        if orig_w > 0 {
            opendash_core::media::build_responsive_srcsets(
                &row.mime_type,
                orig_w,
                &file_url,
                &variants,
            )
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };
    (
        file_url,
        thumbnail_url,
        variants,
        webp_srcset,
        fallback_srcset,
    )
}

pub fn hydrate_media_row(row: &HydratedMediaRow, is_preview: bool) -> Value {
    let version = row.version.max(1);
    let (file_url, thumbnail_url, variants, webp_srcset, fallback_srcset) =
        build_media_urls_and_srcsets(row, is_preview);
    json!({
        "id": row.id,
        "version": version,
        "url": file_url,
        "thumbnail_url": thumbnail_url,
        "alt": "",
        "width": row.width,
        "height": row.height,
        "kind": row.kind,
        "variants": variants,
        "webp_srcset": webp_srcset,
        "fallback_srcset": fallback_srcset,
    })
}

pub fn hydrate_media_row_for_template(row: &HydratedMediaRow, is_preview: bool) -> Value {
    let mut val = hydrate_media_row(row, is_preview);
    if let Value::Object(ref mut map) = val {
        map.insert(
            crate::template_sec::MEDIA_PROVENANCE_KEY.to_owned(),
            Value::String(crate::template_sec::media_provenance_token().to_owned()),
        );
    }
    val
}

fn hydrate_media_for_ctx(row: &HydratedMediaRow, ctx: &HydrationContext<'_>) -> Value {
    if ctx.tag_media_provenance {
        hydrate_media_row_for_template(row, ctx.is_preview)
    } else {
        hydrate_media_row(row, ctx.is_preview)
    }
}

fn hydrate_entry_row(row: &HydratedEntryRefRow) -> Value {
    json!({
        "id": row.id,
        "type_key": row.type_key,
        "title": row.title,
        "slug": row.slug,
        "url": row.url,
        "cover_url": row.cover_url,
        "excerpt": row.excerpt,
    })
}

fn hydrate_term_row(row: &HydratedTermRefRow, ctx: &HydrationContext<'_>) -> Value {
    let featured_image = row
        .featured_image_media_id
        .as_deref()
        .and_then(|id| ctx.media.get(id))
        .map(|m| hydrate_media_for_ctx(m, ctx))
        .unwrap_or(Value::Null);
    json!({
        "id": row.id,
        "kind": row.taxonomy_key,
        "name": row.name,
        "slug": row.slug,
        "url": row.url,
        "featured_image": featured_image,
    })
}

fn extract_raw_spacing(val: &Value) -> Option<SpacingValue> {
    if let Ok(mut s) = serde_json::from_value::<SpacingValue>(val.clone()) {
        s.canonicalize();
        return Some(s);
    }
    None
}

fn hydrate_spacing_value(spec: &FieldSpec, raw_val: Option<&Value>) -> Value {
    let effective = raw_val.or(spec.default.as_ref());
    let prop = spec.effective_spacing_property();
    let unit = spec.effective_unit();

    if let Some(obj) = effective.and_then(Value::as_object)
        && (obj.contains_key("desktop") || obj.contains_key("tablet") || obj.contains_key("mobile"))
    {
        let desktop = obj.get("desktop").and_then(extract_raw_spacing);
        let tablet = obj.get("tablet").and_then(extract_raw_spacing);
        let mobile = obj.get("mobile").and_then(extract_raw_spacing);
        let classes =
            SpacingValue::format_responsive_classes(desktop.clone(), tablet, mobile, prop, unit);
        let base = desktop.unwrap_or_default();
        let css = base.format_css(unit).unwrap_or_default();
        return json!({
            "top": base.top,
            "right": base.right,
            "bottom": base.bottom,
            "left": base.left,
            "css": css,
            "classes": classes,
        });
    }

    let mut base = effective.and_then(extract_raw_spacing).unwrap_or_default();
    base.canonicalize();
    let css = base.format_css(unit).unwrap_or_default();
    let prefix = match prop {
        SpacingProperty::Padding => SpacingProperty::Padding,
        SpacingProperty::Margin => SpacingProperty::Margin,
    };
    let classes = base.format_classes(prefix, unit);
    json!({
        "top": base.top,
        "right": base.right,
        "bottom": base.bottom,
        "left": base.left,
        "css": css,
        "classes": classes,
    })
}

fn hydrate_link_value(spec: &FieldSpec, raw_val: Option<&Value>) -> Value {
    let candidate = match raw_val {
        Some(v) if !v.is_null() => Some(v),
        _ => spec.default.as_ref(),
    };
    let Some(val) = candidate else {
        return Value::Null;
    };
    let Ok(link) = serde_json::from_value::<LinkValue>(val.clone()) else {
        return Value::Null;
    };
    let trimmed_url = link.url.trim();
    if trimmed_url.is_empty() || !valid_safe_url(trimmed_url) {
        return Value::Null;
    }
    let target = if link.new_tab { "_blank" } else { "" };
    let rel = match (link.new_tab, link.nofollow) {
        (true, true) => "noopener noreferrer nofollow",
        (true, false) => "noopener noreferrer",
        (false, true) => "nofollow",
        (false, false) => "",
    };
    json!({
        "url": trimmed_url,
        "label": link.label,
        "target": target,
        "rel": rel,
        "new_tab": link.new_tab,
        "nofollow": link.nofollow,
    })
}

fn parse_i64_id(val: &Value) -> Option<i64> {
    val.as_i64().filter(|id| *id > 0)
}

fn unwrap_responsive_desktop<'a>(
    spec: &FieldSpec,
    raw_val: Option<&'a Value>,
) -> Option<&'a Value> {
    let val = raw_val?;
    if spec.responsive
        && let Some(obj) = val.as_object()
        && (obj.contains_key("desktop") || obj.contains_key("tablet") || obj.contains_key("mobile"))
    {
        return obj.get("desktop");
    }
    Some(val)
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
    mut richtext_nonces: Option<&mut RichTextNonceTable>,
    visibility: HydrationVisibility,
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
            let hydrated = hydrate_field_values(
                &spec.fields,
                &child_raw,
                ctx,
                richtext_nonces,
                visibility.child_visibility(),
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
    mut richtext_nonces: Option<&mut RichTextNonceTable>,
    visibility: HydrationVisibility,
) -> Result<BTreeMap<String, Value>, &'static str> {
    let mut out = BTreeMap::new();
    for spec in specs {
        if !visibility.allows(spec) {
            continue;
        }
        let val = hydrate_field_value(
            spec,
            raw_values.get(&spec.key),
            ctx,
            richtext_nonces.as_deref_mut(),
            visibility,
        )?;
        out.insert(spec.key.clone(), val);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "field_hydrate_tests.rs"]
mod tests;
