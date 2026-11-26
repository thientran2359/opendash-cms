use super::*;

pub(in crate::storage) async fn resolve_template_2axis_on_connection(
    conn: &mut SqliteConnection,
    candidates: &[TemplateCandidate],
    templates: Option<&BTreeMap<String, String>>,
    parts: Option<&BTreeMap<String, String>>,
    layouts: Option<&BTreeMap<String, String>>,
) -> ApiResult<TemplateResolution> {
    for candidate in candidates {
        let key = candidate.canonical_key().map_err(ApiError::bad_request)?;
        if let Some(record) = get_typed_override_on_connection(conn, &key).await? {
            return Ok(TemplateResolution::DbOverride(record));
        }
        if let Some(html) = theme_source_for_candidate(candidate, &key, templates, parts, layouts) {
            return Ok(TemplateResolution::ActiveTheme {
                target_key: key,
                html: html.to_owned(),
            });
        }
    }
    Ok(TemplateResolution::CoreFallback)
}

/// The `templates`, `parts`, `layouts` maps MUST come from the verified active
/// durable revision (storage::theme::load_active_theme_v3); raw untrusted theme
/// paths, without verification, must never be passed into this resolver.
pub async fn resolve_template_2axis(
    db: &SqlitePool,
    candidates: &[TemplateCandidate],
    templates: Option<&BTreeMap<String, String>>,
    parts: Option<&BTreeMap<String, String>>,
    layouts: Option<&BTreeMap<String, String>>,
) -> ApiResult<TemplateResolution> {
    let mut conn = db.acquire().await?;
    resolve_template_2axis_on_connection(&mut conn, candidates, templates, parts, layouts).await
}

pub async fn list_overrides(db: &SqlitePool) -> ApiResult<Vec<TemplateOverrideRecord>> {
    let rows: Vec<OverrideRow> = sqlx::query_as(
        "SELECT id, target_kind, canonical_target_key, target_json, ast_json, version,
                base_theme_id, base_revision_digest, updated_at
           FROM template_overrides
          ORDER BY canonical_target_key ASC",
    )
    .fetch_all(db)
    .await?;
    rows.into_iter().map(row_to_record).collect()
}

pub(in crate::storage) async fn validate_cpt_target_fields_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    entry_id: Option<i64>,
) -> ApiResult<Option<Vec<String>>> {
    let type_key = type_key.trim();
    if type_key.is_empty() {
        return Ok(None);
    }
    if !valid_content_type_key(type_key) {
        return Err(ApiError::bad_request("scope_type_key không hợp lệ."));
    }
    if let Some(eid) = entry_id
        && eid <= 0
    {
        return Err(ApiError::bad_request("scope_entry_id không hợp lệ."));
    }
    let def_json: Option<String> =
        sqlx::query_scalar("SELECT definition_json FROM content_types WHERE key = ?")
            .bind(type_key)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(raw) = def_json else {
        return Err(ApiError::not_found(
            "Không tìm thấy loại nội dung cho scope.",
        ));
    };
    let def: ContentTypeDefinition = serde_json::from_str(&raw)
        .map_err(|_| ApiError::internal("Định nghĩa loại nội dung không hợp lệ."))?;
    let field_keys: Vec<String> = def.fields.into_iter().map(|f| f.key).collect();

    if let Some(eid) = entry_id {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM content_entries WHERE id = ? AND content_type_key = ? AND trashed_at IS NULL",
        )
        .bind(eid)
        .bind(type_key)
        .fetch_optional(&mut **tx)
        .await?;
        if exists.is_none() {
            return Err(ApiError::not_found("Không tìm thấy entry cho scope."));
        }
    }

    Ok(Some(field_keys))
}

