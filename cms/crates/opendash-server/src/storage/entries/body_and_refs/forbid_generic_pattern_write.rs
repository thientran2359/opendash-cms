use super::*;

/// Pattern is a separate, strictly private canonical entry protocol. Generic
/// content writes (including composite field/layout hooks) may not mutate it.
pub(in crate::storage) fn forbid_generic_pattern_write(type_key: &str) -> ApiResult<()> {
    if type_key == "pattern" {
        Err(ApiError::forbidden(
            "Pattern chỉ được sửa qua API riêng tư /api/admin/patterns.",
        ))
    } else {
        Ok(())
    }
}

pub(in crate::storage) fn verify_supported_values(
    type_key: &str,
    values: &BTreeMap<String, Value>,
    settings: &ContentTypeSettings,
) -> ApiResult<()> {
    // These are protected Core fields only on Post/Page. A custom CPT may
    // legitimately define unrelated fields named `body` or `featured_image`.
    if !matches!(type_key, "post" | "page") {
        return Ok(());
    }
    if values.contains_key("body")
        && !(settings.supports(ContentSupport::Editor)
            || settings.supports(ContentSupport::Builder))
    {
        return Err(ApiError::unprocessable(
            "Không thể chỉnh sửa 'body' khi tính năng này bị tắt.",
        ));
    }
    if values.contains_key("featured_image") && !settings.supports(ContentSupport::FeaturedImage) {
        return Err(ApiError::unprocessable(
            "Không thể chỉnh sửa 'featured_image' khi tính năng này bị tắt.",
        ));
    }
    Ok(())
}

