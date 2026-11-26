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

pub const TZDB_VERSION: &str = "2026f";
pub const MAX_SITE_TITLE_SCALARS: usize = 120;
pub const MAX_TAGLINE_SCALARS: usize = 240;
pub const MAX_SITE_LOCALES: usize = 20;
pub const MAX_LOCALE_LABEL_SCALARS: usize = 80;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleQuery {
    #[serde(default)]
    pub locale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteLocale {
    pub code: String,
    pub bcp47_tag: String,
    pub label: String,
    pub direction: String,
    pub is_default: bool,
    pub enabled: bool,
    pub position: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteLocalesSnapshot {
    pub version: i64,
    pub multilingual_enabled: bool,
    pub max_locales: usize,
    pub default_code: String,
    pub items: Vec<SiteLocale>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateLocaleInput {
    pub code: String,
    pub label: String,
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateLocaleInput {
    pub label: String,
    pub direction: String,
    pub enabled: bool,
    pub position: i64,
}

#[derive(Debug)]
pub enum LocaleMutationError {
    Api(ApiError),
    Conflicts(crate::storage::permalinks::PermalinkConflictError),
}

impl From<ApiError> for LocaleMutationError {
    fn from(err: ApiError) -> Self {
        Self::Api(err)
    }
}

impl From<sqlx::Error> for LocaleMutationError {
    fn from(err: sqlx::Error) -> Self {
        Self::Api(err.into())
    }
}

impl From<crate::storage::permalinks::PermalinkPutError> for LocaleMutationError {
    fn from(err: crate::storage::permalinks::PermalinkPutError) -> Self {
        match err {
            crate::storage::permalinks::PermalinkPutError::Api(a) => Self::Api(a),
            crate::storage::permalinks::PermalinkPutError::Conflicts(c) => Self::Conflicts(c),
        }
    }
}

pub fn normalize_locale_code(tag: &str) -> Result<(String, String), &'static str> {
    if !opendash_core::id::valid_bcp47_tag(tag) {
        return Err(
            "Ngôn ngữ nội dung phải là mã BCP47 ASCII hợp lệ (tối đa 35 bytes, ví dụ vi hoặc en-US).",
        );
    }
    let code = tag.to_ascii_lowercase();
    if matches!(code.as_str(), "multilingual" | "orphans")
        || crate::seo::is_reserved_namespace_key(&code)
    {
        return Err("Mã ngôn ngữ trùng với tiền tố hệ thống được bảo vệ.");
    }
    Ok((code, tag.to_owned()))
}

pub(crate) fn generate_translation_group_id(prefix: &str) -> String {
    format!("{prefix}-{}", crate::auth::generate_random_token())
}

fn validate_locale_label(raw: &str) -> Result<String, &'static str> {
    if raw.chars().any(char::is_control) {
        return Err("Nhãn ngôn ngữ không được chứa ký tự điều khiển.");
    }
    let trimmed = raw.trim();
    let count = trimmed.chars().count();
    if !(1..=MAX_LOCALE_LABEL_SCALARS).contains(&count) {
        return Err("Nhãn ngôn ngữ phải có từ 1 đến 80 ký tự.");
    }
    Ok(trimmed.to_owned())
}

fn validate_locale_direction(raw: Option<&str>) -> Result<String, &'static str> {
    match raw {
        None => Ok("ltr".to_owned()),
        Some("ltr") => Ok("ltr".to_owned()),
        Some("rtl") => Ok("rtl".to_owned()),
        Some(_) => Err("Hướng hiển thị ngôn ngữ chỉ chấp nhận 'ltr' hoặc 'rtl'."),
    }
}

pub(crate) async fn get_default_locale_code_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<String> {
    let code: Option<String> = sqlx::query_scalar(
        "SELECT code FROM site_locales WHERE is_default = 1 AND deleted_at IS NULL LIMIT 1",
    )
    .fetch_optional(&mut *conn)
    .await?;
    code.ok_or_else(|| ApiError::internal("Thiếu ngôn ngữ mặc định trong site_locales."))
}

pub(crate) async fn get_default_locale_code(db: &SqlitePool) -> ApiResult<String> {
    let mut conn = db.acquire().await?;
    get_default_locale_code_in_conn(&mut conn).await
}

pub(crate) async fn enabled_secondary_locale_for_segment(
    db: &SqlitePool,
    segment: &str,
) -> ApiResult<Option<String>> {
    if segment.is_empty() {
        return Ok(None);
    }
    let mut conn = db.acquire().await?;
    let code: Option<String> = sqlx::query_scalar(
        "SELECT code FROM site_locales WHERE code = ? AND enabled = 1 AND is_default = 0 AND deleted_at IS NULL AND (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1",
    )
    .bind(segment)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(code)
}

/// Resolve `(code, bcp47_tag, direction)` for `locale` (or the default locale when `None`).
pub(crate) async fn get_locale_bcp47_and_dir_in_conn(
    conn: &mut SqliteConnection,
    locale: Option<&str>,
) -> ApiResult<(String, String, String)> {
    let row: Option<(String, String, String)> =
        match locale {
            Some(code) => {
                sqlx::query_as(
                    "SELECT code, bcp47_tag, direction FROM site_locales WHERE code = ? AND deleted_at IS NULL LIMIT 1",
                )
                .bind(code)
                .fetch_optional(&mut *conn)
                .await?
            }
            None => sqlx::query_as(
                "SELECT code, bcp47_tag, direction FROM site_locales WHERE is_default = 1 AND deleted_at IS NULL LIMIT 1",
            )
            .fetch_optional(&mut *conn)
            .await?,
        };
    row.ok_or_else(|| ApiError::internal("Không tìm thấy cấu hình ngôn ngữ trong site_locales."))
}

pub(crate) async fn get_locale_bcp47_and_dir(
    db: &SqlitePool,
    locale: Option<&str>,
) -> ApiResult<(String, String, String)> {
    let mut conn = db.acquire().await?;
    get_locale_bcp47_and_dir_in_conn(&mut conn, locale).await
}

#[derive(Debug, Clone)]
pub(crate) struct EnabledLocaleInfo {
    pub code: String,
    pub bcp47_tag: String,
    pub label: String,
    pub direction: String,
    pub is_default: bool,
}

pub(crate) async fn list_enabled_locales_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<Vec<EnabledLocaleInfo>> {
    let rows: Vec<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT code, bcp47_tag, label, direction, is_default
           FROM site_locales
          WHERE enabled = 1 AND deleted_at IS NULL
            AND (is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
          ORDER BY is_default DESC, position ASC, code ASC",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(code, bcp47_tag, label, direction, is_def)| EnabledLocaleInfo {
                code,
                bcp47_tag,
                label,
                direction,
                is_default: is_def != 0,
            },
        )
        .collect())
}

pub(crate) async fn resolve_requested_locale_in_conn(
    conn: &mut SqliteConnection,
    requested: Option<&str>,
) -> ApiResult<String> {
    let Some(raw) = requested else {
        return get_default_locale_code_in_conn(conn).await;
    };
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(ApiError::bad_request("Tham số locale không được rỗng."));
    }
    let found: Option<String> =
        sqlx::query_scalar("SELECT code FROM site_locales WHERE code = ? AND deleted_at IS NULL")
            .bind(&normalized)
            .fetch_optional(&mut *conn)
            .await?;
    found.ok_or_else(|| ApiError::bad_request("Ngôn ngữ được yêu cầu không tồn tại."))
}

pub(crate) async fn resolve_requested_locale(
    db: &SqlitePool,
    requested: Option<&str>,
) -> ApiResult<String> {
    let mut conn = db.acquire().await?;
    resolve_requested_locale_in_conn(&mut conn, requested).await
}

