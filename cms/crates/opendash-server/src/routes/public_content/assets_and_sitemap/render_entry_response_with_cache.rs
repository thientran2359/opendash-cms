use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn render_entry_response_with_cache(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    entry: crate::storage::entries::ContentEntryRecord,
    canonical_path_override: Option<String>,
    is_home: bool,
    raw_query: Option<&str>,
    cache: &crate::page::BlockTemplateCache,
    services: Option<&PublicRenderServices>,
) -> ApiResult<Response> {
    let db = db.clone();
    let list_pages = crate::public_context::parse_list_pages(raw_query);
    let list_base_path = canonical_path_override.as_deref().unwrap_or("/");
    let snapshot = public_content::load_entry_render_snapshot_with_options(
        &db,
        entry.id,
        themes_root.as_ref().map(|Extension(root)| root.0.as_path()),
        cache,
        list_base_path,
        &list_pages,
    )
    .await?;
    let Some(snapshot) = snapshot else {
        return render_not_found_response_for_locale(&db, themes_root, None, services).await;
    };
    let public_content::PublicEntryRenderSnapshot {
        entry,
        locale: entry_locale,
        effective_schema,
        eligible_public_taxonomies,
        terms,
        active: active,
        mut resolution,
        body: entry_body,
        budget: mut entry_render_budget,
    } = snapshot;
    let schema = &effective_schema.base.descriptor;
    if is_home {
        let page_layout_key = entry
            .page_layout_key
            .as_ref()
            .filter(|key| {
                active.as_ref().is_some_and(|theme| {
                    theme
                        .manifest
                        .page_layouts
                        .iter()
                        .any(|layout| &layout.key == *key)
                })
            })
            .cloned();
        // Preserve the Page's own Builder body. Only the shared template
        // candidate chain changes: PageLayout -> Page -> Index.
        let candidates =
            overrides::build_candidates(&PublicTemplateContext::Page { page_layout_key })?;
        resolution = overrides::resolve_template_2axis(
            &db,
            &candidates,
            active.as_ref().map(|v| &v.templates),
            active.as_ref().map(|v| &v.parts),
            active.as_ref().map(|v| &v.page_layouts),
        )
        .await?;
    }
    let type_key = entry.content_type_key.clone();
    let entry_id_for_seo = entry.id;
    let entry_title_for_seo = entry.title.clone();
    let canonical_path = canonical_path_override
        .ok_or_else(|| ApiError::not_found("Không tìm thấy canonical nội dung."))?;
    let canonical_path_for_seo = canonical_path.clone();
    let _context = if is_home {
        PublicRequestContext::Home {
            canonical_path: canonical_path.clone(),
        }
    } else {
        PublicRequestContext::Single {
            entry_id: entry.id,
            type_key: type_key.clone(),
            slug: entry.slug.clone(),
            canonical_path: canonical_path.clone(),
        }
    };
    let site = settings::get_general_for_locale(&db, Some(&entry_locale)).await?;
    let pub_settings = settings::get_public(&db).await?;
    let mut richtext_nonces = crate::template_sec::RichTextNonceTable::default();
    let (site_opts, theme_opts) = settings::get_active_public_theme_options_for_locale_with_nonces(
        &db,
        Some(&entry_locale),
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
    let (archive_path, view) = {
        let mut conn = db.acquire().await?;
        let path =
            permalinks::canonical_archive_path_in_tx(&mut conn, &type_key, &entry_locale).await?;
        let mut proj_map =
            crate::storage::plugin_projections::batch_load_entry_projections_in_conn(
                &mut conn,
                &[entry.id],
            )
            .await?;
        let mut view = public_content::hydrate_single_entry_view_in_conn(
            &mut conn,
            &entry,
            &effective_schema,
            Some(canonical_path_for_seo.clone()),
            &terms,
            &eligible_public_taxonomies,
            FieldRole::Editor,
            false,
            Some(&mut richtext_nonces),
        )
        .await?;
        if let Some(proj) = proj_map.remove(&entry.id) {
            view.ext = proj.ext;
            view.theme_ext = proj.theme_ext;
        }
        (path, view)
    };
    let breadcrumbs = if is_home {
        content_render::home_breadcrumbs(
            content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
            Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
        )
    } else {
        content_render::single_breadcrumbs(
            content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
            Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
            &type_key,
            &schema.label,
            schema.settings.has_archive,
            archive_path.clone(),
            view.primary_category.as_ref(),
            &view.title,
            Some(canonical_path_for_seo.clone()),
        )
    };
    let entry_has_form = entry_body
        .as_ref()
        .is_some_and(|body| body.has_active_contact_form);
    let builder_body_fragment = entry_body.as_ref().map(|body| {
        match crate::document::normalize_public_document_with_options(&body.html, true) {
            Ok(parts) => {
                let mut fragment = parts.authorized_styles.concat();
                fragment.push_str(&parts.body_fragment);
                fragment
            }
            Err(_) => body.html.clone(),
        }
    });
    let classic_body_fragment = if entry_body.is_none() {
        content_render::render_entry_body_fragment(&entry.values, &effective_schema)
    } else {
        None
    };
    let entry_has_builder_body = builder_body_fragment.is_some();
    let entry_body_html = builder_body_fragment
        .as_deref()
        .or(classic_body_fragment.as_deref());
    let fallback_html = match entry_body {
        Some(body) => body.html,
        None => content_render::render_single_with_context_for_locale(
            schema,
            &view,
            None,
            &site_ctx,
            archive_path.as_deref(),
            &breadcrumbs,
            &fmt_cfg,
            &entry_locale,
        )?,
    };
    entry_render_budget.locale = entry_locale.clone();
    let mut budget = overrides::PublicCompositionBudget::with_render_budget(entry_render_budget);
    budget.render_budget.richtext_nonces.extend(richtext_nonces);
    let custom_opts = crate::storage::options_pages::load_active_custom_options_public(
        &db,
        Some(&entry_locale),
        false,
        Some(&mut budget.render_budget.richtext_nonces),
    )
    .await?;
    let locale_ctx = seo::build_theme_locale_context(
        &db,
        Some(&entry_locale),
        if is_home {
            seo::ThemeLocaleTarget::Home
        } else {
            seo::ThemeLocaleTarget::Entry { entry_id: entry.id }
        },
    )
    .await?;
    let inactive_search = content_render::SearchView::inactive_for_locale_ctx(&locale_ctx);
    budget.set_locale_context(active.as_ref(), &entry_locale, locale_ctx);
    let ct_value = serde_json::json!({ "key": schema.key, "label": schema.label });
    let pagination = PaginationView::empty();
    let empty_entries = Vec::new();
    let body = render_resolved_public_template_html(
        &db,
        resolution,
        active.as_ref(),
        &site_ctx,
        &site_opts,
        &theme_opts,
        Some(&entry_locale),
        &fallback_html,
        Some((&type_key, &entry.slug)),
        PublicRouteContext {
            route_kind: if is_home { "home" } else { "single" },
            route_path: Some(&canonical_path_for_seo),
            resolved_locale: &entry_locale,
            locale_target: if is_home {
                seo::ThemeLocaleTarget::Home
            } else {
                seo::ThemeLocaleTarget::Entry { entry_id: entry.id }
            },
            entry_id: Some(entry.id),
            entry_type_key: Some(&type_key),
            entry_schema: Some(&effective_schema),
            entry_body_html,
            entry_has_builder_body,
            custom_options: &custom_opts,
            breadcrumbs: &breadcrumbs,
            pagination: &pagination,
            archive_url: archive_path.as_deref(),
            previous_url: None,
            next_url: None,
            entries: &empty_entries,
            taxonomy: None,
            term: None,
            term_schema: None,
            term_summary: None,
            content_type: Some(&ct_value),
            archive_type: Some(content_render::ArchiveTypeSummary {
                key: schema.key.clone(),
                label: schema.label.clone(),
                description: schema.description.clone(),
            }),
            entry: Some(&view),
            list_pages: Some(&list_pages),
            search: &inactive_search,
        },
        &mut budget,
        cache,
    )
    .await?;
    let (header, footer, shell_has_form) = if body.canvas_layout || body.theme_context.is_null() {
        (None, None, false)
    } else {
        overrides::resolve_and_render_header_footer_with_template_data_for_locale(
            &db,
            active.as_ref(),
            &body.theme_context,
            &body.precompiled_shell,
            Some(&entry_locale),
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
    let seo_view = if is_home {
        seo::build_home_seo_view_for_locale(&db, Some(&entry_locale), &canonical_path_for_seo, true)
            .await?
    } else {
        seo::build_entry_seo_view_for_locale(
            &db,
            entry_id_for_seo,
            &entry_title_for_seo,
            Some(&entry_locale),
            &canonical_path_for_seo,
        )
        .await?
    };
    let type_class = format!("od-type-{type_key}");
    let body_classes: &[&str] = if is_home {
        &["od-route-home"]
    } else {
        &["od-route-single", &type_class]
    };
    let route_condition = if is_home {
        EnqueueCondition::Home
    } else if type_key == "page" {
        EnqueueCondition::Page
    } else {
        EnqueueCondition::Single
    };
    let enqueued = enqueued_for_route(active.as_ref(), route_condition);
    let canonical_query = crate::cache::canonicalize_public_query(raw_query, QueryScope::Single);
    let composed = content_render::compose_document_with_seo_with_assets(
        header.as_deref(),
        &body.html,
        footer.as_deref(),
        &seo_view,
        body_classes,
        &enqueued,
        active.is_some(),
    )?;
    let composed = apply_public_render_hooks(
        composed,
        services,
        if is_home { "home" } else { "single" },
        &canonical_path_for_seo,
        &canonical_query,
        false,
        &entry_locale,
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
        body.has_active_contact_form || entry_has_form || shell_has_form,
    ))
}

pub(crate) async fn media_response(
    method: &Method,
    headers: &HeaderMap,
    db: &SqlitePool,
    store: &MediaStore,
    id: &str,
    thumbnail: bool,
) -> ApiResult<Response> {
    if !media::valid_media_id(id) || !public_content::media_is_referenced(db, id).await? {
        return Err(ApiError::not_found("Ảnh công khai không tồn tại."));
    }
    let read_lease = store.acquire_read_lease(id).await?;
    let asset = media::find(db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Ảnh công khai không tồn tại."))?;
    if thumbnail && asset.kind != "image" {
        return Err(ApiError::not_found("Ảnh công khai không tồn tại."));
    }
    if !thumbnail && asset.kind == "video" {
        let path = store.canonical_path(id, &asset.extension)?;
        return crate::media::stream_authorized_video(
            method, headers, &asset, &path, read_lease, false,
        )
        .await;
    }
    let target = if thumbnail {
        crate::media::LeasedImageTarget::Thumbnail(&asset)
    } else {
        crate::media::LeasedImageTarget::Original(&asset)
    };
    crate::media::serve_leased_image(store, &read_lease, method, headers, target, true).await
}

pub async fn media_file(
    method: Method,
    headers: HeaderMap,
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    media_response(&method, &headers, &db, &store, &id, false).await
}

pub async fn media_thumb(
    method: Method,
    headers: HeaderMap,
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    media_response(&method, &headers, &db, &store, &id, true).await
}

pub async fn media_variant(
    method: Method,
    headers: HeaderMap,
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    Path((id, spec_ext)): Path<(String, String)>,
) -> ApiResult<Response> {
    let (spec, format) = opendash_core::media::parse_variant_spec_ext(&spec_ext)
        .ok_or_else(|| ApiError::not_found("Biến thể media công khai không tồn tại."))?;
    if !media::valid_media_id(&id) || !public_content::media_is_referenced(&db, &id).await? {
        return Err(ApiError::not_found(
            "Biến thể media công khai không tồn tại.",
        ));
    }
    let read_lease = store.acquire_read_lease(&id).await?;
    let asset = media::find(&db, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("Biến thể media công khai không tồn tại."))?;
    if asset.kind != "image" {
        return Err(ApiError::not_found(
            "Biến thể media công khai không tồn tại.",
        ));
    }
    media::find_variant(&db, &id, asset.version, spec, format)
        .await?
        .ok_or_else(|| ApiError::not_found("Biến thể media công khai không tồn tại."))?;
    crate::media::serve_leased_image(
        store.as_ref(),
        &read_lease,
        &method,
        &headers,
        crate::media::LeasedImageTarget::Variant {
            asset: &asset,
            spec,
            format,
        },
        true,
    )
    .await
}
