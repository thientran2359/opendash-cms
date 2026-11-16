use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::error::{ApiError, ApiResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsMode {
    Starttls,
    ImplicitTls,
}

impl TlsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starttls => "starttls",
            Self::ImplicitTls => "implicit_tls",
        }
    }

    pub fn parse(raw: &str) -> ApiResult<Self> {
        match raw {
            "starttls" => Ok(Self::Starttls),
            "implicit_tls" => Ok(Self::ImplicitTls),
            _ => Err(ApiError::bad_request("tls_mode phải là starttls hoặc implicit_tls.")),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SmtpSettingsPublic {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub tls_mode: TlsMode,
    pub username: String,
    pub has_password: bool,
    pub from_address: String,
    pub recipient_address: String,
    pub version: i64,
}

#[derive(Debug, Clone)]
pub struct SmtpSettingsInternal {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub tls_mode: TlsMode,
    pub username: String,
    pub secret_ref: Option<String>,
    pub from_address: String,
    pub recipient_address: String,
    pub version: i64,
}

impl SmtpSettingsInternal {
    pub fn into_public(self) -> SmtpSettingsPublic {
        SmtpSettingsPublic {
            enabled: self.enabled,
            host: self.host,
            port: self.port,
            tls_mode: self.tls_mode,
            username: self.username,
            has_password: self.secret_ref.is_some(),
            from_address: self.from_address,
            recipient_address: self.recipient_address,
            version: self.version,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportSnapshot {
    pub settings_version: i64,
    pub host: String,
    pub port: u16,
    pub tls_mode: TlsMode,
    pub username: String,
    pub secret_ref: Option<String>,
    pub from_address: String,
    pub recipient_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationAttemptPublic {
    pub id: i64,
    pub attempt_number: i64,
    pub version: i64,
    pub state: String,
    pub diagnostic_code: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NotificationAttemptFull {
    pub id: i64,
    pub submission_id: i64,
    pub attempt_number: i64,
    pub state: String,
    pub version: i64,
    pub snapshot: TransportSnapshot,
}

fn map_settings_row(row: &sqlx::sqlite::SqliteRow) -> ApiResult<SmtpSettingsInternal> {
    let tls_raw: String = row.get("tls_mode");
    let port_i64: i64 = row.get("port");
    Ok(SmtpSettingsInternal {
        enabled: row.get::<i64, _>("enabled") != 0,
        host: row.get("host"),
        port: u16::try_from(port_i64).unwrap_or(587),
        tls_mode: TlsMode::parse(&tls_raw)?,
        username: row.get("username"),
        secret_ref: row.get("secret_ref"),
        from_address: row.get("from_address"),
        recipient_address: row.get("recipient_address"),
        version: row.get("version"),
    })
}

pub async fn get_smtp_settings(db: &SqlitePool) -> ApiResult<SmtpSettingsInternal> {
    let row = sqlx::query(
        "SELECT enabled, host, port, tls_mode, username, secret_ref, from_address, recipient_address, version
         FROM site_smtp_settings WHERE id = 1",
    )
    .fetch_one(db)
    .await?;
    map_settings_row(&row)
}

pub async fn get_smtp_settings_in_conn(
    conn: &mut SqliteConnection,
) -> ApiResult<SmtpSettingsInternal> {
    let row = sqlx::query(
        "SELECT enabled, host, port, tls_mode, username, secret_ref, from_address, recipient_address, version
         FROM site_smtp_settings WHERE id = 1",
    )
    .fetch_one(&mut *conn)
    .await?;
    map_settings_row(&row)
}

pub struct SmtpSettingsUpdate {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub tls_mode: TlsMode,
    pub username: String,
    pub secret_ref: Option<String>,
    pub from_address: String,
    pub recipient_address: String,
}

pub async fn put_smtp_settings_cas(
    db: &SqlitePool,
    expected_version: i64,
    update: SmtpSettingsUpdate,
) -> ApiResult<SmtpSettingsInternal> {
    let res = sqlx::query(
        "UPDATE site_smtp_settings
         SET enabled = ?, host = ?, port = ?, tls_mode = ?, username = ?,
             secret_ref = ?, from_address = ?, recipient_address = ?,
             version = version + 1, updated_at = datetime('now')
         WHERE id = 1 AND version = ?",
    )
    .bind(if update.enabled { 1_i64 } else { 0_i64 })
    .bind(&update.host)
    .bind(i64::from(update.port))
    .bind(update.tls_mode.as_str())
    .bind(&update.username)
    .bind(&update.secret_ref)
    .bind(&update.from_address)
    .bind(&update.recipient_address)
    .bind(expected_version)
    .execute(db)
    .await?;

    if res.rows_affected() == 0 {
        return Err(ApiError::conflict("Cấu hình SMTP đã thay đổi (If-Match không khớp)."));
    }
    get_smtp_settings(db).await
}

pub async fn create_initial_attempt_in_conn(
    conn: &mut SqliteConnection,
    submission_id: i64,
) -> ApiResult<Option<i64>> {
    let settings = get_smtp_settings_in_conn(conn).await?;
    let initial_state = if settings.enabled { "pending" } else { "disabled" };
    let snapshot = TransportSnapshot {
        settings_version: settings.version,
        host: settings.host,
        port: settings.port,
        tls_mode: settings.tls_mode,
        username: settings.username,
        secret_ref: settings.secret_ref,
        from_address: settings.from_address,
        recipient_address: settings.recipient_address,
    };
    let snapshot_json = serde_json::to_string(&snapshot)
        .map_err(|_| ApiError::internal("Không thể lưu snapshot SMTP."))?;

    let row = sqlx::query(
        "INSERT INTO contact_notification_attempts
            (submission_id, attempt_number, state, version, transport_snapshot_json)
         VALUES (?, 1, ?, 1, ?)
         RETURNING id",
    )
    .bind(submission_id)
    .bind(initial_state)
    .bind(&snapshot_json)
    .fetch_one(&mut *conn)
    .await?;

    sqlx::query("UPDATE contact_submissions SET notification_state = ? WHERE id = ?")
        .bind(initial_state)
        .bind(submission_id)
        .execute(&mut *conn)
        .await?;

    let id: i64 = row.get("id");
    if settings.enabled {
        Ok(Some(id))
    } else {
        Ok(None)
    }
}

pub async fn claim_pending_attempt(
    db: &SqlitePool,
    attempt_id: i64,
) -> ApiResult<Option<NotificationAttemptFull>> {
    let mut tx = db.begin().await?;
    let res = sqlx::query(
        "UPDATE contact_notification_attempts
         SET state = 'sending', version = version + 1, started_at = datetime('now')
         WHERE id = ? AND state = 'pending'",
    )
    .bind(attempt_id)
    .execute(&mut *tx)
    .await?;

    if res.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(None);
    }

    let row = sqlx::query(
        "SELECT id, submission_id, attempt_number, state, version, transport_snapshot_json
         FROM contact_notification_attempts WHERE id = ?",
    )
    .bind(attempt_id)
    .fetch_one(&mut *tx)
    .await?;
    let sub_id: i64 = row.get("submission_id");
    sqlx::query("UPDATE contact_submissions SET notification_state = 'sending' WHERE id = ?")
        .bind(sub_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let snap_json: String = row.get("transport_snapshot_json");
    let snapshot: TransportSnapshot = serde_json::from_str(&snap_json)
        .map_err(|_| ApiError::internal("Snapshot SMTP không hợp lệ."))?;
    Ok(Some(NotificationAttemptFull {
        id: row.get("id"),
        submission_id: row.get("submission_id"),
        attempt_number: row.get("attempt_number"),
        state: row.get("state"),
        version: row.get("version"),
        snapshot,
    }))
}

pub async fn finish_attempt(
    db: &SqlitePool,
    attempt_id: i64,
    new_state: &str,
    diagnostic_code: Option<&str>,
) -> ApiResult<NotificationAttemptPublic> {
    let mut tx = db.begin().await?;
    let row = sqlx::query(
        "UPDATE contact_notification_attempts
         SET state = ?, version = version + 1, finished_at = datetime('now'), diagnostic_code = ?
         WHERE id = ? AND state = 'sending'
         RETURNING id, submission_id, attempt_number, version, state, diagnostic_code, started_at, finished_at",
    )
    .bind(new_state)
    .bind(diagnostic_code)
    .bind(attempt_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = row else {
        return Err(ApiError::conflict("Trạng thái attempt đã thay đổi."));
    };
    let sub_id: i64 = row.get("submission_id");
    sqlx::query("UPDATE contact_submissions SET notification_state = ? WHERE id = ?")
        .bind(new_state)
        .bind(sub_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(NotificationAttemptPublic {
        id: row.get("id"),
        attempt_number: row.get("attempt_number"),
        version: row.get("version"),
        state: row.get("state"),
        diagnostic_code: row.get("diagnostic_code"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
    })
}

pub async fn claim_or_create_manual_attempt(
    db: &SqlitePool,
    submission_id: i64,
    expected_attempt_version: i64,
    acknowledge_duplicate_risk: bool,
) -> ApiResult<NotificationAttemptFull> {
    let mut tx = db.begin().await?;
    let sub_exists: Option<i64> =
        sqlx::query_scalar("SELECT id FROM contact_submissions WHERE id = ?")
            .bind(submission_id)
            .fetch_optional(&mut *tx)
            .await?;
    if sub_exists.is_none() {
        return Err(ApiError::not_found("Không tìm thấy thư liên hệ."));
    }

    let latest = sqlx::query(
        "SELECT id, attempt_number, state, version
         FROM contact_notification_attempts
         WHERE submission_id = ?
         ORDER BY attempt_number DESC
         LIMIT 1",
    )
    .bind(submission_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(latest) = latest else {
        return Err(ApiError::not_found("Không tìm thấy attempt thông báo."));
    };

    let latest_id: i64 = latest.get("id");
    let latest_num: i64 = latest.get("attempt_number");
    let latest_state: String = latest.get("state");
    let latest_ver: i64 = latest.get("version");

    if latest_ver != expected_attempt_version {
        return Err(ApiError::conflict(
            "Phiên bản attempt thông báo không khớp (If-Match).",
        ));
    }
    if latest_state == "sending" {
        return Err(ApiError::conflict("Thông báo đang được gửi."));
    }
    if latest_state == "sent" {
        return Err(ApiError::conflict("Thông báo đã được gửi thành công."));
    }
    if latest_state == "unknown" && !acknowledge_duplicate_risk {
        return Err(ApiError::conflict(
            "Attempt trước ở trạng thái unknown; cần acknowledge_duplicate_risk=true để gửi lại.",
        ));
    }

    if latest_state == "pending" {
        // Refresh snapshot from current settings and claim pending attempt #latest_num
        let settings = get_smtp_settings_in_conn(&mut tx).await?;
        let snapshot = TransportSnapshot {
            settings_version: settings.version,
            host: settings.host,
            port: settings.port,
            tls_mode: settings.tls_mode,
            username: settings.username,
            secret_ref: settings.secret_ref,
            from_address: settings.from_address,
            recipient_address: settings.recipient_address,
        };
        let snap_json = serde_json::to_string(&snapshot)
            .map_err(|_| ApiError::internal("Không thể lưu snapshot SMTP."))?;
        let updated = sqlx::query(
            "UPDATE contact_notification_attempts
             SET state = 'sending', version = version + 1, started_at = datetime('now'),
                 transport_snapshot_json = ?
             WHERE id = ? AND state = 'pending' AND version = ?
             RETURNING id, submission_id, attempt_number, state, version",
        )
        .bind(&snap_json)
        .bind(latest_id)
        .bind(expected_attempt_version)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = updated else {
            return Err(ApiError::conflict("Attempt đã thay đổi."));
        };
        tx.commit().await?;
        return Ok(NotificationAttemptFull {
            id: row.get("id"),
            submission_id: row.get("submission_id"),
            attempt_number: row.get("attempt_number"),
            state: row.get("state"),
            version: row.get("version"),
            snapshot,
        });
    }

    if latest_num >= 3 {
        return Err(ApiError::conflict(
            "Đã đạt giới hạn tối đa 3 lần thử thông báo cho thư liên hệ này.",
        ));
    }

    let settings = get_smtp_settings_in_conn(&mut tx).await?;
    let snapshot = TransportSnapshot {
        settings_version: settings.version,
        host: settings.host,
        port: settings.port,
        tls_mode: settings.tls_mode,
        username: settings.username,
        secret_ref: settings.secret_ref,
        from_address: settings.from_address,
        recipient_address: settings.recipient_address,
    };
    let snap_json = serde_json::to_string(&snapshot)
        .map_err(|_| ApiError::internal("Không thể lưu snapshot SMTP."))?;

    let row = sqlx::query(
        "INSERT INTO contact_notification_attempts
            (submission_id, attempt_number, state, version, transport_snapshot_json, started_at)
         VALUES (?, ?, 'sending', 1, ?, datetime('now'))
         RETURNING id, submission_id, attempt_number, state, version",
    )
    .bind(submission_id)
    .bind(latest_num + 1)
    .bind(&snap_json)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(NotificationAttemptFull {
        id: row.get("id"),
        submission_id: row.get("submission_id"),
        attempt_number: row.get("attempt_number"),
        state: row.get("state"),
        version: row.get("version"),
        snapshot,
    })
}

pub async fn list_attempts_for_submission(
    db: &SqlitePool,
    submission_id: i64,
) -> ApiResult<Vec<NotificationAttemptPublic>> {
    let rows = sqlx::query(
        "SELECT id, attempt_number, version, state, diagnostic_code, started_at, finished_at
         FROM contact_notification_attempts
         WHERE submission_id = ?
         ORDER BY attempt_number ASC",
    )
    .bind(submission_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| NotificationAttemptPublic {
            id: row.get("id"),
            attempt_number: row.get("attempt_number"),
            version: row.get("version"),
            state: row.get("state"),
            diagnostic_code: row.get("diagnostic_code"),
            started_at: row.get("started_at"),
            finished_at: row.get("finished_at"),
        })
        .collect())
}

pub async fn recover_inflight_notifications(db: &SqlitePool) -> ApiResult<u64> {
    let mut tx = db.begin().await?;
    let res = sqlx::query(
        "UPDATE contact_notification_attempts
         SET state = 'unknown',
             version = version + 1,
             finished_at = COALESCE(finished_at, datetime('now')),
             diagnostic_code = 'interrupted_on_startup'
         WHERE state = 'sending'",
    )
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() > 0 {
        sqlx::query(
            "UPDATE contact_submissions
             SET notification_state = 'unknown'
             WHERE id IN (
                 SELECT DISTINCT submission_id FROM contact_notification_attempts
                 WHERE state = 'unknown' AND diagnostic_code = 'interrupted_on_startup'
             )",
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(res.rows_affected())
}

pub struct SubmissionMailContext {
    pub values_json: String,
    pub schema_json: String,
    pub stored_reply_to: Option<String>,
    pub created_at: String,
    pub form_title: String,
}

pub async fn get_submission_mail_context(
    db: &SqlitePool,
    submission_id: i64,
) -> Result<Option<SubmissionMailContext>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT s.values_json, s.schema_snapshot_json, s.reply_to_email, s.created_at, f.title
         FROM contact_submissions s
         JOIN contact_forms f ON f.id = s.form_id
         WHERE s.id = ?",
    )
    .bind(submission_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|r| SubmissionMailContext {
        values_json: r.get("values_json"),
        schema_json: r.get("schema_snapshot_json"),
        stored_reply_to: r.get("reply_to_email"),
        created_at: r.get("created_at"),
        form_title: r.get("title"),
    }))
}
