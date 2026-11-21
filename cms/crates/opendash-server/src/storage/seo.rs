//! M7-S03-T01 SQLite storage for custom entry paths, ID-based redirects, and route claims.
use std::collections::BTreeMap;

use opendash_core::content::PatchField;
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use crate::{
    error::{ApiError, ApiResult},
    public_context::CanonicalUrlBuilder,
    seo::validate_custom_entry_path,
    storage::entries::ContentEntryRecord,
};

// C3 extends the legacy validator's protected namespace. Centralize the
// additional roots here so all storage claims enforce one policy.
pub fn is_reserved_route_root(root: &str) -> bool {
    matches!(root, "uploads" | "static" | "favicon.ico")
        || crate::seo::validate_custom_entry_path(&format!("/{root}")).is_err()
}

pub fn validate_c3_custom_entry_path(raw: &str) -> Result<String, &'static str> {
    let path = validate_custom_entry_path(raw)?;
    let root = path[1..].split('/').next().unwrap_or("");
    if matches!(root, "uploads" | "static" | "favicon.ico") {
        return Err("Đường dẫn dùng tiền tố hệ thống được bảo vệ.");
    }
    Ok(path)
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryRouteSnapshot {
    pub entry_version: i64,
    pub custom_path: Option<String>,
    pub public_url: Option<String>,
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

/// Public Reading Settings state. A configured Page ID is never silently
/// replaced when the Page becomes draft, private, or is deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingPageState {
    Unconfigured,
    Published(i64),
    Missing,
}

pub async fn reading_page_state(db: &SqlitePool, homepage: bool) -> ApiResult<ReadingPageState> {
    let sql = if homepage {
        "SELECT r.homepage_page_id,
                EXISTS(SELECT 1 FROM content_entries e
                    JOIN content_types t ON t.key=e.content_type_key
                    WHERE e.id=r.homepage_page_id AND e.content_type_key='page'
                    AND e.publication_status='published' AND t.provider_status='active'
                    AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1)
           FROM site_reading_settings r WHERE r.id=1"
    } else {
        "SELECT r.posts_page_id,
                EXISTS(SELECT 1 FROM content_entries e
                    JOIN content_types t ON t.key=e.content_type_key
                    WHERE e.id=r.posts_page_id AND e.content_type_key='page'
                    AND e.publication_status='published' AND t.provider_status='active'
                    AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1)
           FROM site_reading_settings r WHERE r.id=1"
    };
    let (page_id, published): (Option<i64>, i64) = sqlx::query_as(sql).fetch_one(db).await?;
    Ok(match page_id {
        None => ReadingPageState::Unconfigured,
        Some(id) if published != 0 => ReadingPageState::Published(id),
        Some(_) => ReadingPageState::Missing,
    })
}

/// Resolves the canonical public path for an entry ID (custom path if set, else default `/content/{type}/{slug}`).
/// Returns `None` if the entry does not exist or its CPT provider is inactive.
pub async fn resolve_entry_path_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
) -> ApiResult<Option<String>> {
    crate::storage::permalinks::canonical_entry_path_in_tx(conn, entry_id).await
}

/// Resolves the canonical public path ONLY if the entry is published and its CPT is active.
pub async fn resolve_public_entry_path_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
) -> ApiResult<Option<String>> {
    let visible: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM content_entries e
           JOIN content_types t ON t.key=e.content_type_key
           WHERE e.id=? AND e.publication_status='published'
             AND e.content_type_key!='pattern' AND t.provider_status='active'
             AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1)",
    )
    .bind(entry_id)
    .fetch_one(&mut *conn)
    .await?;
    if visible == 0 {
        return Ok(None);
    }
    crate::storage::permalinks::canonical_entry_path_in_tx(conn, entry_id).await
}

