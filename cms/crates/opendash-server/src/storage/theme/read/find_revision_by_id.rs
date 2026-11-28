use super::*;

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

pub(in crate::storage) async fn find_revision_by_digest(
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

/// Stage validated V1 theme files durably under `themes/.staging/<run_uuid>/`,
/// atomically publish to `themes/revisions/<revision_digest>/` with `.complete` marker,
/// and commit metadata to SQLite (`BEGIN IMMEDIATE`) without activating it.
#[allow(clippy::too_many_arguments)]
pub async fn stage_and_commit_revision(
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
    let validated = crate::theme::validate_theme_files(files.clone())
        .map_err(|_| ApiError::bad_request("Theme v1 files hoặc manifest không hợp lệ."))?;
    if validated.manifest_json != manifest_json {
        return Err(ApiError::bad_request(
            "Theme v1 manifest_json không khớp file theme.json.",
        ));
    }
    let manifest = validated.manifest;
    if manifest.theme_id != theme_id || manifest.name != display_name || manifest.version != version
    {
        return Err(ApiError::bad_request(
            "Theme metadata không khớp manifest v1.",
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
        if format.as_deref().is_some_and(|f| f != "v1") {
            return Err(ApiError::conflict("Theme ID đang thuộc package format cũ."));
        }
        if let Some(existing) = find_revision_by_digest(&mut conn, &revision_digest).await? {
            if existing.theme_id == theme_id
                && existing.version == version
                && existing.source_format == ThemeSourceFormat::V1
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
        if format.as_deref().is_some_and(|f| f != "v1") {
            return Err(ApiError::conflict("Theme ID đang thuộc package format cũ."));
        }
        if let Some(existing) = find_revision_by_digest(&mut conn, &revision_digest).await? {
            if existing.theme_id == theme_id
                && existing.version == version
                && existing.source_format == ThemeSourceFormat::V1
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
            "INSERT INTO theme_packages (theme_id, display_name, source_format)
             VALUES (?, ?, 'v1')
             ON CONFLICT(theme_id) DO NOTHING",
        )
        .bind(theme_id)
        .bind(display_name)
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            "INSERT INTO theme_revisions (
                revision_id, theme_id, version, revision_digest, source_format,
                manifest_json, file_digests_json, provenance_json
             ) VALUES (?, ?, ?, ?, 'v1', ?, ?, ?)",
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
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT theme_id, display_name, source_format, created_at
         FROM theme_packages
         ORDER BY theme_id ASC",
    )
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(|(theme_id, display_name, source_format, created_at)| {
            Ok(ThemePackageRecord {
                theme_id,
                display_name,
                source_format: ThemeSourceFormat::from_db(&source_format)?,
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
