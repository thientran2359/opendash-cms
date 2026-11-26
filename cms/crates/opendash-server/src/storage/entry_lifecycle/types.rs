//! Shared storage-owned entry lifecycle model, predicates, and transactional transitions.
//!
//! All ordinary entry lifecycle transitions (Trash, Restore, Purge) execute inside a
//! single caller-reserved SQLite writer transaction with authoritative actor, role,
//! provider, locale, Reading Page, schema, and CAS validation.

use super::*;

pub(super) use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) use opendash_core::{
    content::{
        ContentPublication, EffectiveFieldStatus, valid_content_type_key, valid_field_value,
    },
    id::valid_kebab_segment_with_limit,
};

pub(super) use serde::Serialize;

pub(super) use serde_json::Value;

pub(super) use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

pub(super) use crate::{
    auth::{Role, SessionContext, current_time_secs},
    error::{ApiError, ApiResult},
    storage::entries::PluginActionFence,
};

/// Shared storage-owned SQL lifecycle fragments for explicit alias application.
pub(crate) const ACTIVE_ENTRY_PREDICATE: &str = "e.trashed_at IS NULL";

pub(crate) const TRASHED_ENTRY_PREDICATE: &str = "e.trashed_at IS NOT NULL";

/// Shared predicate limiting Trash/maintenance queries to currently manageable (non-deleted, non-parked) locales.
pub(crate) const MANAGEABLE_LOCALE_EXISTS_SQL: &str = "EXISTS (SELECT 1 FROM site_locales sl \
    WHERE sl.code = e.locale \
      AND sl.deleted_at IS NULL \
      AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1))";

/// Server-only lifecycle operation enum (never deserialized from untrusted guest payloads).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleOperation {
    Trash,
    Restore,
    Purge,
}

/// Server-only lifecycle transition command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleCommand {
    pub operation: LifecycleOperation,
    pub type_key: String,
    pub entry_id: i64,
    pub expected_version: i64,
}

/// Scope for lifecycle execution: ordinary active CPTs vs Admin-only dormant retained-Trash purge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleScope {
    Ordinary,
    RetainedAdmin,
}

/// Server-validated confirmation evidence bound to target, version, and live session token hash.
/// Deliberately does NOT implement `Deserialize`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleConfirmationEvidence {
    None,
    ExplicitPurge {
        type_key: String,
        entry_id: i64,
        expected_version: i64,
        session_token_hash: String,
    },
    PublicRestore {
        type_key: String,
        entry_id: i64,
        expected_version: i64,
        session_token_hash: String,
    },
}

/// Server-only authorization context passed into `apply_lifecycle_in_tx`.
/// Deliberately does NOT implement `Deserialize`.
#[derive(Debug, Clone)]
pub struct LifecycleAuthority<'a> {
    pub session: &'a SessionContext,
    pub plugin_fence: Option<&'a PluginActionFence>,
    pub confirmation: LifecycleConfirmationEvidence,
    pub scope: LifecycleScope,
}

/// Authorized HTTP response summary for Trash and Restore transitions.
/// Deliberately excludes entry body, private values, and public URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LifecycleSummary {
    pub id: i64,
    pub type_key: String,
    pub locale: String,
    pub version: i64,
    pub publication_status: String,
    pub trashed_at: Option<String>,
    pub published_at: Option<String>,
    pub secondary: SecondaryOutcome,
}

impl LifecycleSummary {
    pub fn from_receipt(receipt: &EntryCommitReceipt, secondary: SecondaryOutcome) -> Self {
        Self {
            id: receipt.id,
            type_key: receipt.type_key.clone(),
            locale: receipt.locale.clone(),
            version: receipt
                .version_after
                .or(receipt.version_before)
                .unwrap_or(1),
            publication_status: receipt
                .publication_after
                .clone()
                .or_else(|| receipt.publication_before.clone())
                .unwrap_or_else(|| "draft".into()),
            trashed_at: receipt.trashed_at.clone(),
            published_at: receipt.published_at.clone(),
            secondary,
        }
    }
}

