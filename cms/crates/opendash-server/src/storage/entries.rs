//! CRUD for saved content entries. Never puts SQL inside HTTP routes.

use std::collections::BTreeMap;

use opendash_core::content::ContentEntryInput;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::error::{ApiError, ApiResult};

#[derive(Debug, Clone, Serialize)]
pub struct ContentEntryRecord {
    pub id: i64,
    pub content_type_key: String,
    pub slug: String,
    pub title: String,
    pub values: BTreeMap<String, Value>,
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
        publication_status,
        version,
        created_at,
        updated_at,
    })
}

pub async fn get_entry(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
) -> ApiResult<Option<ContentEntryRecord>> {
    let row: Option<EntryRow> = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at
         FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_optional(db)
    .await?;
    row.map(decode_row).transpose()
}

pub async fn list_entries(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
) -> ApiResult<Vec<ContentEntryRecord>> {
    let rows: Vec<EntryRow> = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at
         FROM content_entries WHERE content_type_key = ?
         ORDER BY id DESC LIMIT ? OFFSET ?",
    )
    .bind(type_key)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;
    rows.into_iter().map(decode_row).collect()
}

pub async fn create_entry(
    db: &SqlitePool,
    type_key: &str,
    input: &ContentEntryInput,
) -> ApiResult<ContentEntryRecord> {
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
    .execute(db)
    .await;
    let id = match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            return Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."));
        }
        other => other?.last_insert_rowid(),
    };
    get_entry(db, type_key, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải nội dung vừa tạo."))
}

pub async fn update_entry(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
    expected_version: i64,
    input: &ContentEntryInput,
) -> ApiResult<ContentEntryRecord> {
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
    .execute(db)
    .await;
    match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."))
        }
        other => {
            if other?.rows_affected() == 0 {
                return Err(missing_or_stale(db, type_key, id).await?);
            }
            get_entry(db, type_key, id)
                .await?
                .ok_or_else(|| ApiError::internal("Không thể tải nội dung vừa cập nhật."))
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
