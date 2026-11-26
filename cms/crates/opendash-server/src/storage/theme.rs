//! M3/M5 theme package, immutable revision, and singleton active selection storage.
//! Never mutate `home_page` when activating or installing a theme.
#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::{Component, Path, PathBuf},
};

use opendash_core::registry::ProviderStatus;
use opendash_core::{id::validate_package_id, theme::ThemeV3Manifest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::error::{ApiError, ApiResult};

const COMPLETE_MARKER: &str = ".complete";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeSourceFormat {
    V1Legacy,
    V2,
    V3,
}

impl ThemeSourceFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1Legacy => "v1_legacy",
            Self::V2 => "v2",
            Self::V3 => "v3",
        }
    }

    fn from_db(raw: &str) -> ApiResult<Self> {
        match raw {
            "v1_legacy" => Ok(Self::V1Legacy),
            "v2" => Ok(Self::V2),
            "v3" => Ok(Self::V3),
            _ => Err(ApiError::internal("Định dạng theme không hợp lệ trong DB.")),
        }
    }
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

fn sha256_hex(bytes: &[u8]) -> String {
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

fn safe_relative_path(raw: &str) -> ApiResult<PathBuf> {
    if raw.is_empty() || raw.contains('\\') || raw.contains(':') || raw.contains('\0') {
        return Err(ApiError::bad_request("Đường dẫn file theme không hợp lệ."));
    }
    let rel = Path::new(raw);
    let mut clean = PathBuf::new();
    for comp in rel.components() {
        match comp {
            Component::Normal(seg) => {
                let s = seg
                    .to_str()
                    .ok_or_else(|| ApiError::bad_request("Tên file theme không hợp lệ."))?;
                if s.is_empty() || s == "." || s == ".." || s == COMPLETE_MARKER {
                    return Err(ApiError::bad_request("Đường dẫn file theme bị từ chối."));
                }
                clean.push(s);
            }
            _ => return Err(ApiError::bad_request("Đường dẫn file theme không hợp lệ.")),
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(ApiError::bad_request("Đường dẫn file theme rỗng."));
    }
    Ok(clean)
}

/// Ensure `<site_root>/themes/{.staging,revisions,archives}` exist and clean up
/// any stale `.staging/*` directories left by crashed runs without touching `revisions/`.
pub fn ensure_themes_layout_and_quarantine_stale(themes_root: &Path) -> ApiResult<usize> {
    let staging = themes_root.join(".staging");
    let revisions = themes_root.join("revisions");
    let archives = themes_root.join("archives");
    for dir in [&staging, &revisions, &archives] {
        fs::create_dir_all(dir)
            .map_err(|_| ApiError::internal("Không thể khởi tạo thư mục themes/."))?;
    }
    let mut cleaned = 0usize;
    let entries = fs::read_dir(&staging)
        .map_err(|_| ApiError::internal("Không thể đọc thư mục themes/.staging."))?;
    for entry in entries {
        let entry =
            entry.map_err(|_| ApiError::internal("Không thể duyệt thư mục themes/.staging."))?;
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|_| {
                ApiError::internal("Không thể dọn staging cũ trong themes/.staging.")
            })?;
        } else {
            fs::remove_file(&path).map_err(|_| {
                ApiError::internal("Không thể dọn file staging cũ trong themes/.staging.")
            })?;
        }
        cleaned += 1;
    }
    Ok(cleaned)
}

/// Verify A-DURABILITY `.complete` marker and file SHA-256 digests on disk.
pub fn verify_revision_durability(
    themes_root: &Path,
    revision: &ThemeRevisionRecord,
) -> RevisionDurabilityStatus {
    if revision.source_format == ThemeSourceFormat::V1Legacy {
        return RevisionDurabilityStatus::Valid;
    }
    let rev_dir = themes_root
        .join("revisions")
        .join(&revision.revision_digest);
    if !rev_dir.is_dir() {
        return RevisionDurabilityStatus::MissingDirectory;
    }
    let marker_path = rev_dir.join(COMPLETE_MARKER);
    let Ok(marker) = fs::read_to_string(&marker_path) else {
        return RevisionDurabilityStatus::MissingCompleteMarker;
    };
    if marker.trim() != revision.revision_digest {
        return RevisionDurabilityStatus::CompleteMarkerMismatch;
    }
    for (rel, expected_hex) in &revision.file_digests {
        let Ok(clean) = safe_relative_path(rel) else {
            return RevisionDurabilityStatus::MissingFile(rel.clone());
        };
        let target = rev_dir.join(clean);
        let Ok(bytes) = fs::read(&target) else {
            return RevisionDurabilityStatus::MissingFile(rel.clone());
        };
        if &sha256_hex(&bytes) != expected_hex {
            return RevisionDurabilityStatus::DigestMismatch(rel.clone());
        }
    }
    RevisionDurabilityStatus::Valid
}

fn dir_matches_revision(
    rev_dir: &Path,
    digest: &str,
    file_digests: &BTreeMap<String, String>,
) -> bool {
    let marker_path = rev_dir.join(COMPLETE_MARKER);
    let Ok(marker) = fs::read_to_string(&marker_path) else {
        return false;
    };
    if marker.trim() != digest {
        return false;
    }
    for (rel, expected_hex) in file_digests {
        let Ok(clean) = safe_relative_path(rel) else {
            return false;
        };
        let Ok(bytes) = fs::read(rev_dir.join(clean)) else {
            return false;
        };
        if &sha256_hex(&bytes) != expected_hex {
            return false;
        }
    }
    true
}

