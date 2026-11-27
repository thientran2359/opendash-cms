use super::*;

/// Narrow sparse metadata & single-taxonomy mutation shared by Quick Edit and Bulk Taxonomy.
pub async fn entry_metadata_patch(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(patch): Json<crate::storage::entry_lifecycle::MetadataPatch>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let expected_ver = expected_version(&headers)?;
    valid_type_key(&type_key)?;

    let mut plan = crate::storage::entry_lifecycle::prepare_metadata_patch(
        &db,
        &actor,
        &type_key,
        id,
        expected_ver,
        &patch,
    )
    .await?;
    let actor_role = actor_field_role(&actor);

    let prepare_payload = json!({
        "operation": "metadata",
        "type_key": &type_key,
        "entry_id": id,
        "locale": &plan.entry_locale,
        "title": &plan.candidate_title,
        "slug": &plan.candidate_slug,
        "candidate_values": &plan.candidate_values,
        "term_ids": &plan.candidate_term_ids,
    });
    let hook_patch = hooks
        .dispatch_content_prepare_save_with_scope(
            &type_key,
            actor_role,
            &plan.effective,
            prepare_payload,
            crate::hooks::PrepareSaveMutationScope::Metadata {
                requested_title: plan.requested_title,
                requested_slug: plan.requested_slug,
                taxonomy_key: plan.requested_taxonomy_key.clone(),
                entry_locale: plan.entry_locale.clone(),
            },
            Some(&db),
        )
        .await?;
    crate::storage::entry_lifecycle::apply_prepare_save_to_metadata_plan(&mut plan, hook_patch)?;

    let candidate = json!({
        "operation": "metadata",
        "type_key": &type_key,
        "entry_id": id,
        "expected_version": expected_ver,
        "locale": &plan.entry_locale,
        "translation_group_id": &plan.translation_group_id,
        "title": &plan.candidate_title,
        "slug": &plan.candidate_slug,
        "publication_status": &plan.candidate_status,
        "candidate_values": &plan.candidate_values,
        "term_ids": &plan.candidate_term_ids,
    });
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }

    let (row, schema_token, receipt) = crate::storage::entry_lifecycle::commit_metadata_patch(
        &db,
        &actor,
        &type_key,
        id,
        expected_ver,
        &patch,
        &plan,
    )
    .await?;

    let projection = crate::storage::entry_lifecycle::recompute_projections_nonfatal(
        &db,
        &hooks,
        receipt.id,
        receipt.publication_after.as_deref() == Some("published"),
    )
    .await;
    let hook_out =
        hooks.enqueue_entry_lifecycle_hooks(execution_principal_for_actor(&actor), &receipt, lease);

    let response = crate::storage::entry_lifecycle::MetadataPatchResponse {
        schema_token,
        secondary: crate::storage::entry_lifecycle::SecondaryOutcome {
            projection,
            hook_enqueued: hook_out.enqueued,
            hook_dropped: hook_out.dropped,
        },
        row,
    };

    Ok((
        StatusCode::OK,
        [
            (header::ETAG, etag(response.row.version)),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        Json(response),
    )
        .into_response())
}

