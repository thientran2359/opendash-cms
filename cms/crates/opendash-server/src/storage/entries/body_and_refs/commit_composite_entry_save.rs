use super::*;

pub async fn commit_composite_entry_save(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: Option<i64>,
    input: &CompositeEntrySaveInput,
    plan: &CompositePreflightPlan,
) -> ApiResult<CompositeEntrySaveResult> {
    let mut tx = begin_entry_write(db, actor).await?;
    let effective =
        verify_schema_token_in_tx(&mut tx, type_key, &plan.preflight_schema_token).await?;
    verify_supported_values(type_key, &input.values, &effective.base.descriptor.settings)?;
    let existing_locale: Option<String> = if let Some(id) = entry_id {
        let expected_ver = input
            .expected_version
            .ok_or_else(|| ApiError::bad_request("Thiếu phiên bản nội dung."))?;
        current_entry_in_write(&mut tx, type_key, id, expected_ver).await?;
        let loc: String = sqlx::query_scalar(
            "SELECT locale FROM content_entries WHERE content_type_key = ? AND id = ?",
        )
        .bind(type_key)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        Some(loc)
    } else {
        None
    };
    let entry_locale = resolve_entry_write_locale_in_tx(
        &mut tx,
        input.locale.as_deref(),
        existing_locale.as_deref(),
    )
    .await?;
    let requested_mode = match &input.content_mode {
        PatchField::Omitted => None,
        PatchField::Clear => return Err(ApiError::bad_request("content_mode không được null.")),
        PatchField::Set(mode) => {
            if *mode == ContentMode::Builder
                && !effective
                    .base
                    .descriptor
                    .settings
                    .supports(ContentSupport::Builder)
            {
                return Err(ApiError::unprocessable(
                    "Builder chưa được bật cho loại nội dung này.",
                ));
            }
            Some(mode.as_str())
        }
    };
    let is_draft_capable =
        effective.base.descriptor.settings.publication == ContentPublication::DraftPublish;
    if !is_draft_capable && plan.publication_status.as_deref() == Some("draft") {
        return Err(ApiError::unprocessable(
            "Loại nội dung lưu ngay không hỗ trợ trạng thái bản nháp.",
        ));
    }

    crate::storage::media::require_existing_references_in_tx(
        &mut tx,
        &plan.effective_ct_def,
        &plan.merged_input,
    )
    .await?;
    if let Some(ids) = &plan.merged_input.term_ids {
        validate_term_ids_for_locale(&mut tx, type_key, ids, &entry_locale).await?;
    }
    let effective_term_ids: Vec<i64> = match &plan.merged_input.term_ids {
        Some(ids) => ids.clone(),
        None => match entry_id {
            Some(id) => read_term_ids(&mut tx, id).await?,
            None => Vec::new(),
        },
    };
    let next_primary_category = resolve_primary_category_patch_in_tx(
        &mut tx,
        type_key,
        entry_id,
        &input.primary_category_term_id,
        plan.merged_input.term_ids.is_some(),
        &effective_term_ids,
    )
    .await?;

    let resolved_slug = if entry_id.is_none() && input.auto_unique_slug {
        let has_custom_path =
            matches!(&input.custom_path, PatchField::Set(raw) if !raw.trim().is_empty());
        resolve_auto_unique_slug_for_locale_in_tx(
            &mut tx,
            type_key,
            &plan.merged_input.slug,
            plan.merged_input.term_ids.as_deref(),
            next_primary_category,
            has_custom_path,
            Some(&entry_locale),
        )
        .await?
    } else {
        plan.merged_input.slug.clone()
    };

    let values_json = serde_json::to_string(&plan.merged_input.values)?;
    let (page_layout_key, custom_template_key) =
        resolve_entry_template_patches_in_tx(&mut tx, type_key, entry_id, input).await?;
    let custom_created_at = match input.created_at.as_deref() {
        Some(raw) if !raw.trim().is_empty() => Some(normalize_entry_timestamp(raw, "created_at")?),
        _ => None,
    };
    let custom_updated_at = match input.updated_at.as_deref() {
        Some(raw) if !raw.trim().is_empty() => Some(normalize_entry_timestamp(raw, "updated_at")?),
        _ => None,
    };

    let (saved_id, previous_body, was_ever_published) = match entry_id {
        None => {
            // Migration 0008 deliberately rejects a directly published INSERT
            // for the built-in types. Keep that security trigger intact and
            // transition to published *within this same writer transaction*.
            // No intermediate draft can be observed or committed.
            let group_id = crate::storage::settings::generate_translation_group_id("eg");
            let status_clause = if is_draft_capable {
                "draft"
            } else {
                "published"
            };
            let res = sqlx::query(
                "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, content_mode, page_layout_key, custom_template_key, primary_category_term_id, created_at, updated_at, published_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, COALESCE(?, 'editor'), ?, ?, ?, COALESCE(?, datetime('now')), COALESCE(?, ?, datetime('now')), CASE WHEN ? = 'published' THEN datetime('now') ELSE NULL END)",
            )
            .bind(type_key)
            .bind(&entry_locale)
            .bind(&group_id)
            .bind(&resolved_slug)
            .bind(&plan.merged_input.title)
            .bind(&values_json)
            .bind(status_clause)
            .bind(requested_mode)
            .bind(page_layout_key.as_deref())
            .bind(custom_template_key.as_deref())
            .bind(next_primary_category)
            .bind(custom_created_at.as_deref())
            .bind(custom_updated_at.as_deref())
            .bind(custom_created_at.as_deref())
            .bind(status_clause)
            .execute(&mut *tx)
            .await;
            let new_id = match res {
                Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                    return Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."));
                }
                other => other?.last_insert_rowid(),
            };
            if is_draft_capable && plan.publication_status.as_deref() == Some("published") {
                // Initial create remains v1; this is one logical atomic Save,
                // not a separate publication/CAS transition.
                sqlx::query(
                    "UPDATE content_entries
                        SET publication_status = 'published',
                            published_at = COALESCE(published_at, datetime('now'))
                      WHERE id = ? AND content_type_key = ? AND publication_status = 'draft'",
                )
                .bind(new_id)
                .bind(type_key)
                .execute(&mut *tx)
                .await?;
            }
            (new_id, None, false)
        }
        Some(id) => {
            let expected_ver = input.expected_version.unwrap();
            current_entry_in_write(&mut tx, type_key, id, expected_ver).await?;
            crate::storage::revisions::capture_baseline_if_missing_in_tx(&mut *tx, id).await?;
            let (old_values_json, was_ever_published_int): (String, i64) = sqlx::query_as(
                "SELECT values_json, has_been_published FROM content_entries WHERE content_type_key = ? AND id = ?",
            )
            .bind(type_key)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            let prev_body = extract_body_from_values_json(&old_values_json);
            let res = sqlx::query(
                "UPDATE content_entries
                 SET slug = ?, title = ?, values_json = ?,
                     publication_status = COALESCE(?, publication_status),
                     published_at = CASE
                         WHEN COALESCE(?, publication_status) = 'published'
                         THEN COALESCE(published_at, datetime('now'))
                         ELSE published_at
                     END,
                     page_layout_key = ?, custom_template_key = ?,
                     primary_category_term_id = ?,
                     content_mode = COALESCE(?, content_mode),
                     created_at = COALESCE(?, created_at),
                     version = version + 1,
                     updated_at = COALESCE(?, datetime('now'))
                 WHERE content_type_key = ? AND id = ? AND version = ? AND trashed_at IS NULL",
            )
            .bind(&resolved_slug)
            .bind(&plan.merged_input.title)
            .bind(&values_json)
            .bind(plan.publication_status.as_deref())
            .bind(plan.publication_status.as_deref())
            .bind(page_layout_key.as_deref())
            .bind(custom_template_key.as_deref())
            .bind(next_primary_category)
            .bind(requested_mode)
            .bind(custom_created_at.as_deref())
            .bind(custom_updated_at.as_deref())
            .bind(type_key)
            .bind(id)
            .bind(expected_ver)
            .execute(&mut *tx)
            .await;
            match res {
                Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                    return Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."));
                }
                other => {
                    if other?.rows_affected() == 0 {
                        return Err(ApiError::precondition_failed(
                            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
                        ));
                    }
                }
            }
            (id, prev_body, was_ever_published_int != 0)
        }
    };

    if let Some(ids) = &plan.merged_input.term_ids {
        replace_term_ids(&mut tx, saved_id, ids).await?;
    }
    sync_entry_body_refs_in_conn(
        &mut *tx,
        saved_id,
        &effective,
        &plan.merged_input.values,
        previous_body.as_deref(),
        actor_field_role(actor.user.role),
    )
    .await?;

    let row: EntryRow = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
         FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(saved_id)
    .fetch_one(&mut *tx)
    .await?;
    let mut record = decoded_with_terms(&mut tx, row).await?;

    let custom_path = crate::storage::seo::apply_entry_route_patch_in_tx(
        &mut tx,
        saved_id,
        &input.custom_path,
        was_ever_published,
    )
    .await?;
    let seo = crate::storage::seo::apply_entry_seo_patch_in_tx(
        &mut tx,
        saved_id,
        record.version,
        &input.seo,
    )
    .await?;
    crate::storage::seo::sync_entry_seo_media_ref_in_tx(&mut tx, saved_id).await?;

    if !was_ever_published {
        sqlx::query(
            "DELETE FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?",
        )
        .bind(saved_id.to_string())
        .execute(&mut *tx)
        .await?;
    }
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    let (rev_source, clear_uid) = if entry_id.is_some() {
        (
            opendash_core::revisions::RevisionSource::Save,
            Some(actor.user.id),
        )
    } else {
        (opendash_core::revisions::RevisionSource::Create, None)
    };
    crate::storage::revisions::record_entry_revision_and_clear_autosave_in_tx(
        &mut *tx,
        saved_id,
        rev_source,
        crate::storage::revisions::RevisionActor::User(actor),
        clear_uid,
    )
    .await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    filter_entry_values_for_role(&mut record, &effective, actor_field_role(actor.user.role));

    Ok(CompositeEntrySaveResult {
        entry: record,
        custom_path,
        seo,
        schema_token: effective.schema_token,
    })
}

