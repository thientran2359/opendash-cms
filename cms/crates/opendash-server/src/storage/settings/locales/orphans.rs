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

use super::mutations::reassign_locale_patterns_to_default_in_tx;
use super::queries::*;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeOrphanLocaleInput {
    pub confirm_code: String,
}

#[derive(Debug, Clone, Serialize)]

pub struct OrphanedLocaleCounts {
    pub entries: i64,
    pub terms: i64,
    pub settings: i64,
    pub menu_slots: i64,
    pub routes: i64,
    pub projections: i64,
    pub media_seo_refs: i64,
    pub total: i64,
}

impl OrphanedLocaleCounts {
    fn update_total(&mut self) {
        self.total = self.entries
            + self.terms
            + self.settings
            + self.menu_slots
            + self.routes
            + self.projections
            + self.media_seo_refs;
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OrphanedLocaleSummary {
    pub code: String,
    pub bcp47_tag: String,
    pub label: String,
    pub deleted_at: String,
    pub counts: OrphanedLocaleCounts,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrphanedLocalesSnapshot {
    pub version: i64,
    pub items: Vec<OrphanedLocaleSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PurgeOrphanLocaleResult {
    pub snapshot: OrphanedLocalesSnapshot,
    pub purged_code: String,
    pub deleted: OrphanedLocaleCounts,
}

pub(in crate::storage::settings) async fn count_locale_data_in_conn(
    conn: &mut SqliteConnection,
    code: &str,
) -> ApiResult<OrphanedLocaleCounts> {
    let (entries, terms, settings, menu_slots, routes, projections, media_seo_refs):
        (i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM content_entries WHERE locale = ?1 AND content_type_key != 'pattern'),
            (SELECT COUNT(*) FROM taxonomy_terms WHERE locale = ?1),
            (SELECT COUNT(*) FROM site_general_settings_locales WHERE locale = ?1)
              + (SELECT COUNT(*) FROM site_reading_settings WHERE locale = ?1)
              + (SELECT COUNT(*) FROM site_home_seo WHERE locale = ?1)
              + (SELECT COUNT(*) FROM theme_option_locale_values WHERE locale = ?1)
              + (SELECT COUNT(*) FROM theme_option_locale_versions WHERE locale = ?1)
              + (SELECT COUNT(*) FROM theme_option_media_refs WHERE locale = ?1),
            (SELECT COUNT(*) FROM site_menu_slot_assignments WHERE locale = ?1)
              + (SELECT COUNT(*) FROM site_menu_slot_versions WHERE locale = ?1),
            (SELECT COUNT(*) FROM entry_public_paths WHERE locale = ?1)
              + (SELECT COUNT(*) FROM entry_redirects WHERE locale = ?1)
              + (SELECT COUNT(*) FROM permalink_route_redirects WHERE locale = ?1),
            (SELECT COUNT(*) FROM plugin_entry_projections WHERE locale = ?1),
            (SELECT COUNT(*) FROM seo_public_media_refs WHERE source_kind = 'entry_seo'
                AND source_id IN (SELECT id FROM content_entries WHERE locale = ?1 AND content_type_key != 'pattern'))",
    )
    .bind(code)
    .fetch_one(conn)
    .await?;
    let mut counts = OrphanedLocaleCounts {
        entries,
        terms,
        settings,
        menu_slots,
        routes,
        projections,
        media_seo_refs,
        total: 0,
    };
    counts.update_total();
    Ok(counts)
}

pub(in crate::storage::settings) async fn list_orphaned_locales_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<OrphanedLocalesSnapshot> {
    let version = sqlx::query_scalar("SELECT version FROM site_locales_state WHERE id = 1")
        .fetch_one(&mut *conn)
        .await?;
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT code, bcp47_tag, label, deleted_at FROM site_locales
         WHERE deleted_at IS NOT NULL AND is_default = 0 ORDER BY code",
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut items = Vec::with_capacity(rows.len());
    for (code, bcp47_tag, label, deleted_at) in rows {
        let counts = count_locale_data_in_conn(conn, &code).await?;
        items.push(OrphanedLocaleSummary {
            code,
            bcp47_tag,
            label,
            deleted_at,
            counts,
        });
    }
    Ok(OrphanedLocalesSnapshot { version, items })
}

pub async fn list_orphaned_locales(db: &SqlitePool) -> ApiResult<OrphanedLocalesSnapshot> {
    let mut tx = db.begin().await?;
    let snapshot = list_orphaned_locales_in_conn(&mut tx).await?;
    tx.commit().await?;
    Ok(snapshot)
}

/// Patterns are shared by every locale; only their owning row's locale and,
/// on collision, slug change. Their identity and references remain intact.

pub(in crate::storage::settings) async fn delete_locale_rows(
    conn: &mut SqliteConnection,
    code: &str,
    statements: &[&'static str],
) -> ApiResult<i64> {
    let mut count = 0;
    for &statement in statements {
        count += sqlx::query(statement)
            .bind(code)
            .execute(&mut *conn)
            .await?
            .rows_affected() as i64;
    }
    Ok(count)
}

pub async fn purge_orphan_locale(
    db: &SqlitePool,
    raw_code: &str,
    expected_version: i64,
    input: PurgeOrphanLocaleInput,
) -> Result<PurgeOrphanLocaleResult, LocaleMutationError> {
    let code = raw_code.trim().to_ascii_lowercase();
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    require_locale_version_in_conn(&mut tx, expected_version).await?;
    let row: Option<(bool, Option<String>)> =
        sqlx::query_as("SELECT is_default, deleted_at FROM site_locales WHERE code = ?")
            .bind(&code)
            .fetch_optional(&mut *tx)
            .await?;
    match row {
        None => return Err(ApiError::not_found("Không tìm thấy ngôn ngữ.").into()),
        Some((true, _)) => {
            return Err(ApiError::unprocessable("Không thể xóa ngôn ngữ mặc định.").into());
        }
        Some((false, None)) => return Err(ApiError::unprocessable("Ngôn ngữ chưa bị xóa.").into()),
        Some((false, Some(_))) => {}
    }
    if input.confirm_code.trim().to_ascii_lowercase() != code {
        return Err(ApiError::unprocessable("Mã xác nhận xóa vĩnh viễn không khớp.").into());
    }
    let default_code = get_default_locale_code_in_conn(&mut tx).await?;
    reassign_locale_patterns_to_default_in_tx(&mut tx, &code, &default_code).await?;
    sqlx::query(
        "UPDATE site_reading_settings
            SET homepage_page_id = CASE WHEN homepage_page_id IN (SELECT id FROM content_entries WHERE locale = ?1) THEN NULL ELSE homepage_page_id END,
                posts_page_id = CASE WHEN posts_page_id IN (SELECT id FROM content_entries WHERE locale = ?1) THEN NULL ELSE posts_page_id END,
                version = version + 1, updated_at = datetime('now')
          WHERE locale != ?1 AND (homepage_page_id IN (SELECT id FROM content_entries WHERE locale = ?1)
            OR posts_page_id IN (SELECT id FROM content_entries WHERE locale = ?1))",
    ).bind(&code).execute(&mut *tx).await?;
    let media_seo_refs = delete_locale_rows(&mut tx, &code, &[
        "DELETE FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id IN (SELECT id FROM content_entries WHERE locale = ?)",
    ]).await?;
    delete_locale_rows(&mut tx, &code, &[
        "DELETE FROM entry_terms WHERE entry_id IN (SELECT id FROM content_entries WHERE locale = ?1) OR term_id IN (SELECT id FROM taxonomy_terms WHERE locale = ?1)",
        "DELETE FROM permalink_route_claims WHERE locale = ?",
    ]).await?;
    let routes = delete_locale_rows(
        &mut tx,
        &code,
        &[
            "DELETE FROM entry_public_paths WHERE locale = ?",
            "DELETE FROM entry_redirects WHERE locale = ?",
            "DELETE FROM permalink_route_redirects WHERE locale = ?",
        ],
    )
    .await?;
    let settings = delete_locale_rows(
        &mut tx,
        &code,
        &[
            "DELETE FROM site_general_settings_locales WHERE locale = ?",
            "DELETE FROM site_reading_settings WHERE locale = ?",
            "DELETE FROM site_home_seo WHERE locale = ?",
            "DELETE FROM theme_option_locale_values WHERE locale = ?",
            "DELETE FROM theme_option_locale_versions WHERE locale = ?",
            "DELETE FROM theme_option_media_refs WHERE locale = ?",
        ],
    )
    .await?;
    let menu_slots = delete_locale_rows(
        &mut tx,
        &code,
        &[
            "DELETE FROM site_menu_slot_assignments WHERE locale = ?",
            "DELETE FROM site_menu_slot_versions WHERE locale = ?",
        ],
    )
    .await?;
    let projections = delete_locale_rows(
        &mut tx,
        &code,
        &["DELETE FROM plugin_entry_projections WHERE locale = ?"],
    )
    .await?;
    crate::storage::search::delete_locale_search_index_in_conn(&mut tx, &code).await?;
    let entries = delete_locale_rows(
        &mut tx,
        &code,
        &["DELETE FROM content_entries WHERE locale = ?"],
    )
    .await?;
    sqlx::query(
        "UPDATE taxonomy_terms SET parent_id = NULL WHERE locale = ? AND parent_id IS NOT NULL",
    )
    .bind(&code)
    .execute(&mut *tx)
    .await?;
    let terms = delete_locale_rows(
        &mut tx,
        &code,
        &["DELETE FROM taxonomy_terms WHERE locale = ?"],
    )
    .await?;
    sqlx::query(
        "DELETE FROM site_locales WHERE code = ? AND deleted_at IS NOT NULL AND is_default = 0",
    )
    .bind(&code)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE site_locales_state SET version = version + 1 WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    crate::storage::permalinks::sync_claims_with_options(&mut tx, true).await?;
    let mut deleted = OrphanedLocaleCounts {
        entries,
        terms,
        settings,
        menu_slots,
        routes,
        projections,
        media_seo_refs,
        total: 0,
    };
    deleted.update_total();
    let snapshot = list_orphaned_locales_in_conn(&mut tx).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(PurgeOrphanLocaleResult {
        snapshot,
        purged_code: code,
        deleted,
    })
}
