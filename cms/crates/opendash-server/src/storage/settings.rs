//! SITE-01 typed General Settings. SQL remains in storage.
use std::sync::LazyLock;

use std::collections::{BTreeMap, BTreeSet};

use opendash_core::{
    block_schema::{
        BlockFieldSpec, MediaFieldKind, SelectOption, flattened_field_specs, validate_field_specs,
        validate_field_values, validate_single_field_value,
    },
    id::valid_snake_key_with_limit,
};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use crate::{
    error::{ApiError, ApiResult},
    storage::media::valid_media_id,
};

pub const TZDB_VERSION: &str = "2026f";
pub const MAX_SITE_TITLE_SCALARS: usize = 120;
pub const MAX_TAGLINE_SCALARS: usize = 240;

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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadingSettingsInput {
    pub homepage_page_id: Option<i64>,
    pub posts_page_id: Option<i64>,
}

async fn reading_page_status_in_conn(
    conn: &mut SqliteConnection,
    id: Option<i64>,
) -> ApiResult<ReadingPageStatus> {
    let Some(id) = id else {
        return Ok(ReadingPageStatus::Unconfigured);
    };
    let row: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT e.content_type_key, e.publication_status,
                COALESCE(json_extract(t.definition_json, '$.settings.public'), 1)
           FROM content_entries e
           JOIN content_types t ON t.key = e.content_type_key
          WHERE e.id = ? AND t.provider_status = 'active'",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(match row {
        None => ReadingPageStatus::Deleted,
        Some((key, publication, public))
            if key == "page" && publication == "published" && public == 1 =>
        {
            ReadingPageStatus::Published
        }
        Some(_) => ReadingPageStatus::Draft,
    })
}

/// A read on an existing SQLite connection can join a public route's snapshot.
pub async fn get_reading_in_conn(conn: &mut SqliteConnection) -> ApiResult<ReadingSettings> {
    let (homepage_page_id, posts_page_id, version): (Option<i64>, Option<i64>, i64) =
        sqlx::query_as(
            "SELECT homepage_page_id, posts_page_id, version
               FROM site_reading_settings WHERE id = 1",
        )
        .fetch_one(&mut *conn)
        .await?;
    let homepage_status = reading_page_status_in_conn(conn, homepage_page_id).await?;
    let posts_page_status = reading_page_status_in_conn(conn, posts_page_id).await?;
    Ok(ReadingSettings {
        homepage_page_id,
        posts_page_id,
        homepage_status,
        posts_page_status,
        version,
    })
}

pub async fn get_reading(db: &SqlitePool) -> ApiResult<ReadingSettings> {
    let mut tx = db.begin().await?;
    // Keep IDs, publication state and version from one SQLite read snapshot.
    let snapshot = get_reading_in_conn(&mut tx).await?;
    tx.commit().await?;
    Ok(snapshot)
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
          WHERE id = 1 AND (homepage_page_id = ? OR posts_page_id = ?)",
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
pub async fn save_reading(
    db: &SqlitePool,
    input: ReadingSettingsInput,
    expected_version: i64,
) -> ApiResult<ReadingSettings> {
    let mut tx = db.begin().await?;
    // Acquire a write reservation before reading a version or validating IDs.
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let current = get_reading_in_conn(&mut tx).await?;
    if current.version != expected_version {
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
        if id <= 0
            || reading_page_status_in_conn(&mut tx, Some(id)).await? != ReadingPageStatus::Published
        {
            return Err(ApiError::bad_request(
                "Page được chọn phải tồn tại, public và đã xuất bản.",
            ));
        }
    }
    let updated = sqlx::query(
        "UPDATE site_reading_settings
            SET homepage_page_id = ?, posts_page_id = ?,
                version = version + 1, updated_at = datetime('now')
          WHERE id = 1 AND version = ?",
    )
    .bind(input.homepage_page_id)
    .bind(input.posts_page_id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::precondition_failed(
            "Reading Settings đã thay đổi. Tải lại trước khi lưu.",
        ));
    }
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    let saved = get_reading_in_conn(&mut tx).await?;
    tx.commit().await?;
    Ok(saved)
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
    let mut conn = db.acquire().await?;
    get_general_in_conn(&mut conn).await
}

