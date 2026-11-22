//! Public CPT archive/single routes and referenced-only media delivery.
//! Legacy CPT Save stays live; core post/page draft data is never served here.

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Extension,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use opendash_core::content::ContentTypeDefinition;
use sqlx::SqlitePool;

use crate::{
    content_render::{self, SiteContext, entry_view_with_url},
    error::{ApiError, ApiResult},
    media::MediaStore,
    public_context::{
        ARCHIVE_SIZE, CanonicalUrlBuilder, MAX_ARCHIVE_PAGE, PublicQuery, PublicRequestContext,
        QueryScope, TaxonomyKind, parse_query,
    },
    routes::themes::ThemeStoreRoot,
    storage::{
        content::load_content_type_definition,
        entries::EntryFilters,
        media,
        overrides::{self, PublicTemplateContext, TemplateResolution},
        page::active_content_templates,
        permalinks, public_content,
        seo::{self, ReadingPageState},
        settings, theme as theme_storage,
    },
};

struct ResolvedPublicHtml {
    html: String,
    has_active_contact_form: bool,
    /// Page Layouts own their canvas; wrapping them in the site's default
    /// header/footer would violate the layout choice.
    canvas_layout: bool,
}

async fn resolve_public_template_html(
    db: &SqlitePool,
    candidates: &[opendash_core::theme::TemplateCandidate],
    active: Option<&theme_storage::ActiveThemeV3>,
    site_ctx: &SiteContext<'_>,
    theme_opts: &BTreeMap<String, serde_json::Value>,
    fallback_html: &str,
    route_entry: Option<(&str, &str)>,
    route_context: serde_json::Value,
    budget: &mut overrides::PublicCompositionBudget,
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
        fallback_html,
        route_entry,
        route_context,
        budget,
    )
    .await
}

