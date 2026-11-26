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

use super::read::*;
use crate::storage::settings::{
    sync_theme_option_media_refs_in_conn, validate_url_or_link_spec_strict,
};

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
pub(in crate::storage::settings) async fn put_owner_settings(
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
