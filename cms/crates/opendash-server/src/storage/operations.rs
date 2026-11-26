use serde::{Deserialize, Serialize};

use sqlx::{Row, SqlitePool};

use crate::error::{ApiError, ApiResult};

pub const MAX_RETAINED_OPERATION_RECEIPTS: i64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Backup,
    Restore,
    Reconcile,
    Purge,
    MediaDelete,
}

impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Backup => "backup",
            Self::Restore => "restore",
            Self::Reconcile => "reconcile",
            Self::Purge => "purge",
            Self::MediaDelete => "media_delete",
        }
    }

    pub fn parse(raw: &str) -> ApiResult<Self> {
        match raw {
            "backup" => Ok(Self::Backup),
            "restore" => Ok(Self::Restore),
            "reconcile" => Ok(Self::Reconcile),
            "purge" => Ok(Self::Purge),
            "media_delete" => Ok(Self::MediaDelete),
            _ => Err(ApiError::bad_request("Loại tác vụ vận hành không hợp lệ.")),
        }
    }

    #[allow(dead_code)]
    pub fn is_heavy(self) -> bool {
        matches!(
            self,
            Self::Backup | Self::Restore | Self::Reconcile | Self::Purge
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl OperationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn parse(raw: &str) -> ApiResult<Self> {
        match raw {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(ApiError::bad_request("Trạng thái tác vụ không hợp lệ.")),
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperationJob {
    pub id: String,
    pub kind: OperationKind,
    pub state: OperationState,
    pub version: i64,
    pub actor_user_id: Option<String>,
    pub reason: Option<String>,
    pub summary_json: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub requested_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct JobCreateInput {
    pub id: String,
    pub kind: OperationKind,
    pub actor_user_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JobTransitionInput {
    pub expected_version: i64,
    pub next_state: OperationState,
    pub summary_json: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

pub fn is_valid_job_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn map_job_row(row: &sqlx::sqlite::SqliteRow) -> ApiResult<OperationJob> {
    let kind_raw: String = row.try_get("kind")?;
    let state_raw: String = row.try_get("state")?;
    Ok(OperationJob {
        id: row.try_get("id")?,
        kind: OperationKind::parse(&kind_raw)?,
        state: OperationState::parse(&state_raw)?,
        version: row.try_get("version")?,
        actor_user_id: row.try_get("actor_user_id")?,
        reason: row.try_get("reason")?,
        summary_json: row.try_get("summary_json")?,
        error_code: row.try_get("error_code")?,
        error_message: row.try_get("error_message")?,
        requested_at: row.try_get("requested_at")?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub async fn create_job(db: &SqlitePool, input: JobCreateInput) -> ApiResult<OperationJob> {
    if !is_valid_job_id(&input.id) {
        return Err(ApiError::bad_request("JobId phải là chuỗi 64 ký tự hex."));
    }

    let insert_res = sqlx::query(
        r#"
        INSERT INTO operation_jobs (id, kind, state, version, actor_user_id, reason)
        VALUES (?, ?, 'queued', 1, ?, ?)
        "#,
    )
    .bind(&input.id)
    .bind(input.kind.as_str())
    .bind(&input.actor_user_id)
    .bind(&input.reason)
    .execute(db)
    .await;

    if let Err(err) = insert_res {
        if let sqlx::Error::Database(db_err) = &err {
            if db_err.is_unique_violation() {
                if input.kind.is_heavy() {
                    return Err(ApiError::conflict(
                        "Đã có tác vụ bảo trì nặng đang hoạt động trên site này.",
                    ));
                }
                return Err(ApiError::conflict("Mã tác vụ đã tồn tại."));
            }
        }
        return Err(err.into());
    }

    prune_old_receipts(db, MAX_RETAINED_OPERATION_RECEIPTS).await?;
    get_job(db, &input.id)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được tác vụ vừa tạo."))
}

pub async fn get_job(db: &SqlitePool, id: &str) -> ApiResult<Option<OperationJob>> {
    let row = sqlx::query(
        r#"
        SELECT id, kind, state, version, actor_user_id, reason, summary_json,
               error_code, error_message, requested_at, started_at, finished_at, updated_at
        FROM operation_jobs
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(db)
    .await?;

    row.as_ref().map(map_job_row).transpose()
}

#[allow(dead_code)]
pub async fn list_jobs(db: &SqlitePool, limit: i64) -> ApiResult<Vec<OperationJob>> {
    let clamped = limit.clamp(1, MAX_RETAINED_OPERATION_RECEIPTS);
    let rows = sqlx::query(
        r#"
        SELECT id, kind, state, version, actor_user_id, reason, summary_json,
               error_code, error_message, requested_at, started_at, finished_at, updated_at
        FROM operation_jobs
        ORDER BY requested_at DESC, rowid DESC
        LIMIT ?
        "#,
    )
    .bind(clamped)
    .fetch_all(db)
    .await?;

    rows.iter().map(map_job_row).collect()
}

pub async fn transition_job_cas(
    db: &SqlitePool,
    id: &str,
    input: JobTransitionInput,
) -> ApiResult<OperationJob> {
    let existing = get_job(db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy tác vụ vận hành."))?;

    if existing.version != input.expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản tác vụ đã thay đổi (CAS mismatch).",
        ));
    }
    if existing.state.is_terminal() {
        return Err(ApiError::precondition_failed(
            "Tác vụ đã ở trạng thái kết thúc.",
        ));
    }

    let next_str = input.next_state.as_str();
    let result = sqlx::query(
        r#"
        UPDATE operation_jobs
        SET state = ?,
            version = version + 1,
            summary_json = COALESCE(?, summary_json),
            error_code = COALESCE(?, error_code),
            error_message = COALESCE(?, error_message),
            started_at = CASE WHEN ? = 'running' AND started_at IS NULL THEN datetime('now') ELSE started_at END,
            finished_at = CASE WHEN ? IN ('succeeded', 'failed', 'cancelled', 'interrupted') AND finished_at IS NULL THEN datetime('now') ELSE finished_at END,
            updated_at = datetime('now')
        WHERE id = ? AND version = ?
        "#,
    )
    .bind(next_str)
    .bind(&input.summary_json)
    .bind(&input.error_code)
    .bind(&input.error_message)
    .bind(next_str)
    .bind(next_str)
    .bind(id)
    .bind(input.expected_version)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::precondition_failed(
            "Phiên bản tác vụ đã thay đổi (CAS mismatch).",
        ));
    }

    if input.next_state.is_terminal() {
        prune_old_receipts(db, MAX_RETAINED_OPERATION_RECEIPTS).await?;
    }

    get_job(db, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được tác vụ sau khi cập nhật."))
}

pub async fn prune_old_receipts(db: &SqlitePool, max_retained: i64) -> ApiResult<u64> {
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM operation_jobs WHERE state IN ('queued', 'running')",
    )
    .fetch_one(db)
    .await?;

    let keep_terminal = (max_retained - active_count).max(0);
    let deleted = sqlx::query(
        r#"
        DELETE FROM operation_jobs
        WHERE id IN (
            SELECT id FROM operation_jobs
            WHERE state IN ('succeeded', 'failed', 'cancelled', 'interrupted')
            ORDER BY requested_at DESC, rowid DESC
            LIMIT -1 OFFSET ?
        )
        "#,
    )
    .bind(keep_terminal)
    .execute(db)
    .await?;

    Ok(deleted.rows_affected())
}

pub async fn mark_interrupted_on_startup(db: &SqlitePool) -> ApiResult<u64> {
    let updated = sqlx::query(
        r#"
        UPDATE operation_jobs
        SET state = 'interrupted',
            version = version + 1,
            error_code = COALESCE(error_code, 'server_restarted'),
            error_message = COALESCE(error_message, 'Tác vụ bị gián đoạn do máy chủ khởi động lại.'),
            finished_at = COALESCE(finished_at, datetime('now')),
            updated_at = datetime('now')
        WHERE state IN ('queued', 'running')
        "#,
    )
    .execute(db)
    .await?;

    Ok(updated.rows_affected())
}

#[allow(dead_code)]
pub async fn get_maintenance_generation(db: &SqlitePool) -> ApiResult<i64> {
    let generation: i64 =
        sqlx::query_scalar("SELECT generation FROM maintenance_generation WHERE id = 1")
            .fetch_one(db)
            .await?;
    Ok(generation)
}

#[allow(dead_code)]
pub async fn bump_maintenance_generation(db: &SqlitePool, kind: OperationKind) -> ApiResult<i64> {
    sqlx::query(
        r#"
        UPDATE maintenance_generation
        SET generation = generation + 1,
            last_kind = ?,
            updated_at = datetime('now')
        WHERE id = 1
        "#,
    )
    .bind(kind.as_str())
    .execute(db)
    .await?;

    get_maintenance_generation(db).await
}

pub async fn list_backup_jobs(db: &SqlitePool, limit: i64) -> ApiResult<Vec<OperationJob>> {
    let clamped = limit.clamp(1, MAX_RETAINED_OPERATION_RECEIPTS);
    let rows = sqlx::query(
        r#"
        SELECT id, kind, state, version, actor_user_id, reason, summary_json,
               error_code, error_message, requested_at, started_at, finished_at, updated_at
        FROM operation_jobs
        WHERE kind = 'backup'
        ORDER BY requested_at DESC, rowid DESC
        LIMIT ?
        "#,
    )
    .bind(clamped)
    .fetch_all(db)
    .await?;

    rows.iter().map(map_job_row).collect()
}

pub async fn mark_backup_artifact_deleted_cas(
    db: &SqlitePool,
    id: &str,
    expected_version: i64,
    updated_summary_json: &str,
) -> ApiResult<OperationJob> {
    let result = sqlx::query(
        r#"
        UPDATE operation_jobs
        SET summary_json = ?,
            version = version + 1,
            updated_at = datetime('now')
        WHERE id = ? AND kind = 'backup' AND state = 'succeeded' AND version = ?
        "#,
    )
    .bind(updated_summary_json)
    .bind(id)
    .bind(expected_version)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::precondition_failed(
            "Phiên bản bản sao lưu đã thay đổi hoặc không thể xóa (CAS mismatch).",
        ));
    }

    get_job(db, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được tác vụ sau khi xóa artifact."))
}

pub async fn is_active_admin_user(db: &SqlitePool, user_id: i64) -> ApiResult<bool> {
    let role: Option<String> = sqlx::query_scalar("SELECT role FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    Ok(role.as_deref() == Some("admin"))
}

#[cfg(test)]
#[path = "operations_tests.rs"]
mod tests;
