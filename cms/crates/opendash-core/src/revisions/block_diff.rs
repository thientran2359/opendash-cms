use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;
use serde_json::Value;

use crate::builder::{BlockNode, BuilderDocument};

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

fn node_kind_for_block_id(block_id: &str) -> BlockNodeKind {
    match block_id {
        "core/section" => BlockNodeKind::Section,
        "core/row" => BlockNodeKind::Row,
        "core/column" => BlockNodeKind::Column,
        _ => BlockNodeKind::Element,
    }
}

fn default_label_for_node(node: &BlockNode) -> String {
    match node.block_id.as_str() {
        "core/section" => "Section".to_owned(),
        "core/row" => "Row".to_owned(),
        "core/column" => "Column".to_owned(),
        "core/stack" => "Stack".to_owned(),
        "core/heading" => node.props.get("level").and_then(Value::as_str).map_or_else(
            || "Heading".to_owned(),
            |lvl| format!("Heading ({})", lvl.to_ascii_uppercase()),
        ),
        "core/text" => "Text".to_owned(),
        "core/image" => "Image".to_owned(),
        "core/video" => "Video".to_owned(),
        "core/button" => "Button".to_owned(),
        "core/list" => format!(
            "List ({})",
            node.props
                .get("type_key")
                .and_then(Value::as_str)
                .unwrap_or("post")
        ),
        "core/contact-form" => "Contact Form".to_owned(),
        "core/search-form" => "Search Form".to_owned(),
        "core/search-results" => "Search Results".to_owned(),
        "core/entry-content" => "Entry Content".to_owned(),
        "core/loop-item" => "Loop Item".to_owned(),
        "core/pattern-ref" => node
            .props
            .get("pattern_id")
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string())
            })
            .map_or_else(|| "Pattern".to_owned(), |pid| format!("Pattern (#{pid})")),
        "core/template-part" => node
            .props
            .get("part_key")
            .and_then(Value::as_str)
            .map_or_else(
                || "Template Part".to_owned(),
                |key| format!("Template Part ({key})"),
            ),
        other => format!("Block ({other})"),
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

fn flatten_nodes_rec(
    items: &[BlockNode],
    parent_id: Option<&str>,
    parent_path: Option<&str>,
    limit: usize,
    nodes: &mut Vec<FlatBlockNode>,
    total: &mut usize,
    counts: &mut HashMap<(BlockNodeKind, String, String), usize>,
) {
    for (idx, node) in items.iter().enumerate() {
        *total += 1;
        let path = match parent_path {
            Some(prefix) => format!("{prefix}.children[{idx}]"),
            None => format!("blocks[{idx}]"),
        };
        if nodes.len() < limit {
            let kind = node_kind_for_block_id(&node.block_id);
            let mut props = BTreeMap::new();
            for (k, v) in &node.props {
                props.insert(format!("props.{k}"), canonical_json_value(v));
            }
            insert_serialized(&mut props, "meta", &node.meta);
            let label = node
                .meta
                .as_ref()
                .and_then(|m| m.name.as_deref())
                .filter(|s| !s.is_empty())
                .map_or_else(|| default_label_for_node(node), str::to_owned);
            let occ = next_occurrence(counts, kind, &node.block_id, &node.id);
            nodes.push(FlatBlockNode {
                key: (kind, node.block_id.clone(), node.id.clone(), occ),
                label,
                parent_id: parent_id.map(str::to_owned),
                path: path.clone(),
                order_index: idx,
                props,
            });
        }
        flatten_nodes_rec(
            &node.children,
            Some(node.id.as_str()),
            Some(path.as_str()),
            limit,
            nodes,
            total,
            counts,
        );
    }
}

fn flatten_document(doc: Option<&BuilderDocument>, limit: usize) -> (Vec<FlatBlockNode>, usize) {
    let Some(doc) = doc else {
        return (Vec::new(), 0);
    };
    let mut nodes = Vec::new();
    let mut total = 0usize;
    let mut counts = HashMap::new();
    flatten_nodes_rec(
        &doc.blocks,
        None,
        None,
        limit,
        &mut nodes,
        &mut total,
        &mut counts,
    );
    (nodes, total)
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

/// Computes a bounded structural & prop diff across two V1 `BuilderDocument` snapshots.
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
