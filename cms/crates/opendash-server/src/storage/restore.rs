//! Read-only defensive schema fingerprinting, migration validation, and copy sanitation (M8-S05-T01).

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Connection, Row, SqliteConnection};

use crate::error::{ApiError, ApiResult};

pub const MAX_SCHEMA_OBJECTS: i64 = 256;
pub const MAX_SCHEMA_SQL_PER_OBJECT_BYTES: i64 = 256 * 1024;
pub const MAX_SCHEMA_TOTAL_SQL_BYTES: i64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SchemaObjectFingerprint {
    pub object_type: String,
    pub name: String,
    pub tbl_name: String,
    pub sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaFingerprintFixture {
    pub latest_schema_version: i64,
    pub objects: Vec<SchemaObjectFingerprint>,
}

fn compiled_migrator() -> &'static Migrator {
    &crate::storage::MIGRATOR
}

pub async fn build_expected_schema_objects(
    declared_version: i64,
) -> ApiResult<BTreeSet<SchemaObjectFingerprint>> {
    let base = compiled_migrator();
    let max_compiled = base.migrations.iter().map(|m| m.version).max().unwrap_or(0);
    if declared_version <= 0 || declared_version > max_compiled {
        return Err(ApiError::bad_request(
            "Phiên bản schema trong backup không được hỗ trợ hoặc mới hơn phiên bản hiện tại.",
        ));
    }

    let subset_migrations: Vec<_> = base
        .migrations
        .iter()
        .filter(|m| m.version <= declared_version)
        .cloned()
        .collect();
    if subset_migrations.is_empty() {
        return Err(ApiError::bad_request(
            "Không có migration hợp lệ cho schema backup.",
        ));
    }

    let sub_migrator = Migrator {
        migrations: Cow::Owned(subset_migrations),
        ignore_missing: base.ignore_missing,
        locking: base.locking,
        no_tx: base.no_tx,
        table_name: base.table_name.clone(),
        create_schemas: base.create_schemas.clone(),
    };

    let mut mem = SqliteConnection::connect("sqlite::memory:")
        .await
        .map_err(|_| ApiError::internal("Không khởi tạo được SQLite memory để dựng schema."))?;
    sub_migrator
        .run(&mut mem)
        .await
        .map_err(|_| ApiError::internal("Không chạy được compiled migrations trên memory DB."))?;

    let rows = sqlx::query(
        "SELECT type, name, tbl_name, COALESCE(sql, '') AS sql FROM sqlite_schema ORDER BY type, name, tbl_name",
    )
    .fetch_all(&mut mem)
    .await?;

    let mut set = BTreeSet::new();
    for row in rows {
        let object_type: String = row.try_get("type")?;
        let name: String = row.try_get("name")?;
        if object_type == "table" && name == "sqlite_sequence" {
            continue;
        }
        set.insert(SchemaObjectFingerprint {
            object_type,
            name,
            tbl_name: row.try_get("tbl_name")?,
            sql: row.try_get("sql")?,
        });
    }
    mem.close().await?;
    Ok(set)
}

