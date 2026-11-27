use super::*;

pub async fn entry_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Query(pagination): Query<EntryPagination>,
) -> ApiResult<Json<Vec<entry_storage::ContentEntryRecord>>> {
    let actor = require_session(&headers, &db).await?;
    content_schema(&db, &type_key).await?;
    let effective = crate::storage::content::load_effective_content_schema(&db, &type_key).await?;
    let limit = pagination.limit.unwrap_or(50);
    let offset = pagination.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) || !(0..=1_000_000).contains(&offset) {
        return Err(ApiError::bad_request("Phân trang không hợp lệ."));
    }
    if pagination
        .category_id
        .into_iter()
        .chain(pagination.tag_id)
        .any(|id| id <= 0)
    {
        return Err(ApiError::bad_request("ID bộ lọc taxonomy phải dương."));
    }
    if pagination.include_descendants.unwrap_or(false) && pagination.category_id.is_none() {
        return Err(ApiError::bad_request(
            "include_descendants chỉ được dùng cùng category_id.",
        ));
    }
    let locale_filter = match pagination.locale.as_deref() {
        Some(raw) => {
            let trimmed = raw.trim().to_ascii_lowercase();
            if trimmed == "all" {
                None
            } else {
                Some(crate::storage::settings::resolve_requested_locale(&db, Some(&trimmed)).await?)
            }
        }
        None => None,
    };
    let filters = entry_storage::EntryFilters {
        category_id: pagination.category_id,
        tag_id: pagination.tag_id,
        include_descendants: pagination.include_descendants.unwrap_or(false),
        locale: locale_filter,
        ..entry_storage::EntryFilters::default()
    };
    let mut items = entry_storage::list_entries(&db, &type_key, limit, offset, filters).await?;
    if let Some(eff) = effective {
        let role = match actor.user.role {
            crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
            crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
        };
        for item in &mut items {
            entry_storage::filter_entry_values_for_role(item, &eff, role);
        }
    }
    Ok(Json(items))
}

pub async fn entry_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let mut entry = entry_storage::get_entry(&db, &type_key, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
    if let Some(eff) =
        crate::storage::content::load_effective_content_schema(&db, &type_key).await?
    {
        let role = match actor.user.role {
            crate::auth::Role::Admin => opendash_core::content::FieldRole::Admin,
            crate::auth::Role::Editor => opendash_core::content::FieldRole::Editor,
        };
        entry_storage::filter_entry_values_for_role(&mut entry, &eff, role);
    }
    Ok(([(header::ETAG, etag(entry.version))], Json(entry)).into_response())
}

