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

pub(super) fn collect_builder_document_media_ids(
    document: &BuilderDocument,
    declared_blocks: &BTreeMap<String, crate::storage::page::RegisteredBlockDef>,
    media_ids: &mut BTreeSet<String>,
    entry_ids: &mut BTreeSet<i64>,
    term_ids: &mut BTreeSet<i64>,
) {
    for section in &document.sections {
        if let Some(bg_media) = section
            .style
            .as_ref()
            .and_then(|s| s.background_media_id.as_deref())
            .filter(|id| opendash_core::field_schema::valid_sha256_media_id(id))
        {
            media_ids.insert(bg_media.to_owned());
        }
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    match &element.content {
                        ElementContent::Known(KnownElement::Image(props)) => {
                            if opendash_core::field_schema::valid_sha256_media_id(&props.media_id) {
                                media_ids.insert(props.media_id.clone());
                            }
                        }
                        ElementContent::Known(KnownElement::Video(props)) => {
                            if opendash_core::field_schema::valid_sha256_media_id(&props.media_id) {
                                media_ids.insert(props.media_id.clone());
                            }
                            if let Some(poster) = props
                                .poster_image_id
                                .as_deref()
                                .filter(|id| opendash_core::field_schema::valid_sha256_media_id(id))
                            {
                                media_ids.insert(poster.to_owned());
                            }
                        }
                        ElementContent::Known(KnownElement::PluginBlock(props)) => {
                            let Some(def) = declared_blocks.get(&props.block_id) else {
                                continue;
                            };
                            if def.status != ProviderStatus::Active
                                || def.declaration_state != DeclarationState::Declared
                            {
                                continue;
                            }
                            crate::field_hydrate::collect_hydration_ids(
                                &def.schema.schema.fields,
                                &props.values,
                                false,
                                media_ids,
                                entry_ids,
                                term_ids,
                            );
                            collect_default_media_ids(&def.schema.schema.fields, media_ids);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn collect_binding_media_ids(
    document: &BuilderDocument,
    bindings: &builder_binding::BindingResults,
) -> BTreeSet<String> {
    use opendash_core::{builder::BindingTarget, field_schema::valid_sha256_media_id};
    let mut ids = BTreeSet::new();
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if let ElementContent::Known(KnownElement::Image(props)) = &element.content
                        && props
                            .binding
                            .as_ref()
                            .is_some_and(|b| b.target == BindingTarget::MediaId)
                        && let Some(builder_binding::BindingResult::Value(mid)) =
                            bindings.get(&element.id)
                        && valid_sha256_media_id(mid)
                    {
                        ids.insert(mid.clone());
                    }
                }
            }
        }
    }
    ids
}

pub(crate) async fn hydrate_binding_media_in_conn(
    conn: &mut SqliteConnection,
    state: &mut BuilderRenderContext,
    document: &BuilderDocument,
    bindings: &builder_binding::BindingResults,
) -> ApiResult<()> {
    let missing: BTreeSet<String> = collect_binding_media_ids(document, bindings)
        .into_iter()
        .filter(|id| !state.hydration_media.contains_key(id))
        .collect();
    if !missing.is_empty() {
        state.hydration_media.extend(
            crate::storage::fields::load_media_hydration_map_in_conn(conn, &missing).await?,
        );
    }
    Ok(())
}

pub(crate) async fn hydrate_binding_media_cow_in_conn<'a>(
    conn: &mut SqliteConnection,
    state: &'a BuilderRenderContext,
    document: &BuilderDocument,
    bindings: &builder_binding::BindingResults,
) -> ApiResult<std::borrow::Cow<'a, BuilderRenderContext>> {
    let missing: BTreeSet<String> = collect_binding_media_ids(document, bindings)
        .into_iter()
        .filter(|id| !state.hydration_media.contains_key(id))
        .collect();
    if missing.is_empty() {
        return Ok(std::borrow::Cow::Borrowed(state));
    }
    let mut cloned = state.clone();
    cloned
        .hydration_media
        .extend(crate::storage::fields::load_media_hydration_map_in_conn(conn, &missing).await?);
    Ok(std::borrow::Cow::Owned(cloned))
}
