use std::{collections::BTreeMap, sync::Arc};

use opendash_core::hooks::HookEvent;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection, SqlitePool};
use tokio::sync::watch;

use crate::{
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    operations::SiteCoordinator,
    seo::{
        MAX_JSON_LD_BLOCKS, MAX_JSON_LD_TOTAL_BYTES, SeoMetadataInput, json_value_depth,
        validate_json_ld_block, validate_json_ld_blocks, validate_seo_input,
    },
    storage::{plugin as plugin_storage, public_content},
};

pub const MAX_ENTRY_EXT_BYTES: usize = 32 * 1024;
pub const MAX_ENTRY_EXT_DEPTH: usize = 6;
pub const MAX_ENTRY_EXT_KEYS: usize = 64;
const BATCH_RECOMPUTE_LIMIT: i64 = 100;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeoHeadFilterOutput {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub og_image_id: Option<String>,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub noindex: Option<bool>,
    #[serde(default)]
    pub json_ld: Vec<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntrySeoPluginOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub og_image_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub noindex: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StoredSeoProjection {
    #[serde(flatten)]
    meta: EntrySeoPluginOverride,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    json_ld: Vec<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct MergedEntryProjection {
    pub seo_override: Option<EntrySeoPluginOverride>,
    pub json_ld: Vec<Value>,
    pub ext: BTreeMap<String, Value>,
}

struct EntryProjectionSource {
    entry_id: i64,
    version: i64,
    content_type_key: String,
    locale: String,
    payload: Value,
    plugin_generations: BTreeMap<String, i64>,
}

pub async fn delete_entry_projections_in_tx(
    tx: &mut SqliteConnection,
    entry_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM plugin_entry_projections WHERE entry_id = ?")
        .bind(entry_id)
        .execute(tx)
        .await?;
    Ok(())
}

pub async fn delete_plugin_projections_in_tx(
    tx: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM plugin_entry_projections WHERE plugin_id = ?")
        .bind(plugin_id)
        .execute(tx)
        .await?;
    Ok(())
}

async fn verify_public_og_media(db: &SqlitePool, og_image_id: &str) -> ApiResult<bool> {
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

fn validate_entry_ext_value(val: &Value) -> Result<Value, &'static str> {
    let Some(map) = val.as_object() else {
        return Err("Đầu ra filter:entry/view bắt buộc phải là JSON Object.");
    };
    if map.len() > MAX_ENTRY_EXT_KEYS {
        return Err("Đầu ra filter:entry/view vượt quá tối đa 64 keys.");
    }
    if json_value_depth(val) > MAX_ENTRY_EXT_DEPTH {
        return Err("Đầu ra filter:entry/view vượt quá độ sâu tối đa 6 tầng.");
    }
    let bytes = serde_json::to_vec(val)
        .map(|v| v.len())
        .map_err(|_| "Không thể serialize filter:entry/view.")?;
    if bytes > MAX_ENTRY_EXT_BYTES {
        return Err("Đầu ra filter:entry/view vượt quá giới hạn 32 KiB.");
    }
    Ok(val.clone())
}

async fn load_published_entry_source(
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

struct ComputedPluginProjection {
    plugin_id: String,
    seo_json: Option<String>,
    ext_json: Option<String>,
}

async fn evaluate_entry_projections(
    db: &SqlitePool,
    hooks: &HookDispatcher,
    source: &EntryProjectionSource,
    plugin_filter: Option<&str>,
) -> ApiResult<Vec<ComputedPluginProjection>> {
    let seo_raw = hooks
        .dispatch_projection_filters(
            HookEvent::FilterSeoHead,
            plugin_filter,
            source.payload.clone(),
        )
        .await?;
    let ext_raw = hooks
        .dispatch_projection_filters(
            HookEvent::FilterEntryView,
            plugin_filter,
            source.payload.clone(),
        )
        .await?;

    let mut per_plugin: BTreeMap<String, (Option<String>, Option<String>)> = BTreeMap::new();

    for (pid, raw_val) in seo_raw {
        let parsed: SeoHeadFilterOutput = match serde_json::from_value(raw_val) {
            Ok(v) => v,
            Err(_) => {
                plugin_storage::record_plugin_execution_result(
                    db,
                    &pid,
                    "projection:filter_seo_head",
                    "error",
                    "Đầu ra filter:seo/head không đúng schema.",
                    Some("Đầu ra filter:seo/head không đúng schema."),
                )
                .await;
                continue;
            }
        };

        let meta_input = SeoMetadataInput {
            title: parsed.title,
            description: parsed.description,
            og_image_id: parsed.og_image_id,
            canonical_url: parsed.canonical_url,
            noindex: parsed.noindex.unwrap_or(false),
        };
        let mut clean_meta = match validate_seo_input(&meta_input) {
            Ok(clean) => clean,
            Err(ApiError(_, msg)) => {
                plugin_storage::record_plugin_execution_result(
                    db,
                    &pid,
                    "projection:filter_seo_head",
                    "error",
                    &msg,
                    Some(&msg),
                )
                .await;
                continue;
            }
        };

        if let Some(ref mid) = clean_meta.og_image_id
            && !verify_public_og_media(db, mid).await?
        {
            let msg =
                format!("og_image_id '{mid}' không tồn tại hoặc chưa được công khai trên site.");
            plugin_storage::record_plugin_execution_result(
                db,
                &pid,
                "projection:filter_seo_head",
                "error",
                &msg,
                Some(&msg),
            )
            .await;
            clean_meta.og_image_id = None;
        }

        let valid_json_ld = match validate_json_ld_blocks(&parsed.json_ld) {
            Ok(blocks) => blocks,
            Err(err_msg) => {
                plugin_storage::record_plugin_execution_result(
                    db,
                    &pid,
                    "projection:filter_seo_head",
                    "error",
                    err_msg,
                    Some(err_msg),
                )
                .await;
                continue;
            }
        };

        let stored = StoredSeoProjection {
            meta: EntrySeoPluginOverride {
                title: clean_meta.title,
                description: clean_meta.description,
                og_image_id: clean_meta.og_image_id,
                canonical_url: clean_meta.canonical_url,
                noindex: clean_meta.noindex,
            },
            json_ld: valid_json_ld,
        };
        let seo_str =
            serde_json::to_string(&stored).map_err(|e| ApiError::internal(e.to_string()))?;
        per_plugin.entry(pid).or_default().0 = Some(seo_str);
    }

    for (pid, raw_val) in ext_raw {
        match validate_entry_ext_value(&raw_val) {
            Ok(valid) => {
                let ext_str =
                    serde_json::to_string(&valid).map_err(|e| ApiError::internal(e.to_string()))?;
                per_plugin.entry(pid).or_default().1 = Some(ext_str);
            }
            Err(err_msg) => {
                plugin_storage::record_plugin_execution_result(
                    db,
                    &pid,
                    "projection:filter_entry_view",
                    "error",
                    err_msg,
                    Some(err_msg),
                )
                .await;
            }
        }
    }

    Ok(per_plugin
        .into_iter()
        .map(
            |(plugin_id, (seo_json, ext_json))| ComputedPluginProjection {
                plugin_id,
                seo_json,
                ext_json,
            },
        )
        .collect())
}

/// Recheck eligibility, lifecycle, and source version after reserving the SQLite writer.
/// A trashed, draft, private, or missing entry clears stale public projections. A stale
/// computation whose entry version or locale moved on (including across trash+restore)
/// must neither overwrite nor erase projections belonging to the newer version.
async fn projection_write_allowed_in_tx(
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

async fn write_entry_projections_in_tx(
    tx: &mut SqliteConnection,
    entry_id: i64,
    locale: &str,
    expected_source_version: i64,
    captured_plugin_generations: &BTreeMap<String, i64>,
    items: &[ComputedPluginProjection],
    plugin_filter: Option<&str>,
) -> Result<(), sqlx::Error> {
    if !projection_write_allowed_in_tx(tx, entry_id, Some(locale), Some(expected_source_version))
        .await?
    {
        return Ok(());
    }
    if let Some(pid) = plugin_filter {
        let state: Option<(i64, i64)> =
            sqlx::query_as("SELECT enabled, revision_generation FROM plugins WHERE id = ?")
                .bind(pid)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((1, current_gen)) = state else {
            return Ok(());
        };
        if captured_plugin_generations
            .get(pid)
            .is_some_and(|captured| *captured != current_gen)
        {
            return Ok(());
        }
        if let Some(item) = items.iter().find(|it| it.plugin_id == pid) {
            sqlx::query(
                "INSERT INTO plugin_entry_projections (entry_id, plugin_id, locale, seo_json, ext_json, updated_at)
                 VALUES (?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(entry_id, plugin_id) DO UPDATE SET
                    locale = excluded.locale,
                    seo_json = excluded.seo_json,
                    ext_json = excluded.ext_json,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            )
            .bind(entry_id)
            .bind(&item.plugin_id)
            .bind(locale)
            .bind(&item.seo_json)
            .bind(&item.ext_json)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "DELETE FROM plugin_entry_projections WHERE entry_id = ? AND plugin_id = ?",
            )
            .bind(entry_id)
            .bind(pid)
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

    let mut fresh_items = Vec::new();
    for item in items {
        if let Some(&cur_gen) = current_plugins.get(&item.plugin_id)
            && captured_plugin_generations
                .get(&item.plugin_id)
                .is_none_or(|captured| *captured == cur_gen)
        {
            fresh_items.push(item);
        }
    }

    // Preserve rows belonging to plugins enabled or upgraded after our source snapshot.
    let mut keep_ids: Vec<&str> = fresh_items.iter().map(|it| it.plugin_id.as_str()).collect();
    for (pid, &cur_gen) in &current_plugins {
        if captured_plugin_generations.get(pid) != Some(&cur_gen) {
            keep_ids.push(pid.as_str());
        }
    }
    if keep_ids.is_empty() {
        delete_entry_projections_in_tx(tx, entry_id).await?;
        return Ok(());
    }

    let keep_ids_json = serde_json::to_string(&keep_ids).unwrap_or_else(|_| "[]".into());
    sqlx::query(
        "DELETE FROM plugin_entry_projections
          WHERE entry_id = ?
            AND plugin_id NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(entry_id)
    .bind(&keep_ids_json)
    .execute(&mut *tx)
    .await?;

    for item in fresh_items {
        sqlx::query(
            "INSERT INTO plugin_entry_projections (entry_id, plugin_id, locale, seo_json, ext_json, updated_at)
             VALUES (?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(entry_id, plugin_id) DO UPDATE SET
                locale = excluded.locale,
                seo_json = excluded.seo_json,
                ext_json = excluded.ext_json,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        )
        .bind(entry_id)
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
    Ok(())
}

pub async fn spawn_recompute_plugin_projections_batched(
    db: SqlitePool,
    hooks: Arc<HookDispatcher>,
    coordinator: Arc<SiteCoordinator>,
    plugin_id: String,
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

            let enabled: Option<i64> =
                sqlx::query_scalar("SELECT enabled FROM plugins WHERE id = ?")
                    .bind(&plugin_id)
                    .fetch_optional(&db)
                    .await
                    .ok()
                    .flatten();
            if enabled != Some(1) {
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
                        "spawn_recompute_plugin_projections_batched query error ({plugin_id}): {err}"
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
                    && let Ok(items) =
                        evaluate_entry_projections(&db, &hooks, &source, Some(&plugin_id)).await
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
                        Some(&plugin_id),
                    )
                    .await
                    {
                        eprintln!("batch write projection error ({plugin_id}, {eid}): {err}");
                        break;
                    }
                }
                let _ = tx.commit().await;
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
    let rows = sqlx::query(
        "SELECT pep.entry_id, pep.plugin_id, pep.seo_json, pep.ext_json
           FROM plugin_entry_projections pep
           JOIN json_each(?) ids ON ids.value = pep.entry_id
           JOIN content_entries e ON e.id = pep.entry_id
            AND e.trashed_at IS NULL
            AND e.publication_status = 'published'
           JOIN plugins p ON p.id = pep.plugin_id AND p.enabled = 1
          ORDER BY pep.entry_id ASC, pep.plugin_id ASC",
    )
    .bind(&ids_json)
    .fetch_all(&mut *conn)
    .await?;

    let mut json_ld_bytes_per_entry: BTreeMap<i64, usize> = BTreeMap::new();

    for row in rows {
        let entry_id: i64 = row.try_get("entry_id")?;
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
            merged.ext.insert(plugin_id, ext_val);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn projection_locale_gate_preserves_parked_rows_and_rejects_stale_writes() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO site_locales(code,bcp47_tag,label) VALUES('en','en','English')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE site_locales_state SET multilingual_enabled = 1 WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO plugins (id,name,version,manifest_json,wasm_blob,enabled)
             VALUES('projection-test','Projection Test','1.0.0','{}',X'00',1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO content_entries
             (id,content_type_key,locale,translation_group_id,slug,title,values_json,publication_status)
             VALUES(1,'post','en','grp-en','example','Example','{}','draft')",
        ).execute(&pool).await.unwrap();
        sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        let source = load_published_entry_source(&pool, 1)
            .await
            .unwrap()
            .unwrap();
        let items = vec![ComputedPluginProjection {
            plugin_id: "projection-test".into(),
            seo_json: None,
            ext_json: Some(r#"{"badge":"original"}"#.into()),
        }];
        let mut tx = pool.begin().await.unwrap();
        write_entry_projections_in_tx(
            &mut tx,
            1,
            &source.locale,
            source.version,
            &source.plugin_generations,
            &items,
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        for (toggle, locale_enabled, deleted_at) in
            [(0, 1, None), (1, 0, None), (1, 0, Some("2026-01-01"))]
        {
            let mut tx = pool.begin().await.unwrap();
            sqlx::query("UPDATE site_locales_state SET multilingual_enabled = ? WHERE id = 1")
                .bind(toggle)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("UPDATE site_locales SET enabled = ?, deleted_at = ? WHERE code = 'en'")
                .bind(locale_enabled)
                .bind(deleted_at)
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            assert!(
                load_published_entry_source(&pool, 1)
                    .await
                    .unwrap()
                    .is_none()
            );

            let mut tx = pool.begin().await.unwrap();
            // Neither the missing-source cleanup nor an in-flight empty
            // computation may erase parked projections.
            assert!(
                !projection_write_allowed_in_tx(&mut tx, 1, None, None)
                    .await
                    .unwrap()
            );
            write_entry_projections_in_tx(
                &mut tx,
                1,
                "en",
                source.version,
                &source.plugin_generations,
                &[],
                None,
            )
            .await
            .unwrap();
            write_entry_projections_in_tx(
                &mut tx,
                1,
                "en",
                source.version,
                &source.plugin_generations,
                &[],
                Some("projection-test"),
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            let stored: String = sqlx::query_scalar(
                "SELECT ext_json FROM plugin_entry_projections WHERE entry_id = 1",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(stored, r#"{"badge":"original"}"#);
        }

        let mut tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE site_locales SET enabled = 1, deleted_at = NULL WHERE code = 'en'")
            .execute(&mut *tx)
            .await
            .unwrap();
        // A stale computed locale must not overwrite or erase the current one.
        write_entry_projections_in_tx(
            &mut tx,
            1,
            "vi",
            source.version,
            &source.plugin_generations,
            &[],
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert!(
            load_published_entry_source(&pool, 1)
                .await
                .unwrap()
                .is_some()
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_entry_projections")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);

        let mut tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE content_entries SET publication_status = 'draft' WHERE id = 1")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            !projection_write_allowed_in_tx(&mut tx, 1, None, None)
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_entry_projections")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn stale_projection_cannot_reinsert_during_trash_or_overwrite_after_restore() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO plugins (id,name,version,manifest_json,wasm_blob,enabled,revision_generation)
             VALUES('projection-race','Projection Race','1.0.0','{}',X'00',1,1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO content_entries
             (id,content_type_key,locale,translation_group_id,slug,title,values_json,publication_status,version)
             VALUES(1,'post','vi','grp-race','race-post','Race Post','{}','draft',1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE content_entries SET publication_status = 'published', version = 2 WHERE id = 1",
        )
        .execute(&pool)
        .await
        .unwrap();

        let v2_source = load_published_entry_source(&pool, 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v2_source.version, 2);
        let v2_items = vec![ComputedPluginProjection {
            plugin_id: "projection-race".into(),
            seo_json: None,
            ext_json: Some(r#"{"ver":"v2"}"#.into()),
        }];

        // Entry is trashed (v3): pre-trash v2 computation arrives late and must NOT write.
        sqlx::query(
            "UPDATE content_entries SET trashed_at = datetime('now'), version = 3 WHERE id = 1",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            load_published_entry_source(&pool, 1)
                .await
                .unwrap()
                .is_none()
        );
        {
            let mut tx = pool.begin().await.unwrap();
            write_entry_projections_in_tx(
                &mut tx,
                1,
                &v2_source.locale,
                v2_source.version,
                &v2_source.plugin_generations,
                &v2_items,
                None,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }
        let count_trashed: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM plugin_entry_projections WHERE entry_id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count_trashed, 0);

        // Entry is restored (v4) and republished (v5) with fresh projection {"ver":"v5"}.
        sqlx::query(
            "UPDATE content_entries SET trashed_at = NULL, publication_status = 'published', version = 5 WHERE id = 1",
        )
        .execute(&pool)
        .await
        .unwrap();
        let v5_source = load_published_entry_source(&pool, 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v5_source.version, 5);
        let v5_items = vec![ComputedPluginProjection {
            plugin_id: "projection-race".into(),
            seo_json: None,
            ext_json: Some(r#"{"ver":"v5"}"#.into()),
        }];
        {
            let mut tx = pool.begin().await.unwrap();
            write_entry_projections_in_tx(
                &mut tx,
                1,
                &v5_source.locale,
                v5_source.version,
                &v5_source.plugin_generations,
                &v5_items,
                None,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }

        // Pre-trash v2 computation arrives after restore+republish: must NEITHER overwrite NOR erase v5!
        {
            let mut tx = pool.begin().await.unwrap();
            write_entry_projections_in_tx(
                &mut tx,
                1,
                &v2_source.locale,
                v2_source.version,
                &v2_source.plugin_generations,
                &v2_items,
                None,
            )
            .await
            .unwrap();
            write_entry_projections_in_tx(
                &mut tx,
                1,
                &v2_source.locale,
                v2_source.version,
                &v2_source.plugin_generations,
                &[],
                Some("projection-race"),
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }
        let stored_v5: String = sqlx::query_scalar(
            "SELECT ext_json FROM plugin_entry_projections WHERE entry_id = 1 AND plugin_id = 'projection-race'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stored_v5, r#"{"ver":"v5"}"#);

        // Plugin generation advances from 1 to 2: stale gen-1 compute for v5 must not overwrite gen-2 row.
        sqlx::query("UPDATE plugins SET revision_generation = 2 WHERE id = 'projection-race'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE plugin_entry_projections SET ext_json = '{\"ver\":\"v5-gen2\"}' WHERE entry_id = 1",
        )
        .execute(&pool)
        .await
        .unwrap();
        {
            let mut tx = pool.begin().await.unwrap();
            write_entry_projections_in_tx(
                &mut tx,
                1,
                &v5_source.locale,
                v5_source.version,
                &v5_source.plugin_generations,
                &v5_items,
                Some("projection-race"),
            )
            .await
            .unwrap();
            write_entry_projections_in_tx(
                &mut tx,
                1,
                &v5_source.locale,
                v5_source.version,
                &v5_source.plugin_generations,
                &v5_items,
                None,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }
        let stored_gen2: String = sqlx::query_scalar(
            "SELECT ext_json FROM plugin_entry_projections WHERE entry_id = 1 AND plugin_id = 'projection-race'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stored_gen2, r#"{"ver":"v5-gen2"}"#);
    }

    #[tokio::test]
    async fn public_seo_read_uses_projections_with_zero_wasm_and_monotonic_noindex() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

        let trap_wasm = wat::parse_str(
            r#"(module
                (memory (export "memory") 1)
                (func (export "opendash_run") (result i32)
                    unreachable)
            )"#,
        )
        .unwrap();
        let manifest_json = json!({
            "id": "trap-seo-ext",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Trap SEO Ext",
            "filters": ["filter:seo/head", "filter:entry/view"],
            "permissions": { "content_read": ["post"] }
        })
        .to_string();
        sqlx::query(
            "INSERT INTO plugins (id, name, version, manifest_json, wasm_blob, enabled)
             VALUES (?, ?, ?, ?, ?, 1)",
        )
        .bind("trap-seo-ext")
        .bind("Trap SEO Ext")
        .bind("1.0.0")
        .bind(&manifest_json)
        .bind(&trap_wasm)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO content_entries
             (id, content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, version)
             VALUES (1, 'post', 'vi', 'grp-1', 'zero-wasm-post', 'Zero Wasm Post', '{}', 'draft', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO entry_seo (entry_id, noindex) VALUES (1, 1)")
            .execute(&pool)
            .await
            .unwrap();
        let stored_seo = json!({
            "title": "Plugin SEO Title",
            "description": "Plugin Desc",
            "noindex": false,
            "json_ld": [{
                "@context": "https://schema.org",
                "@type": "Article",
                "headline": "Bad</script><script>"
            }]
        })
        .to_string();
        let stored_ext = json!({ "badge": "hot" }).to_string();
        sqlx::query(
            "INSERT INTO plugin_entry_projections (entry_id, plugin_id, locale, seo_json, ext_json)
             VALUES (1, 'trap-seo-ext', 'vi', ?, ?)",
        )
        .bind(&stored_seo)
        .bind(&stored_ext)
        .execute(&pool)
        .await
        .unwrap();

        // Reading SEO & projections never spawns Wasm (trap_wasm would fail if invoked).
        let seo_view = crate::storage::seo::build_entry_seo_view_for_locale(
            &pool,
            1,
            "Zero Wasm Post",
            Some("vi"),
            "/zero-wasm-post",
        )
        .await
        .unwrap();
        assert_eq!(seo_view.title, "Plugin SEO Title");
        assert_eq!(seo_view.description.as_deref(), Some("Plugin Desc"));
        assert_eq!(seo_view.robots, "noindex,nofollow");
        assert_eq!(seo_view.json_ld.len(), 1);
        let escaped = crate::content_render::serialize_json_ld_safe(&seo_view.json_ld[0]);
        assert!(escaped.contains(r"\u003c/script\u003e"));
        assert!(!escaped.contains("</script>"));

        let loaded = batch_load_entry_projections(&pool, &[1]).await.unwrap();
        assert_eq!(
            loaded[&1].ext["trap-seo-ext"]["badge"],
            serde_json::json!("hot")
        );
    }
}
