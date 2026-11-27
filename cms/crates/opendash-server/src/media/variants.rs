//! Deterministic responsive image variants and encode-only lossy WebP compression (`1A` + `2A`).

use image::codecs::jpeg::JpegEncoder;

use super::*;

#[derive(Debug, Clone)]
pub struct GeneratedVariant {
    pub spec: MediaVariantSpec,
    pub format: MediaVariantFormat,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn image_has_transparency(img: &DynamicImage) -> bool {
    if !img.color().has_alpha() {
        return false;
    }
    let rgba = img.to_rgba8();
    rgba.pixels().any(|px| px.0[3] < 255)
}

fn encode_jpeg_with_quality(img: &DynamicImage, quality: u8) -> ApiResult<Vec<u8>> {
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, quality);
    encoder
        .encode_image(&rgb)
        .map_err(|_| ApiError::internal("Không mã hóa được biến thể JPEG."))?;
    Ok(out)
}

fn encode_png(img: &DynamicImage) -> ApiResult<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    img.write_to(&mut cursor, ImageFormat::Png)
        .map_err(|_| ApiError::internal("Không mã hóa được biến thể PNG."))?;
    Ok(cursor.into_inner())
}

/// Encodes already-decoded & validated pixels using `webp =0.3.1` strictly as an
/// encode-only backend, applying alpha C-heap bounds and strict `webp < fallback` gate.
pub(crate) fn encode_webp_size_gated(
    img: &DynamicImage,
    source_mime: &str,
    has_alpha: bool,
    fallback_len: Option<usize>,
) -> Option<Vec<u8>> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }

    let lossy_quality = if source_mime.eq_ignore_ascii_case("image/png") {
        85.0_f32
    } else {
        82.0_f32
    };

    let candidate: Option<Vec<u8>> = if has_alpha {
        let rgba = img.to_rgba8();
        let encoder = webp::Encoder::from_rgba(rgba.as_raw(), w, h);
        let pixels = u64::from(w) * u64::from(h);
        if pixels <= MAX_ALPHA_LOSSLESS_PIXELS {
            let lossless = encoder
                .encode_simple(true, 75.0)
                .ok()
                .map(|mem| mem.to_vec());
            let passes_gate = lossless
                .as_ref()
                .zip(fallback_len)
                .is_some_and(|(bytes, limit)| bytes.len() < limit);
            if passes_gate || (lossless.is_some() && fallback_len.is_none()) {
                lossless
            } else {
                encoder
                    .encode_simple(false, 85.0)
                    .ok()
                    .map(|mem| mem.to_vec())
            }
        } else {
            encoder
                .encode_simple(false, 85.0)
                .ok()
                .map(|mem| mem.to_vec())
        }
    } else {
        let rgb = img.to_rgb8();
        let encoder = webp::Encoder::from_rgb(rgb.as_raw(), w, h);
        encoder
            .encode_simple(false, lossy_quality)
            .ok()
            .map(|mem| mem.to_vec())
    };

    let bytes = candidate?;
    if bytes.is_empty() {
        return None;
    }
    if let Some(limit) = fallback_len
        && bytes.len() >= limit
    {
        return None;
    }
    Some(bytes)
}

fn push_variant(
    out: &mut Vec<GeneratedVariant>,
    spec: MediaVariantSpec,
    format: MediaVariantFormat,
    width: u32,
    height: u32,
    bytes: Vec<u8>,
) {
    let sha256 = sha256_hex(&bytes);
    out.push(GeneratedVariant {
        spec,
        format,
        width,
        height,
        bytes,
        sha256,
    });
}

