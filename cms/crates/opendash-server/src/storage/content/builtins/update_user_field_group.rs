use super::*;

pub async fn update_user_field_group(
    db: &SqlitePool,
    group_id_param: &str,
    expected_version: i64,
    input: &UserFieldGroupInput,
) -> ApiResult<FieldGroupRecord> {
    let (scoped_id, incoming_fields) = input
        .validate_and_normalize(Some(group_id_param))
        .map_err(map_input_validation_error)?;
    let group_id = scoped_id.to_canonical();
    let fields_json = serde_json::to_string(&incoming_fields)?;

    let mut tx = begin_content_write_tx(db).await?;
    let existing = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nhóm trường."))?;
    if existing.owner != ResourceOwner::User {
        return Err(ApiError::forbidden(
            "Chỉ có thể cập nhật nhóm trường thuộc quyền sở hữu của User.",
        ));
    }
    if existing.status == FieldGroupStatus::Withdrawn {
        return Err(ApiError::conflict(
            "Nhóm trường đã bị rút khai báo; không thể cập nhật.",
        ));
    }
    if existing.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nhóm trường đã thay đổi. Vui lòng tải lại.",
        ));
    }

    let old_targets: Vec<String> = existing
        .targets
        .iter()
        .map(|t| t.type_key.clone())
        .collect();
    let mut all_affected_types = BTreeSet::new();
    for t in &old_targets {
        all_affected_types.insert(t.clone());
    }
    for t in &input.target_types {
        all_affected_types.insert(t.clone());
    }

    let mut old_snapshots = HashMap::new();
    let mut any_target_has_entries = false;
    for t in &all_affected_types {
        let Some(old_eff) = load_effective_content_schema_in_tx(&mut tx, t).await? else {
            return Err(ApiError::not_found("Loại nội dung mục tiêu không tồn tại."));
        };
        let has_entries: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ?)",
        )
        .bind(t)
        .fetch_one(&mut *tx)
        .await?;
        if has_entries {
            any_target_has_entries = true;
        }
        old_snapshots.insert(t.clone(), (old_eff, has_entries));
    }

    // Check if any field key was removed completely when any linked type has entries
    let incoming_keys: HashSet<&str> = incoming_fields.iter().map(|f| f.key.as_str()).collect();
    for old_f in &existing.fields {
        if !incoming_keys.contains(old_f.key.as_str()) && any_target_has_entries {
            return Err(ApiError::conflict(format!(
                "cannot_remove_field_with_data: Không thể xóa hẳn trường '{}' khi đã có dữ liệu; hãy đặt declaration_state = withdrawn.",
                old_f.key
            )));
        }
    }

    sqlx::query(
        "UPDATE content_field_groups
         SET title = ?, description = ?, fields_json = ?, default_region = ?,
             default_order = ?, collapsible = ?, version = version + 1,
             updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         WHERE group_id = ? AND version = ?",
    )
    .bind(&input.title)
    .bind(&input.description)
    .bind(&fields_json)
    .bind(input.default_region.as_str())
    .bind(input.default_order)
    .bind(if input.collapsible { 1_i64 } else { 0_i64 })
    .bind(&group_id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;

    let new_target_set: HashSet<&str> = input.target_types.iter().map(|s| s.as_str()).collect();
    for t in &input.target_types {
        sqlx::query(
            "INSERT INTO content_field_group_targets (group_id, type_key, association_state)
             VALUES (?, ?, 'declared')
             ON CONFLICT(group_id, type_key) DO UPDATE SET
                 association_state = 'declared',
                 updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        )
        .bind(&group_id)
        .bind(t)
        .execute(&mut *tx)
        .await?;
    }

    for old_t in &old_targets {
        if !new_target_set.contains(old_t.as_str()) {
            let has_values =
                has_persisted_group_values_for_type_in_tx(&mut tx, old_t, &existing.fields).await?;
            let has_layout_ref: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM content_editor_layouts WHERE type_key = ? AND instr(layout_json, ?) > 0)",
            )
            .bind(old_t)
            .bind(&group_id)
            .fetch_one(&mut *tx)
            .await?;

            if has_values || has_layout_ref {
                sqlx::query(
                    "UPDATE content_field_group_targets
                     SET association_state = 'withdrawn',
                         updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     WHERE group_id = ? AND type_key = ?",
                )
                .bind(&group_id)
                .bind(old_t)
                .execute(&mut *tx)
                .await?;
            } else {
                sqlx::query(
                    "DELETE FROM content_field_group_targets WHERE group_id = ? AND type_key = ?",
                )
                .bind(&group_id)
                .bind(old_t)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    for t in &all_affected_types {
        let (old_eff, has_entries) = old_snapshots.get(t).expect("snapshot preloaded");
        let new_eff = load_effective_content_schema_in_tx(&mut tx, t)
            .await?
            .ok_or_else(|| ApiError::internal("Không thể tải schema sau cập nhật."))?;
        verify_effective_schema_transition(Some(old_eff), &new_eff, *has_entries)
            .map_err(|e| map_transition_error(e, true))?;
    }

    let updated = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải field group sau cập nhật."))?;
    for t in &all_affected_types {
        crate::storage::search::sync_entries_for_type_in_conn(&mut tx, t).await?;
    }
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(updated)
}

pub async fn withdraw_user_field_group(
    db: &SqlitePool,
    group_id_param: &str,
    expected_version: i64,
) -> ApiResult<FieldGroupRecord> {
    let parsed = if group_id_param.contains(':') {
        ScopedResourceId::parse(group_id_param).map_err(map_input_validation_error)?
    } else {
        ScopedResourceId::new(ResourceOwner::User, group_id_param.to_owned())
            .map_err(map_input_validation_error)?
    };
    if parsed.owner != ResourceOwner::User {
        return Err(ApiError::forbidden(
            "Chỉ có thể rút nhóm trường thuộc quyền sở hữu của User.",
        ));
    }
    let group_id = parsed.to_canonical();

    let mut tx = begin_content_write_tx(db).await?;
    let existing = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nhóm trường."))?;
    if existing.owner != ResourceOwner::User {
        return Err(ApiError::forbidden(
            "Chỉ có thể rút nhóm trường thuộc quyền sở hữu của User.",
        ));
    }
    if existing.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nhóm trường đã thay đổi. Vui lòng tải lại.",
        ));
    }

    sqlx::query(
        "UPDATE content_field_groups
         SET status = 'withdrawn', version = version + 1,
             updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         WHERE group_id = ? AND version = ?",
    )
    .bind(&group_id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE content_field_group_targets
         SET association_state = 'withdrawn',
             updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         WHERE group_id = ?",
    )
    .bind(&group_id)
    .execute(&mut *tx)
    .await?;

    let withdrawn = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải field group sau khi rút."))?;
    for target in &existing.targets {
        crate::storage::search::sync_entries_for_type_in_conn(&mut tx, &target.type_key).await?;
    }
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(withdrawn)
}
