//! CRUD for saved content entries. Never puts SQL inside HTTP routes.

use std::collections::BTreeMap;

use opendash_core::content::ContentEntryInput;
use serde::Serialize;
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    auth::{self, SessionContext},
    error::{ApiError, ApiResult},
};

#[derive(Debug, Clone, Serialize)]
pub struct ContentEntryRecord {
    pub id: i64,
    pub content_type_key: String,
    pub slug: String,
    pub title: String,
    pub values: BTreeMap<String, Value>,
    /// Site-owned term IDs. Stable across content slug edits, publication and provider disable.
    pub term_ids: Vec<i64>,
    /// Server-owned status; legacy CPT entries remain published on Save.
    pub publication_status: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

type EntryRow = (
    i64,
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
);

pub(super) fn decode_row(row: EntryRow) -> ApiResult<ContentEntryRecord> {
    let (
        id,
        content_type_key,
        slug,
        title,
        json,
        publication_status,
        version,
        created_at,
        updated_at,
    ) = row;
    let values: BTreeMap<String, Value> = serde_json::from_str(&json)
        .map_err(|_| ApiError::internal("Dữ liệu custom fields không hợp lệ."))?;
    Ok(ContentEntryRecord {
        id,
        content_type_key,
        slug,
        title,
        values,
        term_ids: Vec::new(),
        publication_status,
        version,
        created_at,
        updated_at,
    })
}

/// Category/tag query predicates. Missing IDs mean no restriction.
#[derive(Debug, Clone, Copy, Default)]
pub struct EntryFilters {
    pub category_id: Option<i64>,
    pub tag_id: Option<i64>,
    pub include_descendants: bool,
}

async fn read_term_ids(tx: &mut Transaction<'_, Sqlite>, id: i64) -> ApiResult<Vec<i64>> {
    Ok(
        sqlx::query_scalar("SELECT term_id FROM entry_terms WHERE entry_id = ? ORDER BY term_id")
            .bind(id)
            .fetch_all(&mut **tx)
            .await?,
    )
}

async fn decoded_with_terms(
    tx: &mut Transaction<'_, Sqlite>,
    row: EntryRow,
) -> ApiResult<ContentEntryRecord> {
    let mut record = decode_row(row)?;
    record.term_ids = read_term_ids(tx, record.id).await?;
    Ok(record)
}

/// Acquire a writer before checking a term's existence or entry CAS. The same
/// SQLite writer reservation is used by taxonomy term deletion, so no race can
/// commit a link to a term that has just been deleted.
async fn begin_entry_write<'a>(
    db: &'a SqlitePool,
    actor: &SessionContext,
) -> ApiResult<Transaction<'a, Sqlite>> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let current: Option<(i64, String)> = sqlx::query_as(
        "SELECT u.id, u.role FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(&actor.token_hash)
    .bind(auth::current_time_secs())
    .fetch_optional(&mut *tx)
    .await?;
    match current {
        Some((id, role)) if id == actor.user.id && matches!(role.as_str(), "admin" | "editor") => {
            Ok(tx)
        }
        Some(_) => Err(ApiError::forbidden(
            "Bạn không có quyền chỉnh sửa nội dung.",
        )),
        None => Err(ApiError::unauthorized("Phiên đăng nhập đã bị thu hồi.")),
    }
}

