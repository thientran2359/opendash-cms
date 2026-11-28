use super::*;

pub(super) use std::{collections::BTreeMap, fs, io::Write, path::Path};

pub(super) use opendash_core::{block_schema::BlockSchema, registry::ProviderStatus};

pub(super) use sha2::{Digest, Sha256};

pub(super) use sqlx::{Row, SqliteConnection, SqlitePool};

pub(in crate::storage) const COMPLETE_MARKER: &str = ".complete";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::storage) struct StoredBlock {
    pub(in crate::storage) schema: BlockSchema,
    pub(in crate::storage) template: String,
}

#[derive(Debug, Clone)]
pub enum PluginStorageError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    PreconditionRequired(String),
    PreconditionFailed(String),
    Internal(String),
}

pub(super) use opendash_core::plugin::{PluginManifest, PluginPermissions};

pub(super) use crate::plugin::PluginSummary;

pub struct CachedActivePluginRow {
    pub id: String,
    pub wasm_blob: Vec<u8>,
    pub manifest_json: String,
    pub revision_generation: i64,
    pub revision_digest: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OwnerLogEntry {
    pub id: i64,
    pub correlation_id: String,
    pub level: String,
    pub message: String,
    pub created_at: String,
}

/// Read the host projection's source row only if its locale is visible in
/// this scope. The check and hydration use one SQLite read snapshot.
pub(crate) async fn get_entry_for_host(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
    enforce_public_locale: bool,
) -> crate::error::ApiResult<Option<crate::storage::entries::ContentEntryRecord>> {
    let mut tx = db.begin().await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM content_entries e
          JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
          WHERE e.id = ? AND e.content_type_key = ? AND e.trashed_at IS NULL
            AND (? = 0 OR (sl.enabled = 1 AND e.publication_status = 'published'
              AND (sl.is_default = 1 OR
                  (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1))))",
    )
    .bind(entry_id)
    .bind(type_key)
    .bind(i64::from(enforce_public_locale))
    .fetch_one(&mut *tx)
    .await?;
    let entry = if visible {
        crate::storage::entries::get_entry_in_tx(&mut tx, type_key, entry_id).await?
    } else {
        None
    };
    tx.commit().await?;
    Ok(entry)
}
