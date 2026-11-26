//! Shared storage-owned entry lifecycle model, predicates, and transactional transitions.
//!
//! All ordinary entry lifecycle transitions (Trash, Restore, Purge) execute inside a
//! single caller-reserved SQLite writer transaction with authoritative actor, role,
//! provider, locale, Reading Page, schema, and CAS validation.

use std::collections::{BTreeMap, HashMap, HashSet};

use opendash_core::{
    content::{
        ContentPublication, EffectiveFieldStatus, valid_content_type_key, valid_field_value,
    },
    id::valid_kebab_segment_with_limit,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

use crate::{
    auth::{Role, SessionContext, current_time_secs},
    error::{ApiError, ApiResult},
    storage::entries::PluginActionFence,
};

/// Shared storage-owned SQL lifecycle fragments for explicit alias application.
pub(crate) const ACTIVE_ENTRY_PREDICATE: &str = "e.trashed_at IS NULL";
#[allow(dead_code)]
pub(crate) const TRASHED_ENTRY_PREDICATE: &str = "e.trashed_at IS NOT NULL";
pub(crate) const ACTIVE_ENTRY_UNALIASED: &str = "trashed_at IS NULL";

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

/// Server-only committed entry mutation kind for trusted post-commit hooks and receipts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum EntryMutationKind {
    Save,
    Metadata,
    Publish,
    Unpublish,
    Trash,
    Restore,
    Purge,
}

/// Server-only commit receipt produced strictly by storage inside the committed writer transaction.
/// Deliberately does NOT implement `Deserialize`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryCommitReceipt {
    pub operation: EntryMutationKind,
    pub id: i64,
    pub type_key: String,
    pub locale: String,
    pub translation_group_id: String,
    pub version_before: Option<i64>,
    pub version_after: Option<i64>,
    pub publication_before: Option<String>,
    pub publication_after: Option<String>,
    pub trashed_at: Option<String>,
    pub published_at: Option<String>,
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

/// Secondary post-commit projection work outcome, kept separate from durable DB commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ProjectionOutcome {
    NotNeeded,
    Current,
    Failed { reason: String },
}

/// Bounded post-commit secondary work summary returned alongside committed responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[allow(dead_code)]
pub struct SecondaryOutcome {
    pub projection: ProjectionOutcome,
    pub hook_enqueued: u32,
    pub hook_dropped: u32,
}

impl SecondaryOutcome {
    #[allow(dead_code)]
    pub fn none() -> Self {
        Self {
            projection: ProjectionOutcome::NotNeeded,
            hook_enqueued: 0,
            hook_dropped: 0,
        }
    }
}

/// Private storage-scope snapshot of a retained entry row loaded under a reserved writer
/// after authority and CAS validation.
#[derive(Debug, Clone)]
pub(super) struct RetainedEntryIdentity {
    pub id: i64,
    pub type_key: String,
    pub locale: String,
    pub translation_group_id: String,
    pub slug: String,
    pub title: String,
    pub values_json: String,
    pub version: i64,
    pub publication_status: String,
    pub trashed_at: Option<String>,
    pub published_at: Option<String>,
    pub primary_category_term_id: Option<i64>,
}

/// Retained row header for preflight checks that must distinguish 404 (absent),
/// 412 (stale version), and 409 (trashed target) without exposing entry body/values.
#[derive(Debug, Clone)]
pub(crate) struct RetainedEntryPreflight {
    pub id: i64,
    pub locale: String,
    pub translation_group_id: String,
    pub version: i64,
    pub trashed_at: Option<String>,
}

pub(crate) async fn load_retained_preflight_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<Option<RetainedEntryPreflight>> {
    let row: Option<(i64, String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, locale, translation_group_id, version, trashed_at
           FROM content_entries
          WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(
        |(id, locale, translation_group_id, version, trashed_at)| RetainedEntryPreflight {
            id,
            locale,
            translation_group_id,
            version,
            trashed_at,
        },
    ))
}

