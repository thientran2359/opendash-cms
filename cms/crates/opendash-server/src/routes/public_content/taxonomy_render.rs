use super::*;

/// The public fallback uses the same term-ID renderer as the legacy routes.
/// The canonical path was already selected by storage; no slug-based routing
/// is allowed to override it.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn render_canonical_taxonomy(
    db: &SqlitePool,
    themes_root: Option<ThemeStoreRoot>,
    term_id: i64,
    locale: String,
    canonical_base: String,
    raw_query: Option<&str>,
    expected_kind: Option<TaxonomyKind>,
    services: Option<&PublicRenderServices>,
) -> ApiResult<Response> {
    let kind_key = require_term_kind(db, term_id, expected_kind).await?;
    let kind = match kind_key.as_str() {
        "tag" => TaxonomyKind::Tag,
        // Generic taxonomies use the same typed filtering and query
        // serialization as the category archive, including descendants.
        _ => TaxonomyKind::Category,
    };
    let query =
        parse_query(raw_query, QueryScope::TaxonomyArchive(kind)).map_err(ApiError::bad_request)?;
    let offset = (query.page - 1) * ARCHIVE_SIZE;
    let (confirmed_kind, term, page, term_detail, term_richtext_nonces) =
        public_content::taxonomy_public_page_and_detail_by_id(
            db,
            term_id,
            query.filters.clone(),
            ARCHIVE_SIZE,
            offset,
        )
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy taxonomy."))?;
    if confirmed_kind != kind_key {
        return Err(ApiError::not_found("Không tìm thấy taxonomy."));
    }
    let active_theme = match themes_root {
        Some(root) => theme_storage::load_active_theme(db, &root.0).await?,
        None => None,
    };
    let term_schema =
        crate::storage::taxonomy::load_effective_term_schema(db, &kind_key).await?;
    let candidates = overrides::build_candidates(&PublicTemplateContext::Taxonomy {
        taxonomy: kind_key.clone(),
        term_id,
        slug: term.slug.clone(),
    })?;
    let resolution = overrides::resolve_template_2axis(
        db,
        &candidates,
        active_theme.as_ref().map(|v| &v.templates),
        active_theme.as_ref().map(|v| &v.parts),
        active_theme.as_ref().map(|v| &v.page_layouts),
    )
    .await?;
    let pre_inspected = template_data::inspect_surface_for_resolution(
        db,
        &resolution,
        active_theme.as_ref(),
        None,
        term_schema.as_ref(),
    )
    .await?;
    let canonical_path = CanonicalUrlBuilder::with_query(
        canonical_base.clone(),
        query.clone(),
        QueryScope::TaxonomyArchive(kind),
    );
    let _context = if matches!(kind_key.as_str(), "category" | "tag") {
        PublicRequestContext::TaxonomyArchive {
            term_id: term.id,
            kind,
            slug: term.slug.clone(),
            query: query.clone(),
            canonical_path,
        }
    } else {
        PublicRequestContext::GenericTaxonomyArchive {
            term_id: term.id,
            kind_key: kind_key.clone(),
            slug: term.slug.clone(),
            query: query.clone(),
            canonical_path,
        }
    };
    let mut tax_richtext_nonces = crate::template_sec::RichTextNonceTable::default();
    let entries = {
        let mut conn = db.acquire().await?;
        public_content::hydrate_entry_views_projected_in_conn(
            &mut conn,
            page.entries,
            Some(&locale),
            false,
            FieldRole::Editor,
            Some(&mut tax_richtext_nonces),
            &pre_inspected.inspection.loop_projection,
        )
        .await?
    };
    let previous = (query.page > 1)
        .then(|| {
            Some(CanonicalUrlBuilder::with_query(
                canonical_base.clone(),
                PublicQuery {
                    page: query.page - 1,
                    ..query.clone()
                },
                QueryScope::TaxonomyArchive(kind),
            ))
        })
        .flatten();
    let next = (query.page < MAX_ARCHIVE_PAGE && offset + ARCHIVE_SIZE < page.total)
        .then(|| {
            Some(CanonicalUrlBuilder::with_query(
                canonical_base.clone(),
                PublicQuery {
                    page: query.page + 1,
                    ..query.clone()
                },
                QueryScope::TaxonomyArchive(kind),
            ))
        })
        .flatten();
    let pagination = PaginationView::compute(
        query.page,
        ARCHIVE_SIZE,
        page.total,
        previous.clone(),
        next.clone(),
        |p| {
            Some(CanonicalUrlBuilder::with_query(
                canonical_base.clone(),
                PublicQuery {
                    page: p,
                    ..query.clone()
                },
                QueryScope::TaxonomyArchive(kind),
            ))
        },
    );
    let ancestors = {
        let mut conn = db.acquire().await?;
        public_content::load_term_ancestor_chain_in_conn(&mut conn, term_id, &kind_key).await?
    };
    let breadcrumbs = content_render::taxonomy_breadcrumbs(
        content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
        Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
        ancestors,
        &term.name,
        Some(canonical_base.clone()),
    );
    let site = settings::get_general_for_locale(db, Some(&locale)).await?;
    let pub_settings = settings::get_public(db).await?;
    let mut richtext_nonces = crate::template_sec::RichTextNonceTable::default();
    let (site_opts, theme_opts) = settings::get_active_public_theme_options_for_locale_with_nonces(
        db,
        Some(&locale),
        Some(&mut richtext_nonces),
    )
    .await?;
    // Until S03 role composition, use the core archive template for mixed CPTs.
    // Site-owned content is resolved by its own actual schema above.
    let schema = ContentTypeDefinition {
        key: kind_key,
        label: term.name,
        singular_label: None,
        description: None,
        settings: Default::default(),
    };
    let site_ctx = SiteContext {
        title: &site.site_title,
        tagline: &site.tagline,
        timezone: &site.timezone,
        origin: normalized_origin(pub_settings.public_origin.as_deref()),
        current_year: content_render::current_year_in_timezone(&site.timezone),
    };
    let fmt_cfg = crate::template_sec::RenderFormattingConfig {
        timezone: site.timezone.clone(),
        ..Default::default()
    };
    let tax_value = serde_json::json!({ "key": schema.key, "id": term_id, "slug": term.slug, "name": schema.label });
    let fallback_html = content_render::render_archive_with_site_and_config_for_locale(
        &schema,
        &entries,
        previous.as_deref(),
        next.as_deref(),
        &pagination,
        &breadcrumbs,
        Some(&tax_value),
        None,
        &site_ctx,
        &fmt_cfg,
        &locale,
    )?;
    let mut budget = overrides::PublicCompositionBudget::default();
    budget.render_budget.richtext_nonces.extend(richtext_nonces);
    budget
        .render_budget
        .richtext_nonces
        .extend(tax_richtext_nonces);
    budget
        .render_budget
        .richtext_nonces
        .extend(term_richtext_nonces);
    let custom_opts = crate::storage::options_pages::load_active_custom_options_public(
        db,
        Some(&locale),
        false,
        Some(&mut budget.render_budget.richtext_nonces),
    )
    .await?;
    let locale_ctx = seo::build_theme_locale_context(
        db,
        Some(&locale),
        seo::ThemeLocaleTarget::Term { term_id },
    )
    .await?;
    let inactive_search = content_render::SearchView::inactive_for_locale_ctx(&locale_ctx);
    budget.set_locale_context(active_theme.as_ref(), &locale, locale_ctx);
    let taxonomy_label = term_schema
        .as_ref()
        .map(|ts| ts.taxonomy.label.clone())
        .unwrap_or_else(|| schema.key.clone());
    let term_summary =
        content_render::TermRenderSummary::from_detail(&term_detail, taxonomy_label);
    let ct_value = serde_json::json!({ "key": schema.key, "label": schema.label });
    let cache = crate::page::BlockTemplateCache::global();
    let body = render_resolved_public_template_html_with_surface(
        db,
        resolution,
        active_theme.as_ref(),
        &site_ctx,
        &site_opts,
        &theme_opts,
        Some(&locale),
        &fallback_html,
        None,
        PublicRouteContext {
            route_kind: "taxonomy",
            route_path: Some(&canonical_base),
            resolved_locale: &locale,
            locale_target: seo::ThemeLocaleTarget::Term { term_id },
            entry_id: None,
            entry_type_key: None,
            entry_schema: None,
            entry_body_html: None,
            entry_has_builder_body: false,
            custom_options: &custom_opts,
            breadcrumbs: &breadcrumbs,
            pagination: &pagination,
            archive_url: None,
            previous_url: previous.as_deref(),
            next_url: next.as_deref(),
            entries: &entries,
            taxonomy: Some(&tax_value),
            term: Some(&term_detail),
            term_schema: term_schema.as_ref(),
            term_summary: Some(term_summary),
            content_type: Some(&ct_value),
            archive_type: None,
            entry: None,
            list_pages: None,
            search: &inactive_search,
        },
        Some(pre_inspected),
        &mut budget,
        cache,
    )
    .await?;
    let (header, footer, shell_has_form) = if body.canvas_layout || body.theme_context.is_null() {
        (None, None, false)
    } else {
        overrides::resolve_and_render_header_footer_with_template_data_for_locale(
            db,
            active_theme.as_ref(),
            &body.theme_context,
            &body.precompiled_shell,
            Some(&locale),
            &fmt_cfg,
            body.template_data
                .as_ref()
                .map(|d| d.as_render_data(&site_ctx))
                .as_ref(),
            &mut budget,
            cache,
        )
        .await?
    };
    let seo_view = seo::build_term_seo_view_for_locale(
        db,
        term_id,
        &locale,
        &schema.label,
        &canonical_base,
        &query,
    )
    .await?;
    let tax_class = format!("od-tax-{}", schema.key);
    let enqueued = enqueued_for_route(active_theme.as_ref(), EnqueueCondition::Taxonomy);
    let canonical_query =
        crate::cache::canonicalize_public_query(raw_query, QueryScope::TaxonomyArchive(kind));
    let composed = content_render::compose_document_with_seo_with_assets(
        header.as_deref(),
        &body.html,
        footer.as_deref(),
        &seo_view,
        &["od-route-taxonomy", &tax_class],
        &enqueued,
        active_theme.is_some(),
    )?;
    let composed = apply_public_render_hooks(
        composed,
        services,
        "taxonomy",
        &canonical_base,
        &canonical_query,
        false,
        &locale,
        &body.theme_context,
        &budget,
    )
    .await?;
    Ok(crate::routes::public::rendered_layout_with_form_csp(
        content_render::finalize_document_richtext(
            content_render::finalize_document_images(
                content_render::finalize_document_icons(
                    composed,
                    &budget.render_budget.icon_nonces,
                )?,
                &budget.render_budget.image_nonces,
            )?,
            &budget.render_budget.richtext_nonces,
        )?,
        body.has_active_contact_form || shell_has_form,
    ))
}
