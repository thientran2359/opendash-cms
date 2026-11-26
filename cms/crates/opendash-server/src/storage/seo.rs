//! M7-S03-T01 SQLite storage for custom entry paths, ID-based redirects, and route claims.
use std::collections::BTreeMap;

use opendash_core::content::PatchField;
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use crate::{
    error::{ApiError, ApiResult},
    seo::validate_custom_entry_path,
    storage::entries::ContentEntryRecord,
};

// Delegate protected root checks to seo::is_reserved_system_root as single source of truth.
pub fn is_reserved_route_root(root: &str) -> bool {
    crate::seo::is_reserved_system_root(root)
}

pub fn validate_c3_custom_entry_path(raw: &str) -> Result<String, &'static str> {
    validate_custom_entry_path(raw)
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
    reading_page_state_for_locale(db, None, homepage).await
}

pub async fn reading_page_state_for_locale(
    db: &SqlitePool,
    locale: Option<&str>,
    homepage: bool,
) -> ApiResult<ReadingPageState> {
    let col = if homepage {
        "homepage_page_id"
    } else {
        "posts_page_id"
    };
    let row: Option<(Option<i64>, i64)> = match locale {
        Some(loc) => {
            let sql = format!(
                "SELECT r.{col},
                        EXISTS(SELECT 1 FROM content_entries e
                            JOIN content_types t ON t.key=e.content_type_key
                            WHERE e.id=r.{col} AND e.locale=r.locale AND e.content_type_key='page'
                            AND e.publication_status='published' AND t.provider_status='active'
                            AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1)
                   FROM site_reading_settings r
                   JOIN site_locales sl ON sl.code = r.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                  WHERE r.locale = ?",
            );
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .bind(loc)
                .fetch_optional(db)
                .await?
        }
        None => {
            let sql = format!(
                "SELECT r.{col},
                        EXISTS(SELECT 1 FROM content_entries e
                            JOIN content_types t ON t.key=e.content_type_key
                            WHERE e.id=r.{col} AND e.locale=r.locale AND e.content_type_key='page'
                            AND e.publication_status='published' AND t.provider_status='active'
                            AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1)
                   FROM site_reading_settings r
                   JOIN site_locales sl ON sl.code = r.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                  WHERE sl.is_default = 1",
            );
            sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .fetch_optional(db)
                .await?
        }
    };
    let Some((page_id, published)) = row else {
        return Ok(ReadingPageState::Unconfigured);
    };
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
             AND e.content_type_key NOT IN ('pattern', 'section')
             AND t.provider_status='active'
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

pub async fn resolve_public_entry_paths_by_entry_ids_in_conn(
    conn: &mut SqliteConnection,
    ids: &[i64],
) -> ApiResult<BTreeMap<i64, String>> {
    let mut out = BTreeMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    for chunk in ids.chunks(500) {
        let ids_json = serde_json::to_string(chunk)?;
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT e.id, c.path
               FROM content_entries e
               JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
               JOIN content_types t ON t.key = e.content_type_key
               JOIN permalink_route_claims c
                 ON c.target_kind = 'entry'
                AND c.target_id = CAST(e.id AS TEXT)
                AND c.locale = e.locale
              WHERE e.id IN (SELECT value FROM json_each(?))
                AND e.publication_status = 'published'
                AND e.content_type_key NOT IN ('pattern', 'section')
                AND t.provider_status = 'active'
                AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
        )
        .bind(ids_json)
        .fetch_all(&mut *conn)
        .await?;
        for (eid, path) in rows {
            out.insert(eid, path);
        }
    }
    Ok(out)
}

