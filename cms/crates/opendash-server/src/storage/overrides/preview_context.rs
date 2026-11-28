use super::*;

pub(in crate::storage) enum PreviewPartSource {
    Builder(BuilderDocument),
    Theme(String),
    Missing,
}

pub(in crate::storage) struct PreviewRenderContext<'a> {
    pub(in crate::storage) state: &'a BuilderRenderContext,
    pub(in crate::storage) graph: &'a BTreeMap<String, PreviewPartSource>,
    pub(in crate::storage) theme_context: &'a serde_json::Value,
    pub(in crate::storage) route: Option<(&'a str, &'a str)>,
    pub(in crate::storage) render_locale: &'a str,
    pub(in crate::storage) cache: &'a page::BlockTemplateCache,
    pub(in crate::storage) url_policy: &'a template_sec::ThemeUrlPolicy,
    pub(in crate::storage) fmt_cfg: &'a template_sec::RenderFormattingConfig,
    pub(in crate::storage) search: Option<page::BuilderSearchData<'a>>,
    pub(in crate::storage) template_data: Option<&'a content_render::TemplateRenderData<'a>>,
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn build_entry_preview_context_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    record: &entries::ContentEntryRecord,
    effective_schema: &EffectiveContentSchema,
    actor_role: FieldRole,
    active: Option<&ActiveTheme>,
    site: &SiteContext<'_>,
    site_options: &BTreeMap<String, serde_json::Value>,
    options: &BTreeMap<String, serde_json::Value>,
    menus: &BTreeMap<String, menu::MenuViewModel>,
    route_kind: &str,
    mut richtext_nonces: Option<&mut template_sec::RichTextNonceTable>,
) -> ApiResult<BuiltEntryPreviewContext> {
    let definition = &effective_schema.base.descriptor;
    let eligible = public_content::load_eligible_public_taxonomies_in_conn(
        &mut **tx,
        &record.content_type_key,
    )
    .await?;
    let mut terms_map = public_content::batch_load_public_entry_terms_in_conn(
        &mut **tx,
        &[(record.id, record.content_type_key.clone())],
    )
    .await?;
    let terms = terms_map.remove(&record.id).unwrap_or_default();
    let view = public_content::hydrate_single_entry_view_in_conn(
        &mut **tx,
        record,
        effective_schema,
        None,
        &terms,
        &eligible,
        actor_role,
        true,
        richtext_nonces.as_deref_mut(),
    )
    .await?;
    let archive_url = permalinks::canonical_archive_path_in_tx(
        &mut **tx,
        &record.content_type_key,
        &record.locale,
    )
    .await?;
    let breadcrumbs = content_render::single_breadcrumbs(
        content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
        Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
        &record.content_type_key,
        &definition.label,
        definition.settings.has_archive,
        archive_url.clone(),
        view.primary_category.as_ref(),
        &view.title,
        view.url.clone(),
    );
    let ct_value = serde_json::to_value(definition)?;
    let custom_options = crate::storage::options_pages::load_active_custom_options_public_in_conn(
        &mut **tx,
        Some(&record.locale),
        true,
        richtext_nonces,
    )
    .await?;
    let theme_assets = active.map(ActiveTheme::asset_url_map).unwrap_or_default();
    let empty_entries = Vec::new();
    let pagination = PaginationView::empty();
    let search_url =
        crate::storage::seo::resolve_locale_search_url_in_conn(&mut **tx, Some(&record.locale))
            .await?;
    let search_view = content_render::SearchView::inactive(&search_url);
    let theme_context = content_render::build_public_theme_context(PublicThemeContextInput {
        site,
        site_options,
        options,
        custom_options: &custom_options,
        theme_assets: &theme_assets,
        menus,
        route_kind,
        route_path: None,
        breadcrumbs: &breadcrumbs,
        pagination: &pagination,
        archive_url: archive_url.as_deref(),
        previous_url: None,
        next_url: None,
        entries: &empty_entries,
        related_entries: &empty_entries,
        recent_posts: &empty_entries,
        taxonomy: None,
        term: None,
        content_type: Some(&ct_value),
        entry: Some(&view),
        search: &search_view,
    })?;
    Ok(BuiltEntryPreviewContext {
        view,
        archive_url,
        breadcrumbs,
        theme_context,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn preview_typed_override_with_cache_and_locale(
    db: &SqlitePool,
    themes_root: &Path,
    actor: &SessionContext,
    target: &TemplateCandidate,
    document: &BuilderDocument,
    preview_entry: Option<&TemplatePreviewEntry>,
    preview_term: Option<&TemplatePreviewTerm>,
    expected_version: Option<i64>,
    preview_locale: Option<&str>,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    let key = target.canonical_key().map_err(ApiError::bad_request)?;
    let mut tx = db.begin().await?;
    let effective_locale =
        settings::resolve_requested_locale_in_conn(&mut *tx, preview_locale).await?;
    let existing = get_typed_override_on_connection(&mut tx, &key).await?;
    if expected_version.is_some_and(|v| existing.as_ref().is_none_or(|r| r.version != v)) {
        return Err(ApiError::precondition_failed("Template override changed."));
    }
    let mut state = validate_typed_document_with_context_in_tx(
        &mut tx,
        target,
        document,
        existing.as_ref().map(|r| &r.document),
        true,
        Some(&effective_locale),
    )
    .await?;
    let active = theme::load_active_theme_in_conn(&mut tx, themes_root).await?;
    let mut budget = PreviewCompositionBudget::new();
    let url_policy =
        active_theme_url_policy_in_conn(&mut tx, active.as_ref(), Some(&key), None, None).await?;
    let graph = preview_part_graph(&mut tx, target, document, active.as_ref(), &mut budget).await?;
    let actor_role = actor_field_role(actor);
    let entry = resolve_preview_entry(
        &mut tx,
        target,
        preview_entry,
        active.as_ref(),
        &effective_locale,
        actor_role,
    )
    .await?;
    let resolved_term = resolve_preview_term_in_tx(
        &mut tx,
        target,
        preview_term,
        &effective_locale,
        Some(&mut budget.richtext_nonces),
    )
    .await?;
    let general =
        settings::get_general_in_conn_for_locale(&mut *tx, Some(&effective_locale)).await?;
    let pub_settings = settings::get_public_in_conn(&mut *tx).await?;
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
    let manifest_json = active
        .as_ref()
        .map(|a| serde_json::to_string(&a.manifest))
        .transpose()?;
    let active_ref = active
        .as_ref()
        .zip(manifest_json.as_deref())
        .map(|(a, m)| (a.manifest.theme_id.as_str(), m));
    let (site_options, options) =
        settings::resolve_merged_public_options_with_overrides_in_conn_with_nonces(
            &mut tx,
            active_ref,
            None,
            None,
            true,
            Some(&effective_locale),
            Some(&mut budget.richtext_nonces),
        )
        .await?;
    state.site_options = site_options.clone();
    state.options = options.clone();
    state.menus = menu::resolve_all_menus_for_locale_and_path_in_conn(
        &mut *tx,
        Some(&effective_locale),
        None,
    )
    .await?;
    let menus = if let Some(active) = &active {
        menu::resolve_slot_menus_for_locale_in_tx(
            &mut tx,
            &active.manifest.theme_id,
            &effective_locale,
        )
        .await?
    } else {
        BTreeMap::new()
    };
    let mut body_diagnostics = Vec::new();
    let mut route = None;
    let mut built_entry: Option<(content_render::EntryView, Vec<content_render::BreadcrumbItem>)> =
        None;
    let mut entry_has_builder_body = false;
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
            Some(&mut budget.richtext_nonces),
        )
        .await?;
        let built_view = built.view;
        let built_breadcrumbs = built.breadcrumbs;
        let definition = &effective_schema.base.descriptor;
        let raw_body_has_blocks = record
            .values
            .get("body")
            .and_then(serde_json::Value::as_str)
            .is_some_and(opendash_core::builder_blocks::contains_blocks);
        let builder_rendered = if raw_body_has_blocks
            && definition.settings.supports(ContentSupport::Builder)
            && record.content_type_key != "pattern"
        {
            let mut rb = budget.as_render_budget();
            let res = builder::preview_saved_page_body_in_tx_with_options_and_budget(
                &mut tx,
                actor,
                &record.content_type_key,
                record.id,
                Some(&site_options),
                Some(&options),
                Some(general.timezone.as_str()),
                &mut rb,
                cache,
            )
            .await?;
            budget.sync_from_render_budget(&rb);
            if let Some(mut rendered) = res {
                body_diagnostics.append(&mut rendered.diagnostics);
                Some(rendered.html)
            } else {
                None
            }
        } else {
            None
        };
        entry_has_builder_body = builder_rendered.is_some();
        let body = builder_rendered
            .map(Ok)
            .unwrap_or_else(|| {
                let html = content_render::render_single_with_context_for_locale(
                    definition,
                    &built_view,
                    None,
                    &site,
                    built.archive_url.as_deref(),
                    &built_breadcrumbs,
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
        built_entry = Some((built_view, built_breadcrumbs));
        (built.theme_context, body)
    } else {
        let custom_options =
            crate::storage::options_pages::load_active_custom_options_public_in_conn(
                &mut tx,
                Some(&effective_locale),
                true,
                Some(&mut budget.richtext_nonces),
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
    let preview_locale_ctx = crate::storage::seo::build_theme_locale_context_in_conn(
        &mut tx,
        Some(&effective_locale),
        crate::storage::seo::ThemeLocaleTarget::None,
    )
    .await?;
    content_render::sync_preview_locale_and_search(&mut theme_context, &preview_locale_ctx);
    let preview_search_view = (target.kind == TemplateCandidateKind::Search).then(|| {
        content_render::SearchView::preview_sample(&state.locale_search_url, &effective_locale)
    });
    let preview_search_entries = if target.kind == TemplateCandidateKind::Search {
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
    collect_db_docs_and_hydrate_state_in_tx(&mut tx, &mut state, &[document], true).await?;
    let surface = inspect_preview_surfaces_in_tx(
        &mut tx,
        target,
        Some(document),
        None,
        active.as_ref(),
        &url_policy,
        is_canvas_target(active.as_ref(), target),
    )
    .await?;
    let real_entry_ref = entry
        .as_ref()
        .zip(built_entry.as_ref())
        .map(|((rec, eff), (bv, bc))| (rec, eff, bv, bc.clone()));
    let owned_td = build_preview_template_data_in_tx(
        &mut tx,
        BuildPreviewTemplateDataInput {
            target,
            site_options: &site_options,
            menus_by_slot: menus.clone(),
            locale: &effective_locale,
            actor_role,
            surface: &surface,
            real_entry: real_entry_ref,
            entry_body_html: entry_has_builder_body.then(|| body.clone()),
            entry_has_builder_body,
            resolved_term: resolved_term.as_ref(),
        },
        Some(&mut budget.richtext_nonces),
    )
    .await?;
    let td = owned_td.as_render_data(&site);
    populate_template_entry_schemas_in_conn(&mut tx, &mut state, Some(&td)).await?;
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
        template_data: Some(&td),
    };
    let root_scope = if target.kind == TemplateCandidateKind::Part {
        part_scope_prefix(target.part_key.as_deref().unwrap_or("part"), 0)
    } else {
        "ov".to_owned()
    };
    let mut rendered = render_preview_builder(
        &mut tx,
        document,
        Some(&body),
        &ctx,
        &mut budget,
        RenderAnnotationScope::None,
        &root_scope,
    )
    .await?;
    rendered.diagnostics.append(&mut body_diagnostics);
    if !budget.hook_nonces.is_empty() {
        let clear: Vec<(String, String)> = budget
            .hook_nonces
            .entries()
            .iter()
            .map(|h| (h.nonce.clone(), String::new()))
            .collect();
        rendered.html = content_render::apply_render_hooks_to_document(
            rendered.html,
            "",
            &[],
            &[],
            "",
            &clear,
        )?;
    }
    rendered.html = content_render::finalize_document_icons(rendered.html, &budget.icon_nonces)?;
    rendered.html = content_render::finalize_document_images(rendered.html, &budget.image_nonces)?;
    rendered.html =
        content_render::finalize_document_richtext(rendered.html, &budget.richtext_nonces)?;
    tx.rollback().await?;
    Ok(rendered)
}

pub(in crate::storage) async fn build_preview_context_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    active: Option<&ActiveTheme>,
    transient: Option<&TransientStudioOverrides>,
    locale: &str,
    richtext_nonces: Option<&mut template_sec::RichTextNonceTable>,
) -> ApiResult<(
    settings::GeneralSettings,
    settings::PublicSiteSettings,
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, menu::MenuViewModel>,
)> {
    let mut general = settings::get_general_in_conn_for_locale(&mut **tx, Some(locale)).await?;
    let pub_settings = settings::get_public_in_conn(&mut **tx).await?;
    if let Some(g) = transient.and_then(|t| t.general.as_ref()) {
        let valid = settings::GeneralSettingsInput {
            site_title: g.site_title.clone(),
            tagline: g.tagline.clone(),
            timezone: g.timezone.clone(),
            // CMS language is not a public preview override.
            cms_locale: None,
        }
        .validate()
        .map_err(ApiError::unprocessable)?;
        if valid.timezone != general.timezone && !settings::is_current_timezone(&valid.timezone) {
            return Err(ApiError::unprocessable(
                "Múi giờ không có trong danh mục IANA tzdb 2026f.",
            ));
        }
        general.site_title = valid.site_title;
        general.tagline = valid.tagline;
        general.timezone = valid.timezone;
    }

    let manifest_json = active
        .as_ref()
        .map(|a| serde_json::to_string(&a.manifest))
        .transpose()?;
    let active_ref = active
        .as_ref()
        .zip(manifest_json.as_deref())
        .map(|(a, m)| (a.manifest.theme_id.as_str(), m));
    let (site_options, options) =
        settings::resolve_merged_public_options_with_overrides_in_conn_with_nonces(
            &mut **tx,
            active_ref,
            transient.and_then(|t| t.global_options.as_ref()),
            transient.and_then(|t| t.theme_options.as_ref()),
            true,
            Some(locale),
            richtext_nonces,
        )
        .await?;

    let mut menus = BTreeMap::new();
    if let Some(active) = active {
        let manifest_json = manifest_json.as_deref().unwrap_or("{}");
        let mut assignments =
            menu::list_slot_assignments_for_locale_in_tx(tx, &active.manifest.theme_id, locale)
                .await?;
        if let Some(slot_ov) = transient.and_then(|t| t.menu_slots.as_ref()) {
            let declared = menu::extract_declared_menu_slots(manifest_json);
            for (slot_key, maybe_id) in slot_ov {
                if !declared.iter().any(|s| s == slot_key) {
                    return Err(ApiError::unprocessable(
                        "Vị trí menu không được khai báo trong theme.",
                    ));
                }
                match maybe_id {
                    None => {
                        assignments.remove(slot_key);
                    }
                    Some(id) => {
                        if *id <= 0 || menu::get_menu_in_tx(tx, *id).await?.is_none() {
                            return Err(ApiError::unprocessable("Menu không tồn tại."));
                        }
                        assignments.insert(slot_key.clone(), *id);
                    }
                }
            }
        }
        for (slot_key, menu_id) in assignments {
            if let Some(view) = menu::resolve_menu_view_for_path_in_tx(tx, menu_id, None).await? {
                menus.insert(slot_key, view);
            }
        }
    } else if transient
        .and_then(|t| t.menu_slots.as_ref())
        .is_some_and(|m| !m.is_empty())
    {
        return Err(ApiError::unprocessable(
            "Không có theme v1 đang kích hoạt để xem trước menu_slots.",
        ));
    }
    Ok((general, pub_settings, site_options, options, menus))
}
