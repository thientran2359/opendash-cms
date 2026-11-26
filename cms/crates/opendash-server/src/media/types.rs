//! Content-addressed image file and thumbnail management.
//! Files are only read by canonical SHA-256 id and a server-trusted extension.

use super::*;

pub(super) use std::{
    io::Cursor,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) use image::{DynamicImage, ImageFormat, ImageReader, Limits};

pub(super) use sha2::{Digest, Sha256};

pub(super) use tokio::fs;

pub(super) use crate::{
    error::{ApiError, ApiResult},
    storage::media::{MediaAsset, valid_media_id},
};

pub const MAX_MEDIA_BYTES: usize = 8 * 1024 * 1024;

pub(super) const MAX_DIMENSION: u32 = 6000;

pub(super) const MAX_PIXELS: u64 = 20_000_000;

#[derive(Debug)]
pub struct DecodedMedia {
    pub original: Vec<u8>,
    pub thumbnail: Vec<u8>,
    pub id: String,
    pub extension: &'static str,
    pub mime_type: &'static str,
    pub width: u32,
    pub height: u32,
    pub thumb_width: u32,
    pub thumb_height: u32,
}

#[derive(Debug, Default)]
pub(super) struct MediaReadState {
    pub(super) active_readers: std::collections::HashMap<String, usize>,
    pub(super) deleting: std::collections::HashSet<String>,
}

#[derive(Debug, Default)]
pub struct MediaReadGate {
    pub(super) state: std::sync::Mutex<MediaReadState>,
    pub(super) notify: tokio::sync::Notify,
}

pub struct MediaReadLease {
    pub(super) gate: std::sync::Arc<MediaReadGate>,
    pub(super) media_id: String,
}

pub struct MediaDeleteGateGuard {
    pub(super) gate: std::sync::Arc<MediaReadGate>,
    pub(super) media_id: String,
}

#[derive(Debug, Clone)]
pub struct MediaStore {
    pub(super) root: PathBuf,
    pub(super) read_gate: std::sync::Arc<MediaReadGate>,
}

pub(super) struct RangeFileBody {
    pub(super) file: fs::File,
    pub(super) remaining: u64,
    pub(super) _read_lease: MediaReadLease,
}

/// Stream an already-authorized video asset with RFC 9110 Range, HEAD, and conditional ETag support.
pub async fn stream_authorized_video(
    method: &axum::http::Method,
    headers: &axum::http::HeaderMap,
    asset: &MediaAsset,
    path: &std::path::Path,
    media_store: &MediaStore,
    is_admin: bool,
) -> ApiResult<axum::response::Response> {
    use axum::{
        body::Body,
        http::{StatusCode, header},
        response::IntoResponse,
    };
    use tokio::io::AsyncSeekExt;

    let read_lease = media_store.acquire_read_lease(&asset.id)?;
    let mut file = fs::File::open(path)
        .await
        .map_err(|_| ApiError::not_found("Không tìm thấy file video."))?;
    let meta = file
        .metadata()
        .await
        .map_err(|_| ApiError::not_found("Không đọc được thông tin file video."))?;
    let file_size = meta.len();
    let etag = format!("\"{}\"", asset.id);
    let cache_control = if is_admin {
        "private, no-store"
    } else {
        "no-store"
    };

    if let Some(inm) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        && etag_matches_if_none_match(inm, &etag)
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag.as_str()),
                (header::ACCEPT_RANGES, "bytes"),
                (header::CACHE_CONTROL, cache_control),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (
                    header::CONTENT_SECURITY_POLICY,
                    "default-src 'none'; sandbox",
                ),
            ],
            Body::empty(),
        )
            .into_response());
    }

    if *method == axum::http::Method::HEAD {
        let len_str = file_size.to_string();
        return Ok((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, asset.mime_type.as_str()),
                (header::CONTENT_LENGTH, len_str.as_str()),
                (header::ACCEPT_RANGES, "bytes"),
                (header::ETAG, etag.as_str()),
                (header::CACHE_CONTROL, cache_control),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (
                    header::CONTENT_SECURITY_POLICY,
                    "default-src 'none'; sandbox",
                ),
            ],
            Body::empty(),
        )
            .into_response());
    }

    let if_range_allows = match headers.get(header::IF_RANGE).and_then(|v| v.to_str().ok()) {
        None => true,
        Some(v) => v.trim() == etag,
    };

    if if_range_allows
        && let Some(range_hdr) = headers.get(header::RANGE).and_then(|v| v.to_str().ok())
    {
        match parse_single_byte_range(range_hdr, file_size) {
            Err(()) => {
                let cr = format!("bytes */{file_size}");
                return Ok((
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    [
                        (header::CONTENT_RANGE, cr.as_str()),
                        (header::ACCEPT_RANGES, "bytes"),
                        (header::ETAG, etag.as_str()),
                        (header::CACHE_CONTROL, cache_control),
                        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                    ],
                    Body::empty(),
                )
                    .into_response());
            }
            Ok((start, end)) => {
                file.seek(std::io::SeekFrom::Start(start))
                    .await
                    .map_err(|_| ApiError::internal("Không seek được file video."))?;
                let len = end - start + 1;
                let len_str = len.to_string();
                let cr = format!("bytes {start}-{end}/{file_size}");
                return Ok((
                    StatusCode::PARTIAL_CONTENT,
                    [
                        (header::CONTENT_TYPE, asset.mime_type.as_str()),
                        (header::CONTENT_LENGTH, len_str.as_str()),
                        (header::CONTENT_RANGE, cr.as_str()),
                        (header::ACCEPT_RANGES, "bytes"),
                        (header::ETAG, etag.as_str()),
                        (header::CACHE_CONTROL, cache_control),
                        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                        (
                            header::CONTENT_SECURITY_POLICY,
                            "default-src 'none'; sandbox",
                        ),
                    ],
                    Body::new(RangeFileBody {
                        file,
                        remaining: len,
                        _read_lease: read_lease,
                    }),
                )
                    .into_response());
            }
        }
    }

    let len_str = file_size.to_string();
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, asset.mime_type.as_str()),
            (header::CONTENT_LENGTH, len_str.as_str()),
            (header::ACCEPT_RANGES, "bytes"),
            (header::ETAG, etag.as_str()),
            (header::CACHE_CONTROL, cache_control),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; sandbox",
            ),
        ],
        Body::new(RangeFileBody {
            file,
            remaining: file_size,
            _read_lease: read_lease,
        }),
    )
        .into_response())
}

pub(super) async fn write_once(path: &std::path::Path, bytes: &[u8]) -> ApiResult<()> {
    use tokio::io::AsyncWriteExt;
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    if fs::try_exists(path).await.unwrap_or(false) {
        return Ok(());
    }
    // A unique same-directory staging file protects readers from partially
    // written images. Rename is atomic on the same volume.
    let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let staging = path.with_extension(format!("part-{}-{sequence}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)
        .await
        .map_err(|_| ApiError::internal("Không mở được file tạm cho media."))?;
    let write_result = async {
        file.write_all(bytes).await?;
        file.sync_all().await
    }
    .await;
    drop(file); // Windows cannot reliably rename a file still held open.
    let written = match write_result {
        Ok(()) => fs::rename(&staging, path).await,
        Err(error) => Err(error),
    };
    if let Err(error) = written {
        let _ = fs::remove_file(&staging).await;
        if error.kind() == std::io::ErrorKind::AlreadyExists
            && fs::try_exists(path).await.unwrap_or(false)
        {
            return Ok(());
        }
        return Err(ApiError::internal("Không ghi được file media an toàn."));
    }
    Ok(())
}
