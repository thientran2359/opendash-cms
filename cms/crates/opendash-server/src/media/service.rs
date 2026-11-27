//! Image decode/canonicalize pipeline and leased `MediaStore` filesystem operations.

use super::*;

impl DecodedMedia {
    pub fn metadata(&self, filename: String) -> MediaAsset {
        MediaAsset {
            kind: "image".to_string(),
            duration_ms: None,
            video_codec: None,
            audio_codec: None,
            id: self.id.clone(),
            content_sha256: self.id.clone(),
            filename,
            mime_type: self.mime_type.into(),
            extension: self.extension.into(),
            byte_size: self.original.len() as i64,
            width: self.width as i64,
            height: self.height as i64,
            thumb_width: Some(self.thumb_width as i64),
            thumb_height: Some(self.thumb_height as i64),
            lifecycle_state: "active".to_string(),
            generation: 1,
            version: 1,
            variants_ready_version: 1,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

/// Validates container format, MIME match, dimensions, and allocation bounds before pixel decode.
pub(crate) fn decode_validated_image(
    bytes: &[u8],
    declared_type: &str,
) -> ApiResult<(DynamicImage, ImageFormat, &'static str, &'static str)> {
    if bytes.is_empty() || bytes.len() > MAX_MEDIA_BYTES {
        return Err(ApiError::bad_request("Ảnh trống hoặc vượt quá 8 MiB."));
    }
    let format = image::guess_format(bytes)
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
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
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
    Ok((image, format, extension, mime))
}

/// Decode in a blocking worker. Checking dimensions and decoder allocation limits
/// before decoding avoids decompression bombs and huge thumbnail buffers.
pub fn decode_image(bytes: Vec<u8>, declared_type: &str) -> ApiResult<DecodedMedia> {
    let (image, format, extension, mime) = decode_validated_image(&bytes, declared_type)?;
    let (width, height) = (image.width(), image.height());
    // Never enlarge a tiny upload merely to build its thumbnail.
    let thumb = image.thumbnail(width.min(320), height.min(320));
    let mut thumbnail_cur = Cursor::new(Vec::new());
    thumb
        .write_to(&mut thumbnail_cur, ImageFormat::Png)
        .map_err(|_| ApiError::internal("Không thể tạo ảnh thu nhỏ."))?;
    let thumbnail = thumbnail_cur.into_inner();
    // Re-encode pixels in the original raster format to strip EXIF/metadata.
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
    let variants =
        generate_variants_for_decoded(&image, mime, original.len(), &thumb, thumbnail.len())?;

    Ok(DecodedMedia {
        original,
        thumbnail,
        id,
        extension,
        mime_type: mime,
        width,
        height,
        thumb_width: thumb.width(),
        thumb_height: thumb.height(),
        variants,
    })
}

impl MediaStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: root.join("media"),
            read_gate: Arc::new(MediaReadGate::default()),
        }
    }

    #[allow(dead_code)]
    pub fn media_root(&self) -> &Path {
        &self.root
    }

    pub fn quarantine_job_dir(&self, job_id: &str) -> PathBuf {
        self.uploads_root().join(".quarantine").join(job_id)
    }

