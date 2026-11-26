use super::*;

// Administrative locale resolution accepts disabled locales and rejects
// deleted ones. Public lookup instead treats known but inactive locales as
// missing, while retaining validation for malformed/unknown codes.
pub(in crate::storage) async fn resolve_public_locale_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    locale: Option<&str>,
) -> ApiResult<Option<String>> {
    let Some(raw) = locale else {
        return crate::storage::settings::get_default_locale_code_in_conn(tx)
            .await
            .map(Some);
    };
    let normalized = raw.trim().to_ascii_lowercase();
    let row: Option<(String, i64)> = sqlx::query_as(
        "SELECT sl.code, (sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1))
           FROM site_locales sl WHERE sl.code = ?",
    )
    .bind(normalized)
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some((code, 1)) => Ok(Some(code)),
        Some(_) => Ok(None),
        None => crate::storage::settings::resolve_requested_locale_in_conn(tx, Some(raw))
            .await
            .map(Some),
    }
}

pub struct PublicEntryRenderSnapshot {
    pub entry: ContentEntryRecord,
    pub locale: String,
    pub effective_schema: opendash_core::content::EffectiveContentSchema,
    pub eligible_public_taxonomies: Vec<String>,
    pub terms: Vec<crate::content_render::PublicTermView>,
    pub active: Option<crate::storage::theme::ActiveThemeV3>,
    pub resolution: crate::storage::overrides::TemplateResolution,
    pub body: Option<crate::page::RenderedLayout>,
    pub budget: crate::page::RenderBudget,
}

/// Resolve published entry, its own Builder body, and the selected shared
/// template using one read snapshot. The entry document never participates in
/// the shared template candidate chain.
pub async fn load_entry_render_snapshot(
    db: &SqlitePool,
    id: i64,
    themes_root: Option<&std::path::Path>,
) -> ApiResult<Option<PublicEntryRenderSnapshot>> {
    load_entry_render_snapshot_with_cache(
        db,
        id,
        themes_root,
        crate::page::BlockTemplateCache::global(),
    )
    .await
}

pub async fn load_entry_render_snapshot_with_cache(
    db: &SqlitePool,
    id: i64,
    themes_root: Option<&std::path::Path>,
    cache: &crate::page::BlockTemplateCache,
) -> ApiResult<Option<PublicEntryRenderSnapshot>> {
    load_entry_render_snapshot_with_options(
        db,
        id,
        themes_root,
        cache,
        "/",
        &std::collections::BTreeMap::new(),
    )
    .await
}

/// Deny public archives independently from entry visibility. Legacy CPTs
/// without settings retain their existing public archive behavior.
pub async fn require_public_archive_in_conn(
    conn: &mut sqlx::SqliteConnection,
    type_key: &str,
) -> ApiResult<()> {
    let visible: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM content_types
            WHERE key = ? AND key != 'pattern' AND provider_status = 'active'
              AND COALESCE(json_extract(definition_json, '$.settings.public'), 1) = 1
              AND COALESCE(json_extract(definition_json, '$.settings.has_archive'), 1) = 1
        )",
    )
    .bind(type_key)
    .fetch_one(&mut *conn)
    .await?;
    if visible == 0 {
        return Err(crate::error::ApiError::not_found(
            "Archive nội dung không tồn tại.",
        ));
    }
    Ok(())
}

pub async fn require_public_archive(db: &SqlitePool, type_key: &str) -> ApiResult<()> {
    let mut conn = db.acquire().await?;
    require_public_archive_in_conn(&mut conn, type_key).await
}

/// Single-snapshot Archive page + exact filtered total count without torn reads.
pub async fn list_published_with_total(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
) -> ApiResult<(Vec<ContentEntryRecord>, i64)> {
    let mut tx = db.begin().await?;
    require_public_archive_in_conn(&mut tx, type_key).await?;
    let res = entries::list_published_filtered_with_total_in_tx(
        &mut tx,
        type_key,
        limit,
        offset,
        filters,
        entries::PublishedListSort::IdDesc,
    )
    .await?;
    tx.commit().await?;
    Ok(res)
}

