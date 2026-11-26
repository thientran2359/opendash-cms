use std::{future::Future, net::IpAddr, pin::Pin, sync::Arc, time::Duration};

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::Mailbox,
    transport::smtp::{
        authentication::Credentials,
        client::{Tls, TlsParameters},
    },
};
use sqlx::SqlitePool;
use tokio::sync::Semaphore;

use crate::{
    error::{ApiError, ApiResult},
    secrets::SecretStore,
    storage::smtp::{
        self as smtp_store, NotificationAttemptFull, NotificationAttemptPublic, TlsMode,
    },
};

pub const SMTP_MAX_CONCURRENT_SENDS: usize = 4;
pub const SMTP_TOTAL_DEADLINE: Duration = Duration::from_secs(10);
pub const MAX_SMTP_BODY_BYTES: usize = 32 * 1024;
pub const MAX_SMTP_HOST_BYTES: usize = 253;

#[derive(Debug, Clone)]
pub struct MailRequest {
    pub host: String,
    pub port: u16,
    pub tls_mode: TlsMode,
    pub username: String,
    pub password: Option<String>,
    pub from: Mailbox,
    pub to: Mailbox,
    pub reply_to: Option<Mailbox>,
    pub subject: String,
    pub body: String,
}

#[derive(Debug)]
pub struct SmtpTransportError(pub String);

pub trait SmtpSender: Send + Sync {
    fn send_mail<'a>(
        &'a self,
        req: &'a MailRequest,
    ) -> Pin<Box<dyn Future<Output = Result<(), SmtpTransportError>> + Send + 'a>>;
}

pub struct LettreSmtpSender {
    #[cfg(test)]
    test_ca_pem: Option<Vec<u8>>,
}

impl LettreSmtpSender {
    pub fn new() -> Self {
        Self {
            #[cfg(test)]
            test_ca_pem: None,
        }
    }

    #[cfg(test)]
    pub fn with_test_ca_pem(ca_pem: Vec<u8>) -> Self {
        Self {
            test_ca_pem: Some(ca_pem),
        }
    }

    fn build_tls_parameters(&self, host: &str) -> Result<TlsParameters, SmtpTransportError> {
        #[allow(unused_mut)]
        let mut builder = TlsParameters::builder(host.to_string());
        #[cfg(test)]
        if let Some(pem) = &self.test_ca_pem {
            let cert = lettre::transport::smtp::client::Certificate::from_pem(pem)
                .map_err(|e| SmtpTransportError(format!("invalid test CA: {e}")))?;
            builder = builder.add_root_certificate(cert);
        }
        builder
            .build()
            .map_err(|e| SmtpTransportError(format!("tls params: {e}")))
    }
}

impl SmtpSender for LettreSmtpSender {
    fn send_mail<'a>(
        &'a self,
        req: &'a MailRequest,
    ) -> Pin<Box<dyn Future<Output = Result<(), SmtpTransportError>> + Send + 'a>> {
        Box::pin(async move {
            let mut msg_builder = Message::builder()
                .from(req.from.clone())
                .to(req.to.clone())
                .subject(&req.subject);
            if let Some(rt) = &req.reply_to {
                msg_builder = msg_builder.reply_to(rt.clone());
            }
            let message = msg_builder
                .body(req.body.clone())
                .map_err(|e| SmtpTransportError(format!("message build: {e}")))?;

            let tls_params = self.build_tls_parameters(&req.host)?;
            let mut transport_builder = match req.tls_mode {
                TlsMode::ImplicitTls => AsyncSmtpTransport::<Tokio1Executor>::relay(&req.host)
                    .map_err(|e| SmtpTransportError(format!("relay init: {e}")))?
                    .tls(Tls::Wrapper(tls_params)),
                TlsMode::Starttls => {
                    AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&req.host)
                        .map_err(|e| SmtpTransportError(format!("starttls init: {e}")))?
                        .tls(Tls::Required(tls_params))
                }
            }
            .port(req.port)
            .timeout(Some(SMTP_TOTAL_DEADLINE));

            if !req.username.is_empty() || req.password.is_some() {
                transport_builder = transport_builder.credentials(Credentials::new(
                    req.username.clone(),
                    req.password.clone().unwrap_or_default(),
                ));
            }

            let mailer = transport_builder.build();
            mailer
                .send(message)
                .await
                .map(|_| ())
                .map_err(|e| SmtpTransportError(format!("smtp send: {e}")))
        })
    }
}