/// Generates all non-upscaling, size-gated responsive variants for a decoded image.
pub fn generate_variants_for_decoded(
    img: &DynamicImage,
    source_mime: &str,
    canonical_len: usize,
    thumb_img: &DynamicImage,
    thumb_png_len: usize,
) -> ApiResult<Vec<GeneratedVariant>> {
    let (orig_w, orig_h) = img.dimensions();
    let has_alpha = image_has_transparency(img);
    let mut variants = Vec::new();

    // Thumbnail: only optional `thumbnail.webp` if strictly smaller than canonical `.thumb.png`.
    let thumb_alpha = image_has_transparency(thumb_img);
    if let Some(thumb_webp) =
        encode_webp_size_gated(thumb_img, source_mime, thumb_alpha, Some(thumb_png_len))
    {
        push_variant(
            &mut variants,
            MediaVariantSpec::Thumbnail,
            MediaVariantFormat::Webp,
            thumb_img.width(),
            thumb_img.height(),
            thumb_webp,
        );
    }

    // Responsive non-upscaling widths: medium (768w), large (1536w).
    for (spec, threshold) in [
        (MediaVariantSpec::Medium, MEDIUM_MAX_WIDTH),
        (MediaVariantSpec::Large, LARGE_MAX_WIDTH),
    ] {
        if orig_w <= threshold {
            continue;
        }
        let (target_w, target_h) = scaled_dimensions_for_width(orig_w, orig_h, threshold);
        let resized = img.resize_exact(target_w, target_h, imageops::FilterType::CatmullRom);
        let resized_alpha = has_alpha && image_has_transparency(&resized);

        if source_mime.eq_ignore_ascii_case("image/webp") {
            let webp_limit = Some(canonical_len);
            if let Some(webp_bytes) =
                encode_webp_size_gated(&resized, source_mime, resized_alpha, webp_limit)
            {
                push_variant(
                    &mut variants,
                    spec,
                    MediaVariantFormat::Webp,
                    target_w,
                    target_h,
                    webp_bytes,
                );
            }
            continue;
        }

        let (fallback_format, fallback_bytes) = if source_mime.eq_ignore_ascii_case("image/jpeg") {
            (
                MediaVariantFormat::Jpeg,
                encode_jpeg_with_quality(&resized, 82)?,
            )
        } else if resized_alpha {
            (MediaVariantFormat::Png, encode_png(&resized)?)
        } else {
            let jpg = encode_jpeg_with_quality(&resized, 85)?;
            let png = encode_png(&resized)?;
            if jpg.len() < png.len() {
                (MediaVariantFormat::Jpeg, jpg)
            } else {
                (MediaVariantFormat::Png, png)
            }
        };

        let fallback_len = fallback_bytes.len();
        push_variant(
            &mut variants,
            spec,
            fallback_format,
            target_w,
            target_h,
            fallback_bytes,
        );

        if let Some(webp_bytes) =
            encode_webp_size_gated(&resized, source_mime, resized_alpha, Some(fallback_len))
        {
            push_variant(
                &mut variants,
                spec,
                MediaVariantFormat::Webp,
                target_w,
                target_h,
                webp_bytes,
            );
        }
    }

    // Full WebP optimization only for non-WebP sources when strictly smaller than canonical bytes.
    if !source_mime.eq_ignore_ascii_case("image/webp")
        && let Some(full_webp) =
            encode_webp_size_gated(img, source_mime, has_alpha, Some(canonical_len))
    {
        push_variant(
            &mut variants,
            MediaVariantSpec::Full,
            MediaVariantFormat::Webp,
            orig_w,
            orig_h,
            full_webp,
        );
    }

    Ok(variants)
}

/// Atomically writes `bytes` to `path` via a same-directory temp file + `sync_all` + `rename`,
/// overwriting any existing target safely.
pub async fn write_atomic_overwrite(path: &Path, bytes: &[u8]) -> ApiResult<()> {
    use tokio::io::AsyncWriteExt;
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|_| ApiError::internal("Không tạo được thư mục chứa biến thể media."))?;
    }
    let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let staging = path.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)
        .await
        .map_err(|_| ApiError::internal("Không mở được tệp tạm cho media."))?;
    let write_result = async {
        file.write_all(bytes).await?;
        file.sync_all().await
    }
    .await;
    drop(file);
    let rename_result = match write_result {
        Ok(()) => fs::rename(&staging, path).await,
        Err(error) => Err(error),
    };
    if rename_result.is_err() {
        let _ = fs::remove_file(&staging).await;
        return Err(ApiError::internal("Không ghi đè được tệp media an toàn."));
    }
    Ok(())
}
