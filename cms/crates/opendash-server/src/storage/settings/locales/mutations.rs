//! SITE-01 typed General Settings. SQL remains in storage.
use std::sync::LazyLock;

use std::collections::{BTreeMap, BTreeSet};

use opendash_core::{
    field_schema::{
        CollectedMediaIds, FieldContext, FieldSpec, FieldType, collect_media_ids,
        default_value_for_field_spec, global_site_option_defaults, global_site_option_fields,
        valid_sha256_media_id, validate_field_values,
    },
    i18n::{
        CMS_CATALOG_VERSION, CMS_LANGUAGE_CATALOG, CmsLocaleOption, find_cms_locale,
        resolve_effective_ui_locale,
    },
};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use crate::error::{ApiError, ApiResult};

use super::orphans::count_locale_data_in_conn;
use super::queries::*;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetMultilingualEnabledInput {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeleteLocaleResponse {
    pub snapshot: SiteLocalesSnapshot,
    pub deleted_code: String,
    pub soft_deleted: bool,
    pub orphan_total: i64,
}

pub async fn set_multilingual_enabled(
    db: &SqlitePool,
    expected_version: i64,
    input: SetMultilingualEnabledInput,
) -> Result<SiteLocalesSnapshot, LocaleMutationError> {
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    require_locale_version_in_conn(&mut tx, expected_version).await?;
    if multilingual_enabled_in_conn(&mut tx).await? != input.enabled {
        sqlx::query("UPDATE site_locales_state SET multilingual_enabled = ?, version = version + 1 WHERE id = 1")
            .bind(input.enabled)
            .execute(&mut *tx)
            .await?;
        crate::storage::permalinks::sync_claims_with_options(&mut tx, true).await?;
        crate::storage::search::rebuild_search_index_in_conn(&mut tx).await?;
    }
    let snapshot = list_locales_in_conn(&mut tx).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(snapshot)
}

pub(in crate::storage::settings) async fn reassign_locale_patterns_to_default_in_tx(
    conn: &mut SqliteConnection,
    from_locale: &str,
    default_locale: &str,
) -> ApiResult<()> {
    let patterns: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, slug FROM content_entries WHERE content_type_key = 'pattern' AND locale = ? ORDER BY id",
    ).bind(from_locale).fetch_all(&mut *conn).await?;
    if patterns.is_empty() {
        return Ok(());
    }
    sqlx::query("UPDATE site_locales_state SET rename_in_progress = 1 WHERE id = 1")
        .execute(&mut *conn)
        .await?;
    for (id, original) in patterns {
        let mut slug = original.clone();
        let mut suffix_no = 0u64;
        loop {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = 'pattern' AND locale = ? AND slug = ?)",
            ).bind(default_locale).bind(&slug).fetch_one(&mut *conn).await?;
            if !exists {
                break;
            }
            let suffix = if suffix_no == 0 {
                format!("-{id}")
            } else {
                format!("-{id}-{suffix_no}")
            };
            let mut end = original.len().min(64usize.saturating_sub(suffix.len()));
            while !original.is_char_boundary(end) {
                end -= 1;
            }
            slug = format!("{}{}", original[..end].trim_end_matches('-'), suffix);
            suffix_no += 1;
        }
        sqlx::query("UPDATE content_entries SET locale = ?, slug = ? WHERE id = ?")
            .bind(default_locale)
            .bind(&slug)
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("UPDATE site_locales_state SET rename_in_progress = 0 WHERE id = 1")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn delete_locale(
    db: &SqlitePool,
    raw_code: &str,
    expected_version: i64,
) -> Result<DeleteLocaleResponse, LocaleMutationError> {
    let code = raw_code.trim().to_ascii_lowercase();
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    require_locale_version_in_conn(&mut tx, expected_version).await?;
    let is_default: Option<bool> = sqlx::query_scalar(
        "SELECT is_default FROM site_locales WHERE code = ? AND deleted_at IS NULL",
    )
    .bind(&code)
    .fetch_optional(&mut *tx)
    .await?;
    match is_default {
        None => return Err(ApiError::not_found("Không tìm thấy ngôn ngữ.").into()),
        Some(true) => {
            return Err(ApiError::unprocessable("Không thể xóa ngôn ngữ mặc định.").into());
        }
        Some(false) => {}
    }
    let counts = count_locale_data_in_conn(&mut tx, &code).await?;
    sqlx::query("UPDATE site_locales SET enabled = 0, deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE code = ?")
        .bind(&code).execute(&mut *tx).await?;
    crate::storage::permalinks::sync_claims_with_options(&mut tx, true).await?;
    let soft_deleted = counts.total > 0;
    if !soft_deleted {
        let default_code = get_default_locale_code_in_conn(&mut tx).await?;
        reassign_locale_patterns_to_default_in_tx(&mut tx, &code, &default_code).await?;
        sqlx::query("DELETE FROM permalink_route_claims WHERE locale = ?")
            .bind(&code)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM site_locales WHERE code = ?")
            .bind(&code)
            .execute(&mut *tx)
            .await?;
    }
    crate::storage::search::delete_locale_search_index_in_conn(&mut tx, &code).await?;
    sqlx::query("UPDATE site_locales_state SET version = version + 1 WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let snapshot = list_locales_in_conn(&mut tx).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(DeleteLocaleResponse {
        snapshot,
        deleted_code: code,
        soft_deleted,
        orphan_total: counts.total,
    })
}

