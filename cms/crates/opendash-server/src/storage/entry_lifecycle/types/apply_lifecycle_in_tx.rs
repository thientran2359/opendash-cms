use super::*;

/// Applies one entry lifecycle transition (`Trash`, `Restore`, or `Purge`) inside an
/// already-reserved SQLite writer transaction (`UPDATE bootstrap ...` must already have
/// been executed by the caller before invoking this helper).
///
/// Validates actor session/role, plugin fence (if present), CPT/scope eligibility,
/// CAS version (412 before 409 state conflict), locale manageability, Reading Page
/// assignment guard, confirmation evidence, and restore schema/route compatibility.
pub(in crate::storage) async fn apply_lifecycle_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    command: &LifecycleCommand,
    authority: &LifecycleAuthority<'_>,
) -> ApiResult<EntryCommitReceipt> {
    if !valid_content_type_key(&command.type_key) {
        return Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."));
    }
    if matches!(command.type_key.as_str(), "pattern" | "section") {
        return Err(ApiError::forbidden(
            "entry_lifecycle_forbidden: Không hỗ trợ thao tác vòng đời chung cho Pattern hoặc Section.",
        ));
    }
    if command.entry_id < 1 {
        return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
    }
    if command.expected_version < 1 {
        return Err(ApiError::bad_request("expected_version không hợp lệ."));
    }

    verify_live_session_and_role_in_tx(tx, authority.session).await?;

    if authority.scope == LifecycleScope::RetainedAdmin {
        if authority.plugin_fence.is_some() {
            return Err(ApiError::forbidden(
                "retained_trash_forbidden: Plugin không có quyền thao tác vùng bảo trì Thùng rác.",
            ));
        }
        if authority.session.user.role != Role::Admin {
            return Err(ApiError::forbidden(
                "retained_trash_forbidden: Chỉ Quản trị viên được phép xóa vĩnh viễn dữ liệu bảo lưu.",
            ));
        }
        if command.operation != LifecycleOperation::Purge {
            return Err(ApiError::forbidden(
                "retained_trash_forbidden: Vùng bảo trì Thùng rác chỉ hỗ trợ xóa vĩnh viễn.",
            ));
        }
    }

    if command.operation == LifecycleOperation::Purge && authority.session.user.role != Role::Admin
    {
        return Err(ApiError::forbidden(
            "entry_purge_forbidden: Chỉ Quản trị viên được phép xóa vĩnh viễn nội dung.",
        ));
    }

    if let Some(fence) = authority.plugin_fence {
        verify_plugin_fence_in_tx(tx, authority.session, fence).await?;
    }

    // Load CPT provider/declaration metadata before target lookup.
    let ct_row: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM content_types WHERE key = ?",
    )
    .bind(&command.type_key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((provider_status, declaration_state)) = ct_row else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    };
    let type_is_active = provider_status == "active" && declaration_state == "declared";
    match authority.scope {
        LifecycleScope::Ordinary if !type_is_active => {
            return Err(ApiError::conflict(
                "provider_inactive: Loại nội dung hiện không hoạt động hoặc đã bị thu hồi.",
            ));
        }
        LifecycleScope::RetainedAdmin if type_is_active => {
            return Err(ApiError::conflict(
                "provider_active: Loại nội dung đang hoạt động; hãy sử dụng Thùng rác thông thường.",
            ));
        }
        _ => {}
    }

    // Load authoritative retained target and enforce CAS (412) before state/policy conflicts (409).
    let Some(row) =
        load_retained_entry_identity_in_tx(tx, &command.type_key, command.entry_id).await?
    else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };
    if row.version != command.expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi thao tác.",
        ));
    }

    // Check locale eligibility under the reserved writer.
    let locale_row: Option<(i64, i64, Option<String>)> =
        sqlx::query_as("SELECT enabled, is_default, deleted_at FROM site_locales WHERE code = ?")
            .bind(&row.locale)
            .fetch_optional(&mut **tx)
            .await?;
    let Some((locale_enabled, locale_is_default, locale_deleted_at)) = locale_row else {
        return Err(ApiError::conflict(
            "locale_deleted: Ngôn ngữ của nội dung không còn tồn tại trong cấu hình.",
        ));
    };
    if locale_deleted_at.is_some() {
        return Err(ApiError::conflict(
            "locale_deleted: Ngôn ngữ của nội dung đã bị xóa; hãy dùng công cụ dọn ngôn ngữ mồ côi.",
        ));
    }
    let multilingual_enabled: i64 =
        sqlx::query_scalar("SELECT multilingual_enabled FROM site_locales_state WHERE id = 1")
            .fetch_one(&mut **tx)
            .await?;
    let locale_manageable = locale_is_default != 0 || multilingual_enabled != 0;
    if !locale_manageable {
        return Err(ApiError::conflict(
            "locale_parked: Ngôn ngữ phụ đang bị tạm dừng; hãy bật lại chế độ đa ngôn ngữ trước khi thao tác.",
        ));
    }

    match command.operation {
        LifecycleOperation::Trash => {
            if row.trashed_at.is_some() {
                return Err(ApiError::conflict(
                    "entry_already_trashed: Nội dung đã nằm trong Thùng rác.",
                ));
            }
            if row.type_key == "page" {
                let assigned_locale: Option<String> = sqlx::query_scalar(
                    "SELECT locale FROM site_reading_settings
                      WHERE homepage_page_id = ? OR posts_page_id = ?
                      LIMIT 1",
                )
                .bind(row.id)
                .bind(row.id)
                .fetch_optional(&mut **tx)
                .await?;
                if assigned_locale.is_some() {
                    return Err(ApiError::conflict(
                        "reading_page_assigned: Không thể chuyển vào Thùng rác vì Page đang được gán làm Trang chủ hoặc Trang bài viết trong Cài đặt Đọc.",
                    ));
                }
            }

            let updated = sqlx::query(
                "UPDATE content_entries
                    SET trashed_at = datetime('now'),
                        version = version + 1,
                        updated_at = datetime('now')
                  WHERE content_type_key = ? AND id = ? AND version = ? AND trashed_at IS NULL",
            )
            .bind(&row.type_key)
            .bind(row.id)
            .bind(row.version)
            .execute(&mut **tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(ApiError::precondition_failed(
                    "Phiên bản nội dung đã thay đổi trước khi chuyển vào Thùng rác.",
                ));
            }

            // Revoke derived public SEO media refs and public plugin projections transactionally,
            // while retaining canonical entry_seo, Builder/pattern refs, terms, and slug/route claims.
            sqlx::query(
                "DELETE FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id = ?",
            )
            .bind(row.id)
            .execute(&mut **tx)
            .await?;
            crate::storage::plugin_projections::delete_entry_projections_in_tx(tx, row.id).await?;
            crate::storage::revisions::clear_all_entry_autosaves_in_tx(&mut **tx, row.id).await?;

            let (version_after, trashed_at, published_at): (i64, Option<String>, Option<String>) =
                sqlx::query_as(
                    "SELECT version, trashed_at, published_at
                       FROM content_entries
                      WHERE content_type_key = ? AND id = ?",
                )
                .bind(&row.type_key)
                .bind(row.id)
                .fetch_one(&mut **tx)
                .await?;

            Ok(EntryCommitReceipt {
                operation: EntryMutationKind::Trash,
                id: row.id,
                type_key: row.type_key,
                locale: row.locale,
                translation_group_id: row.translation_group_id,
                version_before: Some(row.version),
                version_after: Some(version_after),
                publication_before: Some(row.publication_status.clone()),
                publication_after: Some(row.publication_status),
                trashed_at,
                published_at,
            })
        }
        LifecycleOperation::Restore => {
            if row.trashed_at.is_none() {
                return Err(ApiError::conflict(
                    "entry_not_trashed: Nội dung hiện không nằm trong Thùng rác.",
                ));
            }
            if locale_enabled == 0 {
                return Err(ApiError::conflict(
                    "locale_disabled: Không thể khôi phục khi ngôn ngữ của nội dung đang tắt.",
                ));
            }
            let Some(effective) = crate::storage::content::load_effective_content_schema_in_tx(
                &mut **tx,
                &row.type_key,
            )
            .await?
            else {
                return Err(ApiError::conflict(
                    "provider_inactive: Không thể tải schema hiện hành của loại nội dung.",
                ));
            };
            let is_immediate =
                effective.base.descriptor.settings.publication == ContentPublication::Immediate;
            let is_public_immediate = is_immediate && effective.base.descriptor.settings.public;

            if is_public_immediate {
                let confirmed = matches!(
                    &authority.confirmation,
                    LifecycleConfirmationEvidence::PublicRestore {
                        type_key,
                        entry_id,
                        expected_version,
                        session_token_hash,
                    } if type_key == &command.type_key
                        && *entry_id == command.entry_id
                        && *expected_version == command.expected_version
                        && session_token_hash == &authority.session.token_hash
                );
                if !confirmed {
                    return Err(ApiError::conflict(
                        "confirmation_required: Khôi phục nội dung chế độ lưu ngay sẽ hiển thị công khai ngay lập tức; cần xác nhận rõ ràng.",
                    ));
                }
            }

            validate_restore_compatibility_in_tx(tx, &row, &effective).await?;

            let was_ever_published: i64 = sqlx::query_scalar(
                "SELECT has_been_published FROM content_entries WHERE content_type_key = ? AND id = ?",
            )
            .bind(&row.type_key)
            .bind(row.id)
            .fetch_one(&mut **tx)
            .await?;
            let has_retained_claim: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?)",
            )
            .bind(row.id.to_string())
            .fetch_one(&mut **tx)
            .await?;

            let next_status = if is_immediate { "published" } else { "draft" };
            let updated = sqlx::query(
                "UPDATE content_entries
                    SET trashed_at = NULL,
                        publication_status = ?,
                        published_at = CASE
                            WHEN ? = 'published' THEN COALESCE(published_at, datetime('now'))
                            ELSE published_at
                        END,
                        version = version + 1,
                        updated_at = datetime('now')
                  WHERE content_type_key = ? AND id = ? AND version = ? AND trashed_at IS NOT NULL",
            )
            .bind(next_status)
            .bind(next_status)
            .bind(&row.type_key)
            .bind(row.id)
            .bind(row.version)
            .execute(&mut **tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(ApiError::precondition_failed(
                    "Phiên bản nội dung đã thay đổi trước khi khôi phục.",
                ));
            }

            crate::storage::seo::sync_entry_seo_media_ref_in_tx(tx, row.id).await?;
            let needs_claim_rebuild = (was_ever_published == 0 && next_status == "published")
                || (has_retained_claim == 0
                    && (was_ever_published != 0 || next_status == "published"));
            if needs_claim_rebuild {
                crate::storage::permalinks::sync_claims_in_tx(tx).await?;
            }

            let (version_after, publication_after, trashed_at, published_at): (
                i64,
                String,
                Option<String>,
                Option<String>,
            ) = sqlx::query_as(
                "SELECT version, publication_status, trashed_at, published_at
                   FROM content_entries
                  WHERE content_type_key = ? AND id = ?",
            )
            .bind(&row.type_key)
            .bind(row.id)
            .fetch_one(&mut **tx)
            .await?;

            Ok(EntryCommitReceipt {
                operation: EntryMutationKind::Restore,
                id: row.id,
                type_key: row.type_key,
                locale: row.locale,
                translation_group_id: row.translation_group_id,
                version_before: Some(row.version),
                version_after: Some(version_after),
                publication_before: Some(row.publication_status),
                publication_after: Some(publication_after),
                trashed_at,
                published_at,
            })
        }
        LifecycleOperation::Purge => {
            if row.trashed_at.is_none() {
                return Err(ApiError::conflict(
                    "entry_not_trashed: Chỉ có thể xóa vĩnh viễn nội dung đã nằm trong Thùng rác.",
                ));
            }
            let confirmed = matches!(
                &authority.confirmation,
                LifecycleConfirmationEvidence::ExplicitPurge {
                    type_key,
                    entry_id,
                    expected_version,
                    session_token_hash,
                } if type_key == &command.type_key
                    && *entry_id == command.entry_id
                    && *expected_version == command.expected_version
                    && session_token_hash == &authority.session.token_hash
            );
            if !confirmed {
                return Err(ApiError::conflict(
                    "confirmation_required: Thao tác xóa vĩnh viễn cần xác nhận rõ ràng không thể hoàn tác.",
                ));
            }
            physical_delete_cleanup_in_tx(tx, &row).await
        }
    }
}

