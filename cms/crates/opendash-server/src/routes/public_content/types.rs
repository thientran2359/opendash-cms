//! Public CPT archive/single routes and referenced-only media delivery.
//! Legacy CPT Save stays live; core post/page draft data is never served here.

use super::*;

pub(super) use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(super) use axum::{
    Extension,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};

pub(super) use opendash_core::content::{ContentTypeDefinition, FieldRole};

pub(super) use opendash_core::theme::{
    EnqueueCondition, ResolvedEnqueuedAssets, resolve_enqueued_assets,
};

pub(super) use sqlx::SqlitePool;

pub(super) use crate::{
    content_render::{self, PaginationView, PublicThemeContextInput, SiteContext},
    error::{ApiError, ApiResult},
    media::MediaStore,
    public_context::{
        ARCHIVE_SIZE, CanonicalUrlBuilder, MAX_ARCHIVE_PAGE, PublicQuery, PublicRequestContext,
        QueryScope, TaxonomyKind, parse_query,
    },
    routes::themes::ThemeStoreRoot,
    storage::{
        content as content_storage,
        entries::EntryFilters,
        media,
        overrides::{self, PublicTemplateContext, TemplateResolution},
        permalinks, public_content,
        seo::{self, ReadingPageState},
        settings, theme as theme_storage,
    },
};

#[derive(Clone)]
pub(crate) struct PublicRenderServices {
    pub hooks: Arc<crate::hooks::HookDispatcher>,
    pub render_cache: Arc<crate::cache::ExtensionRenderCache>,
    pub origin_host: String,
}

pub(super) struct PublicRouteContext<'a> {
    pub(super) route_kind: &'a str,
    pub(super) route_path: Option<&'a str>,
    pub(super) entry_id: Option<i64>,
    pub(super) entry_type_key: Option<&'a str>,
    pub(super) breadcrumbs: &'a [content_render::BreadcrumbItem],
    pub(super) pagination: &'a content_render::PaginationView,
    pub(super) archive_url: Option<&'a str>,
    pub(super) previous_url: Option<&'a str>,
    pub(super) next_url: Option<&'a str>,
    pub(super) entries: &'a [content_render::EntryView],
    pub(super) taxonomy: Option<&'a serde_json::Value>,
    pub(super) content_type: Option<&'a serde_json::Value>,
    pub(super) entry: Option<&'a content_render::EntryView>,
}

