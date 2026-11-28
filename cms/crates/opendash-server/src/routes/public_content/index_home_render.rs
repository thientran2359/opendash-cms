use super::*;

/// Renders `/` or `/{locale}` when Reading Page is `Unconfigured` using `Index`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn render_index_home_response_for_locale(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    locale: &str,
    canonical_path: String,
    query: PublicQuery,
    raw_query: Option<&str>,
    services: Option<&PublicRenderServices>,
) -> ApiResult<Response> {
    let page = query.page;
    let effective_schema = content_storage::load_effective_content_schema(db, "post").await?;
    let active_post_schema = effective_schema
        .as_ref()
        .filter(|s| {
            s.base.status == opendash_core::registry::ProviderStatus::Active
                && s.base.declaration_state == opendash_core::registry::DeclarationState::Declared
                && s.base.descriptor.settings.public
        })
        .map(|s| s.base.descriptor.clone());
    let l10n = content_render::core_l10n(locale);
    let schema = active_post_schema.clone().unwrap_or_else(|| ContentTypeDefinition {
        key: "post".to_owned(),
        label: l10n.latest_posts_heading.to_string(),
        singular_label: None,
        description: None,
        settings: Default::default(),
    });
    let per_page = i64::from(schema.settings.archive_per_page).max(1);
    let offset = (page - 1) * per_page;
    let sort = crate::storage::entries::PublishedListSort::from_content_default_sort(
        schema.settings.default_sort,
    );
    let (records, total_items) = if active_post_schema.is_some() {
        public_content::list_published_for_index_home(db, per_page, offset, Some(locale), sort)
            .await?
    } else {
        (Vec::new(), 0)
    };
    let has_more = offset.saturating_add(per_page) < total_items;
    let mut home_richtext_nonces = crate::template_sec::RichTextNonceTable::default();
    let entries = {
        let mut conn = db.acquire().await?;
        public_content::hydrate_entry_views_with_options_in_conn(
            &mut conn,
            records,
            Some(locale),
            false,
            FieldRole::Editor,
            Some(&mut home_richtext_nonces),
        )
        .await?
    };
    let previous = (page > 1)
        .then(|| {
            Some(CanonicalUrlBuilder::with_query(
                canonical_path.clone(),
                PublicQuery {
                    page: page - 1,
                    filters: Default::default(),
                },
                QueryScope::ContentArchive,
            ))
        })
        .flatten();
    let next = (has_more && page < MAX_ARCHIVE_PAGE)
        .then(|| {
            Some(CanonicalUrlBuilder::with_query(
                canonical_path.clone(),
                PublicQuery {
                    page: page + 1,
                    filters: Default::default(),
                },
                QueryScope::ContentArchive,
            ))
        })
        .flatten();
    let pagination = PaginationView::compute(
        page,
        per_page,
        total_items,
        previous.clone(),
        next.clone(),
        |p| {
            Some(CanonicalUrlBuilder::with_query(
                canonical_path.clone(),
                PublicQuery {
                    page: p,
                    filters: Default::default(),
                },
                QueryScope::ContentArchive,
            ))
        },
    );
    let breadcrumbs = content_render::home_breadcrumbs(
        &l10n.home_label,
        Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
    );
    let active = match themes_root {
        Some(Extension(root)) => theme_storage::load_active_theme(db, &root.0).await?,
        None => None,
    };
    let site = settings::get_general_for_locale(db, Some(locale)).await?;
    let pub_settings = settings::get_public(db).await?;
    let mut richtext_nonces = crate::template_sec::RichTextNonceTable::default();
    let (site_opts, theme_opts) = settings::get_active_public_theme_options_for_locale_with_nonces(
        db,
        Some(locale),
        Some(&mut richtext_nonces),
    )
    .await?;
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
    let fallback_html = content_render::render_archive_with_site_and_config_for_locale(
        &schema,
        &entries,
        previous.as_deref(),
        next.as_deref(),
        &pagination,
        &breadcrumbs,
        None,
        None,
        &site_ctx,
        &fmt_cfg,
        locale,
    )?;
    let candidates = overrides::build_candidates(&PublicTemplateContext::Index)?;
    let mut budget = overrides::PublicCompositionBudget::default();
    budget.render_budget.richtext_nonces.extend(richtext_nonces);
    budget
        .render_budget
        .richtext_nonces
        .extend(home_richtext_nonces);
    let custom_opts = crate::storage::options_pages::load_active_custom_options_public(
        db,
        Some(locale),
        false,
        Some(&mut budget.render_budget.richtext_nonces),
    )
    .await?;
    let locale_ctx =
        seo::build_theme_locale_context(db, Some(locale), seo::ThemeLocaleTarget::Home).await?;
    let inactive_search = content_render::SearchView::inactive_for_locale_ctx(&locale_ctx);
    budget.set_locale_context(active.as_ref(), locale, locale_ctx);
    let ct_value = serde_json::json!({ "key": schema.key, "label": schema.label });
    let cache = crate::page::BlockTemplateCache::global();
    let body = resolve_public_template_html(
        db,
        &candidates,
        active.as_ref(),
        &site_ctx,
        &site_opts,
        &theme_opts,
        Some(locale),
        &fallback_html,
        None,
        PublicRouteContext {
            route_kind: "home",
            route_path: Some(&canonical_path),
            entry_id: None,
            entry_type_key: None,
            custom_options: &custom_opts,
            breadcrumbs: &breadcrumbs,
            pagination: &pagination,
            archive_url: None,
            previous_url: previous.as_deref(),
            next_url: next.as_deref(),
            entries: &entries,
            taxonomy: None,
            term: None,
            term_schema: None,
            content_type: Some(&ct_value),
            entry: None,
            list_pages: None,
            search: &inactive_search,
        },
        &mut budget,
        cache,
    )
    .await?;
    let (header, footer, shell_has_form) = if body.canvas_layout || body.theme_context.is_null() {
        (None, None, false)
    } else {
        overrides::resolve_and_render_header_footer_with_context_for_locale(
            db,
            active.as_ref(),
            &body.theme_context,
            &body.precompiled_shell,
            Some(locale),
            &fmt_cfg,
            &mut budget,
            cache,
        )
        .await?
    };
    let canonical_for_seo = CanonicalUrlBuilder::with_query(
        canonical_path.clone(),
        query.clone(),
        QueryScope::ContentArchive,
    );
    let allow_alternates =
        query.page == 1 && query.filters == crate::public_context::PublicFilters::default();
    let allow_custom_canonical = query.page == 1;
    let seo_view = seo::build_home_seo_view_for_locale_with_canonical(
        db,
        Some(locale),
        &canonical_for_seo,
        allow_alternates,
        allow_custom_canonical,
    )
    .await?;
    let enqueued = enqueued_for_route(active.as_ref(), EnqueueCondition::Home);
    let canonical_query =
        crate::cache::canonicalize_public_query(raw_query, QueryScope::ContentArchive);
    let composed = content_render::compose_document_with_seo_with_assets(
        header.as_deref(),
        &body.html,
        footer.as_deref(),
        &seo_view,
        &["od-route-home", "od-route-index"],
        &enqueued,
        active.is_some(),
    )?;
    let composed = apply_public_render_hooks(
        composed,
        services,
        "home",
        &canonical_path,
        &canonical_query,
        false,
        locale,
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
