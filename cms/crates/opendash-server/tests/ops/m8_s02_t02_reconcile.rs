use std::{
    fs::{self, File},
    io::Cursor,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "opendash-m8-s02-t02-{label}-{}-{id}",
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

struct RunningServer {
    child: Option<Child>,
    base_url: String,
    site_root: PathBuf,
}

impl RunningServer {
    async fn start(site_root: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);
        Self::start_on_port(site_root, port).await
    }

    async fn start_on_port(site_root: &Path, port: u16) -> Self {
        let bin = env!("CARGO_BIN_EXE_opendash");
        let child = Command::new(bin)
            .env("OPENDASH_SITE_ROOT", site_root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn opendash server");

        let base_url = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..80 {
            if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await {
                if resp.status() == StatusCode::OK {
                    return Self {
                        child: Some(child),
                        base_url,
                        site_root: site_root.to_path_buf(),
                    };
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("server failed to become ready at {base_url}");
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop();
        let _ = self.site_root.exists();
    }
}

fn tiny_png(r: u8) -> Vec<u8> {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([r, 120, 180, 255])));
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .expect("encode png");
    out
}

async fn setup_admin_and_editor(
    client: &Client,
    base_url: &str,
) -> (String, String, String, String) {
    let setup = client
        .post(format!("{base_url}/auth/setup"))
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .expect("setup admin");
    assert_eq!(setup.status(), StatusCode::CREATED);

    let login_admin = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .expect("login admin");
    assert_eq!(login_admin.status(), StatusCode::OK);
    let admin_cookie = login_admin
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or("").to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let admin_json: Value = login_admin.json().await.expect("admin json");
    let admin_csrf = admin_json["csrf_token"]
        .as_str()
        .expect("admin csrf")
        .to_string();

    let create_editor = client
        .post(format!("{base_url}/api/admin/users"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword123!",
            "role": "editor"
        }))
        .send()
        .await
        .expect("create editor");
    assert_eq!(create_editor.status(), StatusCode::CREATED);

    let login_editor = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword123!"
        }))
        .send()
        .await
        .expect("login editor");
    assert_eq!(login_editor.status(), StatusCode::OK);
    let editor_cookie = login_editor
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or("").to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let editor_json: Value = login_editor.json().await.expect("editor json");
    let editor_csrf = editor_json["csrf_token"]
        .as_str()
        .expect("editor csrf")
        .to_string();

    (admin_cookie, admin_csrf, editor_cookie, editor_csrf)
}

