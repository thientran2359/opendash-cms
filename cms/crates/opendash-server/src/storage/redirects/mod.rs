mod graph_and_import;
mod not_found_logs;
mod rules;

pub use graph_and_import::*;
pub use not_found_logs::*;
pub use rules::*;

use std::collections::HashMap;

use opendash_core::redirects::{CsvRowError, MAX_NOT_FOUND_LOG_ROWS, RedirectValidationError};
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

#[derive(Debug)]
pub enum RedirectStorageError {
    Database(sqlx::Error),
    Json(serde_json::Error),
    Validation(RedirectValidationError),
    Row(CsvRowError),
    NotFound,
    PreconditionFailed,
    ConflictSourcePath,
    CycleOrChainExceeded,
    QuotaExceeded,
    InvalidBulkDeleteIds,
    InvalidClearDays,
    InvalidFilter,
}

impl From<sqlx::Error> for RedirectStorageError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

impl From<serde_json::Error> for RedirectStorageError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<RedirectValidationError> for RedirectStorageError {
    fn from(value: RedirectValidationError) -> Self {
        Self::Validation(value)
    }
}

impl From<CsvRowError> for RedirectStorageError {
    fn from(value: CsvRowError) -> Self {
        Self::Row(value)
    }
}

impl RedirectStorageError {
    pub fn from_api_err(err: crate::error::ApiError) -> Self {
        Self::Database(sqlx::Error::Protocol(err.1.into_owned()))
    }
}

pub type RedirectStorageResult<T> = Result<T, RedirectStorageError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveManualRedirect {
    pub id: i64,
    pub target_url: String,
    pub status_code: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManualRedirectItem {
    pub id: i64,
    pub source_path: String,
    pub target_url: String,
    pub status_code: u16,
    pub enabled: bool,
    pub note: String,
    pub hit_count: i64,
    pub last_hit_at: Option<String>,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub shadowed_by_live_route: bool,
    pub shadowed_target_kind: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TelemetryRedirectHit {
    pub count: u64,
    pub last_hit_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct TelemetryNotFoundHit {
    pub count: u64,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub last_referrer: Option<String>,
    pub last_user_agent: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TelemetryFlushBatch {
    pub redirect_hits: HashMap<i64, TelemetryRedirectHit>,
    pub not_found_hits: HashMap<String, TelemetryNotFoundHit>,
}

impl TelemetryFlushBatch {
    pub fn is_empty(&self) -> bool {
        self.redirect_hits.is_empty() && self.not_found_hits.is_empty()
    }
}

/// Resolves effective public origin from `site_public_settings.public_origin`,
/// falling back to the request header origin when no public origin is configured.
pub(super) async fn resolve_site_origin_in_conn(
    conn: &mut SqliteConnection,
    header_origin: Option<&str>,
) -> RedirectStorageResult<Option<String>> {
    let configured: Option<Option<String>> =
        sqlx::query_scalar("SELECT public_origin FROM site_public_settings WHERE id = 1")
            .fetch_optional(&mut *conn)
            .await?;
    let trimmed = configured
        .flatten()
        .map(|s| s.trim().trim_end_matches('/').to_owned())
        .filter(|s| !s.is_empty());
    Ok(trimmed.or_else(|| header_origin.map(str::to_owned)))
}

/// Fast single-row lookup along the `source_path COLLATE BINARY` unique index.
pub async fn find_active_manual_redirect(
    db: &SqlitePool,
    source_path: &str,
) -> RedirectStorageResult<Option<ActiveManualRedirect>> {
    let row: Option<(i64, String, i64)> = sqlx::query_as(
        "SELECT id, target_url, status_code
           FROM manual_redirects
          WHERE source_path = ? AND enabled = 1",
    )
    .bind(source_path)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(id, target_url, code)| ActiveManualRedirect {
        id,
        target_url,
        status_code: u16::try_from(code).unwrap_or(301),
    }))
}

pub(crate) async fn flush_telemetry_batch_in_tx(
    tx: &mut SqliteConnection,
    batch: &TelemetryFlushBatch,
) -> RedirectStorageResult<()> {
    for (&redirect_id, hit) in &batch.redirect_hits {
        let count_i64 = i64::try_from(hit.count).unwrap_or(i64::MAX);
        sqlx::query(
            "UPDATE manual_redirects
                SET hit_count = hit_count + ?,
                    last_hit_at = CASE
                        WHEN last_hit_at IS NULL OR last_hit_at < ? THEN ?
                        ELSE last_hit_at
                    END
              WHERE id = ?",
        )
        .bind(count_i64)
        .bind(&hit.last_hit_at)
        .bind(&hit.last_hit_at)
        .bind(redirect_id)
        .execute(&mut *tx)
        .await?;
    }

    for (path, entry) in &batch.not_found_hits {
        let count_i64 = i64::try_from(entry.count).unwrap_or(i64::MAX);
        sqlx::query(
            "INSERT INTO not_found_logs (
                path, hit_count, first_seen_at, last_seen_at, last_referrer, last_user_agent
             ) VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(path) DO UPDATE SET
                hit_count = not_found_logs.hit_count + excluded.hit_count,
                first_seen_at = MIN(not_found_logs.first_seen_at, excluded.first_seen_at),
                last_seen_at = MAX(not_found_logs.last_seen_at, excluded.last_seen_at),
                last_referrer = CASE
                    WHEN excluded.last_seen_at >= not_found_logs.last_seen_at
                    THEN COALESCE(excluded.last_referrer, not_found_logs.last_referrer)
                    ELSE not_found_logs.last_referrer
                END,
                last_user_agent = CASE
                    WHEN excluded.last_seen_at >= not_found_logs.last_seen_at
                    THEN COALESCE(excluded.last_user_agent, not_found_logs.last_user_agent)
                    ELSE not_found_logs.last_user_agent
                END",
        )
        .bind(path)
        .bind(count_i64)
        .bind(&entry.first_seen_at)
        .bind(&entry.last_seen_at)
        .bind(&entry.last_referrer)
        .bind(&entry.last_user_agent)
        .execute(&mut *tx)
        .await?;
    }

    if !batch.not_found_hits.is_empty() {
        prune_not_found_logs_in_tx(tx, MAX_NOT_FOUND_LOG_ROWS).await?;
    }
    Ok(())
}

pub async fn flush_telemetry_batch(
    db: &SqlitePool,
    batch: &TelemetryFlushBatch,
) -> RedirectStorageResult<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    flush_telemetry_batch_in_tx(&mut tx, batch).await?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn prune_not_found_logs_in_tx(
    tx: &mut SqliteConnection,
    max_rows: usize,
) -> RedirectStorageResult<()> {
    let limit = i64::try_from(max_rows).unwrap_or(5_000);
    sqlx::query(
        "DELETE FROM not_found_logs
          WHERE id IN (
              SELECT id FROM not_found_logs
               ORDER BY hit_count DESC, last_seen_at DESC, id DESC
               LIMIT -1 OFFSET ?
          )",
    )
    .bind(limit)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

pub(super) fn escape_like_pattern(q: &str) -> String {
    let mut out = String::with_capacity(q.len() + 2);
    out.push('%');
    for ch in q.chars() {
        match ch {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out.push('%');
    out
}
