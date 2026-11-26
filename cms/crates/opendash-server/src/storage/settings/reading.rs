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

use super::locales::{get_default_locale_code_in_conn, resolve_requested_locale_in_conn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingPageStatus {
    Unconfigured,
    Published,
    Draft,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadingSettings {
    pub homepage_page_id: Option<i64>,
    pub posts_page_id: Option<i64>,
    pub homepage_status: ReadingPageStatus,
    pub posts_page_status: ReadingPageStatus,
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadingSettingsInput {
    pub homepage_page_id: Option<i64>,
    pub posts_page_id: Option<i64>,
}

async fn inspect_reading_page_in_conn(
    conn: &mut SqliteConnection,
    id: Option<i64>,
) -> ApiResult<(ReadingPageStatus, Option<String>)> {
    let Some(id) = id else {
        return Ok((ReadingPageStatus::Unconfigured, None));
    };
    let row: Option<(String, String, i64, String)> = sqlx::query_as(
        "SELECT e.content_type_key, e.publication_status,
                COALESCE(json_extract(t.definition_json, '$.settings.public'), 1),
                e.locale
           FROM content_entries e
           JOIN content_types t ON t.key = e.content_type_key
          WHERE e.id = ? AND e.trashed_at IS NULL AND t.provider_status = 'active'",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(match row {
        None => (ReadingPageStatus::Deleted, None),
        Some((key, publication, public, locale))
            if key == "page" && publication == "published" && public == 1 =>
        {
            (ReadingPageStatus::Published, Some(locale))
        }
        Some((key, _, _, locale)) if key == "page" => (ReadingPageStatus::Draft, Some(locale)),
        Some(_) => (ReadingPageStatus::Draft, None),
    })
}

async fn reading_page_status_in_conn(
    conn: &mut SqliteConnection,
    id: Option<i64>,
) -> ApiResult<ReadingPageStatus> {
    Ok(inspect_reading_page_in_conn(conn, id).await?.0)
}

/// A read on an existing SQLite connection can join a public route's snapshot.
pub async fn get_reading_in_conn_for_locale(
    conn: &mut SqliteConnection,
    requested_locale: Option<&str>,
) -> ApiResult<ReadingSettings> {
    let resolved_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;
    let row: Option<(Option<i64>, Option<i64>, i64)> = sqlx::query_as(
        "SELECT homepage_page_id, posts_page_id, version
           FROM site_reading_settings WHERE locale = ?",
    )
    .bind(&resolved_locale)
    .fetch_optional(&mut *conn)
    .await?;
    let (homepage_page_id, posts_page_id, version) = row.unwrap_or((None, None, 1));
    let homepage_status = reading_page_status_in_conn(conn, homepage_page_id).await?;
    let posts_page_status = reading_page_status_in_conn(conn, posts_page_id).await?;
    Ok(ReadingSettings {
        homepage_page_id,
        posts_page_id,
        homepage_status,
        posts_page_status,
        version,
        locale: requested_locale.map(|_| resolved_locale),
    })
}

pub async fn get_reading_for_locale(
    db: &SqlitePool,
    requested_locale: Option<&str>,
) -> ApiResult<ReadingSettings> {
    let mut tx = db.begin().await?;
    // Keep IDs, publication state and version from one SQLite read snapshot.
    let snapshot = get_reading_in_conn_for_locale(&mut tx, requested_locale).await?;
    tx.commit().await?;
    Ok(snapshot)
}

pub async fn get_reading(db: &SqlitePool) -> ApiResult<ReadingSettings> {
    get_reading_for_locale(db, None).await
}

/// Delete never clears an assignment: retain a tombstone and advance the
/// settings ETag in the *same transaction* as removal of the Page.
pub async fn bump_reading_version_on_deleted_entry_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
) -> ApiResult<()> {
    sqlx::query(
        "UPDATE site_reading_settings
            SET version = version + 1, updated_at = datetime('now')
          WHERE homepage_page_id = ? OR posts_page_id = ?",
    )
    .bind(entry_id)
    .bind(entry_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// One SQLite write transaction is the CAS boundary for settings and routes.
/// Invalid or unpublished IDs cannot be assigned; existing tombstones are
/// retained until the administrator explicitly chooses new values.
pub async fn save_reading_for_locale(
    db: &SqlitePool,
    requested_locale: Option<&str>,
    input: ReadingSettingsInput,
    expected_version: i64,
) -> ApiResult<ReadingSettings> {
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res: ApiResult<ReadingSettings> = async {
        let resolved_locale = resolve_requested_locale_in_conn(&mut conn, requested_locale).await?;
        let existing_ver: Option<i64> =
            sqlx::query_scalar("SELECT version FROM site_reading_settings WHERE locale = ?")
                .bind(&resolved_locale)
                .fetch_optional(&mut *conn)
                .await?;
        let current_version = existing_ver.unwrap_or(1);
        if current_version != expected_version {
            return Err(ApiError::precondition_failed(
                "Reading Settings đã thay đổi. Tải lại trước khi lưu.",
            ));
        }
        if input.homepage_page_id.is_some() && input.homepage_page_id == input.posts_page_id {
            return Err(ApiError::bad_request(
                "Trang chủ và Trang bài viết phải là hai Page khác nhau.",
            ));
        }
        for id in [input.homepage_page_id, input.posts_page_id]
            .into_iter()
            .flatten()
        {
            if id <= 0 {
                return Err(ApiError::bad_request(
                    "Page được chọn phải tồn tại, public và đã xuất bản.",
                ));
            }
            let (status, page_locale) = inspect_reading_page_in_conn(&mut conn, Some(id)).await?;
            if let Some(ref pl) = page_locale
                && pl != &resolved_locale
            {
                return Err(ApiError::unprocessable(
                    "Page được chọn phải thuộc đúng ngôn ngữ đang cấu hình.",
                ));
            }
            if status != ReadingPageStatus::Published {
                return Err(ApiError::bad_request(
                    "Page được chọn phải tồn tại, public và đã xuất bản.",
                ));
            }
        }
        if existing_ver.is_none() {
            sqlx::query(
                "INSERT INTO site_reading_settings
                    (locale, homepage_page_id, posts_page_id, version, updated_at)
                 VALUES (?, ?, ?, 2, datetime('now'))",
            )
            .bind(&resolved_locale)
            .bind(input.homepage_page_id)
            .bind(input.posts_page_id)
            .execute(&mut *conn)
            .await?;
        } else {
            let updated = sqlx::query(
                "UPDATE site_reading_settings
                    SET homepage_page_id = ?, posts_page_id = ?,
                        version = version + 1, updated_at = datetime('now')
                  WHERE locale = ? AND version = ?",
            )
            .bind(input.homepage_page_id)
            .bind(input.posts_page_id)
            .bind(&resolved_locale)
            .bind(expected_version)
            .execute(&mut *conn)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(ApiError::precondition_failed(
                    "Reading Settings đã thay đổi. Tải lại trước khi lưu.",
                ));
            }
        }
        crate::storage::permalinks::sync_claims_in_tx(&mut conn).await?;
        get_reading_in_conn_for_locale(&mut conn, requested_locale).await
    }
    .await;
    match res {
        Ok(saved) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(saved)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

pub async fn save_reading(
    db: &SqlitePool,
    input: ReadingSettingsInput,
    expected_version: i64,
) -> ApiResult<ReadingSettings> {
    save_reading_for_locale(db, None, input, expected_version).await
}
