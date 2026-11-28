use std::collections::{BTreeMap, BTreeSet};

use sqlx::{Row, SqliteConnection};

use crate::error::ApiResult;

pub(in crate::storage) async fn load_owner_option_maps_in_conn(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    default_locale: &str,
    target_locale: &str,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    let global_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT option_key, value_json
           FROM theme_option_values
          WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind)
    .bind(owner_id)
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
        let locale_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT option_key, value_json
               FROM theme_option_locale_values
              WHERE owner_kind = ? AND owner_id = ? AND locale = ?",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .bind(target_locale)
        .fetch_all(&mut *conn)
        .await?;
        for (k, v_json) in locale_rows {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
                locale_map.insert(k, v);
            }
        }
    }

    Ok((global_map, locale_map))
}

pub(in crate::storage) async fn load_owner_option_version_in_conn(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    default_locale: &str,
    target_locale: &str,
) -> ApiResult<i64> {
    let ver = if target_locale == default_locale {
        sqlx::query_scalar(
            "SELECT version FROM theme_option_versions WHERE owner_kind = ? AND owner_id = ?",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .fetch_optional(&mut *conn)
        .await?
        .unwrap_or(1)
    } else {
        sqlx::query_scalar(
            "SELECT version
               FROM theme_option_locale_versions
              WHERE owner_kind = ? AND owner_id = ? AND locale = ?",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .bind(target_locale)
        .fetch_optional(&mut *conn)
        .await?
        .unwrap_or(1)
    };
    Ok(ver)
}

pub(in crate::storage) async fn bump_owner_option_version_in_conn(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    theme_id: &str,
    secondary_locale: Option<&str>,
    expected_version: i64,
) -> ApiResult<bool> {
    if let Some(locale) = secondary_locale {
        let current_ver: Option<i64> = sqlx::query_scalar(
            "SELECT version
               FROM theme_option_locale_versions
              WHERE owner_kind = ? AND owner_id = ? AND locale = ?",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .bind(locale)
        .fetch_optional(&mut *conn)
        .await?;
        match current_ver {
            None => {
                if expected_version != 1 {
                    return Ok(false);
                }
                sqlx::query(
                    "INSERT INTO theme_option_locale_versions
                     (theme_id, owner_kind, owner_id, locale, version)
                     VALUES (?, ?, ?, ?, 2)",
                )
                .bind(theme_id)
                .bind(owner_kind)
                .bind(owner_id)
                .bind(locale)
                .execute(&mut *conn)
                .await?;
            }
            Some(ver) => {
                if ver != expected_version {
                    return Ok(false);
                }
                let res = sqlx::query(
                    "UPDATE theme_option_locale_versions
                        SET version = version + 1,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                      WHERE owner_kind = ? AND owner_id = ? AND locale = ? AND version = ?",
                )
                .bind(owner_kind)
                .bind(owner_id)
                .bind(locale)
                .bind(expected_version)
                .execute(&mut *conn)
                .await?;
                if res.rows_affected() == 0 {
                    return Ok(false);
                }
            }
        }
    } else {
        let current_ver: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM theme_option_versions WHERE owner_kind = ? AND owner_id = ?",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .fetch_optional(&mut *conn)
        .await?;
        match current_ver {
            None => {
                if expected_version != 1 {
                    return Ok(false);
                }
                sqlx::query(
                    "INSERT INTO theme_option_versions (theme_id, owner_kind, owner_id, version)
                     VALUES (?, ?, ?, 2)",
                )
                .bind(theme_id)
                .bind(owner_kind)
                .bind(owner_id)
                .execute(&mut *conn)
                .await?;
            }
            Some(ver) => {
                if ver != expected_version {
                    return Ok(false);
                }
                let res = sqlx::query(
                    "UPDATE theme_option_versions
                        SET version = version + 1,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                      WHERE owner_kind = ? AND owner_id = ? AND version = ?",
                )
                .bind(owner_kind)
                .bind(owner_id)
                .bind(expected_version)
                .execute(&mut *conn)
                .await?;
                if res.rows_affected() == 0 {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

pub(in crate::storage) async fn upsert_owner_option_value_in_conn(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    theme_id: &str,
    secondary_locale: Option<&str>,
    option_key: &str,
    value_json: &str,
) -> ApiResult<()> {
    if let Some(locale) = secondary_locale {
        sqlx::query(
            "INSERT INTO theme_option_locale_values
             (theme_id, owner_kind, owner_id, locale, option_key, value_json, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(owner_kind, owner_id, locale, option_key) DO UPDATE SET
                 value_json = excluded.value_json,
                 updated_at = excluded.updated_at",
        )
        .bind(theme_id)
        .bind(owner_kind)
        .bind(owner_id)
        .bind(locale)
        .bind(option_key)
        .bind(value_json)
        .execute(&mut *conn)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO theme_option_values
             (theme_id, owner_kind, owner_id, option_key, value_json, updated_at)
             VALUES (?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(owner_kind, owner_id, option_key) DO UPDATE SET
                 value_json = excluded.value_json,
                 updated_at = excluded.updated_at",
        )
        .bind(theme_id)
        .bind(owner_kind)
        .bind(owner_id)
        .bind(option_key)
        .bind(value_json)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub(in crate::storage) async fn sync_theme_option_media_refs_in_conn(
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
