//! M7-S03-T01 SQLite storage for custom entry paths, ID-based redirects, and route claims.
use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use crate::{
    error::{ApiError, ApiResult},
    public_context::CanonicalUrlBuilder,
    seo::validate_custom_entry_path,
    storage::entries::ContentEntryRecord,
};

#[derive(Debug, Clone, Serialize)]
pub struct EntryRouteSnapshot {
    pub entry_version: i64,
    pub custom_path: Option<String>,
}

pub enum PublicRouteResolution {
    Entry {
        record: ContentEntryRecord,
        canonical_path: String,
    },
    Redirect {
        location: String,
    },
    NotFound,
}

/// Resolves the canonical public path for an entry ID (custom path if set, else default `/content/{type}/{slug}`).
/// Returns `None` if the entry does not exist or its CPT provider is inactive.
pub async fn resolve_entry_path_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
) -> ApiResult<Option<String>> {
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT e.content_type_key, e.slug, p.path
           FROM content_entries e
           JOIN content_types t ON t.key = e.content_type_key
           LEFT JOIN entry_public_paths p ON p.entry_id = e.id
          WHERE e.id = ? AND t.provider_status = 'active'",
    )
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.and_then(|(type_key, slug, custom)| {
        custom.or_else(|| CanonicalUrlBuilder::entry_from_id(entry_id, &type_key, &slug))
    }))
}

/// Resolves the canonical public path ONLY if the entry is published and its CPT is active.
pub async fn resolve_public_entry_path_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
) -> ApiResult<Option<String>> {
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT e.content_type_key, e.slug, p.path
           FROM content_entries e
           JOIN content_types t ON t.key = e.content_type_key
           LEFT JOIN entry_public_paths p ON p.entry_id = e.id
          WHERE e.id = ?
            AND e.publication_status = 'published'
            AND t.provider_status = 'active'",
    )
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.and_then(|(type_key, slug, custom)| {
        custom.or_else(|| CanonicalUrlBuilder::entry_from_id(entry_id, &type_key, &slug))
    }))
}

pub async fn resolve_public_entry_paths_by_ids(
    db: &SqlitePool,
    entries: &[ContentEntryRecord],
) -> ApiResult<BTreeMap<i64, String>> {
    let mut out = BTreeMap::new();
    if entries.is_empty() {
        return Ok(out);
    }
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT entry_id, path FROM entry_public_paths",
    )
    .fetch_all(db)
    .await?;
    let custom_map: HashMap<i64, String> = rows.into_iter().collect();
    for e in entries {
        if let Some(url) = custom_map
            .get(&e.id)
            .cloned()
            .or_else(|| CanonicalUrlBuilder::entry_from_id(e.id, &e.content_type_key, &e.slug))
        {
            out.insert(e.id, url);
        }
    }
    Ok(out)
}

/// Validates a custom path claim atomically inside an open transaction.
pub async fn validate_route_claim_in_tx(
    conn: &mut SqliteConnection,
    raw_path: &str,
    owner_entry_id: Option<i64>,
) -> ApiResult<String> {
    let path = validate_custom_entry_path(raw_path).map_err(ApiError::bad_request)?;
    let first_seg = path[1..].split('/').next().unwrap_or("");
    let cpt_conflict: Option<String> = sqlx::query_scalar(
        "SELECT key FROM content_types WHERE key = ? AND provider_status = 'active' LIMIT 1",
    )
    .bind(first_seg)
    .fetch_optional(&mut *conn)
    .await?;
    if cpt_conflict.is_some() {
        return Err(ApiError::conflict(
            "Đường dẫn trùng với gốc loại nội dung đang hoạt động.",
        ));
    }

    let existing_owner: Option<i64> =
        sqlx::query_scalar("SELECT entry_id FROM entry_public_paths WHERE path = ?")
            .bind(&path)
            .fetch_optional(&mut *conn)
            .await?;
    if let Some(eid) = existing_owner
        && Some(eid) != owner_entry_id
    {
        return Err(ApiError::conflict("Đường dẫn tùy chỉnh đã được bài viết khác sử dụng."));
    }

    let redirect_owner: Option<i64> =
        sqlx::query_scalar("SELECT entry_id FROM entry_redirects WHERE source_path = ?")
            .bind(&path)
            .fetch_optional(&mut *conn)
            .await?;
    if let Some(eid) = redirect_owner
        && Some(eid) != owner_entry_id
    {
        return Err(ApiError::conflict(
            "Đường dẫn đang được giữ làm redirect cho một bài viết khác.",
        ));
    }

    Ok(path)
}

