use super::*;

pub async fn prepare_entry_translation(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    source_id: i64,
    expected_source_version: i64,
    input: &CreateEntryTranslationInput,
) -> ApiResult<EntryTranslationPlan> {
    if matches!(type_key, "pattern" | "section") {
        return Err(ApiError::forbidden(
            "Không hỗ trợ tạo bản dịch trực tiếp cho Pattern/Section.",
        ));
    }
    crate::storage::entry_lifecycle::ensure_active_entry_version(
        db,
        type_key,
        source_id,
        expected_source_version,
    )
    .await?;
    let source = get_entry(db, type_key, source_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy bài gốc để dịch."))?;
    if source.version != expected_source_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung gốc đã thay đổi. Hãy tải lại trước khi tạo bản dịch.",
        ));
    }
    let mut conn = db.acquire().await?;
    crate::storage::settings::require_multilingual_enabled_in_conn(&mut conn).await?;
    let target_locale = input.target_locale.trim().to_ascii_lowercase();
    if target_locale.is_empty() || !opendash_core::id::valid_lowercase_locale_code(&target_locale) {
        return Err(ApiError::unprocessable("Mã ngôn ngữ đích không hợp lệ."));
    }
    if target_locale == source.locale {
        return Err(ApiError::unprocessable(
            "Ngôn ngữ đích phải khác ngôn ngữ của bài gốc.",
        ));
    }
    let source_active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM site_locales WHERE code = ? AND deleted_at IS NULL)",
    )
    .bind(&source.locale)
    .fetch_one(&mut *conn)
    .await?;
    if !source_active {
        return Err(ApiError::not_found("Không tìm thấy ngôn ngữ."));
    }
    let locale_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM site_locales WHERE code = ? AND deleted_at IS NULL)",
    )
    .bind(&target_locale)
    .fetch_one(&mut *conn)
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
    .bind(&source.translation_group_id)
    .bind(&target_locale)
    .fetch_one(&mut *conn)
    .await?;
    if dup_group != 0 {
        return Err(ApiError::conflict(
            "Nhóm dịch đã có bản dịch ở ngôn ngữ đích.",
        ));
    }
    if !opendash_core::id::valid_kebab_segment_with_limit(&input.slug, 64) {
        return Err(ApiError::bad_request(
            "Slug bản dịch phải là kebab-case, tối đa 64 ký tự.",
        ));
    }
    if !input.auto_unique_slug {
        let slug_taken: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1 FROM content_entries
                WHERE content_type_key = ? AND locale = ? AND slug = ?
             )",
        )
        .bind(type_key)
        .bind(&target_locale)
        .bind(&input.slug)
        .fetch_one(&mut *conn)
        .await?;
        if slug_taken != 0 {
            return Err(ApiError::conflict("Slug đã tồn tại ở ngôn ngữ đích."));
        }
    }

    let effective =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *conn, type_key)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let caller_role = actor_field_role(actor.user.role);
    let mut copied_values = BTreeMap::new();
    for (k, v) in &source.values {
        if let Some(fe) = effective.effective_fields.get(k)
            && (caller_role < fe.definition.min_view_role
                || caller_role < fe.definition.min_edit_role)
        {
            continue;
        }
        copied_values.insert(k.clone(), v.clone());
    }
    for (k, fe) in &effective.effective_fields {
        if fe.effective_status != EffectiveFieldStatus::Withdrawn
            && fe.definition.required
            && (caller_role < fe.definition.min_view_role
                || caller_role < fe.definition.min_edit_role)
        {
            if let Some(def) = &fe.definition.default_value {
                copied_values
                    .entry(k.clone())
                    .or_insert_with(|| def.clone());
            } else {
                return Err(ApiError::forbidden(format!(
                    "field_copy_forbidden: Vai trò hiện tại không có quyền sao chép trường bắt buộc '{k}'."
                )));
            }
        }
    }

    if let Some(raw_body) = copied_values
        .get("body")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        && opendash_core::builder_blocks::contains_blocks(&raw_body)
    {
        let mut parsed = opendash_core::builder_blocks::parse_blocks_lenient_or_recover(&raw_body);
        let list_ids = collect_builder_list_term_ids(&parsed.document);
        let list_remap = remap_list_term_ids_in_conn(&mut *conn, &list_ids, &target_locale).await?;
        if apply_builder_list_term_remap(&mut parsed.document, &list_remap) {
            let serialized = opendash_core::builder_blocks::serialize_builder_document_to_blocks(
                &parsed.document,
            )
            .map_err(|_| {
                ApiError::internal("Không thể đóng gói lại Builder block sau ánh xạ term.")
            })?;
            copied_values.insert("body".to_owned(), Value::String(serialized));
        }
    }

    let term_map =
        remap_term_ids_to_locale_in_conn(&mut *conn, type_key, &source.term_ids, &target_locale)
            .await?;
    let mut remapped_term_ids = Vec::new();
    let mut seen_terms = std::collections::HashSet::new();
    for old_id in &source.term_ids {
        if let Some(&new_id) = term_map.get(old_id)
            && seen_terms.insert(new_id)
        {
            remapped_term_ids.push(new_id);
        }
    }
    let remapped_primary = source
        .primary_category_term_id
        .and_then(|old_id| term_map.get(&old_id).copied())
        .filter(|id| remapped_term_ids.contains(id));

    let title = match input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(t) => t.to_owned(),
        None => source.title.clone(),
    };
    let check_input = ContentEntryInput {
        title: title.clone(),
        slug: input.slug.clone(),
        locale: Some(target_locale.clone()),
        values: copied_values.clone(),
        term_ids: Some(remapped_term_ids.clone()),
    };
    check_input
        .validate_against_effective(&effective)
        .map_err(ApiError::unprocessable)?;

    Ok(EntryTranslationPlan {
        target_locale,
        translation_group_id: source.translation_group_id,
        slug: input.slug.clone(),
        title,
        values: copied_values,
        term_ids: remapped_term_ids,
        primary_category_term_id: remapped_primary,
        page_layout_key: source.page_layout_key,
        custom_template_key: source.custom_template_key,
        content_mode: source.content_mode,
    })
}
