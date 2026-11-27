use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;
use serde_json::Value;

use crate::builder::{
    BuilderDocument, ColumnNode, ElementContent, ElementNode, KnownElement, RowNode, SectionNode,
};

use super::MAX_BLOCK_DIFF_NODES;
use super::canonical::canonical_json_value;
use super::types::{
    BlockDiffItem, BlockDiffKind, BlockDiffSummary, BlockNodeKind, BlockPropChange,
};

type NodeKey = (BlockNodeKind, String, String, usize);

#[derive(Debug, Clone)]
struct FlatBlockNode {
    key: NodeKey,
    label: String,
    parent_id: Option<String>,
    path: String,
    order_index: usize,
    props: BTreeMap<String, Value>,
}

fn insert_serialized<T: Serialize>(
    props: &mut BTreeMap<String, Value>,
    key: &str,
    value: &Option<T>,
) {
    if let Some(item) = value
        && let Ok(val) = serde_json::to_value(item)
    {
        let canonical = canonical_json_value(&val);
        if canonical != Value::Object(Default::default()) && !canonical.is_null() {
            props.insert(key.to_owned(), canonical);
        }
    }
}

fn element_type_and_label(content: &ElementContent) -> (String, String, BTreeMap<String, Value>) {
    let mut props = BTreeMap::new();
    match content {
        ElementContent::Unresolved(map) => {
            for (k, v) in map {
                props.insert(format!("unresolved.{k}"), canonical_json_value(v));
            }
            let raw_type = map
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unresolved");
            (raw_type.to_owned(), raw_type.to_owned(), props)
        }
        ElementContent::Known(known) => {
            if let Some(binding) = known.binding()
                && let Ok(val) = serde_json::to_value(binding)
            {
                props.insert("binding".to_owned(), canonical_json_value(&val));
            }
            match known {
                KnownElement::Heading(p) => {
                    insert_props_object(&mut props, p, &["binding"]);
                    (
                        "core/heading".to_owned(),
                        format!("Heading (H{})", p.level),
                        props,
                    )
                }
                KnownElement::Text(p) => {
                    insert_props_object(&mut props, p, &["binding"]);
                    ("core/text".to_owned(), "Text".to_owned(), props)
                }
                KnownElement::Image(p) => {
                    insert_props_object(&mut props, p, &["binding"]);
                    ("core/image".to_owned(), "Image".to_owned(), props)
                }
                KnownElement::Video(p) => {
                    insert_props_object(&mut props, p, &[]);
                    ("core/video".to_owned(), "Video".to_owned(), props)
                }
                KnownElement::Button(p) => {
                    insert_props_object(&mut props, p, &["binding"]);
                    ("core/button".to_owned(), "Button".to_owned(), props)
                }
                KnownElement::List(p) => {
                    insert_props_object(&mut props, p, &[]);
                    (
                        "core/list".to_owned(),
                        format!("List ({})", p.type_key),
                        props,
                    )
                }
                KnownElement::ContactForm(p) => {
                    insert_props_object(&mut props, p, &[]);
                    (
                        "core/contact_form".to_owned(),
                        format!("Contact Form (#{})", p.form_id),
                        props,
                    )
                }
                KnownElement::SearchForm(p) => {
                    insert_props_object(&mut props, p, &[]);
                    (
                        "core/search_form".to_owned(),
                        "Search Form".to_owned(),
                        props,
                    )
                }
                KnownElement::SearchResults(p) => {
                    insert_props_object(&mut props, p, &[]);
                    (
                        "core/search_results".to_owned(),
                        "Search Results".to_owned(),
                        props,
                    )
                }
                KnownElement::ContentSlot(_) => (
                    "core/content_slot".to_owned(),
                    "Content Slot".to_owned(),
                    props,
                ),
                KnownElement::PatternRef(p) => {
                    insert_props_object(&mut props, p, &[]);
                    (
                        "core/pattern_ref".to_owned(),
                        format!("Pattern (#{})", p.pattern_id),
                        props,
                    )
                }
                KnownElement::TemplatePart(p) => {
                    insert_props_object(&mut props, p, &[]);
                    (
                        "core/template_part".to_owned(),
                        format!("Template Part ({})", p.part_key),
                        props,
                    )
                }
                KnownElement::Legacy(p) => {
                    insert_props_object(&mut props, p, &[]);
                    (
                        format!("legacy:{}", p.block_id),
                        format!("Legacy ({})", p.block_id),
                        props,
                    )
                }
                KnownElement::PluginBlock(p) => {
                    for (k, v) in &p.values {
                        props.insert(format!("values.{k}"), canonical_json_value(v));
                    }
                    (
                        format!("plugin:{}", p.block_id),
                        format!("Block ({})", p.block_id),
                        props,
                    )
                }
            }
        }
    }
}

fn insert_props_object<T: Serialize>(
    out: &mut BTreeMap<String, Value>,
    value: &T,
    skip_keys: &[&str],
) {
    if let Ok(Value::Object(map)) = serde_json::to_value(value) {
        for (k, v) in map {
            if !skip_keys.contains(&k.as_str()) {
                out.insert(format!("props.{k}"), canonical_json_value(&v));
            }
        }
    }
}