/// Ensures creating a new CPT key does not collide with any existing custom path or redirect.
pub async fn check_cpt_root_claim_in_tx(
    conn: &mut SqliteConnection,
    type_key: &str,
) -> ApiResult<()> {
    let exact = format!("/{type_key}");
    let prefix = format!("/{type_key}/%");
    let custom_hit: Option<i64> = sqlx::query_scalar(
        "SELECT entry_id FROM entry_public_paths WHERE path = ? OR path LIKE ? LIMIT 1",
    )
    .bind(&exact)
    .bind(&prefix)
    .fetch_optional(&mut *conn)
    .await?;
    if custom_hit.is_some() {
        return Err(ApiError::conflict(
            "Khóa loại nội dung trùng với đường dẫn tùy chỉnh đang tồn tại.",
        ));
    }
    let redirect_hit: Option<i64> = sqlx::query_scalar(
        "SELECT entry_id FROM entry_redirects WHERE source_path = ? OR source_path LIKE ? LIMIT 1",
    )
    .bind(&exact)
    .bind(&prefix)
    .fetch_optional(&mut *conn)
    .await?;
    if redirect_hit.is_some() {
        return Err(ApiError::conflict(
            "Khóa loại nội dung trùng với redirect đang tồn tại.",
        ));
    }
    Ok(())
}

/// Checks whether a default `/content/{type_key}/{slug}` path is claimed by another entry's redirect.
pub async fn check_default_slug_claim_in_tx(
    conn: &mut SqliteConnection,
    type_key: &str,
    slug: &str,
    owner_entry_id: Option<i64>,
) -> ApiResult<()> {
    let Some(default_path) = CanonicalUrlBuilder::entry(type_key, slug) else {
        return Ok(());
    };
    let redirect_owner: Option<i64> =
        sqlx::query_scalar("SELECT entry_id FROM entry_redirects WHERE source_path = ?")
            .bind(&default_path)
            .fetch_optional(&mut *conn)
            .await?;
    if let Some(eid) = redirect_owner
        && Some(eid) != owner_entry_id
    {
        return Err(ApiError::conflict(
            "Slug mặc định đang được giữ làm redirect cho một bài viết khác.",
        ));
    }
    Ok(())
}

/// Records redirect from old default slug URL when an entry's slug changes (and reclaims own alias if reused).
pub async fn record_slug_change_redirect_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    type_key: &str,
    old_slug: &str,
    new_slug: &str,
) -> ApiResult<()> {
    if old_slug == new_slug {
        return Ok(());
    }
    if let Some(new_default) = CanonicalUrlBuilder::entry(type_key, new_slug) {
        check_default_slug_claim_in_tx(conn, type_key, new_slug, Some(entry_id)).await?;
        sqlx::query("DELETE FROM entry_redirects WHERE source_path = ? AND entry_id = ?")
            .bind(&new_default)
            .bind(entry_id)
            .execute(&mut *conn)
            .await?;
    }
    let has_custom: Option<String> =
        sqlx::query_scalar("SELECT path FROM entry_public_paths WHERE entry_id = ?")
            .bind(entry_id)
            .fetch_optional(&mut *conn)
            .await?;
    if has_custom.is_none()
        && let Some(old_default) = CanonicalUrlBuilder::entry(type_key, old_slug)
    {
        sqlx::query(
            "INSERT OR IGNORE INTO entry_redirects (source_path, entry_id) VALUES (?, ?)",
        )
        .bind(&old_default)
        .bind(entry_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Sets or clears `entry_public_paths` inside an active transaction, enforcing ENTRY CAS
/// and cross-table uniqueness, and capturing old custom path in `entry_redirects`.
pub async fn set_entry_route_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    expected_entry_version: i64,
    custom_path: Option<&str>,
) -> ApiResult<EntryRouteSnapshot> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT version FROM content_entries WHERE id = ?",
    )
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((current_ver,)) = row else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };
    if current_ver != expected_entry_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        ));
    }

    let normalized_new = match custom_path.map(str::trim) {
        None | Some("") => None,
        Some(raw) => Some(validate_route_claim_in_tx(conn, raw, Some(entry_id)).await?),
    };
    let old_custom: Option<String> =
        sqlx::query_scalar("SELECT path FROM entry_public_paths WHERE entry_id = ?")
            .bind(entry_id)
            .fetch_optional(&mut *conn)
            .await?;

    if normalized_new == old_custom {
        return Ok(EntryRouteSnapshot {
            entry_version: current_ver,
            custom_path: old_custom,
        });
    }

    let updated = sqlx::query(
        "UPDATE content_entries
            SET version = version + 1,
                updated_at = datetime('now')
          WHERE id = ? AND version = ?",
    )
    .bind(entry_id)
    .bind(expected_entry_version)
    .execute(&mut *conn)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi đồng thời.",
        ));
    }

    if let Some(old_path) = old_custom {
        sqlx::query("INSERT OR IGNORE INTO entry_redirects (source_path, entry_id) VALUES (?, ?)")
            .bind(&old_path)
            .bind(entry_id)
            .execute(&mut *conn)
            .await?;
    }

    match &normalized_new {
        Some(new_path) => {
            sqlx::query("DELETE FROM entry_redirects WHERE source_path = ? AND entry_id = ?")
                .bind(new_path)
                .bind(entry_id)
                .execute(&mut *conn)
                .await?;
            sqlx::query(
                "INSERT INTO entry_public_paths (entry_id, path)
                 VALUES (?, ?)
                 ON CONFLICT(entry_id) DO UPDATE SET path = excluded.path",
            )
            .bind(entry_id)
            .bind(new_path)
            .execute(&mut *conn)
            .await?;
        }
        None => {
            sqlx::query("DELETE FROM entry_public_paths WHERE entry_id = ?")
                .bind(entry_id)
                .execute(&mut *conn)
                .await?;
        }
    }

    Ok(EntryRouteSnapshot {
        entry_version: expected_entry_version + 1,
        custom_path: normalized_new,
    })
}

