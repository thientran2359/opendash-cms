//! Extracts public-safe, ACL-filtered plain text from content entries for
//! FTS5 indexing and structured snippet generation.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::{
    block_schema::RegisteredBlockSchema,
    builder::BuilderDocument,
    builder_blocks::{
        contains_blocks, extract_document_visible_text, extract_visible_text_with_schemas,
    },
    content::{EffectiveContentSchema, EffectiveFieldStatus},
    field_schema::{FieldRole, FieldSpec, FieldType, PublicExposure},
    registry::RegistryCatalog,
    richtext::strip_richtext_to_plain,
};

use super::normalize::{
    MAX_INDEX_BODY_BYTES, MAX_INDEX_SUMMARY_BYTES, MAX_INDEX_TERMS_BYTES, MAX_INDEX_TITLE_BYTES,
    fold_vietnamese_stroke, truncate_utf8_bytes,
};

const MAX_FIELD_RECURSION_DEPTH: usize = 4;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchableEntryDocument {
    pub title_folded: String,
    pub summary_folded: String,
    pub body_folded: String,
    pub terms_folded: String,
    pub snippet_source_text: String,
}

/// Extracts only fields satisfying all 3 public ACL axes
/// (`Active` && `Public` && `min_view_role <= Editor`) and indexable text types
/// (`Text | Textarea | Richtext`, plus nested `Group`/`Repeater` containers).
pub fn extract_searchable_document(
    title: &str,
    values: &BTreeMap<String, Value>,
    effective_schema: &EffectiveContentSchema,
    legacy_builder_doc: Option<&BuilderDocument>,
    block_schemas: &RegistryCatalog<RegisteredBlockSchema>,
    public_term_names: &[String],
) -> SearchableEntryDocument {
    let ordered_keys = ordered_schema_keys(effective_schema);
    let mut summary_parts = Vec::new();
    let mut body_parts = Vec::new();

    for key in ordered_keys {
        let Some(fe) = effective_schema.effective_fields.get(key) else {
            continue;
        };
        let (min_view_role, _min_edit_role, public_exposure) = fe.definition.effective_acl();
        if fe.effective_status != EffectiveFieldStatus::Active
            || public_exposure != PublicExposure::Public
            || min_view_role > FieldRole::Editor
        {
            continue;
        }

        let extracted = extract_spec_text(
            &fe.definition,
            values.get(key),
            if key == "body" {
                legacy_builder_doc
            } else {
                None
            },
            block_schemas,
            0,
        );
        if extracted.is_empty() {
            continue;
        }
        if matches!(key, "summary" | "excerpt" | "description") {
            summary_parts.push(extracted);
        } else {
            body_parts.push(extracted);
        }
    }

    let summary_plain = collapse_spaces(&summary_parts.join(" "));
    let body_plain = collapse_spaces(&body_parts.join(" "));
    let snippet_source_text = match (summary_plain.is_empty(), body_plain.is_empty()) {
        (true, true) => String::new(),
        (false, true) => summary_plain.clone(),
        (true, false) => body_plain.clone(),
        (false, false) => format!("{summary_plain} {body_plain}"),
    };

    let title_folded = truncate_utf8_bytes(
        &fold_vietnamese_stroke(&collapse_spaces(title)),
        MAX_INDEX_TITLE_BYTES,
    )
    .to_owned();
    let summary_folded = truncate_utf8_bytes(
        &fold_vietnamese_stroke(&summary_plain),
        MAX_INDEX_SUMMARY_BYTES,
    )
    .to_owned();
    let body_folded =
        truncate_utf8_bytes(&fold_vietnamese_stroke(&body_plain), MAX_INDEX_BODY_BYTES).to_owned();
    let terms_joined = collapse_spaces(&public_term_names.join(" "));
    let terms_folded = truncate_utf8_bytes(
        &fold_vietnamese_stroke(&terms_joined),
        MAX_INDEX_TERMS_BYTES,
    )
    .to_owned();

    SearchableEntryDocument {
        title_folded,
        summary_folded,
        body_folded,
        terms_folded,
        snippet_source_text,
    }
}

