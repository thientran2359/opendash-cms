use super::*;

pub(crate) fn enqueued_for_route(
    active: Option<&theme_storage::ActiveThemeV3>,
    condition: EnqueueCondition,
) -> ResolvedEnqueuedAssets {
    active.map_or_else(ResolvedEnqueuedAssets::default, |active| {
        resolve_enqueued_assets(&active.manifest, &active.revision_digest, condition)
    })
}

/// A missing or withdrawn Reading Page renders only a non-entry context. The
/// status and robots header remain authoritative even if an Index template
/// supplies the visual shell.
#[allow(dead_code)]
pub(crate) async fn render_not_found_response(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
) -> ApiResult<Response> {
    render_not_found_response_for_locale(db, themes_root, None, None).await
}

pub(crate) async fn render_not_found_response_for_locale(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    locale: Option<&str>,
    services: Option<&PublicRenderServices>,
) -> ApiResult<Response> {
    let active = match themes_root {
        Some(Extension(root)) => theme_storage::load_active_theme_v3(db, &root.0).await?,
        None => None,
    };
    let site = settings::get_general_for_locale(db, locale).await?;
    let pub_settings = settings::get_public(db).await?;
    let mut richtext_nonces = crate::template_sec::RichTextNonceTable::default();
    let (site_opts, opts) = settings::get_active_public_theme_options_for_locale_with_nonces(
        db,
        locale,
        Some(&mut richtext_nonces),
    )
    .await?;
    let (resolved_locale, bcp47_tag, dir) = settings::get_locale_bcp47_and_dir(db, locale).await?;
    let site_ctx = SiteContext {
        title: &site.site_title,
        tagline: &site.tagline,
        timezone: &site.timezone,
        origin: normalized_origin(pub_settings.public_origin.as_deref()),
        current_year: content_render::current_year_in_timezone(&site.timezone),
    };
    let fmt_cfg = crate::template_sec::RenderFormattingConfig {
        timezone: site.timezone.clone(),
    };
    let mut budget = overrides::PublicCompositionBudget::default();
    budget.render_budget.richtext_nonces.extend(richtext_nonces);
    let locale_ctx =
        seo::build_theme_locale_context(db, Some(&resolved_locale), seo::ThemeLocaleTarget::None)
            .await?;
    budget.set_locale_context(active.as_ref(), &resolved_locale, locale_ctx);
    let l10n = content_render::core_l10n(&resolved_locale);
    let breadcrumbs = content_render::not_found_breadcrumbs(
        content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
        Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
        content_render::DEFAULT_BREADCRUMB_NOT_FOUND_LABEL,
    );
    let pagination = PaginationView::empty();
    let empty_entries = Vec::new();
    let cache = crate::page::BlockTemplateCache::global();
    let body = resolve_public_template_html(
        db,
        &overrides::build_candidates(&PublicTemplateContext::NotFound)?,
        active.as_ref(),
        &site_ctx,
        &site_opts,
        &opts,
        locale,
        &format!("<main><h1>{}</h1></main>", l10n.not_found_title),
        None,
        PublicRouteContext {
            route_kind: "not_found",
            route_path: None,
            entry_id: None,
            entry_type_key: None,
            breadcrumbs: &breadcrumbs,
            pagination: &pagination,
            archive_url: None,
            previous_url: None,
            next_url: None,
            entries: &empty_entries,
            taxonomy: None,
            content_type: None,
            entry: None,
        },
        &mut budget,
        cache,
    )
    .await?;
    let (header_html, footer_html, shell_has_form) =
        if body.canvas_layout || body.theme_context.is_null() {
            (None, None, false)
        } else {
            overrides::resolve_and_render_header_footer_with_context_for_locale(
                db,
                active.as_ref(),
                &body.theme_context,
                &body.precompiled_shell,
                locale,
                &fmt_cfg,
                &mut budget,
                cache,
            )
            .await?
        };
    let view =
        crate::seo::SeoView::for_noindex_page_with_dir(&bcp47_tag, &dir, l10n.not_found_title);
    let enqueued = enqueued_for_route(active.as_ref(), EnqueueCondition::NotFound);
    let html = content_render::compose_document_with_seo_with_assets(
        header_html.as_deref(),
        &body.html,
        footer_html.as_deref(),
        &view,
        &["od-route-not-found"],
        &enqueued,
        active.is_some(),
    )?;
    let html = apply_public_render_hooks(
        html,
        services,
        "not_found",
        "/404",
        "",
        true,
        &resolved_locale,
        &body.theme_context,
        &budget,
    )
    .await?;
    let html = content_render::finalize_document_icons(html, &budget.render_budget.icon_nonces)?;
    let html =
        content_render::finalize_document_richtext(html, &budget.render_budget.richtext_nonces)?;
    let mut response = crate::routes::public::rendered_layout_with_form_csp(
        html,
        body.has_active_contact_form || shell_has_form,
    );
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
        .headers_mut()
        .insert("X-Robots-Tag", "noindex".parse().expect("static header"));
    Ok(response)
}

