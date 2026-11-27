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
use sqlx::{Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "opendash-m8-s05-t02-{label}-{}-{id}",
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

async fn login_session(
    client: &Client,
    base_url: &str,
    username: &str,
    password: &str,
) -> (String, String) {
    let login_resp = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({
            "username": username,
            "password": password
        }))
        .send()
        .await
        .expect("login request");
    assert_eq!(login_resp.status(), StatusCode::OK);
    let cookie_header = login_resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|s| s.split(';').next().map(|kv| kv.to_string()))
        .expect("session cookie");
    let body: Value = login_resp.json().await.expect("login json");
    let csrf = body["csrf_token"].as_str().expect("csrf_token").to_string();
    (cookie_header, csrf)
}

async fn poll_job_until_terminal(
    client: &Client,
    base_url: &str,
    cookie: &str,
    job_id: &str,
) -> Value {
    for _ in 0..150 {
        let resp = client
            .get(format!("{base_url}/api/admin/operations/jobs/{job_id}"))
            .header(reqwest::header::COOKIE, cookie)
            .send()
            .await
            .expect("poll job");
        assert_eq!(resp.status(), StatusCode::OK);
        let job: Value = resp.json().await.expect("job json");
        let state = job["state"].as_str().unwrap_or("");
        if matches!(state, "succeeded" | "failed" | "cancelled") {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("job {job_id} did not reach terminal state");
}

#[tokio::test]
async fn admin_restore_safe_mode_and_operator_activation_flow() {
    let tmp = TempDirGuard::new("flow");
    let source_root = tmp.path().join("source-site");
    let source_db = source_root.join("database").join("opendash.sqlite");
    fs::create_dir_all(source_root.join("database")).expect("db dir");

    let srv = spawn_server(&source_root, &source_db).await;
    let client = Client::new();
    let setup_resp = client
        .post(format!("{}/auth/setup", srv.base_url))
        .json(&json!({
            "username": "admin",
            "password": "admin-password-123"
        }))
        .send()
        .await
        .expect("setup admin");
    assert_eq!(setup_resp.status(), StatusCode::CREATED);
    let (cookie, csrf) = login_session(&client, &srv.base_url, "admin", "admin-password-123").await;

    // Create a contact form on source so we can test contact 503 during safe mode on restored root.
    let form_resp = client
        .post(format!("{}/api/admin/contact/forms", srv.base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "lien-he-restore",
            "title": "Liên hệ",
            "status": "active",
            "schema": {
                "version": 1,
                "fields": [
                    { "key": "name", "label": "Tên", "kind": "text", "required": true }
                ]
            }
        }))
        .send()
        .await
        .expect("create form");
    assert_eq!(form_resp.status(), StatusCode::CREATED);
    let form_json: Value = form_resp.json().await.expect("form json");
    let form_id = form_json["id"].as_i64().expect("form id");

    // 1. Create backup on source
    let passphrase = "restore-test-passphrase-1234";
    let backup_resp = client
        .post(format!("{}/api/admin/operations/backups", srv.base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "passphrase": passphrase,
            "confirm_passphrase": passphrase,
            "reason": "m8-s05-t02 backup"
        }))
        .send()
        .await
        .expect("create backup");
    assert_eq!(backup_resp.status(), StatusCode::ACCEPTED);
    let backup_job: Value = backup_resp.json().await.expect("backup job json");
    let backup_job_id = backup_job["id"].as_str().expect("backup job id");
    let finished_backup =
        poll_job_until_terminal(&client, &srv.base_url, &cookie, backup_job_id).await;
    assert_eq!(finished_backup["state"], "succeeded");

    // 2. Download encrypted archive
    let dl_resp = client
        .get(format!(
            "{}/api/admin/operations/backups/{backup_job_id}/file",
            srv.base_url
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("download backup");
    assert_eq!(dl_resp.status(), StatusCode::OK);
    let archive_bytes = dl_resp.bytes().await.expect("archive bytes").to_vec();
    assert!(!archive_bytes.is_empty());

    // 3. Upload archive via streaming restore-uploads
    let upload_resp = client
        .post(format!(
            "{}/api/admin/operations/restore-uploads",
            srv.base_url
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

    // 4. Launch restore job into separate root
    let restore_resp = client
        .post(format!("{}/api/admin/operations/restores", srv.base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "upload_id": upload_id,
            "passphrase": passphrase,
            "reason": "m8-s05-t02 restore"
        }))
        .send()
        .await
        .expect("create restore");
    assert_eq!(restore_resp.status(), StatusCode::ACCEPTED);
    let restore_job: Value = restore_resp.json().await.expect("restore job json");
    let restore_job_id = restore_job["id"].as_str().expect("restore job id");
    let finished_restore =
        poll_job_until_terminal(&client, &srv.base_url, &cookie, restore_job_id).await;
    assert_eq!(
        finished_restore["state"], "succeeded",
        "restore job failed: {finished_restore}"
    );
    let summary: Value = serde_json::from_str(
        finished_restore["summary_json"]
            .as_str()
            .expect("summary_json"),
    )
    .expect("parse summary_json");
    let restored_root = PathBuf::from(summary["target_root"].as_str().expect("target_root"));
    assert!(restored_root.join("READY.json").is_file());
    assert!(!source_root.join("READY.json").exists());

    // Stop source server and boot restored root as separate server
    drop(srv);
    let restored_db = restored_root.join("database").join("opendash.sqlite");
    let restored_srv = spawn_server(&restored_root, &restored_db).await;

    // 5. Safe mode checks on restored server:
    // Public page has X-Robots-Tag: noindex, nofollow
    let pub_resp = client
        .get(format!("{}/", restored_srv.base_url))
        .send()
        .await
        .expect("public get");
    assert_eq!(pub_resp.status(), StatusCode::OK);
    assert_eq!(
        pub_resp
            .headers()
            .get("x-robots-tag")
            .and_then(|v| v.to_str().ok()),
        Some("noindex, nofollow")
    );

    // Anonymous contact submit returns 503
    let contact_resp = client
        .post(format!(
            "{}/api/public/contact/{form_id}",
            restored_srv.base_url
        ))
        .header(reqwest::header::ORIGIN, &restored_srv.base_url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body("form_version=1&nonce=abc&name=Test")
        .send()
        .await
        .expect("public contact in safe mode");
    assert_eq!(contact_resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    // Login with known Admin credentials works
    let (r_cookie, r_csrf) = login_session(
        &client,
        &restored_srv.base_url,
        "admin",
        "admin-password-123",
    )
    .await;

    // Recovery status reports safe_mode = true, smtp_enabled = false
    let status_resp = client
        .get(format!(
            "{}/api/admin/operations/recovery/status",
            restored_srv.base_url
        ))
        .header(reqwest::header::COOKIE, &r_cookie)
        .send()
        .await
        .expect("recovery status");
    assert_eq!(status_resp.status(), StatusCode::OK);
    let status_json: Value = status_resp.json().await.expect("status json");
    assert_eq!(status_json["safe_mode"], true);
    assert_eq!(status_json["smtp_enabled"], false);
    let generation = status_json["expected_generation"]
        .as_str()
        .expect("expected_generation")
        .to_string();

    // SMTP test blocked in safe mode
    let smtp_test_resp = client
        .post(format!(
            "{}/api/admin/settings/smtp/test",
            restored_srv.base_url
        ))
        .header(reqwest::header::COOKIE, &r_cookie)
        .header("X-CSRF-Token", &r_csrf)
        .send()
        .await
        .expect("smtp test in safe mode");
    assert_eq!(smtp_test_resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    // Activate fails on wrong password
    let bad_pw = client
        .post(format!(
            "{}/api/admin/operations/recovery/activate",
            restored_srv.base_url
        ))
        .header(reqwest::header::COOKIE, &r_cookie)
        .header("X-CSRF-Token", &r_csrf)
        .json(&json!({
            "password": "wrong-password",
            "expected_generation": generation
        }))
        .send()
        .await
        .expect("activate wrong pw");
    assert_eq!(bad_pw.status(), StatusCode::UNAUTHORIZED);

    // Activate fails on wrong generation (CAS)
    let bad_gen = client
        .post(format!("{}/api/admin/operations/recovery/activate", restored_srv.base_url))
        .header(reqwest::header::COOKIE, &r_cookie)
        .header("X-CSRF-Token", &r_csrf)
        .json(&json!({
            "password": "admin-password-123",
            "expected_generation": "0000000000000000000000000000000000000000000000000000000000000000"
        }))
        .send()
        .await
        .expect("activate wrong gen");
    assert_eq!(bad_gen.status(), StatusCode::PRECONDITION_FAILED);

    // Activate succeeds with valid password and generation
    let ok_act = client
        .post(format!(
            "{}/api/admin/operations/recovery/activate",
            restored_srv.base_url
        ))
        .header(reqwest::header::COOKIE, &r_cookie)
        .header("X-CSRF-Token", &r_csrf)
        .json(&json!({
            "password": "admin-password-123",
            "expected_generation": generation
        }))
        .send()
        .await
        .expect("activate ok");
    assert_eq!(ok_act.status(), StatusCode::OK);
    let act_json: Value = ok_act.json().await.expect("activate json");
    assert_eq!(act_json["activated"], true);
    assert_eq!(act_json["safe_mode"], false);
    assert_eq!(act_json["smtp_enabled"], false);

    // After activation: public route no longer has X-Robots-Tag: noindex, nofollow
    let pub_after = client
        .get(format!("{}/", restored_srv.base_url))
        .send()
        .await
        .expect("public after activate");
    assert_eq!(pub_after.status(), StatusCode::OK);
    assert!(pub_after.headers().get("x-robots-tag").is_none());

    // Source DB still intact
    let mut conn = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&source_db)
            .read_only(true),
    )
    .await
    .expect("open source db");
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM contact_forms WHERE slug = 'lien-he-restore'")
            .fetch_one(&mut conn)
            .await
            .expect("query source");
    assert_eq!(count.0, 1);
}
