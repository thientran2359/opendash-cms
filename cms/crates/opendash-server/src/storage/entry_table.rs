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
    pub taxonomy_key: Option<String>,
    pub term_id: Option<i64>,
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
            taxonomy_key: None,
            term_id: None,
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
 ), generic_scope(id, depth) AS (
   SELECT id, 1 FROM taxonomy_terms WHERE id = ? AND kind = ?
   UNION ALL
   SELECT c.id, generic_scope.depth + 1 FROM taxonomy_terms c
   JOIN generic_scope ON c.parent_id = generic_scope.id
   WHERE generic_scope.depth < 8 AND ? = 1
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
   ))
   AND (? IS NULL OR EXISTS (
     SELECT 1 FROM entry_terms et WHERE et.entry_id = e.id
       AND et.term_id IN (SELECT id FROM generic_scope)
   ))";

macro_rules! bind_scope {
    ($query:expr, $key:expr, $filter:expr) => {
        $query
            .bind($filter.category_id)
            .bind(i64::from($filter.include_descendants))
            .bind($filter.term_id)
            .bind(&$filter.taxonomy_key)
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
            .bind($filter.term_id)
    };
}

async fn validate_terms(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    filter: &EntryTableFilter,
) -> ApiResult<()> {
    entries::validate_filter_in_tx(
        tx,
        type_key,
        &entries::EntryFilters {
            category_id: filter.category_id,
            tag_id: filter.tag_id,
            taxonomy_key: filter.taxonomy_key.clone(),
            term_id: filter.term_id,
            include_descendants: filter.include_descendants,
        },
        false,
    )
    .await
}

/// A single SQLite read transaction guarantees counts, ACL and rows are from
/// one snapshot; separate HTTP pages intentionally make no cursor-isolation claim.
pub async fn query(
    db: &SqlitePool,
    type_key: &str,
    caller_role: FieldRole,
    filter: &EntryTableFilter,
) -> ApiResult<EntryTableResult> {
    if type_key == "pattern" {
        return Err(ApiError::forbidden("Section dùng bảng quản lý riêng tư."));
    }
    let mut tx = db.begin().await?;
    let Some(effective) = content::load_effective_content_schema_in_tx(&mut tx, type_key).await?
    else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    };
    if effective.base.status != ProviderStatus::Active {
        return Err(ApiError::not_found("Loại nội dung không khả dụng."));
    }
    validate_terms(&mut tx, type_key, filter).await?;

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
        // SEO's public resolver checks provider state and CPT visibility as
        // well as publication, so a private CPT remains editable without
        // giving its Admin row a misleading public View link.
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

#[cfg(test)]
mod c1_public_url_tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn published_private_cpt_remains_in_admin_without_view_url() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        sqlx::query(
            r#"INSERT INTO content_types(key,definition_json,owner_kind)
               VALUES('news','{"key":"news","label":"News","fields":[],"settings":{"public":false,"has_archive":true,"publication":"immediate","supports":["editor","featured_image"]}}','user')"#,
        )
        .execute(&db)
        .await
        .unwrap();
        let id = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
             VALUES('news','hidden-article','Hidden Article','{}','published','vi','eg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();

        let hidden = query(&db, "news", FieldRole::Admin, &EntryTableFilter::default())
            .await
            .unwrap();
        assert_eq!(hidden.total, 1);
        assert_eq!(hidden.status_counts.published, 1);
        assert_eq!(hidden.items[0].id, id);
        assert!(hidden.items[0].public_url.is_none());

        sqlx::query(
            "UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.public', json('true')) WHERE key='news'",
        )
        .execute(&db)
        .await
        .unwrap();
        // This legacy fixture writes directly to SQLite instead of using the
        // normal content-type writer. Rebuild C3 claims in the same serialized
        // transaction the production writer would use after its mutation.
        let mut tx = db.begin().await.unwrap();
        super::super::permalinks::sync_claims_in_tx(&mut tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let visible = query(&db, "news", FieldRole::Admin, &EntryTableFilter::default())
            .await
            .unwrap();
        assert_eq!(
            visible.items[0].public_url.as_deref(),
            Some("/news/hidden-article")
        );
    }
}
