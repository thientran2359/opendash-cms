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
pub(crate) const TRASHED_ENTRY_PREDICATE: &str = "e.trashed_at IS NOT NULL";
pub(crate) const ACTIVE_ENTRY_UNALIASED: &str = "trashed_at IS NULL";
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
pub enum ProjectionOutcome {
    NotNeeded,
    Current,
    Failed { reason: String },
}

/// Bounded post-commit secondary work summary returned alongside committed responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecondaryOutcome {
    pub projection: ProjectionOutcome,
    pub hook_enqueued: u32,
    pub hook_dropped: u32,
}

impl SecondaryOutcome {
    pub fn none() -> Self {
        Self {
            projection: ProjectionOutcome::NotNeeded,
            hook_enqueued: 0,
            hook_dropped: 0,
        }
    }
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

/// Bounded metadata-only Trash entry view returned to authorized plugins (`content_trash_get`/`list`).
/// Never includes body, field values, SEO private data, or public URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrashEntryMetadata {
    pub id: i64,
    pub type_key: String,
    pub locale: String,
    pub title: String,
    pub slug: String,
    pub publication_status: String,
    pub version: i64,
    pub trashed_at: String,
    pub published_at: Option<String>,
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
    pub has_been_published: i64,
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
        i64,
        Option<String>,
        Option<String>,
        Option<i64>,
    );
    let row: Option<RowTuple> = sqlx::query_as(
        "SELECT id, content_type_key, locale, translation_group_id, slug, title, values_json,
                version, publication_status, has_been_published, trashed_at, published_at,
                primary_category_term_id
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
            has_been_published,
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
            has_been_published,
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
    let active = match &fence.owner_fence {
        crate::storage::entries::OwnerFence::Plugin {
            plugin_id,
            expected_generation,
        } => {
            super::plugin::is_plugin_generation_active(&mut **tx, plugin_id, *expected_generation)
                .await?
        }
        crate::storage::entries::OwnerFence::Theme {
            revision_id,
            selection_version,
            ..
        } => {
            super::plugin::verify_active_theme_fence_in_tx(
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

/// Recomputes post-commit projections without ever turning a durable DB commit into a failure.
pub async fn recompute_projections_nonfatal(
    db: &SqlitePool,
    hooks: &crate::hooks::HookDispatcher,
    entry_id: i64,
    needed: bool,
) -> ProjectionOutcome {
    if !needed {
        return ProjectionOutcome::NotNeeded;
    }
    match crate::storage::plugin_projections::recompute_entry_projections(db, hooks, entry_id).await
    {
        Ok(()) => ProjectionOutcome::Current,
        Err(err) => {
            eprintln!(
                "projection_recompute_failed entry_id={entry_id} status={}",
                err.0.as_u16()
            );
            ProjectionOutcome::Failed {
                reason: "projection_recompute_failed".into(),
            }
        }
    }
}

/// Loads all currently assigned Reading page IDs across all retained `site_reading_settings` rows
/// within the caller's read transaction so active table rows can expose exact blocked reasons.
pub(crate) async fn load_assigned_reading_page_ids_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
) -> ApiResult<HashSet<i64>> {
    let rows: Vec<(Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT homepage_page_id, posts_page_id FROM site_reading_settings")
            .fetch_all(&mut **tx)
            .await?;
    let mut ids = HashSet::new();
    for (home, posts) in rows {
        if let Some(id) = home {
            ids.insert(id);
        }
        if let Some(id) = posts {
            ids.insert(id);
        }
    }
    Ok(ids)
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

/// Returns `(kind, locale, active_and_declared)` for a single term ID (used by S03 hook scope checks).
pub(crate) async fn lookup_term_scope_in_conn(
    conn: &mut SqliteConnection,
    term_id: i64,
) -> ApiResult<Option<(String, String, bool)>> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT t.kind, t.locale, d.provider_status, d.declaration_state
           FROM taxonomy_terms t
           JOIN taxonomy_definitions d ON d.key = t.kind
          WHERE t.id = ?",
    )
    .bind(term_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(
        row.map(|(kind, locale, provider_status, declaration_state)| {
            (
                kind,
                locale,
                provider_status == "active" && declaration_state == "declared",
            )
        }),
    )
}

fn deserialize_non_null_option<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedAtPatch {
    pub local_datetime: String,
    pub timezone: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxonomyDeltaOp {
    Add,
    Remove,
    Replace,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataTaxonomyPatch {
    pub key: String,
    pub operation: TaxonomyDeltaOp,
    pub term_ids: Vec<i64>,
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

impl MetadataPatch {
    pub fn has_any_field(&self) -> bool {
        self.title.is_some()
            || self.slug.is_some()
            || self.publication_status.is_some()
            || self.published_at.is_some()
            || self.taxonomy.is_some()
    }
}

#[derive(Debug, Serialize)]
pub struct MetadataPatchResponse {
    #[serde(flatten)]
    pub row: crate::storage::entry_table::EntryTableRow,
    pub schema_token: String,
    pub secondary: SecondaryOutcome,
}

pub struct MetadataPreflightPlan {
    pub preflight_schema_token: String,
    pub effective: opendash_core::content::EffectiveContentSchema,
    pub entry_locale: String,
    pub translation_group_id: String,
    pub candidate_title: String,
    pub candidate_slug: String,
    pub candidate_values: BTreeMap<String, Value>,
    pub candidate_term_ids: Vec<i64>,
    pub candidate_status: String,
    pub explicit_published_at_utc: Option<String>,
    pub requested_title: bool,
    pub requested_slug: bool,
    pub requested_taxonomy_key: Option<String>,
    pub hook_modified_terms: bool,
}

fn validate_metadata_title(raw: &str) -> ApiResult<String> {
    if raw.chars().any(char::is_control) {
        return Err(ApiError::unprocessable(
            "Tiêu đề không được chứa ký tự điều khiển.",
        ));
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 200 {
        return Err(ApiError::unprocessable(
            "Tiêu đề phải có từ 1 đến 200 ký tự.",
        ));
    }
    Ok(trimmed.to_owned())
}

fn validate_metadata_slug(raw: &str) -> ApiResult<String> {
    if !valid_kebab_segment_with_limit(raw, 64) {
        return Err(ApiError::unprocessable(
            "Slug không hợp lệ (chỉ gồm chữ thường, số, gạch ngang và tối đa 64 ký tự).",
        ));
    }
    Ok(raw.to_owned())
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

async fn resolve_taxonomy_delta_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    entry_id: i64,
    entry_locale: &str,
    delta: &MetadataTaxonomyPatch,
) -> ApiResult<(Vec<i64>, std::collections::BTreeSet<i64>)> {
    if !valid_content_type_key(&delta.key) {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Khóa phân loại không hợp lệ.",
        ));
    }
    let tax_row: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM taxonomy_definitions WHERE key = ?",
    )
    .bind(&delta.key)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((provider_status, declaration_state)) = tax_row else {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Phân loại không tồn tại hoặc không khả dụng.",
        ));
    };
    if provider_status != "active" || declaration_state != "declared" {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Phân loại hiện không hoạt động hoặc đã bị thu hồi.",
        ));
    }
    let eligible = crate::storage::content::load_effective_taxonomies_in_tx(conn, type_key).await?;
    if !eligible.iter().any(|k| k == &delta.key) {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Phân loại không được phép dùng cho loại nội dung này.",
        ));
    }
    if matches!(
        delta.operation,
        TaxonomyDeltaOp::Add | TaxonomyDeltaOp::Remove
    ) && delta.term_ids.is_empty()
    {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Thao tác thêm hoặc gỡ phân loại cần ít nhất một term_id.",
        ));
    }
    let max_for_kind = if delta.key == "category" { 32 } else { 64 };
    if delta.term_ids.len() > max_for_kind {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Số lượng term vượt quá giới hạn của phân loại.",
        ));
    }
    let mut input_ids = std::collections::BTreeSet::new();
    for &tid in &delta.term_ids {
        if tid <= 0 {
            return Err(ApiError::unprocessable(
                "invalid_taxonomy_delta: Danh sách term_ids chứa ID không hợp lệ.",
            ));
        }
        if !input_ids.insert(tid) {
            return Err(ApiError::unprocessable(
                "invalid_taxonomy_delta: Danh sách term_ids không được chứa ID trùng lặp.",
            ));
        }
        let found: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT t.kind, t.locale, d.provider_status, d.declaration_state
               FROM taxonomy_terms t
               JOIN taxonomy_definitions d ON d.key = t.kind
              WHERE t.id = ?",
        )
        .bind(tid)
        .fetch_optional(&mut *conn)
        .await?;
        let Some((kind, term_locale, ps, ds)) = found else {
            return Err(ApiError::unprocessable(
                "invalid_taxonomy_delta: Term ID không tồn tại.",
            ));
        };
        if ps != "active" || ds != "declared" {
            return Err(ApiError::unprocessable(
                "invalid_taxonomy_delta: Term thuộc phân loại không hoạt động.",
            ));
        }
        if kind != delta.key {
            return Err(ApiError::unprocessable(
                "invalid_taxonomy_delta: Term ID không thuộc phân loại được chỉ định.",
            ));
        }
        if term_locale != entry_locale {
            return Err(ApiError::unprocessable(
                "invalid_taxonomy_delta: Term thuộc ngôn ngữ khác với nội dung.",
            ));
        }
    }

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
    let mut current_kind_ids = std::collections::BTreeSet::new();
    for (tid, kind, ps, ds) in existing_rows {
        if kind == delta.key {
            current_kind_ids.insert(tid);
        } else {
            other_ids.insert(tid);
            if ps == "active" && ds == "declared" && eligible.iter().any(|k| k == &kind) {
                other_active_eligible_count += 1;
            }
        }
    }

    let next_kind_ids: std::collections::BTreeSet<i64> = match delta.operation {
        TaxonomyDeltaOp::Add => current_kind_ids.union(&input_ids).copied().collect(),
        TaxonomyDeltaOp::Remove => current_kind_ids.difference(&input_ids).copied().collect(),
        TaxonomyDeltaOp::Replace => input_ids,
    };

    if next_kind_ids.len() > max_for_kind {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Số lượng term sau khi cập nhật vượt quá giới hạn của phân loại.",
        ));
    }
    if other_active_eligible_count + next_kind_ids.len() > 96 {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Tổng số term hoạt động của nội dung vượt quá giới hạn 96.",
        ));
    }

    let merged: Vec<i64> = other_ids.union(&next_kind_ids).copied().collect();
    Ok((merged, next_kind_ids))
}

async fn validate_hook_adjusted_terms_in_conn(
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

async fn prepare_metadata_patch_inner(
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

async fn commit_metadata_patch_inner(
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

    if status_changed {
        crate::storage::seo::sync_entry_seo_media_ref_in_tx(&mut tx, entry_id).await?;
        if final_status == "draft" {
            super::plugin_projections::delete_entry_projections_in_tx(&mut tx, entry_id).await?;
        }
    }

    let category_terms_changed = if terms_changed && type_key == "post" {
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
    };

    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok((committed_row, effective.schema_token, receipt))
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
