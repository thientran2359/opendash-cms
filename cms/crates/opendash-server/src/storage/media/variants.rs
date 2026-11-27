use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::Serialize;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use super::types::{
    MediaAsset, MediaAssetDetails, MediaVariantFormat, MediaVariantRow, MediaVariantSpec,
    MediaVariantView, build_responsive_srcsets,
};
use crate::{
    error::{ApiError, ApiResult},
    media::{GeneratedVariant, MediaStore},
};

const VARIANT_SQL_CHUNK: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct MediaVariantsStatus {
    pub total_images: i64,
    pub ready_images: i64,
    pub pending_images: i64,
    pub variant_files_bytes: i64,
    pub active_run_id: Option<String>,
}

fn url_prefix(admin_urls: bool) -> &'static str {
    if admin_urls {
        "/api/admin/media"
    } else {
        "/media"
    }
}

pub fn variant_row_to_view(row: &MediaVariantRow, admin_urls: bool) -> Option<MediaVariantView> {
    let spec = MediaVariantSpec::parse(&row.spec)?;
    let format = MediaVariantFormat::parse_db(&row.format)?;
    let prefix = url_prefix(admin_urls);
    Some(MediaVariantView {
        spec,
        format,
        width: u32::try_from(row.width).ok()?,
        height: u32::try_from(row.height).ok()?,
        byte_size: u64::try_from(row.byte_size).ok()?,
        url: format!(
            "{prefix}/{}/variant/{}.{}?v={}",
            row.media_id,
            spec.as_str(),
            format.extension(),
            row.version
        ),
    })
}

pub fn build_details_from_parts(
    asset: MediaAsset,
    rows: &[MediaVariantRow],
    admin_urls: bool,
) -> MediaAssetDetails {
    let prefix = url_prefix(admin_urls);
    let variants: Vec<MediaVariantView> = rows
        .iter()
        .filter(|r| r.version == asset.version)
        .filter_map(|r| variant_row_to_view(r, admin_urls))
        .collect();

    let (webp_srcset, fallback_srcset) = if asset.kind == "image" {
        let orig_w = u32::try_from(asset.width).unwrap_or(0);
        let file_url = format!("{prefix}/{}/file?v={}", asset.id, asset.version);
        build_responsive_srcsets(&asset.mime_type, orig_w, &file_url, &variants)
    } else {
        (None, None)
    };

    let etag = format!("\"{}\"", asset.metadata_etag());
    MediaAssetDetails {
        asset,
        variants,
        webp_srcset,
        fallback_srcset,
        etag,
    }
}

pub async fn list_variants(
    db: &SqlitePool,
    media_id: &str,
    version: i64,
) -> ApiResult<Vec<MediaVariantRow>> {
    Ok(sqlx::query_as::<_, MediaVariantRow>(
        "SELECT media_id, version, spec, format, width, height, byte_size, relative_path, sha256, created_at
         FROM media_variants
         WHERE media_id = ? AND version = ?
         ORDER BY spec ASC, format ASC",
    )
    .bind(media_id)
    .bind(version)
    .fetch_all(db)
    .await?)
}

pub async fn find_variant(
    db: &SqlitePool,
    media_id: &str,
    version: i64,
    spec: MediaVariantSpec,
    format: MediaVariantFormat,
) -> ApiResult<Option<MediaVariantRow>> {
    Ok(sqlx::query_as::<_, MediaVariantRow>(
        "SELECT media_id, version, spec, format, width, height, byte_size, relative_path, sha256, created_at
         FROM media_variants
         WHERE media_id = ? AND version = ? AND spec = ? AND format = ?",
    )
    .bind(media_id)
    .bind(version)
    .bind(spec.as_str())
    .bind(format.as_str())
    .fetch_optional(db)
    .await?)
}

