//! Public CPT archive/single routes and referenced-only media delivery.
//! Legacy CPT Save stays live; core post/page draft data is never served here.

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Extension,
    extract::{OriginalUri, Path, RawQuery, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use opendash_core::{content::ContentTypeDefinition, id::valid_kebab_segment_with_limit};
use sqlx::SqlitePool;

use crate::{
    content_render::{self, SiteContext, entry_view_with_url},
    error::{ApiError, ApiResult},
    media::MediaStore,
    public_context::{
        ARCHIVE_SIZE, CanonicalUrlBuilder, MAX_ARCHIVE_PAGE, PublicQuery, PublicRequestContext,
        QueryScope, TaxonomyKind, parse_query, validate_public_request_path, validate_single_query,
    },
    routes::themes::ThemeStoreRoot,
    storage::{
        content::load_content_type_definition, entries::EntryFilters, media,
        overrides::{self, OverrideRole},
        page::active_content_templates, permalinks::{self, PublicRouteTarget},
        public_content, settings, theme as theme_storage,
    },
};

fn check_type_key(type_key: &str) -> ApiResult<()> {
    if CanonicalUrlBuilder::valid_type(type_key) {
        Ok(())
    } else {
        Err(ApiError::not_found("Không tìm thấy loại nội dung."))
    }
}

fn public_html(html: String) -> Response {
    (
        [
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; img-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Html(html),
    )
        .into_response()
}

fn permanent_redirect(location: String) -> Response {
    (StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, location)], "").into_response()
}

pub async fn archive(
    State(db): State<SqlitePool>,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    Path(type_key): Path<String>,
    OriginalUri(uri): OriginalUri,
    RawQuery(raw): RawQuery,
) -> ApiResult<Response> {
    validate_public_request_path(uri.path()).map_err(ApiError::bad_request)?;
    check_type_key(&type_key)?;
    let legacy_path = CanonicalUrlBuilder::archive(&type_key)
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    if uri.path() != legacy_path {
        return Err(ApiError::not_found("Đường dẫn nội dung không hợp lệ."));
    }
    let resolved = permalinks::resolve_public_path(&db, uri.path()).await?
        .ok_or_else(|| ApiError::not_found("Archive nội dung không tồn tại."))?;
    if !matches!(&resolved.target, PublicRouteTarget::Archive(key) if key == &type_key) {
        return Err(ApiError::not_found("Archive nội dung không tồn tại."));
    }
    let query = parse_query(raw.as_deref(), QueryScope::ContentArchive)
        .map_err(ApiError::bad_request)?;
    if resolved.redirect {
        return Ok(permanent_redirect(CanonicalUrlBuilder::with_query(
            resolved.canonical_path, query, QueryScope::ContentArchive,
        )));
    }
    render_canonical_archive(db, themes_root, type_key, resolved.canonical_path, raw).await
}

/// Called both by legacy static routes and the canonical fallback router.
/// The incoming route must already have been resolved by storage.
pub(crate) async fn render_canonical_archive(
    db: SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    type_key: String,
    canonical_base: String,
    raw: Option<String>,
) -> ApiResult<Response> {
    // Enforce the CPT's public/archive settings before loading or rendering
    // content. The storage archive query independently checks this policy.
    public_content::require_public_archive(&db, &type_key).await?;
    let query =
        parse_query(raw.as_deref(), QueryScope::ContentArchive).map_err(ApiError::bad_request)?;
    let _context = PublicRequestContext::Archive {
        type_key: type_key.clone(),
        query: query.clone(),
        canonical_path: CanonicalUrlBuilder::with_query(
            canonical_base.clone(), query.clone(), QueryScope::ContentArchive,
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
    let active_v2 = match themes_root {
        Some(Extension(root)) => theme_storage::load_active_v2_role_templates(&db, &root.0).await?,
        None => None,
    };
    let (list_template, _) = active_content_templates(&db).await?;
    let selected = active_v2
        .as_ref()
        .and_then(|v2| v2.roles.get("archive").map(String::as_str))
        .or(list_template.as_deref());
    let site = settings::get_general(&db).await?;
    let theme_opts = settings::get_active_public_theme_options(&db).await?;
    let site_ctx = SiteContext {
        title: &site.site_title,
        tagline: &site.tagline,
    };
    let body_html = match overrides::resolve_effective_override(&db, OverrideRole::Archive, Some(&type_key), None).await? {
        Some(record) => overrides::render_override_for_public(&db, &record, None).await?.html,
        None => content_render::render_archive_with_site(
            &schema,
            &entries,
            previous.as_deref(),
            next.as_deref(),
            selected,
            &site_ctx,
        )?,
    };
    let (header, footer) = overrides::resolve_and_render_header_footer(
        &db,
        active_v2.as_ref(),
        &site_ctx,
        &theme_opts,
        Some(&type_key),
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
            &body_html,
            footer.as_deref(),
            &seo_view,
        )?,
        false,
    ))
}

