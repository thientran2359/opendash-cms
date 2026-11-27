//! M3/M5 theme package, immutable revision, and singleton active selection storage.
//! Never mutate `home_page` when activating or installing a theme.

#![allow(dead_code)]

use super::*;

pub(super) use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::{Component, Path, PathBuf},
};

pub(super) use opendash_core::registry::ProviderStatus;

pub(super) use opendash_core::{id::validate_package_id, theme::ThemeV3Manifest};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use sha2::{Digest, Sha256};

pub(super) use sqlx::SqlitePool;

pub(super) use crate::error::{ApiError, ApiResult};

pub(in crate::storage) const COMPLETE_MARKER: &str = ".complete";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeSourceFormat {
    V1Legacy,
    V2,
    V3,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemePackageRecord {
    pub theme_id: String,
    pub display_name: String,
    pub source_format: ThemeSourceFormat,
    pub is_legacy: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeRevisionRecord {
    pub revision_id: String,
    pub theme_id: String,
    pub version: String,
    pub revision_digest: String,
    pub source_format: ThemeSourceFormat,
    pub manifest_json: String,
    pub file_digests: BTreeMap<String, String>,
    pub provenance_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveThemeSelection {
    pub revision_id: Option<String>,
    pub theme_id: Option<String>,
    pub version: i64,
    pub source_format: Option<ThemeSourceFormat>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevisionDurabilityStatus {
    Valid,
    MissingDirectory,
    MissingCompleteMarker,
    CompleteMarkerMismatch,
    MissingFile(String),
    DigestMismatch(String),
}

#[derive(Debug)]
pub enum RevisionInstallOutcome {
    Installed(ThemeRevisionRecord),
    AlreadyExists(ThemeRevisionRecord),
}

#[derive(Debug)]
pub enum RevisionActivationOutcome {
    Activated(ActiveThemeSelection),
    NotFound,
    MissingExpectedVersion,
    Stale,
    CorruptRevision(RevisionDurabilityStatus),
}

/// Admin-only projection: never serialize manifest, path, provenance or file-map metadata.
#[derive(Debug, Serialize)]
pub struct ThemeRevisionSummary {
    pub revision_id: String,
    pub theme_id: String,
    pub version: String,
    pub revision_digest: String,
    pub source_format: ThemeSourceFormat,
}

#[derive(Debug, Serialize)]
pub struct ThemeList {
    pub version: i64,
    pub revisions: Vec<ThemeRevisionSummary>,
    pub selection: ActiveThemeSelection,
}

pub(in crate::storage) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

/// Compute individual file SHA-256 digests and the canonical file-map SHA-256 digest.
pub fn compute_canonical_file_map_digest(
    files: &BTreeMap<String, Vec<u8>>,
) -> (String, BTreeMap<String, String>) {
    let mut file_digests = BTreeMap::new();
    let mut hasher = Sha256::new();
    for (rel_path, content) in files {
        let file_hex = sha256_hex(content);
        hasher.update(rel_path.as_bytes());
        hasher.update(b"\0");
        hasher.update(file_hex.as_bytes());
        hasher.update(b"\0");
        hasher.update((content.len() as u64).to_le_bytes());
        hasher.update(b"\n");
        file_digests.insert(rel_path.clone(), file_hex);
    }
    let digest = hasher.finalize();
    let mut rev_hex = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut rev_hex, "{b:02x}");
    }
    (rev_hex, file_digests)
}

pub async fn ensure_default_starter_theme(
    db: &SqlitePool,
    themes_root: &Path,
) -> ApiResult<ThemeRevisionRecord> {
    use crate::builtin_themes::{
        STARTER_THEME_ID, STARTER_THEME_NAME, STARTER_THEME_VERSION, builtin_starter_theme_files,
    };

    let files = builtin_starter_theme_files();
    let pkg = crate::theme::validate_theme_v3_files(files.clone()).map_err(|error| {
        eprintln!("Built-in Starter Theme validation: {error:?}");
        ApiError::internal("Built-in Starter Theme v3 không hợp lệ.")
    })?;
    let (digest, file_digests) = compute_canonical_file_map_digest(&files);
    repair_builtin_starter_revision_dir_if_needed(themes_root, &digest, &file_digests)?;

    // Cleanly replace any pre-V1 opendash-starter revisions with differing digests
    // without triggering 409 version conflicts or FK ON DELETE RESTRICT failures.
    let stale_rev_ids: Vec<String> = sqlx::query_scalar(
        "SELECT revision_id FROM theme_revisions
          WHERE theme_id = ? AND source_format = 'v3' AND revision_digest != ?",
    )
    .bind(STARTER_THEME_ID)
    .bind(&digest)
    .fetch_all(db)
    .await?;
    if !stale_rev_ids.is_empty() {
        let mut conn = db.acquire().await?;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
        let res: ApiResult<()> = async {
            let active_rev: Option<String> =
                sqlx::query_scalar("SELECT revision_id FROM theme_active_selection WHERE id = 1")
                    .fetch_optional(&mut *conn)
                    .await?
                    .flatten();
            if active_rev
                .as_deref()
                .is_some_and(|id| stale_rev_ids.iter().any(|stale| stale == id))
            {
                sqlx::query("UPDATE theme_active_selection SET revision_id = NULL WHERE id = 1")
                    .execute(&mut *conn)
                    .await?;
            }
            sqlx::query(
                "DELETE FROM theme_revisions
                  WHERE theme_id = ? AND source_format = 'v3' AND revision_digest != ?",
            )
            .bind(STARTER_THEME_ID)
            .bind(&digest)
            .execute(&mut *conn)
            .await?;
            Ok(())
        }
        .await;
        match res {
            Ok(()) => {
                sqlx::query("COMMIT").execute(&mut *conn).await?;
                crate::cache::SiteCacheEpoch::global().bump();
            }
            Err(err) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(err);
            }
        }
    }

    let starter = match stage_and_commit_v3_revision(
        db,
        themes_root,
        "bootstrap-starter-v3",
        STARTER_THEME_ID,
        STARTER_THEME_NAME,
        STARTER_THEME_VERSION,
        &pkg.manifest_json,
        r#"{"source":"builtin_starter"}"#,
        &files,
    )
    .await?
    {
        RevisionInstallOutcome::Installed(starter)
        | RevisionInstallOutcome::AlreadyExists(starter) => starter,
    };

    let selection = get_active_selection(db).await?;
    if selection.revision_id.is_none() {
        match activate_revision(
            db,
            themes_root,
            &starter.revision_id,
            Some(selection.version),
        )
        .await?
        {
            RevisionActivationOutcome::Activated(_) => {}
            other => {
                eprintln!("Starter activation error: {other:?}");
                return Err(ApiError::internal(
                    "Không thể kích hoạt Starter theme mặc định.",
                ));
            }
        }
        if selection.version == 1 {
            // Fresh install: preserve historical CAS/ETag v1 while the
            // activation transaction still synced blocks and media ACL.
            let done = sqlx::query(
                "UPDATE theme_active_selection SET version = 1
                 WHERE id = 1 AND revision_id = ? AND version = 2",
            )
            .bind(&starter.revision_id)
            .execute(db)
            .await?;
            if done.rows_affected() != 1 {
                return Err(ApiError::internal("Không thể giữ ETag v1 mặc định."));
            }
        }
    }
    Ok(starter)
}

/// Locally verified active theme v3 source. Each template is indexed by the
/// canonical typed resolver key, never by the older coarse role names.
#[derive(Clone, Debug)]
pub struct ActiveThemeV3 {
    pub revision_id: String,
    pub revision_digest: String,
    pub manifest: ThemeV3Manifest,
    pub templates: BTreeMap<String, String>,
    pub parts: BTreeMap<String, String>,
    pub page_layouts: BTreeMap<String, String>,
    pub l10n_catalogs: BTreeMap<String, std::sync::Arc<opendash_core::l10n::Catalog>>,
}

/// Provider-owned selection metadata is kept separate from site-owned typed
/// template overrides. A withdrawn provider never changes the stored row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerTemplateSelection {
    pub owner_kind: String,
    pub owner_id: String,
    pub role: String,
    pub scope_type_key: Option<String>,
    pub template_selection_key: Option<String>,
    pub ast_json: String,
    pub version: i64,
}