pub async fn find_details(
    db: &SqlitePool,
    media_id: &str,
    admin_urls: bool,
) -> ApiResult<Option<MediaAssetDetails>> {
    let Some(asset) = super::queries::find(db, media_id).await? else {
        return Ok(None);
    };
    let rows = if asset.kind == "image" {
        list_variants(db, &asset.id, asset.version).await?
    } else {
        Vec::new()
    };
    Ok(Some(build_details_from_parts(asset, &rows, admin_urls)))
}

#[allow(dead_code)]
pub async fn find_many_details_for_ids(
    db: &SqlitePool,
    ids: &[String],
    admin_urls: bool,
) -> ApiResult<BTreeMap<String, MediaAssetDetails>> {
    let unique: Vec<String> = ids
        .iter()
        .filter(|id| super::queries::valid_media_id(id))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if unique.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut assets = Vec::new();
    for chunk in unique.chunks(VARIANT_SQL_CHUNK) {
        let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT id, COALESCE(content_sha256, id) AS content_sha256, kind, filename, mime_type,
                    extension, byte_size, width, height, thumb_width, thumb_height, duration_ms,
                    video_codec, audio_codec, lifecycle_state, generation, version,
                    variants_ready_version, created_at, COALESCE(updated_at, created_at) AS updated_at
             FROM media_assets
             WHERE lifecycle_state = 'active' AND id IN (",
        );
        let mut separated = qb.separated(", ");
        for id in chunk {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
        assets.extend(qb.build_query_as::<MediaAsset>().fetch_all(db).await?);
    }

    let image_ids: Vec<String> = assets
        .iter()
        .filter(|a| a.kind == "image")
        .map(|a| a.id.clone())
        .collect();

    let mut variants_by_media: BTreeMap<String, Vec<MediaVariantRow>> = BTreeMap::new();
    for chunk in image_ids.chunks(VARIANT_SQL_CHUNK) {
        let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT v.media_id, v.version, v.spec, v.format, v.width, v.height,
                    v.byte_size, v.relative_path, v.sha256, v.created_at
             FROM media_variants v
             INNER JOIN media_assets a ON a.id = v.media_id AND a.version = v.version
             WHERE a.lifecycle_state = 'active' AND v.media_id IN (",
        );
        let mut separated = qb.separated(", ");
        for id in chunk {
            separated.push_bind(id);
        }
        separated.push_unseparated(") ORDER BY v.spec ASC, v.format ASC");
        for row in qb.build_query_as::<MediaVariantRow>().fetch_all(db).await? {
            variants_by_media
                .entry(row.media_id.clone())
                .or_default()
                .push(row);
        }
    }

    let mut out = BTreeMap::new();
    for asset in assets {
        let rows = variants_by_media.remove(&asset.id).unwrap_or_default();
        let id = asset.id.clone();
        out.insert(id, build_details_from_parts(asset, &rows, admin_urls));
    }
    Ok(out)
}

pub async fn get_variants_status(db: &SqlitePool) -> ApiResult<MediaVariantsStatus> {
    let counts = sqlx::query(
        "SELECT
             COUNT(*) AS total_images,
             COALESCE(SUM(CASE WHEN variants_ready_version >= version THEN 1 ELSE 0 END), 0) AS ready_images,
             COALESCE(SUM(CASE WHEN variants_ready_version < version THEN 1 ELSE 0 END), 0) AS pending_images
         FROM media_assets
         WHERE kind = 'image' AND lifecycle_state = 'active'",
    )
    .fetch_one(db)
    .await?;

    let variant_files_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(v.byte_size), 0)
         FROM media_variants v
         INNER JOIN media_assets a ON a.id = v.media_id AND a.version = v.version
         WHERE a.lifecycle_state = 'active'",
    )
    .fetch_one(db)
    .await?;

    let active_run_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM operation_jobs
         WHERE kind = 'media_regenerate' AND state = 'running'
         ORDER BY requested_at DESC, rowid DESC LIMIT 1",
    )
    .fetch_optional(db)
    .await?;

    Ok(MediaVariantsStatus {
        total_images: counts.try_get("total_images")?,
        ready_images: counts.try_get("ready_images")?,
        pending_images: counts.try_get("pending_images")?,
        variant_files_bytes,
        active_run_id,
    })
}