pub async fn resolve_public_entry_paths_by_ids(
    db: &SqlitePool,
    entries: &[ContentEntryRecord],
) -> ApiResult<BTreeMap<i64, String>> {
    if entries.is_empty() {
        return Ok(BTreeMap::new());
    }
    let ids: Vec<i64> = entries.iter().map(|e| e.id).collect();
    let mut conn = db.acquire().await?;
    resolve_public_entry_paths_by_entry_ids_in_conn(&mut conn, &ids).await
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
        "SELECT key FROM content_types
          WHERE key = ?
            AND key NOT IN ('post', 'page', 'pattern', 'section')
            AND provider_status = 'active'
            AND COALESCE(json_extract(definition_json, '$.settings.public'), 1) = 1
          LIMIT 1",
    )
    .bind(first_seg)
    .fetch_optional(&mut *conn)
    .await?;
    if cpt_conflict.is_some() {
        return Err(ApiError::conflict(
            "Đường dẫn trùng với gốc loại nội dung đang hoạt động.",
        ));
    }

    let taxonomy_conflict: Option<String> = sqlx::query_scalar(
        "SELECT key FROM taxonomy_definitions
          WHERE key = ? AND public = 1 AND provider_status = 'active' AND declaration_state = 'declared'
          LIMIT 1",
    )
    .bind(first_seg)
    .fetch_optional(&mut *conn)
    .await?;
    if taxonomy_conflict.is_some() {
        return Err(ApiError::conflict(
            "Đường dẫn trùng với gốc taxonomy đang hoạt động.",
        ));
    }

    let rewrite_conflict: Option<String> = sqlx::query_scalar(
        "SELECT resource_key FROM provider_rewrite_defaults
          WHERE rewrite_base = ? AND provider_status = 'active' AND declaration_state = 'declared'
          LIMIT 1",
    )
    .bind(first_seg)
    .fetch_optional(&mut *conn)
    .await?;
    if rewrite_conflict.is_some() {
        return Err(ApiError::conflict(
            "Đường dẫn trùng với tiền tố rewrite mặc định đang hoạt động.",
        ));
    }

    let entry_locale = if let Some(eid) = owner_entry_id {
        sqlx::query_scalar::<_, String>("SELECT locale FROM content_entries WHERE id = ?")
            .bind(eid)
            .fetch_optional(&mut *conn)
            .await?
    } else {
        None
    };
    let entry_locale = match entry_locale {
        Some(loc) => loc,
        None => crate::storage::settings::get_default_locale_code_in_conn(conn).await?,
    };
    let default_locale = crate::storage::settings::get_default_locale_code_in_conn(conn).await?;
    let secondary_roots =
        crate::storage::permalinks::load_secondary_locale_roots_in_conn(conn).await?;
    if entry_locale == default_locale
        && crate::storage::permalinks::is_secondary_locale_root(first_seg, &secondary_roots)
    {
        return Err(ApiError::conflict(
            "Đường dẫn trùng với tiền tố ngôn ngữ đang hoạt động.",
        ));
    }

    if path == "/posts" {
        let posts_page_id: Option<i64> =
            sqlx::query_scalar("SELECT posts_page_id FROM site_reading_settings WHERE locale = ?")
                .bind(&entry_locale)
                .fetch_optional(&mut *conn)
                .await?
                .flatten();
        if posts_page_id.is_none() {
            return Err(ApiError::conflict(
                "Đường dẫn /posts đang dành cho trang lưu trữ bài viết mặc định.",
            ));
        }
    }

    let existing_owner: Option<i64> =
        sqlx::query_scalar("SELECT entry_id FROM entry_public_paths WHERE locale = ? AND path = ?")
            .bind(&entry_locale)
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

    let redirect_owner: Option<i64> = sqlx::query_scalar(
        "SELECT entry_id FROM entry_redirects WHERE locale = ? AND source_path = ?",
    )
    .bind(&entry_locale)
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

    let effective_path =
        crate::storage::permalinks::prefix_for_locale(&entry_locale, &default_locale, &path);
    let claim_owner: Option<(String, String)> =
        sqlx::query_as("SELECT target_kind,target_id FROM permalink_route_claims WHERE path=?")
            .bind(&effective_path)
            .fetch_optional(&mut *conn)
            .await?;
    if let Some((kind, id)) = claim_owner
        && (kind != "entry" || id.parse::<i64>().ok() != owner_entry_id)
    {
        return Err(ApiError::conflict(format!(
            "permalink_collision:path={effective_path} existing={kind}:{id}",
        )));
    }
    let historical_owner: Option<(String, String)> = sqlx::query_as(
        "SELECT r.target_kind,r.target_id
           FROM permalink_route_redirects r
           JOIN site_locales sl ON sl.code = r.locale AND sl.deleted_at IS NULL
          WHERE r.source_path=? AND r.origin != 'locale_swap'",
    )
    .bind(&effective_path)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some((kind, id)) = historical_owner
        && (kind != "entry" || id.parse::<i64>().ok() != owner_entry_id)
    {
        return Err(ApiError::conflict(format!(
            "permalink_collision:path={effective_path} redirect_owner={kind}:{id}",
        )));
    }

    Ok(path)
}