    pub fn uploads_root(&self) -> PathBuf {
        self.root
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.root.clone())
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.uploads_root().join(".staging")
    }

    pub fn variants_root(&self) -> PathBuf {
        self.uploads_root().join("variants")
    }

    pub fn variant_relative_path(
        id: &str,
        version: i64,
        spec: MediaVariantSpec,
        format: MediaVariantFormat,
    ) -> ApiResult<String> {
        if !valid_media_id(id) || version < 1 {
            return Err(ApiError::bad_request(
                "Tham số biến thể media không hợp lệ.",
            ));
        }
        Ok(format!(
            "variants/{}/{id}/v{version}/{}.{}",
            &id[..2],
            spec.as_str(),
            format.extension()
        ))
    }

    pub fn variant_path(
        &self,
        id: &str,
        version: i64,
        spec: MediaVariantSpec,
        format: MediaVariantFormat,
    ) -> ApiResult<PathBuf> {
        Ok(self
            .uploads_root()
            .join(Self::variant_relative_path(id, version, spec, format)?))
    }

    pub(super) fn path(&self, id: &str, extension: &str, thumbnail: bool) -> ApiResult<PathBuf> {
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

    pub fn canonical_path(&self, id: &str, extension: &str) -> ApiResult<PathBuf> {
        self.path(id, extension, false)
    }

    pub fn thumbnail_path(&self, id: &str, extension: &str) -> ApiResult<PathBuf> {
        self.path(id, extension, true)
    }

    pub async fn create_upload_staging_dir(&self) -> ApiResult<PathBuf> {
        static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);
        let seq = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
        let dir = self
            .staging_dir()
            .join(format!("upload-{}-{seq}", std::process::id()));
        fs::create_dir_all(&dir)
            .await
            .map_err(|_| ApiError::internal("Không tạo được thư mục tạm cho upload."))?;
        Ok(dir)
    }

    pub async fn stage_image_bundle(
        &self,
        stage_dir: &Path,
        decoded: &DecodedMedia,
    ) -> ApiResult<()> {
        write_atomic_overwrite(&stage_dir.join("canonical"), &decoded.original).await?;
        write_atomic_overwrite(&stage_dir.join("thumb.png"), &decoded.thumbnail).await?;
        for v in &decoded.variants {
            let name = format!("{}.{}", v.spec.as_str(), v.format.extension());
            write_atomic_overwrite(&stage_dir.join(name), &v.bytes).await?;
        }
        Ok(())
    }

    pub async fn promote_staged_image_bundle(
        &self,
        stage_dir: &Path,
        actual_id: &str,
        decoded: &DecodedMedia,
        version: i64,
    ) -> ApiResult<Vec<PathBuf>> {
        let mut promoted = Vec::new();
        let orig_dst = self.canonical_path(actual_id, decoded.extension)?;
        let thumb_dst = self.thumbnail_path(actual_id, decoded.extension)?;
        if let Some(parent) = orig_dst.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|_| ApiError::internal("Không tạo được thư mục Media Library."))?;
        }

        let res: ApiResult<()> = async {
            fs::rename(stage_dir.join("canonical"), &orig_dst)
                .await
                .map_err(|_| ApiError::internal("Không di chuyển được file media gốc."))?;
            promoted.push(orig_dst.clone());

            fs::rename(stage_dir.join("thumb.png"), &thumb_dst)
                .await
                .map_err(|_| ApiError::internal("Không di chuyển được ảnh thu nhỏ."))?;
            promoted.push(thumb_dst.clone());

            for v in &decoded.variants {
                let src_name = format!("{}.{}", v.spec.as_str(), v.format.extension());
                let dst = self.variant_path(actual_id, version, v.spec, v.format)?;
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent).await.map_err(|_| {
                        ApiError::internal("Không tạo được thư mục biến thể media.")
                    })?;
                }
                fs::rename(stage_dir.join(src_name), &dst)
                    .await
                    .map_err(|_| ApiError::internal("Không di chuyển được biến thể media."))?;
                promoted.push(dst);
            }
            Ok(())
        }
        .await;

        if let Err(err) = res {
            Self::remove_paths_best_effort(&promoted).await;
            return Err(err);
        }
        Ok(promoted)
    }

    pub async fn promote_staged_video_file(
        &self,
        staged_file: &Path,
        actual_id: &str,
    ) -> ApiResult<Vec<PathBuf>> {
        let target = self.canonical_path(actual_id, "mp4")?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|_| ApiError::internal("Không tạo được thư mục Media Library."))?;
        }
        fs::rename(staged_file, &target)
            .await
            .map_err(|_| ApiError::internal("Không lưu được file video an toàn."))?;
        Ok(vec![target])
    }

    pub async fn remove_paths_best_effort(paths: &[PathBuf]) {
        for p in paths {
            let _ = fs::remove_file(p).await;
        }
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

    pub async fn read_with_lease(
        &self,
        _lease: &MediaReadLease,
        id: &str,
        ext: &str,
        thumb: bool,
    ) -> ApiResult<Vec<u8>> {
        let path = self.path(id, ext, thumb)?;
        fs::read(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ApiError::not_found("Không tìm thấy tệp media.")
            } else {
                ApiError::internal("Không đọc được file ảnh đã lưu.")
            }
        })
    }

    pub async fn metadata_with_lease(
        &self,
        _lease: &MediaReadLease,
        id: &str,
        ext: &str,
        thumb: bool,
    ) -> ApiResult<u64> {
        let path = self.path(id, ext, thumb)?;
        let meta = fs::metadata(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ApiError::not_found("Không tìm thấy tệp media.")
            } else {
                ApiError::internal("Không đọc được thông tin file media.")
            }
        })?;
        Ok(meta.len())
    }

    pub async fn read_variant_with_lease(
        &self,
        _lease: &MediaReadLease,
        id: &str,
        version: i64,
        spec: MediaVariantSpec,
        format: MediaVariantFormat,
    ) -> ApiResult<Vec<u8>> {
        let path = self.variant_path(id, version, spec, format)?;
        fs::read(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ApiError::not_found("Không tìm thấy biến thể media.")
            } else {
                ApiError::internal("Không đọc được tệp biến thể media.")
            }
        })
    }

    pub async fn variant_metadata_with_lease(
        &self,
        _lease: &MediaReadLease,
        id: &str,
        version: i64,
        spec: MediaVariantSpec,
        format: MediaVariantFormat,
    ) -> ApiResult<u64> {
        let path = self.variant_path(id, version, spec, format)?;
        let meta = fs::metadata(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ApiError::not_found("Không tìm thấy biến thể media.")
            } else {
                ApiError::internal("Không đọc được thông tin biến thể media.")
            }
        })?;
        Ok(meta.len())
    }

    pub async fn cleanup_staging(&self) {
        let dir = self.staging_dir();
        if let Ok(mut entries) = fs::read_dir(&dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let p = entry.path();
                if entry.file_type().await.is_ok_and(|ft| ft.is_dir()) {
                    let _ = fs::remove_dir_all(&p).await;
                } else {
                    let _ = fs::remove_file(&p).await;
                }
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
}

pub(super) async fn write_once(path: &Path, bytes: &[u8]) -> ApiResult<()> {
    use tokio::io::AsyncWriteExt;
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    if fs::try_exists(path).await.unwrap_or(false) {
        return Ok(());
    }
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
    drop(file);
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
