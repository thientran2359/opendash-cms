use std::collections::{BTreeMap, HashMap};

use super::*;

fn claim_kind_label(kind: &str) -> Option<&'static str> {
    match kind {
        "home" => Some("home"),
        "entry" => Some("entry"),
        "archive" => Some("archive"),
        "term" => Some("term"),
        _ => None,
    }
}

/// Checks which candidate normalized paths are currently shadowed by an actually
/// live public canonical claim, mirroring `resolve_public_path` visibility rules
/// so draft, trashed, or provider-inactive claims do not raise false warnings.
pub(crate) async fn find_live_claim_kinds_for_paths_in_conn(
    conn: &mut SqliteConnection,
    paths: &[String],
) -> ApiResult<BTreeMap<String, &'static str>> {
    if paths.is_empty() {
        return Ok(BTreeMap::new());
    }
    let default_locale = crate::storage::settings::get_default_locale_code_in_conn(conn).await?;
    let paths_json = serde_json::to_string(paths)?;
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT c.path, c.target_kind, c.target_id, c.locale
           FROM permalink_route_claims c
           JOIN site_locales sl
             ON sl.code = c.locale
            AND sl.enabled = 1
            AND sl.deleted_at IS NULL
            AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
          WHERE c.path IN (SELECT value FROM json_each(?))
            AND (
                c.target_kind = 'home'
                OR (
                    c.target_kind = 'entry'
                    AND EXISTS (
                        SELECT 1
                          FROM content_entries e
                          JOIN content_types t ON t.key = e.content_type_key
                         WHERE CAST(e.id AS TEXT) = c.target_id
                           AND e.locale = c.locale
                           AND e.trashed_at IS NULL
                           AND e.publication_status = 'published'
                           AND e.content_type_key NOT IN ('pattern', 'section')
                           AND t.provider_status = 'active'
                           AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                    )
                )
                OR (
                    c.target_kind = 'term'
                    AND EXISTS (
                        SELECT 1
                          FROM taxonomy_terms tm
                          JOIN taxonomy_definitions td ON td.key = tm.kind
                         WHERE CAST(tm.id AS TEXT) = c.target_id
                           AND tm.locale = c.locale
                           AND td.public = 1
                           AND td.provider_status = 'active'
                           AND td.declaration_state = 'declared'
                    )
                )
                OR (
                    c.target_kind = 'archive'
                    AND EXISTS (
                        SELECT 1
                          FROM content_types t
                         WHERE t.key = c.target_id
                           AND t.key NOT IN ('pattern', 'section', 'page')
                           AND t.provider_status = 'active'
                           AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                           AND COALESCE(json_extract(t.definition_json, '$.settings.has_archive'), 1) = 1
                    )
                    AND (
                        c.target_id != 'post'
                        OR NOT EXISTS (
                            SELECT 1
                              FROM site_reading_settings r
                             WHERE r.locale = c.locale
                               AND r.posts_page_id IS NOT NULL
                               AND NOT EXISTS (
                                   SELECT 1
                                     FROM content_entries p
                                     JOIN content_types pt ON pt.key = p.content_type_key
                                    WHERE p.id = r.posts_page_id
                                      AND p.locale = r.locale
                                      AND p.content_type_key = 'page'
                                      AND p.trashed_at IS NULL
                                      AND p.publication_status = 'published'
                                      AND pt.provider_status = 'active'
                                      AND COALESCE(json_extract(pt.definition_json, '$.settings.public'), 1) = 1
                               )
                        )
                    )
                )
            )",
    )
    .bind(paths_json)
    .fetch_all(&mut *conn)
    .await?;

    let mut out = BTreeMap::new();
    for (path, kind, _target_id, locale) in rows {
        if kind == "home" && path != prefix_for_locale(&locale, &default_locale, "/") {
            continue;
        }
        if let Some(label) = claim_kind_label(&kind) {
            out.insert(path, label);
        }
    }
    Ok(out)
}

/// Loads active `permalink_route_redirects` edges (`source_path -> canonical_path`)
/// whose target is currently live/public, matching `resolve_public_path`.
pub(crate) async fn load_live_auto_redirect_edges_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<HashMap<String, String>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT r.source_path, c.path
           FROM permalink_route_redirects r
           JOIN site_locales sl
             ON sl.code = r.locale
            AND sl.enabled = 1
            AND sl.deleted_at IS NULL
            AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN permalink_route_claims c
             ON c.target_kind = r.target_kind
            AND c.target_id = r.target_id
            AND c.locale = r.locale
          WHERE c.path != r.source_path
            AND (
                r.target_kind = 'home'
                OR (
                    r.target_kind = 'entry'
                    AND EXISTS (
                        SELECT 1
                          FROM content_entries e
                          JOIN content_types t ON t.key = e.content_type_key
                         WHERE CAST(e.id AS TEXT) = r.target_id
                           AND e.locale = r.locale
                           AND e.trashed_at IS NULL
                           AND e.publication_status = 'published'
                           AND e.content_type_key NOT IN ('pattern', 'section')
                           AND t.provider_status = 'active'
                           AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                    )
                )
                OR (
                    r.target_kind = 'term'
                    AND EXISTS (
                        SELECT 1
                          FROM taxonomy_terms tm
                          JOIN taxonomy_definitions td ON td.key = tm.kind
                         WHERE CAST(tm.id AS TEXT) = r.target_id
                           AND tm.locale = r.locale
                           AND td.public = 1
                           AND td.provider_status = 'active'
                           AND td.declaration_state = 'declared'
                    )
                )
                OR (
                    r.target_kind = 'archive'
                    AND EXISTS (
                        SELECT 1
                          FROM content_types t
                         WHERE t.key = r.target_id
                           AND t.key NOT IN ('pattern', 'section', 'page')
                           AND t.provider_status = 'active'
                           AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                           AND COALESCE(json_extract(t.definition_json, '$.settings.has_archive'), 1) = 1
                    )
                    AND (
                        r.target_id != 'post'
                        OR NOT EXISTS (
                            SELECT 1
                              FROM site_reading_settings rs
                             WHERE rs.locale = r.locale
                               AND rs.posts_page_id IS NOT NULL
                               AND NOT EXISTS (
                                   SELECT 1
                                     FROM content_entries p
                                     JOIN content_types pt ON pt.key = p.content_type_key
                                    WHERE p.id = rs.posts_page_id
                                      AND p.locale = rs.locale
                                      AND p.content_type_key = 'page'
                                      AND p.trashed_at IS NULL
                                      AND p.publication_status = 'published'
                                      AND pt.provider_status = 'active'
                                      AND COALESCE(json_extract(pt.definition_json, '$.settings.public'), 1) = 1
                               )
                        )
                    )
                )
            )",
    )
    .fetch_all(&mut *conn)
    .await?;

    Ok(rows.into_iter().collect())
}
