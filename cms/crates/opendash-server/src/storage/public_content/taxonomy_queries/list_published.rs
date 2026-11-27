use super::*;

/// Publication filtering happens in SQL before paging, so a draft never
/// displaces a published entry or influences public next/previous links.
pub async fn list_published(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
) -> ApiResult<Vec<ContentEntryRecord>> {
    require_public_archive(db, type_key).await?;
    // Share the admin/P06 taxonomy predicates. Publication, category and tag
    // restrictions all run in SQL before paging; EXISTS cannot duplicate rows.
    // Keep the saved Builder media ACL query below unchanged (Forge P02).
    entries::list_published_filtered(db, type_key, limit, offset, filters).await
}

/// Validate category/tag/generic filters for a builtin (`category`/`tag`) or
/// custom taxonomy archive within one read snapshot.
pub(in crate::storage) async fn validate_taxonomy_page_filters_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    kind: &str,
    hierarchical: i64,
    filters: &PublicFilters,
) -> ApiResult<()> {
    if !matches!(kind, "category" | "tag") {
        // C3 introduces custom taxonomy archives without inheriting another
        // taxonomy's query language. Its own root term is the only generic filter;
        // rejecting extra recognized filters avoids silently dropping constraints.
        if filters.category_id.is_some()
            || filters.tag_id.is_some()
            || filters.taxonomy_key.is_some()
            || filters.term_id.is_some()
            || (filters.descendants && hierarchical == 0)
        {
            return Err(crate::error::ApiError::bad_request(
                "Bộ lọc trang taxonomy tùy chỉnh không hợp lệ.",
            ));
        }
        return Ok(());
    }
    for (filter_id, filter_kind) in [
        (filters.category_id, TaxonomyKind::Category),
        (filters.tag_id, TaxonomyKind::Tag),
    ] {
        if let Some(filter_id) = filter_id {
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM taxonomy_terms t JOIN taxonomy_definitions d ON d.key=t.kind
                 JOIN site_locales sl ON sl.code=t.locale AND sl.deleted_at IS NULL
                 WHERE t.id=? AND t.kind=? AND d.public=1
                   AND d.provider_status='active' AND d.declaration_state='declared'
                   AND sl.enabled=1 AND (sl.is_default=1 OR
                       (SELECT multilingual_enabled FROM site_locales_state WHERE id=1)=1)",
            )
            .bind(filter_id)
            .bind(filter_kind.as_str())
            .fetch_optional(&mut **tx)
            .await?;
            if exists.is_none() {
                return Err(crate::error::ApiError::bad_request(
                    "Bộ lọc taxonomy không hợp lệ.",
                ));
            }
        }
    }
    if filters.taxonomy_key.is_some() != filters.term_id.is_some()
        || (filters.taxonomy_key.is_some()
            && (filters.category_id.is_some() || filters.tag_id.is_some()))
    {
        return Err(crate::error::ApiError::bad_request(
            "Generic taxonomy filter thiếu cặp hoặc trộn aliases.",
        ));
    }
    if let (Some(key), Some(id)) = (&filters.taxonomy_key, filters.term_id) {
        let record: Option<(i64, i64)> = sqlx::query_as(
            "SELECT d.hierarchical,d.public FROM taxonomy_terms t
             JOIN taxonomy_definitions d ON d.key=t.kind
             JOIN site_locales sl ON sl.code=t.locale AND sl.deleted_at IS NULL
             WHERE t.id=? AND t.kind=? AND d.provider_status='active'
               AND d.declaration_state='declared' AND sl.enabled=1
               AND (sl.is_default=1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id=1)=1)",
        )
        .bind(id)
        .bind(key)
        .fetch_optional(&mut **tx)
        .await?;
        let Some((generic_hierarchical, public)) = record else {
            return Err(crate::error::ApiError::bad_request(
                "Generic taxonomy filter không hợp lệ.",
            ));
        };
        if public == 0 || (filters.descendants && generic_hierarchical == 0) {
            return Err(crate::error::ApiError::bad_request(
                "Taxonomy private hoặc không phân cấp.",
            ));
        }
    }
    Ok(())
}

