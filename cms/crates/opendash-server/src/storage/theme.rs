//! M3/M5 theme package, immutable revision, and singleton active selection storage.
//! Never mutate `home_page` when activating or installing a theme.
#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::{
    error::{ApiError, ApiResult},
    theme::{Block, Fields},
};

const RESERVED_LEGACY_PREFIX: &str = "legacy-v1-";
const COMPLETE_MARKER: &str = ".complete";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeSourceFormat {
    V1Legacy,
    V2,
}

impl ThemeSourceFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1Legacy => "v1_legacy",
            Self::V2 => "v2",
        }
    }

    fn from_db(raw: &str) -> ApiResult<Self> {
        match raw {
            "v1_legacy" => Ok(Self::V1Legacy),
            "v2" => Ok(Self::V2),
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
    pub legacy_block_id: Option<String>,
    pub provenance_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveThemeSelection {
    pub revision_id: Option<String>,
    pub theme_id: Option<String>,
    pub version: i64,
    pub legacy_block_id: Option<String>,
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

#[derive(Debug, Serialize)]
pub struct ThemeItem {
    pub id: String,
    pub compatible_block_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ThemeList {
    pub items: Vec<ThemeItem>,
    pub active_id: Option<String>,
    pub version: i64,
    /// Null means no saved homepage or no explicit activation.
    pub homepage_compatible: Option<bool>,
}

pub enum ActivationOutcome {
    Activated(ThemeList),
    NotFound,
    Stale,
}

#[derive(Debug)]
pub struct SelectedTemplate {
    pub template: Option<String>,
    pub visibility: &'static str,
}

#[derive(Debug)]
pub struct HomeSnapshot {
    pub block: Block,
    pub original_fields: Fields,
    pub selected: SelectedTemplate,
}

/// Validate a v2 `theme_id`: lowercase ASCII kebab-case (1..=64 chars),
/// never claiming the reserved `legacy-v1-` identity domain.
pub fn valid_v2_theme_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 64 || id.starts_with(RESERVED_LEGACY_PREFIX) {
        return false;
    }
    let bytes = id.as_bytes();
    if bytes.first() == Some(&b'-') || bytes.last() == Some(&b'-') {
        return false;
    }
    let mut prev_dash = false;
    for &b in bytes {
        match b {
            b'a'..=b'z' | b'0'..=b'9' => prev_dash = false,
            b'-' => {
                if prev_dash {
                    return false;
                }
                prev_dash = true;
            }
            _ => return false,
        }
    }
    true
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
        let entry = entry
            .map_err(|_| ApiError::internal("Không thể duyệt thư mục themes/.staging."))?;
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)
                .map_err(|_| ApiError::internal("Không thể dọn staging cũ trong themes/.staging."))?;
        } else {
            fs::remove_file(&path)
                .map_err(|_| ApiError::internal("Không thể dọn file staging cũ trong themes/.staging."))?;
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
    let rev_dir = themes_root.join("revisions").join(&revision.revision_digest);
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

fn dir_matches_revision(rev_dir: &Path, digest: &str, file_digests: &BTreeMap<String, String>) -> bool {
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
    Option<String>,
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
        legacy_block_id,
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
        legacy_block_id,
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
                manifest_json, file_digests_json, legacy_block_id, provenance_json, created_at
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
                manifest_json, file_digests_json, legacy_block_id, provenance_json, created_at
         FROM theme_revisions
         WHERE revision_digest = ?",
    )
    .bind(digest)
    .fetch_optional(conn)
    .await?;
    row.map(row_to_revision).transpose()
}

/// Stage validated v2 theme files durably under `themes/.staging/<run_uuid>/`,
/// atomically publish to `themes/revisions/<revision_digest>/` with `.complete` marker,
/// and commit metadata to SQLite (`BEGIN IMMEDIATE`) without activating it.
#[allow(clippy::too_many_arguments)]
pub async fn stage_and_commit_v2_revision(
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
    if !valid_v2_theme_id(theme_id) {
        return Err(ApiError::bad_request(
            "theme_id v2 phải là lowercase kebab-case (1–64 ký tự) và không dùng tiền tố legacy.",
        ));
    }
    if version.trim().is_empty() || version.len() > 64 {
        return Err(ApiError::bad_request("Phiên bản theme không hợp lệ."));
    }
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
        if let Some(existing) = find_revision_by_digest(&mut conn, &revision_digest).await? {
            if existing.theme_id == theme_id && existing.version == version {
                return Ok(RevisionInstallOutcome::AlreadyExists(existing));
            }
            return Err(ApiError::conflict(
                "Digest revision đã tồn tại cho một theme/version khác.",
            ));
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
            return Err(ApiError::internal("Không thể ghi marker .complete cho revision."));
        }
    }

    // Commit SQLite metadata only after durable publication succeeds.
    let revision_id = format!("rev-{revision_digest}");
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;

    let tx_res: ApiResult<RevisionInstallOutcome> = async {
        if let Some(existing) = find_revision_by_digest(&mut conn, &revision_digest).await? {
            if existing.theme_id == theme_id && existing.version == version {
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
             VALUES (?, ?, 'v2', 0)
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
             ) VALUES (?, ?, ?, ?, 'v2', ?, ?, NULL, ?)",
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
            Ok(outcome)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

pub async fn get_active_selection(db: &SqlitePool) -> ApiResult<ActiveThemeSelection> {
    let row: (Option<String>, Option<String>, i64, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT a.revision_id, r.theme_id, a.version, r.legacy_block_id, r.source_format
             FROM theme_active_selection a
             LEFT JOIN theme_revisions r ON r.revision_id = a.revision_id
             WHERE a.id = 1",
        )
        .fetch_one(db)
        .await?;
    let source_format = row
        .4
        .as_deref()
        .map(ThemeSourceFormat::from_db)
        .transpose()?;
    Ok(ActiveThemeSelection {
        revision_id: row.0,
        theme_id: row.1,
        version: row.2,
        legacy_block_id: row.3,
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
        .map(|(theme_id, display_name, source_format, is_legacy, created_at)| {
            Ok(ThemePackageRecord {
                theme_id,
                display_name,
                source_format: ThemeSourceFormat::from_db(&source_format)?,
                is_legacy: is_legacy != 0,
                created_at,
            })
        })
        .collect()
}

pub async fn list_revisions(
    db: &SqlitePool,
    theme_id: &str,
) -> ApiResult<Vec<ThemeRevisionRecord>> {
    let rows: Vec<RevisionRow> = sqlx::query_as(
        "SELECT revision_id, theme_id, version, revision_digest, source_format,
                manifest_json, file_digests_json, legacy_block_id, provenance_json, created_at
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
                manifest_json, file_digests_json, legacy_block_id, provenance_json, created_at
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
    let declarations = if revision.source_format == ThemeSourceFormat::V2 {
        crate::theme::parse_theme_declarations(&revision.manifest_json)
            .map_err(|msg| ApiError(axum::http::StatusCode::UNPROCESSABLE_ENTITY, msg.into()))?
    } else {
        crate::theme::ThemeDeclarations::default()
    };
    let durability = verify_revision_durability(themes_root, &revision);
    if durability != RevisionDurabilityStatus::Valid {
        return Ok(RevisionActivationOutcome::CorruptRevision(durability));
    }

    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    if let Err(err) =
        crate::storage::content::sync_theme_declarations_in_tx(&mut conn, &revision.theme_id, &declarations)
            .await
    {
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
            sqlx::query("COMMIT").execute(&mut *conn).await?;
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

#[derive(Clone, Debug)]
pub struct ActiveV2RoleTemplates {
    pub revision_digest: String,
    pub roles: BTreeMap<String, String>,
}

pub async fn load_active_v2_role_templates(
    db: &SqlitePool,
    themes_root: &Path,
) -> ApiResult<Option<ActiveV2RoleTemplates>> {
    let selection = get_active_selection(db).await?;
    let Some(revision_id) = selection.revision_id else {
        return Ok(None);
    };
    let row: Option<RevisionRow> = sqlx::query_as(
        "SELECT revision_id, theme_id, version, revision_digest, source_format,
                manifest_json, file_digests_json, legacy_block_id, provenance_json, created_at
         FROM theme_revisions
         WHERE revision_id = ?",
    )
    .bind(revision_id)
    .fetch_optional(db)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let revision = row_to_revision(row)?;
    if revision.source_format != ThemeSourceFormat::V2 {
        return Ok(None);
    }
    if verify_revision_durability(themes_root, &revision) != RevisionDurabilityStatus::Valid {
        return Ok(None);
    }
    let Ok(manifest) = serde_json::from_str::<crate::theme::ThemeV2Manifest>(&revision.manifest_json) else {
        return Ok(None);
    };
    let rev_dir = themes_root.join("revisions").join(&revision.revision_digest);
    let mut roles = BTreeMap::new();
    for (role, rel_path) in manifest.templates {
        let Ok(safe_rel) = crate::package::validate_v2_relative_path(&rel_path) else {
            continue;
        };
        let full_path = rev_dir.join(&safe_rel);
        if let Ok(content) = std::fs::read_to_string(&full_path) {
            roles.insert(role, content);
        }
    }
    Ok(Some(ActiveV2RoleTemplates {
        revision_digest: revision.revision_digest,
        roles,
    }))
}

/// Source block ID is never overwritten or silently translated. NULL active
/// means the original legacy template. Explicit unsupported means blank page.
pub fn select_template(
    source_id: &str,
    source_template: String,
    active_id: Option<&str>,
    active_alias: Option<&str>,
    active_template: Option<String>,
) -> SelectedTemplate {
    match active_id {
        None => SelectedTemplate {
            template: Some(source_template),
            visibility: "rendered",
        },
        Some(id) if source_id == id || Some(source_id) == active_alias => SelectedTemplate {
            template: active_template,
            visibility: "rendered",
        },
        Some(_) => SelectedTemplate {
            template: None,
            visibility: "unsupported",
        },
    }
}

/// Save and render using one SQLite write snapshot. The renderer executes
/// before commit, so a template error cannot leave a successful write behind.
/// No hook is invoked here; callers dispatch only after this transaction commits.
pub async fn save_home_page_and_render<T, F>(
    db: &SqlitePool,
    block: &Block,
    source_template: String,
    render: F,
) -> ApiResult<T>
where
    F: FnOnce(SelectedTemplate) -> ApiResult<T>,
{
    let mut tx = db.begin().await?;
    // Builder conversion is one-way. Reserve the same singleton writer used
    // by layout Save *before* touching M3 home_page; a legacy request racing
    // with conversion can never revive the old public source.
    let changed = sqlx::query(
        "UPDATE builder_homepage SET version = version + 1
         WHERE id = 1 AND document_json IS NULL AND version < 9223372036854775807",
    )
    .execute(&mut *tx)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "Homepage đã chuyển sang Builder dạng cây. Hãy mở /api/admin/builder/homepage.",
        ));
    }
    sqlx::query(
        "INSERT INTO home_page (id, block_id, values_json) VALUES (1, ?, ?)
         ON CONFLICT(id) DO UPDATE SET block_id = excluded.block_id, values_json = excluded.values_json",
    )
    .bind(&block.kind)
    .bind(serde_json::to_string(&block.values)?)
    .execute(&mut *tx)
    .await?;
    let (active_id, alias, active_template): (Option<String>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT r.legacy_block_id, t.compatible_block_id, t.template
             FROM theme_active_selection a
             LEFT JOIN theme_revisions r ON r.revision_id = a.revision_id
             LEFT JOIN theme_blocks t ON t.id = r.legacy_block_id
             WHERE a.id = 1",
        )
        .fetch_one(&mut *tx)
        .await?;
    let selected = select_template(
        &block.kind,
        source_template,
        active_id.as_deref(),
        alias.as_deref(),
        active_template,
    );
    let output = render(selected)?;
    tx.commit().await?;
    Ok(output)
}

pub async fn active_selection(
    db: &SqlitePool,
) -> ApiResult<(Option<String>, Option<String>, Option<String>)> {
    let selection: (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT r.legacy_block_id, t.compatible_block_id, t.template
         FROM theme_active_selection a
         LEFT JOIN theme_revisions r ON r.revision_id = a.revision_id
         LEFT JOIN theme_blocks t ON t.id = r.legacy_block_id
         WHERE a.id = 1",
    )
    .fetch_one(db)
    .await?;
    Ok(selection)
}

/// Atomic, single-query view of the saved homepage and current active theme.
/// Values and fields are never projected through a different theme's schema.
pub async fn public_home(db: &SqlitePool) -> ApiResult<Option<HomeSnapshot>> {
    let row: Option<(
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT p.block_id, p.values_json, src.fields_json, src.template,
                r.legacy_block_id, active.compatible_block_id, active.template
         FROM home_page p
         JOIN theme_blocks src ON src.id = p.block_id
         JOIN theme_active_selection a ON a.id = 1
         LEFT JOIN theme_revisions r ON r.revision_id = a.revision_id
         LEFT JOIN theme_blocks active ON active.id = r.legacy_block_id
         WHERE p.id = 1",
    )
    .fetch_optional(db)
    .await?;
    row.map(
        |(kind, values, fields, original_template, active_id, alias, active_template)| {
            let selected = select_template(
                &kind,
                original_template,
                active_id.as_deref(),
                alias.as_deref(),
                active_template,
            );
            Ok(HomeSnapshot {
                block: Block {
                    kind,
                    values: serde_json::from_str(&values)?,
                },
                original_fields: serde_json::from_str(&fields)?,
                selected,
            })
        },
    )
    .transpose()
}

pub async fn list(db: &SqlitePool) -> ApiResult<ThemeList> {
    let (active_id, version): (Option<String>, i64) = sqlx::query_as(
        "SELECT r.legacy_block_id, a.version
         FROM theme_active_selection a
         LEFT JOIN theme_revisions r ON r.revision_id = a.revision_id
         WHERE a.id = 1",
    )
    .fetch_one(db)
    .await?;
    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, compatible_block_id FROM theme_blocks ORDER BY id")
            .fetch_all(db)
            .await?;
    // Once the Builder layout is saved, the old M3 home_page is retained
    // solely for backup/restore. Its single-block compatibility must not be
    // reported as compatibility of the new (possibly mixed) Builder tree.
    let saved: Option<String> = sqlx::query_scalar(
        "SELECT p.block_id FROM home_page p
         JOIN builder_homepage b ON b.id = 1 AND b.document_json IS NULL
         WHERE p.id = 1",
    )
    .fetch_optional(db)
    .await?;
    let homepage_compatible = match (saved.as_deref(), active_id.as_deref()) {
        (Some(source), Some(id)) => Some(
            rows.iter()
                .find(|(key, _)| key == id)
                .is_some_and(|(_, alias)| source == id || alias.as_deref() == Some(source)),
        ),
        _ => None,
    };
    Ok(ThemeList {
        items: rows
            .into_iter()
            .map(|(id, compatible_block_id)| ThemeItem {
                id,
                compatible_block_id,
            })
            .collect(),
        active_id,
        version,
        homepage_compatible,
    })
}

pub async fn activate(db: &SqlitePool, id: &str, version: i64) -> ApiResult<ActivationOutcome> {
    if !opendash_core::id::valid_namespaced_block_id(id) {
        return Err(ApiError::bad_request("ID theme không hợp lệ."));
    }
    let revision_id: Option<String> =
        sqlx::query_scalar("SELECT revision_id FROM theme_legacy_mappings WHERE block_id = ?")
            .bind(id)
            .fetch_optional(db)
            .await?;
    let Some(revision_id) = revision_id else {
        return Ok(ActivationOutcome::NotFound);
    };
    // Optimistic CAS: two tabs with same version cannot both activate.
    let result = sqlx::query(
        "UPDATE theme_active_selection
         SET revision_id = ?, version = version + 1
         WHERE id = 1 AND version = ?",
    )
    .bind(revision_id)
    .bind(version)
    .execute(db)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(ActivationOutcome::Stale);
    }
    Ok(ActivationOutcome::Activated(list(db).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compatibility_selection_never_uses_old_html_for_unsupported_blocks() {
        let legacy = select_template("a/banner", "A".into(), None, None, None);
        assert_eq!(legacy.template.as_deref(), Some("A"));
        let alias = select_template(
            "a/banner",
            "A".into(),
            Some("b/banner"),
            Some("a/banner"),
            Some("B".into()),
        );
        assert_eq!(alias.template.as_deref(), Some("B"));
        let absent = select_template(
            "a/banner",
            "A".into(),
            Some("c/banner"),
            None,
            Some("C".into()),
        );
        assert_eq!(absent.visibility, "unsupported");
        assert!(absent.template.is_none());
    }
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod theme_tests;
