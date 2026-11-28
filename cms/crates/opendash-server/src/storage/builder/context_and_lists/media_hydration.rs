use super::*;

fn collect_default_media_ids(
    fields: &[opendash_core::field_schema::FieldSpec],
    media_ids: &mut BTreeSet<String>,
) {
    use opendash_core::field_schema::{FieldType, valid_sha256_media_id};
    for spec in fields {
        if spec.is_ui_only() || spec.field_type == FieldType::Secret {
            continue;
        }
        match spec.field_type {
            FieldType::Group | FieldType::Repeater => {
                collect_default_media_ids(&spec.fields, media_ids);
            }
            FieldType::Media => {
                if let Some(id) = spec
                    .default
                    .as_ref()
                    .and_then(Value::as_str)
                    .filter(|s| valid_sha256_media_id(s))
                {
                    media_ids.insert(id.to_owned());
                }
            }
            _ => {}
        }
    }
}

fn collect_builder_nodes_hydration_rec(
    nodes: &[opendash_core::builder::BlockNode],
    declared_blocks: &BTreeMap<String, crate::storage::page::RegisteredBlockDef>,
    media_ids: &mut BTreeSet<String>,
    entry_ids: &mut BTreeSet<i64>,
    term_ids: &mut BTreeSet<i64>,
) {
    for node in nodes {
        let Some(def) = declared_blocks.get(&node.block_id) else {
            continue;
        };
        if def.status != ProviderStatus::Active
            || def.declaration_state != DeclarationState::Declared
        {
            continue;
        }
        crate::field_hydrate::collect_hydration_ids(
            &def.schema.schema.fields,
            &node.props,
            crate::field_hydrate::HydrationVisibility::All,
            media_ids,
            entry_ids,
            term_ids,
        );
        collect_default_media_ids(&def.schema.schema.fields, media_ids);
        collect_builder_nodes_hydration_rec(
            &node.children,
            declared_blocks,
            media_ids,
            entry_ids,
            term_ids,
        );
    }
}

pub(super) fn collect_builder_document_media_ids(
    document: &BuilderDocument,
    declared_blocks: &BTreeMap<String, crate::storage::page::RegisteredBlockDef>,
    media_ids: &mut BTreeSet<String>,
    entry_ids: &mut BTreeSet<i64>,
    term_ids: &mut BTreeSet<i64>,
) {
    collect_builder_nodes_hydration_rec(
        &document.blocks,
        declared_blocks,
        media_ids,
        entry_ids,
        term_ids,
    );
}
