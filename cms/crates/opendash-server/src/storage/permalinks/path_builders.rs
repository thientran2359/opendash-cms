use super::*;

/// Serialized writer, optimistic configuration CAS, and whole-namespace validation.
pub async fn put_settings(
    db: &SqlitePool,
    expected_version: i64,
    input: &PermalinkSettingsInput,
) -> Result<PermalinkSettings, PermalinkPutError> {
    let mut tx = db.begin().await.map_err(ApiError::from)?;
    sqlx::query("UPDATE bootstrap SET initialized=initialized WHERE id=1")
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    let current = read_settings(&mut tx).await?;
    if current.version != expected_version {
        return Err(
            ApiError::precondition_failed("Permalink settings đã thay đổi; hãy tải lại.").into(),
        );
    }
    validate_input_in_tx(&mut tx, input).await?;
    let cpt_json = serde_json::to_string(&input.content_type_bases).map_err(ApiError::from)?;
    let taxonomy_json = serde_json::to_string(&input.taxonomy_bases).map_err(ApiError::from)?;
    let updated = sqlx::query(
        "UPDATE site_permalink_settings SET version=version+1, post_structure=?,
           content_type_bases_json=?, taxonomy_bases_json=? WHERE id=1 AND version=?",
    )
    .bind(&input.post_structure)
    .bind(cpt_json)
    .bind(taxonomy_json)
    .bind(expected_version)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::from)?;
    if updated.rows_affected() != 1 {
        return Err(
            ApiError::precondition_failed("Permalink settings đã thay đổi; hãy tải lại.").into(),
        );
    }
    sync_claims_with_options(&mut tx, true).await?;
    let output = read_settings(&mut tx).await?;
    tx.commit().await.map_err(ApiError::from)?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(output)
}

pub(in crate::storage) fn entry_path(
    settings: &PermalinkSettings,
    id: i64,
    key: &str,
    slug: &str,
    custom: Option<&str>,
    primary_category_slug: Option<&str>,
) -> String {
    if let Some(path) = custom {
        return path.to_owned();
    }
    if key == "page" {
        return format!("/{slug}");
    }
    if key == "post" {
        let cat = primary_category_slug.unwrap_or("uncategorized");
        return settings
            .post_structure
            .replace("%category%", cat)
            .replace("%postname%", slug)
            .replace("%post_id%", &id.to_string());
    }
    if let Some(Some(base)) = settings.content_type_bases.get(key) {
        return format!("/{base}/{slug}");
    }
    format!("/{key}/{slug}")
}

pub(in crate::storage) fn term_path(
    settings: &PermalinkSettings,
    id: i64,
    kind: &str,
    slug: &str,
) -> String {
    let _ = id;
    match settings.taxonomy_bases.get(kind) {
        Some(Some(base)) => format!("/{base}/{slug}"),
        _ => format!("/{kind}/{slug}"),
    }
}

pub(in crate::storage) fn archive_path(settings: &PermalinkSettings, key: &str) -> String {
    if key == "post" {
        return "/posts".to_owned();
    }
    match settings.content_type_bases.get(key) {
        Some(Some(base)) => format!("/{base}"),
        _ => format!("/{key}"),
    }
}

