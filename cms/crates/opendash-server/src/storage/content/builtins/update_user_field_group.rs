use super::*;
use crate::storage::content::builtins::sync_field_groups::{
    TargetDomainSnapshots, collect_target_snapshots_in_tx, upsert_group_targets_in_tx,
    verify_snapshots_after_write_in_tx,
};

pub async fn create_user_field_group(
    db: &SqlitePool,
    input: &UserFieldGroupInput,
) -> ApiResult<FieldGroupRecord> {
    let (scoped_id, norm) = input
        .validate_and_normalize(None)
        .map_err(map_input_validation_error)?;
    let group_id = scoped_id.to_canonical();
    let targets = extract_materialized_targets(&norm.location)
        .map_err(|e| map_transition_error(e, true))?;
    let fields_json = serde_json::to_string(&norm.fields)?;
    let location_json = serde_json::to_string(&norm.location)?;
    let presentation_json = serde_json::to_string(&norm.presentation)?;
    let domain_str = domain_to_db(norm.domain);

    let mut tx = begin_content_write_tx(db).await?;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT group_id FROM content_field_groups WHERE group_id = ?")
            .bind(&group_id)
            .fetch_optional(&mut *tx)
            .await?;
    if existing.is_some() {
        return Err(ApiError::conflict(
            "group_id hoặc local_key của nhóm trường đã tồn tại.",
        ));
    }

    let snapshots = collect_target_snapshots_in_tx(&mut tx, norm.domain, &targets, false).await?;

    sqlx::query(
        "INSERT INTO content_field_groups
         (group_id, owner_kind, owner_id, local_key, title, description, fields_json,
          domain, location_rules_json, presentation_json, active,
          default_region, default_order, collapsible, status, version)
         VALUES (?, 'user', NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', 1)",
    )
    .bind(&group_id)
    .bind(&scoped_id.local_key)
    .bind(&input.title)
    .bind(&input.description)
    .bind(&fields_json)
    .bind(domain_str)
    .bind(&location_json)
    .bind(&presentation_json)
    .bind(i64::from(norm.active))
    .bind(norm.presentation.region.as_str())
    .bind(norm.presentation.order)
    .bind(i64::from(norm.presentation.collapsible))
    .execute(&mut *tx)
    .await?;

    upsert_group_targets_in_tx(&mut tx, &group_id, &[], &targets, &[], false).await?;
    verify_snapshots_after_write_in_tx(&mut tx, &snapshots, true).await?;

    let created = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải field group vừa tạo."))?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(created)
}

pub async fn update_user_field_group(
    db: &SqlitePool,
    group_id_param: &str,
    expected_version: i64,
    input: &UserFieldGroupInput,
) -> ApiResult<FieldGroupRecord> {
    let (scoped_id, norm) = input
        .validate_and_normalize(Some(group_id_param))
        .map_err(map_input_validation_error)?;
    let group_id = scoped_id.to_canonical();
    let incoming_targets = extract_materialized_targets(&norm.location)
        .map_err(|e| map_transition_error(e, true))?;
    let fields_json = serde_json::to_string(&norm.fields)?;
    let location_json = serde_json::to_string(&norm.location)?;
    let presentation_json = serde_json::to_string(&norm.presentation)?;
    let domain_str = domain_to_db(norm.domain);

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

    let mut old_has_persisted_data = false;
    for old_t in &existing.targets {
        if has_persisted_group_values_for_target_in_tx(
            &mut tx,
            &old_t.target_kind,
            &old_t.target_key,
            &existing.fields,
        )
        .await?
        {
            old_has_persisted_data = true;
            break;
        }
    }
    if existing.domain != norm.domain && old_has_persisted_data {
        return Err(ApiError::conflict(
            "incompatible_schema_change: Không thể chuyển phạm vi domain của nhóm trường khi đã có dữ liệu.",
        ));
    }

    let mut old_snapshots = TargetDomainSnapshots::default();
    if existing.domain == norm.domain {
        let mut combined: Vec<(String, String)> = existing
            .targets
            .iter()
            .map(|t| (t.target_kind.clone(), t.target_key.clone()))
            .collect();
        combined.extend(incoming_targets.iter().cloned());
        old_snapshots = collect_target_snapshots_in_tx(&mut tx, norm.domain, &combined, false).await?;
    } else {
        old_snapshots =
            collect_target_snapshots_in_tx(&mut tx, norm.domain, &incoming_targets, false).await?;
    }

    let any_target_has_data = old_has_persisted_data
        || old_snapshots.content.values().any(|(_, h)| *h)
        || old_snapshots.term.values().any(|(_, h)| *h);
    let incoming_keys: HashSet<&str> = norm
        .fields
        .iter()
        .filter(|f| !f.is_ui_only())
        .map(|f| f.key.as_str())
        .collect();
    for old_f in &existing.fields {
        if !old_f.is_ui_only() && !incoming_keys.contains(old_f.key.as_str()) && any_target_has_data {
            return Err(ApiError::conflict(format!(
                "cannot_remove_field_with_data: Không thể xóa hẳn trường '{}' khi đã có dữ liệu; hãy tắt nhóm trường hoặc thu hồi.",
                old_f.key
            )));
        }
    }

    sqlx::query(
        "UPDATE content_field_groups
         SET title = ?, description = ?, fields_json = ?,
             domain = ?, location_rules_json = ?, presentation_json = ?, active = ?,
             default_region = ?, default_order = ?, collapsible = ?, version = version + 1,
             updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         WHERE group_id = ? AND version = ?",
    )
    .bind(&input.title)
    .bind(&input.description)
    .bind(&fields_json)
    .bind(domain_str)
    .bind(&location_json)
    .bind(&presentation_json)
    .bind(i64::from(norm.active))
    .bind(norm.presentation.region.as_str())
    .bind(norm.presentation.order)
    .bind(i64::from(norm.presentation.collapsible))
    .bind(&group_id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;

    if existing.domain != norm.domain {
        sqlx::query("DELETE FROM content_field_group_targets WHERE group_id = ?")
            .bind(&group_id)
            .execute(&mut *tx)
            .await?;
        upsert_group_targets_in_tx(&mut tx, &group_id, &[], &incoming_targets, &[], false).await?;
    } else {
        upsert_group_targets_in_tx(
            &mut tx,
            &group_id,
            &existing.targets,
            &incoming_targets,
            &existing.fields,
            true,
        )
        .await?;
    }

    verify_snapshots_after_write_in_tx(&mut tx, &old_snapshots, true).await?;

    let updated = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải field group sau cập nhật."))?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(updated)
}

pub async fn set_user_field_group_active(
    db: &SqlitePool,
    group_id_param: &str,
    expected_version: i64,
    active: bool,
) -> ApiResult<FieldGroupRecord> {
    let parsed = if group_id_param.contains(':') {
        ScopedResourceId::parse(group_id_param).map_err(map_input_validation_error)?
    } else {
        ScopedResourceId::new(ResourceOwner::User, group_id_param.to_owned())
            .map_err(map_input_validation_error)?
    };
    if parsed.owner != ResourceOwner::User {
        return Err(ApiError::forbidden(
            "Chỉ có thể bật/tắt nhóm trường thuộc quyền sở hữu của User.",
        ));
    }
    let group_id = parsed.to_canonical();
    let mut tx = begin_content_write_tx(db).await?;
    let existing = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nhóm trường."))?;
    if existing.owner != ResourceOwner::User {
        return Err(ApiError::forbidden(
            "Chỉ có thể bật/tắt nhóm trường thuộc quyền sở hữu của User.",
        ));
    }
    if existing.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nhóm trường đã thay đổi. Vui lòng tải lại.",
        ));
    }
    let targets: Vec<(String, String)> = existing
        .targets
        .iter()
        .map(|t| (t.target_kind.clone(), t.target_key.clone()))
        .collect();
    let snapshots = collect_target_snapshots_in_tx(&mut tx, existing.domain, &targets, false).await?;

    sqlx::query(
        "UPDATE content_field_groups
         SET active = ?, version = version + 1,
             updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         WHERE group_id = ? AND version = ?",
    )
    .bind(i64::from(active))
    .bind(&group_id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;

    verify_snapshots_after_write_in_tx(&mut tx, &snapshots, true).await?;

    let updated = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải field group sau cập nhật."))?;
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
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(withdrawn)
}

