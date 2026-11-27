//! HTTP image, variant, thumbnail, and RFC 9110 Range video delivery over held `MediaReadLease`s.

use super::*;

pub enum LeasedImageTarget<'a> {
    Original(&'a MediaAsset),
    Thumbnail(&'a MediaAsset),
    Variant {
        asset: &'a MediaAsset,
        spec: MediaVariantSpec,
        format: MediaVariantFormat,
    },
}

/// Serves an already read-leased image, thumbnail, or variant with `If-None-Match` (`304`),
/// metadata-only `HEAD`, `no-cache` revalidation headers, and strict `sandbox` CSP.
pub async fn serve_leased_image(
    store: &MediaStore,
    read_lease: &MediaReadLease,
    method: &axum::http::Method,
    headers: &axum::http::HeaderMap,
    target: LeasedImageTarget<'_>,
    is_public: bool,
) -> ApiResult<axum::response::Response> {
    use axum::{
        body::Body,
        http::{Method, StatusCode, header},
        response::Response,
    };

    let (mime, etag) = match target {
        LeasedImageTarget::Original(asset) => (
            asset.mime_type.as_str(),
            opendash_core::media::format_media_http_etag(asset.generation, asset.version, None),
        ),
        LeasedImageTarget::Thumbnail(asset) => (
            "image/png",
            opendash_core::media::format_media_http_etag(
                asset.generation,
                asset.version,
                Some("thumb"),
            ),
        ),
        LeasedImageTarget::Variant {
            asset,
            spec,
            format,
        } => {
            let suffix = format!("{}-{}", spec.as_str(), format.as_str());
            (
                format.mime_type(),
                opendash_core::media::format_media_http_etag(
                    asset.generation,
                    asset.version,
                    Some(&suffix),
                ),
            )
        }
    };

    let cache_control = if is_public {
        "public, no-cache"
    } else {
        "private, no-cache"
    };

    let with_common_headers = |mut builder: axum::http::response::Builder| {
        builder = builder
            .header(header::ETAG, etag.as_str())
            .header(header::CACHE_CONTROL, cache_control)
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .header(
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; sandbox",
            );
        if is_public {
            builder = builder.header(header::REFERRER_POLICY, "no-referrer");
        }
        builder
    };

    if let Some(inm) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        && etag_matches_if_none_match(inm, &etag)
    {
        return with_common_headers(Response::builder().status(StatusCode::NOT_MODIFIED))
            .body(Body::empty())
            .map_err(|_| ApiError::internal("Không tạo được phản hồi 304 media."));
    }

    if *method == Method::HEAD {
        let len = match target {
            LeasedImageTarget::Original(asset) => {
                store
                    .metadata_with_lease(read_lease, &asset.id, &asset.extension, false)
                    .await?
            }
            LeasedImageTarget::Thumbnail(asset) => {
                store
                    .metadata_with_lease(read_lease, &asset.id, &asset.extension, true)
                    .await?
            }
            LeasedImageTarget::Variant {
                asset,
                spec,
                format,
            } => {
                store
                    .variant_metadata_with_lease(read_lease, &asset.id, asset.version, spec, format)
                    .await?
            }
        };
        return with_common_headers(
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .header(header::CONTENT_LENGTH, len),
        )
        .body(Body::empty())
        .map_err(|_| ApiError::internal("Không tạo được phản hồi HEAD media."));
    }

    let bytes = match target {
        LeasedImageTarget::Original(asset) => {
            store
                .read_with_lease(read_lease, &asset.id, &asset.extension, false)
                .await?
        }
        LeasedImageTarget::Thumbnail(asset) => {
            store
                .read_with_lease(read_lease, &asset.id, &asset.extension, true)
                .await?
        }
        LeasedImageTarget::Variant {
            asset,
            spec,
            format,
        } => {
            store
                .read_variant_with_lease(read_lease, &asset.id, asset.version, spec, format)
                .await?
        }
    };

    with_common_headers(
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CONTENT_LENGTH, bytes.len()),
    )
    .body(Body::from(bytes))
    .map_err(|_| ApiError::internal("Không tạo được phản hồi media."))
}

pub(super) struct RangeFileBody {
    pub(super) file: fs::File,
    pub(super) remaining: u64,
    pub(super) _read_lease: MediaReadLease,
}

impl axum::body::HttpBody for RangeFileBody {
    type Data = axum::body::Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if self.remaining == 0 {
            return std::task::Poll::Ready(None);
        }
        let max_read = self.remaining.min(64 * 1024) as usize;
        let mut buf = vec![0u8; max_read];
        let mut read_buf = tokio::io::ReadBuf::new(&mut buf);
        match tokio::io::AsyncRead::poll_read(std::pin::Pin::new(&mut self.file), cx, &mut read_buf)
        {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Some(Err(e))),
            std::task::Poll::Ready(Ok(())) => {
                let n = read_buf.filled().len();
                if n == 0 {
                    self.remaining = 0;
                    return std::task::Poll::Ready(None);
                }
                self.remaining -= n as u64;
                buf.truncate(n);
                std::task::Poll::Ready(Some(Ok(http_body::Frame::data(axum::body::Bytes::from(
                    buf,
                )))))
            }
        }
    }
}

/// Stream an already-authorized video asset with RFC 9110 Range, HEAD, and conditional ETag support.
pub async fn stream_authorized_video(
    method: &axum::http::Method,
    headers: &axum::http::HeaderMap,
    asset: &MediaAsset,
    path: &Path,
    read_lease: MediaReadLease,
    is_admin: bool,
) -> ApiResult<axum::response::Response> {
    use axum::{
        body::Body,
        http::{StatusCode, header},
        response::IntoResponse,
    };
    use tokio::io::AsyncSeekExt;

    let mut file = fs::File::open(path)
        .await
        .map_err(|_| ApiError::not_found("Không tìm thấy file video."))?;
    let meta = file
        .metadata()
        .await
        .map_err(|_| ApiError::not_found("Không đọc được thông tin file video."))?;
    let file_size = meta.len();
    let etag = format!("\"{}\"", asset.metadata_etag());
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

pub(crate) fn etag_matches_if_none_match(header_val: &str, etag: &str) -> bool {
    let trimmed = header_val.trim();
    if trimmed == "*" {
        return true;
    }
    trimmed.split(',').any(|part| {
        let p = part.trim();
        p == etag || p.strip_prefix("W/") == Some(etag)
    })
}

pub(super) fn parse_single_byte_range(raw: &str, file_size: u64) -> Result<(u64, u64), ()> {
    let spec = raw.trim().strip_prefix("bytes=").ok_or(())?;
    if spec.contains(',') || file_size == 0 {
        return Err(());
    }
    let (start_s, end_s) = spec.split_once('-').ok_or(())?;
    let start_s = start_s.trim();
    let end_s = end_s.trim();
    if start_s.is_empty() {
        if end_s.is_empty() || !end_s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(());
        }
        let suffix: u64 = end_s.parse().map_err(|_| ())?;
        if suffix == 0 {
            return Err(());
        }
        let start = file_size.saturating_sub(suffix);
        Ok((start, file_size - 1))
    } else {
        if !start_s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(());
        }
        let start: u64 = start_s.parse().map_err(|_| ())?;
        if start >= file_size {
            return Err(());
        }
        if end_s.is_empty() {
            Ok((start, file_size - 1))
        } else {
            if !end_s.bytes().all(|b| b.is_ascii_digit()) {
                return Err(());
            }
            let end: u64 = end_s.parse().map_err(|_| ())?;
            if start > end {
                return Err(());
            }
            Ok((start, end.min(file_size - 1)))
        }
    }
}
