use super::*;

pub async fn install_plugin_revision(
    db_pool: &SqlitePool,
    plugins_root: &Path,
    manifest: &PluginManifest,
    manifest_json: &str,
    wasm_bytes: &[u8],
    extensions: Option<&opendash_core::admin_extensions::AdminExtensions>,
    blocks: &[(BlockSchemaV1, String)],
) -> Result<i64, PluginStorageError> {
    let blocks_json = canonical_blocks_json(&manifest.id, blocks)?;
    // Fail combined-cap checks before publishing a durable ZIP revision.
    crate::storage::admin_extensions::require_capacity(db_pool, "plugin", &manifest.id, extensions)
        .await
        .map_err(|e| {
            if e.0 == axum::http::StatusCode::CONFLICT {
                PluginStorageError::Conflict(e.1.into_owned())
            } else {
                PluginStorageError::Internal(e.1.into_owned())
            }
        })?;
    let revision_digest = compute_plugin_revision_digest_with_blocks_json(
        manifest_json,
        wasm_bytes,
        extensions,
        blocks_json.as_deref(),
    );
    let run_uuid = crate::auth::generate_random_token();

    // Check version collision before staging on disk
    let existing_by_version: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, revision_digest FROM plugin_revisions WHERE plugin_id = ? AND version = ?",
    )
    .bind(&manifest.id)
    .bind(&manifest.version)
    .fetch_optional(db_pool)
    .await
    .map_err(|e| PluginStorageError::Internal(format!("Lỗi truy vấn plugin_revisions: {e}")))?;

    if let Some((_, existing_digest)) = &existing_by_version {
        if existing_digest != &revision_digest {
            return Err(PluginStorageError::Conflict(format!(
                "Phiên bản '{}' của plugin '{}' đã tồn tại với nội dung khác.",
                manifest.version, manifest.id
            )));
        }
    }

    // Catch ownership conflicts before staging an immutable directory. The
    // transaction below repeats the check atomically with the registry write.
    for (schema, _) in blocks {
        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT owner_kind, owner_id FROM builder_block_definitions WHERE block_id = ?",
        )
        .bind(&schema.id)
        .fetch_optional(db_pool)
        .await
        .map_err(|e| PluginStorageError::Internal(e.to_string()))?;
        if let Some((kind, owner_id)) = existing
            && (kind != "plugin" || owner_id != manifest.id)
        {
            return Err(PluginStorageError::Conflict(
                "ID block đã thuộc quyền sở hữu của gói khác.".into(),
            ));
        }
    }

    stage_and_publish_plugin_revision_with_blocks(
        plugins_root,
        &run_uuid,
        &revision_digest,
        manifest_json,
        wasm_bytes,
        extensions,
        blocks_json.as_deref(),
    )
    .map_err(PluginStorageError::Internal)?;

    let mut tx = db_pool
        .begin()
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi mở transaction plugin: {e}")))?;

    // Upsert plugin row first so FK plugin_revisions(plugin_id) -> plugins(id) is satisfied.
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
    .bind(manifest_json)
    .bind(wasm_bytes)
    .execute(&mut *tx)
    .await
    .map_err(|e| PluginStorageError::Internal(format!("Lỗi lưu plugin vào database: {e}")))?;

    let existing_rev: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, revision_digest FROM plugin_revisions WHERE plugin_id = ? AND version = ?",
    )
    .bind(&manifest.id)
    .bind(&manifest.version)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| PluginStorageError::Internal(format!("Lỗi kiểm tra revision plugin: {e}")))?;

    let revision_id = if let Some((rev_id, existing_digest)) = existing_rev {
        if existing_digest != revision_digest {
            return Err(PluginStorageError::Conflict(format!(
                "Phiên bản '{}' của plugin '{}' đã tồn tại với nội dung khác.",
                manifest.version, manifest.id
            )));
        }
        rev_id
    } else {
        let res = sqlx::query(
            "INSERT INTO plugin_revisions (plugin_id, version, revision_digest, manifest_schema, manifest_json, blocks_json)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&manifest.id)
        .bind(&manifest.version)
        .bind(&revision_digest)
        .bind(manifest.manifest_schema as i64)
        .bind(manifest_json)
        .bind(blocks_json.as_deref().unwrap_or("{}"))
        .execute(&mut *tx)
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi tạo plugin_revisions: {e}")))?;
        res.last_insert_rowid()
    };

    sqlx::query("UPDATE plugins SET active_revision_id = ? WHERE id = ?")
        .bind(revision_id)
        .bind(&manifest.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            PluginStorageError::Internal(format!("Lỗi cập nhật active_revision_id: {e}"))
        })?;

    crate::storage::content::sync_plugin_cnt_declarations_in_tx(
        &mut tx,
        &manifest.id,
        &manifest.declarations,
    )
    .await
    .map_err(map_cnt_err)?;
    crate::storage::admin_extensions::require_capacity_in_conn(
        &mut tx,
        "plugin",
        &manifest.id,
        extensions,
    )
    .await
    .map_err(map_cnt_err)?;
    crate::storage::admin_extensions::sync_owner_in_conn(
        &mut tx,
        "plugin",
        &manifest.id,
        extensions,
        "active",
        &manifest
            .actions
            .iter()
            .map(|action| action.id.clone())
            .collect::<Vec<_>>(),
    )
    .await
    .map_err(map_cnt_err)?;

    crate::storage::page::sync_owner_blocks_in_tx(
        &mut tx,
        "plugin",
        &manifest.id,
        ProviderStatus::Active,
        &revision_digest,
        blocks,
    )
    .await
    .map_err(map_cnt_err)?;
    crate::storage::page::resync_plugin_block_acls_in_tx(&mut tx)
        .await
        .map_err(map_cnt_err)?;
    crate::storage::plugin_projections::delete_plugin_projections_in_tx(&mut tx, &manifest.id)
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi xóa plugin projections cũ: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi commit cài đặt plugin: {e}")))?;
    crate::cache::SiteCacheEpoch::global().bump();

    Ok(revision_id)
}

