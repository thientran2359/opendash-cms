use super::*;

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn render_public_builder(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    slot: Option<&str>,
    depth: usize,
    stack: &[String],
    ctx: &PublicBuilderContext<'_>,
    budget: &mut PublicCompositionBudget,
    scope: &str,
) -> ApiResult<RenderedLayout> {
    let mut parts = BTreeMap::new();
    let mut child_diagnostics = Vec::new();
    let mut child_forms = false;
    let schemas = ctx.state.unified_block_schemas()?;
    // Count every occurrence, including repeated references. A stored Builder
    // part wins before consulting the immutable active-theme source.
    for key in document_parts_and_slot_count_with_schemas(document, &schemas)?.0 {
        budget.charge(1)?;
        if let Some(mut rendered) =
            Box::pin(render_public_part(tx, &key, depth + 1, stack, ctx, budget)).await?
        {
            child_forms |= rendered.has_active_contact_form;
            child_diagnostics.append(&mut rendered.diagnostics);
            parts.insert(key, rendered.html);
        }
    }
    let lists = builder::resolve_lists_in_tx_for_locale_with_budget(
        tx,
        document,
        ctx.list_pages,
        Some(ctx.render_locale),
        &mut budget.render_budget,
    )
    .await?;
    let forms = resolve_contact_forms_in_tx(tx, document).await?;
    let empty_patterns = page::PatternResults::new();
    let mut rendered = page::render_builder_with_budget_and_base_path(
        ctx.state,
        document,
        false,
        &lists,
        &forms,
        Some(&page::BuilderTemplateContext {
            content_html: slot,
            parts: &parts,
            search: ctx.search,
            template_data: ctx.template_data,
        }),
        &empty_patterns,
        false,
        RenderAnnotationScope::None,
        scope,
        ctx.list_base_path,
        &mut budget.render_budget,
        ctx.cache,
    )?;
    rendered.has_active_contact_form |= child_forms;
    rendered.diagnostics.append(&mut child_diagnostics);
    Ok(rendered)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn inspect_part_slot_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    key: &str,
    depth: usize,
    stack: &[String],
    active: Option<&ActiveTheme>,
    url_policy: &template_sec::ThemeUrlPolicy,
    schemas: &opendash_core::registry::RegistryCatalog<
        opendash_core::block_schema::RegisteredBlockSchema,
    >,
    acc: &mut SurfaceScanAcc,
) -> ApiResult<(bool, Option<Arc<template_sec::CompiledCsecComposition>>)> {
    let target = TemplateCandidate {
        part_key: Some(key.to_owned()),
        ..candidate(TemplateCandidateKind::Part)
    };
    let canonical = target.canonical_key().map_err(ApiError::internal)?;
    if stack.contains(&canonical) || depth > 3 {
        return Err(ApiError::internal("Template part cycle or depth limit."));
    }
    acc.part_expansions += 1;
    if acc.part_expansions > 32 {
        return Err(ApiError::internal("Template parts exceed 32 expansions."));
    }
    if let Some(cached) = acc.inspected_parts.get(&canonical) {
        return Ok((true, cached.clone()));
    }
    if let Some(row) = get_typed_override_on_connection(&mut **tx, &canonical).await? {
        let (_, slots, entry_blocks) = document_slot_scan_with_schemas(&row.document, schemas)?;
        validate_content_slots(&target, slots, entry_blocks)?;
        let mut next_stack = stack.to_vec();
        next_stack.push(canonical.clone());
        Box::pin(inspect_db_document_surfaces_in_tx(
            tx,
            &row.document,
            depth,
            &next_stack,
            active,
            url_policy,
            schemas,
            acc,
        ))
        .await?;
        acc.inspected_parts.insert(canonical, None);
        return Ok((true, None));
    }
    let Some(active) = active else {
        return Ok((false, None));
    };
    let Some(source) = active.parts.get(key) else {
        return Ok((false, None));
    };
    let compiled = template_sec::ThemeCompositionCache::global()
        .get_or_compile(
            &active.revision_digest,
            &canonical,
            source,
            &active.parts,
            template_sec::CsecContentKind::NonEntry,
            url_policy,
        )
        .map_err(ApiError::internal)?;
    if depth + compiled.max_part_depth > 3 {
        return Err(ApiError::internal("Template part depth exceeds 3."));
    }
    acc.merge_theme_compiled(&compiled);
    acc.inspected_parts
        .insert(canonical, Some(compiled.clone()));
    Ok((true, Some(compiled)))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn inspect_db_document_surfaces_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    depth: usize,
    stack: &[String],
    active: Option<&ActiveTheme>,
    url_policy: &template_sec::ThemeUrlPolicy,
    schemas: &opendash_core::registry::RegistryCatalog<
        opendash_core::block_schema::RegisteredBlockSchema,
    >,
    acc: &mut SurfaceScanAcc,
) -> ApiResult<()> {
    inspect_document_template_requirements(document, acc);
    for key in document_parts_and_slot_count_with_schemas(document, schemas)?.0 {
        Box::pin(inspect_part_slot_in_tx(
            tx,
            &key,
            depth + 1,
            stack,
            active,
            url_policy,
            schemas,
            acc,
        ))
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn resolve_and_render_header_footer_with_context_for_locale(
    db: &SqlitePool,
    active_v3: Option<&ActiveTheme>,
    theme_context: &serde_json::Value,
    precompiled_shell: &PrecompiledPublicShell,
    locale: Option<&str>,
    fmt_cfg: &template_sec::RenderFormattingConfig,
    budget: &mut PublicCompositionBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<(Option<String>, Option<String>, bool)> {
    resolve_and_render_header_footer_with_template_data_for_locale(
        db,
        active_v3,
        theme_context,
        precompiled_shell,
        locale,
        fmt_cfg,
        None,
        budget,
        cache,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn resolve_and_render_header_footer_with_template_data_for_locale(
    db: &SqlitePool,
    active_v3: Option<&ActiveTheme>,
    theme_context: &serde_json::Value,
    precompiled_shell: &PrecompiledPublicShell,
    locale: Option<&str>,
    fmt_cfg: &template_sec::RenderFormattingConfig,
    template_data: Option<&content_render::TemplateRenderData<'_>>,
    budget: &mut PublicCompositionBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<(Option<String>, Option<String>, bool)> {
    let mut tx = db.begin().await?;
    let effective_locale = settings::resolve_requested_locale_in_conn(&mut *tx, locale).await?;
    let mut state =
        builder::read_render_context_for_locale_tx(&mut tx, Some(&effective_locale), false, None)
            .await?;
    let mut part_docs = Vec::new();
    for slot_key in ["header", "footer"] {
        let target = TemplateCandidate {
            part_key: Some(slot_key.to_owned()),
            ..candidate(TemplateCandidateKind::Part)
        };
        let canonical = target.canonical_key().map_err(ApiError::internal)?;
        if let Some(row) = get_typed_override_on_connection(&mut *tx, &canonical).await? {
            part_docs.push(row.document);
        }
    }
    if !part_docs.is_empty() {
        let part_refs: Vec<&BuilderDocument> = part_docs.iter().collect();
        collect_db_docs_and_hydrate_state_in_tx(&mut tx, &mut state, &part_refs, false).await?;
    }
    populate_template_entry_schemas_in_conn(&mut tx, &mut state, template_data).await?;
    let url_policy = active_theme_url_policy_in_conn(&mut tx, active_v3, None, None, None).await?;
    let empty_pages = BTreeMap::new();
    let header_search_view: Option<content_render::SearchView> = theme_context
        .get("search")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .filter(|s: &content_render::SearchView| s.is_search_route)
        .map(|mut s| {
            s.is_search_route = false;
            s
        });
    let header_pagination = content_render::PaginationView::empty();
    let header_search = header_search_view
        .as_ref()
        .map(|view| page::BuilderSearchData {
            view,
            entries: &[],
            pagination: &header_pagination,
        });
    let context = PublicBuilderContext {
        state: &state,
        active: active_v3,
        theme_context,
        route_entry: None,
        render_locale: &effective_locale,
        list_pages: &empty_pages,
        list_base_path: "/",
        cache,
        url_policy: &url_policy,
        fmt_cfg,
        search: header_search,
        template_data,
    };

    async fn render_shell_slot(
        tx: &mut Transaction<'_, Sqlite>,
        slot_key: &str,
        precompiled: Option<&template_sec::CompiledCsecComposition>,
        context: &PublicBuilderContext<'_>,
        budget: &mut PublicCompositionBudget,
    ) -> ApiResult<Option<RenderedLayout>> {
        let target = TemplateCandidate {
            part_key: Some(slot_key.to_owned()),
            ..candidate(TemplateCandidateKind::Part)
        };
        let canonical = target.canonical_key().map_err(ApiError::internal)?;
        if let Some(row) = get_typed_override_on_connection(&mut **tx, &canonical).await? {
            let schemas = context.state.unified_block_schemas()?;
            let (_, slots, entry_blocks) =
                document_slot_scan_with_schemas(&row.document, &schemas)?;
            validate_content_slots(&target, slots, entry_blocks)?;
            let scope = part_scope_prefix(slot_key, budget.expansions);
            return Ok(Some(
                Box::pin(render_public_builder(
                    tx,
                    &row.document,
                    None,
                    0,
                    &[canonical],
                    context,
                    budget,
                    &scope,
                ))
                .await?,
            ));
        }
        if let Some(compiled) = precompiled {
            let html = budget.render_theme_with_config(
                compiled,
                context.theme_context,
                None,
                context.fmt_cfg,
            )?;
            return Ok(Some(RenderedLayout {
                html,
                diagnostics: Vec::new(),
                has_active_contact_form: false,
            }));
        }
        render_public_part(tx, slot_key, 0, &[], context, budget).await
    }

    let header = render_shell_slot(
        &mut tx,
        "header",
        precompiled_shell.header.as_deref(),
        &context,
        budget,
    )
    .await?;
    let footer = render_shell_slot(
        &mut tx,
        "footer",
        precompiled_shell.footer.as_deref(),
        &context,
        budget,
    )
    .await?;
    let has_form = header.as_ref().is_some_and(|r| r.has_active_contact_form)
        || footer.as_ref().is_some_and(|r| r.has_active_contact_form);
    tx.commit().await?;
    Ok((header.map(|r| r.html), footer.map(|r| r.html), has_form))
}
