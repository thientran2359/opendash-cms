use super::*;

pub(in crate::storage) async fn verify_live_session_and_role_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &SessionContext,
) -> ApiResult<()> {
    let current: Option<(i64, String)> = sqlx::query_as(
        "SELECT u.id, u.role
           FROM sessions s
           JOIN users u ON u.id = s.user_id
          WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(&actor.token_hash)
    .bind(current_time_secs())
    .fetch_optional(&mut **tx)
    .await?;
    match current {
        Some((id, role)) if id == actor.user.id && role == actor.user.role.as_str() => Ok(()),
        Some(_) => Err(ApiError::forbidden("Quyền của tài khoản đã bị thu hồi.")),
        None => Err(ApiError::unauthorized("Phiên đăng nhập đã bị thu hồi.")),
    }
}

pub(in crate::storage) async fn verify_plugin_fence_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &SessionContext,
    fence: &PluginActionFence,
) -> ApiResult<()> {
    let active = match &fence.owner_fence {
        crate::storage::entries::OwnerFence::Plugin {
            plugin_id,
            expected_generation,
        } => {
            crate::storage::plugin::is_plugin_generation_active(
                &mut **tx,
                plugin_id,
                *expected_generation,
            )
            .await?
        }
        crate::storage::entries::OwnerFence::Theme {
            revision_id,
            selection_version,
            ..
        } => {
            crate::storage::plugin::verify_active_theme_fence_in_tx(
                &mut **tx,
                revision_id,
                *selection_version,
            )
            .await?
        }
    };
    if !active {
        return Err(ApiError::conflict(
            "Plugin đã bị vô hiệu hóa hoặc thay đổi phiên bản trong lúc thực thi.",
        ));
    }
    let role_ok = crate::storage::plugin::is_actor_role_active(
        &mut **tx,
        actor.user.id,
        &fence.required_role,
    )
    .await?;
    if !role_ok {
        return Err(ApiError::forbidden("Quyền của tài khoản đã bị thu hồi."));
    }
    Ok(())
}

