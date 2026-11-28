use super::*;

#[allow(clippy::too_many_arguments)]
pub async fn preview_studio_with_cache_and_locale(
    db: &SqlitePool,
    themes_root: &Path,
    actor: &SessionContext,
    target: &TemplateCandidate,
    document: Option<&BuilderDocument>,
    preview_entry: Option<&TemplatePreviewEntry>,
    preview_annotations: bool,
    transient: Option<&TransientStudioOverrides>,
    preview_locale: Option<&str>,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    use TemplateCandidateKind as K;
    let key = target.canonical_key().map_err(ApiError::bad_request)?;
    let mut tx = db.begin().await?;
    let effective_locale =
        settings::resolve_requested_locale_in_conn(&mut *tx, preview_locale).await?;
    validate_typed_target_in_tx(&mut tx, target).await?;
    let active = theme::load_active_theme_v3_in_conn(&mut tx, themes_root).await?;
    validate_transient_options_pages_refs_in_conn(
        &mut tx,
        active.as_ref().map(|a| a.manifest.theme_id.as_str()),
        transient.and_then(|t| t.options_pages.as_ref()),
    )
    .await?;
    let url_policy =
        active_theme_url_policy_in_conn(&mut tx, active.as_ref(), Some(&key), None, None).await?;
    let mut state =
        builder::read_render_context_for_locale_tx(&mut tx, Some(&effective_locale), true, None)
            .await?;
    let mut options_richtext_nonces = template_sec::RichTextNonceTable::default();
    let (general, pub_settings, site_options, options, menus) = build_preview_context_in_tx(
        &mut tx,
        active.as_ref(),
        transient,
        &effective_locale,
        Some(&mut options_richtext_nonces),
    )
    .await?;
    state.site_options = site_options.clone();
    state.options = options.clone();
    for (slot_key, view) in &menus {
        state
            .menus
            .insert(slot_key.clone(), serde_json::to_value(view)?);
    }
    if let Some(slot_ov) = transient.and_then(|t| t.menu_slots.as_ref()) {
        for (slot_key, maybe_id) in slot_ov {
            if maybe_id.is_none() {
                state.menus.remove(slot_key);
            }
        }
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

    let default_home_entry: Option<TemplatePreviewEntry> = if preview_entry.is_none()
        && target.kind == K::Home
    {
        let reading =
            settings::get_reading_in_conn_for_locale(&mut *tx, Some(&effective_locale)).await?;
        match reading.homepage_page_id {
            Some(id) => {
                let exists: Option<i64> = sqlx::query_scalar(
                        "SELECT id FROM content_entries WHERE id = ? AND content_type_key = 'page' AND locale = ? AND trashed_at IS NULL",
                    )
                    .bind(id)
                    .bind(&effective_locale)
                    .fetch_optional(&mut *tx)
                    .await?;
                exists.map(|entry_id| TemplatePreviewEntry {
                    type_key: "page".to_owned(),
                    entry_id,
                })
            }
            None => None,
        }
    } else {
        None
    };
    let effective_sample = preview_entry.or(default_home_entry.as_ref());
    let actor_role = actor_field_role(actor);
    let entry = resolve_preview_entry(
        &mut tx,
        target,
        effective_sample,
        active.as_ref(),
        &effective_locale,
        actor_role,
    )
    .await?;

    let mut diagnostics = Vec::new();
    let mut has_active_contact_form = false;
    let mut route = None;
    let mut shared_rb = page::RenderBudget::new();
    shared_rb.richtext_nonces.extend(options_richtext_nonces);
    let (mut theme_context, body) = if let Some((record, effective_schema)) = &entry {
        route = Some((record.content_type_key.as_str(), record.slug.as_str()));
        let built = build_entry_preview_context_in_tx(
            &mut tx,
            record,
            effective_schema,
            actor_role,
            active.as_ref(),
            &site,
            &site_options,
            &options,
            &menus,
            route_kind_for_candidate(target.kind),
            &effective_locale,
            transient,
            Some(&mut shared_rb.richtext_nonces),
        )
        .await?;
        let definition = &effective_schema.base.descriptor;
        let raw_body_has_blocks = record
            .values
            .get("body")
            .and_then(serde_json::Value::as_str)
            .is_some_and(opendash_core::builder_blocks::contains_blocks);
        let body = if raw_body_has_blocks
            && definition.settings.supports(ContentSupport::Builder)
            && record.content_type_key != "pattern"
        {
            if let Some(mut rendered) =
                builder::preview_saved_page_body_in_tx_with_options_and_budget(
                    &mut tx,
                    actor,
                    &record.content_type_key,
                    record.id,
                    Some(&site_options),
                    Some(&options),
                    Some(general.timezone.as_str()),
                    &mut shared_rb,
                    cache,
                )
                .await?
            {
                diagnostics.append(&mut rendered.diagnostics);
                Some(rendered.html)
            } else {
                None
            }
        } else {
            None
        }
        .map(Ok)
        .unwrap_or_else(|| {
            let html = content_render::render_single_with_context_for_locale(
                definition,
                &built.view,
                None,
                &site,
                built.archive_url.as_deref(),
                &built.breadcrumbs,
                &fmt_cfg,
                &effective_locale,
            )?;
            crate::document::normalize_document_with_policy(
                &html,
                crate::document::DocumentNormalizeOptions {
                    allow_trusted_theme_markup: false,
                    allow_admin_media_preview: true,
                },
            )
            .map(|doc| doc.body_fragment)
            .map_err(|_| ApiError::bad_request("Invalid preview body."))
        })?;
        (built.theme_context, body)
    } else {
        let custom_options =
            crate::storage::options_pages::load_active_custom_options_public_with_overrides_in_conn(
                &mut tx,
                Some(&effective_locale),
                true,
                transient.and_then(|t| t.options_pages.as_ref()),
                Some(&mut shared_rb.richtext_nonces),
            )
            .await?;
        let theme_context = build_placeholder_preview_theme_context(
            target,
            active.as_ref(),
            &site,
            &site_options,
            &options,
            &custom_options,
            &menus,
            &effective_locale,
        )?;
        (
            theme_context,
            "<div data-template-preview-placeholder>Preview content</div>".to_owned(),
        )
    };
    if body.len() > template_sec::MAX_RENDERED_HTML_BYTES {
        return Err(ApiError::bad_request("Preview output exceeds 512 KiB."));
    }

    let slot_fragment = crate::document::normalize_document_with_policy(
        &body,
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
    .unwrap_or_else(|_| body.clone());

    let scope = if preview_annotations {
        RenderAnnotationScope::TemplateAll
    } else {
        RenderAnnotationScope::None
    };
    let mut preview_budget = PreviewCompositionBudget::with_render_budget(shared_rb.clone());
    let mut public_budget = PublicCompositionBudget::with_render_budget(shared_rb);
    let preview_locale_ctx = crate::storage::seo::build_theme_locale_context_in_conn(
        &mut tx,
        Some(&effective_locale),
        crate::storage::seo::ThemeLocaleTarget::None,
    )
    .await?;
    content_render::sync_preview_locale_and_search(&mut theme_context, &preview_locale_ctx);
    public_budget.set_locale_context(active.as_ref(), &effective_locale, preview_locale_ctx);
    let preview_search_view = (target.kind == K::Search).then(|| {
        content_render::SearchView::preview_sample(&state.locale_search_url, &effective_locale)
    });
    let preview_search_entries = if target.kind == K::Search {
        content_render::SearchView::preview_sample_entries(&effective_locale)
    } else {
        Vec::new()
    };
    let preview_search_pagination = PaginationView::empty();
    let preview_search = preview_search_view
        .as_ref()
        .map(|view| page::BuilderSearchData {
            view,
            entries: &preview_search_entries,
            pagination: &preview_search_pagination,
        });
    let mut canvas_layout = is_canvas_target(active.as_ref(), target);
    let main_scope = if target.kind == K::Part {
        part_scope_prefix(target.part_key.as_deref().unwrap_or("part"), 0)
    } else {
        "ov".to_owned()
    };

    enum PreviewMainPlan<'a> {
        DraftDoc(&'a BuilderDocument),
        Resolved(TemplateResolution),
    }
    let main_plan = if let Some(doc) = document {
        let existing = get_typed_override_on_connection(&mut tx, &key).await?;
        validate_typed_document_in_tx(&mut tx, target, doc, existing.as_ref().map(|r| &r.document))
            .await?;
        PreviewMainPlan::DraftDoc(doc)
    } else {
        let chain = build_reset_candidate_chain_for_locale_in_conn(
            &mut tx,
            target,
            Some(&effective_locale),
        )
        .await?;
        let resolved = resolve_template_2axis_on_connection(
            &mut tx,
            &chain,
            active.as_ref().map(|a| &a.templates),
            active.as_ref().map(|a| &a.parts),
            active.as_ref().map(|a| &a.page_layouts),
        )
        .await?;
        match &resolved {
            TemplateResolution::DbOverride(record) => {
                canvas_layout = is_canvas_target(active.as_ref(), &record.target);
            }
            TemplateResolution::ActiveTheme { target_key, .. } => {
                canvas_layout = is_canvas_resolved_key(active.as_ref(), target_key);
            }
            TemplateResolution::CoreFallback => {
                canvas_layout = false;
            }
        }
        PreviewMainPlan::Resolved(resolved)
    };

    let mut shell_part_docs = Vec::new();
    if target.kind != K::Part && !canvas_layout {
        for slot_key in ["header", "footer"] {
            let part_target = TemplateCandidate {
                part_key: Some(slot_key.to_owned()),
                ..candidate(K::Part)
            };
            let canonical = part_target.canonical_key().map_err(ApiError::bad_request)?;
            if let Some(row) = get_typed_override_on_connection(&mut *tx, &canonical).await? {
                shell_part_docs.push(row.document);
            }
        }
    }
    {
        let mut hydrate_roots: Vec<&BuilderDocument> = shell_part_docs.iter().collect();
        match &main_plan {
            PreviewMainPlan::DraftDoc(doc) => hydrate_roots.push(doc),
            PreviewMainPlan::Resolved(TemplateResolution::DbOverride(record)) => {
                hydrate_roots.push(&record.document);
            }
            _ => {}
        }
        if !hydrate_roots.is_empty() {
            collect_db_docs_and_hydrate_state_in_tx(&mut tx, &mut state, &hydrate_roots, true)
                .await?;
        }
    }

    let main_html = match main_plan {
        PreviewMainPlan::DraftDoc(doc) => {
            let graph =
                preview_part_graph(&mut tx, target, doc, active.as_ref(), &mut preview_budget)
                    .await?;
            let ctx = PreviewRenderContext {
                state: &state,
                graph: &graph,
                theme_context: &theme_context,
                route,
                render_locale: &effective_locale,
                cache,
                url_policy: &url_policy,
                fmt_cfg: &fmt_cfg,
                search: preview_search,
            };
            let mut rendered = render_preview_builder(
                &mut tx,
                doc,
                Some(&slot_fragment),
                &ctx,
                &mut preview_budget,
                scope,
                &main_scope,
            )
            .await?;
            diagnostics.append(&mut rendered.diagnostics);
            has_active_contact_form |= rendered.has_active_contact_form;
            rendered.html
        }
        PreviewMainPlan::Resolved(resolved) => match resolved {
            TemplateResolution::DbOverride(record) => {
                let graph = preview_part_graph(
                    &mut tx,
                    &record.target,
                    &record.document,
                    active.as_ref(),
                    &mut preview_budget,
                )
                .await?;
                let ctx = PreviewRenderContext {
                    state: &state,
                    graph: &graph,
                    theme_context: &theme_context,
                    route,
                    render_locale: &effective_locale,
                    cache,
                    url_policy: &url_policy,
                    fmt_cfg: &fmt_cfg,
                    search: preview_search,
                };
                let slot = if target.kind == K::Part {
                    None
                } else {
                    Some(slot_fragment.as_str())
                };
                let mut rendered = render_preview_builder(
                    &mut tx,
                    &record.document,
                    slot,
                    &ctx,
                    &mut preview_budget,
                    scope,
                    &main_scope,
                )
                .await?;
                diagnostics.append(&mut rendered.diagnostics);
                has_active_contact_form |= rendered.has_active_contact_form;
                rendered.html
            }
            TemplateResolution::ActiveTheme { target_key, html } => {
                let slot = if target.kind == K::Part {
                    None
                } else {
                    Some(slot_fragment.as_str())
                };
                let tpl_url_policy = active_theme_url_policy_in_conn(
                    &mut tx,
                    active.as_ref(),
                    Some(&target_key),
                    entry.as_ref().map(|(_, eff)| eff),
                    None,
                )
                .await?;
                let out = render_csec_template_for_preview_with_policy(
                    &target_key,
                    &html,
                    active.as_ref(),
                    &tpl_url_policy,
                    &theme_context,
                    slot,
                    &mut public_budget,
                    &fmt_cfg,
                )?;
                preview_budget.sync_from_render_budget(&public_budget.render_budget);
                out
            }
            TemplateResolution::CoreFallback => {
                if target.kind == K::Part {
                    String::new()
                } else {
                    format!(
                        "<main><h1>{}</h1><p>{}</p>{}</main>",
                        escape_preview_text(&general.site_title),
                        escape_preview_text(&general.tagline),
                        slot_fragment
                    )
                }
            }
        },
    };

    let (header_html, footer_html) = if target.kind != K::Part && !canvas_layout {
        let hdr = render_preview_named_part_in_tx(
            &mut tx,
            "header",
            &state,
            active.as_ref(),
            &theme_context,
            route,
            &effective_locale,
            &mut preview_budget,
            cache,
        )
        .await?;
        let ftr = render_preview_named_part_in_tx(
            &mut tx,
            "footer",
            &state,
            active.as_ref(),
            &theme_context,
            route,
            &effective_locale,
            &mut preview_budget,
            cache,
        )
        .await?;
        if let Some(ref h) = hdr {
            has_active_contact_form |= h.has_active_contact_form;
        }
        if let Some(ref f) = ftr {
            has_active_contact_form |= f.has_active_contact_form;
        }
        (hdr.map(|r| r.html), ftr.map(|r| r.html))
    } else {
        (None, None)
    };

    let (_, bcp47_tag, dir) =
        settings::get_locale_bcp47_and_dir_in_conn(&mut *tx, Some(&effective_locale)).await?;
    let seo = crate::seo::SeoView::for_noindex_page_with_dir(&bcp47_tag, &dir, &general.site_title);
    let enqueued = preview_enqueued_styles(active.as_ref(), preview_condition(target.kind));
    let full_html = content_render::compose_document_with_seo_with_policy(
        header_html.as_deref(),
        &main_html,
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
    tx.rollback().await?;
    Ok(RenderedLayout {
        has_active_contact_form,
        html: full_html,
        diagnostics,
    })
}

pub(in crate::storage) fn escape_preview_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub async fn preview_studio_with_cache(
    db: &SqlitePool,
    themes_root: &Path,
    actor: &SessionContext,
    target: &TemplateCandidate,
    document: Option<&BuilderDocument>,
    preview_entry: Option<&TemplatePreviewEntry>,
    preview_annotations: bool,
    transient: Option<&TransientStudioOverrides>,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    preview_studio_with_cache_and_locale(
        db,
        themes_root,
        actor,
        target,
        document,
        preview_entry,
        preview_annotations,
        transient,
        None,
        cache,
    )
    .await
}
