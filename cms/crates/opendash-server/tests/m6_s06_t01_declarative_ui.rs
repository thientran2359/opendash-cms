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
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn build_plugin_zip(manifest: Value, wasm_bytes: &[u8]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("plugin.wasm", opts).unwrap();
        zip.write_all(wasm_bytes).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

fn extract_cookie(res: &reqwest::Response) -> String {
    res.headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|s| s.split(';').next())
        .collect::<Vec<_>>()
        .join("; ")
}

async fn spawn_server(site_root: &Path) -> (ServerHandle, String) {
    let port = free_port();
    let bind = format!("127.0.0.1:{port}");
    let base_url = format!("http://{bind}");
    let exe = env!("CARGO_BIN_EXE_opendash");
    let child = Command::new(exe)
        .env("OPENDASH_SITE_ROOT", site_root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn opendash");
    let handle = ServerHandle { child };
    let client = Client::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(r) = client.get(format!("{base_url}/api/health")).send().await {
            if r.status().is_success() {
                return (handle, base_url);
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server did not become healthy in time");
}

#[tokio::test]
async fn m6_s06_t01_declarative_ui_schema_rbac_csrf_and_forged_action() {
    let dir = TempDir::new("m6_s06");
    let (_srv, base_url) = spawn_server(&dir.path).await;
    let client = Client::new();

    // 1. Setup admin & editor users
    let setup_res = client
        .post(format!("{base_url}/auth/setup"))
        .header("Origin", &base_url)
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(setup_res.status().is_success());
    let login_admin = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", &base_url)
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login_admin.status().as_u16(), 200);
    let admin_cookie = extract_cookie(&login_admin);

    let me: Value = client
        .get(format!("{base_url}/auth/me"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let csrf = me["csrf_token"].as_str().unwrap().to_string();

    // Create editor user
    let create_editor = client
        .post(format!("{base_url}/api/admin/users"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword123!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert!(create_editor.status().is_success());

    // 2. Install plugin with admin-only & editor-allowed actions
    let wasm = wat::parse_str(
        r#"(module
            (import "opendash_v1" "host_write_output" (func $write (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "{\"ok\":true}")
            (func (export "opendash_run") (result i32)
                (drop (call $write (i32.const 0) (i32.const 11)))
                (i32.const 0)
            )
        )"#,
    )
    .unwrap();
    let manifest = json!({
        "id": "ui-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "UI Plugin",
        "description": "Declarative UI test plugin",
        "entrypoint": "plugin.wasm",
        "permissions": { "sqlite": [], "http": [] },
        "actions": [
            { "id": "editor_ping", "title": "Editor Ping", "required_role": "editor", "capabilities": [] },
            { "id": "admin_sync", "title": "Admin Sync", "required_role": "admin", "capabilities": [] }
        ]
    });
    let zip = build_plugin_zip(manifest, &wasm);
    let install_res = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("Content-Type", "application/zip")
        .header("X-CSRF-Token", &csrf)
        .body(zip)
        .send()
        .await
        .unwrap();
    let install_status = install_res.status().as_u16();
    let install_body = install_res.text().await.unwrap();
    assert_eq!(install_status, 201, "install failed: {install_body}");

    // 3. Unauthenticated ui-schema -> 401
    let anon_res = client
        .get(format!("{base_url}/api/admin/plugins/ui-plugin/ui-schema"))
        .send()
        .await
        .unwrap();
    assert_eq!(anon_res.status().as_u16(), 401);

    // 4. Admin ui-schema -> 200 (sees both actions)
    let admin_schema: Value = client
        .get(format!("{base_url}/api/admin/plugins/ui-plugin/ui-schema"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(admin_schema["schema_version"], 1);
    assert_eq!(admin_schema["screens"][0]["nodes"].as_array().unwrap().len(), 2);

    // 5. Missing plugin -> 404
    let missing_res = client
        .get(format!("{base_url}/api/admin/plugins/nonexistent/ui-schema"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .send()
        .await
        .unwrap();
    assert_eq!(missing_res.status().as_u16(), 404);

    // 6. Editor login: ui-schema shows only editor_ping; admin_sync & missing CSRF -> 403; forged action -> 404
    let login_res = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", &base_url)
        .json(&json!({ "username": "editor1", "password": "EditorPassword123!" }))
        .send()
        .await
        .unwrap();
    assert!(login_res.status().is_success());
    let editor_cookie = extract_cookie(&login_res);
    let editor_me: Value = client
        .get(format!("{base_url}/auth/me"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let editor_csrf = editor_me["csrf_token"].as_str().unwrap().to_string();

    let editor_schema: Value = client
        .get(format!("{base_url}/api/admin/plugins/ui-plugin/ui-schema"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(editor_schema["screens"][0]["nodes"].as_array().unwrap().len(), 1);

    // Missing CSRF -> 403
    let no_csrf_res = client
        .post(format!("{base_url}/api/admin/plugins/ui-plugin/actions/editor_ping"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(no_csrf_res.status().as_u16(), 403);

    // Editor calling admin_sync -> 403
    let forbidden_res = client
        .post(format!("{base_url}/api/admin/plugins/ui-plugin/actions/admin_sync"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(forbidden_res.status().as_u16(), 403);

    // Forged action -> 404
    let forged_res = client
        .post(format!("{base_url}/api/admin/plugins/ui-plugin/actions/forged_action"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(forged_res.status().as_u16(), 404);

    // Editor calling editor_ping -> 200
    let ok_res = client
        .post(format!("{base_url}/api/admin/plugins/ui-plugin/actions/editor_ping"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({ "payload": "hello" }))
        .send()
        .await
        .unwrap();
    assert_eq!(ok_res.status().as_u16(), 200);
}