/// Check site-owned part edges in the final post-save graph, not only the
/// submitted node. A missing part may be provided by the active theme; the
/// public renderer must enforce the same depth/fan-out budget for theme parts.
pub(in crate::storage) async fn validate_part_graph_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    submitted: &BuilderDocument,
) -> ApiResult<()> {
    let schemas = crate::storage::page::load_unified_block_catalog_in_conn(&mut **tx).await?;
    let target_key = target.canonical_key().map_err(ApiError::bad_request)?;
    let (initial, _) = document_parts_and_slot_count_with_schemas(submitted, &schemas)?;
    let mut stack = vec![(initial, 0_usize, vec![target_key.clone()])];
    let mut expansions = 0_usize;
    while let Some((parts, depth, path)) = stack.pop() {
        for part_key in parts {
            let part_candidate = TemplateCandidate {
                part_key: Some(part_key),
                ..candidate(TemplateCandidateKind::Part)
            };
            let key = part_candidate
                .canonical_key()
                .map_err(ApiError::bad_request)?;
            if path.iter().any(|ancestor| ancestor == &key) {
                return Err(ApiError::bad_request("Template part cycle detected."));
            }
            if depth >= 3 {
                return Err(ApiError::bad_request("Template part depth exceeds 3."));
            }
            expansions += 1;
            if expansions > 32 {
                return Err(ApiError::bad_request(
                    "Template parts exceed 32 expansions.",
                ));
            }
            let nested_doc = if key == target_key {
                Some(submitted.clone())
            } else {
                get_typed_override_on_connection(&mut *tx, &key)
                    .await?
                    .map(|record| record.document)
            };
            if let Some(nested_doc) = nested_doc {
                let (children, count) =
                    document_parts_and_slot_count_with_schemas(&nested_doc, &schemas)?;
                validate_content_slots(&part_candidate, count)?;
                let mut next_path = path.clone();
                next_path.push(key);
                stack.push((children, depth + 1, next_path));
            }
        }
    }
    Ok(())
}

pub(in crate::storage) async fn validate_typed_target_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
) -> ApiResult<Option<Vec<String>>> {
    target.canonical_key().map_err(ApiError::bad_request)?;
    if let Some(kind) = target.content_type.as_deref() {
        return validate_cpt_target_fields_in_tx(tx, kind, target.entry_id).await;
    }
    if let Some(term_id) = target.term_id {
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM taxonomy_terms WHERE id = ? AND kind = ?)",
        )
        .bind(term_id)
        .bind(target.taxonomy.as_deref())
        .fetch_one(&mut **tx)
        .await?;
        if exists == 0 {
            return Err(ApiError::not_found(
                "Taxonomy term for template target not found.",
            ));
        }
    }
    Ok(None)
}

pub async fn reset_typed_override(
    db: &SqlitePool,
    override_id: i64,
    expected_version: Option<i64>,
    candidates: &[TemplateCandidate],
    templates: Option<&BTreeMap<String, String>>,
    parts: Option<&BTreeMap<String, String>>,
    layouts: Option<&BTreeMap<String, String>>,
) -> ApiResult<ResetTemplateOutcome> {
    let expected_version = expected_version
        .ok_or_else(|| ApiError::precondition_required("If-Match required for reset."))?;
    let mut tx = db.begin().await?;
    let row: Option<OverrideRow> = sqlx::query_as(
        "SELECT id, target_kind, canonical_target_key, target_json, ast_json, version,
                base_theme_id, base_revision_digest, updated_at
           FROM template_overrides WHERE id = ?",
    )
    .bind(override_id)
    .fetch_optional(&mut *tx)
    .await?;
    let existing = row
        .map(row_to_record)
        .transpose()?
        .ok_or_else(|| ApiError::not_found("Template override not found."))?;
    if existing.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Template override changed before reset.",
        ));
    }
    if candidates
        .first()
        .and_then(|candidate| candidate.canonical_key().ok())
        .as_deref()
        != Some(existing.canonical_target_key.as_str())
    {
        return Err(ApiError::bad_request(
            "Reset candidate chain must begin with target.",
        ));
    }
    let affected = sqlx::query("DELETE FROM template_overrides WHERE id = ? AND version = ?")
        .bind(override_id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if affected != 1 {
        return Err(ApiError::precondition_failed(
            "Template override changed concurrently.",
        ));
    }
    let next = resolve_template_2axis_on_connection(&mut tx, candidates, templates, parts, layouts)
        .await?
        .effective_source();
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(ResetTemplateOutcome {
        reset_id: existing.id,
        canonical_target_key: existing.canonical_target_key,
        next_effective_source: next,
    })
}

