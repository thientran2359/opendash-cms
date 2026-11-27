use super::*;

/// Resolves the canonical public path ONLY if the entry is published and its CPT is active.
pub async fn resolve_public_entry_path_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
) -> ApiResult<Option<String>> {
    let visible: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM content_entries e
           JOIN content_types t ON t.key=e.content_type_key
           WHERE e.id=? AND e.trashed_at IS NULL AND e.publication_status='published'
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
                AND e.trashed_at IS NULL
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
            AND e.trashed_at IS NULL
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

pub async fn get_entry_seo(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<EntrySeoSnapshot> {
    let entry_version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM content_entries WHERE content_type_key = ? AND id = ? AND trashed_at IS NULL",
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

pub(in crate::storage) async fn has_multilingual_alternates_prereq(
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
          ORDER BY sl.is_default DESC, sl.position ASC, sl.code ASC",
    )
    .bind(type_key)
    .fetch_all(&mut *tx)
    .await?;
    let current_is_indexable: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM content_entries e
            WHERE e.content_type_key = ? AND e.locale = ? AND e.trashed_at IS NULL AND e.publication_status = 'published'
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
