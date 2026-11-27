use super::*;

pub(in crate::storage) fn apply_builder_list_term_remap(
    doc: &mut opendash_core::builder::BuilderDocument,
    remap: &BTreeMap<i64, i64>,
) -> bool {
    let mut changed = false;
    for section in &mut doc.sections {
        for row in &mut section.children {
            for col in &mut row.children {
                for el in &mut col.children {
                    if let opendash_core::builder::ElementContent::Known(
                        opendash_core::builder::KnownElement::List(props),
                    ) = &mut el.content
                    {
                        if let Some(old) = props.category_id {
                            let next = remap.get(&old).copied();
                            if next != props.category_id {
                                props.category_id = next;
                                changed = true;
                            }
                        }
                        if let Some(old) = props.tag_id {
                            let next = remap.get(&old).copied();
                            if next != props.tag_id {
                                props.tag_id = next;
                                changed = true;
                            }
                        }
                        if let Some(old) = props.term_id {
                            let next = remap.get(&old).copied();
                            if next != props.term_id {
                                props.term_id = next;
                                if next.is_none() {
                                    props.taxonomy_key = None;
                                }
                                changed = true;
                            }
                        }
                        if props.include_descendants
                            && props.category_id.is_none()
                            && props.taxonomy_key.is_none()
                        {
                            props.include_descendants = false;
                            changed = true;
                        }
                    }
                }
            }
        }
    }
    changed
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