/// Resolve stable site-owned IDs to their current public URL. Draft/deleted
/// entries must never fall through to an older slug or another content type.
pub async fn canonical_entry_by_id(db: &SqlitePool, id: i64) -> ApiResult<Option<String>> {
    let mut conn = db.acquire().await?;
    crate::storage::seo::resolve_public_entry_path_in_tx(&mut conn, id).await
}

pub async fn canonical_term_by_id(db: &SqlitePool, id: i64) -> ApiResult<Option<String>> {
    let mut conn = db.acquire().await?;
    crate::storage::permalinks::canonical_term_path_in_tx(&mut conn, id).await
}

#[derive(Debug, Clone)]
pub struct PublicTerm {
    pub id: i64,
    pub name: String,
    pub slug: String,
}

pub struct TaxonomyPublishedPage {
    pub entries: Vec<ContentEntryRecord>,
    pub total: i64,
}

pub async fn load_eligible_public_taxonomies_in_conn(
    conn: &mut sqlx::SqliteConnection,
    type_key: &str,
) -> ApiResult<Vec<String>> {
    let mut map =
        batch_load_eligible_public_taxonomies_in_conn(conn, &[type_key.to_owned()]).await?;
    Ok(map.remove(type_key).unwrap_or_default())
}

pub async fn batch_load_eligible_public_taxonomies(
    db: &SqlitePool,
    type_keys: &[String],
) -> ApiResult<std::collections::BTreeMap<String, Vec<String>>> {
    let mut conn = db.acquire().await?;
    batch_load_eligible_public_taxonomies_in_conn(&mut conn, type_keys).await
}

pub async fn batch_load_public_entry_terms(
    db: &SqlitePool,
    entries: &[(i64, String)],
) -> ApiResult<std::collections::BTreeMap<i64, Vec<crate::content_render::PublicTermView>>> {
    let mut conn = db.acquire().await?;
    batch_load_public_entry_terms_in_conn(&mut conn, entries).await
}

pub(in crate::storage) async fn hydrate_entry_views_in_conn(
    conn: &mut sqlx::SqliteConnection,
    records: Vec<ContentEntryRecord>,
) -> ApiResult<Vec<crate::content_render::EntryView>> {
    if records.is_empty() {
        return Ok(Vec::new());
    }
    let entry_ids: Vec<i64> = records.iter().map(|r| r.id).collect();
    let pairs: Vec<(i64, String)> = records
        .iter()
        .map(|r| (r.id, r.content_type_key.clone()))
        .collect();
    let distinct_types: Vec<String> = records
        .iter()
        .map(|r| r.content_type_key.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    let urls =
        crate::storage::seo::resolve_public_entry_paths_by_entry_ids_in_conn(conn, &entry_ids)
            .await?;
    let mut terms_map = batch_load_public_entry_terms_in_conn(conn, &pairs).await?;
    let tax_map = batch_load_eligible_public_taxonomies_in_conn(conn, &distinct_types).await?;
    let mut schemas = std::collections::BTreeMap::new();
    for tk in &distinct_types {
        if let Some(eff) =
            crate::storage::content::load_effective_content_schema_in_tx(conn, tk).await?
        {
            schemas.insert(tk.clone(), eff);
        }
    }

    let mut proj_map =
        crate::storage::plugin_projections::batch_load_entry_projections_in_conn(conn, &entry_ids)
            .await?;

    let mut views = Vec::with_capacity(records.len());
    for record in records {
        let Some(eff) = schemas.get(&record.content_type_key) else {
            continue;
        };
        let url = urls.get(&record.id).cloned();
        let terms = terms_map.remove(&record.id).unwrap_or_default();
        let eligible_taxonomies = tax_map
            .get(&record.content_type_key)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut view = crate::content_render::entry_view_with_context(
            &record,
            eff,
            url,
            &terms,
            eligible_taxonomies,
            opendash_core::content::FieldRole::Editor,
        );
        view.ext = proj_map
            .remove(&record.id)
            .map(|p| p.ext)
            .unwrap_or_default();
        views.push(view);
    }
    Ok(views)
}
