use std::{
    future::Future,
    net::IpAddr,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

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
                TlsMode::ImplicitTls => {
                    AsyncSmtpTransport::<Tokio1Executor>::relay(&req.host)
                        .map_err(|e| SmtpTransportError(format!("relay init: {e}")))?
                        .tls(Tls::Wrapper(tls_params))
                }
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
        return Err(ApiError::bad_request("Host SMTP không hợp lệ (1..=253 ký tự ASCII)."));
    }
    if !host.is_ascii()
        || host.contains("://")
        || host.contains('/')
        || host.contains('\\')
        || host.contains('@')
        || host.contains('?')
        || host.contains('#')
        || host.chars().any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
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
    let mailbox: Mailbox = trimmed
        .parse()
        .map_err(|_| ApiError::bad_request(format!("{label} phải là đúng 1 địa chỉ email hợp lệ.")))?;
    Ok(mailbox)
}

#[derive(Clone)]
pub struct SmtpService {
    db: SqlitePool,
    secrets: SecretStore,
    permits: Arc<Semaphore>,
    sender: Arc<dyn SmtpSender>,
}

impl SmtpService {
    pub fn new(db: SqlitePool, secrets: SecretStore) -> Self {
        Self {
            db,
            secrets,
            permits: Arc::new(Semaphore::new(SMTP_MAX_CONCURRENT_SENDS)),
            sender: Arc::new(LettreSmtpSender::new()),
        }
    }

    #[cfg(test)]
    pub fn with_sender(
        db: SqlitePool,
        secrets: SecretStore,
        sender: Arc<dyn SmtpSender>,
    ) -> Self {
        Self {
            db,
            secrets,
            permits: Arc::new(Semaphore::new(SMTP_MAX_CONCURRENT_SENDS)),
            sender,
        }
    }