fn write_files_durably(staging_dir: &Path, files: &BTreeMap<String, Vec<u8>>) -> ApiResult<()> {
    fs::create_dir_all(staging_dir)
        .map_err(|_| ApiError::internal("Không thể tạo thư mục staging cho theme."))?;
    for (rel, bytes) in files {
        let clean = safe_relative_path(rel)?;
        let dest = staging_dir.join(clean);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| ApiError::internal("Không thể tạo thư mục con cho file theme."))?;
        }
        {
            let mut f = File::create(&dest)
                .map_err(|_| ApiError::internal("Không thể ghi file theme vào staging."))?;
            f.write_all(bytes)
                .map_err(|_| ApiError::internal("Lỗi ghi nội dung file theme."))?;
            f.sync_all()
                .map_err(|_| ApiError::internal("Lỗi đồng bộ file theme xuống đĩa."))?;
            // File handle is dropped here before rename (required on Windows).
        }
    }
    Ok(())
}

type RevisionRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);

fn row_to_revision(row: RevisionRow) -> ApiResult<ThemeRevisionRecord> {
    let (
        revision_id,
        theme_id,
        version,
        revision_digest,
        source_format,
        manifest_json,
        file_digests_json,
        provenance_json,
        created_at,
    ) = row;
    let file_digests: BTreeMap<String, String> = serde_json::from_str(&file_digests_json)?;
    Ok(ThemeRevisionRecord {
        revision_id,
        theme_id,
        version,
        revision_digest,
        source_format: ThemeSourceFormat::from_db(&source_format)?,
        manifest_json,
        file_digests,
        provenance_json,
        created_at,
    })
}

pub async fn find_revision_by_id(
    db: &SqlitePool,
    revision_id: &str,
) -> ApiResult<Option<ThemeRevisionRecord>> {
    let row: Option<RevisionRow> = sqlx::query_as(
        "SELECT revision_id, theme_id, version, revision_digest, source_format,
                manifest_json, file_digests_json, provenance_json, created_at
         FROM theme_revisions
         WHERE revision_id = ?",
    )
    .bind(revision_id)
    .fetch_optional(db)
    .await?;
    row.map(row_to_revision).transpose()
}

async fn find_revision_by_digest(
    conn: &mut sqlx::SqliteConnection,
    digest: &str,
) -> ApiResult<Option<ThemeRevisionRecord>> {
    let row: Option<RevisionRow> = sqlx::query_as(
        "SELECT revision_id, theme_id, version, revision_digest, source_format,
                manifest_json, file_digests_json, provenance_json, created_at
         FROM theme_revisions
         WHERE revision_digest = ?",
    )
    .bind(digest)
    .fetch_optional(conn)
    .await?;
    row.map(row_to_revision).transpose()
}

