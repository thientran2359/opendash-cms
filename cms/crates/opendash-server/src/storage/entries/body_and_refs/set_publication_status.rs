use super::*;

/// Version-CAS transition. Save/update never touches publication_status.
pub async fn set_publication_status(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
    expected_version: i64,
    status: &str,
) -> ApiResult<ContentEntryRecord> {
    forbid_generic_pattern_write(type_key)?;
    if status != "draft" && status != "published" {
        return Err(ApiError::bad_request("Trạng thái xuất bản không hợp lệ."));
    }
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let settings = current_settings_in_tx(&mut tx, type_key).await?;
    if settings.publication != ContentPublication::DraftPublish {
        return Err(ApiError::bad_request(
            "Loại nội dung này sử dụng chế độ lưu ngay.",
        ));
    }
    current_entry_in_write(&mut tx, type_key, id, expected_version).await?;
    let (locale, current_status): (String, String) = sqlx::query_as(
        "SELECT locale, publication_status FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    resolve_entry_write_locale_in_tx(&mut tx, None, Some(&locale)).await?;
    if current_status == status {
        let record = get_entry_in_tx(&mut tx, type_key, id)
            .await?
            .ok_or_else(|| ApiError::internal("Không thể tải trạng thái xuất bản."))?;
        tx.commit().await?;
        return Ok(record);
    }
    let result = sqlx::query(
        "UPDATE content_entries
         SET publication_status = ?,
             published_at = CASE WHEN ? = 'published' THEN COALESCE(published_at, datetime('now')) ELSE published_at END,
             version = version + 1,
             updated_at = datetime('now')
         WHERE content_type_key = ? AND id = ? AND version = ? AND trashed_at IS NULL",
    )
    .bind(status)
    .bind(status)
    .bind(type_key)
    .bind(id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        ));
    }
    if status == "draft" {
        crate::storage::plugin_projections::delete_entry_projections_in_tx(&mut tx, id).await?;
    } else if status == "published" {
        let effective =
            crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, type_key)
                .await?;
        if let Some(ref eff) = effective {
            let term_ids = read_term_ids(&mut tx, id).await?;
            let (layout, custom): (Option<String>, Option<String>) = sqlx::query_as(
                "SELECT page_layout_key, custom_template_key FROM content_entries WHERE id = ?",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            let loc_ctx = build_entry_location_ctx_in_conn(
                &mut *tx,
                type_key,
                Some(id),
                &locale,
                FieldRole::Admin,
                layout,
                custom,
                &term_ids,
            )
            .await?;
            enforce_taxonomy_publish_rules_in_tx(
                &mut tx,
                type_key,
                id,
                &locale,
                Some(eff),
                Some(&loc_ctx),
            )
            .await?;
        } else {
            enforce_taxonomy_publish_rules_in_tx(&mut tx, type_key, id, &locale, None, None)
                .await?;
        }
    }
    crate::storage::seo::sync_entry_seo_media_ref_in_tx(&mut tx, id).await?;
    let needs_claim_sync = if type_key == "page" {
        true
    } else {
        let id_str = id.to_string();
        let has_claim: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?)",
        )
        .bind(&id_str)
        .fetch_one(&mut *tx)
        .await?;
        let has_entry_redirects: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM entry_redirects WHERE entry_id = ?)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        let has_permalink_redirects: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM permalink_route_redirects WHERE target_kind = 'entry' AND target_id = ?)",
        )
        .bind(&id_str)
        .fetch_one(&mut *tx)
        .await?;
        has_claim == 0 || has_entry_redirects != 0 || has_permalink_redirects != 0
    };
    if needs_claim_sync {
        crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    }
    crate::storage::search::sync_entry_search_index_in_conn(&mut *tx, id).await?;
    let record = get_entry_in_tx(&mut tx, type_key, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải trạng thái xuất bản."))?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(record)
}