pub(crate) async fn render_admin_single_fallback_html(
    db: &SqlitePool,
    record: &entry_storage::ContentEntryRecord,
    effective: &opendash_core::content::EffectiveContentSchema,
    actor_role: opendash_core::content::FieldRole,
    selected_template: Option<&str>,
) -> ApiResult<String> {
    let type_key = &record.content_type_key;
    let eligible =
        public_content::batch_load_eligible_public_taxonomies(db, std::slice::from_ref(type_key))
            .await?
            .remove(type_key)
            .unwrap_or_default();
    let terms = public_content::batch_load_public_entry_terms(db, &[(record.id, type_key.clone())])
        .await?
        .remove(&record.id)
        .unwrap_or_default();
    let mut richtext_nonces = crate::template_sec::RichTextNonceTable::default();
    let view = {
        let mut conn = db.acquire().await?;
        public_content::hydrate_single_entry_view_in_conn(
            &mut conn,
            record,
            effective,
            None,
            &terms,
            &eligible,
            actor_role,
            true,
            Some(&mut richtext_nonces),
        )
        .await?
    };
    let archive_url = {
        let mut conn = db.acquire().await?;
        permalinks::canonical_archive_path_in_tx(&mut conn, type_key, &record.locale).await?
    };
    let general = settings::get_general_for_locale(db, Some(&record.locale)).await?;
    let pub_settings = settings::get_public(db).await?;
    let site_ctx = content_render::SiteContext {
        title: &general.site_title,
        tagline: &general.tagline,
        timezone: &general.timezone,
        origin: pub_settings
            .public_origin
            .as_deref()
            .map(|s| s.trim().trim_end_matches('/'))
            .unwrap_or(""),
        current_year: content_render::current_year_in_timezone(&general.timezone),
    };
    let fmt_cfg = crate::template_sec::RenderFormattingConfig {
        timezone: general.timezone.clone(),
        ..Default::default()
    };
    let definition = &effective.base.descriptor;
    let breadcrumbs = content_render::single_breadcrumbs(
        content_render::DEFAULT_BREADCRUMB_HOME_LABEL,
        Some(content_render::DEFAULT_BREADCRUMB_HOME_URL),
        type_key,
        &definition.label,
        definition.settings.has_archive,
        archive_url.clone(),
        view.primary_category.as_ref(),
        &view.title,
        view.url.clone(),
    );
    content_render::render_single_with_context_and_nonces_for_locale(
        definition,
        &view,
        selected_template,
        &site_ctx,
        archive_url.as_deref(),
        &breadcrumbs,
        &fmt_cfg,
        &record.locale,
        Some(&richtext_nonces),
    )
}

