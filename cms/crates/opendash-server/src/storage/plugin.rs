fn map_cnt_err(err: crate::error::ApiError) -> PluginStorageError {
    if err.0 == axum::http::StatusCode::CONFLICT {
        PluginStorageError::Conflict(err.1.into_owned())
    } else if err.0 == axum::http::StatusCode::BAD_REQUEST {
        PluginStorageError::BadRequest(err.1.into_owned())
    } else {
        PluginStorageError::Internal(err.1.into_owned())
    }
}

use std::{
    fs,
    io::Write,
    path::Path,
};

use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool};

const COMPLETE_MARKER: &str = ".complete";

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
    let mut hasher = Sha256::new();
    hasher.update(manifest_json.as_bytes());
    hasher.update([0u8]);
    hasher.update(wasm_bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn ensure_plugins_layout_and_quarantine_stale(plugins_root: &Path) -> Result<usize, String> {
    let staging = plugins_root.join(".staging");
    let revisions = plugins_root.join("revisions");
    let archives = plugins_root.join("archives");
    for dir in [&staging, &revisions, &archives] {
        fs::create_dir_all(dir)
            .map_err(|e| format!("Không thể khởi tạo thư mục plugins/: {e}"))?;
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
            fs::remove_file(&path)
                .map_err(|e| format!("Không thể dọn file staging cũ trong plugins/.staging: {e}"))?;
        }
        cleaned += 1;
    }
    Ok(cleaned)
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
    let wasm_bytes = fs::read(rev_dir.join("plugin.wasm"))
        .map_err(|_| "Revision plugin thiếu plugin.wasm trên đĩa.".to_string())?;
    if compute_plugin_revision_digest(&manifest_json, &wasm_bytes) != revision_digest {
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

    fs::create_dir_all(&staging_dir)
        .map_err(|e| format!("Không thể tạo thư mục staging plugin: {e}"))?;
    if let Err(err) = write_durable_file(&staging_dir.join("manifest.json"), manifest_json.as_bytes())
        .and_then(|_| write_durable_file(&staging_dir.join("plugin.wasm"), wasm_bytes))
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
) -> Result<i64, PluginStorageError> {
    let revision_digest = compute_plugin_revision_digest(manifest_json, wasm_bytes);
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

    stage_and_publish_plugin_revision(
        plugins_root,
        &run_uuid,
        &revision_digest,
        manifest_json,
        wasm_bytes,
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
            "INSERT INTO plugin_revisions (plugin_id, version, revision_digest, manifest_schema, manifest_json)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&manifest.id)
        .bind(&manifest.version)
        .bind(&revision_digest)
        .bind(manifest.manifest_schema as i64)
        .bind(manifest_json)
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
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi cập nhật active_revision_id: {e}")))?;

    crate::storage::content::sync_plugin_cnt_declarations_in_tx(
        &mut tx,
        &manifest.id,
        &manifest.declarations,
    )
    .await
    .map_err(map_cnt_err)?;

    tx.commit()
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi commit cài đặt plugin: {e}")))?;

    Ok(revision_id)
}

pub async fn upsert_plugin(
    db_pool: &SqlitePool,
    manifest: &PluginManifest,
    manifest_json: &str,
    wasm_bytes: &[u8],
) -> Result<(), String> {
    let mut conn = db_pool
        .acquire()
        .await
        .map_err(|e| format!("Lỗi kết nối cơ sở dữ liệu: {e}"))?;

    sqlx::query(
        "INSERT INTO plugins (id, version, name, description, manifest_json, wasm_blob, enabled, last_error, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, 1, NULL, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET
             version = excluded.version,
             name = excluded.name,
             description = excluded.description,
             manifest_json = excluded.manifest_json,
             wasm_blob = excluded.wasm_blob,
             enabled = 1,
             last_error = NULL,
             updated_at = excluded.updated_at",
    )
    .bind(&manifest.id)
    .bind(&manifest.version)
    .bind(&manifest.name)
    .bind(&manifest.description)
    .bind(manifest_json)
    .bind(wasm_bytes)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("Lỗi lưu plugin vào database: {e}"))?;

    Ok(())
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
    let flag: i64 = if enabled { 1 } else { 0 };
    let res =
        sqlx::query("UPDATE plugins SET enabled = ?, revision_generation = revision_generation + 1, cas_version = cas_version + 1, updated_at = datetime('now') WHERE id = ?")
            .bind(flag)
            .bind(id)
            .execute(db_pool)
            .await
            .map_err(|e| format!("Lỗi cập nhật trạng thái plugin: {e}"))?;

    if res.rows_affected() == 0 {
        return Err("Plugin không tồn tại.".to_string());
    }
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

    let current: Option<i64> = sqlx::query_scalar("SELECT cas_version FROM plugins WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi truy vấn plugin: {e}")))?;

    let Some(current_cas) = current else {
        return Err(PluginStorageError::NotFound("Plugin không tồn tại.".to_string()));
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
        "UPDATE plugins SET enabled = ?, revision_generation = revision_generation + 1, cas_version = cas_version + 1, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(flag)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|e| PluginStorageError::Internal(format!("Lỗi cập nhật trạng thái plugin: {e}")))?;

    crate::storage::content::set_plugin_cnt_provider_status_in_tx(&mut tx, id, enabled)
        .await
        .map_err(map_cnt_err)?;

    let next_cas: i64 = sqlx::query_scalar("SELECT cas_version FROM plugins WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi đọc cas_version mới: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| PluginStorageError::Internal(format!("Lỗi commit trạng thái plugin: {e}")))?;

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
    let write = async {
        sqlx::query("INSERT INTO plugin_logs (plugin_id, correlation_id, level, message) VALUES (?, ?, ?, ?)")
            .bind(id).bind(correlation_id).bind(level).bind(message).execute(db_pool).await?;
        sqlx::query("UPDATE plugins SET last_error = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(last_error)
            .bind(id)
            .execute(db_pool)
            .await?;
        Ok::<_, sqlx::Error>(())
    };
    if !matches!(
        tokio::time::timeout(std::time::Duration::from_secs(1), write).await,
        Ok(Ok(()))
    ) {
        eprintln!("Plugin log unavailable: {id} {correlation_id} {level}: {message}");
    }
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
