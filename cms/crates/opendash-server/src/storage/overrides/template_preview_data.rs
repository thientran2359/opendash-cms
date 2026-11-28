//! Studio Mode 1 & Mode 2 preview `TemplateRenderData` assembly and schema hydration.
//! Avoids duplicate preview queries and keeps Builder CSS / C-SEC isolation intact.

use super::*;
use crate::content_render::{
    AdjacentEntriesView, ArchiveTypeSummary, BreadcrumbItem, EntryView, HydratedMedia,
    LocaleRenderContext, PaginationView, PublicTermDetailView, SiteContext, TemplateRenderData,
    TermRenderSummary, core_l10n, render_entry_body_fragment,
};
use crate::storage::seo;

pub(in crate::storage) async fn populate_template_entry_schemas_in_conn(
    conn: &mut SqliteConnection,
    state: &mut builder::BuilderRenderContext,
    template_data: Option<&TemplateRenderData<'_>>,
) -> ApiResult<()> {
    let Some(td) = template_data else {
        return Ok(());
    };
    if let (Some(entry), Some(schema)) = (td.entry, td.entry_schema) {
        state
            .content_schemas
            .entry(entry.type_key.clone())
            .or_insert_with(|| schema.clone());
    }
    let mut missing_keys = BTreeSet::new();
    for entry in td
        .entry
        .into_iter()
        .chain(td.archive_entries.iter())
        .chain(td.related_entries.iter())
    {
        if !state.content_schemas.contains_key(&entry.type_key) {
            missing_keys.insert(entry.type_key.clone());
        }
    }
    for type_key in missing_keys {
        if let Some(schema) =
            crate::storage::content::load_effective_content_schema_in_tx(conn, &type_key).await?
        {
            state.content_schemas.insert(type_key, schema);
        }
    }
    Ok(())
}

#[derive(Clone)]
pub(in crate::storage) struct OwnedPreviewTemplateData {
    pub site_logo: Option<HydratedMedia>,
    pub locale: LocaleRenderContext,
    pub menus_by_slot: BTreeMap<String, menu::MenuViewModel>,
    pub menus_by_key: BTreeMap<String, menu::MenuViewModel>,
    pub breadcrumbs: Vec<BreadcrumbItem>,
    pub pagination: Option<PaginationView>,
    pub entry: Option<EntryView>,
    pub entry_schema: Option<EffectiveContentSchema>,
    pub entry_body_html: Option<String>,
    pub entry_has_builder_body: bool,
    pub adjacent_entries_global: Option<AdjacentEntriesView>,
    pub adjacent_entries_same_taxonomy: Option<AdjacentEntriesView>,
    pub related_entries: Vec<EntryView>,
    pub archive_entries: Vec<EntryView>,
    pub content_type: Option<ArchiveTypeSummary>,
    pub term: Option<TermRenderSummary>,
}

