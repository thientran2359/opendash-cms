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

pub struct HydrationContext<'a> {
    pub media: &'a BTreeMap<String, HydratedMediaRow>,
    pub entries: &'a BTreeMap<i64, HydratedEntryRefRow>,
    pub terms: &'a BTreeMap<i64, HydratedTermRefRow>,
    pub is_preview: bool,
}

/// Collects all referenced `media_ids`, `entry_ids`, and `term_ids` across a set of
/// specs + merged raw values so the caller can execute at most 3 Zero-N+1 batch queries.
pub fn collect_hydration_ids(
    specs: &[FieldSpec],
    raw_values: &BTreeMap<String, Value>,
    only_public: bool,
    media_ids: &mut BTreeSet<String>,
    entry_ids: &mut BTreeSet<i64>,
    term_ids: &mut BTreeSet<i64>,
) {
    let subset: Vec<FieldSpec> = specs
        .iter()
        .filter(|spec| !spec.is_ui_only() && spec.field_type != FieldType::Secret)
        .filter(|spec| !only_public || spec.public)
        .cloned()
        .collect();
    if subset.is_empty() {
        return;
    }
    let collected_media = collect_media_ids(&subset, raw_values);
    media_ids.extend(collected_media.all_ids());
    let collected_rels = collect_relational_refs(&subset, raw_values);
    entry_ids.extend(collected_rels.entry_ids);
    term_ids.extend(collected_rels.term_ids);
}

