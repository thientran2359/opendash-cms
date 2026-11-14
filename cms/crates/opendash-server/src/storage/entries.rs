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

/// The only allowed SQL orderings for a published Builder list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PublishedListSort {
    #[default]
    IdDesc,
    IdAsc,
}

/// Missing references are a valid list configuration state, not a database failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishedListMissing {
    ContentType,
    Category,
    Tag,
}

#[derive(Debug)]
pub struct PublishedListPage {
    pub entries: Vec<ContentEntryRecord>,
    pub has_next: bool,
}

#[derive(Debug)]
pub enum PublishedListResult {
    Page(PublishedListPage),
    MissingConfig(PublishedListMissing),
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
#[derive(Debug, Clone)]
pub struct PluginActionFence {
    pub plugin_id: String,
    pub expected_generation: i64,
    pub required_role: String,
}

async fn verify_plugin_action_fence(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &SessionContext,
    fence: &PluginActionFence,
) -> ApiResult<()> {
    let active = crate::storage::plugin::is_plugin_generation_active(
        &mut **tx,
        &fence.plugin_id,
        fence.expected_generation,
    )
    .await?;
    if !active {
        return Err(ApiError::conflict(
            "Plugin đã bị vô hiệu hóa hoặc thay đổi phiên bản trong lúc thực thi.",
        ));
    }
    let role_ok = crate::storage::plugin::is_actor_role_active(
        &mut **tx,
        actor.user.id,
        &fence.required_role,
    )
    .await?;
    if !role_ok {
        return Err(ApiError::forbidden("Quyền của tài khoản đã bị thu hồi."));
    }
    Ok(())
}

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

/// Reusable published-only filtering contract for public archives and P06.
/// Draft exclusion runs in SQL before LIMIT/OFFSET.
pub async fn list_published_filtered(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
) -> ApiResult<Vec<ContentEntryRecord>> {
    list_entries_filtered(db, type_key, limit, offset, filters, true).await
}

/// Query one published-only Builder list page on a consistent SQLite read
/// snapshot. Missing CPT/terms are distinct from actual storage errors, so
/// the Builder can preserve configuration and display a diagnostic.
pub async fn list_published_page(
    db: &SqlitePool,
    type_key: &str,
    page: i64,
    page_size: i64,
    sort: PublishedListSort,
    filters: EntryFilters,
) -> ApiResult<PublishedListResult> {
    let mut tx = db.begin().await?;
    let result =
        list_published_page_in_tx(&mut tx, type_key, page, page_size, sort, filters).await?;
    tx.commit().await?;
    Ok(result)
}

/// #106: Use the same #105 predicate and limits within an existing Builder
/// read/write snapshot. Never issue a second independent query on Save.
pub async fn list_published_page_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    page: i64,
    page_size: i64,
    sort: PublishedListSort,
    filters: EntryFilters,
) -> ApiResult<PublishedListResult> {
    if !(1..=100).contains(&page) || !(1..=20).contains(&page_size) {
        return Err(ApiError::bad_request(
            "Trang danh sách phải từ 1–100 và page_size từ 1–20.",
        ));
    }
    if filters.include_descendants && filters.category_id.is_none() {
        return Err(ApiError::bad_request(
            "include_descendants yêu cầu category_id.",
        ));
    }

    let type_exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM content_types WHERE key = ? AND provider_status = 'active'",
    )
    .bind(type_key)
    .fetch_optional(&mut **tx)
    .await?;
    if type_exists.is_none() {
        return Ok(PublishedListResult::MissingConfig(
            PublishedListMissing::ContentType,
        ));
    }

    for (id, kind, missing) in [
        (
            filters.category_id,
            "category",
            PublishedListMissing::Category,
        ),
        (filters.tag_id, "tag", PublishedListMissing::Tag),
    ] {
        if let Some(id) = id {
            if id <= 0 {
                return Err(ApiError::bad_request("Bộ lọc taxonomy không hợp lệ."));
            }
            let actual: Option<String> =
                sqlx::query_scalar("SELECT kind FROM taxonomy_terms WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&mut **tx)
                    .await?;
            if actual.as_deref() != Some(kind) {
                return Ok(PublishedListResult::MissingConfig(missing));
            }
        }
    }

    // One extra row tells the renderer whether to expose a next-page link.
    // Bounds above guarantee no integer overflow or unbounded fetch.
    let offset = (page - 1) * page_size;
    let mut entries =
        query_entries_filtered(tx, type_key, page_size + 1, offset, filters, true, sort).await?;
    // ADR-0011 stops pagination at page 100 even if more records exist.
    let has_next = page < 100 && entries.len() > page_size as usize;
    entries.truncate(page_size as usize);
    Ok(PublishedListResult::Page(PublishedListPage {
        entries,
        has_next,
    }))
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
    let entries = query_entries_filtered(
        &mut tx,
        type_key,
        limit,
        offset,
        filters,
        published_only,
        PublishedListSort::IdDesc,
    )
    .await?;
    tx.commit().await?;
    Ok(entries)
}

