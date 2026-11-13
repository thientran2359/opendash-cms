#[path = "../src/plugin/egress.rs"]
mod egress;

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use egress::{is_forbidden_ip, read_bounded_chunks, redact_headers_for_log, verify_resolved_addrs, MAX_HTTP_BODY_BYTES};
use reqwest::Client;
use serde_json::{Value, json};
use sqlx::sqlite::SqlitePoolOptions;
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
async fn m6_s07_t01_adversarial_isolation_ssrf_trap_disable_and_zero_late_writes() {
    // 1. SSRF / DNS rebinding / chunked cap / redaction assertions
    assert!(is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))));
    assert!(is_forbidden_ip(IpAddr::V6("::ffff:127.0.0.1".parse().unwrap())));
    assert!(is_forbidden_ip(IpAddr::V6("64:ff9b::10.0.0.1".parse().unwrap())));
    assert!(verify_resolved_addrs(
        &[
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)), 443),
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 443),
        ],
        false,
    )
    .is_err());
    let oversized = vec![b'z'; MAX_HTTP_BODY_BYTES + 1];
    assert!(read_bounded_chunks(&oversized[..], MAX_HTTP_BODY_BYTES).is_err());
    let mut hdrs = BTreeMap::new();
    hdrs.insert("Authorization".into(), "Bearer top-secret".into());
    hdrs.insert("Set-Cookie".into(), "sid=secret".into());
    let redacted = redact_headers_for_log(&hdrs);
    assert_eq!(redacted.get("Authorization").unwrap(), "[REDACTED]");
    assert_eq!(redacted.get("Set-Cookie").unwrap(), "[REDACTED]");

    // 2. Live server adversarial isolation
    let dir = TempDir::new("m6_s07");
    let (srv, base_url) = spawn_server(&dir.path).await;
    let client = Client::new();

    client
        .post(format!("{base_url}/auth/setup"))
        .header("Origin", &base_url)
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    let login_res = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", &base_url)
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    let cookie = extract_cookie(&login_res);
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

    // Plugin A: attempts kv_set("late", "should_not_persist") and then traps (`unreachable`)
    let trap_kv_wat = r#"(module
        (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "{\"op\":\"kv_set\",\"key\":\"late\",\"value\":\"bad\"}")
        (func (export "opendash_run") (result i32)
            (drop (call $host_call (i32.const 0) (i32.const 44) (i32.const 512) (i32.const 512)))
            (unreachable)
        )
    )"#;
    let zip_trap = build_plugin_zip(
        json!({
            "id": "trap-kv-plugin",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 2,
            "name": "Trap KV Plugin",
            "description": "Adversarial trap after kv_set",
            "entrypoint": "plugin.wasm",
            "permissions": { "sqlite": ["kv_write"], "http": [] },
            "actions": [{ "id": "act", "title": "Act", "required_role": "admin", "capabilities": ["kv_write"] }]
        }),
        &wat::parse_str(trap_kv_wat).unwrap(),
    );
    let r = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("Content-Type", "application/zip")
        .header("X-CSRF-Token", &csrf)
        .body(zip_trap)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 201);

    // Run trapping plugin -> fails cleanly, server stays up
    let run_trap = client
        .post(format!("{base_url}/api/admin/plugins/trap-kv-plugin/run"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert!(!run_trap.status().is_success());

    // Plugin B: infinite loop (fuel exhaustion)
    let loop_wat = r#"(module
        (memory (export "memory") 1)
        (func (export "opendash_run") (result i32)
            (loop $l (br $l))
            (i32.const 0)
        )
    )"#;
    let zip_loop = build_plugin_zip(
        json!({
            "id": "fuel-loop-plugin",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 2,
            "name": "Fuel Loop",
            "description": "Infinite loop",
            "entrypoint": "plugin.wasm",
            "permissions": { "sqlite": [], "http": [] }
        }),
        &wat::parse_str(loop_wat).unwrap(),
    );
    let r_loop = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("Content-Type", "application/zip")
        .header("X-CSRF-Token", &csrf)
        .body(zip_loop)
        .send()
        .await
        .unwrap();
    assert_eq!(r_loop.status().as_u16(), 201);

    let run_loop = client
        .post(format!("{base_url}/api/admin/plugins/fuel-loop-plugin/run"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert!(!run_loop.status().is_success());

    // Disable trap-kv-plugin -> subsequent run & action must NOT execute
    let dis = client
        .post(format!("{base_url}/api/admin/plugins/trap-kv-plugin/disable"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();
    assert!(dis.status().is_success());

    let run_disabled = client
        .post(format!("{base_url}/api/admin/plugins/trap-kv-plugin/run"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert!(!run_disabled.status().is_success());

    let act_disabled = client
        .post(format!("{base_url}/api/admin/plugins/trap-kv-plugin/actions/act"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert!(!act_disabled.status().is_success());

    // Admin & Public endpoints remain healthy and responsive
    assert!(client.get(format!("{base_url}/api/health")).send().await.unwrap().status().is_success());
    assert!(client.get(format!("{base_url}/")).send().await.unwrap().status().is_success());

    // Stop server, wait briefly, reopen SQLite DB, and verify ZERO late KV writes exist
    drop(srv);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let db_path = dir.path.join("database").join("opendash.sqlite");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{}", db_path.display()))
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_kv WHERE plugin_id = 'trap-kv-plugin'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "trapped plugin must leave zero KV writes in SQLite");
}