/// Validates a restored SQLite file in read-only + `trusted_schema=OFF` mode before ANY write/migration.
pub async fn validate_restored_db_read_only(db_path: &Path) -> ApiResult<i64> {
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(false)
        .read_only(true);
    let mut conn = SqliteConnection::connect_with(&opts)
        .await
        .map_err(|_| ApiError::bad_request("Không mở được SQLite backup ở chế độ chỉ đọc."))?;

    sqlx::query("PRAGMA trusted_schema = OFF")
        .execute(&mut conn)
        .await?;

    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut conn)
        .await?;
    if !integrity.eq_ignore_ascii_case("ok") {
        return Err(ApiError::bad_request("SQLite backup lỗi integrity_check."));
    }

    let fk_rows = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut conn)
        .await?;
    if !fk_rows.is_empty() {
        return Err(ApiError::bad_request(
            "SQLite backup vi phạm foreign_key_check.",
        ));
    }

    // Bound schema metadata BEFORE materializing DDL strings.
    let meta_row = sqlx::query(
        r#"
        SELECT COUNT(*) AS cnt,
               COALESCE(MAX(COALESCE(length(CAST(sql AS BLOB)), 0)), 0) AS max_sql,
               COALESCE(SUM(COALESCE(length(CAST(sql AS BLOB)), 0)), 0) AS total_sql
        FROM sqlite_schema
        "#,
    )
    .fetch_one(&mut conn)
    .await?;
    let cnt: i64 = meta_row.try_get("cnt")?;
    let max_sql: i64 = meta_row.try_get("max_sql")?;
    let total_sql: i64 = meta_row.try_get("total_sql")?;
    if cnt > MAX_SCHEMA_OBJECTS
        || max_sql > MAX_SCHEMA_SQL_PER_OBJECT_BYTES
        || total_sql > MAX_SCHEMA_TOTAL_SQL_BYTES
    {
        return Err(ApiError::bad_request(
            "Metadata sqlite_schema vượt quá giới hạn an toàn (256 objects / 256KiB / 2MiB).",
        ));
    }

    // Verify _sqlx_migrations ledger against compiled migrations.
    let mig_rows =
        sqlx::query("SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version ASC")
            .fetch_all(&mut conn)
            .await
            .map_err(|_| ApiError::bad_request("Backup thiếu bảng _sqlx_migrations hợp lệ."))?;
    if mig_rows.is_empty() {
        return Err(ApiError::bad_request("Backup có _sqlx_migrations rỗng."));
    }

    let compiled = compiled_migrator();
    let mut declared_version = 0i64;
    for (idx, row) in mig_rows.iter().enumerate() {
        let ver: i64 = row.try_get("version")?;
        let checksum: Vec<u8> = row.try_get("checksum")?;
        let success: bool = row.try_get("success")?;
        if !success {
            return Err(ApiError::bad_request(
                "Backup chứa migration chưa hoàn tất.",
            ));
        }
        let Some(expected_mig) = compiled.migrations.get(idx) else {
            return Err(ApiError::bad_request(
                "Backup có schema mới hơn hoặc dư thừa so với phiên bản hiện tại.",
            ));
        };
        if expected_mig.version != ver || expected_mig.checksum.as_ref() != checksum.as_slice() {
            return Err(ApiError::bad_request(
                "Lịch sử migration trong backup không khớp compiled migrations.",
            ));
        }
        declared_version = ver;
    }

    // Compare actual sqlite_schema objects against deterministic compiled schema fingerprint.
    let expected_objects = build_expected_schema_objects(declared_version).await?;
    let actual_rows = sqlx::query(
        "SELECT type, name, tbl_name, COALESCE(sql, '') AS sql FROM sqlite_schema ORDER BY type, name, tbl_name",
    )
    .fetch_all(&mut conn)
    .await?;

    let mut actual_objects = BTreeSet::new();
    for row in actual_rows {
        let object_type: String = row.try_get("type")?;
        let name: String = row.try_get("name")?;
        if object_type == "table" && name == "sqlite_sequence" {
            continue;
        }
        actual_objects.insert(SchemaObjectFingerprint {
            object_type,
            name,
            tbl_name: row.try_get("tbl_name")?,
            sql: row.try_get("sql")?,
        });
    }

    if actual_objects != expected_objects {
        return Err(ApiError::bad_request(
            "Cấu trúc sqlite_schema của backup không khớp fingerprint hợp lệ (phát hiện đối tượng lạ hoặc bị sửa đổi).",
        ));
    }

    // Spec A2 S01: Reject pre-V1 theme revisions before migration 0063 drops them.
    let has_theme_source_format: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM pragma_table_info('theme_revisions') WHERE name = 'source_format' LIMIT 1",
    )
    .fetch_optional(&mut conn)
    .await?;
    if has_theme_source_format.is_some() {
        let non_v1_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM theme_revisions WHERE source_format != 'v1'",
        )
        .fetch_one(&mut conn)
        .await?;
        if non_v1_count > 0 {
            return Err(ApiError::bad_request(
                "Bản sao khôi phục chứa theme revision tiền-V1 không được hỗ trợ.",
            ));
        }
    }

    // Verify at least one viable Admin account exists (no bootstrap bypass).
    let admin_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE role = 'admin' AND length(password_hash) > 0",
    )
    .fetch_one(&mut conn)
    .await?;
    if admin_count < 1 {
        return Err(ApiError::bad_request(
            "Backup không có tài khoản Admin hợp lệ.",
        ));
    }

    conn.close().await?;
    Ok(declared_version)
}

