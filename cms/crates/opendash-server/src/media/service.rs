use super::*;

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
            lifecycle_state: "active".to_string(),
            generation: 1,
            version: 1,
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

impl Drop for MediaReadLease {
    fn drop(&mut self) {
        let mut guard = self.gate.state.lock().expect("media read gate poisoned");
        if let Some(count) = guard.active_readers.get_mut(&self.media_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                guard.active_readers.remove(&self.media_id);
                self.gate.notify.notify_waiters();
            }
        }
    }
}

impl Drop for MediaDeleteGateGuard {
    fn drop(&mut self) {
        let mut guard = self.gate.state.lock().expect("media read gate poisoned");
        guard.deleting.remove(&self.media_id);
        self.gate.notify.notify_waiters();
    }
}

impl MediaStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: root.join("media"),
            read_gate: std::sync::Arc::new(MediaReadGate::default()),
        }
    }

    pub fn acquire_read_lease(&self, id: &str) -> ApiResult<MediaReadLease> {
        let mut guard = self
            .read_gate
            .state
            .lock()
            .expect("media read gate poisoned");
        if guard.deleting.contains(id) {
            return Err(ApiError::not_found("Không tìm thấy media."));
        }
        *guard.active_readers.entry(id.to_string()).or_insert(0) += 1;
        Ok(MediaReadLease {
            gate: std::sync::Arc::clone(&self.read_gate),
            media_id: id.to_string(),
        })
    }

    pub fn has_active_reader(&self, id: &str) -> bool {
        let guard = self
            .read_gate
            .state
            .lock()
            .expect("media read gate poisoned");
        guard.active_readers.get(id).copied().unwrap_or(0) > 0
    }

    pub async fn begin_delete_drain(
        &self,
        id: &str,
        timeout: std::time::Duration,
    ) -> Result<MediaDeleteGateGuard, MediaDeleteGateGuard> {
        let effective = timeout.min(std::time::Duration::from_secs(5));
        let deadline = std::time::Instant::now() + effective;
        {
            let mut guard = self
                .read_gate
                .state
                .lock()
                .expect("media read gate poisoned");
            guard.deleting.insert(id.to_string());
        }
        let gate_guard = MediaDeleteGateGuard {
            gate: std::sync::Arc::clone(&self.read_gate),
            media_id: id.to_string(),
        };

        loop {
            let notified = self.read_gate.notify.notified();
            {
                let guard = self
                    .read_gate
                    .state
                    .lock()
                    .expect("media read gate poisoned");
                if guard.active_readers.get(id).copied().unwrap_or(0) == 0 {
                    return Ok(gate_guard);
                }
            }
            let now = std::time::Instant::now();
            if now >= deadline {
                return Err(gate_guard);
            }
            let _ = tokio::time::timeout(deadline.saturating_duration_since(now), notified).await;
        }
    }

    #[allow(dead_code)]
    pub fn media_root(&self) -> &std::path::Path {
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
        let _lease = self.acquire_read_lease(id)?;
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

pub(super) fn etag_matches_if_none_match(header_val: &str, etag: &str) -> bool {
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
