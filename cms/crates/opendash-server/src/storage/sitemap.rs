//! M7-S04-T01 read-snapshot SQLite queries for dynamic sitemaps.
use sqlx::SqliteConnection;

use crate::error::ApiResult;

pub const SITEMAP_PART_SIZE: i64 = 5_000;
pub const SITEMAP_MAX_TOTAL_URLS: i64 = 1_000_000;
pub const SITEMAP_MAX_PARTS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitemapRoute {
    pub path: String,
    pub lastmod: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SitemapRouteList {
    pub routes: Vec<SitemapRoute>,
}

const INDEXABLE_ROUTES_UNION: &str = r#"
WITH indexable AS (
    SELECT 1 AS route_kind,
           1 AS stable_id,
           '/' AS path,
           NULL AS lastmod
      FROM site_home_seo h
     WHERE h.id = 1
       AND h.noindex = 0
       AND (h.canonical_url IS NULL OR h.canonical_url = ?1 OR h.canonical_url LIKE (?1 || '/%'))

    UNION ALL

    SELECT 2 AS route_kind,
           e.id AS stable_id,
           NULL AS path,
           e.updated_at AS lastmod
      FROM content_entries e
      JOIN content_types t ON t.key = e.content_type_key
      LEFT JOIN entry_seo s ON s.entry_id = e.id
     WHERE e.publication_status = 'published'
       AND e.content_type_key != 'section'
       AND t.provider_status = 'active'
       AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
       AND COALESCE(s.noindex, 0) = 0
       AND (s.canonical_url IS NULL OR s.canonical_url = ?1 OR s.canonical_url LIKE (?1 || '/%'))

    UNION ALL

    SELECT 3 AS route_kind,
           t.rowid AS stable_id,
           t.key AS path,
           NULL AS lastmod
      FROM content_types t
     WHERE t.provider_status = 'active'
       AND t.key != 'section'
       AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
       AND COALESCE(json_extract(t.definition_json, '$.settings.has_archive'), 1) = 1
       AND EXISTS (
           SELECT 1 FROM content_entries e
            WHERE e.content_type_key = t.key
              AND e.publication_status = 'published'
       )

    UNION ALL

    SELECT 4 AS route_kind,
           tm.id AS stable_id,
           NULL AS path,
           NULL AS lastmod
      FROM taxonomy_terms tm
      JOIN taxonomy_definitions td ON td.key = tm.kind
     WHERE td.public = 1
       AND td.provider_status = 'active'
       AND td.declaration_state = 'declared'
       AND EXISTS (
         SELECT 1
           FROM entry_terms et
           JOIN content_entries e ON e.id = et.entry_id
           JOIN content_types ct ON ct.key = e.content_type_key
          WHERE et.term_id = tm.id
            AND e.publication_status = 'published'
            AND ct.provider_status = 'active'
            AND e.content_type_key != 'section'
            AND COALESCE(json_extract(ct.definition_json, '$.settings.public'), 1) = 1
            AND (
              EXISTS (
                SELECT 1 FROM content_type_taxonomy_overrides ov
                JOIN json_each(ov.taxonomies_json) selected
                  ON selected.value = tm.kind
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
                     AND eligible.taxonomy_kind = tm.kind
                     AND eligible.provider_status = 'active'
                     AND eligible.declaration_state = 'declared'
                )
              )
            )
     )
)
"#;

pub async fn count_indexable_routes_in_tx(
    conn: &mut SqliteConnection,
    public_origin: &str,
) -> ApiResult<i64> {
    let sql = format!("{INDEXABLE_ROUTES_UNION} SELECT COUNT(*) FROM indexable");
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(public_origin)
        .fetch_one(&mut *conn)
        .await?;
    Ok(total)
}

pub async fn list_indexable_routes_in_tx(
    conn: &mut SqliteConnection,
    public_origin: &str,
    offset: i64,
    limit: i64,
) -> ApiResult<SitemapRouteList> {
    let sql = format!(
        "{INDEXABLE_ROUTES_UNION} SELECT route_kind, stable_id, path, lastmod FROM indexable ORDER BY route_kind ASC, stable_id ASC LIMIT ?2 OFFSET ?3"
    );
    let rows: Vec<(i64, i64, Option<String>, Option<String>)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(public_origin)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *conn)
        .await?;
    let mut routes = Vec::with_capacity(rows.len());
    for (kind, id, key_or_path, lastmod) in rows {
        // Resolve every URL from the authoritative storage route projection
        // within the sitemap read snapshot; only the home path is fixed.
        let path = match kind {
            1 => Some("/".to_owned()),
            2 => crate::storage::permalinks::canonical_entry_path_in_tx(conn, id).await?,
            3 => match key_or_path {
                Some(key) => crate::storage::permalinks::canonical_archive_path_in_tx(conn, &key).await?,
                None => None,
            },
            4 => crate::storage::permalinks::canonical_term_path_in_tx(conn, id).await?,
            _ => None,
        };
        if let Some(path) = path {
            routes.push(SitemapRoute { path, lastmod });
        }
    }
    Ok(SitemapRouteList { routes })
}
