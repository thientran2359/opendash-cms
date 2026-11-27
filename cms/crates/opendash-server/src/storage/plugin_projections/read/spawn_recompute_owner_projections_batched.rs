use super::*;

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
