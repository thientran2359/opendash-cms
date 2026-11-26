use super::*;

pub(in crate::storage) async fn verify_public_og_media(
    db: &SqlitePool,
    og_image_id: &str,
) -> ApiResult<bool> {
    let active_img: Option<String> = sqlx::query_scalar(
        "SELECT id FROM media_assets WHERE id = ? AND kind = 'image' AND lifecycle_state = 'active'",
    )
    .bind(og_image_id)
    .fetch_optional(db)
    .await?;
    if active_img.is_none() {
        return Ok(false);
    }
    public_content::media_is_referenced(db, og_image_id).await
}

pub(in crate::storage) async fn load_published_entry_source(
    db: &SqlitePool,
    entry_id: i64,
) -> ApiResult<Option<EntryProjectionSource>> {
    let mut tx = db.begin().await?;
    let row = sqlx::query(
        "SELECT e.id, e.version, e.content_type_key, e.locale, e.translation_group_id,
                e.slug, e.title, e.values_json, e.published_at
           FROM content_entries e
           JOIN content_types t ON t.key = e.content_type_key
           JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1
            AND sl.deleted_at IS NULL
            AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
          WHERE e.id = ?
            AND e.trashed_at IS NULL
            AND e.publication_status = 'published'
            AND e.content_type_key != 'pattern'
            AND t.provider_status = 'active'
            AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
    )
    .bind(entry_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };

    let version: i64 = row.try_get("version")?;
    let content_type_key: String = row.try_get("content_type_key")?;
    let locale: String = row.try_get("locale")?;
    let translation_group_id: String = row.try_get("translation_group_id")?;
    let slug: String = row.try_get("slug")?;
    let title: String = row.try_get("title")?;
    let values_json: String = row.try_get("values_json")?;
    let published_at: Option<String> = row.try_get("published_at")?;

    let values: Value = serde_json::from_str(&values_json).unwrap_or_else(|_| json!({}));
    let term_ids: Vec<i64> =
        sqlx::query_scalar("SELECT term_id FROM entry_terms WHERE entry_id = ? ORDER BY term_id")
            .bind(entry_id)
            .fetch_all(&mut *tx)
            .await?;
    let gen_rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT id, revision_generation FROM plugins WHERE enabled = 1")
            .fetch_all(&mut *tx)
            .await?;
    let plugin_generations: BTreeMap<String, i64> = gen_rows.into_iter().collect();
    tx.commit().await?;

    let payload = json!({
        "id": entry_id,
        "entry_id": entry_id,
        "content_type_key": &content_type_key,
        "type_key": &content_type_key,
        "locale": &locale,
        "translation_group_id": &translation_group_id,
        "slug": &slug,
        "title": &title,
        "values": values,
        "term_ids": term_ids,
        "published_at": published_at,
    });

    Ok(Some(EntryProjectionSource {
        entry_id,
        version,
        content_type_key,
        locale,
        payload,
        plugin_generations,
    }))
}

/// Recheck eligibility, lifecycle, and source version after reserving the SQLite writer.
/// A trashed, draft, private, or missing entry clears stale public projections. A stale
/// computation whose entry version or locale moved on (including across trash+restore)
/// must neither overwrite nor erase projections belonging to the newer version.
pub(in crate::storage) async fn projection_write_allowed_in_tx(
    tx: &mut SqliteConnection,
    entry_id: i64,
    computed_locale: Option<&str>,
    expected_source_version: Option<i64>,
) -> Result<bool, sqlx::Error> {
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let current: Option<(String, i64, bool)> = sqlx::query_as(
        "SELECT e.locale, e.version, (sl.enabled = 1 AND sl.deleted_at IS NULL
                    AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1))
           FROM content_entries e
           JOIN content_types t ON t.key = e.content_type_key
           JOIN site_locales sl ON sl.code = e.locale
          WHERE e.id = ? AND e.trashed_at IS NULL AND e.publication_status = 'published'
            AND e.content_type_key != 'pattern'
            AND t.provider_status = 'active'
            AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
    )
    .bind(entry_id)
    .fetch_optional(&mut *tx)
    .await?;
    match current {
        None => {
            delete_entry_projections_in_tx(tx, entry_id).await?;
            Ok(false)
        }
        Some((locale, current_version, active)) => Ok(active
            && computed_locale == Some(locale.as_str())
            && expected_source_version.is_none_or(|ver| ver == current_version)),
    }
}

