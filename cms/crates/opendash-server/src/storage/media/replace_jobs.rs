use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use super::types::{MediaAsset, QueuedMediaVariant};
use crate::error::{ApiError, ApiResult};

const REPLACE_SQL_CHUNK: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaReplacePhase {
    Staged,
    MovingOld,
    PromotingNew,
    Completed,
    RolledBack,
}

impl MediaReplacePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::MovingOld => "moving_old",
            Self::PromotingNew => "promoting_new",
            Self::Completed => "completed",
            Self::RolledBack => "rolled_back",
        }
    }

    pub fn parse(raw: &str) -> ApiResult<Self> {
        match raw {
            "staged" => Ok(Self::Staged),
            "moving_old" => Ok(Self::MovingOld),
            "promoting_new" => Ok(Self::PromotingNew),
            "completed" => Ok(Self::Completed),
            "rolled_back" => Ok(Self::RolledBack),
            _ => Err(ApiError::internal(
                "Trạng thái media replace job không hợp lệ.",
            )),
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Staged | Self::MovingOld | Self::PromotingNew)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaReplaceJob {
    pub job_id: String,
    pub media_id: String,
    pub expected_generation: i64,
    pub expected_version: i64,
    pub new_version: i64,
    pub old_content_sha256: String,
    pub new_content_sha256: String,
    pub new_kind: String,
    pub new_filename: String,
    pub new_mime_type: String,
    pub new_extension: String,
    pub new_byte_size: i64,
    pub new_width: i64,
    pub new_height: i64,
    pub new_thumb_width: Option<i64>,
    pub new_thumb_height: Option<i64>,
    pub new_duration_ms: Option<i64>,
    pub new_video_codec: Option<String>,
    pub new_audio_codec: Option<String>,
    pub new_variants_json: String,
    pub old_canonical_rel_path: String,
    pub old_thumb_rel_path: Option<String>,
    pub staging_rel_dir: String,
    pub quarantine_rel_dir: String,
    pub phase: MediaReplacePhase,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub purge_after: Option<String>,
    pub purged_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewReplaceJobInput {
    pub job_id: String,
    pub media_id: String,
    pub expected_generation: i64,
    pub expected_version: i64,
    pub new_content_sha256: String,
    pub new_kind: String,
    pub new_filename: String,
    pub new_mime_type: String,
    pub new_extension: String,
    pub new_byte_size: i64,
    pub new_width: i64,
    pub new_height: i64,
    pub new_thumb_width: Option<i64>,
    pub new_thumb_height: Option<i64>,
    pub new_duration_ms: Option<i64>,
    pub new_video_codec: Option<String>,
    pub new_audio_codec: Option<String>,
    pub new_variants_json: String,
    pub staging_rel_dir: String,
    pub quarantine_rel_dir: String,
}

fn map_replace_job_row(row: &sqlx::sqlite::SqliteRow) -> ApiResult<MediaReplaceJob> {
    let phase_raw: String = row.try_get("phase")?;
    Ok(MediaReplaceJob {
        job_id: row.try_get("job_id")?,
        media_id: row.try_get("media_id")?,
        expected_generation: row.try_get("expected_generation")?,
        expected_version: row.try_get("expected_version")?,
        new_version: row.try_get("new_version")?,
        old_content_sha256: row.try_get("old_content_sha256")?,
        new_content_sha256: row.try_get("new_content_sha256")?,
        new_kind: row.try_get("new_kind")?,
        new_filename: row.try_get("new_filename")?,
        new_mime_type: row.try_get("new_mime_type")?,
        new_extension: row.try_get("new_extension")?,
        new_byte_size: row.try_get("new_byte_size")?,
        new_width: row.try_get("new_width")?,
        new_height: row.try_get("new_height")?,
        new_thumb_width: row.try_get("new_thumb_width")?,
        new_thumb_height: row.try_get("new_thumb_height")?,
        new_duration_ms: row.try_get("new_duration_ms")?,
        new_video_codec: row.try_get("new_video_codec")?,
        new_audio_codec: row.try_get("new_audio_codec")?,
        new_variants_json: row.try_get("new_variants_json")?,
        old_canonical_rel_path: row.try_get("old_canonical_rel_path")?,
        old_thumb_rel_path: row.try_get("old_thumb_rel_path")?,
        staging_rel_dir: row.try_get("staging_rel_dir")?,
        quarantine_rel_dir: row.try_get("quarantine_rel_dir")?,
        phase: MediaReplacePhase::parse(&phase_raw)?,
        error_message: row.try_get("error_message")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        purge_after: row.try_get("purge_after")?,
        purged_at: row.try_get("purged_at")?,
    })
}

pub async fn get_replace_job(db: &SqlitePool, job_id: &str) -> ApiResult<Option<MediaReplaceJob>> {
    let row = sqlx::query(
        "SELECT job_id, media_id, expected_generation, expected_version, new_version,
                old_content_sha256, new_content_sha256, new_kind, new_filename, new_mime_type,
                new_extension, new_byte_size, new_width, new_height, new_thumb_width,
                new_thumb_height, new_duration_ms, new_video_codec, new_audio_codec,
                new_variants_json, old_canonical_rel_path, old_thumb_rel_path, staging_rel_dir,
                quarantine_rel_dir, phase, error_message, created_at, updated_at,
                purge_after, purged_at
         FROM media_replace_jobs
         WHERE job_id = ?",
    )
    .bind(job_id)
    .fetch_optional(db)
    .await?;
    row.as_ref().map(map_replace_job_row).transpose()
}

pub async fn begin_replace_job_tx1(
    db: &SqlitePool,
    input: &NewReplaceJobInput,
) -> ApiResult<MediaReplaceJob> {
    if !super::queries::valid_media_id(&input.media_id) {
        return Err(ApiError::bad_request("Mã media không hợp lệ."));
    }

    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    let current = sqlx::query_as::<_, MediaAsset>(
        "SELECT id, COALESCE(content_sha256, id) AS content_sha256, kind, filename, mime_type,
                extension, byte_size, width, height, thumb_width, thumb_height, duration_ms,
                video_codec, audio_codec, lifecycle_state, generation, version,
                variants_ready_version, created_at, COALESCE(updated_at, created_at) AS updated_at
         FROM media_assets
         WHERE id = ?",
    )
    .bind(&input.media_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ApiError::not_found("Không tìm thấy media."))?;

    if current.lifecycle_state != "active" {
        return Err(ApiError::conflict(
            "Media đang trong quá trình xóa; không thể thay thế.",
        ));
    }
    if current.kind != input.new_kind {
        return Err(ApiError::unprocessable(
            "Không thể thay thế chéo loại media giữa ảnh và video.",
        ));
    }
    if current.generation != input.expected_generation || current.version != input.expected_version
    {
        return Err(ApiError::precondition_failed(
            "Phiên bản hoặc generation của media đã thay đổi.",
        ));
    }

    let pending_del: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM media_delete_jobs WHERE media_id = ? AND phase != 'finalized' LIMIT 1",
    )
    .bind(&input.media_id)
    .fetch_optional(&mut *tx)
    .await?;
    if pending_del.is_some() {
        return Err(ApiError::conflict(
            "Media đang chờ hoàn tất xóa; không thể thay thế.",
        ));
    }

    let active_replace: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM media_replace_jobs
         WHERE media_id = ? AND phase IN ('staged', 'moving_old', 'promoting_new') LIMIT 1",
    )
    .bind(&input.media_id)
    .fetch_optional(&mut *tx)
    .await?;
    if active_replace.is_some() {
        return Err(ApiError::conflict(
            "Media đang có tiến trình thay thế khác đang thực thi.",
        ));
    }

    let new_version = input.expected_version + 1;
    let shard = &input.media_id[..2];
    let old_canonical_rel_path = format!("media/{shard}/{}.{}", input.media_id, current.extension);
    let old_thumb_rel_path = if current.kind == "image" {
        Some(format!("media/{shard}/{}.thumb.png", input.media_id))
    } else {
        None
    };

    sqlx::query(
        "INSERT INTO media_replace_jobs (
            job_id, media_id, expected_generation, expected_version, new_version,
            old_content_sha256, new_content_sha256, new_kind, new_filename, new_mime_type,
            new_extension, new_byte_size, new_width, new_height, new_thumb_width,
            new_thumb_height, new_duration_ms, new_video_codec, new_audio_codec,
            new_variants_json, old_canonical_rel_path, old_thumb_rel_path,
            staging_rel_dir, quarantine_rel_dir, phase
         ) VALUES (
            ?, ?, ?, ?, ?,
            ?, ?, ?, ?, ?,
            ?, ?, ?, ?, ?,
            ?, ?, ?, ?,
            ?, ?, ?,
            ?, ?, 'promoting_new'
         )",
    )
    .bind(&input.job_id)
    .bind(&input.media_id)
    .bind(input.expected_generation)
    .bind(input.expected_version)
    .bind(new_version)
    .bind(&current.content_sha256)
    .bind(&input.new_content_sha256)
    .bind(&input.new_kind)
    .bind(&input.new_filename)
    .bind(&input.new_mime_type)
    .bind(&input.new_extension)
    .bind(input.new_byte_size)
    .bind(input.new_width)
    .bind(input.new_height)
    .bind(input.new_thumb_width)
    .bind(input.new_thumb_height)
    .bind(input.new_duration_ms)
    .bind(&input.new_video_codec)
    .bind(&input.new_audio_codec)
    .bind(&input.new_variants_json)
    .bind(&old_canonical_rel_path)
    .bind(&old_thumb_rel_path)
    .bind(&input.staging_rel_dir)
    .bind(&input.quarantine_rel_dir)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    get_replace_job(db, &input.job_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được media replace job."))
}