pub(in crate::storage) async fn preview_part_graph_from_edges(
    tx: &mut Transaction<'_, Sqlite>,
    root_key: &str,
    edges: Vec<String>,
    active: Option<&ActiveThemeV3>,
    budget: &mut PreviewCompositionBudget,
) -> ApiResult<BTreeMap<String, PreviewPartSource>> {
    let url_policy = active_theme_url_policy(active)?;
    let schemas = crate::storage::page::load_unified_block_catalog_in_conn(&mut **tx).await?;
    let mut pending = vec![(edges, 0_usize, vec![root_key.to_owned()])];
    let mut graph = BTreeMap::new();
    while let Some((edges, depth, path)) = pending.pop() {
        budget.stack = path;
        for part_key in edges {
            let target = TemplateCandidate {
                part_key: Some(part_key.clone()),
                ..candidate(TemplateCandidateKind::Part)
            };
            let key = target.canonical_key().map_err(ApiError::bad_request)?;
            if budget.stack.contains(&key) {
                return Err(ApiError::bad_request("Template part cycle detected."));
            }
            budget.expansions += 1;
            if depth >= 3 || budget.expansions > 32 {
                return Err(ApiError::bad_request(
                    "Template part expansion limit exceeded.",
                ));
            }
            if !graph.contains_key(&part_key) {
                let source = if let Some(row) = get_typed_override_on_connection(tx, &key).await? {
                    validate_typed_document_in_tx(tx, &target, &row.document, Some(&row.document))
                        .await?;
                    PreviewPartSource::Builder(row.document)
                } else if let Some(source) = active.and_then(|a| a.parts.get(&part_key)).cloned() {
                    PreviewPartSource::Theme(source)
                } else {
                    PreviewPartSource::Missing
                };
                graph.insert(part_key.clone(), source);
            }
            let children = match &graph[&part_key] {
                PreviewPartSource::Builder(doc) => {
                    document_parts_and_slot_count_with_schemas(doc, &schemas)?.0
                }
                PreviewPartSource::Theme(source) => {
                    template_sec::preview_csec_part_keys_with_theme_urls(
                        &part_key,
                        source,
                        &url_policy,
                    )
                    .map_err(ApiError::bad_request)?
                }
                PreviewPartSource::Missing => Vec::new(),
            };
            let mut path = budget.stack.clone();
            path.push(key);
            pending.push((children, depth + 1, path));
        }
    }
    budget.stack.clear();
    Ok(graph)
}

pub(in crate::storage) async fn preview_part_graph(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    document: &BuilderDocument,
    active: Option<&ActiveThemeV3>,
    budget: &mut PreviewCompositionBudget,
) -> ApiResult<BTreeMap<String, PreviewPartSource>> {
    let schemas = crate::storage::page::load_unified_block_catalog_in_conn(&mut **tx).await?;
    let (edges, _) = document_parts_and_slot_count_with_schemas(document, &schemas)?;
    let root_key = target.canonical_key().map_err(ApiError::bad_request)?;
    preview_part_graph_from_edges(tx, &root_key, edges, active, budget).await
}

pub(in crate::storage) async fn collect_db_docs_and_hydrate_state_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    state: &mut BuilderRenderContext,
    roots: &[&BuilderDocument],
    is_preview: bool,
) -> ApiResult<()> {
    let schemas = state.unified_block_schemas()?;
    let mut visited_keys = BTreeSet::new();
    let mut part_docs: Vec<BuilderDocument> = Vec::new();
    let mut pending: Vec<(Vec<String>, usize)> = Vec::new();
    for root in roots {
        let (edges, _) = document_parts_and_slot_count_with_schemas(root, &schemas)?;
        if !edges.is_empty() {
            pending.push((edges, 0));
        }
    }
    while let Some((edges, depth)) = pending.pop() {
        if depth >= 3 {
            continue;
        }
        for part_key in edges {
            let target = TemplateCandidate {
                part_key: Some(part_key),
                ..candidate(TemplateCandidateKind::Part)
            };
            let canonical = target.canonical_key().map_err(ApiError::bad_request)?;
            if !visited_keys.insert(canonical.clone()) {
                continue;
            }
            if let Some(row) = get_typed_override_on_connection(&mut **tx, &canonical).await? {
                let (child_edges, _) =
                    document_parts_and_slot_count_with_schemas(&row.document, &schemas)?;
                if !child_edges.is_empty() {
                    pending.push((child_edges, depth + 1));
                }
                part_docs.push(row.document);
            }
        }
    }
    let mut all_refs: Vec<&BuilderDocument> = roots.to_vec();
    all_refs.extend(part_docs.iter());
    builder::hydrate_render_context_for_documents_in_conn(&mut **tx, state, &all_refs, is_preview)
        .await
}