#[tokio::test]
async fn reconcile_grace_periods_412_identity_checks_unknown_file_fail_closed_and_redacted_purge() {
    let dir = TempDirGuard::new("main");
    let mut server = RunningServer::start(dir.path()).await;
    let client = Client::new();
    let (admin_cookie, admin_csrf, editor_cookie, editor_csrf) =
        setup_admin_and_editor(&client, &server.base_url).await;

    // 1. Editor RBAC => 403, unauthenticated => 401
    let unauth_diag = client
        .get(format!(
            "{}/api/admin/operations/diagnostics",
            server.base_url
        ))
        .send()
        .await
        .expect("unauth diag");
    assert_eq!(unauth_diag.status(), StatusCode::UNAUTHORIZED);

    let editor_diag = client
        .get(format!(
            "{}/api/admin/operations/diagnostics",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &editor_cookie)
        .send()
        .await
        .expect("editor diag");
    assert_eq!(editor_diag.status(), StatusCode::FORBIDDEN);

    let editor_plan = client
        .post(format!(
            "{}/api/admin/operations/reconcile/plan",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &editor_cookie)
        .header("X-CSRF-Token", &editor_csrf)
        .send()
        .await
        .expect("editor plan");
    assert_eq!(editor_plan.status(), StatusCode::FORBIDDEN);

    // 2. Upload an image, then delete its physical file to verify missing_canonical_files keeps DB row
    let up = client
        .post(format!("{}/api/admin/media", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("X-File-Name", "missing-test.png")
        .header(reqwest::header::CONTENT_TYPE, "image/png")
        .body(tiny_png(11))
        .send()
        .await
        .expect("upload png");
    assert_eq!(up.status(), StatusCode::CREATED);
    let up_json: Value = up.json().await.expect("up json");
    let missing_id = up_json["id"].as_str().expect("media id").to_string();
    let missing_shard = &missing_id[..2];
    let missing_orig = dir
        .path()
        .join("uploads")
        .join("media")
        .join(missing_shard)
        .join(format!("{missing_id}.png"));
    fs::remove_file(&missing_orig).expect("remove physical file to simulate missing");

    // 3. Create one young orphan (<24h) and one aged orphan (>=24h)
    let young_id = format!("{:064x}", 0x1111u64);
    let aged_id = format!("{:064x}", 0x2222u64);
    let young_path = dir
        .path()
        .join("uploads")
        .join("media")
        .join(&young_id[..2])
        .join(format!("{young_id}.png"));
    let aged_path = dir
        .path()
        .join("uploads")
        .join("media")
        .join(&aged_id[..2])
        .join(format!("{aged_id}.png"));
    fs::create_dir_all(young_path.parent().unwrap()).expect("create young shard");
    fs::create_dir_all(aged_path.parent().unwrap()).expect("create aged shard");
    fs::write(&young_path, tiny_png(22)).expect("write young orphan");
    fs::write(&aged_path, tiny_png(33)).expect("write aged orphan");
    {
        let f = File::options()
            .write(true)
            .open(&aged_path)
            .expect("open aged");
        let old_time = SystemTime::now() - Duration::from_secs(26 * 3600);
        f.set_modified(old_time).expect("set aged mtime >= 24h");
    }

    // 4. Plan reconcile -> reports missing_canonical_files (DB kept!), young orphan skipped, aged orphan candidate
    let plan_resp = client
        .post(format!(
            "{}/api/admin/operations/reconcile/plan",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .send()
        .await
        .expect("plan reconcile");
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan: Value = plan_resp.json().await.expect("plan json");
    assert_eq!(plan["recent_orphans_under_grace"].as_u64(), Some(1));
    assert_eq!(
        plan["orphan_candidates"].as_array().map(|a| a.len()),
        Some(1)
    );
    assert_eq!(
        plan["orphan_candidates"][0]["media_id"].as_str(),
        Some(aged_id.as_str())
    );
    assert!(
        plan["missing_canonical_files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["media_id"].as_str() == Some(missing_id.as_str()))
    );

    // Missing file's DB metadata must still be intact
    let missing_get = client
        .get(format!("{}/api/admin/media/{missing_id}", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .send()
        .await
        .expect("get missing media metadata");
    assert_eq!(missing_get.status(), StatusCode::OK);

    // 5. Mutate aged orphan bytes after plan -> apply must reject 412!
    fs::write(&aged_path, tiny_png(44)).expect("mutate aged orphan");
    {
        let f = File::options()
            .write(true)
            .open(&aged_path)
            .expect("reopen aged");
        f.set_modified(SystemTime::now() - Duration::from_secs(26 * 3600))
            .expect("restore aged mtime");
    }
    let stale_apply = client
        .post(format!(
            "{}/api/admin/operations/reconcile/apply",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "plan_id": plan["plan_id"],
            "expected_generation": plan["generation"],
            "expected_version": plan["version"],
            "digest": plan["digest"],
            "confirm": true
        }))
        .send()
        .await
        .expect("stale apply");
    assert_eq!(stale_apply.status(), StatusCode::PRECONDITION_FAILED);

    // 6. Re-plan and apply -> moves aged orphan to quarantine, leaves young orphan untouched
    let plan2_resp = client
        .post(format!(
            "{}/api/admin/operations/reconcile/plan",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .send()
        .await
        .expect("plan2");
    assert_eq!(plan2_resp.status(), StatusCode::OK);
    let plan2: Value = plan2_resp.json().await.expect("plan2 json");

    let apply_resp = client
        .post(format!(
            "{}/api/admin/operations/reconcile/apply",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "plan_id": plan2["plan_id"],
            "expected_generation": plan2["generation"],
            "expected_version": plan2["version"],
            "digest": plan2["digest"],
            "confirm": true
        }))
        .send()
        .await
        .expect("apply2");
    assert_eq!(apply_resp.status(), StatusCode::OK);
    let apply_json: Value = apply_resp.json().await.expect("apply json");
    assert_eq!(apply_json["quarantined_files"].as_u64(), Some(1));
    assert!(
        !aged_path.exists(),
        "aged orphan must be moved to quarantine"
    );
    assert!(
        young_path.exists(),
        "young orphan <24h must remain untouched"
    );

    // Re-applying old plan2 after generation bumped must return 412
    let replay_apply = client
        .post(format!(
            "{}/api/admin/operations/reconcile/apply",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "plan_id": plan2["plan_id"],
            "expected_generation": plan2["generation"],
            "expected_version": plan2["version"],
            "digest": plan2["digest"],
            "confirm": true
        }))
        .send()
        .await
        .expect("replay apply");
    assert_eq!(replay_apply.status(), StatusCode::PRECONDITION_FAILED);

    // 7. Unknown file in uploads fails plan closed without deleting it
    let stray = dir.path().join("uploads").join("stray-unknown.txt");
    fs::write(&stray, b"stray").expect("write stray");
    let fail_plan = client
        .post(format!(
            "{}/api/admin/operations/reconcile/plan",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .send()
        .await
        .expect("fail plan");
    assert_eq!(fail_plan.status(), StatusCode::CONFLICT);
    assert!(stray.exists(), "stray file must not be deleted on failure");
    fs::remove_file(&stray).expect("clean stray");

    // 8. Secret blobs: 1 referenced (>=7d), 1 young orphan (<7d), 1 aged orphan (>=7d)
    server.stop();
    let db_path = dir.path().join("database").join("opendash.sqlite");
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .expect("connect db");
    let ref_secret = format!("{:064x}", 0xaaaa_0001u64);
    let young_secret = format!("{:064x}", 0xbbbb_0002u64);
    let aged_secret = format!("{:064x}", 0xcccc_0003u64);

    sqlx::query("UPDATE site_smtp_settings SET secret_ref = ? WHERE id = 1")
        .bind(&ref_secret)
        .execute(&pool)
        .await
        .expect("seed referenced secret in site_smtp_settings");
    pool.close().await;

    let secrets_dir = dir.path().join("database").join("secrets").join("smtp");
    fs::create_dir_all(&secrets_dir).expect("create secrets dir");
    let ref_path = secrets_dir.join(format!("{ref_secret}.bin"));
    let young_sec_path = secrets_dir.join(format!("{young_secret}.bin"));
    let aged_sec_path = secrets_dir.join(format!("{aged_secret}.bin"));
    fs::write(&ref_path, b"SECRET_PAYLOAD_DO_NOT_LEAK").expect("write ref secret");
    fs::write(&young_sec_path, b"YOUNG_ORPHAN_SECRET").expect("write young secret");
    fs::write(&aged_sec_path, b"AGED_ORPHAN_SECRET").expect("write aged secret");
    for p in [&ref_path, &aged_sec_path] {
        let f = File::options()
            .write(true)
            .open(p)
            .expect("open old secret");
        f.set_modified(SystemTime::now() - Duration::from_secs(8 * 24 * 3600))
            .expect("set mtime >= 7d");
    }
    {
        let f = File::options()
            .write(true)
            .open(&young_sec_path)
            .expect("open young secret");
        f.set_modified(SystemTime::now() - Duration::from_secs(2 * 24 * 3600))
            .expect("set mtime 2d");
    }

    server = RunningServer::start(dir.path()).await;

    // Diagnostics must redact secret payloads/paths
    let diag_resp = client
        .get(format!(
            "{}/api/admin/operations/diagnostics",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .send()
        .await
        .expect("diagnostics");
    assert_eq!(diag_resp.status(), StatusCode::OK);
    let diag_text = diag_resp.text().await.expect("diag text");
    assert!(!diag_text.contains("SECRET_PAYLOAD"));
    assert!(!diag_text.contains(&aged_secret));
    let diag_json: Value = serde_json::from_str(&diag_text).expect("diag json");
    assert_eq!(diag_json["orphan_secret_blobs"].as_u64(), Some(2));
    assert_eq!(diag_json["purgeable_secret_blobs"].as_u64(), Some(1));

    // Purge >=7d: removes aged_sec_path, preserves young_sec_path, ref_path, and <7d quarantine
    let purge_resp = client
        .post(format!(
            "{}/api/admin/operations/quarantine/purge",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "confirm": true }))
        .send()
        .await
        .expect("purge");
    assert_eq!(purge_resp.status(), StatusCode::OK);
    let purge_json: Value = purge_resp.json().await.expect("purge json");
    assert_eq!(purge_json["purged_secret_blobs"].as_u64(), Some(1));
    assert!(!aged_sec_path.exists(), "aged orphan secret must be purged");
    assert!(
        young_sec_path.exists(),
        "young orphan secret <7d must remain"
    );
    assert!(ref_path.exists(), "referenced secret must never be purged");

    // Second purge is idempotent
    let purge2 = client
        .post(format!(
            "{}/api/admin/operations/quarantine/purge",
            server.base_url
        ))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "confirm": true }))
        .send()
        .await
        .expect("purge2");
    assert_eq!(purge2.status(), StatusCode::OK);
    let purge2_json: Value = purge2.json().await.expect("purge2 json");
    assert_eq!(purge2_json["purged_secret_blobs"].as_u64(), Some(0));
    assert_eq!(purge2_json["purged_quarantine_dirs"].as_u64(), Some(0));
}
