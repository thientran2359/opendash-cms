use std::{
    fs,
    io::Write,
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
            "opendash-m8-s04-t02-{label}-{}-{id}",
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

async fn login_session(client: &Client, base_url: &str, username: &str, password: &str) -> (String, String) {
    let login_resp = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({
            "username": username,
            "password": password
        }))
        .send()
        .await
        .expect("login");
    assert_eq!(login_resp.status(), StatusCode::OK);
    let cookie = login_resp
        .headers()
        .get("set-cookie")
        .expect("set-cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let body: Value = login_resp.json().await.expect("login json");
    let csrf = body["csrf_token"]
        .as_str()
        .expect("csrf_token")
        .to_string();
    (cookie, csrf)
}

async fn setup_and_login_admin(client: &Client, base_url: &str) -> (String, String) {
    let setup_resp = client
        .post(format!("{base_url}/auth/setup"))
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword12345!"
        }))
        .send()
        .await
        .expect("setup admin");
    assert_eq!(setup_resp.status(), StatusCode::CREATED);
    login_session(client, base_url, "admin", "AdminPassword12345!").await
}

#[tokio::test]
async fn m8_s04_t02_admin_backup_jobs_download_cancel_and_retention() {
    let tmp = TempDirGuard::new("admin-backup");
    let site_root = tmp.path().join("site");
    let db_path = site_root.join("database").join("opendash.sqlite");
    fs::create_dir_all(db_path.parent().unwrap()).expect("mkdir db");

    let server = spawn_server(&site_root, &db_path).await;
    let base_url = server.base_url.clone();

    let client = Client::new();

    // 1. Unauthenticated access -> 401
    let unauth_list = client
        .get(format!("{base_url}/api/admin/operations/backups"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_list.status(), StatusCode::UNAUTHORIZED);

    let (admin_cookie, csrf) = setup_and_login_admin(&client, &base_url).await;

    // 2. Create editor and verify Editor -> 403
    let create_ed = client
        .post(format!("{base_url}/api/admin/users"))
        .header("Cookie", &admin_cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword12345!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_ed.status(), StatusCode::CREATED);
    let (ed_cookie, ed_csrf) =
        login_session(&client, &base_url, "editor1", "EditorPassword12345!").await;

    let passphrase = "admin-backup-passphrase-2026!";
    let req_id_1 = "11".repeat(32);

    let ed_backup = client
        .post(format!("{base_url}/api/admin/operations/backups"))
        .header("Cookie", &ed_cookie)
        .header("X-CSRF-Token", &ed_csrf)
        .json(&json!({
            "passphrase": passphrase,
            "confirm_passphrase": passphrase,
            "request_id": req_id_1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_backup.status(), StatusCode::FORBIDDEN);

    // 3. Missing CSRF -> 403
    let no_csrf = client
        .post(format!("{base_url}/api/admin/operations/backups"))
        .header("Cookie", &admin_cookie)
        .json(&json!({
            "passphrase": passphrase,
            "confirm_passphrase": passphrase,
            "request_id": req_id_1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(no_csrf.status(), StatusCode::FORBIDDEN);

    // 4. Create backup -> 202 Accepted
    let start_resp = client
        .post(format!("{base_url}/api/admin/operations/backups"))
        .header("Cookie", &admin_cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "passphrase": passphrase,
            "confirm_passphrase": passphrase,
            "request_id": req_id_1,
            "reason": "initial-backup"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(start_resp.status(), StatusCode::ACCEPTED);
    let job_1: Value = start_resp.json().await.unwrap();
    let job_id = job_1["id"].as_str().unwrap().to_string();

    // 5. Repeat same request_id + same nonsecret payload -> returns same job_id
    let dup_resp = client
        .post(format!("{base_url}/api/admin/operations/backups"))
        .header("Cookie", &admin_cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "passphrase": passphrase,
            "confirm_passphrase": passphrase,
            "request_id": req_id_1,
            "reason": "initial-backup"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(dup_resp.status(), StatusCode::ACCEPTED);
    let dup_job: Value = dup_resp.json().await.unwrap();
    assert_eq!(dup_job["id"].as_str().unwrap(), job_id);

    // 6. Repeat same request_id + changed nonsecret payload -> 409 Conflict
    let conflict_resp = client
        .post(format!("{base_url}/api/admin/operations/backups"))
        .header("Cookie", &admin_cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "passphrase": passphrase,
            "confirm_passphrase": passphrase,
            "request_id": req_id_1,
            "reason": "changed-reason"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(conflict_resp.status(), StatusCode::CONFLICT);

    // 7. Poll until succeeded
    let mut final_job = Value::Null;
    for _ in 0..180 {
        let j: Value = client
            .get(format!("{base_url}/api/admin/operations/jobs/{job_id}"))
            .header("Cookie", &admin_cookie)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let st = j["state"].as_str().unwrap();
        if st == "succeeded" || st == "failed" || st == "cancelled" {
            final_job = j;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert_eq!(
        final_job["state"].as_str().unwrap(),
        "succeeded",
        "backup job failed: {final_job}"
    );
    let job_dump = final_job.to_string();
    assert!(
        !job_dump.contains(passphrase),
        "passphrase must never appear in job receipt"
    );

    // 8. List backups reports max_artifacts=10, max_receipts=200
    let list_json: Value = client
        .get(format!("{base_url}/api/admin/operations/backups?limit=20"))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list_json["max_artifacts"], 10);
    assert_eq!(list_json["max_receipts"], 200);
    assert_eq!(list_json["active_artifact_count"], 1);
    assert_eq!(list_json["artifacts"][0]["id"], job_id);

    // 9. HEAD + GET encrypted file (Range ignored, private/no-store/nosniff)
    let head_resp = client
        .head(format!(
            "{base_url}/api/admin/operations/backups/{job_id}/file"
        ))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(head_resp.status(), StatusCode::OK);
    assert_eq!(
        head_resp.headers()["cache-control"].to_str().unwrap(),
        "private, no-store"
    );
    assert_eq!(
        head_resp.headers()["x-content-type-options"]
            .to_str()
            .unwrap(),
        "nosniff"
    );

    let dl_resp = client
        .get(format!(
            "{base_url}/api/admin/operations/backups/{job_id}/file"
        ))
        .header("Cookie", &admin_cookie)
        .header("Range", "bytes=0-15")
        .send()
        .await
        .unwrap();
    assert_eq!(dl_resp.status(), StatusCode::OK);
    let dl_bytes = dl_resp.bytes().await.unwrap();
    assert!(dl_bytes.starts_with(b"age-encryption.org/v1\n"));

    // Verify downloaded encrypted artifact with isolated verify worker
    let downloaded_path = tmp.path().join("downloaded.odbackup.age");
    fs::write(&downloaded_path, &dl_bytes).unwrap();
    let verify_dir = tmp.path().join("verify-dl");
    let mut child = Command::new(server_bin())
        .arg("--operations-worker")
        .arg("verify")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        let req = serde_json::to_string(&json!({
            "input_path": downloaded_path,
            "private_dir": verify_dir,
            "passphrase": passphrase
        }))
        .unwrap();
        stdin.write_all(req.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
        let out = child.wait_with_output().unwrap();
        drop(stdin);
        assert!(out.status.success());
        let ver: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(ver["ok"], true);
    }

    // 10. Delete artifact with CSRF -> 200, then GET .../file -> 404
    let del_resp = client
        .delete(format!("{base_url}/api/admin/operations/backups/{job_id}"))
        .header("Cookie", &admin_cookie)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(del_resp.status(), StatusCode::OK);
    let after_del = client
        .get(format!(
            "{base_url}/api/admin/operations/backups/{job_id}/file"
        ))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(after_del.status(), StatusCode::NOT_FOUND);

    // 11. Crash/restart marks running job as interrupted (no auto-retry)
    drop(server);
    let opts = SqliteConnectOptions::new().filename(&db_path);
    let mut conn = SqliteConnection::connect_with(&opts).await.unwrap();
    let stale_id = "22".repeat(32);
    sqlx::query(
        "INSERT INTO operation_jobs (id, kind, state, version) VALUES (?, 'backup', 'running', 1)",
    )
    .bind(&stale_id)
    .execute(&mut conn)
    .await
    .unwrap();
    conn.close().await.unwrap();

    let server2 = spawn_server(&site_root, &db_path).await;
    let (admin_cookie2, _) =
        login_session(&client, &server2.base_url, "admin", "AdminPassword12345!").await;
    let stale_job: Value = client
        .get(format!(
            "{}/api/admin/operations/jobs/{stale_id}",
            server2.base_url
        ))
        .header("Cookie", &admin_cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(stale_job["state"], "interrupted");
}
