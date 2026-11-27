use super::*;

pub(in crate::storage) async fn validate_hook_adjusted_terms_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    entry_id: i64,
    entry_locale: &str,
    allowed_key: &str,
    candidate_ids: &[i64],
) -> ApiResult<Vec<i64>> {
    let eligible = crate::storage::content::load_effective_taxonomies_in_tx(conn, type_key).await?;
    let existing_rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT t.id, t.kind, d.provider_status, d.declaration_state
           FROM entry_terms et
           JOIN taxonomy_terms t ON t.id = et.term_id
           JOIN taxonomy_definitions d ON d.key = t.kind
          WHERE et.entry_id = ?
          ORDER BY t.id ASC",
    )
    .bind(entry_id)
    .fetch_all(&mut *conn)
    .await?;

    let mut other_ids = std::collections::BTreeSet::new();
    let mut other_active_eligible_count = 0usize;
    for (tid, kind, ps, ds) in existing_rows {
        if kind != allowed_key {
            other_ids.insert(tid);
            if ps == "active" && ds == "declared" && eligible.iter().any(|k| k == &kind) {
                other_active_eligible_count += 1;
            }
        }
    }

    let mut next_kind_ids = std::collections::BTreeSet::new();
    for &tid in candidate_ids {
        if other_ids.contains(&tid) {
            continue;
        }
        let info = lookup_term_scope_in_conn(conn, tid).await?;
        let Some((kind, loc, active)) = info else {
            return Err(ApiError::unprocessable(
                "metadata_hook_scope_conflict: Plugin filter trả về term không tồn tại.",
            ));
        };
        if !active || kind != allowed_key || loc != entry_locale {
            return Err(ApiError::unprocessable(
                "metadata_hook_scope_conflict: Plugin filter cố gắng thay đổi phân loại ngoài phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
            ));
        }
        next_kind_ids.insert(tid);
    }

    let max_for_kind = if allowed_key == "category" { 32 } else { 64 };
    if next_kind_ids.len() > max_for_kind || other_active_eligible_count + next_kind_ids.len() > 96
    {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Số lượng term sau khi filter vượt quá giới hạn cho phép.",
        ));
    }
    Ok(other_ids.union(&next_kind_ids).copied().collect())
}

