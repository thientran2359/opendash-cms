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

fn minimal_plugin_wasm(return_code: i32) -> Vec<u8> {
    let wat_src = format!(
        r#"(module
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32)
                i32.const {return_code})
        )"#
    );
    wat::parse_str(&wat_src).unwrap()
}

async fn spawn_server(site_root: &Path, port: u16) -> ServerHandle {
    let exe = env!("CARGO_BIN_EXE_opendash");
    let addr = format!("127.0.0.1:{port}");
    let base_url = format!("http://{addr}");
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

async fn setup_and_login_admin(client: &Client, base_url: &str) -> (String, String) {
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
    assert!(resp.status().is_success(), "setup status: {}", resp.status());
    let login_resp = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", base_url)
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(login_resp.status().is_success(), "login status: {}", login_resp.status());
    let cookie = login_resp
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
async fn m6_s01_t01_plugin_revisions_durability_cas_and_quarantine() {
    let dir = TempDir::new("m6_s01_t01");
    let plugins_dir = dir.path.join("plugins");
    let orphan_staging = plugins_dir.join(".staging").join("crashed-run-orphan");
    fs::create_dir_all(&orphan_staging).unwrap();
    fs::write(orphan_staging.join("partial.tmp"), b"incomplete").unwrap();

    let port1 = free_port();
    let mut server = spawn_server(&dir.path, port1).await;
    assert!(
        !orphan_staging.exists(),
        "stale .staging/* must be quarantined on startup"
    );
    assert!(plugins_dir.join("revisions").is_dir());

    let client = Client::new();
    let (cookie, csrf) = setup_and_login_admin(&client, &server.base_url).await;

    // 1. Fail-closed manifest_schema: 99 and min_host_version: 9.0.0
    let wasm_a = minimal_plugin_wasm(0);
    let bad_schema_zip = build_plugin_zip(
        &json!({
            "id": "rev-plugin",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 99,
            "name": "Bad Schema"
        })
        .to_string(),
        &wasm_a,
    );
    let resp = client
        .post(format!("{}/api/admin/plugins", server.base_url))
        .header("Origin", &server.base_url)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(bad_schema_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let high_host_zip = build_plugin_zip(
        &json!({
            "id": "rev-plugin",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 2,
            "min_host_version": "9.0.0",
            "name": "High Host"
        })
        .to_string(),
        &wasm_a,
    );
    let resp = client
        .post(format!("{}/api/admin/plugins", server.base_url))
        .header("Origin", &server.base_url)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(high_host_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // 2. Valid manifest_schema: 2 + min_host_version: 0.1.0 -> auto-enabled with revision & CAS
    let valid_manifest = json!({
        "id": "rev-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "min_host_version": "0.1.0",
        "name": "Revision Plugin"
    })
    .to_string();
    let valid_zip = build_plugin_zip(&valid_manifest, &wasm_a);
    let resp = client
        .post(format!("{}/api/admin/plugins", server.base_url))
        .header("Origin", &server.base_url)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(valid_zip.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let summary: Value = resp.json().await.unwrap();
    assert_eq!(summary["enabled"], true);
    assert_eq!(summary["revision_generation"], 1);
    assert_eq!(summary["cas_version"], 1);
    assert!(summary["active_revision_id"].as_i64().is_some());

    // Verify A-DURABILITY revision directory & .complete marker
    let rev_entries: Vec<_> = fs::read_dir(plugins_dir.join("revisions"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(rev_entries.len(), 1);
    let rev_dir = rev_entries[0].path();
    assert!(rev_dir.join(".complete").is_file());
    assert!(rev_dir.join("manifest.json").is_file());
    assert!(rev_dir.join("plugin.wasm").is_file());

    // 3. Idempotent re-install of exact same bytes succeeds (201)
    let resp = client
        .post(format!("{}/api/admin/plugins", server.base_url))
        .header("Origin", &server.base_url)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(valid_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let summary2: Value = resp.json().await.unwrap();
    assert_eq!(summary2["active_revision_id"], summary["active_revision_id"]);
    assert_eq!(summary2["revision_generation"], 2);
    assert_eq!(summary2["cas_version"], 2);

    // 4. Same (id, version) with different bytes -> 409 Conflict
    let wasm_b = minimal_plugin_wasm(1);
    let conflict_zip = build_plugin_zip(&valid_manifest, &wasm_b);
    let resp = client
        .post(format!("{}/api/admin/plugins", server.base_url))
        .header("Origin", &server.base_url)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(conflict_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);

    // 5. CAS If-Match on disable/enable: stale 412, valid 200
    let resp = client
        .post(format!("{}/api/admin/plugins/rev-plugin/disable", server.base_url))
        .header("Origin", &server.base_url)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 412);

    let resp = client
        .post(format!("{}/api/admin/plugins/rev-plugin/disable", server.base_url))
        .header("Origin", &server.base_url)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "2")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let dis_json: Value = resp.json().await.unwrap();
    assert_eq!(dis_json["cas_version"], 3);

    let resp = client
        .post(format!("{}/api/admin/plugins/rev-plugin/enable", server.base_url))
        .header("Origin", &server.base_url)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "3")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // 6. Corrupt .complete marker on disk -> run fails closed (409 corrupt_revision)
    fs::remove_file(rev_dir.join(".complete")).unwrap();
    let resp = client
        .post(format!("{}/api/admin/plugins/rev-plugin/run", server.base_url))
        .header("Origin", &server.base_url)
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);
    let run_err: Value = resp.json().await.unwrap();
    assert_eq!(run_err["code"], "corrupt_revision");

    // Restart server and confirm revision metadata persists
    drop(server);
    let port2 = free_port();
    server = spawn_server(&dir.path, port2).await;
    let (cookie2, csrf2) = {
        let resp = client
            .post(format!("{}/auth/login", server.base_url))
            .header("Origin", &server.base_url)
            .json(&json!({
                "username": "admin",
                "password": "AdminPassword123!"
            }))
            .send()
            .await
            .unwrap();
        assert!(resp.status().is_success());
        let c = resp
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
            .get(format!("{}/auth/me", server.base_url))
            .header("Cookie", &c)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        (c, me["csrf_token"].as_str().unwrap().to_string())
    };
    let list: Vec<Value> = client
        .get(format!("{}/api/admin/plugins", server.base_url))
        .header("Cookie", &cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"], "rev-plugin");
    assert_eq!(list[0]["cas_version"], 4);
    assert_eq!(list[0]["revision_generation"], 4);
    let _ = csrf2;
}
