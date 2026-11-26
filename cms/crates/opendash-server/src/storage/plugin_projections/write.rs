use super::*;

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
    sqlx::query(
        "DELETE FROM plugin_entry_projections WHERE owner_kind = 'plugin' AND plugin_id = ?",
    )
    .bind(plugin_id)
    .execute(tx)
    .await?;
    Ok(())
}

pub(in crate::storage) fn validate_entry_ext_value(val: &Value) -> Result<Value, &'static str> {
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

pub(in crate::storage) async fn evaluate_entry_projections(
    db: &SqlitePool,
    hooks: &HookDispatcher,
    source: &EntryProjectionSource,
    owner_filter: Option<(ExtensionOwnerKind, &str)>,
) -> ApiResult<Vec<ComputedPluginProjection>> {
    let seo_raw = hooks
        .dispatch_projection_filters_for_owners(
            HookEvent::FilterSeoHead,
            owner_filter,
            source.payload.clone(),
        )
        .await?;
    let ext_raw = hooks
        .dispatch_projection_filters_for_owners(
            HookEvent::FilterEntryView,
            owner_filter,
            source.payload.clone(),
        )
        .await?;

    #[allow(clippy::type_complexity)]
    let mut per_owner: BTreeMap<
        (ExtensionOwnerKind, String),
        (Option<(String, i64)>, Option<String>, Option<String>),
    > = BTreeMap::new();

    for (owner_kind, pid, fence, raw_val) in seo_raw {
        let parsed: SeoHeadFilterOutput = match serde_json::from_value(raw_val) {
            Ok(v) => v,
            Err(_) => {
                plugin_storage::record_owner_execution_result(
                    db,
                    owner_kind,
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
                plugin_storage::record_owner_execution_result(
                    db,
                    owner_kind,
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
            plugin_storage::record_owner_execution_result(
                db,
                owner_kind,
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
                plugin_storage::record_owner_execution_result(
                    db,
                    owner_kind,
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
        let slot = per_owner.entry((owner_kind, pid)).or_default();
        if slot.0.is_none() {
            slot.0 = fence;
        }
        slot.1 = Some(seo_str);
    }

    for (owner_kind, pid, fence, raw_val) in ext_raw {
        match validate_entry_ext_value(&raw_val) {
            Ok(valid) => {
                let ext_str =
                    serde_json::to_string(&valid).map_err(|e| ApiError::internal(e.to_string()))?;
                let slot = per_owner.entry((owner_kind, pid)).or_default();
                if slot.0.is_none() {
                    slot.0 = fence;
                }
                slot.2 = Some(ext_str);
            }
            Err(err_msg) => {
                plugin_storage::record_owner_execution_result(
                    db,
                    owner_kind,
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

    Ok(per_owner
        .into_iter()
        .map(
            |((owner_kind, plugin_id), (theme_fence, seo_json, ext_json))| {
                ComputedPluginProjection {
                    owner_kind,
                    plugin_id,
                    theme_fence,
                    seo_json,
                    ext_json,
                }
            },
        )
        .collect())
}

pub async fn spawn_recompute_plugin_projections_batched(
    db: SqlitePool,
    hooks: Arc<HookDispatcher>,
    coordinator: Arc<SiteCoordinator>,
    plugin_id: String,
    maintenance_cancel_rx: watch::Receiver<bool>,
    shutdown_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    spawn_recompute_owner_projections_batched(
        db,
        hooks,
        coordinator,
        ExtensionOwnerKind::Plugin,
        plugin_id,
        None,
        maintenance_cancel_rx,
        shutdown_rx,
    )
    .await
}
