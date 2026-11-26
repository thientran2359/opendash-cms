use super::*;

/// Parses `If-Match: "gv<generation>-v<version>"` (missing -> 428, weak/malformed -> 412).
pub fn parse_media_if_match(headers: &HeaderMap) -> ApiResult<(i64, i64)> {
    let raw = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .ok_or_else(|| ApiError::precondition_required("Thiếu header If-Match."))?;

    if raw.is_empty() || raw.starts_with("W/") || raw == "*" {
        return Err(ApiError::precondition_failed(
            "Header If-Match phải có dạng \"gv<generation>-v<version>\".",
        ));
    }

    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);

    let Some((gen_part, ver_part)) = inner.split_once("-v") else {
        return Err(ApiError::precondition_failed(
            "Header If-Match không đúng định dạng gv<generation>-v<version>.",
        ));
    };
    let Some(gen_str) = gen_part.strip_prefix("gv") else {
        return Err(ApiError::precondition_failed(
            "Header If-Match không đúng định dạng gv<generation>-v<version>.",
        ));
    };
    let generation: i64 =
        gen_str.parse().ok().filter(|&v| v >= 1).ok_or_else(|| {
            ApiError::precondition_failed("Generation trong If-Match không hợp lệ.")
        })?;
    let version: i64 = ver_part
        .parse()
        .ok()
        .filter(|&v| v >= 1)
        .ok_or_else(|| ApiError::precondition_failed("Version trong If-Match không hợp lệ."))?;

    Ok((generation, version))
}

pub(super) async fn move_one_file_to_quarantine(
    uploads_root: &Path,
    src_rel: &str,
    dst_rel: &str,
) -> Result<(), &'static str> {
    let src = uploads_root.join(src_rel);
    let dst = uploads_root.join(dst_rel);

    if fs::try_exists(&dst).await.unwrap_or(false) {
        return Ok(());
    }
    if !fs::try_exists(&src).await.unwrap_or(false) {
        return Ok(());
    }

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|_| "quarantine_mkdir_failed")?;
    }

    fs::rename(&src, &dst)
        .await
        .map_err(|_| "quarantine_rename_failed")?;
    Ok(())
}

