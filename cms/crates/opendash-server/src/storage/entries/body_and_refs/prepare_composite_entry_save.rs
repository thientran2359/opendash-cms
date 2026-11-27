use super::*;

pub async fn prepare_composite_entry_save(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: Option<i64>,
    input: &CompositeEntrySaveInput,
) -> ApiResult<CompositePreflightPlan> {
    forbid_generic_pattern_write(type_key)?;
    let client_token = input
        .expected_schema_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ApiError(
                axum::http::StatusCode::PRECONDITION_REQUIRED,
                "missing_schema_token: Thiếu expected_schema_token.".into(),
            )
        })?;

    let effective = crate::storage::content::load_effective_content_schema(db, type_key)
        .await?
        .ok_or_else(|| {
            ApiError::not_found(format!("Không tìm thấy loại nội dung '{type_key}'."))
        })?;

    if client_token != effective.schema_token {
        return Err(ApiError::conflict(format!(
            "schema_version_conflict: schema_token hiện tại là '{}'.",
            effective.schema_token
        )));
    }

    if entry_id.is_none() && input.expected_version.is_some() {
        return Err(ApiError::bad_request(
            "unexpected_version_on_create: Không truyền expected_version khi tạo mới.",
        ));
    }
    if entry_id.is_some() && input.expected_version.is_none() {
        return Err(ApiError(
            axum::http::StatusCode::PRECONDITION_REQUIRED,
            "missing_expected_version: Thiếu expected_version khi cập nhật.".into(),
        ));
    }

    if let Some(status) = &input.publication_status {
        if status != "draft" && status != "published" {
            return Err(ApiError::unprocessable(
                "publication_status chỉ hỗ trợ 'draft' hoặc 'published'.",
            ));
        }
        if effective.base.descriptor.settings.publication == ContentPublication::Immediate
            && status == "draft"
        {
            return Err(ApiError::unprocessable(
                "Loại nội dung lưu ngay không hỗ trợ trạng thái bản nháp.",
            ));
        }
    }
    verify_supported_values(type_key, &input.values, &effective.base.descriptor.settings)?;

    // Step B — Role Gate BEFORE reading existing entry / comparing values (no oracle leak)
    let caller_role = actor_field_role(actor.user.role);
    check_entry_role_gate_before_lookup(&effective, type_key, caller_role, &input.values)?;

    // Validate patch syntax before DB merge/tx
    if let PatchField::Set(raw_path) = &input.custom_path {
        let trimmed = raw_path.trim();
        if !trimmed.is_empty() {
            crate::seo::validate_custom_entry_path(trimmed).map_err(ApiError::unprocessable)?;
        }
    }
    if let PatchField::Set(seo_in) = &input.seo {
        crate::seo::validate_seo_input(seo_in)?;
    }
    // A missing provider must never erase a previously saved choice. New
    // assignments are validated against the *currently active* v3 manifest
    // again after the writer lock is acquired at commit time.
    if let PatchField::Set(tid) = input.primary_category_term_id {
        if type_key != "post" {
            return Err(ApiError::bad_request(
                "primary_category_term_id chỉ áp dụng cho bài viết (post).",
            ));
        }
        if tid <= 0 {
            return Err(ApiError::bad_request(
                "primary_category_term_id phải là ID nguyên dương.",
            ));
        }
    }
    if type_key != "page" && !matches!(input.page_layout_key, PatchField::Omitted) {
        return Err(ApiError::bad_request(
            "page_layout_key chỉ áp dụng cho Page.",
        ));
    }
    if type_key == "page" && !matches!(input.custom_template_key, PatchField::Omitted) {
        return Err(ApiError::bad_request(
            "custom_template_key không áp dụng cho Page.",
        ));
    }
    if !matches!(input.content_mode, PatchField::Omitted) {
        // The editor mode is always available as a safe fallback (including
        // after an Admin disables Builder). Entering Builder mode requires an
        // explicit capability, regardless of CPT key.
        if matches!(input.content_mode, PatchField::Set(ContentMode::Builder))
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
        if matches!(input.content_mode, PatchField::Clear) {
            return Err(ApiError::bad_request("content_mode không được null."));
        }
    }
    for patch in [&input.page_layout_key, &input.custom_template_key] {
        if let PatchField::Set(key) = patch {
            if !opendash_core::id::valid_snake_key(key) {
                return Err(ApiError::unprocessable("Tên layout/template không hợp lệ."));
            }
        }
    }

    let candidate_layout_override = match &input.page_layout_key {
        PatchField::Set(k) => Some(Some(k.clone())),
        PatchField::Clear => Some(None),
        PatchField::Omitted => None,
    };
    let candidate_template_override = match &input.custom_template_key {
        PatchField::Set(k) => Some(Some(k.clone())),
        PatchField::Clear => Some(None),
        PatchField::Omitted => None,
    };

    let (
        plan_locale,
        plan_group_id,
        merged_values,
        merged_term_ids,
        preflight_prev_body,
        preflight_prev_values,
        next_parent_id,
        next_menu_order,
        loc_ctx,
    ) = match entry_id {
        None => {
            let mut conn = db.acquire().await?;
            let loc =
                resolve_entry_write_locale_in_tx(&mut conn, input.locale.as_deref(), None).await?;
            let (next_parent_id, next_menu_order) = resolve_entry_hierarchy_patch_in_conn(
                &mut conn,
                type_key,
                None,
                &input.parent_id,
                input.menu_order,
            )
            .await?;
            validate_entry_parent_in_conn(
                &mut conn,
                type_key,
                &effective.base.descriptor.settings,
                &loc,
                None,
                next_parent_id,
            )
            .await?;
            let layout = candidate_layout_override.flatten();
            let template = candidate_template_override.flatten();
            let non_tax_ctx = build_entry_location_ctx_in_conn(
                &mut conn,
                type_key,
                None,
                &loc,
                caller_role,
                layout.clone(),
                template.clone(),
                &[],
            )
            .await?;
            let synced_terms = compute_sync_entry_terms_step_a_in_conn(
                &mut conn,
                &effective,
                &non_tax_ctx,
                caller_role,
                &input.values,
                input.term_ids.as_deref(),
                &[],
            )
            .await?;
            let ctx_terms = synced_terms.as_deref().unwrap_or(&[]);
            let loc_ctx = build_entry_location_ctx_in_conn(
                &mut conn,
                type_key,
                None,
                &loc,
                caller_role,
                layout,
                template,
                ctx_terms,
            )
            .await?;
            let mut out = apply_location_aware_value_merge(
                &effective,
                &loc_ctx,
                &input.values,
                None,
                true,
            );
            if let Some(ref tids) = synced_terms {
                sync_term_ref_fields_from_entry_terms_in_conn(
                    &mut conn,
                    &effective,
                    &loc_ctx,
                    &mut out,
                    tids,
                    Some(&input.values),
                )
                .await?;
            }
            (
                loc,
                None,
                out,
                synced_terms,
                None,
                None,
                next_parent_id,
                next_menu_order,
                loc_ctx,
            )
        }
        Some(id) => {
            let expected_ver = input.expected_version.unwrap();
            crate::storage::entry_lifecycle::ensure_active_entry_version(
                db,
                type_key,
                id,
                expected_ver,
            )
            .await?;
            let existing = get_entry(db, type_key, id)
                .await?
                .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
            if existing.version != expected_ver {
                return Err(ApiError::precondition_failed(
                    "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
                ));
            }
            let mut conn = db.acquire().await?;
            let loc = resolve_entry_write_locale_in_tx(
                &mut conn,
                input.locale.as_deref(),
                Some(&existing.locale),
            )
            .await?;
            let (next_parent_id, next_menu_order) = resolve_entry_hierarchy_patch_in_conn(
                &mut conn,
                type_key,
                Some(id),
                &input.parent_id,
                input.menu_order,
            )
            .await?;
            validate_entry_parent_in_conn(
                &mut conn,
                type_key,
                &effective.base.descriptor.settings,
                &loc,
                Some(id),
                next_parent_id,
            )
            .await?;
            let layout = candidate_layout_override.unwrap_or(existing.page_layout_key);
            let template = candidate_template_override.unwrap_or(existing.custom_template_key);
            let non_tax_ctx = build_entry_location_ctx_in_conn(
                &mut conn,
                type_key,
                Some(id),
                &loc,
                caller_role,
                layout.clone(),
                template.clone(),
                &[],
            )
            .await?;
            let synced_terms = compute_sync_entry_terms_step_a_in_conn(
                &mut conn,
                &effective,
                &non_tax_ctx,
                caller_role,
                &input.values,
                input.term_ids.as_deref(),
                &existing.term_ids,
            )
            .await?;
            let ctx_terms = synced_terms.as_deref().unwrap_or(&existing.term_ids);
            let loc_ctx = build_entry_location_ctx_in_conn(
                &mut conn,
                type_key,
                Some(id),
                &loc,
                caller_role,
                layout,
                template,
                ctx_terms,
            )
            .await?;
            let prev_body = existing
                .values
                .get("body")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let prev_values = existing.values.clone();
            let mut out = apply_location_aware_value_merge(
                &effective,
                &loc_ctx,
                &input.values,
                Some(&existing.values),
                false,
            );
            if let Some(ref tids) = synced_terms {
                sync_term_ref_fields_from_entry_terms_in_conn(
                    &mut conn,
                    &effective,
                    &loc_ctx,
                    &mut out,
                    tids,
                    Some(&input.values),
                )
                .await?;
            }
            (
                loc,
                Some(existing.translation_group_id),
                out,
                synced_terms,
                prev_body,
                Some(prev_values),
                next_parent_id,
                next_menu_order,
                loc_ctx,
            )
        }
    };

    let merged_input = ContentEntryInput {
        title: input.title.clone(),
        slug: input.slug.clone(),
        locale: Some(plan_locale.clone()),
        values: merged_values,
        term_ids: merged_term_ids,
    };
    merged_input
        .validate_against_effective(&effective, &loc_ctx)
        .map_err(ApiError::unprocessable)?;

    crate::storage::media::require_existing_effective_references(
        db,
        &effective,
        &merged_input.values,
    )
    .await?;
    let mut conn = db.acquire().await?;
    validate_effective_relational_refs_in_conn(
        &mut conn,
        &effective,
        &merged_input.values,
        preflight_prev_values.as_ref(),
        &plan_locale,
    )
    .await?;
    if let Some(body) = merged_input.values.get("body").and_then(Value::as_str)
        && effective.effective_fields.get("body").is_some_and(|field| {
            field.definition.field_type == opendash_core::field_schema::FieldType::Richtext
        })
        && opendash_core::builder_blocks::contains_blocks(body)
    {
        validated_entry_block_refs_in_conn(&mut conn, body).await?;
        let catalog = crate::storage::page::load_unified_block_catalog_in_conn(&mut conn).await?;
        let caller_role = actor_field_role(actor.user.role);
        let allowed_context_fields: Vec<String> = effective
            .effective_fields
            .iter()
            .filter(|(_, fe)| {
                fe.effective_status == EffectiveFieldStatus::Active
                    && fe.definition.effective_acl().0 <= caller_role
                    && fe.definition.effective_acl().2 == PublicExposure::Public
            })
            .map(|(k, _)| k.clone())
            .collect();
        crate::storage::fields::validate_comment_block_refs_in_conn(
            &mut conn,
            body,
            preflight_prev_body.as_deref(),
            &catalog,
            None,
            Some(&allowed_context_fields),
        )
        .await?;
    }

    Ok(CompositePreflightPlan {
        preflight_schema_token: effective.schema_token.clone(),
        locale: plan_locale,
        translation_group_id: plan_group_id,
        effective,
        merged_input,
        publication_status: input.publication_status.clone(),
        parent_id: next_parent_id,
        menu_order: next_menu_order,
        loc_ctx,
    })
}
