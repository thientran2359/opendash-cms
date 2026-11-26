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
    assert_eq!(
        validate_smtp_host("SMTP.Example.com").unwrap(),
        "smtp.example.com"
    );
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
    sender_ok
        .send_mail(&req)
        .await
        .expect("valid TLS CA must succeed");
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