pub async fn list_plugins(db_pool: &SqlitePool) -> Result<Vec<PluginSummary>, String> {
    let rows = sqlx::query(
        "SELECT id, version, name, description, manifest_json, enabled, last_error, installed_at, updated_at, active_revision_id, revision_generation, cas_version
         FROM plugins ORDER BY id ASC",
    )
    .fetch_all(db_pool)
    .await
    .map_err(|e| format!("Lỗi truy vấn plugins: {e}"))?;

    let mut list = Vec::with_capacity(rows.len());
    for row in rows {
        let manifest_json: String = row.get("manifest_json");
        let permissions =
            if let Ok(manifest) = serde_json::from_str::<PluginManifest>(&manifest_json) {
                manifest.permissions
            } else {
                PluginPermissions {
                    sqlite: vec![],
                    http: vec![],
                    content_read: vec![],
                    network: vec![],
                    content_write: vec![],
                    ..Default::default()
                }
            };

        list.push(PluginSummary {
            id: row.get("id"),
            version: row.get("version"),
            name: row.get("name"),
            description: row.get("description"),
            enabled: row.get::<i64, _>("enabled") == 1,
            permissions,
            last_error: row.get("last_error"),
            installed_at: row.get("installed_at"),
            updated_at: row.get("updated_at"),
            active_revision_id: row.get("active_revision_id"),
            revision_generation: row.get("revision_generation"),
            cas_version: row.get("cas_version"),
        });
    }

    Ok(list)
}

pub async fn get_plugin(db_pool: &SqlitePool, id: &str) -> Result<Option<PluginSummary>, String> {
    let row_opt = sqlx::query(
        "SELECT id, version, name, description, manifest_json, enabled, last_error, installed_at, updated_at, active_revision_id, revision_generation, cas_version
         FROM plugins WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db_pool)
    .await
    .map_err(|e| format!("Lỗi truy vấn plugin: {e}"))?;

    let Some(row) = row_opt else {
        return Ok(None);
    };

    let manifest_json: String = row.get("manifest_json");
    let permissions = if let Ok(manifest) = serde_json::from_str::<PluginManifest>(&manifest_json) {
        manifest.permissions
    } else {
        PluginPermissions {
            sqlite: vec![],
            http: vec![],
            content_read: vec![],
            network: vec![],
            content_write: vec![],
            ..Default::default()
        }
    };

    Ok(Some(PluginSummary {
        id: row.get("id"),
        version: row.get("version"),
        name: row.get("name"),
        description: row.get("description"),
        enabled: row.get::<i64, _>("enabled") == 1,
        permissions,
        last_error: row.get("last_error"),
        installed_at: row.get("installed_at"),
        updated_at: row.get("updated_at"),
        active_revision_id: row.get("active_revision_id"),
        revision_generation: row.get("revision_generation"),
        cas_version: row.get("cas_version"),
    }))
}