pub(in crate::storage) async fn write_entry_projections_in_tx(
    tx: &mut SqliteConnection,
    entry_id: i64,
    locale: &str,
    expected_source_version: i64,
    captured_plugin_generations: &BTreeMap<String, i64>,
    items: &[ComputedPluginProjection],
    owner_filter: Option<(ExtensionOwnerKind, &str, Option<&(String, i64)>)>,
) -> Result<(), sqlx::Error> {
    if !projection_write_allowed_in_tx(tx, entry_id, Some(locale), Some(expected_source_version))
        .await?
    {
        return Ok(());
    }
    if let Some((owner_kind, oid, theme_fence)) = owner_filter {
        let active = match owner_kind {
            ExtensionOwnerKind::Plugin => {
                let state: Option<(i64, i64)> =
                    sqlx::query_as("SELECT enabled, revision_generation FROM plugins WHERE id = ?")
                        .bind(oid)
                        .fetch_optional(&mut *tx)
                        .await?;
                matches!(
                    state,
                    Some((1, current_gen))
                        if captured_plugin_generations
                            .get(oid)
                            .is_none_or(|captured| *captured == current_gen)
                )
            }
            ExtensionOwnerKind::Theme => {
                if let Some((rev_id, sel_ver)) = theme_fence {
                    if rev_id.starts_with("dev:") {
                        let cur_theme: Option<String> = sqlx::query_scalar(
                            "SELECT r.theme_id
                               FROM theme_active_selection a
                               JOIN theme_revisions r ON r.revision_id = a.revision_id
                              WHERE a.id = 1",
                        )
                        .fetch_optional(&mut *tx)
                        .await?;
                        cur_theme.as_deref() == Some(oid)
                            && plugin_storage::verify_active_theme_fence_in_tx(
                                &mut *tx, rev_id, *sel_ver,
                            )
                            .await?
                    } else {
                        plugin_storage::verify_active_theme_fence_in_tx(&mut *tx, rev_id, *sel_ver)
                            .await?
                    }
                } else {
                    let cur_theme: Option<String> = sqlx::query_scalar(
                        "SELECT r.theme_id
                           FROM theme_active_selection a
                           JOIN theme_revisions r ON r.revision_id = a.revision_id
                          WHERE a.id = 1",
                    )
                    .fetch_optional(&mut *tx)
                    .await?;
                    cur_theme.as_deref() == Some(oid)
                }
            }
        };
        if !active {
            return Ok(());
        }
        if let Some(item) = items
            .iter()
            .find(|it| it.owner_kind == owner_kind && it.plugin_id == oid)
        {
            if item.owner_kind == ExtensionOwnerKind::Theme
                && let Some((ref rev_id, sel_ver)) = item.theme_fence
                && !plugin_storage::verify_active_theme_fence_in_tx(&mut *tx, rev_id, sel_ver)
                    .await?
            {
                return Ok(());
            }
            sqlx::query(
                "INSERT INTO plugin_entry_projections (entry_id, owner_kind, plugin_id, locale, seo_json, ext_json, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(entry_id, owner_kind, plugin_id) DO UPDATE SET
                    locale = excluded.locale,
                    seo_json = excluded.seo_json,
                    ext_json = excluded.ext_json,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            )
            .bind(entry_id)
            .bind(item.owner_kind.as_str())
            .bind(&item.plugin_id)
            .bind(locale)
            .bind(&item.seo_json)
            .bind(&item.ext_json)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "DELETE FROM plugin_entry_projections
                  WHERE entry_id = ? AND owner_kind = ? AND plugin_id = ?",
            )
            .bind(entry_id)
            .bind(owner_kind.as_str())
            .bind(oid)
            .execute(&mut *tx)
            .await?;
        }
        return Ok(());
    }

    let current_plugins: BTreeMap<String, i64> =
        sqlx::query_as("SELECT id, revision_generation FROM plugins WHERE enabled = 1")
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();

    let mut valid_items: Vec<&ComputedPluginProjection> = Vec::with_capacity(items.len());
    for item in items {
        match item.owner_kind {
            ExtensionOwnerKind::Plugin => {
                if !current_plugins
                    .get(&item.plugin_id)
                    .is_some_and(|&cur_gen| {
                        captured_plugin_generations
                            .get(&item.plugin_id)
                            .is_none_or(|captured| *captured == cur_gen)
                    })
                {
                    continue;
                }
            }
            ExtensionOwnerKind::Theme => {
                if let Some((ref rev_id, sel_ver)) = item.theme_fence {
                    if rev_id.starts_with("dev:") {
                        let cur_theme: Option<String> = sqlx::query_scalar(
                            "SELECT r.theme_id
                               FROM theme_active_selection a
                               JOIN theme_revisions r ON r.revision_id = a.revision_id
                              WHERE a.id = 1",
                        )
                        .fetch_optional(&mut *tx)
                        .await?;
                        if cur_theme.as_deref() != Some(item.plugin_id.as_str()) {
                            continue;
                        }
                    }
                    if !plugin_storage::verify_active_theme_fence_in_tx(&mut *tx, rev_id, sel_ver)
                        .await?
                    {
                        continue;
                    }
                }
            }
        }
        valid_items.push(item);
    }

    // Preserve rows belonging to plugins enabled or upgraded after our source snapshot.
    let mut keep_pairs: Vec<String> = valid_items
        .iter()
        .map(|it| format!("{}:{}", it.owner_kind.as_str(), it.plugin_id))
        .collect();
    for (pid, &cur_gen) in &current_plugins {
        if captured_plugin_generations.get(pid) != Some(&cur_gen) {
            keep_pairs.push(format!("plugin:{pid}"));
        }
    }

    if keep_pairs.is_empty() {
        delete_entry_projections_in_tx(tx, entry_id).await?;
        return Ok(());
    }

    let active_pairs_json = serde_json::to_string(&keep_pairs).unwrap_or_else(|_| "[]".into());
    sqlx::query(
        "DELETE FROM plugin_entry_projections
          WHERE entry_id = ?
            AND (owner_kind || ':' || plugin_id) NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(entry_id)
    .bind(&active_pairs_json)
    .execute(&mut *tx)
    .await?;

    for item in valid_items {
        sqlx::query(
            "INSERT INTO plugin_entry_projections (entry_id, owner_kind, plugin_id, locale, seo_json, ext_json, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(entry_id, owner_kind, plugin_id) DO UPDATE SET
                locale = excluded.locale,
                seo_json = excluded.seo_json,
                ext_json = excluded.ext_json,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        )
        .bind(entry_id)
        .bind(item.owner_kind.as_str())
        .bind(&item.plugin_id)
        .bind(locale)
        .bind(&item.seo_json)
        .bind(&item.ext_json)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

pub async fn recompute_entry_projections(
    db: &SqlitePool,
    hooks: &HookDispatcher,
    entry_id: i64,
) -> ApiResult<()> {
    let Some(source) = load_published_entry_source(db, entry_id).await? else {
        let mut tx = db.begin().await?;
        // A missing source can mean the locale is parked. Recheck under
        // the writer before deciding whether existing projections are stale.
        projection_write_allowed_in_tx(&mut tx, entry_id, None, None).await?;
        tx.commit().await?;
        crate::cache::SiteCacheEpoch::global().bump();
        return Ok(());
    };

    let _ = &source.content_type_key;
    let items = evaluate_entry_projections(db, hooks, &source, None).await?;
    let mut tx = db.begin().await?;
    write_entry_projections_in_tx(
        &mut tx,
        entry_id,
        &source.locale,
        source.version,
        &source.plugin_generations,
        &items,
        None,
    )
    .await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn_recompute_owner_projections_batched(
    db: SqlitePool,
    hooks: Arc<HookDispatcher>,
    coordinator: Arc<SiteCoordinator>,
    owner_kind: ExtensionOwnerKind,
    owner_id: String,
    theme_fence: Option<(String, i64)>,
    mut maintenance_cancel_rx: watch::Receiver<bool>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_id: i64 = 0;

        loop {
            if *shutdown_rx.borrow() {
                return;
            }
            if *maintenance_cancel_rx.borrow() {
                tokio::select! {
                    biased;
                    _ = crate::hooks::wait_watch_bool(&mut shutdown_rx, true) => return,
                    _ = crate::hooks::wait_watch_bool(&mut maintenance_cancel_rx, false) => {}
                }
                continue;
            }

            let still_active = match owner_kind {
                ExtensionOwnerKind::Plugin => {
                    let enabled: Option<i64> =
                        sqlx::query_scalar("SELECT enabled FROM plugins WHERE id = ?")
                            .bind(&owner_id)
                            .fetch_optional(&db)
                            .await
                            .ok()
                            .flatten();
                    enabled == Some(1)
                }
                ExtensionOwnerKind::Theme => {
                    if let Some((ref rev_id, sel_ver)) = theme_fence {
                        if let Ok(mut conn) = db.acquire().await {
                            plugin_storage::verify_active_theme_fence_in_tx(
                                &mut conn, rev_id, sel_ver,
                            )
                            .await
                            .unwrap_or(false)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            };
            if !still_active {
                return;
            }

            let entry_ids: Vec<i64> = match sqlx::query_scalar(
                "SELECT e.id
                   FROM content_entries e
                   JOIN content_types t ON t.key = e.content_type_key
                   JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1
                    AND sl.deleted_at IS NULL
                    AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                  WHERE e.id > ?
                    AND e.trashed_at IS NULL
                    AND e.publication_status = 'published'
                    AND e.content_type_key != 'pattern'
                    AND t.provider_status = 'active'
                    AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                  ORDER BY e.id ASC
                  LIMIT ?",
            )
            .bind(last_id)
            .bind(BATCH_RECOMPUTE_LIMIT)
            .fetch_all(&db)
            .await
            {
                Ok(ids) => ids,
                Err(err) => {
                    eprintln!(
                        "spawn_recompute_owner_projections_batched query error ({}:{owner_id}): {err}",
                        owner_kind.as_str()
                    );
                    return;
                }
            };

            if entry_ids.is_empty() {
                return;
            }

            let mut computed_batch: Vec<(
                i64,
                String,
                i64,
                BTreeMap<String, i64>,
                Vec<ComputedPluginProjection>,
            )> = Vec::with_capacity(entry_ids.len());

            for eid in entry_ids {
                last_id = eid;
                if *shutdown_rx.borrow() {
                    return;
                }
                if *maintenance_cancel_rx.borrow() {
                    tokio::select! {
                        biased;
                        _ = crate::hooks::wait_watch_bool(&mut shutdown_rx, true) => return,
                        _ = crate::hooks::wait_watch_bool(&mut maintenance_cancel_rx, false) => {}
                    }
                }
                if let Ok(Some(source)) = load_published_entry_source(&db, eid).await
                    && let Ok(items) = evaluate_entry_projections(
                        &db,
                        &hooks,
                        &source,
                        Some((owner_kind, &owner_id)),
                    )
                    .await
                {
                    computed_batch.push((
                        eid,
                        source.locale,
                        source.version,
                        source.plugin_generations,
                        items,
                    ));
                }
            }

            if computed_batch.is_empty() {
                continue;
            }

            let lease = loop {
                if *shutdown_rx.borrow() {
                    return;
                }
                match coordinator.admit_mutation() {
                    Ok(lease) => break lease,
                    Err(_) => {
                        tokio::select! {
                            biased;
                            _ = crate::hooks::wait_watch_bool(&mut shutdown_rx, true) => return,
                            _ = crate::hooks::wait_watch_bool(&mut maintenance_cancel_rx, false) => {}
                            _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
                        }
                    }
                }
            };

            if let Ok(mut tx) = db.begin().await {
                for (eid, locale, source_version, captured_gens, items) in &computed_batch {
                    if let Err(err) = write_entry_projections_in_tx(
                        &mut tx,
                        *eid,
                        locale,
                        *source_version,
                        captured_gens,
                        items,
                        Some((owner_kind, &owner_id, theme_fence.as_ref())),
                    )
                    .await
                    {
                        eprintln!(
                            "batch write projection error ({}:{owner_id}, {eid}): {err}",
                            owner_kind.as_str()
                        );
                        break;
                    }
                }
                if tx.commit().await.is_ok() {
                    crate::cache::SiteCacheEpoch::global().bump();
                }
            }
            drop(lease);
        }
    })
}

pub async fn batch_load_entry_projections_in_conn(
    conn: &mut SqliteConnection,
    entry_ids: &[i64],
) -> ApiResult<BTreeMap<i64, MergedEntryProjection>> {
    let mut out: BTreeMap<i64, MergedEntryProjection> = BTreeMap::new();
    if entry_ids.is_empty() {
        return Ok(out);
    }

    let ids_json =
        serde_json::to_string(entry_ids).map_err(|e| ApiError::internal(e.to_string()))?;
    // Theme rows apply first (0), then Plugin rows (1) so SEO precedence is
    // strictly Admin > Plugin > Theme.
    let rows = sqlx::query(
        "SELECT pep.entry_id, pep.owner_kind, pep.plugin_id, pep.seo_json, pep.ext_json
           FROM plugin_entry_projections pep
           JOIN json_each(?) ids ON ids.value = pep.entry_id
           JOIN content_entries e ON e.id = pep.entry_id
            AND e.trashed_at IS NULL
            AND e.publication_status = 'published'
           LEFT JOIN plugins p
             ON pep.owner_kind = 'plugin' AND p.id = pep.plugin_id AND p.enabled = 1
           LEFT JOIN theme_active_selection tas
             ON pep.owner_kind = 'theme' AND tas.id = 1
           LEFT JOIN theme_revisions tr
             ON pep.owner_kind = 'theme' AND tr.revision_id = tas.revision_id AND tr.theme_id = pep.plugin_id
          WHERE p.id IS NOT NULL OR tr.revision_id IS NOT NULL
          ORDER BY pep.entry_id ASC,
                   CASE WHEN pep.owner_kind = 'theme' THEN 0 ELSE 1 END ASC,
                   pep.plugin_id ASC",
    )
    .bind(&ids_json)
    .fetch_all(&mut *conn)
    .await?;

    let mut json_ld_bytes_per_entry: BTreeMap<i64, usize> = BTreeMap::new();

    for row in rows {
        let entry_id: i64 = row.try_get("entry_id")?;
        let owner_kind_str: String = row.try_get("owner_kind")?;
        let plugin_id: String = row.try_get("plugin_id")?;
        let seo_json: Option<String> = row.try_get("seo_json")?;
        let ext_json: Option<String> = row.try_get("ext_json")?;

        let merged = out.entry(entry_id).or_default();

        if let Some(raw_seo) = seo_json
            && let Ok(stored) = serde_json::from_str::<StoredSeoProjection>(&raw_seo)
        {
            let ov = merged
                .seo_override
                .get_or_insert_with(EntrySeoPluginOverride::default);
            if stored.meta.title.is_some() {
                ov.title = stored.meta.title;
            }
            if stored.meta.description.is_some() {
                ov.description = stored.meta.description;
            }
            if stored.meta.og_image_id.is_some() {
                ov.og_image_id = stored.meta.og_image_id;
            }
            if stored.meta.canonical_url.is_some() {
                ov.canonical_url = stored.meta.canonical_url;
            }
            ov.noindex |= stored.meta.noindex;

            let used_bytes = json_ld_bytes_per_entry.entry(entry_id).or_insert(0);
            for block in stored.json_ld {
                if merged.json_ld.len() >= MAX_JSON_LD_BLOCKS {
                    break;
                }
                if let Ok(len) = validate_json_ld_block(&block)
                    && used_bytes.saturating_add(len) <= MAX_JSON_LD_TOTAL_BYTES
                {
                    *used_bytes = used_bytes.saturating_add(len);
                    merged.json_ld.push(block);
                }
            }
        }

        if let Some(raw_ext) = ext_json
            && let Ok(ext_val) = serde_json::from_str::<Value>(&raw_ext)
            && ext_val.is_object()
        {
            if owner_kind_str == "theme" {
                if let Value::Object(map) = ext_val {
                    for (k, v) in map {
                        merged.theme_ext.insert(k, v);
                    }
                }
            } else {
                merged.ext.insert(plugin_id, ext_val);
            }
        }
    }

    Ok(out)
}

pub async fn batch_load_entry_projections(
    db: &SqlitePool,
    entry_ids: &[i64],
) -> ApiResult<BTreeMap<i64, MergedEntryProjection>> {
    if entry_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let mut conn = db.acquire().await?;
    batch_load_entry_projections_in_conn(&mut conn, entry_ids).await
}
