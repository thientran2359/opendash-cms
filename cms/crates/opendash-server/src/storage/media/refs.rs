use std::collections::BTreeMap;

use opendash_core::content::{
    ContentEntryInput, ContentFieldType, ContentTypeDefinition, EffectiveContentSchema,
};
use serde_json::Value;
use sqlx::SqlitePool;

use super::queries::{find, valid_media_id};
use crate::error::{ApiError, ApiResult};

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
