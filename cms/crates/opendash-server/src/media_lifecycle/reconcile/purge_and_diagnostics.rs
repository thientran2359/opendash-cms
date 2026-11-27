use super::*;

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

    let q_ids: Vec<String> = quarantine_dirs.iter().map(|(q, _, _)| q.clone()).collect();
    let delete_jobs_by_id = find_delete_jobs_by_ids(db, &q_ids).await?;
    let replace_jobs_by_id = find_replace_jobs_by_ids(db, &q_ids).await?;
    let secret_refs = collect_referenced_secret_refs(db).await?;
    let now = now_unix_secs();

    let mut purged_quarantine_dirs = 0usize;
    let mut skipped_under_grace = 0usize;
    for (qname, qpath, qmtime) in quarantine_dirs {
        // Active replace jobs in .quarantine/ must NEVER be deleted regardless of file mtime.
        if let Some(rj) = replace_jobs_by_id.get(&qname)
            && rj.phase.is_active()
        {
            skipped_under_grace += 1;
            continue;
        }
        if let Some(j) = delete_jobs_by_id.get(&qname)
            && (j.phase != MediaDeletePhase::Finalized
                || !is_media_unreferenced(db, &j.media_id).await?)
        {
            skipped_under_grace += 1;
            continue;
        }
        let age = now.saturating_sub(qmtime);
        if age < PURGE_GRACE_SECS {
            skipped_under_grace += 1;
            continue;
        }
        fs::remove_dir_all(&qpath)
            .await
            .map_err(|_| ApiError::internal("Không thể xóa thư mục cách ly đã hết hạn."))?;
        if replace_jobs_by_id.contains_key(&qname) {
            mark_replace_job_purged(db, &qname).await?;
        }
        purged_quarantine_dirs += 1;
    }

    let _ = prune_purged_replace_jobs(db).await?;

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

pub(crate) fn is_quarantine_dir_purgeable(
    qname: &str,
    qmtime: u64,
    now: u64,
    delete_jobs_by_id: &HashMap<String, MediaDeleteJob>,
    replace_jobs_by_id: &HashMap<String, MediaReplaceJob>,
) -> bool {
    if let Some(rj) = replace_jobs_by_id.get(qname) {
        if rj.phase.is_active() {
            return false;
        }
        return now.saturating_sub(qmtime) >= PURGE_GRACE_SECS;
    }
    let finalized_or_reconcile = delete_jobs_by_id
        .get(qname)
        .map(|j| j.phase == MediaDeletePhase::Finalized)
        .unwrap_or(true);
    finalized_or_reconcile && now.saturating_sub(qmtime) >= PURGE_GRACE_SECS
}

pub async fn collect_operations_diagnostics(
    db: &SqlitePool,
    media_store: &MediaStore,
    secrets_dir: &Path,
) -> ApiResult<OperationsDiagnostics> {
    let maintenance_generation = get_maintenance_generation(db).await?;
    let pending_delete_jobs = list_pending_delete_jobs(db).await?;

    let mut scanned_files = 0usize;
    let uploads_root = media_store.uploads_root();
    let (_media_files, quarantine_dirs) =
        scan_uploads_tree(&uploads_root, &mut scanned_files).await?;
    let q_ids: Vec<String> = quarantine_dirs.iter().map(|(q, _, _)| q.clone()).collect();
    let delete_jobs_by_id = find_delete_jobs_by_ids(db, &q_ids).await?;
    let replace_jobs_by_id = find_replace_jobs_by_ids(db, &q_ids).await?;

    let disk_secrets = scan_secrets_tree(secrets_dir, &mut scanned_files).await?;
    let secret_refs = collect_referenced_secret_refs(db).await?;
    let now = now_unix_secs();

    let total_quarantine_dirs = quarantine_dirs.len();
    let mut purgeable_quarantine_dirs = 0usize;
    let mut retained_quarantine_dirs = 0usize;
    for (qname, _qpath, qmtime) in &quarantine_dirs {
        if is_quarantine_dir_purgeable(qname, *qmtime, now, &delete_jobs_by_id, &replace_jobs_by_id)
        {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::media::{MediaReplaceJob, MediaReplacePhase};

    fn replace_job_with_phase(job_id: &str, phase: MediaReplacePhase) -> MediaReplaceJob {
        MediaReplaceJob {
            job_id: job_id.to_string(),
            media_id: "media_1".to_string(),
            expected_generation: 1,
            expected_version: 1,
            new_version: 2,
            old_content_sha256: "oldhash".to_string(),
            new_content_sha256: "newhash".to_string(),
            new_kind: "image".to_string(),
            new_filename: "new.png".to_string(),
            new_mime_type: "image/png".to_string(),
            new_extension: "png".to_string(),
            new_byte_size: 1234,
            new_width: 800,
            new_height: 600,
            new_thumb_width: Some(320),
            new_thumb_height: Some(240),
            new_duration_ms: None,
            new_video_codec: None,
            new_audio_codec: None,
            new_variants_json: "[]".to_string(),
            old_canonical_rel_path: "media/me/media_1.png".to_string(),
            old_thumb_rel_path: Some("media/me/media_1.thumb.png".to_string()),
            staging_rel_dir: format!(".staging/replace-{job_id}"),
            quarantine_rel_dir: format!(".quarantine/{job_id}"),
            phase,
            error_message: None,
            created_at: "2026-11-27T00:00:00Z".to_string(),
            updated_at: "2026-11-27T00:00:00Z".to_string(),
            purge_after: None,
            purged_at: None,
        }
    }

    #[test]
    fn active_replace_jobs_protect_quarantine_dir_past_grace_period() {
        let now = 2_000_000u64;
        let qmtime = now - (9 * 86_400);
        let delete_jobs = HashMap::new();

        for phase in [
            MediaReplacePhase::Staged,
            MediaReplacePhase::MovingOld,
            MediaReplacePhase::PromotingNew,
        ] {
            let mut replace_jobs = HashMap::new();
            replace_jobs.insert(
                "rj_active".to_string(),
                replace_job_with_phase("rj_active", phase),
            );
            assert!(
                !is_quarantine_dir_purgeable("rj_active", qmtime, now, &delete_jobs, &replace_jobs,),
                "active replace phase must prevent quarantine purge past grace period"
            );
        }

        for phase in [MediaReplacePhase::Completed, MediaReplacePhase::RolledBack] {
            let mut replace_jobs = HashMap::new();
            replace_jobs.insert(
                "rj_done".to_string(),
                replace_job_with_phase("rj_done", phase),
            );
            assert!(
                is_quarantine_dir_purgeable("rj_done", qmtime, now, &delete_jobs, &replace_jobs,),
                "terminal replace phase should allow quarantine purge past grace period"
            );
        }
    }
}