impl OwnedPreviewTemplateData {
    pub fn as_render_data<'a>(&'a self, site: &'a SiteContext<'a>) -> TemplateRenderData<'a> {
        TemplateRenderData {
            site,
            site_logo: self.site_logo.as_ref(),
            locale: &self.locale,
            menus_by_slot: &self.menus_by_slot,
            menus_by_key: &self.menus_by_key,
            breadcrumbs: &self.breadcrumbs,
            pagination: self.pagination.as_ref(),
            entry: self.entry.as_ref(),
            entry_schema: self.entry_schema.as_ref(),
            entry_body_html: self.entry_body_html.as_deref(),
            entry_has_builder_body: self.entry_has_builder_body,
            adjacent_entries_global: self.adjacent_entries_global.as_ref(),
            adjacent_entries_same_taxonomy: self.adjacent_entries_same_taxonomy.as_ref(),
            related_entries: &self.related_entries,
            archive_entries: &self.archive_entries,
            content_type: self.content_type.as_ref(),
            term: self.term.as_ref(),
            is_preview: true,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn inspect_preview_surfaces_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    target: &TemplateCandidate,
    draft_doc: Option<&BuilderDocument>,
    resolution: Option<&TemplateResolution>,
    active: Option<&ActiveTheme>,
    url_policy: &template_sec::ThemeUrlPolicy,
    is_canvas: bool,
) -> ApiResult<SurfaceScanAcc> {
    let schemas = crate::storage::page::load_unified_block_catalog_in_conn(&mut **tx).await?;
    let mut acc = SurfaceScanAcc::default();
    let root_key = target.canonical_key().map_err(ApiError::bad_request)?;
    let stack = vec![root_key.clone()];

    if let Some(doc) = draft_doc {
        inspect_db_document_surfaces_in_tx(
            tx, doc, 0, &stack, active, url_policy, &schemas, &mut acc,
        )
        .await?;
    } else if let Some(res) = resolution {
        match res {
            TemplateResolution::DbOverride(record) => {
                inspect_db_document_surfaces_in_tx(
                    tx,
                    &record.document,
                    0,
                    &stack,
                    active,
                    url_policy,
                    &schemas,
                    &mut acc,
                )
                .await?;
            }
            TemplateResolution::ActiveTheme { target_key, html } => {
                let empty_parts = BTreeMap::new();
                let parts = active.map(|v| &v.parts).unwrap_or(&empty_parts);
                let kind = super::csec_content_kind_for_target_key(target_key);
                let compiled = if let Some(act) = active {
                    template_sec::ThemeCompositionCache::global().get_or_compile(
                        &act.revision_digest,
                        target_key,
                        html,
                        parts,
                        kind,
                        url_policy,
                    )
                } else {
                    template_sec::validate_and_compile_csec_composition_with_theme_urls(
                        target_key, html, parts, kind, url_policy,
                    )
                    .map(Arc::new)
                }
                .map_err(ApiError::internal)?;
                acc.merge_theme_compiled(&compiled);
            }
            TemplateResolution::CoreFallback => {
                acc.loop_projection = opendash_core::builder::LoopProjection::all();
            }
        }
    } else {
        acc.loop_projection = opendash_core::builder::LoopProjection::all();
    }

    if target.kind != TemplateCandidateKind::Part && !is_canvas {
        inspect_part_slot_in_tx(
            tx, "header", 1, &stack, active, url_policy, &schemas, &mut acc,
        )
        .await?;
        inspect_part_slot_in_tx(
            tx, "footer", 1, &stack, active, url_policy, &schemas, &mut acc,
        )
        .await?;
    }

    Ok(acc)
}

pub(in crate::storage) struct BuildPreviewTemplateDataInput<'a> {
    pub target: &'a TemplateCandidate,
    pub site_options: &'a BTreeMap<String, serde_json::Value>,
    pub menus_by_slot: BTreeMap<String, menu::MenuViewModel>,
    pub locale: &'a str,
    pub actor_role: FieldRole,
    pub surface: &'a SurfaceScanAcc,
    pub real_entry: Option<(
        &'a entries::ContentEntryRecord,
        &'a EffectiveContentSchema,
        &'a EntryView,
        Vec<BreadcrumbItem>,
    )>,
    pub entry_body_html: Option<String>,
    pub entry_has_builder_body: bool,
    pub resolved_term: Option<&'a (PublicTermDetailView, String)>,
}