pub(in crate::storage) async fn commit_metadata_patch_inner(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
    patch: &MetadataPatch,
    plan: &MetadataPreflightPlan,
) -> ApiResult<(
    crate::storage::entry_table::EntryTableRow,
    String,
    EntryCommitReceipt,
)> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;

    verify_live_session_and_role_in_tx(&mut tx, actor).await?;

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
    let client_token = patch.expected_schema_token.as_deref().unwrap_or("").trim();
    if client_token != effective.schema_token
        || plan.preflight_schema_token != effective.schema_token
    {
        return Err(ApiError::precondition_failed(
            "schema_version_conflict: Cấu trúc trường của loại nội dung đã thay đổi; hãy tải lại trước khi lưu.",
        ));
    }

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

    let final_title = validate_metadata_title(&plan.candidate_title)?;
    let final_slug = validate_metadata_slug(&plan.candidate_slug)?;

    let is_draft_publish =
        effective.base.descriptor.settings.publication == ContentPublication::DraftPublish;
    let final_status = match patch.publication_status.as_deref() {
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
            if final_status != "published" && row.published_at.is_none() {
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

    let current_terms_with_kind: Vec<(i64, String)> = sqlx::query_as(
        "SELECT t.id, t.kind
           FROM entry_terms et
           JOIN taxonomy_terms t ON t.id = et.term_id
          WHERE et.entry_id = ?
          ORDER BY t.id ASC",
    )
    .bind(entry_id)
    .fetch_all(&mut *tx)
    .await?;
    let current_all_terms: std::collections::BTreeSet<i64> =
        current_terms_with_kind.iter().map(|(id, _)| *id).collect();
    let current_category_terms: std::collections::BTreeSet<i64> = current_terms_with_kind
        .iter()
        .filter(|(_, k)| k == "category")
        .map(|(id, _)| *id)
        .collect();

    let final_term_ids: Vec<i64> = match &patch.taxonomy {
        Some(delta) if plan.hook_modified_terms => {
            validate_hook_adjusted_terms_in_conn(
                &mut tx,
                type_key,
                entry_id,
                &row.locale,
                &delta.key,
                &plan.candidate_term_ids,
            )
            .await?
        }
        Some(delta) => {
            let (merged, _) =
                resolve_taxonomy_delta_in_conn(&mut tx, type_key, entry_id, &row.locale, delta)
                    .await?;
            merged
        }
        None => current_all_terms.iter().copied().collect(),
    };
    let final_all_terms: std::collections::BTreeSet<i64> = final_term_ids.iter().copied().collect();

    // Preserve primary category unless it was removed from the entry's term set; never invent one.
    let final_primary_category = row
        .primary_category_term_id
        .filter(|pid| final_all_terms.contains(pid));

    let title_changed = final_title != row.title;
    let slug_changed = final_slug != row.slug;
    let status_changed = final_status != row.publication_status;
    let published_at_changed =
        explicit_published_at_utc.is_some() && explicit_published_at_utc != row.published_at;
    let terms_changed = final_all_terms != current_all_terms;
    let primary_changed = final_primary_category != row.primary_category_term_id;

    if !title_changed
        && !slug_changed
        && !status_changed
        && !published_at_changed
        && !terms_changed
        && !primary_changed
    {
        return Err(ApiError::unprocessable(
            "metadata_no_change: Không có thay đổi nào so với dữ liệu hiện tại.",
        ));
    }

    crate::storage::revisions::capture_baseline_if_missing_in_tx(&mut *tx, entry_id).await?;

    let was_ever_published_int: i64 = sqlx::query_scalar(
        "SELECT has_been_published FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(entry_id)
    .fetch_one(&mut *tx)
    .await?;
    let was_ever_published = was_ever_published_int != 0;

    let res = sqlx::query(
        "UPDATE content_entries
            SET title = ?,
                slug = ?,
                publication_status = ?,
                published_at = CASE
                    WHEN ? IS NOT NULL THEN ?
                    WHEN ? = 'published' THEN COALESCE(published_at, datetime('now'))
                    ELSE published_at
                END,
                primary_category_term_id = ?,
                version = version + 1,
                updated_at = datetime('now')
          WHERE content_type_key = ? AND id = ? AND version = ? AND trashed_at IS NULL",
    )
    .bind(&final_title)
    .bind(&final_slug)
    .bind(&final_status)
    .bind(explicit_published_at_utc.as_deref())
    .bind(explicit_published_at_utc.as_deref())
    .bind(&final_status)
    .bind(final_primary_category)
    .bind(type_key)
    .bind(entry_id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await;

    match res {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."));
        }
        other => {
            if other?.rows_affected() != 1 {
                return Err(ApiError::precondition_failed(
                    "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
                ));
            }
        }
    }

    if terms_changed {
        for &tid in current_all_terms.difference(&final_all_terms) {
            sqlx::query("DELETE FROM entry_terms WHERE entry_id = ? AND term_id = ?")
                .bind(entry_id)
                .bind(tid)
                .execute(&mut *tx)
                .await?;
        }
        for &tid in final_all_terms.difference(&current_all_terms) {
            sqlx::query("INSERT INTO entry_terms (entry_id, term_id) VALUES (?, ?)")
                .bind(entry_id)
                .bind(tid)
                .execute(&mut *tx)
                .await?;
        }
    }

    let mut auto_assigned_terms = false;
    if terms_changed || final_status == "published" {
        let (layout, custom): (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT page_layout_key, custom_template_key FROM content_entries WHERE id = ?",
        )
        .bind(entry_id)
        .fetch_one(&mut *tx)
        .await?;
        let loc_ctx = crate::storage::entries::build_entry_location_ctx_in_conn(
            &mut *tx,
            type_key,
            Some(entry_id),
            &row.locale,
            opendash_core::content::FieldRole::Admin,
            layout,
            custom,
            &final_term_ids,
        )
        .await?;
        if terms_changed {
            crate::storage::entries::sync_entry_values_json_with_terms_in_tx(
                &mut tx,
                entry_id,
                &effective,
                &loc_ctx,
                &final_term_ids,
            )
            .await?;
        }
        if final_status == "published" {
            auto_assigned_terms = crate::storage::entries::enforce_taxonomy_publish_rules_in_tx(
                &mut tx,
                type_key,
                entry_id,
                &row.locale,
                Some(&effective),
                Some(&loc_ctx),
            )
            .await?;
        }
    }

    if status_changed {
        crate::storage::seo::sync_entry_seo_media_ref_in_tx(&mut tx, entry_id).await?;
        if final_status == "draft" {
            crate::storage::plugin_projections::delete_entry_projections_in_tx(&mut tx, entry_id)
                .await?;
        }
    }

    let category_terms_changed = if (terms_changed || auto_assigned_terms) && type_key == "post" {
        let final_cats: Vec<i64> = sqlx::query_scalar(
            "SELECT t.id
               FROM entry_terms et
               JOIN taxonomy_terms t ON t.id = et.term_id
              WHERE et.entry_id = ? AND t.kind = 'category'
              ORDER BY t.id ASC",
        )
        .bind(entry_id)
        .fetch_all(&mut *tx)
        .await?;
        let final_cat_set: std::collections::BTreeSet<i64> = final_cats.into_iter().collect();
        final_cat_set != current_category_terms
    } else {
        false
    };

    let needs_claim_sync = slug_changed
        || primary_changed
        || auto_assigned_terms
        || (type_key == "post" && category_terms_changed)
        || (status_changed
            && (was_ever_published || final_status == "published")
            && (type_key == "page" || !was_ever_published));
    if needs_claim_sync {
        if !was_ever_published {
            sqlx::query(
                "DELETE FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?",
            )
            .bind(entry_id.to_string())
            .execute(&mut *tx)
            .await?;
        }
        crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    }

    crate::storage::revisions::capture_entry_revision_in_tx(
        &mut *tx,
        entry_id,
        opendash_core::revisions::RevisionSource::QuickEdit,
        None,
        crate::storage::revisions::RevisionActor::User(actor),
    )
    .await?;

    let caller_role = match actor.user.role {
        Role::Admin => opendash_core::content::FieldRole::Admin,
        Role::Editor => opendash_core::content::FieldRole::Editor,
    };
    let committed_row = crate::storage::entry_table::load_single_active_row_in_tx(
        &mut tx,
        type_key,
        entry_id,
        caller_role,
        &effective,
    )
    .await?;

    let receipt = EntryCommitReceipt {
        operation: EntryMutationKind::Metadata,
        id: committed_row.id,
        type_key: committed_row.type_key.clone(),
        locale: committed_row.locale.clone(),
        translation_group_id: committed_row.translation_group_id.clone(),
        version_before: Some(row.version),
        version_after: Some(committed_row.version),
        publication_before: Some(row.publication_status),
        publication_after: Some(committed_row.publication_status.clone()),
        trashed_at: None,
        published_at: committed_row.published_at.clone(),
        detached_parent_id: None,
    };

    crate::storage::search::sync_entry_search_index_in_conn(&mut *tx, entry_id).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok((committed_row, effective.schema_token, receipt))
}