fn next_occurrence(
    counts: &mut HashMap<(BlockNodeKind, String, String), usize>,
    kind: BlockNodeKind,
    block_type: &str,
    node_id: &str,
) -> usize {
    let entry = counts
        .entry((kind, block_type.to_owned(), node_id.to_owned()))
        .or_insert(0);
    let current = *entry;
    *entry += 1;
    current
}

fn flatten_document(doc: Option<&BuilderDocument>, limit: usize) -> (Vec<FlatBlockNode>, usize) {
    let Some(doc) = doc else {
        return (Vec::new(), 0);
    };
    let mut nodes = Vec::new();
    let mut total = 0usize;
    let mut counts = HashMap::new();

    for (s_idx, section) in doc.sections.iter().enumerate() {
        total += 1;
        if nodes.len() < limit {
            nodes.push(build_section_node(section, s_idx, &mut counts));
        }
        for (r_idx, row) in section.children.iter().enumerate() {
            total += 1;
            let s_path = format!("sections[{s_idx}]");
            if nodes.len() < limit {
                nodes.push(build_row_node(
                    row,
                    &section.id,
                    &s_path,
                    r_idx,
                    &mut counts,
                ));
            }
            for (c_idx, col) in row.children.iter().enumerate() {
                total += 1;
                let r_path = format!("{s_path}.rows[{r_idx}]");
                if nodes.len() < limit {
                    nodes.push(build_column_node(col, &row.id, &r_path, c_idx, &mut counts));
                }
                for (e_idx, elem) in col.children.iter().enumerate() {
                    total += 1;
                    let c_path = format!("{r_path}.columns[{c_idx}]");
                    if nodes.len() < limit {
                        nodes.push(build_element_node(
                            elem,
                            &col.id,
                            &c_path,
                            e_idx,
                            &mut counts,
                        ));
                    }
                }
            }
        }
    }
    (nodes, total)
}

