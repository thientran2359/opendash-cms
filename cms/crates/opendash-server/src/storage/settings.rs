//! SITE-01 typed General Settings. SQL remains in storage.
use std::sync::LazyLock;

use std::collections::{BTreeMap, BTreeSet};

use opendash_core::id::valid_snake_key_with_limit;
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use crate::{
    error::{ApiError, ApiResult},
    storage::media::valid_media_id,
};

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
    if crate::seo::is_reserved_namespace_key(&code) {
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
    let code: Option<String> =
        sqlx::query_scalar("SELECT code FROM site_locales WHERE is_default = 1 LIMIT 1")
            .fetch_optional(&mut *conn)
            .await?;
    code.ok_or_else(|| ApiError::internal("Thiếu ngôn ngữ mặc định trong site_locales."))
}

pub(crate) async fn get_default_locale_code(db: &SqlitePool) -> ApiResult<String> {
    let mut conn = db.acquire().await?;
    get_default_locale_code_in_conn(&mut conn).await
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
    let found: Option<String> = sqlx::query_scalar("SELECT code FROM site_locales WHERE code = ?")
        .bind(&normalized)
        .fetch_optional(&mut *conn)
        .await?;
    found.ok_or_else(|| ApiError::bad_request("Ngôn ngữ được yêu cầu không tồn tại."))
}

