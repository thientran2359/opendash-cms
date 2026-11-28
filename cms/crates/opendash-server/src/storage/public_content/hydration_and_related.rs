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
    pub active: Option<crate::storage::theme::ActiveTheme>,
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

pub async fn list_published_archive_with_total(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
    sort: entries::PublishedListSort,
) -> ApiResult<(Vec<ContentEntryRecord>, i64)> {
    let mut tx = db.begin().await?;
    require_public_archive_in_conn(&mut tx, type_key).await?;
    let res = entries::list_published_filtered_with_total_in_tx(
        &mut tx, type_key, limit, offset, filters, sort,
    )
    .await?;
    tx.commit().await?;
    Ok(res)
}

/// Loads published `"post"` entries for `/` / `/{locale}` (`Unconfigured`)
/// using `Index`, bypassing `has_archive == false` while enforcing `active` & `public`.
pub async fn list_published_for_index_home(
    db: &SqlitePool,
    limit: i64,
    offset: i64,
    locale: Option<&str>,
    sort: entries::PublishedListSort,
) -> ApiResult<(Vec<ContentEntryRecord>, i64)> {
    let mut tx = db.begin().await?;
    let visible: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM content_types
            WHERE key = 'post' AND provider_status = 'active'
              AND COALESCE(json_extract(definition_json, '$.settings.public'), 1) = 1
        )",
    )
    .fetch_one(&mut *tx)
    .await?;
    if visible == 0 {
        tx.commit().await?;
        return Ok((Vec::new(), 0));
    }
    let filters = EntryFilters {
        locale: locale.map(str::to_owned),
        ..Default::default()
    };
    let res = entries::list_published_filtered_with_total_in_tx(
        &mut tx, "post", limit, offset, filters, sort,
    )
    .await?;
    tx.commit().await?;
    Ok(res)
}

