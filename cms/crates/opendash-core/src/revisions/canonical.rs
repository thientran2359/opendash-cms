use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::content::ContentMode;

use super::types::{CanonicalRevisionPayload, CanonicalRevisionSeo};

const MAX_SAFE_INT_F64: f64 = (1i64 << 53) as f64;

/// Recursively sorts JSON object keys and normalizes `-0.0` and safe integer floats
/// (`1.0 -> 1` strictly within `-(2^53)..=(2^53)`) to prevent spurious hash drift
/// across JS `JSON.stringify` and Rust `serde_json::Value` round-trips.
pub fn canonical_json_value(value: &Value) -> Value {
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => value.clone(),
        Value::Number(num) => {
            if let Some(i) = num.as_i64() {
                return Value::Number(i.into());
            }
            if let Some(u) = num.as_u64() {
                return Value::Number(u.into());
            }
            if let Some(f) = num.as_f64() {
                if f == 0.0 {
                    return Value::Number(0.into());
                }
                if f.is_finite()
                    && f.fract() == 0.0
                    && (-MAX_SAFE_INT_F64..=MAX_SAFE_INT_F64).contains(&f)
                {
                    return Value::Number((f as i64).into());
                }
            }
            Value::Number(num.clone())
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical_json_value).collect()),
        Value::Object(obj) => {
            let mut sorted: BTreeMap<&str, Value> = BTreeMap::new();
            for (k, v) in obj {
                sorted.insert(k.as_str(), canonical_json_value(v));
            }
            let mut out = Map::with_capacity(sorted.len());
            for (k, v) in sorted {
                out.insert(k.to_owned(), v);
            }
            Value::Object(out)
        }
    }
}

#[derive(Serialize)]
struct CanonicalHashEnvelope {
    title: String,
    content_mode: ContentMode,
    values: BTreeMap<String, Value>,
    builder_document: Option<Value>,
    term_ids: Vec<i64>,
    primary_category_term_id: Option<i64>,
    page_layout_key: Option<String>,
    custom_template_key: Option<String>,
    seo: Option<CanonicalRevisionSeo>,
}

pub fn canonical_revision_json(payload: &CanonicalRevisionPayload) -> String {
    let mut values = BTreeMap::new();
    for (k, v) in &payload.values {
        values.insert(k.clone(), canonical_json_value(v));
    }
    let builder_document = payload.builder_document_json.as_deref().map(|raw| {
        serde_json::from_str::<Value>(raw)
            .map(|parsed| canonical_json_value(&parsed))
            .unwrap_or_else(|_| Value::String(raw.to_owned()))
    });
    let mut term_ids = payload.term_ids.clone();
    term_ids.sort_unstable();
    term_ids.dedup();
    let seo = payload
        .seo
        .clone()
        .and_then(CanonicalRevisionSeo::into_normalized);

    let envelope = CanonicalHashEnvelope {
        title: payload.title.clone(),
        content_mode: payload.content_mode,
        values,
        builder_document,
        term_ids,
        primary_category_term_id: payload.primary_category_term_id,
        page_layout_key: payload
            .page_layout_key
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned),
        custom_template_key: payload
            .custom_template_key
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned),
        seo,
    };
    serde_json::to_string(&envelope).unwrap_or_default()
}