/// Reopens the validated copied DB read-write to apply pending migrations and sanitize runtime state.
pub async fn validate_and_rebase_copy(db_path: &Path) -> ApiResult<i64> {
    let declared_version = validate_restored_db_read_only(db_path).await?;

    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(false);
    let mut conn = SqliteConnection::connect_with(&opts)
        .await
        .map_err(|_| ApiError::internal("Không mở được bản sao SQLite để migrate/sanitize."))?;

    sqlx::query("PRAGMA trusted_schema = OFF")
        .execute(&mut conn)
        .await?;
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut conn)
        .await?;

    compiled_migrator()
        .run(&mut conn)
        .await
        .map_err(|_| ApiError::internal("Không chạy được migrations trên bản sao khôi phục."))?;
    crate::storage::page::ensure_embedded_core_blocks_in_conn(&mut conn).await?;

    let override_asts: Vec<String> = sqlx::query_scalar("SELECT ast_json FROM template_overrides")
        .fetch_all(&mut conn)
        .await?;
    for raw in override_asts {
        opendash_core::builder::BuilderDocument::from_json(raw.as_bytes())
            .map_err(|err| ApiError::bad_request(err.to_string()))?;
    }
    let entry_docs: Vec<String> =
        sqlx::query_scalar("SELECT document_json FROM entry_builder_documents")
            .fetch_all(&mut conn)
            .await?;
    for raw in entry_docs {
        opendash_core::builder::BuilderDocument::from_json(raw.as_bytes())
            .map_err(|err| ApiError::bad_request(err.to_string()))?;
    }

    // Clear stale auth sessions, autosaves, active operation leases/queues, and disable SMTP auto-sending.
    sqlx::query("DELETE FROM sessions")
        .execute(&mut conn)
        .await?;
    sqlx::query("DELETE FROM entry_autosaves")
        .execute(&mut conn)
        .await?;
    sqlx::query(
        "UPDATE operation_jobs SET state = 'interrupted', version = version + 1, finished_at = COALESCE(finished_at, datetime('now')), updated_at = datetime('now') WHERE state IN ('queued', 'running')",
    )
    .execute(&mut conn)
    .await?;
    sqlx::query(
        "UPDATE site_smtp_settings SET enabled = 0, version = version + 1, updated_at = datetime('now') WHERE id = 1",
    )
    .execute(&mut conn)
    .await?;
    sqlx::query(
        "UPDATE contact_notification_attempts SET state = 'unknown', version = version + 1, diagnostic_code = COALESCE(diagnostic_code, 'restored_unknown') WHERE state IN ('pending', 'sending')",
    )
    .execute(&mut conn)
    .await?;
    sqlx::query("DELETE FROM media_variants")
        .execute(&mut conn)
        .await?;
    sqlx::query("UPDATE media_assets SET variants_ready_version = 0")
        .execute(&mut conn)
        .await?;
    sqlx::query(
        "UPDATE media_replace_jobs
         SET phase = 'rolled_back',
             error_message = COALESCE(error_message, 'restored_cleanup'),
             purged_at = COALESCE(purged_at, datetime('now')),
             updated_at = datetime('now')
         WHERE phase IN ('staged', 'moving_old', 'promoting_new')",
    )
    .execute(&mut conn)
    .await?;
    sqlx::query(
        "UPDATE media_replace_jobs
         SET purged_at = COALESCE(purged_at, datetime('now')),
             updated_at = datetime('now')
         WHERE purged_at IS NULL",
    )
    .execute(&mut conn)
    .await?;

    crate::storage::search::rebuild_search_index_in_conn(&mut conn).await?;

    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut conn)
        .await?;
    if !integrity.eq_ignore_ascii_case("ok") {
        return Err(ApiError::internal(
            "Integrity check thất bại sau khi sanitize bản sao.",
        ));
    }
    let fk_rows = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut conn)
        .await?;
    if !fk_rows.is_empty() {
        return Err(ApiError::internal(
            "Foreign key check thất bại sau khi sanitize bản sao.",
        ));
    }

    let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut conn)
        .await;
    let final_version: i64 =
        sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(&mut conn)
            .await?;
    conn.close().await?;
    Ok(final_version.max(declared_version))
}

pub async fn verify_restored_db_references(db_path: &Path, site_root: &Path) -> ApiResult<()> {
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(false)
        .read_only(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await?;

    let inventory = crate::storage::backup::collect_backup_inventory(&pool).await?;
    pool.close().await;

    for m in &inventory.media {
        let orig = site_root
            .join("uploads")
            .join("media")
            .join(&m.id[..2])
            .join(format!("{}.{}", m.id, m.extension));
        if !orig.is_file() {
            return Err(ApiError::bad_request(
                "Bản sao khôi phục thiếu tệp media gốc được tham chiếu trong DB.",
            ));
        }
    }

    for secret_ref in &inventory.secret_refs {
        let sec = site_root
            .join("secrets")
            .join("smtp")
            .join(format!("{secret_ref}.json"));
        if !sec.is_file() {
            return Err(ApiError::bad_request(
                "Bản sao khôi phục thiếu tệp SMTP secret được tham chiếu trong DB.",
            ));
        }
    }

    for ext in &inventory.extension_secret_refs {
        let sec = site_root
            .join("database")
            .join("secrets")
            .join(&ext.owner_subdir)
            .join(&ext.package_id)
            .join(format!("{}.json", ext.secret_ref));
        if !sec.is_file() {
            return Err(ApiError::bad_request(
                "Bản sao khôi phục thiếu tệp bí mật extension được tham chiếu trong DB.",
            ));
        }
    }

    for tr in &inventory.theme_revisions {
        if tr.source_format != "v1" {
            return Err(ApiError::bad_request(
                "Bản sao khôi phục chứa theme revision tiền-V1 không được hỗ trợ.",
            ));
        }
        let rev_dir = site_root
            .join("themes")
            .join("revisions")
            .join(&tr.revision_digest);
        if !rev_dir.join(".complete").is_file() {
            return Err(ApiError::bad_request(
                "Bản sao khôi phục thiếu .complete của theme revision.",
            ));
        }
    }

    Ok(())
}
