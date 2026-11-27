use std::collections::{BTreeMap, BTreeSet};

use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::types::{
    MediaAsset, MediaAssetDetails, MediaVariantFormat, MediaVariantRow, MediaVariantSpec,
    MediaVariantView, build_responsive_srcsets,
};
use crate::error::ApiResult;

const VARIANT_SQL_CHUNK: usize = 500;

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