pub(crate) async fn load_retained_preflight(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<Option<RetainedEntryPreflight>> {
    let mut conn = db.acquire().await?;
    load_retained_preflight_in_conn(&mut conn, type_key, entry_id).await
}

/// Validates that an authorized ordinary mutation targets an existing, active row
/// matching `expected_version`. Enforces CAS (412) before lifecycle state conflict (409).
pub(crate) async fn ensure_active_entry_version_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
) -> ApiResult<RetainedEntryPreflight> {
    let Some(row) = load_retained_preflight_in_conn(conn, type_key, entry_id).await? else {
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
    Ok(row)
}

pub(crate) async fn ensure_active_entry_version(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
) -> ApiResult<RetainedEntryPreflight> {
    let mut conn = db.acquire().await?;
    ensure_active_entry_version_in_conn(&mut conn, type_key, entry_id, expected_version).await
}

pub(super) async fn load_retained_entry_identity_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<Option<RetainedEntryIdentity>> {
    type RowTuple = (
        i64,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<i64>,
    );
    let row: Option<RowTuple> = sqlx::query_as(
        "SELECT id, content_type_key, locale, translation_group_id, slug, title, values_json,
                version, publication_status, trashed_at, published_at, primary_category_term_id
           FROM content_entries
          WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(entry_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(
        |(
            id,
            type_key,
            locale,
            translation_group_id,
            slug,
            title,
            values_json,
            version,
            publication_status,
            trashed_at,
            published_at,
            primary_category_term_id,
        )| RetainedEntryIdentity {
            id,
            type_key,
            locale,
            translation_group_id,
            slug,
            title,
            values_json,
            version,
            publication_status,
            trashed_at,
            published_at,
            primary_category_term_id,
        },
    ))
}

/// Shared storage-internal physical deletion helper.
///
/// Caller owns the reserved SQLite writer transaction, authority checks, and CAS validation.
/// Never begins or commits a transaction. Deletes the exact `(type_key, id, version)` row,
/// asserts one row affected, advances Reading CAS when applicable, removes derived public
/// SEO media refs and plugin projections, lets foreign key cascades run, synchronizes
/// permalink claims, and returns the pre-delete commit receipt with `version_after = None`.
pub(super) async fn physical_delete_cleanup_in_tx(
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
    super::plugin_projections::delete_entry_projections_in_tx(tx, row.id).await?;
    crate::storage::permalinks::sync_claims_in_tx(tx).await?;
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

async fn verify_live_session_and_role_in_tx(
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

async fn verify_plugin_fence_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &SessionContext,
    fence: &PluginActionFence,
) -> ApiResult<()> {
    let active = super::plugin::is_plugin_generation_active(
        &mut **tx,
        &fence.plugin_id,
        fence.expected_generation,
    )
    .await?;
    if !active {
        return Err(ApiError::conflict(
            "Plugin đã bị vô hiệu hóa hoặc thay đổi phiên bản trong lúc thực thi.",
        ));
    }
    let role_ok =
        super::plugin::is_actor_role_active(&mut **tx, actor.user.id, &fence.required_role).await?;
    if !role_ok {
        return Err(ApiError::forbidden("Quyền của tài khoản đã bị thu hồi."));
    }
    Ok(())
}

async fn validate_restore_compatibility_in_tx(
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

    // Validate all currently active effective fields without discarding unknown or withdrawn keys.
    for (key, field_entry) in &effective.effective_fields {
        if field_entry.effective_status != EffectiveFieldStatus::Active {
            continue;
        }
        let val = values.get(key).unwrap_or(&Value::Null);
        if val.is_null() {
            if field_entry.effective_required() {
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
    super::media::require_existing_effective_references_in_tx(
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

    Ok(())
}

/// Applies one entry lifecycle transition (`Trash`, `Restore`, or `Purge`) inside an
/// already-reserved SQLite writer transaction (`UPDATE bootstrap ...` must already have
/// been executed by the caller before invoking this helper).
///
/// Validates actor session/role, plugin fence (if present), CPT/scope eligibility,
/// CAS version (412 before 409 state conflict), locale manageability, Reading Page
/// assignment guard, confirmation evidence, and restore schema/route compatibility.
pub(super) async fn apply_lifecycle_in_tx(
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
            super::plugin_projections::delete_entry_projections_in_tx(tx, row.id).await?;

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
            crate::storage::permalinks::sync_claims_in_tx(tx).await?;

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
///
/// S01 exposes this at crate/integration-test storage boundary only (not yet via HTTP/guest).
pub async fn execute_lifecycle_transition(
    db: &SqlitePool,
    command: &LifecycleCommand,
    authority: &LifecycleAuthority<'_>,
) -> ApiResult<EntryCommitReceipt> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let receipt = apply_lifecycle_in_tx(&mut tx, command, authority).await?;
    tx.commit().await?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use opendash_core::content::ContentTypeDefinitionInput;

    use crate::auth::{AuthUser, Role, SessionContext};

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        pool
    }

    async fn seed_actor(pool: &SqlitePool, username: &str, role: Role) -> SessionContext {
        let user_id: i64 = sqlx::query(
            "INSERT INTO users (username, password_hash, role, version) VALUES (?, 'hash', ?, 1)",
        )
        .bind(username)
        .bind(role.as_str())
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let token_hash = format!("tok-{username}-{user_id}");
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, csrf_token, expires_at)
             VALUES (?, ?, 'csrf-test', ?)",
        )
        .bind(&token_hash)
        .bind(user_id)
        .bind(current_time_secs() + 3600)
        .execute(pool)
        .await
        .unwrap();
        SessionContext {
            token_hash,
            csrf_token: "csrf-test".into(),
            user: AuthUser {
                id: user_id,
                username: username.into(),
                role,
            },
        }
    }

    async fn seed_entry(
        pool: &SqlitePool,
        type_key: &str,
        locale: &str,
        slug: &str,
        status: &str,
        values_json: &str,
    ) -> i64 {
        let id = sqlx::query(
            "INSERT INTO content_entries
             (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, version)
             VALUES (?, ?, 'eg-' || lower(hex(randomblob(6))), ?, 'Entry Title', ?, 'draft', 1)",
        )
        .bind(type_key)
        .bind(locale)
        .bind(slug)
        .bind(values_json)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();
        if status == "published" {
            sqlx::query(
                "UPDATE content_entries
                    SET publication_status = 'published',
                        published_at = '2025-05-01 10:00:00',
                        version = version + 1
                  WHERE id = ?",
            )
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
        }
        let mut tx = pool.begin().await.unwrap();
        crate::storage::permalinks::sync_claims_in_tx(&mut tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        id
    }

    #[tokio::test]
    async fn trash_restore_purge_invariants_and_cas_precedence() {
        let pool = setup_db().await;
        let admin = seed_actor(&pool, "admin", Role::Admin).await;
        let editor = seed_actor(&pool, "editor", Role::Editor).await;

        let post_id = seed_entry(&pool, "post", "vi", "recover-me", "published", "{}").await;
        let orig_pub_at: Option<String> =
            sqlx::query_scalar("SELECT published_at FROM content_entries WHERE id = ?")
                .bind(post_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(orig_pub_at.as_deref(), Some("2025-05-01 10:00:00"));

        // Attach canonical SEO + derived public SEO ref + plugin projection to verify Trash revokes derived grants only.
        let og_id = "a0111111111111111111111111111111a0111111111111111111111111111111";
        sqlx::query(
            "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state)
             VALUES (?, 'image', 'og.png', 'image/png', 'png', 16, 2, 2, 2, 2, 'active')",
        )
        .bind(og_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO entry_seo (entry_id, title, description, og_image_id, noindex)
             VALUES (?, 'SEO Title', 'SEO Desc', ?, 0)",
        )
        .bind(post_id)
        .bind(og_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO seo_public_media_refs (source_kind, source_id, media_id)
             VALUES ('entry_seo', ?, ?)",
        )
        .bind(post_id)
        .bind(og_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO plugins (id, name, version, manifest_json, wasm_blob, enabled)
             VALUES ('p-seo', 'SEO', '1.0.0', '{}', X'00', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO plugin_entry_projections (entry_id, plugin_id, locale, ext_json)
             VALUES (?, 'p-seo', 'vi', '{\"ok\":true}')",
        )
        .bind(post_id)
        .execute(&pool)
        .await
        .unwrap();

        // Active Purge must reject with 409 (must Trash first).
        let active_purge_err = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Purge,
                type_key: "post".into(),
                entry_id: post_id,
                expected_version: 2,
            },
            &LifecycleAuthority {
                session: &admin,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::ExplicitPurge {
                    type_key: "post".into(),
                    entry_id: post_id,
                    expected_version: 2,
                    session_token_hash: admin.token_hash.clone(),
                },
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(active_purge_err.0, StatusCode::CONFLICT);

        // Editor Trash succeeds, advances version 2 -> 3, preserves publication_status & published_at,
        // revokes derived SEO public media refs & plugin projections, preserves canonical entry_seo & route claims.
        let trash_receipt = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Trash,
                type_key: "post".into(),
                entry_id: post_id,
                expected_version: 2,
            },
            &LifecycleAuthority {
                session: &editor,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::None,
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap();
        assert_eq!(trash_receipt.operation, EntryMutationKind::Trash);
        assert_eq!(trash_receipt.version_before, Some(2));
        assert_eq!(trash_receipt.version_after, Some(3));
        assert_eq!(
            trash_receipt.publication_before.as_deref(),
            Some("published")
        );
        assert_eq!(
            trash_receipt.publication_after.as_deref(),
            Some("published")
        );
        assert!(trash_receipt.trashed_at.is_some());
        assert_eq!(
            trash_receipt.published_at.as_deref(),
            Some("2025-05-01 10:00:00")
        );

        let seo_pub_refs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id = ?",
        )
        .bind(post_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(seo_pub_refs, 0);
        let proj_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM plugin_entry_projections WHERE entry_id = ?")
                .bind(post_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(proj_rows, 0);
        let canonical_seo: Option<String> =
            sqlx::query_scalar("SELECT og_image_id FROM entry_seo WHERE entry_id = ?")
                .bind(post_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(canonical_seo.as_deref(), Some(og_id));
        let retained_claim: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?",
        )
        .bind(post_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(retained_claim, 1);

        // Stale Trash yields 412 BEFORE 409 already-trashed!
        let stale_err = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Trash,
                type_key: "post".into(),
                entry_id: post_id,
                expected_version: 2,
            },
            &LifecycleAuthority {
                session: &editor,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::None,
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(stale_err.0, StatusCode::PRECONDITION_FAILED);

        // Repeated Trash with fresh version 3 yields 409 and does NOT bump version.
        let repeat_err = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Trash,
                type_key: "post".into(),
                entry_id: post_id,
                expected_version: 3,
            },
            &LifecycleAuthority {
                session: &editor,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::None,
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(repeat_err.0, StatusCode::CONFLICT);
        let ver_after_repeat: i64 =
            sqlx::query_scalar("SELECT version FROM content_entries WHERE id = ?")
                .bind(post_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(ver_after_repeat, 3);

        // Editor Purge must be 403.
        let editor_purge_err = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Purge,
                type_key: "post".into(),
                entry_id: post_id,
                expected_version: 3,
            },
            &LifecycleAuthority {
                session: &editor,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::ExplicitPurge {
                    type_key: "post".into(),
                    entry_id: post_id,
                    expected_version: 3,
                    session_token_hash: editor.token_hash.clone(),
                },
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(editor_purge_err.0, StatusCode::FORBIDDEN);

        // Restore draft_publish post returns to 'draft', increments 3 -> 4, preserves published_at!
        let restore_receipt = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Restore,
                type_key: "post".into(),
                entry_id: post_id,
                expected_version: 3,
            },
            &LifecycleAuthority {
                session: &editor,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::None,
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap();
        assert_eq!(restore_receipt.operation, EntryMutationKind::Restore);
        assert_eq!(restore_receipt.version_before, Some(3));
        assert_eq!(restore_receipt.version_after, Some(4));
        assert_eq!(restore_receipt.publication_after.as_deref(), Some("draft"));
        assert!(restore_receipt.trashed_at.is_none());
        assert_eq!(
            restore_receipt.published_at.as_deref(),
            Some("2025-05-01 10:00:00")
        );

        // Trash again (4 -> 5) and Admin Purge without confirmation -> 409, then with confirmation -> succeeds!
        execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Trash,
                type_key: "post".into(),
                entry_id: post_id,
                expected_version: 4,
            },
            &LifecycleAuthority {
                session: &admin,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::None,
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap();

        let no_confirm = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Purge,
                type_key: "post".into(),
                entry_id: post_id,
                expected_version: 5,
            },
            &LifecycleAuthority {
                session: &admin,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::None,
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(no_confirm.0, StatusCode::CONFLICT);

        let purge_receipt = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Purge,
                type_key: "post".into(),
                entry_id: post_id,
                expected_version: 5,
            },
            &LifecycleAuthority {
                session: &admin,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::ExplicitPurge {
                    type_key: "post".into(),
                    entry_id: post_id,
                    expected_version: 5,
                    session_token_hash: admin.token_hash.clone(),
                },
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap();
        assert_eq!(purge_receipt.operation, EntryMutationKind::Purge);
        assert_eq!(purge_receipt.version_before, Some(5));
        assert_eq!(purge_receipt.version_after, None);
        assert_eq!(purge_receipt.publication_after, None);
        assert!(purge_receipt.trashed_at.is_some());

        let remaining: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM content_entries WHERE id = ?")
                .bind(post_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn reading_page_guard_bidirectional_and_immediate_restore_schema_and_locale_fences() {
        let pool = setup_db().await;
        let admin = seed_actor(&pool, "admin", Role::Admin).await;

        // Add disabled secondary locale 'en' with assigned homepage page; Trash must STILL be blocked!
        sqlx::query("INSERT INTO site_locales (code, bcp47_tag, label, enabled) VALUES ('en', 'en', 'English', 1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE site_locales_state SET multilingual_enabled = 1 WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        let en_page = seed_entry(&pool, "page", "en", "home-en", "published", "{}").await;
        sqlx::query(
            "INSERT INTO site_reading_settings (locale, homepage_page_id, posts_page_id, version)
             VALUES ('en', ?, NULL, 1)
             ON CONFLICT(locale) DO UPDATE SET homepage_page_id = excluded.homepage_page_id",
        )
        .bind(en_page)
        .execute(&pool)
        .await
        .unwrap();
        // Disable 'en' (still manageable because multilingual_enabled = 1): Reading assignment MUST still block Trash.
        sqlx::query("UPDATE site_locales SET enabled = 0 WHERE code = 'en'")
            .execute(&pool)
            .await
            .unwrap();
        let reading_block = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Trash,
                type_key: "page".into(),
                entry_id: en_page,
                expected_version: 2,
            },
            &LifecycleAuthority {
                session: &admin,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::None,
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(reading_block.0, StatusCode::CONFLICT);
        assert!(reading_block.1.contains("reading_page_assigned"));

        // Unassign 'en' homepage -> Trash succeeds -> re-assigning trashed Page fails in save_reading_for_locale!
        sqlx::query("UPDATE site_reading_settings SET homepage_page_id = NULL WHERE locale = 'en'")
            .execute(&pool)
            .await
            .unwrap();
        execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Trash,
                type_key: "page".into(),
                entry_id: en_page,
                expected_version: 2,
            },
            &LifecycleAuthority {
                session: &admin,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::None,
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap();

        let assign_trashed = crate::storage::settings::save_reading_for_locale(
            &pool,
            Some("en"),
            crate::storage::settings::ReadingSettingsInput {
                homepage_page_id: Some(en_page),
                posts_page_id: None,
            },
            1,
        )
        .await
        .unwrap_err();
        assert_eq!(assign_trashed.0, StatusCode::BAD_REQUEST);

        // Restore in disabled locale 'en' must be rejected until 'en' is re-enabled!
        let disabled_restore = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Restore,
                type_key: "page".into(),
                entry_id: en_page,
                expected_version: 3,
            },
            &LifecycleAuthority {
                session: &admin,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::None,
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(disabled_restore.0, StatusCode::CONFLICT);
        assert!(disabled_restore.1.contains("locale_disabled"));

        // Immediate public CPT: restore requires PublicRestore confirmation and preserves unknown/withdrawn values!
        let imm_input: ContentTypeDefinitionInput = serde_json::from_value(serde_json::json!({
            "key": "notice",
            "label": "Notice",
            "fields": [{
                "key": "summary",
                "label": "Summary",
                "type": "short_text",
                "required": false,
                "options": [],
                "min_view_role": "editor",
                "min_edit_role": "editor",
                "public_exposure": "public"
            }],
            "settings": {
                "public": true,
                "has_archive": true,
                "publication": "immediate",
                "supports": ["editor"]
            }
        }))
        .unwrap();
        crate::storage::content::insert_user_content_type(&pool, &imm_input)
            .await
            .unwrap();
        let notice_id = seed_entry(
            &pool,
            "notice",
            "vi",
            "imm-1",
            "published",
            r#"{"summary":"ok","legacy_extra":"kept"}"#,
        )
        .await;
        execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Trash,
                type_key: "notice".into(),
                entry_id: notice_id,
                expected_version: 2,
            },
            &LifecycleAuthority {
                session: &admin,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::None,
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap();

        // Missing PublicRestore confirmation -> 409 confirmation_required
        let missing_pub_confirm = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Restore,
                type_key: "notice".into(),
                entry_id: notice_id,
                expected_version: 3,
            },
            &LifecycleAuthority {
                session: &admin,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::None,
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(missing_pub_confirm.0, StatusCode::CONFLICT);
        assert!(missing_pub_confirm.1.contains("confirmation_required"));

        // With PublicRestore confirmation -> restores as published and keeps unknown "legacy_extra"!
        let restored = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Restore,
                type_key: "notice".into(),
                entry_id: notice_id,
                expected_version: 3,
            },
            &LifecycleAuthority {
                session: &admin,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::PublicRestore {
                    type_key: "notice".into(),
                    entry_id: notice_id,
                    expected_version: 3,
                    session_token_hash: admin.token_hash.clone(),
                },
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap();
        assert_eq!(restored.publication_after.as_deref(), Some("published"));
        let stored_vals: String =
            sqlx::query_scalar("SELECT values_json FROM content_entries WHERE id = ?")
                .bind(notice_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(stored_vals.contains("legacy_extra"));
    }

    #[tokio::test]
    async fn startup_builder_migration_preserves_trashed_row_lifecycle_and_pins() {
        let pool = setup_db().await;
        let admin = seed_actor(&pool, "admin", Role::Admin).await;

        let pic_id = "b0111111111111111111111111111111b0111111111111111111111111111111";
        sqlx::query(
            "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state)
             VALUES (?, 'image', 'pic.png', 'image/png', 'png', 16, 2, 2, 2, 2, 'active')",
        )
        .bind(pic_id)
        .execute(&pool)
        .await
        .unwrap();
        let page_id = seed_entry(&pool, "page", "vi", "legacy-demo-page", "published", "{}").await;
        let doc_json = serde_json::json!({
            "schema_version": 1,
            "sections": [{
                "id": "sec-1",
                "children": [{
                    "id": "row-1",
                    "children": [{
                        "id": "col-1",
                        "children": [{
                            "id": "el-1",
                            "type": "image",
                            "media_id": pic_id,
                            "alt": "Pic"
                        }]
                    }]
                }]
            }]
        })
        .to_string();
        sqlx::query("INSERT INTO entry_builder_documents (entry_id, document_json, updated_at) VALUES (?, ?, datetime('now'))")
            .bind(page_id)
            .bind(&doc_json)
            .execute(&pool)
            .await
            .unwrap();

        let trashed = execute_lifecycle_transition(
            &pool,
            &LifecycleCommand {
                operation: LifecycleOperation::Trash,
                type_key: "page".into(),
                entry_id: page_id,
                expected_version: 2,
            },
            &LifecycleAuthority {
                session: &admin,
                plugin_fence: None,
                confirmation: LifecycleConfirmationEvidence::None,
                scope: LifecycleScope::Ordinary,
            },
        )
        .await
        .unwrap();

        // Run startup Builder normalization maintenance: must preserve trashed_at, status, and pins,
        // and must NOT grant public media access!
        let migrated = crate::storage::builder::migrate_demo_entry_builder_docs_on_startup(&pool)
            .await
            .unwrap();
        assert_eq!(migrated, 1);

        let (pub_status, trashed_at, values_json): (String, Option<String>, String) =
            sqlx::query_as(
                "SELECT publication_status, trashed_at, values_json FROM content_entries WHERE id = ?",
            )
            .bind(page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(pub_status, "published");
        assert_eq!(trashed_at, trashed.trashed_at);
        assert!(values_json.contains(pic_id));

        // Media stays conservatively pinned in media_refs, but NOT publicly granted via media_is_referenced!
        let mut tx = pool.begin().await.unwrap();
        let pins = crate::storage::media_refs::collect_pins_in_tx(&mut tx, pic_id)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(pins.is_referenced());
        assert!(
            !crate::storage::public_content::media_is_referenced(&pool, pic_id)
                .await
                .unwrap()
        );
    }
}
