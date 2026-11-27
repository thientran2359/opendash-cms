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
    pub l10n_locale: Option<String>,
    pub l10n_domain: Option<String>,
    pub l10n_catalogs: BTreeMap<String, std::sync::Arc<opendash_core::l10n::Catalog>>,
}

impl Default for RenderFormattingConfig {
    fn default() -> Self {
        Self {
            timezone: "UTC".to_owned(),
            l10n_locale: None,
            l10n_domain: None,
            l10n_catalogs: BTreeMap::new(),
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
    field_template_node_from_spec_with_visibility(
        spec,
        opendash_core::field_schema::HydrationVisibility::All,
    )
}

pub fn field_template_node_from_spec_with_visibility(
    spec: &opendash_core::field_schema::FieldSpec,
    visibility: opendash_core::field_schema::HydrationVisibility,
) -> Option<FieldTemplateNode> {
    use opendash_core::field_schema::FieldType;

    if !visibility.allows(spec) {
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
        FieldType::Group | FieldType::Repeater => {
            let child_vis = visibility.child_visibility();
            let children = spec
                .fields
                .iter()
                .filter_map(|child| {
                    field_template_node_from_spec_with_visibility(child, child_vis)
                        .map(|node| (child.key.clone(), node))
                })
                .collect();
            Some(if spec.field_type == FieldType::Group {
                FieldTemplateNode::Group(children)
            } else {
                FieldTemplateNode::Repeater(children)
            })
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
    pub(crate) taxonomy_term_nodes: BTreeMap<String, FieldTemplateNode>,
    pub(crate) custom_options_nodes: BTreeMap<String, BTreeMap<String, FieldTemplateNode>>,
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
            taxonomy_term_nodes: BTreeMap::new(),
            custom_options_nodes: BTreeMap::new(),
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
            if let Some(node) = field_template_node_from_spec_with_visibility(
                &spec,
                opendash_core::field_schema::HydrationVisibility::OptionsPublicOnly,
            ) {
                site_options_tree.insert(spec.key, node);
            }
        }
        let mut options_tree = site_options_tree.clone();
        for spec in theme_options {
            if let Some(node) = field_template_node_from_spec_with_visibility(
                spec,
                opendash_core::field_schema::HydrationVisibility::OptionsPublicOnly,
            ) {
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
            taxonomy_term_nodes: BTreeMap::new(),
            custom_options_nodes: BTreeMap::new(),
        }
    }

    pub(crate) fn without_cpt_entry_fields(&self) -> Self {
        let mut copy = self.clone();
        copy.entry_field_nodes.clear();
        copy.taxonomy_term_nodes.clear();
        copy
    }

    pub(crate) fn with_cpt_entry_fields(
        &self,
        fields: &[opendash_core::field_schema::FieldSpec],
    ) -> Self {
        let mut copy = self.clone();
        copy.entry_field_nodes.clear();
        let vis = opendash_core::field_schema::HydrationVisibility::ContentPublic {
            max_view_role: opendash_core::content::FieldRole::Editor,
        };
        for spec in fields {
            if let Some(node) = field_template_node_from_spec_with_visibility(spec, vis) {
                copy.entry_field_nodes.insert(spec.key.clone(), node);
            }
        }
        copy
    }

    pub(crate) fn with_taxonomy_term_fields(
        &self,
        fields: &[opendash_core::field_schema::FieldSpec],
    ) -> Self {
        let mut copy = self.clone();
        copy.taxonomy_term_nodes.clear();
        let vis = opendash_core::field_schema::HydrationVisibility::ContentPublic {
            max_view_role: opendash_core::content::FieldRole::Editor,
        };
        for spec in fields {
            if let Some(node) = field_template_node_from_spec_with_visibility(spec, vis) {
                copy.taxonomy_term_nodes.insert(spec.key.clone(), node);
            }
        }
        copy
    }

    pub(crate) fn with_custom_options_pages(
        &self,
        pages: &BTreeMap<String, Vec<opendash_core::field_schema::FieldSpec>>,
    ) -> Self {
        let mut copy = self.clone();
        copy.custom_options_nodes.clear();
        let vis = opendash_core::field_schema::HydrationVisibility::OptionsPublicOnly;
        for (page_key, specs) in pages {
            let mut nodes = BTreeMap::new();
            for spec in specs {
                if let Some(node) = field_template_node_from_spec_with_visibility(spec, vis) {
                    nodes.insert(spec.key.clone(), node);
                }
            }
            copy.custom_options_nodes.insert(page_key.clone(), nodes);
        }
        copy
    }

    pub(crate) fn with_extra_site_and_theme_options(
        &self,
        extra_site: &[opendash_core::field_schema::FieldSpec],
        extra_theme: &[opendash_core::field_schema::FieldSpec],
    ) -> Self {
        let mut copy = self.clone();
        let vis = opendash_core::field_schema::HydrationVisibility::OptionsPublicOnly;
        for spec in extra_site {
            if let Some(node) = field_template_node_from_spec_with_visibility(spec, vis) {
                copy.site_options_tree
                    .entry(spec.key.clone())
                    .or_insert_with(|| node.clone());
                copy.options_tree.entry(spec.key.clone()).or_insert(node);
            }
        }
        for spec in extra_theme {
            if let Some(node) = field_template_node_from_spec_with_visibility(spec, vis) {
                copy.options_tree.entry(spec.key.clone()).or_insert(node);
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

    pub(crate) fn entry_fields_fingerprint(&self) -> String {
        if self.entry_field_nodes.is_empty()
            && self.taxonomy_term_nodes.is_empty()
            && self.custom_options_nodes.is_empty()
        {
            return format!(
                "so={}|to={}",
                self.site_options_tree.len(),
                self.options_tree.len()
            );
        }
        use sha2::{Digest, Sha256};
        let payload = format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}",
            self.entry_field_nodes,
            self.taxonomy_term_nodes,
            self.custom_options_nodes,
            self.site_options_tree,
            self.options_tree
        );
        let digest = Sha256::digest(payload.as_bytes());
        format!("{digest:x}")
    }
}
