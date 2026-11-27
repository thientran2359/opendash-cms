use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use super::types::MediaAsset;
use crate::{
    error::{ApiError, ApiResult},
    media::{DecodedMedia, GeneratedVariant, MediaStore},
};

pub async fn find(db: &SqlitePool, id: &str) -> ApiResult<Option<MediaAsset>> {
    Ok(sqlx::query_as::<_, MediaAsset>(
        "SELECT id, COALESCE(content_sha256, id) AS content_sha256, kind, filename, mime_type,
                extension, byte_size, width, height, thumb_width, thumb_height, duration_ms,
                video_codec, audio_codec, lifecycle_state, generation, version,
                variants_ready_version, created_at, COALESCE(updated_at, created_at) AS updated_at
         FROM media_assets WHERE id = ? AND lifecycle_state = 'active'",
    )
    .bind(id)
    .fetch_optional(db)
    .await?)
}

#[allow(dead_code)]
pub async fn find_any_state(db: &SqlitePool, id: &str) -> ApiResult<Option<MediaAsset>> {
    Ok(sqlx::query_as::<_, MediaAsset>(
        "SELECT id, COALESCE(content_sha256, id) AS content_sha256, kind, filename, mime_type,
                extension, byte_size, width, height, thumb_width, thumb_height, duration_ms,
                video_codec, audio_codec, lifecycle_state, generation, version,
                variants_ready_version, created_at, COALESCE(updated_at, created_at) AS updated_at
         FROM media_assets WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?)
}

#[allow(dead_code)]
pub async fn list(db: &SqlitePool, limit: i64, offset: i64) -> ApiResult<Vec<MediaAsset>> {
    list_filtered(db, None, limit, offset).await
}

pub async fn list_filtered(
    db: &SqlitePool,
    kind: Option<&str>,
    limit: i64,
    offset: i64,
) -> ApiResult<Vec<MediaAsset>> {
    if let Some(kind) = kind {
        Ok(sqlx::query_as::<_, MediaAsset>(
            "SELECT id, COALESCE(content_sha256, id) AS content_sha256, kind, filename, mime_type,
                    extension, byte_size, width, height, thumb_width, thumb_height, duration_ms,
                    video_codec, audio_codec, lifecycle_state, generation, version,
                    variants_ready_version, created_at, COALESCE(updated_at, created_at) AS updated_at
             FROM media_assets
             WHERE kind = ? AND lifecycle_state = 'active'
             ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
        )
        .bind(kind)
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await?)
    } else {
        Ok(sqlx::query_as::<_, MediaAsset>(
            "SELECT id, COALESCE(content_sha256, id) AS content_sha256, kind, filename, mime_type,
                    extension, byte_size, width, height, thumb_width, thumb_height, duration_ms,
                    video_codec, audio_codec, lifecycle_state, generation, version,
                    variants_ready_version, created_at, COALESCE(updated_at, created_at) AS updated_at
             FROM media_assets
             WHERE lifecycle_state = 'active'
             ORDER BY created_at DESC, id DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await?)
    }
}

#[allow(dead_code)]
pub async fn save(db: &SqlitePool, asset: &MediaAsset) -> ApiResult<MediaAsset> {
    let content_sha256 = if asset.content_sha256.is_empty() {
        asset.id.as_str()
    } else {
        asset.content_sha256.as_str()
    };

    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    let existing_state: Option<String> =
        sqlx::query_scalar("SELECT lifecycle_state FROM media_assets WHERE id = ?")
            .bind(&asset.id)
            .fetch_optional(&mut *tx)
            .await?;

    if let Some(state) = existing_state {
        if state != "active" {
            return Err(ApiError::conflict(
                "Media đang trong quá trình xóa; không thể upload lại cho đến khi hoàn tất.",
            ));
        }
        tx.commit().await?;
        crate::cache::SiteCacheEpoch::global().bump();
        return find(db, &asset.id)
            .await?
            .ok_or_else(|| ApiError::internal("Không thể tải media vừa lưu."));
    }

    let pending_delete: Option<String> = sqlx::query_scalar(
        "SELECT job_id FROM media_delete_jobs WHERE media_id = ? AND phase != 'finalized'",
    )
    .bind(&asset.id)
    .fetch_optional(&mut *tx)
    .await?;
    if pending_delete.is_some() {
        return Err(ApiError::conflict(
            "Media đang chờ hoàn tất xóa; không thể upload lại cho đến khi finalized.",
        ));
    }

    let generation: i64 = sqlx::query_scalar(
        "INSERT INTO media_generations (media_id, next_generation)
         VALUES (?, 2)
         ON CONFLICT(media_id) DO UPDATE SET next_generation = next_generation + 1
         RETURNING next_generation - 1",
    )
    .bind(&asset.id)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO media_assets
         (id, content_sha256, kind, filename, mime_type, extension, byte_size, width, height,
          thumb_width, thumb_height, duration_ms, video_codec, audio_codec,
          lifecycle_state, generation, version, variants_ready_version, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, 1, ?, datetime('now'))",
    )
    .bind(&asset.id)
    .bind(content_sha256)
    .bind(&asset.kind)
    .bind(&asset.filename)
    .bind(&asset.mime_type)
    .bind(&asset.extension)
    .bind(asset.byte_size)
    .bind(asset.width)
    .bind(asset.height)
    .bind(asset.thumb_width)
    .bind(asset.thumb_height)
    .bind(asset.duration_ms)
    .bind(&asset.video_codec)
    .bind(&asset.audio_codec)
    .bind(generation)
    .bind(asset.variants_ready_version)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();

    find(db, &asset.id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải media vừa lưu."))
}

/// Resolves content deduplication and allocates a non-colliding `actual_id` in `BEGIN IMMEDIATE`
/// BEFORE moving staged files to live paths (covering post-replace SHA re-uploads for both
/// images and videos). Compensates by unlinking promoted files if any write/commit fails.
pub async fn allocate_or_save_with_variants(
    db: &SqlitePool,
    store: &MediaStore,
    template: &MediaAsset,
    stage_path: &Path,
    decoded_image: Option<&DecodedMedia>,
    variants: &[GeneratedVariant],
) -> ApiResult<MediaAsset> {
    let sha = if template.content_sha256.is_empty() {
        template.id.as_str()
    } else {
        template.content_sha256.as_str()
    };

    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;

    if let Some(existing) = sqlx::query_as::<_, MediaAsset>(
        "SELECT id, COALESCE(content_sha256, id) AS content_sha256, kind, filename, mime_type,
                extension, byte_size, width, height, thumb_width, thumb_height, duration_ms,
                video_codec, audio_codec, lifecycle_state, generation, version,
                variants_ready_version, created_at, COALESCE(updated_at, created_at) AS updated_at
         FROM media_assets
         WHERE content_sha256 = ? AND lifecycle_state = 'active'
         LIMIT 1",
    )
    .bind(sha)
    .fetch_optional(&mut *tx)
    .await?
    {
        let _ = tx.rollback().await;
        remove_staging_target(stage_path).await;
        return Ok(existing);
    }

    let deleting_same_id: Option<String> = sqlx::query_scalar(
        "SELECT lifecycle_state FROM media_assets WHERE id = ? AND lifecycle_state != 'active'",
    )
    .bind(sha)
    .fetch_optional(&mut *tx)
    .await?;
    if deleting_same_id.is_some() {
        remove_staging_target(stage_path).await;
        return Err(ApiError::conflict(
            "Media đang trong quá trình xóa; không thể upload lại cho đến khi hoàn tất.",
        ));
    }

    let mut candidate_id = sha.to_string();
    let mut allocated_id = None;
    for attempt in 0..8 {
        if attempt > 0 {
            let nonce = crate::auth::generate_random_token();
            candidate_id = format!("{:x}", Sha256::digest(format!("{sha}:{nonce}").as_bytes()));
        }
        let id_taken: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM media_assets WHERE id = ? LIMIT 1")
                .bind(&candidate_id)
                .fetch_optional(&mut *tx)
                .await?;
        let pending_del: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM media_delete_jobs WHERE media_id = ? AND phase != 'finalized' LIMIT 1",
        )
        .bind(&candidate_id)
        .fetch_optional(&mut *tx)
        .await?;
        if id_taken.is_none() && pending_del.is_none() {
            allocated_id = Some(candidate_id.clone());
            break;
        }
    }
    let actual_id =
        allocated_id.ok_or_else(|| ApiError::internal("Không cấp phát được mã media duy nhất."))?;

    let promoted: Vec<PathBuf> = match decoded_image {
        Some(decoded) => {
            store
                .promote_staged_image_bundle(stage_path, &actual_id, decoded, 1)
                .await?
        }
        None => {
            store
                .promote_staged_video_file(stage_path, &actual_id)
                .await?
        }
    };

    let write_res: ApiResult<()> = async {
        let generation: i64 = sqlx::query_scalar(
            "INSERT INTO media_generations (media_id, next_generation)
             VALUES (?, 2)
             ON CONFLICT(media_id) DO UPDATE SET next_generation = next_generation + 1
             RETURNING next_generation - 1",
        )
        .bind(&actual_id)
        .fetch_one(&mut *tx)
        .await?;

        let ready_ver = if template.kind == "image" { 1 } else { 0 };
        sqlx::query(
            "INSERT INTO media_assets
             (id, content_sha256, kind, filename, mime_type, extension, byte_size, width, height,
              thumb_width, thumb_height, duration_ms, video_codec, audio_codec,
              lifecycle_state, generation, version, variants_ready_version, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, 1, ?, datetime('now'))",
        )
        .bind(&actual_id)
        .bind(sha)
        .bind(&template.kind)
        .bind(&template.filename)
        .bind(&template.mime_type)
        .bind(&template.extension)
        .bind(template.byte_size)
        .bind(template.width)
        .bind(template.height)
        .bind(template.thumb_width)
        .bind(template.thumb_height)
        .bind(template.duration_ms)
        .bind(&template.video_codec)
        .bind(&template.audio_codec)
        .bind(generation)
        .bind(ready_ver)
        .execute(&mut *tx)
        .await?;

        for v in variants {
            let rel = MediaStore::variant_relative_path(&actual_id, 1, v.spec, v.format)?;
            sqlx::query(
                "INSERT INTO media_variants
                 (media_id, version, spec, format, width, height, byte_size, relative_path, sha256)
                 VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&actual_id)
            .bind(v.spec.as_str())
            .bind(v.format.as_str())
            .bind(i64::from(v.width))
            .bind(i64::from(v.height))
            .bind(v.bytes.len() as i64)
            .bind(rel)
            .bind(&v.sha256)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
    .await;

    remove_staging_target(stage_path).await;
    if let Err(err) = write_res {
        MediaStore::remove_paths_best_effort(&promoted).await;
        return Err(err);
    }

    crate::cache::SiteCacheEpoch::global().bump();
    find(db, &actual_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải media vừa lưu."))
}

async fn remove_staging_target(stage_path: &Path) {
    if tokio::fs::metadata(stage_path)
        .await
        .is_ok_and(|m| m.is_dir())
    {
        let _ = tokio::fs::remove_dir_all(stage_path).await;
    } else {
        let _ = tokio::fs::remove_file(stage_path).await;
    }
}

pub fn valid_media_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
