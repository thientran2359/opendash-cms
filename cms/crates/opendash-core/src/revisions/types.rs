use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::content::ContentMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionSource {
    Create,
    Save,
    BuilderSave,
    BuilderReset,
    QuickEdit,
    SeoSave,
    PatternSave,
    PluginSave,
    Restore,
    Import,
    Baseline,
}

impl RevisionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Save => "save",
            Self::BuilderSave => "builder_save",
            Self::BuilderReset => "builder_reset",
            Self::QuickEdit => "quick_edit",
            Self::SeoSave => "seo_save",
            Self::PatternSave => "pattern_save",
            Self::PluginSave => "plugin_save",
            Self::Restore => "restore",
            Self::Import => "import",
            Self::Baseline => "baseline",
        }
    }

    pub fn from_db(raw: &str) -> Option<Self> {
        match raw {
            "create" => Some(Self::Create),
            "save" => Some(Self::Save),
            "builder_save" => Some(Self::BuilderSave),
            "builder_reset" => Some(Self::BuilderReset),
            "quick_edit" => Some(Self::QuickEdit),
            "seo_save" => Some(Self::SeoSave),
            "pattern_save" => Some(Self::PatternSave),
            "plugin_save" => Some(Self::PluginSave),
            "restore" => Some(Self::Restore),
            "import" => Some(Self::Import),
            "baseline" => Some(Self::Baseline),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalRevisionSeo {
    pub title: Option<String>,
    pub description: Option<String>,
    pub og_image_id: Option<String>,
    pub canonical_url: Option<String>,
    pub noindex: bool,
}

impl CanonicalRevisionSeo {
    /// Trims optional string fields and collapses completely empty SEO payloads to `None`
    /// so initial saves and missing `entry_seo` rows produce identical revision hashes.
    pub fn into_normalized(self) -> Option<Self> {
        let trim_opt = |opt: Option<String>| -> Option<String> {
            opt.and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_owned())
                }
            })
        };
        let normalized = Self {
            title: trim_opt(self.title),
            description: trim_opt(self.description),
            og_image_id: trim_opt(self.og_image_id),
            canonical_url: trim_opt(self.canonical_url),
            noindex: self.noindex,
        };
        if normalized.title.is_none()
            && normalized.description.is_none()
            && normalized.og_image_id.is_none()
            && normalized.canonical_url.is_none()
            && !normalized.noindex
        {
            None
        } else {
            Some(normalized)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalRevisionPayload {
    pub title: String,
    pub content_mode: ContentMode,
    pub values: BTreeMap<String, Value>,
    pub builder_document_json: Option<String>,
    pub term_ids: Vec<i64>,
    pub primary_category_term_id: Option<i64>,
    pub page_layout_key: Option<String>,
    pub custom_template_key: Option<String>,
    pub seo: Option<CanonicalRevisionSeo>,
    #[serde(default)]
    pub parent_id: Option<i64>,
    #[serde(default)]
    pub menu_order: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffOpKind {
    Equal,
    Added,
    Removed,
    Modified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordSpan {
    pub kind: DiffOpKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffOpKind,
    pub line_no_before: Option<usize>,
    pub line_no_after: Option<usize>,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub word_spans: Vec<WordSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDiffResult {
    pub truncated_diff: bool,
    pub hunks: Vec<TextDiffHunk>,
    pub unchanged_collapsed_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockNodeKind {
    Section,
    Row,
    Column,
    Element,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockDiffKind {
    Added,
    Removed,
    Moved,
    Modified,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockPropChange {
    pub prop_path: String,
    pub before: Value,
    pub after: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockDiffItem {
    pub kind: BlockDiffKind,
    pub node_id: String,
    pub node_kind: BlockNodeKind,
    pub block_type: String,
    pub label: String,
    pub before_path: Option<String>,
    pub after_path: Option<String>,
    pub before_order: Option<usize>,
    pub after_order: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prop_changes: Vec<BlockPropChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockDiffSummary {
    pub total_nodes_before: usize,
    pub total_nodes_after: usize,
    pub truncated: bool,
    pub unchanged_count: usize,
    pub items: Vec<BlockDiffItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeTransition {
    pub from: ContentMode,
    pub to: ContentMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDiffItem {
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub editable: bool,
    pub before: Value,
    pub after: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionTermRef {
    pub id: i64,
    pub name: String,
    pub taxonomy_key: Option<String>,
    pub missing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomyDiffSummary {
    pub added_terms: Vec<RevisionTermRef>,
    pub removed_terms: Vec<RevisionTermRef>,
    pub primary_category_before: Option<RevisionTermRef>,
    pub primary_category_after: Option<RevisionTermRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScalarChange<T> {
    pub before: T,
    pub after: T,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevisionDiffPayload {
    pub title_diff: TextDiffResult,
    pub mode_transition: Option<ModeTransition>,
    pub body_text_diff: Option<TextDiffResult>,
    pub body_block_diff: Option<BlockDiffSummary>,
    pub field_diffs: Vec<FieldDiffItem>,
    pub taxonomy_diff: TaxonomyDiffSummary,
    pub layout_diff: Option<ScalarChange<Option<String>>>,
    pub template_diff: Option<ScalarChange<Option<String>>>,
    pub seo_diff: Option<Vec<FieldDiffItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_diff: Option<ScalarChange<Option<i64>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_order_diff: Option<ScalarChange<i32>>,
    pub has_hidden_admin_changes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingMediaRef {
    pub media_id: String,
    pub location: String,
    pub expected_kind: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingListRef {
    pub node_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingBlockTypeRef {
    pub node_id: String,
    pub block_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingTemplateRef {
    pub kind: String,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingDynamicOptionRef {
    pub source: String,
    pub value: String,
    pub field_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedBlockFieldKey {
    pub node_id: String,
    pub block_type: String,
    pub field_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearedBindingNode {
    pub node_id: String,
    pub field_key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingRefsReport {
    pub missing_media: Vec<MissingMediaRef>,
    pub missing_term_ids: Vec<i64>,
    pub missing_entry_ids: Vec<i64>,
    pub missing_pattern_ids: Vec<i64>,
    pub missing_contact_form_ids: Vec<i64>,
    pub missing_dynamic_options: Vec<MissingDynamicOptionRef>,
    pub missing_list_refs: Vec<MissingListRef>,
    pub missing_block_types: Vec<MissingBlockTypeRef>,
    pub cleared_binding_nodes: Vec<ClearedBindingNode>,
    pub missing_templates: Vec<MissingTemplateRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<i64>,
    pub dropped_field_keys: Vec<String>,
    pub dropped_block_field_keys: Vec<DroppedBlockFieldKey>,
    pub non_editable_fields: Vec<String>,
}

impl MissingRefsReport {
    pub fn has_blocking_refs(&self) -> bool {
        !self.missing_media.is_empty()
            || !self.missing_term_ids.is_empty()
            || !self.missing_entry_ids.is_empty()
            || !self.missing_pattern_ids.is_empty()
            || !self.missing_contact_form_ids.is_empty()
            || !self.missing_dynamic_options.is_empty()
            || !self.missing_list_refs.is_empty()
            || !self.missing_block_types.is_empty()
            || !self.cleared_binding_nodes.is_empty()
            || !self.missing_templates.is_empty()
            || self.parent_id.is_some()
    }
}
