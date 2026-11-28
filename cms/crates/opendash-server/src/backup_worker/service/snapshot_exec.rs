use super::*;

pub(crate) fn stream_copy_and_hash(src_path: &Path, dst_path: &Path) -> ApiResult<(u64, String)> {
    ensure_no_reparse_or_symlink(src_path)?;
    let meta = fs::symlink_metadata(src_path)
        .map_err(|_| ApiError::conflict(format!("Thiếu tệp tham chiếu: {}", src_path.display())))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(ApiError::conflict(
            "Từ chối symlink hoặc đối tượng không phải tệp.",
        ));
    }

    let mut src = File::open(src_path)
        .map_err(|_| ApiError::conflict("Không thể mở tệp nguồn khi tạo snapshot."))?;
    let mut dst = create_private_file(dst_path)?;

    let mut buf = vec![0u8; STREAM_BUF_BYTES];
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;

    loop {
        let n = src
            .read(&mut buf)
            .map_err(|_| ApiError::internal("Lỗi đọc tệp khi tạo snapshot."))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        dst.write_all(&buf[..n])
            .map_err(|_| ApiError::internal("Lỗi ghi tệp snapshot (có thể đầy đĩa)."))?;
        total += n as u64;
    }

    dst.sync_all()
        .map_err(|_| ApiError::internal("Không thể fsync tệp snapshot."))?;
    Ok((total, hex_encode(&hasher.finalize())))
}

pub(crate) fn hash_existing_file_streaming(path: &Path) -> ApiResult<(u64, String)> {
    let mut f = File::open(path)
        .map_err(|_| ApiError::internal("Không mở được tệp DB snapshot để băm."))?;
    let mut buf = vec![0u8; STREAM_BUF_BYTES];
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|_| ApiError::internal("Lỗi đọc DB snapshot."))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    Ok((total, hex_encode(&hasher.finalize())))
}

