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

    // Check Field ACL BEFORE reading existing entry / comparing values (no value-comparison oracle)
    let caller_role = actor_field_role(actor.user.role);
    for key in input.values.keys() {
        let Some(fe) = effective.effective_fields.get(key) else {
            return Err(ApiError::unprocessable(format!(
                "unknown_field_key: Trường '{key}' không thuộc loại nội dung '{type_key}'."
            )));
        };
        if caller_role < fe.definition.effective_acl().1 {
            return Err(ApiError::forbidden(format!(
                "field_edit_forbidden: Vai trò hiện tại không có quyền sửa trường '{key}'."
            )));
        }
    }

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

    let (plan_locale, plan_group_id, merged_values, preflight_prev_body) = match entry_id {
        None => {
            let mut conn = db.acquire().await?;
            let loc =
                resolve_entry_write_locale_in_tx(&mut conn, input.locale.as_deref(), None).await?;
            let mut out = BTreeMap::new();
            for (k, fe) in &effective.effective_fields {
                if !input.values.contains_key(k)
                    && fe.effective_status != EffectiveFieldStatus::Withdrawn
                    && let Some(def) = &fe.definition.default
                {
                    out.insert(k.clone(), def.clone());
                }
            }
            for (k, v) in &input.values {
                if !v.is_null() {
                    out.insert(k.clone(), v.clone());
                }
            }
            (loc, None, out, None)
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
            let prev_body = existing
                .values
                .get("body")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let mut out = existing.values;
            for (k, v) in &input.values {
                if v.is_null() {
                    out.remove(k);
                } else {
                    out.insert(k.clone(), v.clone());
                }
            }
            (loc, Some(existing.translation_group_id), out, prev_body)
        }
    };

    let merged_input = ContentEntryInput {
        title: input.title.clone(),
        slug: input.slug.clone(),
        locale: Some(plan_locale.clone()),
        values: merged_values,
        term_ids: input.term_ids.clone(),
    };
    merged_input
        .validate_against_effective(&effective)
        .map_err(ApiError::unprocessable)?;

    let effective_ct_def = effective_as_ct_def(&effective);
    crate::storage::media::require_existing_references(db, &effective_ct_def, &merged_input)
        .await?;
    if let Some(body) = merged_input.values.get("body").and_then(Value::as_str)
        && effective.effective_fields.get("body").is_some_and(|field| {
            field.definition.field_type == opendash_core::field_schema::FieldType::Richtext
        })
        && opendash_core::builder_blocks::contains_blocks(body)
    {
        let mut conn = db.acquire().await?;
        validated_entry_block_refs_in_conn(&mut conn, body).await?;
        let catalog = crate::storage::page::load_unified_block_catalog_in_conn(&mut conn).await?;
        let caller_role = actor_field_role(actor.user.role);
        let allowed_context_fields: Vec<String> = effective
            .effective_fields
            .iter()
            .filter(|(_, fe)| {
                fe.effective_status == EffectiveFieldStatus::Active
                    && fe.definition.effective_acl().0 <= caller_role
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
        effective_ct_def,
        merged_input,
        publication_status: input.publication_status.clone(),
    })
}
