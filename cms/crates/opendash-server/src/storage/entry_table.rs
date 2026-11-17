//! M7-S16-T05: bounded, permission-aware entry table snapshot.
//! The existing bare-array entries API deliberately stays in storage/entries.rs.

use std::collections::BTreeMap;

use opendash_core::{
    content::{EffectiveFieldStatus, FieldRole, PublicExposure},
    registry::ProviderStatus,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    error::{ApiError, ApiResult},
    storage::{content, entries, seo},
};

#[derive(Debug, Clone)]
pub struct EntryTableFilter {
    pub q: String,
    pub status: String,
    pub category_id: Option<i64>,
    pub tag_id: Option<i64>,
    pub include_descendants: bool,
    pub updated_month: Option<String>,
    pub sort: String,
    pub order: String,
    pub limit: i64,
    pub offset: i64,
}

impl Default for EntryTableFilter {
    fn default() -> Self {
        Self {
            q: String::new(),
            status: "all".into(),
            category_id: None,
            tag_id: None,
            include_descendants: false,
            updated_month: None,
            sort: "updated_at".into(),
            order: "desc".into(),
            limit: 20,
            offset: 0,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EntryTableRow {
    pub id: i64,
    pub type_key: String,
    pub title: String,
    pub slug: String,
    pub publication_status: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub term_ids: Vec<i64>,
    pub public_url: Option<String>,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize)]
pub struct StatusCounts {
    pub all: i64,
    pub draft: i64,
    pub published: i64,
}

#[derive(Debug, Serialize)]
pub struct EntryTableResult {
    pub schema_version: i32,
    pub items: Vec<EntryTableRow>,
    pub total: i64,
    pub status_counts: StatusCounts,
    pub limit: i64,
    pub offset: i64,
}

// Identical filters for count and row queries. Status is deliberately absent:
// status_counts represent the current filter selection before status selection.
const CATEGORY_SCOPE: &str = "
 WITH RECURSIVE category_scope(id, depth) AS (
   SELECT id, 1 FROM taxonomy_terms WHERE id = ? AND kind = 'category'
   UNION ALL
   SELECT c.id, category_scope.depth + 1 FROM taxonomy_terms c
   JOIN category_scope ON c.parent_id = category_scope.id
   WHERE category_scope.depth < 8 AND ? = 1
 )";

const SCOPE: &str = "
 FROM content_entries e
 WHERE e.content_type_key = ?
   AND (? = '' OR instr(lower(e.title), lower(?)) > 0
                OR instr(lower(e.slug), lower(?)) > 0)
   AND (? IS NULL OR substr(e.updated_at, 1, 7) = ?)
   AND (? IS NULL OR EXISTS (
     SELECT 1 FROM entry_terms et WHERE et.entry_id = e.id
       AND et.term_id IN (SELECT id FROM category_scope)
   ))
   AND (? IS NULL OR EXISTS (
     SELECT 1 FROM entry_terms et WHERE et.entry_id = e.id AND et.term_id = ?
   ))";

macro_rules! bind_scope {
    ($query:expr, $key:expr, $filter:expr) => {
        $query
            .bind($filter.category_id)
            .bind(i64::from($filter.include_descendants))
            .bind($key)
            .bind(&$filter.q)
            .bind(&$filter.q)
            .bind(&$filter.q)
            .bind(&$filter.updated_month)
            .bind(&$filter.updated_month)
            .bind($filter.category_id)
            .bind($filter.tag_id)
            .bind($filter.tag_id)
    };
}

async fn validate_terms(
    tx: &mut Transaction<'_, Sqlite>,
    filter: &EntryTableFilter,
) -> ApiResult<()> {
    for (id, kind) in [(filter.category_id, "category"), (filter.tag_id, "tag")] {
        if let Some(id) = id {
            let actual: Option<String> =
                sqlx::query_scalar("SELECT kind FROM taxonomy_terms WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&mut **tx)
                    .await?;
            if actual.as_deref() != Some(kind) {
                return Err(ApiError::bad_request(
                    "Category/Tag không tồn tại hoặc sai loại.",
                ));
            }
        }
    }
    Ok(())
}

/// A single SQLite read transaction guarantees counts, ACL and rows are from
/// one snapshot; separate HTTP pages intentionally make no cursor-isolation claim.
pub async fn query(
    db: &SqlitePool,
    type_key: &str,
    caller_role: FieldRole,
    filter: &EntryTableFilter,
) -> ApiResult<EntryTableResult> {
    let mut tx = db.begin().await?;
    let Some(effective) = content::load_effective_content_schema_in_tx(&mut tx, type_key).await?
    else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    };
    if effective.base.status != ProviderStatus::Active {
        return Err(ApiError::not_found("Loại nội dung không khả dụng."));
    }
    validate_terms(&mut tx, filter).await?;

    let count_sql = format!(
        "{CATEGORY_SCOPE} SELECT COUNT(*),
         COALESCE(SUM(CASE WHEN e.publication_status='draft' THEN 1 ELSE 0 END),0),
         COALESCE(SUM(CASE WHEN e.publication_status='published' THEN 1 ELSE 0 END),0)
         {SCOPE}"
    );
    let (all, draft, published): (i64, i64, i64) = bind_scope!(
        sqlx::query_as(sqlx::AssertSqlSafe(count_sql)),
        type_key,
        filter
    )
    .fetch_one(&mut *tx)
    .await?;
    let status_counts = StatusCounts {
        all,
        draft,
        published,
    };
    let total = match filter.status.as_str() {
        "draft" => draft,
        "published" => published,
        _ => all,
    };

    // Closed allowlists only, never interpolated user data.
    let column = match filter.sort.as_str() {
        "title" => "e.title COLLATE NOCASE",
        "created_at" => "e.created_at",
        "id" => "e.id",
        _ => "e.updated_at",
    };
    let direction = if filter.order == "asc" { "ASC" } else { "DESC" };
    let tie = if filter.sort == "id" {
        String::new()
    } else {
        format!(", e.id {direction}")
    };
    let rows_sql = format!(
        "{CATEGORY_SCOPE} SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                e.publication_status, e.version, e.created_at, e.updated_at
         {SCOPE} AND (? = 'all' OR e.publication_status = ?)
         ORDER BY {column} {direction}{tie} LIMIT ? OFFSET ?"
    );
    let selected: Vec<(
        i64,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
    )> = bind_scope!(
        sqlx::query_as(sqlx::AssertSqlSafe(rows_sql)),
        type_key,
        filter
    )
    .bind(&filter.status)
    .bind(&filter.status)
    .bind(filter.limit)
    .bind(filter.offset)
    .fetch_all(&mut *tx)
    .await?;

    let mut items = Vec::with_capacity(selected.len());
    for row in selected {
        let mut record = entries::decode_row(row)?;
        let term_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT term_id FROM entry_terms WHERE entry_id = ? ORDER BY term_id",
        )
        .bind(record.id)
        .fetch_all(&mut *tx)
        .await?;
        record.values.retain(|key, _| {
            effective.effective_fields.get(key).is_some_and(|field| {
                field.effective_status == EffectiveFieldStatus::Active
                    && caller_role >= field.definition.min_view_role
                    && field.definition.public_exposure == PublicExposure::Public
            })
        });
        let public_url = if record.publication_status == "published" {
            seo::resolve_public_entry_path_in_tx(&mut tx, record.id).await?
        } else {
            None
        };
        items.push(EntryTableRow {
            id: record.id,
            type_key: record.content_type_key,
            title: record.title,
            slug: record.slug,
            publication_status: record.publication_status,
            version: record.version,
            created_at: record.created_at,
            updated_at: record.updated_at,
            term_ids,
            public_url,
            fields: record.values,
        });
    }
    tx.commit().await?;
    Ok(EntryTableResult {
        schema_version: 1,
        items,
        total,
        status_counts,
        limit: filter.limit,
        offset: filter.offset,
    })
}
