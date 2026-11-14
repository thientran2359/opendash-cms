//! Journaled media deletion, quarantine moves, and crash recovery (M8-S02-T01).

use std::path::Path;
use std::time::Duration;

use axum::http::{HeaderMap, header};
use sqlx::SqlitePool;
use tokio::fs;

use crate::error::{ApiError, ApiResult};
use crate::media::MediaStore;
use crate::operations::{ExclusiveOpsContext, MutationLease};
pub use crate::storage::media_lifecycle::{
    MediaDeleteJob, MediaDeletePhase, TombstoneError, get_delete_job, tombstone_media_in_tx,
};

/// Parses `If-Match: "gv<generation>-v<version>"` (missing -> 428, weak/malformed -> 412).
pub fn parse_media_if_match(headers: &HeaderMap) -> ApiResult<(i64, i64)> {
    let raw = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .ok_or_else(|| ApiError::precondition_required("Thiếu header If-Match."))?;

    if raw.is_empty() || raw.starts_with("W/") || raw == "*" {
        return Err(ApiError::precondition_failed(
            "Header If-Match phải có dạng \"gv<generation>-v<version>\".",
        ));
    }

    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);

    let Some((gen_part, ver_part)) = inner.split_once("-v") else {
        return Err(ApiError::precondition_failed(
            "Header If-Match không đúng định dạng gv<generation>-v<version>.",
        ));
    };
    let Some(gen_str) = gen_part.strip_prefix("gv") else {
        return Err(ApiError::precondition_failed(
            "Header If-Match không đúng định dạng gv<generation>-v<version>.",
        ));
    };
    let generation: i64 = gen_str
        .parse()
        .ok()
        .filter(|&v| v >= 1)
        .ok_or_else(|| ApiError::precondition_failed("Generation trong If-Match không hợp lệ."))?;
    let version: i64 = ver_part
        .parse()
        .ok()
        .filter(|&v| v >= 1)
        .ok_or_else(|| ApiError::precondition_failed("Version trong If-Match không hợp lệ."))?;

    Ok((generation, version))
}

async fn move_one_file_to_quarantine(
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

async fn execute_delete_job_inner(
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
            if let Err(code) = move_one_file_to_quarantine(&uploads_root, thumb_src, thumb_dst).await
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
