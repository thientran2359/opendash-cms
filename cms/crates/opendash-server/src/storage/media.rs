//! Parameterized media metadata persistence and referential checks for entries.

use std::collections::BTreeMap;

use opendash_core::content::{
    ContentEntryInput, ContentFieldType, ContentTypeDefinition, EffectiveContentSchema,
};
use serde::Serialize;
use serde_json::Value;
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
    pub lifecycle_state: String,
    pub generation: i64,
    pub version: i64,
    pub created_at: String,
}

impl MediaAsset {
    pub fn metadata_etag(&self) -> String {
        format!("gv{}-v{}", self.generation, self.version)
    }
}

pub async fn find(db: &SqlitePool, id: &str) -> ApiResult<Option<MediaAsset>> {
    Ok(sqlx::query_as::<_, MediaAsset>(
        "SELECT id, kind, filename, mime_type, extension, byte_size, width, height,
                thumb_width, thumb_height, duration_ms, video_codec, audio_codec,
                lifecycle_state, generation, version, created_at
         FROM media_assets WHERE id = ? AND lifecycle_state = 'active'",
    )
    .bind(id)
    .fetch_optional(db)
    .await?)
}

#[allow(dead_code)]
pub async fn find_any_state(db: &SqlitePool, id: &str) -> ApiResult<Option<MediaAsset>> {
    Ok(sqlx::query_as::<_, MediaAsset>(
        "SELECT id, kind, filename, mime_type, extension, byte_size, width, height,
                thumb_width, thumb_height, duration_ms, video_codec, audio_codec,
                lifecycle_state, generation, version, created_at
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
                    thumb_width, thumb_height, duration_ms, video_codec, audio_codec,
                    lifecycle_state, generation, version, created_at
             FROM media_assets
             WHERE kind = ? AND lifecycle_state = 'active'
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
                    thumb_width, thumb_height, duration_ms, video_codec, audio_codec,
                    lifecycle_state, generation, version, created_at
             FROM media_assets
             WHERE lifecycle_state = 'active'
             ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await?)
    }
}

pub async fn save(db: &SqlitePool, asset: &MediaAsset) -> ApiResult<MediaAsset> {
    let mut tx = db.begin().await?;
    let existing_state: Option<String> =
        sqlx::query_scalar("SELECT lifecycle_state FROM media_assets WHERE id = ?")
            .bind(&asset.id)
            .fetch_optional(&mut *tx)
            .await?;

    if let Some(state) = existing_state {
        if state != "active" {
            return Err(ApiError::conflict(
                "Media đang trong quá trình xóa; không thể upload lại cho đến khi hoàn tất.",
            ));
        }
        tx.commit().await?;
        return find(db, &asset.id)
            .await?
            .ok_or_else(|| ApiError::internal("Không thể tải media vừa lưu."));
    }

    let pending_delete: Option<String> = sqlx::query_scalar(
        "SELECT job_id FROM media_delete_jobs WHERE media_id = ? AND phase != 'finalized'",
    )
    .bind(&asset.id)
    .fetch_optional(&mut *tx)
    .await?;
    if pending_delete.is_some() {
        return Err(ApiError::conflict(
            "Media đang chờ hoàn tất xóa; không thể upload lại cho đến khi finalized.",
        ));
    }

    let generation: i64 = sqlx::query_scalar(
        "INSERT INTO media_generations (media_id, next_generation)
         VALUES (?, 2)
         ON CONFLICT(media_id) DO UPDATE SET next_generation = next_generation + 1
         RETURNING next_generation - 1",
    )
    .bind(&asset.id)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO media_assets
         (id, kind, filename, mime_type, extension, byte_size, width, height,
          thumb_width, thumb_height, duration_ms, video_codec, audio_codec,
          lifecycle_state, generation, version)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, 1)",
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
    .bind(generation)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

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

pub fn collect_effective_image_ids<'a>(
    effective: &EffectiveContentSchema,
    values: &'a BTreeMap<String, Value>,
) -> Vec<&'a str> {
    let mut ids = Vec::new();
    for (key, entry) in &effective.effective_fields {
        let Some(value) = values.get(key) else {
            continue;
        };
        match entry.definition.field_type {
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

pub async fn require_existing_effective_references(
    db: &SqlitePool,
    effective: &EffectiveContentSchema,
    values: &BTreeMap<String, Value>,
) -> ApiResult<()> {
    for id in collect_effective_image_ids(effective, values) {
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
        verify_active_image_in_tx(conn, id).await?;
    }
    Ok(())
}

pub async fn require_existing_effective_references_in_tx(
    conn: &mut sqlx::SqliteConnection,
    effective: &EffectiveContentSchema,
    values: &BTreeMap<String, Value>,
) -> ApiResult<()> {
    for id in collect_effective_image_ids(effective, values) {
        verify_active_image_in_tx(conn, id).await?;
    }
    Ok(())
}

async fn verify_active_image_in_tx(conn: &mut sqlx::SqliteConnection, id: &str) -> ApiResult<()> {
    if !valid_media_id(id) {
        return Err(ApiError::bad_request(
            "Ảnh tham chiếu chưa có trong Media Library. Hãy chọn ảnh đã upload.",
        ));
    }
    let state: Option<String> = sqlx::query_scalar(
        "SELECT lifecycle_state FROM media_assets WHERE id = ? AND kind = 'image'",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    match state.as_deref() {
        Some("active") => Ok(()),
        Some(_) => Err(ApiError::conflict(
            "Ảnh tham chiếu đang trong quá trình xóa; không thể tạo tham chiếu mới.",
        )),
        None => Err(ApiError::bad_request(
            "Ảnh tham chiếu chưa có trong Media Library. Hãy chọn ảnh đã upload.",
        )),
    }
}

pub fn valid_media_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