pub(in crate::storage) async fn render_preview_builder(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    slot: Option<&str>,
    ctx: &PreviewRenderContext<'_>,
    budget: &mut PreviewCompositionBudget,
    annotation_scope: RenderAnnotationScope,
    scope: &str,
) -> ApiResult<RenderedLayout> {
    let mut parts = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let schemas = ctx.state.unified_block_schemas()?;
    for key in document_parts_and_slot_count_with_schemas(document, &schemas)?.0 {
        if parts.contains_key(&key) {
            continue;
        }
        if let Some(mut part) = Box::pin(render_preview_part(tx, &key, ctx, budget)).await? {
            diagnostics.append(&mut part.diagnostics);
            parts.insert(key, part.html);
        }
    }
    let bindings = builder_binding::resolve_in_tx_with_route_and_locale(
        tx,
        document,
        true,
        ctx.route,
        Some(ctx.render_locale),
    )
    .await?;
    let lists = builder::resolve_lists_in_tx_for_locale(
        tx,
        document,
        &BTreeMap::new(),
        Some(ctx.render_locale),
    )
    .await?;
    let forms = resolve_contact_forms_in_tx(tx, document).await?;
    let empty_patterns = page::PatternResults::new();
    let mut rb = budget.as_render_budget();
    let mut rendered = page::render_builder_with_budget(
        ctx.state,
        document,
        true,
        &bindings,
        &lists,
        &forms,
        Some(&page::BuilderTemplateContext {
            content_html: slot,
            parts: &parts,
        }),
        &empty_patterns,
        false,
        annotation_scope,
        scope,
        &mut rb,
        ctx.cache,
    )?;
    budget.sync_from_render_budget(&rb);
    rendered.diagnostics.append(&mut diagnostics);
    Ok(rendered)
}

