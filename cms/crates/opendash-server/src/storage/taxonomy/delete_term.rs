use sqlx::SqlitePool;

use super::definitions::{begin_admin_write, definition_in_write};
use super::terms::mutation_error;
use super::types::current_term;
use crate::auth::SessionContext;
use crate::error::{ApiError, ApiResult};

pub async fn delete_term(
    db: &SqlitePool,
    actor: &SessionContext,
    id: i64,
    expected_version: i64,
) -> ApiResult<()> {
    let mut tx = begin_admin_write(db, actor, false).await?;
    let current = current_term(&mut tx, id, expected_version).await?;
    if let Some(def) = definition_in_write(&mut tx, &current.kind).await?
        && def.settings.default_term_id == Some(id)
    {
        return Err(ApiError::conflict(
            "default_term_in_use: Không thể xóa term đang được đặt làm mặc định của phân loại.",
        ));
    }
    let children: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM taxonomy_terms WHERE parent_id = ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(mutation_error)?;
    if children != 0 {
        return Err(ApiError::conflict(
            "Category còn mục con. Hãy chuyển mục con trước.",
        ));
    }
    let has_memberships: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'entry_terms'",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(mutation_error)?;
    if has_memberships != 0 {
        let linked_entries: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM entry_terms WHERE term_id = ?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(mutation_error)?;
        if linked_entries != 0 {
            return Err(ApiError::conflict(
                "Term đang được gắn vào nội dung. Hãy gỡ liên kết trước.",
            ));
        }
    }
    let affected = sqlx::query("DELETE FROM taxonomy_terms WHERE id = ? AND version = ?")
        .bind(id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?
        .rows_affected();
    if affected != 1 {
        return Err(ApiError::precondition_failed(
            "Term đã thay đổi, hãy tải lại.",
        ));
    }
    sqlx::query(
        "DELETE FROM seo_public_media_refs WHERE source_kind = 'term_seo' AND source_id = ?",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    crate::storage::search::sync_entries_for_term_in_conn(&mut tx, id).await?;
    tx.commit().await.map_err(mutation_error)?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
}