pub async fn commit_entry_translation(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    source_id: i64,
    expected_source_version: i64,
    input: &CreateEntryTranslationInput,
    plan: &EntryTranslationPlan,
) -> ApiResult<ContentEntryRecord> {
    if matches!(type_key, "pattern" | "section") {
        return Err(ApiError::forbidden(
            "Không hỗ trợ tạo bản dịch trực tiếp cho Pattern/Section.",
        ));
    }
    let mut tx = begin_entry_write(db, actor).await?;
    current_entry_in_write(&mut tx, type_key, source_id, expected_source_version).await?;
    crate::storage::settings::require_multilingual_enabled_in_conn(&mut tx).await?;
    let source_active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM content_entries e JOIN site_locales sl
           ON sl.code = e.locale AND sl.deleted_at IS NULL WHERE e.id = ?)",
    )
    .bind(source_id)
    .fetch_one(&mut *tx)
    .await?;
    if !source_active {
        return Err(ApiError::not_found("Không tìm thấy ngôn ngữ."));
    }
    let effective =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, type_key)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let locale_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM site_locales WHERE code = ? AND deleted_at IS NULL)",
    )
    .bind(&plan.target_locale)
    .fetch_one(&mut *tx)
    .await?;
    if locale_exists == 0 {
        return Err(ApiError::unprocessable(
            "Ngôn ngữ đích không tồn tại trong site_locales.",
        ));
    }
    let dup_group: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM content_entries
            WHERE content_type_key = ? AND translation_group_id = ? AND locale = ?
         )",
    )
    .bind(type_key)
    .bind(&plan.translation_group_id)
    .bind(&plan.target_locale)
    .fetch_one(&mut *tx)
    .await?;
    if dup_group != 0 {
        return Err(ApiError::conflict(
            "Nhóm dịch đã có bản dịch ở ngôn ngữ đích.",
        ));
    }

    let resolved_slug = if input.auto_unique_slug {
        resolve_auto_unique_slug_for_locale_in_tx(
            &mut tx,
            type_key,
            &plan.slug,
            Some(&plan.term_ids),
            plan.primary_category_term_id,
            false,
            Some(&plan.target_locale),
        )
        .await?
    } else {
        plan.slug.clone()
    };

    let status =
        if effective.base.descriptor.settings.publication == ContentPublication::DraftPublish {
            "draft"
        } else {
            "published"
        };
    let values_json = serde_json::to_string(&plan.values)?;
    let res = sqlx::query(
        "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, published_at, content_mode, page_layout_key, custom_template_key, primary_category_term_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, CASE WHEN ? = 'published' THEN datetime('now') ELSE NULL END, ?, ?, ?, ?)",
    )
    .bind(type_key)
    .bind(&plan.target_locale)
    .bind(&plan.translation_group_id)
    .bind(&resolved_slug)
    .bind(&plan.title)
    .bind(&values_json)
    .bind(status)
    .bind(status)
    .bind(plan.content_mode.as_str())
    .bind(plan.page_layout_key.as_deref())
    .bind(plan.custom_template_key.as_deref())
    .bind(plan.primary_category_term_id)
    .execute(&mut *tx)
    .await;
    let new_id = match res {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::conflict("Slug đã tồn tại ở ngôn ngữ đích."));
        }
        other => other?.last_insert_rowid(),
    };

    if !plan.term_ids.is_empty() {
        validate_term_ids_for_locale(&mut tx, type_key, &plan.term_ids, &plan.target_locale)
            .await?;
        replace_term_ids(&mut tx, new_id, &plan.term_ids).await?;
    }

    let source_seo: Option<(String, String, Option<String>, i64)> = sqlx::query_as(
        "SELECT title, description, og_image_id, noindex FROM entry_seo WHERE entry_id = ?",
    )
    .bind(source_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((seo_title, seo_desc, og_image_id, noindex)) = source_seo {
        sqlx::query(
            "INSERT INTO entry_seo (entry_id, title, description, og_image_id, canonical_url, noindex)
             VALUES (?, ?, ?, ?, NULL, ?)",
        )
        .bind(new_id)
        .bind(&seo_title)
        .bind(&seo_desc)
        .bind(og_image_id.as_deref())
        .bind(noindex)
        .execute(&mut *tx)
        .await?;
        crate::storage::seo::sync_entry_seo_media_ref_in_tx(&mut tx, new_id).await?;
    }

    let source_values_json: Option<String> =
        sqlx::query_scalar("SELECT values_json FROM content_entries WHERE id = ?")
            .bind(source_id)
            .fetch_optional(&mut *tx)
            .await?;
    let source_body = source_values_json
        .as_deref()
        .and_then(extract_body_from_values_json);
    sync_entry_body_refs_in_conn(
        &mut *tx,
        new_id,
        &effective,
        &plan.values,
        source_body.as_deref(),
        actor_field_role(actor.user.role),
    )
    .await?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    crate::storage::revisions::record_entry_revision_and_clear_autosave_in_tx(
        &mut *tx,
        new_id,
        opendash_core::revisions::RevisionSource::Create,
        crate::storage::revisions::RevisionActor::User(actor),
        None,
    )
    .await?;

    let row: EntryRow = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
         FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(new_id)
    .fetch_one(&mut *tx)
    .await?;
    let mut record = decoded_with_terms(&mut tx, row).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    filter_entry_values_for_role(&mut record, &effective, actor_field_role(actor.user.role));
    Ok(record)
}

/// Legacy DELETE compatibility wrapper: at S02 activation this moves an authorized
/// active ordinary entry into recoverable Trash (never physical deletion).
pub async fn delete_entry(
    db: &SqlitePool,
    actor: &crate::auth::SessionContext,
    type_key: &str,
    id: i64,
    expected_version: i64,
) -> ApiResult<crate::storage::entry_lifecycle::EntryCommitReceipt> {
    forbid_generic_pattern_write(type_key)?;
    let cmd = crate::storage::entry_lifecycle::LifecycleCommand {
        operation: crate::storage::entry_lifecycle::LifecycleOperation::Trash,
        type_key: type_key.to_owned(),
        entry_id: id,
        expected_version,
    };
    let authority = crate::storage::entry_lifecycle::LifecycleAuthority {
        session: actor,
        plugin_fence: None,
        confirmation: crate::storage::entry_lifecycle::LifecycleConfirmationEvidence::None,
        scope: crate::storage::entry_lifecycle::LifecycleScope::Ordinary,
    };
    crate::storage::entry_lifecycle::execute_lifecycle_transition(db, &cmd, &authority).await
}
