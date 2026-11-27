use super::*;

/// Commits a narrow metadata patch under a single reserved SQLite writer transaction,
/// re-verifying live session/role, active CPT, schema token (412), entry version (412)
/// before lifecycle (409), timezone (409), date <= now, taxonomy bounds, and route claims.
/// Rejects completely unchanged final patches with `422 metadata_no_change` (no version bump or hooks).
/// Emits a privacy-safe structured diagnostic line (`entry_lifecycle_diag`) measuring
/// monotonic storage elapsed time including SQLite writer wait.
pub async fn commit_metadata_patch(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
    patch: &MetadataPatch,
    plan: &MetadataPreflightPlan,
) -> ApiResult<(
    crate::storage::entry_table::EntryTableRow,
    String,
    EntryCommitReceipt,
)> {
    let started = std::time::Instant::now();
    let res =
        commit_metadata_patch_inner(db, actor, type_key, entry_id, expected_version, patch, plan)
            .await;
    let storage_us = started.elapsed().as_micros();
    let role = actor.user.role.as_str();
    match &res {
        Ok((_, _, receipt)) => {
            eprintln!(
                "entry_lifecycle_diag op=metadata_commit type_key={type_key} entry_id={entry_id} actor_id={} actor_role={role} expected_version={expected_version} version_after={} outcome=ok status=200 storage_us={storage_us}",
                actor.user.id,
                receipt.version_after.unwrap_or(0),
            );
        }
        Err(err) => {
            eprintln!(
                "entry_lifecycle_diag op=metadata_commit type_key={type_key} entry_id={entry_id} actor_id={} actor_role={role} expected_version={expected_version} version_after=0 outcome=err status={} storage_us={storage_us}",
                actor.user.id,
                err.0.as_u16(),
            );
        }
    }
    res
}
