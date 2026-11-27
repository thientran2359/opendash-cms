use super::*;

/// Transitions any orphaned `staged`/`files_verified` runs to `quarantined` on startup,
/// deletes staging directories, and removes unreferenced media files only when `already_existed == false`
/// and `media_assets` has zero rows for `media_id`.
pub async fn recover_orphaned_import_runs(
    db: &SqlitePool,
    uploads_root: &std::path::Path,
) -> ApiResult<usize> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT run_id, media_manifest_json FROM import_runs WHERE state IN ('staged', 'files_verified')",
    )
    .fetch_all(db)
    .await?;

    let count = rows.len();
    for (run_id, manifest_json) in rows {
        let _ = tokio::fs::remove_dir_all(uploads_root.join(".staging").join(&run_id)).await;
        if let Ok(entries) = serde_json::from_str::<Vec<StagedMediaEntry>>(&manifest_json) {
            for entry in entries {
                if !entry.already_existed {
                    let refs: i64 =
                        sqlx::query_scalar("SELECT COUNT(*) FROM media_assets WHERE id = ?")
                            .bind(&entry.media_id)
                            .fetch_one(db)
                            .await?;
                    if refs == 0 && entry.media_id.len() >= 2 {
                        let dir = uploads_root.join("media").join(&entry.media_id[..2]);
                        let _ = tokio::fs::remove_file(
                            dir.join(format!("{}.{}", entry.media_id, entry.extension)),
                        )
                        .await;
                        let _ = tokio::fs::remove_file(
                            dir.join(format!("{}.thumb.png", entry.media_id)),
                        )
                        .await;
                        let _ = tokio::fs::remove_dir_all(
                            uploads_root
                                .join("variants")
                                .join(&entry.media_id[..2])
                                .join(&entry.media_id),
                        )
                        .await;
                    }
                }
            }
        }
        sqlx::query(
            "UPDATE import_runs SET state = 'quarantined', updated_at = datetime('now') WHERE run_id = ?",
        )
        .bind(&run_id)
        .execute(db)
        .await?;
    }
    Ok(count)
}
