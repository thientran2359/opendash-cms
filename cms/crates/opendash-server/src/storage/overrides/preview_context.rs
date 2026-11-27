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
}

pub(in crate::storage) fn build_placeholder_preview_entry(type_key: &str) -> EntryView {
    let mut terms = BTreeMap::new();
    terms.insert("category".to_owned(), Vec::new());
    terms.insert("tag".to_owned(), Vec::new());
    EntryView {
        id: 0,
        type_key: type_key.to_owned(),
        title: "Nội dung xem trước".to_owned(),
        slug: "preview".to_owned(),
        url: Some("/".to_owned()),
        created_at: String::new(),
        updated_at: String::new(),
        published_at: None,
        excerpt: String::new(),
        reading_time_minutes: 0,
        fields: BTreeMap::new(),
        field_list: Vec::new(),
        cover_url: None,
        cover_media: None,
        terms,
        all_terms: Vec::new(),
        primary_category: None,
        ext: BTreeMap::new(),
        theme_ext: BTreeMap::new(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn build_entry_preview_context_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    record: &entries::ContentEntryRecord,
    effective_schema: &EffectiveContentSchema,
    actor_role: FieldRole,
    active: Option<&ActiveThemeV3>,
    site: &SiteContext<'_>,
    site_options: &BTreeMap<String, serde_json::Value>,
    options: &BTreeMap<String, serde_json::Value>,
    menus: &BTreeMap<String, menu::MenuViewModel>,
    route_kind: &str,
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
    let media_ids = content_render::collect_entry_media_ids(&[record], effective_schema);
    let media_map = if media_ids.is_empty() {
        BTreeMap::new()
    } else {
        crate::storage::fields::load_media_hydration_map_in_conn(&mut **tx, &media_ids).await?
    };
    let view = content_render::authenticated_preview_view_with_context_and_media(
        record,
        effective_schema,
        None,
        &terms,
        &eligible,
        actor_role,
        &media_map,
    );
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
    let theme_assets = active.map(ActiveThemeV3::asset_url_map).unwrap_or_default();
    let empty_entries = Vec::new();
    let pagination = PaginationView::empty();
    let theme_context = content_render::build_public_theme_context(PublicThemeContextInput {
        site,
        site_options,
        options,
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
        content_type: Some(&ct_value),
        entry: Some(&view),
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
    let mut state = validate_typed_document_in_tx(
        &mut tx,
        target,
        document,
        existing.as_ref().map(|r| &r.document),
    )
    .await?;
    let active = theme::load_active_theme_v3_in_conn(&mut tx, themes_root).await?;
    let url_policy = active_theme_url_policy(active.as_ref())?;
    let mut budget = PreviewCompositionBudget::new();
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
    let (theme_context, body) = if let Some((record, effective_schema)) = &entry {
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
        let theme_context = build_placeholder_preview_theme_context(
            target,
            active.as_ref(),
            &site,
            &site_options,
            &options,
            &menus,
        )?;
        (
            theme_context,
            "<div data-template-preview-placeholder>Preview content</div>".to_owned(),
        )
    };
    if body.len() > template_sec::MAX_RENDERED_HTML_BYTES {
        return Err(ApiError::bad_request("Preview output exceeds 512 KiB."));
    }
    collect_db_docs_and_hydrate_state_in_tx(&mut tx, &mut state, &[document], true).await?;
    let ctx = PreviewRenderContext {
        state: &state,
        graph: &graph,
        theme_context: &theme_context,
        route,
        render_locale: &effective_locale,
        cache,
        url_policy: &url_policy,
        fmt_cfg: &fmt_cfg,
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
    active: Option<&ActiveThemeV3>,
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
            "Không có theme v3 đang kích hoạt để xem trước menu_slots.",
        ));
    }
    Ok((general, pub_settings, site_options, options, menus))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn render_preview_named_part_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    key: &str,
    state: &BuilderRenderContext,
    active: Option<&ActiveThemeV3>,
    theme_context: &serde_json::Value,
    route: Option<(&str, &str)>,
    render_locale: &str,
    budget: &mut PreviewCompositionBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<Option<RenderedLayout>> {
    let url_policy = active_theme_url_policy(active)?;
    let fmt_cfg = template_sec::RenderFormattingConfig {
        timezone: state.timezone.clone(),
        ..Default::default()
    };
    let part_target = TemplateCandidate {
        part_key: Some(key.to_owned()),
        ..candidate(TemplateCandidateKind::Part)
    };
    let canonical = part_target.canonical_key().map_err(ApiError::bad_request)?;
    if let Some(row) = get_typed_override_on_connection(&mut **tx, &canonical).await? {
        validate_typed_document_in_tx(tx, &part_target, &row.document, Some(&row.document)).await?;
        let graph = preview_part_graph(tx, &part_target, &row.document, active, budget).await?;
        let ctx = PreviewRenderContext {
            state,
            graph: &graph,
            theme_context,
            route,
            render_locale,
            cache,
            url_policy: &url_policy,
            fmt_cfg: &fmt_cfg,
        };
        let scope = part_scope_prefix(key, budget.expansions);
        let rendered = render_preview_builder(
            tx,
            &row.document,
            None,
            &ctx,
            budget,
            RenderAnnotationScope::None,
            &scope,
        )
        .await?;
        return Ok(Some(rendered));
    }
    if let Some(source) = active.and_then(|a| a.parts.get(key)) {
        let edges = template_sec::preview_csec_part_keys_with_theme_urls(key, source, &url_policy)
            .map_err(ApiError::bad_request)?;
        let mut graph =
            preview_part_graph_from_edges(tx, &canonical, edges, active, budget).await?;
        graph.insert(key.to_owned(), PreviewPartSource::Theme(source.clone()));
        let ctx = PreviewRenderContext {
            state,
            graph: &graph,
            theme_context,
            route,
            render_locale,
            cache,
            url_policy: &url_policy,
            fmt_cfg: &fmt_cfg,
        };
        return render_preview_part(tx, key, &ctx, budget).await;
    }
    Ok(None)
}
