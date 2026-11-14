//! SITE-01 typed General Settings. SQL remains in storage.
use std::sync::LazyLock;

use std::collections::{BTreeMap, BTreeSet};

use opendash_core::id::valid_snake_key_with_limit;
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use crate::{error::{ApiError, ApiResult}, storage::media::valid_media_id};

pub const TZDB_VERSION: &str = "2026f";
pub const MAX_SITE_TITLE_SCALARS: usize = 120;
pub const MAX_TAGLINE_SCALARS: usize = 240;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct GeneralSettings {
    pub site_title: String,
    pub tagline: String,
    pub timezone: String,
    pub version: i64,
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

pub async fn get_general(db: &SqlitePool) -> Result<GeneralSettings, sqlx::Error> {
    sqlx::query_as::<_, GeneralSettings>(
        "SELECT site_title, tagline, timezone, version FROM site_general_settings WHERE id = 1",
    )
    .fetch_one(db)
    .await
}

pub enum SaveOutcome {
    Saved(GeneralSettings),
    Stale,
    InvalidTimezone,
}

/// The conditional UPDATE is the CAS linearization point. The prior read is
/// only for allowing a legacy timezone already stored by an older catalog.
/// If another writer changes that value/version, the CAS must fail instead.
pub async fn save_general(
    db: &SqlitePool,
    input: GeneralSettingsInput,
    expected_version: i64,
) -> Result<SaveOutcome, sqlx::Error> {
    let existing = get_general(db).await?;
    if existing.version != expected_version {
        return Ok(SaveOutcome::Stale);
    }
    if !is_current_timezone(&input.timezone) && input.timezone != existing.timezone {
        return Ok(SaveOutcome::InvalidTimezone);
    }
    let mut tx = db.begin().await?;
    let updated = sqlx::query(
        "UPDATE site_general_settings
         SET site_title = ?, tagline = ?, timezone = ?, version = version + 1
         WHERE id = 1 AND version = ?",
    )
    .bind(&input.site_title)
    .bind(&input.tagline)
    .bind(&input.timezone)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Ok(SaveOutcome::Stale);
    }
    tx.commit().await?;
    Ok(SaveOutcome::Saved(GeneralSettings {
        site_title: input.site_title,
        tagline: input.tagline,
        timezone: input.timezone,
        version: expected_version + 1,
    }))
}

pub const MAX_THEME_OPTIONS: usize = 64;
pub const MAX_THEME_OPTION_STRING_CHARS: usize = 4096;
pub const MAX_THEME_OPTION_CHOICES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeOptionType {
    String,
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
        validate_option_value(def, &def.default)?;
    }
    Ok(defs)
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

pub async fn get_theme_options_in_conn(
    conn: &mut SqliteConnection,
    theme_id: &str,
) -> ApiResult<Option<ThemeOptionsSnapshot>> {
    let Some(manifest_json) = latest_revision_manifest_in_conn(conn, theme_id).await? else {
        return Ok(None);
    };
    let schema = parse_theme_options_schema(&manifest_json).map_err(ApiError::bad_request)?;
    let version: i64 = sqlx::query_scalar(
        "SELECT version FROM theme_option_versions WHERE theme_id = ?",
    )
    .bind(theme_id)
    .fetch_optional(&mut *conn)
    .await?
    .unwrap_or(1);

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT option_key, value_json FROM theme_option_values WHERE theme_id = ?",
    )
    .bind(theme_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut saved_map = BTreeMap::new();
    for (k, v_json) in rows {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
            saved_map.insert(k, v);
        }
    }

    let mut values = BTreeMap::new();
    for def in &schema {
        let v = saved_map
            .get(&def.key)
            .cloned()
            .unwrap_or_else(|| def.default.clone());
        values.insert(def.key.clone(), v);
    }

    Ok(Some(ThemeOptionsSnapshot {
        theme_id: theme_id.to_owned(),
        version,
        schema,
        values,
    }))
}

pub async fn get_theme_options(
    db: &SqlitePool,
    theme_id: &str,
) -> ApiResult<Option<ThemeOptionsSnapshot>> {
    let mut conn = db.acquire().await?;
    get_theme_options_in_conn(&mut conn, theme_id).await
}

