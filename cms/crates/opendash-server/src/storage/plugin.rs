fn map_cnt_err(err: crate::error::ApiError) -> PluginStorageError {
    if err.0 == axum::http::StatusCode::CONFLICT {
        PluginStorageError::Conflict(err.1.into_owned())
    } else if err.0 == axum::http::StatusCode::BAD_REQUEST {
        PluginStorageError::BadRequest(err.1.into_owned())
    } else {
        PluginStorageError::Internal(err.1.into_owned())
    }
}

use std::{collections::BTreeMap, fs, io::Write, path::Path};

use opendash_core::{block_schema::BlockSchemaV1, registry::ProviderStatus};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool};

const COMPLETE_MARKER: &str = ".complete";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredBlock {
    schema: BlockSchemaV1,
    template: String,
}

fn canonical_blocks_json(
    owner_id: &str,
    blocks: &[(BlockSchemaV1, String)],
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

#[derive(Debug, Clone)]
pub enum PluginStorageError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    PreconditionRequired(String),
    PreconditionFailed(String),
    Internal(String),
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

fn compute_plugin_revision_digest_with_blocks_json(
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

fn manifest_entrypoint_filename(manifest_json: &str) -> Result<&'static str, String> {
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

fn write_durable_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
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

fn stage_and_publish_plugin_revision_with_blocks(
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

use opendash_core::plugin::{PluginManifest, PluginPermissions};

use crate::plugin::PluginSummary;

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
    super::plugin_projections::delete_plugin_projections_in_tx(&mut tx, &manifest.id)
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
    if enabled {
        crate::storage::admin_extensions::require_capacity_in_conn(&mut tx, "plugin", id, None)
            .await
            .map_err(|e| e.1.into_owned())?;
    } else {
        super::plugin_projections::delete_plugin_projections_in_tx(&mut tx, id)
            .await
            .map_err(|e| format!("Lỗi xóa plugin projections: {e}"))?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
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
        super::plugin_projections::delete_plugin_projections_in_tx(&mut tx, id)
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
    Ok(matches!(
        row_opt,
        Some((Some(ref rev_id), ver))
            if rev_id == expected_revision_id && ver == expected_selection_version
    ))
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

pub struct CachedActivePluginRow {
    pub id: String,
    pub wasm_blob: Vec<u8>,
    pub manifest_json: String,
    pub revision_generation: i64,
    pub revision_digest: Option<String>,
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

/// Read the host projection's source row only if its locale is visible in
/// this scope. The check and hydration use one SQLite read snapshot.
pub(crate) async fn get_entry_for_host(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
    enforce_public_locale: bool,
) -> crate::error::ApiResult<Option<super::entries::ContentEntryRecord>> {
    let mut tx = db.begin().await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM content_entries e
          JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
          WHERE e.id = ? AND e.content_type_key = ?
            AND (? = 0 OR (sl.enabled = 1 AND e.publication_status = 'published'
              AND (sl.is_default = 1 OR
                  (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1))))",
    )
    .bind(entry_id)
    .bind(type_key)
    .bind(i64::from(enforce_public_locale))
    .fetch_one(&mut *tx)
    .await?;
    let entry = if visible {
        super::entries::get_entry_in_tx(&mut tx, type_key, entry_id).await?
    } else {
        None
    };
    tx.commit().await?;
    Ok(entry)
}

/// Rows and total share a single predicate, including the locale and provider
/// scope, rather than independently counting unfiltered rows.
pub(crate) async fn list_entries_for_host(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: super::entries::EntryFilters,
    published_only: bool,
    enforce_public_locale: bool,
) -> crate::error::ApiResult<(Vec<super::entries::ContentEntryRecord>, i64)> {
    let mut tx = db.begin().await?;
    let result = super::entries::list_host_entries_in_tx(
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

pub async fn delete_entry_with_plugin_fence(
    db: &SqlitePool,
    actor: &crate::auth::SessionContext,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
    fence: &crate::storage::entries::PluginActionFence,
) -> crate::error::ApiResult<()> {
    use crate::error::ApiError;
    if matches!(type_key, "pattern" | "section") {
        return Err(ApiError::bad_request(
            "Không thể xóa Pattern/Section qua action plugin.",
        ));
    }
    if expected_version < 1 {
        return Err(ApiError::bad_request("expected_version không hợp lệ."));
    }
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let fence_ok = match &fence.owner_fence {
        crate::storage::entries::OwnerFence::Plugin {
            plugin_id,
            expected_generation,
        } => is_plugin_generation_active(&mut *tx, plugin_id, *expected_generation).await?,
        crate::storage::entries::OwnerFence::Theme {
            revision_id,
            selection_version,
            ..
        } => verify_active_theme_fence_in_tx(&mut *tx, revision_id, *selection_version).await?,
    };
    if !fence_ok {
        return Err(ApiError::conflict(
            "Plugin đã bị vô hiệu hóa hoặc thay đổi phiên bản trong lúc thực thi.",
        ));
    }
    if !is_actor_role_active(&mut *tx, actor.user.id, &fence.required_role).await? {
        return Err(ApiError::forbidden("Quyền của tài khoản đã bị thu hồi."));
    }
    let result = sqlx::query(
        "DELETE FROM content_entries WHERE content_type_key = ? AND id = ? AND version = ?",
    )
    .bind(type_key)
    .bind(entry_id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        let current_ver: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM content_entries WHERE content_type_key = ? AND id = ?",
        )
        .bind(type_key)
        .bind(entry_id)
        .fetch_optional(&mut *tx)
        .await?;
        return match current_ver {
            Some(_) => Err(ApiError::precondition_failed(
                "Nội dung đã bị thay đổi trước đó (xung đột version).",
            )),
            None => Err(ApiError::not_found("Không tìm thấy nội dung cần xóa.")),
        };
    }
    if type_key == "page" {
        crate::storage::settings::bump_reading_version_on_deleted_entry_in_tx(&mut tx, entry_id)
            .await?;
    }
    sqlx::query(
        "DELETE FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id = ?",
    )
    .bind(entry_id)
    .execute(&mut *tx)
    .await?;
    super::plugin_projections::delete_entry_projections_in_tx(&mut tx, entry_id).await?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
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

#[cfg(test)]
mod s03_revision_tests {
    use super::*;

    fn sample_schema() -> BlockSchemaV1 {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1, "id": "sample-plugin/notice",
            "title": "Notice", "category": "content", "render": "render.html",
            "fields": []
        }))
        .unwrap()
    }

    #[test]
    fn blockless_plugin_digest_is_backward_compatible() {
        let manifest = "{\"id\":\"sample-plugin\"}";
        let wasm = b"arbitrary wasm fixture";
        let legacy = compute_plugin_revision_digest_with_extensions(manifest, wasm, None);
        assert_eq!(legacy, compute_plugin_revision_digest(manifest, wasm));
        assert_eq!(
            legacy,
            compute_plugin_revision_digest_with_blocks_json(manifest, wasm, None, None)
        );
        let root = std::env::temp_dir().join(format!(
            "od-s03-legacy-plugin-{}",
            crate::auth::generate_random_token()
        ));
        stage_and_publish_plugin_revision(&root, "test", &legacy, manifest, wasm).unwrap();
        assert!(
            !root
                .join("revisions")
                .join(&legacy)
                .join("blocks.json")
                .exists()
        );
        assert!(verify_plugin_revision_durability(&root, &legacy).is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn canonical_block_revision_digest_detects_disk_tampering() {
        let manifest = r#"{"id":"sample-plugin","version":"1.0.0","api_version":1,"name":"Sample","entrypoint":"plugin.wasm","permissions":{}}"#;
        let wasm = b"test wasm";
        let blocks = vec![(sample_schema(), "<p>Safe</p>".to_string())];
        let json = canonical_blocks_json("sample-plugin", &blocks)
            .unwrap()
            .unwrap();
        let digest =
            compute_plugin_revision_digest_with_blocks_json(manifest, wasm, None, Some(&json));
        let old_digest = compute_plugin_revision_digest(manifest, wasm);
        assert_ne!(digest, old_digest);

        let root = std::env::temp_dir().join(format!(
            "od-s03-plugin-blocks-{}",
            crate::auth::generate_random_token()
        ));
        stage_and_publish_plugin_revision_with_blocks(
            &root,
            "test",
            &digest,
            manifest,
            wasm,
            None,
            Some(&json),
        )
        .unwrap();
        assert!(verify_plugin_revision_durability(&root, &digest).is_ok());
        fs::write(
            root.join("revisions").join(&digest).join("blocks.json"),
            "{}",
        )
        .unwrap();
        assert!(verify_plugin_revision_durability(&root, &digest).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn install_rejects_cross_provider_claim_before_disk_staging() {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let schema = sample_schema();
        let schema_json = serde_json::to_string(&schema).unwrap();
        sqlx::query(
            "INSERT INTO builder_block_definitions (block_id,owner_kind,owner_id,provider_status,declaration_state,schema_json,template,revision_digest)
             VALUES (?, 'theme','other-theme','active','declared',?, '<p>Safe</p>','digest')",
        )
        .bind(&schema.id)
        .bind(&schema_json)
        .execute(&db).await.unwrap();

        let manifest_json = r#"{"id":"sample-plugin","version":"1.0.0","api_version":1,"name":"Sample","entrypoint":"plugin.wasm","permissions":{}}"#;
        let manifest = opendash_core::plugin::PluginManifest::from_json_str(manifest_json).unwrap();
        let root = std::env::temp_dir().join(format!(
            "od-s03-plugin-conflict-{}",
            crate::auth::generate_random_token()
        ));
        let result = install_plugin_revision(
            &db,
            &root,
            &manifest,
            manifest_json,
            b"fixture",
            None,
            &[(schema, "<p>Safe</p>".to_string())],
        )
        .await;
        assert!(matches!(result, Err(PluginStorageError::Conflict(_))));
        assert!(!root.join("revisions").exists());
    }
}
