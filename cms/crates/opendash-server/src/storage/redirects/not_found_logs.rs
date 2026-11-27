use std::collections::HashSet;

use opendash_core::redirects::MAX_NOT_FOUND_LOG_ROWS;
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use super::{
    RedirectStorageError, RedirectStorageResult, TelemetryFlushBatch, escape_like_pattern,
    flush_telemetry_batch_in_tx,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotFoundSort {
    HitsDesc,
    RecentDesc,
    OldestAsc,
}

impl NotFoundSort {
    pub fn parse(raw: Option<&str>) -> RedirectStorageResult<Self> {
        match raw.unwrap_or("hits_desc").trim() {
            "hits_desc" => Ok(Self::HitsDesc),
            "recent_desc" => Ok(Self::RecentDesc),
            "oldest_asc" => Ok(Self::OldestAsc),
            _ => Err(RedirectStorageError::InvalidFilter),
        }
    }

    fn order_sql(self) -> &'static str {
        match self {
            Self::HitsDesc => {
                "SELECT id, path, hit_count, first_seen_at, last_seen_at, last_referrer, last_user_agent
                   FROM not_found_logs
                  WHERE (? IS NULL OR path LIKE ? ESCAPE '\\' OR COALESCE(last_referrer, '') LIKE ? ESCAPE '\\')
                  ORDER BY hit_count DESC, last_seen_at DESC, id DESC
                  LIMIT ? OFFSET ?"
            }
            Self::RecentDesc => {
                "SELECT id, path, hit_count, first_seen_at, last_seen_at, last_referrer, last_user_agent
                   FROM not_found_logs
                  WHERE (? IS NULL OR path LIKE ? ESCAPE '\\' OR COALESCE(last_referrer, '') LIKE ? ESCAPE '\\')
                  ORDER BY last_seen_at DESC, id DESC
                  LIMIT ? OFFSET ?"
            }
            Self::OldestAsc => {
                "SELECT id, path, hit_count, first_seen_at, last_seen_at, last_referrer, last_user_agent
                   FROM not_found_logs
                  WHERE (? IS NULL OR path LIKE ? ESCAPE '\\' OR COALESCE(last_referrer, '') LIKE ? ESCAPE '\\')
                  ORDER BY first_seen_at ASC, id ASC
                  LIMIT ? OFFSET ?"
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NotFoundLogItem {
    pub id: i64,
    pub path: String,
    pub hit_count: i64,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub last_referrer: Option<String>,
    pub last_user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotFoundLogPage {
    pub items: Vec<NotFoundLogItem>,
    pub total: usize,
    pub page: i64,
    pub per_page: i64,
    pub max_rows: usize,
}

type RawNotFoundRow = (
    i64,
    String,
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
);

pub async fn list_not_found_logs(
    db: &SqlitePool,
    page: i64,
    per_page: i64,
    q: Option<&str>,
    sort: NotFoundSort,
) -> RedirectStorageResult<NotFoundLogPage> {
    if page < 1 || !(1..=100).contains(&per_page) {
        return Err(RedirectStorageError::InvalidFilter);
    }
    let trimmed_q = q.map(str::trim).filter(|s| !s.is_empty());
    if trimmed_q.is_some_and(|s| s.chars().count() > 256) {
        return Err(RedirectStorageError::InvalidFilter);
    }
    let like_q = trimmed_q.map(escape_like_pattern);

    let mut conn = db.acquire().await?;
    let total_i64: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM not_found_logs
          WHERE (? IS NULL OR path LIKE ? ESCAPE '\\' OR COALESCE(last_referrer, '') LIKE ? ESCAPE '\\')",
    )
    .bind(&like_q)
    .bind(&like_q)
    .bind(&like_q)
    .fetch_one(&mut *conn)
    .await?;

    let offset = (page - 1) * per_page;
    let rows: Vec<RawNotFoundRow> = sqlx::query_as(sort.order_sql())
        .bind(&like_q)
        .bind(&like_q)
        .bind(&like_q)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&mut *conn)
        .await?;

    Ok(NotFoundLogPage {
        items: rows
            .into_iter()
            .map(
                |(
                    id,
                    path,
                    hit_count,
                    first_seen_at,
                    last_seen_at,
                    last_referrer,
                    last_user_agent,
                )| {
                    NotFoundLogItem {
                        id,
                        path,
                        hit_count,
                        first_seen_at,
                        last_seen_at,
                        last_referrer,
                        last_user_agent,
                    }
                },
            )
            .collect(),
        total: usize::try_from(total_i64).unwrap_or(0),
        page,
        per_page,
        max_rows: MAX_NOT_FOUND_LOG_ROWS,
    })
}

pub async fn delete_not_found_log(db: &SqlitePool, id: i64) -> RedirectStorageResult<String> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let deleted_path: Option<String> =
        sqlx::query_scalar("DELETE FROM not_found_logs WHERE id = ? RETURNING path")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(path) = deleted_path else {
        return Err(RedirectStorageError::NotFound);
    };
    tx.commit().await?;
    Ok(path)
}

pub async fn bulk_delete_not_found_logs(
    db: &SqlitePool,
    ids: &[i64],
) -> RedirectStorageResult<Vec<String>> {
    if ids.is_empty() || ids.len() > 100 {
        return Err(RedirectStorageError::InvalidBulkDeleteIds);
    }
    let mut seen = HashSet::with_capacity(ids.len());
    for &id in ids {
        if id <= 0 || !seen.insert(id) {
            return Err(RedirectStorageError::InvalidBulkDeleteIds);
        }
    }
    let ids_json = serde_json::to_string(ids)?;
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let deleted_paths: Vec<String> = sqlx::query_scalar(
        "DELETE FROM not_found_logs
          WHERE id IN (SELECT value FROM json_each(?))
         RETURNING path",
    )
    .bind(ids_json)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(deleted_paths)
}

pub async fn clear_not_found_logs_with_preflush_in_tx(
    tx: &mut SqliteConnection,
    older_than_days: Option<u32>,
    pending_batch: Option<&TelemetryFlushBatch>,
) -> RedirectStorageResult<usize> {
    let deleted = match older_than_days {
        None => {
            let res = sqlx::query("DELETE FROM not_found_logs")
                .execute(&mut *tx)
                .await?;
            usize::try_from(res.rows_affected()).unwrap_or(0)
        }
        Some(days) => {
            if days == 0 || days > 3650 {
                return Err(RedirectStorageError::InvalidClearDays);
            }
            if let Some(batch) = pending_batch {
                if !batch.is_empty() {
                    flush_telemetry_batch_in_tx(tx, batch).await?;
                }
            }
            let modifier = format!("-{days} days");
            let res = sqlx::query(
                "DELETE FROM not_found_logs
                  WHERE last_seen_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?)",
            )
            .bind(modifier)
            .execute(&mut *tx)
            .await?;
            usize::try_from(res.rows_affected()).unwrap_or(0)
        }
    };
    Ok(deleted)
}

pub async fn clear_not_found_logs(
    db: &SqlitePool,
    older_than_days: Option<u32>,
    pending_batch: Option<&TelemetryFlushBatch>,
) -> RedirectStorageResult<usize> {
    if let Some(days) = older_than_days {
        if days == 0 || days > 3650 {
            return Err(RedirectStorageError::InvalidClearDays);
        }
    }
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let deleted =
        clear_not_found_logs_with_preflush_in_tx(&mut tx, older_than_days, pending_batch).await?;
    tx.commit().await?;
    Ok(deleted)
}
