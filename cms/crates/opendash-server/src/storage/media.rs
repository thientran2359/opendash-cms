//! Parameterized media metadata persistence and referential checks for entries.

use opendash_core::content::{ContentEntryInput, ContentFieldType, ContentTypeDefinition};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::{ApiError, ApiResult};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MediaAsset {
    pub id: String,
    pub kind: String,
    pub filename: String,
    pub mime_type: String,
    pub extension: String,
    pub byte_size: i64,
    pub width: i64,
    pub height: i64,
    pub thumb_width: Option<i64>,
    pub thumb_height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub created_at: String,
}

pub async fn find(db: &SqlitePool, id: &str) -> ApiResult<Option<MediaAsset>> {
    Ok(sqlx::query_as::<_, MediaAsset>(
        "SELECT id, kind, filename, mime_type, extension, byte_size, width, height,
                thumb_width, thumb_height, duration_ms, video_codec, audio_codec, created_at
         FROM media_assets WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?)
}

pub async fn list(db: &SqlitePool, limit: i64, offset: i64) -> ApiResult<Vec<MediaAsset>> {
    list_filtered(db, None, limit, offset).await
}

pub async fn list_filtered(
    db: &SqlitePool,
    kind: Option<&str>,
    limit: i64,
    offset: i64,
) -> ApiResult<Vec<MediaAsset>> {
    if let Some(kind) = kind {
        Ok(sqlx::query_as::<_, MediaAsset>(
            "SELECT id, kind, filename, mime_type, extension, byte_size, width, height,
                    thumb_width, thumb_height, duration_ms, video_codec, audio_codec, created_at
             FROM media_assets
             WHERE kind = ?
             ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
        )
        .bind(kind)
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await?)
    } else {
        Ok(sqlx::query_as::<_, MediaAsset>(
            "SELECT id, kind, filename, mime_type, extension, byte_size, width, height,
                    thumb_width, thumb_height, duration_ms, video_codec, audio_codec, created_at
             FROM media_assets
             ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await?)
    }
}

pub async fn save(db: &SqlitePool, asset: &MediaAsset) -> ApiResult<MediaAsset> {
    sqlx::query(
        "INSERT INTO media_assets
         (id, kind, filename, mime_type, extension, byte_size, width, height,
          thumb_width, thumb_height, duration_ms, video_codec, audio_codec)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO NOTHING",
    )
    .bind(&asset.id)
    .bind(&asset.kind)
    .bind(&asset.filename)
    .bind(&asset.mime_type)
    .bind(&asset.extension)
    .bind(asset.byte_size)
    .bind(asset.width)
    .bind(asset.height)
    .bind(asset.thumb_width)
    .bind(asset.thumb_height)
    .bind(asset.duration_ms)
    .bind(&asset.video_codec)
    .bind(&asset.audio_codec)
    .execute(db)
    .await?;
    find(db, &asset.id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải media vừa lưu."))
}

fn collect_image_ids<'a>(
    schema: &ContentTypeDefinition,
    input: &'a ContentEntryInput,
) -> Vec<&'a str> {
    let mut ids = Vec::new();
    for field in &schema.fields {
        let Some(value) = input.values.get(&field.key) else {
            continue;
        };
        match field.field_type {
            ContentFieldType::Image => {
                if let Some(s) = value.as_str() {
                    ids.push(s);
                }
            }
            ContentFieldType::Gallery => {
                if let Some(arr) = value.as_array() {
                    ids.extend(arr.iter().filter_map(|v| v.as_str()));
                }
            }
            _ => continue,
        }
    }
    ids
}

/// Existing legacy entry values are readable; new/updated entries must only
/// reference actual registered images. No path or URL is accepted from a form.
pub async fn require_existing_references(
    db: &SqlitePool,
    schema: &ContentTypeDefinition,
    input: &ContentEntryInput,
) -> ApiResult<()> {
    for id in collect_image_ids(schema, input) {
        if !valid_media_id(id)
            || !matches!(find(db, id).await?, Some(asset) if asset.kind == "image")
        {
            return Err(ApiError::bad_request(
                "Ảnh tham chiếu chưa có trong Media Library. Hãy chọn ảnh đã upload.",
            ));
        }
    }
    Ok(())
}

pub async fn require_existing_references_in_tx(
    conn: &mut sqlx::SqliteConnection,
    schema: &ContentTypeDefinition,
    input: &ContentEntryInput,
) -> ApiResult<()> {
    for id in collect_image_ids(schema, input) {
        if !valid_media_id(id) {
            return Err(ApiError::bad_request(
                "Ảnh tham chiếu chưa có trong Media Library. Hãy chọn ảnh đã upload.",
            ));
        }
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM media_assets WHERE id = ? AND kind = 'image'")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?;
        if exists.is_none() {
            return Err(ApiError::bad_request(
                "Ảnh tham chiếu chưa có trong Media Library. Hãy chọn ảnh đã upload.",
            ));
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
