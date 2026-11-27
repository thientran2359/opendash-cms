use super::*;

pub async fn put_site_taxonomy_eligibility(
    db: &SqlitePool,
    actor: &crate::auth::SessionContext,
    type_key: &str,
    expected_version: i64,
    taxonomies: &[String],
) -> ApiResult<ContentTypeApiItem> {
    if !opendash_core::content::valid_content_type_key(type_key)
        || matches!(type_key, "pattern" | "section")
    {
        return Err(ApiError::bad_request(
            "Không thể cấu hình taxonomy cho Section.",
        ));
    }
    if taxonomies.len() > 64 {
        return Err(ApiError::bad_request("Tối đa 64 taxonomy cho mỗi CPT."));
    }
    let mut seen = std::collections::HashSet::new();
    for key in taxonomies {
        if !opendash_core::content::valid_content_type_key(key) || !seen.insert(key) {
            return Err(ApiError::bad_request(
                "Danh sách taxonomy chứa khóa không hợp lệ hoặc trùng.",
            ));
        }
    }
    let mut tx = begin_content_write_tx(db).await?;
    // Auth at route entry is not a sufficient write fence: a concurrent admin
    // can revoke/demote the actor while this request waits for the writer.
    // Check session expiry, identity and current admin role *after* claiming
    // the SQLite writer and before changing any eligibility or CPT version.
    let current_actor: Option<(i64, String)> = sqlx::query_as(
        "SELECT u.id,u.role FROM sessions s JOIN users u ON u.id=s.user_id
         WHERE s.token_hash=? AND s.expires_at > ?",
    )
    .bind(&actor.token_hash)
    .bind(crate::auth::current_time_secs())
    .fetch_optional(&mut *tx)
    .await?;
    match current_actor {
        Some((id, role)) if id == actor.user.id && role == "admin" => {}
        Some(_) => return Err(ApiError::forbidden("Tài khoản không còn quyền quản trị.")),
        None => return Err(ApiError::unauthorized("Phiên đăng nhập đã bị thu hồi.")),
    }
    let version: Option<i64> = sqlx::query_scalar("SELECT version FROM content_types WHERE key=?")
        .bind(type_key)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(version) = version else {
        return Err(ApiError::not_found("Loại nội dung không tồn tại."));
    };
    if version != expected_version {
        return Err(ApiError::precondition_failed(
            "Schema CPT đã thay đổi; hãy tải lại.",
        ));
    }
    for key in taxonomies {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM taxonomy_definitions WHERE key=?")
                .bind(key)
                .fetch_optional(&mut *tx)
                .await?;
        if exists.is_none() {
            return Err(ApiError::bad_request("Taxonomy được chọn không tồn tại."));
        }
    }
    let canonical = {
        let mut sorted = taxonomies.to_vec();
        sorted.sort();
        serde_json::to_string(&sorted)?
    };
    sqlx::query(
        "INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json)
         VALUES (?,?) ON CONFLICT(content_type_key) DO UPDATE SET
           taxonomies_json=excluded.taxonomies_json,updated_at=datetime('now')",
    )
    .bind(type_key)
    .bind(&canonical)
    .execute(&mut *tx)
    .await?;
    let count = sqlx::query(
        "UPDATE content_types SET version=version+1,
        updated_at=datetime('now') WHERE key=? AND version=?",
    )
    .bind(type_key)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if count != 1 {
        return Err(ApiError::precondition_failed(
            "Schema CPT đã thay đổi; hãy tải lại.",
        ));
    }
    let effective = load_effective_content_schema_in_tx(&mut tx, type_key)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải schema CPT."))?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    crate::storage::search::sync_entries_for_type_in_conn(&mut tx, type_key).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(ContentTypeApiItem {
        base: effective.base,
        version: effective.base_version,
        schema_token: effective.schema_token,
    })
}

pub async fn put_editor_layout(
    db: &SqlitePool,
    type_key: &str,
    expected_version: i64,
    updated_by: i64,
    input: EditorLayoutConfigV1,
) -> ApiResult<EditorLayoutSnapshot> {
    if !valid_content_type_key(type_key) {
        return Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."));
    }
    if expected_version < 0 {
        return Err(ApiError::bad_request("If-Match version không hợp lệ."));
    }
    let mut tx = begin_content_write_tx(db).await?;
    let Some(effective) = load_effective_content_schema_in_tx(&mut tx, type_key).await? else {
        return Err(ApiError::not_found(format!(
            "Không tìm thấy loại nội dung '{type_key}'."
        )));
    };
    let (_, current_version) = load_raw_editor_layout_in_tx(&mut tx, type_key).await?;
    if current_version != expected_version {
        return Err(ApiError::conflict(format!(
            "Xung đột phiên bản bố cục '{type_key}': hiện tại là v{current_version}, If-Match là v{expected_version}."
        )));
    }
    let next_version = current_version + 1;
    let taxonomies = load_effective_taxonomies_in_tx(&mut tx, type_key).await?;
    let plugin_action_panels = load_plugin_action_panels_for_type_in_tx(&mut tx, type_key).await?;
    let base_panels =
        build_core_and_group_panels(&effective, !taxonomies.is_empty(), &plugin_action_panels);
    let (layout, panels) = resolve_and_validate_editor_layout(
        &effective,
        base_panels,
        Some(input),
        next_version as u64,
    )
    .map_err(|e| ApiError::unprocessable(e.message))?;

    let layout_json = serde_json::to_string(&layout)
        .map_err(|e| ApiError::internal(format!("Không thể serialize layout_json: {e}")))?;

    sqlx::query(
        "INSERT INTO content_editor_layouts (type_key, layout_json, version, updated_by, updated_at)
         VALUES (?, ?, ?, ?, (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))
         ON CONFLICT(type_key) DO UPDATE SET
            layout_json = excluded.layout_json,
            version = excluded.version,
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at",
    )
    .bind(type_key)
    .bind(&layout_json)
    .bind(next_version)
    .bind(updated_by)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(EditorLayoutSnapshot {
        layout,
        panels,
        version: next_version,
    })
}
