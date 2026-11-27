use super::*;

pub(in crate::storage) async fn render_preview_part(
    tx: &mut Transaction<'_, Sqlite>,
    key: &str,
    ctx: &PreviewRenderContext<'_>,
    budget: &mut PreviewCompositionBudget,
) -> ApiResult<Option<RenderedLayout>> {
    match ctx.graph.get(key) {
        Some(PreviewPartSource::Builder(doc)) => {
            let scope = part_scope_prefix(key, budget.expansions);
            Ok(Some(
                Box::pin(render_preview_builder(
                    tx,
                    doc,
                    None,
                    ctx,
                    budget,
                    RenderAnnotationScope::None,
                    &scope,
                ))
                .await?,
            ))
        }
        Some(PreviewPartSource::Theme(source)) => {
            // Flatten only contiguous Theme-to-Theme edges. Builder boundaries
            // use the server snapshot rather than the surrounding lexical scope.
            let mut theme_sources = BTreeMap::new();
            let mut boundaries = BTreeSet::new();
            let mut todo =
                template_sec::preview_csec_part_keys_with_theme_urls(key, source, ctx.url_policy)
                    .map_err(ApiError::bad_request)?;
            while let Some(child) = todo.pop() {
                match ctx.graph.get(&child) {
                    Some(PreviewPartSource::Theme(child_source)) => {
                        if !theme_sources.contains_key(&child) {
                            todo.extend(
                                template_sec::preview_csec_part_keys_with_theme_urls(
                                    &child,
                                    child_source,
                                    ctx.url_policy,
                                )
                                .map_err(ApiError::bad_request)?,
                            );
                            theme_sources.insert(child, child_source.clone());
                        }
                    }
                    _ => {
                        boundaries.insert(child);
                    }
                }
            }
            let mut trusted = BTreeMap::new();
            let mut diagnostics = Vec::new();
            for child in boundaries {
                if let Some(mut rendered) =
                    Box::pin(render_preview_part(tx, &child, ctx, budget)).await?
                {
                    trusted.insert(child, rendered.html);
                    diagnostics.append(&mut rendered.diagnostics);
                } else {
                    diagnostics.push(page::BuilderDiagnostic {
                        node_id: child.clone(),
                        code: "template_part_unavailable",
                    });
                    trusted.insert(child, String::new());
                }
            }
            let html = template_sec::render_preview_csec_fragment_with_theme_urls_and_config(
                key,
                source,
                template_sec::json_to_safe_value(ctx.theme_context),
                &theme_sources,
                &trusted,
                budget,
                ctx.url_policy,
                ctx.fmt_cfg,
            )
            .map_err(ApiError::bad_request)?;
            Ok(Some(RenderedLayout {
                html,
                diagnostics,
                has_active_contact_form: false,
            }))
        }
        _ => Ok(None),
    }
}

