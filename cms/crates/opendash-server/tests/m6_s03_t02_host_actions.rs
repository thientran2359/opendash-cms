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
use sqlx::SqlitePool;
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

/// Builds a WASM module that calls `opendash_v1:host_call` with `req_json`,
/// fails (returns 1) if `"ok":false` is in the response, or writes `resp` as output.
fn host_call_wasm(req_json: &str) -> Vec<u8> {
    let req_esc: String = req_json
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect();
    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $set_output (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{req_esc}")
            (func (export "opendash_run") (result i32)
                (local $n i32)
                (local.set $n (call $host_call (i32.const 1024) (i32.const {rlen}) (i32.const 4096) (i32.const 2048)))
                (if (i32.lt_s (local.get $n) (i32.const 7))
                    (then (return (i32.const 1))))
                (if (i32.eq (i32.load8_u (i32.const 4102)) (i32.const 102))
                    (then
                        (drop (call $set_output (i32.const 4096) (local.get $n)))
                        (return (i32.const 1))))
                (drop (call $set_output (i32.const 4096) (local.get $n)))
                i32.const 0)
        )"#,
        rlen = req_json.len(),
    );
    wat::parse_str(&wat_src).unwrap()
}

/// Builds a WASM module that performs two sequential `host_call`s (`req1` then `req2`),
/// returning 1 if either returns `"ok":false`.
fn two_host_calls_wasm(req1: &str, req2: &str) -> Vec<u8> {
    let esc1: String = req1.as_bytes().iter().map(|b| format!("\\{b:02x}")).collect();
    let esc2: String = req2.as_bytes().iter().map(|b| format!("\\{b:02x}")).collect();
    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $set_output (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{esc1}")
            (data (i32.const 4096) "{esc2}")
            (func (export "opendash_run") (result i32)
                (local $n i32)
                (local.set $n (call $host_call (i32.const 1024) (i32.const {l1}) (i32.const 8192) (i32.const 2048)))
                (if (i32.lt_s (local.get $n) (i32.const 7))
                    (then (return (i32.const 1))))
                (if (i32.eq (i32.load8_u (i32.const 8198)) (i32.const 102))
                    (then (return (i32.const 1))))
                (local.set $n (call $host_call (i32.const 4096) (i32.const {l2}) (i32.const 8192) (i32.const 2048)))
                (if (i32.lt_s (local.get $n) (i32.const 7))
                    (then (return (i32.const 1))))
                (if (i32.eq (i32.load8_u (i32.const 8198)) (i32.const 102))
                    (then (return (i32.const 1))))
                (drop (call $set_output (i32.const 8192) (local.get $n)))
                i32.const 0)
        )"#,
        l1 = req1.len(),
        l2 = req2.len(),
    );
    wat::parse_str(&wat_src).unwrap()
}

