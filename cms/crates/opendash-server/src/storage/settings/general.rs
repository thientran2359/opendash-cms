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

use super::locales::{get_default_locale_code_in_conn, resolve_requested_locale_in_conn};

pub const TZDB_VERSION: &str = "2026f";
pub const MAX_SITE_TITLE_SCALARS: usize = 120;
pub const MAX_TAGLINE_SCALARS: usize = 240;

static TIMEZONES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    include_str!("../settings_timezones_2026f.txt")
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
    pub cms_locale: String,
    pub version: i64,
    pub has_override: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralSettingsInput {
    pub site_title: String,
    pub tagline: String,
    pub timezone: String,
    #[serde(default)]
    pub cms_locale: Option<String>,
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

#[derive(Debug, Clone, Serialize)]
pub struct AdminI18nBootstrap {
    pub configured_locale: String,
    pub effective_locale: &'static str,
    pub effective_bcp47: &'static str,
    pub effective_direction: &'static str,
    pub configured_installed: bool,
    pub catalog_version: &'static str,
    pub available_locales: Vec<CmsLocaleOption>,
    pub catalog_etag: String,
    pub catalog_url: String,
}

fn core_pot_document() -> &'static opendash_core::l10n::PotDocument {
    static POT: std::sync::OnceLock<opendash_core::l10n::PotDocument> = std::sync::OnceLock::new();
    POT.get_or_init(|| {
        opendash_core::l10n::PotDocument::parse(include_str!("../../../../../l10n/opendash.pot"))
            .unwrap_or_default()
    })
}

pub async fn get_cms_locale_in_conn(conn: &mut SqliteConnection) -> ApiResult<String> {
    Ok(
        sqlx::query_scalar("SELECT cms_locale FROM site_general_settings WHERE id = 1")
            .fetch_one(&mut *conn)
            .await?,
    )
}

pub async fn get_admin_i18n_bootstrap(db: &SqlitePool) -> ApiResult<AdminI18nBootstrap> {
    get_admin_i18n_bootstrap_with_registry(db, None).await
}

pub async fn get_admin_i18n_bootstrap_with_registry(
    db: &SqlitePool,
    registry: Option<&crate::l10n::CatalogRegistry>,
) -> ApiResult<AdminI18nBootstrap> {
    let mut conn = db.acquire().await?;
    let configured_locale = get_cms_locale_in_conn(&mut conn).await?;
    let mut available_locales: Vec<CmsLocaleOption> = CMS_LANGUAGE_CATALOG.to_vec();
    let pot = core_pot_document();

    if let Some(reg) = registry {
        for option in &mut available_locales {
            if option.code == "en" {
                option.installed = true;
                option.translation_coverage = 100;
            } else if let Some(loaded) =
                reg.resolve(crate::l10n::L10nScope::Core, "opendash", option.code)
            {
                option.installed = true;
                option.translation_coverage = loaded.catalog.coverage_against_pot(pot);
            }
        }
    }

    let matched = find_cms_locale(&configured_locale).and_then(|opt| {
        available_locales
            .iter()
            .find(|a| a.code == opt.code && a.installed)
            .copied()
    });
    let configured_installed = matched.is_some();
    let effective = matched.unwrap_or(available_locales[0]);
    let catalog_etag = registry
        .and_then(|reg| {
            reg.snapshot()
                .admin_json_cache
                .get(effective.code)
                .map(|c| c.etag.clone())
        })
        .unwrap_or_else(|| "\"en-builtin\"".to_owned());
    let catalog_url = format!("/api/i18n/admin/catalog?locale={}", effective.code);

    Ok(AdminI18nBootstrap {
        configured_locale,
        effective_locale: effective.code,
        effective_bcp47: effective.bcp47,
        effective_direction: effective.direction,
        configured_installed,
        catalog_version: CMS_CATALOG_VERSION,
        available_locales,
        catalog_etag,
        catalog_url,
    })
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

pub(crate) async fn get_general_in_conn_for_locale(
    conn: &mut SqliteConnection,
    requested_locale: Option<&str>,
) -> ApiResult<GeneralSettings> {
    let default_locale = get_default_locale_code_in_conn(conn).await?;
    let target_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;
    let (timezone, cms_locale): (String, String) =
        sqlx::query_as("SELECT timezone, cms_locale FROM site_general_settings WHERE id = 1")
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
        cms_locale,
        version,
        has_override,
        locale: requested_locale.map(|_| target_locale),
    })
}

pub(in crate::storage) async fn get_general_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<GeneralSettings> {
    get_general_in_conn_for_locale(conn, None).await
}

pub enum SaveOutcome {
    Saved(GeneralSettings),
    Stale,
    InvalidTimezone,
    SecondaryTimezoneLocked,
    InvalidCmsLocale,
    SecondaryCmsLocaleLocked,
    InvalidLocale(String),
}

/// Runs inside `BEGIN IMMEDIATE` so reads and version/timezone validation share
/// a single SQLite write lock. Secondary locales cannot alter global timezone or CMS language.
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
        let (global_tz, global_cms_locale): (String, String) =
            sqlx::query_as("SELECT timezone, cms_locale FROM site_general_settings WHERE id = 1")
                .fetch_one(&mut *conn)
                .await?;
        let normalized_cms = match input.cms_locale.as_deref() {
            Some(raw) => match find_cms_locale(raw) {
                Some(option) => Some(option.code.to_owned()),
                None => return Ok(SaveOutcome::InvalidCmsLocale),
            },
            None => None,
        };
        let is_default_locale = target_locale == default_locale;
        if !is_default_locale && normalized_cms.as_ref().is_some_and(|locale| locale != &global_cms_locale) {
            return Ok(SaveOutcome::SecondaryCmsLocaleLocked);
        }
        let next_cms_locale = normalized_cms.unwrap_or(global_cms_locale);
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
                    SET site_title = ?, tagline = ?, timezone = ?, cms_locale = ?, version = ?
                  WHERE id = 1",
            )
            .bind(&input.site_title)
            .bind(&input.tagline)
            .bind(&input.timezone)
            .bind(&next_cms_locale)
            .bind(new_version)
            .execute(&mut *conn)
            .await?;
        }

        Ok(SaveOutcome::Saved(GeneralSettings {
            site_title: input.site_title,
            tagline: input.tagline,
            timezone: input.timezone,
            cms_locale: next_cms_locale,
            version: new_version,
            has_override: true,
            locale: requested_locale.map(|_| target_locale),
        }))
    }
    .await;
    match res {
        Ok(outcome) if matches!(outcome, SaveOutcome::Saved(_)) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            crate::cache::SiteCacheEpoch::global().bump();
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