pub(super) async fn get_general_in_conn(
    conn: &mut SqliteConnection,
) -> Result<GeneralSettings, sqlx::Error> {
    sqlx::query_as::<_, GeneralSettings>(
        "SELECT site_title, tagline, timezone, version FROM site_general_settings WHERE id = 1",
    )
    .fetch_one(conn)
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

pub use opendash_core::block_schema::{
    GLOBAL_PUBLIC_URL_OPTION_KEYS, default_value_for_field_spec, global_site_option_defaults,
    global_site_option_fields,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeOptionType {
    String,
    Url,
    Number,
    Boolean,
    Select,
    ImageRef,
    Text,
    Textarea,
    Richtext,
    Range,
    RadioButtons,
    Color,
    Spacing,
    Media,
    Link,
    Repeater,
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
    #[serde(skip)]
    pub spec: Option<BlockFieldSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeOptionsSnapshot {
    pub theme_id: String,
    pub version: i64,
    pub schema: Vec<ThemeOptionDef>,
    pub fields: Vec<BlockFieldSpec>,
    pub values: BTreeMap<String, serde_json::Value>,
}

pub enum ThemeOptionsOutcome {
    Saved(ThemeOptionsSnapshot),
    NotFound,
    Stale,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSiteOptionsSnapshot {
    pub version: i64,
    pub fields: Vec<BlockFieldSpec>,
    pub values: BTreeMap<String, serde_json::Value>,
}

pub enum GlobalSiteOptionsOutcome {
    Saved(GlobalSiteOptionsSnapshot),
    Stale,
}

fn is_media_option_type(option_type: ThemeOptionType) -> bool {
    matches!(
        option_type,
        ThemeOptionType::ImageRef | ThemeOptionType::Media
    )
}

fn valid_plain_string(s: &str, max_chars: usize) -> bool {
    s.chars().count() <= max_chars
        && !s
            .chars()
            .any(|c| c == '\0' || (c.is_control() && c != '\n' && c != '\t'))
}

fn validate_url_or_link_spec_strict(
    spec: &BlockFieldSpec,
    value: &serde_json::Value,
) -> Result<(), &'static str> {
    match spec {
        BlockFieldSpec::Url { .. } => {
            if let Some(s) = value.as_str()
                && !s.is_empty()
            {
                crate::template_sec::validate_theme_option_url(s)?;
            }
        }
        BlockFieldSpec::Link { .. } => {
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
        BlockFieldSpec::Repeater { fields, .. } => {
            if let Some(arr) = value.as_array() {
                for item in arr {
                    if let Some(obj) = item.as_object() {
                        for sub in fields {
                            if let Some(v) = obj.get(sub.key()) {
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

pub fn validate_global_site_options_map(
    submitted: &BTreeMap<String, serde_json::Value>,
) -> Result<BTreeMap<String, serde_json::Value>, &'static str> {
    let fields = global_site_option_fields();
    let leaves = flattened_field_specs(&fields);
    let leaf_map: BTreeMap<&str, &BlockFieldSpec> = leaves.iter().map(|f| (f.key(), *f)).collect();

    let mut normalized = BTreeMap::new();
    for (k, v) in submitted {
        let Some(spec) = leaf_map.get(k.as_str()) else {
            return Err("Option toàn trang không tồn tại trong schema.");
        };
        let clean_val = match spec {
            BlockFieldSpec::Media { .. } => match v {
                serde_json::Value::Null => serde_json::Value::String(String::new()),
                serde_json::Value::String(id) if id.is_empty() || valid_media_id(id) => {
                    serde_json::Value::String(id.clone())
                }
                _ => return Err("Option media phải là media_id SHA-256 hợp lệ hoặc rỗng."),
            },
            _ => v.clone(),
        };
        validate_url_or_link_spec_strict(spec, &clean_val)?;
        normalized.insert(k.clone(), clean_val);
    }

    validate_field_values(&fields, &normalized, None)?;
    Ok(normalized)
}

pub fn validate_option_value(
    def: &ThemeOptionDef,
    value: &serde_json::Value,
) -> Result<(), &'static str> {
    if let Some(spec) = &def.spec {
        return match spec {
            BlockFieldSpec::Media { required, .. } => match value {
                serde_json::Value::Null => {
                    if *required {
                        Err("Option media bắt buộc không được để trống.")
                    } else {
                        Ok(())
                    }
                }
                serde_json::Value::String(id) => {
                    if id.is_empty() {
                        if *required {
                            Err("Option media bắt buộc không được để trống.")
                        } else {
                            Ok(())
                        }
                    } else if valid_media_id(id) {
                        Ok(())
                    } else {
                        Err("Option media phải là media_id SHA-256 hợp lệ hoặc rỗng.")
                    }
                }
                _ => Err("Option media phải là chuỗi media_id hoặc null."),
            },
            BlockFieldSpec::Repeater { .. } => {
                let map = BTreeMap::from([(def.key.clone(), value.clone())]);
                validate_field_values(std::slice::from_ref(spec), &map, None)?;
                validate_url_or_link_spec_strict(spec, value)
            }
            _ => {
                validate_single_field_value(spec, Some(value), false)?;
                validate_url_or_link_spec_strict(spec, value)
            }
        };
    }

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
        _ => Err("Option thiếu schema spec hiện đại."),
    }
}

fn upcast_legacy_def_to_field(def: &ThemeOptionDef) -> BlockFieldSpec {
    match def.option_type {
        ThemeOptionType::String => BlockFieldSpec::Text {
            key: def.key.clone(),
            label: def.label.clone(),
            max_chars: Some(MAX_THEME_OPTION_STRING_CHARS),
            required: false,
            default: def.default.as_str().map(ToOwned::to_owned),
            placeholder: None,
            show_if: None,
        },
        ThemeOptionType::Url => BlockFieldSpec::Url {
            key: def.key.clone(),
            label: def.label.clone(),
            required: false,
            default: def.default.as_str().map(ToOwned::to_owned),
            placeholder: None,
            show_if: None,
        },
        ThemeOptionType::Number => BlockFieldSpec::Number {
            key: def.key.clone(),
            label: def.label.clone(),
            min: def.min,
            max: def.max,
            step: None,
            required: false,
            default: def.default.as_f64(),
            unit: None,
            css_var: None,
            responsive: false,
            show_if: None,
        },
        ThemeOptionType::Boolean => BlockFieldSpec::Boolean {
            key: def.key.clone(),
            label: def.label.clone(),
            default: def.default.as_bool(),
            show_if: None,
        },
        ThemeOptionType::Select => BlockFieldSpec::Select {
            key: def.key.clone(),
            label: def.label.clone(),
            options: def
                .choices
                .iter()
                .map(|c| SelectOption {
                    value: c.value.clone(),
                    label: c.label.clone(),
                    icon: None,
                })
                .collect(),
            required: false,
            default: def.default.as_str().map(ToOwned::to_owned),
            display: None,
            css_var: None,
            responsive: false,
            show_if: None,
        },
        ThemeOptionType::ImageRef | ThemeOptionType::Media => BlockFieldSpec::Media {
            key: def.key.clone(),
            label: def.label.clone(),
            media_kind: MediaFieldKind::Image,
            required: false,
            default: def.default.as_str().map(ToOwned::to_owned),
            css_var: None,
            show_if: None,
        },
        _ => def.spec.clone().unwrap_or_else(|| BlockFieldSpec::Text {
            key: def.key.clone(),
            label: def.label.clone(),
            max_chars: Some(MAX_THEME_OPTION_STRING_CHARS),
            required: false,
            default: def.default.as_str().map(ToOwned::to_owned),
            placeholder: None,
            show_if: None,
        }),
    }
}

fn defs_to_block_fields(defs: &[ThemeOptionDef]) -> Vec<BlockFieldSpec> {
    defs.iter().map(upcast_legacy_def_to_field).collect()
}

fn extract_public_flags_and_strip(
    val: &mut serde_json::Value,
    out_flags: &mut BTreeMap<String, bool>,
) -> Result<(), &'static str> {
    let Some(obj) = val.as_object_mut() else {
        return Err("Khai báo options trong theme.json không đúng cấu trúc.");
    };
    let key = obj
        .get("key")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_default();
    if let Some(raw_pub) = obj.remove("public") {
        let Some(b) = raw_pub.as_bool() else {
            return Err("Thuộc tính public của option phải là boolean.");
        };
        if !key.is_empty() {
            out_flags.insert(key, b);
        }
    }
    if obj.get("type").and_then(serde_json::Value::as_str) == Some("group")
        && let Some(children) = obj
            .get_mut("fields")
            .and_then(serde_json::Value::as_array_mut)
    {
        for child in children {
            extract_public_flags_and_strip(child, out_flags)?;
        }
    }
    Ok(())
}

fn option_type_for_spec(spec: &BlockFieldSpec) -> ThemeOptionType {
    match spec {
        BlockFieldSpec::Text { .. } => ThemeOptionType::Text,
        BlockFieldSpec::Textarea { .. } => ThemeOptionType::Textarea,
        BlockFieldSpec::Richtext { .. } => ThemeOptionType::Richtext,
        BlockFieldSpec::Number { .. } => ThemeOptionType::Number,
        BlockFieldSpec::Range { .. } => ThemeOptionType::Range,
        BlockFieldSpec::Boolean { .. } => ThemeOptionType::Boolean,
        BlockFieldSpec::Select { .. } => ThemeOptionType::Select,
        BlockFieldSpec::RadioButtons { .. } => ThemeOptionType::RadioButtons,
        BlockFieldSpec::Spacing { .. } => ThemeOptionType::Spacing,
        BlockFieldSpec::Color { .. } => ThemeOptionType::Color,
        BlockFieldSpec::Media { .. } => ThemeOptionType::Media,
        BlockFieldSpec::Url { .. } => ThemeOptionType::Url,
        BlockFieldSpec::Link { .. } => ThemeOptionType::Link,
        BlockFieldSpec::Repeater { .. } => ThemeOptionType::Repeater,
        BlockFieldSpec::Group { .. } => ThemeOptionType::Text,
    }
}

fn def_from_modern_leaf_spec(spec: &BlockFieldSpec, public: bool) -> ThemeOptionDef {
    let default = default_value_for_field_spec(spec);
    let (min, max, choices) = match spec {
        BlockFieldSpec::Number { min, max, .. } => (*min, *max, Vec::new()),
        BlockFieldSpec::Range { min, max, .. } => (Some(*min), Some(*max), Vec::new()),
        BlockFieldSpec::Select { options, .. } | BlockFieldSpec::RadioButtons { options, .. } => (
            None,
            None,
            options
                .iter()
                .map(|o| ThemeOptionChoice {
                    value: o.value.clone(),
                    label: o.label.clone(),
                })
                .collect(),
        ),
        _ => (None, None, Vec::new()),
    };
    ThemeOptionDef {
        key: spec.key().to_owned(),
        label: spec.label().to_owned(),
        option_type: option_type_for_spec(spec),
        default,
        public,
        min,
        max,
        choices,
        spec: Some(spec.clone()),
    }
}

pub fn parse_theme_options_with_fields(
    manifest_json: &str,
) -> Result<(Vec<ThemeOptionDef>, Vec<BlockFieldSpec>), &'static str> {
    let manifest: serde_json::Value =
        serde_json::from_str(manifest_json).map_err(|_| "theme.json không hợp lệ.")?;
    let Some(raw_opts) = manifest.get("options") else {
        return Ok((Vec::new(), Vec::new()));
    };
    if raw_opts.is_null() {
        return Ok((Vec::new(), Vec::new()));
    }
    let Some(arr) = raw_opts.as_array() else {
        return Err("Khai báo options trong theme.json không đúng cấu trúc.");
    };

    const MODERN_ONLY_KEYS: &[&str] = &[
        "options",
        "display",
        "responsive",
        "show_if",
        "unit",
        "step",
        "max_chars",
        "max_bytes",
        "css_var",
        "required",
        "placeholder",
        "fields",
        "min_items",
        "max_items",
        "item_label_field",
        "media_kind",
    ];

    let mut defs = Vec::new();
    let mut fields = Vec::new();
    let mut modern_specs_to_validate = Vec::new();
    let mut seen_keys = BTreeSet::new();

    for raw_item in arr {
        let Some(obj) = raw_item.as_object() else {
            return Err("Khai báo options trong theme.json không đúng cấu trúc.");
        };
        let ty = obj.get("type").and_then(serde_json::Value::as_str);
        let has_modern_only = MODERN_ONLY_KEYS.iter().any(|k| obj.contains_key(*k));
        let is_legacy = matches!(ty, Some("string" | "image_ref"))
            || (!has_modern_only && matches!(ty, Some("number" | "boolean" | "select")))
            || (!has_modern_only
                && ty == Some("url")
                && obj
                    .get("default")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|s| !s.is_empty()));

        if is_legacy {
            let def: ThemeOptionDef = serde_json::from_value(raw_item.clone())
                .map_err(|_| "Khai báo options trong theme.json không đúng cấu trúc.")?;
            if !valid_snake_key_with_limit(&def.key, 64) {
                return Err("option_key phải là snake_case (1–64 ký tự).");
            }
            if !seen_keys.insert(def.key.clone()) {
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
            validate_option_value(&def, &def.default)?;
            fields.push(upcast_legacy_def_to_field(&def));
            defs.push(def);
        } else {
            let mut stripped = raw_item.clone();
            let mut public_flags = BTreeMap::new();
            extract_public_flags_and_strip(&mut stripped, &mut public_flags)?;
            let spec: BlockFieldSpec = serde_json::from_value(stripped)
                .map_err(|_| "Khai báo options trong theme.json không đúng cấu trúc.")?;
            for leaf in flattened_field_specs(std::slice::from_ref(&spec)) {
                if !seen_keys.insert(leaf.key().to_owned()) {
                    return Err("Trùng lặp option_key trong theme.json.");
                }
                let is_public = public_flags.get(leaf.key()).copied().unwrap_or(false);
                let leaf_def = def_from_modern_leaf_spec(leaf, is_public);
                validate_option_value(&leaf_def, &leaf_def.default)?;
                defs.push(leaf_def);
            }
            modern_specs_to_validate.push(spec.clone());
            fields.push(spec);
        }
    }

    if !modern_specs_to_validate.is_empty() {
        validate_field_specs(&modern_specs_to_validate)?;
    }

    if defs.len() > MAX_THEME_OPTIONS {
        return Err("Theme vượt quá giới hạn 64 options.");
    }

    Ok((defs, fields))
}

pub fn parse_theme_options_schema(
    manifest_json: &str,
) -> Result<Vec<ThemeOptionDef>, &'static str> {
    parse_theme_options_with_fields(manifest_json).map(|(defs, _)| defs)
}

/// Derive anchor-only permissions from the selected revision's validated
/// manifest plus global site public URL keys; request values and mutable context
/// cannot add capabilities.
pub fn theme_url_policy(
    manifest_json: &str,
) -> Result<crate::template_sec::ThemeUrlPolicy, &'static str> {
    let schema = parse_theme_options_schema(manifest_json)?;
    Ok(crate::template_sec::ThemeUrlPolicy::from_option_keys(
        GLOBAL_PUBLIC_URL_OPTION_KEYS
            .iter()
            .copied()
            .map(ToOwned::to_owned)
            .chain(
                schema
                    .iter()
                    .filter(|def| def.public && def.option_type == ThemeOptionType::Url)
                    .map(|def| def.key.clone()),
            ),
    ))
}

pub fn global_theme_url_policy() -> crate::template_sec::ThemeUrlPolicy {
    crate::template_sec::ThemeUrlPolicy::from_option_keys(
        GLOBAL_PUBLIC_URL_OPTION_KEYS
            .iter()
            .copied()
            .map(ToOwned::to_owned),
    )
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
    let leaf_map: BTreeMap<&str, &BlockFieldSpec> = flattened_field_specs(&fields)
        .into_iter()
        .map(|f| (f.key(), f))
        .collect();

    for (k, v) in &normalized {
        if let Some(BlockFieldSpec::Media { .. }) = leaf_map.get(k.as_str())
            && let Some(media_id) = v.as_str()
            && !media_id.is_empty()
        {
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM media_assets WHERE id = ? AND kind = 'image' AND lifecycle_state = 'active'",
            )
            .bind(media_id)
            .fetch_optional(&mut *conn)
            .await?;
            if exists.is_none() {
                return Err(ApiError::bad_request(
                    "media_id của site_logo không tồn tại.",
                ));
            }
        }
    }

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

        if let Some(BlockFieldSpec::Media { .. }) = leaf_map.get(k.as_str()) {
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
    }
    Ok(outcome)
}

pub async fn get_theme_options_in_conn(
    conn: &mut SqliteConnection,
    theme_id: &str,
) -> ApiResult<Option<ThemeOptionsSnapshot>> {
    let Some(manifest_json) = latest_revision_manifest_in_conn(conn, theme_id).await? else {
        return Ok(None);
    };
    let (schema, fields) =
        parse_theme_options_with_fields(&manifest_json).map_err(ApiError::bad_request)?;
    let version: i64 =
        sqlx::query_scalar("SELECT version FROM theme_option_versions WHERE theme_id = ?")
            .bind(theme_id)
            .fetch_optional(&mut *conn)
            .await?
            .unwrap_or(1);

    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT option_key, value_json FROM theme_option_values WHERE theme_id = ?")
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
        fields,
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
        if is_media_option_type(def.option_type)
            && let Some(media_id) = v.as_str()
            && !media_id.is_empty()
        {
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM media_assets WHERE id = ? AND kind = 'image' AND lifecycle_state = 'active'",
            )
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
            && is_media_option_type(def.option_type)
        {
            if let Some(media_id) = v.as_str().filter(|id| !id.is_empty()) {
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

    let saved_rows: Vec<(String, String)> =
        sqlx::query_as("SELECT option_key, value_json FROM theme_option_values WHERE theme_id = ?")
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

fn project_public_option_value(
    def: &ThemeOptionDef,
    raw: Option<&serde_json::Value>,
    is_preview: bool,
) -> serde_json::Value {
    let media_prefix = if is_preview {
        "/api/admin/media"
    } else {
        "/media"
    };
    match def.option_type {
        ThemeOptionType::ImageRef | ThemeOptionType::Media => {
            // Unsaved default image_ref/media never auto-publicizes media.
            match raw.and_then(|v| v.as_str()) {
                Some(media_id) if valid_media_id(media_id) => {
                    serde_json::Value::String(format!("{media_prefix}/{media_id}/file"))
                }
                _ => serde_json::Value::Null,
            }
        }
        ThemeOptionType::Url => match raw {
            Some(saved) if validate_option_value(def, saved).is_ok() => saved.clone(),
            _ => def.default.clone(),
        },
        _ => match raw {
            Some(saved) if validate_option_value(def, saved).is_ok() => saved.clone(),
            _ => def.default.clone(),
        },
    }
}

pub async fn get_public_global_options_in_conn(
    conn: &mut SqliteConnection,
    is_preview: bool,
) -> ApiResult<BTreeMap<String, serde_json::Value>> {
    let fields = global_site_option_fields();
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
    let media_prefix = if is_preview {
        "/api/admin/media"
    } else {
        "/media"
    };
    let mut out = BTreeMap::new();
    for leaf in flattened_field_specs(&fields) {
        let key = leaf.key();
        let raw = saved_map.get(key);
        let val = match leaf {
            BlockFieldSpec::Media { .. } => match raw.and_then(serde_json::Value::as_str) {
                Some(id) if valid_media_id(id) => {
                    serde_json::Value::String(format!("{media_prefix}/{id}/file"))
                }
                _ => serde_json::Value::String(String::new()),
            },
            BlockFieldSpec::Url { .. } => match raw {
                Some(v)
                    if v.as_str().is_some_and(|s| {
                        s.is_empty() || crate::template_sec::validate_theme_option_url(s).is_ok()
                    }) =>
                {
                    v.clone()
                }
                _ => default_value_for_field_spec(leaf),
            },
            _ => match raw {
                Some(v) if validate_single_field_value(leaf, Some(v), false).is_ok() => v.clone(),
                _ => default_value_for_field_spec(leaf),
            },
        };
        out.insert(key.to_owned(), val);
    }
    Ok(out)
}

pub(super) async fn get_public_theme_options_only_for_mode_in_conn(
    conn: &mut SqliteConnection,
    theme_id: &str,
    manifest_json: &str,
    transient_override: Option<&BTreeMap<String, serde_json::Value>>,
    is_preview: bool,
) -> ApiResult<BTreeMap<String, serde_json::Value>> {
    let Ok(schema) = parse_theme_options_schema(manifest_json) else {
        return Ok(BTreeMap::new());
    };
    let saved_rows: Vec<(String, String)> =
        sqlx::query_as("SELECT option_key, value_json FROM theme_option_values WHERE theme_id = ?")
            .bind(theme_id)
            .fetch_all(&mut *conn)
            .await?;
    let mut effective_raw = BTreeMap::new();
    for (k, v_json) in saved_rows {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
            effective_raw.insert(k, v);
        }
    }
    if let Some(ov) = transient_override {
        let schema_map: BTreeMap<&str, &ThemeOptionDef> =
            schema.iter().map(|d| (d.key.as_str(), d)).collect();
        for (k, v) in ov {
            let Some(def) = schema_map.get(k.as_str()) else {
                return Err(ApiError::unprocessable(
                    "Option key không tồn tại trong schema của theme.",
                ));
            };
            validate_option_value(def, v).map_err(ApiError::unprocessable)?;
            if is_media_option_type(def.option_type)
                && let Some(media_id) = v.as_str()
                && !media_id.is_empty()
            {
                let exists: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM media_assets WHERE id = ? AND kind = 'image' AND lifecycle_state = 'active'",
                )
                .bind(media_id)
                .fetch_optional(&mut *conn)
                .await?;
                if exists.is_none() {
                    return Err(ApiError::unprocessable(
                        "media_id của image_ref không tồn tại.",
                    ));
                }
            }
            effective_raw.insert(k.clone(), v.clone());
        }
    }
    let mut out = BTreeMap::new();
    for def in schema {
        if !def.public {
            continue;
        }
        let val = project_public_option_value(&def, effective_raw.get(&def.key), is_preview);
        out.insert(def.key, val);
    }
    Ok(out)
}

pub(super) async fn resolve_merged_public_options_with_overrides_in_conn(
    conn: &mut SqliteConnection,
    active_theme: Option<(&str, &str)>,
    transient_global: Option<&BTreeMap<String, serde_json::Value>>,
    transient_theme: Option<&BTreeMap<String, serde_json::Value>>,
    is_preview: bool,
) -> ApiResult<BTreeMap<String, serde_json::Value>> {
    let mut merged = get_public_global_options_in_conn(conn, is_preview).await?;
    if let Some(g_ov) = transient_global {
        let normalized = validate_global_site_options_map(g_ov).map_err(ApiError::unprocessable)?;
        let fields = global_site_option_fields();
        let leaf_map: BTreeMap<&str, &BlockFieldSpec> = flattened_field_specs(&fields)
            .into_iter()
            .map(|f| (f.key(), f))
            .collect();
        let media_prefix = if is_preview {
            "/api/admin/media"
        } else {
            "/media"
        };
        for (k, v) in normalized {
            let val = match leaf_map.get(k.as_str()) {
                Some(BlockFieldSpec::Media { .. }) => {
                    if let Some(media_id) = v.as_str().filter(|id| !id.is_empty()) {
                        let exists: Option<i64> = sqlx::query_scalar(
                            "SELECT 1 FROM media_assets WHERE id = ? AND kind = 'image' AND lifecycle_state = 'active'",
                        )
                        .bind(media_id)
                        .fetch_optional(&mut *conn)
                        .await?;
                        if exists.is_none() {
                            return Err(ApiError::unprocessable(
                                "media_id của site_logo không tồn tại.",
                            ));
                        }
                        serde_json::Value::String(format!("{media_prefix}/{media_id}/file"))
                    } else {
                        serde_json::Value::String(String::new())
                    }
                }
                _ => v,
            };
            merged.insert(k, val);
        }
    }

    if let Some((theme_id, manifest_json)) = active_theme {
        let theme_opts = get_public_theme_options_only_for_mode_in_conn(
            conn,
            theme_id,
            manifest_json,
            transient_theme,
            is_preview,
        )
        .await?;
        for (k, v) in theme_opts {
            merged.insert(k, v);
        }
    } else if transient_theme.is_some_and(|m| !m.is_empty()) {
        return Err(ApiError::unprocessable(
            "Không có theme v3 đang kích hoạt để xem trước theme_options.",
        ));
    }

    Ok(merged)
}

pub async fn get_active_merged_public_options_in_conn(
    conn: &mut SqliteConnection,
    is_preview: bool,
) -> ApiResult<BTreeMap<String, serde_json::Value>> {
    let active: Option<(String, String)> = sqlx::query_as(
        "SELECT r.theme_id, r.manifest_json
         FROM theme_active_selection AS a
         JOIN theme_revisions AS r ON r.revision_id = a.revision_id
         WHERE a.id = 1 AND r.source_format = 'v3'",
    )
    .fetch_optional(&mut *conn)
    .await?;
    let active_ref = active.as_ref().map(|(id, m)| (id.as_str(), m.as_str()));
    resolve_merged_public_options_with_overrides_in_conn(conn, active_ref, None, None, is_preview)
        .await
}

/// Returns merged durable Global public options + `public: true` options for the active v3 theme.
pub async fn get_active_public_theme_options(
    db: &SqlitePool,
) -> ApiResult<BTreeMap<String, serde_json::Value>> {
    let mut conn = db.acquire().await?;
    get_active_merged_public_options_in_conn(&mut conn, false).await
}

/// Resolve merged global + public theme values against the revision already captured by the caller.
pub(super) async fn get_public_theme_options_in_conn(
    conn: &mut SqliteConnection,
    theme_id: &str,
    manifest_json: &str,
) -> ApiResult<BTreeMap<String, serde_json::Value>> {
    resolve_merged_public_options_with_overrides_in_conn(
        conn,
        Some((theme_id, manifest_json)),
        None,
        None,
        false,
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
    async fn global_site_options_crud_cas_empty_url_and_unified_theme_options() {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::storage::content::preflight_builtin_key_collisions(&db)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();

        let snap = get_global_site_options(&db).await.unwrap();
        assert_eq!(snap.version, 1);
        assert_eq!(snap.fields.len(), 2);
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
        let public_opts = get_active_public_theme_options(&db).await.unwrap();
        assert_eq!(
            public_opts.get("social_facebook").unwrap(),
            "https://fb.com/opendash"
        );
        assert_eq!(public_opts.get("social_youtube").unwrap(), "");

        // Mixed legacy + modern BlockFieldSpec theme.json options parse and upcast cleanly.
        let manifest = serde_json::json!({
            "options": [
                {"key": "legacy_str", "label": "Legacy", "type": "string", "default": "ok", "public": true},
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
        let (defs, fields) = parse_theme_options_with_fields(&manifest).unwrap();
        assert_eq!(defs.len(), 4);
        assert_eq!(fields.len(), 3);
        let optional_cta_def = defs.iter().find(|d| d.key == "optional_cta").unwrap();
        assert!(validate_option_value(optional_cta_def, &serde_json::json!("")).is_ok());
        assert!(
            validate_option_value(optional_cta_def, &serde_json::json!("javascript:alert(1)"))
                .is_err()
        );
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
        fields: defs_to_block_fields(schema),
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
