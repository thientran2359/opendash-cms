use super::*;

pub(crate) async fn check_no_symlink_and_count(
    path: &Path,
    scanned_files: &mut usize,
) -> ApiResult<std::fs::Metadata> {
    let meta = fs::symlink_metadata(path)
        .await
        .map_err(|_| ApiError::conflict("Không đọc được metadata trong cây dữ liệu."))?;
    if meta.file_type().is_symlink() {
        return Err(ApiError::conflict(
            "Phát hiện symlink trong cây dữ liệu; dừng đối soát.",
        ));
    }
    *scanned_files += 1;
    if *scanned_files > MAX_RECONCILE_FILES {
        return Err(ApiError::conflict(
            "Số lượng tệp vượt giới hạn reconcile (tối đa 20.000 tệp).",
        ));
    }
    Ok(meta)
}

pub(crate) async fn scan_uploads_tree(
    uploads_root: &Path,
    scanned_files: &mut usize,
) -> ApiResult<(Vec<ScannedFile>, Vec<(String, PathBuf, u64)>)> {
    let mut media_files = Vec::new();
    let mut quarantine_dirs = Vec::new();

    if !fs::try_exists(uploads_root).await.unwrap_or(false) {
        return Ok((media_files, quarantine_dirs));
    }
    let root_meta = fs::symlink_metadata(uploads_root)
        .await
        .map_err(|_| ApiError::conflict("Không đọc được thư mục uploads."))?;
    if root_meta.file_type().is_symlink() || !root_meta.is_dir() {
        return Err(ApiError::conflict("Thư mục uploads không hợp lệ."));
    }

    let mut top_entries = fs::read_dir(uploads_root)
        .await
        .map_err(|_| ApiError::conflict("Không thể liệt kê thư mục uploads."))?;

    while let Some(entry) = top_entries
        .next_entry()
        .await
        .map_err(|_| ApiError::conflict("Lỗi đọc mục trong uploads."))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let meta = check_no_symlink_and_count(&path, scanned_files).await?;

        if name == "media" && meta.is_dir() {
            let mut shards = fs::read_dir(&path)
                .await
                .map_err(|_| ApiError::conflict("Không thể đọc uploads/media."))?;
            while let Some(shard_entry) = shards
                .next_entry()
                .await
                .map_err(|_| ApiError::conflict("Lỗi đọc thư mục shard media."))?
            {
                let shard_name = shard_entry.file_name().to_string_lossy().to_string();
                let shard_path = shard_entry.path();
                let shard_meta = check_no_symlink_and_count(&shard_path, scanned_files).await?;
                if !shard_meta.is_dir()
                    || shard_name.len() != 2
                    || !shard_name
                        .bytes()
                        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                {
                    return Err(ApiError::conflict(
                        "Phát hiện tệp hoặc thư mục không hợp lệ trong uploads/media.",
                    ));
                }

                let mut files = fs::read_dir(&shard_path)
                    .await
                    .map_err(|_| ApiError::conflict("Không đọc được thư mục shard media."))?;
                while let Some(file_entry) = files
                    .next_entry()
                    .await
                    .map_err(|_| ApiError::conflict("Lỗi đọc tệp trong shard media."))?
                {
                    let fname = file_entry.file_name().to_string_lossy().to_string();
                    let fpath = file_entry.path();
                    let fmeta = check_no_symlink_and_count(&fpath, scanned_files).await?;
                    if !fmeta.is_file() {
                        return Err(ApiError::conflict(
                            "Phát hiện đối tượng không phải tệp thường trong uploads/media.",
                        ));
                    }
                    let Some(media_id) = parse_canonical_media_filename(&shard_name, &fname) else {
                        return Err(ApiError::conflict(
                            "Phát hiện tệp không rõ định danh trong uploads/media.",
                        ));
                    };
                    let mtime = modified_unix_secs(&fmeta)?;
                    media_files.push(ScannedFile {
                        logical_key: format!("media/{shard_name}/{fname}"),
                        abs_path: fpath,
                        media_id,
                        size_bytes: fmeta.len(),
                        modified_unix_secs: mtime,
                    });
                }
            }
        } else if name == ".quarantine" && meta.is_dir() {
            let mut q_entries = fs::read_dir(&path)
                .await
                .map_err(|_| ApiError::conflict("Không đọc được uploads/.quarantine."))?;
            while let Some(q_entry) = q_entries
                .next_entry()
                .await
                .map_err(|_| ApiError::conflict("Lỗi đọc uploads/.quarantine."))?
            {
                let qname = q_entry.file_name().to_string_lossy().to_string();
                let qpath = q_entry.path();
                let qmeta = check_no_symlink_and_count(&qpath, scanned_files).await?;
                if !qmeta.is_dir() {
                    return Err(ApiError::conflict(
                        "Phát hiện tệp lạ trong uploads/.quarantine.",
                    ));
                }
                let mut newest_mtime = modified_unix_secs(&qmeta)?;
                let mut qfiles = fs::read_dir(&qpath)
                    .await
                    .map_err(|_| ApiError::conflict("Không đọc được thư mục quarantine."))?;
                while let Some(qf) = qfiles
                    .next_entry()
                    .await
                    .map_err(|_| ApiError::conflict("Lỗi đọc tệp quarantine."))?
                {
                    let qf_meta = check_no_symlink_and_count(&qf.path(), scanned_files).await?;
                    if !qf_meta.is_file() {
                        return Err(ApiError::conflict(
                            "Phát hiện đối tượng không hợp lệ trong quarantine.",
                        ));
                    }
                    let qf_mtime = modified_unix_secs(&qf_meta)?;
                    if qf_mtime > newest_mtime {
                        newest_mtime = qf_mtime;
                    }
                }
                quarantine_dirs.push((qname, qpath, newest_mtime));
            }
        } else {
            return Err(ApiError::conflict(
                "Phát hiện tệp hoặc thư mục không xác định trong uploads.",
            ));
        }
    }

    Ok((media_files, quarantine_dirs))
}