pub fn validate_smtp_host(raw: &str) -> ApiResult<String> {
    let host = raw.trim();
    if host.is_empty() || host.len() > MAX_SMTP_HOST_BYTES {
        return Err(ApiError::bad_request(
            "Host SMTP không hợp lệ (1..=253 ký tự ASCII).",
        ));
    }
    if !host.is_ascii()
        || host.contains("://")
        || host.contains('/')
        || host.contains('\\')
        || host.contains('@')
        || host.contains('?')
        || host.contains('#')
        || host
            .chars()
            .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
    {
        return Err(ApiError::bad_request(
            "Host SMTP chỉ chấp nhận tên miền ASCII hoặc địa chỉ IP, không chứa URL/path/thông tin đăng nhập.",
        ));
    }
    if host.parse::<IpAddr>().is_ok() {
        return Ok(host.to_string());
    }
    if host.contains(':') {
        return Err(ApiError::bad_request("Host SMTP không được chứa cổng."));
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.iter().any(|l| {
        l.is_empty()
            || l.len() > 63
            || l.starts_with('-')
            || l.ends_with('-')
            || !l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }) {
        return Err(ApiError::bad_request("Tên miền SMTP không hợp lệ."));
    }
    Ok(host.to_ascii_lowercase())
}

pub fn validate_mailbox(raw: &str, label: &str) -> ApiResult<Mailbox> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > 320
        || trimmed.contains('\r')
        || trimmed.contains('\n')
        || trimmed.chars().any(|c| c.is_control())
    {
        return Err(ApiError::bad_request(format!("{label} không hợp lệ.")));
    }
    let mailbox: Mailbox = trimmed.parse().map_err(|_| {
        ApiError::bad_request(format!("{label} phải là đúng 1 địa chỉ email hợp lệ."))
    })?;
    Ok(mailbox)
}

#[derive(Clone)]
pub struct SmtpService {
    db: SqlitePool,
    secrets: SecretStore,
    permits: Arc<Semaphore>,
    sender: Arc<dyn SmtpSender>,
    recovery: crate::config::RecoveryState,
}

impl SmtpService {
    pub fn new(db: SqlitePool, secrets: SecretStore) -> Self {
        Self::with_recovery_state(db, secrets, crate::config::RecoveryState::new(false))
    }

    pub fn with_recovery_state(
        db: SqlitePool,
        secrets: SecretStore,
        recovery: crate::config::RecoveryState,
    ) -> Self {
        Self {
            db,
            secrets,
            permits: Arc::new(Semaphore::new(SMTP_MAX_CONCURRENT_SENDS)),
            sender: Arc::new(LettreSmtpSender::new()),
            recovery,
        }
    }

    #[cfg(test)]
    pub fn with_sender(db: SqlitePool, secrets: SecretStore, sender: Arc<dyn SmtpSender>) -> Self {
        Self {
            db,
            secrets,
            permits: Arc::new(Semaphore::new(SMTP_MAX_CONCURRENT_SENDS)),
            sender,
            recovery: crate::config::RecoveryState::new(false),
        }
    }

    pub fn spawn_initial_attempt(&self, attempt_id: i64, lease: crate::operations::MutationLease) {
        let svc = self.clone();
        tokio::spawn(async move {
            let _lease = lease;
            if let Ok(Some(attempt)) = smtp_store::claim_pending_attempt(&svc.db, attempt_id).await
            {
                let _ = svc.execute_claimed_attempt(attempt).await;
            }
        });
    }

    pub async fn execute_claimed_attempt(
        &self,
        attempt: NotificationAttemptFull,
    ) -> ApiResult<NotificationAttemptPublic> {
        if self.recovery.is_safe_mode() {
            return smtp_store::finish_attempt(&self.db, attempt.id, "failed", Some("safe_mode"))
                .await;
        }
        let req = match self.prepare_submission_mail(&attempt).await {
            Ok(req) => req,
            Err(code) => {
                return smtp_store::finish_attempt(&self.db, attempt.id, "failed", Some(&code))
                    .await;
            }
        };

        let send_res = tokio::time::timeout(SMTP_TOTAL_DEADLINE, async {
            let _permit = self
                .permits
                .acquire()
                .await
                .map_err(|_| SmtpTransportError("semaphore closed".into()))?;
            self.sender.send_mail(&req).await
        })
        .await;

        match send_res {
            Ok(Ok(())) => smtp_store::finish_attempt(&self.db, attempt.id, "sent", None).await,
            Ok(Err(_)) | Err(_) => {
                smtp_store::finish_attempt(
                    &self.db,
                    attempt.id,
                    "unknown",
                    Some("transport_error_or_timeout"),
                )
                .await
            }
        }
    }

