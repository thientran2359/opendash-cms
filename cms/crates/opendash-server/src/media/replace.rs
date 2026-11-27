//! Crash-safe in-place Media Replace (`3A`) with zero-SQL disk swap and full startup recovery.

use super::*;
use opendash_core::media::QueuedMediaVariant;

use crate::{
    operations::SiteCoordinator,
    storage::media::{
        self as media_storage, MediaAssetDetails, MediaReplaceJob, MediaReplacePhase,
        NewReplaceJobInput,
    },
};

#[derive(Debug)]
pub struct ReplaceStagingGuard(pub(crate) Option<PathBuf>);

impl ReplaceStagingGuard {
    pub fn new(dir: PathBuf) -> Self {
        Self(Some(dir))
    }

    pub fn path(&self) -> Option<&Path> {
        self.0.as_deref()
    }

    pub fn disarm(&mut self) -> Option<PathBuf> {
        self.0.take()
    }
}

impl Drop for ReplaceStagingGuard {
    fn drop(&mut self) {
        if let Some(dir) = self.0.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

pub enum StagedReplaceContent {
    Image {
        decoded: DecodedMedia,
        filename: String,
        variants: Vec<QueuedMediaVariant>,
    },
    Video {
        content_sha256: String,
        filename: String,
        mime_type: String,
        extension: String,
        byte_size: i64,
        width: i64,
        height: i64,
        duration_ms: i64,
        video_codec: String,
        audio_codec: Option<String>,
        staged_file_rel: String,
    },
}

pub struct StagedReplacePayload {
    pub job_id: String,
    pub stage_guard: ReplaceStagingGuard,
    pub content: StagedReplaceContent,
}

impl MediaStore {
    pub async fn create_replace_staging_dir(&self, job_id: &str) -> ApiResult<PathBuf> {
        let dir = self.staging_dir().join(format!("replace-{job_id}"));
        fs::create_dir_all(&dir)
            .await
            .map_err(|_| ApiError::internal("Không tạo được thư mục tạm để thay thế media."))?;
        Ok(dir)
    }

    pub async fn stage_replace_image_bundle(
        &self,
        stage_dir: &Path,
        media_id: &str,
        new_version: i64,
        decoded: &DecodedMedia,
    ) -> ApiResult<Vec<QueuedMediaVariant>> {
        write_atomic_overwrite(&stage_dir.join("canonical"), &decoded.original).await?;
        write_atomic_overwrite(&stage_dir.join("thumb.png"), &decoded.thumbnail).await?;

        let var_stage = stage_dir.join(format!("v{new_version}"));
        if !decoded.variants.is_empty() {
            fs::create_dir_all(&var_stage).await.map_err(|_| {
                ApiError::internal("Không tạo được thư mục tạm cho biến thể media.")
            })?;
        }

        let mut queued = Vec::with_capacity(decoded.variants.len());
        for v in &decoded.variants {
            let file_name = format!("{}.{}", v.spec.as_str(), v.format.extension());
            write_atomic_overwrite(&var_stage.join(&file_name), &v.bytes).await?;
            queued.push(QueuedMediaVariant {
                spec: v.spec,
                format: v.format,
                width: v.width,
                height: v.height,
                byte_size: v.bytes.len() as u64,
                relative_path: Self::variant_relative_path(
                    media_id,
                    new_version,
                    v.spec,
                    v.format,
                )?,
                sha256: v.sha256.clone(),
                staged_rel_path: format!("v{new_version}/{file_name}"),
            });
        }
        Ok(queued)
    }
}

async fn remove_dir_best_effort(path: &Path) {
    if fs::metadata(path).await.is_ok() {
        let _ = fs::remove_dir_all(path).await;
    }
}

fn old_extension_from_rel_path(rel_path: &str) -> Option<&str> {
    Path::new(rel_path).extension().and_then(|ext| ext.to_str())
}

async fn rename_with_parent(src: &Path, dst: &Path, err_msg: &'static str) -> ApiResult<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|_| ApiError::internal(err_msg))?;
    }
    fs::rename(src, dst)
        .await
        .map_err(|_| ApiError::internal(err_msg))
}

/// Restores quarantined files back to live paths and removes partial `new_version` files.
/// Leaves `v{old_ver}/` untouched because variant swap never moves it into `.quarantine/`.
pub async fn rollback_replace_disk(store: &MediaStore, job: &MediaReplaceJob) {
    if !valid_media_id(&job.media_id) {
        return;
    }
    let uploads = store.uploads_root();
    let shard = &job.media_id[..2];
    let quarantine_dir = store.quarantine_job_dir(&job.job_id);
    let live_old_canonical = uploads.join(&job.old_canonical_rel_path);
    let live_new_canonical = uploads.join(format!(
        "media/{shard}/{}.{}",
        job.media_id, job.new_extension
    ));

    let new_var_dir = uploads.join(format!(
        "variants/{shard}/{}/v{}",
        job.media_id, job.new_version
    ));
    remove_dir_best_effort(&new_var_dir).await;

    if let Some(old_ext) = old_extension_from_rel_path(&job.old_canonical_rel_path) {
        let q_canonical = quarantine_dir.join(format!("{}.{}", job.media_id, old_ext));
        if fs::try_exists(&q_canonical).await.unwrap_or(false) {
            if fs::try_exists(&live_new_canonical).await.unwrap_or(false) {
                let _ = fs::remove_file(&live_new_canonical).await;
            }
            if live_old_canonical != live_new_canonical
                && fs::try_exists(&live_old_canonical).await.unwrap_or(false)
            {
                let _ = fs::remove_file(&live_old_canonical).await;
            }
            let _ =
                rename_with_parent(&q_canonical, &live_old_canonical, "rollback canonical").await;
        } else if live_new_canonical != live_old_canonical
            && fs::try_exists(&live_new_canonical).await.unwrap_or(false)
        {
            let _ = fs::remove_file(&live_new_canonical).await;
        }
    }

    if let Some(old_thumb_rel) = job.old_thumb_rel_path.as_deref() {
        let live_thumb = uploads.join(old_thumb_rel);
        let q_thumb = quarantine_dir.join(format!("{}.thumb.png", job.media_id));
        if fs::try_exists(&q_thumb).await.unwrap_or(false) {
            if fs::try_exists(&live_thumb).await.unwrap_or(false) {
                let _ = fs::remove_file(&live_thumb).await;
            }
            let _ = rename_with_parent(&q_thumb, &live_thumb, "rollback thumb").await;
        }
    }

    if let Ok(mut rd) = fs::read_dir(&quarantine_dir).await
        && rd.next_entry().await.ok().flatten().is_none()
    {
        let _ = fs::remove_dir(&quarantine_dir).await;
    }
    remove_dir_best_effort(&uploads.join(&job.staging_rel_dir)).await;
}

pub async fn execute_replace_swap_and_commit(
    db: &sqlx::SqlitePool,
    store: &MediaStore,
    coordinator: &SiteCoordinator,
    current: &MediaAsset,
    staged: StagedReplacePayload,
) -> ApiResult<MediaAssetDetails> {
    execute_replace_swap_and_commit_inner(db, store, coordinator, current, staged, false).await
}

pub(super) async fn execute_replace_swap_and_commit_inner(
    db: &sqlx::SqlitePool,
    store: &MediaStore,
    coordinator: &SiteCoordinator,
    current: &MediaAsset,
    mut staged: StagedReplacePayload,
    force_tx2_error: bool,
) -> ApiResult<MediaAssetDetails> {
    if !store
        .wait_for_no_active_readers_before_swap(&current.id, Duration::from_millis(500))
        .await
    {
        return Err(ApiError::conflict(
            "readers_active: Media đang có người xem hoặc stream đang mở; vui lòng thử lại sau.",
        ));
    }

    let _lease = coordinator.admit_mutation().map_err(|_| {
        ApiError::service_unavailable("Hệ thống đang bảo trì; không thể thay thế media lúc này.")
    })?;

    let stage_dir = staged
        .stage_guard
        .path()
        .ok_or_else(|| ApiError::internal("Thư mục staging replace không hợp lệ."))?
        .to_path_buf();

    let (input, queued_variants, video_stage_file) = match &staged.content {
        StagedReplaceContent::Image {
            decoded,
            filename,
            variants,
        } => {
            let variants_json = serde_json::to_string(variants)
                .map_err(|_| ApiError::internal("Không tuần tự hóa được biến thể mới."))?;
            (
                NewReplaceJobInput {
                    job_id: staged.job_id.clone(),
                    media_id: current.id.clone(),
                    expected_generation: current.generation,
                    expected_version: current.version,
                    new_content_sha256: decoded.id.clone(),
                    new_kind: "image".to_string(),
                    new_filename: filename.clone(),
                    new_mime_type: decoded.mime_type.to_string(),
                    new_extension: decoded.extension.to_string(),
                    new_byte_size: i64::try_from(decoded.original.len()).unwrap_or(0),
                    new_width: i64::from(decoded.width),
                    new_height: i64::from(decoded.height),
                    new_thumb_width: Some(i64::from(decoded.thumb_width)),
                    new_thumb_height: Some(i64::from(decoded.thumb_height)),
                    new_duration_ms: None,
                    new_video_codec: None,
                    new_audio_codec: None,
                    new_variants_json: variants_json,
                    staging_rel_dir: format!(".staging/replace-{}", staged.job_id),
                    quarantine_rel_dir: format!(".quarantine/{}", staged.job_id),
                },
                variants.clone(),
                None,
            )
        }
        StagedReplaceContent::Video {
            content_sha256,
            filename,
            mime_type,
            extension,
            byte_size,
            width,
            height,
            duration_ms,
            video_codec,
            audio_codec,
            staged_file_rel,
        } => (
            NewReplaceJobInput {
                job_id: staged.job_id.clone(),
                media_id: current.id.clone(),
                expected_generation: current.generation,
                expected_version: current.version,
                new_content_sha256: content_sha256.clone(),
                new_kind: "video".to_string(),
                new_filename: filename.clone(),
                new_mime_type: mime_type.clone(),
                new_extension: extension.clone(),
                new_byte_size: *byte_size,
                new_width: *width,
                new_height: *height,
                new_thumb_width: None,
                new_thumb_height: None,
                new_duration_ms: Some(*duration_ms),
                new_video_codec: Some(video_codec.clone()),
                new_audio_codec: audio_codec.clone(),
                new_variants_json: "[]".to_string(),
                staging_rel_dir: format!(".staging/replace-{}", staged.job_id),
                quarantine_rel_dir: format!(".quarantine/{}", staged.job_id),
            },
            Vec::new(),
            Some(staged_file_rel.clone()),
        ),
    };

    let job = media_storage::begin_replace_job_tx1(db, &input).await?;
    let _ = staged.stage_guard.disarm();

    let Some(replace_guard) = store.try_begin_replace_swap(&current.id) else {
        let _ =
            media_storage::mark_replace_job_rolled_back(db, &job.job_id, "readers_active").await;
        remove_dir_best_effort(&stage_dir).await;
        return Err(ApiError::conflict(
            "readers_active: Media vừa có kết nối đọc mới; vui lòng thử lại.",
        ));
    };

    let uploads = store.uploads_root();
    let shard = &job.media_id[..2];
    let quarantine_dir = store.quarantine_job_dir(&job.job_id);

    // Critical section: flat disk swap with 0 SQLite I/O while `replacing = true`.
    let swap_res: ApiResult<()> = async {
        let old_ext = old_extension_from_rel_path(&job.old_canonical_rel_path)
            .ok_or_else(|| ApiError::internal("Đường dẫn tệp gốc cũ không hợp lệ."))?;
        let old_live = uploads.join(&job.old_canonical_rel_path);
        if fs::try_exists(&old_live).await.unwrap_or(false) {
            rename_with_parent(
                &old_live,
                &quarantine_dir.join(format!("{}.{}", job.media_id, old_ext)),
                "Không di chuyển được tệp gốc cũ vào cách ly.",
            )
            .await?;
        }
        if let Some(old_thumb_rel) = job.old_thumb_rel_path.as_deref() {
            let old_thumb = uploads.join(old_thumb_rel);
            if fs::try_exists(&old_thumb).await.unwrap_or(false) {
                rename_with_parent(
                    &old_thumb,
                    &quarantine_dir.join(format!("{}.thumb.png", job.media_id)),
                    "Không di chuyển được thumbnail cũ vào cách ly.",
                )
                .await?;
            }
        }

        let new_live_canonical = store.canonical_path(&job.media_id, &job.new_extension)?;
        if job.new_kind == "image" {
            rename_with_parent(
                &stage_dir.join("canonical"),
                &new_live_canonical,
                "Không thể thay thế tệp ảnh chính.",
            )
            .await?;
            rename_with_parent(
                &stage_dir.join("thumb.png"),
                &store.thumbnail_path(&job.media_id, &job.new_extension)?,
                "Không thể thay thế ảnh thumbnail.",
            )
            .await?;
            let staged_var_dir = stage_dir.join(format!("v{}", job.new_version));
            if fs::try_exists(&staged_var_dir).await.unwrap_or(false) {
                let dst_var_dir = uploads.join(format!(
                    "variants/{shard}/{}/v{}",
                    job.media_id, job.new_version
                ));
                remove_dir_best_effort(&dst_var_dir).await;
                rename_with_parent(
                    &staged_var_dir,
                    &dst_var_dir,
                    "Không thể đưa thư mục biến thể mới vào hoạt động.",
                )
                .await?;
            }
        } else if let Some(rel) = video_stage_file.as_deref() {
            rename_with_parent(
                &stage_dir.join(rel),
                &new_live_canonical,
                "Không thể thay thế tệp video.",
            )
            .await?;
        }
        Ok(())
    }
    .await;

    let tx2_res = match swap_res {
        Ok(()) => {
            if force_tx2_error {
                Err(ApiError::internal("Simulated Tx 2 commit failure."))
            } else {
                media_storage::commit_replace_job_tx2(db, &job, &queued_variants)
                    .await
                    .map(|_| ())
            }
        }
        Err(err) => Err(err),
    };

    if let Err(err) = tx2_res {
        rollback_replace_disk(store, &job).await;
        drop(replace_guard);
        let _ = media_storage::mark_replace_job_rolled_back(db, &job.job_id, &err.1).await;
        return Err(err);
    }

    // Release readers immediately after Tx 2 commits, then remove obsolete `v{old_ver}/`.
    drop(replace_guard);
    remove_dir_best_effort(&stage_dir).await;
    if job.new_kind == "image" && job.expected_version >= 1 {
        let old_var_dir = uploads.join(format!(
            "variants/{shard}/{}/v{}",
            job.media_id, job.expected_version
        ));
        remove_dir_best_effort(&old_var_dir).await;
    }

    media_storage::find_details(db, &current.id, true)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được chi tiết media sau khi replace."))
}

pub async fn recover_pending_replace_jobs_on_startup(
    db: &sqlx::SqlitePool,
    store: &MediaStore,
) -> ApiResult<usize> {
    let pending = media_storage::list_pending_replace_jobs(db).await?;
    let mut recovered = 0usize;
    let uploads = store.uploads_root();
    for job in pending {
        match job.phase {
            MediaReplacePhase::Staged => {
                remove_dir_best_effort(&uploads.join(&job.staging_rel_dir)).await;
                media_storage::mark_replace_job_rolled_back(
                    db,
                    &job.job_id,
                    "server_restarted_staged",
                )
                .await?;
            }
            MediaReplacePhase::MovingOld | MediaReplacePhase::PromotingNew => {
                rollback_replace_disk(store, &job).await;
                media_storage::mark_replace_job_rolled_back(
                    db,
                    &job.job_id,
                    "server_restarted_rollback",
                )
                .await?;
            }
            MediaReplacePhase::Completed | MediaReplacePhase::RolledBack => {}
        }
        recovered += 1;
    }
    Ok(recovered)
}
