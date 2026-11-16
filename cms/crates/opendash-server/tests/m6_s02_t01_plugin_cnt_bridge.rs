use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use reqwest::Client;
use serde_json::{Value, json};
use zip::{ZipWriter, write::SimpleFileOptions};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), nanos));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct ServerHandle {
    child: Child,
    base_url: String,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn build_plugin_zip(manifest_json: &str, wasm_bytes: &[u8]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest_json.as_bytes()).unwrap();
        zip.start_file("plugin.wasm", opts).unwrap();
        zip.write_all(wasm_bytes).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

fn build_theme_zip(theme_json: &str) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();
        zip.start_file("theme.json", opts).unwrap();
        zip.write_all(theme_json.as_bytes()).unwrap();
        zip.start_file("templates/home.html", opts).unwrap();
        zip.write_all(b"<html><body><h1>Theme</h1></body></html>").unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

fn minimal_plugin_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32)
                i32.const 0)
        )"#,
    )
    .unwrap()
}

async fn spawn_server(site_root: &Path, port: u16) -> ServerHandle {
    let exe = env!("CARGO_BIN_EXE_opendash");
    let base_url = format!("http://127.0.0.1:{port}");
    let child = Command::new(exe)
        .env("OPENDASH_SITE_ROOT", site_root)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn opendash server");

    let client = Client::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await {
            if resp.status().is_success() {
                break;
            }
        }
        assert!(Instant::now() < deadline, "server failed to become healthy");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    ServerHandle { child, base_url }
}

