//! Content-addressed image file and thumbnail management.
//! Files are only read by canonical SHA-256 id and a server-trusted extension.

use std::{
    io::Cursor,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use sha2::{Digest, Sha256};
use tokio::fs;

use crate::{
    error::{ApiError, ApiResult},
    storage::media::{MediaAsset, valid_media_id},
};

pub const MAX_MEDIA_BYTES: usize = 8 * 1024 * 1024;
const MAX_DIMENSION: u32 = 6000;
const MAX_PIXELS: u64 = 20_000_000;

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

impl DecodedMedia {
    pub fn metadata(&self, filename: String) -> MediaAsset {
        MediaAsset {
            kind: "image".to_string(),
            duration_ms: None,
            video_codec: None,
            audio_codec: None,
            id: self.id.clone(),
            filename,
            mime_type: self.mime_type.into(),
            extension: self.extension.into(),
            byte_size: self.original.len() as i64,
            width: self.width as i64,
            height: self.height as i64,
            thumb_width: Some(self.thumb_width as i64),
            thumb_height: Some(self.thumb_height as i64),
            created_at: String::new(), // SQLite supplies timestamp.
        }
    }
}

/// Decode in a blocking worker. Checking dimensions and decoder allocation
/// limits before decoding avoids decompression bombs and huge thumbnail buffers.
pub fn decode_image(bytes: Vec<u8>, declared_type: &str) -> ApiResult<DecodedMedia> {
    if bytes.is_empty() || bytes.len() > MAX_MEDIA_BYTES {
        return Err(ApiError::bad_request("Ảnh trống hoặc vượt quá 8 MiB."));
    }
    let format = image::guess_format(&bytes)
        .map_err(|_| ApiError::bad_request("Không nhận diện được định dạng ảnh."))?;
    let (extension, mime) = match format {
        ImageFormat::Jpeg => ("jpg", "image/jpeg"),
        ImageFormat::Png => ("png", "image/png"),
        ImageFormat::WebP => ("webp", "image/webp"),
        _ => {
            return Err(ApiError::unsupported_media_type(
                "Chỉ hỗ trợ JPEG, PNG và WebP.",
            ));
        }
    };
    if declared_type != mime {
        return Err(ApiError::unsupported_media_type(
            "Content-Type không khớp với định dạng ảnh thực tế.",
        ));
    }
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image: DynamicImage = reader
        .decode()
        .map_err(|_| ApiError::bad_request("Ảnh bị lỗi, quá lớn hoặc không thể giải mã."))?;
    let (width, height) = (image.width(), image.height());
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(ApiError::bad_request("Kích thước ảnh vượt giới hạn."));
    }
    // Never enlarge a tiny upload merely to build its thumbnail.
    let thumb = image.thumbnail(width.min(320), height.min(320));
    let mut thumbnail = Cursor::new(Vec::new());
    thumb
        .write_to(&mut thumbnail, ImageFormat::Png)
        .map_err(|_| ApiError::internal("Không thể tạo ảnh thu nhỏ."))?;
    // Re-encode pixels in the original raster format. This drops EXIF/location
    // and arbitrary ancillary metadata rather than publishing the raw upload.
    let mut canonical = Cursor::new(Vec::new());
    image
        .write_to(&mut canonical, format)
        .map_err(|_| ApiError::bad_request("Không thể chuẩn hóa ảnh đã upload."))?;
    let original = canonical.into_inner();
    if original.is_empty() || original.len() > MAX_MEDIA_BYTES {
        return Err(ApiError::bad_request(
            "Ảnh sau khi xử lý vượt quá giới hạn 8 MiB.",
        ));
    }
    let id = format!("{:x}", Sha256::digest(&original));
    Ok(DecodedMedia {
        original,
        thumbnail: thumbnail.into_inner(),
        id,
        extension,
        mime_type: mime,
        width,
        height,
        thumb_width: thumb.width(),
        thumb_height: thumb.height(),
    })
}

#[derive(Debug, Clone)]
pub struct MediaStore {
    root: PathBuf,
}