pub async fn entry_create(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path(type_key): Path<String>,
    Json(mut input): Json<ContentEntryInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let schema = content_schema(&db, &type_key).await?;
    let effective = crate::storage::content::load_effective_content_schema(&db, &type_key).await?;
    let preflight_schema_token = effective.as_ref().map(|e| e.schema_token.clone());
    let actor_role = actor_field_role(&actor);
    let candidate_locale =
        crate::storage::settings::resolve_requested_locale(&db, input.locale.as_deref()).await?;
    if let Some(eff) = &effective {
        entry_storage::check_entry_role_gate_before_lookup(eff, &type_key, actor_role, &input.values)?;
        let mut conn = db.acquire().await?;
        let loc_ctx = entry_storage::build_entry_location_ctx_in_conn(
            &mut conn,
            &type_key,
            None,
            &candidate_locale,
            actor_role,
            None,
            None,
            input.term_ids.as_deref().unwrap_or(&[]),
        )
        .await?;
        input
            .validate_against_effective(eff, &loc_ctx)
            .map_err(ApiError::bad_request)?;
        media_storage::require_existing_effective_references(&db, eff, &input.values).await?;
        let prepare_payload = json!({
            "operation": "create",
            "type_key": &type_key,
            "entry_id": serde_json::Value::Null,
            "locale": &candidate_locale,
            "title": &input.title,
            "slug": &input.slug,
            "candidate_values": &input.values,
            "term_ids": &input.term_ids,
        });
        let patch = hooks
            .dispatch_content_prepare_save(&type_key, actor_role, eff, prepare_payload)
            .await?;
        if apply_prepare_save_patch_to_entry_input(&mut input, patch) {
            entry_storage::check_entry_role_gate_before_lookup(
                eff,
                &type_key,
                actor_role,
                &input.values,
            )?;
            let mut conn2 = db.acquire().await?;
            let loc_ctx2 = entry_storage::build_entry_location_ctx_in_conn(
                &mut conn2,
                &type_key,
                None,
                &candidate_locale,
                actor_role,
                None,
                None,
                input.term_ids.as_deref().unwrap_or(&[]),
            )
            .await?;
            input
                .validate_against_effective(eff, &loc_ctx2)
                .map_err(ApiError::unprocessable)?;
            media_storage::require_existing_effective_references(&db, eff, &input.values).await?;
        }
    }
    let candidate = json!({
        "operation": "create",
        "type_key": &type_key,
        "entry_id": serde_json::Value::Null,
        "expected_version": serde_json::Value::Null,
        "locale": &candidate_locale,
        "translation_group_id": serde_json::Value::Null,
        "title": &input.title,
        "slug": &input.slug,
        "candidate_values": &input.values,
        "term_ids": &input.term_ids,
    });
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let entry = match preflight_schema_token.as_deref() {
        Some(tok) => {
            entry_storage::create_entry_with_schema_fence(
                &db, &actor, &type_key, &schema, &input, tok,
            )
            .await?
        }
        None => entry_storage::create_entry(&db, &actor, &type_key, &schema, &input).await?,
    };
    crate::storage::plugin_projections::recompute_entry_projections(&db, &hooks, entry.id).await?;
    let principal = execution_principal_for_actor(&actor);
    if entry.publication_status == "published" {
        hooks.enqueue_async(crate::hooks::AsyncHookJob {
            event: opendash_core::hooks::HookEvent::ContentAfterPublish,
            principal,
            locale: Some(entry.locale.clone()),
            type_key: Some(type_key.clone()),
            payload: json!({
                "type_key": &type_key,
                "id": entry.id,
                "entry_id": entry.id,
                "version": entry.version,
                "locale": &entry.locale,
                "publication_status": &entry.publication_status,
                "translation_group_id": &entry.translation_group_id,
            }),
            dispatch_depth: 0,
            lease: lease.clone(),
        });
    }
    hooks.enqueue_async(crate::hooks::AsyncHookJob {
        event: opendash_core::hooks::HookEvent::ContentAfterSave,
        principal,
        locale: Some(entry.locale.clone()),
        type_key: Some(type_key.clone()),
        payload: json!({
            "type_key": &type_key,
            "id": entry.id,
            "entry_id": entry.id,
            "operation": "create",
            "version": entry.version,
            "locale": &entry.locale,
            "translation_group_id": &entry.translation_group_id,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok((
        StatusCode::CREATED,
        [
            (header::ETAG, etag(entry.version)),
            (
                header::LOCATION,
                format!("/api/admin/content-types/{type_key}/entries/{}", entry.id),
            ),
        ],
        Json(entry),
    )
        .into_response())
}

pub async fn entry_update(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(mut input): Json<ContentEntryInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let version = expected_version(&headers)?;
    let schema = content_schema(&db, &type_key).await?;
    let effective = crate::storage::content::load_effective_content_schema(&db, &type_key).await?;
    let preflight_schema_token = effective.as_ref().map(|e| e.schema_token.clone());
    let actor_role = actor_field_role(&actor);
    if let Some(eff) = &effective {
        entry_storage::check_entry_role_gate_before_lookup(eff, &type_key, actor_role, &input.values)?;
    }
    let preflight =
        crate::storage::entry_lifecycle::ensure_active_entry_version(&db, &type_key, id, version)
            .await?;
    let existing_meta = Some((preflight.locale, preflight.translation_group_id));
    if let (Some(req_loc), Some((cur_loc, _))) = (&input.locale, &existing_meta)
        && req_loc.trim().to_ascii_lowercase() != *cur_loc
    {
        return Err(ApiError::bad_request(
            "Không thể đổi ngôn ngữ của nội dung đã tạo.",
        ));
    }
    if let Some(eff) = &effective {
        let entry_locale = existing_meta
            .as_ref()
            .map(|(l, _)| l.as_str())
            .unwrap_or("vi");
        let existing_record = entry_storage::get_entry(&db, &type_key, id).await?;
        let ctx_terms = input
            .term_ids
            .as_deref()
            .or_else(|| existing_record.as_ref().map(|r| r.term_ids.as_slice()))
            .unwrap_or(&[]);
        let mut conn = db.acquire().await?;
        let loc_ctx = entry_storage::build_entry_location_ctx_in_conn(
            &mut conn,
            &type_key,
            Some(id),
            entry_locale,
            actor_role,
            existing_record.as_ref().and_then(|r| r.page_layout_key.clone()),
            existing_record.as_ref().and_then(|r| r.custom_template_key.clone()),
            ctx_terms,
        )
        .await?;
        input
            .validate_against_effective(eff, &loc_ctx)
            .map_err(ApiError::bad_request)?;
        media_storage::require_existing_effective_references(&db, eff, &input.values).await?;
        let prepare_payload = json!({
            "operation": "update",
            "type_key": &type_key,
            "entry_id": id,
            "locale": existing_meta.as_ref().map(|(l, _)| l.as_str()),
            "title": &input.title,
            "slug": &input.slug,
            "candidate_values": &input.values,
            "term_ids": &input.term_ids,
        });
        let patch = hooks
            .dispatch_content_prepare_save(&type_key, actor_role, eff, prepare_payload)
            .await?;
        if apply_prepare_save_patch_to_entry_input(&mut input, patch) {
            entry_storage::check_entry_role_gate_before_lookup(
                eff,
                &type_key,
                actor_role,
                &input.values,
            )?;
            let mut conn2 = db.acquire().await?;
            let ctx_terms2 = input
                .term_ids
                .as_deref()
                .or_else(|| existing_record.as_ref().map(|r| r.term_ids.as_slice()))
                .unwrap_or(&[]);
            let loc_ctx2 = entry_storage::build_entry_location_ctx_in_conn(
                &mut conn2,
                &type_key,
                Some(id),
                entry_locale,
                actor_role,
                existing_record.as_ref().and_then(|r| r.page_layout_key.clone()),
                existing_record.as_ref().and_then(|r| r.custom_template_key.clone()),
                ctx_terms2,
            )
            .await?;
            input
                .validate_against_effective(eff, &loc_ctx2)
                .map_err(ApiError::unprocessable)?;
            media_storage::require_existing_effective_references(&db, eff, &input.values).await?;
        }
    }
    let candidate = json!({
        "operation": "update",
        "type_key": &type_key,
        "entry_id": id,
        "expected_version": version,
        "locale": existing_meta.as_ref().map(|(l, _)| l.as_str()),
        "translation_group_id": existing_meta.as_ref().map(|(_, g)| g.as_str()),
        "title": &input.title,
        "slug": &input.slug,
        "candidate_values": &input.values,
        "term_ids": &input.term_ids,
    });
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(&type_key, actor_role, candidate)
        .await
    {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(veto)).into_response());
    }
    let entry = match preflight_schema_token.as_deref() {
        Some(tok) => {
            entry_storage::update_entry_with_schema_fence(
                &db, &actor, &type_key, &schema, id, version, &input, tok,
            )
            .await?
        }
        None => {
            entry_storage::update_entry(&db, &actor, &type_key, &schema, id, version, &input)
                .await?
        }
    };
    crate::storage::plugin_projections::recompute_entry_projections(&db, &hooks, entry.id).await?;
    hooks.enqueue_async(crate::hooks::AsyncHookJob {
        event: opendash_core::hooks::HookEvent::ContentAfterSave,
        principal: execution_principal_for_actor(&actor),
        locale: Some(entry.locale.clone()),
        type_key: Some(type_key.clone()),
        payload: json!({
            "type_key": &type_key,
            "id": entry.id,
            "entry_id": entry.id,
            "operation": "update",
            "version": entry.version,
            "locale": &entry.locale,
            "translation_group_id": &entry.translation_group_id,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok(([(header::ETAG, etag(entry.version))], Json(entry)).into_response())
}

pub async fn entry_delete(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let version = expected_version(&headers)?;
    valid_type_key(&type_key)?;
    let receipt = entry_storage::delete_entry(&db, &actor, &type_key, id, version).await?;
    hooks.enqueue_entry_lifecycle_hooks(execution_principal_for_actor(&actor), &receipt, lease);
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response())
}

pub async fn entry_composite_update(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    State(hooks): State<Arc<HookDispatcher>>,
    headers: HeaderMap,
    Path((type_key, id)): Path<(String, i64)>,
    Json(mut input): Json<entry_storage::CompositeEntrySaveInput>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    valid_type_key(&type_key)?;
    let before_status = entry_storage::get_entry(&db, &type_key, id)
        .await?
        .map(|e| e.publication_status);
    let mut plan =
        entry_storage::prepare_composite_entry_save(&db, &actor, &type_key, Some(id), &input)
            .await?;
    let actor_role = actor_field_role(&actor);
    if let Some(eff) =
        crate::storage::content::load_effective_content_schema(&db, &type_key).await?
    {
        let prepare_payload = json!({
            "operation": "update",
            "type_key": &type_key,
            "entry_id": id,
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
            plan = entry_storage::prepare_composite_entry_save(
                &db,
                &actor,
                &type_key,
                Some(id),
                &input,
            )
            .await?;
        }
    }
    let candidate = json!({
        "operation": "update",
        "type_key": &type_key,
        "entry_id": id,
        "expected_version": input.expected_version,
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
        entry_storage::commit_composite_entry_save(&db, &actor, &type_key, Some(id), &input, &plan)
            .await?;
    crate::storage::plugin_projections::recompute_entry_projections(&db, &hooks, result.entry.id)
        .await?;
    let principal = execution_principal_for_actor(&actor);
    if before_status.as_deref() != Some(result.entry.publication_status.as_str()) {
        let pub_event = if result.entry.publication_status == "published" {
            opendash_core::hooks::HookEvent::ContentAfterPublish
        } else {
            opendash_core::hooks::HookEvent::ContentAfterUnpublish
        };
        hooks.enqueue_async(crate::hooks::AsyncHookJob {
            event: pub_event,
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
            "operation": "update",
            "version": result.entry.version,
            "locale": &result.entry.locale,
            "translation_group_id": &result.entry.translation_group_id,
        }),
        dispatch_depth: 0,
        lease,
    });
    Ok(([(header::ETAG, etag(result.entry.version))], Json(result)).into_response())
}