pub(in crate::storage) async fn build_preview_template_data_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    input: BuildPreviewTemplateDataInput<'_>,
    mut richtext_nonces: Option<&mut template_sec::RichTextNonceTable>,
) -> ApiResult<OwnedPreviewTemplateData> {
    use TemplateCandidateKind as K;

    let site_logo = input
        .site_options
        .get("site_logo")
        .and_then(HydratedMedia::from_value);
    let seo_target = match input.target.kind {
        K::Index => seo::ThemeLocaleTarget::Home,
        K::Archive => input
            .target
            .content_type
            .as_deref()
            .map_or(seo::ThemeLocaleTarget::None, |tk| {
                seo::ThemeLocaleTarget::Archive { type_key: tk }
            }),
        K::Taxonomy => input
            .resolved_term
            .map_or(seo::ThemeLocaleTarget::None, |(t, _)| {
                seo::ThemeLocaleTarget::Term { term_id: t.id }
            }),
        K::Search => seo::ThemeLocaleTarget::Search {
            raw_search_query: None,
        },
        K::Single | K::SingleCustom | K::Singular | K::Page | K::PageLayout => input
            .real_entry
            .as_ref()
            .map_or(seo::ThemeLocaleTarget::None, |(r, _, _, _)| {
                seo::ThemeLocaleTarget::Entry { entry_id: r.id }
            }),
        K::NotFound | K::Part => seo::ThemeLocaleTarget::None,
    };
    let locale_ctx =
        seo::build_locale_render_context_in_conn(&mut **tx, Some(input.locale), seo_target).await?;

    let menus_by_key = if input.surface.requested_menu_keys.is_empty() {
        BTreeMap::new()
    } else {
        let keys: Vec<String> = input.surface.requested_menu_keys.iter().cloned().collect();
        menu::resolve_menus_by_keys_for_path_in_tx(
            &mut **tx,
            &keys,
            None,
            Some(input.locale),
            input.menus_by_slot.len(),
        )
        .await?
    };

    let l10n = core_l10n(input.locale);
    let is_vi = input.locale.trim().to_ascii_lowercase().starts_with("vi")
        || input.locale.trim().is_empty();

    let mut entry: Option<EntryView> = None;
    let mut entry_schema: Option<EffectiveContentSchema> = None;
    let mut entry_body_html: Option<String> = None;
    let mut entry_has_builder_body = false;
    let mut breadcrumbs = Vec::new();
    let mut pagination = None;
    let mut adjacent_entries_global = None;
    let mut adjacent_entries_same_taxonomy = None;
    let mut related_entries = Vec::new();
    let mut archive_entries = Vec::new();
    let mut content_type: Option<ArchiveTypeSummary> = None;
    let term = input
        .resolved_term
        .map(|(detail, tax_label)| TermRenderSummary::from_detail(detail, tax_label.clone()));

    if let Some((record, schema_eff, view, entry_bc)) = input.real_entry {
        entry = Some(view.clone());
        entry_schema = Some(schema_eff.clone());
        breadcrumbs = entry_bc;
        let def = &schema_eff.base.descriptor;
        content_type = Some(ArchiveTypeSummary {
            key: def.key.clone(),
            label: def.label.clone(),
            description: def.description.clone(),
        });
        let has_builder = input.entry_has_builder_body
            || record
                .values
                .get("body")
                .and_then(serde_json::Value::as_str)
                .is_some_and(opendash_core::builder_blocks::contains_blocks);
        entry_has_builder_body = has_builder;
        entry_body_html = if has_builder {
            input.entry_body_html
        } else {
            render_entry_body_fragment(&record.values, schema_eff)
        };
        if input.surface.uses_related_entries {
            let limit = input.surface.related_limit.max(4);
            let mut rel = public_content::query_related_entries_in_tx(
                tx,
                record.id,
                &record.content_type_key,
                limit,
                true,
                &input.surface.loop_projection,
            )
            .await?;
            if rel.is_empty() {
                rel = build_preview_related_stubs(limit, input.locale);
            }
            related_entries = rel;
        }
        if input.surface.uses_adjacent_global {
            let mut adj = public_content::query_adjacent_entries_in_tx(
                tx,
                record.id,
                &record.content_type_key,
                input.locale,
                false,
                true,
            )
            .await?;
            if adj.prev.is_none() && adj.next.is_none() {
                adj = build_preview_adjacent_stub(input.locale);
            }
            adjacent_entries_global = Some(adj);
        }
        if input.surface.uses_adjacent_same_taxonomy {
            let mut adj = public_content::query_adjacent_entries_in_tx(
                tx,
                record.id,
                &record.content_type_key,
                input.locale,
                true,
                true,
            )
            .await?;
            if adj.prev.is_none() && adj.next.is_none() {
                adj = build_preview_adjacent_stub(input.locale);
            }
            adjacent_entries_same_taxonomy = Some(adj);
        }
    } else if matches!(
        input.target.kind,
        K::Single | K::SingleCustom | K::Singular | K::Page | K::PageLayout
    ) {
        let ct_key = input.target.content_type.as_deref().unwrap_or(
            if matches!(input.target.kind, K::Page | K::PageLayout) {
                "page"
            } else {
                "post"
            },
        );
        let loaded_schema =
            crate::storage::content::load_effective_content_schema_in_tx(&mut **tx, ct_key).await?;
        let placeholder = build_placeholder_preview_entry(ct_key);
        breadcrumbs = vec![
            BreadcrumbItem {
                name: l10n.home_label.to_string(),
                url: Some("/".to_owned()),
                is_current: false,
            },
            BreadcrumbItem {
                name: placeholder.title.clone(),
                url: None,
                is_current: true,
            },
        ];
        if let Some(ref eff) = loaded_schema {
            content_type = Some(ArchiveTypeSummary {
                key: eff.base.descriptor.key.clone(),
                label: eff.base.descriptor.label.clone(),
                description: eff.base.descriptor.description.clone(),
            });
        }
        entry = Some(placeholder);
        entry_schema = loaded_schema;
        entry_body_html = Some(if is_vi {
            "<div data-template-preview-placeholder><p>Nội dung bài viết mẫu dùng để xem trước bố cục trong Studio.</p></div>".to_owned()
        } else {
            "<div data-template-preview-placeholder><p>Sample entry body content rendered for Studio layout preview.</p></div>".to_owned()
        });
        if input.surface.uses_related_entries {
            related_entries =
                build_preview_related_stubs(input.surface.related_limit.max(3), input.locale);
        }
        if input.surface.uses_adjacent_global {
            adjacent_entries_global = Some(build_preview_adjacent_stub(input.locale));
        }
        if input.surface.uses_adjacent_same_taxonomy {
            adjacent_entries_same_taxonomy = Some(build_preview_adjacent_stub(input.locale));
        }
    }

    if matches!(
        input.target.kind,
        K::Archive | K::Taxonomy | K::Index | K::Search
    ) {
        let sample = load_preview_archive_samples_in_tx(
            tx,
            input.target,
            input.resolved_term.map(|(t, _)| t.id),
            input.locale,
            input.actor_role,
            &input.surface.loop_projection,
            richtext_nonces.as_deref_mut(),
        )
        .await?;
        archive_entries = sample.entries;
        pagination = Some(PaginationView::compute(
            1,
            12,
            sample.total,
            None,
            (sample.total > 12).then(|| "?page=2".to_owned()),
            |p| Some(format!("?page={p}")),
        ));
        match input.target.kind {
            K::Archive | K::Index => {
                let ct_key = input.target.content_type.as_deref().unwrap_or("post");
                if let Some(eff) =
                    crate::storage::content::load_effective_content_schema_in_tx(&mut **tx, ct_key)
                        .await?
                {
                    let label = eff.base.descriptor.label.clone();
                    content_type = Some(ArchiveTypeSummary {
                        key: eff.base.descriptor.key.clone(),
                        label: label.clone(),
                        description: eff.base.descriptor.description,
                    });
                    breadcrumbs = if input.target.kind == K::Index {
                        vec![BreadcrumbItem {
                            name: l10n.home_label.to_string(),
                            url: Some("/".to_owned()),
                            is_current: true,
                        }]
                    } else {
                        vec![
                            BreadcrumbItem {
                                name: l10n.home_label.to_string(),
                                url: Some("/".to_owned()),
                                is_current: false,
                            },
                            BreadcrumbItem {
                                name: label,
                                url: None,
                                is_current: true,
                            },
                        ]
                    };
                }
            }
            K::Taxonomy => {
                if let Some((detail, tax_label)) = input.resolved_term {
                    content_type = Some(ArchiveTypeSummary {
                        key: detail.kind.clone(),
                        label: tax_label.clone(),
                        description: None,
                    });
                    breadcrumbs = vec![
                        BreadcrumbItem {
                            name: l10n.home_label.to_string(),
                            url: Some("/".to_owned()),
                            is_current: false,
                        },
                        BreadcrumbItem {
                            name: tax_label.clone(),
                            url: None,
                            is_current: false,
                        },
                        BreadcrumbItem {
                            name: detail.name.clone(),
                            url: None,
                            is_current: true,
                        },
                    ];
                }
            }
            K::Search => {
                breadcrumbs = vec![
                    BreadcrumbItem {
                        name: l10n.home_label.to_string(),
                        url: Some("/".to_owned()),
                        is_current: false,
                    },
                    BreadcrumbItem {
                        name: l10n.search_title.to_string(),
                        url: None,
                        is_current: true,
                    },
                ];
            }
            _ => {}
        }
    } else if input.target.kind == K::NotFound {
        breadcrumbs = vec![
            BreadcrumbItem {
                name: l10n.home_label.to_string(),
                url: Some("/".to_owned()),
                is_current: false,
            },
            BreadcrumbItem {
                name: l10n.not_found_title.to_string(),
                url: None,
                is_current: true,
            },
        ];
    } else if breadcrumbs.is_empty() {
        breadcrumbs = vec![BreadcrumbItem {
            name: l10n.home_label.to_string(),
            url: Some("/".to_owned()),
            is_current: true,
        }];
    }

    Ok(OwnedPreviewTemplateData {
        site_logo,
        locale: locale_ctx,
        menus_by_slot: input.menus_by_slot,
        menus_by_key,
        breadcrumbs,
        pagination,
        entry,
        entry_schema,
        entry_body_html,
        entry_has_builder_body,
        adjacent_entries_global,
        adjacent_entries_same_taxonomy,
        related_entries,
        archive_entries,
        content_type,
        term,
    })
}