pub async fn set_plugin_enabled_cas(
    db_pool: &SqlitePool,
    id: &str,
    enabled: bool,
    expected_cas_version: Option<i64>,
) -> Result<i64, PluginStorageError> {
    let mut tx = db_pool
        .begin()
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi mở transaction plugin: {e}")))?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi khóa ghi plugin: {e}")))?;

    let current: Option<i64> = sqlx::query_scalar("SELECT cas_version FROM plugins WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi truy vấn plugin: {e}")))?;

    let Some(current_cas) = current else {
        return Err(PluginStorageError::NotFound(
            "Plugin không tồn tại.".to_string(),
        ));
    };

    if let Some(expected) = expected_cas_version {
        if current_cas != expected {
            return Err(PluginStorageError::PreconditionFailed(format!(
                "CAS version không khớp (hiện tại {current_cas}, yêu cầu {expected})."
            )));
        }
    }

    let flag: i64 = if enabled { 1 } else { 0 };
    sqlx::query(
        "UPDATE plugins SET enabled = ?, last_error = CASE WHEN ? = 1 THEN NULL ELSE last_error END, revision_generation = revision_generation + 1, cas_version = cas_version + 1, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(flag)
    .bind(flag)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| PluginStorageError::Internal(format!("Lỗi cập nhật trạng thái plugin: {e}")))?;

    crate::storage::content::set_plugin_cnt_provider_status_in_tx(&mut tx, id, enabled)
        .await
        .map_err(map_cnt_err)?;
    crate::storage::admin_extensions::set_owner_status_in_conn(&mut tx, "plugin", id, enabled)
        .await
        .map_err(map_cnt_err)?;
    crate::storage::page::set_owner_block_status_in_tx(
        &mut tx,
        "plugin",
        id,
        if enabled {
            ProviderStatus::Active
        } else {
            ProviderStatus::ProviderInactive
        },
    )
    .await
    .map_err(map_cnt_err)?;
    crate::storage::page::resync_plugin_block_acls_in_tx(&mut tx)
        .await
        .map_err(map_cnt_err)?;
    if enabled {
        crate::storage::admin_extensions::require_capacity_in_conn(&mut tx, "plugin", id, None)
            .await
            .map_err(map_cnt_err)?;
    } else {
        crate::storage::plugin_projections::delete_plugin_projections_in_tx(&mut tx, id)
            .await
            .map_err(|e| {
                PluginStorageError::Internal(format!("Lỗi xóa plugin projections: {e}"))
            })?;
    }

    let next_cas: i64 = sqlx::query_scalar("SELECT cas_version FROM plugins WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi đọc cas_version mới: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi commit trạng thái plugin: {e}")))?;
    crate::cache::SiteCacheEpoch::global().bump();

    Ok(next_cas)
}

pub async fn load_plugin_executable_with_generation(
    db_pool: &SqlitePool,
    id: &str,
) -> Result<Option<(Vec<u8>, String, bool, i64, Option<String>)>, sqlx::Error> {
    let row_opt = sqlx::query(
        "SELECT p.wasm_blob, p.manifest_json, p.enabled, p.revision_generation, r.revision_digest
         FROM plugins p
         LEFT JOIN plugin_revisions r ON r.id = p.active_revision_id
         WHERE p.id = ?",
    )
    .bind(id)
    .fetch_optional(db_pool);
    let row_opt = tokio::time::timeout(std::time::Duration::from_secs(1), row_opt)
        .await
        .map_err(|_| sqlx::Error::PoolTimedOut)??;

    Ok(row_opt.map(|row| {
        let wasm_blob: Vec<u8> = row.get("wasm_blob");
        let manifest_json: String = row.get("manifest_json");
        let enabled: bool = row.get::<i64, _>("enabled") == 1;
        let revision_generation: i64 = row.get("revision_generation");
        let revision_digest: Option<String> = row.get("revision_digest");
        (
            wasm_blob,
            manifest_json,
            enabled,
            revision_generation,
            revision_digest,
        )
    }))
}