/// Read-snapshot preflight result used by plugin confirmation creation (holds no writer lock).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleConfirmationPreview {
    pub type_key: String,
    pub entry_id: i64,
    pub locale: String,
    pub title: String,
    pub slug: String,
    pub version: i64,
    pub needs_confirmation: bool,
}

/// Shared storage-internal physical deletion helper.
///
/// Caller owns the reserved SQLite writer transaction, authority checks, and CAS validation.
/// Never begins or commits a transaction. Deletes the exact `(type_key, id, version)` row,
/// asserts one row affected, advances Reading CAS when applicable, removes derived public
/// SEO media refs and plugin projections, lets foreign key cascades run, synchronizes
/// permalink claims, and returns the pre-delete commit receipt with `version_after = None`.
pub(in crate::storage) async fn physical_delete_cleanup_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    row: &RetainedEntryIdentity,
) -> ApiResult<EntryCommitReceipt> {
    let deleted = sqlx::query(
        "DELETE FROM content_entries
          WHERE content_type_key = ? AND id = ? AND version = ?",
    )
    .bind(&row.type_key)
    .bind(row.id)
    .bind(row.version)
    .execute(&mut **tx)
    .await?;
    if deleted.rows_affected() != 1 {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi trước khi xóa vĩnh viễn.",
        ));
    }
    if row.type_key == "page" {
        crate::storage::settings::bump_reading_version_on_deleted_entry_in_tx(tx, row.id).await?;
    }
    sqlx::query(
        "DELETE FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id = ?",
    )
    .bind(row.id)
    .execute(&mut **tx)
    .await?;
    crate::storage::plugin_projections::delete_entry_projections_in_tx(tx, row.id).await?;
    if row.type_key == "page" {
        crate::storage::permalinks::sync_claims_in_tx(tx).await?;
    } else {
        let id_str = row.id.to_string();
        if row.has_been_published != 0 {
            // Preserve existing historical redirects and promote any active canonical claim
            // of a previously published entry into an orphan historical redirect (matching
            // permalinks::sync_claims_in_tx orphan semantics without an O(N) full-table rebuild).
            let old_claims: Vec<(String, String)> = sqlx::query_as(
                "SELECT c.path, c.locale
                   FROM permalink_route_claims c
                   JOIN site_locales sl
                     ON sl.code = c.locale
                    AND sl.enabled = 1
                    AND sl.deleted_at IS NULL
                    AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                  WHERE c.target_kind = 'entry' AND c.target_id = ?",
            )
            .bind(&id_str)
            .fetch_all(&mut **tx)
            .await?;
            for (claim_path, claim_locale) in old_claims {
                if claim_path == "/" {
                    continue;
                }
                sqlx::query(
                    "INSERT INTO permalink_route_redirects(source_path, target_kind, target_id, locale, origin)
                     VALUES (?, 'entry', ?, ?, 'canonical')
                     ON CONFLICT(source_path) DO UPDATE SET
                        target_kind = excluded.target_kind,
                        target_id = excluded.target_id,
                        locale = excluded.locale,
                        origin = excluded.origin,
                        created_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE permalink_route_redirects.origin = 'locale_swap'
                        OR permalink_route_redirects.locale IN
                           (SELECT code FROM site_locales WHERE deleted_at IS NOT NULL)",
                )
                .bind(&claim_path)
                .bind(&id_str)
                .bind(&claim_locale)
                .execute(&mut **tx)
                .await?;
            }
            sqlx::query(
                "DELETE FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?",
            )
            .bind(&id_str)
            .execute(&mut **tx)
            .await?;
        } else {
            sqlx::query(
                "DELETE FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?",
            )
            .bind(&id_str)
            .execute(&mut **tx)
            .await?;
            sqlx::query(
                "DELETE FROM permalink_route_redirects WHERE target_kind = 'entry' AND target_id = ?",
            )
            .bind(&id_str)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(EntryCommitReceipt {
        operation: EntryMutationKind::Purge,
        id: row.id,
        type_key: row.type_key.clone(),
        locale: row.locale.clone(),
        translation_group_id: row.translation_group_id.clone(),
        version_before: Some(row.version),
        version_after: None,
        publication_before: Some(row.publication_status.clone()),
        publication_after: None,
        trashed_at: row.trashed_at.clone(),
        published_at: row.published_at.clone(),
    })
}

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

/// Read-only snapshot preflight for plugin `content_restore` / `content_purge` confirmation creation.
/// Validates authority, plugin fence, active CPT, CAS (412 before 409), trashed state, locale, and
/// determines whether explicit host confirmation is required without holding a SQLite writer.
pub async fn preflight_plugin_lifecycle_confirmation(
    db: &SqlitePool,
    session: &SessionContext,
    command: &LifecycleCommand,
    fence: &PluginActionFence,
) -> ApiResult<LifecycleConfirmationPreview> {
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
    if command.operation == LifecycleOperation::Purge && session.user.role != Role::Admin {
        return Err(ApiError::forbidden(
            "entry_purge_forbidden: Chỉ Quản trị viên được phép xóa vĩnh viễn nội dung.",
        ));
    }

    let mut tx = db.begin().await?;
    verify_live_session_and_role_in_tx(&mut tx, session).await?;
    verify_plugin_fence_in_tx(&mut tx, session, fence).await?;

    let ct_row: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM content_types WHERE key = ?",
    )
    .bind(&command.type_key)
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

    let Some(row) =
        load_retained_entry_identity_in_tx(&mut tx, &command.type_key, command.entry_id).await?
    else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };
    if row.version != command.expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi thao tác.",
        ));
    }
    if row.trashed_at.is_none() {
        return Err(ApiError::conflict(match command.operation {
            LifecycleOperation::Purge => {
                "entry_not_trashed: Chỉ có thể xóa vĩnh viễn nội dung đã nằm trong Thùng rác."
            }
            _ => "entry_not_trashed: Nội dung hiện không nằm trong Thùng rác.",
        }));
    }

    let locale_row: Option<(i64, i64, Option<String>)> =
        sqlx::query_as("SELECT enabled, is_default, deleted_at FROM site_locales WHERE code = ?")
            .bind(&row.locale)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((locale_enabled, locale_is_default, locale_deleted_at)) = locale_row else {
        return Err(ApiError::conflict(
            "locale_deleted: Ngôn ngữ của nội dung đã bị xóa; hãy xử lý trong mục bảo trì ngôn ngữ.",
        ));
    };
    if locale_deleted_at.is_some() {
        return Err(ApiError::conflict(
            "locale_deleted: Ngôn ngữ của nội dung đã bị xóa; hãy xử lý trong mục bảo trì ngôn ngữ.",
        ));
    }
    let multilingual_enabled: i64 =
        sqlx::query_scalar("SELECT multilingual_enabled FROM site_locales_state WHERE id = 1")
            .fetch_one(&mut *tx)
            .await?;
    if locale_is_default == 0 && multilingual_enabled == 0 {
        return Err(ApiError::conflict(
            "locale_parked: Ngôn ngữ phụ đang bị tạm dừng; hãy bật lại chế độ đa ngôn ngữ trước khi thao tác.",
        ));
    }

    let needs_confirmation = match command.operation {
        LifecycleOperation::Purge => true,
        LifecycleOperation::Restore => {
            if locale_enabled == 0 {
                return Err(ApiError::conflict(
                    "locale_disabled: Không thể khôi phục khi ngôn ngữ của nội dung đang tắt.",
                ));
            }
            let Some(effective) = crate::storage::content::load_effective_content_schema_in_tx(
                &mut *tx,
                &row.type_key,
            )
            .await?
            else {
                return Err(ApiError::conflict(
                    "provider_inactive: Không thể tải schema hiện hành của loại nội dung.",
                ));
            };
            validate_restore_compatibility_in_tx(&mut tx, &row, &effective).await?;
            effective.base.descriptor.settings.publication == ContentPublication::Immediate
                && effective.base.descriptor.settings.public
        }
        LifecycleOperation::Trash => false,
    };

    tx.commit().await?;
    Ok(LifecycleConfirmationPreview {
        type_key: row.type_key,
        entry_id: row.id,
        locale: row.locale,
        title: row.title,
        slug: row.slug,
        version: row.version,
        needs_confirmation,
    })
}

