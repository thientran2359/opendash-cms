use super::*;

impl ThemeSourceFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "v1",
        }
    }

    pub(in crate::storage) fn from_db(raw: &str) -> ApiResult<Self> {
        match raw {
            "v1" => Ok(Self::V1),
            _ => Err(ApiError::internal("Định dạng theme không hợp lệ trong DB.")),
        }
    }
}

pub(in crate::storage) fn safe_relative_path(raw: &str) -> ApiResult<PathBuf> {
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

pub(in crate::storage) fn dir_matches_revision(
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

fn sync_staged_file(file: &File) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        // On macOS, std::fs::File::sync_data() calls F_FULLFSYNC per file.
        // Flush each staged file with fsync(2); the revision's `.complete`
        // marker in `stage_and_commit_revision` still calls `sync_all()`
        // (`F_FULLFSYNC`) as the single durable barrier after rename.
        let rc = unsafe { libc::fsync(file.as_raw_fd()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        file.sync_data()
    }
}

pub(in crate::storage) fn write_files_durably(
    staging_dir: &Path,
    files: &BTreeMap<String, Vec<u8>>,
) -> ApiResult<()> {
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
            sync_staged_file(&f)
                .map_err(|_| ApiError::internal("Lỗi đồng bộ file theme xuống đĩa."))?;
            // File handle is dropped here before rename (required on Windows).
        }
    }
    Ok(())
}

pub(in crate::storage) type RevisionRow = (
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

pub(in crate::storage) fn row_to_revision(row: RevisionRow) -> ApiResult<ThemeRevisionRecord> {
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

/// Embed/install the OpenDash Starter only at exclusive server startup. A
/// repaired revision is reconstructed from the signed-in-binary file set;
/// existing site content and any explicitly selected theme are untouched.
pub(in crate::storage) fn repair_builtin_starter_revision_dir_if_needed(
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

pub(in crate::storage) fn validate_owner_template_selection_key(
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

impl CapturedTheme {
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
        crate::storage::search::sync_builder_entries_in_conn(&mut conn).await?;
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