pub(super) async fn execute_delete_job_inner(
    db: &SqlitePool,
    media_store: &MediaStore,
    job: MediaDeleteJob,
) -> ApiResult<MediaDeleteJob> {
    if job.phase == MediaDeletePhase::Finalized {
        return Ok(job);
    }

    let _drain_guard = match media_store
        .begin_delete_drain(&job.media_id, Duration::from_secs(5))
        .await
    {
        Ok(guard) => guard,
        Err(_pending_guard) => {
            crate::storage::media_lifecycle::set_delete_job_error(
                db,
                &job.job_id,
                "readers_active",
            )
            .await?;
            return get_delete_job(db, &job.job_id)
                .await?
                .ok_or_else(|| ApiError::internal("Không đọc lại được delete job."));
        }
    };

    if job.phase == MediaDeletePhase::Tombstoned {
        crate::storage::media_lifecycle::mark_delete_job_moving(db, &job.job_id).await?;
    }

    let uploads_root = media_store.uploads_root();

    if !job.original_moved {
        if let Err(code) = move_one_file_to_quarantine(
            &uploads_root,
            &job.original_rel_path,
            &job.quarantine_original_rel_path,
        )
        .await
        {
            crate::storage::media_lifecycle::set_delete_job_error(db, &job.job_id, code).await?;
            return get_delete_job(db, &job.job_id)
                .await?
                .ok_or_else(|| ApiError::internal("Không đọc lại được delete job."));
        }
        crate::storage::media_lifecycle::mark_original_moved(db, &job.job_id).await?;
    }

    if let (Some(thumb_src), Some(thumb_dst)) = (
        job.thumbnail_rel_path.as_deref(),
        job.quarantine_thumbnail_rel_path.as_deref(),
    ) {
        if !job.thumbnail_moved {
            if let Err(code) =
                move_one_file_to_quarantine(&uploads_root, thumb_src, thumb_dst).await
            {
                crate::storage::media_lifecycle::set_delete_job_error(db, &job.job_id, code)
                    .await?;
                return get_delete_job(db, &job.job_id)
                    .await?
                    .ok_or_else(|| ApiError::internal("Không đọc lại được delete job."));
            }
            crate::storage::media_lifecycle::mark_thumbnail_moved(db, &job.job_id).await?;
        }
    }

    crate::storage::media_lifecycle::finalize_delete_job(
        db,
        &job.job_id,
        &job.media_id,
        job.generation,
    )
    .await?;

    get_delete_job(db, &job.job_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được delete job sau khi hoàn tất."))
}

pub async fn execute_delete_job(
    db: &SqlitePool,
    media_store: &MediaStore,
    job: MediaDeleteJob,
    _lease: &MutationLease,
) -> ApiResult<MediaDeleteJob> {
    execute_delete_job_inner(db, media_store, job).await
}

#[allow(dead_code)]
pub async fn recover_delete_job(
    db: &SqlitePool,
    media_store: &MediaStore,
    job: MediaDeleteJob,
    _maint: &ExclusiveOpsContext,
) -> ApiResult<MediaDeleteJob> {
    execute_delete_job_inner(db, media_store, job).await
}

pub async fn recover_pending_delete_jobs_on_startup(
    db: &SqlitePool,
    media_store: &MediaStore,
) -> ApiResult<usize> {
    let jobs = crate::storage::media_lifecycle::list_pending_delete_jobs(db).await?;
    let mut count = 0usize;
    for job in jobs {
        let _ = execute_delete_job_inner(db, media_store, job).await?;
        count += 1;
    }
    Ok(count)
}

pub(super) fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn modified_unix_secs(meta: &std::fs::Metadata) -> ApiResult<u64> {
    let mtime = meta
        .modified()
        .map_err(|_| ApiError::conflict("Không đọc được thời gian sửa đổi tệp."))?;
    Ok(mtime
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs())
}

pub(super) async fn sha256_file_hex(path: &Path) -> ApiResult<String> {
    let bytes = fs::read(path)
        .await
        .map_err(|_| ApiError::conflict("Không đọc được tệp để tính checksum."))?;
    let digest = Sha256::digest(&bytes);
    Ok(hex_encode(&digest))
}

pub(super) fn new_64hex_id() -> String {
    let token = crate::auth::generate_random_token();
    let digest = Sha256::digest(token.as_bytes());
    hex_encode(&digest)
}

pub(super) fn is_64hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub(super) fn parse_canonical_media_filename(shard: &str, name: &str) -> Option<String> {
    if shard.len() != 2
        || !shard
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return None;
    }
    if let Some(id) = name.strip_suffix(".thumb.png") {
        if is_64hex(id) && id.starts_with(shard) {
            return Some(id.to_string());
        }
        return None;
    }
    let (id, ext) = name.rsplit_once('.')?;
    if !is_64hex(id) || !id.starts_with(shard) {
        return None;
    }
    if matches!(ext, "jpg" | "png" | "webp" | "mp4") {
        Some(id.to_string())
    } else {
        None
    }
}

pub(super) fn parse_secret_filename(name: &str) -> Option<String> {
    let stem = name
        .strip_suffix(".bin")
        .or_else(|| name.strip_suffix(".enc"))
        .unwrap_or(name);
    if is_64hex(stem) {
        Some(stem.to_ascii_lowercase())
    } else {
        None
    }
}