/// Authorized plugin metadata-only single Trash lookup (`content_trash_get`).
/// Scoped to active ordinary CPTs and currently manageable (non-deleted) locales only.
pub async fn get_trashed_entry_metadata_for_host(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
    locale: Option<&str>,
) -> ApiResult<Option<TrashEntryMetadata>> {
    if !valid_content_type_key(type_key) || matches!(type_key, "pattern" | "section") {
        return Ok(None);
    }
    if entry_id < 1 {
        return Ok(None);
    }
    let mut tx = db.begin().await?;
    let ct_active: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM content_types WHERE key = ?",
    )
    .bind(type_key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((provider_status, declaration_state)) = ct_active else {
        return Ok(None);
    };
    if provider_status != "active" || declaration_state != "declared" {
        return Ok(None);
    }
    let resolved_locale = match locale {
        Some(raw) => Some(
            crate::storage::settings::resolve_requested_locale_in_conn(&mut tx, Some(raw)).await?,
        ),
        None => None,
    };
    type Row = (
        i64,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        Option<String>,
    );
    let sql = format!(
        "SELECT e.id, e.content_type_key, e.locale, e.title, e.slug,
                e.publication_status, e.version, e.trashed_at, e.published_at
           FROM content_entries e
          WHERE e.content_type_key = ?
            AND e.id = ?
            AND {TRASHED_ENTRY_PREDICATE}
            AND (? IS NULL OR e.locale = ?)
            AND {MANAGEABLE_LOCALE_EXISTS_SQL}"
    );
    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(type_key)
        .bind(entry_id)
        .bind(resolved_locale.as_deref())
        .bind(resolved_locale.as_deref())
        .fetch_optional(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(row.map(
        |(
            id,
            type_key,
            locale,
            title,
            slug,
            publication_status,
            version,
            trashed_at,
            published_at,
        )| {
            TrashEntryMetadata {
                id,
                type_key,
                locale,
                title,
                slug,
                publication_status,
                version,
                trashed_at,
                published_at,
            }
        },
    ))
}

/// Authorized plugin metadata-only Trash list (`content_trash_list`) in one read snapshot.
/// Deterministic ordering: `trashed_at DESC, id DESC`.
pub async fn list_trashed_entries_metadata_for_host(
    db: &SqlitePool,
    type_key: &str,
    locale: Option<&str>,
    limit: i64,
    offset: i64,
) -> ApiResult<(Vec<TrashEntryMetadata>, i64)> {
    if !valid_content_type_key(type_key) || matches!(type_key, "pattern" | "section") {
        return Ok((Vec::new(), 0));
    }
    let clamped_limit = limit.clamp(1, 50);
    let clamped_offset = offset.clamp(0, 10_000);

    let mut tx = db.begin().await?;
    let ct_active: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM content_types WHERE key = ?",
    )
    .bind(type_key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((provider_status, declaration_state)) = ct_active else {
        return Ok((Vec::new(), 0));
    };
    if provider_status != "active" || declaration_state != "declared" {
        return Ok((Vec::new(), 0));
    }
    let resolved_locale = match locale {
        Some(raw) => Some(
            crate::storage::settings::resolve_requested_locale_in_conn(&mut tx, Some(raw)).await?,
        ),
        None => None,
    };

    let count_sql = format!(
        "SELECT COUNT(*)
           FROM content_entries e
          WHERE e.content_type_key = ?
            AND {TRASHED_ENTRY_PREDICATE}
            AND (? IS NULL OR e.locale = ?)
            AND {MANAGEABLE_LOCALE_EXISTS_SQL}"
    );
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
        .bind(type_key)
        .bind(resolved_locale.as_deref())
        .bind(resolved_locale.as_deref())
        .fetch_one(&mut *tx)
        .await?;

    type Row = (
        i64,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        Option<String>,
    );
    let rows_sql = format!(
        "SELECT e.id, e.content_type_key, e.locale, e.title, e.slug,
                e.publication_status, e.version, e.trashed_at, e.published_at
           FROM content_entries e
          WHERE e.content_type_key = ?
            AND {TRASHED_ENTRY_PREDICATE}
            AND (? IS NULL OR e.locale = ?)
            AND {MANAGEABLE_LOCALE_EXISTS_SQL}
          ORDER BY e.trashed_at DESC, e.id DESC
          LIMIT ? OFFSET ?"
    );
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(rows_sql))
        .bind(type_key)
        .bind(resolved_locale.as_deref())
        .bind(resolved_locale.as_deref())
        .bind(clamped_limit)
        .bind(clamped_offset)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;

    let items = rows
        .into_iter()
        .map(
            |(
                id,
                type_key,
                locale,
                title,
                slug,
                publication_status,
                version,
                trashed_at,
                published_at,
            )| TrashEntryMetadata {
                id,
                type_key,
                locale,
                title,
                slug,
                publication_status,
                version,
                trashed_at,
                published_at,
            },
        )
        .collect();
    Ok((items, total))
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedAtPatch {
    pub local_datetime: String,
    pub timezone: String,
}

/// Sparse narrow metadata patch shared by Quick Edit and Bulk Taxonomy (`PATCH .../metadata`).
/// Omission means retain; explicit `null` and unknown fields are rejected at deserialization.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataPatch {
    #[serde(default, deserialize_with = "deserialize_non_null_option")]
    pub expected_schema_token: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_option")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_option")]
    pub slug: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_option")]
    pub publication_status: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_option")]
    pub published_at: Option<PublishedAtPatch>,
    #[serde(default, deserialize_with = "deserialize_non_null_option")]
    pub taxonomy: Option<MetadataTaxonomyPatch>,
}