pub(in crate::storage) async fn sync_entry_body_refs_in_conn(
    conn: &mut sqlx::SqliteConnection,
    entry_id: i64,
    schema: &EffectiveContentSchema,
    values: &BTreeMap<String, Value>,
    previous_body: Option<&str>,
    actor_role: FieldRole,
) -> ApiResult<()> {
    if matches!(schema.base.descriptor.key.as_str(), "pattern" | "section") {
        return Ok(());
    }
    sqlx::query("DELETE FROM entry_pattern_refs WHERE source_entry_id = ?")
        .bind(entry_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "DELETE FROM entry_builder_media_refs WHERE entry_id = ? AND source_kind = 'builder'",
    )
    .bind(entry_id)
    .execute(&mut *conn)
    .await?;

    sync_entry_field_media_refs_in_conn(conn, entry_id, schema, values).await?;

    let Some(body_field) = schema.effective_fields.get("body") else {
        return Ok(());
    };
    if body_field.definition.field_type != opendash_core::field_schema::FieldType::Richtext {
        return Ok(());
    }
    let Some(raw_body) = values.get("body").and_then(Value::as_str) else {
        return Ok(());
    };
    if !opendash_core::builder_blocks::contains_blocks(raw_body) {
        return Ok(());
    }

    let catalog = crate::storage::page::load_unified_block_catalog_in_conn(conn).await?;
    let allowed_context_fields: Vec<String> = schema
        .effective_fields
        .iter()
        .filter(|(_, fe)| {
            fe.effective_status == EffectiveFieldStatus::Active
                && fe.definition.effective_acl().0 <= actor_role
                && fe.definition.effective_acl().2 == PublicExposure::Public
        })
        .map(|(k, _)| k.clone())
        .collect();
    crate::storage::fields::validate_comment_block_refs_in_conn(
        conn,
        raw_body,
        previous_body,
        &catalog,
        None,
        Some(&allowed_context_fields),
    )
    .await?;

    let refs = validated_entry_block_refs_in_conn(conn, raw_body).await?;
    if refs.pattern_ids.len() > 16 {
        return Err(ApiError::unprocessable(
            "Builder exceeds 16 Pattern references.",
        ));
    }
    let prev_patterns = previous_body
        .map(|b| {
            opendash_core::builder_blocks::extract_block_refs_lenient_with_schemas(b, &catalog)
                .pattern_ids
        })
        .unwrap_or_default();
    let mut existing_patterns = Vec::new();
    for &pid in &refs.pattern_ids {
        if pid <= 0 || pid == entry_id {
            return Err(ApiError::unprocessable(
                "Pattern tham chiếu không hợp lệ hoặc tự tham chiếu.",
            ));
        }
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM content_entries WHERE id = ? AND content_type_key = 'pattern')",
        )
        .bind(pid)
        .fetch_one(&mut *conn)
        .await?;
        if exists != 0 {
            existing_patterns.push(pid);
        } else if !prev_patterns.contains(&pid) {
            return Err(ApiError::unprocessable(
                "Pattern tham chiếu trong comment block không tồn tại.",
            ));
        }
    }

    let body_is_public = schema
        .base
        .descriptor
        .settings
        .supports(ContentSupport::Builder)
        && body_field.definition.effective_acl().2
            == opendash_core::content::PublicExposure::Public
        && body_field.definition.effective_acl().0 == FieldRole::Editor;
    if !body_is_public {
        return Ok(());
    }

    for pid in existing_patterns {
        sqlx::query(
            "INSERT INTO entry_pattern_refs(source_entry_id, target_pattern_id) VALUES (?, ?)",
        )
        .bind(entry_id)
        .bind(pid)
        .execute(&mut *conn)
        .await?;
    }
    for (role, ids) in [
        ("image", &refs.image_ids),
        ("video", &refs.video_ids),
        ("poster", &refs.poster_image_ids),
    ] {
        for media_id in ids {
            sqlx::query(
                "INSERT INTO entry_builder_media_refs(entry_id, media_id, ref_role) VALUES (?, ?, ?)",
            )
            .bind(entry_id)
            .bind(media_id)
            .bind(role)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

pub(super) async fn validate_effective_relational_refs_in_conn(
    conn: &mut SqliteConnection,
    effective: &EffectiveContentSchema,
    values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
    entry_locale: &str,
) -> ApiResult<()> {
    let specs: Vec<opendash_core::field_schema::FieldSpec> = effective
        .effective_fields
        .iter()
        .filter(|(k, _)| k.as_str() != "body")
        .map(|(_, fe)| fe.definition.clone())
        .collect();
    crate::storage::fields::validate_relational_refs_in_conn(
        conn,
        &specs,
        values,
        previous_values,
        Some(entry_locale),
    )
    .await
}

pub(in crate::storage) fn extract_body_from_values_json(values_json: &str) -> Option<String> {
    serde_json::from_str::<BTreeMap<String, Value>>(values_json)
        .ok()
        .and_then(|map| {
            map.get("body")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
}

pub async fn create_entry_with_fence(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &opendash_core::content::ContentTypeDefinition,
    input: &ContentEntryInput,
    fence: Option<&PluginActionFence>,
) -> ApiResult<ContentEntryRecord> {
    forbid_generic_pattern_write(type_key)?;
    let mut tx = begin_entry_write(db, actor).await?;
    let effective =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, type_key)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let settings = &effective.base.descriptor.settings;
    verify_supported_values(type_key, &input.values, settings)?;
    if let Some(f) = fence {
        verify_plugin_action_fence(&mut tx, actor, f).await?;
    }
    let entry_locale =
        resolve_entry_write_locale_in_tx(&mut tx, input.locale.as_deref(), None).await?;
    crate::storage::media::require_existing_references_in_tx(&mut tx, schema, input).await?;
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
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(record)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_entry_with_fence(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &opendash_core::content::ContentTypeDefinition,
    id: i64,
    expected_version: i64,
    input: &ContentEntryInput,
    fence: Option<&PluginActionFence>,
) -> ApiResult<ContentEntryRecord> {
    forbid_generic_pattern_write(type_key)?;
    let mut tx = begin_entry_write(db, actor).await?;
    let effective =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, type_key)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let settings = &effective.base.descriptor.settings;
    verify_supported_values(type_key, &input.values, settings)?;
    if let Some(f) = fence {
        verify_plugin_action_fence(&mut tx, actor, f).await?;
    }
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
    crate::storage::media::require_existing_references_in_tx(&mut tx, schema, input).await?;
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
            tx.commit().await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(record)
        }
    }
}

pub async fn create_entry_with_schema_fence(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &ContentTypeDefinition,
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
    crate::storage::media::require_existing_references_in_tx(&mut tx, schema, input).await?;
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
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(record)
}

pub async fn update_entry_with_schema_fence(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &ContentTypeDefinition,
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
    crate::storage::media::require_existing_references_in_tx(&mut tx, schema, input).await?;
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
            tx.commit().await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(record)
        }
    }
}