/// Called by the canonical fallback router.
/// The incoming route must already have been resolved by storage.
pub(crate) async fn render_canonical_archive(
    db: SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    type_key: String,
    locale: String,
    canonical_base: String,
    raw: Option<String>,
    services: Option<PublicRenderServices>,
) -> ApiResult<Response> {
    if type_key == "post"
        && matches!(
            seo::reading_page_state_for_locale(&db, Some(&locale), false).await?,
            ReadingPageState::Missing
        )
    {
        return render_not_found_response_for_locale(
            &db,
            themes_root,
            Some(&locale),
            services.as_ref(),
        )
        .await;
    }
    // Enforce the CPT's public/archive settings before loading or rendering
    // content. The storage archive query independently checks this policy.
    public_content::require_public_archive(&db, &type_key).await?;
    let query =
        parse_query(raw.as_deref(), QueryScope::ContentArchive).map_err(ApiError::bad_request)?;
    let _context = PublicRequestContext::Archive {
        type_key: type_key.clone(),
        query: query.clone(),
        canonical_path: CanonicalUrlBuilder::with_query(
            canonical_base.clone(),
            query.clone(),
            QueryScope::ContentArchive,
        ),
    };
    let page = query.page;
    let filters = EntryFilters {
        category_id: query.filters.category_id,
        tag_id: query.filters.tag_id,
        taxonomy_key: query.filters.taxonomy_key.clone(),
        term_id: query.filters.term_id,
        include_descendants: query.filters.descendants,
        locale: Some(locale.clone()),
    };
    let effective_schema = content_storage::load_effective_content_schema(&db, &type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let schema = &effective_schema.base.descriptor;
    let offset = (page - 1) * ARCHIVE_SIZE;
    let (records, total_items) =
        public_content::list_published_with_total(&db, &type_key, ARCHIVE_SIZE, offset, filters)
            .await?;
    let has_more = offset.saturating_add(ARCHIVE_SIZE) < total_items;
    let entry_urls = crate::storage::seo::resolve_public_entry_paths_by_ids(&db, &records).await?;
    let pairs: Vec<(i64, String)> = records
        .iter()
        .map(|r| (r.id, r.content_type_key.clone()))
        .collect();
    let mut terms_map = public_content::batch_load_public_entry_terms(&db, &pairs).await?;
    let eligible_taxonomies =
        public_content::batch_load_eligible_public_taxonomies(&db, std::slice::from_ref(&type_key))
            .await?
            .remove(&type_key)
            .unwrap_or_default();
    let archive_ids: Vec<i64> = records.iter().map(|r| r.id).collect();
    let mut proj_map =
        crate::storage::plugin_projections::batch_load_entry_projections(&db, &archive_ids).await?;
    let entries: Vec<_> = records
        .into_iter()
        .map(|record| {
            let u = entry_urls.get(&record.id).cloned();
            let terms = terms_map.remove(&record.id).unwrap_or_default();
            let mut view = content_render::entry_view_with_context(
                &record,
                &effective_schema,
                u,
                &terms,
                &eligible_taxonomies,
                FieldRole::Editor,
            );
            if let Some(proj) = proj_map.remove(&record.id) {
                view.ext = proj.ext;
                view.theme_ext = proj.theme_ext;
            }
            view
        })
        .collect();
    let previous = (page > 1)
        .then(|| {
            Some(CanonicalUrlBuilder::with_query(
                canonical_base.clone(),
                PublicQuery {
                    page: page - 1,
                    ..query.clone()
                },
                QueryScope::ContentArchive,
            ))
        })
        .flatten();
    let next = (has_more && page < MAX_ARCHIVE_PAGE)
        .then(|| {
            Some(CanonicalUrlBuilder::with_query(
                canonical_base.clone(),
                PublicQuery {
                    page: page + 1,
                    ..query.clone()
                },
                QueryScope::ContentArchive,
            ))
        })
        .flatten();
    let pagination = PaginationView::compute(
        page,
        ARCHIVE_SIZE,
        total_items,
        previous.clone(),
        next.clone(),
        |p| {
            Some(CanonicalUrlBuilder::with_query(
                canonical_base.clone(),
                PublicQuery {
                    page: p,
                    ..query.clone()
                },
                QueryScope::ContentArchive,
            ))
        },
    );
    let breadcrumbs = content_render::archive_breadcrumbs(
        content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
        Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
        &schema.label,
        Some(canonical_base.clone()),
    );
    let active_v3 = match themes_root {
        Some(Extension(root)) => theme_storage::load_active_theme_v3(&db, &root.0).await?,
        None => None,
    };
    let site = settings::get_general_for_locale(&db, Some(&locale)).await?;
    let pub_settings = settings::get_public(&db).await?;
    let mut richtext_nonces = crate::template_sec::RichTextNonceTable::default();
    let (site_opts, theme_opts) = settings::get_active_public_theme_options_for_locale_with_nonces(
        &db,
        Some(&locale),
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
    };
    let fallback_html = content_render::render_archive_with_site_and_config_for_locale(
        schema,
        &entries,
        previous.as_deref(),
        next.as_deref(),
        &pagination,
        &breadcrumbs,
        None,
        None,
        &site_ctx,
        &fmt_cfg,
        &locale,
    )?;
    let candidates = overrides::build_candidates(&PublicTemplateContext::Archive {
        content_type: type_key.clone(),
    })?;
    let mut budget = overrides::PublicCompositionBudget::default();
    budget.render_budget.richtext_nonces.extend(richtext_nonces);
    let locale_ctx = seo::build_theme_locale_context(
        &db,
        Some(&locale),
        seo::ThemeLocaleTarget::Archive {
            type_key: &type_key,
        },
    )
    .await?;
    budget.set_locale_context(active_v3.as_ref(), &locale, locale_ctx);
    // Do not export ContentTypeDefinition fields/settings/defaults to a
    // public theme: field-level access is represented exclusively by
    // exposure-filtered EntryView values.
    let ct_value = serde_json::json!({ "key": schema.key, "label": schema.label });
    let cache = crate::page::BlockTemplateCache::global();
    let body = resolve_public_template_html(
        &db,
        &candidates,
        active_v3.as_ref(),
        &site_ctx,
        &site_opts,
        &theme_opts,
        Some(&locale),
        &fallback_html,
        None,
        PublicRouteContext {
            route_kind: "archive",
            route_path: Some(&canonical_base),
            entry_id: None,
            entry_type_key: None,
            breadcrumbs: &breadcrumbs,
            pagination: &pagination,
            archive_url: Some(&canonical_base),
            previous_url: previous.as_deref(),
            next_url: next.as_deref(),
            entries: &entries,
            taxonomy: None,
            content_type: Some(&ct_value),
            entry: None,
        },
        &mut budget,
        cache,
    )
    .await?;
    let (header, footer, shell_has_form) = if body.canvas_layout || body.theme_context.is_null() {
        (None, None, false)
    } else {
        overrides::resolve_and_render_header_footer_with_context_for_locale(
            &db,
            active_v3.as_ref(),
            &body.theme_context,
            &body.precompiled_shell,
            Some(&locale),
            &fmt_cfg,
            &mut budget,
            cache,
        )
        .await?
    };
    let seo_view = seo::build_archive_seo_view_for_locale(
        &db,
        &type_key,
        &locale,
        &schema.label,
        &canonical_base,
        &query,
    )
    .await?;
    let type_class = format!("od-type-{type_key}");
    let enqueued = enqueued_for_route(active_v3.as_ref(), EnqueueCondition::Archive);
    let canonical_query =
        crate::cache::canonicalize_public_query(raw.as_deref(), QueryScope::ContentArchive);
    let composed = content_render::compose_document_with_seo_with_assets(
        header.as_deref(),
        &body.html,
        footer.as_deref(),
        &seo_view,
        &["od-route-archive", &type_class],
        &enqueued,
        active_v3.is_some(),
    )?;
    let composed = apply_public_render_hooks(
        composed,
        services.as_ref(),
        "archive",
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
            content_render::finalize_document_icons(composed, &budget.render_budget.icon_nonces)?,
            &budget.render_budget.richtext_nonces,
        )?,
        body.has_active_contact_form || shell_has_form,
    ))
}