/// Shared category/tag AND predicates for the old archives and new list
/// blocks. Only an internal enum can select the ORDER BY direction.
async fn query_entries_filtered(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
    published_only: bool,
    sort: PublishedListSort,
) -> ApiResult<Vec<ContentEntryRecord>> {
    let direction = match sort {
        PublishedListSort::IdDesc => "DESC",
        PublishedListSort::IdAsc => "ASC",
    };
    // Audited dynamic SQL: direction is a closed Rust enum mapped to one of
    // two literal keywords; all user-controlled values remain bound params.
    let statement = format!(
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
          ORDER BY e.id {direction} LIMIT ? OFFSET ?"
    );
    let rows: Vec<EntryRow> = sqlx::query_as(sqlx::AssertSqlSafe(statement))
        .bind(filters.category_id)
        .bind(i64::from(filters.include_descendants))
        .bind(type_key)
        .bind(i64::from(published_only))
        .bind(filters.category_id)
        .bind(filters.tag_id)
        .bind(filters.tag_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut **tx)
        .await?;
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        entries.push(decoded_with_terms(tx, row).await?);
    }
    Ok(entries)
}

pub async fn create_entry(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &opendash_core::content::ContentTypeDefinition,
    input: &ContentEntryInput,
) -> ApiResult<ContentEntryRecord> {
    create_entry_with_fence(db, actor, type_key, schema, input, None).await
}

pub async fn create_entry_with_fence(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &opendash_core::content::ContentTypeDefinition,
    input: &ContentEntryInput,
    fence: Option<&PluginActionFence>,
) -> ApiResult<ContentEntryRecord> {
    let mut tx = begin_entry_write(db, actor).await?;
    if let Some(f) = fence {
        verify_plugin_action_fence(&mut tx, actor, f).await?;
    }
    crate::storage::seo::check_default_slug_claim_in_tx(&mut tx, type_key, &input.slug, None).await?;
    crate::storage::media::require_existing_references_in_tx(&mut tx, schema, input).await?;
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
    schema: &opendash_core::content::ContentTypeDefinition,
    id: i64,
    expected_version: i64,
    input: &ContentEntryInput,
) -> ApiResult<ContentEntryRecord> {
    update_entry_with_fence(db, actor, type_key, schema, id, expected_version, input, None).await
}