pub(crate) async fn list_locales_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<SiteLocalesSnapshot> {
    let (version, multilingual_enabled): (i64, i64) =
        sqlx::query_as("SELECT version, multilingual_enabled FROM site_locales_state WHERE id = 1")
            .fetch_one(&mut *conn)
            .await?;
    let rows: Vec<(String, String, String, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT code, bcp47_tag, label, direction, is_default, enabled, position
           FROM site_locales
          WHERE deleted_at IS NULL
          ORDER BY is_default DESC, position ASC, code ASC",
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut default_code = String::new();
    let items: Vec<SiteLocale> = rows
        .into_iter()
        .map(
            |(code, bcp47_tag, label, direction, is_default, enabled, position)| {
                let is_def = is_default != 0;
                if is_def {
                    default_code = code.clone();
                }
                SiteLocale {
                    code,
                    bcp47_tag,
                    label,
                    direction,
                    is_default: is_def,
                    enabled: enabled != 0,
                    position,
                }
            },
        )
        .collect();
    if default_code.is_empty() {
        return Err(ApiError::internal(
            "Thiếu ngôn ngữ mặc định trong site_locales.",
        ));
    }
    Ok(SiteLocalesSnapshot {
        version,
        multilingual_enabled: multilingual_enabled != 0,
        max_locales: MAX_SITE_LOCALES,
        default_code,
        items,
    })
}

pub async fn list_locales(db: &SqlitePool) -> ApiResult<SiteLocalesSnapshot> {
    let mut tx = db.begin().await?;
    let snap = list_locales_in_conn(&mut tx).await?;
    tx.commit().await?;
    Ok(snap)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetMultilingualEnabledInput {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeOrphanLocaleInput {
    pub confirm_code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeleteLocaleResponse {
    pub snapshot: SiteLocalesSnapshot,
    pub deleted_code: String,
    pub soft_deleted: bool,
    pub orphan_total: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
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

pub(crate) fn multilingual_disabled_error() -> ApiError {
    ApiError::conflict("Hỗ trợ đa ngôn ngữ đang tắt.")
}

pub(crate) async fn multilingual_enabled_in_conn(conn: &mut SqliteConnection) -> ApiResult<bool> {
    Ok(
        sqlx::query_scalar("SELECT multilingual_enabled FROM site_locales_state WHERE id = 1")
            .fetch_one(conn)
            .await?,
    )
}

pub(crate) async fn require_multilingual_enabled_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<()> {
    if multilingual_enabled_in_conn(conn).await? {
        Ok(())
    } else {
        Err(multilingual_disabled_error())
    }
}

async fn require_locale_version_in_conn(
    conn: &mut SqliteConnection,
    expected: i64,
) -> ApiResult<()> {
    let current: i64 = sqlx::query_scalar("SELECT version FROM site_locales_state WHERE id = 1")
        .fetch_one(conn)
        .await?;
    if current != expected {
        return Err(ApiError::precondition_failed(
            "Danh sách ngôn ngữ đã thay đổi. Tải lại trước khi lưu.",
        ));
    }
    Ok(())
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
    }
    let snapshot = list_locales_in_conn(&mut tx).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(snapshot)
}

async fn count_locale_data_in_conn(
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

async fn list_orphaned_locales_in_conn(
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
async fn reassign_locale_patterns_to_default_in_tx(
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
async fn delete_locale_rows(
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

static TIMEZONES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    include_str!("settings_timezones_2026f.txt")
        .lines()
        .filter(|zone| !zone.is_empty() && !zone.starts_with('#'))
        .collect()
});

pub fn timezone_catalog() -> &'static [&'static str] {
    &TIMEZONES
}

pub fn is_current_timezone(timezone: &str) -> bool {
    TIMEZONES.binary_search(&timezone).is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneralSettings {
    pub site_title: String,
    pub tagline: String,
    pub timezone: String,
    pub cms_locale: String,
    pub version: i64,
    pub has_override: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralSettingsInput {
    pub site_title: String,
    pub tagline: String,
    pub timezone: String,
    #[serde(default)]
    pub cms_locale: Option<String>,
}

impl GeneralSettingsInput {
    pub fn validate(mut self) -> Result<Self, &'static str> {
        // Reject controls before trimming; otherwise leading CR/LF/TAB could
        // silently disappear and bypass the contract.
        if self.site_title.chars().any(char::is_control)
            || self.tagline.chars().any(char::is_control)
        {
            return Err("Tên website hoặc tagline chứa ký tự điều khiển.");
        }
        self.site_title = self.site_title.trim().to_owned();
        self.tagline = self.tagline.trim().to_owned();
        if !(1..=MAX_SITE_TITLE_SCALARS).contains(&self.site_title.chars().count()) {
            return Err("Tên website phải có 1–120 ký tự Unicode, không chứa ký tự điều khiển.");
        }
        if self.tagline.chars().count() > MAX_TAGLINE_SCALARS {
            return Err("Tagline phải có tối đa 240 ký tự Unicode, không chứa ký tự điều khiển.");
        }
        if self.timezone.trim() != self.timezone || self.timezone.is_empty() {
            return Err("Múi giờ không hợp lệ.");
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminI18nBootstrap {
    pub configured_locale: String,
    pub effective_locale: &'static str,
    pub effective_bcp47: &'static str,
    pub effective_direction: &'static str,
    pub configured_installed: bool,
    pub catalog_version: &'static str,
    pub available_locales: &'static [CmsLocaleOption],
}

pub async fn get_cms_locale_in_conn(conn: &mut SqliteConnection) -> ApiResult<String> {
    Ok(
        sqlx::query_scalar("SELECT cms_locale FROM site_general_settings WHERE id = 1")
            .fetch_one(&mut *conn)
            .await?,
    )
}

pub async fn get_admin_i18n_bootstrap(db: &SqlitePool) -> ApiResult<AdminI18nBootstrap> {
    let mut conn = db.acquire().await?;
    let configured_locale = get_cms_locale_in_conn(&mut conn).await?;
    let effective = resolve_effective_ui_locale(&configured_locale);
    let configured_installed =
        find_cms_locale(&configured_locale).is_some_and(|option| option.installed);
    Ok(AdminI18nBootstrap {
        configured_locale,
        effective_locale: effective.code,
        effective_bcp47: effective.bcp47,
        effective_direction: effective.direction,
        configured_installed,
        catalog_version: CMS_CATALOG_VERSION,
        available_locales: CMS_LANGUAGE_CATALOG,
    })
}

pub async fn get_general_for_locale(
    db: &SqlitePool,
    requested_locale: Option<&str>,
) -> ApiResult<GeneralSettings> {
    let mut tx = db.begin().await?;
    let res = get_general_in_conn_for_locale(&mut tx, requested_locale).await?;
    tx.commit().await?;
    Ok(res)
}

pub async fn get_general(db: &SqlitePool) -> ApiResult<GeneralSettings> {
    get_general_for_locale(db, None).await
}

pub(crate) async fn get_general_in_conn_for_locale(
    conn: &mut SqliteConnection,
    requested_locale: Option<&str>,
) -> ApiResult<GeneralSettings> {
    let default_locale = get_default_locale_code_in_conn(conn).await?;
    let target_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;
    let (timezone, cms_locale): (String, String) =
        sqlx::query_as("SELECT timezone, cms_locale FROM site_general_settings WHERE id = 1")
            .fetch_one(&mut *conn)
            .await?;
    let exact: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT site_title, tagline, version
           FROM site_general_settings_locales
          WHERE locale = ?",
    )
    .bind(&target_locale)
    .fetch_optional(&mut *conn)
    .await?;
    let (site_title, tagline, version, has_override) = match exact {
        Some((title, tag, ver)) => (title, tag, ver, true),
        None => {
            let fallback: Option<(String, String)> = sqlx::query_as(
                "SELECT site_title, tagline
                   FROM site_general_settings_locales
                  WHERE locale = ?",
            )
            .bind(&default_locale)
            .fetch_optional(&mut *conn)
            .await?;
            let (title, tag) = match fallback {
                Some(pair) => pair,
                None => {
                    sqlx::query_as(
                        "SELECT site_title, tagline FROM site_general_settings WHERE id = 1",
                    )
                    .fetch_one(&mut *conn)
                    .await?
                }
            };
            (title, tag, 1, false)
        }
    };
    Ok(GeneralSettings {
        site_title,
        tagline,
        timezone,
        cms_locale,
        version,
        has_override,
        locale: requested_locale.map(|_| target_locale),
    })
}

pub(super) async fn get_general_in_conn(conn: &mut SqliteConnection) -> ApiResult<GeneralSettings> {
    get_general_in_conn_for_locale(conn, None).await
}

pub enum SaveOutcome {
    Saved(GeneralSettings),
    Stale,
    InvalidTimezone,
    SecondaryTimezoneLocked,
    InvalidCmsLocale,
    SecondaryCmsLocaleLocked,
    InvalidLocale(String),
}

/// Runs inside `BEGIN IMMEDIATE` so reads and version/timezone validation share
/// a single SQLite write lock. Secondary locales cannot alter global timezone or CMS language.
pub async fn save_general_for_locale(
    db: &SqlitePool,
    requested_locale: Option<&str>,
    input: GeneralSettingsInput,
    expected_version: i64,
) -> Result<SaveOutcome, sqlx::Error> {
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res: Result<SaveOutcome, sqlx::Error> = async {
        let default_locale = match get_default_locale_code_in_conn(&mut conn).await {
            Ok(code) => code,
            Err(ApiError(_, msg)) => return Ok(SaveOutcome::InvalidLocale(msg.into_owned())),
        };
        let target_locale = match resolve_requested_locale_in_conn(&mut conn, requested_locale).await
        {
            Ok(code) => code,
            Err(ApiError(_, msg)) => return Ok(SaveOutcome::InvalidLocale(msg.into_owned())),
        };
        let (global_tz, global_cms_locale): (String, String) =
            sqlx::query_as("SELECT timezone, cms_locale FROM site_general_settings WHERE id = 1")
                .fetch_one(&mut *conn)
                .await?;
        let normalized_cms = match input.cms_locale.as_deref() {
            Some(raw) => match find_cms_locale(raw) {
                Some(option) => Some(option.code.to_owned()),
                None => return Ok(SaveOutcome::InvalidCmsLocale),
            },
            None => None,
        };
        let is_default_locale = target_locale == default_locale;
        if !is_default_locale && normalized_cms.as_ref().is_some_and(|locale| locale != &global_cms_locale) {
            return Ok(SaveOutcome::SecondaryCmsLocaleLocked);
        }
        let next_cms_locale = normalized_cms.unwrap_or(global_cms_locale);
        if is_default_locale {
            if !is_current_timezone(&input.timezone) && input.timezone != global_tz {
                return Ok(SaveOutcome::InvalidTimezone);
            }
        } else if input.timezone != global_tz {
            return Ok(SaveOutcome::SecondaryTimezoneLocked);
        }

        let current_ver: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM site_general_settings_locales WHERE locale = ?",
        )
        .bind(&target_locale)
        .fetch_optional(&mut *conn)
        .await?;

        let new_version = match current_ver {
            None => {
                if expected_version != 1 {
                    return Ok(SaveOutcome::Stale);
                }
                sqlx::query(
                    "INSERT INTO site_general_settings_locales (locale, site_title, tagline, version)
                     VALUES (?, ?, ?, 2)",
                )
                .bind(&target_locale)
                .bind(&input.site_title)
                .bind(&input.tagline)
                .execute(&mut *conn)
                .await?;
                2
            }
            Some(ver) => {
                if ver != expected_version {
                    return Ok(SaveOutcome::Stale);
                }
                let updated = sqlx::query(
                    "UPDATE site_general_settings_locales
                        SET site_title = ?, tagline = ?, version = version + 1
                      WHERE locale = ? AND version = ?",
                )
                .bind(&input.site_title)
                .bind(&input.tagline)
                .bind(&target_locale)
                .bind(expected_version)
                .execute(&mut *conn)
                .await?;
                if updated.rows_affected() == 0 {
                    return Ok(SaveOutcome::Stale);
                }
                expected_version + 1
            }
        };

        if is_default_locale {
            sqlx::query(
                "UPDATE site_general_settings
                    SET site_title = ?, tagline = ?, timezone = ?, cms_locale = ?, version = ?
                  WHERE id = 1",
            )
            .bind(&input.site_title)
            .bind(&input.tagline)
            .bind(&input.timezone)
            .bind(&next_cms_locale)
            .bind(new_version)
            .execute(&mut *conn)
            .await?;
        }

        Ok(SaveOutcome::Saved(GeneralSettings {
            site_title: input.site_title,
            tagline: input.tagline,
            timezone: input.timezone,
            cms_locale: next_cms_locale,
            version: new_version,
            has_override: true,
            locale: requested_locale.map(|_| target_locale),
        }))
    }
    .await;
    match res {
        Ok(outcome) if matches!(outcome, SaveOutcome::Saved(_)) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(outcome)
        }
        Ok(outcome) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Ok(outcome)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

pub async fn save_general(
    db: &SqlitePool,
    input: GeneralSettingsInput,
    expected_version: i64,
) -> Result<SaveOutcome, sqlx::Error> {
    save_general_for_locale(db, None, input, expected_version).await
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeOptionsSnapshot {
    pub theme_id: String,
    pub version: i64,
    pub fields: Vec<FieldSpec>,
    pub values: BTreeMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
}

pub enum ThemeOptionsOutcome {
    Saved(ThemeOptionsSnapshot),
    NotFound,
    Stale,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSiteOptionsSnapshot {
    pub version: i64,
    pub fields: Vec<FieldSpec>,
    pub values: BTreeMap<String, serde_json::Value>,
}

pub enum GlobalSiteOptionsOutcome {
    Saved(GlobalSiteOptionsSnapshot),
    Stale,
}

pub(crate) fn validate_url_or_link_spec_strict(
    spec: &FieldSpec,
    value: &serde_json::Value,
) -> Result<(), &'static str> {
    match spec.field_type {
        FieldType::Url => {
            if let Some(s) = value.as_str()
                && !s.is_empty()
            {
                crate::template_sec::validate_theme_option_url(s)?;
            }
        }
        FieldType::Link => {
            let url_str = match value {
                serde_json::Value::String(s) => Some(s.as_str()),
                serde_json::Value::Object(obj) => {
                    obj.get("url").and_then(serde_json::Value::as_str)
                }
                _ => None,
            };
            if let Some(s) = url_str
                && !s.is_empty()
            {
                crate::template_sec::validate_theme_option_url(s)?;
            }
        }
        FieldType::Group => {
            if let Some(obj) = value.as_object() {
                for sub in &spec.fields {
                    if let Some(v) = obj.get(&sub.key) {
                        validate_url_or_link_spec_strict(sub, v)?;
                    }
                }
            }
        }
        FieldType::Repeater => {
            if let Some(arr) = value.as_array() {
                for item in arr {
                    if let Some(obj) = item.as_object() {
                        for sub in &spec.fields {
                            if let Some(v) = obj.get(&sub.key) {
                                validate_url_or_link_spec_strict(sub, v)?;
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_url_or_link_defaults_strict(spec: &FieldSpec) -> Result<(), &'static str> {
    if spec.is_ui_only() {
        return Ok(());
    }
    match spec.field_type {
        FieldType::Url | FieldType::Link => {
            validate_url_or_link_spec_strict(spec, &default_value_for_field_spec(spec))?;
        }
        FieldType::Group | FieldType::Repeater => {
            if let Some(def) = spec.default.as_ref() {
                validate_url_or_link_spec_strict(spec, def)?;
            }
            for child in &spec.fields {
                validate_url_or_link_defaults_strict(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn validate_global_site_options_map(
    submitted: &BTreeMap<String, serde_json::Value>,
) -> Result<BTreeMap<String, serde_json::Value>, &'static str> {
    let fields = global_site_option_fields();
    let data_map: BTreeMap<&str, &FieldSpec> = fields
        .iter()
        .filter(|s| !s.is_ui_only())
        .map(|s| (s.key.as_str(), s))
        .collect();

    let mut normalized = BTreeMap::new();
    for (k, v) in submitted {
        let Some(spec) = data_map.get(k.as_str()) else {
            return Err("Option toàn trang không tồn tại trong schema.");
        };
        let clean_val = match spec.field_type {
            FieldType::Media => match v {
                serde_json::Value::Null => serde_json::Value::String(String::new()),
                serde_json::Value::String(id) if id.is_empty() || valid_sha256_media_id(id) => {
                    serde_json::Value::String(id.clone())
                }
                _ => return Err("Option media phải là media_id SHA-256 hợp lệ hoặc rỗng."),
            },
            _ => v.clone(),
        };
        validate_url_or_link_spec_strict(spec, &clean_val)?;
        normalized.insert(k.clone(), clean_val);
    }

    let mut merged = global_site_option_defaults();
    for (k, v) in &normalized {
        merged.insert(k.clone(), v.clone());
    }
    validate_field_values(&fields, FieldContext::Options, &merged, None)?;
    Ok(normalized)
}

pub fn parse_theme_options_schema(manifest_json: &str) -> Result<Vec<FieldSpec>, &'static str> {
    let manifest: serde_json::Value =
        serde_json::from_str(manifest_json).map_err(|_| "theme.json không hợp lệ.")?;
    let Some(raw_opts) = manifest.get("options") else {
        return Ok(Vec::new());
    };
    if raw_opts.is_null() {
        return Ok(Vec::new());
    }
    let specs: Vec<FieldSpec> = serde_json::from_value(raw_opts.clone())
        .map_err(|_| "Khai báo options trong theme.json không đúng cấu trúc.")?;
    opendash_core::theme::validate_theme_options_fields(&specs)?;
    for spec in &specs {
        validate_url_or_link_defaults_strict(spec)?;
    }
    Ok(specs)
}

/// Derive anchor-only permissions from the selected revision's validated
/// manifest plus global site public URL keys; request values and mutable context
/// cannot add capabilities.
pub fn theme_url_policy(
    manifest_json: &str,
) -> Result<crate::template_sec::ThemeUrlPolicy, &'static str> {
    let specs = parse_theme_options_schema(manifest_json)?;
    let raw: serde_json::Value =
        serde_json::from_str(manifest_json).map_err(|_| "theme.json không đúng JSON.")?;
    let declared_assets: BTreeSet<String> = raw
        .get("assets")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(ToOwned::to_owned))
        .collect();
    Ok(crate::template_sec::ThemeUrlPolicy::from_option_specs(
        &specs,
        declared_assets,
    ))
}

pub fn global_theme_url_policy() -> crate::template_sec::ThemeUrlPolicy {
    crate::template_sec::ThemeUrlPolicy::from_option_specs(&[], BTreeSet::new())
}

async fn latest_revision_manifest_in_conn(
    conn: &mut SqliteConnection,
    theme_id: &str,
) -> ApiResult<Option<String>> {
    if let Some(dev) = crate::dev_mode::DevOverlayConfig::global()
        && let Ok(Some(snap)) = dev.load_dev_theme()
        && snap.manifest.theme_id == theme_id
    {
        return Ok(Some(snap.manifest_json));
    }
    // Prefer currently active revision if it belongs to this theme_id, else latest installed revision.
    let active: Option<String> = sqlx::query_scalar(
        "SELECT r.manifest_json
         FROM theme_active_selection AS a
         JOIN theme_revisions AS r ON r.revision_id = a.revision_id
         WHERE a.id = 1 AND r.theme_id = ?",
    )
    .bind(theme_id)
    .fetch_optional(&mut *conn)
    .await?;
    if active.is_some() {
        return Ok(active);
    }
    let latest: Option<String> = sqlx::query_scalar(
        "SELECT manifest_json FROM theme_revisions
         WHERE theme_id = ?
         ORDER BY created_at DESC, rowid DESC
         LIMIT 1",
    )
    .bind(theme_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(latest)
}

pub async fn get_global_site_options_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<GlobalSiteOptionsSnapshot> {
    let fields = global_site_option_fields();
    let version: i64 =
        sqlx::query_scalar("SELECT version FROM site_global_option_version WHERE id = 1")
            .fetch_optional(&mut *conn)
            .await?
            .unwrap_or(1);

    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT option_key, value_json FROM site_global_option_values")
            .fetch_all(&mut *conn)
            .await?;
    let mut saved_map = BTreeMap::new();
    for (k, v_json) in rows {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
            saved_map.insert(k, v);
        }
    }

    let mut values = global_site_option_defaults();
    for (k, v) in saved_map {
        if values.contains_key(&k) {
            values.insert(k, v);
        }
    }

    Ok(GlobalSiteOptionsSnapshot {
        version,
        fields,
        values,
    })
}

pub async fn get_global_site_options(db: &SqlitePool) -> ApiResult<GlobalSiteOptionsSnapshot> {
    let mut conn = db.acquire().await?;
    get_global_site_options_in_conn(&mut conn).await
}

pub async fn put_global_site_options_in_conn(
    conn: &mut SqliteConnection,
    expected_version: i64,
    submitted: BTreeMap<String, serde_json::Value>,
) -> ApiResult<GlobalSiteOptionsOutcome> {
    let normalized = validate_global_site_options_map(&submitted).map_err(ApiError::bad_request)?;
    let fields = global_site_option_fields();
    let collected = collect_media_ids(&fields, &normalized);
    crate::storage::fields::verify_collected_media_ids_in_conn(conn, &collected).await?;
    let data_map: BTreeMap<&str, &FieldSpec> = fields
        .iter()
        .filter(|s| !s.is_ui_only())
        .map(|s| (s.key.as_str(), s))
        .collect();

    let current_ver: Option<i64> =
        sqlx::query_scalar("SELECT version FROM site_global_option_version WHERE id = 1")
            .fetch_optional(&mut *conn)
            .await?;
    match current_ver {
        None => {
            if expected_version != 1 {
                return Ok(GlobalSiteOptionsOutcome::Stale);
            }
            sqlx::query("INSERT INTO site_global_option_version (id, version) VALUES (1, 2)")
                .execute(&mut *conn)
                .await?;
        }
        Some(ver) => {
            if ver != expected_version {
                return Ok(GlobalSiteOptionsOutcome::Stale);
            }
            let res = sqlx::query(
                "UPDATE site_global_option_version
                 SET version = version + 1,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = 1 AND version = ?",
            )
            .bind(expected_version)
            .execute(&mut *conn)
            .await?;
            if res.rows_affected() != 1 {
                return Ok(GlobalSiteOptionsOutcome::Stale);
            }
        }
    }

    for (k, v) in &normalized {
        let v_json = serde_json::to_string(v)?;
        sqlx::query(
            "INSERT INTO site_global_option_values (option_key, value_json, updated_at)
             VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(option_key) DO UPDATE SET
                 value_json = excluded.value_json,
                 updated_at = excluded.updated_at",
        )
        .bind(k)
        .bind(v_json)
        .execute(&mut *conn)
        .await?;

        if data_map
            .get(k.as_str())
            .is_some_and(|spec| spec.field_type == FieldType::Media)
        {
            sqlx::query("DELETE FROM site_global_option_media_refs WHERE option_key = ?")
                .bind(k)
                .execute(&mut *conn)
                .await?;
            if let Some(media_id) = v.as_str()
                && !media_id.is_empty()
            {
                sqlx::query(
                    "INSERT INTO site_global_option_media_refs (option_key, media_id) VALUES (?, ?)",
                )
                .bind(k)
                .bind(media_id)
                .execute(&mut *conn)
                .await?;
            }
        }
    }

    let snapshot = get_global_site_options_in_conn(conn).await?;
    Ok(GlobalSiteOptionsOutcome::Saved(snapshot))
}

pub async fn put_global_site_options(
    db: &SqlitePool,
    expected_version: i64,
    submitted: BTreeMap<String, serde_json::Value>,
) -> ApiResult<GlobalSiteOptionsOutcome> {
    let mut tx = db.begin().await?;
    let outcome = put_global_site_options_in_conn(&mut tx, expected_version, submitted).await?;
    if matches!(outcome, GlobalSiteOptionsOutcome::Saved(_)) {
        tx.commit().await?;
        crate::cache::SiteCacheEpoch::global().bump();
    }
    Ok(outcome)
}

async fn load_theme_option_maps_in_conn(
    conn: &mut SqliteConnection,
    theme_id: &str,
    default_locale: &str,
    target_locale: &str,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    let global_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT option_key, value_json FROM theme_option_values WHERE owner_kind = 'theme' AND owner_id = ?",
    )
    .bind(theme_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut global_map = BTreeMap::new();
    for (k, v_json) in global_rows {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
            global_map.insert(k, v);
        }
    }

    let mut locale_map = BTreeMap::new();
    if target_locale != default_locale {
        let loc_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT option_key, value_json
               FROM theme_option_locale_values
              WHERE theme_id = ? AND locale = ?",
        )
        .bind(theme_id)
        .bind(target_locale)
        .fetch_all(&mut *conn)
        .await?;
        for (k, v_json) in loc_rows {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
                locale_map.insert(k, v);
            }
        }
    }

    Ok((global_map, locale_map))
}

async fn sync_theme_option_media_refs_in_conn(
    conn: &mut SqliteConnection,
    theme_id: &str,
    owner_kind: &str,
    owner_id: &str,
    locale: &str,
    option_key: &str,
    media_ids: &BTreeSet<String>,
) -> ApiResult<()> {
    sqlx::query(
        "DELETE FROM theme_option_media_refs
          WHERE owner_kind = ? AND owner_id = ? AND locale = ? AND option_key = ?",
    )
    .bind(owner_kind)
    .bind(owner_id)
    .bind(locale)
    .bind(option_key)
    .execute(&mut *conn)
    .await?;
    for media_id in media_ids {
        sqlx::query(
            "INSERT OR IGNORE INTO theme_option_media_refs
                (theme_id, owner_kind, owner_id, locale, option_key, media_id)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(theme_id)
        .bind(owner_kind)
        .bind(owner_id)
        .bind(locale)
        .bind(option_key)
        .bind(media_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn get_theme_options_in_conn_for_locale(
    conn: &mut SqliteConnection,
    theme_id: &str,
    requested_locale: Option<&str>,
) -> ApiResult<Option<ThemeOptionsSnapshot>> {
    let default_locale = get_default_locale_code_in_conn(conn).await?;
    let target_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;
    let Some(manifest_json) = latest_revision_manifest_in_conn(conn, theme_id).await? else {
        return Ok(None);
    };
    let fields = parse_theme_options_schema(&manifest_json).map_err(ApiError::bad_request)?;
    let (global_map, locale_map) =
        load_theme_option_maps_in_conn(conn, theme_id, &default_locale, &target_locale).await?;

    let version: i64 = if target_locale == default_locale {
        sqlx::query_scalar(
            "SELECT version FROM theme_option_versions WHERE owner_kind = 'theme' AND owner_id = ?",
        )
        .bind(theme_id)
        .fetch_optional(&mut *conn)
        .await?
        .unwrap_or(1)
    } else {
        sqlx::query_scalar(
            "SELECT version FROM theme_option_locale_versions WHERE theme_id = ? AND locale = ?",
        )
        .bind(theme_id)
        .bind(&target_locale)
        .fetch_optional(&mut *conn)
        .await?
        .unwrap_or(1)
    };

    let mut values = BTreeMap::new();
    for spec in &fields {
        if spec.is_ui_only() {
            continue;
        }
        let v = if target_locale != default_locale && spec.translatable {
            locale_map
                .get(&spec.key)
                .or_else(|| global_map.get(&spec.key))
                .cloned()
                .unwrap_or_else(|| default_value_for_field_spec(spec))
        } else {
            global_map
                .get(&spec.key)
                .cloned()
                .unwrap_or_else(|| default_value_for_field_spec(spec))
        };
        values.insert(spec.key.clone(), v);
    }

    Ok(Some(ThemeOptionsSnapshot {
        theme_id: theme_id.to_owned(),
        version,
        fields,
        values,
        locale: requested_locale.map(|_| target_locale),
    }))
}

pub async fn get_theme_options_for_locale(
    db: &SqlitePool,
    theme_id: &str,
    requested_locale: Option<&str>,
) -> ApiResult<Option<ThemeOptionsSnapshot>> {
    let mut tx = db.begin().await?;
    let res = get_theme_options_in_conn_for_locale(&mut tx, theme_id, requested_locale).await?;
    tx.commit().await?;
    Ok(res)
}

pub async fn put_theme_options_in_conn_for_locale(
    conn: &mut SqliteConnection,
    theme_id: &str,
    expected_version: i64,
    submitted: BTreeMap<String, serde_json::Value>,
    requested_locale: Option<&str>,
) -> ApiResult<ThemeOptionsOutcome> {
    let default_locale = get_default_locale_code_in_conn(conn).await?;
    let target_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;
    let Some(manifest_json) = latest_revision_manifest_in_conn(conn, theme_id).await? else {
        return Ok(ThemeOptionsOutcome::NotFound);
    };
    let specs = parse_theme_options_schema(&manifest_json).map_err(ApiError::bad_request)?;
    let spec_map: BTreeMap<&str, &FieldSpec> = specs
        .iter()
        .filter(|s| !s.is_ui_only())
        .map(|s| (s.key.as_str(), s))
        .collect();

    let is_secondary = target_locale != default_locale;
    for k in submitted.keys() {
        let Some(spec) = spec_map.get(k.as_str()) else {
            return Err(ApiError::bad_request(
                "Option key không tồn tại trong schema của theme.",
            ));
        };
        if is_secondary && !spec.translatable {
            return Err(ApiError::bad_request(
                "Không thể lưu option không hỗ trợ dịch ở ngôn ngữ phụ.",
            ));
        }
    }

    let (global_map, locale_map) =
        load_theme_option_maps_in_conn(conn, theme_id, &default_locale, &target_locale).await?;
    let mut saved_effective = BTreeMap::new();
    let mut merged_for_validation = BTreeMap::new();
    for spec in &specs {
        if spec.is_ui_only() {
            continue;
        }
        let saved_val = if is_secondary && spec.translatable {
            locale_map
                .get(&spec.key)
                .or_else(|| global_map.get(&spec.key))
        } else {
            global_map.get(&spec.key)
        };
        if let Some(sv) = saved_val {
            saved_effective.insert(spec.key.clone(), sv.clone());
        }
        let effective = submitted
            .get(&spec.key)
            .cloned()
            .or_else(|| saved_val.cloned())
            .unwrap_or_else(|| default_value_for_field_spec(spec));
        merged_for_validation.insert(spec.key.clone(), effective);
    }

    validate_field_values(
        &specs,
        FieldContext::Options,
        &merged_for_validation,
        Some(&saved_effective),
    )
    .map_err(ApiError::bad_request)?;

    let mut submitted_specs = Vec::new();
    for (k, v) in &submitted {
        let spec = spec_map[k.as_str()];
        validate_url_or_link_spec_strict(spec, v).map_err(ApiError::bad_request)?;
        submitted_specs.push(spec.clone());
    }

    let collected_media = collect_media_ids(&submitted_specs, &submitted);
    crate::storage::fields::verify_collected_media_ids_in_conn(conn, &collected_media).await?;
    crate::storage::fields::validate_relational_refs_in_conn(
        conn,
        &specs,
        &merged_for_validation,
        Some(&saved_effective),
        is_secondary.then_some(target_locale.as_str()),
    )
    .await?;

    if is_secondary {
        let current_ver: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM theme_option_locale_versions WHERE theme_id = ? AND locale = ?",
        )
        .bind(theme_id)
        .bind(&target_locale)
        .fetch_optional(&mut *conn)
        .await?;
        match current_ver {
            None => {
                if expected_version != 1 {
                    return Ok(ThemeOptionsOutcome::Stale);
                }
                sqlx::query(
                    "INSERT INTO theme_option_locale_versions (theme_id, locale, version)
                     VALUES (?, ?, 2)",
                )
                .bind(theme_id)
                .bind(&target_locale)
                .execute(&mut *conn)
                .await?;
            }
            Some(ver) => {
                if ver != expected_version {
                    return Ok(ThemeOptionsOutcome::Stale);
                }
                let res = sqlx::query(
                    "UPDATE theme_option_locale_versions
                        SET version = version + 1,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                      WHERE theme_id = ? AND locale = ? AND version = ?",
                )
                .bind(theme_id)
                .bind(&target_locale)
                .bind(expected_version)
                .execute(&mut *conn)
                .await?;
                if res.rows_affected() != 1 {
                    return Ok(ThemeOptionsOutcome::Stale);
                }
            }
        }
        for (k, v) in &submitted {
            let v_json = serde_json::to_string(v)?;
            sqlx::query(
                "INSERT INTO theme_option_locale_values (theme_id, locale, option_key, value_json, updated_at)
                 VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(theme_id, locale, option_key) DO UPDATE SET
                     value_json = excluded.value_json,
                     updated_at = excluded.updated_at",
            )
            .bind(theme_id)
            .bind(&target_locale)
            .bind(k)
            .bind(v_json)
            .execute(&mut *conn)
            .await?;

            let spec = spec_map[k.as_str()];
            if matches!(
                spec.field_type,
                FieldType::Media | FieldType::Gallery | FieldType::Group | FieldType::Repeater
            ) {
                let single_map = BTreeMap::from([(k.clone(), v.clone())]);
                let media_ids =
                    collect_media_ids(std::slice::from_ref(spec), &single_map).all_ids();
                sync_theme_option_media_refs_in_conn(
                    conn,
                    theme_id,
                    "theme",
                    theme_id,
                    &target_locale,
                    k,
                    &media_ids,
                )
                .await?;
            }
        }
    } else {
        let current_ver: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM theme_option_versions WHERE owner_kind = 'theme' AND owner_id = ?",
        )
        .bind(theme_id)
        .fetch_optional(&mut *conn)
        .await?;

        match current_ver {
            None => {
                if expected_version != 1 {
                    return Ok(ThemeOptionsOutcome::Stale);
                }
                sqlx::query(
                    "INSERT INTO theme_option_versions (theme_id, owner_kind, owner_id, version)
                     VALUES (?, 'theme', ?, 2)",
                )
                .bind(theme_id)
                .bind(theme_id)
                .execute(&mut *conn)
                .await?;
            }
            Some(ver) => {
                if ver != expected_version {
                    return Ok(ThemeOptionsOutcome::Stale);
                }
                let res = sqlx::query(
                    "UPDATE theme_option_versions
                     SET version = version + 1,
                         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE owner_kind = 'theme' AND owner_id = ? AND version = ?",
                )
                .bind(theme_id)
                .bind(expected_version)
                .execute(&mut *conn)
                .await?;
                if res.rows_affected() != 1 {
                    return Ok(ThemeOptionsOutcome::Stale);
                }
            }
        }

        for (k, v) in &submitted {
            let v_json = serde_json::to_string(v)?;
            sqlx::query(
                "INSERT INTO theme_option_values (theme_id, owner_kind, owner_id, option_key, value_json, updated_at)
                 VALUES (?, 'theme', ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(owner_kind, owner_id, option_key) DO UPDATE SET
                     value_json = excluded.value_json,
                     updated_at = excluded.updated_at",
            )
            .bind(theme_id)
            .bind(theme_id)
            .bind(k)
            .bind(v_json)
            .execute(&mut *conn)
            .await?;

            let spec = spec_map[k.as_str()];
            if matches!(
                spec.field_type,
                FieldType::Media | FieldType::Gallery | FieldType::Group | FieldType::Repeater
            ) {
                let single_map = BTreeMap::from([(k.clone(), v.clone())]);
                let media_ids =
                    collect_media_ids(std::slice::from_ref(spec), &single_map).all_ids();
                sync_theme_option_media_refs_in_conn(
                    conn, theme_id, "theme", theme_id, "", k, &media_ids,
                )
                .await?;
            }
        }
    }

    let snapshot = get_theme_options_in_conn_for_locale(conn, theme_id, requested_locale)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được theme options sau khi lưu."))?;
    Ok(ThemeOptionsOutcome::Saved(snapshot))
}

pub async fn put_theme_options_for_locale(
    db: &SqlitePool,
    theme_id: &str,
    expected_version: i64,
    submitted: BTreeMap<String, serde_json::Value>,
    requested_locale: Option<&str>,
) -> ApiResult<ThemeOptionsOutcome> {
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let outcome = put_theme_options_in_conn_for_locale(
        &mut conn,
        theme_id,
        expected_version,
        submitted,
        requested_locale,
    )
    .await;
    match outcome {
        Ok(ThemeOptionsOutcome::Saved(snap)) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(ThemeOptionsOutcome::Saved(snap))
        }
        Ok(other) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Ok(other)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

pub async fn check_theme_options_compatibility(
    db: &SqlitePool,
    theme_id: &str,
    new_manifest_json: &str,
) -> ApiResult<()> {
    let new_specs = parse_theme_options_schema(new_manifest_json).map_err(ApiError::bad_request)?;
    let new_map: BTreeMap<&str, &FieldSpec> = new_specs
        .iter()
        .filter(|s| !s.is_ui_only())
        .map(|s| (s.key.as_str(), s))
        .collect();

    let saved_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT option_key, value_json FROM theme_option_values WHERE owner_kind = 'theme' AND owner_id = ?",
    )
    .bind(theme_id)
    .fetch_all(db)
    .await?;

    let locale_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT option_key, value_json FROM theme_option_locale_values WHERE theme_id = ?",
    )
    .bind(theme_id)
    .fetch_all(db)
    .await?;

    for (key, value_json) in saved_rows.into_iter().chain(locale_rows) {
        let Some(&spec) = new_map.get(key.as_str()) else {
            // Option removed in new revision: retained as dormant data.
            continue;
        };
        let Ok(saved_val) = serde_json::from_str::<serde_json::Value>(&value_json) else {
            return Err(ApiError::conflict(
                "Dữ liệu theme option đã lưu không tương thích với revision mới.",
            ));
        };
        let single_map = BTreeMap::from([(key, saved_val.clone())]);
        if validate_field_values(
            std::slice::from_ref(spec),
            FieldContext::Options,
            &single_map,
            None,
        )
        .is_err()
            || validate_url_or_link_spec_strict(spec, &saved_val).is_err()
        {
            return Err(ApiError::conflict(
                "Schema option của revision mới không tương thích với giá trị đã lưu (409).",
            ));
        }
    }
    Ok(())
}

fn api_error_to_unprocessable(err: ApiError) -> ApiError {
    ApiError(axum::http::StatusCode::UNPROCESSABLE_ENTITY, err.1)
}

pub(super) async fn resolve_merged_public_options_with_overrides_in_conn_with_nonces(
    conn: &mut SqliteConnection,
    active_theme: Option<(&str, &str)>,
    transient_global: Option<&BTreeMap<String, serde_json::Value>>,
    transient_theme: Option<&BTreeMap<String, serde_json::Value>>,
    is_preview: bool,
    requested_locale: Option<&str>,
    mut richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    let default_locale = get_default_locale_code_in_conn(conn).await?;
    let target_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;

    let global_fields = global_site_option_fields();
    let global_rows: Vec<(String, String)> =
        sqlx::query_as("SELECT option_key, value_json FROM site_global_option_values")
            .fetch_all(&mut *conn)
            .await?;
    let mut global_raw = global_site_option_defaults();
    for (k, v_json) in global_rows {
        if global_raw.contains_key(&k)
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json)
        {
            global_raw.insert(k, v);
        }
    }

    if let Some(g_ov) = transient_global {
        let normalized_g =
            validate_global_site_options_map(g_ov).map_err(ApiError::unprocessable)?;
        let g_media = collect_media_ids(&global_fields, &normalized_g);
        crate::storage::fields::verify_collected_media_ids_in_conn(conn, &g_media)
            .await
            .map_err(api_error_to_unprocessable)?;
        for (k, v) in normalized_g {
            global_raw.insert(k, v);
        }
    }

    let mut theme_specs = Vec::new();
    let mut theme_raw = BTreeMap::new();
    if let Some((theme_id, manifest_json)) = active_theme {
        theme_specs = parse_theme_options_schema(manifest_json).unwrap_or_default();
        if !theme_specs.is_empty() {
            let (saved_global_map, locale_map) =
                load_theme_option_maps_in_conn(conn, theme_id, &default_locale, &target_locale)
                    .await?;
            for spec in &theme_specs {
                if spec.is_ui_only() {
                    continue;
                }
                let maybe_val = if target_locale != default_locale && spec.translatable {
                    locale_map
                        .get(&spec.key)
                        .or_else(|| saved_global_map.get(&spec.key))
                        .cloned()
                } else {
                    saved_global_map.get(&spec.key).cloned()
                };
                if let Some(v) = maybe_val {
                    theme_raw.insert(spec.key.clone(), v);
                }
            }
        }

        if let Some(ov) = transient_theme {
            let spec_map: BTreeMap<&str, &FieldSpec> = theme_specs
                .iter()
                .filter(|s| !s.is_ui_only())
                .map(|s| (s.key.as_str(), s))
                .collect();
            let mut merged_for_validation = BTreeMap::new();
            for spec in &theme_specs {
                if spec.is_ui_only() {
                    continue;
                }
                let val = ov
                    .get(&spec.key)
                    .cloned()
                    .or_else(|| theme_raw.get(&spec.key).cloned())
                    .unwrap_or_else(|| default_value_for_field_spec(spec));
                merged_for_validation.insert(spec.key.clone(), val);
            }
            for (k, v) in ov {
                let Some(&spec) = spec_map.get(k.as_str()) else {
                    return Err(ApiError::unprocessable(
                        "Option key không tồn tại trong schema của theme.",
                    ));
                };
                validate_url_or_link_spec_strict(spec, v).map_err(ApiError::unprocessable)?;
            }
            validate_field_values(
                &theme_specs,
                FieldContext::Options,
                &merged_for_validation,
                Some(&theme_raw),
            )
            .map_err(ApiError::unprocessable)?;

            let ov_specs: Vec<FieldSpec> = ov
                .keys()
                .filter_map(|k| spec_map.get(k.as_str()).map(|s| (*s).clone()))
                .collect();
            let ov_media = collect_media_ids(&ov_specs, ov);
            crate::storage::fields::verify_collected_media_ids_in_conn(conn, &ov_media)
                .await
                .map_err(api_error_to_unprocessable)?;
            let req_loc = (target_locale != default_locale).then_some(target_locale.as_str());
            crate::storage::fields::validate_relational_refs_in_conn(
                conn,
                &theme_specs,
                &merged_for_validation,
                Some(&theme_raw),
                req_loc,
            )
            .await
            .map_err(api_error_to_unprocessable)?;

            for (k, v) in ov {
                theme_raw.insert(k.clone(), v.clone());
            }
        }
    } else if transient_theme.is_some_and(|m| !m.is_empty()) {
        return Err(ApiError::unprocessable(
            "Không có theme v3 đang kích hoạt để xem trước theme_options.",
        ));
    }

    let mut media_ids = BTreeSet::new();
    let mut entry_ids = BTreeSet::new();
    let mut term_ids = BTreeSet::new();
    crate::field_hydrate::collect_hydration_ids(
        &global_fields,
        &global_raw,
        true,
        &mut media_ids,
        &mut entry_ids,
        &mut term_ids,
    );
    crate::field_hydrate::collect_hydration_ids(
        &theme_specs,
        &theme_raw,
        true,
        &mut media_ids,
        &mut entry_ids,
        &mut term_ids,
    );

    let entries = crate::storage::fields::load_entry_ref_hydration_map_in_conn(
        conn,
        &entry_ids,
        is_preview,
        Some(&target_locale),
    )
    .await?;
    let terms = crate::storage::fields::load_term_ref_hydration_map_in_conn(
        conn,
        &term_ids,
        is_preview,
        Some(&target_locale),
    )
    .await?;
    for term in terms.values() {
        if let Some(ref mid) = term.featured_image_media_id {
            media_ids.insert(mid.clone());
        }
    }
    let media = crate::storage::fields::load_media_hydration_map_in_conn(conn, &media_ids).await?;

    let ctx = crate::field_hydrate::HydrationContext {
        media: &media,
        entries: &entries,
        terms: &terms,
        is_preview,
    };
    let site_options = crate::field_hydrate::hydrate_field_values(
        &global_fields,
        &global_raw,
        &ctx,
        richtext_nonces.as_deref_mut(),
        true,
    )
    .map_err(ApiError::internal)?;

    let mut merged = site_options.clone();
    if !theme_specs.is_empty() {
        let theme_hydrated = crate::field_hydrate::hydrate_field_values(
            &theme_specs,
            &theme_raw,
            &ctx,
            richtext_nonces,
            true,
        )
        .map_err(ApiError::internal)?;
        merged.extend(theme_hydrated);
    }

    Ok((site_options, merged))
}

pub async fn get_active_merged_public_options_in_conn_for_locale_with_nonces(
    conn: &mut SqliteConnection,
    is_preview: bool,
    requested_locale: Option<&str>,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    let active: Option<(String, String)> = if let Some(dev) =
        crate::dev_mode::DevOverlayConfig::global()
        && let Ok(Some(snap)) = dev.load_dev_theme()
    {
        Some((snap.manifest.theme_id, snap.manifest_json))
    } else {
        sqlx::query_as(
            "SELECT r.theme_id, r.manifest_json
             FROM theme_active_selection AS a
             JOIN theme_revisions AS r ON r.revision_id = a.revision_id
             WHERE a.id = 1 AND r.source_format = 'v3'",
        )
        .fetch_optional(&mut *conn)
        .await?
    };
    let active_ref = active.as_ref().map(|(id, m)| (id.as_str(), m.as_str()));
    resolve_merged_public_options_with_overrides_in_conn_with_nonces(
        conn,
        active_ref,
        None,
        None,
        is_preview,
        requested_locale,
        richtext_nonces,
    )
    .await
}

pub async fn get_active_merged_public_options_in_conn_for_locale(
    conn: &mut SqliteConnection,
    is_preview: bool,
    requested_locale: Option<&str>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    get_active_merged_public_options_in_conn_for_locale_with_nonces(
        conn,
        is_preview,
        requested_locale,
        None,
    )
    .await
}

pub async fn get_active_merged_public_options_in_conn(
    conn: &mut SqliteConnection,
    is_preview: bool,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    get_active_merged_public_options_in_conn_for_locale(conn, is_preview, None).await
}

/// Returns `(site_options, merged_options)` where `merged_options` overlays
/// active theme `public: true` options on top of durable Global public options.
pub async fn get_active_public_theme_options_for_locale_with_nonces(
    db: &SqlitePool,
    requested_locale: Option<&str>,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    let mut conn = db.acquire().await?;
    get_active_merged_public_options_in_conn_for_locale_with_nonces(
        &mut conn,
        false,
        requested_locale,
        richtext_nonces,
    )
    .await
}

pub async fn get_active_public_theme_options_for_locale(
    db: &SqlitePool,
    requested_locale: Option<&str>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    get_active_public_theme_options_for_locale_with_nonces(db, requested_locale, None).await
}

pub async fn get_active_public_theme_options(
    db: &SqlitePool,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    get_active_public_theme_options_for_locale(db, None).await
}

/// Resolve merged global + public theme values against the revision already captured by the caller.
pub(super) async fn get_public_theme_options_in_conn_for_locale_with_nonces(
    conn: &mut SqliteConnection,
    theme_id: &str,
    manifest_json: &str,
    requested_locale: Option<&str>,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    resolve_merged_public_options_with_overrides_in_conn_with_nonces(
        conn,
        Some((theme_id, manifest_json)),
        None,
        None,
        false,
        requested_locale,
        richtext_nonces,
    )
    .await
}

pub(super) async fn get_public_theme_options_in_conn_for_locale(
    conn: &mut SqliteConnection,
    theme_id: &str,
    manifest_json: &str,
    requested_locale: Option<&str>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    get_public_theme_options_in_conn_for_locale_with_nonces(
        conn,
        theme_id,
        manifest_json,
        requested_locale,
        None,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_url_policy_requires_public_url_schema_and_valid_defaults() {
        let schema = serde_json::json!({"options": [
            {"key":"cta_url","label":"CTA","type":"url","default":"/lien-he","public":true},
            {"key":"private_url","label":"Private","type":"url","default":"#secret","public":false},
            {"key":"non_url_text","label":"Text","type":"text","default":"/old","public":true}
        ]});
        let raw = schema.to_string();
        let policy = theme_url_policy(&raw).unwrap();
        let src = r#"<a href="{{ options.cta_url }}">Link</a>"#;
        assert!(
            crate::template_sec::validate_template_csec_with_theme_urls("x", src, &policy).is_ok()
        );
        for key in ["private_url", "non_url_text", "unknown_url"] {
            assert!(
                crate::template_sec::validate_template_csec_with_theme_urls(
                    "x",
                    &format!(r#"<a href="{{{{ options.{key} }}}}">Link</a>"#),
                    &policy,
                )
                .is_err()
            );
        }
        let invalid = serde_json::json!({"options": [{
            "key":"cta_url", "label":"CTA", "type":"url", "default":"javascript:alert(1)", "public":true
        }]});
        assert!(parse_theme_options_schema(&invalid.to_string()).is_err());
        let parsed = parse_theme_options_schema(&raw).unwrap();
        let vals = BTreeMap::from([(
            "cta_url".to_owned(),
            serde_json::json!("/changed?from=header#form"),
        )]);
        assert!(validate_field_values(&parsed, FieldContext::Options, &vals, None).is_ok());
        assert!(validate_url_or_link_spec_strict(&parsed[0], &vals["cta_url"]).is_ok());
    }

    #[test]
    fn catalog_is_pinned_sorted_and_restricts_aliases() {
        assert!(timezone_catalog().windows(2).all(|pair| pair[0] < pair[1]));
        assert!(is_current_timezone("UTC"));
        assert!(is_current_timezone("Asia/Ho_Chi_Minh"));
        assert!(is_current_timezone("America/New_York"));
        assert!(!is_current_timezone("US/Eastern"));
        assert!(!is_current_timezone("../UTC"));
    }

    #[test]
    fn unicode_counts_are_scalars_and_controls_are_invalid() {
        let make = |site_title: String, tagline: &str| GeneralSettingsInput {
            site_title,
            tagline: tagline.into(),
            timezone: "UTC".into(),
            cms_locale: None,
        };
        assert!(make(" Xin chào ".into(), "").validate().is_ok());
        assert!(make("🦀".repeat(120), "").validate().is_ok());
        assert!(make("🦀".repeat(121), "").validate().is_err());
        assert!(make(" \t ".into(), "").validate().is_err());
        assert!(make("\tAccepted?".into(), "").validate().is_err());
        assert!(make("ok".into(), "bad\nvalue").validate().is_err());
    }

    #[tokio::test]
    async fn cms_locale_is_global_canonical_and_backward_compatible() {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::storage::content::preflight_builtin_key_collisions(&db)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let initial = get_general(&db).await.unwrap();
        assert_eq!(initial.cms_locale, "en");
        let input = |cms_locale: Option<&str>| GeneralSettingsInput {
            site_title: initial.site_title.clone(),
            tagline: initial.tagline.clone(),
            timezone: initial.timezone.clone(),
            cms_locale: cms_locale.map(str::to_owned),
        };
        assert!(matches!(
            save_general(&db, input(Some("unknown")), initial.version)
                .await
                .unwrap(),
            SaveOutcome::InvalidCmsLocale
        ));
        let SaveOutcome::Saved(saved) = save_general(&db, input(Some(" Vi-VN ")), initial.version)
            .await
            .unwrap()
        else {
            panic!("canonical CMS locale save failed");
        };
        assert_eq!(saved.cms_locale, "vi");
        assert_eq!(
            get_admin_i18n_bootstrap(&db)
                .await
                .unwrap()
                .effective_locale,
            "vi"
        );
        assert!(matches!(
            save_general(&db, input(Some("en")), initial.version)
                .await
                .unwrap(),
            SaveOutcome::Stale
        ));
        let legacy: GeneralSettingsInput = serde_json::from_value(serde_json::json!({
            "site_title": initial.site_title,
            "tagline": initial.tagline,
            "timezone": initial.timezone,
        }))
        .unwrap();
        let SaveOutcome::Saved(saved) = save_general(&db, legacy, saved.version).await.unwrap()
        else {
            panic!("legacy payload save failed");
        };
        assert_eq!(saved.cms_locale, "vi");
        sqlx::query(
            "INSERT INTO site_locales (code, bcp47_tag, label, direction, is_default, enabled, position)
             VALUES ('en', 'en', 'English', 'ltr', 0, 1, 1)",
        ).execute(&db).await.unwrap();
        let secondary = get_general_for_locale(&db, Some("en")).await.unwrap();
        assert_eq!(secondary.cms_locale, "vi");
        assert!(matches!(
            save_general_for_locale(&db, Some("en"), input(Some("en")), secondary.version)
                .await
                .unwrap(),
            SaveOutcome::SecondaryCmsLocaleLocked
        ));
        let SaveOutcome::Saved(secondary_saved) =
            save_general_for_locale(&db, Some("en"), input(Some(" VI ")), secondary.version)
                .await
                .unwrap()
        else {
            panic!("secondary save preserving CMS locale failed");
        };
        assert_eq!(secondary_saved.cms_locale, "vi");
        let SaveOutcome::Saved(saved) = save_general(&db, input(Some("ar")), saved.version)
            .await
            .unwrap()
        else {
            panic!("uninstalled CMS locale save failed");
        };
        assert_eq!(saved.cms_locale, "ar");
        let boot = get_admin_i18n_bootstrap(&db).await.unwrap();
        assert_eq!(boot.configured_locale, "ar");
        assert!(!boot.configured_installed);
        assert_eq!(
            (
                boot.effective_locale,
                boot.effective_bcp47,
                boot.effective_direction
            ),
            ("en", "en-US", "ltr")
        );
        assert_eq!(boot.available_locales.len(), 32);
    }

    #[tokio::test]
    async fn global_site_options_crud_cas_empty_url_and_unified_theme_options() {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::storage::content::preflight_builtin_key_collisions(&db)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();

        let snap = get_global_site_options(&db).await.unwrap();
        assert_eq!(snap.version, 1);
        assert_eq!(snap.fields.len(), 14);
        assert_eq!(snap.values.get("social_facebook").unwrap(), "");
        assert_eq!(snap.values.get("brand_primary_color").unwrap(), "#2563eb");

        // Stale CAS version returns Stale
        let stale = put_global_site_options(
            &db,
            99,
            BTreeMap::from([(
                "social_facebook".into(),
                serde_json::json!("https://fb.com/od"),
            )]),
        )
        .await
        .unwrap();
        assert!(matches!(stale, GlobalSiteOptionsOutcome::Stale));

        // Unsafe URL fails validation even though empty string is allowed for optional channels
        assert!(
            put_global_site_options(
                &db,
                1,
                BTreeMap::from([(
                    "social_facebook".into(),
                    serde_json::json!("javascript:alert(1)")
                )]),
            )
            .await
            .is_err()
        );
        assert!(
            put_global_site_options(
                &db,
                1,
                BTreeMap::from([("unknown_global".into(), serde_json::json!("x"))]),
            )
            .await
            .is_err()
        );

        let saved = put_global_site_options(
            &db,
            1,
            BTreeMap::from([
                (
                    "social_facebook".into(),
                    serde_json::json!("https://fb.com/opendash"),
                ),
                ("social_youtube".into(), serde_json::json!("")),
                ("contact_phone".into(), serde_json::json!("0901234567")),
            ]),
        )
        .await
        .unwrap();
        let GlobalSiteOptionsOutcome::Saved(saved) = saved else {
            panic!("expected Saved outcome");
        };
        assert_eq!(saved.version, 2);
        assert_eq!(
            saved.values.get("social_facebook").unwrap(),
            "https://fb.com/opendash"
        );
        assert_eq!(saved.values.get("contact_phone").unwrap(), "0901234567");

        // Public merged options expose saved global options even without an active theme.
        let (site_opts, public_opts) = get_active_public_theme_options(&db).await.unwrap();
        assert_eq!(
            site_opts.get("social_facebook").unwrap(),
            "https://fb.com/opendash"
        );
        assert_eq!(
            public_opts.get("social_facebook").unwrap(),
            "https://fb.com/opendash"
        );
        assert_eq!(public_opts.get("social_youtube").unwrap(), "");

        // Unified FieldSpec theme.json options parse and validate nested groups cleanly.
        let manifest = serde_json::json!({
            "options": [
                {"key": "headline", "label": "Headline", "type": "text", "default": "ok", "public": true},
                {"key": "accent", "label": "Màu nhấn", "type": "color", "default": "#2563eb", "public": true},
                {
                    "key": "layout_group",
                    "label": "Bố cục",
                    "type": "group",
                    "fields": [
                        {
                            "key": "header_style",
                            "label": "Kiểu Header",
                            "type": "radio_buttons",
                            "options": [{"value": "clean", "label": "Gọn"}, {"value": "full", "label": "Đầy"}],
                            "default": "clean",
                            "public": true
                        },
                        {"key": "optional_cta", "label": "CTA phụ", "type": "url", "default": "", "required": false, "public": true}
                    ]
                }
            ]
        })
        .to_string();
        let fields = parse_theme_options_schema(&manifest).unwrap();
        assert_eq!(fields.len(), 3);
        let optional_cta_spec = &fields[2].fields[1];
        assert!(
            validate_url_or_link_spec_strict(optional_cta_spec, &serde_json::json!("")).is_ok()
        );
        assert!(
            validate_url_or_link_spec_strict(
                optional_cta_spec,
                &serde_json::json!("javascript:alert(1)")
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn public_theme_options_merge_locale_overlays_over_global_defaults() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO site_locales (code, bcp47_tag, label, direction, is_default, enabled, position)
             VALUES ('en', 'en', 'English', 'ltr', 0, 1, 1)",
        )
        .execute(&db)
        .await
        .unwrap();
        let manifest = serde_json::json!({
            "manifest_version": 3,
            "theme_id": "t-l10n",
            "name": "T",
            "version": "1.0.0",
            "templates": [{ "kind": "index", "file": "index.html" }],
            "options": [
                { "key": "hero_title", "label": "Hero", "type": "text", "default": "Mac dinh", "public": true, "translatable": true },
                { "key": "cta_url", "label": "CTA", "type": "url", "default": "/vi", "public": true, "translatable": true },
                { "key": "accent", "label": "Accent", "type": "color", "default": "#111111", "public": true, "translatable": false }
            ]
        })
        .to_string();
        sqlx::query(
            "INSERT INTO theme_packages (theme_id, display_name, source_format)
             VALUES ('t-l10n', 'T', 'v3')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO theme_revisions (revision_id, theme_id, version, revision_digest, source_format, manifest_json, file_digests_json, provenance_json)
             VALUES ('rev-1', 't-l10n', '1.0.0', 'dig-1', 'v3', ?, '{}', '{}')",
        )
        .bind(&manifest)
        .execute(&db)
        .await
        .unwrap();

        let mut tx = db.begin().await.unwrap();
        put_theme_options_in_conn_for_locale(
            &mut tx,
            "t-l10n",
            1,
            BTreeMap::from([
                ("hero_title".into(), serde_json::json!("Trang chu")),
                ("accent".into(), serde_json::json!("#222222")),
            ]),
            None,
        )
        .await
        .unwrap();
        put_theme_options_in_conn_for_locale(
            &mut tx,
            "t-l10n",
            1,
            BTreeMap::from([("cta_url".into(), serde_json::json!("/en/contact"))]),
            Some("en"),
        )
        .await
        .unwrap();
        let (_, vi_pub) =
            get_public_theme_options_in_conn_for_locale(&mut tx, "t-l10n", &manifest, None)
                .await
                .unwrap();
        let (_, en_pub) =
            get_public_theme_options_in_conn_for_locale(&mut tx, "t-l10n", &manifest, Some("en"))
                .await
                .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(vi_pub["hero_title"], serde_json::json!("Trang chu"));
        assert_eq!(vi_pub["cta_url"], serde_json::json!("/vi"));
        assert_eq!(vi_pub["accent"], serde_json::json!("#222222"));

        assert_eq!(en_pub["hero_title"], serde_json::json!("Trang chu"));
        assert_eq!(en_pub["cta_url"], serde_json::json!("/en/contact"));
        assert_eq!(en_pub["accent"], serde_json::json!("#222222"));
    }

    #[tokio::test]
    async fn translatable_media_gallery_group_repeater_and_orphan_locale_cleanup_work() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO site_locales (code, bcp47_tag, label, direction, is_default, enabled, position)
             VALUES ('en', 'en', 'English', 'ltr', 0, 1, 1)",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("UPDATE site_locales_state SET multilingual_enabled = 1 WHERE id = 1")
            .execute(&db)
            .await
            .unwrap();

        let img_vi = "1".repeat(64);
        let img_en = "2".repeat(64);
        let img_gal = "3".repeat(64);
        let img_rep = "4".repeat(64);
        for (idx, mid) in [&img_vi, &img_en, &img_gal, &img_rep]
            .into_iter()
            .enumerate()
        {
            sqlx::query(
                "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state)
                 VALUES (?, 'image', ?, 'image/png', 'png', 100, 10, 10, 10, 10, 'active')",
            )
            .bind(mid)
            .bind(format!("loc-{idx}.png"))
            .execute(&db)
            .await
            .unwrap();
        }

        let manifest = serde_json::json!({
            "manifest_version": 3,
            "theme_id": "t-media-l10n",
            "name": "TM",
            "version": "1.0.0",
            "templates": [{ "kind": "index", "file": "index.html" }],
            "options": [
                { "key": "hero_img", "label": "HeroImg", "type": "media", "public": true, "translatable": true },
                { "key": "gallery_imgs", "label": "Gallery", "type": "gallery", "public": true, "translatable": true },
                {
                    "key": "promo_box",
                    "label": "Promo",
                    "type": "group",
                    "public": true,
                    "translatable": true,
                    "fields": [
                        { "key": "bg", "label": "BG", "type": "media" },
                        { "key": "cta", "label": "CTA", "type": "link" }
                    ]
                },
                {
                    "key": "slides",
                    "label": "Slides",
                    "type": "repeater",
                    "public": true,
                    "translatable": true,
                    "fields": [
                        { "key": "photo", "label": "Photo", "type": "media" }
                    ]
                }
            ]
        })
        .to_string();
        sqlx::query(
            "INSERT INTO theme_packages (theme_id, display_name, source_format)
             VALUES ('t-media-l10n', 'TM', 'v3')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO theme_revisions (revision_id, theme_id, version, revision_digest, source_format, manifest_json, file_digests_json, provenance_json)
             VALUES ('rev-tm', 't-media-l10n', '1.0.0', 'dig-tm', 'v3', ?, '{}', '{}')",
        )
        .bind(&manifest)
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("UPDATE theme_active_selection SET revision_id = 'rev-tm' WHERE id = 1")
            .execute(&db)
            .await
            .unwrap();

        let mut tx = db.begin().await.unwrap();
        put_theme_options_in_conn_for_locale(
            &mut tx,
            "t-media-l10n",
            1,
            BTreeMap::from([("hero_img".into(), serde_json::json!(img_vi))]),
            None,
        )
        .await
        .unwrap();
        put_theme_options_in_conn_for_locale(
            &mut tx,
            "t-media-l10n",
            1,
            BTreeMap::from([
                ("hero_img".into(), serde_json::json!(img_en)),
                ("gallery_imgs".into(), serde_json::json!([img_gal])),
                (
                    "promo_box".into(),
                    serde_json::json!({ "bg": img_en, "cta": { "url": "/en", "label": "Go", "new_tab": false, "nofollow": false } }),
                ),
                ("slides".into(), serde_json::json!([{ "photo": img_rep }])),
            ]),
            Some("en"),
        )
        .await
        .unwrap();

        let (_, vi_pub) =
            get_public_theme_options_in_conn_for_locale(&mut tx, "t-media-l10n", &manifest, None)
                .await
                .unwrap();
        let (_, en_pub) = get_public_theme_options_in_conn_for_locale(
            &mut tx,
            "t-media-l10n",
            &manifest,
            Some("en"),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(vi_pub["hero_img"]["id"], serde_json::json!(img_vi));
        assert_eq!(en_pub["hero_img"]["id"], serde_json::json!(img_en));
        assert_eq!(en_pub["gallery_imgs"][0]["id"], serde_json::json!(img_gal));
        assert_eq!(en_pub["promo_box"]["bg"]["id"], serde_json::json!(img_en));
        assert_eq!(
            en_pub["slides"][0]["photo"]["id"],
            serde_json::json!(img_rep)
        );

        let ref_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT option_key, locale FROM theme_option_media_refs WHERE owner_id = 't-media-l10n' AND media_id = ? ORDER BY option_key, locale",
        )
        .bind(&img_en)
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(
            ref_rows,
            vec![
                ("hero_img".to_owned(), "en".to_owned()),
                ("promo_box".to_owned(), "en".to_owned()),
            ]
        );

        // Soft-delete 'en' and verify count_locale_data_in_conn + purge_orphan_locale mirror each other cleanly
        sqlx::query("UPDATE site_locales SET enabled = 0, deleted_at = '2026-01-01T00:00:00Z' WHERE code = 'en'")
            .execute(&db)
            .await
            .unwrap();
        let mut ctx = db.acquire().await.unwrap();
        let before_counts = count_locale_data_in_conn(&mut ctx, "en").await.unwrap();
        drop(ctx);
        assert!(before_counts.settings > 0);
        let orphans = list_orphaned_locales(&db).await.unwrap();
        let purged = purge_orphan_locale(
            &db,
            "en",
            orphans.version,
            PurgeOrphanLocaleInput {
                confirm_code: "en".to_owned(),
            },
        )
        .await
        .unwrap();
        assert_eq!(purged.deleted.settings, before_counts.settings);

        let default_refs_left: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM theme_option_media_refs WHERE owner_id = 't-media-l10n' AND locale = '' AND media_id = ?",
        )
        .bind(&img_vi)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(default_refs_left, 1);
    }
}

pub(crate) fn is_secret_sentinel(val: &serde_json::Value) -> Option<&str> {
    let obj = val.as_object()?;
    if obj.len() != 2 || obj.get("configured") != Some(&serde_json::Value::Bool(true)) {
        return None;
    }
    let raw = obj.get("secret_ref")?.as_str()?;
    crate::secrets::SecretRef::parse(raw).ok()?;
    Some(raw)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSettingsSnapshot {
    pub plugin_id: String,
    pub version: i64,
    pub fields: Vec<FieldSpec>,
    pub values: serde_json::Map<String, serde_json::Value>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSettingsOutcome {
    pub snapshot: PluginSettingsSnapshot,
    pub changed: bool,
    pub has_projection_filters: bool,
}

pub async fn get_plugin_manifest_for_settings(
    db: &SqlitePool,
    plugin_id: &str,
) -> ApiResult<Option<opendash_core::plugin::PluginManifest>> {
    let manifest_json: Option<String> =
        sqlx::query_scalar("SELECT manifest_json FROM plugins WHERE id = ?")
            .bind(plugin_id)
            .fetch_optional(db)
            .await?;
    let Some(raw) = manifest_json else {
        return Ok(None);
    };
    let manifest =
        opendash_core::plugin::PluginManifest::from_json_str(&raw).map_err(ApiError::internal)?;
    Ok(Some(manifest))
}

async fn get_owner_settings_redacted_in_conn(
    conn: &mut SqliteConnection,
    owner_kind_db: &str,
    owner_id: &str,
    fields: &[FieldSpec],
) -> ApiResult<PluginSettingsSnapshot> {
    let ver_row: Option<(i64, String)> = sqlx::query_as(
        "SELECT version, updated_at
           FROM theme_option_versions
          WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind_db)
    .bind(owner_id)
    .fetch_optional(&mut *conn)
    .await?;

    let (version, updated_at) = match ver_row {
        Some((v, ts)) => (v, ts),
        None => {
            let fallback_ts: Option<String> = if owner_kind_db == "plugin" {
                sqlx::query_scalar("SELECT updated_at FROM plugins WHERE id = ?")
                    .bind(owner_id)
                    .fetch_optional(&mut *conn)
                    .await?
            } else {
                None
            };
            (1, fallback_ts.unwrap_or_default())
        }
    };

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT option_key, value_json
           FROM theme_option_values
          WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind_db)
    .bind(owner_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut saved_map = BTreeMap::new();
    for (k, v_json) in rows {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
            saved_map.insert(k, v);
        }
    }

    let mut values = serde_json::Map::new();
    for field in fields {
        if field.is_ui_only() {
            continue;
        }
        if field.field_type == FieldType::Secret {
            let configured = saved_map
                .get(&field.key)
                .and_then(is_secret_sentinel)
                .is_some();
            values.insert(
                field.key.clone(),
                serde_json::json!({ "configured": configured }),
            );
        } else {
            let v = saved_map
                .get(&field.key)
                .filter(|val| {
                    // Never leak a stored secret_ref on a non-secret schema field.
                    !val.as_object()
                        .is_some_and(|obj| obj.contains_key("secret_ref"))
                })
                .cloned()
                .unwrap_or_else(|| default_value_for_field_spec(field));
            values.insert(field.key.clone(), v);
        }
    }

    Ok(PluginSettingsSnapshot {
        plugin_id: owner_id.to_owned(),
        version,
        fields: fields.to_vec(),
        values,
        updated_at,
    })
}

pub async fn get_plugin_settings_redacted(
    db: &SqlitePool,
    plugin_id: &str,
    fields: &[FieldSpec],
) -> ApiResult<PluginSettingsSnapshot> {
    let mut conn = db.acquire().await?;
    get_owner_settings_redacted_in_conn(&mut conn, "plugin", plugin_id, fields).await
}

pub async fn get_theme_functions_settings_redacted(
    db: &SqlitePool,
    theme_id: &str,
    fields: &[FieldSpec],
) -> ApiResult<PluginSettingsSnapshot> {
    let mut conn = db.acquire().await?;
    get_owner_settings_redacted_in_conn(&mut conn, "theme_functions", theme_id, fields).await
}

pub async fn put_plugin_settings(
    db: &SqlitePool,
    secrets_dir: &std::path::Path,
    plugin_id: &str,
    expected_version: i64,
    fields: &[FieldSpec],
    submitted: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<PluginSettingsOutcome> {
    use opendash_core::hooks::HookEvent;
    let manifest_json: Option<String> =
        sqlx::query_scalar("SELECT manifest_json FROM plugins WHERE id = ?")
            .bind(plugin_id)
            .fetch_optional(db)
            .await?;
    let has_projection_filters = manifest_json
        .as_deref()
        .and_then(|raw| opendash_core::plugin::PluginManifest::from_json_str(raw).ok())
        .is_some_and(|manifest| {
            manifest
                .filters
                .iter()
                .any(|f| matches!(f, HookEvent::FilterSeoHead | HookEvent::FilterEntryView))
        });
    put_owner_settings(
        db,
        secrets_dir,
        "plugin",
        "plugins",
        plugin_id,
        expected_version,
        fields,
        submitted,
        has_projection_filters,
    )
    .await
}

pub async fn put_theme_functions_settings(
    db: &SqlitePool,
    secrets_dir: &std::path::Path,
    theme_id: &str,
    expected_version: i64,
    fields: &[FieldSpec],
    submitted: &serde_json::Map<String, serde_json::Value>,
    has_projection_filters: bool,
) -> ApiResult<PluginSettingsOutcome> {
    put_owner_settings(
        db,
        secrets_dir,
        "theme_functions",
        "themes",
        theme_id,
        expected_version,
        fields,
        submitted,
        has_projection_filters,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn put_owner_settings(
    db: &SqlitePool,
    secrets_dir: &std::path::Path,
    owner_kind_db: &str,
    secrets_subdir: &str,
    owner_id: &str,
    expected_version: i64,
    fields: &[FieldSpec],
    submitted: &serde_json::Map<String, serde_json::Value>,
    has_projection_filters: bool,
) -> ApiResult<PluginSettingsOutcome> {
    let field_map: BTreeMap<&str, &FieldSpec> = fields
        .iter()
        .filter(|s| !s.is_ui_only())
        .map(|f| (f.key.as_str(), f))
        .collect();
    for key in submitted.keys() {
        if !field_map.contains_key(key.as_str()) {
            return Err(ApiError::bad_request(
                "Khóa cài đặt không tồn tại trong schema của plugin.",
            ));
        }
    }

    let mut conn = db.acquire().await?;
    let current_ver: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM theme_option_versions WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind_db)
    .bind(owner_id)
    .fetch_optional(&mut *conn)
    .await?;
    if current_ver.unwrap_or(1) != expected_version {
        return Err(ApiError::precondition_failed(
            "Cài đặt plugin đã thay đổi ở phiên khác. Tải lại trước khi lưu.",
        ));
    }

    let existing_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT option_key, value_json
           FROM theme_option_values
          WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind_db)
    .bind(owner_id)
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);

    let mut existing_map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for (k, v_json) in existing_rows {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
            existing_map.insert(k, v);
        }
    }

    enum PlannedPluginWrite {
        Upsert {
            key: String,
            value: serde_json::Value,
            media_ids: BTreeSet<String>,
            sync_media_refs: bool,
        },
        Delete {
            key: String,
        },
    }

    let store = crate::secrets::SecretStore::for_plugin_dir(
        secrets_dir.join(secrets_subdir).join(owner_id),
    );
    let mut newly_created_refs: Vec<crate::secrets::SecretRef> = Vec::new();
    let mut old_refs_to_remove: Vec<String> = Vec::new();
    let mut planned: Vec<PlannedPluginWrite> = Vec::new();
    let mut changed = false;

    let cleanup_new_refs = |refs: &[crate::secrets::SecretRef]| {
        for r in refs {
            store.remove_plugin_secret(r.as_str());
        }
    };

    let non_secret_specs: Vec<FieldSpec> = fields
        .iter()
        .filter(|s| !s.is_ui_only() && s.field_type != FieldType::Secret)
        .cloned()
        .collect();
    let mut existing_non_secret = BTreeMap::new();
    for spec in &non_secret_specs {
        if let Some(prev) = existing_map
            .get(&spec.key)
            .filter(|v| !v.as_object().is_some_and(|o| o.contains_key("secret_ref")))
        {
            existing_non_secret.insert(spec.key.clone(), prev.clone());
        }
    }

    let mut submitted_non_secret = BTreeMap::new();
    let mut submitted_media_specs = Vec::new();

    for field in fields {
        if field.is_ui_only() {
            continue;
        }
        if field.field_type == FieldType::Secret {
            let old_ref = existing_map
                .get(&field.key)
                .and_then(is_secret_sentinel)
                .map(str::to_owned);

            match submitted.get(&field.key) {
                None | Some(serde_json::Value::Null) => {
                    if field.required && old_ref.is_none() {
                        cleanup_new_refs(&newly_created_refs);
                        return Err(ApiError::bad_request(format!(
                            "Bí mật bắt buộc '{}' chưa được cấu hình.",
                            field.key
                        )));
                    }
                }
                Some(serde_json::Value::Object(obj))
                    if obj.len() == 1
                        && obj.get("clear") == Some(&serde_json::Value::Bool(true)) =>
                {
                    if field.required {
                        cleanup_new_refs(&newly_created_refs);
                        return Err(ApiError::bad_request(format!(
                            "Không thể xóa bí mật bắt buộc '{}'.",
                            field.key
                        )));
                    }
                    if let Some(old) = old_ref {
                        old_refs_to_remove.push(old);
                        changed = true;
                    } else if existing_map.contains_key(&field.key) {
                        changed = true;
                    }
                    planned.push(PlannedPluginWrite::Delete {
                        key: field.key.clone(),
                    });
                }
                Some(serde_json::Value::String(plaintext)) => {
                    if plaintext.is_empty() {
                        cleanup_new_refs(&newly_created_refs);
                        return Err(ApiError::bad_request(format!(
                            "Giá trị secret '{}' rỗng không hợp lệ; dùng null để giữ nguyên hoặc {{\"clear\": true}} để xóa.",
                            field.key
                        )));
                    }
                    let new_ref = match store.write_plugin_secret(plaintext) {
                        Ok(r) => r,
                        Err(err) => {
                            cleanup_new_refs(&newly_created_refs);
                            return Err(err);
                        }
                    };
                    let stored_val = serde_json::json!({
                        "secret_ref": new_ref.as_str(),
                        "configured": true,
                    });
                    if let Some(old) = old_ref
                        && old != new_ref.as_str()
                    {
                        old_refs_to_remove.push(old);
                    }
                    newly_created_refs.push(new_ref);
                    planned.push(PlannedPluginWrite::Upsert {
                        key: field.key.clone(),
                        value: stored_val,
                        media_ids: BTreeSet::new(),
                        sync_media_refs: false,
                    });
                    changed = true;
                }
                Some(_) => {
                    cleanup_new_refs(&newly_created_refs);
                    return Err(ApiError::bad_request(format!(
                        "Giá trị secret '{}' phải là chuỗi, null hoặc {{\"clear\": true}}.",
                        field.key
                    )));
                }
            }
            continue;
        }

        if let Some(raw_val) = submitted.get(&field.key) {
            let clean_val = if field.field_type == FieldType::Media {
                match raw_val {
                    serde_json::Value::Null => serde_json::Value::Null,
                    serde_json::Value::String(id) if id.is_empty() => serde_json::Value::Null,
                    serde_json::Value::String(id) if valid_sha256_media_id(id) => {
                        serde_json::Value::String(id.clone())
                    }
                    _ => {
                        cleanup_new_refs(&newly_created_refs);
                        return Err(ApiError::bad_request(
                            "Cài đặt media phải là media_id SHA-256 hợp lệ hoặc rỗng.",
                        ));
                    }
                }
            } else {
                raw_val.clone()
            };

            if let Err(msg) = validate_url_or_link_spec_strict(field, &clean_val) {
                cleanup_new_refs(&newly_created_refs);
                return Err(ApiError::bad_request(msg));
            }

            let prev = existing_map.get(&field.key);
            let default_v = default_value_for_field_spec(field);
            if prev != Some(&clean_val) && !(prev.is_none() && clean_val == default_v) {
                changed = true;
            }
            let sync_media_refs = matches!(
                field.field_type,
                FieldType::Media | FieldType::Gallery | FieldType::Group | FieldType::Repeater
            );
            let field_media_ids = if sync_media_refs {
                let single_map = BTreeMap::from([(field.key.clone(), clean_val.clone())]);
                collect_media_ids(std::slice::from_ref(field), &single_map).all_ids()
            } else {
                BTreeSet::new()
            };
            if sync_media_refs {
                submitted_media_specs.push(field.clone());
            }
            submitted_non_secret.insert(field.key.clone(), clean_val.clone());
            planned.push(PlannedPluginWrite::Upsert {
                key: field.key.clone(),
                value: clean_val,
                media_ids: field_media_ids,
                sync_media_refs,
            });
        }
    }

    let mut merged_non_secret = BTreeMap::new();
    for spec in &non_secret_specs {
        let effective = submitted_non_secret
            .get(&spec.key)
            .cloned()
            .or_else(|| existing_non_secret.get(&spec.key).cloned())
            .unwrap_or_else(|| default_value_for_field_spec(spec));
        merged_non_secret.insert(spec.key.clone(), effective);
    }

    if let Err(msg) = validate_field_values(
        &non_secret_specs,
        FieldContext::OwnerSettings,
        &merged_non_secret,
        Some(&existing_non_secret),
    ) {
        cleanup_new_refs(&newly_created_refs);
        return Err(ApiError::bad_request(msg));
    }

    let media_refs: CollectedMediaIds =
        collect_media_ids(&submitted_media_specs, &submitted_non_secret);

    let mut tx = match db.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            cleanup_new_refs(&newly_created_refs);
            return Err(err.into());
        }
    };
    if let Err(err) = sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await
    {
        cleanup_new_refs(&newly_created_refs);
        return Err(err.into());
    }

    if let Err(err) =
        crate::storage::fields::verify_collected_media_ids_in_conn(&mut tx, &media_refs).await
    {
        cleanup_new_refs(&newly_created_refs);
        return Err(err);
    }

    let tx_ver: Option<i64> = match sqlx::query_scalar(
        "SELECT version FROM theme_option_versions WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind_db)
    .bind(owner_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(v) => v,
        Err(err) => {
            cleanup_new_refs(&newly_created_refs);
            return Err(err.into());
        }
    };

    match tx_ver {
        None => {
            if expected_version != 1 {
                cleanup_new_refs(&newly_created_refs);
                return Err(ApiError::precondition_failed(
                    "Cài đặt plugin đã thay đổi ở phiên khác. Tải lại trước khi lưu.",
                ));
            }
            if let Err(err) = sqlx::query(
                "INSERT INTO theme_option_versions (theme_id, owner_kind, owner_id, version)
                 VALUES (?, ?, ?, 2)",
            )
            .bind(owner_id)
            .bind(owner_kind_db)
            .bind(owner_id)
            .execute(&mut *tx)
            .await
            {
                cleanup_new_refs(&newly_created_refs);
                return Err(err.into());
            }
        }
        Some(ver) => {
            if ver != expected_version {
                cleanup_new_refs(&newly_created_refs);
                return Err(ApiError::precondition_failed(
                    "Cài đặt plugin đã thay đổi ở phiên khác. Tải lại trước khi lưu.",
                ));
            }
            match sqlx::query(
                "UPDATE theme_option_versions
                    SET version = version + 1,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                  WHERE owner_kind = ? AND owner_id = ? AND version = ?",
            )
            .bind(owner_kind_db)
            .bind(owner_id)
            .bind(expected_version)
            .execute(&mut *tx)
            .await
            {
                Ok(res) if res.rows_affected() == 1 => {}
                Ok(_) => {
                    cleanup_new_refs(&newly_created_refs);
                    return Err(ApiError::precondition_failed(
                        "Cài đặt plugin đã thay đổi ở phiên khác. Tải lại trước khi lưu.",
                    ));
                }
                Err(err) => {
                    cleanup_new_refs(&newly_created_refs);
                    return Err(err.into());
                }
            }
        }
    }

    for item in planned {
        match item {
            PlannedPluginWrite::Delete { key } => {
                if let Err(err) = sqlx::query(
                    "DELETE FROM theme_option_values
                      WHERE owner_kind = ? AND owner_id = ? AND option_key = ?",
                )
                .bind(owner_kind_db)
                .bind(owner_id)
                .bind(&key)
                .execute(&mut *tx)
                .await
                {
                    cleanup_new_refs(&newly_created_refs);
                    return Err(err.into());
                }
            }
            PlannedPluginWrite::Upsert {
                key,
                value,
                media_ids,
                sync_media_refs,
            } => {
                let v_json = match serde_json::to_string(&value) {
                    Ok(s) => s,
                    Err(err) => {
                        cleanup_new_refs(&newly_created_refs);
                        return Err(err.into());
                    }
                };
                if let Err(err) = sqlx::query(
                    "INSERT INTO theme_option_values (theme_id, owner_kind, owner_id, option_key, value_json, updated_at)
                     VALUES (?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     ON CONFLICT(owner_kind, owner_id, option_key) DO UPDATE SET
                         value_json = excluded.value_json,
                         updated_at = excluded.updated_at",
                )
                .bind(owner_id)
                .bind(owner_kind_db)
                .bind(owner_id)
                .bind(&key)
                .bind(v_json)
                .execute(&mut *tx)
                .await
                {
                    cleanup_new_refs(&newly_created_refs);
                    return Err(err.into());
                }
                if sync_media_refs
                    && let Err(err) = sync_theme_option_media_refs_in_conn(
                        &mut tx,
                        owner_id,
                        owner_kind_db,
                        owner_id,
                        "",
                        &key,
                        &media_ids,
                    )
                    .await
                {
                    cleanup_new_refs(&newly_created_refs);
                    return Err(err);
                }
            }
        }
    }

    let snapshot =
        match get_owner_settings_redacted_in_conn(&mut tx, owner_kind_db, owner_id, fields).await {
            Ok(s) => s,
            Err(err) => {
                cleanup_new_refs(&newly_created_refs);
                return Err(err);
            }
        };

    if let Err(err) = tx.commit().await {
        cleanup_new_refs(&newly_created_refs);
        return Err(err.into());
    }
    crate::cache::SiteCacheEpoch::global().bump();

    for old_ref in old_refs_to_remove {
        store.remove_plugin_secret(&old_ref);
    }

    Ok(PluginSettingsOutcome {
        snapshot,
        changed,
        has_projection_filters,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct PublicSiteSettings {
    pub public_origin: Option<String>,
    pub content_language: String,
    pub indexing_enabled: bool,
    pub version: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicSiteSettingsInput {
    pub public_origin: Option<String>,
    pub content_language: String,
    pub indexing_enabled: bool,
}

pub fn valid_content_language(value: &str) -> bool {
    opendash_core::id::valid_bcp47_tag(value)
}

pub fn normalize_public_origin(raw: Option<String>) -> Result<Option<String>, &'static str> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Ok(None);
    }
    if raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("Public origin không được chứa khoảng trắng hoặc ký tự điều khiển.");
    }
    if raw.contains('?') || raw.contains('#') {
        return Err("Public origin không được chứa query hoặc fragment.");
    }
    let parsed = url::Url::parse(&raw).map_err(|_| "Public origin phải là URL HTTP(S) hợp lệ.")?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Public origin chỉ hỗ trợ giao thức http hoặc https.");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Public origin không được chứa thông tin đăng nhập.");
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("Public origin không được chứa query hoặc fragment.");
    }
    if parsed.path() != "/" {
        return Err("Public origin không được chứa đường dẫn con.");
    }
    if parsed.host_str().is_none_or(str::is_empty) || !parsed.origin().is_tuple() {
        return Err("Public origin phải có tên miền hợp lệ.");
    }
    Ok(Some(parsed.origin().ascii_serialization()))
}

impl PublicSiteSettingsInput {
    pub fn validate(mut self) -> Result<Self, &'static str> {
        self.public_origin = normalize_public_origin(self.public_origin)?;
        if !valid_content_language(&self.content_language) {
            return Err(
                "Ngôn ngữ nội dung phải là mã BCP47 ASCII hợp lệ (tối đa 35 bytes, ví dụ vi hoặc en-US).",
            );
        }
        Ok(self)
    }
}

/// Pure URL projection from configured `public_origin` and site-relative path.
/// Never infers an origin from request headers.
pub fn absolute_public_url(settings: &PublicSiteSettings, path: &str) -> Option<String> {
    let origin = settings.public_origin.as_deref()?;
    if path.is_empty() || path == "/" {
        return Some(format!("{origin}/"));
    }
    if path.starts_with('/') {
        Some(format!("{origin}{path}"))
    } else {
        Some(format!("{origin}/{path}"))
    }
}

pub async fn get_public_in_conn(
    conn: &mut sqlx::SqliteConnection,
) -> Result<PublicSiteSettings, sqlx::Error> {
    sqlx::query_as::<_, PublicSiteSettings>(
        "SELECT public_origin, content_language, indexing_enabled, version
         FROM site_public_settings
         WHERE id = 1",
    )
    .fetch_one(&mut *conn)
    .await
}

pub async fn get_public(db: &SqlitePool) -> Result<PublicSiteSettings, sqlx::Error> {
    let mut conn = db.acquire().await?;
    get_public_in_conn(&mut conn).await
}

pub enum PublicSaveOutcome {
    Saved(PublicSiteSettings),
    Stale,
    ReservedLanguage(&'static str),
    MultiLocaleConflict,
    OrphanedLocaleConflict,
    SyncFailed(ApiError),
}

pub async fn save_public(
    db: &SqlitePool,
    input: PublicSiteSettingsInput,
    expected_version: i64,
) -> Result<PublicSaveOutcome, sqlx::Error> {
    let mut conn = db.begin_with("BEGIN IMMEDIATE").await?;
    let res: Result<PublicSaveOutcome, sqlx::Error> = async {
        let updated = sqlx::query(
            "UPDATE site_public_settings
             SET public_origin = ?, content_language = ?, indexing_enabled = ?, version = version + 1
             WHERE id = 1 AND version = ?",
        )
        .bind(input.public_origin.as_deref())
        .bind(&input.content_language)
        .bind(input.indexing_enabled)
        .bind(expected_version)
        .execute(&mut *conn)
        .await?;
        if updated.rows_affected() == 0 {
            return Ok(PublicSaveOutcome::Stale);
        }

        let locale_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM site_locales WHERE deleted_at IS NULL")
            .fetch_one(&mut *conn)
            .await?;
        let current_default: (String, String) =
            sqlx::query_as("SELECT code, bcp47_tag FROM site_locales WHERE is_default = 1 AND deleted_at IS NULL LIMIT 1")
                .fetch_one(&mut *conn)
                .await?;
        if current_default.1 != input.content_language {
            let multilingual_enabled: bool = sqlx::query_scalar("SELECT multilingual_enabled FROM site_locales_state WHERE id = 1").fetch_one(&mut *conn).await?;
            if locale_count <= 1 && !multilingual_enabled {
                let (new_code, new_tag) = match normalize_locale_code(&input.content_language) {
                    Ok(pair) => pair,
                    Err(msg) => return Ok(PublicSaveOutcome::ReservedLanguage(msg)),
                };
                let orphaned: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM site_locales WHERE code = ? AND deleted_at IS NOT NULL)",
                ).bind(&new_code).fetch_one(&mut *conn).await?;
                if orphaned {
                    return Ok(PublicSaveOutcome::OrphanedLocaleConflict);
                }
                sqlx::query(
                    "UPDATE site_locales_state
                        SET rename_in_progress = 1, version = version + 1
                      WHERE id = 1",
                )
                .execute(&mut *conn)
                .await?;
                sqlx::query(
                    "UPDATE site_locales
                        SET code = ?, bcp47_tag = ?
                      WHERE is_default = 1",
                )
                .bind(&new_code)
                .bind(&new_tag)
                .execute(&mut *conn)
                .await?;
                sqlx::query("UPDATE plugin_entry_projections SET locale = ? WHERE locale = ?")
                    .bind(&new_code)
                    .bind(&current_default.0)
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("UPDATE site_locales_state SET rename_in_progress = 0 WHERE id = 1")
                    .execute(&mut *conn)
                    .await?;
                if let Err(err) = crate::storage::permalinks::sync_claims_in_tx(&mut conn).await {
                    return Ok(PublicSaveOutcome::SyncFailed(err));
                }
            } else {
                return Ok(PublicSaveOutcome::MultiLocaleConflict);
            }
        }

        Ok(PublicSaveOutcome::Saved(PublicSiteSettings {
            public_origin: input.public_origin,
            content_language: input.content_language,
            indexing_enabled: input.indexing_enabled,
            version: expected_version + 1,
        }))
    }
    .await;
    match res {
        Ok(PublicSaveOutcome::Saved(saved)) => {
            conn.commit().await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(PublicSaveOutcome::Saved(saved))
        }
        Ok(other) => {
            let _ = conn.rollback().await;
            Ok(other)
        }
        Err(err) => {
            let _ = conn.rollback().await;
            Err(err)
        }
    }
}

#[cfg(test)]
mod plugin_settings_s02_tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    struct TempDirGuard(PathBuf);

    impl TempDirGuard {
        fn new(prefix: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), nanos));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn count_files_in_dir(dir: &std::path::Path) -> usize {
        if !dir.exists() {
            return 0;
        }
        fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .count()
    }

    fn sample_plugin_schema() -> Vec<FieldSpec> {
        use opendash_core::field_schema::SelectOption;

        let mut api_key = FieldSpec::new(FieldType::Secret, "api_key", "API Key");
        api_key.required = true;

        let optional_token = FieldSpec::new(FieldType::Secret, "optional_token", "Optional Token");

        let mut endpoint = FieldSpec::new(FieldType::Url, "endpoint", "Endpoint");
        endpoint.required = true;
        endpoint.default = Some(serde_json::json!("https://api.example.com"));

        let mut mode = FieldSpec::new(FieldType::Select, "mode", "Mode");
        mode.default = Some(serde_json::json!("safe"));
        mode.options = vec![
            SelectOption {
                value: "safe".into(),
                label: "Safe".into(),
                icon: None,
            },
            SelectOption {
                value: "fast".into(),
                label: "Fast".into(),
                icon: None,
            },
        ];

        let badge_logo = FieldSpec::new(FieldType::Media, "badge_logo", "Badge Logo");

        vec![api_key, optional_token, endpoint, mode, badge_logo]
    }

    #[tokio::test]
    async fn migration_0055_schema_delta_is_zero() {
        let temp = TempDirGuard::new("s02_m55_delta");
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        let migrations_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        let pre_dir = temp.0.join("pre_0055");
        let with_55_dir = temp.0.join("with_0055");
        fs::create_dir_all(&pre_dir).unwrap();
        fs::create_dir_all(&with_55_dir).unwrap();
        for entry in fs::read_dir(&migrations_dir)
            .unwrap()
            .filter_map(Result::ok)
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".sql") && name.as_str() < "0055_" {
                fs::copy(entry.path(), pre_dir.join(entry.file_name())).unwrap();
                fs::copy(entry.path(), with_55_dir.join(entry.file_name())).unwrap();
            } else if name.ends_with(".sql") && name.starts_with("0055_") {
                fs::copy(entry.path(), with_55_dir.join(entry.file_name())).unwrap();
            }
        }

        let pre_migrator = sqlx::migrate::Migrator::new(pre_dir.as_path())
            .await
            .unwrap();
        pre_migrator.run(&db).await.unwrap();

        let before_user_objects: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
                .fetch_one(&db)
                .await
                .unwrap();

        let with_55_migrator = sqlx::migrate::Migrator::new(with_55_dir.as_path())
            .await
            .unwrap();
        with_55_migrator.run(&db).await.unwrap();

        let after_user_objects: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
                .fetch_one(&db)
                .await
                .unwrap();

        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let total_objects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
            .fetch_one(&db)
            .await
            .unwrap();

        assert_eq!(after_user_objects - before_user_objects, 0);
        assert!(total_objects < 256, "sqlite_schema total={total_objects}");
    }

    #[tokio::test]
    async fn migration_0050_schema_delta_is_plus_two_and_under_object_limit() {
        let temp = TempDirGuard::new("s02_m50_delta");
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        let migrations_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        let pre_dir = temp.0.join("pre_0050");
        let with_50_dir = temp.0.join("with_0050");
        fs::create_dir_all(&pre_dir).unwrap();
        fs::create_dir_all(&with_50_dir).unwrap();
        for entry in fs::read_dir(&migrations_dir)
            .unwrap()
            .filter_map(Result::ok)
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".sql") && name.as_str() < "0050_" {
                fs::copy(entry.path(), pre_dir.join(entry.file_name())).unwrap();
                fs::copy(entry.path(), with_50_dir.join(entry.file_name())).unwrap();
            } else if name.ends_with(".sql") && name.starts_with("0050_") {
                fs::copy(entry.path(), with_50_dir.join(entry.file_name())).unwrap();
            }
        }

        let pre_migrator = sqlx::migrate::Migrator::new(pre_dir.as_path())
            .await
            .unwrap();
        pre_migrator.run(&db).await.unwrap();

        let before_user_objects: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
                .fetch_one(&db)
                .await
                .unwrap();

        let with_50_migrator = sqlx::migrate::Migrator::new(with_50_dir.as_path())
            .await
            .unwrap();
        with_50_migrator.run(&db).await.unwrap();

        let after_user_objects: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
                .fetch_one(&db)
                .await
                .unwrap();

        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let total_objects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
            .fetch_one(&db)
            .await
            .unwrap();

        assert_eq!(after_user_objects - before_user_objects, 2);
        assert!(total_objects < 256, "sqlite_schema total={total_objects}");
    }

    #[tokio::test]
    async fn migration_0053_schema_delta_is_zero() {
        let temp = TempDirGuard::new("s01_m53_delta");
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        let migrations_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        let pre_dir = temp.0.join("pre_0053");
        let with_53_dir = temp.0.join("with_0053");
        fs::create_dir_all(&pre_dir).unwrap();
        fs::create_dir_all(&with_53_dir).unwrap();
        for entry in fs::read_dir(&migrations_dir)
            .unwrap()
            .filter_map(Result::ok)
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".sql") && name.as_str() < "0053_" {
                fs::copy(entry.path(), pre_dir.join(entry.file_name())).unwrap();
                fs::copy(entry.path(), with_53_dir.join(entry.file_name())).unwrap();
            } else if name.ends_with(".sql") && name.starts_with("0053_") {
                fs::copy(entry.path(), with_53_dir.join(entry.file_name())).unwrap();
            }
        }

        let pre_migrator = sqlx::migrate::Migrator::new(pre_dir.as_path())
            .await
            .unwrap();
        pre_migrator.run(&db).await.unwrap();

        let before_user_objects: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
                .fetch_one(&db)
                .await
                .unwrap();

        let with_53_migrator = sqlx::migrate::Migrator::new(with_53_dir.as_path())
            .await
            .unwrap();
        with_53_migrator.run(&db).await.unwrap();

        let after_user_objects: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
                .fetch_one(&db)
                .await
                .unwrap();

        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let total_objects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
            .fetch_one(&db)
            .await
            .unwrap();

        assert_eq!(after_user_objects - before_user_objects, 0);
        assert!(total_objects < 256, "sqlite_schema total={total_objects}");
    }

    #[tokio::test]
    async fn theme_and_plugin_with_identical_ids_are_strictly_isolated() {
        let temp = TempDirGuard::new("s02_isolation");
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();

        let theme_media = "a".repeat(64);
        let plugin_media = "b".repeat(64);
        for (idx, mid) in [&theme_media, &plugin_media].into_iter().enumerate() {
            sqlx::query(
                "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state)
                 VALUES (?, 'image', ?, 'image/png', 'png', 100, 10, 10, 10, 10, 'active')",
            )
            .bind(mid)
            .bind(format!("img-{idx}.png"))
            .execute(&db)
            .await
            .unwrap();
        }

        let theme_manifest = serde_json::json!({
            "manifest_version": 3,
            "theme_id": "shared-id",
            "name": "Shared Theme",
            "version": "1.0.0",
            "templates": [{ "kind": "index", "file": "index.html" }],
            "options": [
                { "key": "hero_title", "label": "Hero", "type": "text", "default": "Init Theme", "public": true },
                { "key": "badge_logo", "label": "Logo", "type": "media", "public": true }
            ]
        })
        .to_string();
        sqlx::query(
            "INSERT INTO theme_packages (theme_id, display_name, source_format)
             VALUES ('shared-id', 'Shared Theme', 'v3')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO theme_revisions (revision_id, theme_id, version, revision_digest, source_format, manifest_json, file_digests_json, provenance_json)
             VALUES ('rev-shared', 'shared-id', '1.0.0', 'dig-shared', 'v3', ?, '{}', '{}')",
        )
        .bind(&theme_manifest)
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("UPDATE theme_active_selection SET revision_id = 'rev-shared' WHERE id = 1")
            .execute(&db)
            .await
            .unwrap();

        let plugin_schema = sample_plugin_schema();
        let plugin_manifest = serde_json::json!({
            "id": "shared-id",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Shared Plugin",
            "settings_schema": plugin_schema
        })
        .to_string();
        sqlx::query(
            "INSERT INTO plugins (id, version, name, description, manifest_json, wasm_blob, enabled)
             VALUES ('shared-id', '1.0.0', 'Shared Plugin', '', ?, X'0061736d01000000', 1)",
        )
        .bind(&plugin_manifest)
        .execute(&db)
        .await
        .unwrap();

        let mut tx = db.begin().await.unwrap();
        let theme_outcome = put_theme_options_in_conn_for_locale(
            &mut tx,
            "shared-id",
            1,
            BTreeMap::from([
                ("hero_title".into(), serde_json::json!("Theme Value")),
                ("badge_logo".into(), serde_json::json!(theme_media)),
            ]),
            None,
        )
        .await
        .unwrap();
        assert!(matches!(theme_outcome, ThemeOptionsOutcome::Saved(ref s) if s.version == 2));
        tx.commit().await.unwrap();

        let mut plugin_submit = serde_json::Map::new();
        plugin_submit.insert("api_key".into(), serde_json::json!("plugin_secret_value"));
        plugin_submit.insert(
            "endpoint".into(),
            serde_json::json!("https://plugin.example.com"),
        );
        plugin_submit.insert("badge_logo".into(), serde_json::json!(plugin_media));

        let plugin_outcome = put_plugin_settings(
            &db,
            temp.0.as_path(),
            "shared-id",
            1,
            &sample_plugin_schema(),
            &plugin_submit,
        )
        .await
        .unwrap();
        assert_eq!(plugin_outcome.snapshot.version, 2);

        let theme_snap = get_theme_options_for_locale(&db, "shared-id", None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(theme_snap.version, 2);
        assert_eq!(
            theme_snap.values.get("hero_title"),
            Some(&serde_json::json!("Theme Value"))
        );
        assert_eq!(
            theme_snap.values.get("badge_logo"),
            Some(&serde_json::json!(theme_media))
        );

        let plugin_snap = get_plugin_settings_redacted(&db, "shared-id", &sample_plugin_schema())
            .await
            .unwrap();
        assert_eq!(plugin_snap.version, 2);
        assert_eq!(
            plugin_snap.values.get("endpoint"),
            Some(&serde_json::json!("https://plugin.example.com"))
        );
        assert_eq!(
            plugin_snap.values.get("badge_logo"),
            Some(&serde_json::json!(plugin_media))
        );
        assert_eq!(
            plugin_snap.values.get("api_key"),
            Some(&serde_json::json!({ "configured": true }))
        );

        let mut pin_tx = db.begin().await.unwrap();
        let theme_pins = crate::storage::media_refs::collect_pins_in_tx(&mut pin_tx, &theme_media)
            .await
            .unwrap();
        let plugin_pins =
            crate::storage::media_refs::collect_pins_in_tx(&mut pin_tx, &plugin_media)
                .await
                .unwrap();
        pin_tx.commit().await.unwrap();
        assert!(
            theme_pins
                .pins
                .iter()
                .any(|p| p.source_kind == "theme_option")
        );
        assert!(
            !theme_pins
                .pins
                .iter()
                .any(|p| p.source_kind == "plugin_option")
        );
        assert!(
            plugin_pins
                .pins
                .iter()
                .any(|p| p.source_kind == "plugin_option")
        );
        assert!(
            !plugin_pins
                .pins
                .iter()
                .any(|p| p.source_kind == "theme_option")
        );

        assert!(
            crate::storage::public_content::media_is_referenced(&db, &theme_media)
                .await
                .unwrap()
        );
        assert!(
            !crate::storage::public_content::media_is_referenced(&db, &plugin_media)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn plugin_settings_storage_handles_redaction_cas_preserve_rotate_clear_and_required_secret()
     {
        let temp = TempDirGuard::new("s02_secrets");
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();

        let schema = sample_plugin_schema();
        let plugin_manifest = serde_json::json!({
            "id": "sec-plugin",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Secret Plugin",
            "settings_schema": schema
        })
        .to_string();
        sqlx::query(
            "INSERT INTO plugins (id, version, name, description, manifest_json, wasm_blob, enabled)
             VALUES ('sec-plugin', '1.0.0', 'Secret Plugin', '', ?, X'0061736d01000000', 1)",
        )
        .bind(&plugin_manifest)
        .execute(&db)
        .await
        .unwrap();

        let plugin_secret_dir = temp.0.join("plugins").join("sec-plugin");
        let store = crate::secrets::SecretStore::for_plugin_dir(plugin_secret_dir.clone());

        // 1. Initial GET is redacted and unconfigured.
        let init_snap = get_plugin_settings_redacted(&db, "sec-plugin", &schema)
            .await
            .unwrap();
        assert_eq!(init_snap.version, 1);
        assert_eq!(
            init_snap.values.get("api_key"),
            Some(&serde_json::json!({ "configured": false }))
        );
        assert_eq!(
            init_snap.values.get("optional_token"),
            Some(&serde_json::json!({ "configured": false }))
        );

        // 2. Stale CAS (412) leaves 0 secret blob files on disk.
        let mut stale_submit = serde_json::Map::new();
        stale_submit.insert("api_key".into(), serde_json::json!("should_not_persist"));
        stale_submit.insert(
            "endpoint".into(),
            serde_json::json!("https://v1.example.com"),
        );
        let stale_err = put_plugin_settings(
            &db,
            temp.0.as_path(),
            "sec-plugin",
            99,
            &schema,
            &stale_submit,
        )
        .await
        .unwrap_err();
        assert_eq!(stale_err.0, axum::http::StatusCode::PRECONDITION_FAILED);
        assert_eq!(count_files_in_dir(&plugin_secret_dir), 0);

        // 3. Initial valid save writes 2 secrets + non-secret URL, returns redacted snapshot v2.
        let mut save1 = serde_json::Map::new();
        save1.insert("api_key".into(), serde_json::json!("sk_live_first"));
        save1.insert("optional_token".into(), serde_json::json!("opt_first"));
        save1.insert(
            "endpoint".into(),
            serde_json::json!("https://v1.example.com"),
        );
        let res1 = put_plugin_settings(&db, temp.0.as_path(), "sec-plugin", 1, &schema, &save1)
            .await
            .unwrap();
        assert!(res1.changed);
        assert_eq!(res1.snapshot.version, 2);
        assert_eq!(count_files_in_dir(&plugin_secret_dir), 2);
        assert_eq!(
            res1.snapshot.values.get("api_key"),
            Some(&serde_json::json!({ "configured": true }))
        );
        assert!(
            !serde_json::to_string(&res1.snapshot)
                .unwrap()
                .contains("sk_live_first")
        );
        assert!(
            !serde_json::to_string(&res1.snapshot)
                .unwrap()
                .contains("secret_ref")
        );

        let raw1 = crate::storage::plugin::read_raw_plugin_option_map(&db, "sec-plugin")
            .await
            .unwrap();
        let ref1 = raw1["api_key"]["secret_ref"].as_str().unwrap().to_owned();
        assert_eq!(store.read_plugin_secret(&ref1).unwrap(), "sk_live_first");

        // 4. Updating endpoint with null secrets preserves existing secrets without churn.
        let mut save2 = serde_json::Map::new();
        save2.insert("api_key".into(), serde_json::Value::Null);
        save2.insert("optional_token".into(), serde_json::Value::Null);
        save2.insert(
            "endpoint".into(),
            serde_json::json!("https://v2.example.com"),
        );
        let res2 = put_plugin_settings(&db, temp.0.as_path(), "sec-plugin", 2, &schema, &save2)
            .await
            .unwrap();
        assert!(res2.changed);
        assert_eq!(res2.snapshot.version, 3);
        assert_eq!(count_files_in_dir(&plugin_secret_dir), 2);
        assert_eq!(store.read_plugin_secret(&ref1).unwrap(), "sk_live_first");

        // 5. Rotating api_key removes old secret blob on commit; clearing optional_token deletes both DB & blob.
        let mut save3 = serde_json::Map::new();
        save3.insert("api_key".into(), serde_json::json!("sk_live_rotated"));
        save3.insert(
            "optional_token".into(),
            serde_json::json!({ "clear": true }),
        );
        let res3 = put_plugin_settings(&db, temp.0.as_path(), "sec-plugin", 3, &schema, &save3)
            .await
            .unwrap();
        assert!(res3.changed);
        assert_eq!(res3.snapshot.version, 4);
        assert_eq!(count_files_in_dir(&plugin_secret_dir), 1);
        assert_eq!(
            res3.snapshot.values.get("api_key"),
            Some(&serde_json::json!({ "configured": true }))
        );
        assert_eq!(
            res3.snapshot.values.get("optional_token"),
            Some(&serde_json::json!({ "configured": false }))
        );
        assert!(store.read_plugin_secret(&ref1).is_err());

        let raw3 = crate::storage::plugin::read_raw_plugin_option_map(&db, "sec-plugin")
            .await
            .unwrap();
        assert!(!raw3.contains_key("optional_token"));
        let ref2 = raw3["api_key"]["secret_ref"].as_str().unwrap().to_owned();
        assert_eq!(store.read_plugin_secret(&ref2).unwrap(), "sk_live_rotated");

        // 6. Reject clearing a required secret without deleting existing secret.
        let mut bad_clear = serde_json::Map::new();
        bad_clear.insert("api_key".into(), serde_json::json!({ "clear": true }));
        let clear_err =
            put_plugin_settings(&db, temp.0.as_path(), "sec-plugin", 4, &schema, &bad_clear)
                .await
                .unwrap_err();
        assert_eq!(clear_err.0, axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(count_files_in_dir(&plugin_secret_dir), 1);
        assert_eq!(store.read_plugin_secret(&ref2).unwrap(), "sk_live_rotated");
    }
}