pub(crate) async fn execute_snapshot_worker(req: &SnapshotWorkerRequest) -> ApiResult<PathBuf> {
    let paths = SitePaths::from_parts(req.site_root.clone(), req.db_path.clone());
    ensure_private_directory_tree(&req.snapshot_dir)?;

    let opts = SqliteConnectOptions::new()
        .filename(paths.db_path())
        .create_if_missing(false)
        .read_only(true);
    let db = sqlx::SqlitePool::connect_with(opts)
        .await
        .map_err(|_| ApiError::internal("Worker không mở được SQLite nguồn."))?;

    let inventory = collect_backup_inventory(&db).await?;
    db.close().await;

    let mut planned: Vec<PlannedCopy> = Vec::new();
    let mut known_media_ids: HashSet<String> = HashSet::new();

    for m in &inventory.media {
        known_media_ids.insert(m.id.clone());
        let shard = &m.id[..2];
        let orig_rel = format!("media/{shard}/{}.{}", m.id, m.extension);
        let orig_path = paths.uploads_dir().join(&orig_rel);
        planned.push(PlannedCopy {
            kind: BackupFileKind::MediaOriginal,
            logical_key: orig_rel,
            source_path: Some(orig_path),
            inline_bytes: None,
        });
        if m.kind == "image" {
            let thumb_rel = format!("media/{shard}/{}.thumb.png", m.id);
            let thumb_path = paths.uploads_dir().join(&thumb_rel);
            planned.push(PlannedCopy {
                kind: BackupFileKind::MediaThumbnail,
                logical_key: thumb_rel,
                source_path: Some(thumb_path),
                inline_bytes: None,
            });
        }
    }

    for q_job_id in &inventory.quarantine_job_ids {
        let q_dir = paths.uploads_dir().join(".quarantine").join(q_job_id);
        if q_dir.exists() {
            ensure_no_reparse_or_symlink(&q_dir)?;
            let rd = fs::read_dir(&q_dir)
                .map_err(|_| ApiError::conflict("Không đọc được thư mục quarantine."))?;
            for entry in rd {
                let entry = entry.map_err(|_| ApiError::conflict("Lỗi đọc tệp quarantine."))?;
                let fname = entry.file_name().to_string_lossy().to_string();
                let key = format!("quarantine/{q_job_id}/{fname}");
                planned.push(PlannedCopy {
                    kind: BackupFileKind::QuarantineObject,
                    logical_key: key,
                    source_path: Some(entry.path()),
                    inline_bytes: None,
                });
            }
        }
    }

    // Include canonical unreferenced media files under quarantine/unreferenced/
    let media_root = paths.uploads_dir().join("media");
    if media_root.exists() {
        ensure_no_reparse_or_symlink(&media_root)?;
        let shards = fs::read_dir(&media_root)
            .map_err(|_| ApiError::conflict("Không đọc được uploads/media."))?;
        for shard_ent in shards {
            let shard_ent =
                shard_ent.map_err(|_| ApiError::conflict("Lỗi đọc shard trong uploads/media."))?;
            let shard_name = shard_ent.file_name().to_string_lossy().to_string();
            let shard_path = shard_ent.path();
            ensure_no_reparse_or_symlink(&shard_path)?;
            if !shard_path.is_dir() || shard_name.len() != 2 {
                return Err(ApiError::conflict(
                    "Phát hiện đối tượng lạ trong uploads/media.",
                ));
            }
            let files = fs::read_dir(&shard_path)
                .map_err(|_| ApiError::conflict("Không đọc được shard uploads/media."))?;
            for f_ent in files {
                let f_ent =
                    f_ent.map_err(|_| ApiError::conflict("Lỗi đọc tệp trong uploads/media."))?;
                let fname = f_ent.file_name().to_string_lossy().to_string();
                let stem = fname
                    .strip_suffix(".thumb.png")
                    .or_else(|| fname.rsplit_once('.').map(|(s, _)| s))
                    .unwrap_or("");
                if !is_valid_sha256_hex(stem) || !stem.starts_with(&shard_name) {
                    return Err(ApiError::conflict(
                        "Phát hiện tệp không hợp lệ trong uploads/media; cần reconcile trước.",
                    ));
                }
                if !known_media_ids.contains(stem) {
                    planned.push(PlannedCopy {
                        kind: BackupFileKind::QuarantineObject,
                        logical_key: format!("quarantine/unreferenced/{fname}"),
                        source_path: Some(f_ent.path()),
                        inline_bytes: None,
                    });
                }
            }
        }
    }

    for rev in &inventory.theme_revisions {
        if rev.source_format == "v1" {
            let rev_dir = paths
                .themes_dir()
                .join("revisions")
                .join(&rev.revision_digest);
            if !rev_dir.exists() {
                return Err(ApiError::conflict(format!(
                    "Thiếu thư mục theme revision {}/{}.",
                    rev.theme_id, rev.revision_digest
                )));
            }
            collect_dir_files_recursive(
                &rev_dir,
                &format!("themes/revisions/{}", rev.revision_digest),
                BackupFileKind::ThemePackageFile,
                &mut planned,
            )?;
        }
    }

    for sref in &inventory.secret_refs {
        let json_path = paths.smtp_secrets_dir().join(format!("{sref}.json"));
        let bin_path = paths.smtp_secrets_dir().join(format!("{sref}.bin"));
        if json_path.exists() {
            planned.push(PlannedCopy {
                kind: BackupFileKind::SmtpSecret,
                logical_key: format!("secrets/smtp/{sref}.json"),
                source_path: Some(json_path),
                inline_bytes: None,
            });
        } else if bin_path.exists() {
            planned.push(PlannedCopy {
                kind: BackupFileKind::SmtpSecret,
                logical_key: format!("secrets/smtp/{sref}.bin"),
                source_path: Some(bin_path),
                inline_bytes: None,
            });
        } else {
            return Err(ApiError::conflict(
                "Thiếu tệp bí mật SMTP được tham chiếu; không thể tạo snapshot.",
            ));
        }
    }

    for ext in &inventory.extension_secret_refs {
        let sec_path = paths
            .secrets_dir()
            .join(&ext.owner_subdir)
            .join(&ext.package_id)
            .join(format!("{}.json", ext.secret_ref));
        if sec_path.exists() {
            planned.push(PlannedCopy {
                kind: BackupFileKind::ExtensionSecret,
                logical_key: format!(
                    "secrets/{}/{}/{}.json",
                    ext.owner_subdir, ext.package_id, ext.secret_ref
                ),
                source_path: Some(sec_path),
                inline_bytes: None,
            });
        } else {
            return Err(ApiError::conflict(
                "Thiếu tệp bí mật extension được tham chiếu; không thể tạo snapshot.",
            ));
        }
    }

    // Portable config/site.json
    let cfg_bytes = if paths.site_config_path().exists() {
        ensure_no_reparse_or_symlink(&paths.site_config_path())?;
        let raw = fs::read(&paths.site_config_path())
            .map_err(|_| ApiError::conflict("Không đọc được config/site.json."))?;
        let cfg: PortableRuntimeConfigV1 = serde_json::from_slice(&raw)
            .map_err(|_| ApiError::conflict("config/site.json không hợp lệ."))?;
        cfg.validate().map_err(ApiError::conflict)?;
        serde_json::to_vec_pretty(&cfg)
            .map_err(|_| ApiError::internal("Không thể tuần tự hóa config/site.json."))?
    } else {
        serde_json::to_vec_pretty(&PortableRuntimeConfigV1::default())
            .map_err(|_| ApiError::internal("Không thể tuần tự hóa config mặc định."))?
    };
    planned.push(PlannedCopy {
        kind: BackupFileKind::PortableConfig,
        logical_key: "config/site.json".to_string(),
        source_path: None,
        inline_bytes: Some(cfg_bytes),
    });

    collect_language_packs_for_snapshot(&paths, &mut planned)?;

    // Entry 1: DB snapshot via VACUUM INTO
    let db_entry_name = format_flat_entry_name(1).map_err(ApiError::internal)?;
    let db_dest = req.snapshot_dir.join(&db_entry_name);
    let (schema_version, migration_checksums) =
        vacuum_into_snapshot(paths.db_path(), &db_dest).await?;
    let (db_size, db_sha) = hash_existing_file_streaming(&db_dest)?;

    planned.sort_by(|a, b| a.logical_key.cmp(&b.logical_key));

    let mut records = Vec::with_capacity(planned.len() + 1);
    records.push(BackupFileRecord {
        entry_name: db_entry_name,
        kind: BackupFileKind::Db,
        logical_key: "db/main.sqlite".to_string(),
        size: db_size,
        sha256: db_sha,
    });

    for (idx, item) in planned.into_iter().enumerate() {
        validate_logical_key(item.kind, &item.logical_key).map_err(ApiError::conflict)?;
        let entry_name = format_flat_entry_name(idx + 2).map_err(ApiError::conflict)?;
        let dst_path = req.snapshot_dir.join(&entry_name);

        let (size, sha256) = if let Some(src_path) = item.source_path {
            stream_copy_and_hash(&src_path, &dst_path)?
        } else if let Some(bytes) = item.inline_bytes {
            let mut dst = create_private_file(&dst_path)?;
            dst.write_all(&bytes)
                .map_err(|_| ApiError::internal("Lỗi ghi config snapshot."))?;
            dst.sync_all()
                .map_err(|_| ApiError::internal("Lỗi fsync config snapshot."))?;
            let digest = Sha256::digest(&bytes);
            (bytes.len() as u64, hex_encode(&digest))
        } else {
            return Err(ApiError::internal("Mục snapshot không có nguồn dữ liệu."));
        };

        records.push(BackupFileRecord {
            entry_name,
            kind: item.kind,
            logical_key: item.logical_key,
            size,
            sha256,
        });
    }

    records.sort_by(|a, b| a.logical_key.cmp(&b.logical_key));

    let manifest = BackupManifestV1 {
        format_version: BACKUP_FORMAT_VERSION,
        app_version: req.app_version.clone(),
        source_sha: req.source_sha.clone(),
        schema_version,
        migration_checksums,
        created_at: req.created_at.clone(),
        files: records,
        restore_policy_version: RESTORE_POLICY_VERSION,
    };

    validate_manifest_v1(&manifest).map_err(ApiError::conflict)?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|_| ApiError::internal("Không thể tuần tự hóa manifest.json."))?;
    if manifest_bytes.len() > MAX_BACKUP_MANIFEST_BYTES {
        return Err(ApiError::conflict("manifest.json vượt giới hạn 8 MiB."));
    }

    let manifest_path = req.snapshot_dir.join("manifest.json");
    let mut mf = create_private_file(&manifest_path)?;
    mf.write_all(&manifest_bytes)
        .map_err(|_| ApiError::internal("Lỗi ghi manifest.json."))?;
    mf.sync_all()
        .map_err(|_| ApiError::internal("Lỗi fsync manifest.json."))?;

    Ok(manifest_path)
}