pub async fn put_theme_options_in_conn(
    conn: &mut SqliteConnection,
    theme_id: &str,
    expected_version: i64,
    submitted: BTreeMap<String, serde_json::Value>,
) -> ApiResult<ThemeOptionsOutcome> {
    let Some(manifest_json) = latest_revision_manifest_in_conn(conn, theme_id).await? else {
        return Ok(ThemeOptionsOutcome::NotFound);
    };
    let schema = parse_theme_options_schema(&manifest_json).map_err(ApiError::bad_request)?;
    let schema_map: BTreeMap<&str, &ThemeOptionDef> =
        schema.iter().map(|d| (d.key.as_str(), d)).collect();

    for (k, v) in &submitted {
        let Some(def) = schema_map.get(k.as_str()) else {
            return Err(ApiError::bad_request(
                "Option key không tồn tại trong schema của theme.",
            ));
        };
        validate_option_value(def, v).map_err(ApiError::bad_request)?;
        if def.option_type == ThemeOptionType::ImageRef
            && let Some(media_id) = v.as_str()
        {
            let exists: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM media_assets WHERE id = ? AND kind = 'image'")
                    .bind(media_id)
                    .fetch_optional(&mut *conn)
                    .await?;
            if exists.is_none() {
                return Err(ApiError::bad_request("media_id của image_ref không tồn tại."));
            }
        }
    }

    let current_ver: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM theme_option_versions WHERE theme_id = ?",
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
                "INSERT INTO theme_option_versions (theme_id, version) VALUES (?, 2)",
            )
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

    let snapshot = get_theme_options_in_conn(conn, theme_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được theme options sau khi lưu."))?;
    Ok(ThemeOptionsOutcome::Saved(snapshot))
}

pub async fn put_theme_options(
    db: &SqlitePool,
    theme_id: &str,
    expected_version: i64,
    submitted: BTreeMap<String, serde_json::Value>,
) -> ApiResult<ThemeOptionsOutcome> {
    let mut tx = db.begin().await?;
    let outcome = put_theme_options_in_conn(&mut tx, theme_id, expected_version, submitted).await?;
    if matches!(outcome, ThemeOptionsOutcome::Saved(_)) {
        tx.commit().await?;
    }
    Ok(outcome)
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

    let saved_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT option_key, value_json FROM theme_option_values WHERE theme_id = ?",
    )
    .bind(theme_id)
    .fetch_all(db)
    .await?;

    for (key, value_json) in saved_rows {
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

/// Returns only `public: true` options for the currently active v2 theme revision.
pub async fn get_active_public_theme_options(
    db: &SqlitePool,
) -> ApiResult<BTreeMap<String, serde_json::Value>> {
    let active: Option<(String, String)> = sqlx::query_as(
        "SELECT r.theme_id, r.manifest_json
         FROM theme_active_selection AS a
         JOIN theme_revisions AS r ON r.revision_id = a.revision_id
         WHERE a.id = 1 AND r.source_format = 'v2'",
    )
    .fetch_optional(db)
    .await?;
    let Some((theme_id, manifest_json)) = active else {
        return Ok(BTreeMap::new());
    };
    let Ok(schema) = parse_theme_options_schema(&manifest_json) else {
        return Ok(BTreeMap::new());
    };
    let saved_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT option_key, value_json FROM theme_option_values WHERE theme_id = ?",
    )
    .bind(&theme_id)
    .fetch_all(db)
    .await?;
    let mut saved_map = BTreeMap::new();
    for (k, v_json) in saved_rows {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
            saved_map.insert(k, v);
        }
    }
    let mut out = BTreeMap::new();
    for def in schema {
        if !def.public {
            continue;
        }
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
            _ => saved_map
                .get(&def.key)
                .cloned()
                .unwrap_or(def.default),
        };
        out.insert(def.key, val);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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

pub const MAX_CONTENT_LANGUAGE_BYTES: usize = 35;

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
    if value.is_empty() || value.len() > MAX_CONTENT_LANGUAGE_BYTES {
        return false;
    }
    let mut parts = value.split('-');
    let Some(primary) = parts.next() else {
        return false;
    };
    if !(2..=8).contains(&primary.len()) || !primary.bytes().all(|b| b.is_ascii_alphabetic()) {
        return false;
    }
    parts.all(|sub| (1..=8).contains(&sub.len()) && sub.bytes().all(|b| b.is_ascii_alphanumeric()))
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
}

pub async fn save_public(
    db: &SqlitePool,
    input: PublicSiteSettingsInput,
    expected_version: i64,
) -> Result<PublicSaveOutcome, sqlx::Error> {
    let mut tx = db.begin().await?;
    let updated = sqlx::query(
        "UPDATE site_public_settings
         SET public_origin = ?, content_language = ?, indexing_enabled = ?, version = version + 1
         WHERE id = 1 AND version = ?",
    )
    .bind(input.public_origin.as_deref())
    .bind(&input.content_language)
    .bind(input.indexing_enabled)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Ok(PublicSaveOutcome::Stale);
    }
    tx.commit().await?;
    Ok(PublicSaveOutcome::Saved(PublicSiteSettings {
        public_origin: input.public_origin,
        content_language: input.content_language,
        indexing_enabled: input.indexing_enabled,
        version: expected_version + 1,
    }))
}
