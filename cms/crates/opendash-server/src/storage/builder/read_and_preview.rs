use super::*;

pub(in crate::storage) async fn entry_builder_snapshot(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    id: i64,
) -> ApiResult<EntryBuilderSnapshot> {
    // The deprecated Section CPT must never be reopened via the generic API.
    if type_key == "section" {
        return Err(ApiError::forbidden("Content Builder is not enabled."));
    }
    let schema = crate::storage::content::load_effective_content_schema_in_tx(&mut **tx, type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Content type not found."))?;
    if schema.base.status != ProviderStatus::Active
        || schema.base.declaration_state != DeclarationState::Declared
        || !schema
            .base
            .descriptor
            .settings
            .supports
            .contains(&ContentSupport::Builder)
    {
        return Err(ApiError::forbidden("Content Builder is not enabled."));
    }
    let (
        locale,
        translation_group_id,
        title,
        slug,
        values_json,
        mode,
        publication_status,
        version,
        updated_at,
        raw_doc,
        page_layout_key,
        custom_template_key,
        trashed_at,
    ): (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = if type_key == "pattern" {
        let row: (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
            String,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT e.locale, e.translation_group_id, e.title, e.slug, e.values_json, e.content_mode, e.publication_status, e.version, e.updated_at, d.document_json
               FROM content_entries e LEFT JOIN entry_builder_documents d ON d.entry_id = e.id
              WHERE e.id = ? AND e.content_type_key = ?",
        )
        .bind(id)
        .bind(type_key)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry not found."))?;
        (
            row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9, None, None, None,
        )
    } else {
        let row: Option<(
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT locale, translation_group_id, title, slug, values_json, content_mode, publication_status, version, updated_at,
                    page_layout_key, custom_template_key, trashed_at
               FROM content_entries
              WHERE id = ? AND content_type_key = ?",
        )
        .bind(id)
        .bind(type_key)
        .fetch_optional(&mut **tx)
        .await?;
        let (
            locale,
            translation_group_id,
            title,
            slug,
            values_json,
            mode,
            publication_status,
            version,
            updated_at,
            page_layout_key,
            custom_template_key,
            trashed_at,
        ) = row.ok_or_else(|| ApiError::not_found("Content entry not found."))?;
        (
            locale,
            translation_group_id,
            title,
            slug,
            values_json,
            mode,
            publication_status,
            version,
            updated_at,
            None,
            page_layout_key,
            custom_template_key,
            trashed_at,
        )
    };
    let content_mode = match mode.as_str() {
        "editor" => ContentMode::Editor,
        "builder" => ContentMode::Builder,
        _ => return Err(ApiError::internal("Invalid stored entry content mode.")),
    };
    let values: BTreeMap<String, Value> = serde_json::from_str(&values_json)?;
    let raw_body = values
        .get("body")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let raw_body_has_blocks = raw_body
        .as_deref()
        .is_some_and(opendash_core::builder_blocks::contains_blocks);
    let (document, diagnostics, had_freeform_wrap, had_recovery) = if type_key == "pattern" {
        let doc = raw_doc
            .map(|raw| BuilderDocument::from_json(raw.as_bytes()))
            .transpose()
            .map_err(|_| ApiError::internal("Stored entry Builder document is not readable."))?;
        (doc, Vec::new(), false, false)
    } else {
        match raw_body.as_deref() {
            Some(raw) if !raw.trim().is_empty() => {
                let parsed = opendash_core::builder_blocks::parse_blocks_lenient_or_recover(raw);
                (
                    Some(parsed.document),
                    parsed.diagnostics,
                    parsed.had_freeform_wrap,
                    parsed.had_recovery,
                )
            }
            _ => (None, Vec::new(), false, false),
        }
    };
    let publication_policy = schema.base.descriptor.settings.publication;
    let term_ids = entries::read_term_ids(tx, id).await?;
    Ok(EntryBuilderSnapshot {
        opened: EntryBuilderOpen {
            entry_id: id,
            type_key: type_key.to_owned(),
            content_mode,
            version,
            updated_at,
            document,
            publication_status,
            publication_policy,
            diagnostics,
            had_freeform_wrap,
            had_recovery,
        },
        schema,
        locale,
        translation_group_id,
        title,
        slug,
        values,
        term_ids,
        raw_body,
        raw_body_has_blocks,
        page_layout_key,
        custom_template_key,
        trashed_at,
    })
}

pub async fn read_entry_builder(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
) -> ApiResult<EntryBuilderOpen> {
    if type_key == "pattern" && !matches!(actor.user.role, Role::Admin) {
        return Err(ApiError::forbidden("Pattern Builder requires Admin."));
    }
    let mut tx = db.begin().await?;
    let current = entry_builder_snapshot(&mut tx, type_key, id).await?;
    if type_key != "pattern" {
        check_entry_body_edit_acl(&current.schema, actor)?;
        if current.trashed_at.is_some() {
            return Err(ApiError::conflict(
                "entry_trashed: Nội dung đang ở trong Thùng rác và không thể mở trong Trình dựng trang.",
            ));
        }
    }
    tx.commit().await?;
    Ok(current.opened)
}

/// Shared-template preview uses the selected CPT body inside the caller's
/// read snapshot. A missing saved block document is handled by the text renderer.
#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn preview_saved_page_body_in_tx_with_options_and_budget(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
    site_options_override: Option<&BTreeMap<String, serde_json::Value>>,
    options_override: Option<&BTreeMap<String, serde_json::Value>>,
    timezone_override: Option<&str>,
    budget: &mut page::RenderBudget,
    cache: &page::BlockTemplateCache,
) -> ApiResult<Option<RenderedLayout>> {
    let current = entry_builder_snapshot(tx, type_key, id).await?;
    if type_key != "pattern" && current.trashed_at.is_some() {
        return Err(ApiError::not_found("Content entry not found."));
    }
    if !current.raw_body_has_blocks {
        return Ok(None);
    }
    let Some(document) = current.opened.document.as_ref() else {
        return Ok(None);
    };
    let mut state =
        validate_entry_builder_document(tx, &current, actor_field_role(actor), document).await?;
    if let Some(so) = site_options_override {
        state.site_options = so.clone();
    }
    if let Some(opts) = options_override {
        state.options = opts.clone();
    }
    if let Some(tz) = timezone_override {
        state.timezone = tz.to_owned();
    }
    Ok(Some(
        render_entry_builder_preview_with_budget(
            tx,
            &current,
            &mut state,
            document,
            true,
            RenderAnnotationScope::None,
            budget,
            cache,
        )
        .await?,
    ))
}

pub async fn preview_entry_builder(
    db: &SqlitePool,
    themes_root: &Path,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
    document: &BuilderDocument,
) -> ApiResult<RenderedLayout> {
    preview_entry_builder_with_cache(
        db,
        themes_root,
        actor,
        type_key,
        id,
        document,
        page::BlockTemplateCache::global(),
    )
    .await
}

pub async fn preview_entry_builder_with_cache(
    db: &SqlitePool,
    themes_root: &Path,
    actor: &SessionContext,
    type_key: &str,
    id: i64,
    document: &BuilderDocument,
    cache: &page::BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    let mut tx = db.begin().await?;
    let current = entry_builder_snapshot(&mut tx, type_key, id).await?;
    if type_key != "pattern" {
        check_entry_body_edit_acl(&current.schema, actor)?;
        if current.trashed_at.is_some() {
            return Err(ApiError::conflict(
                "entry_trashed: Nội dung đang ở trong Thùng rác và không thể xem trước trong Trình dựng trang.",
            ));
        }
    }
    let actor_role = actor_field_role(actor);
    let mut state =
        validate_entry_builder_document(&mut tx, &current, actor_role, document).await?;
    let scope = if type_key == "pattern" {
        RenderAnnotationScope::None
    } else {
        RenderAnnotationScope::EntryBodyOnly
    };
    let mut render_budget = page::RenderBudget::new();
    let rendered = render_entry_builder_preview_with_budget(
        &mut tx,
        &current,
        &mut state,
        document,
        false,
        scope,
        &mut render_budget,
        cache,
    )
    .await?;
    let out = if type_key == "pattern" {
        let mut rendered = rendered;
        if !render_budget.hook_nonces.is_empty() {
            let clear: Vec<(String, String)> = render_budget
                .hook_nonces
                .entries()
                .iter()
                .map(|h| (h.nonce.clone(), String::new()))
                .collect();
            rendered.html = crate::content_render::apply_render_hooks_to_document(
                rendered.html,
                "",
                &[],
                &[],
                "",
                &clear,
            )?;
        }
        rendered.html = crate::content_render::finalize_document_icons(
            rendered.html,
            &render_budget.icon_nonces,
        )?;
        rendered.html = crate::content_render::finalize_document_richtext(
            rendered.html,
            &render_budget.richtext_nonces,
        )?;
        rendered
    } else {
        crate::storage::overrides::wrap_entry_body_in_preview_shell_in_tx_with_budget(
            &mut tx,
            themes_root,
            actor_role,
            &current.schema,
            id,
            current.page_layout_key.as_deref(),
            current.custom_template_key.as_deref(),
            rendered,
            render_budget,
            cache,
        )
        .await?
    };
    tx.rollback().await?;
    Ok(out)
}

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
