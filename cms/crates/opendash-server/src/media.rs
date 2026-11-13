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
            id: self.id.clone(),
            filename,
            mime_type: self.mime_type.into(),
            extension: self.extension.into(),
            byte_size: self.original.len() as i64,
            width: self.width as i64,
            height: self.height as i64,
            thumb_width: self.thumb_width as i64,
            thumb_height: self.thumb_height as i64,
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
        if !valid_media_id(id) || !matches!(extension, "png" | "jpg" | "webp") {
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