async fn setup_and_login(client: &Client, base_url: &str) -> (String, String) {
    let resp = client
        .post(format!("{base_url}/auth/setup"))
        .header("Origin", base_url)
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success(), "setup failed: {}", resp.status());

    let login = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", base_url)
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(login.status().is_success(), "login failed: {}", login.status());
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let me: Value = client
        .get(format!("{base_url}/auth/me"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let csrf = me["csrf_token"]
        .as_str()
        .expect("csrf_token in /auth/me")
        .to_string();
    (cookie, csrf)
}

#[tokio::test]
async fn m6_s02_t01_plugin_cnt_bridge_lifecycle_and_collisions() {
    let temp = TempDir::new("m6_s02_t01");
    let port = free_port();
    let _server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;

    let wasm = minimal_plugin_wasm();

    // 1. Plugin declares CPT `job_posting` + custom fields
    let plugin_v1 = json!({
        "id": "job-board",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Job Board",
        "description": "CPT provider plugin",
        "entrypoint": "plugin.wasm",
        "declarations": {
            "content_types": [
                {
                    "key": "job_posting",
                    "label": "Job Posting",
                    "fields": [
                        { "key": "company", "label": "Company", "type": "short_text", "required": true },
                        { "key": "salary", "label": "Salary", "type": "number", "required": false }
                    ]
                }
            ]
        }
    })
    .to_string();
    let zip_v1 = build_plugin_zip(&plugin_v1, &wasm);
    let resp = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(zip_v1)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "plugin install failed: {:?}", resp.text().await);

    // Verify CPT appears with owner Plugin("job-board") and active status
    let list_resp = client
        .get(format!("{base_url}/api/admin/content-types"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);
    let types: Vec<Value> = list_resp.json().await.unwrap();
    let job_ct = types
        .iter()
        .find(|t| t["descriptor"]["key"] == "job_posting")
        .expect("job_posting registered");
    assert_eq!(job_ct["owner"]["kind"], "plugin");
    assert_eq!(job_ct["owner"]["id"], "job-board");
    assert_eq!(job_ct["status"], "active");

    // 2. Install & activate a Theme with its own CPT; verify plugin's CPT stays Active
    let theme_json = json!({
        "manifest_version": 2,
        "theme_id": "agency-theme",
        "version": "1.0.0",
        "name": "Agency Theme",
        "templates": { "home": "templates/home.html" },
        "declarations": {
            "content_types": [
                {
                    "key": "case_study",
                    "label": "Case Study",
                    "fields": []
                }
            ]
        }
    })
    .to_string();
    let theme_zip = build_theme_zip(&theme_json);
    let theme_install = client
        .post(format!("{base_url}/api/admin/themes"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(theme_zip)
        .send()
        .await
        .unwrap();
    assert!(
        theme_install.status().is_success(),
        "theme install failed: {}",
        theme_install.status()
    );
    let rev_id = theme_install.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_string();
    let theme_activate = client
        .put(format!("{base_url}/api/admin/themes/active"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "revision_id": rev_id }))
        .send()
        .await
        .unwrap();
    assert!(theme_activate.status().is_success());

    let types_after_theme: Vec<Value> = client
        .get(format!("{base_url}/api/admin/content-types"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let job_after_theme = types_after_theme
        .iter()
        .find(|t| t["descriptor"]["key"] == "job_posting")
        .unwrap();
    assert_eq!(job_after_theme["status"], "active");

    // 3. Create an entry of `job_posting`
    let entry_create = client
        .post(format!("{base_url}/api/admin/content-types/job_posting/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({
            "slug": "rust-engineer",
            "title": "Rust Engineer",
            "values": {
                "company": "OpenDash",
                "salary": 120000
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(entry_create.status(), 201, "entry create failed: {:?}", entry_create.text().await);
    let entry: Value = entry_create.json().await.unwrap();
    let entry_id = entry["id"].as_i64().unwrap();

    // 4. Disable plugin -> `job_posting` becomes `ProviderInactive`, entry CRUD still works
    let disable_resp = client
        .post(format!("{base_url}/api/admin/plugins/job-board/disable"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .header("if-match", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(disable_resp.status(), 200);
    let disable_body: Value = disable_resp.json().await.unwrap();
    let cas_v2 = disable_body["cas_version"].as_i64().unwrap();

    let types_disabled: Vec<Value> = client
        .get(format!("{base_url}/api/admin/content-types"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let job_disabled = types_disabled
        .iter()
        .find(|t| t["descriptor"]["key"] == "job_posting")
        .unwrap();
    assert_eq!(job_disabled["status"], "provider_inactive");

    // Editor/Admin can still update the entry while provider is inactive
    let entry_update = client
        .put(format!("{base_url}/api/admin/content-types/job_posting/entries/{entry_id}"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({
            "slug": "rust-engineer",
            "title": "Senior Rust Engineer",
            "values": {
                "company": "OpenDash",
                "salary": 150000
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(entry_update.status(), 200);

    // 5. Re-enable plugin -> `job_posting` becomes `Active` again
    let enable_resp = client
        .post(format!("{base_url}/api/admin/plugins/job-board/enable"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .header("if-match", cas_v2.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(enable_resp.status(), 200);

    // 6. Collision: claiming builtin `post` or another owner's `job_posting` -> 409 Conflict
    let hijack_builtin = json!({
        "id": "evil-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Evil",
        "declarations": {
            "content_types": [
                { "key": "post", "label": "Hijacked Post", "fields": [] }
            ]
        }
    })
    .to_string();
    let r_builtin = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(build_plugin_zip(&hijack_builtin, &wasm))
        .send()
        .await
        .unwrap();
    assert_eq!(r_builtin.status(), 409);

    let hijack_other = json!({
        "id": "other-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Other",
        "declarations": {
            "content_types": [
                { "key": "job_posting", "label": "Stolen Job", "fields": [] }
            ]
        }
    })
    .to_string();
    let r_other = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(build_plugin_zip(&hijack_other, &wasm))
        .send()
        .await
        .unwrap();
    assert_eq!(r_other.status(), 409);

    // 7. Incompatible schema upgrade while entries exist -> 409 Conflict
    let incompatible_v2 = json!({
        "id": "job-board",
        "version": "2.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Job Board",
        "declarations": {
            "content_types": [
                {
                    "key": "job_posting",
                    "label": "Job Posting",
                    "fields": [
                        { "key": "company", "label": "Company", "type": "short_text", "required": true },
                        { "key": "salary", "label": "Salary", "type": "boolean", "required": false }
                    ]
                }
            ]
        }
    })
    .to_string();
    let r_incompat = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(build_plugin_zip(&incompatible_v2, &wasm))
        .send()
        .await
        .unwrap();
    assert_eq!(r_incompat.status(), 409);
}
