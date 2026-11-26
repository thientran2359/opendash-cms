use super::*;

/// Recomputes post-commit projections without ever turning a durable DB commit into a failure.
pub async fn recompute_projections_nonfatal(
    db: &SqlitePool,
    hooks: &crate::hooks::HookDispatcher,
    entry_id: i64,
    needed: bool,
) -> ProjectionOutcome {
    if !needed {
        return ProjectionOutcome::NotNeeded;
    }
    match crate::storage::plugin_projections::recompute_entry_projections(db, hooks, entry_id).await
    {
        Ok(()) => ProjectionOutcome::Current,
        Err(err) => {
            eprintln!(
                "projection_recompute_failed entry_id={entry_id} status={}",
                err.0.as_u16()
            );
            ProjectionOutcome::Failed {
                reason: "projection_recompute_failed".into(),
            }
        }
    }
}

/// Loads all currently assigned Reading page IDs across all retained `site_reading_settings` rows
/// within the caller's read transaction so active table rows can expose exact blocked reasons.
pub(crate) async fn load_assigned_reading_page_ids_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
) -> ApiResult<HashSet<i64>> {
    let rows: Vec<(Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT homepage_page_id, posts_page_id FROM site_reading_settings")
            .fetch_all(&mut **tx)
            .await?;
    let mut ids = HashSet::new();
    for (home, posts) in rows {
        if let Some(id) = home {
            ids.insert(id);
        }
        if let Some(id) = posts {
            ids.insert(id);
        }
    }
    Ok(ids)
}
