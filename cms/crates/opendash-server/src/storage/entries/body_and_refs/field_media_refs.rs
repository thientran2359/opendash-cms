use std::collections::BTreeSet;

use opendash_core::content::EffectiveContentSchema;
use serde_json::Value;
use sqlx::SqliteConnection;

use crate::error::ApiResult;

/// Syncs `entry_builder_media_refs` (`source_kind = 'field'`) for an entry across
/// ALL `effective_schema.effective_fields` (including `Active`, `Withdrawn`, and
/// `ProviderInactive`, excluding `body`), preserving public media ACL on theme/plugin changes.
pub(in crate::storage) async fn sync_entry_field_media_refs_in_conn(
    conn: &mut SqliteConnection,
    entry_id: i64,
    effective: &EffectiveContentSchema,
    values: &std::collections::BTreeMap<String, Value>,
) -> ApiResult<()> {
    sqlx::query(
        "DELETE FROM entry_builder_media_refs WHERE entry_id = ? AND source_kind = 'field'",
    )
    .bind(entry_id)
    .execute(&mut *conn)
    .await?;

    let mut media_ids = BTreeSet::new();
    for (key, entry) in &effective.effective_fields {
        if key == "body" {
            continue;
        }
        let Some(val) = values.get(key) else {
            continue;
        };
        let collected = opendash_core::field_schema::collect_media_ids(
            std::slice::from_ref(&entry.definition),
            &std::collections::BTreeMap::from([(entry.definition.key.clone(), val.clone())]),
        );
        media_ids.extend(collected.image_ids);
        media_ids.extend(collected.video_ids);
        media_ids.extend(collected.any_ids);
        media_ids.extend(collected.file_ids);
    }

    for media_id in media_ids {
        let kind: Option<String> = sqlx::query_scalar(
            "SELECT kind FROM media_assets WHERE id = ? AND lifecycle_state = 'active'",
        )
        .bind(&media_id)
        .fetch_optional(&mut *conn)
        .await?;
        let Some(ref_role) = kind.filter(|k| matches!(k.as_str(), "image" | "video" | "file"))
        else {
            continue;
        };
        sqlx::query(
            "INSERT OR IGNORE INTO entry_builder_media_refs(entry_id, media_id, ref_role, source_kind)
             VALUES (?, ?, ?, 'field')",
        )
        .bind(entry_id)
        .bind(&media_id)
        .bind(&ref_role)
        .execute(&mut *conn)
        .await?;
    }

    Ok(())
}
