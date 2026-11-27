use std::collections::BTreeMap;

use opendash_core::redirects::{
    MAX_MANUAL_REDIRECTS_TOTAL, ManualRedirectCsvRow, ManualRedirectInput, RedirectValidationError,
    check_not_self_redirect, normalize_redirect_note, normalize_redirect_source_path,
    normalize_redirect_target_url,
};
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use super::{
    ManualRedirectItem, RedirectStorageError, RedirectStorageResult, escape_like_pattern,
    resolve_site_origin_in_conn, validate_redirect_graph_in_conn,
};
use crate::storage::permalinks::find_live_claim_kinds_for_paths_in_conn;

#[derive(Debug, Clone)]
pub struct RedirectListFilter {
    pub page: i64,
    pub per_page: i64,
    pub q: Option<String>,
    pub status: Option<u16>,
    pub enabled: Option<bool>,
    pub shadowed: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RedirectListPage {
    pub items: Vec<ManualRedirectItem>,
    pub total: usize,
    pub page: i64,
    pub per_page: i64,
    pub shadowed_count: usize,
}

#[derive(Debug, Clone)]
pub struct UpdateManualRedirectOutcome {
    pub item: ManualRedirectItem,
    pub should_evict_hits: bool,
}

pub(super) type RawRuleRow = (
    i64,
    String,
    String,
    i64,
    i64,
    String,
    i64,
    Option<String>,
    i64,
    String,
    String,
);

pub(super) fn map_rule_row(
    row: RawRuleRow,
    live_claims: &BTreeMap<String, &'static str>,
) -> ManualRedirectItem {
    let kind = live_claims.get(&row.1).copied();
    ManualRedirectItem {
        id: row.0,
        source_path: row.1,
        target_url: row.2,
        status_code: u16::try_from(row.3).unwrap_or(301),
        enabled: row.4 != 0,
        note: row.5,
        hit_count: row.6,
        last_hit_at: row.7,
        version: row.8,
        created_at: row.9,
        updated_at: row.10,
        shadowed_by_live_route: kind.is_some(),
        shadowed_target_kind: kind.map(str::to_owned),
    }
}

async fn count_enabled_shadowed_in_conn(
    conn: &mut SqliteConnection,
) -> RedirectStorageResult<usize> {
    let paths: Vec<String> =
        sqlx::query_scalar("SELECT source_path FROM manual_redirects WHERE enabled = 1")
            .fetch_all(&mut *conn)
            .await?;
    if paths.is_empty() {
        return Ok(0);
    }
    let claims = find_live_claim_kinds_for_paths_in_conn(conn, &paths)
        .await
        .map_err(RedirectStorageError::from_api_err)?;
    Ok(paths.iter().filter(|p| claims.contains_key(*p)).count())
}

pub async fn list_manual_redirects(
    db: &SqlitePool,
    filter: &RedirectListFilter,
) -> RedirectStorageResult<RedirectListPage> {
    if filter.page < 1 || !(1..=100).contains(&filter.per_page) {
        return Err(RedirectStorageError::InvalidFilter);
    }
    if let Some(code) = filter.status {
        if code != 301 && code != 302 {
            return Err(RedirectStorageError::InvalidFilter);
        }
    }
    let trimmed_q = filter.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if trimmed_q.is_some_and(|s| s.chars().count() > 256) {
        return Err(RedirectStorageError::InvalidFilter);
    }
    let like_q = trimmed_q.map(escape_like_pattern);

    let mut conn = db.acquire().await?;
    let shadowed_count = count_enabled_shadowed_in_conn(&mut conn).await?;

    let offset = (filter.page - 1) * filter.per_page;
    let rows = if filter.shadowed.is_none() {
        sqlx::query_as::<_, RawRuleRow>(
            "SELECT id, source_path, target_url, status_code, enabled, note, hit_count, last_hit_at, version, created_at, updated_at
               FROM manual_redirects
              WHERE (? IS NULL OR source_path LIKE ? ESCAPE '\\' OR target_url LIKE ? ESCAPE '\\' OR note LIKE ? ESCAPE '\\')
                AND (? IS NULL OR status_code = ?)
                AND (? IS NULL OR enabled = ?)
              ORDER BY updated_at DESC, id DESC
              LIMIT ? OFFSET ?",
        )
        .bind(&like_q)
        .bind(&like_q)
        .bind(&like_q)
        .bind(&like_q)
        .bind(filter.status.map(i64::from))
        .bind(filter.status.map(i64::from))
        .bind(filter.enabled.map(i64::from))
        .bind(filter.enabled.map(i64::from))
        .bind(filter.per_page)
        .bind(offset)
        .fetch_all(&mut *conn)
        .await?
    } else {
        sqlx::query_as::<_, RawRuleRow>(
            "SELECT id, source_path, target_url, status_code, enabled, note, hit_count, last_hit_at, version, created_at, updated_at
               FROM manual_redirects
              WHERE (? IS NULL OR source_path LIKE ? ESCAPE '\\' OR target_url LIKE ? ESCAPE '\\' OR note LIKE ? ESCAPE '\\')
                AND (? IS NULL OR status_code = ?)
                AND (? IS NULL OR enabled = ?)
              ORDER BY updated_at DESC, id DESC",
        )
        .bind(&like_q)
        .bind(&like_q)
        .bind(&like_q)
        .bind(&like_q)
        .bind(filter.status.map(i64::from))
        .bind(filter.status.map(i64::from))
        .bind(filter.enabled.map(i64::from))
        .bind(filter.enabled.map(i64::from))
        .fetch_all(&mut *conn)
        .await?
    };
    let paths: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
    let claims = find_live_claim_kinds_for_paths_in_conn(&mut conn, &paths)
        .await
        .map_err(RedirectStorageError::from_api_err)?;

    if let Some(want_shadowed) = filter.shadowed {
        let filtered: Vec<ManualRedirectItem> = rows
            .into_iter()
            .map(|r| map_rule_row(r, &claims))
            .filter(|item| item.shadowed_by_live_route == want_shadowed)
            .collect();
        let total = filtered.len();
        let start = usize::try_from(offset).unwrap_or(usize::MAX).min(total);
        let end = start
            .saturating_add(usize::try_from(filter.per_page).unwrap_or(50))
            .min(total);
        return Ok(RedirectListPage {
            items: filtered[start..end].to_vec(),
            total,
            page: filter.page,
            per_page: filter.per_page,
            shadowed_count,
        });
    }

    let total_i64: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM manual_redirects
          WHERE (? IS NULL OR source_path LIKE ? ESCAPE '\\' OR target_url LIKE ? ESCAPE '\\' OR note LIKE ? ESCAPE '\\')
            AND (? IS NULL OR status_code = ?)
            AND (? IS NULL OR enabled = ?)",
    )
    .bind(&like_q)
    .bind(&like_q)
    .bind(&like_q)
    .bind(&like_q)
    .bind(filter.status.map(i64::from))
    .bind(filter.status.map(i64::from))
    .bind(filter.enabled.map(i64::from))
    .bind(filter.enabled.map(i64::from))
    .fetch_one(&mut *conn)
    .await?;

