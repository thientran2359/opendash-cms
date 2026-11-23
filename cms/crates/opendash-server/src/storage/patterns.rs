//! Site-owned private Pattern CPT and read-only active-theme Pattern Library.
//! Pattern AST uses the generic entry Builder storage and content_entries CAS.

use std::{collections::BTreeSet, path::Path};

use opendash_core::{
    builder::BuilderDocument,
    id::{valid_kebab_segment_with_limit, valid_snake_key},
};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    error::{ApiError, ApiResult},
    storage::theme,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternInput {
    pub title: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatternRecord {
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub version: i64,
    pub referencing_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

type PatternRow = (i64, String, String, i64, i64, String, String);

fn decode(row: PatternRow) -> PatternRecord {
    PatternRecord {
        id: row.0,
        title: row.1,
        slug: row.2,
        version: row.3,
        referencing_count: row.4,
        created_at: row.5,
        updated_at: row.6,
    }
}

fn validate(input: &PatternInput) -> ApiResult<()> {
    if input.title.trim().is_empty()
        || input.title.chars().count() > 200
        || input.title.chars().any(char::is_control)
    {
        return Err(ApiError::bad_request(
            "Tiêu đề Pattern không hợp lệ (1–200 ký tự).",
        ));
    }
    if !valid_kebab_segment_with_limit(&input.slug, 64) {
        return Err(ApiError::bad_request("Slug Pattern không hợp lệ."));
    }
    Ok(())
}

async fn reserve_writer(tx: &mut Transaction<'_, Sqlite>) -> ApiResult<()> {
    sqlx::query("UPDATE bootstrap SET initialized=initialized WHERE id=1")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn get_in_tx(tx: &mut Transaction<'_, Sqlite>, id: i64) -> ApiResult<PatternRecord> {
    if id <= 0 {
        return Err(ApiError::bad_request("Pattern ID phải dương."));
    }
    let row: Option<PatternRow> = sqlx::query_as(
        "SELECT e.id,e.title,e.slug,e.version,
                (SELECT COUNT(*) FROM entry_pattern_refs r WHERE r.target_pattern_id=e.id),
                e.created_at,e.updated_at
           FROM content_entries e
          WHERE e.id=? AND e.content_type_key='pattern' AND e.publication_status='draft'",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(decode)
        .ok_or_else(|| ApiError::not_found("Pattern không tồn tại."))
}

pub async fn list(db: &SqlitePool) -> ApiResult<Vec<PatternRecord>> {
    let rows: Vec<PatternRow> = sqlx::query_as(
        "SELECT e.id,e.title,e.slug,e.version,
                (SELECT COUNT(*) FROM entry_pattern_refs r WHERE r.target_pattern_id=e.id),
                e.created_at,e.updated_at
           FROM content_entries e
          WHERE e.content_type_key='pattern' AND e.publication_status='draft'
          ORDER BY e.id DESC",
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(decode).collect())
}

pub async fn get(db: &SqlitePool, id: i64) -> ApiResult<PatternRecord> {
    let mut tx = db.begin().await?;
    let record = get_in_tx(&mut tx, id).await?;
    tx.commit().await?;
    Ok(record)
}

pub async fn create(db: &SqlitePool, input: &PatternInput) -> ApiResult<PatternRecord> {
    validate(input)?;
    let mut tx = db.begin().await?;
    reserve_writer(&mut tx).await?;
    let default_locale = crate::storage::settings::get_default_locale_code_in_conn(&mut tx).await?;
    let group_id = crate::storage::settings::generate_translation_group_id("eg");
    let result = sqlx::query(
        "INSERT INTO content_entries
            (content_type_key,locale,translation_group_id,slug,title,values_json,publication_status,content_mode)
         VALUES('pattern', ?, ?, ?, ?, '{}', 'draft', 'builder')",
    )
    .bind(&default_locale)
    .bind(&group_id)
    .bind(&input.slug)
    .bind(&input.title)
    .execute(&mut *tx)
    .await;
    let id = match result {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::conflict("Slug Pattern đã tồn tại."));
        }
        other => other?.last_insert_rowid(),
    };
    sqlx::query(
        "INSERT INTO entry_builder_documents(entry_id,document_json,updated_at)
         VALUES(?,?,datetime('now'))",
    )
    .bind(id)
    .bind(serde_json::to_string(&BuilderDocument::empty())?)
    .execute(&mut *tx)
    .await?;
    let created = get_in_tx(&mut tx, id).await?;
    tx.commit().await?;
    Ok(created)
}

pub async fn update(
    db: &SqlitePool,
    id: i64,
    version: i64,
    input: &PatternInput,
) -> ApiResult<PatternRecord> {
    validate(input)?;
    let mut tx = db.begin().await?;
    reserve_writer(&mut tx).await?;
    let existing = get_in_tx(&mut tx, id).await?;
    if existing.version != version {
        return Err(ApiError::precondition_failed(
            "Pattern đã thay đổi; nháp chưa lưu.",
        ));
    }
    let result = sqlx::query(
        "UPDATE content_entries
            SET title=?,slug=?,version=version+1,updated_at=datetime('now')
          WHERE id=? AND content_type_key='pattern'
            AND publication_status='draft' AND version=?",
    )
    .bind(&input.title)
    .bind(&input.slug)
    .bind(id)
    .bind(version)
    .execute(&mut *tx)
    .await;
    let updated = match result {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::conflict("Slug Pattern đã tồn tại."));
        }
        other => other?,
    };
    if updated.rows_affected() != 1 {
        return Err(ApiError::precondition_failed("Pattern đã thay đổi."));
    }
    let record = get_in_tx(&mut tx, id).await?;
    tx.commit().await?;
    Ok(record)
}

