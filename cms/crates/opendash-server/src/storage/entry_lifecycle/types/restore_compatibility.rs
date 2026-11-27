use super::*;

pub(in crate::storage) async fn validate_restore_compatibility_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    row: &RetainedEntryIdentity,
    effective: &opendash_core::content::EffectiveContentSchema,
) -> ApiResult<()> {
    if row.title.trim().is_empty()
        || row.title.chars().count() > 200
        || row.title.chars().any(char::is_control)
    {
        return Err(ApiError::conflict(
            "restore_schema_conflict: Tiêu đề nội dung không còn hợp lệ theo ràng buộc hiện tại.",
        ));
    }
    if !valid_kebab_segment_with_limit(&row.slug, 64) {
        return Err(ApiError::conflict(
            "restore_schema_conflict: Slug nội dung không còn hợp lệ theo ràng buộc hiện tại.",
        ));
    }
    let values: BTreeMap<String, Value> = serde_json::from_str(&row.values_json).map_err(|_| {
        ApiError::conflict(
            "restore_schema_conflict: Dữ liệu trường nội dung đã lưu không thể đọc hợp lệ.",
        )
    })?;

    let term_rows_for_ctx: Vec<i64> = sqlx::query_scalar(
        "SELECT term_id FROM entry_terms WHERE entry_id = ? ORDER BY term_id ASC",
    )
    .bind(row.id)
    .fetch_all(&mut **tx)
    .await?;
    let (layout_for_ctx, custom_for_ctx): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT page_layout_key, custom_template_key FROM content_entries WHERE id = ?",
    )
    .bind(row.id)
    .fetch_one(&mut **tx)
    .await?;
    let loc_ctx = crate::storage::entries::build_entry_location_ctx_in_conn(
        &mut **tx,
        &row.type_key,
        Some(row.id),
        &row.locale,
        opendash_core::content::FieldRole::Admin,
        layout_for_ctx,
        custom_for_ctx,
        &term_rows_for_ctx,
    )
    .await?;

    // Validate all currently active effective fields without discarding unknown or withdrawn keys.
    for (key, field_entry) in &effective.effective_fields {
        if field_entry.effective_status != EffectiveFieldStatus::Active {
            continue;
        }
        let val = values.get(key).unwrap_or(&Value::Null);
        if val.is_null() {
            if field_entry.effective_required()
                && effective.is_field_active_in_context(field_entry, &loc_ctx)
            {
                return Err(ApiError::conflict(format!(
                    "restore_schema_conflict: Thiếu trường bắt buộc '{key}' theo schema hiện hành."
                )));
            }
            continue;
        }
        if !valid_field_value(&field_entry.definition, val) {
            return Err(ApiError::conflict(format!(
                "restore_schema_conflict: Giá trị trường '{key}' không còn tương thích với schema hiện hành."
            )));
        }
    }

    // Validate active media references for active effective fields.
    let mut active_only_values = BTreeMap::new();
    for (key, field_entry) in &effective.effective_fields {
        if field_entry.effective_status == EffectiveFieldStatus::Active
            && let Some(val) = values.get(key)
        {
            active_only_values.insert(key.clone(), val.clone());
        }
    }
    crate::storage::media::require_existing_effective_references_in_tx(
        &mut **tx,
        effective,
        &active_only_values,
    )
    .await
    .map_err(|ApiError(_, msg)| {
        ApiError::conflict(format!(
            "restore_schema_conflict: Tham chiếu media không còn hợp lệ để khôi phục ({msg})."
        ))
    })?;

    // Validate retained terms: compatible withdrawn/inactive taxonomy memberships stay intact,
    // while active eligible memberships must satisfy current locale and cardinality invariants.
    let term_rows: Vec<(i64, String, String, String, String)> = sqlx::query_as(
        "SELECT t.id, t.kind, t.locale, d.provider_status, d.declaration_state
           FROM entry_terms et
           JOIN taxonomy_terms t ON t.id = et.term_id
           JOIN taxonomy_definitions d ON d.key = t.kind
          WHERE et.entry_id = ?
          ORDER BY t.id ASC",
    )
    .bind(row.id)
    .fetch_all(&mut **tx)
    .await?;
    let eligible_taxonomies =
        crate::storage::content::load_effective_taxonomies_in_tx(&mut **tx, &row.type_key).await?;
    let mut active_total = 0usize;
    let mut active_per_kind = HashMap::<String, usize>::new();
    let mut active_category_ids = HashSet::<i64>::new();
    for (tid, kind, term_locale, provider_status, declaration_state) in term_rows {
        if term_locale != row.locale {
            return Err(ApiError::conflict(
                "restore_taxonomy_conflict: Term đính kèm không khớp ngôn ngữ của nội dung.",
            ));
        }
        if provider_status == "active"
            && declaration_state == "declared"
            && eligible_taxonomies.contains(&kind)
        {
            active_total += 1;
            let count = active_per_kind.entry(kind.clone()).or_insert(0);
            *count += 1;
            if *count > if kind == "category" { 32 } else { 64 } {
                return Err(ApiError::conflict(
                    "restore_taxonomy_conflict: Số lượng term vượt giới hạn của taxonomy hiện hành.",
                ));
            }
            if kind == "category" {
                active_category_ids.insert(tid);
            }
        }
    }
    if active_total > 96 {
        return Err(ApiError::conflict(
            "restore_taxonomy_conflict: Tổng số term hoạt động vượt quá giới hạn cho phép.",
        ));
    }
    if let Some(primary_id) = row.primary_category_term_id
        && row.type_key == "post"
        && eligible_taxonomies.iter().any(|k| k == "category")
        && !active_category_ids.contains(&primary_id)
    {
        return Err(ApiError::conflict(
            "restore_taxonomy_conflict: Chuyên mục chính đã lưu không còn thuộc tập chuyên mục hợp lệ.",
        ));
    }
    crate::storage::entries::enforce_taxonomy_selection_mode_in_conn(
        &mut **tx,
        &row.type_key,
        &active_per_kind,
    )
    .await
    .map_err(|ApiError(_, msg)| ApiError::conflict(format!("restore_taxonomy_conflict: {msg}")))?;

    Ok(())
}