/// Storage wrapper that reserves the SQLite writer, executes `apply_lifecycle_in_tx`,
/// commits the transaction, and returns the trusted `EntryCommitReceipt`.
/// Emits a privacy-safe structured diagnostic line (`entry_lifecycle_diag`) measuring
/// monotonic storage elapsed time including SQLite writer wait.
pub async fn execute_lifecycle_transition(
    db: &SqlitePool,
    command: &LifecycleCommand,
    authority: &LifecycleAuthority<'_>,
) -> ApiResult<EntryCommitReceipt> {
    let started = std::time::Instant::now();
    let res: ApiResult<EntryCommitReceipt> = async {
        let mut tx = db.begin().await?;
        sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
            .execute(&mut *tx)
            .await?;
        let receipt = apply_lifecycle_in_tx(&mut tx, command, authority).await?;
        tx.commit().await?;
        crate::cache::SiteCacheEpoch::global().bump();
        Ok(receipt)
    }
    .await;
    let storage_us = started.elapsed().as_micros();
    let op = match command.operation {
        LifecycleOperation::Trash => "trash",
        LifecycleOperation::Restore => "restore",
        LifecycleOperation::Purge => "purge",
    };
    let role = authority.session.user.role.as_str();
    match &res {
        Ok(receipt) => {
            eprintln!(
                "entry_lifecycle_diag op={op} type_key={} entry_id={} actor_id={} actor_role={role} expected_version={} version_after={} outcome=ok status=200 storage_us={storage_us}",
                command.type_key,
                command.entry_id,
                authority.session.user.id,
                command.expected_version,
                receipt.version_after.unwrap_or(0),
            );
        }
        Err(err) => {
            eprintln!(
                "entry_lifecycle_diag op={op} type_key={} entry_id={} actor_id={} actor_role={role} expected_version={} version_after=0 outcome=err status={} storage_us={storage_us}",
                command.type_key,
                command.entry_id,
                authority.session.user.id,
                command.expected_version,
                err.0.as_u16(),
            );
        }
    }
    res
}
