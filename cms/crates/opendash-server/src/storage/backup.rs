//! SQLite storage helpers for consistent snapshot VACUUM INTO and backup inventory (M8-S03-T01).

use std::collections::HashSet;
use std::path::Path;

use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, Row, SqliteConnection, SqlitePool};

use crate::error::{ApiError, ApiResult};
use crate::storage::media_refs::{
    ExtensionSecretInventoryItem, collect_referenced_extension_secret_refs,
    collect_referenced_secret_refs,
};

#[derive(Debug, Clone)]
pub struct BackupMediaItem {
    pub id: String,
    pub kind: String,
    pub extension: String,
}

#[derive(Debug, Clone)]
pub struct BackupThemeRevisionItem {
    pub theme_id: String,
    pub revision_digest: String,
    pub source_format: String,
}

#[derive(Debug, Clone)]
pub struct BackupInventory {
    pub media: Vec<BackupMediaItem>,
    pub quarantine_job_ids: Vec<String>,
    pub theme_revisions: Vec<BackupThemeRevisionItem>,
    pub secret_refs: HashSet<String>,
    pub extension_secret_refs: Vec<ExtensionSecretInventoryItem>,
}

/// Executes `VACUUM main INTO <destination>` on a dedicated connection outside any open tx,
/// then verifies `integrity_check = ok`, empty `foreign_key_check`, and `_sqlx_migrations`.
pub async fn vacuum_into_snapshot(
    db_path: &Path,
    destination: &Path,
) -> ApiResult<(i64, Vec<(i64, String)>)> {
    let dest_str = destination
        .to_str()
        .ok_or_else(|| ApiError::internal("Đường dẫn snapshot DB không phải UTF-8 hợp lệ."))?;

    let src_opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(false);
    let mut src_conn = SqliteConnection::connect_with(&src_opts)
        .await
        .map_err(|_| ApiError::internal("Không mở được kết nối SQLite nguồn để tạo snapshot."))?;

    sqlx::query("VACUUM main INTO ?")
        .bind(dest_str)
        .execute(&mut src_conn)
        .await
        .map_err(|_| ApiError::internal("VACUUM main INTO thất bại khi tạo snapshot."))?;

    src_conn
        .close()
        .await
        .map_err(|_| ApiError::internal("Không đóng được kết nối SQLite nguồn."))?;

    let snap_opts = SqliteConnectOptions::new()
        .filename(destination)
        .create_if_missing(false)
        .read_only(true);
    let mut snap_conn = SqliteConnection::connect_with(&snap_opts)
        .await
        .map_err(|_| ApiError::internal("Không mở được DB snapshot để kiểm tra toàn vẹn."))?;

    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut snap_conn)
        .await?;
    if !integrity.eq_ignore_ascii_case("ok") {
        return Err(ApiError::internal(
            "Kiểm tra integrity_check trên snapshot DB thất bại.",
        ));
    }

    let fk_rows = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut snap_conn)
        .await?;
    if !fk_rows.is_empty() {
        return Err(ApiError::internal(
            "Kiểm tra foreign_key_check trên snapshot DB phát hiện lỗi.",
        ));
    }

    let mig_rows = sqlx::query(
        "SELECT version, lower(hex(checksum)) AS checksum_hex FROM _sqlx_migrations ORDER BY version ASC",
    )
    .fetch_all(&mut snap_conn)
    .await?;

    let mut migrations = Vec::with_capacity(mig_rows.len());
    let mut max_version: i64 = 0;
    for row in mig_rows {
        let version: i64 = row.try_get("version")?;
        let checksum_hex: String = row.try_get("checksum_hex")?;
        if version > max_version {
            max_version = version;
        }
        migrations.push((version, checksum_hex));
    }

    let _ = snap_conn.close().await;
    Ok((max_version, migrations))
}

pub async fn collect_backup_inventory(db: &SqlitePool) -> ApiResult<BackupInventory> {
    let pending_deletes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM media_delete_jobs WHERE phase IN ('tombstoned', 'moving')",
    )
    .fetch_one(db)
    .await?;
    if pending_deletes > 0 {
        return Err(ApiError::conflict(
            "Đang có media delete job chưa hoàn tất; không thể tạo snapshot.",
        ));
    }
    let pending_replaces: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM media_replace_jobs WHERE phase IN ('staged', 'moving_old', 'promoting_new')",
    )
    .fetch_one(db)
    .await?;
    if pending_replaces > 0 {
        return Err(ApiError::conflict(
            "Đang có tác vụ thay thế media chưa hoàn tất; không thể tạo snapshot.",
        ));
    }

    let media_rows = sqlx::query("SELECT id, kind, extension FROM media_assets ORDER BY id ASC")
        .fetch_all(db)
        .await?;
    let mut media = Vec::with_capacity(media_rows.len());
    for row in media_rows {
        media.push(BackupMediaItem {
            id: row.try_get("id")?,
            kind: row.try_get("kind")?,
            extension: row.try_get("extension")?,
        });
    }

    let q_rows = sqlx::query(
        "SELECT job_id FROM media_delete_jobs WHERE phase IN ('quarantined', 'finalized') ORDER BY job_id ASC",
    )
    .fetch_all(db)
    .await?;
    let mut quarantine_job_ids = Vec::with_capacity(q_rows.len());
    for row in q_rows {
        quarantine_job_ids.push(row.try_get("job_id")?);
    }

    let theme_rows = sqlx::query(
        "SELECT theme_id, revision_digest, source_format FROM theme_revisions ORDER BY theme_id ASC, revision_digest ASC",
    )
    .fetch_all(db)
    .await?;
    let mut theme_revisions = Vec::with_capacity(theme_rows.len());
    for row in theme_rows {
        theme_revisions.push(BackupThemeRevisionItem {
            theme_id: row.try_get("theme_id")?,
            revision_digest: row.try_get("revision_digest")?,
            source_format: row.try_get("source_format")?,
        });
    }

    let secret_refs = collect_referenced_secret_refs(db).await?;
    let extension_secret_refs = collect_referenced_extension_secret_refs(db).await?;

    Ok(BackupInventory {
        media,
        quarantine_job_ids,
        theme_revisions,
        secret_refs,
        extension_secret_refs,
    })
}
