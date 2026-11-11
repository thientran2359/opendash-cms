//! Public CPT archive/single routes and referenced-only media delivery.
//! Legacy CPT Save stays live; core post/page draft data is never served here.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use opendash_core::{content::valid_content_type_key, id::valid_kebab_segment_with_limit};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    content_render::{self, SiteContext, entry_view},
    error::{ApiError, ApiResult},
    media::MediaStore,
    storage::{
        content::load_content_type_definition, entries::EntryFilters, media,
        page::active_content_templates, public_content, settings,
    },
};

const ARCHIVE_SIZE: i64 = 20;
const MAX_ARCHIVE_PAGE: i64 = 10_000;

#[derive(Deserialize)]
pub struct ArchiveQuery {
    page: Option<i64>,
    category_id: Option<i64>,
    tag_id: Option<i64>,
    include_descendants: Option<bool>,
}

fn archive_filters(query: &ArchiveQuery) -> ApiResult<EntryFilters> {
    if query.category_id.is_some_and(|id| id <= 0)
        || query.tag_id.is_some_and(|id| id <= 0)
        || (query.include_descendants.unwrap_or(false) && query.category_id.is_none())
    {
        return Err(ApiError::bad_request("Bộ lọc taxonomy không hợp lệ."));
    }
    Ok(EntryFilters {
        category_id: query.category_id,
        tag_id: query.tag_id,
        include_descendants: query.include_descendants.unwrap_or(false),
    })
}

/// Preserve taxonomy selections in the existing public CPT pagination links.
/// This is not a new public taxonomy URL or a Builder list-block endpoint.
fn archive_page_url(type_key: &str, page: i64, filters: EntryFilters) -> String {
    let mut url = format!("/content/{type_key}?page={page}");
    if let Some(id) = filters.category_id {
        url.push_str(&format!("&category_id={id}"));
        if filters.include_descendants {
            url.push_str("&include_descendants=true");
        }
    }
    if let Some(id) = filters.tag_id {
        url.push_str(&format!("&tag_id={id}"));
    }
    url
}

fn check_type_key(type_key: &str) -> ApiResult<()> {
    if valid_content_type_key(type_key) {
        Ok(())
    } else {
        Err(ApiError::not_found("Không tìm thấy loại nội dung."))
    }
}

fn public_html(html: String) -> Response {
    (
        [
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; script-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"),
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
    Path(type_key): Path<String>,
    Query(query): Query<ArchiveQuery>,
) -> ApiResult<Response> {
    check_type_key(&type_key)?;
    let page = query.page.unwrap_or(1);
    if !(1..=MAX_ARCHIVE_PAGE).contains(&page) {
        return Err(ApiError::bad_request("Trang nội dung không hợp lệ."));
    }
    let filters = archive_filters(&query)?;
    let schema = load_content_type_definition(&db, &type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let offset = (page - 1) * ARCHIVE_SIZE;
    let mut records =
        public_content::list_published(&db, &type_key, ARCHIVE_SIZE + 1, offset, filters).await?;
    let has_more = records.len() > ARCHIVE_SIZE as usize;
    records.truncate(ARCHIVE_SIZE as usize);
    let entries: Vec<_> = records
        .into_iter()
        .map(|record| entry_view(record, &schema))
        .collect();
    let previous = (page > 1).then(|| archive_page_url(&type_key, page - 1, filters));
    let next = has_more.then(|| archive_page_url(&type_key, page + 1, filters));
    let (list_template, _) = active_content_templates(&db).await?;
    let site = settings::get_general(&db).await?;
    let html = content_render::render_archive_with_site(
        &schema,
        &entries,
        previous.as_deref(),
        next.as_deref(),
        list_template.as_deref(),
        &SiteContext {
            title: &site.site_title,
            tagline: &site.tagline,
        },
    )?;
    Ok(public_html(html))
}

pub async fn single(
    State(db): State<SqlitePool>,
    Path((type_key, slug)): Path<(String, String)>,
) -> ApiResult<Response> {
    check_type_key(&type_key)?;
    if !valid_kebab_segment_with_limit(&slug, 64) {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    }
    let schema = load_content_type_definition(&db, &type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let entry = public_content::get_by_slug(&db, &type_key, &slug)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    let (_, single_template) = active_content_templates(&db).await?;
    let site = settings::get_general(&db).await?;
    let html = content_render::render_single_with_site(
        &schema,
        &entry_view(entry, &schema),
        single_template.as_deref(),
        &SiteContext {
            title: &site.site_title,
            tagline: &site.tagline,
        },
    )?;
    Ok(public_html(html))
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