pub(crate) async fn scan_secrets_tree(
    secrets_dir: &Path,
    scanned_files: &mut usize,
) -> ApiResult<Vec<ScannedSecretBlob>> {
    let mut blobs = Vec::new();
    if !fs::try_exists(secrets_dir).await.unwrap_or(false) {
        return Ok(blobs);
    }
    let dir_meta = fs::symlink_metadata(secrets_dir)
        .await
        .map_err(|_| ApiError::conflict("Không đọc được thư mục secrets."))?;
    if dir_meta.file_type().is_symlink() || !dir_meta.is_dir() {
        return Err(ApiError::conflict("Thư mục secrets không hợp lệ."));
    }

    let mut entries = fs::read_dir(secrets_dir)
        .await
        .map_err(|_| ApiError::conflict("Không thể liệt kê thư mục secrets."))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|_| ApiError::conflict("Lỗi đọc mục trong thư mục secrets."))?
    {
        let fname = entry.file_name().to_string_lossy().to_string();
        let fpath = entry.path();
        let fmeta = check_no_symlink_and_count(&fpath, scanned_files).await?;
        if !fmeta.is_file() {
            return Err(ApiError::conflict(
                "Phát hiện đối tượng không phải tệp trong thư mục secrets.",
            ));
        }
        let Some(secret_ref) = parse_secret_filename(&fname) else {
            return Err(ApiError::conflict(
                "Phát hiện tệp không rõ định danh trong thư mục secrets.",
            ));
        };
        blobs.push(ScannedSecretBlob {
            secret_ref,
            abs_path: fpath,
            modified_unix_secs: modified_unix_secs(&fmeta)?,
        });
    }

    Ok(blobs)
}

pub(crate) fn compute_plan_digest(
    generation: i64,
    candidates: &[ReconcileOrphanCandidate],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("gen:{generation}\n").as_bytes());
    let mut sorted: Vec<&ReconcileOrphanCandidate> = candidates.iter().collect();
    sorted.sort_by(|a, b| a.logical_key.cmp(&b.logical_key));
    for c in sorted {
        hasher.update(
            format!(
                "{}:{}:{}:{}\n",
                c.logical_key, c.size_bytes, c.modified_unix_secs, c.sha256
            )
            .as_bytes(),
        );
    }
    hex_encode(&hasher.finalize())
}