/// Every DELETE statement is fixed, parameterized and executed in the same
/// transaction. Count direct rows rather than counting dependent cascades.

pub async fn create_locale(
    db: &SqlitePool,
    expected_version: i64,
    input: CreateLocaleInput,
) -> Result<SiteLocalesSnapshot, LocaleMutationError> {
    let (code, bcp47_tag) = normalize_locale_code(&input.code).map_err(ApiError::unprocessable)?;
    let label = validate_locale_label(&input.label).map_err(ApiError::unprocessable)?;
    let direction =
        validate_locale_direction(input.direction.as_deref()).map_err(ApiError::unprocessable)?;

    let mut conn = db.begin_with("BEGIN IMMEDIATE").await?;
    let res: Result<SiteLocalesSnapshot, LocaleMutationError> = async {
        require_locale_version_in_conn(&mut conn, expected_version).await?;
        require_multilingual_enabled_in_conn(&mut conn).await?;
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT deleted_at IS NOT NULL FROM site_locales WHERE code = ?",
        )
        .bind(&code)
        .fetch_optional(&mut *conn)
        .await?;
        if existing == Some((0,)) {
            return Err(ApiError::conflict("Mã ngôn ngữ đã tồn tại.").into());
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM site_locales WHERE deleted_at IS NULL")
            .fetch_one(&mut *conn)
            .await?;
        if count as usize >= MAX_SITE_LOCALES {
            return Err(ApiError::unprocessable(
                "Đã đạt giới hạn tối đa 20 ngôn ngữ trên hệ thống.",
            )
            .into());
        }
        let next_pos: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM site_locales WHERE deleted_at IS NULL",
        )
        .fetch_one(&mut *conn)
        .await?;
        if existing.is_some() {
            sqlx::query(
                "UPDATE site_locales SET bcp47_tag = ?, label = ?, direction = ?,
                    is_default = 0, enabled = 1, deleted_at = NULL, position = ? WHERE code = ?",
            )
            .bind(&bcp47_tag)
            .bind(&label)
            .bind(&direction)
            .bind(next_pos)
            .bind(&code)
            .execute(&mut *conn)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO site_locales (code, bcp47_tag, label, direction, is_default, enabled, position)
                 VALUES (?, ?, ?, ?, 0, 1, ?)",
            )
            .bind(&code)
            .bind(&bcp47_tag)
            .bind(&label)
            .bind(&direction)
            .bind(next_pos)
            .execute(&mut *conn)
            .await?;
        }
        sqlx::query("UPDATE site_locales_state SET version = version + 1 WHERE id = 1")
            .execute(&mut *conn)
            .await?;
        crate::storage::permalinks::sync_claims_with_options(&mut conn, true).await?;
        if existing.is_some() {
            crate::storage::search::rebuild_search_index_in_conn(&mut conn).await?;
        }
        Ok(list_locales_in_conn(&mut conn).await?)
    }
    .await;
    match res {
        Ok(snap) => {
            conn.commit().await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(snap)
        }
        Err(err) => {
            let _ = conn.rollback().await;
            Err(err)
        }
    }
}