pub async fn taxonomy_public_page(
    db: &SqlitePool,
    kind: TaxonomyKind,
    slug: &str,
    filters: PublicFilters,
    limit: i64,
    offset: i64,
) -> ApiResult<Option<(PublicTerm, TaxonomyPublishedPage)>> {
    taxonomy_public_page_for_locale(db, kind, slug, None, filters, limit, offset).await
}

pub async fn taxonomy_public_page_for_locale(
    db: &SqlitePool,
    kind: TaxonomyKind,
    slug: &str,
    locale: Option<&str>,
    filters: PublicFilters,
    limit: i64,
    offset: i64,
) -> ApiResult<Option<(PublicTerm, TaxonomyPublishedPage)>> {
    let mut tx = db.begin().await?;
    let Some(active_locale) = resolve_public_locale_in_tx(&mut tx, locale).await? else {
        return Ok(None);
    };
    let row: Option<(i64, String, String, String, i64)> = sqlx::query_as(
        "SELECT t.id, t.name, t.slug, t.locale, d.hierarchical
           FROM taxonomy_terms t
           JOIN site_locales sl ON sl.code = t.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN taxonomy_definitions d ON d.key = t.kind
          WHERE t.kind = ? AND t.locale = ? AND t.slug = ? AND d.public = 1
            AND d.provider_status = 'active' AND d.declaration_state = 'declared'",
    )
    .bind(kind.as_str())
    .bind(&active_locale)
    .bind(slug)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, name, slug, term_locale, hierarchical)) = row else {
        return Ok(None);
    };
    validate_taxonomy_page_filters_in_tx(&mut tx, kind.as_str(), hierarchical, &filters).await?;
    let page = list_taxonomy_published_in_tx(
        &mut tx,
        kind.as_str(),
        id,
        &term_locale,
        filters,
        limit,
        offset,
    )
    .await?;
    tx.commit().await?;
    Ok(Some((PublicTerm { id, name, slug }, page)))
}

/// Resolve a term archive by its stable ID, including site-defined taxonomies
/// across any enabled locale. Never re-queries by slug to avoid cross-locale aliasing.
pub async fn taxonomy_public_page_by_id(
    db: &SqlitePool,
    term_id: i64,
    filters: PublicFilters,
    limit: i64,
    offset: i64,
) -> ApiResult<Option<(String, PublicTerm, TaxonomyPublishedPage)>> {
    let mut tx = db.begin().await?;
    let row: Option<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT t.kind, t.name, t.slug, t.locale, d.hierarchical
           FROM taxonomy_terms t
           JOIN site_locales sl ON sl.code = t.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN taxonomy_definitions d ON d.key = t.kind
          WHERE t.id = ? AND d.public = 1 AND d.provider_status = 'active'
            AND d.declaration_state = 'declared'",
    )
    .bind(term_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((kind, name, slug, term_locale, hierarchical)) = row else {
        return Ok(None);
    };
    validate_taxonomy_page_filters_in_tx(&mut tx, &kind, hierarchical, &filters).await?;
    let page = list_taxonomy_published_in_tx(
        &mut tx,
        &kind,
        term_id,
        &term_locale,
        filters,
        limit,
        offset,
    )
    .await?;
    tx.commit().await?;
    Ok(Some((
        kind,
        PublicTerm {
            id: term_id,
            name,
            slug,
        },
        page,
    )))
}

