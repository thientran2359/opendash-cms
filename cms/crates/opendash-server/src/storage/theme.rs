//! Explicit M3 theme selection. Never mutate home_page when activating a theme.
use serde::Serialize;
use sqlx::SqlitePool;

use crate::{
    error::{ApiError, ApiResult},
    theme::{Block, Fields},
};

#[derive(Debug, Serialize)]
pub struct ThemeItem {
    pub id: String,
    pub compatible_block_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ThemeList {
    pub items: Vec<ThemeItem>,
    pub active_id: Option<String>,
    pub version: i64,
    /// Null means no saved homepage or no explicit activation.
    pub homepage_compatible: Option<bool>,
}

pub enum ActivationOutcome {
    Activated(ThemeList),
    NotFound,
    Stale,
}

#[derive(Debug)]
pub struct SelectedTemplate {
    pub template: Option<String>,
    pub visibility: &'static str,
}

#[derive(Debug)]
pub struct HomeSnapshot {
    pub block: Block,
    pub original_fields: Fields,
    pub selected: SelectedTemplate,
}

/// Source block ID is never overwritten or silently translated. NULL active
/// means the original legacy template. Explicit unsupported means blank page.
pub fn select_template(
    source_id: &str,
    source_template: String,
    active_id: Option<&str>,
    active_alias: Option<&str>,
    active_template: Option<String>,
) -> SelectedTemplate {
    match active_id {
        None => SelectedTemplate {
            template: Some(source_template),
            visibility: "rendered",
        },
        Some(id) if source_id == id || Some(source_id) == active_alias => SelectedTemplate {
            template: active_template,
            visibility: "rendered",
        },
        Some(_) => SelectedTemplate {
            template: None,
            visibility: "unsupported",
        },
    }
}

/// Save and render using one SQLite write snapshot. The renderer executes
/// before commit, so a template error cannot leave a successful write behind.
/// No hook is invoked here; callers dispatch only after this transaction commits.
pub async fn save_home_page_and_render<T, F>(
    db: &SqlitePool,
    block: &Block,
    source_template: String,
    render: F,
) -> ApiResult<T>
where
    F: FnOnce(SelectedTemplate) -> ApiResult<T>,
{
    let mut tx = db.begin().await?;
    // Builder conversion is one-way. Reserve the same singleton writer used
    // by layout Save *before* touching M3 home_page; a legacy request racing
    // with conversion can never revive the old public source.
    let changed = sqlx::query(
        "UPDATE builder_homepage SET version = version + 1
         WHERE id = 1 AND document_json IS NULL AND version < 9223372036854775807",
    )
    .execute(&mut *tx)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "Homepage đã chuyển sang Builder dạng cây. Hãy mở /api/admin/builder/homepage.",
        ));
    }
    sqlx::query(
        "INSERT INTO home_page (id, block_id, values_json) VALUES (1, ?, ?)
         ON CONFLICT(id) DO UPDATE SET block_id = excluded.block_id, values_json = excluded.values_json",
    )
    .bind(&block.kind)
    .bind(serde_json::to_string(&block.values)?)
    .execute(&mut *tx)
    .await?;
    let (active_id, alias, active_template): (Option<String>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT a.block_id, t.compatible_block_id, t.template
             FROM site_active_theme a
             LEFT JOIN theme_blocks t ON t.id = a.block_id
             WHERE a.id = 1",
        )
        .fetch_one(&mut *tx)
        .await?;
    let selected = select_template(
        &block.kind,
        source_template,
        active_id.as_deref(),
        alias.as_deref(),
        active_template,
    );
    let output = render(selected)?;
    tx.commit().await?;
    Ok(output)
}

pub async fn active_selection(
    db: &SqlitePool,
) -> ApiResult<(Option<String>, Option<String>, Option<String>)> {
    let selection: (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT a.block_id, t.compatible_block_id, t.template
         FROM site_active_theme a
         LEFT JOIN theme_blocks t ON t.id = a.block_id
         WHERE a.id = 1",
    )
    .fetch_one(db)
    .await?;
    Ok(selection)
}

