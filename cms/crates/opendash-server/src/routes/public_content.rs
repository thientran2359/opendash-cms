//! Public CPT archive/single routes and referenced-only media delivery.
//! Legacy CPT Save stays live; core post/page draft data is never served here.

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Extension,
    extract::{OriginalUri, Path, RawQuery, State},
    http::{StatusCode, header},
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
        QueryScope, TaxonomyKind, parse_query, validate_single_query,
    },
    routes::themes::ThemeStoreRoot,
    storage::{
        content::load_content_type_definition, entries::EntryFilters, media,
        overrides::{self, OverrideRole},
        page::active_content_templates, public_content, settings, theme as theme_storage,
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

pub async fn archive(
    State(db): State<SqlitePool>,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    Path(type_key): Path<String>,
    OriginalUri(uri): OriginalUri,
    RawQuery(raw): RawQuery,
) -> ApiResult<Response> {
    check_type_key(&type_key)?;
    let canonical_base = CanonicalUrlBuilder::archive(&type_key)
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    if uri.path() != canonical_base {
        return Err(ApiError::not_found("Đường dẫn nội dung không hợp lệ."));
    }
    let query =
        parse_query(raw.as_deref(), QueryScope::ContentArchive).map_err(ApiError::bad_request)?;
    let _context = PublicRequestContext::Archive {
        type_key: type_key.clone(),
        query,
        canonical_path: CanonicalUrlBuilder::content_page(&type_key, query).unwrap_or_default(),
    };
    let page = query.page;
    let filters = EntryFilters {
        category_id: query.filters.category_id,
        tag_id: query.filters.tag_id,
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
            CanonicalUrlBuilder::content_page(
                &type_key,
                PublicQuery {
                    page: page - 1,
                    ..query
                },
            )
        })
        .flatten();
    let next = (has_more && page < MAX_ARCHIVE_PAGE)
        .then(|| {
            CanonicalUrlBuilder::content_page(
                &type_key,
                PublicQuery {
                    page: page + 1,
                    ..query
                },
            )
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
        CanonicalUrlBuilder::archive(&type_key).as_deref(),
        false,
    )
    .await?;
    Ok(public_html(content_render::compose_document_with_seo(
        header.as_deref(),
        &body_html,
        footer.as_deref(),
        &seo_view,
    )?))
}

pub async fn single(
    State(db): State<SqlitePool>,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    Path((type_key, slug)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    RawQuery(raw): RawQuery,
) -> ApiResult<Response> {
    validate_single_query(raw.as_deref()).map_err(ApiError::bad_request)?;
    check_type_key(&type_key)?;
    if !valid_kebab_segment_with_limit(&slug, 64) {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    }
    let canonical_path = CanonicalUrlBuilder::entry(&type_key, &slug)
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    if uri.path() != canonical_path {
        return Err(ApiError::not_found("Đường dẫn nội dung không hợp lệ."));
    }
    if let Some(custom_path) =
        crate::storage::seo::custom_path_for_published_slug(&db, &type_key, &slug).await?
    {
        return Ok((
            StatusCode::MOVED_PERMANENTLY,
            [(axum::http::header::LOCATION, custom_path)],
            "",
        )
            .into_response());
    }
    let entry = match public_content::get_by_slug(&db, &type_key, &slug).await? {
        Some(entry) => entry,
        None => {
            if let crate::storage::seo::PublicRouteResolution::Redirect { location } =
                crate::storage::seo::resolve_custom_or_redirect_path(&db, uri.path()).await?
            {
                return Ok((
                    StatusCode::MOVED_PERMANENTLY,
                    [(axum::http::header::LOCATION, location)],
                    "",
                )
                    .into_response());
            }
            return Err(ApiError::not_found("Không tìm thấy nội dung."));
        }
    };
    render_single_entry_response(&db, themes_root, entry, Some(canonical_path)).await
}

pub(crate) async fn render_single_entry_response(
    db: &SqlitePool,
    themes_root: Option<Extension<ThemeStoreRoot>>,
    entry: crate::storage::entries::ContentEntryRecord,
    canonical_path_override: Option<String>,
) -> ApiResult<Response> {
    let db = db.clone();
    let type_key = entry.content_type_key.clone();
    let slug = entry.slug.clone();
    let entry_id_for_seo = entry.id;
    let entry_title_for_seo = entry.title.clone();
    let schema = load_content_type_definition(&db, &type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let canonical_path_for_seo = canonical_path_override.clone()
        .or_else(|| CanonicalUrlBuilder::entry_from_id(entry_id_for_seo, &type_key, &slug))
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    let canonical_path = canonical_path_override
        .or_else(|| CanonicalUrlBuilder::entry_from_id(entry.id, &type_key, &slug))
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    let _context = PublicRequestContext::Single {
        entry_id: entry.id,
        type_key: type_key.clone(),
        slug: slug.clone(),
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
    let body_html = match overrides::resolve_effective_override(&db, role, Some(&type_key), Some(entry.id)).await? {
        Some(record) => overrides::render_override_for_public(&db, &record, Some((&type_key, &slug))).await?.html,
        None => content_render::render_single_with_site(
            &schema,
            &entry_view_with_url(entry, &schema, Some(canonical_path)),
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
    let seo_view = crate::storage::seo::build_entry_seo_view(
        &db,
        entry_id_for_seo,
        &entry_title_for_seo,
        &canonical_path_for_seo,
    )
    .await?;
    Ok(public_html(content_render::compose_document_with_seo(
        header.as_deref(),
        &body_html,
        footer.as_deref(),
        &seo_view,
    )?))
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
    let canonical_base = CanonicalUrlBuilder::taxonomy(kind, slug)
        .ok_or_else(|| ApiError::not_found("Không tìm thấy taxonomy."))?;
    if request_path != canonical_base {
        return Err(ApiError::not_found("Đường dẫn taxonomy không hợp lệ."));
    }
    let query =
        parse_query(raw_query, QueryScope::TaxonomyArchive(kind)).map_err(ApiError::bad_request)?;
    let offset = (query.page - 1) * ARCHIVE_SIZE;
    let (term, page) =
        public_content::taxonomy_public_page(db, kind, slug, query.filters, ARCHIVE_SIZE, offset)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy taxonomy."))?;
    let _context = PublicRequestContext::TaxonomyArchive {
        term_id: term.id,
        kind,
        slug: term.slug.clone(),
        query,
        canonical_path: CanonicalUrlBuilder::taxonomy_page(kind, &term.slug, query)
            .unwrap_or_default(),
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
            CanonicalUrlBuilder::taxonomy_page(
                kind,
                slug,
                PublicQuery {
                    page: query.page - 1,
                    ..query
                },
            )
        })
        .flatten();
    let next = (query.page < MAX_ARCHIVE_PAGE && offset + ARCHIVE_SIZE < page.total)
        .then(|| {
            CanonicalUrlBuilder::taxonomy_page(
                kind,
                slug,
                PublicQuery {
                    page: query.page + 1,
                    ..query
                },
            )
        })
        .flatten();
    let site = settings::get_general(db).await?;
    let theme_opts = settings::get_active_public_theme_options(db).await?;
    // Until S03 role composition, use the core archive template for mixed CPTs.
    // Site-owned content is resolved by its own actual schema above.
    let schema = ContentTypeDefinition {
        key: kind.as_str().to_owned(),
        label: term.name,
        fields: Vec::new(),
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
        CanonicalUrlBuilder::taxonomy(kind, slug).as_deref(),
        false,
    )
    .await?;
    Ok(public_html(content_render::compose_document_with_seo(
        header.as_deref(),
        &body_html,
        footer.as_deref(),
        &seo_view,
    )?))
}

async fn media_response(
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
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    media_response(&db, &store, &id, false).await
}

pub async fn media_thumb(
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    media_response(&db, &store, &id, true).await
}
