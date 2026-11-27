//! SQLite storage helpers for journaled media deletion (M8-S02-T01).

use serde::Serialize;
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::error::{ApiError, ApiResult};
use crate::storage::media_refs::{MediaPinSummary, collect_pins_in_tx};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaDeletePhase {
    Tombstoned,
    Moving,
    Quarantined,
    Finalized,
}

impl MediaDeletePhase {
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tombstoned => "tombstoned",
            Self::Moving => "moving",
            Self::Quarantined => "quarantined",
            Self::Finalized => "finalized",
        }
    }

    pub fn parse(raw: &str) -> ApiResult<Self> {
        match raw {
            "tombstoned" => Ok(Self::Tombstoned),
            "moving" => Ok(Self::Moving),
            "quarantined" => Ok(Self::Quarantined),
            "finalized" => Ok(Self::Finalized),
            _ => Err(ApiError::internal(
                "Trạng thái media delete job không hợp lệ.",
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaDeleteJob {
    pub job_id: String,
    pub media_id: String,
    pub generation: i64,
    pub expected_version: i64,
    pub phase: MediaDeletePhase,
    pub original_rel_path: String,
    pub thumbnail_rel_path: Option<String>,
    pub quarantine_original_rel_path: String,
    pub quarantine_thumbnail_rel_path: Option<String>,
    pub original_moved: bool,
    pub thumbnail_moved: bool,
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug)]
pub enum TombstoneError {
    Referenced(MediaPinSummary),
    Api(ApiError),
}

impl From<ApiError> for TombstoneError {
    fn from(err: ApiError) -> Self {
        Self::Api(err)
    }
}

impl From<sqlx::Error> for TombstoneError {
    fn from(err: sqlx::Error) -> Self {
        Self::Api(err.into())
    }
}

fn map_delete_job_row(row: &sqlx::sqlite::SqliteRow) -> ApiResult<MediaDeleteJob> {
    let phase_raw: String = row.try_get("phase")?;
    let original_moved_int: i64 = row.try_get("original_moved")?;
    let thumbnail_moved_int: i64 = row.try_get("thumbnail_moved")?;
    Ok(MediaDeleteJob {
        job_id: row.try_get("job_id")?,
        media_id: row.try_get("media_id")?,
        generation: row.try_get("generation")?,
        expected_version: row.try_get("expected_version")?,
        phase: MediaDeletePhase::parse(&phase_raw)?,
        original_rel_path: row.try_get("original_rel_path")?,
        thumbnail_rel_path: row.try_get("thumbnail_rel_path")?,
        quarantine_original_rel_path: row.try_get("quarantine_original_rel_path")?,
        quarantine_thumbnail_rel_path: row.try_get("quarantine_thumbnail_rel_path")?,
        original_moved: original_moved_int != 0,
        thumbnail_moved: thumbnail_moved_int != 0,
        error_code: row.try_get("error_code")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        completed_at: row.try_get("completed_at")?,
    })
}

pub async fn get_delete_job_in_conn(
    conn: &mut SqliteConnection,
    job_id: &str,
) -> ApiResult<Option<MediaDeleteJob>> {
    let row = sqlx::query(
        r#"
        SELECT job_id, media_id, generation, expected_version, phase,
               original_rel_path, thumbnail_rel_path,
               quarantine_original_rel_path, quarantine_thumbnail_rel_path,
               original_moved, thumbnail_moved, error_code,
               created_at, updated_at, completed_at
        FROM media_delete_jobs
        WHERE job_id = ?
        "#,
    )
    .bind(job_id)
    .fetch_optional(&mut *conn)
    .await?;

    row.as_ref().map(map_delete_job_row).transpose()
}

pub async fn get_delete_job(db: &SqlitePool, job_id: &str) -> ApiResult<Option<MediaDeleteJob>> {
    let mut conn = db.acquire().await?;
    get_delete_job_in_conn(&mut conn, job_id).await
}

pub async fn tombstone_media_in_tx(
    conn: &mut SqliteConnection,
    media_id: &str,
    expected_generation: i64,
    expected_version: i64,
) -> Result<MediaDeleteJob, TombstoneError> {
    if !crate::storage::media::valid_media_id(media_id) {
        return Err(ApiError::bad_request("Mã media không hợp lệ.").into());
    }

    let row = sqlx::query(
        "SELECT kind, extension, lifecycle_state, generation, version FROM media_assets WHERE id = ?",
    )
    .bind(media_id)
    .fetch_optional(&mut *conn)
    .await?;

    let Some(row) = row else {
        return Err(ApiError::not_found("Không tìm thấy media.").into());
    };

    let kind: String = row.try_get("kind")?;
    let extension: String = row.try_get("extension")?;
    let lifecycle_state: String = row.try_get("lifecycle_state")?;
    let generation: i64 = row.try_get("generation")?;
    let version: i64 = row.try_get("version")?;

    if lifecycle_state != "active" {
        return Err(ApiError::conflict("Media đang trong quá trình xóa.").into());
    }
    if generation != expected_generation || version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản hoặc generation của media đã thay đổi.",
        )
        .into());
    }

    let active_replace: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM media_replace_jobs
         WHERE media_id = ? AND phase IN ('staged', 'moving_old', 'promoting_new')
         LIMIT 1",
    )
    .bind(media_id)
    .fetch_optional(&mut *conn)
    .await?;
    if active_replace.is_some() {
        return Err(ApiError::conflict("Media đang được thay thế; không thể xóa lúc này.").into());
    }

    let pins = collect_pins_in_tx(&mut *conn, media_id).await?;
    if pins.is_referenced() {
        return Err(TombstoneError::Referenced(pins));
    }

    let updated = sqlx::query(
        "UPDATE media_assets
         SET lifecycle_state = 'deleting', version = version + 1
         WHERE id = ? AND generation = ? AND version = ? AND lifecycle_state = 'active'",
    )
    .bind(media_id)
    .bind(expected_generation)
    .bind(expected_version)
    .execute(&mut *conn)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(ApiError::precondition_failed("Phiên bản media đã thay đổi đồng thời.").into());
    }

    let job_id = crate::auth::generate_random_token();
    let shard = &media_id[..2];
    let original_rel_path = format!("media/{shard}/{media_id}.{extension}");
    let quarantine_original_rel_path = format!(".quarantine/{job_id}/{media_id}.{extension}");
    let (thumbnail_rel_path, quarantine_thumbnail_rel_path) = if kind == "image" {
        (
            Some(format!("media/{shard}/{media_id}.thumb.png")),
            Some(format!(".quarantine/{job_id}/{media_id}.thumb.png")),
        )
    } else {
        (None, None)
    };

    sqlx::query(
        r#"
        INSERT INTO media_delete_jobs (
            job_id, media_id, generation, expected_version, phase,
            original_rel_path, thumbnail_rel_path,
            quarantine_original_rel_path, quarantine_thumbnail_rel_path
        )
        VALUES (?, ?, ?, ?, 'tombstoned', ?, ?, ?, ?)
        "#,
    )
    .bind(&job_id)
    .bind(media_id)
    .bind(expected_generation)
    .bind(expected_version)
    .bind(&original_rel_path)
    .bind(&thumbnail_rel_path)
    .bind(&quarantine_original_rel_path)
    .bind(&quarantine_thumbnail_rel_path)
    .execute(&mut *conn)
    .await?;

    get_delete_job_in_conn(&mut *conn, &job_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được media delete job.").into())
}

