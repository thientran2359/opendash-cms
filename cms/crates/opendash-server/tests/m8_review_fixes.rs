use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "opendash-m8-review-{label}-{}-{id}",
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

async fn spawn_server_with_env(
    site_root: &Path,
    db_path: &Path,
    extra_env: &[(&str, &str)],
) -> ServerGuard {
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let mut cmd = Command::new(server_bin());
    cmd.env("OPENDASH_SITE_ROOT", site_root)
        .env("OPENDASH_DB_PATH", db_path)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let child = cmd.spawn().expect("spawn opendash");

    let expected_host = extra_env
        .iter()
        .find(|(k, _)| *k == "OPENDASH_PUBLIC_ORIGIN")
        .and_then(|(_, v)| {
            v.trim()
                .trim_end_matches('/')
                .strip_prefix("https://")
                .or_else(|| v.trim().trim_end_matches('/').strip_prefix("http://"))
        });

    let client = Client::new();
    for _ in 0..80 {
        let mut req = client.get(format!("{base_url}/api/health"));
        if let Some(host) = expected_host {
            req = req.header("Host", host);
        }
        if let Ok(resp) = req.send().await
            && resp.status() == StatusCode::OK
        {
            return ServerGuard { child, base_url };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("server did not become ready on {base_url}");
}

#[tokio::test]
async fn r4_proxy_origin_trailing_slash_normalized_and_invalid_origins_rejected() {
    let dir = TempDirGuard::new("r4-origin");
    let site_root = dir.path().join("site");
    let db_path = dir.path().join("cms.sqlite");
    fs::create_dir_all(&site_root).expect("site_root");

    // 1. Negative startup checks for credentials, path, query, fragment
    for bad_origin in [
        "https://user:pass@cms.example.com",
        "https://cms.example.com/subpath",
        "https://cms.example.com/?q=1",
        "https://cms.example.com/#frag",
    ] {
        let port = free_port();
        let bad_dir = dir.path().join("bad-site");
        let _ = fs::create_dir_all(&bad_dir);
        let status = Command::new(server_bin())
            .env("OPENDASH_SITE_ROOT", &bad_dir)
            .env("OPENDASH_DB_PATH", dir.path().join("bad.sqlite"))
            .env("OPENDASH_PORT", port.to_string())
            .env("OPENDASH_PUBLIC_ORIGIN", bad_origin)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("run invalid origin check");
        assert!(
            !status.success(),
            "expected startup failure for invalid OPENDASH_PUBLIC_ORIGIN={bad_origin}"
        );
    }

    // 2. Trailing slash in OPENDASH_PUBLIC_ORIGIN must normalize cleanly for both Host & Origin
    let server = spawn_server_with_env(
        &site_root,
        &db_path,
        &[("OPENDASH_PUBLIC_ORIGIN", "https://cms.example.com/")],
    )
    .await;
    let client = Client::new();

    let health_ok = client
        .get(format!("{}/api/health", server.base_url))
        .header("Host", "cms.example.com")
        .send()
        .await
        .expect("health request");
    assert_eq!(health_ok.status(), StatusCode::OK);

    let health_default_port = client
        .get(format!("{}/api/health", server.base_url))
        .header("Host", "cms.example.com:443")
        .send()
        .await
        .expect("health :443 request");
    assert_eq!(health_default_port.status(), StatusCode::OK);

    let health_forged = client
        .get(format!("{}/api/health", server.base_url))
        .header("Host", "evil.example.com")
        .send()
        .await
        .expect("forged host");
    assert_eq!(health_forged.status(), StatusCode::BAD_REQUEST);

    let setup_ok = client
        .post(format!("{}/auth/setup", server.base_url))
        .header("Host", "cms.example.com")
        .header("Origin", "https://cms.example.com")
        .json(&json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .expect("setup");
    assert_eq!(setup_ok.status(), StatusCode::CREATED);

    let login_bad_origin = client
        .post(format!("{}/auth/login", server.base_url))
        .header("Host", "cms.example.com")
        .header("Origin", "https://evil.example.com")
        .json(&json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .expect("login bad origin");
    assert_eq!(login_bad_origin.status(), StatusCode::FORBIDDEN);

    let login_ok = client
        .post(format!("{}/auth/login", server.base_url))
        .header("Host", "cms.example.com")
        .header("Origin", "https://cms.example.com")
        .json(&json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .expect("login ok");
    assert_eq!(login_ok.status(), StatusCode::OK);
    let set_cookie = login_ok
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("set-cookie")
        .to_str()
        .expect("cookie str");
    assert!(
        set_cookie.contains("Secure"),
        "expected Secure cookie flag when trusted origin is https: {set_cookie}"
    );
}

#[tokio::test]
async fn r2_cancel_restore_kills_worker_and_preserves_source_and_previous_restores() {
    let dir = TempDirGuard::new("r2-cancel");
    let site_root = dir.path().join("site");
    let db_path = dir.path().join("cms.sqlite");
    fs::create_dir_all(&site_root).expect("site_root");

    let server = spawn_server_with_env(&site_root, &db_path, &[]).await;
    let client = Client::new();

    let setup = client
        .post(format!("{}/auth/setup", server.base_url))
        .json(&json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .expect("setup");
    assert_eq!(setup.status(), StatusCode::CREATED);

    let login = client
        .post(format!("{}/auth/login", server.base_url))
        .json(&json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .expect("login");
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("set-cookie")
        .to_str()
        .expect("cookie str")
        .split(';')
        .next()
        .expect("cookie pair")
        .to_string();
    let login_json: Value = login.json().await.expect("login json");
    let csrf = login_json["csrf_token"]
        .as_str()
        .expect("csrf_token")
        .to_string();

    // Create a post on the source site to verify source remains intact
    let created_entry = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "title": "Source Sentinel",
            "slug": "source-sentinel",
            "values": {}
        }))
        .send()
        .await
        .expect("create entry");
    assert_eq!(created_entry.status(), StatusCode::CREATED);

    // Create a backup archive
    let passphrase = "Correct-Horse-Battery-Staple-2026!";
    let backup_req = client
        .post(format!("{}/api/admin/operations/backups", server.base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "passphrase": passphrase,
            "confirm_passphrase": passphrase,
            "reason": "r2-test"
        }))
        .send()
        .await
        .expect("create backup");
    assert_eq!(backup_req.status(), StatusCode::ACCEPTED);
    let backup_job: Value = backup_req.json().await.expect("backup job json");
    let backup_id = backup_job["id"].as_str().expect("backup id").to_string();

    let mut last_json = Value::Null;
    for _ in 0..300 {
        let st = client
            .get(format!(
                "{}/api/admin/operations/jobs/{backup_id}",
                server.base_url
            ))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .expect("poll backup");
        let st_json: Value = st.json().await.expect("poll json");
        if st_json["state"] == "succeeded" {
            last_json = st_json;
            break;
        }
        if st_json["state"] == "failed" || st_json["state"] == "cancelled" {
            panic!("backup job ended unexpectedly: {st_json}");
        }
        last_json = st_json;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        last_json["state"], "succeeded",
        "backup job did not succeed in time: {last_json}"
    );

    let archive_bytes = client
        .get(format!(
            "{}/api/admin/operations/backups/{backup_id}/file",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("download backup")
        .bytes()
        .await
        .expect("archive bytes");
    assert!(!archive_bytes.is_empty());

    // Preserve a previous completed restore directory sentinel
    let prev_restore_dir = site_root
        .join("backups")
        .join("restored")
        .join("previous-ok");
    fs::create_dir_all(&prev_restore_dir).expect("prev restore dir");
    fs::write(
        prev_restore_dir.join("READY.json"),
        b"{\"job_id\":\"previous-ok\"}",
    )
    .expect("prev ready");

    // Upload restore archive
    let upload_resp = client
        .post(format!(
            "{}/api/admin/operations/restore-uploads",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(archive_bytes)
        .send()
        .await
        .expect("upload restore");
    assert_eq!(upload_resp.status(), StatusCode::CREATED);
    let upload_json: Value = upload_resp.json().await.expect("upload json");
    let upload_id = upload_json["upload_id"].as_str().expect("upload_id");

    // Start restore and immediately cancel it
    let restore_resp = client
        .post(format!("{}/api/admin/operations/restores", server.base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "upload_id": upload_id, "passphrase": passphrase }))
        .send()
        .await
        .expect("create restore");
    assert_eq!(restore_resp.status(), StatusCode::ACCEPTED);
    let restore_job: Value = restore_resp.json().await.expect("restore job json");
    let restore_id = restore_job["id"].as_str().expect("restore id").to_string();

    let cancel_started = Instant::now();
    let cancel_resp = client
        .post(format!(
            "{}/api/admin/operations/jobs/{restore_id}/cancel",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .expect("cancel restore");
    assert_eq!(cancel_resp.status(), StatusCode::OK);
    let cancel_json: Value = cancel_resp.json().await.expect("cancel json");
    assert_eq!(cancel_json["state"], "cancelled");
    assert!(
        cancel_started.elapsed() < Duration::from_secs(10),
        "cancel took too long: {:?}",
        cancel_started.elapsed()
    );

    // Cancelled restore must not leave READY.json behind
    assert!(
        !site_root
            .join("backups")
            .join("restored")
            .join(&restore_id)
            .join("READY.json")
            .exists()
    );
    // Previous completed restore & source entry remain intact
    assert!(prev_restore_dir.join("READY.json").exists());
    let entries_resp = client
        .get(format!(
            "{}/api/admin/content-types/post/entries",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("list entries");
    assert_eq!(entries_resp.status(), StatusCode::OK);
    let entries_json: Value = entries_resp.json().await.expect("entries json");
    let entries_text = serde_json::to_string(&entries_json).expect("entries text");
    assert!(entries_text.contains("source-sentinel"));
}
