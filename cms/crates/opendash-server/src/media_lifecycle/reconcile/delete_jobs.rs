use super::*;

pub fn parse_media_if_match(headers: &HeaderMap) -> ApiResult<(i64, i64)> {
    let raw = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .ok_or_else(|| ApiError::precondition_required("Thiếu header If-Match."))?;

    opendash_core::media::parse_media_etag_token(raw).ok_or_else(|| {
        ApiError::precondition_failed("Header If-Match phải có dạng \"gv<generation>-v<version>\".")
    })
}

pub(crate) async fn move_one_file_to_quarantine(
    uploads_root: &Path,
    src_rel: &str,
    dst_rel: &str,
) -> Result<(), &'static str> {
    let src = uploads_root.join(src_rel);
    let dst = uploads_root.join(dst_rel);

    if fs::try_exists(&dst).await.unwrap_or(false) {
        return Ok(());
    }
    if !fs::try_exists(&src).await.unwrap_or(false) {
        return Ok(());
    }

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|_| "quarantine_mkdir_failed")?;
    }

    fs::rename(&src, &dst)
        .await
        .map_err(|_| "quarantine_rename_failed")?;
    Ok(())
}

pub(crate) async fn execute_delete_job_inner(
    db: &SqlitePool,
    media_store: &MediaStore,
    job: MediaDeleteJob,
) -> ApiResult<MediaDeleteJob> {
    if job.phase == MediaDeletePhase::Finalized {
        return Ok(job);
    }

    let _drain_guard = match media_store
        .begin_delete_drain(&job.media_id, Duration::from_secs(5))
        .await
    {
        Ok(guard) => guard,
        Err(_pending_guard) => {
            crate::storage::media_lifecycle::set_delete_job_error(
                db,
                &job.job_id,
                "readers_active",
            )
            .await?;
            return get_delete_job(db, &job.job_id)
                .await?
                .ok_or_else(|| ApiError::internal("Không đọc lại được delete job."));
        }
    };

    if job.phase == MediaDeletePhase::Tombstoned {
        crate::storage::media_lifecycle::mark_delete_job_moving(db, &job.job_id).await?;
    }

    let uploads_root = media_store.uploads_root();

    if !job.original_moved {
        if let Err(code) = move_one_file_to_quarantine(
            &uploads_root,
            &job.original_rel_path,
            &job.quarantine_original_rel_path,
        )
        .await
        {
            crate::storage::media_lifecycle::set_delete_job_error(db, &job.job_id, code).await?;
            return get_delete_job(db, &job.job_id)
                .await?
                .ok_or_else(|| ApiError::internal("Không đọc lại được delete job."));
        }
        crate::storage::media_lifecycle::mark_original_moved(db, &job.job_id).await?;
    }

    if let (Some(thumb_src), Some(thumb_dst)) = (
        job.thumbnail_rel_path.as_deref(),
        job.quarantine_thumbnail_rel_path.as_deref(),
    ) {
        if !job.thumbnail_moved {
            if let Err(code) =
                move_one_file_to_quarantine(&uploads_root, thumb_src, thumb_dst).await
            {
                crate::storage::media_lifecycle::set_delete_job_error(db, &job.job_id, code)
                    .await?;
                return get_delete_job(db, &job.job_id)
                    .await?
                    .ok_or_else(|| ApiError::internal("Không đọc lại được delete job."));
            }
            crate::storage::media_lifecycle::mark_thumbnail_moved(db, &job.job_id).await?;
        }
    }

    crate::storage::media_lifecycle::finalize_delete_job(
        db,
        &job.job_id,
        &job.media_id,
        job.generation,
    )
    .await?;

    if job.media_id.len() >= 2 {
        let _ = fs::remove_dir_all(
            media_store
                .variants_root()
                .join(&job.media_id[..2])
                .join(&job.media_id),
        )
        .await;
    }

    get_delete_job(db, &job.job_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được delete job sau khi hoàn tất."))
}

pub async fn execute_delete_job(
    db: &SqlitePool,
    media_store: &MediaStore,
    job: MediaDeleteJob,
    _lease: &MutationLease,
) -> ApiResult<MediaDeleteJob> {
    execute_delete_job_inner(db, media_store, job).await
}

#[allow(dead_code)]
pub async fn recover_delete_job(
    db: &SqlitePool,
    media_store: &MediaStore,
    job: MediaDeleteJob,
    _maint: &ExclusiveOpsContext,
) -> ApiResult<MediaDeleteJob> {
    execute_delete_job_inner(db, media_store, job).await
}

pub async fn recover_pending_delete_jobs_on_startup(
    db: &SqlitePool,
    media_store: &MediaStore,
) -> ApiResult<usize> {
    let jobs = crate::storage::media_lifecycle::list_pending_delete_jobs(db).await?;
    let mut count = 0usize;
    for job in jobs {
        let _ = execute_delete_job_inner(db, media_store, job).await?;
        count += 1;
    }
    Ok(count)
}

pub(crate) fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn modified_unix_secs(meta: &std::fs::Metadata) -> ApiResult<u64> {
    let mtime = meta
        .modified()
        .map_err(|_| ApiError::conflict("Không đọc được thời gian sửa đổi tệp."))?;
    Ok(mtime
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs())
}

pub(crate) async fn sha256_file_hex(path: &Path) -> ApiResult<String> {
    let bytes = fs::read(path)
        .await
        .map_err(|_| ApiError::conflict("Không đọc được tệp để tính checksum."))?;
    let digest = Sha256::digest(&bytes);
    Ok(hex_encode(&digest))
}

pub(crate) fn new_64hex_id() -> String {
    let token = crate::auth::generate_random_token();
    let digest = Sha256::digest(token.as_bytes());
    hex_encode(&digest)
}

pub(crate) fn is_64hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub(crate) fn parse_canonical_media_filename(shard: &str, name: &str) -> Option<String> {
    if shard.len() != 2
        || !shard
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return None;
    }
    if let Some(id) = name.strip_suffix(".thumb.png") {
        if is_64hex(id) && id.starts_with(shard) {
            return Some(id.to_string());
        }
        return None;
    }
    let (id, ext) = name.rsplit_once('.')?;
    if !is_64hex(id) || !id.starts_with(shard) {
        return None;
    }
    if matches!(ext, "jpg" | "png" | "webp" | "mp4") {
        Some(id.to_string())
    } else {
        None
    }
}

pub(crate) fn parse_secret_filename(name: &str) -> Option<String> {
    let stem = name
        .strip_suffix(".bin")
        .or_else(|| name.strip_suffix(".enc"))
        .unwrap_or(name);
    if is_64hex(stem) {
        Some(stem.to_ascii_lowercase())
    } else {
        None
    }
}
