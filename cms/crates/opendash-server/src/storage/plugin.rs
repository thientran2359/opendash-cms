use sqlx::{Row, SqlitePool};

use opendash_core::plugin::{PluginManifest, PluginPermissions};

use crate::plugin::PluginSummary;

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
        "SELECT id, version, name, description, manifest_json, enabled, last_error, installed_at, updated_at
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
        });
    }

    Ok(list)
}

pub async fn get_plugin(db_pool: &SqlitePool, id: &str) -> Result<Option<PluginSummary>, String> {
    let row_opt = sqlx::query(
        "SELECT id, version, name, description, manifest_json, enabled, last_error, installed_at, updated_at
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
    }))
}

pub async fn set_plugin_enabled(
    db_pool: &SqlitePool,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let flag: i64 = if enabled { 1 } else { 0 };
    let res =
        sqlx::query("UPDATE plugins SET enabled = ?, updated_at = datetime('now') WHERE id = ?")
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
