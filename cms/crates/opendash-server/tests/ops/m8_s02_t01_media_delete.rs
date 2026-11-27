use std::{
    fs,
    io::Cursor,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
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
            "opendash-m8-s02-t01-{label}-{}-{id}",
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
    port: u16,
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
                        port,
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

    async fn restart(&mut self) {
        self.stop();
        *self = Self::start_on_port(&self.site_root, self.port).await;
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn tiny_png(seed: u8) -> Vec<u8> {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
        2,
        2,
        Rgba([seed, seed.wrapping_add(40), 200, 255]),
    ));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png)
        .expect("encode png");
    out.into_inner()
}

async fn setup_and_login(
    client: &Client,
    base_url: &str,
    username: &str,
    password: &str,
    do_setup: bool,
) -> (String, String) {
    if do_setup {
        let setup = client
            .post(format!("{base_url}/auth/setup"))
            .json(&json!({ "username": username, "password": password }))
            .send()
            .await
            .expect("setup");
        assert_eq!(setup.status(), StatusCode::CREATED);
    }

    let login = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({ "username": username, "password": password }))
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
    let json: Value = login.json().await.expect("login json");
    let csrf = json["csrf_token"].as_str().expect("csrf").to_string();
    (cookie, csrf)
}

async fn upload_png(client: &Client, base_url: &str, cookie: &str, csrf: &str, seed: u8) -> Value {
    let resp = client
        .post(format!("{base_url}/api/admin/media"))
        .header(reqwest::header::COOKIE, cookie)
        .header("X-CSRF-Token", csrf)
        .header(reqwest::header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", format!("sample-{seed}.png"))
        .body(tiny_png(seed))
        .send()
        .await
        .expect("upload png");
    assert_eq!(resp.status(), StatusCode::CREATED);
    resp.json().await.expect("upload json")
}

#[tokio::test]
async fn media_pins_acl_preconditions_aba_generation_quarantine_and_crash_recovery() {
    let dir = TempDirGuard::new("lifecycle");
    let mut server = RunningServer::start(dir.path()).await;
    let client = Client::new();

    let (admin_cookie, admin_csrf) =
        setup_and_login(&client, &server.base_url, "admin", "Password123!", true).await;

    // Create an editor user to verify Editor cannot delete shared media (403)
    let editor_create = client
        .post(format!("{}/api/admin/users", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "username": "editor1",
            "password": "Password123!",
            "role": "editor"
        }))
        .send()
        .await
        .expect("create editor");
    assert_eq!(editor_create.status(), StatusCode::CREATED);
    let (editor_cookie, editor_csrf) =
        setup_and_login(&client, &server.base_url, "editor1", "Password123!", false).await;

    // Upload image 1
    let asset = upload_png(&client, &server.base_url, &admin_cookie, &admin_csrf, 11).await;
    let id = asset["id"].as_str().expect("id").to_string();
    assert_eq!(asset["generation"].as_i64(), Some(1));
    assert_eq!(asset["version"].as_i64(), Some(1));

    // GET /api/admin/media/{id} exposes gv1-v1 ETag
    let get_meta = client
        .get(format!("{}/api/admin/media/{id}", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .send()
        .await
        .expect("get meta");
    assert_eq!(get_meta.status(), StatusCode::OK);
    assert_eq!(
        get_meta
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok()),
        Some("\"gv1-v1\"")
    );

    // Editor cannot delete (403)
    let ed_del = client
        .delete(format!("{}/api/admin/media/{id}", server.base_url))
        .header(reqwest::header::COOKIE, &editor_cookie)
        .header("X-CSRF-Token", &editor_csrf)
        .header("If-Match", "\"gv1-v1\"")
        .send()
        .await
        .expect("editor delete");
    assert_eq!(ed_del.status(), StatusCode::FORBIDDEN);

    // Missing If-Match -> 428; stale/bad -> 412; absent -> 404
    let no_if_match = client
        .delete(format!("{}/api/admin/media/{id}", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .send()
        .await
        .expect("no if-match");
    assert_eq!(no_if_match.status(), StatusCode::PRECONDITION_REQUIRED);

    let bad_if_match = client
        .delete(format!("{}/api/admin/media/{id}", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"gv1-v99\"")
        .send()
        .await
        .expect("stale if-match");
    assert_eq!(bad_if_match.status(), StatusCode::PRECONDITION_FAILED);

    let absent_id = format!("{:064x}", 999);
    let not_found = client
        .delete(format!("{}/api/admin/media/{absent_id}", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"gv1-v1\"")
        .send()
        .await
        .expect("absent media");
    assert_eq!(not_found.status(), StatusCode::NOT_FOUND);

    // Pin via Home SEO -> DELETE must fail 409 with pin diagnostic
    let seo_put = client
        .put(format!("{}/api/admin/seo/home", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({
            "title": "Home",
            "description": "Desc",
            "og_image_id": id,
            "canonical_url": null,
            "noindex": false
        }))
        .send()
        .await
        .expect("pin via home seo");
    assert_eq!(seo_put.status(), StatusCode::OK);

    let blocked_seo = client
        .delete(format!("{}/api/admin/media/{id}", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"gv1-v1\"")
        .send()
        .await
        .expect("delete pinned by seo");
    assert_eq!(blocked_seo.status(), StatusCode::CONFLICT);
    let blocked_json: Value = blocked_seo.json().await.expect("409 json");
    assert_eq!(blocked_json["pins"]["total_pins"].as_u64(), Some(1));
    assert_eq!(
        blocked_json["pins"]["pins"][0]["source_kind"].as_str(),
        Some("home_seo")
    );

    // Unpin Home SEO
    let seo_unpin = client
        .put(format!("{}/api/admin/seo/home", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"v2\"")
        .json(&json!({
            "title": "Home",
            "description": "Desc",
            "og_image_id": null,
            "canonical_url": null,
            "noindex": false
        }))
        .send()
        .await
        .expect("unpin home seo");
    assert_eq!(seo_unpin.status(), StatusCode::OK);

    // Pin via opaque plugin_kv & malformed JSON test (stop server briefly to seed plugin_kv / malformed JSON)
    server.stop();
    let db_path = dir.path().join("database").join("opendash.sqlite");
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .expect("connect db");
    sqlx::query(
        "INSERT INTO plugins (id, name, version, enabled, manifest_json, wasm_blob) VALUES ('p1', 'P1', '1.0.0', 0, '{}', X'')",
    )
    .execute(&pool)
    .await
    .expect("insert inactive plugin");
    sqlx::query("INSERT INTO plugin_kv (plugin_id, key, value_json) VALUES ('p1', 'k1', ?)")
        .bind(format!("\"/media/{id}/thumbnail\""))
        .execute(&pool)
        .await
        .expect("insert opaque kv pin");
    pool.close().await;

    server.restart().await;
    let (admin_cookie, admin_csrf) =
        setup_and_login(&client, &server.base_url, "admin", "Password123!", false).await;

    let blocked_kv = client
        .delete(format!("{}/api/admin/media/{id}", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"gv1-v1\"")
        .send()
        .await
        .expect("delete pinned by opaque kv");
    assert_eq!(blocked_kv.status(), StatusCode::CONFLICT);
    let kv_json: Value = blocked_kv.json().await.expect("kv 409 json");
    assert_eq!(
        kv_json["pins"]["pins"][0]["source_kind"].as_str(),
        Some("plugin_kv")
    );

    // Malformed persisted JSON in plugin_kv blocks deletion fail-closed (409)
    server.stop();
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .expect("reconnect db");
    sqlx::query(
        "UPDATE plugin_kv SET value_json = '{bad-json' WHERE plugin_id = 'p1' AND key = 'k1'",
    )
    .execute(&pool)
    .await
    .expect("corrupt plugin_kv json");
    pool.close().await;

    server.restart().await;
    let (admin_cookie, admin_csrf) =
        setup_and_login(&client, &server.base_url, "admin", "Password123!", false).await;
    let blocked_malformed = client
        .delete(format!("{}/api/admin/media/{id}", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"gv1-v1\"")
        .send()
        .await
        .expect("delete with malformed json");
    assert_eq!(blocked_malformed.status(), StatusCode::CONFLICT);

    // Clean plugin_kv -> DELETE succeeds (200 finalized) and moves files to .quarantine
    server.stop();
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .expect("reconnect db");
    sqlx::query("DELETE FROM plugin_kv WHERE plugin_id = 'p1'")
        .execute(&pool)
        .await
        .expect("clear plugin_kv");
    pool.close().await;

    server.restart().await;
    let (admin_cookie, admin_csrf) =
        setup_and_login(&client, &server.base_url, "admin", "Password123!", false).await;

    let del_ok = client
        .delete(format!("{}/api/admin/media/{id}", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"gv1-v1\"")
        .send()
        .await
        .expect("delete gen 1");
    assert_eq!(del_ok.status(), StatusCode::OK);
    let del_job: Value = del_ok.json().await.expect("delete job json");
    assert_eq!(del_job["phase"].as_str(), Some("finalized"));
    let job_id = del_job["job_id"].as_str().expect("job_id");

    let shard = &id[..2];
    assert!(
        !dir.path()
            .join("uploads")
            .join("media")
            .join(shard)
            .join(format!("{id}.png"))
            .exists()
    );
    assert!(
        dir.path()
            .join("uploads")
            .join(".quarantine")
            .join(job_id)
            .join(format!("{id}.png"))
            .exists()
    );
    assert!(
        dir.path()
            .join("uploads")
            .join(".quarantine")
            .join(job_id)
            .join(format!("{id}.thumb.png"))
            .exists()
    );

    // Reader on deleted asset returns 404
    let get_deleted = client
        .get(format!("{}/api/admin/media/{id}", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .send()
        .await
        .expect("get deleted");
    assert_eq!(get_deleted.status(), StatusCode::NOT_FOUND);

    // Re-upload same asset (ABA protection) -> gets generation = 2
    let reuploaded = upload_png(&client, &server.base_url, &admin_cookie, &admin_csrf, 11).await;
    assert_eq!(reuploaded["id"].as_str(), Some(id.as_str()));
    assert_eq!(reuploaded["generation"].as_i64(), Some(2));
    assert_eq!(reuploaded["version"].as_i64(), Some(1));

    // Stale gv1-v1 fails 412; gv2-v1 succeeds (2nd delete cycle)
    let stale_aba = client
        .delete(format!("{}/api/admin/media/{id}", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"gv1-v1\"")
        .send()
        .await
        .expect("stale aba delete");
    assert_eq!(stale_aba.status(), StatusCode::PRECONDITION_FAILED);

    let del_gen2 = client
        .delete(format!("{}/api/admin/media/{id}", server.base_url))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", "\"gv2-v1\"")
        .send()
        .await
        .expect("delete gen 2");
    assert_eq!(del_gen2.status(), StatusCode::OK);

    // Crash recovery test: upload image 2, simulate crash right after tombstone before file move,
    // verify deleting asset returns 404 on HEAD/GET, re-upload returns 409, and restart finalizes!
    let asset2 = upload_png(&client, &server.base_url, &admin_cookie, &admin_csrf, 77).await;
    let id2 = asset2["id"].as_str().expect("id2").to_string();
    let shard2 = &id2[..2];

    server.stop();
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .expect("reconnect db");
    let crash_job_id = format!("{:064x}", 0xdead);
    sqlx::query("UPDATE media_assets SET lifecycle_state = 'deleting', version = 2 WHERE id = ?")
        .bind(&id2)
        .execute(&pool)
        .await
        .expect("mark deleting");
    sqlx::query(
        "INSERT INTO media_delete_jobs (
            job_id, media_id, generation, expected_version, phase,
            original_rel_path, thumbnail_rel_path,
            quarantine_original_rel_path, quarantine_thumbnail_rel_path
        ) VALUES (?, ?, 1, 1, 'moving', ?, ?, ?, ?)",
    )
    .bind(&crash_job_id)
    .bind(&id2)
    .bind(format!("media/{shard2}/{id2}.png"))
    .bind(format!("media/{shard2}/{id2}.thumb.png"))
    .bind(format!(".quarantine/{crash_job_id}/{id2}.png"))
    .bind(format!(".quarantine/{crash_job_id}/{id2}.thumb.png"))
    .execute(&pool)
    .await
    .expect("insert crashed delete job");
    pool.close().await;

    // Restart recovers pending delete job idempotently!
    server.restart().await;
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
        .expect("reconnect db");
    let phase: String = sqlx::query_scalar("SELECT phase FROM media_delete_jobs WHERE job_id = ?")
        .bind(&crash_job_id)
        .fetch_one(&pool)
        .await
        .expect("query recovered job");
    assert_eq!(phase, "finalized");
    let still_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_assets WHERE id = ?")
        .bind(&id2)
        .fetch_one(&pool)
        .await
        .expect("count media_assets");
    assert_eq!(still_exists, 0);
    pool.close().await;
}
