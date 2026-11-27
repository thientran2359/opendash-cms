use super::*;
use opendash_core::builder::{BlockNode, BuilderDocument};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) fn node(id: &str, block_id: &str, props: Value, children: Vec<BlockNode>) -> BlockNode {
    let map = props
        .as_object()
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    BlockNode {
        id: id.into(),
        block_id: block_id.into(),
        props: map,
        children,
        meta: None,
    }
}

pub(super) fn doc_in_section(
    sec_id: &str,
    row_id: &str,
    col_id: &str,
    children: Vec<BlockNode>,
) -> BuilderDocument {
    BuilderDocument {
        schema_version: 1,
        blocks: vec![node(
            sec_id,
            "core/section",
            json!({}),
            vec![node(
                row_id,
                "core/row",
                json!({}),
                vec![node(col_id, "core/column", json!({}), children)],
            )],
        )],
    }
}

mod suite_01;
mod suite_02;
mod suite_03;
mod suite_04;
