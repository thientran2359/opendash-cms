//! Authenticated media upload, metadata listing and safe image serving.

use std::sync::Arc;

use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::SqlitePool;
use tokio::sync::Semaphore;

use crate::{
    auth::{require_session, require_session_with_csrf},
    error::{ApiError, ApiResult},
    media::{MAX_MEDIA_BYTES, MediaStore, decode_image},
    storage::media::{self as media_storage, MediaAsset},
};

#[derive(Debug, Deserialize, Default)]
pub struct MediaPagination {
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn media_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(pagination): Query<MediaPagination>,
) -> ApiResult<Json<Vec<MediaAsset>>> {
    require_session(&headers, &db).await?;
    let limit = pagination.limit.unwrap_or(50);
    let offset = pagination.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) || !(0..=1_000_000).contains(&offset) {
        return Err(ApiError::bad_request(
            "Phân trang thư viện ảnh không hợp lệ.",
        ));
    }
    Ok(Json(media_storage::list(&db, limit, offset).await?))
}

fn display_filename(headers: &HeaderMap, extension: &str) -> String {
    let fallback = format!("upload.{extension}");
    let Some(raw) = headers
        .get("x-file-name")
        .and_then(|header| header.to_str().ok())
    else {
        return fallback;
    };
    // Client encodes original file name as encodeURIComponent(file.name).
    let encoded = format!("filename={raw}");
    let decoded = url::form_urlencoded::parse(encoded.as_bytes())
        .find(|(key, _)| key == "filename")
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default();
    let clean: String = decoded
        .chars()
        .filter(|ch| !ch.is_control() && *ch != '/' && *ch != '\\')
        .take(120)
        .collect();
    if clean.trim().is_empty() {
        fallback
    } else {
        clean
    }
}

pub async fn media_upload(
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    if bytes.is_empty() || bytes.len() > MAX_MEDIA_BYTES {
        return Err(ApiError::bad_request("Ảnh trống hoặc vượt quá 8 MiB."));
    }
    let declared_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    static DECODE_SLOTS: Semaphore = Semaphore::const_new(2);
    let _slot = DECODE_SLOTS
        .acquire()
        .await
        .map_err(|_| ApiError::internal("Bộ xử lý ảnh không khả dụng."))?;
    let decoded = tokio::task::spawn_blocking(move || decode_image(bytes.to_vec(), &declared_type))
        .await
        .map_err(|_| ApiError::internal("Không thể xử lý ảnh."))??;
    let filename = display_filename(&headers, decoded.extension);
    store.persist(&decoded).await?;
    let metadata = media_storage::save(&db, &decoded.metadata(filename)).await?;
    Ok((StatusCode::CREATED, Json(metadata)).into_response())
}

async fn image_response(
    db: &SqlitePool,
    store: &MediaStore,
    headers: &HeaderMap,
    id: &str,
    thumb: bool,
) -> ApiResult<Response> {
    require_session(headers, db).await?;
    if !media_storage::valid_media_id(id) {
        return Err(ApiError::bad_request("Mã ảnh không hợp lệ."));
    }
    let asset = media_storage::find(db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Ảnh không tồn tại."))?;
    let bytes = store.read(id, &asset.extension, thumb).await?;
    let mime = if thumb {
        "image/png"
    } else {
        asset.mime_type.as_str()
    };
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "private, no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; sandbox",
            ),
        ],
        bytes,
    )
        .into_response())
}

pub async fn media_original(
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    image_response(&db, &store, &headers, &id, false).await
}

pub async fn media_thumbnail(
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    image_response(&db, &store, &headers, &id, true).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_path_segments_from_filename() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-file-name",
            "a%2F..%5Csecret%3Cimg%3E.png".parse().unwrap(),
        );
        assert_eq!(display_filename(&headers, "png"), "a..secret<img>.png");
    }
}
