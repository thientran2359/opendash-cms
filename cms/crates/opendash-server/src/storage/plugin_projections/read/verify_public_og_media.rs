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
