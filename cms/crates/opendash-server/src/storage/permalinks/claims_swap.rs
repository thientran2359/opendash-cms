use super::*;

pub(crate) async fn sync_claims_with_options(
    conn: &mut SqliteConnection,
    collect_conflicts: bool,
) -> Result<(), PermalinkPutError> {
    sync_claims_with_options_and_swap(conn, collect_conflicts, None).await
}