impl MediaStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: root.join("media"),
        }
    }

    pub fn uploads_root(&self) -> PathBuf {
        self.root
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.root.clone())
    }

    fn path(&self, id: &str, extension: &str, thumbnail: bool) -> ApiResult<PathBuf> {
        let allowed_ext = if thumbnail {
            matches!(extension, "png" | "jpg" | "webp")
        } else {
            matches!(extension, "png" | "jpg" | "webp" | "mp4")
        };
        if !valid_media_id(id) || !allowed_ext {
            return Err(ApiError::bad_request("Mã ảnh không hợp lệ."));
        }
        let file = if thumbnail {
            format!("{id}.thumb.png")
        } else {
            format!("{id}.{extension}")
        };
        Ok(self.root.join(&id[..2]).join(file))
    }

    pub async fn persist(&self, image: &DecodedMedia) -> ApiResult<()> {
        let original = self.path(&image.id, image.extension, false)?;
        let thumb = self.path(&image.id, image.extension, true)?;
        fs::create_dir_all(original.parent().expect("media path has parent"))
            .await
            .map_err(|_| ApiError::internal("Không tạo được thư mục Media Library."))?;
        write_once(&original, &image.original).await?;
        write_once(&thumb, &image.thumbnail).await?;
        Ok(())
    }

    pub async fn read(&self, id: &str, ext: &str, thumb: bool) -> ApiResult<Vec<u8>> {
        let path = self.path(id, ext, thumb)?;
        fs::read(path)
            .await
            .map_err(|_| ApiError::internal("Không đọc được file ảnh đã lưu."))
    }

    pub fn canonical_path(&self, id: &str, extension: &str) -> ApiResult<PathBuf> {
        self.path(id, extension, false)
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.root.join(".staging")
    }

    pub async fn cleanup_staging(&self) {
        let dir = self.staging_dir();
        if let Ok(mut entries) = fs::read_dir(&dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let _ = fs::remove_file(entry.path()).await;
            }
        }
    }

    pub async fn create_video_staging_path(&self) -> ApiResult<PathBuf> {
        static NEXT_STAGING: AtomicU64 = AtomicU64::new(0);
        let dir = self.staging_dir();
        fs::create_dir_all(&dir)
            .await
            .map_err(|_| ApiError::internal("Không tạo được thư mục tạm cho video."))?;
        let seq = NEXT_STAGING.fetch_add(1, Ordering::Relaxed);
        Ok(dir.join(format!("video-{}-{seq}.part", std::process::id())))
    }

    pub async fn persist_video_from_staging(
        &self,
        staging: &std::path::Path,
        id: &str,
    ) -> ApiResult<()> {
        let target = self.path(id, "mp4", false)?;
        if fs::try_exists(&target).await.unwrap_or(false) {
            let _ = fs::remove_file(staging).await;
            return Ok(());
        }
        fs::create_dir_all(target.parent().expect("media path has parent"))
            .await
            .map_err(|_| ApiError::internal("Không tạo được thư mục Media Library."))?;
        if let Err(error) = fs::rename(staging, &target).await {
            let _ = fs::remove_file(staging).await;
            if error.kind() == std::io::ErrorKind::AlreadyExists
                && fs::try_exists(&target).await.unwrap_or(false)
            {
                return Ok(());
            }
            return Err(ApiError::internal("Không lưu được file video an toàn."));
        }
        Ok(())
    }
}


struct RangeFileBody {
    file: fs::File,
    remaining: u64,
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
        match tokio::io::AsyncRead::poll_read(std::pin::Pin::new(&mut self.file), cx, &mut read_buf) {
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
                std::task::Poll::Ready(Some(Ok(http_body::Frame::data(
                    axum::body::Bytes::from(buf),
                ))))
            }
        }
    }
}

fn etag_matches_if_none_match(header_val: &str, etag: &str) -> bool {
    let trimmed = header_val.trim();
    if trimmed == "*" {
        return true;
    }
    trimmed.split(',').any(|part| {
        let p = part.trim();
        p == etag || p.strip_prefix("W/") == Some(etag)
    })
}

fn parse_single_byte_range(raw: &str, file_size: u64) -> Result<(u64, u64), ()> {
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

/// Stream an already-authorized video asset with RFC 9110 Range, HEAD, and conditional ETag support.
pub async fn stream_authorized_video(
    method: &axum::http::Method,
    headers: &axum::http::HeaderMap,
    asset: &MediaAsset,
    path: &std::path::Path,
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
    let etag = format!("\"{}\"", asset.id);
    let cache_control = if is_admin { "private, no-store" } else { "no-store" };

    if let Some(inm) = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok())
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
        }),
    )
        .into_response())
}

async fn write_once(path: &std::path::Path, bytes: &[u8]) -> ApiResult<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_and_reencodes_three_raster_formats() {
        let image = DynamicImage::new_rgb8(3, 2);
        for (format, mime) in [
            (ImageFormat::Png, "image/png"),
            (ImageFormat::Jpeg, "image/jpeg"),
            (ImageFormat::WebP, "image/webp"),
        ] {
            let mut source = Cursor::new(Vec::new());
            image.write_to(&mut source, format).expect("encode fixture");
            let parsed = decode_image(source.into_inner(), mime).expect("media accepted");
            assert_eq!(parsed.mime_type, mime);
            assert_eq!((parsed.width, parsed.height), (3, 2));
            assert_eq!((parsed.thumb_width, parsed.thumb_height), (3, 2));
            assert_eq!(image::guess_format(&parsed.original).unwrap(), format);
            assert_eq!(
                image::guess_format(&parsed.thumbnail).unwrap(),
                ImageFormat::Png
            );
        }
    }
    #[test]
    fn rejects_html_and_fake_png() {
        assert!(
            decode_image(
                b"<svg><script>alert(1)</script></svg>".to_vec(),
                "image/svg+xml"
            )
            .is_err()
        );
        assert!(decode_image(b"not really an image".to_vec(), "image/png").is_err());
        assert!(decode_image(Vec::new(), "image/png").is_err());
    }
    #[test]
    fn canonical_path_never_accepts_traversal() {
        let store = MediaStore::new(std::env::temp_dir());
        assert!(store.path("../x", "png", false).is_err());
        assert!(store.path(&"a".repeat(64), "../png", false).is_err());
        assert!(
            store
                .path(&"a".repeat(64), "png", true)
                .unwrap()
                .ends_with(format!("{}.thumb.png", "a".repeat(64)))
        );
    }
}
