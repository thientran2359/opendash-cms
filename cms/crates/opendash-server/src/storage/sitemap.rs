//! M7-S04-T01 read-snapshot SQLite queries for dynamic sitemaps.
use std::collections::BTreeMap;

use sqlx::SqliteConnection;

use crate::{
    error::ApiResult,
    seo::HreflangAlternate,
    storage::{seo as seo_storage, settings},
};

pub const SITEMAP_PART_SIZE: i64 = 5_000;
pub const SITEMAP_MAX_TOTAL_URLS: i64 = 1_000_000;
pub const SITEMAP_MAX_PARTS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitemapRoute {
    pub path: String,
    pub lastmod: Option<String>,
    pub alternates: Vec<HreflangAlternate>,
}

#[derive(Debug, Clone)]
pub struct SitemapRouteList {
    pub routes: Vec<SitemapRoute>,
}

const INDEXABLE_ROUTES_UNION: &str = r#"
WITH indexable AS (
    SELECT 1 AS route_kind,
           e.id AS stable_id,
           '' AS archive_key,
           reading.locale AS locale,
           COALESCE(
             claims.path,
             CASE WHEN sl.is_default = 1 THEN '/' ELSE '/' || sl.code END
           ) AS canonical_path,
           e.updated_at AS lastmod
      FROM site_reading_settings reading
      JOIN site_locales sl ON sl.code = reading.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
      JOIN content_entries e ON e.id = reading.homepage_page_id
      JOIN content_types t ON t.key = e.content_type_key
      LEFT JOIN permalink_route_claims claims
        ON claims.target_kind = 'entry'
       AND claims.target_id = CAST(e.id AS TEXT)
       AND claims.locale = reading.locale
      LEFT JOIN entry_seo s ON s.entry_id = e.id
      LEFT JOIN site_home_seo h ON h.locale = reading.locale
     WHERE e.content_type_key = 'page'
       AND e.locale = reading.locale
       AND e.trashed_at IS NULL
       AND e.publication_status = 'published'
       AND t.provider_status = 'active'
       AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
       AND COALESCE(s.noindex, h.noindex, 0) = 0
       AND (
         COALESCE(s.canonical_url, h.canonical_url) IS NULL
         OR COALESCE(s.canonical_url, h.canonical_url) = (
           rtrim(?1, '/') || COALESCE(
             claims.path,
             CASE WHEN sl.is_default = 1 THEN '/' ELSE '/' || sl.code END
           )
         )
         OR (sl.is_default = 1 AND COALESCE(s.canonical_url, h.canonical_url) = rtrim(?1, '/'))
       )

    UNION ALL

    SELECT 2 AS route_kind,
           e.id AS stable_id,
           '' AS archive_key,
           e.locale AS locale,
           claims.path AS canonical_path,
           e.updated_at AS lastmod
      FROM content_entries e
      JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
      JOIN content_types t ON t.key = e.content_type_key
      LEFT JOIN permalink_route_claims claims
        ON claims.target_kind = 'entry'
       AND claims.target_id = CAST(e.id AS TEXT)
       AND claims.locale = e.locale
      LEFT JOIN entry_seo s ON s.entry_id = e.id
     WHERE e.trashed_at IS NULL
       AND e.publication_status = 'published'
       AND e.content_type_key != 'pattern'
       AND NOT EXISTS (
           SELECT 1 FROM site_reading_settings reading
            WHERE reading.locale = e.locale
              AND (reading.homepage_page_id = e.id OR reading.posts_page_id = e.id)
       )
       AND t.provider_status = 'active'
       AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
       AND COALESCE(json_extract(t.definition_json, '$.settings.exclude_from_sitemap'), 0) = 0
       AND COALESCE(s.noindex, 0) = 0
       AND (
         s.canonical_url IS NULL
         OR (claims.path IS NOT NULL AND s.canonical_url = (rtrim(?1, '/') || claims.path))
       )

    UNION ALL

    SELECT 3 AS route_kind,
           t.rowid AS stable_id,
           t.key AS archive_key,
           sl.code AS locale,
           claims.path AS canonical_path,
           NULL AS lastmod
      FROM content_types t
      JOIN site_locales sl ON sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
      LEFT JOIN permalink_route_claims claims
        ON claims.target_kind = 'archive'
       AND claims.target_id = t.key
       AND claims.locale = sl.code
     WHERE t.provider_status = 'active'
       AND t.key != 'pattern'
       AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
       AND COALESCE(json_extract(t.definition_json, '$.settings.has_archive'), 1) = 1
       AND COALESCE(json_extract(t.definition_json, '$.settings.exclude_from_sitemap'), 0) = 0
       -- A posts archive claimed by Reading Settings is not indexable when
       -- its assigned Page was withdrawn/deleted. That URL serves 404 noindex.
       AND NOT (t.key = 'post' AND EXISTS (
         SELECT 1 FROM site_reading_settings reading
           LEFT JOIN content_entries blog
             ON blog.id = reading.posts_page_id AND blog.locale = reading.locale
           LEFT JOIN content_types blog_type ON blog_type.key = blog.content_type_key
          WHERE reading.locale = sl.code
            AND reading.posts_page_id IS NOT NULL
            AND (blog.id IS NULL OR blog.content_type_key != 'page'
              OR blog.trashed_at IS NOT NULL
              OR blog.publication_status != 'published'
              OR blog_type.provider_status IS NULL OR blog_type.provider_status != 'active'
              OR COALESCE(json_extract(blog_type.definition_json, '$.settings.public'), 1) != 1)
       ))
       AND EXISTS (
           SELECT 1 FROM content_entries e
            WHERE e.content_type_key = t.key
              AND e.locale = sl.code
              AND e.trashed_at IS NULL
              AND e.publication_status = 'published'
       )

    UNION ALL

    SELECT 4 AS route_kind,
           tm.id AS stable_id,
           '' AS archive_key,
           tm.locale AS locale,
           claims.path AS canonical_path,
           NULL AS lastmod
      FROM taxonomy_terms tm
      JOIN site_locales sl ON sl.code = tm.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
      JOIN taxonomy_definitions td ON td.key = tm.kind
      LEFT JOIN permalink_route_claims claims
        ON claims.target_kind = 'term'
       AND claims.target_id = CAST(tm.id AS TEXT)
       AND claims.locale = tm.locale
     WHERE td.public = 1
       AND td.provider_status = 'active'
       AND td.declaration_state = 'declared'
       AND tm.seo_noindex = 0
       AND (
         tm.seo_canonical_url IS NULL
         OR (claims.path IS NOT NULL AND tm.seo_canonical_url = (rtrim(?1, '/') || claims.path))
       )
       AND EXISTS (
         SELECT 1
           FROM entry_terms et
           JOIN content_entries e ON e.id = et.entry_id
           JOIN content_types ct ON ct.key = e.content_type_key
          WHERE et.term_id = tm.id
            AND e.locale = tm.locale
            AND e.trashed_at IS NULL
            AND e.publication_status = 'published'
            AND ct.provider_status = 'active'
            AND e.content_type_key != 'pattern'
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
         SELECT route_kind, stable_id, archive_key, canonical_path, lastmod
           FROM indexable
          ORDER BY route_kind ASC, stable_id ASC, locale ASC
          LIMIT ?2 OFFSET ?3"
    );
    let rows: Vec<(i64, i64, String, Option<String>, Option<String>)> =
        sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(public_origin)
            .bind(limit)
            .bind(offset)
            .fetch_all(&mut *conn)
            .await?;

    let enabled_locales = settings::list_enabled_locales_in_conn(conn).await?;
    let mut home_alternates = Vec::<HreflangAlternate>::new();
    let mut entry_alternates = BTreeMap::<i64, Vec<HreflangAlternate>>::new();
    let mut archive_alternates = BTreeMap::<String, Vec<HreflangAlternate>>::new();
    let mut term_alternates = BTreeMap::<i64, Vec<HreflangAlternate>>::new();

    if enabled_locales.len() >= 2 && rows.iter().any(|(_, _, _, p, _)| p.is_some()) {
        let has_home = rows.iter().any(|(rk, _, _, p, _)| *rk == 1 && p.is_some());
        let entry_ids: Vec<i64> = rows
            .iter()
            .filter(|(rk, _, _, p, _)| *rk == 2 && p.is_some())
            .map(|(_, id, _, _, _)| *id)
            .collect();
        let has_archives = rows.iter().any(|(rk, _, _, p, _)| *rk == 3 && p.is_some());
        let term_ids: Vec<i64> = rows
            .iter()
            .filter(|(rk, _, _, p, _)| *rk == 4 && p.is_some())
            .map(|(_, id, _, _, _)| *id)
            .collect();

        if has_home {
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
                    AND e.trashed_at IS NULL
                    AND e.publication_status = 'published'
                    AND t.provider_status = 'active'
                    AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                    AND COALESCE(s.noindex, h.noindex, 0) = 0
                  ORDER BY sl.is_default DESC, sl.position ASC, sl.code ASC",
            )
            .fetch_all(&mut *conn)
            .await?;
            let members: Vec<(String, bool, String)> = homes
                .into_iter()
                .filter(|(_, _, path, custom)| {
                    seo_storage::is_self_canonical_for_origin(
                        custom.as_deref(),
                        public_origin,
                        path,
                    )
                })
                .map(|(bcp47, is_def, path, _)| (bcp47, is_def != 0, path))
                .collect();
            home_alternates = seo_storage::finalize_alternates_for_origin(public_origin, &members);
        }

        if !entry_ids.is_empty() {
            let ids_json = serde_json::to_string(&entry_ids)?;
            let sib_rows: Vec<(i64, String, i64, String)> = sqlx::query_as(
                "SELECT cur.id, sib_sl.bcp47_tag, sib_sl.is_default, claims.path
                   FROM content_entries cur
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
                  WHERE cur.id IN (SELECT value FROM json_each(?1))
                    AND cur.trashed_at IS NULL
                    AND sib.trashed_at IS NULL
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
                    AND (s.canonical_url IS NULL OR s.canonical_url = (rtrim(?2, '/') || claims.path))
                  ORDER BY cur.id ASC, sib_sl.is_default DESC, sib_sl.position ASC, sib_sl.code ASC",
            )
            .bind(ids_json)
            .bind(public_origin)
            .fetch_all(&mut *conn)
            .await?;
            let mut grouped = BTreeMap::<i64, Vec<(String, bool, String)>>::new();
            for (cur_id, bcp47, is_def, path) in sib_rows {
                grouped
                    .entry(cur_id)
                    .or_default()
                    .push((bcp47, is_def != 0, path));
            }
            for (cur_id, members) in grouped {
                entry_alternates.insert(
                    cur_id,
                    seo_storage::finalize_alternates_for_origin(public_origin, &members),
                );
            }
        }

        if has_archives {
            let arch_sql = format!(
                "{INDEXABLE_ROUTES_UNION}
                 SELECT ix.archive_key, sl.bcp47_tag, sl.is_default, ix.canonical_path
                   FROM indexable ix
                   JOIN site_locales sl ON sl.code = ix.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                  WHERE ix.route_kind = 3
                    AND ix.canonical_path IS NOT NULL
                  ORDER BY ix.archive_key ASC, sl.is_default DESC, sl.position ASC, sl.code ASC"
            );
            let arch_rows: Vec<(String, String, i64, String)> =
                sqlx::query_as(sqlx::AssertSqlSafe(arch_sql))
                    .bind(public_origin)
                    .fetch_all(&mut *conn)
                    .await?;
            let mut grouped = BTreeMap::<String, Vec<(String, bool, String)>>::new();
            for (key, bcp47, is_def, path) in arch_rows {
                grouped
                    .entry(key)
                    .or_default()
                    .push((bcp47, is_def != 0, path));
            }
            for (key, members) in grouped {
                archive_alternates.insert(
                    key,
                    seo_storage::finalize_alternates_for_origin(public_origin, &members),
                );
            }
        }

        if !term_ids.is_empty() {
            let ids_json = serde_json::to_string(&term_ids)?;
            let term_rows: Vec<(i64, String, i64, String)> = sqlx::query_as(
                "SELECT cur.id, sib_sl.bcp47_tag, sib_sl.is_default, claims.path
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
                  WHERE cur.id IN (SELECT value FROM json_each(?1))
                    AND td.public = 1
                    AND td.provider_status = 'active'
                    AND td.declaration_state = 'declared'
                    AND sib.seo_noindex = 0
                    AND (sib.seo_canonical_url IS NULL OR sib.seo_canonical_url = (rtrim(?2, '/') || claims.path))
                    AND EXISTS (
                      SELECT 1
                        FROM entry_terms et
                        JOIN content_entries e ON e.id = et.entry_id
                        JOIN content_types ct ON ct.key = e.content_type_key
                       WHERE et.term_id = sib.id
                         AND e.locale = sib.locale
                         AND e.trashed_at IS NULL
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
                  ORDER BY cur.id ASC, sib_sl.is_default DESC, sib_sl.position ASC, sib_sl.code ASC",
            )
            .bind(ids_json)
            .bind(public_origin)
            .fetch_all(&mut *conn)
            .await?;
            let mut grouped = BTreeMap::<i64, Vec<(String, bool, String)>>::new();
            for (cur_id, bcp47, is_def, path) in term_rows {
                grouped
                    .entry(cur_id)
                    .or_default()
                    .push((bcp47, is_def != 0, path));
            }
            for (cur_id, members) in grouped {
                term_alternates.insert(
                    cur_id,
                    seo_storage::finalize_alternates_for_origin(public_origin, &members),
                );
            }
        }
    }

    let mut routes = Vec::with_capacity(rows.len());
    for (route_kind, stable_id, archive_key, path, lastmod) in rows {
        let Some(path) = path else {
            continue;
        };
        let alternates = match route_kind {
            1 => home_alternates.clone(),
            2 => entry_alternates.remove(&stable_id).unwrap_or_default(),
            3 => archive_alternates
                .get(&archive_key)
                .cloned()
                .unwrap_or_default(),
            4 => term_alternates.remove(&stable_id).unwrap_or_default(),
            _ => Vec::new(),
        };
        routes.push(SitemapRoute {
            path,
            lastmod,
            alternates,
        });
    }
    Ok(SitemapRouteList { routes })
}
