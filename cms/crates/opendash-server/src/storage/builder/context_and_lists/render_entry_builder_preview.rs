use super::*;

pub(in crate::storage) async fn render_entry_builder_preview(
    tx: &mut Transaction<'_, Sqlite>,
    current: &EntryBuilderSnapshot,
    state: &mut BuilderRenderContext,
    document: &BuilderDocument,
    shared_preview: bool,
) -> ApiResult<RenderedLayout> {
    render_entry_builder_preview_with_scope(
        tx,
        current,
        state,
        document,
        shared_preview,
        RenderAnnotationScope::None,
    )
    .await
}

pub(in crate::storage) async fn render_entry_builder_preview_with_scope(
    tx: &mut Transaction<'_, Sqlite>,
    current: &EntryBuilderSnapshot,
    state: &mut BuilderRenderContext,
    document: &BuilderDocument,
    shared_preview: bool,
    annotation_scope: RenderAnnotationScope,
) -> ApiResult<RenderedLayout> {
    let mut budget = page::RenderBudget::new();
    let mut rendered = render_entry_builder_preview_with_budget(
        tx,
        current,
        state,
        document,
        shared_preview,
        annotation_scope,
        &mut budget,
        page::BlockTemplateCache::global(),
    )
    .await?;
    rendered.html =
        crate::content_render::finalize_document_icons(rendered.html, &budget.icon_nonces)?;
    rendered.html =
        crate::content_render::finalize_document_images(rendered.html, &budget.image_nonces)?;
    rendered.html =
        crate::content_render::finalize_document_richtext(rendered.html, &budget.richtext_nonces)?;
    Ok(rendered)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn render_entry_builder_preview_with_budget(
    tx: &mut Transaction<'_, Sqlite>,
    current: &EntryBuilderSnapshot,
    state: &mut BuilderRenderContext,
    document: &BuilderDocument,
    shared_preview: bool,
    annotation_scope: RenderAnnotationScope,
    budget: &mut page::RenderBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    hydrate_render_context_for_documents_in_conn(&mut **tx, state, &[document], true).await?;
    let lists = resolve_lists_in_tx_for_locale_with_budget(
        tx,
        document,
        &BTreeMap::new(),
        Some(current.locale.as_str()),
        budget,
    )
    .await?;
    let forms = resolve_contact_forms_in_tx(tx, document).await?;
    let patterns =
        resolve_patterns_in_tx_with_budget(tx, state, document, true, budget, cache).await?;
    let parts = BTreeMap::new();
    let context = shared_preview.then_some(page::BuilderTemplateContext {
        content_html: None,
        parts: &parts,
        search: None,
    });
    let scope = if current.opened.type_key == "pattern" {
        format!("pat_{}", current.opened.entry_id)
    } else {
        "entry".to_owned()
    };
    page::render_builder_with_budget(
        state,
        document,
        true,
        &lists,
        &forms,
        context.as_ref(),
        &patterns,
        false,
        annotation_scope,
        &scope,
        budget,
        cache,
    )
}

impl BuilderRenderContext {
    pub fn unified_block_schemas(
        &self,
    ) -> ApiResult<RegistryCatalog<opendash_core::block_schema::RegisteredBlockSchema>> {
        use opendash_core::registry::OwnedDescriptor;
        let mut catalog = RegistryCatalog::new();
        for (block_id, def) in &self.declared_blocks {
            if !valid_namespaced_block_id(block_id) {
                return Err(ApiError::internal("Invalid declared block ID"));
            }
            catalog.insert_loaded(
                block_id.clone(),
                OwnedDescriptor {
                    owner: def.owner.clone(),
                    status: def.status,
                    declaration_state: def.declaration_state,
                    descriptor: def.schema.clone(),
                },
            );
        }
        Ok(catalog)
    }

    #[cfg(test)]
    pub fn validate(&self, document: &BuilderDocument) -> ApiResult<()> {
        document
            .validate_for_save(
                &self.unified_block_schemas()?,
                &BuilderContextTarget::EntryBody {
                    content_type: "page".to_owned(),
                },
                None,
            )
            .map_err(|error| ApiError::bad_request(error.to_string()))
    }
}