    Ok(RedirectListPage {
        items: rows.into_iter().map(|r| map_rule_row(r, &claims)).collect(),
        total: usize::try_from(total_i64).unwrap_or(0),
        page: filter.page,
        per_page: filter.per_page,
        shadowed_count,
    })
}

pub async fn get_manual_redirect_by_id(
    db: &SqlitePool,
    id: i64,
) -> RedirectStorageResult<ManualRedirectItem> {
    let mut conn = db.acquire().await?;
    let row: RawRuleRow = sqlx::query_as(
        "SELECT id, source_path, target_url, status_code, enabled, note, hit_count, last_hit_at, version, created_at, updated_at
           FROM manual_redirects WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(RedirectStorageError::NotFound)?;
    let claims = find_live_claim_kinds_for_paths_in_conn(&mut conn, std::slice::from_ref(&row.1))
        .await
        .map_err(RedirectStorageError::from_api_err)?;
    Ok(map_rule_row(row, &claims))
}

pub async fn create_manual_redirect(
    db: &SqlitePool,
    raw_source_path: &str,
    raw_target_url: &str,
    raw_status_code: u16,
    enabled: bool,
    raw_note: Option<&str>,
    resolve_not_found: bool,
    public_origin: Option<&str>,
) -> RedirectStorageResult<ManualRedirectItem> {
    let source_path = normalize_redirect_source_path(raw_source_path)?;
    let target_url = normalize_redirect_target_url(raw_target_url)?;
    let status_code = opendash_core::redirects::RedirectStatusCode::try_from(raw_status_code)?;
    let note = normalize_redirect_note(raw_note)?;

    let input = ManualRedirectInput {
        source_path: source_path.clone(),
        target_url: target_url.clone(),
        status_code,
        enabled,
        note: note.clone(),
    };

    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;

    let effective_origin = resolve_site_origin_in_conn(&mut tx, public_origin).await?;
    check_not_self_redirect(&source_path, &target_url, effective_origin.as_deref())?;

    let existing_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM manual_redirects WHERE source_path = ?")
            .bind(&input.source_path)
            .fetch_optional(&mut *tx)
            .await?;
    if existing_id.is_some() {
        return Err(RedirectStorageError::ConflictSourcePath);
    }

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM manual_redirects")
        .fetch_one(&mut *tx)
        .await?;
    if usize::try_from(total).unwrap_or(usize::MAX) >= MAX_MANUAL_REDIRECTS_TOTAL {
        return Err(RedirectStorageError::QuotaExceeded);
    }

    if input.enabled {
        validate_redirect_graph_in_conn(
            &mut tx,
            std::slice::from_ref(&input),
            None,
            effective_origin.as_deref(),
        )
        .await?;
    }

    let row: RawRuleRow = sqlx::query_as(
        "INSERT INTO manual_redirects (source_path, target_url, status_code, enabled, note)
         VALUES (?, ?, ?, ?, ?)
         RETURNING id, source_path, target_url, status_code, enabled, note, hit_count, last_hit_at, version, created_at, updated_at",
    )
    .bind(&input.source_path)
    .bind(&input.target_url)
    .bind(input.status_code.as_i64())
    .bind(i64::from(input.enabled))
    .bind(&input.note)
    .fetch_one(&mut *tx)
    .await?;

    if resolve_not_found {
        sqlx::query("DELETE FROM not_found_logs WHERE path = ?")
            .bind(&input.source_path)
            .execute(&mut *tx)
            .await?;
    }

    let claims = find_live_claim_kinds_for_paths_in_conn(&mut tx, std::slice::from_ref(&row.1))
        .await
        .map_err(RedirectStorageError::from_api_err)?;
    tx.commit().await?;
    Ok(map_rule_row(row, &claims))
}

#[allow(clippy::too_many_arguments)]
pub async fn update_manual_redirect_cas(
    db: &SqlitePool,
    id: i64,
    expected_version: i64,
    raw_source_path: &str,
    raw_target_url: &str,
    raw_status_code: u16,
    enabled: bool,
    raw_note: Option<&str>,
    public_origin: Option<&str>,
) -> RedirectStorageResult<UpdateManualRedirectOutcome> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;

    let current: (i64, String) =
        sqlx::query_as("SELECT version, source_path FROM manual_redirects WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(RedirectStorageError::NotFound)?;
    if current.0 != expected_version {
        return Err(RedirectStorageError::PreconditionFailed);
    }

    let effective_origin = resolve_site_origin_in_conn(&mut tx, public_origin).await?;
    let source_path = normalize_redirect_source_path(raw_source_path)?;
    let target_url = normalize_redirect_target_url(raw_target_url)?;
    check_not_self_redirect(&source_path, &target_url, effective_origin.as_deref())?;
    let status_code = opendash_core::redirects::RedirectStatusCode::try_from(raw_status_code)?;
    let note = normalize_redirect_note(raw_note)?;

    let duplicate: Option<i64> =
        sqlx::query_scalar("SELECT id FROM manual_redirects WHERE source_path = ? AND id != ?")
            .bind(&source_path)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if duplicate.is_some() {
        return Err(RedirectStorageError::ConflictSourcePath);
    }

    let input = ManualRedirectInput {
        source_path: source_path.clone(),
        target_url,
        status_code,
        enabled,
        note,
    };
    if input.enabled {
        validate_redirect_graph_in_conn(
            &mut tx,
            std::slice::from_ref(&input),
            Some(id),
            effective_origin.as_deref(),
        )
        .await?;
    }

    let row: RawRuleRow = sqlx::query_as(
        "UPDATE manual_redirects
            SET source_path = ?,
                target_url = ?,
                status_code = ?,
                enabled = ?,
                note = ?,
                version = version + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
          WHERE id = ? AND version = ?
         RETURNING id, source_path, target_url, status_code, enabled, note, hit_count, last_hit_at, version, created_at, updated_at",
    )
    .bind(&input.source_path)
    .bind(&input.target_url)
    .bind(input.status_code.as_i64())
    .bind(i64::from(input.enabled))
    .bind(&input.note)
    .bind(id)
    .bind(expected_version)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(RedirectStorageError::PreconditionFailed)?;

    let claims = find_live_claim_kinds_for_paths_in_conn(&mut tx, std::slice::from_ref(&row.1))
        .await
        .map_err(RedirectStorageError::from_api_err)?;
    tx.commit().await?;
    let should_evict_hits = !enabled || current.1 != source_path;
    Ok(UpdateManualRedirectOutcome {
        item: map_rule_row(row, &claims),
        should_evict_hits,
    })
}

pub async fn delete_manual_redirect(db: &SqlitePool, id: i64) -> RedirectStorageResult<()> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let res = sqlx::query("DELETE FROM manual_redirects WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if res.rows_affected() == 0 {
        return Err(RedirectStorageError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn export_manual_redirects_csv(
    db: &SqlitePool,
) -> RedirectStorageResult<Vec<ManualRedirectCsvRow>> {
    let mut conn = db.acquire().await?;
    let rows: Vec<(String, String, i64, i64, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT source_path, target_url, status_code, enabled, note, hit_count, last_hit_at
           FROM manual_redirects
          ORDER BY id ASC",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(source_path, target_url, status_code, enabled, note, hit_count, last_hit_at)| {
                ManualRedirectCsvRow {
                    source_path,
                    target_url,
                    status_code: u16::try_from(status_code).unwrap_or(301),
                    enabled: enabled != 0,
                    note,
                    hit_count,
                    last_hit_at,
                }
            },
        )
        .collect())
}

pub(super) fn validation_or_db(err: RedirectValidationError) -> RedirectStorageError {
    RedirectStorageError::Validation(err)
}