pub(super) async fn check_no_symlink_and_count(
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

pub(super) async fn scan_uploads_tree(
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

pub(super) async fn scan_secrets_tree(
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

pub(super) fn compute_plan_digest(
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

pub async fn plan_reconcile(
    db: &SqlitePool,
    media_store: &MediaStore,
    secrets_dir: &Path,
    actor_user_id: Option<String>,
    _maint: &ExclusiveOpsContext,
) -> ApiResult<ReconcilePlan> {
    let scanned_records = count_reconcile_records_bounded(db, MAX_RECONCILE_RECORDS).await?;
    let media_rows = load_media_inventory_bounded(db, MAX_RECONCILE_RECORDS).await?;
    let all_delete_jobs = crate::storage::media_lifecycle::list_all_delete_jobs(db).await?;
    let secret_refs = collect_referenced_secret_refs(db).await?;
    let generation = get_maintenance_generation(db).await?;

    let mut scanned_files = 0usize;
    let uploads_root = media_store.uploads_root();
    let (disk_media_files, quarantine_dirs) =
        scan_uploads_tree(&uploads_root, &mut scanned_files).await?;
    let disk_secrets = scan_secrets_tree(secrets_dir, &mut scanned_files).await?;

    let now = now_unix_secs();

    let mut known_media_ids: HashSet<String> = HashSet::with_capacity(media_rows.len());
    let mut disk_keys: HashSet<String> = HashSet::with_capacity(disk_media_files.len());
    for f in &disk_media_files {
        disk_keys.insert(f.logical_key.clone());
    }

    let mut missing_canonical_files = Vec::new();
    for row in &media_rows {
        known_media_ids.insert(row.id.clone());
        if row.lifecycle_state == "active" {
            let shard = &row.id[..2];
            let orig_key = format!("media/{shard}/{}.{}", row.id, row.extension);
            if !disk_keys.contains(&orig_key) {
                missing_canonical_files.push(MissingCanonicalDiagnostic {
                    media_id: row.id.clone(),
                    logical_key: orig_key,
                    lifecycle_state: row.lifecycle_state.clone(),
                });
            }
            if row.kind == "image" {
                let thumb_key = format!("media/{shard}/{}.thumb.png", row.id);
                if !disk_keys.contains(&thumb_key) {
                    missing_canonical_files.push(MissingCanonicalDiagnostic {
                        media_id: row.id.clone(),
                        logical_key: thumb_key,
                        lifecycle_state: row.lifecycle_state.clone(),
                    });
                }
            }
        }
    }

    let mut pending_delete_jobs = 0usize;
    let mut active_delete_media_ids: HashSet<String> = HashSet::new();
    let mut job_by_id: HashMap<String, MediaDeleteJob> = HashMap::new();
    for j in all_delete_jobs {
        if j.phase != MediaDeletePhase::Finalized {
            pending_delete_jobs += 1;
            active_delete_media_ids.insert(j.media_id.clone());
        }
        job_by_id.insert(j.job_id.clone(), j);
    }

    let mut orphan_candidates = Vec::new();
    let mut recent_orphans_under_grace = 0usize;
    for f in disk_media_files {
        if known_media_ids.contains(&f.media_id)
            || active_delete_media_ids.contains(&f.media_id)
            || media_store.has_active_reader(&f.media_id)
        {
            continue;
        }
        let age = now.saturating_sub(f.modified_unix_secs);
        if age >= ORPHAN_GRACE_SECS {
            let sha256 = sha256_file_hex(&f.abs_path).await?;
            orphan_candidates.push(ReconcileOrphanCandidate {
                media_id: f.media_id,
                logical_key: f.logical_key,
                size_bytes: f.size_bytes,
                modified_unix_secs: f.modified_unix_secs,
                sha256,
            });
        } else {
            recent_orphans_under_grace += 1;
        }
    }
    orphan_candidates.sort_by(|a, b| a.logical_key.cmp(&b.logical_key));

    let mut purgeable_quarantine_count = 0usize;
    let mut retained_quarantine_count = 0usize;
    for (qname, _qpath, qmtime) in &quarantine_dirs {
        let age = now.saturating_sub(*qmtime);
        let finalized_or_reconcile = job_by_id
            .get(qname)
            .map(|j| j.phase == MediaDeletePhase::Finalized)
            .unwrap_or(true);
        if finalized_or_reconcile && age >= PURGE_GRACE_SECS {
            purgeable_quarantine_count += 1;
        } else {
            retained_quarantine_count += 1;
        }
    }

    let mut orphan_secret_count = 0usize;
    let mut purgeable_secret_count = 0usize;
    for s in &disk_secrets {
        if !secret_refs.contains(&s.secret_ref) {
            orphan_secret_count += 1;
            if now.saturating_sub(s.modified_unix_secs) >= PURGE_GRACE_SECS {
                purgeable_secret_count += 1;
            }
        }
    }

    let plan_id = new_64hex_id();
    let digest = compute_plan_digest(generation, &orphan_candidates);
    let plan = ReconcilePlan {
        plan_id: plan_id.clone(),
        generation,
        version: 1,
        digest,
        created_at_unix: now,
        scanned_records,
        scanned_files,
        missing_canonical_files,
        orphan_candidates,
        recent_orphans_under_grace,
        pending_delete_jobs,
        purgeable_quarantine_count,
        retained_quarantine_count,
        orphan_secret_count,
        purgeable_secret_count,
    };

    let summary_json = serde_json::to_string(&plan)
        .map_err(|_| ApiError::internal("Không thể tuần tự hóa kế hoạch đối soát."))?;
    let job = create_job(
        db,
        JobCreateInput {
            id: plan_id.clone(),
            kind: OperationKind::Reconcile,
            actor_user_id,
            reason: Some("reconcile_plan".to_string()),
        },
    )
    .await?;
    let _ = transition_job_cas(
        db,
        &job.id,
        JobTransitionInput {
            expected_version: job.version,
            next_state: OperationState::Succeeded,
            summary_json: Some(summary_json),
            error_code: None,
            error_message: None,
        },
    )
    .await?;
    let _ = prune_old_receipts(db, MAX_RETAINED_OPERATION_RECEIPTS).await?;

    Ok(plan)
}

pub async fn apply_reconcile(
    db: &SqlitePool,
    media_store: &MediaStore,
    actor_user_id: Option<String>,
    req: ApplyReconcileRequest,
    _maint: &ExclusiveOpsContext,
) -> ApiResult<ApplyReconcileResult> {
    if !req.confirm {
        return Err(ApiError::bad_request(
            "Cần xác nhận rõ ràng (confirm=true) để áp dụng đối soát.",
        ));
    }

    let plan_job = get_job(db, &req.plan_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy kế hoạch đối soát."))?;
    if plan_job.kind != OperationKind::Reconcile {
        return Err(ApiError::bad_request(
            "Mã tác vụ không phải kế hoạch đối soát.",
        ));
    }
    let plan_json = plan_job
        .summary_json
        .ok_or_else(|| ApiError::precondition_failed("Kế hoạch đối soát không có dữ liệu."))?;
    let plan: ReconcilePlan = serde_json::from_str(&plan_json)
        .map_err(|_| ApiError::precondition_failed("Dữ liệu kế hoạch đối soát không hợp lệ."))?;

    let current_gen = get_maintenance_generation(db).await?;
    if current_gen != plan.generation
        || req.expected_generation != plan.generation
        || req.expected_version != plan.version
        || req.digest != plan.digest
    {
        return Err(ApiError::precondition_failed(
            "Trạng thái hoặc digest của kế hoạch đối soát đã thay đổi; vui lòng lập kế hoạch lại.",
        ));
    }

    let uploads_root = media_store.uploads_root();
    for c in &plan.orphan_candidates {
        if media_store.has_active_reader(&c.media_id)
            || !is_media_unreferenced(db, &c.media_id).await?
        {
            return Err(ApiError::precondition_failed(
                "Tệp ứng viên đã được tham chiếu hoặc đang có luồng đọc; vui lòng lập kế hoạch lại.",
            ));
        }
        let abs_path = uploads_root.join(&c.logical_key);
        let meta = fs::symlink_metadata(&abs_path).await.map_err(|_| {
            ApiError::precondition_failed(
                "Tệp ứng viên không còn tồn tại; vui lòng lập kế hoạch lại.",
            )
        })?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(ApiError::precondition_failed(
                "Danh tính tệp ứng viên đã thay đổi.",
            ));
        }
        let mtime = modified_unix_secs(&meta)?;
        let sha = sha256_file_hex(&abs_path).await?;
        if meta.len() != c.size_bytes || mtime != c.modified_unix_secs || sha != c.sha256 {
            return Err(ApiError::precondition_failed(
                "Nội dung hoặc thời gian sửa đổi tệp ứng viên đã thay đổi.",
            ));
        }
    }

    let apply_job_id = new_64hex_id();
    let created = create_job(
        db,
        JobCreateInput {
            id: apply_job_id.clone(),
            kind: OperationKind::Reconcile,
            actor_user_id,
            reason: Some(format!("reconcile_apply:{}", plan.plan_id)),
        },
    )
    .await?;
    let running = transition_job_cas(
        db,
        &created.id,
        JobTransitionInput {
            expected_version: created.version,
            next_state: OperationState::Running,
            summary_json: None,
            error_code: None,
            error_message: None,
        },
    )
    .await?;

    let mut quarantined_files = 0usize;
    for c in &plan.orphan_candidates {
        let file_name = Path::new(&c.logical_key)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ApiError::internal("Tên tệp ứng viên không hợp lệ."))?;
        let dst_rel = format!(".quarantine/{apply_job_id}/{file_name}");
        move_one_file_to_quarantine(&uploads_root, &c.logical_key, &dst_rel)
            .await
            .map_err(|code| ApiError::internal(format!("Không thể cách ly tệp: {code}")))?;
        quarantined_files += 1;
    }

    let next_gen = bump_maintenance_generation(db, OperationKind::Reconcile).await?;
    let summary = serde_json::json!({
        "plan_id": plan.plan_id,
        "quarantined_files": quarantined_files,
        "generation": next_gen,
    });
    let finished = transition_job_cas(
        db,
        &running.id,
        JobTransitionInput {
            expected_version: running.version,
            next_state: OperationState::Succeeded,
            summary_json: Some(summary.to_string()),
            error_code: None,
            error_message: None,
        },
    )
    .await?;
    let _ = prune_old_receipts(db, MAX_RETAINED_OPERATION_RECEIPTS).await?;

    Ok(ApplyReconcileResult {
        job: finished,
        quarantined_files,
        generation: next_gen,
    })
}

pub async fn purge_quarantine(
    db: &SqlitePool,
    media_store: &MediaStore,
    secrets_dir: &Path,
    actor_user_id: Option<String>,
    confirm: bool,
    _maint: &ExclusiveOpsContext,
) -> ApiResult<PurgeQuarantineResult> {
    if !confirm {
        return Err(ApiError::bad_request(
            "Cần xác nhận rõ ràng (confirm=true) để dọn vùng cách ly.",
        ));
    }

    let mut scanned_files = 0usize;
    let uploads_root = media_store.uploads_root();
    let (_media_files, quarantine_dirs) =
        scan_uploads_tree(&uploads_root, &mut scanned_files).await?;
    let disk_secrets = scan_secrets_tree(secrets_dir, &mut scanned_files).await?;

    let job_id = new_64hex_id();
    let created = create_job(
        db,
        JobCreateInput {
            id: job_id.clone(),
            kind: OperationKind::Purge,
            actor_user_id,
            reason: Some("quarantine_purge".to_string()),
        },
    )
    .await?;
    let running = transition_job_cas(
        db,
        &created.id,
        JobTransitionInput {
            expected_version: created.version,
            next_state: OperationState::Running,
            summary_json: None,
            error_code: None,
            error_message: None,
        },
    )
    .await?;

    let all_delete_jobs = crate::storage::media_lifecycle::list_all_delete_jobs(db).await?;
    let job_by_id: HashMap<String, MediaDeleteJob> = all_delete_jobs
        .into_iter()
        .map(|j| (j.job_id.clone(), j))
        .collect();
    let secret_refs = collect_referenced_secret_refs(db).await?;
    let now = now_unix_secs();

    let mut purged_quarantine_dirs = 0usize;
    let mut skipped_under_grace = 0usize;
    for (qname, qpath, qmtime) in quarantine_dirs {
        let age = now.saturating_sub(qmtime);
        if age < PURGE_GRACE_SECS {
            skipped_under_grace += 1;
            continue;
        }
        if let Some(j) = job_by_id.get(&qname) {
            if j.phase != MediaDeletePhase::Finalized
                || !is_media_unreferenced(db, &j.media_id).await?
            {
                skipped_under_grace += 1;
                continue;
            }
        }
        fs::remove_dir_all(&qpath)
            .await
            .map_err(|_| ApiError::internal("Không thể xóa thư mục cách ly đã hết hạn."))?;
        purged_quarantine_dirs += 1;
    }

    let mut purged_secret_blobs = 0usize;
    for s in disk_secrets {
        if secret_refs.contains(&s.secret_ref) {
            continue;
        }
        let age = now.saturating_sub(s.modified_unix_secs);
        if age < PURGE_GRACE_SECS {
            skipped_under_grace += 1;
            continue;
        }
        fs::remove_file(&s.abs_path)
            .await
            .map_err(|_| ApiError::internal("Không thể xóa tệp secret mồ côi đã hết hạn."))?;
        purged_secret_blobs += 1;
    }

    let next_gen = bump_maintenance_generation(db, OperationKind::Purge).await?;
    let summary = serde_json::json!({
        "purged_quarantine_dirs": purged_quarantine_dirs,
        "purged_secret_blobs": purged_secret_blobs,
        "skipped_under_grace": skipped_under_grace,
        "generation": next_gen,
    });
    let finished = transition_job_cas(
        db,
        &running.id,
        JobTransitionInput {
            expected_version: running.version,
            next_state: OperationState::Succeeded,
            summary_json: Some(summary.to_string()),
            error_code: None,
            error_message: None,
        },
    )
    .await?;
    let _ = prune_old_receipts(db, MAX_RETAINED_OPERATION_RECEIPTS).await?;

    Ok(PurgeQuarantineResult {
        job: finished,
        purged_quarantine_dirs,
        purged_secret_blobs,
        skipped_under_grace,
        generation: next_gen,
    })
}

pub async fn collect_operations_diagnostics(
    db: &SqlitePool,
    media_store: &MediaStore,
    secrets_dir: &Path,
) -> ApiResult<OperationsDiagnostics> {
    let maintenance_generation = get_maintenance_generation(db).await?;
    let all_delete_jobs = crate::storage::media_lifecycle::list_all_delete_jobs(db).await?;
    let pending_delete_jobs: Vec<MediaDeleteJob> = all_delete_jobs
        .iter()
        .filter(|j| j.phase != MediaDeletePhase::Finalized)
        .cloned()
        .collect();
    let job_by_id: HashMap<String, MediaDeleteJob> = all_delete_jobs
        .into_iter()
        .map(|j| (j.job_id.clone(), j))
        .collect();

    let mut scanned_files = 0usize;
    let uploads_root = media_store.uploads_root();
    let (_media_files, quarantine_dirs) =
        scan_uploads_tree(&uploads_root, &mut scanned_files).await?;
    let disk_secrets = scan_secrets_tree(secrets_dir, &mut scanned_files).await?;
    let secret_refs = collect_referenced_secret_refs(db).await?;
    let now = now_unix_secs();

    let total_quarantine_dirs = quarantine_dirs.len();
    let mut purgeable_quarantine_dirs = 0usize;
    let mut retained_quarantine_dirs = 0usize;
    for (qname, _qpath, qmtime) in &quarantine_dirs {
        let finalized_or_reconcile = job_by_id
            .get(qname)
            .map(|j| j.phase == MediaDeletePhase::Finalized)
            .unwrap_or(true);
        if finalized_or_reconcile && now.saturating_sub(*qmtime) >= PURGE_GRACE_SECS {
            purgeable_quarantine_dirs += 1;
        } else {
            retained_quarantine_dirs += 1;
        }
    }

    let total_secret_blobs = disk_secrets.len();
    let mut orphan_secret_blobs = 0usize;
    let mut purgeable_secret_blobs = 0usize;
    let mut orphan_secrets_redacted = Vec::new();
    for s in disk_secrets {
        if !secret_refs.contains(&s.secret_ref) {
            orphan_secret_blobs += 1;
            let age_secs = now.saturating_sub(s.modified_unix_secs);
            let purgeable = age_secs >= PURGE_GRACE_SECS;
            if purgeable {
                purgeable_secret_blobs += 1;
            }
            orphan_secrets_redacted.push(RedactedSecretDiagnostic {
                ref_prefix: format!("{}...", &s.secret_ref[..8]),
                purgeable,
                age_hours: age_secs / 3600,
            });
        }
    }

    let recent_jobs = list_jobs(db, 20).await?;

    Ok(OperationsDiagnostics {
        maintenance_generation,
        pending_delete_jobs,
        total_quarantine_dirs,
        purgeable_quarantine_dirs,
        retained_quarantine_dirs,
        total_secret_blobs,
        orphan_secret_blobs,
        purgeable_secret_blobs,
        orphan_secrets_redacted,
        recent_jobs,
    })
}