pub async fn get_entry_route(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<EntryRouteSnapshot> {
    let row: Option<(i64, Option<String>)> = sqlx::query_as(
        "SELECT e.version, p.path
           FROM content_entries e
           LEFT JOIN entry_public_paths p ON p.entry_id = e.id
          WHERE e.content_type_key = ? AND e.id = ?",
    )
    .bind(type_key)
    .bind(entry_id)
    .fetch_optional(db)
    .await?;
    let Some((entry_version, custom_path)) = row else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };
    Ok(EntryRouteSnapshot {
        entry_version,
        custom_path,
    })
}

pub async fn put_entry_route(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
    expected_entry_version: i64,
    custom_path: Option<&str>,
) -> ApiResult<EntryRouteSnapshot> {
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res = async {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM content_entries WHERE content_type_key = ? AND id = ?",
        )
        .bind(type_key)
        .bind(entry_id)
        .fetch_optional(&mut *conn)
        .await?;
        if exists.is_none() {
            return Err(ApiError::not_found("Không tìm thấy nội dung."));
        }
        set_entry_route_in_tx(&mut conn, entry_id, expected_entry_version, custom_path).await
    }
    .await;
    match res {
        Ok(out) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(out)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

/// Checks if a published entry has a custom path (used by `/content/{type}/{slug}` to emit 301).
pub async fn custom_path_for_published_slug(
    db: &SqlitePool,
    type_key: &str,
    slug: &str,
) -> ApiResult<Option<String>> {
    let custom: Option<String> = sqlx::query_scalar(
        "SELECT p.path
           FROM content_entries e
           JOIN content_types t ON t.key = e.content_type_key
           JOIN entry_public_paths p ON p.entry_id = e.id
          WHERE e.content_type_key = ?
            AND e.slug = ?
            AND e.publication_status = 'published'
            AND t.provider_status = 'active'",
    )
    .bind(type_key)
    .bind(slug)
    .fetch_optional(db)
    .await?;
    Ok(custom)
}

/// Resolves an arbitrary public fallback path against `entry_public_paths` and `entry_redirects`.
/// Never returns draft/unpublished/deleted entries or redirects to them.
pub async fn resolve_custom_or_redirect_path(
    db: &SqlitePool,
    path: &str,
) -> ApiResult<PublicRouteResolution> {
    if validate_custom_entry_path(path).is_ok() {
        let direct_id: Option<i64> = sqlx::query_scalar(
            "SELECT e.id
               FROM entry_public_paths p
               JOIN content_entries e ON e.id = p.entry_id
               JOIN content_types t ON t.key = e.content_type_key
              WHERE p.path = ?
                AND e.publication_status = 'published'
                AND t.provider_status = 'active'",
        )
        .bind(path)
        .fetch_optional(db)
        .await?;
        if let Some(eid) = direct_id
            && let Some(record) = crate::storage::public_content::get_published_by_id(db, eid).await?
        {
            return Ok(PublicRouteResolution::Entry {
                record,
                canonical_path: path.to_owned(),
            });
        }
    }

    let mut tx = db.begin().await?;
    let redirect_eid: Option<i64> = sqlx::query_scalar(
        "SELECT r.entry_id
           FROM entry_redirects r
           JOIN content_entries e ON e.id = r.entry_id
           JOIN content_types t ON t.key = e.content_type_key
          WHERE r.source_path = ?
            AND e.publication_status = 'published'
            AND t.provider_status = 'active'",
    )
    .bind(path)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(eid) = redirect_eid
        && let Some(target) = resolve_public_entry_path_in_tx(&mut tx, eid).await?
        && target != path
    {
        tx.commit().await?;
        return Ok(PublicRouteResolution::Redirect { location: target });
    }
    tx.commit().await?;
    Ok(PublicRouteResolution::NotFound)
}