pub(in crate::storage) fn build_placeholder_preview_theme_context(
    target: &TemplateCandidate,
    active: Option<&ActiveThemeV3>,
    site: &SiteContext<'_>,
    site_options: &BTreeMap<String, serde_json::Value>,
    options: &BTreeMap<String, serde_json::Value>,
    custom_options: &BTreeMap<String, serde_json::Value>,
    menus: &BTreeMap<String, menu::MenuViewModel>,
) -> ApiResult<serde_json::Value> {
    use TemplateCandidateKind as K;
    let route_kind = route_kind_for_candidate(target.kind);
    let theme_assets = active.map(ActiveThemeV3::asset_url_map).unwrap_or_default();
    let empty_entries = Vec::new();
    let pagination = PaginationView::empty();

    let archive_url: Option<&str> = match target.kind {
        K::Single | K::SingleCustom | K::Singular | K::Index | K::Archive => Some("/posts"),
        K::Home | K::Page | K::PageLayout | K::Taxonomy | K::NotFound | K::Part => None,
    };

    let placeholder_entry = matches!(
        target.kind,
        K::Home | K::Page | K::PageLayout | K::Single | K::SingleCustom | K::Singular | K::Index
    )
    .then(|| {
        let ct_key = target.content_type.as_deref().unwrap_or(
            if matches!(target.kind, K::Home | K::Page | K::PageLayout) {
                "page"
            } else {
                "post"
            },
        );
        build_placeholder_preview_entry(ct_key)
    });

    let ct_value: Option<serde_json::Value> = if matches!(
        target.kind,
        K::Home | K::Page | K::PageLayout | K::Single | K::SingleCustom | K::Singular | K::Index
    ) {
        let ct_key = target.content_type.as_deref().unwrap_or(
            if matches!(target.kind, K::Home | K::Page | K::PageLayout) {
                "page"
            } else {
                "post"
            },
        );
        let label = if matches!(target.kind, K::Home | K::Page | K::PageLayout) {
            "Trang"
        } else {
            "Bài viết"
        };
        Some(serde_json::json!({ "key": ct_key, "label": label }))
    } else if target.kind == K::Archive {
        let ct_key = target.content_type.as_deref().unwrap_or("post");
        Some(serde_json::json!({ "key": ct_key, "label": ct_key }))
    } else {
        None
    };

    let taxonomy_value: Option<serde_json::Value> = if target.kind == K::Taxonomy {
        let tax_key = target.taxonomy.as_deref().unwrap_or("category");
        Some(serde_json::json!({
            "key": tax_key,
            "id": 0,
            "slug": "sample-term",
            "name": "Chuyên mục mẫu",
        }))
    } else {
        None
    };

    let placeholder_term =
        (target.kind == K::Taxonomy).then(|| content_render::PublicTermDetailView {
            id: 0,
            kind: target.taxonomy.as_deref().unwrap_or("category").to_owned(),
            name: "Chuyên mục mẫu".to_owned(),
            slug: "sample-term".to_owned(),
            description: String::new(),
            sort_order: 0,
            url: Some("/".to_owned()),
            featured_image: None,
            fields: BTreeMap::new(),
        });

    let breadcrumbs = match target.kind {
        K::Home => content_render::home_breadcrumbs(
            content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
            Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
        ),
        K::Archive => {
            let ct_key = target.content_type.as_deref().unwrap_or("post");
            content_render::archive_breadcrumbs(
                content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
                Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
                ct_key,
                archive_url.map(str::to_owned),
            )
        }
        K::Taxonomy => content_render::taxonomy_breadcrumbs(
            content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
            Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
            Vec::new(),
            "Chuyên mục mẫu",
            None,
        ),
        K::NotFound => content_render::not_found_breadcrumbs(
            content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
            Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
            content_render::DEFAULT_BREADCRUMB_NOT_FOUND_LABEL,
        ),
        K::Part => Vec::new(),
        _ => {
            let ct_key = target.content_type.as_deref().unwrap_or("page");
            content_render::single_breadcrumbs(
                content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
                Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
                ct_key,
                if matches!(target.kind, K::Page | K::PageLayout) {
                    "Trang"
                } else {
                    "Bài viết"
                },
                archive_url.is_some(),
                archive_url.map(str::to_owned),
                None,
                placeholder_entry
                    .as_ref()
                    .map(|e| e.title.as_str())
                    .unwrap_or("Nội dung xem trước"),
                placeholder_entry.as_ref().and_then(|e| e.url.clone()),
            )
        }
    };

    content_render::build_public_theme_context(PublicThemeContextInput {
        site,
        site_options,
        options,
        custom_options,
        theme_assets: &theme_assets,
        menus,
        route_kind,
        route_path: None,
        breadcrumbs: &breadcrumbs,
        pagination: &pagination,
        archive_url,
        previous_url: None,
        next_url: None,
        entries: &empty_entries,
        related_entries: &empty_entries,
        recent_posts: &empty_entries,
        taxonomy: taxonomy_value.as_ref(),
        term: placeholder_term.as_ref(),
        content_type: ct_value.as_ref(),
        entry: placeholder_entry.as_ref(),
    })
}