async fn spawn_server(root: &Path, port: u16) -> ServerHandle {
    let bin = env!("CARGO_BIN_EXE_opendash");
    let mut cmd = Command::new(bin);
    cmd.env("OPENDASH_SITE_ROOT", root)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .env("OPENDASH_PLUGIN_ALLOW_LOOPBACK", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = cmd.spawn().unwrap();
    let handle = ServerHandle { child };

    let client = Client::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if Instant::now() > deadline {
            panic!("server failed to become ready");
        }
        if let Ok(resp) = client
            .get(format!("http://127.0.0.1:{port}/api/health"))
            .send()
            .await
        {
            if resp.status().is_success() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    handle
}

async fn setup_and_login(client: &Client, base_url: &str) -> (String, String) {
    let r = client
        .post(format!("{base_url}/auth/setup"))
        .header("Origin", base_url)
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success());
    let r = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", base_url)
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let cookie = r
        .headers()
        .get("set-cookie")
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

async fn create_and_login_editor(
    client: &Client,
    base_url: &str,
    admin_cookie: &str,
    admin_csrf: &str,
) -> (String, String) {
    let r = client
        .post(format!("{base_url}/api/admin/users"))
        .header("Cookie", admin_cookie)
        .header("Origin", base_url)
        .header("x-csrf-token", admin_csrf)
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword123!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);

    let r = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", base_url)
        .json(&json!({ "username": "editor1", "password": "EditorPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let cookie = r
        .headers()
        .get("set-cookie")
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m6_s03_t02_host_actions_rbac_fencing_and_leases() {
    let temp = TempDir::new("m6_s03_t02");
    let port = free_port();
    let _server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_and_login(&client, &base_url).await;
    let (editor_cookie, editor_csrf) =
        create_and_login_editor(&client, &base_url, &admin_cookie, &admin_csrf).await;

    // 1. Unknown op is default-denied
    let unknown_manifest = json!({
        "id": "unknown-op-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Unknown Op",
        "actions": [{ "id": "probe", "required_role": "admin", "capabilities": [] }]
    })
    .to_string();
    let r = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &admin_csrf)
        .body(build_plugin_zip(
            &unknown_manifest,
            &host_call_wasm(r#"{"op":"raw_sql","query":"SELECT 1"}"#),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);

    let act_unknown = client
        .post(format!("{base_url}/api/admin/plugins/unknown-op-plugin/actions/probe"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(act_unknown.status(), 400);

    // 2. HTTP loopback to /api/admin is blocked even with allow_loopback=1
    let loopback_manifest = json!({
        "id": "loopback-admin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Loopback Admin",
        "permissions": { "http": [base_url.clone()] },
        "actions": [{ "id": "call-admin", "required_role": "admin", "capabilities": ["http"] }]
    })
    .to_string();
    let r = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &admin_csrf)
        .body(build_plugin_zip(
            &loopback_manifest,
            &host_call_wasm(&format!(
                r#"{{"op":"http_request","method":"GET","url":"{base_url}/api/admin/plugins"}}"#
            )),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);
    let act_loop = client
        .post(format!("{base_url}/api/admin/plugins/loopback-admin/actions/call-admin"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(act_loop.status(), 400);

    // 3. Mixed KV + content writes rejected in manifest & runtime
    let mixed_manifest = json!({
        "id": "mixed-manifest",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Mixed",
        "permissions": { "sqlite": ["kv_write"], "content_write": ["post"] },
        "actions": [{ "id": "mix", "required_role": "admin", "capabilities": ["kv_write", "content_write"] }]
    })
    .to_string();
    let r = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &admin_csrf)
        .body(build_plugin_zip(&mixed_manifest, &host_call_wasm("{}")))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);

    // Runtime mixed check (manifest only declares content_write, guest tries kv_set + content_save)
    let mixed_rt_manifest = json!({
        "id": "mixed-rt",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Mixed RT",
        "permissions": { "sqlite": ["kv_write"], "content_write": ["post"] },
        "actions": [{ "id": "save-post", "required_role": "editor", "capabilities": ["content_write"] }]
    })
    .to_string();
    let r = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &admin_csrf)
        .body(build_plugin_zip(
            &mixed_rt_manifest,
            &two_host_calls_wasm(
                r#"{"op":"content_save","type_key":"post","slug":"a","title":"A","values":{}}"#,
                r#"{"op":"kv_set","key":"k","value":"v"}"#,
            ),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);
    let act_mix = client
        .post(format!("{base_url}/api/admin/plugins/mixed-rt/actions/save-post"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(act_mix.status(), 400);

    // 4. Single content_save command via canonical Save orchestration + RBAC admission check
    let content_manifest = json!({
        "id": "content-action-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Content Action",
        "permissions": { "content_write": ["post"] },
        "actions": [
            { "id": "admin-only-save", "required_role": "admin", "capabilities": ["content_write"] },
            { "id": "editor-save", "required_role": "editor", "capabilities": ["content_write"] }
        ]
    })
    .to_string();
    let r = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &admin_csrf)
        .body(build_plugin_zip(
            &content_manifest,
            &host_call_wasm(
                r#"{"op":"content_save","type_key":"post","slug":"action-post","title":"Action Post","values":{}}"#,
            ),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);

    // Editor cannot invoke admin-only-save -> 403
    let editor_denied = client
        .post(format!("{base_url}/api/admin/plugins/content-action-plugin/actions/admin-only-save"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &editor_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_denied.status(), 403);

    // Editor CAN invoke editor-save -> 200 and creates entry!
    let editor_ok = client
        .post(format!("{base_url}/api/admin/plugins/content-action-plugin/actions/editor-save"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &editor_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_ok.status(), 200);
    let body: Value = editor_ok.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["entry"]["slug"], "action-post");

    // 5. Commit fencing & lease cleanup: start a slow HTTP+KV action, disable plugin mid-flight,
    // and verify (a) concurrent re-entrant call gets 409 recursive_dispatch,
    // (b) mid-flight disable fences the KV commit (409),
    // (c) lease is cleaned up after failure so re-enabling allows subsequent actions!
    let (tx_connected, rx_connected) = tokio::sync::oneshot::channel::<()>();
    let (tx_pause, rx_pause) = tokio::sync::oneshot::channel::<()>();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mock_port = listener.local_addr().unwrap().port();
    let mock_origin = format!("http://127.0.0.1:{mock_port}");
    tokio::spawn(async move {
        let mut tx_conn_opt = Some(tx_connected);
        let mut rx_opt = Some(rx_pause);
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            if let Some(tx_c) = tx_conn_opt.take() {
                let _ = tx_c.send(());
            }
            if let Some(rx) = rx_opt.take() {
                let _ = rx.await;
            }
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await;
            let _ = stream.flush().await;
            let _ = stream.shutdown().await;
        }
    });

    let slow_manifest = json!({
        "id": "slow-fenced-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 2,
        "name": "Slow Fenced",
        "permissions": { "sqlite": ["kv_write"], "http": [mock_origin.clone()] },
        "actions": [{ "id": "sync-kv", "required_role": "admin", "capabilities": ["http", "kv_write"] }]
    })
    .to_string();
    let r = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("content-type", "application/zip")
        .header("x-csrf-token", &admin_csrf)
        .body(build_plugin_zip(
            &slow_manifest,
            &two_host_calls_wasm(
                &format!(r#"{{"op":"http_request","method":"GET","url":"{mock_origin}/ping"}}"#),
                r#"{"op":"kv_set","key":"fenced","value":"should_not_commit"}"#,
            ),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);

    let client_bg = client.clone();
    let url_bg = format!("{base_url}/api/admin/plugins/slow-fenced-plugin/actions/sync-kv");
    let cookie_bg = admin_cookie.clone();
    let origin_bg = base_url.clone();
    let csrf_bg = admin_csrf.clone();
    let inflight = tokio::spawn(async move {
        client_bg
            .post(url_bg)
            .header("Cookie", cookie_bg)
            .header("Origin", origin_bg)
            .header("x-csrf-token", csrf_bg)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
    });

    if tokio::time::timeout(Duration::from_secs(3), rx_connected)
        .await
        .is_err()
    {
        let _ = tx_pause.send(());
        let r = inflight.await.unwrap();
        let st = r.status();
        let bd = r.text().await.unwrap();
        panic!("rx_connected timed out! inflight={st} {bd}");
    }

    // Concurrent re-entrant dispatch on same plugin -> 409 recursive_dispatch
    let reentrant = client
        .post(format!("{base_url}/api/admin/plugins/slow-fenced-plugin/actions/sync-kv"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(reentrant.status(), 409);

    // Disable plugin mid-flight -> generation fencing rejects commit
    let dis = client
        .post(format!("{base_url}/api/admin/plugins/slow-fenced-plugin/disable"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &admin_csrf)
        .header("if-match", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(dis.status(), 200);

    let _ = tx_pause.send(());
    let inflight_resp = inflight.await.unwrap();
    let inflight_status = inflight_resp.status();
    let inflight_body = inflight_resp.text().await.unwrap();
    assert_eq!(inflight_status, 409, "inflight_body: {inflight_body}");

    // Verify nothing was committed to plugin_kv
    let pool = SqlitePool::connect(&format!("sqlite://{}", temp.path.join("database/opendash.sqlite").display()))
        .await
        .unwrap();
    let kv_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM plugin_kv WHERE plugin_id = 'slow-fenced-plugin' AND key = 'fenced'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(kv_count, 0);

    // Re-enable and verify lease was cleaned up so action can run again and commit!
    let en = client
        .post(format!("{base_url}/api/admin/plugins/slow-fenced-plugin/enable"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &admin_csrf)
        .header("if-match", "2")
        .send()
        .await
        .unwrap();
    assert_eq!(en.status(), 200);

    let after_cleanup = client
        .post(format!("{base_url}/api/admin/plugins/slow-fenced-plugin/actions/sync-kv"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("x-csrf-token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(after_cleanup.status(), 200);
}
