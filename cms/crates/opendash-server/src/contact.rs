use std::net::IpAddr;
use std::sync::Arc;

use opendash_core::contact::{FieldValidationError, validate_and_normalize_submission};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio::sync::Semaphore;

use crate::{
    auth::current_time_secs,
    error::{ApiError, ApiResult},
    storage::contact as store,
};

pub const MAX_CONCURRENT_SUBMISSIONS: usize = 4;

#[derive(Debug, Clone)]
pub struct SubmitInput {
    pub form_id: i64,
    pub form_version: i64,
    pub nonce: String,
    pub honeypot: String,
    pub values: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub form_id: i64,
    pub submission_id: Option<i64>,
    pub deduplicated: bool,
    pub honeypot_triggered: bool,
    pub initial_attempt_id: Option<i64>,
}

#[derive(Debug)]
pub enum SubmitOutcome {
    Accepted(Receipt),
    RateLimited { retry_after_secs: u64 },
    ValidationFailed(Vec<FieldValidationError>),
}

#[derive(Clone)]
pub struct ContactAdmissionLimiter {
    semaphore: Arc<Semaphore>,
}

impl Default for ContactAdmissionLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl ContactAdmissionLimiter {
    pub fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_SUBMISSIONS)),
        }
    }
}

pub fn normalize_peer_ip(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let mut segs = v6.segments();
            segs[4] = 0;
            segs[5] = 0;
            segs[6] = 0;
            segs[7] = 0;
            std::net::Ipv6Addr::from(segs).to_string()
        }
    }
}

fn hash_peer_with_salt(normalized_peer: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalized_peer.as_bytes());
    hasher.update(b":");
    hasher.update(salt.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn valid_nonce(nonce: &str) -> bool {
    nonce.len() == 64
        && nonce
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub async fn submit(
    db: &SqlitePool,
    limiter: &ContactAdmissionLimiter,
    peer_ip: IpAddr,
    input: SubmitInput,
) -> ApiResult<SubmitOutcome> {
    if input.form_id <= 0 {
        return Err(ApiError::not_found("Không tìm thấy form liên hệ."));
    }
    let Ok(_permit) = limiter.semaphore.clone().try_acquire_owned() else {
        return Ok(SubmitOutcome::RateLimited {
            retry_after_secs: 5,
        });
    };

    let now_secs = current_time_secs();
    let normalized_peer = normalize_peer_ip(peer_ip);
    if let Some(retry_after_secs) = store::check_and_increment_rate_limits(
        db,
        input.form_id,
        |salt| hash_peer_with_salt(&normalized_peer, salt),
        now_secs,
    )
    .await?
    {
        return Ok(SubmitOutcome::RateLimited { retry_after_secs });
    }

    if !input.honeypot.trim().is_empty() {
        return Ok(SubmitOutcome::Accepted(Receipt {
            form_id: input.form_id,
            submission_id: None,
            deduplicated: false,
            honeypot_triggered: true,
            initial_attempt_id: None,
        }));
    }

    if !valid_nonce(&input.nonce) {
        return Err(ApiError::unprocessable("Nonce biểu mẫu không hợp lệ."));
    }

    let form = store::get_form(db, input.form_id).await?;
    if form.status != "active" {
        return Err(ApiError::not_found("Form liên hệ hiện không hoạt động."));
    }
    if form.version != input.form_version {
        return Err(ApiError::conflict(
            "Phiên bản form đã thay đổi; vui lòng tải lại trang.",
        ));
    }

    let validated = match validate_and_normalize_submission(&form.schema, &input.values) {
        Ok(v) => v,
        Err(errs) => return Ok(SubmitOutcome::ValidationFailed(errs)),
    };

    let schema_snapshot_json =
        serde_json::to_string(&form.schema).map_err(|e| ApiError::internal(e.to_string()))?;
    let values_json =
        serde_json::to_string(&validated.values).map_err(|e| ApiError::internal(e.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(values_json.as_bytes());
    let payload_hash = format!("{:x}", hasher.finalize());

    let mut tx = db.begin().await?;
    let (submission_id, deduplicated, initial_attempt_id) = store::record_submission_in_tx(
        &mut tx,
        form.id,
        form.version,
        &input.nonce,
        &payload_hash,
        &schema_snapshot_json,
        &values_json,
        validated.reply_to_email.as_deref(),
        now_secs,
    )
    .await?;
    tx.commit().await?;

    Ok(SubmitOutcome::Accepted(Receipt {
        form_id: form.id,
        submission_id: Some(submission_id),
        deduplicated,
        honeypot_triggered: false,
        initial_attempt_id,
    }))
}