fn extract_spec_text(
    spec: &FieldSpec,
    raw_value: Option<&Value>,
    legacy_builder_doc: Option<&BuilderDocument>,
    block_schemas: &RegistryCatalog<RegisteredBlockSchema>,
    depth: usize,
) -> String {
    if depth > MAX_FIELD_RECURSION_DEPTH {
        return String::new();
    }
    match spec.field_type {
        FieldType::Text | FieldType::Textarea | FieldType::Richtext => {
            if depth == 0 && spec.key == "body" {
                extract_body_field(
                    spec.field_type,
                    raw_value,
                    legacy_builder_doc,
                    block_schemas,
                )
            } else {
                extract_standard_text_field(spec.field_type, raw_value, block_schemas)
            }
        }
        FieldType::Group => {
            let Some(obj) = raw_value.and_then(Value::as_object) else {
                return String::new();
            };
            extract_nested_object(&spec.fields, obj, block_schemas, depth + 1)
        }
        FieldType::Repeater => {
            let Some(items) = raw_value.and_then(Value::as_array) else {
                return String::new();
            };
            let mut parts = Vec::new();
            for item in items {
                let Some(obj) = item.as_object() else {
                    continue;
                };
                let nested = extract_nested_object(&spec.fields, obj, block_schemas, depth + 1);
                if !nested.is_empty() {
                    parts.push(nested);
                }
            }
            collapse_spaces(&parts.join(" "))
        }
        _ => String::new(),
    }
}

fn extract_nested_object(
    fields: &[FieldSpec],
    obj: &serde_json::Map<String, Value>,
    block_schemas: &RegistryCatalog<RegisteredBlockSchema>,
    depth: usize,
) -> String {
    if depth > MAX_FIELD_RECURSION_DEPTH {
        return String::new();
    }
    let mut parts = Vec::new();
    for child in fields {
        if child.is_withdrawn() {
            continue;
        }
        let (min_view_role, _min_edit_role, public_exposure) = child.effective_acl();
        if public_exposure != PublicExposure::Public || min_view_role > FieldRole::Editor {
            continue;
        }
        let nested = extract_spec_text(child, obj.get(&child.key), None, block_schemas, depth);
        if !nested.is_empty() {
            parts.push(nested);
        }
    }
    collapse_spaces(&parts.join(" "))
}

fn ordered_schema_keys(effective_schema: &EffectiveContentSchema) -> Vec<&str> {
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::with_capacity(effective_schema.effective_fields.len());
    for field in &effective_schema.base.descriptor.fields {
        if seen.insert(field.key.as_str()) {
            ordered.push(field.key.as_str());
        }
    }
    for group in &effective_schema.attached_groups {
        for field in &group.fields {
            if seen.insert(field.key.as_str()) {
                ordered.push(field.key.as_str());
            }
        }
    }
    for key in effective_schema.effective_fields.keys() {
        if seen.insert(key.as_str()) {
            ordered.push(key.as_str());
        }
    }
    ordered
}

fn extract_body_field(
    field_type: FieldType,
    raw_value: Option<&Value>,
    legacy_builder_doc: Option<&BuilderDocument>,
    block_schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> String {
    if let Some(raw) = raw_value.and_then(Value::as_str) {
        if contains_blocks(raw) {
            return collapse_spaces(&extract_visible_text_with_schemas(raw, block_schemas));
        }
        let cleaned = if field_type == FieldType::Richtext {
            strip_richtext_to_plain(raw)
        } else {
            collapse_spaces(raw)
        };
        if !cleaned.is_empty() {
            return cleaned;
        }
    }
    if let Some(doc) = legacy_builder_doc {
        return collapse_spaces(&extract_document_visible_text(doc, block_schemas));
    }
    String::new()
}

fn extract_standard_text_field(
    field_type: FieldType,
    raw_value: Option<&Value>,
    block_schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> String {
    let Some(raw) = raw_value.and_then(Value::as_str) else {
        return String::new();
    };
    if field_type == FieldType::Richtext {
        if contains_blocks(raw) {
            collapse_spaces(&extract_visible_text_with_schemas(raw, block_schemas))
        } else {
            strip_richtext_to_plain(raw)
        }
    } else {
        collapse_spaces(raw)
    }
}

fn collapse_spaces(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}