/// Batch-loads active, declared, and public (`d.public = 1`) taxonomies for a
/// set of CPT keys using <= 2 batch queries, respecting `content_type_taxonomy_overrides`.
pub async fn batch_load_eligible_public_taxonomies_in_conn(
    conn: &mut sqlx::SqliteConnection,
    type_keys: &[String],
) -> ApiResult<std::collections::BTreeMap<String, Vec<String>>> {
    let mut distinct = std::collections::BTreeSet::new();
    let mut out = std::collections::BTreeMap::<String, Vec<String>>::new();
    for key in type_keys {
        if distinct.insert(key.clone()) {
            out.insert(key.clone(), Vec::new());
        }
    }
    if distinct.is_empty() {
        return Ok(out);
    }
    let all_keys: Vec<String> = distinct.into_iter().collect();
    let mut overridden = std::collections::BTreeSet::new();
    for chunk in all_keys.chunks(500) {
        let keys_json = serde_json::to_string(chunk)?;
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT o.content_type_key, d.key
               FROM content_type_taxonomy_overrides o
               LEFT JOIN json_each(o.taxonomies_json) sel ON 1 = 1
               LEFT JOIN taxonomy_definitions d
                 ON d.key = sel.value
                AND d.public = 1
                AND d.provider_status = 'active'
                AND d.declaration_state = 'declared'
              WHERE o.content_type_key IN (SELECT value FROM json_each(?))",
        )
        .bind(keys_json)
        .fetch_all(&mut *conn)
        .await?;
        for (tk, tax_opt) in rows {
            overridden.insert(tk.clone());
            if let Some(tax) = tax_opt {
                out.entry(tk).or_default().push(tax);
            }
        }
    }
    let remaining: Vec<String> = all_keys
        .into_iter()
        .filter(|k| !overridden.contains(k))
        .collect();
    for chunk in remaining.chunks(500) {
        let keys_json = serde_json::to_string(chunk)?;
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT e.content_type_key, e.taxonomy_kind
               FROM content_type_taxonomy_eligibility e
               JOIN taxonomy_definitions d ON d.key = e.taxonomy_kind
              WHERE e.content_type_key IN (SELECT value FROM json_each(?))
                AND e.provider_status = 'active'
                AND e.declaration_state = 'declared'
                AND d.public = 1
                AND d.provider_status = 'active'
                AND d.declaration_state = 'declared'
              ORDER BY e.content_type_key ASC, e.taxonomy_kind ASC",
        )
        .bind(keys_json)
        .fetch_all(&mut *conn)
        .await?;
        for (tk, tax) in rows {
            out.entry(tk).or_default().push(tax);
        }
    }
    for list in out.values_mut() {
        list.sort();
        list.dedup();
    }
    Ok(out)
}

/// Batch-loads public, active, declared and CPT-eligible `PublicTermView`s for entries
/// (supports both draft preview and published entries), with C3 canonical URLs (`None` if unclaimed).
pub async fn batch_load_public_entry_terms_in_conn(
    conn: &mut sqlx::SqliteConnection,
    entries: &[(i64, String)],
) -> ApiResult<std::collections::BTreeMap<i64, Vec<crate::content_render::PublicTermView>>> {
    let mut out =
        std::collections::BTreeMap::<i64, Vec<crate::content_render::PublicTermView>>::new();
    if entries.is_empty() {
        return Ok(out);
    }
    let mut ids_set = std::collections::BTreeSet::new();
    for (eid, _) in entries {
        ids_set.insert(*eid);
        out.entry(*eid).or_default();
    }
    let entry_ids: Vec<i64> = ids_set.into_iter().collect();
    let mut raw_rows: Vec<(i64, i64, String, String, String)> = Vec::new();
    let mut term_ids_set = std::collections::BTreeSet::new();
    for chunk in entry_ids.chunks(500) {
        let ids_json = serde_json::to_string(chunk)?;
        let rows: Vec<(i64, i64, String, String, String)> = sqlx::query_as(
            "SELECT et.entry_id, t.id, t.kind, t.name, t.slug
               FROM entry_terms et
               JOIN content_entries e ON e.id = et.entry_id
               JOIN taxonomy_terms t ON t.id = et.term_id
               JOIN taxonomy_definitions d ON d.key = t.kind
               LEFT JOIN content_type_taxonomy_overrides override
                 ON override.content_type_key = e.content_type_key
              WHERE et.entry_id IN (SELECT value FROM json_each(?))
                AND e.trashed_at IS NULL
                AND t.locale = e.locale
                AND d.public = 1
                AND d.provider_status = 'active'
                AND d.declaration_state = 'declared'
                AND (
                    (override.content_type_key IS NOT NULL AND EXISTS (
                        SELECT 1 FROM json_each(override.taxonomies_json) sel
                         WHERE sel.value = t.kind
                    ))
                    OR
                    (override.content_type_key IS NULL AND EXISTS (
                        SELECT 1 FROM content_type_taxonomy_eligibility el
                         WHERE el.content_type_key = e.content_type_key
                           AND el.taxonomy_kind = t.kind
                           AND el.provider_status = 'active'
                           AND el.declaration_state = 'declared'
                    ))
                )
              ORDER BY et.entry_id ASC, t.kind ASC, t.id ASC",
        )
        .bind(ids_json)
        .fetch_all(&mut *conn)
        .await?;
        for r in rows {
            term_ids_set.insert(r.1);
            raw_rows.push(r);
        }
    }
    let term_ids: Vec<i64> = term_ids_set.into_iter().collect();
    let term_urls =
        crate::storage::permalinks::batch_canonical_term_paths_in_tx(conn, &term_ids).await?;
    for (eid, tid, kind, name, slug) in raw_rows {
        out.entry(eid)
            .or_default()
            .push(crate::content_render::PublicTermView {
                id: tid,
                kind,
                name,
                slug,
                url: term_urls.get(&tid).cloned(),
            });
    }
    Ok(out)
}

