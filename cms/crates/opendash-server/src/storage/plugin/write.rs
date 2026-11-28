use super::*;

pub(in crate::storage) fn map_cnt_err(err: crate::error::ApiError) -> PluginStorageError {
    if err.0 == axum::http::StatusCode::CONFLICT {
        PluginStorageError::Conflict(err.1.into_owned())
    } else if err.0 == axum::http::StatusCode::BAD_REQUEST {
        PluginStorageError::BadRequest(err.1.into_owned())
    } else {
        PluginStorageError::Internal(err.1.into_owned())
    }
}

pub(in crate::storage) fn canonical_blocks_json(
    owner_id: &str,
    blocks: &[(BlockSchema, String)],
) -> Result<Option<String>, PluginStorageError> {
    if blocks.is_empty() {
        return Ok(None);
    }
    if blocks.len() > 32 {
        return Err(PluginStorageError::BadRequest(
            "Plugin vượt quá 32 blocks.".into(),
        ));
    }
    let mut map = BTreeMap::new();
    for (schema, template) in blocks {
        schema
            .validate(owner_id)
            .map_err(|e| PluginStorageError::BadRequest(e.into()))?;
        crate::template_sec::validate_block_template_csec(&schema.id, template, schema)
            .map_err(|e| PluginStorageError::BadRequest(e.into()))?;
        let (_, slug) = schema
            .id
            .split_once('/')
            .ok_or_else(|| PluginStorageError::BadRequest("Block ID không hợp lệ.".into()))?;
        if map
            .insert(
                slug.to_owned(),
                StoredBlock {
                    schema: schema.clone(),
                    template: template.clone(),
                },
            )
            .is_some()
        {
            return Err(PluginStorageError::BadRequest("Block trùng slug.".into()));
        }
    }
    serde_json::to_string(&map)
        .map(Some)
        .map_err(|e| PluginStorageError::Internal(format!("Không serialize được blocks.json: {e}")))
}

impl std::fmt::Display for PluginStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(m)
            | Self::NotFound(m)
            | Self::Conflict(m)
            | Self::PreconditionRequired(m)
            | Self::PreconditionFailed(m)
            | Self::Internal(m) => write!(f, "{m}"),
        }
    }
}

pub fn compute_plugin_revision_digest(manifest_json: &str, wasm_bytes: &[u8]) -> String {
    compute_plugin_revision_digest_with_extensions(manifest_json, wasm_bytes, None)
}

/// A v2 admin declaration is part of the immutable installed revision, even
/// though the Wasm ABI's parsed PluginManifest intentionally does not expose
/// this UI-only data. Changing only declarations must conflict with an already
/// installed plugin ID/version rather than silently replacing its registry.
pub fn compute_plugin_revision_digest_with_extensions(
    manifest_json: &str,
    wasm_bytes: &[u8],
    extensions: Option<&opendash_core::admin_extensions::AdminExtensions>,
) -> String {
    compute_plugin_revision_digest_with_blocks_json(manifest_json, wasm_bytes, extensions, None)
}

