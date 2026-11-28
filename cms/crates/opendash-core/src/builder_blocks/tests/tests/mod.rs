use super::*;
use crate::builder::BlockNode;
use serde_json::json;

fn bnode(
    id: &str,
    block_id: &str,
    props: serde_json::Value,
    children: Vec<BlockNode>,
) -> BlockNode {
    let map = props
        .as_object()
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    BlockNode {
        id: id.into(),
        block_id: block_id.into(),
        props: map,
        meta: None,
        children,
    }
}

mod suite_01;
mod suite_02;