pub async fn commit_replace_job_tx2(
    db: &SqlitePool,
    job: &MediaReplaceJob,
    variants: &[QueuedMediaVariant],
) -> ApiResult<MediaAsset> {
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    let ready_ver = if job.new_kind == "image" {
        job.new_version
    } else {
        0
    };

    let updated = sqlx::query(
        "UPDATE media_assets
         SET content_sha256 = ?, filename = ?, mime_type = ?, extension = ?,
             byte_size = ?, width = ?, height = ?, thumb_width = ?, thumb_height = ?,
             duration_ms = ?, video_codec = ?, audio_codec = ?,
             generation = generation + 1, version = ?, variants_ready_version = ?,
             updated_at = datetime('now')
         WHERE id = ? AND generation = ? AND version = ? AND lifecycle_state = 'active'",
    )
    .bind(&job.new_content_sha256)
    .bind(&job.new_filename)
    .bind(&job.new_mime_type)
    .bind(&job.new_extension)
    .bind(job.new_byte_size)
    .bind(job.new_width)
    .bind(job.new_height)
    .bind(job.new_thumb_width)
    .bind(job.new_thumb_height)
    .bind(job.new_duration_ms)
    .bind(&job.new_video_codec)
    .bind(&job.new_audio_codec)
    .bind(job.new_version)
    .bind(ready_ver)
    .bind(&job.media_id)
    .bind(job.expected_generation)
    .bind(job.expected_version)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "Trạng thái media đã thay đổi khi hoàn tất replace.",
        ));
    }

    sqlx::query("DELETE FROM media_variants WHERE media_id = ?")
        .bind(&job.media_id)
        .execute(&mut *tx)
        .await?;

    for v in variants {
        sqlx::query(
            "INSERT INTO media_variants
             (media_id, version, spec, format, width, height, byte_size, relative_path, sha256)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&job.media_id)
        .bind(job.new_version)
        .bind(v.spec.as_str())
        .bind(v.format.as_str())
        .bind(i64::from(v.width))
        .bind(i64::from(v.height))
        .bind(i64::try_from(v.byte_size).unwrap_or(0))
        .bind(&v.relative_path)
        .bind(&v.sha256)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "UPDATE media_replace_jobs
         SET phase = 'completed',
             error_message = NULL,
             purge_after = datetime('now', '+7 days'),
             updated_at = datetime('now')
         WHERE job_id = ?",
    )
    .bind(&job.job_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    super::queries::find(db, &job.media_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được media vừa thay thế."))
}

pub async fn mark_replace_job_rolled_back(
    db: &SqlitePool,
    job_id: &str,
    error_message: &str,
) -> ApiResult<()> {
    sqlx::query(
        "UPDATE media_replace_jobs
         SET phase = 'rolled_back',
             error_message = ?,
             purge_after = COALESCE(purge_after, datetime('now', '+7 days')),
             updated_at = datetime('now')
         WHERE job_id = ? AND phase != 'completed'",
    )
    .bind(error_message)
    .bind(job_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn list_pending_replace_jobs(db: &SqlitePool) -> ApiResult<Vec<MediaReplaceJob>> {
    let rows = sqlx::query(
        "SELECT job_id, media_id, expected_generation, expected_version, new_version,
                old_content_sha256, new_content_sha256, new_kind, new_filename, new_mime_type,
                new_extension, new_byte_size, new_width, new_height, new_thumb_width,
                new_thumb_height, new_duration_ms, new_video_codec, new_audio_codec,
                new_variants_json, old_canonical_rel_path, old_thumb_rel_path, staging_rel_dir,
                quarantine_rel_dir, phase, error_message, created_at, updated_at,
                purge_after, purged_at
         FROM media_replace_jobs
         WHERE phase IN ('staged', 'moving_old', 'promoting_new')
         ORDER BY created_at ASC, job_id ASC",
    )
    .fetch_all(db)
    .await?;
    rows.iter().map(map_replace_job_row).collect()
}

pub async fn find_replace_jobs_by_ids(
    db: &SqlitePool,
    job_ids: &[String],
) -> ApiResult<HashMap<String, MediaReplaceJob>> {
    let mut out = HashMap::new();
    if job_ids.is_empty() {
        return Ok(out);
    }
    for chunk in job_ids.chunks(REPLACE_SQL_CHUNK) {
        let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT job_id, media_id, expected_generation, expected_version, new_version,
                    old_content_sha256, new_content_sha256, new_kind, new_filename, new_mime_type,
                    new_extension, new_byte_size, new_width, new_height, new_thumb_width,
                    new_thumb_height, new_duration_ms, new_video_codec, new_audio_codec,
                    new_variants_json, old_canonical_rel_path, old_thumb_rel_path, staging_rel_dir,
                    quarantine_rel_dir, phase, error_message, created_at, updated_at,
                    purge_after, purged_at
             FROM media_replace_jobs
             WHERE job_id IN (",
        );
        let mut sep = qb.separated(", ");
        for id in chunk {
            sep.push_bind(id);
        }
        sep.push_unseparated(")");
        for row in qb.build().fetch_all(db).await? {
            let job = map_replace_job_row(&row)?;
            out.insert(job.job_id.clone(), job);
        }
    }
    Ok(out)
}

pub async fn mark_replace_job_purged(db: &SqlitePool, job_id: &str) -> ApiResult<()> {
    sqlx::query(
        "UPDATE media_replace_jobs
         SET purged_at = COALESCE(purged_at, datetime('now')),
             updated_at = datetime('now')
         WHERE job_id = ?",
    )
    .bind(job_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn prune_purged_replace_jobs(db: &SqlitePool) -> ApiResult<u64> {
    let res = sqlx::query(
        "DELETE FROM media_replace_jobs
         WHERE phase IN ('completed', 'rolled_back')
           AND purged_at IS NOT NULL
           AND updated_at < datetime('now', '-7 days')",
    )
    .execute(db)
    .await?;
    Ok(res.rows_affected())
}