/// Loads ancestor chain for a taxonomy term (from root ancestor down to immediate parent)
/// using 1 `WITH RECURSIVE` query (depth <= 8) + Rust cycle guard + batch C3 paths.
pub async fn load_term_ancestor_chain_in_conn(
    conn: &mut sqlx::SqliteConnection,
    term_id: i64,
    expected_kind: &str,
) -> ApiResult<Vec<crate::content_render::BreadcrumbItem>> {
    let rows: Vec<(i64, String, i64)> = sqlx::query_as(
        "WITH RECURSIVE ancestors(id, name, parent_id, depth) AS (
             SELECT p.id, p.name, p.parent_id, 1
               FROM taxonomy_terms cur
               JOIN taxonomy_terms p ON p.id = cur.parent_id AND p.kind = cur.kind
               JOIN taxonomy_definitions d ON d.key = p.kind
              WHERE cur.id = ? AND cur.kind = ?
                AND d.public = 1 AND d.provider_status = 'active' AND d.declaration_state = 'declared'
             UNION ALL
             SELECT p.id, p.name, p.parent_id, ancestors.depth + 1
               FROM ancestors
               JOIN taxonomy_terms p ON p.id = ancestors.parent_id AND p.kind = ?
               JOIN taxonomy_definitions d ON d.key = p.kind
              WHERE ancestors.depth < 8
                AND d.public = 1 AND d.provider_status = 'active' AND d.declaration_state = 'declared'
         )
         SELECT id, name, depth FROM ancestors ORDER BY depth DESC",
    )
    .bind(term_id)
    .bind(expected_kind)
    .bind(expected_kind)
    .fetch_all(&mut *conn)
    .await?;

    let mut visited = std::collections::HashSet::from([term_id]);
    let mut ordered = Vec::new();
    for (aid, name, _) in rows {
        if visited.insert(aid) {
            ordered.push((aid, name));
        }
    }
    let ancestor_ids: Vec<i64> = ordered.iter().map(|(aid, _)| *aid).collect();
    let urls =
        crate::storage::permalinks::batch_canonical_term_paths_in_tx(conn, &ancestor_ids).await?;
    Ok(ordered
        .into_iter()
        .map(|(aid, name)| crate::content_render::BreadcrumbItem {
            name,
            url: urls.get(&aid).cloned(),
            is_current: false,
        })
        .collect())
}
