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

pub(in crate::storage::settings) fn validate_url_or_link_defaults_strict(
    spec: &FieldSpec,
) -> Result<(), &'static str> {
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

pub(in crate::storage::settings) async fn latest_revision_manifest_in_conn(
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

pub(in crate::storage::settings) fn api_error_to_unprocessable(err: ApiError) -> ApiError {
    ApiError(axum::http::StatusCode::UNPROCESSABLE_ENTITY, err.1)
}
