use super::*;

pub async fn plan_reconcile(
    db: &SqlitePool,
    media_store: &MediaStore,
    secrets_dir: &Path,
    actor_user_id: Option<String>,
    _maint: &ExclusiveOpsContext,
) -> ApiResult<ReconcilePlan> {
    let scanned_records = count_reconcile_records_bounded(db, MAX_RECONCILE_RECORDS).await?;
    let media_rows = load_media_inventory_bounded(db, MAX_RECONCILE_RECORDS).await?;
    let active_variant_keys = list_active_variant_paths_bounded(db, MAX_RECONCILE_FILES).await?;
    let pending_delete = list_pending_delete_jobs(db).await?;
    let secret_refs = collect_referenced_secret_refs(db).await?;
    let generation = get_maintenance_generation(db).await?;

    let mut scanned_files = 0usize;
    let uploads_root = media_store.uploads_root();
    let (disk_media_files, quarantine_dirs) =
        scan_uploads_tree(&uploads_root, &mut scanned_files).await?;
    let q_ids: Vec<String> = quarantine_dirs.iter().map(|(q, _, _)| q.clone()).collect();
    let delete_jobs_by_id = find_delete_jobs_by_ids(db, &q_ids).await?;
    let replace_jobs_by_id = find_replace_jobs_by_ids(db, &q_ids).await?;
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

    let pending_delete_jobs = pending_delete.len();
    let active_delete_media_ids: HashSet<String> =
        pending_delete.into_iter().map(|j| j.media_id).collect();

    let mut orphan_candidates = Vec::new();
    let mut recent_orphans_under_grace = 0usize;
    for f in disk_media_files {
        if active_delete_media_ids.contains(&f.media_id)
            || media_store.has_active_reader(&f.media_id)
        {
            continue;
        }
        let is_expected = match f.kind {
            ReconcileOrphanKind::Canonical => known_media_ids.contains(&f.media_id),
            ReconcileOrphanKind::Variant => active_variant_keys.contains(&f.logical_key),
        };
        if is_expected {
            continue;
        }
        let age = now.saturating_sub(f.modified_unix_secs);
        if age >= ORPHAN_GRACE_SECS {
            let sha256 = sha256_file_hex(&f.abs_path).await?;
            orphan_candidates.push(ReconcileOrphanCandidate {
                media_id: f.media_id,
                logical_key: f.logical_key,
                kind: f.kind,
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
        if is_quarantine_dir_purgeable(qname, *qmtime, now, &delete_jobs_by_id, &replace_jobs_by_id)
        {
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

    let has_variant_candidates = plan
        .orphan_candidates
        .iter()
        .any(|c| c.kind == ReconcileOrphanKind::Variant);
    let active_variant_keys = if has_variant_candidates {
        list_active_variant_paths_bounded(db, MAX_RECONCILE_FILES).await?
    } else {
        HashSet::new()
    };

    let uploads_root = media_store.uploads_root();
    for c in &plan.orphan_candidates {
        if media_store.has_active_reader(&c.media_id) {
            return Err(ApiError::precondition_failed(
                "Tệp ứng viên đang có luồng đọc; vui lòng lập kế hoạch lại.",
            ));
        }
        let still_orphan = match c.kind {
            ReconcileOrphanKind::Canonical => is_media_unreferenced(db, &c.media_id).await?,
            ReconcileOrphanKind::Variant => !active_variant_keys.contains(&c.logical_key),
        };
        if !still_orphan {
            return Err(ApiError::precondition_failed(
                "Tệp ứng viên đã được tham chiếu hoặc kích hoạt; vui lòng lập kế hoạch lại.",
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
        let dst_name = match c.kind {
            ReconcileOrphanKind::Canonical => Path::new(&c.logical_key)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| ApiError::internal("Tên tệp ứng viên không hợp lệ."))?
                .to_string(),
            ReconcileOrphanKind::Variant => c.logical_key.replace('/', "__"),
        };
        let dst_rel = format!(".quarantine/{apply_job_id}/{dst_name}");
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
