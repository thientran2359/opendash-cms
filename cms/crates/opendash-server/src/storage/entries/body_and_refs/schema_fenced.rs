use super::*;

pub async fn create_entry_with_schema_fence(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    _schema: &ContentTypeDefinition,
    input: &ContentEntryInput,
    preflight_schema_token: &str,
) -> ApiResult<ContentEntryRecord> {
    forbid_generic_pattern_write(type_key)?;
    let mut tx = begin_entry_write(db, actor).await?;
    let effective = verify_schema_token_in_tx(&mut tx, type_key, preflight_schema_token).await?;
    let settings = &effective.base.descriptor.settings;
    verify_supported_values(type_key, &input.values, settings)?;
    let entry_locale =
        resolve_entry_write_locale_in_tx(&mut tx, input.locale.as_deref(), None).await?;
    crate::storage::media::require_existing_effective_references_in_tx(
        &mut tx,
        &effective,
        &input.values,
    )
    .await?;
    validate_effective_relational_refs_in_conn(
        &mut tx,
        &effective,
        &input.values,
        None,
        &entry_locale,
    )
    .await?;
    if let Some(ids) = &input.term_ids {
        validate_term_ids_for_locale(&mut tx, type_key, ids, &entry_locale).await?;
    }
    let group_id = crate::storage::settings::generate_translation_group_id("eg");
    let json = serde_json::to_string(&input.values)?;
    let status = if settings.publication == ContentPublication::DraftPublish {
        "draft"
    } else {
        "published"
    };
    let result = sqlx::query(
        "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, published_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, CASE WHEN ? = 'published' THEN datetime('now') ELSE NULL END)",
    )
    .bind(type_key)
    .bind(&entry_locale)
    .bind(&group_id)
    .bind(&input.slug)
    .bind(&input.title)
    .bind(json)
    .bind(status)
    .bind(status)
    .execute(&mut *tx)
    .await;
    let id = match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            return Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."));
        }
        other => other?.last_insert_rowid(),
    };
    if let Some(ids) = &input.term_ids {
        replace_term_ids(&mut tx, id, ids).await?;
    }
    if status == "published" {
        enforce_taxonomy_publish_rules_in_tx(&mut tx, type_key, id, &entry_locale, None, None)
            .await?;
    }
    sync_entry_body_refs_in_conn(
        &mut *tx,
        id,
        &effective,
        &input.values,
        None,
        actor_field_role(actor.user.role),
    )
    .await?;
    let row: EntryRow = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
         FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let record = decoded_with_terms(&mut tx, row).await?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    crate::storage::search::sync_entry_search_index_in_conn(&mut *tx, id).await?;
    crate::storage::revisions::record_entry_revision_and_clear_autosave_in_tx(
        &mut *tx,
        id,
        opendash_core::revisions::RevisionSource::Create,
        crate::storage::revisions::RevisionActor::User(actor),
        None,
    )
    .await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(record)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_entry_with_schema_fence(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    _schema: &ContentTypeDefinition,
    id: i64,
    expected_version: i64,
    input: &ContentEntryInput,
    preflight_schema_token: &str,
) -> ApiResult<ContentEntryRecord> {
    forbid_generic_pattern_write(type_key)?;
    let mut tx = begin_entry_write(db, actor).await?;
    let effective = verify_schema_token_in_tx(&mut tx, type_key, preflight_schema_token).await?;
    verify_supported_values(type_key, &input.values, &effective.base.descriptor.settings)?;
    current_entry_in_write(&mut tx, type_key, id, expected_version).await?;
    let (old_values_json, existing_locale): (String, String) = sqlx::query_as(
        "SELECT values_json, locale FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let entry_locale =
        resolve_entry_write_locale_in_tx(&mut tx, input.locale.as_deref(), Some(&existing_locale))
            .await?;
    let previous_body = extract_body_from_values_json(&old_values_json);
    let previous_values = serde_json::from_str::<BTreeMap<String, Value>>(&old_values_json).ok();
    crate::storage::media::require_existing_effective_references_in_tx(
        &mut tx,
        &effective,
        &input.values,
    )
    .await?;
    validate_effective_relational_refs_in_conn(
        &mut tx,
        &effective,
        &input.values,
        previous_values.as_ref(),
        &entry_locale,
    )
    .await?;
    if let Some(ids) = &input.term_ids {
        validate_term_ids_for_locale(&mut tx, type_key, ids, &entry_locale).await?;
    }
    crate::storage::revisions::capture_baseline_if_missing_in_tx(&mut *tx, id).await?;
    let json = serde_json::to_string(&input.values)?;
    let result = sqlx::query(
        "UPDATE content_entries
         SET slug = ?, title = ?, values_json = ?, version = version + 1,
             updated_at = datetime('now')
         WHERE content_type_key = ? AND id = ? AND version = ? AND trashed_at IS NULL",
    )
    .bind(&input.slug)
    .bind(&input.title)
    .bind(json)
    .bind(type_key)
    .bind(id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await;
    match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."))
        }
        other => {
            if other?.rows_affected() == 0 {
                return Err(ApiError::precondition_failed(
                    "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
                ));
            }
            if let Some(ids) = &input.term_ids {
                replace_term_ids(&mut tx, id, ids).await?;
            }
            sync_entry_body_refs_in_conn(
                &mut *tx,
                id,
                &effective,
                &input.values,
                previous_body.as_deref(),
                actor_field_role(actor.user.role),
            )
            .await?;
            let row: EntryRow = sqlx::query_as(
                "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
                 FROM content_entries WHERE content_type_key = ? AND id = ?",
            )
            .bind(type_key)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            let record = decoded_with_terms(&mut tx, row).await?;
            crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
            crate::storage::search::sync_entry_search_index_in_conn(&mut *tx, id).await?;
            crate::storage::revisions::record_entry_revision_and_clear_autosave_in_tx(
                &mut *tx,
                id,
                opendash_core::revisions::RevisionSource::Save,
                crate::storage::revisions::RevisionActor::User(actor),
                Some(actor.user.id),
            )
            .await?;
            tx.commit().await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(record)
        }
    }
}