pub(in crate::storage) fn compute_plugin_revision_digest_with_blocks_json(
    manifest_json: &str,
    wasm_bytes: &[u8],
    extensions: Option<&opendash_core::admin_extensions::AdminExtensions>,
    blocks_json: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(manifest_json.as_bytes());
    hasher.update([0u8]);
    hasher.update(wasm_bytes);
    if let Some(ext) = extensions {
        hasher.update(b"\0admin_extensions\0");
        hasher.update(serde_json::to_vec(ext).expect("validated admin declarations serialize"));
    }
    if let Some(blocks_json) = blocks_json {
        hasher.update(b"\0blocks_json\0");
        hasher.update(blocks_json.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn ensure_plugins_layout_and_quarantine_stale(plugins_root: &Path) -> Result<usize, String> {
    let staging = plugins_root.join(".staging");
    let revisions = plugins_root.join("revisions");
    let archives = plugins_root.join("archives");
    for dir in [&staging, &revisions, &archives] {
        fs::create_dir_all(dir).map_err(|e| format!("Không thể khởi tạo thư mục plugins/: {e}"))?;
    }
    let mut cleaned = 0usize;
    let entries = fs::read_dir(&staging)
        .map_err(|e| format!("Không thể đọc thư mục plugins/.staging: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Không thể duyệt plugins/.staging: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)
                .map_err(|e| format!("Không thể dọn staging cũ trong plugins/.staging: {e}"))?;
        } else {
            fs::remove_file(&path).map_err(|e| {
                format!("Không thể dọn file staging cũ trong plugins/.staging: {e}")
            })?;
        }
        cleaned += 1;
    }
    Ok(cleaned)
}

pub(in crate::storage) fn manifest_entrypoint_filename(
    manifest_json: &str,
) -> Result<&'static str, String> {
    let parsed: serde_json::Value = serde_json::from_str(manifest_json)
        .map_err(|_| "manifest.json của revision plugin không hợp lệ.".to_string())?;
    match parsed.get("entrypoint").and_then(serde_json::Value::as_str) {
        None | Some("plugin.wasm") => Ok("plugin.wasm"),
        Some("plugin.js") => Ok("plugin.js"),
        Some(_) => Err("Entrypoint revision plugin không được hỗ trợ.".to_string()),
    }
}

pub fn verify_plugin_revision_durability(
    plugins_root: &Path,
    revision_digest: &str,
) -> Result<(), String> {
    let rev_dir = plugins_root.join("revisions").join(revision_digest);
    if !rev_dir.is_dir() {
        return Err("Thư mục revision plugin trên đĩa không tồn tại.".to_string());
    }
    let marker = rev_dir.join(COMPLETE_MARKER);
    let marker_text = fs::read_to_string(&marker)
        .map_err(|_| "Revision plugin thiếu marker .complete.".to_string())?;
    if marker_text.trim() != revision_digest {
        return Err("Marker .complete của revision plugin không khớp digest.".to_string());
    }
    let manifest_json = fs::read_to_string(rev_dir.join("manifest.json"))
        .map_err(|_| "Revision plugin thiếu manifest.json trên đĩa.".to_string())?;
    let entrypoint = manifest_entrypoint_filename(&manifest_json)?;
    let wasm_bytes = fs::read(rev_dir.join(entrypoint))
        .map_err(|_| format!("Revision plugin thiếu {entrypoint} trên đĩa."))?;
    let ext_path = rev_dir.join("admin-extensions.json");
    let extension = if ext_path.exists() {
        let raw = fs::read(&ext_path)
            .map_err(|_| "Revision plugin không đọc được admin-extensions.json.".to_string())?;
        let parsed: opendash_core::admin_extensions::AdminExtensions = serde_json::from_slice(&raw)
            .map_err(|_| "Admin extension revision không hợp lệ.".to_string())?;
        let canonical = serde_json::to_vec(&parsed)
            .map_err(|_| "Admin extension revision không serialize được.".to_string())?;
        if raw != canonical {
            return Err("Admin extension revision không còn canonical.".into());
        }
        Some(parsed)
    } else {
        None
    };
    let block_path = rev_dir.join("blocks.json");
    let blocks_json = if block_path.exists() {
        let raw = fs::read_to_string(&block_path)
            .map_err(|_| "Revision plugin không đọc được blocks.json.".to_string())?;
        let saved: BTreeMap<String, StoredBlock> = serde_json::from_str(&raw)
            .map_err(|_| "Revision plugin blocks.json không đúng cấu trúc.".to_string())?;
        if saved.is_empty() || saved.len() > 32 {
            return Err("Revision plugin blocks.json không hợp lệ.".into());
        }
        let manifest: opendash_core::plugin::PluginManifest =
            opendash_core::plugin::PluginManifest::from_json_str(&manifest_json)
                .map_err(str::to_owned)?;
        let mut blocks = Vec::new();
        for (slug, block) in saved {
            if block.schema.id != format!("{}/{slug}", manifest.id) {
                return Err("Revision plugin blocks.json có slug không khớp.".into());
            }
            blocks.push((block.schema, block.template));
        }
        let canonical = canonical_blocks_json(&manifest.id, &blocks)
            .map_err(|e| e.to_string())?
            .ok_or("Revision plugin blocks.json trống.")?;
        if raw != canonical {
            return Err("Revision plugin blocks.json không canonical.".into());
        }
        Some(canonical)
    } else {
        None
    };
    if compute_plugin_revision_digest_with_blocks_json(
        &manifest_json,
        &wasm_bytes,
        extension.as_ref(),
        blocks_json.as_deref(),
    ) != revision_digest
    {
        return Err("Nội dung revision plugin trên đĩa bị hỏng hoặc lệch digest.".to_string());
    }
    Ok(())
}

pub(in crate::storage) fn write_durable_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = fs::File::create(path)
        .map_err(|e| format!("Không thể tạo file plugin {}: {e}", path.display()))?;
    file.write_all(bytes)
        .map_err(|e| format!("Không thể ghi file plugin {}: {e}", path.display()))?;
    file.sync_all()
        .map_err(|e| format!("Không thể sync_all file plugin {}: {e}", path.display()))?;
    Ok(())
}

pub fn stage_and_publish_plugin_revision(
    plugins_root: &Path,
    run_uuid: &str,
    revision_digest: &str,
    manifest_json: &str,
    wasm_bytes: &[u8],
) -> Result<(), String> {
    stage_and_publish_plugin_revision_with_extensions(
        plugins_root,
        run_uuid,
        revision_digest,
        manifest_json,
        wasm_bytes,
        None,
    )
}

pub fn stage_and_publish_plugin_revision_with_extensions(
    plugins_root: &Path,
    run_uuid: &str,
    revision_digest: &str,
    manifest_json: &str,
    wasm_bytes: &[u8],
    extensions: Option<&opendash_core::admin_extensions::AdminExtensions>,
) -> Result<(), String> {
    stage_and_publish_plugin_revision_with_blocks(
        plugins_root,
        run_uuid,
        revision_digest,
        manifest_json,
        wasm_bytes,
        extensions,
        None,
    )
}

pub(in crate::storage) fn stage_and_publish_plugin_revision_with_blocks(
    plugins_root: &Path,
    run_uuid: &str,
    revision_digest: &str,
    manifest_json: &str,
    wasm_bytes: &[u8],
    extensions: Option<&opendash_core::admin_extensions::AdminExtensions>,
    blocks_json: Option<&str>,
) -> Result<(), String> {
    if run_uuid.is_empty()
        || run_uuid.contains('/')
        || run_uuid.contains('\\')
        || run_uuid.contains("..")
    {
        return Err("run_uuid staging plugin không hợp lệ.".to_string());
    }
    fs::create_dir_all(plugins_root.join(".staging"))
        .map_err(|e| format!("Không thể tạo thư mục plugins/.staging: {e}"))?;
    fs::create_dir_all(plugins_root.join("revisions"))
        .map_err(|e| format!("Không thể tạo thư mục plugins/revisions: {e}"))?;
    let staging_dir = plugins_root.join(".staging").join(run_uuid);
    let rev_dir = plugins_root.join("revisions").join(revision_digest);

    let entrypoint = manifest_entrypoint_filename(manifest_json)?;
    fs::create_dir_all(&staging_dir)
        .map_err(|e| format!("Không thể tạo thư mục staging plugin: {e}"))?;
    if let Err(err) =
        write_durable_file(&staging_dir.join("manifest.json"), manifest_json.as_bytes())
            .and_then(|_| write_durable_file(&staging_dir.join(entrypoint), wasm_bytes))
            .and_then(|_| match extensions {
                Some(ext) => {
                    let raw = serde_json::to_vec(ext).map_err(|e| e.to_string())?;
                    write_durable_file(&staging_dir.join("admin-extensions.json"), &raw)
                }
                None => Ok(()),
            })
            .and_then(|_| match blocks_json {
                Some(raw) => write_durable_file(&staging_dir.join("blocks.json"), raw.as_bytes()),
                None => Ok(()),
            })
    {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(err);
    }

    if rev_dir.exists() {
        let _ = fs::remove_dir_all(&staging_dir);
        return verify_plugin_revision_durability(plugins_root, revision_digest);
    }

    if let Err(err) = fs::rename(&staging_dir, &rev_dir) {
        let _ = fs::remove_dir_all(&staging_dir);
        if rev_dir.exists() {
            return verify_plugin_revision_durability(plugins_root, revision_digest);
        }
        return Err(format!("Không thể publish thư mục revision plugin: {err}"));
    }

    write_durable_file(
        &rev_dir.join(COMPLETE_MARKER),
        format!("{revision_digest}\n").as_bytes(),
    )?;
    Ok(())
}

pub async fn set_plugin_enabled(
    db_pool: &SqlitePool,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut tx = db_pool.begin().await.map_err(|e| e.to_string())?;
    let flag: i64 = if enabled { 1 } else { 0 };
    let res =
        sqlx::query("UPDATE plugins SET enabled = ?, last_error = CASE WHEN ? = 1 THEN NULL ELSE last_error END, revision_generation = revision_generation + 1, cas_version = cas_version + 1, updated_at = datetime('now') WHERE id = ?")
            .bind(flag)
            .bind(flag)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Lỗi cập nhật trạng thái plugin: {e}"))?;

    if res.rows_affected() == 0 {
        return Err("Plugin không tồn tại.".to_string());
    }
    crate::storage::content::set_plugin_cnt_provider_status_in_tx(&mut tx, id, enabled)
        .await
        .map_err(|error| error.1.into_owned())?;
    crate::storage::admin_extensions::set_owner_status_in_conn(&mut tx, "plugin", id, enabled)
        .await
        .map_err(|e| e.1.into_owned())?;
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
    .map_err(|e| e.1.into_owned())?;
    crate::storage::page::resync_plugin_block_acls_in_tx(&mut tx)
        .await
        .map_err(|e| e.1.into_owned())?;
    crate::storage::search::sync_builder_entries_in_conn(&mut tx)
        .await
        .map_err(|e| e.1.into_owned())?;
    if enabled {
        crate::storage::admin_extensions::require_capacity_in_conn(&mut tx, "plugin", id, None)
            .await
            .map_err(|e| e.1.into_owned())?;
    } else {
        crate::storage::plugin_projections::delete_plugin_projections_in_tx(&mut tx, id)
            .await
            .map_err(|e| format!("Lỗi xóa plugin projections: {e}"))?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
}

#[allow(dead_code)]
pub async fn delete_plugin_owned_rows_in_tx(
    tx: &mut SqliteConnection,
    plugin_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM plugin_kv WHERE owner_kind = 'plugin' AND plugin_id = ?")
        .bind(plugin_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM plugin_entry_projections WHERE owner_kind = 'plugin' AND plugin_id = ?",
    )
    .bind(plugin_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM plugin_logs WHERE owner_kind = 'plugin' AND plugin_id = ?")
        .bind(plugin_id)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

pub async fn record_plugin_execution_result(
    db_pool: &SqlitePool,
    id: &str,
    correlation_id: &str,
    level: &str,
    message: &str,
    last_error: Option<&str>,
) {
    record_owner_execution_result(
        db_pool,
        opendash_core::hooks::ExtensionOwnerKind::Plugin,
        id,
        correlation_id,
        level,
        message,
        last_error,
    )
    .await;
}

pub async fn record_owner_execution_result(
    db_pool: &SqlitePool,
    owner_kind: opendash_core::hooks::ExtensionOwnerKind,
    id: &str,
    correlation_id: &str,
    level: &str,
    message: &str,
    last_error: Option<&str>,
) {
    let write = async {
        sqlx::query(
            "INSERT INTO plugin_logs (owner_kind, plugin_id, correlation_id, level, message)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(owner_kind.as_str())
        .bind(id)
        .bind(correlation_id)
        .bind(level)
        .bind(message)
        .execute(db_pool)
        .await?;
        if owner_kind == opendash_core::hooks::ExtensionOwnerKind::Plugin {
            sqlx::query(
                "UPDATE plugins SET last_error = ?, updated_at = datetime('now') WHERE id = ?",
            )
            .bind(last_error)
            .bind(id)
            .execute(db_pool)
            .await?;
        }
        Ok::<_, sqlx::Error>(())
    };
    if !matches!(
        tokio::time::timeout(std::time::Duration::from_secs(1), write).await,
        Ok(Ok(()))
    ) {
        eprintln!(
            "Extension log unavailable: {}:{id} {correlation_id} {level}: {message}",
            owner_kind.as_str()
        );
    }
}

#[allow(dead_code)]
pub async fn insert_plugin_log_in_conn(
    conn: &mut SqliteConnection,
    plugin_id: &str,
    correlation_id: &str,
    level: &str,
    message: &str,
) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
    insert_owner_log_in_conn(
        conn,
        opendash_core::hooks::ExtensionOwnerKind::Plugin,
        plugin_id,
        correlation_id,
        level,
        message,
    )
    .await
}

pub async fn insert_owner_log_in_conn(
    conn: &mut SqliteConnection,
    owner_kind: opendash_core::hooks::ExtensionOwnerKind,
    owner_id: &str,
    correlation_id: &str,
    level: &str,
    message: &str,
) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
    sqlx::query(
        "INSERT INTO plugin_logs (owner_kind, plugin_id, correlation_id, level, message)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(owner_kind.as_str())
    .bind(owner_id)
    .bind(correlation_id)
    .bind(level)
    .bind(message)
    .execute(conn)
    .await
}

#[allow(dead_code)]
pub async fn read_raw_plugin_option_map(
    db: &SqlitePool,
    plugin_id: &str,
) -> Result<BTreeMap<String, serde_json::Value>, sqlx::Error> {
    read_raw_owner_option_map(
        db,
        opendash_core::hooks::ExtensionOwnerKind::Plugin,
        plugin_id,
    )
    .await
}

/// Legacy plugin `content_delete` wrapper: at S02 activation routes to recoverable Trash
/// through the shared lifecycle transaction while preserving role & plugin generation fences.
pub async fn delete_entry_with_plugin_fence(
    db: &SqlitePool,
    actor: &crate::auth::SessionContext,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
    fence: &crate::storage::entries::PluginActionFence,
) -> crate::error::ApiResult<crate::storage::entry_lifecycle::EntryCommitReceipt> {
    let cmd = crate::storage::entry_lifecycle::LifecycleCommand {
        operation: crate::storage::entry_lifecycle::LifecycleOperation::Trash,
        type_key: type_key.to_owned(),
        entry_id,
        expected_version,
    };
    let authority = crate::storage::entry_lifecycle::LifecycleAuthority {
        session: actor,
        plugin_fence: Some(fence),
        confirmation: crate::storage::entry_lifecycle::LifecycleConfirmationEvidence::None,
        scope: crate::storage::entry_lifecycle::LifecycleScope::Ordinary,
    };
    crate::storage::entry_lifecycle::execute_lifecycle_transition(db, &cmd, &authority).await
}
