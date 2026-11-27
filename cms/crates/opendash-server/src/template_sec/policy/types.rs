use super::*;

pub const MAX_RENDERED_HTML_BYTES: usize = 512 * 1024;

/// Mirrors `document::MAX_PUBLIC_DOCUMENT_BYTES` without coupling `template_sec.rs`
/// to `crate::document` (some integration tests include `template_sec.rs` standalone).
pub const MAX_ICON_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

pub const MAX_ICONS_PER_REQUEST: usize = 128;

pub const TEMPLATE_FUEL_LIMIT: u64 = 10_000;

/// Host formatting configuration supplied per request or preview to C-SEC helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFormattingConfig {
    pub timezone: String,
}

impl Default for RenderFormattingConfig {
    fn default() -> Self {
        Self {
            timezone: "UTC".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldTemplateNode {
    Scalar,
    Url,
    Link,
    Media,
    Gallery,
    Icon,
    Richtext,
    EntryRef { multiple: bool },
    TermRef { multiple: bool },
    Group(BTreeMap<String, FieldTemplateNode>),
    Repeater(BTreeMap<String, FieldTemplateNode>),
}

pub fn field_template_node_from_spec(
    spec: &opendash_core::field_schema::FieldSpec,
) -> Option<FieldTemplateNode> {
    use opendash_core::field_schema::FieldType;

    if spec.is_ui_only() || spec.field_type == FieldType::Secret {
        return None;
    }
    match spec.field_type {
        FieldType::Url => Some(FieldTemplateNode::Url),
        FieldType::Link => Some(FieldTemplateNode::Link),
        FieldType::Media => Some(FieldTemplateNode::Media),
        FieldType::Gallery => Some(FieldTemplateNode::Gallery),
        FieldType::Icon => Some(FieldTemplateNode::Icon),
        FieldType::Richtext => Some(FieldTemplateNode::Richtext),
        FieldType::EntryRef => Some(FieldTemplateNode::EntryRef {
            multiple: spec.multiple,
        }),
        FieldType::TermRef => Some(FieldTemplateNode::TermRef {
            multiple: spec.multiple,
        }),
        FieldType::Group => {
            let mut children = BTreeMap::new();
            for child in &spec.fields {
                if let Some(node) = field_template_node_from_spec(child) {
                    children.insert(child.key.clone(), node);
                }
            }
            Some(FieldTemplateNode::Group(children))
        }
        FieldType::Repeater => {
            let mut children = BTreeMap::new();
            for child in &spec.fields {
                if let Some(node) = field_template_node_from_spec(child) {
                    children.insert(child.key.clone(), node);
                }
            }
            Some(FieldTemplateNode::Repeater(children))
        }
        FieldType::Text
        | FieldType::Textarea
        | FieldType::Number
        | FieldType::Range
        | FieldType::Email
        | FieldType::Select
        | FieldType::Checkbox
        | FieldType::RadioButtons
        | FieldType::Boolean
        | FieldType::Color
        | FieldType::Date
        | FieldType::Datetime
        | FieldType::Time
        | FieldType::Spacing => Some(FieldTemplateNode::Scalar),
        FieldType::Secret | FieldType::Tab | FieldType::Accordion | FieldType::Message => None,
    }
}

/// Explicit, manifest-derived theme URL and declared-asset permissions.
#[derive(Debug, Clone, Default)]
pub struct ThemeUrlPolicy {
    pub(crate) permitted_href_expressions: BTreeSet<String>,
    pub(crate) permitted_src_expressions: BTreeSet<String>,
    pub(crate) declared_assets: BTreeSet<String>,
    pub(crate) site_options_tree: BTreeMap<String, FieldTemplateNode>,
    pub(crate) options_tree: BTreeMap<String, FieldTemplateNode>,
    pub(crate) entry_field_nodes: BTreeMap<String, FieldTemplateNode>,
}

impl ThemeUrlPolicy {
    pub(crate) fn new(
        permitted_href_expressions: BTreeSet<String>,
        permitted_src_expressions: BTreeSet<String>,
        declared_assets: BTreeSet<String>,
    ) -> Self {
        Self {
            permitted_href_expressions,
            permitted_src_expressions,
            declared_assets,
            site_options_tree: BTreeMap::new(),
            options_tree: BTreeMap::new(),
            entry_field_nodes: BTreeMap::new(),
        }
    }

    pub(crate) fn from_option_keys(keys: impl IntoIterator<Item = String>) -> Self {
        Self::new(
            keys.into_iter()
                .map(|key| format!("options.{key}"))
                .collect(),
            BTreeSet::new(),
            BTreeSet::new(),
        )
    }

    pub(crate) fn from_option_specs(
        theme_options: &[opendash_core::field_schema::FieldSpec],
        declared_assets: BTreeSet<String>,
    ) -> Self {
        let mut site_options_tree = BTreeMap::new();
        for spec in opendash_core::field_schema::global_site_option_fields() {
            if spec.is_ui_only() || !spec.public {
                continue;
            }
            if let Some(node) = field_template_node_from_spec(&spec) {
                site_options_tree.insert(spec.key, node);
            }
        }
        let mut options_tree = site_options_tree.clone();
        for spec in theme_options {
            if spec.is_ui_only() || !spec.public {
                continue;
            }
            if let Some(node) = field_template_node_from_spec(spec) {
                options_tree.insert(spec.key.clone(), node);
            }
        }
        Self {
            permitted_href_expressions: BTreeSet::new(),
            permitted_src_expressions: BTreeSet::new(),
            declared_assets,
            site_options_tree,
            options_tree,
            entry_field_nodes: BTreeMap::new(),
        }
    }

    pub(crate) fn without_cpt_entry_fields(&self) -> Self {
        let mut copy = self.clone();
        copy.entry_field_nodes.clear();
        copy
    }

    pub(crate) fn with_cpt_entry_fields(
        &self,
        fields: &[opendash_core::field_schema::FieldSpec],
    ) -> Self {
        let mut copy = self.clone();
        copy.entry_field_nodes.clear();
        for spec in fields {
            if spec.is_ui_only()
                || spec.effective_acl().2 != opendash_core::content::PublicExposure::Public
            {
                continue;
            }
            if let Some(node) = field_template_node_from_spec(spec) {
                copy.entry_field_nodes.insert(spec.key.clone(), node);
            }
        }
        copy
    }

    pub(crate) fn allows_cite_href(&self, expr: &str) -> bool {
        self.permitted_href_expressions.contains(expr)
    }

    pub(crate) fn allows_media_src(&self, expr: &str) -> bool {
        self.permitted_src_expressions.contains(expr)
    }

    pub(crate) fn allows_declared_asset(&self, path: &str) -> bool {
        self.declared_assets.contains(path)
    }
}
