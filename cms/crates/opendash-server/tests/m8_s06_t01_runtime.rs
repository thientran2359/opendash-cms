use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "opendash-m8-s06-t01-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct ServerGuard {
    child: Child,
    base_url: String,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind port");
    listener.local_addr().expect("local_addr").port()
}

fn server_bin() -> &'static str {
    env!("CARGO_BIN_EXE_opendash")
}

async fn spawn_server(site_root: &Path, db_path: &Path) -> ServerGuard {
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let child = Command::new(server_bin())
        .env("OPENDASH_SITE_ROOT", site_root)
        .env("OPENDASH_DB_PATH", db_path)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn opendash");

    let client = Client::new();
    for _ in 0..80 {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await
            && resp.status() == StatusCode::OK
        {
            return ServerGuard { child, base_url };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("server did not become ready on {base_url}");
}

#[test]
fn check_config_cli_validates_without_side_effects_and_rejects_nonloopback() {
    let tmp = TempDirGuard::new("check-config");
    let site_root = tmp.path().join("site");
    let db_path = site_root.join("database").join("opendash.sqlite");
    fs::create_dir_all(site_root.join("config")).expect("config dir");
    fs::write(
        site_root.join("config").join("site.json"),
        serde_json::to_vec_pretty(&json!({
            "version": 1,
            "listen_ip": "127.0.0.1",
            "port": 18766,
            "trusted_public_origin": "https://cms.example.com",
            "log_level": "info",
            "recovery_safe_mode": false
        }))
        .unwrap(),
    )
    .expect("write site.json");

    let out = Command::new(server_bin())
        .arg("--check-config")
        .env("OPENDASH_SITE_ROOT", &site_root)
        .env("OPENDASH_DB_PATH", &db_path)
        .output()
        .expect("run --check-config");
    assert!(out.status.success(), "check-config failed: {:?}", out);
    assert!(!db_path.exists(), "--check-config must not create DB");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let summary: Value = serde_json::from_str(stdout.trim()).expect("summary json");
    assert_eq!(summary["listen_ip"], "127.0.0.1");
    assert_eq!(summary["trusted_public_origin"], "https://cms.example.com");

    // Non-loopback listen_ip must be rejected
    fs::write(
        site_root.join("config").join("site.json"),
        serde_json::to_vec_pretty(&json!({
            "version": 1,
            "listen_ip": "0.0.0.0",
            "port": 18766,
            "trusted_public_origin": null,
            "log_level": "info",
            "recovery_safe_mode": false
        }))
        .unwrap(),
    )
    .expect("write bad site.json");

    let bad = Command::new(server_bin())
        .arg("--check-config")
        .env("OPENDASH_SITE_ROOT", &site_root)
        .env("OPENDASH_DB_PATH", &db_path)
        .output()
        .expect("run --check-config bad");
    assert!(!bad.status.success(), "non-loopback listen_ip must fail");
}

#[tokio::test]
async fn proxy_authority_secure_cookies_and_mandatory_origin() {
    let tmp = TempDirGuard::new("proxy");
    let site_root = tmp.path().join("site");
    let db_path = site_root.join("database").join("opendash.sqlite");
    fs::create_dir_all(site_root.join("config")).expect("config dir");
    fs::create_dir_all(site_root.join("database")).expect("db dir");
    fs::write(
        site_root.join("config").join("site.json"),
        serde_json::to_vec_pretty(&json!({
            "version": 1,
            "listen_ip": "127.0.0.1",
            "port": 18766,
            "trusted_public_origin": "https://cms.example.com",
            "log_level": "info",
            "recovery_safe_mode": false
        }))
        .unwrap(),
    )
    .expect("write site.json");

    let srv = spawn_server(&site_root, &db_path).await;
    let client = Client::new();

    // Spoofed Host rejected even with X-Forwarded-Host
    let spoof = client
        .get(format!("{}/api/health", srv.base_url))
        .header(reqwest::header::HOST, "evil.example.com")
        .header("X-Forwarded-Host", "cms.example.com")
        .send()
        .await
        .expect("spoofed host");
    assert_eq!(spoof.status(), StatusCode::BAD_REQUEST);

    // Configured public Host (and normalized :443) allowed
    let legit_host = client
        .get(format!("{}/api/health", srv.base_url))
        .header(reqwest::header::HOST, "cms.example.com:443")
        .send()
        .await
        .expect("legit host");
    assert_eq!(legit_host.status(), StatusCode::OK);

    // Setup without Origin must be rejected (no Referer loophole)
    let no_origin_setup = client
        .post(format!("{}/auth/setup", srv.base_url))
        .header(reqwest::header::HOST, "cms.example.com")
        .header(reqwest::header::REFERER, "https://cms.example.com/builder")
        .json(&json!({ "username": "admin", "password": "admin-password-123" }))
        .send()
        .await
        .expect("no origin setup");
    assert_eq!(no_origin_setup.status(), StatusCode::FORBIDDEN);

    // Setup with exact trusted Origin succeeds
    let setup_ok = client
        .post(format!("{}/auth/setup", srv.base_url))
        .header(reqwest::header::HOST, "cms.example.com")
        .header(reqwest::header::ORIGIN, "https://cms.example.com")
        .json(&json!({ "username": "admin", "password": "admin-password-123" }))
        .send()
        .await
        .expect("setup ok");
    assert_eq!(setup_ok.status(), StatusCode::CREATED);

    // Login sets Secure + HttpOnly + SameSite=Lax cookie
    let login_resp = client
        .post(format!("{}/auth/login", srv.base_url))
        .header(reqwest::header::HOST, "cms.example.com")
        .header(reqwest::header::ORIGIN, "https://cms.example.com")
        .json(&json!({ "username": "admin", "password": "admin-password-123" }))
        .send()
        .await
        .expect("login ok");
    assert_eq!(login_resp.status(), StatusCode::OK);
    let set_cookie = login_resp
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .expect("set-cookie");
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Lax"));
    assert!(set_cookie.contains("; Secure"), "expected Secure flag: {set_cookie}");

    // Null Origin rejected
    let null_origin = client
        .post(format!("{}/auth/login", srv.base_url))
        .header(reqwest::header::HOST, "cms.example.com")
        .header(reqwest::header::ORIGIN, "null")
        .json(&json!({ "username": "admin", "password": "admin-password-123" }))
        .send()
        .await
        .expect("null origin");
    assert_eq!(null_origin.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn local_loopback_mode_omits_secure_cookie_and_reports_clean_health() {
    let tmp = TempDirGuard::new("local");
    let site_root = tmp.path().join("site");
    let db_path = site_root.join("database").join("opendash.sqlite");
    fs::create_dir_all(site_root.join("database")).expect("db dir");

    let srv = spawn_server(&site_root, &db_path).await;
    let client = Client::new();

    let health: Value = client
        .get(format!("{}/api/health", srv.base_url))
        .send()
        .await
        .expect("health")
        .json()
        .await
        .expect("health json");
    assert_eq!(health["status"], "ok");
    assert_eq!(health["safe_mode"], false);

    let setup_resp = client
        .post(format!("{}/auth/setup", srv.base_url))
        .json(&json!({ "username": "admin", "password": "admin-password-123" }))
        .send()
        .await
        .expect("setup");
    assert_eq!(setup_resp.status(), StatusCode::CREATED);

    let login_resp = client
        .post(format!("{}/auth/login", srv.base_url))
        .json(&json!({ "username": "admin", "password": "admin-password-123" }))
        .send()
        .await
        .expect("login");
    assert_eq!(login_resp.status(), StatusCode::OK);
    let set_cookie = login_resp
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .expect("set-cookie");
    assert!(!set_cookie.contains("; Secure"));
}