pub async fn set_delete_job_error(db: &SqlitePool, job_id: &str, code: &str) -> ApiResult<()> {
    sqlx::query(
        "UPDATE media_delete_jobs SET error_code = ?, updated_at = datetime('now') WHERE job_id = ?",
    )
    .bind(code)
    .bind(job_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn mark_delete_job_moving(db: &SqlitePool, job_id: &str) -> ApiResult<()> {
    sqlx::query(
        "UPDATE media_delete_jobs SET phase = 'moving', error_code = NULL, updated_at = datetime('now') WHERE job_id = ? AND phase = 'tombstoned'",
    )
    .bind(job_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn mark_original_moved(db: &SqlitePool, job_id: &str) -> ApiResult<()> {
    sqlx::query(
        "UPDATE media_delete_jobs SET original_moved = 1, error_code = NULL, updated_at = datetime('now') WHERE job_id = ?",
    )
    .bind(job_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn mark_thumbnail_moved(db: &SqlitePool, job_id: &str) -> ApiResult<()> {
    sqlx::query(
        "UPDATE media_delete_jobs SET thumbnail_moved = 1, error_code = NULL, updated_at = datetime('now') WHERE job_id = ?",
    )
    .bind(job_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn finalize_delete_job(
    db: &SqlitePool,
    job_id: &str,
    media_id: &str,
    generation: i64,
) -> ApiResult<()> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE media_delete_jobs SET phase = 'quarantined', error_code = NULL, updated_at = datetime('now') WHERE job_id = ? AND phase IN ('tombstoned', 'moving')",
    )
    .bind(job_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM media_assets WHERE id = ? AND generation = ? AND lifecycle_state = 'deleting'",
    )
    .bind(media_id)
    .bind(generation)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE media_delete_jobs SET phase = 'finalized', error_code = NULL, completed_at = COALESCE(completed_at, datetime('now')), updated_at = datetime('now') WHERE job_id = ?",
    )
    .bind(job_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
}

pub async fn list_pending_delete_jobs(db: &SqlitePool) -> ApiResult<Vec<MediaDeleteJob>> {
    let rows = sqlx::query(
        r#"
        SELECT job_id, media_id, generation, expected_version, phase,
               original_rel_path, thumbnail_rel_path,
               quarantine_original_rel_path, quarantine_thumbnail_rel_path,
               original_moved, thumbnail_moved, error_code,
               created_at, updated_at, completed_at
        FROM media_delete_jobs
        WHERE phase != 'finalized'
        ORDER BY created_at ASC, job_id ASC
        "#,
    )
    .fetch_all(db)
    .await?;

    rows.iter().map(map_delete_job_row).collect()
}

#[allow(dead_code)]
pub async fn list_all_delete_jobs(db: &SqlitePool) -> ApiResult<Vec<MediaDeleteJob>> {
    let rows = sqlx::query(
        r#"
        SELECT job_id, media_id, generation, expected_version, phase,
               original_rel_path, thumbnail_rel_path,
               quarantine_original_rel_path, quarantine_thumbnail_rel_path,
               original_moved, thumbnail_moved, error_code,
               created_at, updated_at, completed_at
        FROM media_delete_jobs
        ORDER BY created_at DESC, job_id DESC
        LIMIT 200
        "#,
    )
    .fetch_all(db)
    .await?;

    rows.iter().map(map_delete_job_row).collect()
}

pub async fn find_delete_jobs_by_ids(
    db: &SqlitePool,
    job_ids: &[String],
) -> ApiResult<std::collections::HashMap<String, MediaDeleteJob>> {
    let mut out = std::collections::HashMap::new();
    if job_ids.is_empty() {
        return Ok(out);
    }
    for chunk in job_ids.chunks(200) {
        let mut qb: sqlx::QueryBuilder<sqlx::Sqlite> = sqlx::QueryBuilder::new(
            "SELECT job_id, media_id, generation, expected_version, phase,
                    original_rel_path, thumbnail_rel_path,
                    quarantine_original_rel_path, quarantine_thumbnail_rel_path,
                    original_moved, thumbnail_moved, error_code,
                    created_at, updated_at, completed_at
             FROM media_delete_jobs
             WHERE job_id IN (",
        );
        let mut sep = qb.separated(", ");
        for id in chunk {
            sep.push_bind(id);
        }
        sep.push_unseparated(")");
        for row in qb.build().fetch_all(db).await? {
            let job = map_delete_job_row(&row)?;
            out.insert(job.job_id.clone(), job);
        }
    }
    Ok(out)
}