#[allow(clippy::too_many_arguments)]
pub async fn update_entry_with_fence(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &opendash_core::content::ContentTypeDefinition,
    id: i64,
    expected_version: i64,
    input: &ContentEntryInput,
    fence: Option<&PluginActionFence>,
) -> ApiResult<ContentEntryRecord> {
    let mut tx = begin_entry_write(db, actor).await?;
    if let Some(f) = fence {
        verify_plugin_action_fence(&mut tx, actor, f).await?;
    }
    current_entry_in_write(&mut tx, type_key, id, expected_version).await?;
    let old_slug: String = sqlx::query_scalar(
        "SELECT slug FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    crate::storage::seo::check_default_slug_claim_in_tx(&mut tx, type_key, &input.slug, Some(id)).await?;
    crate::storage::media::require_existing_references_in_tx(&mut tx, schema, input).await?;
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
            crate::storage::seo::record_slug_change_redirect_in_tx(&mut tx, id, type_key, &old_slug, &input.slug).await?;
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
    let mut tx = db.begin().await?;
    let result = sqlx::query(
        "DELETE FROM content_entries
         WHERE content_type_key = ? AND id = ? AND version = ?",
    )
    .bind(type_key)
    .bind(id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Err(missing_or_stale(db, type_key, id).await?);
    }
    sqlx::query("DELETE FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
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
    let mut tx = db.begin().await?;
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
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Err(missing_or_stale(db, type_key, id).await?);
    }
    crate::storage::seo::sync_entry_seo_media_ref_in_tx(&mut tx, id).await?;
    tx.commit().await?;
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

    async fn list_test_entry(
        db: &SqlitePool,
        type_key: &str,
        slug: &str,
        published: bool,
        term_ids: &[i64],
    ) -> i64 {
        let id = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
             VALUES(?,?,'Safe','{}','draft')",
        )
        .bind(type_key)
        .bind(slug)
        .execute(db)
        .await
        .unwrap()
        .last_insert_rowid();
        if published {
            sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = ?")
                .bind(id)
                .execute(db)
                .await
                .unwrap();
        }
        for &term in term_ids {
            sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES (?,?)")
                .bind(id)
                .bind(term)
                .execute(db)
                .await
                .unwrap();
        }
        id
    }

    fn expect_list_page(result: PublishedListResult) -> PublishedListPage {
        match result {
            PublishedListResult::Page(page) => page,
            PublishedListResult::MissingConfig(reason) => {
                panic!("unexpected missing list configuration: {reason:?}")
            }
        }
    }

    #[tokio::test]
    async fn builder_published_list_filters_before_paging_for_post_page_cpt() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        // Migration 0008 seeds Post/Page; only the custom CPT is new here.
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind)
             VALUES('news',json_object('key','news','fields',json_array()),'user')",
        )
        .execute(&db)
        .await
        .unwrap();
        let root = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug) VALUES('category','Root','list-root')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let child = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,parent_id)
             VALUES('category','Child','list-child',?)",
        )
        .bind(root)
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let tag = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug) VALUES('tag','Tag','list-tag')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let other_tag = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug) VALUES('tag','Other','other-tag')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();

        let first = list_test_entry(&db, "post", "p1", true, &[root, child, tag]).await;
        list_test_entry(&db, "post", "draft", false, &[child, tag]).await;
        let second = list_test_entry(&db, "post", "p2", true, &[root, tag]).await;
        list_test_entry(&db, "post", "wrong-tag", true, &[child, other_tag]).await;
        let third = list_test_entry(&db, "post", "p3", true, &[child, tag]).await;
        let page_id = list_test_entry(&db, "page", "page1", true, &[child, tag]).await;
        let cpt_id = list_test_entry(&db, "news", "news1", true, &[child, tag]).await;
        list_test_entry(&db, "news", "draft-news", false, &[child, tag]).await;

        let filters = EntryFilters {
            category_id: Some(root),
            tag_id: Some(tag),
            include_descendants: true,
        };
        for (sort, expected) in [
            (PublishedListSort::IdDesc, [third, second, first]),
            (PublishedListSort::IdAsc, [first, second, third]),
        ] {
            for (i, &id) in expected.iter().enumerate() {
                let page = expect_list_page(
                    list_published_page(&db, "post", i as i64 + 1, 1, sort, filters)
                        .await
                        .unwrap(),
                );
                assert_eq!(page.entries.iter().map(|e| e.id).collect::<Vec<_>>(), [id]);
                assert_eq!(page.has_next, i < 2);
            }
        }

        let exact = expect_list_page(
            list_published_page(
                &db,
                "post",
                1,
                20,
                PublishedListSort::IdAsc,
                EntryFilters {
                    include_descendants: false,
                    ..filters
                },
            )
            .await
            .unwrap(),
        );
        assert_eq!(
            exact.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            [first, second],
            "exact category only; a duplicated direct+descendant link cannot duplicate rows"
        );
        assert!(!exact.has_next);
        for (type_key, id) in [("page", page_id), ("news", cpt_id)] {
            let result = expect_list_page(
                list_published_page(&db, type_key, 1, 20, PublishedListSort::IdDesc, filters)
                    .await
                    .unwrap(),
            );
            assert_eq!(
                result.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
                [id]
            );
            assert!(!result.has_next);
        }

        let past_end = expect_list_page(
            list_published_page(&db, "post", 100, 1, PublishedListSort::IdAsc, filters)
                .await
                .unwrap(),
        );
        assert!(past_end.entries.is_empty());
        assert!(!past_end.has_next);

        // Existing public archive contract stays descending with offset paging.
        let old = list_published_filtered(&db, "post", 2, 1, filters)
            .await
            .unwrap();
        assert_eq!(
            old.iter().map(|e| e.id).collect::<Vec<_>>(),
            [second, first]
        );
    }

    #[tokio::test]
    async fn builder_list_rejects_bad_bounds_and_distinguishes_missing_config() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind)
             VALUES('news',json_object('key','news','fields',json_array()),'user')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind)
             VALUES('deleted-cpt',json_object('key','deleted-cpt','fields',json_array()),'user')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("DELETE FROM content_types WHERE key = 'deleted-cpt'")
            .execute(&db)
            .await
            .unwrap();
        let category = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug) VALUES('category','List','list-category')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let tag = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug) VALUES('tag','List','list-tag')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        for (page, size) in [(0, 6), (101, 6), (-1, 6), (1, 0), (1, 21), (1, -1)] {
            let error = list_published_page(
                &db,
                "news",
                page,
                size,
                PublishedListSort::IdDesc,
                EntryFilters::default(),
            )
            .await
            .unwrap_err();
            assert_eq!(error.0, axum::http::StatusCode::BAD_REQUEST);
        }
        for filters in [
            EntryFilters {
                include_descendants: true,
                ..Default::default()
            },
            EntryFilters {
                category_id: Some(-1),
                ..Default::default()
            },
        ] {
            let error = list_published_page(&db, "news", 1, 6, PublishedListSort::IdDesc, filters)
                .await
                .unwrap_err();
            assert_eq!(error.0, axum::http::StatusCode::BAD_REQUEST);
        }
        for (type_key, filters, expected) in [
            (
                "deleted-cpt",
                EntryFilters::default(),
                PublishedListMissing::ContentType,
            ),
            (
                "news",
                EntryFilters {
                    category_id: Some(tag),
                    ..Default::default()
                },
                PublishedListMissing::Category,
            ),
            (
                "news",
                EntryFilters {
                    tag_id: Some(category),
                    ..Default::default()
                },
                PublishedListMissing::Tag,
            ),
            (
                "news",
                EntryFilters {
                    category_id: Some(999999),
                    ..Default::default()
                },
                PublishedListMissing::Category,
            ),
            (
                "news",
                EntryFilters {
                    tag_id: Some(999999),
                    ..Default::default()
                },
                PublishedListMissing::Tag,
            ),
        ] {
            let result =
                list_published_page(&db, type_key, 1, 6, PublishedListSort::IdDesc, filters)
                    .await
                    .unwrap();
            assert!(
                matches!(result, PublishedListResult::MissingConfig(reason) if reason == expected)
            );
        }

        // Deleting a previously valid, unused term must not turn into a 500.
        sqlx::query("DELETE FROM taxonomy_terms WHERE id = ?")
            .bind(tag)
            .execute(&db)
            .await
            .unwrap();
        let result = list_published_page(
            &db,
            "news",
            1,
            6,
            PublishedListSort::IdDesc,
            EntryFilters {
                tag_id: Some(tag),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            result,
            PublishedListResult::MissingConfig(PublishedListMissing::Tag)
        ));

        // Even when more than 100 pages exist, page 100 is the last linkable page.
        for i in 0..101 {
            list_test_entry(&db, "news", &format!("entry-{i}"), true, &[]).await;
        }
        let final_page = expect_list_page(
            list_published_page(
                &db,
                "news",
                100,
                1,
                PublishedListSort::IdDesc,
                EntryFilters::default(),
            )
            .await
            .unwrap(),
        );
        assert_eq!(final_page.entries.len(), 1);
        assert!(!final_page.has_next);

        db.close().await;
        let error = list_published_page(
            &db,
            "news",
            1,
            6,
            PublishedListSort::IdDesc,
            EntryFilters::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.0, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }

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
