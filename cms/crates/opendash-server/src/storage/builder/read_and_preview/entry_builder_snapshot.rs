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
                let catalog =
                    crate::storage::page::load_unified_block_catalog_in_conn(&mut **tx).await?;
                let parsed =
                    opendash_core::builder_blocks::parse_blocks_lenient_with_schemas(raw, &catalog);
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
