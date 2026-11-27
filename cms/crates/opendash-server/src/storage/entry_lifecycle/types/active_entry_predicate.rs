use super::*;

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
        detached_parent_id: None,
    })
}
