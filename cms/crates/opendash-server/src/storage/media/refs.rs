use std::collections::BTreeMap;

use opendash_core::content::EffectiveContentSchema;
use serde_json::Value;
use sqlx::SqlitePool;

use super::queries::{find, valid_media_id};
use crate::error::{ApiError, ApiResult};

pub fn collect_effective_media_ids(
    effective: &EffectiveContentSchema,
    values: &BTreeMap<String, Value>,
) -> opendash_core::field_schema::CollectedMediaIds {
    let specs: Vec<opendash_core::field_schema::FieldSpec> = effective
        .effective_fields
        .values()
        .map(|entry| entry.definition.clone())
        .collect();
    opendash_core::field_schema::collect_media_ids(&specs, values)
}

pub async fn require_existing_effective_references(
    db: &SqlitePool,
    effective: &EffectiveContentSchema,
    values: &BTreeMap<String, Value>,
) -> ApiResult<()> {
    let collected = collect_effective_media_ids(effective, values);
    let mut conn = db.acquire().await?;
    verify_collected_media_with_quarantine_in_conn(&mut conn, &collected).await
}

pub async fn require_existing_effective_references_in_tx(
    conn: &mut sqlx::SqliteConnection,
    effective: &EffectiveContentSchema,
    values: &BTreeMap<String, Value>,
) -> ApiResult<()> {
    let collected = collect_effective_media_ids(effective, values);
    verify_collected_media_with_quarantine_in_conn(conn, &collected).await
}

async fn verify_collected_media_with_quarantine_in_conn(
    conn: &mut sqlx::SqliteConnection,
    collected: &opendash_core::field_schema::CollectedMediaIds,
) -> ApiResult<()> {
    for id in &collected.image_ids {
        verify_active_media_with_kind_in_conn(conn, id, Some("image")).await?;
    }
    for id in &collected.video_ids {
        verify_active_media_with_kind_in_conn(conn, id, Some("video")).await?;
    }
    for id in collected.any_ids.iter().chain(collected.file_ids.iter()) {
        verify_active_media_with_kind_in_conn(conn, id, None).await?;
    }
    Ok(())
}

async fn verify_active_media_with_kind_in_conn(
    conn: &mut sqlx::SqliteConnection,
    id: &str,
    expected_kind: Option<&str>,
) -> ApiResult<()> {
    if !valid_media_id(id) {
        return Err(ApiError::bad_request(
            "Media tham chiếu chưa có trong Media Library. Hãy chọn media đã upload.",
        ));
    }
    let state: Option<String> = match expected_kind {
        Some(kind) => {
            sqlx::query_scalar("SELECT lifecycle_state FROM media_assets WHERE id = ? AND kind = ?")
                .bind(id)
                .bind(kind)
                .fetch_optional(&mut *conn)
                .await?
        }
        None => {
            sqlx::query_scalar("SELECT lifecycle_state FROM media_assets WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?
        }
    };
    match state.as_deref() {
        Some("active") => Ok(()),
        Some(_) => Err(ApiError::conflict(
            "Media tham chiếu đang trong quá trình xóa; không thể tạo tham chiếu mới.",
        )),
        None => Err(ApiError::bad_request(
            "Media tham chiếu chưa có trong Media Library. Hãy chọn media đã upload.",
        )),
    }
}
