use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use age::secrecy::SecretString;
use opendash_core::backup::{
    BACKUP_FORMAT_VERSION, BackupFileKind, BackupFileRecord, BackupManifestV1,
    PortableRuntimeConfigV1, RESTORE_POLICY_VERSION,
};
use reqwest::{Client, StatusCode, header, redirect::Policy};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{
    Connection,
    sqlite::{SqliteConnectOptions, SqliteConnection, SqlitePoolOptions},
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

static SEQ: AtomicU64 = AtomicU64::new(1);

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("opendash-m8-s08-t01-{label}-{nanos}-{id}"));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
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

fn allocate_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().unwrap().port()
}

async fn spawn_server(site_root: &Path, db_path: &Path) -> ServerGuard {
    let port = allocate_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
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
            && resp.status().is_success()
        {
            return ServerGuard { child, base_url };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("server did not become healthy at {base_url}");
}

async fn setup_and_login(
    client: &Client,
    base_url: &str,
    username: &str,
    password: &str,
) -> (String, String) {
    let setup_resp = client
        .post(format!("{base_url}/auth/setup"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .expect("setup admin");
    assert_eq!(setup_resp.status(), StatusCode::CREATED);
    login_session(client, base_url, username, password).await
}

async fn login_session(
    client: &Client,
    base_url: &str,
    username: &str,
    password: &str,
) -> (String, String) {
    let resp = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .expect("login request");
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|s| s.split(';').next().map(|kv| kv.to_string()))
        .expect("session cookie");
    let body: Value = resp.json().await.expect("login json");
    let csrf = body["csrf_token"].as_str().expect("csrf_token").to_string();
    (cookie, csrf)
}

async fn poll_job(client: &Client, base_url: &str, cookie: &str, job_id: &str) -> Value {
    for _ in 0..180 {
        let resp = client
            .get(format!("{base_url}/api/admin/operations/jobs/{job_id}"))
            .header(header::COOKIE, cookie)
            .send()
            .await
            .expect("poll job");
        assert_eq!(resp.status(), StatusCode::OK);
        let job: Value = resp.json().await.expect("job json");
        if matches!(
            job["state"].as_str().unwrap_or(""),
            "succeeded" | "failed" | "cancelled" | "interrupted"
        ) {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("job {job_id} timed out");
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn minimal_1x1_png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ]
}

fn build_theme_zip(theme_id: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = json!({
            "manifest_version": 3,
            "theme_id": theme_id,
            "name": theme_id,
            "version": "1.0.0",
            "templates": [{ "kind": "single", "file": "single.html" }],
            "parts": { "header": "header.html" },
            "menus": {
                "slots": ["primary"]
            }
        });
        zip.start_file("theme.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("header.html", opts).unwrap();
        zip.write_all(
            b"<header>{% if menus.primary is defined %}{% for item in menus.primary.items %}<a href=\"{{ item.url }}\">{{ item.label }}</a>{% endfor %}{% endif %}</header>",
        )
        .unwrap();
        zip.start_file("single.html", opts).unwrap();
        zip.write_all(b"<article>{{ entry.title }}{{ od_content() }}</article>")
            .unwrap();
        zip.finish().unwrap();
    }
    buf
}

fn encrypt_age_bytes(plaintext: &[u8], passphrase: &str) -> Vec<u8> {
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.to_string()));
    recipient.set_work_factor(10);
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .expect("age encryptor");
    let mut out = Vec::new();
    let mut writer = encryptor.wrap_output(&mut out).expect("age wrap");
    writer.write_all(plaintext).expect("age write");
    writer.finish().expect("age finish");
    out
}

// S03 public Home is selected from a published Page through Reading Settings.
// Keep this setup through HTTP so the tests exercise publication and CAS.
async fn create_s03_home(client: &Client, base: &str, cookie: &str, csrf: &str) -> i64 {
    let created = client
        .post(format!("{base}/api/admin/content-types/page/entries"))
        .header("Cookie", cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .json(&json!({"title":"S03 Home","slug":"s03-home","values":{"body":"Home"}}))
        .send()
        .await
        .unwrap();
    let status = created.status();
    let text = created.text().await.unwrap();
    assert_eq!(status.as_u16(), 201, "create S03 Page: {text}");
    let entry: Value = serde_json::from_str(&text).unwrap();
    let id = entry["id"].as_i64().unwrap();
    let publish = client
        .post(format!(
            "{base}/api/admin/content-types/page/entries/{id}/publication"
        ))
        .header("Cookie", cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header(
            "If-Match",
            format!("\"v{}\"", entry["version"].as_i64().unwrap()),
        )
        .json(&json!({"status":"published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(publish.status().as_u16(), 200, "publish Home Page");
    let reading = client
        .get(format!("{base}/api/admin/settings/reading"))
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(reading.status().as_u16(), 200);
    let token = reading.headers()["etag"].to_str().unwrap().to_owned();
    let selected = client
        .put(format!("{base}/api/admin/settings/reading"))
        .header("Cookie", cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header("If-Match", token)
        .json(&json!({"homepage_page_id":id,"posts_page_id":null}))
        .send()
        .await
        .unwrap();
    let status = selected.status();
    let text = selected.text().await.unwrap();
    assert_eq!(status.as_u16(), 200, "save S03 Reading: {text}");
    id
}

async fn save_s03_home_doc(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    page_id: i64,
    version: i64,
    document: &Value,
) -> reqwest::Response {
    client
        .put(format!("{base}/api/admin/entries/page/{page_id}/builder"))
        .header("Cookie", cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header("If-Match", format!("\"v{version}\""))
        .json(document)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn full_v1_backup_restore_reedit_and_equivalence() {
    let tmp = TempDirGuard::new("full-v1");
    let source_root = tmp.path().join("source-site");
    // Use an external DB path outside source_root to test external DB backup + rebasing.
    let external_db_dir = tmp.path().join("external-db");
    let source_db = external_db_dir.join("prod-external.sqlite");
    fs::create_dir_all(&source_root).unwrap();
    fs::create_dir_all(&external_db_dir).unwrap();

    let srv = spawn_server(&source_root, &source_db).await;
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let (cookie, csrf) =
        setup_and_login(&client, &srv.base_url, "admin", "admin-password-123").await;

    // 1. Settings (general + public)
    let gen_put = client
        .put(format!("{}/api/admin/settings/general", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "site_title": "OpenDash M8 E2E Source",
            "tagline": "Baseline tagline",
            "timezone": "Asia/Ho_Chi_Minh"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(gen_put.status(), StatusCode::OK);

    let pub_put = client
        .put(format!("{}/api/admin/settings/public", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "public_origin": srv.base_url,
            "content_language": "vi",
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert!(
        pub_put.status().is_success(),
        "public settings: {}",
        pub_put.status()
    );

    // 2. Theme install + activate
    let theme_zip = build_theme_zip("m8-e2e-theme");
    let theme_inst = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(theme_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(theme_inst.status(), StatusCode::CREATED);
    let rev_id = theme_inst.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_string();

    let theme_list: Value = client
        .get(format!("{}/api/admin/themes", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let theme_ver = theme_list["version"].as_i64().unwrap();
    let theme_act = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{theme_ver}\""))
        .json(&json!({ "revision_id": rev_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(theme_act.status(), StatusCode::OK);

    // 3. Menu create + items
    let menu_resp = client
        .post(format!("{}/api/admin/menus", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "menu_key": "primary", "title": "Primary Navigation" }))
        .send()
        .await
        .unwrap();
    assert_eq!(menu_resp.status(), StatusCode::CREATED);
    let menu_json: Value = menu_resp.json().await.unwrap();
    let menu_id = menu_json["menu"]["id"].as_i64().unwrap();
    let menu_ver = menu_json["menu"]["version"].as_i64().unwrap();

    let menu_upd = client
        .put(format!("{}/api/admin/menus/{menu_id}", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{menu_ver}\""))
        .json(&json!({
            "title": "Primary Navigation",
            "items": [
                {
                    "client_key": "home-1",
                    "position": 0,
                    "label": "Trang chủ",
                    "target_kind": "external",
                    "external_url": "https://example.com/"
                },
                {
                    "client_key": "post-1",
                    "position": 1,
                    "label": "Bài viết",
                    "target_kind": "external",
                    "external_url": "https://example.com/bai-viet-e2e"
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(menu_upd.status(), StatusCode::OK);

    // 4. Custom Content Type + Published Entry + Route + SEO + Draft Entry
    let ct_resp = client
        .post(format!("{}/api/admin/content-types", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "key": "case_study",
            "label": "Case Study",
            "fields": [{ "key": "client_name", "label": "Client", "type": "short_text", "required": true }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ct_resp.status(), StatusCode::CREATED);

    let cpt_entry_resp = client
        .post(format!(
            "{}/api/admin/content-types/case_study/entries",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "e2e-case-study",
            "title": "E2E Case Study",
            "values": { "client_name": "Acme VN" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(cpt_entry_resp.status(), StatusCode::CREATED);
    let cpt_entry_id = cpt_entry_resp.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let pub_entry_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "e2e-post",
            "title": "E2E Published Post",
            "values": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_entry_resp.status(), StatusCode::CREATED);
    let pub_entry: Value = pub_entry_resp.json().await.unwrap();
    let pub_entry_id = pub_entry["id"].as_i64().unwrap();
    let mut pub_ver = pub_entry["version"].as_i64().unwrap();

    let publish_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{pub_entry_id}/publication",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{pub_ver}\""))
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(publish_resp.status(), StatusCode::OK);
    pub_ver = publish_resp.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    let route_resp = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{pub_entry_id}/route",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{pub_ver}\""))
        .json(&json!({ "custom_path": "/bai-viet-e2e" }))
        .send()
        .await
        .unwrap();
    assert_eq!(route_resp.status(), StatusCode::OK);

    // Draft entry must NOT leak publicly after restore.
    let draft_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "secret-draft-only",
            "title": "Secret Draft Title",
            "values": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(draft_resp.status(), StatusCode::CREATED);

    // 5. Media upload + pin in Home SEO + published Reading Page Builder.
    let png_bytes = minimal_1x1_png();
    let media_resp = client
        .post(format!("{}/api/admin/media", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-Media-Filename", "hero-e2e.png")
        .body(png_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(media_resp.status(), StatusCode::CREATED);
    let media_id = media_resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let seo_home = client
        .put(format!("{}/api/admin/seo/home", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({
            "title": "Home SEO E2E",
            "description": "Home SEO description",
            "og_image_id": media_id,
            "canonical_url": null,
            "noindex": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(seo_home.status(), StatusCode::OK);

    let source_media_bytes = client
        .get(format!("{}/media/{media_id}/file", srv.base_url))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
        .to_vec();

    let home_id = create_s03_home(&client, &srv.base_url, &cookie, &csrf).await;
    let hero_doc = json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec1",
            "children": [{
                "id": "row1",
                "children": [{
                    "id": "col1",
                    "children": [{
                        "id": "el1",
                        "type": "core/text",
                        "props": {
                            "text": "M8_E2E_BASELINE_HERO"
                        }
                    }]
                }]
            }]
        }]
    });
    let builder_put = save_s03_home_doc(
        &client,
        &srv.base_url,
        &cookie,
        &csrf,
        home_id,
        2,
        &hero_doc,
    )
    .await;
    assert_eq!(builder_put.status(), StatusCode::OK);

    // 6. Contact form
    let form_resp = client
        .post(format!("{}/api/admin/contact/forms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "lien-he-e2e",
            "title": "Liên hệ E2E",
            "status": "active",
            "schema": {
                "version": 1,
                "fields": [
                    { "key": "name", "label": "Họ tên", "kind": "text", "required": true }
                ]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(form_resp.status(), StatusCode::CREATED);
    let form_id = form_resp.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    // 7. Seed plugin + plugin_kv directly in source DB to assert plugin + KV preservation.
    {
        let mut conn =
            SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&source_db))
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO plugins (id, name, version, enabled, manifest_json, wasm_blob)
             VALUES ('e2e-plugin', 'E2E Plugin', '1.0.0', 0, '{}', X'')",
        )
        .execute(&mut conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO plugin_kv (plugin_id, key, value_json) VALUES ('e2e-plugin', 'sync_cursor', '{\"cursor\":42}')",
        )
        .execute(&mut conn)
        .await
        .unwrap();
    }

    // 8. Create encrypted backup + download artifact + record SHA256
    let passphrase = "m8-s08-strong-passphrase";
    let backup_resp = client
        .post(format!("{}/api/admin/operations/backups", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "passphrase": passphrase,
            "confirm_passphrase": passphrase,
            "reason": "m8-s08-t01 full v1 e2e"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(backup_resp.status(), StatusCode::ACCEPTED);
    let backup_job_id = backup_resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let backup_job = poll_job(&client, &srv.base_url, &cookie, &backup_job_id).await;
    assert_eq!(
        backup_job["state"], "succeeded",
        "backup failed: {backup_job}"
    );

    let dl_resp = client
        .get(format!(
            "{}/api/admin/operations/backups/{backup_job_id}/file",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(dl_resp.status(), StatusCode::OK);
    let age_bytes = dl_resp.bytes().await.unwrap().to_vec();
    let _artifact_sha256 = sha256_hex(&age_bytes);
    assert!(age_bytes.len() > 64);

    // 9. Mutate source after backup (must NOT appear in restored target).
    let mut_put = client
        .put(format!("{}/api/admin/settings/general", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({
            "site_title": "MUTATED SOURCE AFTER BACKUP",
            "tagline": "Mutated",
            "timezone": "UTC"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(mut_put.status(), StatusCode::OK);

    // 10. Upload backup + Admin restore to separate root
    let upload_resp = client
        .post(format!(
            "{}/api/admin/operations/restore-uploads",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(age_bytes)
        .send()
        .await
        .unwrap();
    assert_eq!(upload_resp.status(), StatusCode::CREATED);
    let upload_id = upload_resp.json::<Value>().await.unwrap()["upload_id"]
        .as_str()
        .unwrap()
        .to_string();

    let restore_resp = client
        .post(format!("{}/api/admin/operations/restores", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "upload_id": upload_id,
            "passphrase": passphrase,
            "reason": "m8-s08-t01 restore"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(restore_resp.status(), StatusCode::ACCEPTED);
    let restore_job_id = restore_resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let restore_job = poll_job(&client, &srv.base_url, &cookie, &restore_job_id).await;
    assert_eq!(
        restore_job["state"], "succeeded",
        "restore failed: {restore_job}"
    );
    let restore_summary: Value =
        serde_json::from_str(restore_job["summary_json"].as_str().unwrap()).unwrap();
    let restored_root = PathBuf::from(restore_summary["target_root"].as_str().unwrap());
    assert!(restored_root.join("READY.json").is_file());
    let rebased_db = restored_root.join("database").join("opendash.sqlite");
    assert!(
        rebased_db.is_file(),
        "external DB must be rebased inside restored root"
    );

    // 11. Stop source server, start target server on restored_root
    drop(srv);
    let target = spawn_server(&restored_root, &rebased_db).await;

    // Reset exception: source session cookie must NOT work on target (401).
    let old_cookie_check = client
        .get(format!("{}/api/admin/settings/general", target.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(old_cookie_check.status(), StatusCode::UNAUTHORIZED);

    // Safe mode: public has X-Robots-Tag: noindex, nofollow and contact returns 503.
    let pub_safe = client
        .get(format!("{}/", target.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_safe.status(), StatusCode::OK);
    assert_eq!(
        pub_safe
            .headers()
            .get("x-robots-tag")
            .and_then(|v| v.to_str().ok()),
        Some("noindex, nofollow")
    );
    let contact_safe = client
        .post(format!("{}/api/public/contact/{form_id}", target.base_url))
        .header(header::ORIGIN, &target.base_url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body("form_version=1&nonce=abc&name=Test")
        .send()
        .await
        .unwrap();
    assert_eq!(contact_safe.status(), StatusCode::SERVICE_UNAVAILABLE);

    // Re-auth + activate recovery
    let (t_cookie, t_csrf) =
        login_session(&client, &target.base_url, "admin", "admin-password-123").await;
    let status_json: Value = client
        .get(format!(
            "{}/api/admin/operations/recovery/status",
            target.base_url
        ))
        .header(header::COOKIE, &t_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status_json["safe_mode"], true);
    assert_eq!(status_json["smtp_enabled"], false);
    let generation = status_json["expected_generation"].as_str().unwrap();

    let act_resp = client
        .post(format!(
            "{}/api/admin/operations/recovery/activate",
            target.base_url
        ))
        .header(header::COOKIE, &t_cookie)
        .header("X-CSRF-Token", &t_csrf)
        .json(&json!({
            "password": "admin-password-123",
            "expected_generation": generation
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(act_resp.status(), StatusCode::OK);

    // 12. Verify 100% equivalence on restored target (before source mutation!)
    let gen_after: Value = client
        .get(format!("{}/api/admin/settings/general", target.base_url))
        .header(header::COOKIE, &t_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(gen_after["site_title"], "OpenDash M8 E2E Source");

    let home_html = client
        .get(format!("{}/", target.base_url))
        .send()
        .await
        .unwrap();
    assert!(home_html.headers().get("x-robots-tag").is_none());
    let home_body = home_html.text().await.unwrap();
    assert!(home_body.contains("M8_E2E_BASELINE_HERO"));

    let entry_pub = client
        .get(format!("{}/bai-viet-e2e", target.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(entry_pub.status(), StatusCode::OK);
    let entry_html = entry_pub.text().await.unwrap();
    assert!(entry_html.contains("E2E Published Post"));

    // Draft entry must NOT leak publicly.
    let draft_pub = client
        .get(format!("{}/post/secret-draft-only", target.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(draft_pub.status(), StatusCode::NOT_FOUND);

    // Media file bytes match 100%.
    let media_pub = client
        .get(format!("{}/media/{media_id}/file", target.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(media_pub.status(), StatusCode::OK);
    assert_eq!(
        media_pub.bytes().await.unwrap().as_ref(),
        source_media_bytes.as_slice()
    );

    // Plugin KV + custom CPT & post entry IDs preserved in DB.
    {
        let mut conn = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&rebased_db)
                .read_only(true),
        )
        .await
        .unwrap();
        let kv: (String,) = sqlx::query_as(
            "SELECT value_json FROM plugin_kv WHERE plugin_id = 'e2e-plugin' AND key = 'sync_cursor'",
        )
        .fetch_one(&mut conn)
        .await
        .unwrap();
        assert_eq!(kv.0, "{\"cursor\":42}");
        let cpt_eid: (i64,) =
            sqlx::query_as("SELECT id FROM content_entries WHERE slug = 'e2e-case-study'")
                .fetch_one(&mut conn)
                .await
                .unwrap();
        assert_eq!(cpt_eid.0, cpt_entry_id);
        let post_eid: (i64,) =
            sqlx::query_as("SELECT id FROM content_entries WHERE slug = 'e2e-post'")
                .fetch_one(&mut conn)
                .await
                .unwrap();
        assert_eq!(post_eid.0, pub_entry_id);
    }

    // 13. Re-edit & Save on restored target -> verify public reflects edit
    let reedit_doc = json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec1",
            "children": [{
                "id": "row1",
                "children": [{
                    "id": "col1",
                    "children": [{
                        "id": "el1",
                        "type": "core/text",
                        "props": {
                            "text": "M8_E2E_REEDITED_HERO"
                        }
                    }]
                }]
            }]
        }]
    });
    let reedit_put = save_s03_home_doc(
        &client,
        &target.base_url,
        &t_cookie,
        &t_csrf,
        home_id,
        3,
        &reedit_doc,
    )
    .await;
    assert_eq!(reedit_put.status(), StatusCode::OK);
    let reedited_home = client
        .get(format!("{}/", target.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(reedited_home.contains("M8_E2E_REEDITED_HERO"));
}

#[tokio::test]
async fn upgrade_0031_to_0033_and_interrupted_job_recovery() {
    let tmp = TempDirGuard::new("upgrade-0031");
    let site_root = tmp.path().join("site");
    let db_path = site_root.join("database").join("opendash.sqlite");
    fs::create_dir_all(db_path.parent().unwrap()).unwrap();

    // Copy migrations 0001..=0031 to build a pre-M8 0031 database.
    let mig_0031 = tmp.path().join("mig-0031");
    fs::create_dir_all(&mig_0031).unwrap();
    let repo_mig = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("migrations");
    for entry in fs::read_dir(&repo_mig).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".sql") && name.as_str() <= "0031_zzzz" {
            fs::copy(entry.path(), mig_0031.join(&name)).unwrap();
        }
    }

    let pool = SqlitePoolOptions::new()
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await
        .unwrap();
    sqlx::migrate::Migrator::new(mig_0031.as_path())
        .await
        .unwrap()
        .run(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE site_general_settings SET site_title = 'Legacy 0031 Site', tagline = 'Tag' WHERE id = 1",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;

    // Boot current binary against 0031 DB -> upgrades to 0033 cleanly and preserves data.
    let srv = spawn_server(&site_root, &db_path).await;
    let client = Client::new();
    let (cookie, _) = setup_and_login(&client, &srv.base_url, "admin", "admin-password-123").await;
    let gen_settings: Value = client
        .get(format!("{}/api/admin/settings/general", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(gen_settings["site_title"], "Legacy 0031 Site");

    // Seed an in-flight running job, restart server -> must transition to interrupted.
    drop(srv);
    let inflight_id = "1".repeat(64);
    {
        let mut conn =
            SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&db_path))
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO operation_jobs (id, kind, state, actor_user_id, reason)
             VALUES (?1, 'backup', 'running', '1', 'crash sim')",
        )
        .bind(&inflight_id)
        .execute(&mut conn)
        .await
        .unwrap();
    }

    let srv2 = spawn_server(&site_root, &db_path).await;
    let (cookie2, _) = login_session(&client, &srv2.base_url, "admin", "admin-password-123").await;
    let job = poll_job(&client, &srv2.base_url, &cookie2, &inflight_id).await;
    assert_eq!(job["state"], "interrupted");
}

#[tokio::test]
async fn negative_corpus_preserves_source_and_never_publishes_target() {
    let tmp = TempDirGuard::new("negative");
    let source_root = tmp.path().join("source-site");
    let source_db = source_root.join("database").join("opendash.sqlite");
    fs::create_dir_all(source_db.parent().unwrap()).unwrap();

    let srv = spawn_server(&source_root, &source_db).await;
    let client = Client::new();
    let (cookie, csrf) =
        setup_and_login(&client, &srv.base_url, "admin", "admin-password-123").await;

    // Create 1 valid backup so we have a real .odbackup.age payload.
    let passphrase = "neg-corpus-passphrase";
    let b_resp = client
        .post(format!("{}/api/admin/operations/backups", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "passphrase": passphrase,
            "confirm_passphrase": passphrase,
            "reason": "negative corpus seed"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(b_resp.status(), StatusCode::ACCEPTED);
    let b_job_id = b_resp.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let b_job = poll_job(&client, &srv.base_url, &cookie, &b_job_id).await;
    assert_eq!(b_job["state"], "succeeded");

    let valid_age = client
        .get(format!(
            "{}/api/admin/operations/backups/{b_job_id}/file",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap()
        .to_vec();

    // 1. Tampered ciphertext -> restore fails, no READY.json created in restores/.
    let mut tampered = valid_age.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0xFF;
    let up_tamper = client
        .post(format!(
            "{}/api/admin/operations/restore-uploads",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(tampered)
        .send()
        .await
        .unwrap();
    assert_eq!(up_tamper.status(), StatusCode::CREATED);
    let tamper_id = up_tamper.json::<Value>().await.unwrap()["upload_id"]
        .as_str()
        .unwrap()
        .to_string();
    let r_tamper = client
        .post(format!("{}/api/admin/operations/restores", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "upload_id": tamper_id, "passphrase": passphrase, "reason": "tamper" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r_tamper.status(), StatusCode::ACCEPTED);
    let r_tamper_job_id = r_tamper.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r_tamper_job = poll_job(&client, &srv.base_url, &cookie, &r_tamper_job_id).await;
    assert_eq!(r_tamper_job["state"], "failed");

    // 2. Wrong passphrase -> restore fails.
    let up_wrong_pw = client
        .post(format!(
            "{}/api/admin/operations/restore-uploads",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(valid_age)
        .send()
        .await
        .unwrap();
    assert_eq!(up_wrong_pw.status(), StatusCode::CREATED);
    let wrong_pw_id = up_wrong_pw.json::<Value>().await.unwrap()["upload_id"]
        .as_str()
        .unwrap()
        .to_string();
    let r_wrong_pw = client
        .post(format!("{}/api/admin/operations/restores", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "upload_id": wrong_pw_id, "passphrase": "wrong-passphrase-xyz", "reason": "wrong-pw" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r_wrong_pw.status(), StatusCode::ACCEPTED);
    let r_wrong_pw_id = r_wrong_pw.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r_wrong_pw_job = poll_job(&client, &srv.base_url, &cookie, &r_wrong_pw_id).await;
    assert_eq!(r_wrong_pw_job["state"], "failed");

    // 3. Newer schema_version in manifest -> restore fails closed.
    let cfg = PortableRuntimeConfigV1::default();
    let cfg_bytes = serde_json::to_vec(&cfg).unwrap();
    let dummy_db = b"SQLite format 3\0";
    let newer_manifest = BackupManifestV1 {
        format_version: BACKUP_FORMAT_VERSION,
        app_version: "99.0.0".into(),
        source_sha: "future".into(),
        schema_version: 9999,
        migration_checksums: vec![],
        created_at: "1731600000".into(),
        files: vec![
            BackupFileRecord {
                entry_name: "00000001.bin".into(),
                kind: BackupFileKind::Db,
                logical_key: "db/main.sqlite".into(),
                size: dummy_db.len() as u64,
                sha256: sha256_hex(dummy_db),
            },
            BackupFileRecord {
                entry_name: "00000002.bin".into(),
                kind: BackupFileKind::PortableConfig,
                logical_key: "config/site.json".into(),
                size: cfg_bytes.len() as u64,
                sha256: sha256_hex(&cfg_bytes),
            },
        ],
        restore_policy_version: RESTORE_POLICY_VERSION,
    };
    let mut newer_zip = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut newer_zip));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("00000001.bin", opts).unwrap();
        zip.write_all(dummy_db).unwrap();
        zip.start_file("00000002.bin", opts).unwrap();
        zip.write_all(&cfg_bytes).unwrap();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(serde_json::to_vec(&newer_manifest).unwrap().as_slice())
            .unwrap();
        zip.finish().unwrap();
    }
    let newer_age = encrypt_age_bytes(&newer_zip, passphrase);
    let up_newer = client
        .post(format!(
            "{}/api/admin/operations/restore-uploads",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(newer_age)
        .send()
        .await
        .unwrap();
    assert_eq!(up_newer.status(), StatusCode::CREATED);
    let newer_id = up_newer.json::<Value>().await.unwrap()["upload_id"]
        .as_str()
        .unwrap()
        .to_string();
    let r_newer = client
        .post(format!("{}/api/admin/operations/restores", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "upload_id": newer_id, "passphrase": passphrase, "reason": "newer-schema" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r_newer.status(), StatusCode::ACCEPTED);
    let r_newer_id = r_newer.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r_newer_job = poll_job(&client, &srv.base_url, &cookie, &r_newer_id).await;
    assert_eq!(r_newer_job["state"], "failed");

    // Verify no target root in backups/restored was marked READY.json.
    let restores_dir = source_root.join("backups").join("restored");
    if restores_dir.exists() {
        for entry in fs::read_dir(&restores_dir).unwrap() {
            let entry = entry.unwrap();
            assert!(
                !entry.path().join("READY.json").exists(),
                "failed restore must not write READY.json"
            );
        }
    }

    // 4. Artifact quota 507: seed remaining 9 .odbackup.age files in backups/artifacts/ -> 11th request returns 507.
    let backups_dir = source_root.join("backups").join("artifacts");
    fs::create_dir_all(&backups_dir).unwrap();
    for idx in 2..=10 {
        fs::write(
            backups_dir.join(format!("quota-filler-{idx}.odbackup.age")),
            b"placeholder",
        )
        .unwrap();
    }
    let quota_resp = client
        .post(format!("{}/api/admin/operations/backups", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "passphrase": passphrase,
            "confirm_passphrase": passphrase,
            "reason": "quota 507"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(quota_resp.status(), StatusCode::INSUFFICIENT_STORAGE);

    // Source still healthy and serving admin API.
    let still_ok = client
        .get(format!("{}/api/admin/settings/general", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(still_ok.status(), StatusCode::OK);
}