pub async fn list_images_for_regenerate(
    db: &SqlitePool,
    after_id: Option<&str>,
    limit: i64,
    force: bool,
) -> ApiResult<Vec<MediaAsset>> {
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT id, COALESCE(content_sha256, id) AS content_sha256, kind, filename, mime_type,
                extension, byte_size, width, height, thumb_width, thumb_height, duration_ms,
                video_codec, audio_codec, lifecycle_state, generation, version,
                variants_ready_version, created_at, COALESCE(updated_at, created_at) AS updated_at
         FROM media_assets
         WHERE kind = 'image' AND lifecycle_state = 'active'",
    );
    if !force {
        qb.push(" AND variants_ready_version < version");
    }
    if let Some(cursor) = after_id {
        qb.push(" AND id > ");
        qb.push_bind(cursor);
    }
    qb.push(" ORDER BY id ASC LIMIT ");
    qb.push_bind(limit);
    Ok(qb.build_query_as::<MediaAsset>().fetch_all(db).await?)
}

pub async fn count_remaining_for_regenerate(
    db: &SqlitePool,
    after_id: Option<&str>,
    force: bool,
) -> ApiResult<i64> {
    let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT COUNT(*) FROM media_assets WHERE kind = 'image' AND lifecycle_state = 'active'",
    );
    if !force {
        qb.push(" AND variants_ready_version < version");
    }
    if let Some(cursor) = after_id {
        qb.push(" AND id > ");
        qb.push_bind(cursor);
    }
    let count: i64 = qb.build_query_scalar().fetch_one(db).await?;
    Ok(count)
}

pub async fn commit_regenerated_variants_cas(
    db: &SqlitePool,
    media_id: &str,
    expected_generation: i64,
    expected_version: i64,
    variants: &[GeneratedVariant],
) -> ApiResult<bool> {
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    let updated = sqlx::query(
        "UPDATE media_assets
         SET variants_ready_version = ?, updated_at = datetime('now')
         WHERE id = ? AND generation = ? AND version = ? AND lifecycle_state = 'active'",
    )
    .bind(expected_version)
    .bind(media_id)
    .bind(expected_generation)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() == 0 {
        let _ = tx.rollback().await;
        return Ok(false);
    }

    sqlx::query("DELETE FROM media_variants WHERE media_id = ?")
        .bind(media_id)
        .execute(&mut *tx)
        .await?;

    for v in variants {
        let rel = MediaStore::variant_relative_path(media_id, expected_version, v.spec, v.format)?;
        sqlx::query(
            "INSERT INTO media_variants
             (media_id, version, spec, format, width, height, byte_size, relative_path, sha256)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(media_id)
        .bind(expected_version)
        .bind(v.spec.as_str())
        .bind(v.format.as_str())
        .bind(i64::from(v.width))
        .bind(i64::from(v.height))
        .bind(i64::try_from(v.bytes.len()).unwrap_or(0))
        .bind(rel)
        .bind(&v.sha256)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(true)
}

pub async fn list_active_variant_paths_bounded(
    db: &SqlitePool,
    max_rows: usize,
) -> ApiResult<HashSet<String>> {
    let limit = (max_rows + 1) as i64;
    let paths: Vec<String> = sqlx::query_scalar(
        "SELECT mv.relative_path
         FROM media_variants mv
         INNER JOIN media_assets ma ON ma.id = mv.media_id AND mv.version = ma.version
         WHERE ma.lifecycle_state = 'active'
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(db)
    .await?;
    if paths.len() > max_rows {
        return Err(ApiError::conflict(
            "Số lượng biến thể media vượt giới hạn reconcile.",
        ));
    }
    Ok(paths.into_iter().collect())
}