pub async fn canonical_entry_path_in_tx(
    conn: &mut SqliteConnection,
    id: i64,
) -> ApiResult<Option<String>> {
    sqlx::query_scalar(
        "SELECT c.path FROM permalink_route_claims c
           JOIN content_entries e ON c.target_kind='entry' AND c.target_id=CAST(e.id AS TEXT)
            AND c.locale = e.locale
           JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN content_types t ON t.key=e.content_type_key
          WHERE e.id=? AND e.trashed_at IS NULL AND e.content_type_key NOT IN ('pattern', 'section')
            AND t.provider_status='active'
            AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn canonical_term_path_in_tx(
    conn: &mut SqliteConnection,
    id: i64,
) -> ApiResult<Option<String>> {
    sqlx::query_scalar(
        "SELECT c.path FROM permalink_route_claims c
           JOIN taxonomy_terms t ON c.target_kind='term' AND c.target_id=CAST(t.id AS TEXT)
            AND c.locale = t.locale
           JOIN site_locales sl ON sl.code = t.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN taxonomy_definitions d ON d.key=t.kind
          WHERE t.id=? AND d.public=1 AND d.provider_status='active'
            AND d.declaration_state='declared'",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn batch_canonical_term_paths_in_tx(
    conn: &mut SqliteConnection,
    term_ids: &[i64],
) -> ApiResult<BTreeMap<i64, String>> {
    let mut out = BTreeMap::new();
    if term_ids.is_empty() {
        return Ok(out);
    }
    for chunk in term_ids.chunks(500) {
        let ids_json = serde_json::to_string(chunk)?;
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT t.id, c.path
               FROM permalink_route_claims c
               JOIN taxonomy_terms t
                 ON c.target_kind = 'term'
                AND c.target_id = CAST(t.id AS TEXT)
                AND c.locale = t.locale
               JOIN site_locales sl ON sl.code = t.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
               JOIN taxonomy_definitions d ON d.key = t.kind
              WHERE t.id IN (SELECT value FROM json_each(?))
                AND d.public = 1
                AND d.provider_status = 'active'
                AND d.declaration_state = 'declared'",
        )
        .bind(ids_json)
        .fetch_all(&mut *conn)
        .await?;
        for (tid, path) in rows {
            out.insert(tid, path);
        }
    }
    Ok(out)
}

pub async fn canonical_archive_path_in_tx(
    conn: &mut SqliteConnection,
    key: &str,
    locale: &str,
) -> ApiResult<Option<String>> {
    sqlx::query_scalar(
        "SELECT c.path FROM permalink_route_claims c
           JOIN content_types t ON c.target_kind='archive' AND c.target_id=t.key
           JOIN site_locales sl ON sl.code = c.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
          WHERE t.key=? AND c.locale=? AND t.key NOT IN ('pattern', 'section', 'page')
            AND t.provider_status='active'
            AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1
            AND COALESCE(json_extract(t.definition_json,'$.settings.has_archive'),1)=1",
    )
    .bind(key)
    .bind(locale)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

/// Resolve only publicly accessible destinations. Old aliases always go
/// directly to the current canonical path and never form redirect chains.
pub async fn resolve_public_path(
    db: &SqlitePool,
    path: &str,
) -> ApiResult<Option<ResolvedPublicRoute>> {
    if !path.starts_with('/') || path.contains('?') || path.contains('#') {
        return Ok(None);
    }
    let mut tx = db.begin().await?;
    let current: Option<(String, String, String)> = sqlx::query_as(
        "SELECT c.target_kind, c.target_id, c.locale
           FROM permalink_route_claims c
           JOIN site_locales sl ON sl.code = c.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
          WHERE c.path = ?",
    )
    .bind(path)
    .fetch_optional(&mut *tx)
    .await?;
    let (kind, id, locale, redirect) = match current {
        Some((kind, id, loc)) => (kind, id, loc, false),
        None => {
            let old: Option<(String, String, String)> = sqlx::query_as(
                "SELECT r.target_kind, r.target_id, r.locale
                   FROM permalink_route_redirects r
                   JOIN site_locales sl ON sl.code = r.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                  WHERE r.source_path = ?",
            )
            .bind(path)
            .fetch_optional(&mut *tx)
            .await?;
            match old {
                Some((kind, id, loc)) => (kind, id, loc, true),
                None => return Ok(None),
            }
        }
    };
    let default_locale = crate::storage::settings::get_default_locale_code_in_conn(&mut tx).await?;
    let (target, canonical_path) = match kind.as_str() {
        "home" => {
            let canonical = prefix_for_locale(&locale, &default_locale, "/");
            if !redirect && path != canonical {
                return Ok(None);
            }
            (PublicRouteTarget::Home { locale }, canonical)
        }
        "entry" => {
            let Ok(entry_id) = id.parse::<i64>() else {
                return Ok(None);
            };
            let visible: i64 = sqlx::query_scalar(
                "SELECT EXISTS(
                   SELECT 1 FROM content_entries e
                   JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                   JOIN content_types t ON t.key=e.content_type_key
                   WHERE e.id=? AND e.locale=? AND e.trashed_at IS NULL AND e.publication_status='published'
                     AND e.content_type_key NOT IN ('pattern', 'section')
                     AND t.provider_status='active'
                     AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1
                 )",
            )
            .bind(entry_id)
            .bind(&locale)
            .fetch_one(&mut *tx)
            .await?;
            if visible == 0 {
                return Ok(None);
            }
            let Some(canonical) = canonical_entry_path_in_tx(&mut tx, entry_id).await? else {
                return Ok(None);
            };
            let is_home: i64 = sqlx::query_scalar(
                "SELECT EXISTS(
                   SELECT 1 FROM site_reading_settings
                    WHERE locale = ? AND homepage_page_id = ?
                 )",
            )
            .bind(&locale)
            .bind(entry_id)
            .fetch_one(&mut *tx)
            .await?;
            (
                PublicRouteTarget::Entry {
                    id: entry_id,
                    locale,
                    is_home: is_home != 0,
                },
                canonical,
            )
        }
        "term" => {
            let Ok(term_id) = id.parse::<i64>() else {
                return Ok(None);
            };
            let Some(canonical) = canonical_term_path_in_tx(&mut tx, term_id).await? else {
                return Ok(None);
            };
            (
                PublicRouteTarget::Term {
                    id: term_id,
                    locale,
                },
                canonical,
            )
        }
        "archive" => {
            if id == "post" {
                // An assigned Blog Page which later becomes draft/deleted is a
                // dead route, not an implicit fallback to the default archive.
                let posts_page_id: Option<i64> = sqlx::query_scalar(
                    "SELECT r.posts_page_id
                       FROM site_reading_settings r
                       JOIN site_locales sl ON sl.code = r.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                      WHERE r.locale = ?",
                )
                .bind(&locale)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
                if let Some(page_id) = posts_page_id {
                    let published: i64 = sqlx::query_scalar(
                        "SELECT EXISTS(
                           SELECT 1 FROM content_entries e
                           JOIN content_types t ON t.key = e.content_type_key
                          WHERE e.id = ? AND e.locale = ? AND e.content_type_key = 'page'
                            AND e.trashed_at IS NULL
                            AND e.publication_status = 'published'
                            AND t.provider_status = 'active'
                            AND COALESCE(json_extract(t.definition_json,'$.settings.public'),1)=1
                         )",
                    )
                    .bind(page_id)
                    .bind(&locale)
                    .fetch_one(&mut *tx)
                    .await?;
                    if published == 0 {
                        return Ok(None);
                    }
                }
            }
            let Some(canonical) = canonical_archive_path_in_tx(&mut tx, &id, &locale).await? else {
                return Ok(None);
            };
            (
                PublicRouteTarget::Archive {
                    type_key: id,
                    locale,
                },
                canonical,
            )
        }
        _ => return Ok(None),
    };
    if redirect && canonical_path == path {
        return Ok(None);
    }
    tx.commit().await?;
    Ok(Some(ResolvedPublicRoute {
        target,
        canonical_path,
        redirect,
    }))
}