pub async fn entry_composite_create(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Json(mut input): Json<entry_storage::CompositeEntrySaveInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let mut plan =
        entry_storage::prepare_composite_entry_save(&db, &actor, &type_key, None, &input).await?;
    let actor_role = actor_field_role(&actor);
    if let Some(eff) =
        crate::storage::content::load_effective_content_schema(&db, &type_key).await?
    {
        let prepare_payload = json!({
            "operation": "create",
            "type_key": &type_key,
            "entry_id": serde_json::Value::Null,
            "locale": &plan.locale,
            "title": &plan.merged_input.title,
            "slug": &plan.merged_input.slug,
            "candidate_values": &plan.merged_input.values,
            "term_ids": &plan.merged_input.term_ids,
        });
        let patch = hooks
            .dispatch_content_prepare_save(&type_key, actor_role, &eff, prepare_payload)
            .await?;
        if apply_prepare_save_patch_to_composite_input(&mut input, patch) {
            plan =
                entry_storage::prepare_composite_entry_save(&db, &actor, &type_key, None, &input)
                    .await?;
        }
    }
    let candidate = json!({
        "operation": "create",
        "type_key": &type_key,
        "entry_id": serde_json::Value::Null,
        "expected_version": serde_json::Value::Null,
        "locale": &plan.locale,
        "translation_group_id": &plan.translation_group_id,
        "title": &plan.merged_input.title,
        "slug": &plan.merged_input.slug,
        "candidate_values": &plan.merged_input.values,
        "term_ids": &plan.merged_input.term_ids,
    });
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let result =
        entry_storage::commit_composite_entry_save(&db, &actor, &type_key, None, &input, &plan)
            .await?;
    crate::storage::plugin_projections::recompute_entry_projections(&db, &hooks, result.entry.id)
        .await?;
    let principal = execution_principal_for_actor(&actor);
    if result.entry.publication_status == "published" {
        hooks.enqueue_async(crate::hooks::AsyncHookJob {
            event: opendash_core::hooks::HookEvent::ContentAfterPublish,
            principal,
            locale: Some(result.entry.locale.clone()),
            type_key: Some(type_key.clone()),
            payload: json!({
                "type_key": &type_key,
                "id": result.entry.id,
                "entry_id": result.entry.id,
                "version": result.entry.version,
                "locale": &result.entry.locale,
                "publication_status": &result.entry.publication_status,
                "translation_group_id": &result.entry.translation_group_id,
            }),
            dispatch_depth: 0,
            lease: lease.clone(),
        });
    }
    hooks.enqueue_async(crate::hooks::AsyncHookJob {
        event: opendash_core::hooks::HookEvent::ContentAfterSave,
        principal,
        locale: Some(result.entry.locale.clone()),
        type_key: Some(type_key.clone()),
        payload: json!({
            "type_key": &type_key,
            "id": result.entry.id,
            "entry_id": result.entry.id,
            "operation": "create",
            "version": result.entry.version,
            "locale": &result.entry.locale,
            "translation_group_id": &result.entry.translation_group_id,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok((
        StatusCode::CREATED,
        [
            (header::ETAG, etag(result.entry.version)),
            (
                header::LOCATION,
                format!(
                    "/api/admin/content-types/{type_key}/entries/{}",
                    result.entry.id
                ),
            ),
        ],
        Json(result),
    )
        .into_response())
}

pub async fn entry_translation_create(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(input): Json<entry_storage::CreateEntryTranslationInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let expected_source_version = expected_version(&headers)?;
    let plan = entry_storage::prepare_entry_translation(
        &db,
        &actor,
        &type_key,
        id,
        expected_source_version,
        &input,
    )
    .await?;
    let candidate = json!({
        "operation": "create_translation",
        "type_key": &type_key,
        "source_entry_id": id,
        "entry_id": serde_json::Value::Null,
        "expected_source_version": expected_source_version,
        "locale": &plan.target_locale,
        "translation_group_id": &plan.translation_group_id,
        "title": &plan.title,
        "slug": &plan.slug,
        "candidate_values": &plan.values,
        "term_ids": &plan.term_ids,
    });
    let actor_role = actor_field_role(&actor);
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let created = entry_storage::commit_entry_translation(
        &db,
        &actor,
        &type_key,
        id,
        expected_source_version,
        &input,
        &plan,
    )
    .await?;
    crate::storage::plugin_projections::recompute_entry_projections(&db, &hooks, created.id)
        .await?;
    let principal = execution_principal_for_actor(&actor);
    if created.publication_status == "published" {
        hooks.enqueue_async(crate::hooks::AsyncHookJob {
            event: opendash_core::hooks::HookEvent::ContentAfterPublish,
            principal,
            locale: Some(created.locale.clone()),
            type_key: Some(type_key.clone()),
            payload: json!({
                "type_key": &type_key,
                "id": created.id,
                "entry_id": created.id,
                "version": created.version,
                "locale": &created.locale,
                "publication_status": &created.publication_status,
                "translation_group_id": &created.translation_group_id,
            }),
            dispatch_depth: 0,
            lease: lease.clone(),
        });
    }
    hooks.enqueue_async(crate::hooks::AsyncHookJob {
        event: opendash_core::hooks::HookEvent::ContentAfterSave,
        principal,
        locale: Some(created.locale.clone()),
        type_key: Some(type_key.clone()),
        payload: json!({
            "type_key": &type_key,
            "id": created.id,
            "entry_id": created.id,
            "operation": "create_translation",
            "version": created.version,
            "locale": &created.locale,
            "translation_group_id": &created.translation_group_id,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok((
        StatusCode::CREATED,
        [
            (header::ETAG, etag(created.version)),
            (
                header::LOCATION,
                format!("/api/admin/content-types/{type_key}/entries/{}", created.id),
            ),
        ],
        Json(created),
    )
        .into_response())
}
