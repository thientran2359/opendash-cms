use super::*;

pub(in crate::storage) async fn mutate_entry_builder(
    db: &SqlitePool,
    actor: &SessionContext,
    hooks: &HookDispatcher,
    type_key: &str,
    id: i64,
    expected_version: i64,
    mutation: EntryBuilderMutation<'_>,
) -> Result<EntryBuilderOpen, EntryBuilderMutationError> {
    if type_key == "pattern" && !matches!(actor.user.role, Role::Admin) {
        return Err(ApiError::forbidden("Pattern Builder requires Admin.").into());
    }
    let role = actor_field_role(actor);
    // Complete read-only preflight before plugin invocation. No writer lease is
    // held during the bounded, veto-capable hook.
    let mut read_tx = db.begin().await?;
    let before = entry_builder_snapshot(&mut read_tx, type_key, id).await?;
    if type_key != "pattern" {
        check_entry_body_edit_acl(&before.schema, actor)?;
    }
    check_entry_builder_cas(&before, expected_version, &mutation)?;
    if let EntryBuilderMutation::Save(document) = &mutation {
        let mut state =
            validate_entry_builder_document(&mut read_tx, &before, role, document).await?;
        render_entry_builder_preview(&mut read_tx, &before, &mut state, document, false).await?;
    }
    read_tx.commit().await?;
    let mut candidate_values = before.values.clone();
    if type_key != "pattern" {
        match &mutation {
            EntryBuilderMutation::Save(document) => {
                let candidate_blocks =
                    opendash_core::builder_blocks::serialize_builder_document_to_blocks(document)
                        .map_err(|err| ApiError::bad_request(err.to_string()))?;
                candidate_values.insert("body".to_owned(), Value::String(candidate_blocks));
            }
            EntryBuilderMutation::Reset => {
                let plain = opendash_core::builder_blocks::extract_visible_text(
                    before.raw_body.as_deref().unwrap_or(""),
                );
                if plain.is_empty() {
                    candidate_values.remove("body");
                } else {
                    candidate_values.insert("body".to_owned(), Value::String(plain));
                }
            }
        }
    }
    let operation = mutation.operation();
    hooks
        .dispatch_content_before_save_for_actor(
            type_key,
            role,
            json!({
                "operation": "update",
                "type_key": type_key,
                "entry_id": id,
                "expected_version": expected_version,
                "locale": &before.locale,
                "translation_group_id": &before.translation_group_id,
                "schema_token": &before.schema.schema_token,
                "title": &before.title,
                "slug": &before.slug,
                "candidate_values": &candidate_values,
                "term_ids": &before.term_ids,
                "builder_operation": operation,
            }),
        )
        .await
        .map_err(EntryBuilderMutationError::Veto)?;

    // Same writer reservation and session/role recheck as ordinary entry edits.
    let mut tx = entries::begin_entry_write(db, actor).await?;
    let current = entry_builder_snapshot(&mut tx, type_key, id).await?;
    if type_key != "pattern" {
        check_entry_body_edit_acl(&current.schema, actor)?;
    }
    check_entry_builder_cas(&current, expected_version, &mutation)?;
    if current.schema.schema_token != before.schema.schema_token {
        return Err(ApiError::precondition_failed(
            "Content schema changed during Builder operation.",
        )
        .into());
    }

    if type_key == "pattern" {
        let (mode, document) = match mutation {
            EntryBuilderMutation::Save(document) => {
                let mut state =
                    validate_entry_builder_document(&mut tx, &current, role, document).await?;
                render_entry_builder_preview(&mut tx, &current, &mut state, document, false)
                    .await?;
                let raw = String::from_utf8(
                    document
                        .to_json()
                        .map_err(|err| ApiError::bad_request(err.to_string()))?,
                )
                .map_err(|_| ApiError::bad_request("Invalid Builder JSON encoding."))?;
                sqlx::query(
                    "INSERT INTO entry_builder_documents(entry_id, document_json, updated_at)
                     VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     ON CONFLICT(entry_id) DO UPDATE SET document_json=excluded.document_json, updated_at=excluded.updated_at",
                )
                .bind(id)
                .bind(raw)
                .execute(&mut *tx)
                .await?;
                (ContentMode::Builder, Some(document))
            }
            EntryBuilderMutation::Reset => {
                sqlx::query("DELETE FROM entry_builder_documents WHERE entry_id = ?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                (ContentMode::Editor, None)
            }
        };
        replace_entry_builder_refs(&mut tx, id, document).await?;
        let mode_db = match mode {
            ContentMode::Editor => "editor",
            ContentMode::Builder => "builder",
        };
        let affected = sqlx::query(
            "UPDATE content_entries SET content_mode=?, version=version+1,
                     updated_at=strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
              WHERE id=? AND content_type_key=? AND version=?",
        )
        .bind(mode_db)
        .bind(id)
        .bind(type_key)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(
                ApiError::precondition_failed("Entry changed during Builder operation.").into(),
            );
        }
    } else {
        let (mode_db, next_values, ref_doc) = match mutation {
            EntryBuilderMutation::Save(document) => {
                let mut state =
                    validate_entry_builder_document(&mut tx, &current, role, document).await?;
                // Enforce the same renderer output/node/list/form limits even when
                // the caller saves directly without ever requesting Preview.
                render_entry_builder_preview(&mut tx, &current, &mut state, document, false)
                    .await?;
                let body_blocks =
                    opendash_core::builder_blocks::serialize_builder_document_to_blocks(document)
                        .map_err(|err| ApiError::bad_request(err.to_string()))?;
                let body_val = Value::String(body_blocks);
                if let Some(body_field) = current.schema.effective_fields.get("body")
                    && !opendash_core::content::valid_field_value(&body_field.definition, &body_val)
                {
                    return Err(ApiError::bad_request(
                        "Comment block body exceeds RichText validation limit.",
                    )
                    .into());
                }
                let mut values = current.values.clone();
                values.insert("body".to_owned(), body_val);
                let ref_doc = if body_allows_public_refs(&current.schema) {
                    Some(document)
                } else {
                    None
                };
                ("builder", values, ref_doc)
            }
            EntryBuilderMutation::Reset => {
                let plain = opendash_core::builder_blocks::extract_visible_text(
                    current.raw_body.as_deref().unwrap_or(""),
                );
                let mut values = current.values.clone();
                if plain.is_empty() {
                    values.remove("body");
                } else {
                    values.insert("body".to_owned(), Value::String(plain));
                }
                ("editor", values, None)
            }
        };
        let next_values_json = serde_json::to_string(&next_values).map_err(ApiError::from)?;
        if next_values_json.len() > crate::routes::entries::MAX_CONTENT_ENTRY_JSON_BYTES {
            return Err(ApiError::bad_request("Entry JSON exceeds 768 KiB limit.").into());
        }
        let affected = sqlx::query(
            "UPDATE content_entries
                SET values_json=?, content_mode=?, version=version+1,
                    updated_at=strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
              WHERE id=? AND content_type_key=? AND version=? AND trashed_at IS NULL",
        )
        .bind(next_values_json)
        .bind(mode_db)
        .bind(id)
        .bind(type_key)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(
                ApiError::precondition_failed("Entry changed during Builder operation.").into(),
            );
        }
        sqlx::query("DELETE FROM entry_builder_documents WHERE entry_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        replace_entry_builder_refs(&mut tx, id, ref_doc).await?;
    }
    let saved = entry_builder_snapshot(&mut tx, type_key, id).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    let _ = crate::storage::plugin_projections::recompute_entry_projections(db, hooks, id).await;
    hooks
        .dispatch_content_after_save_for_actor(
            type_key,
            role,
            json!({
                "operation": "update",
                "entry_id": id,
                "version": saved.opened.version,
                "type_key": type_key,
                "locale": &saved.locale,
                "translation_group_id": &saved.translation_group_id,
                "values": &saved.values,
                "term_ids": &saved.term_ids,
                "builder_operation": operation,
            }),
        )
        .await;
    Ok(saved.opened)
}
