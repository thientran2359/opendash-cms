//! SITE-01 typed General Settings. SQL remains in storage.
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

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
