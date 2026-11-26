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

pub(in crate::storage::settings) async fn get_owner_settings_redacted_in_conn(
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
