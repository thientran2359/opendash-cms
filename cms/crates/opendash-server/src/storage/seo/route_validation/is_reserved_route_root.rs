use super::*;

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
                            AND e.trashed_at IS NULL
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
                            AND e.trashed_at IS NULL
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

/// Sets or clears `entry_public_paths` inside an active transaction, enforcing ENTRY CAS
/// and cross-table uniqueness, and capturing old custom path in `entry_redirects` only if ever published.
pub(in crate::storage) async fn set_entry_route_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    expected_entry_version: i64,
    custom_path: Option<&str>,
) -> ApiResult<EntryRouteSnapshot> {
    let row: Option<(i64, i64, String, Option<String>)> = sqlx::query_as(
        "SELECT version, has_been_published, locale, trashed_at FROM content_entries WHERE id = ?",
    )
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((current_ver, has_been_published, entry_locale, trashed_at)) = row else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };
    if current_ver != expected_entry_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        ));
    }
    if trashed_at.is_some() {
        return Err(ApiError::conflict(
            "entry_trashed: Nội dung đang ở trong Thùng rác và không thể chỉnh sửa đường dẫn cho đến khi khôi phục.",
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
          WHERE id = ? AND version = ? AND trashed_at IS NULL",
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
          WHERE e.content_type_key = ? AND e.id = ? AND e.trashed_at IS NULL",
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
