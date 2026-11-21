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
           e.id AS stable_id,
           '/' AS path,
           e.updated_at AS lastmod
      FROM site_reading_settings reading
      JOIN content_entries e ON e.id = reading.homepage_page_id
      JOIN content_types t ON t.key = e.content_type_key
      LEFT JOIN entry_seo s ON s.entry_id = e.id
      LEFT JOIN site_home_seo h ON h.id = 1
     WHERE reading.id = 1 AND e.content_type_key = 'page'
       AND e.publication_status = 'published'
       AND t.provider_status = 'active'
       AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
       AND COALESCE(s.noindex, h.noindex, 0) = 0
       AND (COALESCE(s.canonical_url, h.canonical_url) IS NULL
         OR COALESCE(s.canonical_url, h.canonical_url) = ?1
         OR COALESCE(s.canonical_url, h.canonical_url) LIKE (?1 || '/%'))

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
       AND NOT EXISTS (
           SELECT 1 FROM site_reading_settings reading
            WHERE reading.id = 1
              AND (reading.homepage_page_id = e.id OR reading.posts_page_id = e.id)
       )
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
       -- A posts archive claimed by Reading Settings is not indexable when
       -- its assigned Page was withdrawn/deleted. That URL serves 404 noindex.
       AND NOT (t.key = 'post' AND EXISTS (
         SELECT 1 FROM site_reading_settings reading
           LEFT JOIN content_entries blog ON blog.id = reading.posts_page_id
           LEFT JOIN content_types blog_type ON blog_type.key = blog.content_type_key
          WHERE reading.id = 1 AND reading.posts_page_id IS NOT NULL
            AND (blog.id IS NULL OR blog.content_type_key != 'page'
              OR blog.publication_status != 'published'
              OR blog_type.provider_status IS NULL OR blog_type.provider_status != 'active'
              OR COALESCE(json_extract(blog_type.definition_json, '$.settings.public'), 1) != 1)
       ))
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
        "{INDEXABLE_ROUTES_UNION}
         SELECT CASE WHEN page.route_kind = 1 THEN '/' ELSE claims.path END AS canonical_path,
                page.lastmod
           FROM (
             SELECT route_kind, stable_id, path, lastmod FROM indexable
              ORDER BY route_kind ASC, stable_id ASC LIMIT ?2 OFFSET ?3
           ) AS page
           LEFT JOIN permalink_route_claims claims
             ON claims.target_kind = CASE page.route_kind
                  WHEN 2 THEN 'entry' WHEN 3 THEN 'archive'
                  WHEN 4 THEN 'term' ELSE 'home' END
            AND claims.target_id = CASE page.route_kind
                  WHEN 3 THEN page.path ELSE CAST(page.stable_id AS TEXT) END
          ORDER BY page.route_kind ASC, page.stable_id ASC"
    );
    // One bounded, snapshot-consistent claim projection per sitemap part.
    // The indexable CTE owns ACL/noindex/publication filtering and stable
    // pagination; every non-home path comes directly from C3 route claims.
    let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(public_origin)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *conn)
        .await?;
    let mut routes = Vec::with_capacity(rows.len());
    for (path, lastmod) in rows {
        if let Some(path) = path {
            routes.push(SitemapRoute { path, lastmod });
        }
    }
    Ok(SitemapRouteList { routes })
}