pub struct ResolvedPublicHtml {
    pub html: String,
    pub has_active_contact_form: bool,
    /// Page Layouts own their canvas; wrapping them in the site's default
    /// header/footer would violate the layout choice.
    pub canvas_layout: bool,
    pub theme_context: serde_json::Value,
    pub precompiled_shell: overrides::PrecompiledPublicShell,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_resolved_public_template_html(
    db: &SqlitePool,
    resolution: TemplateResolution,
    active: Option<&theme_storage::ActiveThemeV3>,
    site_ctx: &SiteContext<'_>,
    site_opts: &BTreeMap<String, serde_json::Value>,
    theme_opts: &BTreeMap<String, serde_json::Value>,
    locale: Option<&str>,
    fallback_html: &str,
    route_entry: Option<(&str, &str)>,
    route_ctx: PublicRouteContext<'_>,
    budget: &mut overrides::PublicCompositionBudget,
    cache: &crate::page::BlockTemplateCache,
) -> ApiResult<ResolvedPublicHtml> {
    let url_policy = overrides::active_theme_url_policy(active)?;
    let inspection =
        overrides::inspect_public_surfaces_and_precompile(db, &resolution, active, &url_policy)
            .await?;

    if !inspection.has_shell_or_theme && matches!(&resolution, TemplateResolution::CoreFallback) {
        return Ok(ResolvedPublicHtml {
            html: fallback_html.to_owned(),
            has_active_contact_form: false,
            canvas_layout: false,
            theme_context: serde_json::Value::Null,
            precompiled_shell: overrides::PrecompiledPublicShell::default(),
        });
    }

    let menus =
        crate::storage::menu::resolve_active_slot_menus_for_path(db, route_ctx.route_path, locale)
            .await?;
    let related_entries = if inspection.uses_related_entries
        && let (Some(eid), Some(tkey)) = (route_ctx.entry_id, route_ctx.entry_type_key)
    {
        public_content::query_related_entries(db, eid, tkey, 6).await?
    } else {
        Vec::new()
    };
    let exclude_post_id = (route_ctx.entry_type_key == Some("post"))
        .then_some(route_ctx.entry_id)
        .flatten();
    let recent_posts = if inspection.uses_recent_posts {
        public_content::query_recent_posts(db, exclude_post_id, locale, 6).await?
    } else {
        Vec::new()
    };
    let theme_assets = active
        .map(theme_storage::ActiveThemeV3::asset_url_map)
        .unwrap_or_default();
    let context = content_render::build_public_theme_context(PublicThemeContextInput {
        site: site_ctx,
        site_options: site_opts,
        options: theme_opts,
        theme_assets: &theme_assets,
        menus: &menus,
        route_kind: route_ctx.route_kind,
        route_path: route_ctx.route_path,
        breadcrumbs: route_ctx.breadcrumbs,
        pagination: route_ctx.pagination,
        archive_url: route_ctx.archive_url,
        previous_url: route_ctx.previous_url,
        next_url: route_ctx.next_url,
        entries: route_ctx.entries,
        related_entries: &related_entries,
        recent_posts: &recent_posts,
        taxonomy: route_ctx.taxonomy,
        content_type: route_ctx.content_type,
        entry: route_ctx.entry,
    })?;
    let fmt_cfg = crate::template_sec::RenderFormattingConfig {
        timezone: site_ctx.timezone.to_owned(),
    };

    // A fallback renderer can return a complete HTML document. Only its
    // normalized body fragment and authorized styles may enter a typed slot;
    // injecting a second document envelope would invalidate the whole page.
    let slot_html = if matches!(&resolution, TemplateResolution::CoreFallback) {
        String::new()
    } else {
        match crate::document::normalize_public_document_with_options(fallback_html, true) {
            Ok(parts) => {
                let mut fragment = parts.authorized_styles.concat();
                fragment.push_str(&parts.body_fragment);
                fragment
            }
            Err(_) => {
                // Keep the existing outer normalizer's safe diagnostic
                // fallback rather than injecting malformed content.
                return Ok(ResolvedPublicHtml {
                    html: fallback_html.to_owned(),
                    has_active_contact_form: false,
                    canvas_layout: false,
                    theme_context: context,
                    precompiled_shell: inspection.precompiled_shell,
                });
            }
        }
    };
    match resolution {
        TemplateResolution::CoreFallback => Ok(ResolvedPublicHtml {
            html: fallback_html.to_owned(),
            has_active_contact_form: false,
            canvas_layout: false,
            theme_context: context,
            precompiled_shell: inspection.precompiled_shell,
        }),
        TemplateResolution::DbOverride(record) => {
            let canvas_layout = record.target.kind
                == opendash_core::theme::TemplateCandidateKind::PageLayout
                && overrides::is_canvas_page_layout(active, record.target.layout_key.as_deref());
            let rendered = overrides::render_resolved_public_builder_with_cache_for_locale(
                db,
                &record,
                active,
                &context,
                route_entry,
                locale,
                Some(&slot_html),
                budget,
                cache,
            )
            .await?;
            Ok(ResolvedPublicHtml {
                html: rendered.html,
                has_active_contact_form: rendered.has_active_contact_form,
                canvas_layout,
                theme_context: context,
                precompiled_shell: inspection.precompiled_shell,
            })
        }
        TemplateResolution::ActiveTheme { target_key, html } => {
            let compiled = match inspection.precompiled_body {
                Some(compiled) => compiled,
                None => {
                    use crate::template_sec::CsecContentKind;
                    let kind = if target_key == "index" {
                        CsecContentKind::Index
                    } else if matches!(target_key.as_str(), "home" | "page" | "singular" | "single")
                        || target_key.starts_with("single:")
                        || target_key.starts_with("single_custom:")
                        || target_key.starts_with("page_layout:")
                    {
                        CsecContentKind::Entry
                    } else {
                        CsecContentKind::NonEntry
                    };
                    let empty_parts = BTreeMap::new();
                    let parts = active.map(|v| &v.parts).unwrap_or(&empty_parts);
                    if let Some(active) = active {
                        crate::template_sec::ThemeCompositionCache::global().get_or_compile(
                            &active.revision_digest,
                            &target_key,
                            &html,
                            parts,
                            kind,
                            &url_policy,
                        )
                    } else {
                        crate::template_sec::validate_and_compile_csec_composition_with_theme_urls(
                            &target_key,
                            &html,
                            parts,
                            kind,
                            &url_policy,
                        )
                        .map(Arc::new)
                    }
                    .map_err(ApiError::internal)?
                }
            };
            let html =
                budget.render_theme_with_config(&compiled, &context, Some(&slot_html), &fmt_cfg)?;
            Ok(ResolvedPublicHtml {
                html,
                has_active_contact_form: false,
                canvas_layout: overrides::is_canvas_page_layout(
                    active,
                    target_key.strip_prefix("page_layout:"),
                ),
                theme_context: context,
                precompiled_shell: inspection.precompiled_shell,
            })
        }
    }
}

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
    let (confirmed_kind, term, page) = public_content::taxonomy_public_page_by_id(
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
    let entry_urls =
        crate::storage::seo::resolve_public_entry_paths_by_ids(db, &page.entries).await?;
    let pairs: Vec<(i64, String)> = page
        .entries
        .iter()
        .map(|r| (r.id, r.content_type_key.clone()))
        .collect();
    let distinct_types: Vec<String> = page
        .entries
        .iter()
        .map(|r| r.content_type_key.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut terms_map = public_content::batch_load_public_entry_terms(db, &pairs).await?;
    let eligible_map =
        public_content::batch_load_eligible_public_taxonomies(db, &distinct_types).await?;
    let mut schemas = BTreeMap::new();
    for tk in &distinct_types {
        let eff = content_storage::load_effective_content_schema(db, tk)
            .await?
            .ok_or_else(|| ApiError::internal("Không tìm thấy schema nội dung."))?;
        schemas.insert(tk.clone(), eff);
    }
    let tax_entry_ids: Vec<i64> = page.entries.iter().map(|r| r.id).collect();
    let mut proj_map =
        crate::storage::plugin_projections::batch_load_entry_projections(db, &tax_entry_ids)
            .await?;
    let media_ids = content_render::collect_multi_type_entry_media_ids(&page.entries, &schemas);
    let media_map = if media_ids.is_empty() {
        BTreeMap::new()
    } else {
        let mut conn = db.acquire().await?;
        crate::storage::fields::load_media_hydration_map_in_conn(&mut conn, &media_ids).await?
    };
    let mut entries = Vec::with_capacity(page.entries.len());
    for record in page.entries {
        if let Some(eff) = schemas.get(&record.content_type_key) {
            let u = entry_urls.get(&record.id).cloned();
            let terms = terms_map.remove(&record.id).unwrap_or_default();
            let eligible = eligible_map
                .get(&record.content_type_key)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let mut view = content_render::entry_view_with_context_and_media(
                &record,
                eff,
                u,
                &terms,
                eligible,
                FieldRole::Editor,
                &media_map,
            );
            if let Some(proj) = proj_map.remove(&record.id) {
                view.ext = proj.ext;
                view.theme_ext = proj.theme_ext;
            }
            entries.push(view);
        }
    }
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
        fields: Vec::new(),
        settings: Default::default(),
    };
    let active_v3 = match themes_root {
        Some(root) => theme_storage::load_active_theme_v3(db, &root.0).await?,
        None => None,
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
    let candidates = overrides::build_candidates(&PublicTemplateContext::Taxonomy {
        taxonomy: schema.key.clone(),
        term_id,
        slug: term.slug.clone(),
    })?;
    let mut budget = overrides::PublicCompositionBudget::default();
    budget.render_budget.richtext_nonces.extend(richtext_nonces);
    let locale_ctx = seo::build_theme_locale_context(
        db,
        Some(&locale),
        seo::ThemeLocaleTarget::Term { term_id },
    )
    .await?;
    budget.set_locale_context(active_v3.as_ref(), &locale, locale_ctx);
    let ct_value = serde_json::json!({ "key": schema.key, "label": schema.label });
    let cache = crate::page::BlockTemplateCache::global();
    let body = resolve_public_template_html(
        db,
        &candidates,
        active_v3.as_ref(),
        &site_ctx,
        &site_opts,
        &theme_opts,
        Some(&locale),
        &fallback_html,
        None,
        PublicRouteContext {
            route_kind: "taxonomy",
            route_path: Some(&canonical_base),
            entry_id: None,
            entry_type_key: None,
            breadcrumbs: &breadcrumbs,
            pagination: &pagination,
            archive_url: None,
            previous_url: previous.as_deref(),
            next_url: next.as_deref(),
            entries: &entries,
            taxonomy: Some(&tax_value),
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
            db,
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
    let enqueued = enqueued_for_route(active_v3.as_ref(), EnqueueCondition::Taxonomy);
    let canonical_query =
        crate::cache::canonicalize_public_query(raw_query, QueryScope::TaxonomyArchive(kind));
    let composed = content_render::compose_document_with_seo_with_assets(
        header.as_deref(),
        &body.html,
        footer.as_deref(),
        &seo_view,
        &["od-route-taxonomy", &tax_class],
        &enqueued,
        active_v3.is_some(),
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