async fn validate_term_ids(tx: &mut Transaction<'_, Sqlite>, ids: &[i64]) -> ApiResult<()> {
    if ids.len() > 96 || ids.iter().any(|id| *id <= 0) {
        return Err(ApiError::bad_request("term_ids không hợp lệ."));
    }
    let mut unique = std::collections::HashSet::new();
    let (mut categories, mut tags) = (0_usize, 0_usize);
    for &id in ids {
        if !unique.insert(id) {
            return Err(ApiError::bad_request("term_ids không được trùng lặp."));
        }
        let kind: Option<String> =
            sqlx::query_scalar("SELECT kind FROM taxonomy_terms WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut **tx)
                .await?;
        match kind.as_deref() {
            Some("category") => categories += 1,
            Some("tag") => tags += 1,
            _ => return Err(ApiError::bad_request("Category/Tag ID không tồn tại.")),
        }
        if categories > 32 || tags > 64 {
            return Err(ApiError::bad_request(
                "Mỗi entry tối đa 32 category và 64 tag.",
            ));
        }
    }
    Ok(())
}

async fn replace_term_ids(tx: &mut Transaction<'_, Sqlite>, id: i64, ids: &[i64]) -> ApiResult<()> {
    sqlx::query("DELETE FROM entry_terms WHERE entry_id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    for &term_id in ids {
        sqlx::query("INSERT INTO entry_terms (entry_id, term_id) VALUES (?, ?)")
            .bind(id)
            .bind(term_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn current_entry_in_write(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    id: i64,
    expected_version: i64,
) -> ApiResult<()> {
    let version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    match version {
        None => Err(ApiError::not_found("Không tìm thấy nội dung.")),
        Some(v) if v != expected_version => Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        )),
        Some(_) => Ok(()),
    }
}

pub async fn get_entry(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
) -> ApiResult<Option<ContentEntryRecord>> {
    let mut tx = db.begin().await?;
    let row: Option<EntryRow> = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at
         FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let entry = match row {
        Some(row) => Some(decoded_with_terms(&mut tx, row).await?),
        None => None,
    };
    tx.commit().await?;
    Ok(entry)
}

pub async fn list_entries(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
) -> ApiResult<Vec<ContentEntryRecord>> {
    list_entries_filtered(db, type_key, limit, offset, filters, false).await
}

/// Reusable published-only filtering contract for P06 and the P02 public
/// renderer handoff. Draft exclusion runs in SQL before LIMIT/OFFSET.
#[allow(dead_code)] // Called by public_content after Forge P02's merge/handoff.
pub async fn list_published_filtered(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
) -> ApiResult<Vec<ContentEntryRecord>> {
    list_entries_filtered(db, type_key, limit, offset, filters, true).await
}

async fn list_entries_filtered(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
    published_only: bool,
) -> ApiResult<Vec<ContentEntryRecord>> {
    let mut tx = db.begin().await?;
    for (id, kind) in [(filters.category_id, "category"), (filters.tag_id, "tag")] {
        if let Some(id) = id {
            if id <= 0 {
                return Err(ApiError::bad_request("Bộ lọc taxonomy không hợp lệ."));
            }
            let actual: Option<String> =
                sqlx::query_scalar("SELECT kind FROM taxonomy_terms WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if actual.as_deref() != Some(kind) {
                return Err(ApiError::bad_request(
                    "Category/Tag trong bộ lọc không tồn tại hoặc sai loại.",
                ));
            }
        }
    }
    let rows: Vec<EntryRow> = sqlx::query_as(
        "WITH RECURSIVE category_scope(id, depth) AS (
             SELECT id, 1 FROM taxonomy_terms WHERE id = ? AND kind = 'category'
             UNION ALL
             SELECT child.id, category_scope.depth + 1
             FROM taxonomy_terms child JOIN category_scope ON child.parent_id = category_scope.id
             WHERE category_scope.depth < 8 AND ? = 1
         )
         SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json, e.publication_status,
                e.version, e.created_at, e.updated_at
           FROM content_entries e
          WHERE e.content_type_key = ?
            AND (? = 0 OR e.publication_status = 'published')
            AND (? IS NULL OR EXISTS (
                SELECT 1 FROM entry_terms et
                 WHERE et.entry_id = e.id AND et.term_id IN (SELECT id FROM category_scope)
            ))
            AND (? IS NULL OR EXISTS (
                SELECT 1 FROM entry_terms et WHERE et.entry_id = e.id AND et.term_id = ?
            ))
          ORDER BY e.id DESC LIMIT ? OFFSET ?",
    )
    .bind(filters.category_id)
    .bind(i64::from(filters.include_descendants))
    .bind(type_key)
    .bind(i64::from(published_only))
    .bind(filters.category_id)
    .bind(filters.tag_id)
    .bind(filters.tag_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await?;
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        entries.push(decoded_with_terms(&mut tx, row).await?);
    }
    tx.commit().await?;
    Ok(entries)
}

pub async fn create_entry(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    input: &ContentEntryInput,
) -> ApiResult<ContentEntryRecord> {
    let mut tx = begin_entry_write(db, actor).await?;
    if let Some(ids) = &input.term_ids {
        validate_term_ids(&mut tx, ids).await?;
    }
    let json = serde_json::to_string(&input.values)?;
    let result = sqlx::query(
        "INSERT INTO content_entries (content_type_key, slug, title, values_json, publication_status)
         VALUES (?, ?, ?, ?, CASE WHEN ? IN ('post', 'page') THEN 'draft' ELSE 'published' END)",
    )
    .bind(type_key)
    .bind(&input.slug)
    .bind(&input.title)
    .bind(json)
    .bind(type_key)
    .execute(&mut *tx)
    .await;
    let id = match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            return Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."));
        }
        other => other?.last_insert_rowid(),
    };
    if let Some(ids) = &input.term_ids {
        replace_term_ids(&mut tx, id, ids).await?;
    }
    let row: EntryRow = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at
         FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let record = decoded_with_terms(&mut tx, row).await?;
    tx.commit().await?;
    Ok(record)
}