/// Single-snapshot Archive page + exact filtered total count without torn reads.
pub async fn list_published_with_total(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
) -> ApiResult<(Vec<ContentEntryRecord>, i64)> {
    list_published_archive_with_total(
        db,
        type_key,
        limit,
        offset,
        filters,
        entries::PublishedListSort::IdDesc,
    )
    .await
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

pub async fn hydrate_single_entry_view_in_conn(
    conn: &mut sqlx::SqliteConnection,
    record: &ContentEntryRecord,
    effective_schema: &opendash_core::content::EffectiveContentSchema,
    url: Option<String>,
    terms: &[crate::content_render::PublicTermView],
    eligible_public_taxonomies: &[String],
    max_view_role: opendash_core::content::FieldRole,
    is_preview: bool,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<crate::content_render::EntryView> {
    let eval_map = batch_load_entry_eval_contexts_in_conn(
        conn,
        &[(
            record.id,
            record.content_type_key.clone(),
            record.page_layout_key.clone(),
            record.custom_template_key.clone(),
        )],
        max_view_role,
    )
    .await?;
    let eval_ctx = eval_map.get(&record.id).cloned().unwrap_or_default();
    let ordered_specs: Vec<opendash_core::field_schema::FieldSpec> =
        crate::content_render::ordered_public_field_defs_for_entry(
            effective_schema,
            max_view_role,
            &eval_ctx,
        )
        .into_iter()
        .cloned()
        .collect();
    let mut media_ids = std::collections::BTreeSet::new();
    let mut entry_ids = std::collections::BTreeSet::new();
    let mut term_ids = std::collections::BTreeSet::new();
    crate::field_hydrate::collect_hydration_ids(
        &ordered_specs,
        &record.values,
        crate::field_hydrate::HydrationVisibility::ContentPublic { max_view_role },
        &mut media_ids,
        &mut entry_ids,
        &mut term_ids,
    );
    let entries_map = if entry_ids.is_empty() {
        std::collections::BTreeMap::new()
    } else {
        crate::storage::fields::load_entry_ref_hydration_map_in_conn(
            conn,
            &entry_ids,
            is_preview,
            Some(&record.locale),
        )
        .await?
    };
    let terms_hydrate_map = if term_ids.is_empty() {
        std::collections::BTreeMap::new()
    } else {
        let map = crate::storage::fields::load_term_ref_hydration_map_in_conn(
            conn,
            &term_ids,
            is_preview,
            Some(&record.locale),
        )
        .await?;
        for t in map.values() {
            if let Some(ref fid) = t.featured_image_media_id {
                media_ids.insert(fid.clone());
            }
        }
        map
    };
    let media_map = if media_ids.is_empty() {
        std::collections::BTreeMap::new()
    } else {
        crate::storage::fields::load_media_hydration_map_in_conn(conn, &media_ids).await?
    };
    let parent_map = if let Some(pid) = record.parent_id {
        batch_load_entry_parent_views_in_conn(conn, &[pid]).await?
    } else {
        std::collections::BTreeMap::new()
    };
    let hydration_ctx = crate::field_hydrate::HydrationContext {
        media: &media_map,
        entries: &entries_map,
        terms: &terms_hydrate_map,
        is_preview,
        tag_media_provenance: true,
    };
    let mut view = crate::content_render::build_hydrated_entry_view_with_eval_context(
        record,
        effective_schema,
        url,
        terms,
        eligible_public_taxonomies,
        max_view_role,
        &eval_ctx,
        Some(&hydration_ctx),
        richtext_nonces,
    );
    if let Some(pid) = record.parent_id
        && let Some((ptype, plocale, parent_view)) = parent_map.get(&pid)
        && ptype == &record.content_type_key
        && plocale == &record.locale
    {
        view.parent = Some(parent_view.clone());
    }
    Ok(if is_preview {
        crate::content_render::rewrite_preview_media_urls(view, &record.content_type_key)
    } else {
        view
    })
}

pub async fn hydrate_entry_views_with_options_in_conn(
    conn: &mut sqlx::SqliteConnection,
    records: Vec<ContentEntryRecord>,
    active_locale: Option<&str>,
    is_preview: bool,
    max_view_role: opendash_core::content::FieldRole,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<Vec<crate::content_render::EntryView>> {
    hydrate_entry_views_projected_in_conn(
        conn,
        records,
        active_locale,
        is_preview,
        max_view_role,
        richtext_nonces,
        &opendash_core::builder::LoopProjection::all(),
    )
    .await
}

pub async fn hydrate_entry_views_projected_in_conn(
    conn: &mut sqlx::SqliteConnection,
    records: Vec<ContentEntryRecord>,
    active_locale: Option<&str>,
    is_preview: bool,
    max_view_role: opendash_core::content::FieldRole,
    mut richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
    projection: &opendash_core::builder::LoopProjection,
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
    let eval_meta: Vec<(i64, String, Option<String>, Option<String>)> = records
        .iter()
        .map(|r| (r.id, r.content_type_key.clone(), r.page_layout_key.clone(), r.custom_template_key.clone()))
        .collect();

    let urls =
        crate::storage::seo::resolve_public_entry_paths_by_entry_ids_in_conn(conn, &entry_ids)
            .await?;
    let parent_map = if projection.needs_all_fields {
        let parent_ids: Vec<i64> = records.iter().filter_map(|r| r.parent_id).collect();
        batch_load_entry_parent_views_in_conn(conn, &parent_ids).await?
    } else {
        std::collections::BTreeMap::new()
    };
    let eval_map = batch_load_entry_eval_contexts_in_conn(conn, &eval_meta, max_view_role).await?;
    let load_terms = projection.needs_all_fields
        || projection.needs_terms
        || projection.needs_primary_category;
    let mut terms_map = if load_terms {
        batch_load_public_entry_terms_in_conn(conn, &pairs).await?
    } else {
        std::collections::BTreeMap::new()
    };
    let tax_map = if load_terms {
        batch_load_eligible_public_taxonomies_in_conn(conn, &distinct_types).await?
    } else {
        std::collections::BTreeMap::new()
    };
    let mut schemas = std::collections::BTreeMap::new();
    for tk in &distinct_types {
        if let Some(eff) =
            crate::storage::content::load_effective_content_schema_in_tx(conn, tk).await?
        {
            schemas.insert(tk.clone(), eff);
        }
    }

    let mut media_ids = std::collections::BTreeSet::new();
    let mut ref_entry_ids = std::collections::BTreeSet::new();
    let mut ref_term_ids = std::collections::BTreeSet::new();
    for record in &records {
        if let Some(eff) = schemas.get(&record.content_type_key)
            && let Some(eval_ctx) = eval_map.get(&record.id)
        {
            let ordered = crate::content_render::ordered_public_field_defs_for_entry(
                eff,
                max_view_role,
                eval_ctx,
            );
            let specs: Vec<opendash_core::field_schema::FieldSpec> =
                filter_specs_for_loop_projection(ordered, projection)
                    .into_iter()
                    .cloned()
                    .collect();
            crate::field_hydrate::collect_hydration_ids(
                &specs,
                &record.values,
                crate::field_hydrate::HydrationVisibility::ContentPublic { max_view_role },
                &mut media_ids,
                &mut ref_entry_ids,
                &mut ref_term_ids,
            );
        }
    }
    let locale_hint = active_locale.or_else(|| records.first().map(|r| r.locale.as_str()));
    let entries_map = if ref_entry_ids.is_empty() {
        std::collections::BTreeMap::new()
    } else {
        crate::storage::fields::load_entry_ref_hydration_map_in_conn(
            conn,
            &ref_entry_ids,
            is_preview,
            locale_hint,
        )
        .await?
    };
    let ref_terms_map = if ref_term_ids.is_empty() {
        std::collections::BTreeMap::new()
    } else {
        let map = crate::storage::fields::load_term_ref_hydration_map_in_conn(
            conn,
            &ref_term_ids,
            is_preview,
            locale_hint,
        )
        .await?;
        for t in map.values() {
            if let Some(ref fid) = t.featured_image_media_id {
                media_ids.insert(fid.clone());
            }
        }
        map
    };
    let media_map = if media_ids.is_empty() {
        std::collections::BTreeMap::new()
    } else {
        crate::storage::fields::load_media_hydration_map_in_conn(conn, &media_ids).await?
    };
    let hydration_ctx = crate::field_hydrate::HydrationContext {
        media: &media_map,
        entries: &entries_map,
        terms: &ref_terms_map,
        is_preview,
        tag_media_provenance: true,
    };

    let mut proj_map = if projection.needs_all_fields {
        crate::storage::plugin_projections::batch_load_entry_projections_in_conn(conn, &entry_ids)
            .await?
    } else {
        std::collections::BTreeMap::new()
    };

    let mut views = Vec::with_capacity(records.len());
    for record in records {
        let Some(eff) = schemas.get(&record.content_type_key) else {
            continue;
        };
        let eval_ctx = eval_map.get(&record.id).cloned().unwrap_or_default();
        let url = urls.get(&record.id).cloned();
        let terms = terms_map.remove(&record.id).unwrap_or_default();
        let eligible_taxonomies = tax_map
            .get(&record.content_type_key)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut view = crate::content_render::build_hydrated_entry_view_with_eval_context(
            &record,
            eff,
            url,
            &terms,
            eligible_taxonomies,
            max_view_role,
            &eval_ctx,
            Some(&hydration_ctx),
            richtext_nonces.as_deref_mut(),
        );
        if let Some(pid) = record.parent_id
            && let Some((ptype, plocale, parent_view)) = parent_map.get(&pid)
            && ptype == &record.content_type_key
            && plocale == &record.locale
        {
            view.parent = Some(parent_view.clone());
        }
        if is_preview {
            view =
                crate::content_render::rewrite_preview_media_urls(view, &record.content_type_key);
        }
        if let Some(proj) = proj_map.remove(&record.id) {
            view.ext = proj.ext;
            view.theme_ext = proj.theme_ext;
        }
        views.push(view);
    }
    Ok(views)
}

pub(in crate::storage) async fn hydrate_entry_views_in_conn(
    conn: &mut sqlx::SqliteConnection,
    records: Vec<ContentEntryRecord>,
) -> ApiResult<Vec<crate::content_render::EntryView>> {
    hydrate_entry_views_with_options_in_conn(
        conn,
        records,
        None,
        false,
        opendash_core::content::FieldRole::Editor,
        None,
    )
    .await
}
