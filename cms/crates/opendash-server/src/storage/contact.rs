use std::collections::BTreeMap;

use opendash_core::{
    contact::{FormSchema, validate_form_schema},
    id::valid_kebab_segment_with_limit,
};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use crate::{
    auth::generate_random_token,
    error::{ApiError, ApiResult},
};

pub const MAX_ACTIVE_RATE_BUCKETS: i64 = 10_000;
pub const RATE_LIMIT_PER_MIN: i64 = 5;
pub const RATE_LIMIT_PER_HOUR: i64 = 20;
pub const RATE_LIMIT_GLOBAL_PER_MIN: i64 = 100;
pub const DEDUP_TTL_SECS: i64 = 86_400;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactFormRecord {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub status: String,
    pub schema: FormSchema,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct ContactFormRow {
    id: i64,
    slug: String,
    title: String,
    status: String,
    schema_json: String,
    version: i64,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ContactFormRow> for ContactFormRecord {
    type Error = ApiError;
    fn try_from(row: ContactFormRow) -> Result<Self, Self::Error> {
        let schema: FormSchema = serde_json::from_str(&row.schema_json)
            .map_err(|e| ApiError::internal(format!("Schema form lỗi trong DB: {e}")))?;
        Ok(Self {
            id: row.id,
            slug: row.slug,
            title: row.title,
            status: row.status,
            schema,
            version: row.version,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactSubmissionSummary {
    pub id: i64,
    pub form_id: i64,
    pub form_version: i64,
    pub notification_state: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactSubmissionDetail {
    pub id: i64,
    pub form_id: i64,
    pub form_version: i64,
    pub schema_snapshot: FormSchema,
    pub values: BTreeMap<String, serde_json::Value>,
    pub reply_to_email: Option<String>,
    pub notification_state: String,
    pub created_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct ContactSubmissionRow {
    id: i64,
    form_id: i64,
    form_version: i64,
    schema_snapshot_json: String,
    values_json: String,
    reply_to_email: Option<String>,
    notification_state: String,
    created_at: String,
}

impl TryFrom<ContactSubmissionRow> for ContactSubmissionDetail {
    type Error = ApiError;
    fn try_from(row: ContactSubmissionRow) -> Result<Self, Self::Error> {
        let schema_snapshot: FormSchema = serde_json::from_str(&row.schema_snapshot_json)
            .map_err(|e| ApiError::internal(format!("Snapshot schema form lỗi: {e}")))?;
        let values: BTreeMap<String, serde_json::Value> = serde_json::from_str(&row.values_json)
            .map_err(|e| ApiError::internal(format!("Dữ liệu submission lỗi: {e}")))?;
        Ok(Self {
            id: row.id,
            form_id: row.form_id,
            form_version: row.form_version,
            schema_snapshot,
            values,
            reply_to_email: row.reply_to_email,
            notification_state: row.notification_state,
            created_at: row.created_at,
        })
    }
}

fn validate_form_meta(slug: &str, title: &str, status: &str, schema: &FormSchema) -> ApiResult<()> {
    if !valid_kebab_segment_with_limit(slug, 64) {
        return Err(ApiError::unprocessable(
            "Slug form phải là kebab-case ASCII (tối đa 64 byte).",
        ));
    }
    let t = title.trim();
    if t.is_empty() || t.chars().count() > 160 || t.chars().any(char::is_control) {
        return Err(ApiError::unprocessable(
            "Tiêu đề form không hợp lệ (1..=160 ký tự).",
        ));
    }
    if status != "active" && status != "disabled" {
        return Err(ApiError::unprocessable(
            "Trạng thái form phải là active hoặc disabled.",
        ));
    }
    validate_form_schema(schema).map_err(ApiError::unprocessable)?;
    Ok(())
}

pub async fn list_forms(db: &SqlitePool) -> ApiResult<Vec<ContactFormRecord>> {
    let rows: Vec<ContactFormRow> = sqlx::query_as(
        "SELECT id, slug, title, status, schema_json, version, created_at, updated_at
           FROM contact_forms
          ORDER BY id ASC",
    )
    .fetch_all(db)
    .await?;
    rows.into_iter().map(ContactFormRecord::try_from).collect()
}

pub async fn get_form_in_conn(
    conn: &mut SqliteConnection,
    id: i64,
) -> ApiResult<Option<ContactFormRecord>> {
    let row: Option<ContactFormRow> = sqlx::query_as(
        "SELECT id, slug, title, status, schema_json, version, created_at, updated_at
           FROM contact_forms
          WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(ContactFormRecord::try_from).transpose()
}

pub async fn get_form(db: &SqlitePool, id: i64) -> ApiResult<ContactFormRecord> {
    let mut conn = db.acquire().await?;
    get_form_in_conn(&mut conn, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy form liên hệ."))
}

pub async fn create_form(
    db: &SqlitePool,
    slug: &str,
    title: &str,
    status: &str,
    schema: &FormSchema,
) -> ApiResult<ContactFormRecord> {
    validate_form_meta(slug, title, status, schema)?;
    let schema_json =
        serde_json::to_string(schema).map_err(|e| ApiError::internal(e.to_string()))?;
    let res = sqlx::query(
        "INSERT INTO contact_forms (slug, title, status, schema_json)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(slug)
    .bind(title.trim())
    .bind(status)
    .bind(schema_json)
    .execute(db)
    .await;

    match res {
        Ok(r) => get_form(db, r.last_insert_rowid()).await,
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            Err(ApiError::conflict("Slug form đã tồn tại."))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn update_form(
    db: &SqlitePool,
    id: i64,
    expected_version: i64,
    slug: &str,
    title: &str,
    status: &str,
    schema: &FormSchema,
) -> ApiResult<ContactFormRecord> {
    validate_form_meta(slug, title, status, schema)?;
    let schema_json =
        serde_json::to_string(schema).map_err(|e| ApiError::internal(e.to_string()))?;
    let res = sqlx::query(
        "UPDATE contact_forms
            SET slug = ?1,
                title = ?2,
                status = ?3,
                schema_json = ?4,
                version = version + 1,
                updated_at = datetime('now')
          WHERE id = ?5 AND version = ?6",
    )
    .bind(slug)
    .bind(title.trim())
    .bind(status)
    .bind(schema_json)
    .bind(id)
    .bind(expected_version)
    .execute(db)
    .await;

    match res {
        Ok(r) if r.rows_affected() == 1 => get_form(db, id).await,
        Ok(_) => {
            let exists: Option<(i64,)> =
                sqlx::query_as("SELECT id FROM contact_forms WHERE id = ?1")
                    .bind(id)
                    .fetch_optional(db)
                    .await?;
            if exists.is_none() {
                Err(ApiError::not_found("Không tìm thấy form liên hệ."))
            } else {
                Err(ApiError::precondition_failed(
                    "Form đã thay đổi ở phiên bản khác.",
                ))
            }
        }
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            Err(ApiError::conflict("Slug form đã tồn tại."))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn delete_form(db: &SqlitePool, id: i64, expected_version: i64) -> ApiResult<()> {
    let mut tx = db.begin().await?;
    let row: Option<(i64,)> = sqlx::query_as("SELECT version FROM contact_forms WHERE id = ?1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some((ver,)) = row else {
        return Err(ApiError::not_found("Không tìm thấy form liên hệ."));
    };
    if ver != expected_version {
        return Err(ApiError::precondition_failed(
            "Form đã thay đổi ở phiên bản khác.",
        ));
    }
    let sub_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM contact_submissions WHERE form_id = ?1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if sub_count > 0 {
        return Err(ApiError::conflict(
            "Không thể xóa form đã có dữ liệu gửi; hãy chuyển sang trạng thái disabled.",
        ));
    }
    sqlx::query("DELETE FROM contact_forms WHERE id = ?1 AND version = ?2")
        .bind(id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn list_submissions(
    db: &SqlitePool,
    form_id: Option<i64>,
    limit: i64,
    offset: i64,
) -> ApiResult<Vec<ContactSubmissionSummary>> {
    #[derive(sqlx::FromRow)]
    struct SummaryRow {
        id: i64,
        form_id: i64,
        form_version: i64,
        notification_state: String,
        created_at: String,
    }
    let rows: Vec<SummaryRow> = if let Some(fid) = form_id {
        sqlx::query_as(
            "SELECT id, form_id, form_version, notification_state, created_at
               FROM contact_submissions
              WHERE form_id = ?1
              ORDER BY id DESC
              LIMIT ?2 OFFSET ?3",
        )
        .bind(fid)
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, form_id, form_version, notification_state, created_at
               FROM contact_submissions
              ORDER BY id DESC
              LIMIT ?1 OFFSET ?2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(db)
        .await?
    };
    Ok(rows
        .into_iter()
        .map(|r| ContactSubmissionSummary {
            id: r.id,
            form_id: r.form_id,
            form_version: r.form_version,
            notification_state: r.notification_state,
            created_at: r.created_at,
        })
        .collect())
}

pub async fn get_submission(db: &SqlitePool, id: i64) -> ApiResult<ContactSubmissionDetail> {
    let row: Option<ContactSubmissionRow> = sqlx::query_as(
        "SELECT id, form_id, form_version, schema_snapshot_json, values_json,
                reply_to_email, notification_state, created_at
           FROM contact_submissions
          WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    let Some(row) = row else {
        return Err(ApiError::not_found("Không tìm thấy lượt gửi liên hệ."));
    };
    ContactSubmissionDetail::try_from(row)
}

pub async fn delete_submission(db: &SqlitePool, id: i64) -> ApiResult<()> {
    let res = sqlx::query("DELETE FROM contact_submissions WHERE id = ?1")
        .bind(id)
        .execute(db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::not_found("Không tìm thấy lượt gửi liên hệ."));
    }
    Ok(())
}

pub async fn get_or_init_rate_salt_in_tx(conn: &mut SqliteConnection) -> ApiResult<String> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT rate_salt FROM contact_site_state WHERE id = 1")
            .fetch_optional(&mut *conn)
            .await?;
    if let Some(salt) = existing {
        return Ok(salt);
    }
    let salt = generate_random_token();
    sqlx::query("INSERT OR IGNORE INTO contact_site_state (id, rate_salt) VALUES (1, ?1)")
        .bind(&salt)
        .execute(&mut *conn)
        .await?;
    let stored: String =
        sqlx::query_scalar("SELECT rate_salt FROM contact_site_state WHERE id = 1")
            .fetch_one(&mut *conn)
            .await?;
    Ok(stored)
}

pub async fn check_and_increment_rate_limits(
    db: &SqlitePool,
    form_id: i64,
    peer_hash_fn: impl FnOnce(&str) -> String,
    now_secs: i64,
) -> ApiResult<Option<u64>> {
    let mut tx = db.begin().await?;
    let salt = get_or_init_rate_salt_in_tx(&mut tx).await?;
    let peer_hash = peer_hash_fn(&salt);

    sqlx::query("DELETE FROM contact_rate_buckets WHERE expires_at <= ?1")
        .bind(now_secs)
        .execute(&mut *tx)
        .await?;

    let min_window = now_secs / 60;
    let min_expires = (min_window + 1) * 60;
    let hour_window = now_secs / 3600;
    let hour_expires = (hour_window + 1) * 3600;

    let min_key = format!("m:{form_id}:{peer_hash}:{min_window}");
    let hour_key = format!("h:{form_id}:{peer_hash}:{hour_window}");
    let global_key = format!("g:{min_window}");

    let min_count: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT count FROM contact_rate_buckets WHERE bucket_key = ?1), 0)",
    )
    .bind(&min_key)
    .fetch_one(&mut *tx)
    .await?;
    if min_count >= RATE_LIMIT_PER_MIN {
        tx.commit().await?;
        return Ok(Some((min_expires - now_secs).max(1) as u64));
    }

    let hour_count: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT count FROM contact_rate_buckets WHERE bucket_key = ?1), 0)",
    )
    .bind(&hour_key)
    .fetch_one(&mut *tx)
    .await?;
    if hour_count >= RATE_LIMIT_PER_HOUR {
        tx.commit().await?;
        return Ok(Some((hour_expires - now_secs).max(1) as u64));
    }

    let global_count: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT count FROM contact_rate_buckets WHERE bucket_key = ?1), 0)",
    )
    .bind(&global_key)
    .fetch_one(&mut *tx)
    .await?;
    if global_count >= RATE_LIMIT_GLOBAL_PER_MIN {
        tx.commit().await?;
        return Ok(Some((min_expires - now_secs).max(1) as u64));
    }

    let new_buckets_needed = i64::from(min_count == 0)
        + i64::from(hour_count == 0)
        + i64::from(global_count == 0);
    if new_buckets_needed > 0 {
        let active_buckets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contact_rate_buckets")
            .fetch_one(&mut *tx)
            .await?;
        if active_buckets + new_buckets_needed > MAX_ACTIVE_RATE_BUCKETS {
            tx.commit().await?;
            return Ok(Some((min_expires - now_secs).max(1) as u64));
        }
    }

    for (key, expires) in [
        (&min_key, min_expires),
        (&hour_key, hour_expires),
        (&global_key, min_expires),
    ] {
        sqlx::query(
            "INSERT INTO contact_rate_buckets (bucket_key, count, expires_at)
             VALUES (?1, 1, ?2)
             ON CONFLICT(bucket_key) DO UPDATE SET count = count + 1",
        )
        .bind(key)
        .bind(expires)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
pub async fn record_submission_in_tx(
    conn: &mut SqliteConnection,
    form_id: i64,
    form_version: i64,
    nonce: &str,
    payload_hash: &str,
    schema_snapshot_json: &str,
    values_json: &str,
    reply_to_email: Option<&str>,
    now_secs: i64,
) -> ApiResult<(i64, bool)> {
    sqlx::query("DELETE FROM contact_submission_dedup WHERE expires_at <= ?1")
        .bind(now_secs)
        .execute(&mut *conn)
        .await?;

    let existing: Option<(String, i64)> = sqlx::query_as(
        "SELECT payload_hash, submission_id
           FROM contact_submission_dedup
          WHERE form_id = ?1 AND nonce = ?2 AND expires_at > ?3",
    )
    .bind(form_id)
    .bind(nonce)
    .bind(now_secs)
    .fetch_optional(&mut *conn)
    .await?;

    if let Some((prev_hash, prev_sub_id)) = existing {
        if prev_hash == payload_hash {
            return Ok((prev_sub_id, true));
        }
        return Err(ApiError::conflict(
            "Yêu cầu trùng lặp với nội dung khác.",
        ));
    }

    let res = sqlx::query(
        "INSERT INTO contact_submissions
            (form_id, form_version, schema_snapshot_json, values_json, reply_to_email, notification_state)
         VALUES (?1, ?2, ?3, ?4, ?5, 'disabled')",
    )
    .bind(form_id)
    .bind(form_version)
    .bind(schema_snapshot_json)
    .bind(values_json)
    .bind(reply_to_email)
    .execute(&mut *conn)
    .await?;
    let submission_id = res.last_insert_rowid();

    sqlx::query(
        "INSERT INTO contact_submission_dedup (form_id, nonce, payload_hash, submission_id, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(form_id)
    .bind(nonce)
    .bind(payload_hash)
    .bind(submission_id)
    .bind(now_secs + DEDUP_TTL_SECS)
    .execute(&mut *conn)
    .await?;

    Ok((submission_id, false))
}
