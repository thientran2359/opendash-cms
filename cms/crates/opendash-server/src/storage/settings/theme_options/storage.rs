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

use super::super::locales::{get_default_locale_code_in_conn, resolve_requested_locale_in_conn};
use super::schema::*;

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

pub(in crate::storage::settings) async fn load_theme_option_maps_in_conn(
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

pub(in crate::storage::settings) async fn sync_theme_option_media_refs_in_conn(
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