/// Read-only descriptors for the *active*, locally durable theme revision.
/// Never serialize installed HTML/templates/manifest or arbitrary filesystem
/// paths into the Admin library catalog.
#[derive(Debug, Serialize)]
pub struct LocalThemeTemplateDescriptor {
    pub role: String,
    pub theme_id: String,
    pub revision_id: String,
    pub revision_digest: String,
    pub can_apply: bool,
    pub reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct LocalThemeCatalog {
    pub active_theme_id: Option<String>,
    pub theme_state: &'static str,
    pub theme_templates: Vec<LocalThemeTemplateDescriptor>,
}

#[derive(Debug, Clone)]
pub struct ActiveThemeFunctionsSnapshot {
    pub theme_id: String,
    pub revision_id: String,
    pub revision_digest: String,
    pub selection_version: i64,
    pub manifest_json: String,
    pub file_digests: BTreeMap<String, String>,
}

/// Capture the active manifest and verified disk revision once. Preview loads
/// only the requested part sources from this snapshot.
pub(in crate::storage) struct CapturedThemeV3 {
    pub manifest: ThemeV3Manifest,
    pub(in crate::storage) revision: ThemeRevisionRecord,
    pub(in crate::storage) rev_dir: std::path::PathBuf,
    pub(in crate::storage) dev_overlay: Option<crate::dev_mode::DevThemeSnapshot>,
}

pub(in crate::storage) async fn capture_active_theme_v3_in_conn(
    conn: &mut sqlx::SqliteConnection,
    themes_root: &Path,
) -> ApiResult<Option<CapturedThemeV3>> {
    if let Some(dev) = crate::dev_mode::DevOverlayConfig::global()
        && let Some(dev_root) = dev.dev_theme_root()
    {
        match dev.load_dev_theme() {
            Ok(Some(snap)) => {
                let synthetic_rev = ThemeRevisionRecord {
                    revision_id: format!("dev:{}", snap.manifest.theme_id),
                    theme_id: snap.manifest.theme_id.clone(),
                    version: snap.manifest.version.clone(),
                    revision_digest: snap.revision_digest.clone(),
                    source_format: ThemeSourceFormat::V3,
                    manifest_json: snap.manifest_json.clone(),
                    file_digests: BTreeMap::new(),
                    provenance_json: "{}".to_owned(),
                    created_at: String::new(),
                };
                return Ok(Some(CapturedThemeV3 {
                    manifest: snap.manifest.clone(),
                    revision: synthetic_rev,
                    rev_dir: dev_root.to_path_buf(),
                    dev_overlay: Some(snap),
                }));
            }
            Ok(None) => {}
            Err(err) => {
                eprintln!("Dev theme overlay load failed: {err}");
                return Ok(None);
            }
        }
    }
    // Snapshot the active revision in a single SQL query. Files may be loaded
    // afterwards because immutable digest/complete marker checks hold.
    let row: Option<RevisionRow> = sqlx::query_as(
        "SELECT r.revision_id, r.theme_id, r.version, r.revision_digest, r.source_format,
                r.manifest_json, r.file_digests_json, r.provenance_json, r.created_at
           FROM theme_active_selection a
           JOIN theme_revisions r ON r.revision_id = a.revision_id
          WHERE a.id = 1",
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let revision = row_to_revision(row)?;
    if revision.source_format != ThemeSourceFormat::V3 {
        return Ok(None);
    }
    if verify_revision_durability(themes_root, &revision) != RevisionDurabilityStatus::Valid {
        return Ok(None);
    }
    let Ok(manifest) = serde_json::from_str::<ThemeV3Manifest>(&revision.manifest_json) else {
        return Ok(None);
    };
    if manifest.validate().is_err()
        || manifest.theme_id != revision.theme_id
        || manifest.version != revision.version
    {
        return Ok(None);
    }
    let rev_dir = themes_root
        .join("revisions")
        .join(&revision.revision_digest);
    // SQLite manifest must match the verified disk file, including bytes.
    if std::fs::read_to_string(rev_dir.join("theme.json"))
        .ok()
        .as_deref()
        != Some(revision.manifest_json.as_str())
    {
        return Ok(None);
    }
    Ok(Some(CapturedThemeV3 {
        manifest,
        revision,
        rev_dir,
        dev_overlay: None,
    }))
}