/// Stage validated v3 theme files durably under `themes/.staging/<run_uuid>/`,
/// atomically publish to `themes/revisions/<revision_digest>/` with `.complete` marker,
/// and commit metadata to SQLite (`BEGIN IMMEDIATE`) without activating it.
#[allow(clippy::too_many_arguments)]
pub async fn stage_and_commit_v3_revision(
    db: &SqlitePool,
    themes_root: &Path,
    run_uuid: &str,
    theme_id: &str,
    display_name: &str,
    version: &str,
    manifest_json: &str,
    provenance_json: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> ApiResult<RevisionInstallOutcome> {
    // A direct storage caller cannot bypass the ZIP manifest allowlist,
    // C-SEC composition or declared asset checks before writing to disk.
    let validated = crate::theme::validate_theme_v3_files(files.clone())
        .map_err(|_| ApiError::bad_request("Theme v3 files hoặc manifest không hợp lệ."))?;
    if validated.manifest_json != manifest_json {
        return Err(ApiError::bad_request(
            "Theme v3 manifest_json không khớp file theme.json.",
        ));
    }
    let manifest = validated.manifest;
    if manifest.theme_id != theme_id || manifest.name != display_name || manifest.version != version
    {
        return Err(ApiError::bad_request(
            "Theme metadata không khớp manifest v3.",
        ));
    }
    crate::theme::parse_theme_declarations(manifest_json).map_err(ApiError::bad_request)?;
    let theme_action_ids: Vec<String> = manifest
        .functions
        .as_ref()
        .map(|f| f.actions.iter().map(|a| a.id.clone()).collect())
        .unwrap_or_default();
    if let Some(ext) = &manifest.admin_extensions {
        ext.validate("theme", &theme_action_ids)
            .map_err(ApiError::bad_request)?;
    }
    crate::storage::admin_extensions::require_capacity(
        db,
        "theme",
        theme_id,
        manifest.admin_extensions.as_ref(),
    )
    .await?;
    validate_package_id(theme_id).map_err(ApiError::bad_request)?;
    if run_uuid.is_empty()
        || run_uuid.contains('/')
        || run_uuid.contains('\\')
        || run_uuid.contains("..")
    {
        return Err(ApiError::bad_request("run_uuid staging không hợp lệ."));
    }
    for rel in files.keys() {
        let _ = safe_relative_path(rel)?;
    }

    let (revision_digest, file_digests) = compute_canonical_file_map_digest(files);
    let file_digests_json = serde_json::to_string(&file_digests)?;

    // Check DB pre-conditions before writing disk.
    {
        let mut conn = db.acquire().await?;
        let format: Option<String> =
            sqlx::query_scalar("SELECT source_format FROM theme_packages WHERE theme_id = ?")
                .bind(theme_id)
                .fetch_optional(&mut *conn)
                .await?;
        if format.as_deref().is_some_and(|f| f != "v3") {
            return Err(ApiError::conflict("Theme ID đang thuộc package format cũ."));
        }
        if let Some(existing) = find_revision_by_digest(&mut conn, &revision_digest).await? {
            if existing.theme_id == theme_id
                && existing.version == version
                && existing.source_format == ThemeSourceFormat::V3
            {
                // An immutable revision can lose its on-disk files after a
                // partial restore or operator accident. Re-publish those
                // exact validated bytes without replacing its DB identity.
                if dir_matches_revision(
                    &themes_root.join("revisions").join(&revision_digest),
                    &revision_digest,
                    &file_digests,
                ) {
                    return Ok(RevisionInstallOutcome::AlreadyExists(existing));
                }
            } else {
                return Err(ApiError::conflict(
                    "Digest revision đã tồn tại cho một theme/version khác.",
                ));
            }
        }
        let version_conflict: Option<String> = sqlx::query_scalar(
            "SELECT revision_digest FROM theme_revisions WHERE theme_id = ? AND version = ?",
        )
        .bind(theme_id)
        .bind(version)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(other_digest) = version_conflict
            && other_digest != revision_digest
        {
            return Err(ApiError::conflict(
                "Cùng theme_id và version nhưng khác canonical digest.",
            ));
        }
    }

    // A-DURABILITY write-before-commit filesystem protocol.
    let staging_dir = themes_root.join(".staging").join(run_uuid);
    let rev_dir = themes_root.join("revisions").join(&revision_digest);
    fs::create_dir_all(themes_root.join(".staging"))
        .map_err(|_| ApiError::internal("Không thể tạo thư mục .staging."))?;
    fs::create_dir_all(themes_root.join("revisions"))
        .map_err(|_| ApiError::internal("Không thể tạo thư mục revisions."))?;

    write_files_durably(&staging_dir, files)?;

    if rev_dir.is_dir() && dir_matches_revision(&rev_dir, &revision_digest, &file_digests) {
        let _ = fs::remove_dir_all(&staging_dir);
    } else {
        if rev_dir.exists() {
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(ApiError::conflict(
                "Thư mục revision trên đĩa không khớp .complete hoặc bị hỏng.",
            ));
        }
        if let Err(err) = fs::rename(&staging_dir, &rev_dir) {
            let _ = fs::remove_dir_all(&staging_dir);
            eprintln!("Theme revision rename error: {err}");
            return Err(ApiError::internal(
                "Không thể publish thư mục revision vào themes/revisions/.",
            ));
        }
        let marker_path = rev_dir.join(COMPLETE_MARKER);
        let write_marker = (|| -> std::io::Result<()> {
            let mut f = File::create(&marker_path)?;
            f.write_all(revision_digest.as_bytes())?;
            f.write_all(b"\n")?;
            f.sync_all()?;
            Ok(())
        })();
        if let Err(err) = write_marker {
            eprintln!("Theme .complete marker error: {err}");
            return Err(ApiError::internal(
                "Không thể ghi marker .complete cho revision.",
            ));
        }
    }

    // Commit SQLite metadata only after durable publication succeeds.
    let revision_id = format!("rev-{revision_digest}");
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;

    let tx_res: ApiResult<RevisionInstallOutcome> = async {
        let format: Option<String> =
            sqlx::query_scalar("SELECT source_format FROM theme_packages WHERE theme_id = ?")
                .bind(theme_id)
                .fetch_optional(&mut *conn)
                .await?;
        if format.as_deref().is_some_and(|f| f != "v3") {
            return Err(ApiError::conflict("Theme ID đang thuộc package format cũ."));
        }
        if let Some(existing) = find_revision_by_digest(&mut conn, &revision_digest).await? {
            if existing.theme_id == theme_id
                && existing.version == version
                && existing.source_format == ThemeSourceFormat::V3
            {
                return Ok(RevisionInstallOutcome::AlreadyExists(existing));
            }
            return Err(ApiError::conflict(
                "Digest revision đã tồn tại cho một theme/version khác.",
            ));
        }
        let conflict: Option<String> = sqlx::query_scalar(
            "SELECT revision_digest FROM theme_revisions WHERE theme_id = ? AND version = ?",
        )
        .bind(theme_id)
        .bind(version)
        .fetch_optional(&mut *conn)
        .await?;
        if conflict.is_some() {
            return Err(ApiError::conflict(
                "Cùng theme_id và version nhưng khác canonical digest.",
            ));
        }

        sqlx::query(
            "INSERT INTO theme_packages (theme_id, display_name, source_format, is_legacy)
             VALUES (?, ?, 'v3', 0)
             ON CONFLICT(theme_id) DO NOTHING",
        )
        .bind(theme_id)
        .bind(display_name)
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            "INSERT INTO theme_revisions (
                revision_id, theme_id, version, revision_digest, source_format,
                manifest_json, file_digests_json, legacy_block_id, provenance_json
             ) VALUES (?, ?, ?, ?, 'v3', ?, ?, NULL, ?)",
        )
        .bind(&revision_id)
        .bind(theme_id)
        .bind(version)
        .bind(&revision_digest)
        .bind(manifest_json)
        .bind(&file_digests_json)
        .bind(provenance_json)
        .execute(&mut *conn)
        .await?;

        let inserted = find_revision_by_digest(&mut conn, &revision_digest)
            .await?
            .ok_or_else(|| ApiError::internal("Không đọc lại được revision vừa lưu."))?;
        Ok(RevisionInstallOutcome::Installed(inserted))
    }
    .await;

    match tx_res {
        Ok(outcome) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(outcome)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

/// Embed/install the OpenDash Starter only at exclusive server startup. A
/// repaired revision is reconstructed from the signed-in-binary file set;
/// existing site content and any explicitly selected theme are untouched.
fn repair_builtin_starter_revision_dir_if_needed(
    themes_root: &Path,
    digest: &str,
    file_digests: &BTreeMap<String, String>,
) -> ApiResult<()> {
    let revision_dir = themes_root.join("revisions").join(digest);
    if let Ok(metadata) = fs::symlink_metadata(&revision_dir) {
        // Never follow a symlink or replace an unknown object while repairing.
        if metadata.file_type().is_symlink() {
            return Err(ApiError::internal(
                "Starter revision path là symlink; không thể tự phục hồi.",
            ));
        }
        if !dir_matches_revision(&revision_dir, digest, file_digests) {
            if !metadata.is_dir() {
                return Err(ApiError::internal(
                    "Starter revision path không phải thư mục.",
                ));
            }
            fs::remove_dir_all(&revision_dir)
                .map_err(|_| ApiError::internal("Không thể phục hồi thư mục Starter revision."))?;
        }
    }
    Ok(())
}

pub async fn ensure_default_starter_theme(
    db: &SqlitePool,
    themes_root: &Path,
) -> ApiResult<ThemeRevisionRecord> {
    use crate::builtin_themes::{
        STARTER_THEME_ID, STARTER_THEME_NAME, STARTER_THEME_VERSION, builtin_starter_theme_files,
        builtin_starter_theme_v100_files, builtin_starter_theme_v101_files,
        builtin_starter_theme_v110_files,
    };
    // Pre-existing canonical v1.0.0 / v1.0.1 / v1.1.0 records are eligible for
    // exact-byte repair. Fresh sites must never receive a fabricated legacy revision.
    for (legacy_version, repair_label, legacy_files) in [
        (
            "1.0.0",
            "bootstrap-starter-v100-repair",
            builtin_starter_theme_v100_files(),
        ),
        (
            "1.0.1",
            "bootstrap-starter-v101-repair",
            builtin_starter_theme_v101_files(),
        ),
        (
            "1.1.0",
            "bootstrap-starter-v110-repair",
            builtin_starter_theme_v110_files(),
        ),
    ] {
        let (legacy_digest, legacy_file_digests) = compute_canonical_file_map_digest(&legacy_files);
        let installed_legacy = {
            let mut conn = db.acquire().await?;
            find_revision_by_digest(&mut conn, &legacy_digest).await?
        };
        if let Some(old) = installed_legacy {
            if old.theme_id == STARTER_THEME_ID
                && old.version == legacy_version
                && old.source_format == ThemeSourceFormat::V3
            {
                repair_builtin_starter_revision_dir_if_needed(
                    themes_root,
                    &legacy_digest,
                    &legacy_file_digests,
                )?;
                let old_pkg =
                    crate::theme::validate_theme_v3_files(legacy_files.clone()).map_err(|_| {
                        ApiError::internal("Frozen Starter legacy snapshot không hợp lệ.")
                    })?;
                stage_and_commit_v3_revision(
                    db,
                    themes_root,
                    repair_label,
                    STARTER_THEME_ID,
                    STARTER_THEME_NAME,
                    legacy_version,
                    &old_pkg.manifest_json,
                    r#"{"source":"builtin_starter"}"#,
                    &legacy_files,
                )
                .await?;
            }
        }
    }
    let files = builtin_starter_theme_files();
    let pkg = crate::theme::validate_theme_v3_files(files.clone()).map_err(|error| {
        eprintln!("Built-in Starter Theme validation: {error:?}");
        ApiError::internal("Built-in Starter Theme v3 không hợp lệ.")
    })?;
    let (digest, file_digests) = compute_canonical_file_map_digest(&files);
    repair_builtin_starter_revision_dir_if_needed(themes_root, &digest, &file_digests)?;

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

pub async fn get_active_selection(db: &SqlitePool) -> ApiResult<ActiveThemeSelection> {
    let row: (Option<String>, Option<String>, i64, Option<String>) = sqlx::query_as(
        "SELECT a.revision_id, r.theme_id, a.version, r.source_format
             FROM theme_active_selection a
             LEFT JOIN theme_revisions r ON r.revision_id = a.revision_id
             WHERE a.id = 1",
    )
    .fetch_one(db)
    .await?;
    let source_format = row
        .3
        .as_deref()
        .map(ThemeSourceFormat::from_db)
        .transpose()?;
    Ok(ActiveThemeSelection {
        revision_id: row.0,
        theme_id: row.1,
        version: row.2,
        source_format,
    })
}

pub async fn list_packages(db: &SqlitePool) -> ApiResult<Vec<ThemePackageRecord>> {
    let rows: Vec<(String, String, String, i64, String)> = sqlx::query_as(
        "SELECT theme_id, display_name, source_format, is_legacy, created_at
         FROM theme_packages
         ORDER BY is_legacy ASC, theme_id ASC",
    )
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(
            |(theme_id, display_name, source_format, is_legacy, created_at)| {
                Ok(ThemePackageRecord {
                    theme_id,
                    display_name,
                    source_format: ThemeSourceFormat::from_db(&source_format)?,
                    is_legacy: is_legacy != 0,
                    created_at,
                })
            },
        )
        .collect()
}

pub async fn list_revisions(
    db: &SqlitePool,
    theme_id: &str,
) -> ApiResult<Vec<ThemeRevisionRecord>> {
    let rows: Vec<RevisionRow> = sqlx::query_as(
        "SELECT revision_id, theme_id, version, revision_digest, source_format,
                manifest_json, file_digests_json, provenance_json, created_at
         FROM theme_revisions
         WHERE theme_id = ?
         ORDER BY created_at ASC, revision_id ASC",
    )
    .bind(theme_id)
    .fetch_all(db)
    .await?;
    rows.into_iter().map(row_to_revision).collect()
}

/// Activate or rollback to an existing immutable revision using optimistic CAS (`expected_version`).
/// Verifies A-DURABILITY on disk before mutating `theme_active_selection`.
pub async fn activate_revision(
    db: &SqlitePool,
    themes_root: &Path,
    revision_id: &str,
    expected_version: Option<i64>,
) -> ApiResult<RevisionActivationOutcome> {
    let Some(version) = expected_version else {
        return Ok(RevisionActivationOutcome::MissingExpectedVersion);
    };
    let row: Option<RevisionRow> = sqlx::query_as(
        "SELECT revision_id, theme_id, version, revision_digest, source_format,
                manifest_json, file_digests_json, provenance_json, created_at
         FROM theme_revisions
         WHERE revision_id = ?",
    )
    .bind(revision_id)
    .fetch_optional(db)
    .await?;
    let Some(row) = row else {
        return Ok(RevisionActivationOutcome::NotFound);
    };
    let revision = row_to_revision(row)?;
    // Legacy ZIP revisions can stay in historical DB records; only validated
    // v3 revisions may be newly activated after the S02 cutover.
    if revision.source_format != ThemeSourceFormat::V3 {
        return Ok(RevisionActivationOutcome::NotFound);
    }
    let manifest: ThemeV3Manifest = serde_json::from_str(&revision.manifest_json)
        .map_err(|_| ApiError::bad_request("Theme revision manifest v3 không hợp lệ."))?;
    manifest.validate().map_err(ApiError::bad_request)?;
    if manifest.theme_id != revision.theme_id || manifest.version != revision.version {
        return Err(ApiError::bad_request(
            "Theme revision metadata không khớp manifest.",
        ));
    }
    let declarations = crate::theme::parse_theme_declarations(&revision.manifest_json)
        .map_err(|msg| ApiError(axum::http::StatusCode::UNPROCESSABLE_ENTITY, msg.into()))?;
    let extension_manifest: serde_json::Value = serde_json::from_str(&revision.manifest_json)
        .map_err(|_| ApiError::internal("Theme manifest của revision bị hỏng."))?;
    let extension_decls: Option<opendash_core::admin_extensions::AdminExtensions> =
        extension_manifest
            .get("admin_extensions")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| ApiError::internal("Admin extensions theme revision bị hỏng."))?;
    let durability = verify_revision_durability(themes_root, &revision);
    if durability != RevisionDurabilityStatus::Valid {
        return Ok(RevisionActivationOutcome::CorruptRevision(durability));
    }

    // Only read files explicitly included in this verified immutable revision.
    // Never discover arbitrary filesystem content under the revisions directory.
    let mut block_files = BTreeMap::new();
    let revision_dir = themes_root
        .join("revisions")
        .join(&revision.revision_digest);
    for path in revision
        .file_digests
        .keys()
        .filter(|path| path.starts_with("blocks/"))
    {
        let clean = safe_relative_path(path)?;
        let bytes = fs::read(revision_dir.join(clean))
            .map_err(|_| ApiError::internal("Không đọc được block trong theme revision."))?;
        block_files.insert(path.clone(), bytes);
    }
    let blocks = crate::theme::collect_package_blocks(&block_files, &revision.theme_id)
        .map_err(ApiError::bad_request)?;

    // Validate functions.settings_schema compatibility (forbid switching secret <-> non-secret).
    if let Some(func) = &manifest.functions {
        let saved_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT option_key, value_json FROM theme_option_values
              WHERE owner_kind = 'theme_functions' AND owner_id = ?",
        )
        .bind(&revision.theme_id)
        .fetch_all(db)
        .await?;
        for (key, value_json) in saved_rows {
            let Some(field) = func.settings_schema.iter().find(|f| f.key == key) else {
                continue;
            };
            let is_stored_secret = serde_json::from_str::<serde_json::Value>(&value_json)
                .ok()
                .and_then(|v| {
                    v.as_object()
                        .and_then(|m| m.get("secret_ref"))
                        .and_then(serde_json::Value::as_str)
                        .map(|_| true)
                })
                .unwrap_or(false);
            let wants_secret =
                field.field_type == opendash_core::plugin::PluginSettingFieldType::Secret;
            if is_stored_secret != wants_secret {
                return Err(ApiError::conflict(
                    "Schema option của revision mới không tương thích với giá trị đã lưu (409).",
                ));
            }
        }
    }
    let theme_action_ids: Vec<String> = manifest
        .functions
        .as_ref()
        .map(|f| f.actions.iter().map(|a| a.id.clone()).collect())
        .unwrap_or_default();

    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let block_update = crate::storage::page::sync_owner_blocks_in_tx(
        &mut conn,
        "theme",
        &revision.theme_id,
        ProviderStatus::Active,
        &revision.revision_digest,
        &blocks,
    )
    .await;
    if let Err(err) = block_update {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(err);
    }
    if let Err(err) = crate::storage::content::sync_theme_declarations_in_tx(
        &mut conn,
        &revision.theme_id,
        &declarations,
    )
    .await
    {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(err);
    }
    let extension_update = async {
        crate::storage::admin_extensions::require_capacity_in_conn(
            &mut conn,
            "theme",
            &revision.theme_id,
            extension_decls.as_ref(),
        )
        .await?;
        crate::storage::admin_extensions::sync_owner_in_conn(
            &mut conn,
            "theme",
            &revision.theme_id,
            extension_decls.as_ref(),
            "active",
            &theme_action_ids,
        )
        .await?;
        crate::storage::admin_extensions::set_theme_active_in_conn(
            &mut conn,
            Some(&revision.theme_id),
        )
        .await
    }
    .await;
    if let Err(err) = extension_update {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(err);
    }
    let res = sqlx::query(
        "UPDATE theme_active_selection
         SET revision_id = ?, version = version + 1
         WHERE id = 1 AND version = ?",
    )
    .bind(revision_id)
    .bind(version)
    .execute(&mut *conn)
    .await;

    match res {
        Ok(done) if done.rows_affected() == 1 => {
            if let Err(err) =
                sqlx::query("DELETE FROM plugin_entry_projections WHERE owner_kind = 'theme'")
                    .execute(&mut *conn)
                    .await
            {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(err.into());
            }
            // The effective CPT body visibility and active theme selection must
            // already reflect the new revision when projecting public media.
            // Rebuild after every declaration update, still before COMMIT.
            if let Err(err) = crate::storage::page::resync_plugin_block_acls_in_tx(&mut conn).await
            {
                let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                return Err(err);
            }
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(RevisionActivationOutcome::Activated(
                get_active_selection(db).await?,
            ))
        }
        Ok(_) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Ok(RevisionActivationOutcome::Stale)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err.into())
        }
    }
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
    pub locales: BTreeMap<String, BTreeMap<String, String>>,
}