pub(in crate::storage) async fn render_public_part(
    tx: &mut Transaction<'_, Sqlite>,
    key: &str,
    depth: usize,
    stack: &[String],
    ctx: &PublicBuilderContext<'_>,
    budget: &mut PublicCompositionBudget,
) -> ApiResult<Option<RenderedLayout>> {
    let target = TemplateCandidate {
        part_key: Some(key.to_owned()),
        ..candidate(TemplateCandidateKind::Part)
    };
    let canonical = target.canonical_key().map_err(ApiError::internal)?;
    if stack.contains(&canonical) || depth > 3 {
        return Err(ApiError::internal("Template part cycle or depth limit."));
    }
    if let Some(row) = get_typed_override_on_connection(tx, &canonical).await? {
        let schemas = ctx.state.unified_block_schemas()?;
        let (_, slots) = document_parts_and_slot_count_with_schemas(&row.document, &schemas)?;
        validate_content_slots(&target, slots)?;
        let mut next_stack = stack.to_vec();
        next_stack.push(canonical);
        let scope = part_scope_prefix(key, budget.expansions);
        return Ok(Some(
            Box::pin(render_public_builder(
                tx,
                &row.document,
                None,
                depth,
                &next_stack,
                ctx,
                budget,
                &scope,
            ))
            .await?,
        ));
    }
    let Some(active) = ctx.active else {
        return Ok(None);
    };
    let Some(source) = active.parts.get(key) else {
        return Ok(None);
    };
    let compiled = template_sec::ThemeCompositionCache::global()
        .get_or_compile(
            &active.revision_digest,
            &canonical,
            source,
            &active.parts,
            template_sec::CsecContentKind::NonEntry,
            ctx.url_policy,
        )
        .map_err(ApiError::internal)?;
    if depth + compiled.max_part_depth > 3 {
        return Err(ApiError::internal("Template part depth exceeds 3."));
    }
    let html = budget.render_theme_with_config(&compiled, ctx.theme_context, None, ctx.fmt_cfg)?;
    Ok(Some(RenderedLayout {
        html,
        diagnostics: Vec::new(),
        has_active_contact_form: false,
    }))
}

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
    let bindings = builder_binding::resolve_in_tx_with_route_and_locale(
        tx,
        document,
        false,
        ctx.route_entry,
        Some(ctx.render_locale),
    )
    .await?;
    let lists = builder::resolve_lists_in_tx_for_locale(
        tx,
        document,
        &BTreeMap::new(),
        Some(ctx.render_locale),
    )
    .await?;
    let forms = resolve_contact_forms_in_tx(tx, document).await?;
    let empty_patterns = page::PatternResults::new();
    let mut rendered = page::render_builder_with_budget(
        ctx.state,
        document,
        false,
        &bindings,
        &lists,
        &forms,
        Some(&page::BuilderTemplateContext {
            content_html: slot,
            parts: &parts,
        }),
        &empty_patterns,
        false,
        RenderAnnotationScope::None,
        scope,
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
    active: Option<&ActiveThemeV3>,
    url_policy: &template_sec::ThemeUrlPolicy,
    schemas: &opendash_core::registry::RegistryCatalog<
        opendash_core::block_schema::RegisteredBlockSchema,
    >,
    uses_related_entries: &mut bool,
    uses_recent_posts: &mut bool,
) -> ApiResult<(bool, Option<Arc<template_sec::CompiledCsecComposition>>)> {
    let target = TemplateCandidate {
        part_key: Some(key.to_owned()),
        ..candidate(TemplateCandidateKind::Part)
    };
    let canonical = target.canonical_key().map_err(ApiError::internal)?;
    if stack.contains(&canonical) || depth > 3 {
        return Err(ApiError::internal("Template part cycle or depth limit."));
    }
    if let Some(row) = get_typed_override_on_connection(&mut **tx, &canonical).await? {
        let (_, slots) = document_parts_and_slot_count_with_schemas(&row.document, schemas)?;
        validate_content_slots(&target, slots)?;
        let mut next_stack = stack.to_vec();
        next_stack.push(canonical);
        Box::pin(inspect_db_document_surfaces_in_tx(
            tx,
            &row.document,
            depth,
            &next_stack,
            active,
            url_policy,
            schemas,
            uses_related_entries,
            uses_recent_posts,
        ))
        .await?;
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
    *uses_related_entries |= compiled.uses_related_entries;
    *uses_recent_posts |= compiled.uses_recent_posts;
    Ok((true, Some(compiled)))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn inspect_db_document_surfaces_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    depth: usize,
    stack: &[String],
    active: Option<&ActiveThemeV3>,
    url_policy: &template_sec::ThemeUrlPolicy,
    schemas: &opendash_core::registry::RegistryCatalog<
        opendash_core::block_schema::RegisteredBlockSchema,
    >,
    uses_related_entries: &mut bool,
    uses_recent_posts: &mut bool,
) -> ApiResult<()> {
    for key in document_parts_and_slot_count_with_schemas(document, schemas)?.0 {
        Box::pin(inspect_part_slot_in_tx(
            tx,
            &key,
            depth + 1,
            stack,
            active,
            url_policy,
            schemas,
            uses_related_entries,
            uses_recent_posts,
        ))
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn resolve_and_render_header_footer_with_context_for_locale(
    db: &SqlitePool,
    active_v3: Option<&ActiveThemeV3>,
    theme_context: &serde_json::Value,
    precompiled_shell: &PrecompiledPublicShell,
    locale: Option<&str>,
    fmt_cfg: &template_sec::RenderFormattingConfig,
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
    let url_policy = active_theme_url_policy(active_v3)?;
    let context = PublicBuilderContext {
        state: &state,
        active: active_v3,
        theme_context,
        route_entry: None,
        render_locale: &effective_locale,
        cache,
        url_policy: &url_policy,
        fmt_cfg,
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
            let (_, slots) = document_parts_and_slot_count_with_schemas(&row.document, &schemas)?;
            validate_content_slots(&target, slots)?;
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