pub async fn is_plugin_generation_active(
    conn: &mut SqliteConnection,
    id: &str,
    expected_generation: i64,
) -> Result<bool, sqlx::Error> {
    if expected_generation == 0
        && crate::dev_mode::DevOverlayConfig::global().is_some_and(|dev| dev.has_dev_plugin(id))
    {
        return Ok(true);
    }
    let row_opt: Option<(i64, i64)> =
        sqlx::query_as("SELECT enabled, revision_generation FROM plugins WHERE id = ?")
            .bind(id)
            .fetch_optional(conn)
            .await?;
    Ok(matches!(row_opt, Some((1, rev_gen)) if rev_gen == expected_generation))
}

pub async fn verify_active_theme_fence_in_tx(
    conn: &mut SqliteConnection,
    expected_revision_id: &str,
    expected_selection_version: i64,
) -> Result<bool, sqlx::Error> {
    let row_opt: Option<(Option<String>, i64)> =
        sqlx::query_as("SELECT revision_id, version FROM theme_active_selection WHERE id = 1")
            .fetch_optional(conn)
            .await?;
    if let Some(dev_theme_id) = expected_revision_id.strip_prefix("dev:") {
        let dev_matches = crate::dev_mode::DevOverlayConfig::global()
            .is_some_and(|dev| dev.dev_theme_id() == Some(dev_theme_id));
        return Ok(
            dev_matches && matches!(row_opt, Some((_, ver)) if ver == expected_selection_version)
        );
    }
    Ok(matches!(
        row_opt,
        Some((Some(ref rev_id), ver))
            if rev_id == expected_revision_id && ver == expected_selection_version
    ))
}

pub async fn list_enabled_plugin_revision_digests(
    db_pool: &SqlitePool,
) -> Result<Vec<(String, String, String)>, String> {
    let rows = sqlx::query(
        "SELECT p.id, p.manifest_json, COALESCE(r.revision_digest, '') AS revision_digest
         FROM plugins p
         LEFT JOIN plugin_revisions r ON r.id = p.active_revision_id
         WHERE p.enabled = 1
         ORDER BY p.id",
    )
    .fetch_all(db_pool)
    .await
    .map_err(|e| format!("Lỗi đọc danh sách plugin revisions: {e}"))?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.get("id");
        let manifest_json: String = row.get("manifest_json");
        let digest: String = row.get("revision_digest");
        let domain = serde_json::from_str::<opendash_core::plugin::PluginManifest>(&manifest_json)
            .ok()
            .and_then(|m| m.text_domain)
            .unwrap_or_else(|| id.clone());
        out.push((id, domain, digest));
    }
    Ok(out)
}

pub async fn load_plugin_executable(
    db_pool: &SqlitePool,
    id: &str,
) -> Result<Option<(Vec<u8>, String, bool)>, sqlx::Error> {
    let row_opt = sqlx::query("SELECT wasm_blob, manifest_json, enabled FROM plugins WHERE id = ?")
        .bind(id)
        .fetch_optional(db_pool);
    let row_opt = tokio::time::timeout(std::time::Duration::from_secs(1), row_opt)
        .await
        .map_err(|_| sqlx::Error::PoolTimedOut)??;

    Ok(row_opt.map(|row| {
        let wasm_blob: Vec<u8> = row.get("wasm_blob");
        let manifest_json: String = row.get("manifest_json");
        let enabled: bool = row.get::<i64, _>("enabled") == 1;
        (wasm_blob, manifest_json, enabled)
    }))
}

pub async fn content_hook_subscriptions(
    db: &SqlitePool,
    event: opendash_core::hooks::HookEvent,
    type_key: &str,
) -> Result<Vec<(i32, String)>, String> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, manifest_json FROM plugins WHERE enabled = 1 ORDER BY id")
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?;
    let mut subs = Vec::new();
    for (id, json) in rows {
        match PluginManifest::from_json_str(&json) {
            Ok(manifest)
                if manifest.id == id
                    && manifest.hooks.contains(&event)
                    && manifest.can_read_content_type(type_key) =>
            {
                subs.push((manifest.hook_priority, id));
            }
            Ok(_) => {}
            Err(error) => eprintln!("Invalid subscription manifest {id}: {error}"),
        }
    }
    subs.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(subs)
}