pub async fn resolve_public_entry_paths_by_ids(
    db: &SqlitePool,
    entries: &[ContentEntryRecord],
) -> ApiResult<BTreeMap<i64, String>> {
    let mut out = BTreeMap::new();
    if entries.is_empty() {
        return Ok(out);
    }
    let mut conn = db.acquire().await?;
    for e in entries {
        // Even a stale caller must not construct a public URL for a private CPT.
        // Only resolve the bounded page supplied by the caller, never scan every
        // custom path in the database.
        if let Some(url) = resolve_public_entry_path_in_tx(&mut conn, e.id).await? {
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
    let path = validate_c3_custom_entry_path(raw_path).map_err(ApiError::bad_request)?;
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
        return Err(ApiError::conflict(
            "Đường dẫn tùy chỉnh đã được bài viết khác sử dụng.",
        ));
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

    let claim_owner: Option<(String, String)> =
        sqlx::query_as("SELECT target_kind,target_id FROM permalink_route_claims WHERE path=?")
            .bind(&path)
            .fetch_optional(&mut *conn)
            .await?;
    if let Some((kind, id)) = claim_owner
        && (kind != "entry" || id.parse::<i64>().ok() != owner_entry_id)
    {
        return Err(ApiError::conflict(format!(
            "permalink_collision:path={path} existing={kind}:{id}",
        )));
    }
    let historical_owner: Option<(String, String)> = sqlx::query_as(
        "SELECT target_kind,target_id FROM permalink_route_redirects WHERE source_path=?",
    )
    .bind(&path)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some((kind, id)) = historical_owner
        && (kind != "entry" || id.parse::<i64>().ok() != owner_entry_id)
    {
        return Err(ApiError::conflict(format!(
            "permalink_collision:path={path} redirect_owner={kind}:{id}",
        )));
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
        sqlx::query("INSERT OR IGNORE INTO entry_redirects (source_path, entry_id) VALUES (?, ?)")
            .bind(&old_default)
            .bind(entry_id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// Sets or clears `entry_public_paths` inside an active transaction, enforcing ENTRY CAS
/// and cross-table uniqueness, and capturing old custom path in `entry_redirects`.
async fn set_entry_route_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    expected_entry_version: i64,
    custom_path: Option<&str>,
) -> ApiResult<EntryRouteSnapshot> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT version FROM content_entries WHERE id = ?")
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
            public_url: None,
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
        public_url: None,
    })
}

pub async fn get_entry_route(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<EntryRouteSnapshot> {
    // Both the entry token and canonical claim must come from one SQLite
    // snapshot. A separate pool query could mix a stale token with a new URL.
    let mut tx = db.begin().await?;
    let row: Option<(i64, Option<String>)> = sqlx::query_as(
        "SELECT e.version, p.path
           FROM content_entries e
           LEFT JOIN entry_public_paths p ON p.entry_id = e.id
          WHERE e.content_type_key = ? AND e.id = ?",
    )
    .bind(type_key)
    .bind(entry_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((entry_version, custom_path)) = row else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };
    let public_url = resolve_public_entry_path_in_tx(&mut tx, entry_id).await?;
    tx.commit().await?;
    Ok(EntryRouteSnapshot {
        entry_version,
        custom_path,
        public_url,
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
        let mut result =
            set_entry_route_in_tx(&mut conn, entry_id, expected_entry_version, custom_path).await?;
        crate::storage::permalinks::sync_claims_in_tx(&mut conn).await?;
        // Resolve only after the canonical claims have been synchronized. The
        // response remains within this writer transaction and matches its ETag.
        result.public_url = resolve_public_entry_path_in_tx(&mut conn, entry_id).await?;
        Ok(result)
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
            AND e.content_type_key != 'pattern'
            AND t.provider_status = 'active'
            AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
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
                AND e.content_type_key != 'pattern'
                AND t.provider_status = 'active'
                AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
        )
        .bind(path)
        .fetch_optional(db)
        .await?;
        if let Some(eid) = direct_id
            && let Some(record) =
                crate::storage::public_content::get_published_by_id(db, eid).await?
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
            AND e.content_type_key != 'pattern'
            AND t.provider_status = 'active'
            AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
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

use crate::seo::{
    EntrySeoSnapshot, HomeSeoSnapshot, SeoMetadataInput, SeoView, validate_seo_input,
};
use crate::storage::settings::{self, absolute_public_url};

pub async fn sync_entry_seo_media_ref_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
) -> ApiResult<()> {
    sqlx::query(
        "DELETE FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id = ?",
    )
    .bind(entry_id)
    .execute(&mut *conn)
    .await?;
    let media_id: Option<String> = sqlx::query_scalar(
        "SELECT s.og_image_id
           FROM entry_seo s
           JOIN content_entries e ON e.id = s.entry_id
           JOIN content_types t ON t.key = e.content_type_key
          WHERE s.entry_id = ?
            AND e.publication_status = 'published'
            AND t.provider_status = 'active'
            AND e.content_type_key != 'pattern'
            AND s.og_image_id IS NOT NULL",
    )
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?
    .flatten();
    if let Some(mid) = media_id {
        sqlx::query(
            "INSERT OR IGNORE INTO seo_public_media_refs (source_kind, source_id, media_id)
             VALUES ('entry_seo', ?, ?)",
        )
        .bind(entry_id)
        .bind(mid)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub(crate) async fn verify_og_media_exists_in_tx(
    conn: &mut SqliteConnection,
    og_image_id: Option<&str>,
) -> ApiResult<()> {
    let Some(mid) = og_image_id else {
        return Ok(());
    };
    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM media_assets WHERE id = ? AND kind = 'image' AND lifecycle_state = 'active'")
            .bind(mid)
            .fetch_optional(&mut *conn)
            .await?;
    if exists.is_none() {
        return Err(ApiError::unprocessable(
            "Ảnh OG không tồn tại trong thư viện media.",
        ));
    }
    Ok(())
}

pub async fn get_entry_seo(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<EntrySeoSnapshot> {
    let entry_version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(entry_id)
    .fetch_optional(db)
    .await?;
    let Some(entry_version) = entry_version else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };
    let row: Option<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    )> = sqlx::query_as(
        "SELECT title, description, og_image_id, canonical_url, noindex
               FROM entry_seo WHERE entry_id = ?",
    )
    .bind(entry_id)
    .fetch_optional(db)
    .await?;
    let (title, description, og_image_id, canonical_url, noindex) =
        row.unwrap_or((None, None, None, None, 0));
    Ok(EntrySeoSnapshot {
        entry_version,
        title,
        description,
        og_image_id,
        canonical_url,
        noindex: noindex != 0,
    })
}

pub async fn put_entry_seo(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
    input: SeoMetadataInput,
) -> ApiResult<EntrySeoSnapshot> {
    let clean = validate_seo_input(&input)?;
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res = async {
        let current_ver: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM content_entries WHERE content_type_key = ? AND id = ?",
        )
        .bind(type_key)
        .bind(entry_id)
        .fetch_optional(&mut *conn)
        .await?;
        let Some(current_ver) = current_ver else {
            return Err(ApiError::not_found("Không tìm thấy nội dung."));
        };
        if current_ver != expected_version {
            return Err(ApiError::precondition_failed(
                "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
            ));
        }
        verify_og_media_exists_in_tx(&mut conn, clean.og_image_id.as_deref()).await?;

        let bumped = sqlx::query(
            "UPDATE content_entries
                SET version = version + 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
              WHERE id = ? AND version = ?",
        )
        .bind(entry_id)
        .bind(expected_version)
        .execute(&mut *conn)
        .await?;
        if bumped.rows_affected() == 0 {
            return Err(ApiError::precondition_failed(
                "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
            ));
        }

        sqlx::query(
            "INSERT INTO entry_seo (entry_id, title, description, og_image_id, canonical_url, noindex)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(entry_id) DO UPDATE SET
                title = excluded.title,
                description = excluded.description,
                og_image_id = excluded.og_image_id,
                canonical_url = excluded.canonical_url,
                noindex = excluded.noindex",
        )
        .bind(entry_id)
        .bind(&clean.title)
        .bind(&clean.description)
        .bind(&clean.og_image_id)
        .bind(&clean.canonical_url)
        .bind(i64::from(clean.noindex))
        .execute(&mut *conn)
        .await?;

        sync_entry_seo_media_ref_in_tx(&mut conn, entry_id).await?;

        Ok(EntrySeoSnapshot {
            entry_version: expected_version + 1,
            title: clean.title,
            description: clean.description,
            og_image_id: clean.og_image_id,
            canonical_url: clean.canonical_url,
            noindex: clean.noindex,
        })
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

pub async fn get_home_seo(db: &SqlitePool) -> ApiResult<HomeSeoSnapshot> {
    let row: (
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    ) = sqlx::query_as(
        "SELECT version, title, description, og_image_id, canonical_url, noindex
               FROM site_home_seo WHERE id = 1",
    )
    .fetch_one(db)
    .await?;
    Ok(HomeSeoSnapshot {
        version: row.0,
        title: row.1,
        description: row.2,
        og_image_id: row.3,
        canonical_url: row.4,
        noindex: row.5 != 0,
    })
}

pub async fn put_home_seo(
    db: &SqlitePool,
    expected_version: i64,
    input: SeoMetadataInput,
) -> ApiResult<HomeSeoSnapshot> {
    let clean = validate_seo_input(&input)?;
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res = async {
        verify_og_media_exists_in_tx(&mut conn, clean.og_image_id.as_deref()).await?;
        let updated = sqlx::query(
            "UPDATE site_home_seo
                SET title = ?, description = ?, og_image_id = ?, canonical_url = ?, noindex = ?,
                    version = version + 1
              WHERE id = 1 AND version = ?",
        )
        .bind(&clean.title)
        .bind(&clean.description)
        .bind(&clean.og_image_id)
        .bind(&clean.canonical_url)
        .bind(i64::from(clean.noindex))
        .bind(expected_version)
        .execute(&mut *conn)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(ApiError::precondition_failed(
                "Phiên bản SEO trang chủ đã thay đổi. Hãy tải lại trước khi lưu.",
            ));
        }

        sqlx::query(
            "DELETE FROM seo_public_media_refs WHERE source_kind = 'home_seo' AND source_id = 1",
        )
        .execute(&mut *conn)
        .await?;
        if let Some(ref mid) = clean.og_image_id {
            sqlx::query(
                "INSERT OR IGNORE INTO seo_public_media_refs (source_kind, source_id, media_id)
                 VALUES ('home_seo', 1, ?)",
            )
            .bind(mid)
            .execute(&mut *conn)
            .await?;
        }

        Ok(HomeSeoSnapshot {
            version: expected_version + 1,
            title: clean.title,
            description: clean.description,
            og_image_id: clean.og_image_id,
            canonical_url: clean.canonical_url,
            noindex: clean.noindex,
        })
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

fn project_og_image_url(
    pub_settings: &settings::PublicSiteSettings,
    media_id: Option<String>,
) -> Option<String> {
    let mid = media_id?;
    let rel = format!("/media/{mid}/file");
    Some(absolute_public_url(pub_settings, &rel).unwrap_or(rel))
}

pub async fn build_entry_seo_view(
    db: &SqlitePool,
    entry_id: i64,
    entry_title: &str,
    canonical_path: &str,
) -> ApiResult<SeoView> {
    let general = settings::get_general(db).await?;
    let pub_settings = settings::get_public(db).await?;
    let row: Option<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    )> = sqlx::query_as(
        "SELECT title, description, og_image_id, canonical_url, noindex
               FROM entry_seo WHERE entry_id = ?",
    )
    .bind(entry_id)
    .fetch_optional(db)
    .await?;
    let (custom_title, description, og_image_id, custom_canonical, entry_noindex) =
        row.unwrap_or((None, None, None, None, 0));
    let title = custom_title.unwrap_or_else(|| format!("{entry_title} — {}", general.site_title));
    let canonical_url =
        custom_canonical.or_else(|| absolute_public_url(&pub_settings, canonical_path));
    let og_image_url = if og_image_id.is_some() {
        let public_mid: Option<String> = sqlx::query_scalar(
            "SELECT media_id FROM seo_public_media_refs
              WHERE source_kind = 'entry_seo' AND source_id = ? LIMIT 1",
        )
        .bind(entry_id)
        .fetch_optional(db)
        .await?;
        project_og_image_url(&pub_settings, public_mid)
    } else {
        None
    };
    let robots = if !pub_settings.indexing_enabled || entry_noindex != 0 {
        "noindex,nofollow".to_string()
    } else {
        "index,follow".to_string()
    };
    Ok(SeoView {
        lang: pub_settings.content_language.clone(),
        title: title.clone(),
        description: description.clone(),
        canonical_url: canonical_url.clone(),
        robots,
        og_title: title,
        og_description: description,
        og_url: canonical_url,
        og_image_url,
    })
}

pub async fn build_home_seo_view(db: &SqlitePool) -> ApiResult<SeoView> {
    let general = settings::get_general(db).await?;
    let pub_settings = settings::get_public(db).await?;
    let home = get_home_seo(db).await?;
    let mut title = home.title.unwrap_or_else(|| {
        if general.tagline.trim().is_empty() {
            general.site_title.clone()
        } else {
            format!("{} — {}", general.site_title, general.tagline)
        }
    });
    let mut description = home.description;
    let mut canonical_url = home.canonical_url;
    let mut og_image_url = project_og_image_url(&pub_settings, home.og_image_id);
    let mut noindex = home.noindex;
    if let ReadingPageState::Published(id) = reading_page_state(db, true).await? {
        let entry_meta: Option<(
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
        )> = sqlx::query_as(
            "SELECT title, description, og_image_id, canonical_url, noindex
                   FROM entry_seo WHERE entry_id = ?",
        )
        .bind(id)
        .fetch_optional(db)
        .await?;
        if let Some((entry_title, entry_description, entry_image, entry_canonical, entry_noindex)) =
            entry_meta
        {
            if let Some(entry_title) = entry_title {
                title = entry_title;
            }
            description = entry_description.or(description);
            canonical_url = entry_canonical.or(canonical_url);
            if entry_image.is_some() {
                let media_id: Option<String> = sqlx::query_scalar(
                    "SELECT media_id FROM seo_public_media_refs
                      WHERE source_kind = 'entry_seo' AND source_id = ? LIMIT 1",
                )
                .bind(id)
                .fetch_optional(db)
                .await?;
                og_image_url = project_og_image_url(&pub_settings, media_id);
            }
            noindex = entry_noindex != 0;
        }
    }
    let canonical_url = canonical_url.or_else(|| absolute_public_url(&pub_settings, "/"));
    let robots = if !pub_settings.indexing_enabled || noindex {
        "noindex,nofollow".to_string()
    } else {
        "index,follow".to_string()
    };
    Ok(SeoView {
        lang: pub_settings.content_language.clone(),
        title: title.clone(),
        description: description.clone(),
        canonical_url: canonical_url.clone(),
        robots,
        og_title: title,
        og_description: description,
        og_url: canonical_url,
        og_image_url,
    })
}

pub async fn build_simple_seo_view(
    db: &SqlitePool,
    page_title: &str,
    canonical_path: Option<&str>,
    noindex_override: bool,
) -> ApiResult<SeoView> {
    let general = settings::get_general(db).await?;
    let pub_settings = settings::get_public(db).await?;
    let title = format!("{page_title} — {}", general.site_title);
    let canonical_url = canonical_path.and_then(|p| absolute_public_url(&pub_settings, p));
    let robots = if noindex_override || !pub_settings.indexing_enabled {
        "noindex,nofollow".to_string()
    } else {
        "index,follow".to_string()
    };
    Ok(SeoView {
        lang: pub_settings.content_language.clone(),
        title: title.clone(),
        description: None,
        canonical_url: canonical_url.clone(),
        robots,
        og_title: title,
        og_description: None,
        og_url: canonical_url,
        og_image_url: None,
    })
}

pub(crate) async fn apply_entry_route_patch_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    patch: &PatchField<String>,
) -> ApiResult<Option<String>> {
    let old_custom: Option<String> =
        sqlx::query_scalar("SELECT path FROM entry_public_paths WHERE entry_id = ?")
            .bind(entry_id)
            .fetch_optional(&mut *conn)
            .await?;

    let normalized_new = match patch {
        PatchField::Omitted => return Ok(old_custom),
        PatchField::Clear => None,
        PatchField::Set(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(validate_route_claim_in_tx(conn, trimmed, Some(entry_id)).await?)
            }
        }
    };

    if normalized_new == old_custom {
        return Ok(old_custom);
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

    Ok(normalized_new)
}

pub(crate) async fn apply_entry_seo_patch_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    entry_version: i64,
    patch: &PatchField<SeoMetadataInput>,
) -> ApiResult<Option<EntrySeoSnapshot>> {
    match patch {
        PatchField::Omitted => {
            let row: Option<(
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                i64,
            )> = sqlx::query_as(
                "SELECT title, description, og_image_id, canonical_url, noindex
                     FROM entry_seo WHERE entry_id = ?",
            )
            .bind(entry_id)
            .fetch_optional(&mut *conn)
            .await?;
            Ok(row.map(
                |(title, description, og_image_id, canonical_url, noindex)| EntrySeoSnapshot {
                    entry_version,
                    title,
                    description,
                    og_image_id,
                    canonical_url,
                    noindex: noindex != 0,
                },
            ))
        }
        PatchField::Clear => {
            sqlx::query(
                "DELETE FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id = ?",
            )
            .bind(entry_id)
            .execute(&mut *conn)
            .await?;
            sqlx::query("DELETE FROM entry_seo WHERE entry_id = ?")
                .bind(entry_id)
                .execute(&mut *conn)
                .await?;
            Ok(None)
        }
        PatchField::Set(raw_seo) => {
            let clean = validate_seo_input(raw_seo)?;
            verify_og_media_exists_in_tx(conn, clean.og_image_id.as_deref()).await?;
            sqlx::query(
                "INSERT INTO entry_seo (entry_id, title, description, og_image_id, canonical_url, noindex)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(entry_id) DO UPDATE SET
                    title = excluded.title,
                    description = excluded.description,
                    og_image_id = excluded.og_image_id,
                    canonical_url = excluded.canonical_url,
                    noindex = excluded.noindex",
            )
            .bind(entry_id)
            .bind(&clean.title)
            .bind(&clean.description)
            .bind(&clean.og_image_id)
            .bind(&clean.canonical_url)
            .bind(i64::from(clean.noindex))
            .execute(&mut *conn)
            .await?;
            sync_entry_seo_media_ref_in_tx(conn, entry_id).await?;
            Ok(Some(EntrySeoSnapshot {
                entry_version,
                title: clean.title,
                description: clean.description,
                og_image_id: clean.og_image_id,
                canonical_url: clean.canonical_url,
                noindex: clean.noindex,
            }))
        }
    }
}