pub(crate) fn collect_dir_files_recursive(
    dir: &Path,
    prefix_key: &str,
    kind: BackupFileKind,
    out: &mut Vec<PlannedCopy>,
) -> ApiResult<()> {
    ensure_no_reparse_or_symlink(dir)?;
    let rd = fs::read_dir(dir).map_err(|_| ApiError::conflict("Không đọc được thư mục gói."))?;
    for entry in rd {
        let entry = entry.map_err(|_| ApiError::conflict("Lỗi đọc mục trong thư mục gói."))?;
        let fname = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        ensure_no_reparse_or_symlink(&path)?;
        let key = format!("{prefix_key}/{fname}");
        if path.is_dir() {
            collect_dir_files_recursive(&path, &key, kind, out)?;
        } else {
            out.push(PlannedCopy {
                kind,
                logical_key: key,
                source_path: Some(path),
                inline_bytes: None,
            });
        }
    }
    Ok(())
}

fn collect_language_packs_for_snapshot(
    paths: &SitePaths,
    out: &mut Vec<PlannedCopy>,
) -> ApiResult<()> {
    let lang_root = paths.languages_dir();
    if !lang_root.exists() {
        return Ok(());
    }
    ensure_no_reparse_or_symlink(&lang_root)?;
    let mut count = 0usize;
    for scope in ["core", "themes", "plugins"] {
        let dir = lang_root.join(scope);
        if !dir.exists() {
            continue;
        }
        ensure_no_reparse_or_symlink(&dir)?;
        let rd = fs::read_dir(&dir).map_err(|_| {
            ApiError::conflict(String::from(
                "Không đọc được thư mục languages/ để snapshot.",
            ))
        })?;
        for entry in rd {
            let entry = entry.map_err(|_| {
                ApiError::conflict(String::from("Lỗi đọc mục trong thư mục languages/."))
            })?;
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with(".tmp-") {
                continue;
            }
            let path = entry.path();
            ensure_no_reparse_or_symlink(&path)?;
            if !path.is_file() || !(fname.ends_with(".po") || fname.ends_with(".mo")) {
                continue;
            }
            count += 1;
            if count > opendash_core::l10n::MAX_SITE_LANGUAGE_FILES {
                return Err(ApiError::conflict(String::from(
                    "Số lượng tệp trong languages/ vượt quá giới hạn 128.",
                )));
            }
            out.push(PlannedCopy {
                kind: BackupFileKind::LanguagePack,
                logical_key: format!("languages/{scope}/{fname}"),
                source_path: Some(path),
                inline_bytes: None,
            });
        }
    }
    Ok(())
}