/// Strictly parses ASCII `YYYY-MM-DDTHH:mm:ss` (mandatory seconds, no suffix/fraction),
/// enforces echoed site IANA timezone, rejects nonexistent/ambiguous DST wall times
/// via `jiff::tz::Disambiguation::Reject`, rejects future timestamps, and returns
/// canonical SQLite UTC `"YYYY-MM-DD HH:MM:SS"`.
pub(crate) fn parse_and_convert_published_at(
    patch: &PublishedAtPatch,
    current_site_timezone: &str,
) -> ApiResult<String> {
    if patch.timezone.trim() != current_site_timezone {
        return Err(ApiError::conflict(
            "timezone_changed: Múi giờ hệ thống đã thay đổi; hãy tải lại biểu mẫu trước khi lưu ngày xuất bản.",
        ));
    }
    let raw = patch.local_datetime.as_str();
    let b = raw.as_bytes();
    if b.len() != 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || !b[0..4].iter().all(u8::is_ascii_digit)
        || !b[5..7].iter().all(u8::is_ascii_digit)
        || !b[8..10].iter().all(u8::is_ascii_digit)
        || !b[11..13].iter().all(u8::is_ascii_digit)
        || !b[14..16].iter().all(u8::is_ascii_digit)
        || !b[17..19].iter().all(u8::is_ascii_digit)
    {
        return Err(ApiError::unprocessable(
            "invalid_published_at: Ngày xuất bản phải đúng định dạng YYYY-MM-DDTHH:mm:ss (bắt buộc có giây, không có hậu tố múi giờ).",
        ));
    }
    let year: i16 = raw[0..4].parse().map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let month: i8 = raw[5..7].parse().map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let day: i8 = raw[8..10].parse().map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let hour: i8 = raw[11..13].parse().map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let minute: i8 = raw[14..16].parse().map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let second: i8 = raw[17..19].parse().map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    if year < 1970 {
        return Err(ApiError::unprocessable(
            "invalid_published_at: Năm của ngày xuất bản phải từ 1970 trở đi.",
        ));
    }
    let date = jiff::civil::Date::new(year, month, day).map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let time = jiff::civil::Time::new(hour, minute, second, 0).map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let civil_dt = jiff::civil::DateTime::from_parts(date, time);
    let tz = jiff::tz::TimeZone::get(current_site_timezone).map_err(|_| {
        ApiError::internal("Múi giờ hệ thống không tồn tại trong cơ sở dữ liệu IANA.")
    })?;
    let utc_ts = tz
        .to_ambiguous_timestamp(civil_dt)
        .disambiguate(jiff::tz::Disambiguation::Reject)
        .map_err(|_| {
            ApiError::unprocessable(
                "invalid_published_at: Thời điểm cục bộ không tồn tại hoặc mơ hồ do chuyển giờ mùa hè (DST); hãy chọn thời điểm hợp lệ không mơ hồ.",
            )
        })?;
    if utc_ts > jiff::Timestamp::now() {
        return Err(ApiError::unprocessable(
            "published_at_in_future: Không hỗ trợ đặt ngày xuất bản trong tương lai.",
        ));
    }
    let utc_dt = utc_ts.to_zoned_datetime(jiff::tz::TimeZone::UTC).datetime();
    Ok(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        utc_dt.year(),
        utc_dt.month(),
        utc_dt.day(),
        utc_dt.hour(),
        utc_dt.minute(),
        utc_dt.second(),
    ))
}