    async fn prepare_submission_mail(
        &self,
        attempt: &NotificationAttemptFull,
    ) -> Result<MailRequest, String> {
        let snap = &attempt.snapshot;
        let host = validate_smtp_host(&snap.host).map_err(|_| "invalid_host".to_string())?;
        if snap.port == 0 {
            return Err("invalid_port".to_string());
        }
        let from = validate_mailbox(&snap.from_address, "from_address")
            .map_err(|_| "invalid_from".to_string())?;
        let to = validate_mailbox(&snap.recipient_address, "recipient_address")
            .map_err(|_| "invalid_recipient".to_string())?;

        let password = match snap.secret_ref.as_deref() {
            Some(r) => Some(
                self.secrets
                    .read_smtp_secret(r)
                    .map_err(|_| "secret_unavailable".to_string())?,
            ),
            None => None,
        };

        let ctx = smtp_store::get_submission_mail_context(&self.db, attempt.submission_id)
            .await
            .map_err(|_| "db_error".to_string())?
            .ok_or_else(|| "submission_missing".to_string())?;

        let values_json = ctx.values_json;
        let schema_json = ctx.schema_json;
        let stored_reply_to = ctx.stored_reply_to;
        let created_at = ctx.created_at;
        let form_title = ctx.form_title;

        let payload: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&values_json).map_err(|_| "invalid_payload".to_string())?;
        let schema: opendash_core::contact::FormSchema =
            serde_json::from_str(&schema_json).map_err(|_| "invalid_schema".to_string())?;

        let mut reply_to: Option<Mailbox> = None;
        if let Some(rt_email) = stored_reply_to.as_deref() {
            if let Ok(mb) = validate_mailbox(rt_email, "reply_to") {
                reply_to = Some(mb);
            }
        } else if let Some(rt_key) = schema.reply_to_field.as_deref() {
            let is_email_field = schema
                .fields
                .iter()
                .any(|f| f.key == rt_key && f.kind == opendash_core::contact::FormKind::Email);
            if is_email_field {
                if let Some(val) = payload.get(rt_key).and_then(|v| v.as_str()) {
                    if let Ok(mb) = validate_mailbox(val, "reply_to") {
                        reply_to = Some(mb);
                    }
                }
            }
        }

        let mut body = format!(
            "Thư liên hệ mới #{}\nForm: {}\nThời gian: {}\n\n",
            attempt.submission_id, form_title, created_at
        );
        for field in &schema.fields {
            if let Some(val) = payload.get(&field.key).and_then(|v| v.as_str()) {
                body.push_str(&format!("{}: {}\n", field.label, val));
            }
        }
        if body.len() > MAX_SMTP_BODY_BYTES {
            body.truncate(MAX_SMTP_BODY_BYTES);
            while !body.is_char_boundary(body.len()) {
                body.pop();
            }
        }

        Ok(MailRequest {
            host,
            port: snap.port,
            tls_mode: snap.tls_mode,
            username: snap.username.clone(),
            password,
            from,
            to,
            reply_to,
            subject: format!("OpenDash contact #{}", attempt.submission_id),
            body,
        })
    }

    pub async fn send_test_email(&self) -> ApiResult<()> {
        if self.recovery.is_safe_mode() {
            return Err(ApiError::service_unavailable(
                "Hệ thống đang ở safe mode sau khi khôi phục; SMTP đang bị tắt.",
            ));
        }
        let settings = smtp_store::get_smtp_settings(&self.db).await?;
        let host = validate_smtp_host(&settings.host)?;
        let from = validate_mailbox(&settings.from_address, "from_address")?;
        let to = validate_mailbox(&settings.recipient_address, "recipient_address")?;
        let password = match settings.secret_ref.as_deref() {
            Some(r) => Some(self.secrets.read_smtp_secret(r)?),
            None => None,
        };

        let req = MailRequest {
            host,
            port: settings.port,
            tls_mode: settings.tls_mode,
            username: settings.username,
            password,
            from,
            to,
            reply_to: None,
            subject: "OpenDash SMTP test".to_string(),
            body: "Đây là email kiểm tra cấu hình SMTP từ OpenDash Admin.".to_string(),
        };

        let res = tokio::time::timeout(SMTP_TOTAL_DEADLINE, async {
            let _permit = self
                .permits
                .acquire()
                .await
                .map_err(|_| ApiError::internal("Không thể cấp phép kết nối SMTP."))?;
            self.sender.send_mail(&req).await.map_err(|_| {
                ApiError(
                    axum::http::StatusCode::BAD_GATEWAY,
                    "Không thể gửi email kiểm tra qua máy chủ SMTP.".into(),
                )
            })
        })
        .await;

        match res {
            Ok(inner) => inner,
            Err(_) => Err(ApiError(
                axum::http::StatusCode::BAD_GATEWAY,
                "Hết thời gian chờ kết nối SMTP (10s).".into(),
            )),
        }
    }
}

#[cfg(test)]
#[path = "smtp_tests.rs"]
mod tests;