pub async fn update_entry(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
    expected_version: i64,
    input: &ContentEntryInput,
) -> ApiResult<ContentEntryRecord> {
    let mut tx = begin_entry_write(db, actor).await?;
    current_entry_in_write(&mut tx, type_key, id, expected_version).await?;
    if let Some(ids) = &input.term_ids {
        validate_term_ids(&mut tx, ids).await?;
    }
    let json = serde_json::to_string(&input.values)?;
    let result = sqlx::query(
        "UPDATE content_entries
         SET slug = ?, title = ?, values_json = ?, version = version + 1,
             updated_at = datetime('now')
         WHERE content_type_key = ? AND id = ? AND version = ?",
    )
    .bind(&input.slug)
    .bind(&input.title)
    .bind(json)
    .bind(type_key)
    .bind(id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await;
    match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."))
        }
        other => {
            if other?.rows_affected() == 0 {
                return Err(ApiError::precondition_failed(
                    "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
                ));
            }
            if let Some(ids) = &input.term_ids {
                replace_term_ids(&mut tx, id, ids).await?;
            }
            let row: EntryRow = sqlx::query_as(
                "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at
                 FROM content_entries WHERE content_type_key = ? AND id = ?",
            )
            .bind(type_key)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            let record = decoded_with_terms(&mut tx, row).await?;
            tx.commit().await?;
            Ok(record)
        }
    }
}

pub async fn delete_entry(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
    expected_version: i64,
) -> ApiResult<()> {
    let result = sqlx::query(
        "DELETE FROM content_entries
         WHERE content_type_key = ? AND id = ? AND version = ?",
    )
    .bind(type_key)
    .bind(id)
    .bind(expected_version)
    .execute(db)
    .await?;
    if result.rows_affected() == 0 {
        return Err(missing_or_stale(db, type_key, id).await?);
    }
    Ok(())
}

/// Version-CAS transition. Save/update never touches publication_status.
pub async fn set_publication_status(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
    expected_version: i64,
    status: &str,
) -> ApiResult<ContentEntryRecord> {
    let result = sqlx::query(
        "UPDATE content_entries
         SET publication_status = ?, version = version + 1, updated_at = datetime('now')
         WHERE content_type_key = ? AND id = ? AND version = ?
           AND content_type_key IN ('post', 'page')",
    )
    .bind(status)
    .bind(type_key)
    .bind(id)
    .bind(expected_version)
    .execute(db)
    .await?;
    if result.rows_affected() == 0 {
        return Err(missing_or_stale(db, type_key, id).await?);
    }
    get_entry(db, type_key, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải trạng thái xuất bản."))
}

async fn missing_or_stale(db: &SqlitePool, type_key: &str, id: i64) -> ApiResult<ApiError> {
    if get_entry(db, type_key, id).await?.is_some() {
        Ok(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        ))
    } else {
        Ok(ApiError::not_found("Không tìm thấy nội dung."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn published_taxonomy_filter_excludes_drafts_before_pagination() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let root = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug) VALUES('category','Root','root')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let child = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,parent_id) VALUES('category','Child','child',?)",
        )
        .bind(root)
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let tag =
            sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug) VALUES('tag','Tag','tag')")
                .execute(&db)
                .await
                .unwrap()
                .last_insert_rowid();

        let mut ids = vec![];
        for (slug, is_published) in [
            ("published-first", true),
            ("draft-newer", false),
            ("published-last", true),
        ] {
            let id = sqlx::query(
                "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
                 VALUES('post',?,'Safe','{}','draft')",
            )
            .bind(slug)
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();
            if is_published {
                sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
                    .bind(id)
                    .execute(&db)
                    .await
                    .unwrap();
            }
            // More than one matching category link to one entry must never
            // inflate public results or displace a later page.
            for term in [root, child, tag] {
                sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES (?,?)")
                    .bind(id)
                    .bind(term)
                    .execute(&db)
                    .await
                    .unwrap();
            }
            ids.push(id);
        }
        let filters = EntryFilters {
            category_id: Some(root),
            tag_id: Some(tag),
            include_descendants: true,
        };
        let first = list_published_filtered(&db, "post", 1, 0, filters)
            .await
            .unwrap();
        let second = list_published_filtered(&db, "post", 1, 1, filters)
            .await
            .unwrap();
        let last = list_published_filtered(&db, "post", 1, 2, filters)
            .await
            .unwrap();
        assert_eq!(first.iter().map(|e| e.id).collect::<Vec<_>>(), [ids[2]]);
        assert_eq!(second.iter().map(|e| e.id).collect::<Vec<_>>(), [ids[0]]);
        assert!(last.is_empty());
        assert_eq!(first[0].term_ids, [root, child, tag]);
        let admin = list_entries(&db, "post", 10, 0, filters).await.unwrap();
        assert_eq!(
            admin.len(),
            3,
            "Admin sees unpublished draft before pagination"
        );

        // Reparented category links retain stable IDs and public filters must
        // follow the current tree, not stale slug/path snapshots.
        let other = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug) VALUES('category','Other','other')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        for id in &ids {
            sqlx::query("DELETE FROM entry_terms WHERE entry_id=? AND term_id=?")
                .bind(id)
                .bind(root)
                .execute(&db)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE taxonomy_terms SET parent_id=? WHERE id=?")
            .bind(other)
            .bind(child)
            .execute(&db)
            .await
            .unwrap();
        assert!(
            list_published_filtered(&db, "post", 10, 0, filters)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            list_published_filtered(
                &db,
                "post",
                10,
                0,
                EntryFilters {
                    category_id: Some(other),
                    ..filters
                }
            )
            .await
            .unwrap()
            .len(),
            2
        );
    }
}
