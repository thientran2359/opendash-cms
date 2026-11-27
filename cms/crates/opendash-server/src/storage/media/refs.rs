use std::collections::BTreeMap;

use opendash_core::content::{ContentEntryInput, ContentTypeDefinition, EffectiveContentSchema};
use serde_json::Value;
use sqlx::SqlitePool;

use super::queries::{find, valid_media_id};
use crate::error::{ApiError, ApiResult};

fn collect_image_ids(schema: &ContentTypeDefinition, input: &ContentEntryInput) -> Vec<String> {
    let mut ids = Vec::new();
    for field in &schema.fields {
        if field.is_ui_only() {
            continue;
        }
        let Some(value) = input.values.get(&field.key) else {
            continue;
        };
        let collected = opendash_core::field_schema::collect_media_ids(
            std::slice::from_ref(field),
            &std::collections::BTreeMap::from([(field.key.clone(), value.clone())]),
        );
        ids.extend(collected.image_ids);
        ids.extend(collected.video_ids);
        ids.extend(collected.any_ids);
        ids.extend(collected.file_ids);
    }
    ids
}

pub fn collect_effective_image_ids(
    effective: &EffectiveContentSchema,
    values: &BTreeMap<String, Value>,
) -> Vec<String> {
    let mut ids = Vec::new();
    for (key, entry) in &effective.effective_fields {
        let Some(value) = values.get(key) else {
            continue;
        };
        let collected = opendash_core::field_schema::collect_media_ids(
            std::slice::from_ref(&entry.definition),
            &std::collections::BTreeMap::from([(entry.definition.key.clone(), value.clone())]),
        );
        ids.extend(collected.image_ids);
        ids.extend(collected.video_ids);
        ids.extend(collected.any_ids);
        ids.extend(collected.file_ids);
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
        if !valid_media_id(&id)
            || !matches!(find(db, &id).await?, Some(asset) if asset.kind == "image")
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
        if !valid_media_id(&id)
            || !matches!(find(db, &id).await?, Some(asset) if asset.kind == "image")
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
        verify_active_image_in_tx(conn, &id).await?;
    }
    Ok(())
}

pub async fn require_existing_effective_references_in_tx(
    conn: &mut sqlx::SqliteConnection,
    effective: &EffectiveContentSchema,
    values: &BTreeMap<String, Value>,
) -> ApiResult<()> {
    for id in collect_effective_image_ids(effective, values) {
        verify_active_image_in_tx(conn, &id).await?;
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
