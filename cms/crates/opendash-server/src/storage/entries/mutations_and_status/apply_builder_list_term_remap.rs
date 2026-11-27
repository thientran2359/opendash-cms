use super::*;

fn remap_nodes(
    nodes: &mut [opendash_core::builder::BlockNode],
    remap: &BTreeMap<i64, i64>,
) -> bool {
    let mut changed = false;
    for node in nodes {
        if node.block_id == "core/list" {
            if let Some(old) =
                opendash_core::builder::parse_positive_i64_prop(node.props.get("term_id"))
            {
                match remap.get(&old).copied() {
                    Some(next) if next != old => {
                        node.props
                            .insert("term_id".to_owned(), serde_json::Value::from(next));
                        changed = true;
                    }
                    None => {
                        node.props.remove("term_id");
                        node.props.remove("taxonomy_key");
                        changed = true;
                    }
                    _ => {}
                }
            }
            if node
                .props
                .get("include_descendants")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
                && node
                    .props
                    .get("taxonomy_key")
                    .and_then(serde_json::Value::as_str)
                    .is_none_or(|s| s.trim().is_empty())
            {
                node.props.insert(
                    "include_descendants".to_owned(),
                    serde_json::Value::Bool(false),
                );
                changed = true;
            }
        }
        if remap_nodes(&mut node.children, remap) {
            changed = true;
        }
    }
    changed
}

pub(in crate::storage) fn apply_builder_list_term_remap(
    doc: &mut opendash_core::builder::BuilderDocument,
    remap: &BTreeMap<i64, i64>,
) -> bool {
    remap_nodes(&mut doc.blocks, remap)
}

pub(in crate::storage) async fn missing_or_stale(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
) -> ApiResult<ApiError> {
    if get_entry(db, type_key, id).await?.is_some() {
        Ok(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        ))
    } else {
        Ok(ApiError::not_found("Không tìm thấy nội dung."))
    }
}
