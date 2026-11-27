use super::*;

pub(super) async fn stage_and_dedup_sample_media(
    db: &SqlitePool,
    uploads_root: &std::path::Path,
    run_id: &str,
    object_key: &str,
    safe_rel: &str,
    decoded: crate::media::DecodedMedia,
) -> ApiResult<StagedMediaEntry> {
    let existing_by_sha: Option<(String, String)> = sqlx::query_as(
        "SELECT id, extension FROM media_assets
         WHERE COALESCE(content_sha256, id) = ? AND lifecycle_state = 'active'
         LIMIT 1",
    )
    .bind(&decoded.id)
    .fetch_optional(db)
    .await?;

    let (actual_id, already_existed, queued_variants) = if let Some((eid, _eext)) = existing_by_sha
    {
        (eid, true, Vec::new())
    } else {
        let mut candidate = decoded.id.clone();
        let mut chosen = None;
        for attempt in 0..8 {
            if attempt > 0 {
                let nonce = crate::auth::generate_random_token();
                candidate = sha256_hex(format!("{}:{nonce}", decoded.id).as_bytes());
            }
            let id_taken: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM media_assets WHERE id = ? LIMIT 1")
                    .bind(&candidate)
                    .fetch_optional(db)
                    .await?;
            let del_pending: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM media_delete_jobs WHERE media_id = ? AND phase != 'finalized' LIMIT 1",
                )
                .bind(&candidate)
                .fetch_optional(db)
                .await?;
            if id_taken.is_none() && del_pending.is_none() {
                chosen = Some(candidate.clone());
                break;
            }
        }
        let allocated = chosen.ok_or_else(|| {
            ApiError::internal("Không cấp phát được mã media duy nhất cho sample.")
        })?;
        let (already, qv) = write_staged_and_promote(
            uploads_root,
            run_id,
            &allocated,
            decoded.extension,
            &decoded.original,
            &decoded.thumbnail,
            &decoded.variants,
        )
        .await?;
        (allocated, already, qv)
    };

    let filename = safe_rel
        .rsplit('/')
        .next()
        .unwrap_or("sample.png")
        .to_string();
    let asset = decoded.metadata(filename);
    Ok(StagedMediaEntry {
        object_key: object_key.to_string(),
        media_id: actual_id,
        extension: asset.extension,
        sha256: decoded.id.clone(),
        content_sha256: decoded.id,
        already_existed,
        filename: asset.filename,
        mime_type: asset.mime_type,
        byte_size: asset.byte_size,
        width: asset.width,
        height: asset.height,
        thumb_width: asset.thumb_width.unwrap_or(0),
        thumb_height: asset.thumb_height.unwrap_or(0),
        variants: queued_variants,
    })
}

pub(super) async fn persist_imported_media_entries_in_tx(
    conn: &mut sqlx::SqliteConnection,
    media_entries: &[StagedMediaEntry],
    theme_id: &str,
    plan: &ImportPlan,
    created: &mut Vec<ImportOutcomeItem>,
) -> ApiResult<()> {
    for m in media_entries {
        let content_sha = if m.content_sha256.is_empty() {
            m.sha256.as_str()
        } else {
            m.content_sha256.as_str()
        };
        sqlx::query(
            "INSERT INTO media_assets
             (id, content_sha256, filename, mime_type, extension, byte_size, width, height,
              thumb_width, thumb_height, variants_ready_version, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, datetime('now')) ON CONFLICT(id) DO NOTHING",
        )
        .bind(&m.media_id)
        .bind(content_sha)
        .bind(&m.filename)
        .bind(&m.mime_type)
        .bind(&m.extension)
        .bind(m.byte_size)
        .bind(m.width)
        .bind(m.height)
        .bind(m.thumb_width)
        .bind(m.thumb_height)
        .execute(&mut *conn)
        .await?;

        if !m.already_existed {
            for v in &m.variants {
                sqlx::query(
                    "INSERT INTO media_variants
                     (media_id, version, spec, format, width, height, byte_size, relative_path, sha256)
                     VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(media_id, version, spec, format) DO NOTHING",
                )
                .bind(&m.media_id)
                .bind(v.spec.as_str())
                .bind(v.format.as_str())
                .bind(i64::from(v.width))
                .bind(i64::from(v.height))
                .bind(i64::try_from(v.byte_size).unwrap_or(0))
                .bind(&v.relative_path)
                .bind(&v.sha256)
                .execute(&mut *conn)
                .await?;
            }
        }

        sqlx::query(
            "INSERT INTO import_provenance
             (theme_id, sample_key, resource_kind, object_key, revision_id, revision_digest, target_id, target_hash)
             VALUES (?, ?, 'media', ?, ?, ?, ?, ?)",
        )
        .bind(theme_id)
        .bind(&plan.sample_key)
        .bind(&m.object_key)
        .bind(&plan.revision_id)
        .bind(&plan.revision_digest)
        .bind(&m.media_id)
        .bind(&m.sha256)
        .execute(&mut *conn)
        .await?;

        created.push(ImportOutcomeItem {
            resource_kind: SampleGroup::Media,
            object_key: m.object_key.clone(),
            target_id: m.media_id.clone(),
            status: "created".to_string(),
            reason: None,
        });
    }
    Ok(())
}
