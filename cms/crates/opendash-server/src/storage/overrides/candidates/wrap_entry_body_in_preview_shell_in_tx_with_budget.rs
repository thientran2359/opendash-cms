use super::*;

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn wrap_entry_body_in_preview_shell_in_tx_with_budget(
    tx: &mut Transaction<'_, Sqlite>,
    themes_root: &Path,
    actor_role: FieldRole,
    schema: &EffectiveContentSchema,
    entry_id: i64,
    page_layout_key: Option<&str>,
    custom_template_key: Option<&str>,
    entry_rendered: RenderedLayout,
    mut entry_render_budget: page::RenderBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    let mut record = entries::get_entry_in_tx(tx, &schema.base.descriptor.key, entry_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry not found."))?;
    entries::filter_entry_values_for_role(&mut record, schema, actor_role);
    let active = theme::load_active_theme_in_conn(&mut **tx, themes_root).await?;
    let url_policy =
        active_theme_url_policy_in_conn(&mut **tx, active.as_ref(), None, Some(schema), None)
            .await?;
    let mut state =
        builder::read_render_context_for_locale_tx(tx, Some(&record.locale), true, None).await?;
    let mut options_richtext_nonces = template_sec::RichTextNonceTable::default();
    let (general, pub_settings, site_options, options, menus) = build_preview_context_in_tx(
        tx,
        active.as_ref(),
        None,
        &record.locale,
        Some(&mut options_richtext_nonces),
    )
    .await?;
    entry_render_budget
        .richtext_nonces
        .extend(options_richtext_nonces);
    state.site_options = site_options.clone();
    state.options = options.clone();
    for (slot_key, view) in &menus {
        state
            .menus
            .insert(slot_key.clone(), serde_json::to_value(view)?);
    }
    state.timezone = general.timezone.clone();
    let fmt_cfg = template_sec::RenderFormattingConfig {
        timezone: state.timezone.clone(),
        ..Default::default()
    };
    let site = SiteContext {
        title: &general.site_title,
        tagline: &general.tagline,
        timezone: &general.timezone,
        origin: normalized_origin(pub_settings.public_origin.as_deref()),
        current_year: content_render::current_year_in_timezone(&general.timezone),
    };
    let shell_route_kind = if schema.base.descriptor.key == "page" {
        "page"
    } else {
        "single"
    };
    let built = build_entry_preview_context_in_tx(
        tx,
        &record,
        schema,
        actor_role,
        active.as_ref(),
        &site,
        &site_options,
        &options,
        &menus,
        shell_route_kind,
        Some(&mut entry_render_budget.richtext_nonces),
    )
    .await?;
    let built_view = built.view;
    let built_breadcrumbs = built.breadcrumbs;
    let mut theme_context = built.theme_context;
    let ctx_target = if schema.base.descriptor.key == "page" {
        PublicTemplateContext::Page {
            page_layout_key: page_layout_key.map(ToOwned::to_owned),
        }
    } else {
        PublicTemplateContext::Single {
            content_type: schema.base.descriptor.key.clone(),
            entry_id: record.id,
            slug: record.slug.clone(),
            custom_template_key: custom_template_key.map(ToOwned::to_owned),
        }
    };
    let candidates = build_candidates(&ctx_target)?;
    let resolution = resolve_template_2axis_on_connection(
        &mut **tx,
        &candidates,
        active.as_ref().map(|a| &a.templates),
        active.as_ref().map(|a| &a.parts),
        active.as_ref().map(|a| &a.page_layouts),
    )
    .await?;
    let slot_fragment = crate::document::normalize_document_with_policy(
        &entry_rendered.html,
        crate::document::DocumentNormalizeOptions {
            allow_trusted_theme_markup: true,
            allow_admin_media_preview: true,
        },
    )
    .map(|parts| {
        let mut f = parts.authorized_styles.concat();
        f.push_str(&parts.body_fragment);
        f
    })
    .unwrap_or_else(|_| entry_rendered.html.clone());

    let mut preview_budget =
        PreviewCompositionBudget::with_render_budget(entry_render_budget.clone());
    let mut public_budget = PublicCompositionBudget::with_render_budget(entry_render_budget);
    let preview_locale_ctx = crate::storage::seo::build_theme_locale_context_in_conn(
        &mut **tx,
        Some(&record.locale),
        crate::storage::seo::ThemeLocaleTarget::Entry {
            entry_id: record.id,
        },
    )
    .await?;
    if let Some(obj) = theme_context.as_object_mut() {
        obj.insert("locale".to_owned(), preview_locale_ctx.clone());
    }
    public_budget.set_locale_context(active.as_ref(), &record.locale, preview_locale_ctx);
    let route = Some((record.content_type_key.as_str(), record.slug.as_str()));
    let mut diagnostics = entry_rendered.diagnostics;
    let mut has_active_contact_form = entry_rendered.has_active_contact_form;
    let mut canvas_layout = match &resolution {
        TemplateResolution::DbOverride(override_rec) => {
            is_canvas_target(active.as_ref(), &override_rec.target)
        }
        TemplateResolution::ActiveTheme { target_key, .. } => {
            is_canvas_resolved_key(active.as_ref(), target_key)
        }
        TemplateResolution::CoreFallback => false,
    };
    let mut shell_part_docs = Vec::new();
    if !canvas_layout {
        for slot_key in ["header", "footer"] {
            let part_target = TemplateCandidate {
                part_key: Some(slot_key.to_owned()),
                ..candidate(TemplateCandidateKind::Part)
            };
            let canonical = part_target.canonical_key().map_err(ApiError::internal)?;
            if let Some(row) = get_typed_override_on_connection(&mut **tx, &canonical).await? {
                shell_part_docs.push(row.document);
            }
        }
    }
    {
        let mut hydrate_roots: Vec<&BuilderDocument> = shell_part_docs.iter().collect();
        if let TemplateResolution::DbOverride(override_rec) = &resolution {
            hydrate_roots.push(&override_rec.document);
        }
        if !hydrate_roots.is_empty() {
            collect_db_docs_and_hydrate_state_in_tx(tx, &mut state, &hydrate_roots, true).await?;
        }
    }

    let preview_is_canvas = match &resolution {
        TemplateResolution::DbOverride(override_rec) => {
            is_canvas_target(active.as_ref(), &override_rec.target)
        }
        TemplateResolution::ActiveTheme { target_key, .. } => {
            is_canvas_resolved_key(active.as_ref(), target_key)
        }
        TemplateResolution::CoreFallback => false,
    };
    let effective_target = match &resolution {
        TemplateResolution::DbOverride(override_rec) => override_rec.target.clone(),
        _ => candidates
            .first()
            .cloned()
            .unwrap_or_else(|| candidate(TemplateCandidateKind::Index)),
    };
    let surface = inspect_preview_surfaces_in_tx(
        tx,
        &effective_target,
        None,
        Some(&resolution),
        active.as_ref(),
        &url_policy,
        preview_is_canvas,
    )
    .await?;
    let owned_td = build_preview_template_data_in_tx(
        tx,
        BuildPreviewTemplateDataInput {
            target: &effective_target,
            site_options: &site_options,
            menus_by_slot: menus.clone(),
            locale: &record.locale,
            actor_role,
            surface: &surface,
            real_entry: Some((&record, schema, &built_view, built_breadcrumbs)),
            entry_body_html: Some(slot_fragment.clone()),
            entry_has_builder_body: true,
            resolved_term: None,
        },
        Some(&mut preview_budget.richtext_nonces),
    )
    .await?;
    let td = owned_td.as_render_data(&site);
    populate_template_entry_schemas_in_conn(&mut **tx, &mut state, Some(&td)).await?;

    let template_html = match resolution {
        TemplateResolution::DbOverride(override_rec) => {
            canvas_layout = is_canvas_target(active.as_ref(), &override_rec.target);
            let graph = preview_part_graph(
                tx,
                &override_rec.target,
                &override_rec.document,
                active.as_ref(),
                &mut preview_budget,
            )
            .await?;
            let ctx = PreviewRenderContext {
                state: &state,
                graph: &graph,
                theme_context: &theme_context,
                route,
                render_locale: &record.locale,
                cache,
                url_policy: &url_policy,
                fmt_cfg: &fmt_cfg,
                search: None,
                template_data: Some(&td),
            };
            let mut tpl_rendered = render_preview_builder(
                tx,
                &override_rec.document,
                Some(&slot_fragment),
                &ctx,
                &mut preview_budget,
                RenderAnnotationScope::None,
                "ov",
            )
            .await?;
            diagnostics.append(&mut tpl_rendered.diagnostics);
            has_active_contact_form |= tpl_rendered.has_active_contact_form;
            tpl_rendered.html
        }
        TemplateResolution::ActiveTheme { target_key, html } => {
            canvas_layout = is_canvas_resolved_key(active.as_ref(), &target_key);
            let tpl_url_policy = active_theme_url_policy_in_conn(
                &mut **tx,
                active.as_ref(),
                Some(&target_key),
                Some(schema),
                None,
            )
            .await?;
            let out = render_csec_template_for_preview_with_policy(
                &target_key,
                &html,
                active.as_ref(),
                &tpl_url_policy,
                &theme_context,
                Some(&slot_fragment),
                &mut public_budget,
                &fmt_cfg,
            )?;
            preview_budget.sync_from_render_budget(&public_budget.render_budget);
            out
        }
        TemplateResolution::CoreFallback => slot_fragment,
    };

    let (header_html, footer_html) = if !canvas_layout {
        let hdr = render_preview_named_part_in_tx(
            tx,
            "header",
            &state,
            active.as_ref(),
            &theme_context,
            route,
            &record.locale,
            Some(&td),
            &mut preview_budget,
            cache,
        )
        .await?;
        let ftr = render_preview_named_part_in_tx(
            tx,
            "footer",
            &state,
            active.as_ref(),
            &theme_context,
            route,
            &record.locale,
            Some(&td),
            &mut preview_budget,
            cache,
        )
        .await?;
        (hdr.map(|r| r.html), ftr.map(|r| r.html))
    } else {
        (None, None)
    };

    let (_, bcp47_tag, dir) =
        settings::get_locale_bcp47_and_dir_in_conn(&mut **tx, Some(&record.locale)).await?;
    let seo = crate::seo::SeoView::for_noindex_page_with_dir(&bcp47_tag, &dir, &record.title);
    let condition = if record.content_type_key == "page" {
        EnqueueCondition::Page
    } else {
        EnqueueCondition::Single
    };
    let enqueued = preview_enqueued_styles(active.as_ref(), condition);
    let full_html = content_render::compose_document_with_seo_with_policy(
        header_html.as_deref(),
        &template_html,
        footer_html.as_deref(),
        &seo,
        &[],
        &enqueued,
        true,
        true,
    )?;
    let full_html = if !preview_budget.hook_nonces.is_empty() {
        let clear: Vec<(String, String)> = preview_budget
            .hook_nonces
            .entries()
            .iter()
            .map(|h| (h.nonce.clone(), String::new()))
            .collect();
        content_render::apply_render_hooks_to_document(full_html, "", &[], &[], "", &clear)?
    } else {
        full_html
    };
    let full_html =
        content_render::finalize_document_icons(full_html, &preview_budget.icon_nonces)?;
    let full_html =
        content_render::finalize_document_images(full_html, &preview_budget.image_nonces)?;
    let full_html =
        content_render::finalize_document_richtext(full_html, &preview_budget.richtext_nonces)?;
    Ok(RenderedLayout {
        has_active_contact_form,
        html: full_html,
        diagnostics,
    })
}
