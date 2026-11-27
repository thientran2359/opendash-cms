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

pub(in crate::storage) async fn validate_typed_target_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
) -> ApiResult<()> {
    target.canonical_key().map_err(ApiError::bad_request)?;
    if let Some(type_key) = target.content_type.as_deref() {
        let type_key = type_key.trim();
        if !type_key.is_empty() {
            if !valid_content_type_key(type_key) {
                return Err(ApiError::bad_request("scope_type_key không hợp lệ."));
            }
            if let Some(eid) = target.entry_id
                && eid <= 0
            {
                return Err(ApiError::bad_request("scope_entry_id không hợp lệ."));
            }
            if crate::storage::content::load_effective_content_schema_in_tx(&mut **tx, type_key)
                .await?
                .is_none()
            {
                return Err(ApiError::not_found(
                    "Không tìm thấy loại nội dung cho scope.",
                ));
            }
            if let Some(eid) = target.entry_id {
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
        }
        return Ok(());
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
    Ok(())
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
    let mut rb = budget.as_render_budget();
    let lists = builder::resolve_lists_in_tx_for_locale_with_budget(
        tx,
        document,
        &BTreeMap::new(),
        Some(ctx.render_locale),
        &mut rb,
    )
    .await?;
    let forms = resolve_contact_forms_in_tx(tx, document).await?;
    let empty_patterns = page::PatternResults::new();
    let mut rendered = page::render_builder_with_budget(
        ctx.state,
        document,
        true,
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
    let part_url_policy = ctx.url_policy.without_cpt_entry_fields();
    let compiled = template_sec::ThemeCompositionCache::global()
        .get_or_compile(
            &active.revision_digest,
            &canonical,
            source,
            &active.parts,
            template_sec::CsecContentKind::NonEntry,
            &part_url_policy,
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
