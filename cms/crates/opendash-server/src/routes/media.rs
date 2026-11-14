//! Authenticated media upload, metadata listing and safe image/video serving.

use std::{path::PathBuf, pin::Pin, sync::Arc};

use axum::{
    Json,
    body::{Body, Bytes, HttpBody},
    extract::{Path, Query, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio::{io::AsyncWriteExt, sync::Semaphore};

use crate::{
    auth::{require_session, require_session_with_csrf},
    error::{ApiError, ApiResult},
    media::{MAX_MEDIA_BYTES, MediaStore, decode_image},
    media_video::{MAX_VIDEO_BYTES, inspect_mp4},
    storage::media::{self as media_storage, MediaAsset},
};

#[derive(Debug, Deserialize, Default)]
pub struct MediaPagination {
    limit: Option<i64>,
    offset: Option<i64>,
    kind: Option<String>,
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
            "Phân trang thư viện media không hợp lệ.",
        ));
    }
    let kind = match pagination.kind.as_deref() {
        None | Some("") | Some("all") => None,
        Some("image") => Some("image"),
        Some("video") => Some("video"),
        Some(_) => return Err(ApiError::bad_request("Bộ lọc loại media không hợp lệ.")),
    };
    Ok(Json(media_storage::list_filtered(&db, kind, limit, offset).await?))
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

struct StagingCleanupGuard(Option<PathBuf>);

impl Drop for StagingCleanupGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub async fn media_video_upload(
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    headers: HeaderMap,
    mut body: Body,
) -> ApiResult<Response> {
    require_session_with_csrf(&headers, &db).await?;
    static VIDEO_INGEST_SLOTS: Semaphore = Semaphore::const_new(2);
    let _slot = VIDEO_INGEST_SLOTS
        .try_acquire()
        .map_err(|_| ApiError::too_many_requests("Đang có quá nhiều lượt upload video đồng thời."))?;

    let declared_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|s| s.split(';').next())
        .map(str::trim)
        .unwrap_or("");
    if !declared_type.eq_ignore_ascii_case("video/mp4") {
        return Err(ApiError::unsupported_media_type(
            "Chỉ hỗ trợ upload video định dạng video/mp4.",
        ));
    }

    if let Some(len) = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
        && (len == 0 || len > MAX_VIDEO_BYTES)
    {
        return Err(ApiError::bad_request(
            "Video trống hoặc vượt quá giới hạn 100 MiB.",
        ));
    }

    let staging_path = store.create_video_staging_path().await?;
    let mut guard = StagingCleanupGuard(Some(staging_path.clone()));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging_path)
        .await
        .map_err(|_| ApiError::internal("Không tạo được file tạm cho video."))?;

    let mut hasher = Sha256::new();
    let mut total_bytes: u64 = 0;

    while let Some(frame_res) =
        std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await
    {
        let frame = frame_res
            .map_err(|_| ApiError::bad_request("Luồng upload video bị gián đoạn hoặc vượt giới hạn."))?;
        if let Some(chunk) = frame.data_ref() {
            let next_total = total_bytes
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| ApiError::bad_request("Kích thước video vượt giới hạn."))?;
            if next_total > MAX_VIDEO_BYTES as u64 {
                return Err(ApiError::bad_request("Video vượt quá giới hạn 100 MiB."));
            }
            hasher.update(chunk);
            file.write_all(chunk)
                .await
                .map_err(|_| ApiError::internal("Không ghi được dữ liệu video tạm."))?;
            total_bytes = next_total;
        }
    }

    if total_bytes == 0 {
        return Err(ApiError::bad_request("Video trống (0 byte)."));
    }
    file.sync_all()
        .await
        .map_err(|_| ApiError::internal("Không đồng bộ được file video tạm."))?;
    drop(file);

    let sha256 = format!("{:x}", hasher.finalize());
    let inspect_path = staging_path.clone();
    let validated = tokio::task::spawn_blocking(move || inspect_mp4(&inspect_path, total_bytes, sha256))
        .await
        .map_err(|_| ApiError::internal("Không thể kiểm tra metadata video."))??;

    let filename = display_filename(&headers, "mp4");
    store
        .persist_video_from_staging(&staging_path, &validated.id)
        .await?;
    guard.0 = None;
    let metadata = media_storage::save(&db, &validated.metadata(filename)).await?;
    Ok((StatusCode::CREATED, Json(metadata)).into_response())
}

async fn image_response(
    db: &SqlitePool,
    store: &MediaStore,
    method: &Method,
    headers: &HeaderMap,
    id: &str,
    thumb: bool,
) -> ApiResult<Response> {
    require_session(headers, db).await?;
    if !media_storage::valid_media_id(id) {
        return Err(ApiError::bad_request("Mã media không hợp lệ."));
    }
    let asset = media_storage::find(db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Media không tồn tại."))?;
    if thumb && asset.kind == "video" {
        return Err(ApiError::not_found("Video không có thumbnail."));
    }
    if !thumb && asset.kind == "video" {
        let path = store.canonical_path(id, &asset.extension)?;
        return crate::media::stream_authorized_video(method, headers, &asset, &path, true).await;
    }
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
    method: Method,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    image_response(&db, &store, &method, &headers, &id, false).await
}

pub async fn media_thumbnail(
    State(db): State<SqlitePool>,
    State(store): State<Arc<MediaStore>>,
    method: Method,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    image_response(&db, &store, &method, &headers, &id, true).await
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