pub async fn single(
    State(db): State<SqlitePool>,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    Path((type_key, slug)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    RawQuery(raw): RawQuery,
) -> ApiResult<Response> {
    validate_public_request_path(uri.path()).map_err(ApiError::bad_request)?;
    validate_single_query(raw.as_deref()).map_err(ApiError::bad_request)?;
    check_type_key(&type_key)?;
    if !valid_kebab_segment_with_limit(&slug, 64) {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    }
    let legacy_path = CanonicalUrlBuilder::entry(&type_key, &slug)
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    if uri.path() != legacy_path {
        return Err(ApiError::not_found("Đường dẫn nội dung không hợp lệ."));
    }
    let resolved = permalinks::resolve_public_path(&db, uri.path()).await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    let PublicRouteTarget::Entry(entry_id) = resolved.target else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };
    // Resolve by stable ID so an old slug never selects a replacement entry.
    let entry = public_content::get_published_by_id(&db, entry_id).await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    if resolved.redirect {
        return Ok(permanent_redirect(resolved.canonical_path));
    }
    render_single_entry_response(&db, themes_root, entry, Some(resolved.canonical_path)).await
}

pub(crate) async fn render_single_entry_response(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    entry: crate::storage::entries::ContentEntryRecord,
    canonical_path_override: Option<String>,
) -> ApiResult<Response> {
    let db = db.clone();
    let type_key = entry.content_type_key.clone();
    let entry_id_for_seo = entry.id;
    let entry_title_for_seo = entry.title.clone();
    let schema = load_content_type_definition(&db, &type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let canonical_path = canonical_path_override
        .ok_or_else(|| ApiError::not_found("Không tìm thấy canonical nội dung."))?;
    let canonical_path_for_seo = canonical_path.clone();
    let _context = PublicRequestContext::Single {
        entry_id: entry.id,
        type_key: type_key.clone(),
        slug: entry.slug.clone(),
        canonical_path: canonical_path.clone(),
    };
    let active_v2 = match themes_root {
        Some(Extension(root)) => theme_storage::load_active_v2_role_templates(&db, &root.0).await?,
        None => None,
    };
    let role = if type_key == "page" { OverrideRole::Page } else { OverrideRole::Single };
    let (_, single_template) = active_content_templates(&db).await?;
    let selected = active_v2
        .as_ref()
        .and_then(|v2| {
            v2.roles
                .get(role.as_str())
                .or_else(|| if role == OverrideRole::Page { v2.roles.get("single") } else { None })
                .map(String::as_str)
        })
        .or(single_template.as_deref());
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
    let (body_html, allow_form_self) = match overrides::resolve_effective_override(&db, role, Some(&type_key), Some(entry.id)).await? {
        Some(record) => {
            let rendered = overrides::render_override_for_public(&db, &record, Some((&type_key, &entry.slug))).await?;
            (rendered.html, rendered.has_active_contact_form)
        }
        None => (
            content_render::render_single_with_site_and_archive(
                &schema,
                &entry_view_with_url(entry, &schema, Some(canonical_path)),
                selected,
                &site_ctx,
                archive_path.as_deref(),
            )?,
            false,
        ),
    };
    let (header, footer) = overrides::resolve_and_render_header_footer(
        &db,
        active_v2.as_ref(),
        &site_ctx,
        &theme_opts,
        Some(&type_key),
    )
    .await?;
    let seo_view = crate::storage::seo::build_entry_seo_view(
        &db,
        entry_id_for_seo,
        &entry_title_for_seo,
        &canonical_path_for_seo,
    )
    .await?;
    Ok(crate::routes::public::rendered_layout_with_form_csp(
        content_render::compose_document_with_seo(
            header.as_deref(),
            &body_html,
            footer.as_deref(),
            &seo_view,
        )?,
        allow_form_self,
    ))
}

pub async fn category_archive(
    State(db): State<SqlitePool>,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    Path(slug): Path<String>,
    OriginalUri(uri): OriginalUri,
    RawQuery(raw): RawQuery,
) -> ApiResult<Response> {
    taxonomy_archive(
        &db,
        themes_root.map(|Extension(r)| r),
        TaxonomyKind::Category,
        &slug,
        uri.path(),
        raw.as_deref(),
    )
    .await
}

pub async fn tag_archive(
    State(db): State<SqlitePool>,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    Path(slug): Path<String>,
    OriginalUri(uri): OriginalUri,
    RawQuery(raw): RawQuery,
) -> ApiResult<Response> {
    taxonomy_archive(
        &db,
        themes_root.map(|Extension(r)| r),
        TaxonomyKind::Tag,
        &slug,
        uri.path(),
        raw.as_deref(),
    )
    .await
}

async fn taxonomy_archive(
    db: &SqlitePool,
    themes_root: Option<ThemeStoreRoot>,
    kind: TaxonomyKind,
    slug: &str,
    request_path: &str,
    raw_query: Option<&str>,
) -> ApiResult<Response> {
    validate_public_request_path(request_path).map_err(ApiError::bad_request)?;
    let legacy_path = CanonicalUrlBuilder::taxonomy(kind, slug)
        .ok_or_else(|| ApiError::not_found("Không tìm thấy taxonomy."))?;
    if request_path != legacy_path {
        return Err(ApiError::not_found("Đường dẫn taxonomy không hợp lệ."));
    }
    let resolved = permalinks::resolve_public_path(db, request_path).await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy taxonomy."))?;
    let PublicRouteTarget::Term(term_id) = resolved.target else {
        return Err(ApiError::not_found("Không tìm thấy taxonomy."));
    };
    let query = parse_query(raw_query, QueryScope::TaxonomyArchive(kind))
        .map_err(ApiError::bad_request)?;
    if resolved.redirect {
        // Verify the stored target's current taxonomy kind before redirecting.
        require_term_kind(db, term_id, Some(kind)).await?;
        return Ok(permanent_redirect(CanonicalUrlBuilder::with_query(
            resolved.canonical_path, query, QueryScope::TaxonomyArchive(kind),
        )));
    }
    render_canonical_taxonomy(db, themes_root, term_id, resolved.canonical_path, raw_query, Some(kind)).await
}

async fn require_term_kind(
    db: &SqlitePool,
    term_id: i64,
    expected: Option<TaxonomyKind>,
) -> ApiResult<String> {
    // The storage ACL helper hides private/withdrawn/provider-inactive terms.
    // Routes must not issue SQL or infer visibility from a bare term row.
    if public_content::canonical_term_by_id(db, term_id).await?.is_none() {
        return Err(ApiError::not_found("Không tìm thấy taxonomy."));
    }
    let term = crate::storage::taxonomy::get_term(db, term_id).await?
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
    Ok(permanent_redirect(CanonicalUrlBuilder::with_query(canonical_base, query, scope)))
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
    let (confirmed_kind, term, page) =
        public_content::taxonomy_public_page_by_id(db, term_id, query.filters.clone(), ARCHIVE_SIZE, offset)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy taxonomy."))?;
    if confirmed_kind != kind_key {
        return Err(ApiError::not_found("Không tìm thấy taxonomy."));
    }
    let canonical_path = CanonicalUrlBuilder::with_query(
        canonical_base.clone(), query.clone(), QueryScope::TaxonomyArchive(kind),
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
    let entry_urls = crate::storage::seo::resolve_public_entry_paths_by_ids(db, &page.entries).await?;
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
    let active_v2 = match themes_root {
        Some(root) => theme_storage::load_active_v2_role_templates(db, &root.0).await?,
        None => None,
    };
    let selected = active_v2
        .as_ref()
        .and_then(|v2| v2.roles.get("taxonomy_archive").map(String::as_str));
    let site_ctx = SiteContext {
        title: &site.site_title,
        tagline: &site.tagline,
    };
    let body_html = match overrides::resolve_effective_override(db, OverrideRole::TaxonomyArchive, None, None).await? {
        Some(record) => overrides::render_override_for_public(db, &record, None).await?.html,
        None => content_render::render_archive_with_site(
            &schema,
            &entries,
            previous.as_deref(),
            next.as_deref(),
            selected,
            &site_ctx,
        )?,
    };
    let (header, footer) = overrides::resolve_and_render_header_footer(
        db,
        active_v2.as_ref(),
        &site_ctx,
        &theme_opts,
        None,
    )
    .await?;
    let seo_view = crate::storage::seo::build_simple_seo_view(
        db,
        &schema.label,
        Some(&canonical_base),
        false,
    )
    .await?;
    Ok(crate::routes::public::rendered_layout_with_form_csp(
        content_render::compose_document_with_seo(
            header.as_deref(),
            &body_html,
            footer.as_deref(),
            &seo_view,
        )?,
        false,
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
        return crate::media::stream_authorized_video(method, headers, &asset, &path, store, false).await;
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