/// Ensures creating a new CPT key does not collide with any existing custom path, claim, or live redirect.
pub async fn check_cpt_root_claim_in_tx(
    conn: &mut SqliteConnection,
    type_key: &str,
) -> ApiResult<()> {
    let locale_conflict: Option<String> = sqlx::query_scalar(
        "SELECT code FROM site_locales WHERE code = ? AND enabled = 1 AND deleted_at IS NULL AND is_default = 0 LIMIT 1",
    )
    .bind(type_key)
    .fetch_optional(&mut *conn)
    .await?;
    if locale_conflict.is_some() {
        return Err(ApiError::conflict(
            "Khóa loại nội dung trùng với tiền tố ngôn ngữ đang hoạt động.",
        ));
    }
    let exact = format!("/{type_key}");
    let prefix = format!("/{type_key}/%");
    let custom_hit: Option<i64> = sqlx::query_scalar(
        "SELECT p.entry_id FROM entry_public_paths p
           JOIN site_locales sl ON sl.code = p.locale AND sl.deleted_at IS NULL
          WHERE p.path = ? OR p.path LIKE ? LIMIT 1",
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
    let claim_hit: Option<String> = sqlx::query_scalar(
        "SELECT path FROM permalink_route_claims WHERE path = ? OR path LIKE ? LIMIT 1",
    )
    .bind(&exact)
    .bind(&prefix)
    .fetch_optional(&mut *conn)
    .await?;
    if claim_hit.is_some() {
        return Err(ApiError::conflict(
            "Khóa loại nội dung trùng với đường dẫn công khai đang tồn tại.",
        ));
    }
    // Only live-target redirects block new CPT namespaces; orphan redirects whose target
    // row was deleted remain exact-path redirects without blocking new namespaces.
    let redirect_hit: Option<i64> = sqlx::query_scalar(
        "SELECT r.entry_id FROM entry_redirects r
           JOIN content_entries e ON e.id = r.entry_id
           JOIN site_locales sl ON sl.code = r.locale AND sl.deleted_at IS NULL
          WHERE r.source_path = ? OR r.source_path LIKE ? LIMIT 1",
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
    let route_redirect_hit: Option<String> = sqlx::query_scalar(
        "SELECT r.target_id FROM permalink_route_redirects r
           JOIN site_locales sl ON sl.code = r.locale AND sl.deleted_at IS NULL
          WHERE (r.source_path = ? OR r.source_path LIKE ?)
            AND r.origin != 'locale_swap'
            AND (
                (r.target_kind = 'entry' AND EXISTS (SELECT 1 FROM content_entries e WHERE CAST(e.id AS TEXT) = r.target_id))
                OR (r.target_kind = 'term' AND EXISTS (SELECT 1 FROM taxonomy_terms t WHERE CAST(t.id AS TEXT) = r.target_id))
                OR r.target_kind = 'archive'
            )
          LIMIT 1",
    )
    .bind(&exact)
    .bind(&prefix)
    .fetch_optional(&mut *conn)
    .await?;
    if route_redirect_hit.is_some() {
        return Err(ApiError::conflict(
            "Khóa loại nội dung trùng với redirect đang tồn tại.",
        ));
    }
    Ok(())
}

/// Sets or clears `entry_public_paths` inside an active transaction, enforcing ENTRY CAS
/// and cross-table uniqueness, and capturing old custom path in `entry_redirects` only if ever published.
async fn set_entry_route_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    expected_entry_version: i64,
    custom_path: Option<&str>,
) -> ApiResult<EntryRouteSnapshot> {
    let row: Option<(i64, i64, String)> = sqlx::query_as(
        "SELECT version, has_been_published, locale FROM content_entries WHERE id = ?",
    )
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((current_ver, has_been_published, entry_locale)) = row else {
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

    if has_been_published != 0
        && let Some(old_path) = old_custom
    {
        sqlx::query(
            "INSERT OR IGNORE INTO entry_redirects (locale, source_path, entry_id) VALUES (?, ?, ?)",
        )
        .bind(&entry_locale)
        .bind(&old_path)
        .bind(entry_id)
        .execute(&mut *conn)
        .await?;
    }

    match &normalized_new {
        Some(new_path) => {
            sqlx::query(
                "DELETE FROM entry_redirects WHERE locale = ? AND source_path = ? AND entry_id = ?",
            )
            .bind(&entry_locale)
            .bind(new_path)
            .bind(entry_id)
            .execute(&mut *conn)
            .await?;
            sqlx::query(
                "INSERT INTO entry_public_paths (entry_id, locale, path)
                 VALUES (?, ?, ?)
                 ON CONFLICT(entry_id) DO UPDATE SET
                     locale = excluded.locale,
                     path = excluded.path",
            )
            .bind(entry_id)
            .bind(&entry_locale)
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
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(out)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

/// Checks if a published default-locale entry has a custom path (used by `/content/{type}/{slug}` to emit 301).
pub async fn custom_path_for_published_slug(
    db: &SqlitePool,
    type_key: &str,
    slug: &str,
) -> ApiResult<Option<String>> {
    let custom: Option<String> = sqlx::query_scalar(
        "SELECT p.path
           FROM content_entries e
           JOIN site_locales sl ON sl.code = e.locale AND sl.is_default = 1 AND sl.enabled = 1 AND sl.deleted_at IS NULL
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

/// Resolves an arbitrary public fallback path against `entry_public_paths` and `entry_redirects`
/// (scoped by active locale prefix). Never returns draft/unpublished/disabled-locale entries.
pub async fn resolve_custom_or_redirect_path(
    db: &SqlitePool,
    path: &str,
) -> ApiResult<PublicRouteResolution> {
    let mut tx = db.begin().await?;
    let default_locale = crate::storage::settings::get_default_locale_code_in_conn(&mut tx).await?;
    let secondary_roots =
        crate::storage::permalinks::load_secondary_locale_roots_in_conn(&mut tx).await?;
    let first_seg = path.trim_start_matches('/').split('/').next().unwrap_or("");
    let (lookup_locale, relative_path) = if secondary_roots.contains(first_seg) {
        let prefix = format!("/{first_seg}");
        if path == prefix {
            (first_seg.to_owned(), "/")
        } else if let Some(rest) = path.strip_prefix(&prefix)
            && rest.starts_with('/')
        {
            (first_seg.to_owned(), rest)
        } else {
            (default_locale.clone(), path)
        }
    } else {
        (default_locale.clone(), path)
    };

    if validate_custom_entry_path(relative_path).is_ok() {
        let direct_id: Option<i64> = sqlx::query_scalar(
            "SELECT e.id
               FROM entry_public_paths p
               JOIN content_entries e ON e.id = p.entry_id
               JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
               JOIN content_types t ON t.key = e.content_type_key
              WHERE p.locale = ?
                AND p.path = ?
                AND e.publication_status = 'published'
                AND e.content_type_key != 'pattern'
                AND t.provider_status = 'active'
                AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
        )
        .bind(&lookup_locale)
        .bind(relative_path)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(eid) = direct_id {
            tx.commit().await?;
            if let Some(record) =
                crate::storage::public_content::get_published_by_id(db, eid).await?
            {
                let canonical = crate::storage::permalinks::prefix_for_locale(
                    &lookup_locale,
                    &default_locale,
                    relative_path,
                );
                return Ok(PublicRouteResolution::Entry {
                    record,
                    canonical_path: canonical,
                });
            }
            return Ok(PublicRouteResolution::NotFound);
        }
    }

    let redirect_eid: Option<i64> = sqlx::query_scalar(
        "SELECT r.entry_id
           FROM entry_redirects r
           JOIN content_entries e ON e.id = r.entry_id
           JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN content_types t ON t.key = e.content_type_key
          WHERE r.locale = ?
            AND r.source_path = ?
            AND e.publication_status = 'published'
            AND e.content_type_key != 'pattern'
            AND t.provider_status = 'active'
            AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
    )
    .bind(&lookup_locale)
    .bind(relative_path)
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

use crate::public_context::{PublicFilters, PublicQuery};
use crate::seo::{
    EntrySeoSnapshot, HomeSeoSnapshot, HreflangAlternate, SeoMetadataInput, SeoView,
    validate_seo_input,
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
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(out)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

pub async fn get_home_seo_for_locale(
    db: &SqlitePool,
    requested_locale: Option<&str>,
) -> ApiResult<HomeSeoSnapshot> {
    let mut tx = db.begin().await?;
    let resolved_locale =
        crate::storage::settings::resolve_requested_locale_in_conn(&mut tx, requested_locale)
            .await?;
    let row: Option<(
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    )> = sqlx::query_as(
        "SELECT version, title, description, og_image_id, canonical_url, noindex
               FROM site_home_seo WHERE locale = ?",
    )
    .bind(&resolved_locale)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    let (version, title, description, og_image_id, canonical_url, noindex) =
        row.unwrap_or((1, None, None, None, None, 0));
    Ok(HomeSeoSnapshot {
        version,
        title,
        description,
        og_image_id,
        canonical_url,
        noindex: noindex != 0,
        locale: requested_locale.map(|_| resolved_locale),
    })
}

pub async fn get_home_seo(db: &SqlitePool) -> ApiResult<HomeSeoSnapshot> {
    get_home_seo_for_locale(db, None).await
}

pub async fn put_home_seo_for_locale(
    db: &SqlitePool,
    requested_locale: Option<&str>,
    expected_version: i64,
    input: SeoMetadataInput,
) -> ApiResult<HomeSeoSnapshot> {
    let clean = validate_seo_input(&input)?;
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res = async {
        let resolved_locale =
            crate::storage::settings::resolve_requested_locale_in_conn(&mut conn, requested_locale)
                .await?;
        verify_og_media_exists_in_tx(&mut conn, clean.og_image_id.as_deref()).await?;
        let current_ver: Option<i64> =
            sqlx::query_scalar("SELECT version FROM site_home_seo WHERE locale = ?")
                .bind(&resolved_locale)
                .fetch_optional(&mut *conn)
                .await?;
        let new_version = match current_ver {
            None => {
                if expected_version != 1 {
                    return Err(ApiError::precondition_failed(
                        "Phiên bản SEO trang chủ đã thay đổi. Hãy tải lại trước khi lưu.",
                    ));
                }
                sqlx::query(
                    "INSERT INTO site_home_seo
                        (locale, title, description, og_image_id, canonical_url, noindex, version)
                     VALUES (?, ?, ?, ?, ?, ?, 2)",
                )
                .bind(&resolved_locale)
                .bind(&clean.title)
                .bind(&clean.description)
                .bind(&clean.og_image_id)
                .bind(&clean.canonical_url)
                .bind(i64::from(clean.noindex))
                .execute(&mut *conn)
                .await?;
                2
            }
            Some(ver) => {
                if ver != expected_version {
                    return Err(ApiError::precondition_failed(
                        "Phiên bản SEO trang chủ đã thay đổi. Hãy tải lại trước khi lưu.",
                    ));
                }
                let updated = sqlx::query(
                    "UPDATE site_home_seo
                        SET title = ?, description = ?, og_image_id = ?, canonical_url = ?, noindex = ?,
                            version = version + 1
                      WHERE locale = ? AND version = ?",
                )
                .bind(&clean.title)
                .bind(&clean.description)
                .bind(&clean.og_image_id)
                .bind(&clean.canonical_url)
                .bind(i64::from(clean.noindex))
                .bind(&resolved_locale)
                .bind(expected_version)
                .execute(&mut *conn)
                .await?;
                if updated.rows_affected() == 0 {
                    return Err(ApiError::precondition_failed(
                        "Phiên bản SEO trang chủ đã thay đổi. Hãy tải lại trước khi lưu.",
                    ));
                }
                expected_version + 1
            }
        };

        Ok(HomeSeoSnapshot {
            version: new_version,
            title: clean.title,
            description: clean.description,
            og_image_id: clean.og_image_id,
            canonical_url: clean.canonical_url,
            noindex: clean.noindex,
            locale: requested_locale.map(|_| resolved_locale),
        })
    }
    .await;
    match res {
        Ok(out) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(out)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

pub async fn put_home_seo(
    db: &SqlitePool,
    expected_version: i64,
    input: SeoMetadataInput,
) -> ApiResult<HomeSeoSnapshot> {
    put_home_seo_for_locale(db, None, expected_version, input).await
}

fn project_og_image_url(
    pub_settings: &settings::PublicSiteSettings,
    media_id: Option<String>,
) -> Option<String> {
    let mid = media_id?;
    let rel = format!("/media/{mid}/file");
    Some(absolute_public_url(pub_settings, &rel).unwrap_or(rel))
}

pub(crate) fn join_origin_and_path(origin: &str, path: &str) -> String {
    let base = origin.trim_end_matches('/');
    if path.is_empty() || path == "/" {
        format!("{base}/")
    } else if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

pub(crate) fn is_self_canonical_for_origin(
    custom_canonical: Option<&str>,
    origin: &str,
    path: &str,
) -> bool {
    match custom_canonical {
        None => true,
        Some(c) => {
            let expected = join_origin_and_path(origin, path);
            c == expected || (path == "/" && c == origin.trim_end_matches('/'))
        }
    }
}

pub(crate) fn finalize_alternates_for_origin(
    origin: &str,
    members: &[(String, bool, String)],
) -> Vec<HreflangAlternate> {
    if members.len() < 2 {
        return Vec::new();
    }
    let mut out: Vec<HreflangAlternate> = members
        .iter()
        .map(|(bcp47, _, path)| HreflangAlternate {
            hreflang: bcp47.clone(),
            href: join_origin_and_path(origin, path),
        })
        .collect();
    if let Some((_, _, default_path)) = members.iter().find(|(_, is_def, _)| *is_def) {
        out.push(HreflangAlternate {
            hreflang: "x-default".to_owned(),
            href: join_origin_and_path(origin, default_path),
        });
    }
    out
}

async fn has_multilingual_alternates_prereq(
    conn: &mut SqliteConnection,
    pub_settings: &settings::PublicSiteSettings,
    noindex: bool,
) -> ApiResult<Option<String>> {
    if noindex || !pub_settings.indexing_enabled {
        return Ok(None);
    }
    let Some(origin) = pub_settings.public_origin.as_deref() else {
        return Ok(None);
    };
    let enabled = settings::list_enabled_locales_in_conn(conn).await?;
    if enabled.len() < 2 {
        return Ok(None);
    }
    Ok(Some(origin.to_owned()))
}

pub async fn build_entry_seo_view(
    db: &SqlitePool,
    entry_id: i64,
    entry_title: &str,
    canonical_path: &str,
) -> ApiResult<SeoView> {
    let mut conn = db.acquire().await?;
    let locale: Option<String> =
        sqlx::query_scalar("SELECT locale FROM content_entries WHERE id = ?")
            .bind(entry_id)
            .fetch_optional(&mut *conn)
            .await?;
    drop(conn);
    build_entry_seo_view_for_locale(db, entry_id, entry_title, locale.as_deref(), canonical_path)
        .await
}

pub async fn build_entry_seo_view_for_locale(
    db: &SqlitePool,
    entry_id: i64,
    entry_title: &str,
    locale: Option<&str>,
    canonical_path: &str,
) -> ApiResult<SeoView> {
    let general = settings::get_general_for_locale(db, locale).await?;
    let pub_settings = settings::get_public(db).await?;
    let (resolved_locale, lang, dir) = settings::get_locale_bcp47_and_dir(db, locale).await?;
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
    let mut proj = super::plugin_projections::batch_load_entry_projections(db, &[entry_id])
        .await?
        .remove(&entry_id)
        .unwrap_or_default();
    let (ov_title, ov_description, ov_og_image_id, ov_canonical, ov_noindex) =
        match proj.seo_override.take() {
            Some(ov) => (
                ov.title,
                ov.description,
                ov.og_image_id,
                ov.canonical_url,
                ov.noindex,
            ),
            None => (None, None, None, None, false),
        };
    let title = custom_title
        .or(ov_title)
        .unwrap_or_else(|| format!("{entry_title} — {}", general.site_title));
    let description = description.or(ov_description);
    let custom_canonical = custom_canonical.or(ov_canonical);
    let canonical_url = custom_canonical
        .clone()
        .or_else(|| absolute_public_url(&pub_settings, canonical_path));
    let og_image_url = if og_image_id.is_some() {
        let public_mid: Option<String> = sqlx::query_scalar(
            "SELECT media_id FROM seo_public_media_refs
              WHERE source_kind = 'entry_seo' AND source_id = ? LIMIT 1",
        )
        .bind(entry_id)
        .fetch_optional(db)
        .await?;
        project_og_image_url(&pub_settings, public_mid)
    } else if let Some(mid) = ov_og_image_id {
        if super::public_content::media_is_referenced(db, &mid).await? {
            project_og_image_url(&pub_settings, Some(mid))
        } else {
            None
        }
    } else {
        None
    };
    let noindex = (entry_noindex != 0) || ov_noindex;
    let robots = if !pub_settings.indexing_enabled || noindex {
        "noindex,nofollow".to_string()
    } else {
        "index,follow".to_string()
    };

    let mut alternates = Vec::new();
    let mut tx = db.begin().await?;
    if let Some(origin) =
        has_multilingual_alternates_prereq(&mut tx, &pub_settings, noindex).await?
        && is_self_canonical_for_origin(custom_canonical.as_deref(), &origin, canonical_path)
    {
        let sibs: Vec<(String, i64, String, Option<String>)> = sqlx::query_as(
            "SELECT sib_sl.bcp47_tag, sib_sl.is_default, claims.path, s.canonical_url
               FROM content_entries cur
               JOIN site_locales cur_sl ON cur_sl.code = cur.locale AND cur_sl.enabled = 1 AND cur_sl.deleted_at IS NULL AND (cur_sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
               JOIN content_entries sib
                 ON sib.content_type_key = cur.content_type_key
                AND sib.translation_group_id = cur.translation_group_id
               JOIN site_locales sib_sl ON sib_sl.code = sib.locale AND sib_sl.enabled = 1 AND sib_sl.deleted_at IS NULL AND (sib_sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
               JOIN content_types t ON t.key = sib.content_type_key
               JOIN permalink_route_claims claims
                 ON claims.target_kind = 'entry'
                AND claims.target_id = CAST(sib.id AS TEXT)
                AND claims.locale = sib.locale
               LEFT JOIN entry_seo s ON s.entry_id = sib.id
              WHERE cur.id = ? AND cur.locale = ?
                AND cur.publication_status = 'published'
                AND sib.publication_status = 'published'
                AND sib.content_type_key NOT IN ('pattern', 'section')
                AND NOT EXISTS (
                    SELECT 1 FROM site_reading_settings reading
                     WHERE reading.locale = sib.locale
                       AND (reading.homepage_page_id = sib.id OR reading.posts_page_id = sib.id)
                )
                AND t.provider_status = 'active'
                AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                AND COALESCE(s.noindex, 0) = 0
              ORDER BY sib_sl.is_default DESC, sib_sl.position ASC, sib_sl.code ASC",
        )
        .bind(entry_id)
        .bind(&resolved_locale)
        .fetch_all(&mut *tx)
        .await?;
        let members: Vec<(String, bool, String)> = sibs
            .into_iter()
            .filter(|(_, _, path, sib_canonical)| {
                is_self_canonical_for_origin(sib_canonical.as_deref(), &origin, path)
            })
            .map(|(bcp47, is_def, path, _)| (bcp47, is_def != 0, path))
            .collect();
        alternates = finalize_alternates_for_origin(&origin, &members);
    }
    tx.commit().await?;

    Ok(SeoView {
        lang,
        dir,
        title: title.clone(),
        description: description.clone(),
        canonical_url: canonical_url.clone(),
        alternates,
        robots,
        og_title: title,
        og_description: description,
        og_url: canonical_url,
        og_image_url,
        json_ld: proj.json_ld,
    })
}

pub async fn build_home_seo_view(db: &SqlitePool) -> ApiResult<SeoView> {
    build_home_seo_view_for_locale(db, None, "/", true).await
}

pub async fn build_home_seo_view_for_locale(
    db: &SqlitePool,
    locale: Option<&str>,
    canonical_path: &str,
    allow_alternates: bool,
) -> ApiResult<SeoView> {
    let general = settings::get_general_for_locale(db, locale).await?;
    let pub_settings = settings::get_public(db).await?;
    let (_resolved_locale, lang, dir) = settings::get_locale_bcp47_and_dir(db, locale).await?;
    let home = get_home_seo_for_locale(db, locale).await?;
    let mut has_explicit_title = home.title.is_some();
    let mut title = home.title.unwrap_or_else(|| {
        if general.tagline.trim().is_empty() {
            general.site_title.clone()
        } else {
            format!("{} — {}", general.site_title, general.tagline)
        }
    });
    let mut description = home.description;
    let mut custom_canonical = home.canonical_url;
    let mut og_image_url = project_og_image_url(&pub_settings, home.og_image_id);
    let mut noindex = home.noindex;
    let mut home_json_ld = Vec::new();
    let page_state = reading_page_state_for_locale(db, locale, true).await?;
    if let ReadingPageState::Published(id) = page_state {
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
                has_explicit_title = true;
            }
            description = entry_description.or(description);
            custom_canonical = entry_canonical.or(custom_canonical);
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
            noindex |= entry_noindex != 0;
        }
        let mut home_proj = super::plugin_projections::batch_load_entry_projections(db, &[id])
            .await?
            .remove(&id)
            .unwrap_or_default();
        if let Some(ov) = home_proj.seo_override.take() {
            if !has_explicit_title && let Some(ov_title) = ov.title {
                title = ov_title;
            }
            description = description.or(ov.description);
            custom_canonical = custom_canonical.or(ov.canonical_url);
            if og_image_url.is_none()
                && let Some(mid) = ov.og_image_id
                && super::public_content::media_is_referenced(db, &mid).await?
            {
                og_image_url = project_og_image_url(&pub_settings, Some(mid));
            }
            noindex |= ov.noindex;
        }
        home_json_ld = home_proj.json_ld;
    }
    let canonical_url = custom_canonical
        .clone()
        .or_else(|| absolute_public_url(&pub_settings, canonical_path));
    let robots = if !pub_settings.indexing_enabled || noindex {
        "noindex,nofollow".to_string()
    } else {
        "index,follow".to_string()
    };

    let mut alternates = Vec::new();
    let mut tx = db.begin().await?;
    if allow_alternates
        && matches!(page_state, ReadingPageState::Published(_))
        && let Some(origin) =
            has_multilingual_alternates_prereq(&mut tx, &pub_settings, noindex).await?
        && is_self_canonical_for_origin(custom_canonical.as_deref(), &origin, canonical_path)
    {
        let homes: Vec<(String, i64, String, Option<String>)> = sqlx::query_as(
            "SELECT sl.bcp47_tag, sl.is_default, claims.path, COALESCE(s.canonical_url, h.canonical_url)
               FROM site_reading_settings reading
               JOIN site_locales sl ON sl.code = reading.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
               JOIN content_entries e ON e.id = reading.homepage_page_id
               JOIN content_types t ON t.key = e.content_type_key
               JOIN permalink_route_claims claims
                 ON claims.target_kind = 'entry'
                AND claims.target_id = CAST(e.id AS TEXT)
                AND claims.locale = reading.locale
               LEFT JOIN entry_seo s ON s.entry_id = e.id
               LEFT JOIN site_home_seo h ON h.locale = reading.locale
              WHERE e.content_type_key = 'page'
                AND e.locale = reading.locale
                AND e.publication_status = 'published'
                AND t.provider_status = 'active'
                AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                AND COALESCE(s.noindex, h.noindex, 0) = 0
              ORDER BY sl.is_default DESC, sl.position ASC, sl.code ASC",
        )
        .fetch_all(&mut *tx)
        .await?;
        let members: Vec<(String, bool, String)> = homes
            .into_iter()
            .filter(|(_, _, path, home_canonical)| {
                is_self_canonical_for_origin(home_canonical.as_deref(), &origin, path)
            })
            .map(|(bcp47, is_def, path, _)| (bcp47, is_def != 0, path))
            .collect();
        alternates = finalize_alternates_for_origin(&origin, &members);
    }
    tx.commit().await?;

    Ok(SeoView {
        lang,
        dir,
        title: title.clone(),
        description: description.clone(),
        canonical_url: canonical_url.clone(),
        alternates,
        robots,
        og_title: title,
        og_description: description,
        og_url: canonical_url,
        og_image_url,
        json_ld: home_json_ld,
    })
}

pub async fn build_archive_seo_view_for_locale(
    db: &SqlitePool,
    type_key: &str,
    locale: &str,
    page_title: &str,
    canonical_path: &str,
    query: &PublicQuery,
) -> ApiResult<SeoView> {
    let mut view =
        build_simple_seo_view_for_locale(db, Some(locale), page_title, Some(canonical_path), false)
            .await?;
    if query.page > 1 || query.filters != PublicFilters::default() {
        return Ok(view);
    }
    let pub_settings = settings::get_public(db).await?;
    let mut tx = db.begin().await?;
    let Some(origin) = has_multilingual_alternates_prereq(&mut tx, &pub_settings, false).await?
    else {
        return Ok(view);
    };
    let rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT sl.bcp47_tag, sl.is_default, claims.path
           FROM content_types t
           JOIN site_locales sl ON sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN permalink_route_claims claims
             ON claims.target_kind = 'archive'
            AND claims.target_id = t.key
            AND claims.locale = sl.code
          WHERE t.key = ?
            AND t.provider_status = 'active'
            AND t.key != 'pattern'
            AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
            AND COALESCE(json_extract(t.definition_json, '$.settings.has_archive'), 1) = 1
            AND NOT (t.key = 'post' AND EXISTS (
              SELECT 1 FROM site_reading_settings reading
                LEFT JOIN content_entries blog ON blog.id = reading.posts_page_id AND blog.locale = reading.locale
                LEFT JOIN content_types blog_type ON blog_type.key = blog.content_type_key
               WHERE reading.locale = sl.code
                 AND reading.posts_page_id IS NOT NULL
                 AND (blog.id IS NULL OR blog.content_type_key != 'page'
                   OR blog.publication_status != 'published'
                   OR blog_type.provider_status IS NULL OR blog_type.provider_status != 'active'
                   OR COALESCE(json_extract(blog_type.definition_json, '$.settings.public'), 1) != 1)
            ))
            AND EXISTS (
                SELECT 1 FROM content_entries e
                 WHERE e.content_type_key = t.key
                   AND e.locale = sl.code
                   AND e.publication_status = 'published'
            )
          ORDER BY sl.is_default DESC, sl.position ASC, sl.code ASC",
    )
    .bind(type_key)
    .fetch_all(&mut *tx)
    .await?;
    let current_is_indexable: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM content_entries e
            WHERE e.content_type_key = ? AND e.locale = ? AND e.publication_status = 'published'
         )",
    )
    .bind(type_key)
    .bind(locale)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    if current_is_indexable != 0 {
        let members: Vec<(String, bool, String)> = rows
            .into_iter()
            .map(|(bcp47, is_def, path)| (bcp47, is_def != 0, path))
            .collect();
        view.alternates = finalize_alternates_for_origin(&origin, &members);
    }
    Ok(view)
}

pub async fn build_term_seo_view_for_locale(
    db: &SqlitePool,
    term_id: i64,
    locale: &str,
    page_title: &str,
    canonical_path: &str,
    query: &PublicQuery,
) -> ApiResult<SeoView> {
    let mut view =
        build_simple_seo_view_for_locale(db, Some(locale), page_title, Some(canonical_path), false)
            .await?;
    if query.page > 1 || query.filters != PublicFilters::default() {
        return Ok(view);
    }
    let pub_settings = settings::get_public(db).await?;
    let mut tx = db.begin().await?;
    let Some(origin) = has_multilingual_alternates_prereq(&mut tx, &pub_settings, false).await?
    else {
        return Ok(view);
    };
    let rows: Vec<(String, String, i64, String)> = sqlx::query_as(
        "SELECT sib.locale, sib_sl.bcp47_tag, sib_sl.is_default, claims.path
           FROM taxonomy_terms cur
           JOIN taxonomy_terms sib
             ON sib.kind = cur.kind
            AND sib.translation_group_id = cur.translation_group_id
           JOIN site_locales sib_sl ON sib_sl.code = sib.locale AND sib_sl.enabled = 1 AND sib_sl.deleted_at IS NULL AND (sib_sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN taxonomy_definitions td ON td.key = sib.kind
           JOIN permalink_route_claims claims
             ON claims.target_kind = 'term'
            AND claims.target_id = CAST(sib.id AS TEXT)
            AND claims.locale = sib.locale
          WHERE cur.id = ?
            AND td.public = 1
            AND td.provider_status = 'active'
            AND td.declaration_state = 'declared'
            AND EXISTS (
              SELECT 1
                FROM entry_terms et
                JOIN content_entries e ON e.id = et.entry_id
                JOIN content_types ct ON ct.key = e.content_type_key
               WHERE et.term_id = sib.id
                 AND e.locale = sib.locale
                 AND e.publication_status = 'published'
                 AND ct.provider_status = 'active'
                 AND e.content_type_key != 'pattern'
                 AND COALESCE(json_extract(ct.definition_json, '$.settings.public'), 1) = 1
                 AND (
                   EXISTS (
                     SELECT 1 FROM content_type_taxonomy_overrides ov
                     JOIN json_each(ov.taxonomies_json) selected
                       ON selected.value = sib.kind
                    WHERE ov.content_type_key = e.content_type_key
                   )
                   OR (
                     NOT EXISTS (
                       SELECT 1 FROM content_type_taxonomy_overrides ov
                        WHERE ov.content_type_key = e.content_type_key
                     )
                     AND EXISTS (
                       SELECT 1 FROM content_type_taxonomy_eligibility eligible
                        WHERE eligible.content_type_key = e.content_type_key
                          AND eligible.taxonomy_kind = sib.kind
                          AND eligible.provider_status = 'active'
                          AND eligible.declaration_state = 'declared'
                     )
                   )
                 )
            )
          ORDER BY sib_sl.is_default DESC, sib_sl.position ASC, sib_sl.code ASC",
    )
    .bind(term_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    if rows.iter().any(|(loc, _, _, _)| loc == locale) {
        let members: Vec<(String, bool, String)> = rows
            .into_iter()
            .map(|(_, bcp47, is_def, path)| (bcp47, is_def != 0, path))
            .collect();
        view.alternates = finalize_alternates_for_origin(&origin, &members);
    }
    Ok(view)
}

pub async fn build_simple_seo_view(
    db: &SqlitePool,
    page_title: &str,
    canonical_path: Option<&str>,
    noindex_override: bool,
) -> ApiResult<SeoView> {
    build_simple_seo_view_for_locale(db, None, page_title, canonical_path, noindex_override).await
}

pub async fn build_simple_seo_view_for_locale(
    db: &SqlitePool,
    locale: Option<&str>,
    page_title: &str,
    canonical_path: Option<&str>,
    noindex_override: bool,
) -> ApiResult<SeoView> {
    let general = settings::get_general_for_locale(db, locale).await?;
    let pub_settings = settings::get_public(db).await?;
    let (_resolved_locale, lang, dir) = settings::get_locale_bcp47_and_dir(db, locale).await?;
    let title = format!("{page_title} — {}", general.site_title);
    let canonical_url = canonical_path.and_then(|p| absolute_public_url(&pub_settings, p));
    let robots = if noindex_override || !pub_settings.indexing_enabled {
        "noindex,nofollow".to_string()
    } else {
        "index,follow".to_string()
    };
    Ok(SeoView {
        lang,
        dir,
        title: title.clone(),
        description: None,
        canonical_url: canonical_url.clone(),
        alternates: Vec::new(),
        robots,
        og_title: title,
        og_description: None,
        og_url: canonical_url,
        og_image_url: None,
        json_ld: Vec::new(),
    })
}

pub enum ThemeLocaleTarget<'a> {
    Home,
    Entry { entry_id: i64 },
    Archive { type_key: &'a str },
    Term { term_id: i64 },
    None,
}

/// Builds the public `locale` template context (`code`, `bcp47_tag`, `label`,
/// `direction`, `home_url`, `alternates`) independently of `public_origin`.
pub async fn build_theme_locale_context(
    db: &SqlitePool,
    current_locale: Option<&str>,
    target: ThemeLocaleTarget<'_>,
) -> ApiResult<serde_json::Value> {
    let mut conn = db.acquire().await?;
    build_theme_locale_context_in_conn(&mut conn, current_locale, target).await
}

pub(crate) async fn build_theme_locale_context_in_conn(
    conn: &mut SqliteConnection,
    current_locale: Option<&str>,
    target: ThemeLocaleTarget<'_>,
) -> ApiResult<serde_json::Value> {
    let enabled = settings::list_enabled_locales_in_conn(conn).await?;
    if enabled.is_empty() {
        return Ok(serde_json::json!({
            "code": "vi",
            "bcp47_tag": "vi",
            "label": "Tiếng Việt",
            "direction": "ltr",
            "home_url": "/",
            "alternates": [],
        }));
    }
    let default_code = enabled
        .iter()
        .find(|l| l.is_default)
        .map(|l| l.code.as_str())
        .unwrap_or_else(|| enabled[0].code.as_str())
        .to_owned();
    let current = current_locale
        .and_then(|c| enabled.iter().find(|l| l.code == c))
        .or_else(|| enabled.iter().find(|l| l.is_default))
        .unwrap_or(&enabled[0]);

    let mut translated_paths: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    match target {
        ThemeLocaleTarget::Home => {
            let rows: Vec<(String,)> = sqlx::query_as(
                "SELECT sr.locale
                   FROM site_reading_settings sr
                   JOIN content_entries e ON e.id = sr.homepage_page_id
                  WHERE e.publication_status = 'published' AND e.content_type_key = 'page'",
            )
            .fetch_all(&mut *conn)
            .await?;
            for (loc,) in rows {
                if !enabled.iter().any(|l| l.code == loc) {
                    continue;
                }
                translated_paths.insert(
                    loc.clone(),
                    crate::storage::permalinks::prefix_for_locale(&loc, &default_code, "/"),
                );
            }
        }
        ThemeLocaleTarget::Entry { entry_id } => {
            let group: Option<(String, String)> = sqlx::query_as(
                "SELECT content_type_key, translation_group_id FROM content_entries WHERE id = ?",
            )
            .bind(entry_id)
            .fetch_optional(&mut *conn)
            .await?;
            if let Some((type_key, group_id)) = group {
                let rows: Vec<(String, String)> = sqlx::query_as(
                    "SELECT e.locale, c.path
                       FROM content_entries e
                       JOIN permalink_route_claims c
                         ON c.target_kind = 'entry' AND c.target_id = e.id AND c.locale = e.locale
                      WHERE e.content_type_key = ?
                        AND e.translation_group_id = ?
                        AND e.publication_status = 'published'",
                )
                .bind(type_key)
                .bind(group_id)
                .fetch_all(&mut *conn)
                .await?;
                for (loc, path) in rows {
                    if enabled.iter().any(|l| l.code == loc) {
                        translated_paths.insert(loc, path);
                    }
                }
            }
        }
        ThemeLocaleTarget::Archive { type_key } => {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT c.locale, c.path
                   FROM permalink_route_claims c
                  WHERE c.target_kind = 'archive' AND c.target_id = ?",
            )
            .bind(type_key)
            .fetch_all(&mut *conn)
            .await?;
            for (loc, path) in rows {
                if enabled.iter().any(|l| l.code == loc) {
                    translated_paths.insert(loc, path);
                }
            }
        }
        ThemeLocaleTarget::Term { term_id } => {
            let group: Option<(String, String)> = sqlx::query_as(
                "SELECT kind, translation_group_id FROM taxonomy_terms WHERE id = ?",
            )
            .bind(term_id)
            .fetch_optional(&mut *conn)
            .await?;
            if let Some((kind, group_id)) = group {
                let rows: Vec<(String, String)> = sqlx::query_as(
                    "SELECT t.locale, c.path
                       FROM taxonomy_terms t
                       JOIN permalink_route_claims c
                         ON c.target_kind = 'term' AND c.target_id = t.id AND c.locale = t.locale
                      WHERE t.kind = ? AND t.translation_group_id = ?",
                )
                .bind(kind)
                .bind(group_id)
                .fetch_all(&mut *conn)
                .await?;
                for (loc, path) in rows {
                    if enabled.iter().any(|l| l.code == loc) {
                        translated_paths.insert(loc, path);
                    }
                }
            }
        }
        ThemeLocaleTarget::None => {}
    }

    let alternates: Vec<serde_json::Value> = enabled
        .iter()
        .map(|loc| {
            let home_url =
                crate::storage::permalinks::prefix_for_locale(&loc.code, &default_code, "/");
            let (url, has_translation) = match translated_paths.get(&loc.code) {
                Some(path) => (path.clone(), true),
                None => (home_url, false),
            };
            serde_json::json!({
                "code": loc.code,
                "bcp47_tag": loc.bcp47_tag,
                "label": loc.label,
                "url": url,
                "is_current": loc.code == current.code,
                "has_translation": has_translation,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "code": current.code,
        "bcp47_tag": current.bcp47_tag,
        "label": current.label,
        "direction": current.direction,
        "home_url": crate::storage::permalinks::prefix_for_locale(&current.code, &default_code, "/"),
        "alternates": alternates,
    }))
}

pub(crate) async fn apply_entry_route_patch_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    patch: &PatchField<String>,
    record_old_redirect: bool,
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

    if record_old_redirect || normalized_new.is_some() {
        let entry_locale: String =
            sqlx::query_scalar("SELECT locale FROM content_entries WHERE id = ?")
                .bind(entry_id)
                .fetch_one(&mut *conn)
                .await?;
        if record_old_redirect && let Some(old_path) = old_custom {
            sqlx::query(
                "INSERT OR IGNORE INTO entry_redirects (locale, source_path, entry_id) VALUES (?, ?, ?)",
            )
            .bind(&entry_locale)
            .bind(&old_path)
            .bind(entry_id)
            .execute(&mut *conn)
            .await?;
        }

        match &normalized_new {
            Some(new_path) => {
                sqlx::query(
                    "DELETE FROM entry_redirects WHERE locale = ? AND source_path = ? AND entry_id = ?",
                )
                .bind(&entry_locale)
                .bind(new_path)
                .bind(entry_id)
                .execute(&mut *conn)
                .await?;
                sqlx::query(
                    "INSERT INTO entry_public_paths (entry_id, locale, path)
                     VALUES (?, ?, ?)
                     ON CONFLICT(entry_id) DO UPDATE SET
                         locale = excluded.locale,
                         path = excluded.path",
                )
                .bind(entry_id)
                .bind(&entry_locale)
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
    } else {
        sqlx::query("DELETE FROM entry_public_paths WHERE entry_id = ?")
            .bind(entry_id)
            .execute(&mut *conn)
            .await?;
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
