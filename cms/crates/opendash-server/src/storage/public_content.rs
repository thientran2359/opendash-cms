//! Read-only public content lookup and media reference ACL.
//! Public controllers never issue SQL directly.

use sqlx::SqlitePool;

use super::entries::{ContentEntryRecord, decode_row};
use crate::error::ApiResult;

/// Slug is unique within each CPT; lookup never relies on a numeric admin ID.
pub async fn get_by_slug(
    db: &SqlitePool,
    type_key: &str,
    slug: &str,
) -> ApiResult<Option<ContentEntryRecord>> {
    let row: Option<(i64, String, String, String, String, i64, String, String)> = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, version, created_at, updated_at
         FROM content_entries WHERE content_type_key = ? AND slug = ?",
    )
    .bind(type_key)
    .bind(slug)
    .fetch_optional(db)
    .await?;
    row.map(decode_row).transpose()
}

/// Images become public only while referenced by an image/gallery custom field
/// of an existing CPT entry. A private unreferenced upload remains 404 publicly.
/// JSON paths are derived from validated field keys persisted in CPT schemas.
pub async fn media_is_referenced(db: &SqlitePool, id: &str) -> ApiResult<bool> {
    let result: i64 = sqlx::query_scalar(
        r#"SELECT EXISTS (
           SELECT 1
           FROM content_entries AS e
           JOIN content_types AS t ON t.key = e.content_type_key
           JOIN json_each(t.definition_json, '$.fields') AS f
           WHERE
             (
               json_extract(f.value, '$.type') = 'image'
               AND json_extract(e.values_json, '$.' || json_extract(f.value, '$.key')) = ?
             )
             OR (
               json_extract(f.value, '$.type') = 'gallery'
               AND EXISTS (
                 SELECT 1
                 FROM json_each(json_extract(e.values_json, '$.' || json_extract(f.value, '$.key'))) AS item
                 WHERE item.value = ?
               )
             )
           LIMIT 1
         )"#,
    )
    .bind(id)
    .bind(id)
    .fetch_one(db)
    .await?;
    Ok(result != 0)
}