pub(crate) async fn list_locales_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<SiteLocalesSnapshot> {
    let version: i64 = sqlx::query_scalar("SELECT version FROM site_locales_state WHERE id = 1")
        .fetch_one(&mut *conn)
        .await?;
    let rows: Vec<(String, String, String, String, i64, i64, i64)> = sqlx::query_as(
        "SELECT code, bcp47_tag, label, direction, is_default, enabled, position
           FROM site_locales
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

pub async fn create_locale(
    db: &SqlitePool,
    expected_version: i64,
    input: CreateLocaleInput,
) -> Result<SiteLocalesSnapshot, LocaleMutationError> {
    let (code, bcp47_tag) = normalize_locale_code(&input.code).map_err(ApiError::unprocessable)?;
    let label = validate_locale_label(&input.label).map_err(ApiError::unprocessable)?;
    let direction =
        validate_locale_direction(input.direction.as_deref()).map_err(ApiError::unprocessable)?;

    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res: Result<SiteLocalesSnapshot, LocaleMutationError> = async {
        let current_ver: i64 =
            sqlx::query_scalar("SELECT version FROM site_locales_state WHERE id = 1")
                .fetch_one(&mut *conn)
                .await?;
        if current_ver != expected_version {
            return Err(ApiError::precondition_failed(
                "Danh sách ngôn ngữ đã thay đổi. Tải lại trước khi lưu.",
            )
            .into());
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM site_locales")
            .fetch_one(&mut *conn)
            .await?;
        if count as usize >= MAX_SITE_LOCALES {
            return Err(ApiError::unprocessable(
                "Đã đạt giới hạn tối đa 20 ngôn ngữ trên hệ thống.",
            )
            .into());
        }
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM site_locales WHERE code = ?")
                .bind(&code)
                .fetch_optional(&mut *conn)
                .await?;
        if exists.is_some() {
            return Err(ApiError::conflict("Mã ngôn ngữ đã tồn tại.").into());
        }
        let next_pos: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(position), -1) + 1 FROM site_locales")
                .fetch_one(&mut *conn)
                .await?;
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
        sqlx::query("UPDATE site_locales_state SET version = version + 1 WHERE id = 1")
            .execute(&mut *conn)
            .await?;
        crate::storage::permalinks::sync_claims_with_options(&mut conn, true).await?;
        Ok(list_locales_in_conn(&mut conn).await?)
    }
    .await;
    match res {
        Ok(snap) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(snap)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
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

    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res: Result<SiteLocalesSnapshot, LocaleMutationError> = async {
        let current_ver: i64 =
            sqlx::query_scalar("SELECT version FROM site_locales_state WHERE id = 1")
                .fetch_one(&mut *conn)
                .await?;
        if current_ver != expected_version {
            return Err(ApiError::precondition_failed(
                "Danh sách ngôn ngữ đã thay đổi. Tải lại trước khi lưu.",
            )
            .into());
        }
        let existing: Option<(i64, i64)> =
            sqlx::query_as("SELECT is_default, enabled FROM site_locales WHERE code = ?")
                .bind(&code)
                .fetch_optional(&mut *conn)
                .await?;
        let Some((is_default, was_enabled)) = existing else {
            return Err(ApiError::not_found("Không tìm thấy ngôn ngữ.").into());
        };
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
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(snap)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
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
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res: Result<SiteLocalesSnapshot, LocaleMutationError> = async {
        let current_ver: i64 =
            sqlx::query_scalar("SELECT version FROM site_locales_state WHERE id = 1")
                .fetch_one(&mut *conn)
                .await?;
        if current_ver != expected_version {
            return Err(ApiError::precondition_failed(
                "Danh sách ngôn ngữ đã thay đổi. Tải lại trước khi lưu.",
            )
            .into());
        }
        let target: Option<(String, i64, i64)> = sqlx::query_as(
            "SELECT bcp47_tag, is_default, enabled FROM site_locales WHERE code = ?",
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
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(snap)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
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
          WHERE e.id = ? AND t.provider_status = 'active'",
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

pub async fn get_reading_in_conn(conn: &mut SqliteConnection) -> ApiResult<ReadingSettings> {
    get_reading_in_conn_for_locale(conn, None).await
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
    pub version: i64,
    pub has_override: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralSettingsInput {
    pub site_title: String,
    pub tagline: String,
    pub timezone: String,
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

pub(super) async fn get_general_in_conn_for_locale(
    conn: &mut SqliteConnection,
    requested_locale: Option<&str>,
) -> ApiResult<GeneralSettings> {
    let default_locale = get_default_locale_code_in_conn(conn).await?;
    let target_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;
    let timezone: String =
        sqlx::query_scalar("SELECT timezone FROM site_general_settings WHERE id = 1")
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
    InvalidLocale(String),
}

/// Runs inside `BEGIN IMMEDIATE` so reads and version/timezone validation share
/// a single SQLite write lock. Secondary locales cannot alter global timezone.
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
        let global_tz: String =
            sqlx::query_scalar("SELECT timezone FROM site_general_settings WHERE id = 1")
                .fetch_one(&mut *conn)
                .await?;
        let is_default_locale = target_locale == default_locale;
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
                    SET site_title = ?, tagline = ?, timezone = ?, version = ?
                  WHERE id = 1",
            )
            .bind(&input.site_title)
            .bind(&input.tagline)
            .bind(&input.timezone)
            .bind(new_version)
            .execute(&mut *conn)
            .await?;
        }

        Ok(SaveOutcome::Saved(GeneralSettings {
            site_title: input.site_title,
            tagline: input.tagline,
            timezone: input.timezone,
            version: new_version,
            has_override: true,
            locale: requested_locale.map(|_| target_locale),
        }))
    }
    .await;
    match res {
        Ok(outcome) if matches!(outcome, SaveOutcome::Saved(_)) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
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

pub const MAX_THEME_OPTIONS: usize = 64;
pub const MAX_THEME_OPTION_STRING_CHARS: usize = 4096;
pub const MAX_THEME_OPTION_CHOICES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeOptionType {
    String,
    Url,
    Number,
    Boolean,
    Select,
    ImageRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeOptionChoice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeOptionDef {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub option_type: ThemeOptionType,
    pub default: serde_json::Value,
    #[serde(default)]
    pub public: bool,
    #[serde(default)]
    pub translatable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<ThemeOptionChoice>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeOptionsSnapshot {
    pub theme_id: String,
    pub version: i64,
    pub schema: Vec<ThemeOptionDef>,
    pub values: BTreeMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
}

pub enum ThemeOptionsOutcome {
    Saved(ThemeOptionsSnapshot),
    NotFound,
    Stale,
}

fn valid_plain_string(s: &str, max_chars: usize) -> bool {
    s.chars().count() <= max_chars
        && !s
            .chars()
            .any(|c| c == '\0' || (c.is_control() && c != '\n' && c != '\t'))
}

pub fn validate_option_value(
    def: &ThemeOptionDef,
    value: &serde_json::Value,
) -> Result<(), &'static str> {
    match def.option_type {
        ThemeOptionType::String => {
            let s = value
                .as_str()
                .ok_or("Giá trị option kiểu string phải là chuỗi.")?;
            if !valid_plain_string(s, MAX_THEME_OPTION_STRING_CHARS) {
                return Err("Option string vượt quá 4,096 ký tự hoặc chứa ký tự điều khiển.");
            }
            Ok(())
        }
        ThemeOptionType::Url => {
            let s = value
                .as_str()
                .ok_or("Giá trị option kiểu url phải là chuỗi.")?;
            crate::template_sec::validate_theme_option_url(s)
        }
        ThemeOptionType::Number => {
            let n = value
                .as_f64()
                .ok_or("Giá trị option kiểu number phải là số hữu hạn.")?;
            if !n.is_finite() {
                return Err("Option number phải là số hữu hạn.");
            }
            if let Some(min) = def.min
                && n < min
            {
                return Err("Option number nhỏ hơn giá trị min.");
            }
            if let Some(max) = def.max
                && n > max
            {
                return Err("Option number lớn hơn giá trị max.");
            }
            Ok(())
        }
        ThemeOptionType::Boolean => {
            if !value.is_boolean() {
                return Err("Option kiểu boolean phải là true hoặc false.");
            }
            Ok(())
        }
        ThemeOptionType::Select => {
            let s = value
                .as_str()
                .ok_or("Option kiểu select phải là chuỗi lựa chọn.")?;
            if !def.choices.iter().any(|c| c.value == s) {
                return Err("Giá trị select không nằm trong danh sách choices.");
            }
            Ok(())
        }
        ThemeOptionType::ImageRef => match value {
            serde_json::Value::Null => Ok(()),
            serde_json::Value::String(id) if valid_media_id(id) => Ok(()),
            _ => Err("Option image_ref phải là media_id hợp lệ hoặc null."),
        },
    }
}

pub fn parse_theme_options_schema(
    manifest_json: &str,
) -> Result<Vec<ThemeOptionDef>, &'static str> {
    let manifest: serde_json::Value =
        serde_json::from_str(manifest_json).map_err(|_| "theme.json không hợp lệ.")?;
    let Some(raw_opts) = manifest.get("options") else {
        return Ok(Vec::new());
    };
    if raw_opts.is_null() {
        return Ok(Vec::new());
    }
    let defs: Vec<ThemeOptionDef> = serde_json::from_value(raw_opts.clone())
        .map_err(|_| "Khai báo options trong theme.json không đúng cấu trúc.")?;
    if defs.len() > MAX_THEME_OPTIONS {
        return Err("Theme vượt quá giới hạn 64 options.");
    }
    let mut seen = BTreeSet::new();
    for def in &defs {
        if !valid_snake_key_with_limit(&def.key, 64) {
            return Err("option_key phải là snake_case (1–64 ký tự).");
        }
        if !seen.insert(def.key.clone()) {
            return Err("Trùng lặp option_key trong theme.json.");
        }
        if def.label.trim().is_empty() || !valid_plain_string(&def.label, 128) {
            return Err("Nhãn option không hợp lệ.");
        }
        if let Some(min) = def.min
            && !min.is_finite()
        {
            return Err("Giá trị min của option không hợp lệ.");
        }
        if let Some(max) = def.max
            && !max.is_finite()
        {
            return Err("Giá trị max của option không hợp lệ.");
        }
        if let (Some(min), Some(max)) = (def.min, def.max)
            && min > max
        {
            return Err("min không được lớn hơn max.");
        }
        if def.option_type == ThemeOptionType::Select {
            if def.choices.is_empty() || def.choices.len() > MAX_THEME_OPTION_CHOICES {
                return Err("Option select phải có 1–64 choices.");
            }
            let mut choice_vals = BTreeSet::new();
            for c in &def.choices {
                if c.value.trim().is_empty()
                    || !valid_plain_string(&c.value, 128)
                    || c.label.trim().is_empty()
                    || !valid_plain_string(&c.label, 128)
                    || !choice_vals.insert(c.value.as_str())
                {
                    return Err("Choice trong option select không hợp lệ hoặc bị trùng.");
                }
            }
        }
        if def.translatable
            && !matches!(
                def.option_type,
                ThemeOptionType::String | ThemeOptionType::Url
            )
        {
            return Err("Chỉ option kiểu string hoặc url mới được khai báo translatable: true.");
        }
        validate_option_value(def, &def.default)?;
    }
    Ok(defs)
}

/// Derive anchor-only permissions from the selected revision's validated
/// manifest; request values and mutable context cannot add capabilities.
pub fn theme_url_policy(
    manifest_json: &str,
) -> Result<crate::template_sec::ThemeUrlPolicy, &'static str> {
    let schema = parse_theme_options_schema(manifest_json)?;
    Ok(crate::template_sec::ThemeUrlPolicy::from_option_keys(
        schema
            .iter()
            .filter(|def| def.public && def.option_type == ThemeOptionType::Url)
            .map(|def| def.key.clone()),
    ))
}

async fn latest_revision_manifest_in_conn(
    conn: &mut SqliteConnection,
    theme_id: &str,
) -> ApiResult<Option<String>> {
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
    let schema = parse_theme_options_schema(&manifest_json).map_err(ApiError::bad_request)?;
    let global_rows: Vec<(String, String)> =
        sqlx::query_as("SELECT option_key, value_json FROM theme_option_values WHERE theme_id = ?")
            .bind(theme_id)
            .fetch_all(&mut *conn)
            .await?;
    let mut global_map = BTreeMap::new();
    for (k, v_json) in global_rows {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
            global_map.insert(k, v);
        }
    }

    let (version, locale_map) = if target_locale == default_locale {
        let ver: i64 =
            sqlx::query_scalar("SELECT version FROM theme_option_versions WHERE theme_id = ?")
                .bind(theme_id)
                .fetch_optional(&mut *conn)
                .await?
                .unwrap_or(1);
        (ver, BTreeMap::new())
    } else {
        let ver: i64 = sqlx::query_scalar(
            "SELECT version FROM theme_option_locale_versions WHERE theme_id = ? AND locale = ?",
        )
        .bind(theme_id)
        .bind(&target_locale)
        .fetch_optional(&mut *conn)
        .await?
        .unwrap_or(1);
        let loc_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT option_key, value_json
               FROM theme_option_locale_values
              WHERE theme_id = ? AND locale = ?",
        )
        .bind(theme_id)
        .bind(&target_locale)
        .fetch_all(&mut *conn)
        .await?;
        let mut map = BTreeMap::new();
        for (k, v_json) in loc_rows {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
                map.insert(k, v);
            }
        }
        (ver, map)
    };

    let mut values = BTreeMap::new();
    for def in &schema {
        let v = if target_locale != default_locale && def.translatable {
            locale_map
                .get(&def.key)
                .or_else(|| global_map.get(&def.key))
                .cloned()
                .unwrap_or_else(|| def.default.clone())
        } else {
            global_map
                .get(&def.key)
                .cloned()
                .unwrap_or_else(|| def.default.clone())
        };
        values.insert(def.key.clone(), v);
    }

    Ok(Some(ThemeOptionsSnapshot {
        theme_id: theme_id.to_owned(),
        version,
        schema,
        values,
        locale: requested_locale.map(|_| target_locale),
    }))
}

pub async fn get_theme_options_in_conn(
    conn: &mut SqliteConnection,
    theme_id: &str,
) -> ApiResult<Option<ThemeOptionsSnapshot>> {
    get_theme_options_in_conn_for_locale(conn, theme_id, None).await
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

pub async fn get_theme_options(
    db: &SqlitePool,
    theme_id: &str,
) -> ApiResult<Option<ThemeOptionsSnapshot>> {
    get_theme_options_for_locale(db, theme_id, None).await
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
    let schema = parse_theme_options_schema(&manifest_json).map_err(ApiError::bad_request)?;
    let schema_map: BTreeMap<&str, &ThemeOptionDef> =
        schema.iter().map(|d| (d.key.as_str(), d)).collect();

    let is_secondary = target_locale != default_locale;
    for (k, v) in &submitted {
        let Some(def) = schema_map.get(k.as_str()) else {
            return Err(ApiError::bad_request(
                "Option key không tồn tại trong schema của theme.",
            ));
        };
        if is_secondary && !def.translatable {
            return Err(ApiError::bad_request(
                "Không thể lưu option không hỗ trợ dịch ở ngôn ngữ phụ.",
            ));
        }
        validate_option_value(def, v).map_err(ApiError::bad_request)?;
        if def.option_type == ThemeOptionType::ImageRef
            && let Some(media_id) = v.as_str()
        {
            let exists: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM media_assets WHERE id = ? AND kind = 'image' AND lifecycle_state = 'active'")
                    .bind(media_id)
                    .fetch_optional(&mut *conn)
                    .await?;
            if exists.is_none() {
                return Err(ApiError::bad_request(
                    "media_id của image_ref không tồn tại.",
                ));
            }
        }
    }

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
        }
    } else {
        let current_ver: Option<i64> =
            sqlx::query_scalar("SELECT version FROM theme_option_versions WHERE theme_id = ?")
                .bind(theme_id)
                .fetch_optional(&mut *conn)
                .await?;

        match current_ver {
            None => {
                if expected_version != 1 {
                    return Ok(ThemeOptionsOutcome::Stale);
                }
                sqlx::query("INSERT INTO theme_option_versions (theme_id, version) VALUES (?, 2)")
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
                     WHERE theme_id = ? AND version = ?",
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
                "INSERT INTO theme_option_values (theme_id, option_key, value_json, updated_at)
                 VALUES (?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(theme_id, option_key) DO UPDATE SET
                     value_json = excluded.value_json,
                     updated_at = excluded.updated_at",
            )
            .bind(theme_id)
            .bind(k)
            .bind(v_json)
            .execute(&mut *conn)
            .await?;

            if let Some(def) = schema_map.get(k.as_str())
                && def.option_type == ThemeOptionType::ImageRef
            {
                if let Some(media_id) = v.as_str() {
                    sqlx::query(
                        "INSERT INTO theme_option_media_refs (theme_id, option_key, media_id)
                         VALUES (?, ?, ?)
                         ON CONFLICT(theme_id, option_key) DO UPDATE SET
                             media_id = excluded.media_id",
                    )
                    .bind(theme_id)
                    .bind(k)
                    .bind(media_id)
                    .execute(&mut *conn)
                    .await?;
                } else {
                    sqlx::query(
                        "DELETE FROM theme_option_media_refs WHERE theme_id = ? AND option_key = ?",
                    )
                    .bind(theme_id)
                    .bind(k)
                    .execute(&mut *conn)
                    .await?;
                }
            }
        }
    }

    let snapshot = get_theme_options_in_conn_for_locale(conn, theme_id, requested_locale)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được theme options sau khi lưu."))?;
    Ok(ThemeOptionsOutcome::Saved(snapshot))
}

pub async fn put_theme_options_in_conn(
    conn: &mut SqliteConnection,
    theme_id: &str,
    expected_version: i64,
    submitted: BTreeMap<String, serde_json::Value>,
) -> ApiResult<ThemeOptionsOutcome> {
    put_theme_options_in_conn_for_locale(conn, theme_id, expected_version, submitted, None).await
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

pub async fn put_theme_options(
    db: &SqlitePool,
    theme_id: &str,
    expected_version: i64,
    submitted: BTreeMap<String, serde_json::Value>,
) -> ApiResult<ThemeOptionsOutcome> {
    put_theme_options_for_locale(db, theme_id, expected_version, submitted, None).await
}

pub async fn check_theme_options_compatibility(
    db: &SqlitePool,
    theme_id: &str,
    new_manifest_json: &str,
) -> ApiResult<()> {
    let new_schema =
        parse_theme_options_schema(new_manifest_json).map_err(ApiError::bad_request)?;
    let new_map: BTreeMap<&str, &ThemeOptionDef> =
        new_schema.iter().map(|d| (d.key.as_str(), d)).collect();

    let saved_rows: Vec<(String, String)> =
        sqlx::query_as("SELECT option_key, value_json FROM theme_option_values WHERE theme_id = ?")
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
        let Some(new_def) = new_map.get(key.as_str()) else {
            // Option removed in new revision: retained as dormant data.
            continue;
        };
        let Ok(saved_val) = serde_json::from_str::<serde_json::Value>(&value_json) else {
            return Err(ApiError::conflict(
                "Dữ liệu theme option đã lưu không tương thích với revision mới.",
            ));
        };
        if validate_option_value(new_def, &saved_val).is_err() {
            return Err(ApiError::conflict(
                "Schema option của revision mới không tương thích với giá trị đã lưu (409).",
            ));
        }
    }
    Ok(())
}

/// Returns only `public: true` options for the currently active v3 theme revision.
pub async fn get_active_public_theme_options_for_locale(
    db: &SqlitePool,
    requested_locale: Option<&str>,
) -> ApiResult<BTreeMap<String, serde_json::Value>> {
    let active: Option<(String, String)> = sqlx::query_as(
        "SELECT r.theme_id, r.manifest_json
         FROM theme_active_selection AS a
         JOIN theme_revisions AS r ON r.revision_id = a.revision_id
         WHERE a.id = 1 AND r.source_format = 'v3'",
    )
    .fetch_optional(db)
    .await?;
    let Some((theme_id, manifest_json)) = active else {
        return Ok(BTreeMap::new());
    };
    let mut conn = db.acquire().await?;
    get_public_theme_options_in_conn_for_locale(
        &mut conn,
        &theme_id,
        &manifest_json,
        requested_locale,
    )
    .await
}

pub async fn get_active_public_theme_options(
    db: &SqlitePool,
) -> ApiResult<BTreeMap<String, serde_json::Value>> {
    get_active_public_theme_options_for_locale(db, None).await
}

/// Resolve only public values against the revision already captured by the caller.
pub(super) async fn get_public_theme_options_in_conn_for_locale(
    conn: &mut SqliteConnection,
    theme_id: &str,
    manifest_json: &str,
    requested_locale: Option<&str>,
) -> ApiResult<BTreeMap<String, serde_json::Value>> {
    let Ok(schema) = parse_theme_options_schema(manifest_json) else {
        return Ok(BTreeMap::new());
    };
    let default_locale = get_default_locale_code_in_conn(conn).await?;
    let target_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;
    let saved_rows: Vec<(String, String)> =
        sqlx::query_as("SELECT option_key, value_json FROM theme_option_values WHERE theme_id = ?")
            .bind(theme_id)
            .fetch_all(&mut *conn)
            .await?;
    let mut saved_map = BTreeMap::new();
    for (k, v_json) in saved_rows {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
            saved_map.insert(k, v);
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
        .bind(&target_locale)
        .fetch_all(&mut *conn)
        .await?;
        for (k, v_json) in loc_rows {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
                locale_map.insert(k, v);
            }
        }
    }
    let mut out = BTreeMap::new();
    for def in schema {
        if !def.public {
            continue;
        }
        let effective = if target_locale != default_locale && def.translatable {
            locale_map.get(&def.key).or_else(|| saved_map.get(&def.key))
        } else {
            saved_map.get(&def.key)
        };
        let val = match def.option_type {
            ThemeOptionType::ImageRef => {
                // Unsaved default image_ref never auto-publicizes media.
                match saved_map.get(&def.key).and_then(|v| v.as_str()) {
                    Some(media_id) if valid_media_id(media_id) => {
                        serde_json::Value::String(format!("/media/{media_id}/file"))
                    }
                    _ => serde_json::Value::Null,
                }
            }
            ThemeOptionType::Url => match effective {
                Some(saved) if validate_option_value(&def, saved).is_ok() => saved.clone(),
                _ => def.default.clone(),
            },
            _ => effective.cloned().unwrap_or(def.default),
        };
        out.insert(def.key, val);
    }
    Ok(out)
}

pub(super) async fn get_public_theme_options_in_conn(
    conn: &mut SqliteConnection,
    theme_id: &str,
    manifest_json: &str,
) -> ApiResult<BTreeMap<String, serde_json::Value>> {
    get_public_theme_options_in_conn_for_locale(conn, theme_id, manifest_json, None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_url_policy_requires_public_url_schema_and_valid_defaults() {
        let schema = serde_json::json!({"options": [
            {"key":"cta_url","label":"CTA","type":"url","default":"/lien-he","public":true},
            {"key":"private_url","label":"Private","type":"url","default":"#secret","public":false},
            {"key":"legacy_string","label":"Legacy","type":"string","default":"/old","public":true}
        ]});
        let raw = schema.to_string();
        let policy = theme_url_policy(&raw).unwrap();
        let src = r#"<a href="{{ options.cta_url }}">Link</a>"#;
        assert!(
            crate::template_sec::validate_template_csec_with_theme_urls("x", src, &policy).is_ok()
        );
        for key in ["private_url", "legacy_string", "unknown_url"] {
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
        assert!(
            validate_option_value(
                &parse_theme_options_schema(&raw).unwrap()[0],
                &serde_json::json!("/changed?from=header#form"),
            )
            .is_ok()
        );
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
        };
        assert!(make(" Xin chào ".into(), "").validate().is_ok());
        assert!(make("🦀".repeat(120), "").validate().is_ok());
        assert!(make("🦀".repeat(121), "").validate().is_err());
        assert!(make(" \t ".into(), "").validate().is_err());
        assert!(make("\tAccepted?".into(), "").validate().is_err());
        assert!(make("ok".into(), "bad\nvalue").validate().is_err());
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
                { "key": "hero_title", "label": "Hero", "type": "string", "default": "Mac dinh", "public": true, "translatable": true },
                { "key": "cta_url", "label": "CTA", "type": "url", "default": "/vi", "public": true, "translatable": true },
                { "key": "accent", "label": "Accent", "type": "string", "default": "#111", "public": true, "translatable": false }
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
                ("accent".into(), serde_json::json!("#222")),
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
        let vi_pub =
            get_public_theme_options_in_conn_for_locale(&mut tx, "t-l10n", &manifest, None)
                .await
                .unwrap();
        let en_pub =
            get_public_theme_options_in_conn_for_locale(&mut tx, "t-l10n", &manifest, Some("en"))
                .await
                .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(vi_pub["hero_title"], serde_json::json!("Trang chu"));
        assert_eq!(vi_pub["cta_url"], serde_json::json!("/vi"));
        assert_eq!(vi_pub["accent"], serde_json::json!("#222"));

        assert_eq!(en_pub["hero_title"], serde_json::json!("Trang chu"));
        assert_eq!(en_pub["cta_url"], serde_json::json!("/en/contact"));
        assert_eq!(en_pub["accent"], serde_json::json!("#222"));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsOwnerKind {
    Theme,
    Plugin,
}

impl SettingsOwnerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Plugin => "plugin",
        }
    }
}

pub async fn get_owner_settings_in_conn(
    conn: &mut SqliteConnection,
    owner_kind: SettingsOwnerKind,
    owner_id: &str,
    schema: &[ThemeOptionDef],
) -> ApiResult<ThemeOptionsSnapshot> {
    let version: i64 = sqlx::query_scalar(
        "SELECT version FROM theme_option_versions WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind.as_str())
    .bind(owner_id)
    .fetch_optional(&mut *conn)
    .await?
    .unwrap_or(1);

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT option_key, value_json FROM theme_option_values WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind.as_str())
    .bind(owner_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut saved_map = BTreeMap::new();
    for (k, v_json) in rows {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
            saved_map.insert(k, v);
        }
    }

    let mut values = BTreeMap::new();
    for def in schema {
        let v = saved_map
            .get(&def.key)
            .cloned()
            .unwrap_or_else(|| def.default.clone());
        values.insert(def.key.clone(), v);
    }

    Ok(ThemeOptionsSnapshot {
        theme_id: owner_id.to_owned(),
        version,
        schema: schema.to_vec(),
        values,
        locale: None,
    })
}

pub async fn put_owner_settings_in_conn(
    conn: &mut SqliteConnection,
    owner_kind: SettingsOwnerKind,
    owner_id: &str,
    expected_version: i64,
    schema: &[ThemeOptionDef],
    submitted: BTreeMap<String, serde_json::Value>,
) -> ApiResult<ThemeOptionsOutcome> {
    let schema_map: BTreeMap<&str, &ThemeOptionDef> =
        schema.iter().map(|d| (d.key.as_str(), d)).collect();

    for (k, v) in &submitted {
        let Some(def) = schema_map.get(k.as_str()) else {
            return Err(ApiError::bad_request(
                "Option key không tồn tại trong schema.",
            ));
        };
        validate_option_value(def, v).map_err(ApiError::bad_request)?;
    }

    let current_ver: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM theme_option_versions WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind.as_str())
    .bind(owner_id)
    .fetch_optional(&mut *conn)
    .await?;

    match current_ver {
        None => {
            if expected_version != 1 {
                return Ok(ThemeOptionsOutcome::Stale);
            }
            sqlx::query(
                "INSERT INTO theme_option_versions (theme_id, owner_kind, owner_id, version) VALUES (?, ?, ?, 2)",
            )
            .bind(owner_id)
            .bind(owner_kind.as_str())
            .bind(owner_id)
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
                 WHERE owner_kind = ? AND owner_id = ? AND version = ?",
            )
            .bind(owner_kind.as_str())
            .bind(owner_id)
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
             VALUES (?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(theme_id, option_key) DO UPDATE SET
                 owner_kind = excluded.owner_kind,
                 owner_id = excluded.owner_id,
                 value_json = excluded.value_json,
                 updated_at = excluded.updated_at",
        )
        .bind(owner_id)
        .bind(owner_kind.as_str())
        .bind(owner_id)
        .bind(k)
        .bind(v_json)
        .execute(&mut *conn)
        .await?;
    }

    let snapshot = get_owner_settings_in_conn(conn, owner_kind, owner_id, schema).await?;
    Ok(ThemeOptionsOutcome::Saved(snapshot))
}

pub const MAX_CONTENT_LANGUAGE_BYTES: usize = opendash_core::id::MAX_BCP47_TAG_BYTES;

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
    SyncFailed(ApiError),
}

pub async fn save_public(
    db: &SqlitePool,
    input: PublicSiteSettingsInput,
    expected_version: i64,
) -> Result<PublicSaveOutcome, sqlx::Error> {
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
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

        let locale_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM site_locales")
            .fetch_one(&mut *conn)
            .await?;
        let current_default: (String, String) =
            sqlx::query_as("SELECT code, bcp47_tag FROM site_locales WHERE is_default = 1 LIMIT 1")
                .fetch_one(&mut *conn)
                .await?;
        if current_default.1 != input.content_language {
            if locale_count <= 1 {
                let (new_code, new_tag) = match normalize_locale_code(&input.content_language) {
                    Ok(pair) => pair,
                    Err(msg) => return Ok(PublicSaveOutcome::ReservedLanguage(msg)),
                };
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
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(PublicSaveOutcome::Saved(saved))
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