async fn render_resolved_public_template_html(
    db: &SqlitePool,
    resolution: TemplateResolution,
    active: Option<&theme_storage::ActiveThemeV3>,
    site_ctx: &SiteContext<'_>,
    theme_opts: &BTreeMap<String, serde_json::Value>,
    fallback_html: &str,
    route_entry: Option<(&str, &str)>,
    route_context: serde_json::Value,
    budget: &mut overrides::PublicCompositionBudget,
) -> ApiResult<ResolvedPublicHtml> {
    let context = if matches!(&resolution, TemplateResolution::CoreFallback) {
        serde_json::Value::Null
    } else {
        let menus = crate::storage::menu::resolve_active_slot_menus(db).await?;
        let mut context = serde_json::json!({
            "site": site_ctx,
            "options": theme_opts,
            "theme_options": theme_opts,
            "menus": menus,
        });
        // Values are generated from the ACL-filtered public route view; theme
        // source may only render them through C-SEC escaping.
        if let (Some(base), Some(extra)) = (context.as_object_mut(), route_context.as_object()) {
            base.extend(
                extra
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }
        context
    };
    // A fallback renderer can return a complete HTML document. Only its
    // normalized body fragment and authorized styles may enter a typed slot;
    // injecting a second document envelope would invalidate the whole page.
    let slot_html = if matches!(&resolution, TemplateResolution::CoreFallback) {
        String::new()
    } else {
        match crate::document::normalize_public_document(fallback_html) {
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
                });
            }
        }
    };
    match resolution {
        TemplateResolution::CoreFallback => Ok(ResolvedPublicHtml {
            html: fallback_html.to_owned(),
            has_active_contact_form: false,
            canvas_layout: false,
        }),
        TemplateResolution::DbOverride(record) => {
            let canvas_layout =
                record.target.kind == opendash_core::theme::TemplateCandidateKind::PageLayout;
            let rendered = overrides::render_resolved_public_builder(
                db,
                &record,
                active,
                &context,
                route_entry,
                Some(&slot_html),
                budget,
            )
            .await?;
            Ok(ResolvedPublicHtml {
                html: rendered.html,
                has_active_contact_form: rendered.has_active_contact_form,
                canvas_layout,
            })
        }
        TemplateResolution::ActiveTheme { target_key, html } => {
            use crate::template_sec::{CsecContentKind, validate_and_compile_csec_composition};
            // The last candidate can be `index` even for a Single/Archive
            // route. Validate the selected template's own slot contract, not
            // the request's most-specific (and possibly absent) candidate.
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
            let parts = active.map(|v| &v.parts).cloned().unwrap_or_default();
            let composition =
                validate_and_compile_csec_composition(&target_key, &html, &parts, kind)
                    .map_err(ApiError::internal)?;
            let html = budget.render_theme(&composition, &context, Some(&slot_html))?;
            Ok(ResolvedPublicHtml {
                html,
                has_active_contact_form: false,
                canvas_layout: target_key.starts_with("page_layout:"),
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
pub(crate) async fn render_not_found_response(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
) -> ApiResult<Response> {
    let active = match themes_root {
        Some(Extension(root)) => theme_storage::load_active_theme_v3(db, &root.0).await?,
        None => None,
    };
    let site = settings::get_general(db).await?;
    let opts = settings::get_active_public_theme_options(db).await?;
    let site_ctx = SiteContext {
        title: &site.site_title,
        tagline: &site.tagline,
    };
    let mut budget = overrides::PublicCompositionBudget::default();
    let body = resolve_public_template_html(
        db,
        &overrides::build_candidates(&PublicTemplateContext::NotFound)?,
        active.as_ref(),
        &site_ctx,
        &opts,
        "<main><h1>404 — Không tìm thấy trang</h1></main>",
        None,
        serde_json::json!({ "route": { "kind": "not_found" } }),
        &mut budget,
    )
    .await?;
    let (header_html, footer_html, shell_has_form) = overrides::resolve_and_render_header_footer(
        db,
        active.as_ref(),
        &site_ctx,
        &opts,
        &mut budget,
    )
    .await?;
    let view = crate::seo::SeoView::for_noindex_page("vi", "404 — Không tìm thấy trang");
    let html = content_render::compose_document_with_seo(
        header_html.as_deref(),
        &body.html,
        footer_html.as_deref(),
        &view,
    )?;
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
    canonical_base: String,
    raw: Option<String>,
) -> ApiResult<Response> {
    if type_key == "post"
        && matches!(
            seo::reading_page_state(&db, false).await?,
            ReadingPageState::Missing
        )
    {
        return render_not_found_response(&db, themes_root).await;
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
    };
    let schema = load_content_type_definition(&db, &type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let offset = (page - 1) * ARCHIVE_SIZE;
    let mut records =
        public_content::list_published(&db, &type_key, ARCHIVE_SIZE + 1, offset, filters).await?;
    let has_more = records.len() > ARCHIVE_SIZE as usize;
    records.truncate(ARCHIVE_SIZE as usize);
    let entry_urls = crate::storage::seo::resolve_public_entry_paths_by_ids(&db, &records).await?;
    let entries: Vec<_> = records
        .into_iter()
        .map(|record| {
            let u = entry_urls.get(&record.id).cloned();
            entry_view_with_url(record, &schema, u)
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
    let active_v3 = match themes_root {
        Some(Extension(root)) => theme_storage::load_active_theme_v3(&db, &root.0).await?,
        None => None,
    };
    let (list_template, _) = active_content_templates(&db).await?;
    let site = settings::get_general(&db).await?;
    let theme_opts = settings::get_active_public_theme_options(&db).await?;
    let site_ctx = SiteContext {
        title: &site.site_title,
        tagline: &site.tagline,
    };
    let fallback_html = content_render::render_archive_with_site(
        &schema,
        &entries,
        previous.as_deref(),
        next.as_deref(),
        list_template.as_deref(),
        &site_ctx,
    )?;
    let candidates = overrides::build_candidates(&PublicTemplateContext::Archive {
        content_type: type_key.clone(),
    })?;
    let mut budget = overrides::PublicCompositionBudget::default();
    let body = resolve_public_template_html(
        &db,
        &candidates,
        active_v3.as_ref(),
        &site_ctx,
        &theme_opts,
        &fallback_html,
        None,
        serde_json::json!({
            // Do not export ContentTypeDefinition fields/settings/defaults to a
            // public theme: field-level access is represented exclusively by
            // exposure-filtered EntryView values.
            "content_type": {"key": schema.key, "label": schema.label},
            "entries": entries,
            "previous_url": previous,
            "next_url": next,
            "route": {"kind":"archive", "path":canonical_base},
        }),
        &mut budget,
    )
    .await?;
    let (header, footer, shell_has_form) = overrides::resolve_and_render_header_footer(
        &db,
        active_v3.as_ref(),
        &site_ctx,
        &theme_opts,
        &mut budget,
    )
    .await?;
    let seo_view = crate::storage::seo::build_simple_seo_view(
        &db,
        &schema.label,
        Some(&canonical_base),
        false,
    )
    .await?;
    Ok(crate::routes::public::rendered_layout_with_form_csp(
        content_render::compose_document_with_seo(
            header.as_deref(),
            &body.html,
            footer.as_deref(),
            &seo_view,
        )?,
        body.has_active_contact_form || shell_has_form,
    ))
}

pub(crate) async fn render_single_entry_response(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    entry: crate::storage::entries::ContentEntryRecord,
    canonical_path_override: Option<String>,
) -> ApiResult<Response> {
    render_entry_response_with_cache(
        db,
        themes_root,
        entry,
        canonical_path_override,
        false,
        crate::page::BlockTemplateCache::global(),
    )
    .await
}

pub(crate) async fn render_homepage_entry_response(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    entry: crate::storage::entries::ContentEntryRecord,
) -> ApiResult<Response> {
    if entry.content_type_key != "page" {
        return render_not_found_response(db, themes_root).await;
    }
    render_entry_response_with_cache(
        db,
        themes_root,
        entry,
        Some("/".to_owned()),
        true,
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
    cache: &crate::page::BlockTemplateCache,
) -> ApiResult<Response> {
    let db = db.clone();
    let snapshot = public_content::load_entry_render_snapshot_with_cache(
        &db,
        entry.id,
        themes_root.as_ref().map(|Extension(root)| root.0.as_path()),
        cache,
    )
    .await?;
    let Some(snapshot) = snapshot else {
        return render_not_found_response(&db, themes_root).await;
    };
    let public_content::PublicEntryRenderSnapshot {
        entry,
        schema,
        active: active_v3,
        mut resolution,
        body: entry_body,
        budget: entry_render_budget,
    } = snapshot;
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
    let (_, single_template) = active_content_templates(&db).await?;
    let site = settings::get_general(&db).await?;
    let theme_opts = settings::get_active_public_theme_options(&db).await?;
    let site_ctx = SiteContext {
        title: &site.site_title,
        tagline: &site.tagline,
    };
    let archive_path = {
        let mut conn = db.acquire().await?;
        permalinks::canonical_archive_path_in_tx(&mut conn, &type_key).await?
    };
    let entry_has_form = entry_body
        .as_ref()
        .is_some_and(|body| body.has_active_contact_form);
    let fallback_html = match entry_body {
        Some(body) => body.html,
        None => content_render::render_single_with_site_and_archive(
            &schema,
            &entry_view_with_url(entry.clone(), &schema, Some(canonical_path)),
            single_template.as_deref(),
            &site_ctx,
            archive_path.as_deref(),
        )?,
    };
    let mut budget = overrides::PublicCompositionBudget::with_render_budget(entry_render_budget);
    let body = render_resolved_public_template_html(
        &db, resolution, active_v3.as_ref(), &site_ctx, &theme_opts,
        &fallback_html, Some((&type_key, &entry.slug)),
        serde_json::json!({
            "content_type": {"key": schema.key, "label": schema.label},
            "entry": entry_view_with_url(entry.clone(), &schema, Some(canonical_path_for_seo.clone())),
            "archive_url": archive_path,
            "route": {"kind":if is_home { "home" } else { "single" }, "path":canonical_path_for_seo},
        }),
        &mut budget,
    ).await?;
    let (header, footer, shell_has_form) = if body.canvas_layout {
        (None, None, false)
    } else {
        overrides::resolve_and_render_header_footer_with_cache(
            &db,
            active_v3.as_ref(),
            &site_ctx,
            &theme_opts,
            &mut budget,
            cache,
        )
        .await?
    };
    let seo_view = if is_home {
        seo::build_home_seo_view(&db).await?
    } else {
        seo::build_entry_seo_view(
            &db,
            entry_id_for_seo,
            &entry_title_for_seo,
            &canonical_path_for_seo,
        )
        .await?
    };
    Ok(crate::routes::public::rendered_layout_with_form_csp(
        content_render::compose_document_with_seo(
            header.as_deref(),
            &body.html,
            footer.as_deref(),
            &seo_view,
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
    let mut schemas = BTreeMap::new();
    let mut entries = Vec::new();
    for record in page.entries {
        let type_key = record.content_type_key.clone();
        if !schemas.contains_key(&type_key) {
            let schema = load_content_type_definition(db, &type_key)
                .await?
                .ok_or_else(|| ApiError::internal("Không tìm thấy schema nội dung."))?;
            schemas.insert(type_key.clone(), schema);
        }
        if let Some(schema) = schemas.get(&type_key) {
            let u = entry_urls.get(&record.id).cloned();
            entries.push(entry_view_with_url(record, schema, u));
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
    let site = settings::get_general(db).await?;
    let theme_opts = settings::get_active_public_theme_options(db).await?;
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
    };
    let fallback_html = content_render::render_archive_with_site(
        &schema,
        &entries,
        previous.as_deref(),
        next.as_deref(),
        None,
        &site_ctx,
    )?;
    let candidates = overrides::build_candidates(&PublicTemplateContext::Taxonomy {
        taxonomy: schema.key.clone(),
        term_id,
        slug: term.slug.clone(),
    })?;
    let mut budget = overrides::PublicCompositionBudget::default();
    let body = resolve_public_template_html(
        db,
        &candidates,
        active_v3.as_ref(),
        &site_ctx,
        &theme_opts,
        &fallback_html,
        None,
        serde_json::json!({
            "content_type": {"key": schema.key, "label": schema.label},
            "entries": entries,
            "taxonomy": {"key":schema.key, "id":term_id, "slug":term.slug, "name":schema.label},
            "previous_url": previous,
            "next_url": next,
            "route": {"kind":"taxonomy", "path":canonical_base},
        }),
        &mut budget,
    )
    .await?;
    let (header, footer, shell_has_form) = overrides::resolve_and_render_header_footer(
        db,
        active_v3.as_ref(),
        &site_ctx,
        &theme_opts,
        &mut budget,
    )
    .await?;
    let seo_view =
        crate::storage::seo::build_simple_seo_view(db, &schema.label, Some(&canonical_base), false)
            .await?;
    Ok(crate::routes::public::rendered_layout_with_form_csp(
        content_render::compose_document_with_seo(
            header.as_deref(),
            &body.html,
            footer.as_deref(),
            &seo_view,
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
