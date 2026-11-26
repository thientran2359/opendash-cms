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

use super::locales::normalize_locale_code;
use crate::error::{ApiError, ApiResult};

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