pub(in crate::storage) async fn resolve_preview_entry(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    sample: Option<&TemplatePreviewEntry>,
    active: Option<&ActiveThemeV3>,
    preview_locale: &str,
    actor_role: FieldRole,
) -> ApiResult<Option<(entries::ContentEntryRecord, EffectiveContentSchema)>> {
    use TemplateCandidateKind as K;
    let selected = if let Some(sample) = sample {
        if sample.entry_id <= 0 || !valid_content_type_key(&sample.type_key) {
            return Err(ApiError::bad_request("Invalid preview entry."));
        }
        Some((sample.type_key.clone(), sample.entry_id))
    } else if target.kind == K::Single {
        match (&target.content_type, target.entry_id, &target.selector_slug) {
            (Some(t), Some(id), _) => Some((t.clone(), id)),
            (Some(t), None, Some(slug)) => {
                let id: i64 = sqlx::query_scalar(
                    "SELECT id FROM content_entries WHERE content_type_key = ? AND slug = ? AND locale = ? AND trashed_at IS NULL",
                )
                .bind(t)
                .bind(slug)
                .bind(preview_locale)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or_else(|| ApiError::not_found("Preview entry not found."))?;
                Some((t.clone(), id))
            }
            _ => None,
        }
    } else {
        None
    };
    let Some((type_key, id)) = selected else {
        return Ok(None);
    };
    if type_key == "pattern" {
        return Err(ApiError::not_found("Preview entry not found."));
    }
    let valid = match target.kind {
        K::Home | K::Page | K::PageLayout => type_key == "page",
        K::Single => {
            type_key != "page"
                && target.content_type.as_ref().is_none_or(|t| t == &type_key)
                && target.entry_id.is_none_or(|target_id| target_id == id)
        }
        K::SingleCustom => {
            type_key != "page"
                && active
                    .and_then(|a| {
                        a.manifest.templates.iter().find(|d| {
                            d.kind == K::SingleCustom && d.custom_key == target.custom_key
                        })
                    })
                    .is_none_or(|d| d.allowed_types.contains(&type_key))
        }
        K::Singular => type_key != "page",
        K::Index => true,
        _ => false,
    };
    if !valid {
        return Err(ApiError::bad_request(
            "Preview entry does not match template target.",
        ));
    }
    let mut entry = entries::get_entry_in_tx(tx, &type_key, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Preview entry not found."))?;
    if target.kind == K::Single
        && target
            .selector_slug
            .as_ref()
            .is_some_and(|s| s != &entry.slug)
    {
        return Err(ApiError::bad_request(
            "Preview entry slug does not match template target.",
        ));
    }
    let schema = content::load_effective_content_schema_in_tx(tx, &type_key)
        .await?
        .filter(|s| {
            s.base.status == ProviderStatus::Active
                && s.base.declaration_state == DeclarationState::Declared
        })
        .ok_or_else(|| ApiError::not_found("Preview entry type not available."))?;
    entries::filter_entry_values_for_role(&mut entry, &schema, actor_role);
    Ok(Some((entry, schema)))
}

pub async fn inspect_public_surfaces_and_precompile(
    db: &SqlitePool,
    resolution: &TemplateResolution,
    active: Option<&ActiveThemeV3>,
    url_policy: &template_sec::ThemeUrlPolicy,
) -> ApiResult<PublicSurfaceInspection> {
    let mut tx = db.begin().await?;
    let schemas = crate::storage::page::load_unified_block_catalog_in_conn(&mut *tx).await?;
    let mut precompiled_body = None;
    let mut precompiled_shell = PrecompiledPublicShell::default();
    let mut uses_related_entries = false;
    let mut uses_recent_posts = false;

    let body_canvas_layout = match resolution {
        TemplateResolution::DbOverride(record) => {
            let stack = vec![record.canonical_target_key.clone()];
            inspect_db_document_surfaces_in_tx(
                &mut tx,
                &record.document,
                0,
                &stack,
                active,
                url_policy,
                &schemas,
                &mut uses_related_entries,
                &mut uses_recent_posts,
            )
            .await?;
            is_canvas_target(active, &record.target)
        }
        TemplateResolution::ActiveTheme { target_key, html } => {
            let empty_parts = BTreeMap::new();
            let parts = active.map(|a| &a.parts).unwrap_or(&empty_parts);
            let compiled = if let Some(active) = active {
                template_sec::ThemeCompositionCache::global().get_or_compile(
                    &active.revision_digest,
                    target_key,
                    html,
                    parts,
                    csec_content_kind_for_target_key(target_key),
                    url_policy,
                )
            } else {
                template_sec::validate_and_compile_csec_composition_with_theme_urls(
                    target_key,
                    html,
                    parts,
                    csec_content_kind_for_target_key(target_key),
                    url_policy,
                )
                .map(Arc::new)
            }
            .map_err(ApiError::internal)?;
            uses_related_entries |= compiled.uses_related_entries;
            uses_recent_posts |= compiled.uses_recent_posts;
            precompiled_body = Some(compiled);
            is_canvas_resolved_key(active, target_key)
        }
        TemplateResolution::CoreFallback => false,
    };

    let mut shell_part_present = false;
    if !body_canvas_layout {
        let part_url_policy = url_policy.without_cpt_entry_fields();
        let (has_header, compiled_header) = inspect_part_slot_in_tx(
            &mut tx,
            "header",
            0,
            &[],
            active,
            &part_url_policy,
            &schemas,
            &mut uses_related_entries,
            &mut uses_recent_posts,
        )
        .await?;
        let (has_footer, compiled_footer) = inspect_part_slot_in_tx(
            &mut tx,
            "footer",
            0,
            &[],
            active,
            &part_url_policy,
            &schemas,
            &mut uses_related_entries,
            &mut uses_recent_posts,
        )
        .await?;
        shell_part_present = has_header || has_footer;
        precompiled_shell.header = compiled_header;
        precompiled_shell.footer = compiled_footer;
    }

    tx.commit().await?;
    let has_shell_or_theme =
        !matches!(resolution, TemplateResolution::CoreFallback) || shell_part_present;
    Ok(PublicSurfaceInspection {
        precompiled_body,
        precompiled_shell,
        uses_related_entries,
        uses_recent_posts,
        has_shell_or_theme,
    })
}

pub(in crate::storage) fn target_requires_and_forbids_slot(
    kind: TemplateCandidateKind,
) -> (bool, bool) {
    use TemplateCandidateKind as K;
    let requires = matches!(
        kind,
        K::Home | K::Page | K::PageLayout | K::Single | K::SingleCustom | K::Singular
    );
    let forbids = matches!(kind, K::Archive | K::Taxonomy | K::NotFound | K::Part);
    (requires, forbids)
}