pub(in crate::storage) async fn prepare_metadata_patch_inner(
    db: &SqlitePool,
    _actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
    patch: &MetadataPatch,
) -> ApiResult<MetadataPreflightPlan> {
    if !valid_content_type_key(type_key) {
        return Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."));
    }
    if matches!(type_key, "pattern" | "section") {
        return Err(ApiError::forbidden(
            "entry_lifecycle_forbidden: Không hỗ trợ thao tác vòng đời chung cho Pattern hoặc Section.",
        ));
    }
    if entry_id < 1 {
        return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
    }
    let client_token = patch
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
    if !patch.has_any_field() {
        return Err(ApiError::unprocessable(
            "metadata_empty_patch: Cần chọn ít nhất một trường siêu dữ liệu để cập nhật.",
        ));
    }

    let mut tx = db.begin().await?;
    let ct_row: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM content_types WHERE key = ?",
    )
    .bind(type_key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((provider_status, declaration_state)) = ct_row else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    };
    if provider_status != "active" || declaration_state != "declared" {
        return Err(ApiError::conflict(
            "provider_inactive: Loại nội dung hiện không hoạt động hoặc đã bị thu hồi.",
        ));
    }
    let Some(effective) =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, type_key).await?
    else {
        return Err(ApiError::conflict(
            "provider_inactive: Không thể tải schema hiện hành của loại nội dung.",
        ));
    };
    if client_token != effective.schema_token {
        return Err(ApiError::precondition_failed(
            "schema_version_conflict: Cấu trúc trường của loại nội dung đã thay đổi; hãy tải lại trước khi lưu.",
        ));
    }

    // Enforce CAS (412) before lifecycle conflict (409 entry_trashed).
    let Some(row) = load_retained_entry_identity_in_tx(&mut tx, type_key, entry_id).await? else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };
    if row.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        ));
    }
    if row.trashed_at.is_some() {
        return Err(ApiError::conflict(
            "entry_trashed: Nội dung đang ở trong Thùng rác và không thể chỉnh sửa cho đến khi khôi phục.",
        ));
    }

    // Verify locale manageability (matching Single Edit contract).
    let locale_row: Option<(i64, Option<String>)> =
        sqlx::query_as("SELECT is_default, deleted_at FROM site_locales WHERE code = ?")
            .bind(&row.locale)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((locale_is_default, locale_deleted_at)) = locale_row else {
        return Err(ApiError::conflict(
            "locale_deleted: Ngôn ngữ của nội dung không còn tồn tại trong cấu hình.",
        ));
    };
    if locale_deleted_at.is_some() {
        return Err(ApiError::conflict(
            "locale_deleted: Ngôn ngữ của nội dung đã bị xóa; hãy dùng công cụ dọn ngôn ngữ mồ côi.",
        ));
    }
    if locale_is_default == 0 {
        let multilingual_enabled: i64 =
            sqlx::query_scalar("SELECT multilingual_enabled FROM site_locales_state WHERE id = 1")
                .fetch_one(&mut *tx)
                .await?;
        if multilingual_enabled == 0 {
            return Err(ApiError::conflict(
                "locale_parked: Ngôn ngữ phụ đang bị tạm dừng; hãy bật lại chế độ đa ngôn ngữ trước khi thao tác.",
            ));
        }
    }

    let candidate_title = match patch.title.as_deref() {
        Some(raw) => validate_metadata_title(raw)?,
        None => row.title.clone(),
    };
    let candidate_slug = match patch.slug.as_deref() {
        Some(raw) => validate_metadata_slug(raw)?,
        None => row.slug.clone(),
    };

    let is_draft_publish =
        effective.base.descriptor.settings.publication == ContentPublication::DraftPublish;
    let candidate_status = match patch.publication_status.as_deref() {
        Some(st) => {
            if !is_draft_publish {
                return Err(ApiError::conflict(
                    "publication_policy_unsupported: Loại nội dung lưu ngay không hỗ trợ đổi trạng thái xuất bản qua thao tác này.",
                ));
            }
            if st != "draft" && st != "published" {
                return Err(ApiError::unprocessable(
                    "publication_status chỉ hỗ trợ 'draft' hoặc 'published'.",
                ));
            }
            st.to_owned()
        }
        None => row.publication_status.clone(),
    };

    let explicit_published_at_utc = match &patch.published_at {
        Some(date_patch) => {
            if candidate_status != "published" && row.published_at.is_none() {
                return Err(ApiError::unprocessable(
                    "draft_cannot_set_published_at: Bản nháp chưa từng xuất bản không thể đặt ngày xuất bản nếu không xuất bản tường minh.",
                ));
            }
            let general = crate::storage::settings::get_general_in_conn(&mut tx).await?;
            Some(parse_and_convert_published_at(
                date_patch,
                &general.timezone,
            )?)
        }
        None => None,
    };

    let candidate_term_ids = match &patch.taxonomy {
        Some(delta) => {
            let (merged, _) =
                resolve_taxonomy_delta_in_conn(&mut tx, type_key, entry_id, &row.locale, delta)
                    .await?;
            merged
        }
        None => {
            sqlx::query_scalar(
                "SELECT term_id FROM entry_terms WHERE entry_id = ? ORDER BY term_id ASC",
            )
            .bind(entry_id)
            .fetch_all(&mut *tx)
            .await?
        }
    };

    let candidate_values: BTreeMap<String, Value> = serde_json::from_str(&row.values_json)
        .map_err(|_| ApiError::internal("Dữ liệu custom fields không hợp lệ."))?;
    tx.commit().await?;

    Ok(MetadataPreflightPlan {
        preflight_schema_token: effective.schema_token.clone(),
        effective,
        entry_locale: row.locale,
        translation_group_id: row.translation_group_id,
        candidate_title,
        candidate_slug,
        candidate_values,
        candidate_term_ids,
        candidate_status,
        explicit_published_at_utc,
        requested_title: patch.title.is_some(),
        requested_slug: patch.slug.is_some(),
        requested_taxonomy_key: patch.taxonomy.as_ref().map(|t| t.key.clone()),
        hook_modified_terms: false,
    })
}

/// Applies validated, scope-checked `filter:content/prepare_save` modifications to a metadata plan
/// and re-validates title/slug bounds before `before_save` and commit.
pub fn apply_prepare_save_to_metadata_plan(
    plan: &mut MetadataPreflightPlan,
    hook_patch: crate::hooks::ContentPrepareSavePatch,
) -> ApiResult<()> {
    if let Some(title) = hook_patch.title {
        plan.candidate_title = validate_metadata_title(&title)?;
    }
    if let Some(slug) = hook_patch.slug {
        plan.candidate_slug = validate_metadata_slug(&slug)?;
    }
    if let Some(term_ids) = hook_patch.term_ids {
        plan.candidate_term_ids = term_ids;
        plan.hook_modified_terms = true;
    }
    Ok(())
}
