use super::*;

pub(crate) async fn sync_claims_with_options_and_swap(
    conn: &mut SqliteConnection,
    collect_conflicts: bool,
    swapped_from: Option<&str>,
) -> Result<(), PermalinkPutError> {
    sync_claims_with_options_and_swap_and_limit(
        conn,
        collect_conflicts,
        swapped_from,
        MAX_REDIRECTS,
    )
    .await
}

pub(super) fn record_redirect_limit(
    collector: &mut ConflictCollector,
    max_redirects: usize,
) -> Result<(), PermalinkPutError> {
    collector.record(
        "/redirects",
        "permalink_redirect_limit",
        "max_redirects",
        format!("permalink_redirect_limit: tối đa {max_redirects} redirects đang hoạt động."),
    )
}

/// Always called while the caller already holds SQLite's single-writer
/// reservation. Any failure (including a namespace conflict) rolls back the
/// caller's data mutation as well as the proposed claims.
pub async fn sync_claims_in_tx(conn: &mut SqliteConnection) -> ApiResult<()> {
    sync_claims_with_options(conn, false)
        .await
        .map_err(PermalinkPutError::into_api_error)
}
