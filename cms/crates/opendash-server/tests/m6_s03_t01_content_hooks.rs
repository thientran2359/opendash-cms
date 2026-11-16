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

/// Builds a WASM module that ignores input and writes `output_json` via `opendash_v1:host_write_output`.
fn static_output_wasm(output_json: &str) -> Vec<u8> {
    let bytes = output_json.as_bytes();
    let len = bytes.len();
    let escaped: String = bytes.iter().map(|b| format!("\\{b:02x}")).collect();
    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_write_output" (func $set_output (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{escaped}")
            (func (export "opendash_run") (result i32)
                (drop (call $set_output (i32.const 1024) (i32.const {len})))
                i32.const 0)
        )"#
    );
    wat::parse_str(&wat_src).unwrap()
}

/// Builds a WASM module that attempts `opendash_v1:host_call` (`kv_set`).
/// Returns 1 (fail-closed) when the host rejects `kv_set`, or writes `allow` if it succeeded.
fn kv_write_attempt_wasm() -> Vec<u8> {
    let req = r#"{"op":"kv_set","key":"tamper","value":"1"}"#;
    let out = r#"{"decision":"allow"}"#;
    let req_esc: String = req.as_bytes().iter().map(|b| format!("\\{b:02x}")).collect();
    let out_esc: String = out.as_bytes().iter().map(|b| format!("\\{b:02x}")).collect();
    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $set_output (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{req_esc}")
            (data (i32.const 1536) "{out_esc}")
            (func (export "opendash_run") (result i32)
                (local $rc i32)
                (local.set $rc (call $host_call (i32.const 1024) (i32.const {rlen}) (i32.const 2048) (i32.const 512)))
                (if (i32.lt_s (local.get $rc) (i32.const 7))
                    (then (return (i32.const 1))))
                ;; Response starts with {{"ok":false...}} -> byte at offset 6 is 'f' (102)
                (if (i32.eq (i32.load8_u (i32.const 2054)) (i32.const 102))
                    (then (return (i32.const 1))))
                (drop (call $set_output (i32.const 1536) (i32.const {olen})))
                i32.const 0)
        )"#,
        rlen = req.len(),
        olen = out.len(),
    );
    wat::parse_str(&wat_src).unwrap()
}

fn trap_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32)
                unreachable)
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

    ServerHandle { child }
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
    assert!(resp.status().is_success());

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
    assert!(login.status().is_success());
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
    let csrf = me["csrf_token"].as_str().unwrap().to_string();
    (cookie, csrf)
}

#[tokio::test]
async fn m6_s03_t01_before_save_veto_and_after_save_isolation() {
    let temp = TempDir::new("m6_s03_t01");
    let port = free_port();
    let _server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;

    // 1. Install a veto plugin subscribed to content/before_save WITHOUT `post` in content_read -> should NOT fire on `post`
    let veto_json = r#"{"decision":"veto","code":"blocked","message":"Bad\u0007 title","field_errors":{"title":"No"}}"#;
    let unprivileged_manifest = json!({
        "id": "veto-unpriv",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Veto Unpriv",
        "hooks": ["content/before_save"],
        "hook_priority": 1,
        "permissions": { "content_read": ["page"] }
    })
    .to_string();
    let r = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(build_plugin_zip(&unprivileged_manifest, &static_output_wasm(veto_json)))
        .send()
        .await
        .unwrap();
    let status = r.status();
    let body = r.text().await.unwrap();
    assert_eq!(status, 201, "install veto-unpriv failed: {body}");

    let post_ok = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({ "slug": "hello-world", "title": "Hello", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(post_ok.status(), 201);

    // 2. Install a `content/before_save` plugin with `content_read: ["post"]` that vetoes with control chars
    let priv_veto_manifest = json!({
        "id": "veto-post",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Veto Post",
        "hooks": ["content/before_save"],
        "hook_priority": 5,
        "permissions": { "content_read": ["post"] }
    })
    .to_string();
    let r2 = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(build_plugin_zip(&priv_veto_manifest, &static_output_wasm(veto_json)))
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), 201);

    let vetoed_create = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({ "slug": "blocked-post", "title": "Blocked", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(vetoed_create.status(), 422);
    let veto_body: Value = vetoed_create.json().await.unwrap();
    assert_eq!(veto_body["code"], "blocked");
    assert_eq!(veto_body["message"], "Bad title");
    assert_eq!(veto_body["field_errors"]["title"], "No");

    // Verify vetoed entry was NOT written to DB
    let list_posts: Vec<Value> = client
        .get(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!list_posts.iter().any(|e| e["slug"] == "blocked-post"));

    // 3. Disable `veto-post`, install a plugin that attempts KV write during `content/before_save` -> must fail closed (422)
    let dis = client
        .post(format!("{base_url}/api/admin/plugins/veto-post/disable"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .header("if-match", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(dis.status(), 200);

    let kv_tamper_manifest = json!({
        "id": "kv-tamper",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "KV Tamper",
        "hooks": ["content/before_save"],
        "permissions": { "sqlite": ["kv_write"], "content_read": ["post"] }
    })
    .to_string();
    let r3 = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(build_plugin_zip(&kv_tamper_manifest, &kv_write_attempt_wasm()))
        .send()
        .await
        .unwrap();
    assert_eq!(r3.status(), 201);

    let tamper_create = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({ "slug": "tamper-post", "title": "Tamper", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(tamper_create.status(), 422);

    // Disable `kv-tamper`
    let dis2 = client
        .post(format!("{base_url}/api/admin/plugins/kv-tamper/disable"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .header("if-match", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(dis2.status(), 200);

    // 4. Install an `Allow` before_save plugin AND a trapping `content/after_save` plugin -> Save succeeds (201/200) and commits!
    let allow_manifest = json!({
        "id": "allow-post",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Allow Post",
        "hooks": ["content/before_save"],
        "permissions": { "content_read": ["post"] }
    })
    .to_string();
    client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(build_plugin_zip(
            &allow_manifest,
            &static_output_wasm(r#"{"decision":"allow"}"#),
        ))
        .send()
        .await
        .unwrap();

    let after_trap_manifest = json!({
        "id": "after-trap",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "After Trap",
        "hooks": ["content/after_save"],
        "permissions": { "content_read": ["post"] }
    })
    .to_string();
    client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &csrf)
        .body(build_plugin_zip(&after_trap_manifest, &trap_wasm()))
        .send()
        .await
        .unwrap();

    let allowed_create = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .json(&json!({ "slug": "allowed-post", "title": "Allowed Post", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(allowed_create.status(), 201);
    let created_entry: Value = allowed_create.json().await.unwrap();
    let entry_id = created_entry["id"].as_i64().unwrap();

    // Update also fires hooks and enforces CAS (412 on stale version, 200 on v1)
    let stale_upd = client
        .put(format!("{base_url}/api/admin/content-types/post/entries/{entry_id}"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .header("If-Match", "\"v99\"")
        .json(&json!({ "slug": "allowed-post", "title": "Updated", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_upd.status(), 412);

    let ok_upd = client
        .put(format!("{base_url}/api/admin/content-types/post/entries/{entry_id}"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "slug": "allowed-post", "title": "Updated", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(ok_upd.status(), 200);
}
