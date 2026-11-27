use super::*;

pub async fn hook_subscriptions(
    db: &SqlitePool,
    event: opendash_core::hooks::HookEvent,
) -> Result<Vec<String>, String> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, manifest_json FROM plugins WHERE enabled = 1 ORDER BY id")
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    for (id, json) in rows {
        match PluginManifest::from_json_str(&json) {
            Ok(manifest) if manifest.id == id && manifest.hooks.contains(&event) => ids.push(id),
            Ok(_) => {}
            Err(error) => eprintln!("Invalid subscription manifest {id}: {error}"),
        }
    }
    Ok(ids)
}

pub async fn load_all_enabled_plugins_for_cache(
    db: &SqlitePool,
) -> Result<Vec<CachedActivePluginRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT p.id, p.wasm_blob, p.manifest_json, p.revision_generation, r.revision_digest
         FROM plugins p
         LEFT JOIN plugin_revisions r ON r.id = p.active_revision_id
         WHERE p.enabled = 1
         ORDER BY p.id COLLATE BINARY ASC",
    )
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| CachedActivePluginRow {
            id: row.get("id"),
            wasm_blob: row.get("wasm_blob"),
            manifest_json: row.get("manifest_json"),
            revision_generation: row.get("revision_generation"),
            revision_digest: row.get("revision_digest"),
        })
        .collect())
}

pub async fn list_owner_logs(
    db: &SqlitePool,
    owner_kind: opendash_core::hooks::ExtensionOwnerKind,
    owner_id: &str,
    limit: i64,
) -> crate::error::ApiResult<Vec<OwnerLogEntry>> {
    let rows = sqlx::query(
        "SELECT id, correlation_id, level, message, created_at
           FROM plugin_logs
          WHERE owner_kind = ? AND plugin_id = ?
          ORDER BY id DESC
          LIMIT ?",
    )
    .bind(owner_kind.as_str())
    .bind(owner_id)
    .bind(limit)
    .fetch_all(db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(OwnerLogEntry {
            id: row.get("id"),
            correlation_id: row.get("correlation_id"),
            level: row.get("level"),
            message: row.get("message"),
            created_at: row.get("created_at"),
        });
    }
    Ok(out)
}

/// Synchronizes a mounted `--dev-plugin` manifest, declarations, admin extensions,
/// and blocks into SQLite without polluting immutable `plugin_revisions` or
/// persisting raw dev code bytes into `plugins.wasm_blob`.
pub async fn sync_dev_plugin_in_db(
    db_pool: &SqlitePool,
    snap: &crate::dev_mode::DevPluginSnapshot,
) -> crate::error::ApiResult<()> {
    let manifest = &snap.manifest;
    let mut tx = db_pool.begin_with("BEGIN IMMEDIATE").await?;

    let existing_committed: Option<Option<i64>> =
        sqlx::query_scalar("SELECT active_revision_id FROM plugins WHERE id = ?")
            .bind(&manifest.id)
            .fetch_optional(&mut *tx)
            .await?;

    if existing_committed.flatten().is_some() {
        // Preserve committed plugin revision metadata and executable blob untouched;
        // live Dev Mode execution reads code & manifest from DevOverlayConfig directly.
        sqlx::query(
            "UPDATE plugins
             SET enabled = 1,
                 last_error = NULL,
                 revision_generation = revision_generation + 1,
                 cas_version = cas_version + 1,
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(&manifest.id)
        .execute(&mut *tx)
        .await?;
    } else {
        // Dev-only plugin (never installed as a committed ZIP revision): store an empty
        // placeholder blob so raw dev code never enters SQLite or VACUUM INTO backups,
        // and cannot execute once the server restarts without --dev-plugin.
        let empty_blob: &[u8] = &[];
        sqlx::query(
            "INSERT INTO plugins (id, version, name, description, manifest_json, wasm_blob, enabled, last_error, revision_generation, cas_version, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 1, NULL, 1, 1, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                 version = excluded.version,
                 name = excluded.name,
                 description = excluded.description,
                 manifest_json = excluded.manifest_json,
                 wasm_blob = excluded.wasm_blob,
                 enabled = 1,
                 last_error = NULL,
                 revision_generation = plugins.revision_generation + 1,
                 cas_version = plugins.cas_version + 1,
                 updated_at = excluded.updated_at",
        )
        .bind(&manifest.id)
        .bind(&manifest.version)
        .bind(&manifest.name)
        .bind(&manifest.description)
        .bind(&snap.manifest_json)
        .bind(empty_blob)
        .execute(&mut *tx)
        .await?;
    }

    crate::storage::content::sync_plugin_cnt_declarations_in_tx(
        &mut tx,
        &manifest.id,
        &manifest.declarations,
    )
    .await?;
    crate::storage::admin_extensions::require_capacity_in_conn(
        &mut tx,
        "plugin",
        &manifest.id,
        snap.admin_extensions.as_ref(),
    )
    .await?;
    crate::storage::admin_extensions::sync_owner_in_conn(
        &mut tx,
        "plugin",
        &manifest.id,
        snap.admin_extensions.as_ref(),
        "active",
        &manifest
            .actions
            .iter()
            .map(|a| a.id.clone())
            .collect::<Vec<_>>(),
    )
    .await?;
    crate::storage::page::sync_owner_blocks_in_tx(
        &mut tx,
        "plugin",
        &manifest.id,
        ProviderStatus::Active,
        &snap.revision_digest,
        &snap.blocks,
    )
    .await?;
    crate::storage::page::resync_plugin_block_acls_in_tx(&mut tx).await?;
    crate::storage::plugin_projections::delete_plugin_projections_in_tx(&mut tx, &manifest.id)
        .await
        .map_err(|_| crate::error::ApiError::internal("Lỗi xóa plugin projections cũ."))?;

    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
}

pub async fn read_raw_owner_option_map(
    db: &SqlitePool,
    owner_kind: opendash_core::hooks::ExtensionOwnerKind,
    owner_id: &str,
) -> Result<BTreeMap<String, serde_json::Value>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT option_key, value_json FROM theme_option_values WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind.options_owner_kind())
    .bind(owner_id)
    .fetch_all(db)
    .await?;
    let mut map = BTreeMap::new();
    for (k, v_json) in rows {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json) {
            map.insert(k, v);
        }
    }
    Ok(map)
}

/// Rows and total share a single predicate, including the locale and provider
/// scope, rather than independently counting unfiltered rows.
pub(crate) async fn list_entries_for_host(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: crate::storage::entries::EntryFilters,
    published_only: bool,
    enforce_public_locale: bool,
) -> crate::error::ApiResult<(Vec<crate::storage::entries::ContentEntryRecord>, i64)> {
    let mut tx = db.begin().await?;
    let result = crate::storage::entries::list_host_entries_in_tx(
        &mut tx,
        type_key,
        limit,
        offset,
        filters,
        published_only,
        enforce_public_locale,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}

pub async fn is_actor_role_active(
    conn: &mut sqlx::SqliteConnection,
    user_id: i64,
    required_role: &str,
) -> Result<bool, sqlx::Error> {
    let role_opt: Option<String> = sqlx::query_scalar("SELECT role FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(conn)
        .await?;
    Ok(match (role_opt.as_deref(), required_role) {
        (Some("admin"), _) => true,
        (Some("editor"), "editor") => true,
        _ => false,
    })
}