fn build_section_node(
    section: &SectionNode,
    order_index: usize,
    counts: &mut HashMap<(BlockNodeKind, String, String), usize>,
) -> FlatBlockNode {
    let mut props = BTreeMap::new();
    insert_serialized(&mut props, "style", &section.style);
    insert_serialized(&mut props, "responsive", &section.responsive);
    insert_serialized(&mut props, "meta", &section.meta);
    let label = section
        .meta
        .as_ref()
        .and_then(|m| m.name.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("Section")
        .to_owned();
    let occ = next_occurrence(counts, BlockNodeKind::Section, "section", &section.id);
    FlatBlockNode {
        key: (
            BlockNodeKind::Section,
            "section".to_owned(),
            section.id.clone(),
            occ,
        ),
        label,
        parent_id: None,
        path: format!("sections[{order_index}]"),
        order_index,
        props,
    }
}

fn build_row_node(
    row: &RowNode,
    parent_id: &str,
    parent_path: &str,
    order_index: usize,
    counts: &mut HashMap<(BlockNodeKind, String, String), usize>,
) -> FlatBlockNode {
    let mut props = BTreeMap::new();
    insert_serialized(&mut props, "responsive", &row.responsive);
    insert_serialized(&mut props, "meta", &row.meta);
    let label = row
        .meta
        .as_ref()
        .and_then(|m| m.name.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("Row")
        .to_owned();
    let occ = next_occurrence(counts, BlockNodeKind::Row, "row", &row.id);
    FlatBlockNode {
        key: (BlockNodeKind::Row, "row".to_owned(), row.id.clone(), occ),
        label,
        parent_id: Some(parent_id.to_owned()),
        path: format!("{parent_path}.rows[{order_index}]"),
        order_index,
        props,
    }
}

fn build_column_node(
    col: &ColumnNode,
    parent_id: &str,
    parent_path: &str,
    order_index: usize,
    counts: &mut HashMap<(BlockNodeKind, String, String), usize>,
) -> FlatBlockNode {
    let mut props = BTreeMap::new();
    insert_serialized(&mut props, "responsive", &col.responsive);
    insert_serialized(&mut props, "meta", &col.meta);
    let label = col
        .meta
        .as_ref()
        .and_then(|m| m.name.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("Column")
        .to_owned();
    let occ = next_occurrence(counts, BlockNodeKind::Column, "column", &col.id);
    FlatBlockNode {
        key: (
            BlockNodeKind::Column,
            "column".to_owned(),
            col.id.clone(),
            occ,
        ),
        label,
        parent_id: Some(parent_id.to_owned()),
        path: format!("{parent_path}.columns[{order_index}]"),
        order_index,
        props,
    }
}

fn build_element_node(
    elem: &ElementNode,
    parent_id: &str,
    parent_path: &str,
    order_index: usize,
    counts: &mut HashMap<(BlockNodeKind, String, String), usize>,
) -> FlatBlockNode {
    let (block_type, fallback_label, mut props) = element_type_and_label(&elem.content);
    insert_serialized(&mut props, "meta", &elem.meta);
    let label = elem
        .meta
        .as_ref()
        .and_then(|m| m.name.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or(&fallback_label)
        .to_owned();
    let occ = next_occurrence(counts, BlockNodeKind::Element, &block_type, &elem.id);
    FlatBlockNode {
        key: (BlockNodeKind::Element, block_type, elem.id.clone(), occ),
        label,
        parent_id: Some(parent_id.to_owned()),
        path: format!("{parent_path}.elements[{order_index}]"),
        order_index,
        props,
    }
}

fn collect_prop_changes(
    prefix: &str,
    before: &Value,
    after: &Value,
    out: &mut Vec<BlockPropChange>,
) {
    if before == after {
        return;
    }
    if let (Value::Object(bm), Value::Object(am)) = (before, after) {
        let mut keys = BTreeSet::new();
        keys.extend(bm.keys().map(String::as_str));
        keys.extend(am.keys().map(String::as_str));
        for k in keys {
            let next_path = format!("{prefix}.{k}");
            let bv = bm.get(k).unwrap_or(&Value::Null);
            let av = am.get(k).unwrap_or(&Value::Null);
            collect_prop_changes(&next_path, bv, av, out);
        }
        return;
    }
    out.push(BlockPropChange {
        prop_path: prefix.to_owned(),
        before: before.clone(),
        after: after.clone(),
    });
}

fn diff_prop_maps(
    before: &BTreeMap<String, Value>,
    after: &BTreeMap<String, Value>,
) -> Vec<BlockPropChange> {
    let mut keys = BTreeSet::new();
    keys.extend(before.keys().map(String::as_str));
    keys.extend(after.keys().map(String::as_str));
    let mut changes = Vec::new();
    for k in keys {
        let bv = before.get(k).unwrap_or(&Value::Null);
        let av = after.get(k).unwrap_or(&Value::Null);
        collect_prop_changes(k, bv, av, &mut changes);
    }
    changes
}

pub fn compute_builder_block_diff(
    before: Option<&BuilderDocument>,
    after: Option<&BuilderDocument>,
) -> BlockDiffSummary {
    let per_side_limit = MAX_BLOCK_DIFF_NODES / 2;
    let (before_nodes, total_nodes_before) = flatten_document(before, per_side_limit);
    let (after_nodes, total_nodes_after) = flatten_document(after, per_side_limit);
    let truncated = total_nodes_before > per_side_limit || total_nodes_after > per_side_limit;

    let before_map: HashMap<&NodeKey, &FlatBlockNode> =
        before_nodes.iter().map(|n| (&n.key, n)).collect();
    let after_map: HashMap<&NodeKey, &FlatBlockNode> =
        after_nodes.iter().map(|n| (&n.key, n)).collect();

    let mut items = Vec::new();
    let mut unchanged_count = 0usize;

    for b in &before_nodes {
        if !after_map.contains_key(&b.key) {
            items.push(BlockDiffItem {
                kind: BlockDiffKind::Removed,
                node_id: b.key.2.clone(),
                node_kind: b.key.0,
                block_type: b.key.1.clone(),
                label: b.label.clone(),
                before_path: Some(b.path.clone()),
                after_path: None,
                before_order: Some(b.order_index),
                after_order: None,
                prop_changes: Vec::new(),
            });
        }
    }

    for a in &after_nodes {
        let Some(b) = before_map.get(&a.key) else {
            items.push(BlockDiffItem {
                kind: BlockDiffKind::Added,
                node_id: a.key.2.clone(),
                node_kind: a.key.0,
                block_type: a.key.1.clone(),
                label: a.label.clone(),
                before_path: None,
                after_path: Some(a.path.clone()),
                before_order: None,
                after_order: Some(a.order_index),
                prop_changes: Vec::new(),
            });
            continue;
        };
        let prop_changes = diff_prop_maps(&b.props, &a.props);
        let moved =
            b.parent_id != a.parent_id || b.order_index != a.order_index || b.path != a.path;
        if moved {
            items.push(BlockDiffItem {
                kind: BlockDiffKind::Moved,
                node_id: a.key.2.clone(),
                node_kind: a.key.0,
                block_type: a.key.1.clone(),
                label: a.label.clone(),
                before_path: Some(b.path.clone()),
                after_path: Some(a.path.clone()),
                before_order: Some(b.order_index),
                after_order: Some(a.order_index),
                prop_changes,
            });
        } else if !prop_changes.is_empty() {
            items.push(BlockDiffItem {
                kind: BlockDiffKind::Modified,
                node_id: a.key.2.clone(),
                node_kind: a.key.0,
                block_type: a.key.1.clone(),
                label: a.label.clone(),
                before_path: Some(b.path.clone()),
                after_path: Some(a.path.clone()),
                before_order: Some(b.order_index),
                after_order: Some(a.order_index),
                prop_changes,
            });
        } else {
            unchanged_count += 1;
        }
    }

    BlockDiffSummary {
        total_nodes_before,
        total_nodes_after,
        truncated,
        unchanged_count,
        items,
    }
}