#[derive(Debug, Clone, Serialize)]
pub struct ReferencingEntry {
    pub id: i64,
    pub type_key: String,
    pub title: String,
}

#[derive(Debug)]
pub enum DeleteOutcome {
    Deleted,
    Referenced {
        referencing_count: i64,
        referencing_entries: Vec<ReferencingEntry>,
    },
}

pub async fn delete(
    db: &SqlitePool,
    id: i64,
    version: i64,
    force: bool,
) -> ApiResult<DeleteOutcome> {
    let mut tx = db.begin().await?;
    reserve_writer(&mut tx).await?;
    let existing = get_in_tx(&mut tx, id).await?;
    if existing.version != version {
        return Err(ApiError::precondition_failed(
            "Pattern đã thay đổi; tải lại trước khi xóa.",
        ));
    }
    if existing.referencing_count > 0 && !force {
        let rows: Vec<(i64, String, String)> = sqlx::query_as(
            "SELECT e.id,e.content_type_key,e.title
               FROM entry_pattern_refs r
               JOIN content_entries e ON e.id=r.source_entry_id
              WHERE r.target_pattern_id=?
              ORDER BY e.id LIMIT 50",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        return Ok(DeleteOutcome::Referenced {
            referencing_count: existing.referencing_count,
            referencing_entries: rows
                .into_iter()
                .map(|(id, type_key, title)| ReferencingEntry {
                    id,
                    type_key,
                    title,
                })
                .collect(),
        });
    }
    let affected = sqlx::query(
        "DELETE FROM content_entries
          WHERE id=? AND content_type_key='pattern'
            AND publication_status='draft' AND version=?",
    )
    .bind(id)
    .bind(version)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if affected != 1 {
        return Err(ApiError::precondition_failed("Pattern đã thay đổi."));
    }
    // Builder AST, media pins and backlink projections cascade with the entry.
    // Callers still retain their original AST with a now-missing pattern_id.
    tx.commit().await?;
    Ok(DeleteOutcome::Deleted)
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemePatternItem {
    pub source: &'static str,
    pub id: String,
    pub key: String,
    pub title: String,
    pub category: String,
    pub description: Option<String>,
    pub preview_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ThemePatternCatalog {
    pub categories: Vec<String>,
    pub items: Vec<ThemePatternItem>,
}

#[derive(Debug, Serialize)]
pub struct ThemePatternDocument {
    pub source: &'static str,
    pub key: String,
    pub title: String,
    pub document: BuilderDocument,
}

/// Read the verified active v3 manifest; inactive/legacy themes yield an empty
/// catalog. Preview URLs use the actual revision-digest asset route.
pub async fn list_theme_patterns(
    db: &SqlitePool,
    themes_root: &Path,
    category: Option<&str>,
    q: Option<&str>,
) -> ApiResult<ThemePatternCatalog> {
    let mut categories = BTreeSet::new();
    let mut items = Vec::new();
    if let Some(active) = theme::load_active_theme_v3(db, themes_root).await? {
        let query = q.unwrap_or("").trim().to_lowercase();
        let filter = category.unwrap_or("").trim();
        for pattern in active.manifest.patterns {
            categories.insert(pattern.category.clone());
            if !filter.is_empty() && filter != "all" && filter != pattern.category {
                continue;
            }
            if !query.is_empty()
                && !format!(
                    "{} {} {}",
                    pattern.title,
                    pattern.category,
                    pattern.description.as_deref().unwrap_or("")
                )
                .to_lowercase()
                .contains(&query)
            {
                continue;
            }
            items.push(ThemePatternItem {
                source: "theme",
                id: format!("theme:{}:{}", active.manifest.theme_id, pattern.key),
                key: pattern.key,
                title: pattern.title,
                category: pattern.category,
                description: pattern.description,
                preview_url: pattern
                    .preview_asset
                    .map(|file| crate::theme::theme_asset_url(&active.revision_digest, &file)),
            });
        }
    }
    let mut all_categories = vec!["all".to_string()];
    all_categories.extend(categories);
    Ok(ThemePatternCatalog {
        categories: all_categories,
        items,
    })
}

pub async fn get_theme_pattern(
    db: &SqlitePool,
    themes_root: &Path,
    key: &str,
) -> ApiResult<ThemePatternDocument> {
    if !valid_snake_key(key) {
        return Err(ApiError::bad_request("Khóa Pattern không hợp lệ."));
    }
    let active = theme::load_active_theme_v3(db, themes_root)
        .await?
        .ok_or_else(|| ApiError::not_found("Không có active theme v3."))?;
    let pattern = active
        .manifest
        .patterns
        .iter()
        .find(|pattern| pattern.key == key)
        .ok_or_else(|| ApiError::not_found("Theme Pattern không tồn tại."))?;
    // The manifest was validated and its revision files passed the durable
    // digest check. Never read a caller-supplied filesystem path.
    let file = themes_root
        .join("revisions")
        .join(&active.revision_digest)
        .join(&pattern.file);
    let bytes =
        std::fs::read(file).map_err(|_| ApiError::not_found("Theme Pattern không khả dụng."))?;
    let document = BuilderDocument::from_json(&bytes)
        .map_err(|_| ApiError::not_found("Theme Pattern không khả dụng."))?;
    Ok(ThemePatternDocument {
        source: "theme",
        key: pattern.key.clone(),
        title: pattern.title.clone(),
        document,
    })
}
