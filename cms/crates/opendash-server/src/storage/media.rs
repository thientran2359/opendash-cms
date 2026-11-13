//! Parameterized media metadata persistence and referential checks for entries.

use opendash_core::content::{ContentEntryInput, ContentFieldType, ContentTypeDefinition};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::{ApiError, ApiResult};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MediaAsset {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub extension: String,
    pub byte_size: i64,
    pub width: i64,
    pub height: i64,
    pub thumb_width: i64,
    pub thumb_height: i64,
    pub created_at: String,
}

pub async fn find(db: &SqlitePool, id: &str) -> ApiResult<Option<MediaAsset>> {
    Ok(sqlx::query_as::<_, MediaAsset>(
        "SELECT id, filename, mime_type, extension, byte_size, width, height,
                thumb_width, thumb_height, created_at FROM media_assets WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?)
}

pub async fn list(db: &SqlitePool, limit: i64, offset: i64) -> ApiResult<Vec<MediaAsset>> {
    Ok(sqlx::query_as::<_, MediaAsset>(
        "SELECT id, filename, mime_type, extension, byte_size, width, height,
                thumb_width, thumb_height, created_at FROM media_assets
         ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?)
}

pub async fn save(db: &SqlitePool, asset: &MediaAsset) -> ApiResult<MediaAsset> {
    sqlx::query(
        "INSERT INTO media_assets
         (id, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO NOTHING",
    )
    .bind(&asset.id)
    .bind(&asset.filename)
    .bind(&asset.mime_type)
    .bind(&asset.extension)
    .bind(asset.byte_size)
    .bind(asset.width)
    .bind(asset.height)
    .bind(asset.thumb_width)
    .bind(asset.thumb_height)
    .execute(db)
    .await?;
    find(db, &asset.id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải ảnh vừa lưu."))
}

/// Existing legacy entry values are readable; new/updated entries must only
/// reference actual registered images. No path or URL is accepted from a form.
pub async fn require_existing_references(
    db: &SqlitePool,
    schema: &ContentTypeDefinition,
    input: &ContentEntryInput,
) -> ApiResult<()> {
    for field in &schema.fields {
        let Some(value) = input.values.get(&field.key) else {
            continue;
        };
        let ids: Vec<&str> = match field.field_type {
            ContentFieldType::Image => value.as_str().into_iter().collect(),
            ContentFieldType::Gallery => value.as_array().map_or_else(Vec::new, |values| {
                values.iter().filter_map(|value| value.as_str()).collect()
            }),
            _ => continue,
        };
        for id in ids {
            if !valid_media_id(id) || find(db, id).await?.is_none() {
                return Err(ApiError::bad_request(
                    "Ảnh tham chiếu chưa có trong Media Library. Hãy chọn ảnh đã upload.",
                ));
            }
        }
    }
    Ok(())
}


pub async fn require_existing_references_in_tx(
    conn: &mut sqlx::SqliteConnection,
    schema: &ContentTypeDefinition,
    input: &ContentEntryInput,
) -> ApiResult<()> {
    for field in &schema.fields {
        let Some(value) = input.values.get(&field.key) else {
            continue;
        };
        let ids: Vec<&str> = match field.field_type {
            ContentFieldType::Image => value.as_str().into_iter().collect(),
            ContentFieldType::Gallery => value.as_array().map_or_else(Vec::new, |values| {
                values.iter().filter_map(|value| value.as_str()).collect()
            }),
            _ => continue,
        };
        for id in ids {
            if !valid_media_id(id) {
                return Err(ApiError::bad_request(
                    "Ảnh tham chiếu chưa có trong Media Library. Hãy chọn ảnh đã upload.",
                ));
            }
            let exists: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM media_assets WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&mut *conn)
                    .await?;
            if exists.is_none() {
                return Err(ApiError::bad_request(
                    "Ảnh tham chiếu chưa có trong Media Library. Hãy chọn ảnh đã upload.",
                ));
            }
        }
    }
    Ok(())
}

pub fn valid_media_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
