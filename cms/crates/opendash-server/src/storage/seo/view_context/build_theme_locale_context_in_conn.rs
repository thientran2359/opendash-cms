use super::*;

pub(crate) async fn resolve_locale_search_url_in_conn(
    conn: &mut SqliteConnection,
    current_locale: Option<&str>,
) -> ApiResult<String> {
    let enabled = settings::list_enabled_locales_in_conn(conn).await?;
    if enabled.is_empty() {
        return Ok("/".to_owned());
    }
    let default_code = enabled
        .iter()
        .find(|l| l.is_default)
        .map(|l| l.code.as_str())
        .unwrap_or_else(|| enabled[0].code.as_str())
        .to_owned();
    let current = current_locale
        .and_then(|code| enabled.iter().find(|l| l.code == code))
        .or_else(|| enabled.iter().find(|l| l.is_default))
        .unwrap_or(&enabled[0]);
    Ok(crate::storage::permalinks::prefix_for_locale(
        &current.code,
        &default_code,
        "/",
    ))
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
            "search_url": "/",
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
                  WHERE e.trashed_at IS NULL AND e.publication_status = 'published' AND e.content_type_key = 'page'",
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
                "SELECT content_type_key, translation_group_id FROM content_entries WHERE id = ? AND trashed_at IS NULL",
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
                        AND e.trashed_at IS NULL
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
        ThemeLocaleTarget::Search { raw_search_query } => {
            let clean_query = raw_search_query.map(str::trim).filter(|q| !q.is_empty());
            for loc in &enabled {
                let base_url =
                    crate::storage::permalinks::prefix_for_locale(&loc.code, &default_code, "/");
                let url = crate::public_context::CanonicalUrlBuilder::search_url(
                    &base_url,
                    clean_query,
                    None,
                    1,
                );
                translated_paths.insert(loc.code.clone(), url);
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

    let base_locale_url =
        crate::storage::permalinks::prefix_for_locale(&current.code, &default_code, "/");
    Ok(serde_json::json!({
        "code": current.code,
        "bcp47_tag": current.bcp47_tag,
        "label": current.label,
        "direction": current.direction,
        "home_url": base_locale_url,
        "search_url": base_locale_url,
        "alternates": alternates,
    }))
}
