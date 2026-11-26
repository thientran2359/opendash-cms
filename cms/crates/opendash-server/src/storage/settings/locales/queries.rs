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

pub(in crate::storage::settings) fn validate_locale_label(
    raw: &str,
) -> Result<String, &'static str> {
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

pub(in crate::storage::settings) fn validate_locale_direction(
    raw: Option<&str>,
) -> Result<String, &'static str> {
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

pub(in crate::storage::settings) async fn require_locale_version_in_conn(
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
