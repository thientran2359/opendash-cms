mod tests;
mod validate;

pub use validate::{
    matches_pattern_list, valid_allowed_child_token, valid_allowed_context_token,
    valid_allowed_parent_token, validate_block_catalog_templates,
};

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::field_schema::{
    CollectedMediaIds, FieldContext, FieldSpec, SelectOptionsSource,
    collect_media_ids as collect_unified_media_ids, collect_options_source_values,
    extract_visible_text as extract_unified_visible_text, validate_field_values,
};

pub const BLOCK_SCHEMA_VERSION: u8 = 1;
pub const MAX_NODE_VALUES_KEYS: usize = 64;
pub const MAX_NODE_VALUES_BYTES: usize = 65_536;
pub const MAX_BLOCK_PRESETS: usize = 12;
pub const MAX_DEFAULT_CHILDREN_DEPTH: usize = 2;
pub const MAX_DEFAULT_CHILDREN_NODES: usize = 8;

/// Khóa trường bị cấm trong `BlockSchema.fields` để tránh shadow biến hệ thống C-SEC.
pub const RESERVED_BLOCK_FIELD_KEYS: &[&str] = &[
    "children_html",
    "children_meta",
    "parent_ctx",
    "site_options",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockCategory {
    Layout,
    Media,
    Interactive,
    Content,
    Marketing,
    Template,
    Plugin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultChildSpec {
    pub block_id: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DefaultChildSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockPresetSpec {
    pub key: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DefaultChildSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockSchema {
    pub schema_version: u8,
    pub id: String,
    pub title: String,
    pub category: BlockCategory,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub render: String,
    #[serde(default)]
    pub supports_children: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_parents: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_children: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_contexts: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_children: Vec<DefaultChildSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<BlockPresetSpec>,
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisteredBlockSchema {
    pub schema: BlockSchema,
}

impl BlockSchema {
    /// Phase 1 local validation of a block schema declaration under `expected_owner_prefix`.
    pub fn validate(&self, expected_owner_prefix: &str) -> Result<(), &'static str> {
        validate::validate_block_schema(self, expected_owner_prefix)
    }

    /// Returns true iff this non-core block may safely render inside `core/loop-item`.
    pub fn is_loop_schema_compatible(&self) -> bool {
        fn has_disallowed_loop_field(fields: &[FieldSpec]) -> bool {
            use crate::field_schema::FieldType;
            fields.iter().any(|field| {
                matches!(
                    field.field_type,
                    FieldType::Repeater
                        | FieldType::Richtext
                        | FieldType::Gallery
                        | FieldType::EntryRef
                        | FieldType::TermRef
                ) || field.options_source.is_some()
                    || has_disallowed_loop_field(&field.fields)
            })
        }

        let explicit_loop = self.allowed_contexts.as_ref().is_some_and(|tokens| {
            tokens.iter().any(|t| t == "loop_item") && !tokens.iter().any(|t| t == "!loop_item")
        });
        explicit_loop && !self.supports_children && !has_disallowed_loop_field(&self.fields)
    }

    /// Re-hydrates missing prop keys using top-level `field.default` values without
    /// overwriting explicit `null` or empty values provided in `values`.
    pub fn apply_field_defaults(
        &self,
        values: &BTreeMap<String, Value>,
    ) -> BTreeMap<String, Value> {
        let mut out = values.clone();
        for field in &self.fields {
            if field.is_ui_only() {
                continue;
            }
            if !out.contains_key(&field.key)
                && let Some(default_val) = &field.default
            {
                out.insert(field.key.clone(), default_val.clone());
            }
        }
        out
    }

    /// Sparse props normalization: removes a top-level prop key ONLY when its JSON
    /// value strictly equals (`==`) `field.default`. Never strips `""` if `default != ""`.
    pub fn strip_default_props(&self, props: &mut BTreeMap<String, Value>) {
        for field in &self.fields {
            if field.is_ui_only() {
                continue;
            }
            if let Some(default_val) = &field.default
                && props.get(&field.key) == Some(default_val)
            {
                props.remove(&field.key);
            }
        }
    }

    pub fn validate_values(
        &self,
        values: &BTreeMap<String, Value>,
        previous_values: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), &'static str> {
        if values.len() > MAX_NODE_VALUES_KEYS {
            return Err("Số lượng khóa props của block vượt quá giới hạn 64.");
        }
        let serialized_len = serde_json::to_vec(values)
            .map_err(|_| "Không thể tuần tự hóa dữ liệu block.")?
            .len();
        if serialized_len > MAX_NODE_VALUES_BYTES {
            return Err("Kích thước dữ liệu props của block vượt quá 64 KiB.");
        }

        let effective = self.apply_field_defaults(values);
        validate_field_values(
            &self.fields,
            FieldContext::StudioBlock,
            &effective,
            previous_values,
        )
    }

    pub fn collect_media_ids(&self, values: &BTreeMap<String, Value>) -> CollectedMediaIds {
        collect_unified_media_ids(&self.fields, values)
    }

    pub fn extract_visible_text(&self, values: &BTreeMap<String, Value>, out: &mut Vec<String>) {
        extract_unified_visible_text(&self.fields, values, out);
    }

    pub fn collect_pattern_ids(&self, values: &BTreeMap<String, Value>) -> BTreeSet<i64> {
        collect_options_source_values(&self.fields, values, SelectOptionsSource::Patterns)
            .into_iter()
            .filter_map(|raw| raw.parse::<i64>().ok().filter(|&id| id > 0))
            .collect()
    }

    pub fn has_non_empty_pattern_source_value(&self, values: &BTreeMap<String, Value>) -> bool {
        !collect_options_source_values(&self.fields, values, SelectOptionsSource::Patterns)
            .is_empty()
    }

    pub fn collect_template_part_keys(&self, values: &BTreeMap<String, Value>) -> Vec<String> {
        collect_options_source_values(&self.fields, values, SelectOptionsSource::TemplateParts)
    }

    pub fn has_active_contact_form(&self, values: &BTreeMap<String, Value>) -> bool {
        !collect_options_source_values(&self.fields, values, SelectOptionsSource::ContactForms)
            .is_empty()
    }
}