    pub fn spawn_initial_attempt(&self, attempt_id: i64) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Ok(Some(attempt)) = smtp_store::claim_pending_attempt(&svc.db, attempt_id).await {
                let _ = svc.execute_claimed_attempt(attempt).await;
            }
        });
    }

    pub async fn execute_claimed_attempt(
        &self,
        attempt: NotificationAttemptFull,
    ) -> ApiResult<NotificationAttemptPublic> {
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
            let is_email_field = schema.fields.iter().any(|f| {
                f.key == rt_key && f.kind == opendash_core::contact::FormKind::Email
            });
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
            self.sender
                .send_mail(&req)
                .await
                .map_err(|_| {
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
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    const TEST_CA_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
MIIDKTCCAhGgAwIBAgIUA8j/X4MauTXT7doYoWdZASAwXnswDQYJKoZIhvcNAQEL\n\
BQAwGzEZMBcGA1UEAwwQT3BlbkRhc2ggVGVzdCBDQTAgFw0yNjExMTQwNTU4Mjda\n\
GA8yMTI2MTAyMTA1NTgyN1owGzEZMBcGA1UEAwwQT3BlbkRhc2ggVGVzdCBDQTCC\n\
ASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAKjzq3ryQw6KKs6RIBjHNvu5\n\
InbXtTH8yFMAeU8UWWFcqtasldaeZcqocmWK04anARf5Mbl6r/abRW7z3rtW4e48\n\
B8/1GV5KHVKKIiOV0ji/rD+PtP7QTlB+YB+UCKeu6LPfNC6nnh7c0xRCmKoP28R/\n\
BRn0UwFfsor9tNcF7FrLOromfvcHNo5eLVD8N9Wdkgd+RgQ8SIch8VycKw7C2vNM\n\
cKDuxtaEN+HyObVYr6IPqkZfdAacHfmianvy6EbDTBX82w+vspgG6Mj1v1c0r0U2\n\
2lqEQiWcuJK0r7T1Nw/055ZP4e93zOGzs7g6S9KAsu+UdQ0jUgS6zu0iRsDIztkC\n\
AwEAAaNjMGEwHQYDVR0OBBYEFApNCKUZ2zLWhRdsq6//d+xhBlQCMB8GA1UdIwQY\n\
MBaAFApNCKUZ2zLWhRdsq6//d+xhBlQCMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0P\n\
AQH/BAQDAgEGMA0GCSqGSIb3DQEBCwUAA4IBAQAuIqq+V6yliVJggVPzUBofBvS8\n\
WqhGATv0zBCiie/mu1csls6NYzjVE1eHl15vLGGnDCMypOiqVf5uoXcSeK/ZPVQ6\n\
4HJb7d4GgoJOLO/D1GQyVLwi/XwGhYbWIQjP3efAW4ofHf25u8La4R6gWAlgz2Rr\n\
WqW7YyAHBwzx2sN4aSpsQG5K3hUZCKJY2J7ECCDckf8lEF6UhU0vukVPdRCCST22\n\
EtdF8yZbrxqloV8LxM+/FEGFc7OGyKxiz3zLDAPEuZ+5gca4OvvKmgnClaZD25vb\n\
/9yGK2FLaYkIiyBR0bPfzB31u7qbjbGZqBQFQ3mheehG98Nxhlq2Dpp1T7bt\n\
-----END CERTIFICATE-----\n";

    const TEST_SRV_CRT: &str = "-----BEGIN CERTIFICATE-----\n\
MIIDMjCCAhqgAwIBAgIUEEaZy1FtyfK//hGHlTzCq+iek9MwDQYJKoZIhvcNAQEL\n\
BQAwGzEZMBcGA1UEAwwQT3BlbkRhc2ggVGVzdCBDQTAgFw0yNjExMTQwNTU4Mjha\n\
GA8yMTI2MTAyMTA1NTgyOFowFDESMBAGA1UEAwwJbG9jYWxob3N0MIIBIjANBgkq\n\
hkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAuxqVEUesWELa2BjLB72hNFC6Z9b4EHbT\n\
HHyacjGv+kNufGRFoRpIgOi2MXVJOt8UuYL03bOej7MgmX9aN7RCutZlYG3953OL\n\
5TSODidjM7i46Qviwq4spdDtemyFkhouN/W7+2PCRuwW7CO3tB3jPv/a2y/8Ewr+\n\
gsWPyEv39gun+er0g3AMz6hNR7vZQgG2Lacwz5iRpEC2ZywZM8naspS5dKyro4OB\n\
5VEka9JfMpNN9HZetfGI6wIyyL9ocsovSNIzWro2m7+3KQgCQGuXeJsV3ev2a0Gb\n\
9k8wLZWkto8GEAcLIQGsObGmZhCbqA8lnn1jo3tx55mdqaJ99ukOeQIDAQABo3Mw\n\
cTAaBgNVHREEEzARgglsb2NhbGhvc3SHBH8AAAEwEwYDVR0lBAwwCgYIKwYBBQUH\n\
AwEwHQYDVR0OBBYEFJxKO7O6L5e6xcgDNyXQrclpONLHMB8GA1UdIwQYMBaAFApN\n\
CKUZ2zLWhRdsq6//d+xhBlQCMA0GCSqGSIb3DQEBCwUAA4IBAQBzZXSkZnxQ1Fa9\n\
2eiikS1jNLW9DsoeyFYOM3A6VF5Ra2Vs4ZhoWOE7lV2xuJhEGdwPNxyALAJdN8k8\n\
IY6hvg99Cbh5Cf2ZkgjQ+gmk30ozWK2Wkkrnzl/T0zq4BTHWg3PWTEtvH42UowyX\n\
gD9uyYmS4nZf2YHmazV03Fp99mnUpTo2kAEcGGZJFnhhF68s8ZXNsRgkGNmbwhly\n\
Xqe7euXLDmP8r5zpOBOBpuFjjjfDDS6ZBL5GF0tEFmlBZ0TiF5xML4ZW9rpdid6f\n\
LlprUPSB+XDibIf9sWUw9I080wU2NjGarTBVxMCG0ai+PR8iAtHcAFZeOq5+2p7f\n\
isDRXraP\n\
-----END CERTIFICATE-----\n";

    const TEST_SRV_KEY: &str = "-----BEGIN PRIVATE KEY-----\n\
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7GpURR6xYQtrY\n\
GMsHvaE0ULpn1vgQdtMcfJpyMa/6Q258ZEWhGkiA6LYxdUk63xS5gvTds56PsyCZ\n\
f1o3tEK61mVgbf3nc4vlNI4OJ2MzuLjpC+LCriyl0O16bIWSGi439bv7Y8JG7Bbs\n\
I7e0HeM+/9rbL/wTCv6CxY/IS/f2C6f56vSDcAzPqE1Hu9lCAbYtpzDPmJGkQLZn\n\
LBkzydqylLl0rKujg4HlUSRr0l8yk030dl618YjrAjLIv2hyyi9I0jNaujabv7cp\n\
CAJAa5d4mxXd6/ZrQZv2TzAtlaS2jwYQBwshAaw5saZmEJuoDyWefWOje3HnmZ2p\n\
on326Q55AgMBAAECggEAXKAgQ9mznE4cgPZlLIPYFkoQve9SBvSMNoIJA5dNgxt2\n\
yuXpAoQ8eHS8HWfuxNJDNEN8bXTKfmEzNkoxo3n9vvj8XuyU0cwF6YZG/QLKvYQp\n\
JlXYAICxYbU4m262AUosr09brGwKdYnTHzvfaa+SHFIVSoVKyTKcJKkhMQtKyebH\n\
/77t1354raFUVbaPKhNYlXJozWHb6Yj+2fdeMWjzzpS1U9HNz3fuPdFAbDNsGGv+\n\
0y/yPUE3S4dFnjFDsIBQNLyiuTdqKi7RUYTZn34msNR0tQ1SYe2PupLOIy9R1jKN\n\
B8xVN7V0s06g770mGQQzsCI5J0G2+0pvPiFT/VmxcQKBgQD/zfntAkNm981+MwQH\n\
mZ8geV5V9fUZsu1JoR8tyKTrV3GvlmhpenPvaCBEqbrUBc5dyDa3YyRACY+/EKrS\n\
LL+3axvZStznNVPmy4h3pca7JjfaN5IDxy0G6L1H4zTKnYfGqiSfI2Iy/NRKhl7L\n\
IYmE27YRQd3eqJ8gF3oBFQWPywKBgQC7PyvZNyirbcpRaIYqN0dDOe0mONcgHDYp\n\
7u/51hVVbnLkLc9W8yOZxB1qBSmgWrD/M+2MpN4aWi7ug/rxma4s1bBb+P8mSyzi\n\
EaMuiOhq9XnBll1skjd6EP1/NGZN2wF6VmBw8hTpr/oUFT0tndQFydVbVVC0MFqj\n\
4gF12r4KSwKBgArm01TneMnlm6taLzzHrkFrCrstHUH4Hya5KCQCXgztkBnEljSQ\n\
lGpKjL2AYkbw7GeQxcww/HWCMs8Yvb7xS0Zc2TX8e8eavTWjzPAUb5DAQzcYkS96\n\
CJA30QFETJap+gRcce0aS/ASWFXIvjd3+W1hE7ROk08O7qEmfvuW0sLbAoGBAIxr\n\
LDvh5Iric7WnF+3ytGtStmhSeelnnqrFt4r5I9iEUn6RaArBnTID4zcNlhDd0hwf\n\
2gX94V2LweQ2b2rdH8QZn/7o9JbIC2mhZmlQXFNRsTQeygLQh+/va9sH9axvD2Wp\n\
/vrUYCB7SqPi5ZUe5dBNcsXibfoixOZfZ071ckyPAoGAWeP3AuUeUHeaOtJjW408\n\
/D4w5hsjFsNozvDx9IMHB668O2ArjU7G3l3Xvy0PPKViNq4Lm8Zqbi5eFzmGyTIX\n\
mE4G5NyfpexQdboNjHVeP8NeJW7EPynxxzATDYD4STTRks5xeL5r0ikX4DGnvD0Y\n\
5AAS5RyZgzJ9QM7JgdjYEgs=\n\
-----END PRIVATE KEY-----\n";

    #[test]
    fn validates_host_and_mailboxes_strictly() {
        assert_eq!(validate_smtp_host("SMTP.Example.com").unwrap(), "smtp.example.com");
        assert_eq!(validate_smtp_host("127.0.0.1").unwrap(), "127.0.0.1");
        assert!(validate_smtp_host("https://smtp.example.com").is_err());
        assert!(validate_smtp_host("smtp.example.com/path").is_err());
        assert!(validate_smtp_host("user@smtp.example.com").is_err());
        assert!(validate_smtp_host("smtp.example.com:587").is_err());

        assert!(validate_mailbox("admin@example.com", "from").is_ok());
        assert!(validate_mailbox("a@b.com\r\nBcc: x@y.com", "from").is_err());
    }

    #[tokio::test]
    async fn lettre_tls_fixture_accepts_valid_ca_and_rejects_bad_ca_or_downgrade() {
        let certs: Vec<rustls_pki_types::CertificateDer<'static>> =
            rustls_pemfile::certs(&mut TEST_SRV_CRT.as_bytes())
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        let key = rustls_pemfile::private_key(&mut TEST_SRV_KEY.as_bytes())
            .unwrap()
            .unwrap();
        let server_cfg = Arc::new(
            rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .unwrap(),
        );
        let acceptor = tokio_rustls::TlsAcceptor::from(server_cfg);

        // 1. Implicit TLS happy path with test CA
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let acceptor_clone = acceptor.clone();
        let srv_task = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let tls = acceptor_clone.accept(tcp).await.unwrap();
            let (rd, mut wr) = tokio::io::split(tls);
            let mut reader = BufReader::new(rd);
            wr.write_all(b"220 localhost ESMTP\r\n").await.unwrap();
            let mut line = String::new();
            let mut in_data = false;
            loop {
                line.clear();
                let n = reader.read_line(&mut line).await.unwrap();
                if n == 0 {
                    break;
                }
                if in_data {
                    if line == ".\r\n" {
                        in_data = false;
                        wr.write_all(b"250 2.0.0 Ok\r\n").await.unwrap();
                    }
                    continue;
                }
                let upper = line.to_ascii_uppercase();
                if upper.starts_with("EHLO") || upper.starts_with("HELO") {
                    wr.write_all(b"250-localhost\r\n250 OK\r\n").await.unwrap();
                } else if upper.starts_with("MAIL FROM:") || upper.starts_with("RCPT TO:") {
                    wr.write_all(b"250 Ok\r\n").await.unwrap();
                } else if upper.starts_with("DATA") {
                    in_data = true;
                    wr.write_all(b"354 End data\r\n").await.unwrap();
                } else if upper.starts_with("QUIT") {
                    wr.write_all(b"221 Bye\r\n").await.unwrap();
                    break;
                } else {
                    wr.write_all(b"250 Ok\r\n").await.unwrap();
                }
            }
        });

        let req = MailRequest {
            host: "localhost".into(),
            port,
            tls_mode: TlsMode::ImplicitTls,
            username: String::new(),
            password: None,
            from: "noreply@example.com".parse().unwrap(),
            to: "admin@example.com".parse().unwrap(),
            reply_to: None,
            subject: "OpenDash contact #1".into(),
            body: "Hello".into(),
        };
        let sender_ok = LettreSmtpSender::with_test_ca_pem(TEST_CA_PEM.as_bytes().to_vec());
        sender_ok.send_mail(&req).await.expect("valid TLS CA must succeed");
        let _ = srv_task.await;

        // 2. Untrusted cert rejected by default LettreSmtpSender::new()
        let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port2 = listener2.local_addr().unwrap().port();
        tokio::spawn(async move {
            if let Ok((tcp, _)) = listener2.accept().await {
                let _ = acceptor.accept(tcp).await;
            }
        });
        let mut req2 = req.clone();
        req2.port = port2;
        assert!(LettreSmtpSender::new().send_mail(&req2).await.is_err());

        // 3. STARTTLS downgrade rejected (plain server without STARTTLS capability)
        let listener3 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port3 = listener3.local_addr().unwrap().port();
        tokio::spawn(async move {
            if let Ok((mut tcp, _)) = listener3.accept().await {
                let _ = tcp.write_all(b"220 localhost ESMTP\r\n").await;
                let mut buf = [0u8; 512];
                let _ = tokio::io::AsyncReadExt::read(&mut tcp, &mut buf).await;
                let _ = tcp.write_all(b"250-localhost\r\n250 PIPELINING\r\n").await;
            }
        });
        let mut req3 = req;
        req3.port = port3;
        req3.tls_mode = TlsMode::Starttls;
        assert!(sender_ok.send_mail(&req3).await.is_err());
    }
}