pub async fn delete_user_field_group(
    db: &SqlitePool,
    group_id_param: &str,
    expected_version: i64,
    purge: bool,
) -> ApiResult<()> {
    let parsed = if group_id_param.contains(':') {
        ScopedResourceId::parse(group_id_param).map_err(map_input_validation_error)?
    } else {
        ScopedResourceId::new(ResourceOwner::User, group_id_param.to_owned())
            .map_err(map_input_validation_error)?
    };
    if parsed.owner != ResourceOwner::User {
        return Err(ApiError::forbidden(
            "Chỉ có thể xóa nhóm trường thuộc quyền sở hữu của User.",
        ));
    }
    let group_id = parsed.to_canonical();
    let mut tx = begin_content_write_tx(db).await?;
    let existing = load_field_group_in_tx(&mut tx, &group_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nhóm trường."))?;
    if existing.owner != ResourceOwner::User {
        return Err(ApiError::forbidden(
            "Chỉ có thể xóa nhóm trường thuộc quyền sở hữu của User.",
        ));
    }
    if existing.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nhóm trường đã thay đổi. Vui lòng tải lại.",
        ));
    }

    let mut has_data = false;
    for t in &existing.targets {
        if has_persisted_group_values_for_target_in_tx(
            &mut tx,
            &t.target_kind,
            &t.target_key,
            &existing.fields,
        )
        .await?
        {
            has_data = true;
            break;
        }
    }
    if !purge && has_data {
        return Err(ApiError::conflict(
            "cannot_delete_field_group_with_data: Nhóm trường đã có dữ liệu; hãy thu hồi hoặc thêm ?purge=true.",
        ));
    }
    if purge {
        let data_keys: Vec<String> = existing
            .fields
            .iter()
            .filter(|f| !f.is_ui_only())
            .map(|f| f.key.clone())
            .collect();
        match existing.domain {
            FieldGroupDomain::Content => {
                for key in &data_keys {
                    sqlx::query("UPDATE content_entries SET values_json = json_remove(values_json, ?) WHERE json_type(values_json, ?) IS NOT NULL")
                        .bind(format!("$.{key}"))
                        .bind(format!("$.{key}"))
                        .execute(&mut *tx)
                        .await?;
                }
            }
            FieldGroupDomain::Term => {
                for key in &data_keys {
                    sqlx::query("UPDATE taxonomy_terms SET values_json = json_remove(values_json, ?) WHERE json_type(values_json, ?) IS NOT NULL")
                        .bind(format!("$.{key}"))
                        .bind(format!("$.{key}"))
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("DELETE FROM term_field_media_refs WHERE field_key = ?")
                        .bind(key)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            FieldGroupDomain::OptionsPage => {
                for t in &existing.targets {
                    for key in &data_keys {
                        for tbl in [
                            "theme_option_values",
                            "theme_option_locale_values",
                            "theme_option_media_refs",
                        ] {
                            let sql = format!(
                                "DELETE FROM {tbl} WHERE owner_kind = 'options_page' AND owner_id = ? AND option_key = ?"
                            );
                            sqlx::query(sqlx::AssertSqlSafe(sql))
                                .bind(&t.target_key)
                                .bind(key)
                                .execute(&mut *tx)
                                .await?;
                        }
                    }
                }
            }
        }
    }
    sqlx::query("DELETE FROM content_field_group_targets WHERE group_id = ?")
        .bind(&group_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM content_field_groups WHERE group_id = ? AND version = ?")
        .bind(&group_id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
}