impl ActiveThemeV3 {
    pub fn asset_url_map(&self) -> BTreeMap<String, String> {
        self.manifest
            .assets
            .iter()
            .map(|asset| {
                (
                    asset.clone(),
                    format!("/theme/{}/{}", self.revision_digest, asset),
                )
            })
            .collect()
    }

    /// Per-key merges theme dictionaries (`default_locale -> primary_subtag -> exact code`)
    /// so missing keys in secondary dictionaries never trigger strict UndefinedError.
    pub fn merged_translations(&self, locale_code: &str) -> BTreeMap<String, String> {
        let mut merged = BTreeMap::new();
        if let Some(default_locale) = self.manifest.default_locale.as_deref() {
            let def_code = default_locale.to_ascii_lowercase();
            if let Some(dict) = self.locales.get(&def_code) {
                for (k, v) in dict {
                    merged.insert(k.clone(), v.clone());
                }
            }
        }
        let target_code = locale_code.to_ascii_lowercase();
        if let Some((primary, _)) = target_code.split_once('-') {
            if let Some(dict) = self.locales.get(primary) {
                for (k, v) in dict {
                    merged.insert(k.clone(), v.clone());
                }
            }
        }
        if let Some(dict) = self.locales.get(&target_code) {
            for (k, v) in dict {
                merged.insert(k.clone(), v.clone());
            }
        }
        merged
    }
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

fn validate_owner_template_selection_key(
    owner_kind: &str,
    owner_id: &str,
    role: &str,
    scope_type_key: Option<&str>,
) -> ApiResult<()> {
    if !matches!(owner_kind, "theme" | "plugin")
        || !matches!(
            role,
            "page" | "header" | "footer" | "archive" | "single" | "taxonomy_archive"
        )
        || scope_type_key.is_some_and(|key| !opendash_core::id::valid_snake_key(key))
    {
        return Err(ApiError::bad_request(
            "Định danh template selection không hợp lệ.",
        ));
    }
    validate_package_id(owner_id).map_err(ApiError::bad_request)
}

pub async fn upsert_owner_template_selection_in_conn(
    conn: &mut sqlx::SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    role: &str,
    scope_type_key: Option<&str>,
    template_selection_key: Option<&str>,
    ast_json: &str,
) -> ApiResult<OwnerTemplateSelection> {
    validate_owner_template_selection_key(owner_kind, owner_id, role, scope_type_key)?;
    if template_selection_key.is_some_and(|key| {
        !opendash_core::id::valid_kebab_segment_with_limit(key, 64)
            && !opendash_core::id::valid_snake_key(key)
    }) {
        return Err(ApiError::bad_request(
            "Tên template selection không hợp lệ.",
        ));
    }
    let _: opendash_core::builder::BuilderDocument = serde_json::from_str(ast_json)
        .map_err(|_| ApiError::bad_request("AST template selection không hợp lệ."))?;
    sqlx::query(
        "INSERT INTO owner_template_selections
         (owner_kind, owner_id, role, scope_type_key, template_selection_key, ast_json, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, datetime('now'))
         ON CONFLICT(owner_kind, owner_id, role, scope_type_key) DO UPDATE SET
           template_selection_key = excluded.template_selection_key,
           ast_json = excluded.ast_json,
           version = owner_template_selections.version + 1,
           updated_at = excluded.updated_at",
    )
    .bind(owner_kind)
    .bind(owner_id)
    .bind(role)
    .bind(scope_type_key.unwrap_or(""))
    .bind(template_selection_key)
    .bind(ast_json)
    .execute(&mut *conn)
    .await?;
    get_owner_template_selection_in_conn(conn, owner_kind, owner_id, role, scope_type_key)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được template selection vừa lưu."))
}

pub async fn get_owner_template_selection_in_conn(
    conn: &mut sqlx::SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    role: &str,
    scope_type_key: Option<&str>,
) -> ApiResult<Option<OwnerTemplateSelection>> {
    validate_owner_template_selection_key(owner_kind, owner_id, role, scope_type_key)?;
    let row: Option<(String, String, String, String, Option<String>, String, i64)> =
        sqlx::query_as(
            "SELECT owner_kind, owner_id, role, scope_type_key, template_selection_key, ast_json, version
               FROM owner_template_selections
              WHERE owner_kind = ? AND owner_id = ? AND role = ? AND scope_type_key = ?",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .bind(role)
        .bind(scope_type_key.unwrap_or(""))
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row.map(
        |(owner_kind, owner_id, role, scope, template_selection_key, ast_json, version)| {
            OwnerTemplateSelection {
                owner_kind,
                owner_id,
                role,
                scope_type_key: (!scope.is_empty()).then_some(scope),
                template_selection_key,
                ast_json,
                version,
            }
        },
    ))
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

pub async fn list_local_theme_templates(
    db: &SqlitePool,
    themes_root: &Path,
) -> ApiResult<LocalThemeCatalog> {
    let selection = get_active_selection(db).await?;
    if selection.revision_id.is_none() {
        return Ok(LocalThemeCatalog {
            active_theme_id: None,
            theme_state: "no_active_theme",
            theme_templates: vec![],
        });
    }
    if selection.source_format != Some(ThemeSourceFormat::V3) {
        return Ok(LocalThemeCatalog {
            active_theme_id: selection.theme_id,
            theme_state: "unsupported_theme_format",
            theme_templates: vec![],
        });
    }
    // This accessor verifies the active manifest, file digests and complete
    // marker before enumerating role keys. A withdrawn/corrupt provider yields
    // a deterministic fallback, never stale or remotely fetched templates.
    let Some(active) = load_active_theme_v3(db, themes_root).await? else {
        return Ok(LocalThemeCatalog {
            active_theme_id: selection.theme_id,
            theme_state: "active_theme_unavailable",
            theme_templates: vec![],
        });
    };
    let theme_id = active.manifest.theme_id.clone();
    let mut theme_templates = Vec::new();
    let targets = active
        .templates
        .keys()
        .cloned()
        .chain(active.parts.keys().map(|key| format!("part:{key}")))
        .chain(
            active
                .page_layouts
                .keys()
                .map(|key| format!("page_layout:{key}")),
        );
    for role in targets {
        let reason = Some("Quản lý template qua typed target và Template Overrides.");
        theme_templates.push(LocalThemeTemplateDescriptor {
            role,
            theme_id: theme_id.clone(),
            revision_id: active.revision_id.clone(),
            revision_digest: active.revision_digest.clone(),
            can_apply: false,
            reason,
        });
    }
    Ok(LocalThemeCatalog {
        active_theme_id: Some(theme_id),
        theme_state: "active",
        theme_templates,
    })
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

pub async fn load_active_theme_functions_snapshot(
    db: &SqlitePool,
) -> ApiResult<Option<ActiveThemeFunctionsSnapshot>> {
    let row: Option<(String, String, String, i64, String, String, String)> = sqlx::query_as(
        "SELECT r.theme_id, r.revision_id, r.revision_digest, a.version,
                r.source_format, r.manifest_json, r.file_digests_json
           FROM theme_active_selection a
           JOIN theme_revisions r ON r.revision_id = a.revision_id
          WHERE a.id = 1",
    )
    .fetch_optional(db)
    .await?;
    let Some((
        theme_id,
        revision_id,
        revision_digest,
        selection_version,
        source_format,
        manifest_json,
        file_digests_json,
    )) = row
    else {
        return Ok(None);
    };
    if ThemeSourceFormat::from_db(&source_format)? != ThemeSourceFormat::V3 {
        return Ok(None);
    }
    let file_digests: BTreeMap<String, String> = serde_json::from_str(&file_digests_json)?;
    Ok(Some(ActiveThemeFunctionsSnapshot {
        theme_id,
        revision_id,
        revision_digest,
        selection_version,
        manifest_json,
        file_digests,
    }))
}

pub async fn load_active_theme_v3(
    db: &SqlitePool,
    themes_root: &Path,
) -> ApiResult<Option<ActiveThemeV3>> {
    let mut conn = db.acquire().await?;
    load_active_theme_v3_in_conn(&mut conn, themes_root).await
}

/// Capture the active manifest and verified disk revision once. Preview loads
/// only the requested part sources from this snapshot.
pub(super) struct CapturedThemeV3 {
    pub manifest: ThemeV3Manifest,
    revision: ThemeRevisionRecord,
    rev_dir: std::path::PathBuf,
    dev_overlay: Option<crate::dev_mode::DevThemeSnapshot>,
}

impl CapturedThemeV3 {
    pub(super) fn part(&self, key: &str) -> ApiResult<Option<String>> {
        if let Some(ref snap) = self.dev_overlay {
            return Ok(snap.parts.get(key).cloned());
        }
        let Some(file) = self.manifest.parts.get(key) else {
            return Ok(None);
        };
        let bytes = fs::read(self.rev_dir.join(file))
            .map_err(|_| ApiError::bad_request("Preview theme part unavailable."))?;
        if self.revision.file_digests.get(file) != Some(&sha256_hex(&bytes)) {
            return Err(ApiError::bad_request(
                "Preview theme part failed integrity check.",
            ));
        }
        Ok(Some(String::from_utf8(bytes).map_err(|_| {
            ApiError::bad_request("Invalid theme part UTF-8.")
        })?))
    }
}

pub(super) async fn capture_active_theme_v3_in_conn(
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

pub(super) async fn load_active_theme_v3_in_conn(
    conn: &mut sqlx::SqliteConnection,
    themes_root: &Path,
) -> ApiResult<Option<ActiveThemeV3>> {
    let Some(CapturedThemeV3 {
        manifest,
        revision,
        rev_dir,
        dev_overlay,
    }) = capture_active_theme_v3_in_conn(conn, themes_root).await?
    else {
        return Ok(None);
    };
    if let Some(snap) = dev_overlay {
        return Ok(Some(ActiveThemeV3 {
            revision_id: revision.revision_id,
            revision_digest: snap.revision_digest,
            manifest: snap.manifest,
            templates: snap.templates,
            parts: snap.parts,
            page_layouts: snap.page_layouts,
            locales: snap.locales,
        }));
    }
    let mut templates = BTreeMap::new();
    for declaration in &manifest.templates {
        let Ok(key) = declaration.canonical_key() else {
            return Ok(None);
        };
        let Ok(source) = std::fs::read_to_string(rev_dir.join(&declaration.file)) else {
            return Ok(None);
        };
        templates.insert(key, source);
    }
    let mut parts = BTreeMap::new();
    for (key, file) in &manifest.parts {
        let Ok(source) = std::fs::read_to_string(rev_dir.join(file)) else {
            return Ok(None);
        };
        parts.insert(key.clone(), source);
    }
    let mut page_layouts = BTreeMap::new();
    for layout in &manifest.page_layouts {
        let Ok(source) = std::fs::read_to_string(rev_dir.join(&layout.file)) else {
            return Ok(None);
        };
        page_layouts.insert(layout.key.clone(), source);
    }
    let mut locales = BTreeMap::new();
    for rel_path in revision.file_digests.keys() {
        if let Some(rest) = rel_path.strip_prefix("locales/") {
            if let Some(code) = rest.strip_suffix(".json") {
                let Ok(bytes) = std::fs::read(rev_dir.join(rel_path)) else {
                    return Ok(None);
                };
                let Ok(dict) = crate::theme::parse_theme_locale_json(&bytes) else {
                    return Ok(None);
                };
                locales.insert(code.to_owned(), dict);
            }
        }
    }
    Ok(Some(ActiveThemeV3 {
        revision_id: revision.revision_id,
        revision_digest: revision.revision_digest,
        manifest,
        templates,
        parts,
        page_layouts,
        locales,
    }))
}

/// Synchronizes a mounted `--dev-theme` manifest declarations, blocks, and admin extensions
/// into SQLite without polluting immutable `theme_revisions`.
pub async fn sync_dev_theme_in_db(
    db: &SqlitePool,
    snap: &crate::dev_mode::DevThemeSnapshot,
) -> ApiResult<()> {
    let declarations = crate::theme::parse_theme_declarations(&snap.manifest_json)
        .map_err(|msg| ApiError(axum::http::StatusCode::UNPROCESSABLE_ENTITY, msg.into()))?;
    let theme_action_ids: Vec<String> = snap
        .manifest
        .functions
        .as_ref()
        .map(|f| f.actions.iter().map(|a| a.id.clone()).collect())
        .unwrap_or_default();

    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;

    let work: ApiResult<()> = async {
        crate::storage::page::sync_owner_blocks_in_tx(
            &mut conn,
            "theme",
            &snap.manifest.theme_id,
            ProviderStatus::Active,
            &snap.revision_digest,
            &snap.blocks,
        )
        .await?;
        crate::storage::content::sync_theme_declarations_in_tx(
            &mut conn,
            &snap.manifest.theme_id,
            &declarations,
        )
        .await?;
        crate::storage::admin_extensions::require_capacity_in_conn(
            &mut conn,
            "theme",
            &snap.manifest.theme_id,
            snap.admin_extensions.as_ref(),
        )
        .await?;
        crate::storage::admin_extensions::sync_owner_in_conn(
            &mut conn,
            "theme",
            &snap.manifest.theme_id,
            snap.admin_extensions.as_ref(),
            "active",
            &theme_action_ids,
        )
        .await?;
        crate::storage::admin_extensions::set_theme_active_in_conn(
            &mut conn,
            Some(&snap.manifest.theme_id),
        )
        .await?;
        sqlx::query("DELETE FROM plugin_entry_projections WHERE owner_kind = 'theme'")
            .execute(&mut *conn)
            .await?;
        crate::storage::page::resync_plugin_block_acls_in_tx(&mut conn).await?;
        Ok(())
    }
    .await;

    if let Err(err) = work {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(err);
    }
    sqlx::query("COMMIT").execute(&mut *conn).await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
}

/// Source block ID is never overwritten or silently translated. NULL active
pub async fn list(db: &SqlitePool) -> ApiResult<ThemeList> {
    // All fields (including revision visibility and CAS selection) are one read
    // snapshot. A concurrent activate/install cannot interleave two GET fields.
    let mut tx = db.begin().await?;
    let (revision_id, theme_id, version, source_format): (
        Option<String>,
        Option<String>,
        i64,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT a.revision_id, r.theme_id, a.version, r.source_format
         FROM theme_active_selection a
         LEFT JOIN theme_revisions r ON r.revision_id = a.revision_id
         WHERE a.id = 1",
    )
    .fetch_one(&mut *tx)
    .await?;
    let selection = ActiveThemeSelection {
        revision_id,
        theme_id,
        version,
        source_format: source_format
            .as_deref()
            .map(ThemeSourceFormat::from_db)
            .transpose()?,
    };
    let raw_revisions: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT revision_id, theme_id, version, revision_digest, source_format
         FROM theme_revisions ORDER BY theme_id ASC, version ASC, revision_id ASC",
    )
    .fetch_all(&mut *tx)
    .await?;
    let revisions = raw_revisions
        .into_iter()
        .map(
            |(revision_id, theme_id, version, revision_digest, source_format)| {
                Ok(ThemeRevisionSummary {
                    revision_id,
                    theme_id,
                    version,
                    revision_digest,
                    source_format: ThemeSourceFormat::from_db(&source_format)?,
                })
            },
        )
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(ThemeList {
        version,
        revisions,
        selection,
    })
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod theme_tests;
