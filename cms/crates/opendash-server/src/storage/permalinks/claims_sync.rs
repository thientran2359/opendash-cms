use super::*;

/// Always called while the caller already holds SQLite's single-writer
/// reservation. Any failure (including a namespace conflict) rolls back the
/// caller's data mutation as well as the proposed claims.
pub async fn sync_claims_in_tx(conn: &mut SqliteConnection) -> ApiResult<()> {
    sync_claims_with_options(conn, false)
        .await
        .map_err(PermalinkPutError::into_api_error)
}