/// Atomic, single-query view of the saved homepage and current active theme.
/// Values and fields are never projected through a different theme's schema.
pub async fn public_home(db: &SqlitePool) -> ApiResult<Option<HomeSnapshot>> {
    let row: Option<(
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT p.block_id, p.values_json, src.fields_json, src.template,
                a.block_id, active.compatible_block_id, active.template
         FROM home_page p
         JOIN theme_blocks src ON src.id = p.block_id
         JOIN site_active_theme a ON a.id = 1
         LEFT JOIN theme_blocks active ON active.id = a.block_id
         WHERE p.id = 1",
    )
    .fetch_optional(db)
    .await?;
    row.map(
        |(kind, values, fields, original_template, active_id, alias, active_template)| {
            let selected = select_template(
                &kind,
                original_template,
                active_id.as_deref(),
                alias.as_deref(),
                active_template,
            );
            Ok(HomeSnapshot {
                block: Block {
                    kind,
                    values: serde_json::from_str(&values)?,
                },
                original_fields: serde_json::from_str(&fields)?,
                selected,
            })
        },
    )
    .transpose()
}

pub async fn list(db: &SqlitePool) -> ApiResult<ThemeList> {
    let (active_id, version): (Option<String>, i64) =
        sqlx::query_as("SELECT block_id, version FROM site_active_theme WHERE id = 1")
            .fetch_one(db)
            .await?;
    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, compatible_block_id FROM theme_blocks ORDER BY id")
            .fetch_all(db)
            .await?;
    // Once the Builder layout is saved, the old M3 home_page is retained
    // solely for backup/restore. Its single-block compatibility must not be
    // reported as compatibility of the new (possibly mixed) Builder tree.
    let saved: Option<String> = sqlx::query_scalar(
        "SELECT p.block_id FROM home_page p
         JOIN builder_homepage b ON b.id = 1 AND b.document_json IS NULL
         WHERE p.id = 1",
    )
    .fetch_optional(db)
    .await?;
    let homepage_compatible = match (saved.as_deref(), active_id.as_deref()) {
        (Some(source), Some(id)) => Some(
            rows.iter()
                .find(|(key, _)| key == id)
                .is_some_and(|(_, alias)| source == id || alias.as_deref() == Some(source)),
        ),
        _ => None,
    };
    Ok(ThemeList {
        items: rows
            .into_iter()
            .map(|(id, compatible_block_id)| ThemeItem {
                id,
                compatible_block_id,
            })
            .collect(),
        active_id,
        version,
        homepage_compatible,
    })
}

pub async fn activate(db: &SqlitePool, id: &str, version: i64) -> ApiResult<ActivationOutcome> {
    if !opendash_core::id::valid_namespaced_block_id(id) {
        return Err(ApiError::bad_request("ID theme không hợp lệ."));
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM theme_blocks WHERE id = ?)")
        .bind(id)
        .fetch_one(db)
        .await?;
    if !exists {
        return Ok(ActivationOutcome::NotFound);
    }
    // Optimistic CAS: two tabs with same version cannot both activate.
    let result = sqlx::query(
        "UPDATE site_active_theme
         SET block_id = ?, version = version + 1
         WHERE id = 1 AND version = ?",
    )
    .bind(id)
    .bind(version)
    .execute(db)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(ActivationOutcome::Stale);
    }
    Ok(ActivationOutcome::Activated(list(db).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compatibility_selection_never_uses_old_html_for_unsupported_blocks() {
        let legacy = select_template("a/banner", "A".into(), None, None, None);
        assert_eq!(legacy.template.as_deref(), Some("A"));
        let alias = select_template(
            "a/banner",
            "A".into(),
            Some("b/banner"),
            Some("a/banner"),
            Some("B".into()),
        );
        assert_eq!(alias.template.as_deref(), Some("B"));
        let absent = select_template(
            "a/banner",
            "A".into(),
            Some("c/banner"),
            None,
            Some("C".into()),
        );
        assert_eq!(absent.visibility, "unsupported");
        assert!(absent.template.is_none());
    }
}
