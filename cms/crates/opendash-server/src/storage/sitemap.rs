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
           COALESCE(p.path, '/content/' || e.content_type_key || '/' || e.slug) AS path,
           e.updated_at AS lastmod
      FROM content_entries e
      JOIN content_types t ON t.key = e.content_type_key
      LEFT JOIN entry_public_paths p ON p.entry_id = e.id
      LEFT JOIN entry_seo s ON s.entry_id = e.id
     WHERE e.publication_status = 'published'
       AND t.provider_status = 'active'
       AND COALESCE(s.noindex, 0) = 0
       AND (s.canonical_url IS NULL OR s.canonical_url = ?1 OR s.canonical_url LIKE (?1 || '/%'))

    UNION ALL

    SELECT 3 AS route_kind,
           t.rowid AS stable_id,
           '/content/' || t.key AS path,
           NULL AS lastmod
      FROM content_types t
     WHERE t.provider_status = 'active'
       AND EXISTS (
           SELECT 1 FROM content_entries e
            WHERE e.content_type_key = t.key
              AND e.publication_status = 'published'
       )

    UNION ALL

    SELECT 4 AS route_kind,
           tm.id AS stable_id,
           CASE tm.kind
               WHEN 'category' THEN '/taxonomy/category/' || tm.slug
               ELSE '/taxonomy/tag/' || tm.slug
           END AS path,
           NULL AS lastmod
      FROM taxonomy_terms tm
     WHERE EXISTS (
         SELECT 1
           FROM entry_terms et
           JOIN content_entries e ON e.id = et.entry_id
           JOIN content_types ct ON ct.key = e.content_type_key
          WHERE et.term_id = tm.id
            AND e.publication_status = 'published'
            AND ct.provider_status = 'active'
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
        "{INDEXABLE_ROUTES_UNION} SELECT path, lastmod FROM indexable ORDER BY route_kind ASC, stable_id ASC LIMIT ?2 OFFSET ?3"
    );
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(public_origin)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *conn)
        .await?;
    let routes = rows
        .into_iter()
        .map(|(path, lastmod)| SitemapRoute { path, lastmod })
        .collect();
    Ok(SitemapRouteList { routes })
}
