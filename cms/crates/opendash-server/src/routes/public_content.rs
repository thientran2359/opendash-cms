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
        content::load_content_type_definition, media, page::active_content_templates,
        public_content, settings,
    },
};

const ARCHIVE_SIZE: i64 = 20;
const MAX_ARCHIVE_PAGE: i64 = 10_000;

#[derive(Deserialize)]
pub struct ArchiveQuery {
    page: Option<i64>,
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
    let schema = load_content_type_definition(&db, &type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let offset = (page - 1) * ARCHIVE_SIZE;
    let mut records =
        public_content::list_published(&db, &type_key, ARCHIVE_SIZE + 1, offset).await?;
    let has_more = records.len() > ARCHIVE_SIZE as usize;
    records.truncate(ARCHIVE_SIZE as usize);
    let entries: Vec<_> = records
        .into_iter()
        .map(|record| entry_view(record, &schema))
        .collect();
    let previous = (page > 1).then(|| format!("/content/{type_key}?page={}", page - 1));
    let next = has_more.then(|| format!("/content/{type_key}?page={}", page + 1));
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
