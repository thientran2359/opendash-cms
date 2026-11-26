//! Public CPT archive/single routes and referenced-only media delivery.
//! Legacy CPT Save stays live; core post/page draft data is never served here.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use axum::{
    Extension,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::content::{ContentTypeDefinition, FieldRole};
use opendash_core::theme::{EnqueueCondition, ResolvedEnqueuedAssets, resolve_enqueued_assets};
use sqlx::SqlitePool;

use crate::{
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

fn normalized_origin(public_origin: Option<&str>) -> &str {
    public_origin
        .map(|s| s.trim().trim_end_matches('/'))
        .unwrap_or("")
}

fn enqueued_for_route(
    active: Option<&theme_storage::ActiveThemeV3>,
    condition: EnqueueCondition,
) -> ResolvedEnqueuedAssets {
    active.map_or_else(ResolvedEnqueuedAssets::default, |active| {
        resolve_enqueued_assets(&active.manifest, &active.revision_digest, condition)
    })
}

struct PublicRouteContext<'a> {
    route_kind: &'a str,
    route_path: Option<&'a str>,
    entry_id: Option<i64>,
    entry_type_key: Option<&'a str>,
    breadcrumbs: &'a [content_render::BreadcrumbItem],
    pagination: &'a content_render::PaginationView,
    archive_url: Option<&'a str>,
    previous_url: Option<&'a str>,
    next_url: Option<&'a str>,
    entries: &'a [content_render::EntryView],
    taxonomy: Option<&'a serde_json::Value>,
    content_type: Option<&'a serde_json::Value>,
    entry: Option<&'a content_render::EntryView>,
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
async fn resolve_public_template_html(
    db: &SqlitePool,
    candidates: &[opendash_core::theme::TemplateCandidate],
    active: Option<&theme_storage::ActiveThemeV3>,
    site_ctx: &SiteContext<'_>,
    theme_opts: &BTreeMap<String, serde_json::Value>,
    locale: Option<&str>,
    fallback_html: &str,
    route_entry: Option<(&str, &str)>,
    route_ctx: PublicRouteContext<'_>,
    budget: &mut overrides::PublicCompositionBudget,
    cache: &crate::page::BlockTemplateCache,
) -> ApiResult<ResolvedPublicHtml> {
    let resolution = overrides::resolve_template_2axis(
        db,
        candidates,
        active.map(|v| &v.templates),
        active.map(|v| &v.parts),
        active.map(|v| &v.page_layouts),
    )
    .await?;
    render_resolved_public_template_html(
        db,
        resolution,
        active,
        site_ctx,
        theme_opts,
        locale,
        fallback_html,
        route_entry,
        route_ctx,
        budget,
        cache,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn render_resolved_public_template_html(
    db: &SqlitePool,
    resolution: TemplateResolution,
    active: Option<&theme_storage::ActiveThemeV3>,
    site_ctx: &SiteContext<'_>,
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

fn permanent_redirect(location: String) -> Response {
    (
        StatusCode::MOVED_PERMANENTLY,
        [(header::LOCATION, location)],
        "",
    )
        .into_response()
}

/// A missing or withdrawn Reading Page renders only a non-entry context. The
/// status and robots header remain authoritative even if an Index template
/// supplies the visual shell.
#[allow(dead_code)]
pub(crate) async fn render_not_found_response(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
) -> ApiResult<Response> {
    render_not_found_response_for_locale(db, themes_root, None).await
}

pub(crate) async fn render_not_found_response_for_locale(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    locale: Option<&str>,
) -> ApiResult<Response> {
    let active = match themes_root {
        Some(Extension(root)) => theme_storage::load_active_theme_v3(db, &root.0).await?,
        None => None,
    };
    let site = settings::get_general_for_locale(db, locale).await?;
    let pub_settings = settings::get_public(db).await?;
    let opts = settings::get_active_public_theme_options_for_locale(db, locale).await?;
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
    let html = content_render::finalize_document_icons(html, &budget.render_budget.icon_nonces)?;
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
) -> ApiResult<Response> {
    if type_key == "post"
        && matches!(
            seo::reading_page_state_for_locale(&db, Some(&locale), false).await?,
            ReadingPageState::Missing
        )
    {
        return render_not_found_response_for_locale(&db, themes_root, Some(&locale)).await;
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
    let theme_opts =
        settings::get_active_public_theme_options_for_locale(&db, Some(&locale)).await?;
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
    Ok(crate::routes::public::rendered_layout_with_form_csp(
        content_render::finalize_document_icons(
            content_render::compose_document_with_seo_with_assets(
                header.as_deref(),
                &body.html,
                footer.as_deref(),
                &seo_view,
                &["od-route-archive", &type_class],
                &enqueued,
                active_v3.is_some(),
            )?,
            &budget.render_budget.icon_nonces,
        )?,
        body.has_active_contact_form || shell_has_form,
    ))
}

#[allow(dead_code)]
pub(crate) async fn render_single_entry_response(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    entry: crate::storage::entries::ContentEntryRecord,
    canonical_path_override: Option<String>,
) -> ApiResult<Response> {
    render_single_entry_response_with_query(db, themes_root, entry, canonical_path_override, None)
        .await
}

pub(crate) async fn render_single_entry_response_with_query(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    entry: crate::storage::entries::ContentEntryRecord,
    canonical_path_override: Option<String>,
    raw_query: Option<&str>,
) -> ApiResult<Response> {
    render_entry_response_with_cache(
        db,
        themes_root,
        entry,
        canonical_path_override,
        false,
        raw_query,
        crate::page::BlockTemplateCache::global(),
    )
    .await
}

pub(crate) async fn render_homepage_entry_response_for_locale(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    entry: crate::storage::entries::ContentEntryRecord,
    canonical_path: String,
    raw_query: Option<&str>,
) -> ApiResult<Response> {
    if entry.content_type_key != "page" {
        return render_not_found_response_for_locale(db, themes_root, None).await;
    }
    render_entry_response_with_cache(
        db,
        themes_root,
        entry,
        Some(canonical_path),
        true,
        raw_query,
        crate::page::BlockTemplateCache::global(),
    )
    .await
}

async fn render_entry_response_with_cache(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    entry: crate::storage::entries::ContentEntryRecord,
    canonical_path_override: Option<String>,
    is_home: bool,
    raw_query: Option<&str>,
    cache: &crate::page::BlockTemplateCache,
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
        return render_not_found_response_for_locale(&db, themes_root, None).await;
    };
    let public_content::PublicEntryRenderSnapshot {
        entry,
        locale: entry_locale,
        effective_schema,
        eligible_public_taxonomies,
        terms,
        active: active_v3,
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
                active_v3.as_ref().is_some_and(|theme| {
                    theme
                        .manifest
                        .page_layouts
                        .iter()
                        .any(|layout| &layout.key == *key)
                })
            })
            .cloned();
        // Preserve the Page's own Builder body. Only the shared template
        // candidate chain changes: Home -> PageLayout -> Page -> Index.
        let candidates =
            overrides::build_candidates(&PublicTemplateContext::Home { page_layout_key })?;
        resolution = overrides::resolve_template_2axis(
            &db,
            &candidates,
            active_v3.as_ref().map(|v| &v.templates),
            active_v3.as_ref().map(|v| &v.parts),
            active_v3.as_ref().map(|v| &v.page_layouts),
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
    let theme_opts =
        settings::get_active_public_theme_options_for_locale(&db, Some(&entry_locale)).await?;
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
    let (archive_path, mut proj_map) = {
        let mut conn = db.acquire().await?;
        let path =
            permalinks::canonical_archive_path_in_tx(&mut conn, &type_key, &entry_locale).await?;
        let proj_map = crate::storage::plugin_projections::batch_load_entry_projections_in_conn(
            &mut conn,
            &[entry.id],
        )
        .await?;
        (path, proj_map)
    };
    let mut view = content_render::entry_view_with_context(
        &entry,
        &effective_schema,
        Some(canonical_path_for_seo.clone()),
        &terms,
        &eligible_public_taxonomies,
        FieldRole::Editor,
    );
    if let Some(proj) = proj_map.remove(&entry.id) {
        view.ext = proj.ext;
        view.theme_ext = proj.theme_ext;
    }
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
    budget.set_locale_context(active_v3.as_ref(), &entry_locale, locale_ctx);
    let ct_value = serde_json::json!({ "key": schema.key, "label": schema.label });
    let pagination = PaginationView::empty();
    let empty_entries = Vec::new();
    let body = render_resolved_public_template_html(
        &db,
        resolution,
        active_v3.as_ref(),
        &site_ctx,
        &theme_opts,
        Some(&entry_locale),
        &fallback_html,
        Some((&type_key, &entry.slug)),
        PublicRouteContext {
            route_kind: if is_home { "home" } else { "single" },
            route_path: Some(&canonical_path_for_seo),
            entry_id: Some(entry.id),
            entry_type_key: Some(&type_key),
            breadcrumbs: &breadcrumbs,
            pagination: &pagination,
            archive_url: archive_path.as_deref(),
            previous_url: None,
            next_url: None,
            entries: &empty_entries,
            taxonomy: None,
            content_type: Some(&ct_value),
            entry: Some(&view),
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
            Some(&entry_locale),
            &fmt_cfg,
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
    let enqueued = enqueued_for_route(active_v3.as_ref(), route_condition);
    Ok(crate::routes::public::rendered_layout_with_form_csp(
        content_render::finalize_document_icons(
            content_render::compose_document_with_seo_with_assets(
                header.as_deref(),
                &body.html,
                footer.as_deref(),
                &seo_view,
                body_classes,
                &enqueued,
                active_v3.is_some(),
            )?,
            &budget.render_budget.icon_nonces,
        )?,
        body.has_active_contact_form || entry_has_form || shell_has_form,
    ))
}

async fn require_term_kind(
    db: &SqlitePool,
    term_id: i64,
    expected: Option<TaxonomyKind>,
) -> ApiResult<String> {
    // The storage ACL helper hides private/withdrawn/provider-inactive terms.
    // Routes must not issue SQL or infer visibility from a bare term row.
    if public_content::canonical_term_by_id(db, term_id)
        .await?
        .is_none()
    {
        return Err(ApiError::not_found("Không tìm thấy taxonomy."));
    }
    let term = crate::storage::taxonomy::get_term(db, term_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy taxonomy."))?;
    let kind = term.kind;
    if expected.is_some_and(|requested| requested.as_str() != kind) {
        return Err(ApiError::not_found("Không tìm thấy taxonomy."));
    }
    Ok(kind)
}

/// Redirect a historical term path directly to its current canonical URL,
/// retaining only the typed pagination/filter query (never untrusted keys).
pub(crate) async fn redirect_canonical_taxonomy(
    db: &SqlitePool,
    term_id: i64,
    canonical_base: String,
    raw_query: Option<&str>,
) -> ApiResult<Response> {
    let kind = require_term_kind(db, term_id, None).await?;
    let scope = QueryScope::TaxonomyArchive(if kind == "tag" {
        TaxonomyKind::Tag
    } else {
        TaxonomyKind::Category
    });
    let query = parse_query(raw_query, scope).map_err(ApiError::bad_request)?;
    Ok(permanent_redirect(CanonicalUrlBuilder::with_query(
        canonical_base,
        query,
        scope,
    )))
}

/// The public fallback uses the same term-ID renderer as the legacy routes.
/// The canonical path was already selected by storage; no slug-based routing
/// is allowed to override it.
pub(crate) async fn render_canonical_taxonomy(
    db: &SqlitePool,
    themes_root: Option<ThemeStoreRoot>,
    term_id: i64,
    locale: String,
    canonical_base: String,
    raw_query: Option<&str>,
    expected_kind: Option<TaxonomyKind>,
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
    let mut entries = Vec::with_capacity(page.entries.len());
    for record in page.entries {
        if let Some(eff) = schemas.get(&record.content_type_key) {
            let u = entry_urls.get(&record.id).cloned();
            let terms = terms_map.remove(&record.id).unwrap_or_default();
            let eligible = eligible_map
                .get(&record.content_type_key)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let mut view = content_render::entry_view_with_context(
                &record,
                eff,
                u,
                &terms,
                eligible,
                FieldRole::Editor,
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
    let theme_opts =
        settings::get_active_public_theme_options_for_locale(db, Some(&locale)).await?;
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
    Ok(crate::routes::public::rendered_layout_with_form_csp(
        content_render::finalize_document_icons(
            content_render::compose_document_with_seo_with_assets(
                header.as_deref(),
                &body.html,
                footer.as_deref(),
                &seo_view,
                &["od-route-taxonomy", &tax_class],
                &enqueued,
                active_v3.is_some(),
            )?,
            &budget.render_budget.icon_nonces,
        )?,
        body.has_active_contact_form || shell_has_form,
    ))
}

async fn media_response(
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
    let asset = media::find(db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Ảnh công khai không tồn tại."))?;
    if thumbnail && asset.kind == "video" {
        return Err(ApiError::not_found("Video không có thumbnail."));
    }
    if !thumbnail && asset.kind == "video" {
        let path = store.canonical_path(id, &asset.extension)?;
        return crate::media::stream_authorized_video(method, headers, &asset, &path, store, false)
            .await;
    }
    let bytes = store.read(id, &asset.extension, thumbnail).await?;
    let mime = if thumbnail {
        "image/png"
    } else {
        asset.mime_type.as_str()
    };
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CACHE_CONTROL, "no-store"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; sandbox",
            ),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        bytes,
    )
        .into_response())
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