pub fn hydrate_media_row(row: &HydratedMediaRow, is_preview: bool) -> Value {
    let prefix = if is_preview {
        "/api/admin/media"
    } else {
        "/media"
    };
    let thumbnail_url = if row.kind == "image" {
        format!("{prefix}/{}/thumbnail", row.id)
    } else {
        String::new()
    };
    json!({
        "id": row.id,
        "url": format!("{prefix}/{}/file", row.id),
        "thumbnail_url": thumbnail_url,
        "alt": "",
        "width": row.width,
        "height": row.height,
        "kind": row.kind,
    })
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
        .map(|m| hydrate_media_row(m, ctx.is_preview))
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

pub fn hydrate_field_value(
    spec: &FieldSpec,
    raw_val: Option<&Value>,
    ctx: &HydrationContext<'_>,
    mut richtext_nonces: Option<&mut RichTextNonceTable>,
) -> Result<Value, &'static str> {
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
                .map(|row| hydrate_media_row(row, ctx.is_preview))
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
                .map(|row| hydrate_media_row(row, ctx.is_preview))
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
            let hydrated =
                hydrate_field_values(&spec.fields, &child_raw, ctx, richtext_nonces, false)?;
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
                    false,
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
    only_public: bool,
) -> Result<BTreeMap<String, Value>, &'static str> {
    let mut out = BTreeMap::new();
    for spec in specs {
        if spec.is_ui_only() || spec.field_type == FieldType::Secret {
            continue;
        }
        if only_public && !spec.public {
            continue;
        }
        let val = hydrate_field_value(
            spec,
            raw_values.get(&spec.key),
            ctx,
            richtext_nonces.as_deref_mut(),
        )?;
        out.insert(spec.key.clone(), val);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendash_core::field_schema::FieldContext;

    fn sample_hash(fill: char) -> String {
        std::iter::repeat_n(fill, 64).collect()
    }

    #[test]
    fn hydrates_all_option_field_shapes_with_nonces_and_public_filter() {
        let img_id = sample_hash('a');
        let vid_id = sample_hash('b');
        let term_img_id = sample_hash('c');

        let specs: Vec<FieldSpec> = serde_json::from_value(json!([
            { "type": "tab", "key": "tab_main", "label": "Main" },
            { "type": "text", "key": "headline", "label": "Headline", "default": "Default", "public": true },
            { "type": "text", "key": "internal_note", "label": "Secret Note", "public": false },
            { "type": "media", "key": "hero_img", "label": "Hero", "public": true },
            { "type": "gallery", "key": "slides", "label": "Slides", "media_kind": "any", "public": true },
            { "type": "link", "key": "cta", "label": "CTA", "public": true },
            { "type": "icon", "key": "badge_icon", "label": "Icon", "public": true },
            { "type": "spacing", "key": "outer_margin", "label": "Margin", "property": "margin", "min": -128, "public": true },
            { "type": "richtext", "key": "bio", "label": "Bio", "public": true },
            { "type": "entry_ref", "key": "featured", "label": "Featured", "target_types": ["post"], "public": true },
            { "type": "term_ref", "key": "topic", "label": "Topic", "taxonomy": "category", "public": true },
            {
                "type": "group",
                "key": "banner",
                "label": "Banner",
                "public": true,
                "fields": [
                    { "type": "media", "key": "bg", "label": "BG" },
                    { "type": "link", "key": "btn", "label": "Btn" }
                ]
            },
            {
                "type": "repeater",
                "key": "cards",
                "label": "Cards",
                "public": true,
                "fields": [
                    { "type": "text", "key": "title", "label": "Title" },
                    { "type": "richtext", "key": "body", "label": "Body" }
                ]
            }
        ]))
        .unwrap();
        opendash_core::field_schema::validate_field_specs(&specs, FieldContext::Options).unwrap();

        let raw = BTreeMap::from([
            ("headline".into(), json!("OpenDash V1")),
            ("internal_note".into(), json!("do not leak")),
            ("hero_img".into(), json!(img_id)),
            ("slides".into(), json!([img_id, vid_id, sample_hash('f')])),
            (
                "cta".into(),
                json!({ "url": "/contact", "label": "Contact", "new_tab": true, "nofollow": true }),
            ),
            ("badge_icon".into(), json!("star")),
            ("outer_margin".into(), json!({ "top": -16, "bottom": 24 })),
            (
                "bio".into(),
                json!("<p>Hello <script>alert(1)</script><strong>world</strong></p>"),
            ),
            ("featured".into(), json!(10)),
            ("topic".into(), json!(5)),
            (
                "banner".into(),
                json!({ "bg": img_id, "btn": { "url": "", "label": "Empty" } }),
            ),
            (
                "cards".into(),
                json!([{ "title": "One", "body": "<p>Safe card</p>" }]),
            ),
        ]);

        let media = BTreeMap::from([
            (
                img_id.clone(),
                HydratedMediaRow {
                    id: img_id.clone(),
                    kind: "image".into(),
                    width: Some(1200),
                    height: Some(630),
                },
            ),
            (
                vid_id.clone(),
                HydratedMediaRow {
                    id: vid_id.clone(),
                    kind: "video".into(),
                    width: Some(1920),
                    height: Some(1080),
                },
            ),
            (
                term_img_id.clone(),
                HydratedMediaRow {
                    id: term_img_id.clone(),
                    kind: "image".into(),
                    width: Some(400),
                    height: Some(400),
                },
            ),
        ]);
        let entries = BTreeMap::from([(
            10,
            HydratedEntryRefRow {
                id: 10,
                type_key: "post".into(),
                title: "Post Ten".into(),
                slug: "post-ten".into(),
                url: "/post-ten".into(),
                cover_url: Some(format!("/media/{img_id}/file")),
                excerpt: "Summary".into(),
                published_at: None,
            },
        )]);
        let terms = BTreeMap::from([(
            5,
            HydratedTermRefRow {
                id: 5,
                taxonomy_key: "category".into(),
                name: "News".into(),
                slug: "news".into(),
                url: "/category/news".into(),
                featured_image_media_id: Some(term_img_id.clone()),
            },
        )]);
        let ctx = HydrationContext {
            media: &media,
            entries: &entries,
            terms: &terms,
            is_preview: false,
        };

        let mut nonces = RichTextNonceTable::default();
        let hydrated = hydrate_field_values(&specs, &raw, &ctx, Some(&mut nonces), true).unwrap();

        assert!(!hydrated.contains_key("tab_main"));
        assert!(!hydrated.contains_key("internal_note"));
        assert_eq!(hydrated["headline"], json!("OpenDash V1"));
        assert_eq!(
            hydrated["hero_img"]["url"],
            json!(format!("/media/{img_id}/file"))
        );
        assert_eq!(
            hydrated["hero_img"]["thumbnail_url"],
            json!(format!("/media/{img_id}/thumbnail"))
        );
        assert_eq!(hydrated["hero_img"]["width"], json!(1200));
        assert_eq!(hydrated["slides"].as_array().unwrap().len(), 2);
        assert_eq!(hydrated["slides"][1]["thumbnail_url"], json!(""));
        assert_eq!(hydrated["cta"]["target"], json!("_blank"));
        assert_eq!(
            hydrated["cta"]["rel"],
            json!("noopener noreferrer nofollow")
        );
        assert_eq!(hydrated["badge_icon"], json!({ "name": "star" }));
        assert!(
            hydrated["outer_margin"]["classes"]
                .as_str()
                .unwrap()
                .contains("-mt-4")
        );
        assert!(
            hydrated["outer_margin"]["classes"]
                .as_str()
                .unwrap()
                .contains("mb-6")
        );
        assert!(
            hydrated["bio"]
                .as_str()
                .unwrap()
                .starts_with("OD_RICHTEXT_")
        );
        assert_eq!(hydrated["featured"]["title"], json!("Post Ten"));
        assert_eq!(hydrated["topic"]["kind"], json!("category"));
        assert_eq!(
            hydrated["topic"]["featured_image"]["url"],
            json!(format!("/media/{term_img_id}/file"))
        );
        assert_eq!(hydrated["banner"]["bg"]["id"], json!(img_id));
        assert_eq!(hydrated["banner"]["btn"], Value::Null);
        let card_nonce = hydrated["cards"][0]["body"].as_str().unwrap();
        assert!(card_nonce.starts_with("OD_RICHTEXT_"));

        // Nonce substitution works inside <body> and fails closed inside attributes
        let html = format!(
            "<!doctype html><html><body><div>{}</div><article>{}</article></body></html>",
            hydrated["bio"].as_str().unwrap(),
            card_nonce
        );
        let substituted = nonces.substitute_into_document(html).unwrap();
        assert!(substituted.contains("<strong>world</strong>"));
        assert!(!substituted.contains("<script>"));
        assert!(substituted.contains("<p>Safe card</p>"));

        let bad_attr = format!("<div title=\"{}\"></div>", card_nonce);
        assert!(nonces.substitute_into_document(bad_attr).is_err());

        // Plain sanitized mode (for plugin host SiteOptionsGet) returns HTML without nonces
        let plain = hydrate_field_values(&specs, &raw, &ctx, None, true).unwrap();
        assert!(
            plain["bio"]
                .as_str()
                .unwrap()
                .contains("<strong>world</strong>")
        );
        assert!(!plain["bio"].as_str().unwrap().starts_with("OD_RICHTEXT_"));
    }

    #[test]
    fn hydrates_responsive_studio_block_scalars_and_nested_repeater_groups() {
        let specs: Vec<FieldSpec> = serde_json::from_value(json!([
            {
                "type": "radio_buttons",
                "key": "align",
                "label": "Align",
                "responsive": true,
                "default": "center",
                "options": [
                    { "value": "left", "label": "Left" },
                    { "value": "center", "label": "Center" }
                ]
            },
            {
                "type": "range",
                "key": "radius",
                "label": "Radius",
                "min": 0,
                "max": 32,
                "responsive": true,
                "default": 8
            },
            {
                "type": "select",
                "key": "layout",
                "label": "Layout",
                "responsive": true,
                "default": "grid",
                "options": [
                    { "value": "grid", "label": "Grid" },
                    { "value": "stack", "label": "Stack" }
                ]
            },
            {
                "type": "repeater",
                "key": "items",
                "label": "Items",
                "max_items": 1,
                "fields": [
                    { "type": "text", "key": "title", "label": "Title" },
                    {
                        "type": "group",
                        "key": "meta",
                        "label": "Meta",
                        "fields": [
                            { "type": "icon", "key": "badge", "label": "Badge" }
                        ]
                    }
                ]
            }
        ]))
        .unwrap();
        opendash_core::field_schema::validate_field_specs(&specs, FieldContext::StudioBlock)
            .unwrap();

        let raw = BTreeMap::from([
            (
                "align".into(),
                json!({ "desktop": "left", "mobile": "center" }),
            ),
            ("radius".into(), json!({ "tablet": 12 })),
            ("layout".into(), json!({ "desktop": "stack" })),
            (
                "items".into(),
                json!([
                    { "title": "First", "meta": { "badge": "star" } },
                    { "title": "Truncated", "meta": { "badge": "check" } }
                ]),
            ),
        ]);
        let media = BTreeMap::new();
        let entries = BTreeMap::new();
        let terms = BTreeMap::new();
        let ctx = HydrationContext {
            media: &media,
            entries: &entries,
            terms: &terms,
            is_preview: false,
        };
        let hydrated = hydrate_field_values(&specs, &raw, &ctx, None, false).unwrap();
        assert_eq!(hydrated["align"], json!("left"));
        assert_eq!(hydrated["radius"], json!(8));
        assert_eq!(hydrated["layout"], json!("stack"));
        assert_eq!(hydrated["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            hydrated["items"][0]["meta"]["badge"],
            json!({ "name": "star" })
        );
    }
}