pub async fn update_locale(
    db: &SqlitePool,
    raw_code: &str,
    expected_version: i64,
    input: UpdateLocaleInput,
) -> Result<SiteLocalesSnapshot, LocaleMutationError> {
    let code = raw_code.trim().to_ascii_lowercase();
    let label = validate_locale_label(&input.label).map_err(ApiError::unprocessable)?;
    let direction =
        validate_locale_direction(Some(&input.direction)).map_err(ApiError::unprocessable)?;

    let mut conn = db.begin_with("BEGIN IMMEDIATE").await?;
    let res: Result<SiteLocalesSnapshot, LocaleMutationError> = async {
        require_locale_version_in_conn(&mut conn, expected_version).await?;
        let existing: Option<(i64, i64)> = sqlx::query_as(
            "SELECT is_default, enabled FROM site_locales WHERE code = ? AND deleted_at IS NULL",
        )
        .bind(&code)
        .fetch_optional(&mut *conn)
        .await?;
        let Some((is_default, was_enabled)) = existing else {
            return Err(ApiError::not_found("Không tìm thấy ngôn ngữ.").into());
        };
        if is_default == 0 {
            require_multilingual_enabled_in_conn(&mut conn).await?;
        }
        if is_default != 0 && !input.enabled {
            return Err(ApiError::unprocessable("Không thể tắt ngôn ngữ mặc định.").into());
        }
        sqlx::query(
            "UPDATE site_locales
                SET label = ?, direction = ?, enabled = ?, position = ?
              WHERE code = ?",
        )
        .bind(&label)
        .bind(&direction)
        .bind(i64::from(input.enabled))
        .bind(input.position)
        .bind(&code)
        .execute(&mut *conn)
        .await?;
        sqlx::query("UPDATE site_locales_state SET version = version + 1 WHERE id = 1")
            .execute(&mut *conn)
            .await?;
        if (was_enabled != 0) != input.enabled {
            crate::storage::permalinks::sync_claims_with_options(&mut conn, true).await?;
            crate::storage::search::rebuild_search_index_in_conn(&mut conn).await?;
        }
        Ok(list_locales_in_conn(&mut conn).await?)
    }
    .await;
    match res {
        Ok(snap) => {
            conn.commit().await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(snap)
        }
        Err(err) => {
            let _ = conn.rollback().await;
            Err(err)
        }
    }
}

pub async fn set_default_locale(
    db: &SqlitePool,
    raw_code: &str,
    expected_version: i64,
) -> Result<SiteLocalesSnapshot, LocaleMutationError> {
    let code = raw_code.trim().to_ascii_lowercase();
    let mut conn = db.begin_with("BEGIN IMMEDIATE").await?;
    let res: Result<SiteLocalesSnapshot, LocaleMutationError> = async {
        require_locale_version_in_conn(&mut conn, expected_version).await?;
        require_multilingual_enabled_in_conn(&mut conn).await?;
        let target: Option<(String, i64, i64)> = sqlx::query_as(
            "SELECT bcp47_tag, is_default, enabled FROM site_locales WHERE code = ? AND deleted_at IS NULL",
        )
        .bind(&code)
        .fetch_optional(&mut *conn)
        .await?;
        let Some((bcp47_tag, is_default, enabled)) = target else {
            return Err(ApiError::not_found("Không tìm thấy ngôn ngữ.").into());
        };
        if enabled == 0 {
            return Err(ApiError::conflict(
                "Không thể đặt ngôn ngữ đang tắt làm ngôn ngữ mặc định.",
            )
            .into());
        }
        if is_default == 0 {
            let old_default = get_default_locale_code_in_conn(&mut conn).await?;
            sqlx::query("UPDATE site_locales SET is_default = 0 WHERE is_default = 1")
                .execute(&mut *conn)
                .await?;
            sqlx::query("UPDATE site_locales SET is_default = 1 WHERE code = ?")
                .bind(&code)
                .execute(&mut *conn)
                .await?;
            sqlx::query(
                "UPDATE site_public_settings
                    SET content_language = ?, version = version + 1
                  WHERE id = 1",
            )
            .bind(&bcp47_tag)
            .execute(&mut *conn)
            .await?;
            sqlx::query("UPDATE site_locales_state SET version = version + 1 WHERE id = 1")
                .execute(&mut *conn)
                .await?;
            crate::storage::permalinks::sync_claims_with_options_and_swap(
                &mut conn,
                true,
                Some(&old_default),
            )
            .await?;
        } else {
            sqlx::query("UPDATE site_locales_state SET version = version + 1 WHERE id = 1")
                .execute(&mut *conn)
                .await?;
            crate::storage::permalinks::sync_claims_with_options(&mut conn, true).await?;
        }
        Ok(list_locales_in_conn(&mut conn).await?)
    }
    .await;
    match res {
        Ok(snap) => {
            conn.commit().await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(snap)
        }
        Err(err) => {
            let _ = conn.rollback().await;
            Err(err)
        }
    }
}