/// Read-only preflight for `PATCH .../entries/{id}/metadata`.
/// Holds no SQLite writer across plugin `prepare_save` / `before_save` hook execution.
pub async fn prepare_metadata_patch(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
    patch: &MetadataPatch,
) -> ApiResult<MetadataPreflightPlan> {
    let started = std::time::Instant::now();
    let res =
        prepare_metadata_patch_inner(db, actor, type_key, entry_id, expected_version, patch).await;
    let storage_us = started.elapsed().as_micros();
    let role = actor.user.role.as_str();
    match &res {
        Ok(_) => {
            eprintln!(
                "entry_lifecycle_diag op=metadata_prepare type_key={type_key} entry_id={entry_id} actor_id={} actor_role={role} expected_version={expected_version} version_after=0 outcome=ok status=200 storage_us={storage_us}",
                actor.user.id,
            );
        }
        Err(err) => {
            eprintln!(
                "entry_lifecycle_diag op=metadata_prepare type_key={type_key} entry_id={entry_id} actor_id={} actor_role={role} expected_version={expected_version} version_after=0 outcome=err status={} storage_us={storage_us}",
                actor.user.id,
                err.0.as_u16(),
            );
        }
    }
    res
}

/// Commits a narrow metadata patch under a single reserved SQLite writer transaction,
/// re-verifying live session/role, active CPT, schema token (412), entry version (412)
/// before lifecycle (409), timezone (409), date <= now, taxonomy bounds, and route claims.
/// Rejects completely unchanged final patches with `422 metadata_no_change` (no version bump or hooks).
/// Emits a privacy-safe structured diagnostic line (`entry_lifecycle_diag`) measuring
/// monotonic storage elapsed time including SQLite writer wait.
pub async fn commit_metadata_patch(
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
    let started = std::time::Instant::now();
    let res =
        commit_metadata_patch_inner(db, actor, type_key, entry_id, expected_version, patch, plan)
            .await;
    let storage_us = started.elapsed().as_micros();
    let role = actor.user.role.as_str();
    match &res {
        Ok((_, _, receipt)) => {
            eprintln!(
                "entry_lifecycle_diag op=metadata_commit type_key={type_key} entry_id={entry_id} actor_id={} actor_role={role} expected_version={expected_version} version_after={} outcome=ok status=200 storage_us={storage_us}",
                actor.user.id,
                receipt.version_after.unwrap_or(0),
            );
        }
        Err(err) => {
            eprintln!(
                "entry_lifecycle_diag op=metadata_commit type_key={type_key} entry_id={entry_id} actor_id={} actor_role={role} expected_version={expected_version} version_after=0 outcome=err status={} storage_us={storage_us}",
                actor.user.id,
                err.0.as_u16(),
            );
        }
    }
    res
}
