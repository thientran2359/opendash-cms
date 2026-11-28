use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use reqwest::{Client, StatusCode, header, redirect::Policy};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use tokio::sync::{mpsc, oneshot};
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
        #[cfg(unix)]
        let base = std::env::temp_dir().canonicalize().unwrap();
        #[cfg(not(unix))]
        let base = std::env::temp_dir();
        let path = base.join(format!("{prefix}_{}_{}", std::process::id(), nanos));
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

async fn spawn_server(site_root: &Path, port: u16) -> ServerHandle {
    let exe = env!("CARGO_BIN_EXE_opendash");
    let base_url = format!("http://127.0.0.1:{port}");
    let log_path = site_root.join("server.log");
    let log = fs::File::create(&log_path).unwrap();
    let child = Command::new(exe)
        .env("OPENDASH_SITE_ROOT", site_root)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .env("OPENDASH_PLUGIN_ALLOW_LOOPBACK", "1")
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log))
        .spawn()
        .expect("spawn opendash server");

    let client = Client::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await
            && resp.status().is_success()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "server failed to become healthy; log={}",
            fs::read_to_string(&log_path).unwrap_or_default()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    ServerHandle { child }
}

async fn setup_and_login(client: &Client, base_url: &str) -> (String, String) {
    let setup = client
        .post(format!("{base_url}/auth/setup"))
        .header("Origin", base_url)
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    login_user(client, base_url, "admin", "AdminPassword123!").await
}

async fn login_user(
    client: &Client,
    base_url: &str,
    username: &str,
    password: &str,
) -> (String, String) {
    let login = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", base_url)
        .json(&json!({
            "username": username,
            "password": password
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = login.json().await.unwrap();
    let csrf = body["csrf_token"].as_str().unwrap().to_owned();
    (cookie, csrf)
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

async fn install_plugin(
    client: &Client,
    base_url: &str,
    cookie: &str,
    csrf: &str,
    mut manifest: Value,
    wasm: &[u8],
) {
    if manifest.get("min_host_version").is_none()
        && let Some(obj) = manifest.as_object_mut()
    {
        obj.insert("min_host_version".to_owned(), json!("0.1.0"));
    }
    let resp = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", cookie)
        .header("Origin", base_url)
        .header("X-CSRF-Token", csrf)
        .header("Content-Type", "application/zip")
        .body(build_plugin_zip(&manifest.to_string(), wasm))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let text = resp.text().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "install failed: {text}");
}

async fn open_readonly_db(site_root: &Path) -> SqlitePool {
    let db_path = site_root.join("database").join("opendash.sqlite");
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    SqlitePool::connect_with(opts).await.unwrap()
}

async fn wait_for_kv(db: &SqlitePool, plugin_id: &str, key: &str, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline {
        let row: Option<String> =
            sqlx::query_scalar("SELECT value_json FROM plugin_kv WHERE plugin_id = ? AND key = ?")
                .bind(plugin_id)
                .bind(key)
                .fetch_optional(db)
                .await
                .unwrap_or(None);
        if row.as_deref() == Some(expected) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    panic!("timed out waiting for plugin_kv {plugin_id}/{key} == {expected}");
}

fn wat_escape(s: &str) -> String {
    s.as_bytes().iter().map(|b| format!("\\{b:02x}")).collect()
}

fn decode_hex_bytes(hex: &str) -> Vec<u8> {
    let bytes = hex.as_bytes();
    assert_eq!(bytes.len() % 2, 0, "odd hex len: {hex}");
    (0..bytes.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(std::str::from_utf8(&bytes[i..i + 2]).unwrap(), 16).unwrap())
        .collect()
}

fn static_output_wasm(out: &str) -> Vec<u8> {
    let out_esc = wat_escape(out);
    wat::parse_str(format!(
        r#"(module
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{out_esc}")
            (func (export "opendash_run") (result i32)
                (drop (call $out (i32.const 1024) (i32.const {})))
                (i32.const 0)
            )
        )"#,
        out.len()
    ))
    .unwrap()
}

fn kv_set_wasm(key: &str, value: &str) -> Vec<u8> {
    let req = json!({ "op": "kv_set", "key": key, "value": value }).to_string();
    let out = r#"{"ok":true}"#;
    wat::parse_str(format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{}")
            (data (i32.const 2048) "{}")
            (func (export "opendash_run") (result i32)
                (drop (call $call (i32.const 1024) (i32.const {}) (i32.const 4096) (i32.const 512)))
                (drop (call $out (i32.const 2048) (i32.const {})))
                (i32.const 0)
            )
        )"#,
        wat_escape(&req),
        wat_escape(out),
        req.len(),
        out.len(),
    ))
    .unwrap()
}

/// Calls `plugin_secret_get("api_key")` and `plugin_settings_get`, hex-encodes both JSON responses
/// into a POST body `{"secret_hex":"...","settings_hex":"..."}` sent to `webhook_url`.
fn cuj1_secret_webhook_wasm(secret_key: &str, webhook_url: &str) -> Vec<u8> {
    let sec_req = json!({ "op": "plugin_secret_get", "key": secret_key }).to_string();
    let set_req = json!({ "op": "plugin_settings_get" }).to_string();
    let p1 = format!(
        r#"{{"op":"http_request","method":"POST","url":"{webhook_url}","body":"{{\"secret_hex\":\""#
    );
    let p2 = r#"\",\"settings_hex\":\""#;
    let p3 = r#"\"}"}"#;
    let ok_out = r#"{"ok":true}"#;

    let mut cur = 0usize;
    let mut segs = String::new();
    let mut add = |s: &str| {
        let off = cur;
        segs.push_str(&format!("(data (i32.const {off}) \"{}\")\n", wat_escape(s)));
        cur += s.len() + 4;
        (off, s.len())
    };
    let (sec_off, sec_len) = add(&sec_req);
    let (set_off, set_len) = add(&set_req);
    let (p1_off, p1_len) = add(&p1);
    let (p2_off, p2_len) = add(p2);
    let (p3_off, p3_len) = add(p3);
    let (out_off, out_len) = add(ok_out);
    assert!(cur < 4096);

    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            {segs}
            (func $nibble (param $v i32) (result i32)
                (if (result i32) (i32.lt_u (local.get $v) (i32.const 10))
                    (then (i32.add (local.get $v) (i32.const 48)))
                    (else (i32.add (local.get $v) (i32.const 87)))
                )
            )
            (func $hex (param $src i32) (param $len i32) (param $dst i32) (result i32)
                (local $i i32) (local $b i32) (local $o i32)
                (local.set $i (i32.const 0))
                (loop $lp
                    (if (i32.lt_u (local.get $i) (local.get $len))
                        (then
                            (local.set $b (i32.load8_u (i32.add (local.get $src) (local.get $i))))
                            (local.set $o (i32.add (local.get $dst) (i32.mul (local.get $i) (i32.const 2))))
                            (i32.store8 (local.get $o) (call $nibble (i32.shr_u (local.get $b) (i32.const 4))))
                            (i32.store8 (i32.add (local.get $o) (i32.const 1)) (call $nibble (i32.and (local.get $b) (i32.const 15))))
                            (local.set $i (i32.add (local.get $i) (i32.const 1)))
                            (br $lp)
                        )
                    )
                )
                (i32.mul (local.get $len) (i32.const 2))
            )
            (func (export "opendash_run") (result i32)
                (local $n1 i32) (local $n2 i32) (local $pos i32)
                (local.set $n1 (call $call (i32.const {sec_off}) (i32.const {sec_len}) (i32.const 8192) (i32.const 4096)))
                (if (i32.lt_s (local.get $n1) (i32.const 1)) (then (return (i32.const 1))))
                (local.set $n2 (call $call (i32.const {set_off}) (i32.const {set_len}) (i32.const 16384) (i32.const 4096)))
                (if (i32.lt_s (local.get $n2) (i32.const 1)) (then (return (i32.const 2))))
                (local.set $pos (i32.const 32768))
                (memory.copy (local.get $pos) (i32.const {p1_off}) (i32.const {p1_len}))
                (local.set $pos (i32.add (local.get $pos) (i32.const {p1_len})))
                (local.set $pos (i32.add (local.get $pos) (call $hex (i32.const 8192) (local.get $n1) (local.get $pos))))
                (memory.copy (local.get $pos) (i32.const {p2_off}) (i32.const {p2_len}))
                (local.set $pos (i32.add (local.get $pos) (i32.const {p2_len})))
                (local.set $pos (i32.add (local.get $pos) (call $hex (i32.const 16384) (local.get $n2) (local.get $pos))))
                (memory.copy (local.get $pos) (i32.const {p3_off}) (i32.const {p3_len}))
                (local.set $pos (i32.add (local.get $pos) (i32.const {p3_len})))
                (drop (call $call (i32.const 32768) (i32.sub (local.get $pos) (i32.const 32768)) (i32.const 65536) (i32.const 4096)))
                (drop (call $out (i32.const {out_off}) (i32.const {out_len})))
                (i32.const 0)
            )
        )"#
    );
    wat::parse_str(&wat_src).unwrap()
}

/// Calls `http_request` POST to `url` (which hangs on the mock server).
fn http_post_wasm(url: &str) -> Vec<u8> {
    let req =
        json!({ "op": "http_request", "method": "POST", "url": url, "body": "ping" }).to_string();
    let out = r#"{"ok":true}"#;
    wat::parse_str(format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{}")
            (data (i32.const 2048) "{}")
            (func (export "opendash_run") (result i32)
                (drop (call $call (i32.const 1024) (i32.const {}) (i32.const 4096) (i32.const 512)))
                (drop (call $out (i32.const 2048) (i32.const {})))
                (i32.const 0)
            )
        )"#,
        wat_escape(&req),
        wat_escape(out),
        req.len(),
        out.len(),
    ))
    .unwrap()
}

/// Exercises:
/// 1) kv_set("big_ml", multiline > 150B with '\n') + kv_get with cap=8 -> verifies rc == -2
///    and reads full payload via host_last_resp_len + host_read_last_resp without re-calling host_call.
/// 2) kv_set("k_del", "gone"), kv_set("k_keep", "alive"), kv_delete("k_del"),
///    kv_get("k_del"), kv_list({"prefix":"k_","limit":10}).
/// Returns JSON `{"readback_hex":"...","after_del_hex":"...","list_hex":"..."}` via hex encoding.
fn s01_abi_readback_and_kv_tombstone_wasm() -> Vec<u8> {
    let long_ml = format!("line1\nline2\nline3\n{}", "x".repeat(160));
    let r_set_big = json!({ "op": "kv_set", "key": "big_ml", "value": long_ml }).to_string();
    let r_get_big = json!({ "op": "kv_get", "key": "big_ml" }).to_string();
    let r_set_del = json!({ "op": "kv_set", "key": "k_del", "value": "gone" }).to_string();
    let r_set_keep = json!({ "op": "kv_set", "key": "k_keep", "value": "alive" }).to_string();
    let r_del = json!({ "op": "kv_delete", "key": "k_del" }).to_string();
    let r_get_del = json!({ "op": "kv_get", "key": "k_del" }).to_string();
    let r_list = json!({ "op": "kv_list", "prefix": "k_", "limit": 10 }).to_string();

    let mut cur = 0usize;
    let mut segs = String::new();
    let mut add = |s: &str| {
        let off = cur;
        segs.push_str(&format!("(data (i32.const {off}) \"{}\")\n", wat_escape(s)));
        cur += s.len() + 4;
        (off, s.len())
    };
    let (sb_off, sb_len) = add(&r_set_big);
    let (gb_off, gb_len) = add(&r_get_big);
    let (sd_off, sd_len) = add(&r_set_del);
    let (sk_off, sk_len) = add(&r_set_keep);
    let (del_off, del_len) = add(&r_del);
    let (gd_off, gd_len) = add(&r_get_del);
    let (lst_off, lst_len) = add(&r_list);
    let (o1_off, o1_len) = add(r#"{"readback_hex":""#);
    let (o2_off, o2_len) = add(r#"","after_del_hex":""#);
    let (o3_off, o3_len) = add(r#"","list_hex":""#);
    let (o4_off, o4_len) = add(r#""}"#);
    assert!(cur < 4096);

    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_last_resp_len" (func $last_len (result i32)))
            (import "opendash_v1" "host_read_last_resp" (func $read_last (param i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            {segs}
            (func $nibble (param $v i32) (result i32)
                (if (result i32) (i32.lt_u (local.get $v) (i32.const 10))
                    (then (i32.add (local.get $v) (i32.const 48)))
                    (else (i32.add (local.get $v) (i32.const 87)))
                )
            )
            (func $hex (param $src i32) (param $len i32) (param $dst i32) (result i32)
                (local $i i32) (local $b i32) (local $o i32)
                (local.set $i (i32.const 0))
                (loop $lp
                    (if (i32.lt_u (local.get $i) (local.get $len))
                        (then
                            (local.set $b (i32.load8_u (i32.add (local.get $src) (local.get $i))))
                            (local.set $o (i32.add (local.get $dst) (i32.mul (local.get $i) (i32.const 2))))
                            (i32.store8 (local.get $o) (call $nibble (i32.shr_u (local.get $b) (i32.const 4))))
                            (i32.store8 (i32.add (local.get $o) (i32.const 1)) (call $nibble (i32.and (local.get $b) (i32.const 15))))
                            (local.set $i (i32.add (local.get $i) (i32.const 1)))
                            (br $lp)
                        )
                    )
                )
                (i32.mul (local.get $len) (i32.const 2))
            )
            (func (export "opendash_run") (result i32)
                (local $rc i32) (local $rb_len i32) (local $gd_n i32) (local $lst_n i32) (local $pos i32)
                (if (i32.lt_s (call $call (i32.const {sb_off}) (i32.const {sb_len}) (i32.const 8192) (i32.const 512)) (i32.const 1)) (then (return (i32.const 10))))
                ;; Deliberate tiny resp_max = 8 -> must return -2
                (local.set $rc (call $call (i32.const {gb_off}) (i32.const {gb_len}) (i32.const 8192) (i32.const 8)))
                (if (i32.ne (local.get $rc) (i32.const -2)) (then (return (i32.const 11))))
                (local.set $rb_len (call $last_len))
                (if (i32.le_s (local.get $rb_len) (i32.const 150)) (then (return (i32.const 12))))
                (local.set $rc (call $read_last (i32.const 12288) (local.get $rb_len)))
                (if (i32.ne (local.get $rc) (local.get $rb_len)) (then (return (i32.const 13))))

                (if (i32.lt_s (call $call (i32.const {sd_off}) (i32.const {sd_len}) (i32.const 8192) (i32.const 512)) (i32.const 1)) (then (return (i32.const 14))))
                (if (i32.lt_s (call $call (i32.const {sk_off}) (i32.const {sk_len}) (i32.const 8192) (i32.const 512)) (i32.const 1)) (then (return (i32.const 15))))
                (if (i32.lt_s (call $call (i32.const {del_off}) (i32.const {del_len}) (i32.const 8192) (i32.const 512)) (i32.const 1)) (then (return (i32.const 16))))

                (local.set $gd_n (call $call (i32.const {gd_off}) (i32.const {gd_len}) (i32.const 20480) (i32.const 2048)))
                (if (i32.lt_s (local.get $gd_n) (i32.const 1)) (then (return (i32.const 17))))
                (local.set $lst_n (call $call (i32.const {lst_off}) (i32.const {lst_len}) (i32.const 24576) (i32.const 4096)))
                (if (i32.lt_s (local.get $lst_n) (i32.const 1)) (then (return (i32.const 18))))

                (local.set $pos (i32.const 32768))
                (memory.copy (local.get $pos) (i32.const {o1_off}) (i32.const {o1_len}))
                (local.set $pos (i32.add (local.get $pos) (i32.const {o1_len})))
                (local.set $pos (i32.add (local.get $pos) (call $hex (i32.const 12288) (local.get $rb_len) (local.get $pos))))
                (memory.copy (local.get $pos) (i32.const {o2_off}) (i32.const {o2_len}))
                (local.set $pos (i32.add (local.get $pos) (i32.const {o2_len})))
                (local.set $pos (i32.add (local.get $pos) (call $hex (i32.const 20480) (local.get $gd_n) (local.get $pos))))
                (memory.copy (local.get $pos) (i32.const {o3_off}) (i32.const {o3_len}))
                (local.set $pos (i32.add (local.get $pos) (i32.const {o3_len})))
                (local.set $pos (i32.add (local.get $pos) (call $hex (i32.const 24576) (local.get $lst_n) (local.get $pos))))
                (memory.copy (local.get $pos) (i32.const {o4_off}) (i32.const {o4_len}))
                (local.set $pos (i32.add (local.get $pos) (i32.const {o4_len})))

                (drop (call $out (i32.const 32768) (i32.sub (local.get $pos) (i32.const 32768))))
                (i32.const 0)
            )
        )"#
    );
    wat::parse_str(&wat_src).unwrap()
}

fn content_delete_action_wasm(type_key: &str, entry_id: i64, expected_version: i64) -> Vec<u8> {
    let req = json!({
        "op": "content_delete",
        "type_key": type_key,
        "entry_id": entry_id,
        "expected_version": expected_version
    })
    .to_string();
    let out = r#"{"ok":true}"#;
    wat::parse_str(format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{}")
            (data (i32.const 2048) "{}")
            (func (export "opendash_run") (result i32)
                (drop (call $call (i32.const 1024) (i32.const {}) (i32.const 4096) (i32.const 512)))
                (drop (call $out (i32.const 2048) (i32.const {})))
                (i32.const 0)
            )
        )"#,
        wat_escape(&req),
        wat_escape(out),
        req.len(),
        out.len(),
    ))
    .unwrap()
}

/// First attempts `kv_set` (must fail with `"ok":false`), then posts to classifier `url` and
/// rejects if response contains `"spam":true`.
fn contact_classifier_filter_wasm(url: &str) -> Vec<u8> {
    let kv_req = r#"{"op":"kv_set","key":"illegal","value":"1"}"#;
    let http_req =
        json!({ "op": "http_request", "method": "POST", "url": url, "body": "{}" }).to_string();
    let needle_false = r#""ok":false"#;
    let needle_spam = "SPAM_YES";
    let allow_out = r#"{"decision":"allow"}"#;
    let reject_out = r#"{"decision":"reject","code":"spam","message":"Rejected by filter"}"#;

    let mut cur = 0usize;
    let mut segs = String::new();
    let mut add = |s: &str| {
        let off = cur;
        segs.push_str(&format!("(data (i32.const {off}) \"{}\")\n", wat_escape(s)));
        cur += s.len() + 4;
        (off, s.len())
    };
    let (kv_off, kv_len) = add(kv_req);
    let (http_off, http_len) = add(&http_req);
    let (nf_off, nf_len) = add(needle_false);
    let (ns_off, ns_len) = add(needle_spam);
    let (al_off, al_len) = add(allow_out);
    let (rj_off, rj_len) = add(reject_out);
    assert!(cur < 4096);

    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            {segs}
            (func $contains (param $hay i32) (param $hlen i32) (param $ndl i32) (param $nlen i32) (result i32)
                (local $i i32) (local $j i32)
                (local.set $i (i32.const 0))
                (block $done
                    (loop $outer
                        (br_if $done (i32.gt_s (i32.add (local.get $i) (local.get $nlen)) (local.get $hlen)))
                        (local.set $j (i32.const 0))
                        (block $miss
                            (loop $inner
                                (br_if $miss (i32.ne
                                    (i32.load8_u (i32.add (local.get $hay) (i32.add (local.get $i) (local.get $j))))
                                    (i32.load8_u (i32.add (local.get $ndl) (local.get $j)))
                                ))
                                (local.set $j (i32.add (local.get $j) (i32.const 1)))
                                (if (i32.eq (local.get $j) (local.get $nlen)) (then (return (i32.const 1))))
                                (br $inner)
                            )
                        )
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br $outer)
                    )
                )
                (i32.const 0)
            )
            (func (export "opendash_run") (result i32)
                (local $n i32)
                ;; kv_set inside filter:contact/before_submit must be rejected by HostPolicy
                (local.set $n (call $call (i32.const {kv_off}) (i32.const {kv_len}) (i32.const 8192) (i32.const 512)))
                (if (i32.lt_s (local.get $n) (i32.const 1)) (then (return (i32.const 1))))
                (if (i32.eq (call $contains (i32.const 8192) (local.get $n) (i32.const {nf_off}) (i32.const {nf_len})) (i32.const 0))
                    (then (return (i32.const 2)))
                )
                (local.set $n (call $call (i32.const {http_off}) (i32.const {http_len}) (i32.const 12288) (i32.const 2048)))
                (if (i32.lt_s (local.get $n) (i32.const 1)) (then (return (i32.const 3))))
                (if (i32.eq (call $contains (i32.const 12288) (local.get $n) (i32.const {ns_off}) (i32.const {ns_len})) (i32.const 1))
                    (then (drop (call $out (i32.const {rj_off}) (i32.const {rj_len}))))
                    (else (drop (call $out (i32.const {al_off}) (i32.const {al_len}))))
                )
                (i32.const 0)
            )
        )"#
    );
    wat::parse_str(&wat_src).unwrap()
}

/// Dual-filter plugin for CUJ-3:
/// - `filter:seo/head` -> returns SEO override with `noindex: false` and malicious `</script>` JSON-LD.
/// - `filter:entry/view` -> queries `taxonomy_list`, counts `"slug":`, emits `{"price_formatted":"250.000 đ","term_count":<n>}`.
fn cuj3_dual_projection_wasm() -> Vec<u8> {
    let needle_seo = "filter:seo/head";
    let seo_out = r#"{"title":"Shop SEO","description":"Shop Desc","noindex":false,"json_ld":[{"@context":"https://schema.org","@type":"Product","name":"Bad</script><script>alert(1)</script>&Co"}]}"#;
    let tax_req = r#"{"op":"taxonomy_list","kind":"category","limit":100}"#;
    let needle_item = r#""slug":"#;
    let ext_pfx = r#"{"price_formatted":"250.000 đ","term_count":"#;
    let ext_sfx = "}";

    let mut cur = 0usize;
    let mut segs = String::new();
    let mut add = |s: &str| {
        let off = cur;
        segs.push_str(&format!("(data (i32.const {off}) \"{}\")\n", wat_escape(s)));
        cur += s.len() + 4;
        (off, s.len())
    };
    let (nseo_off, nseo_len) = add(needle_seo);
    let (seo_off, seo_len) = add(seo_out);
    let (tax_off, tax_len) = add(tax_req);
    let (nitm_off, nitm_len) = add(needle_item);
    let (ep_off, ep_len) = add(ext_pfx);
    let (es_off, es_len) = add(ext_sfx);
    assert!(cur < 4096);

    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_read_input" (func $in (param i32 i32) (result i32)))
            (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            {segs}
            (func $count_occ (param $hay i32) (param $hlen i32) (param $ndl i32) (param $nlen i32) (result i32)
                (local $i i32) (local $j i32) (local $cnt i32)
                (local.set $i (i32.const 0))
                (local.set $cnt (i32.const 0))
                (block $done
                    (loop $outer
                        (br_if $done (i32.gt_s (i32.add (local.get $i) (local.get $nlen)) (local.get $hlen)))
                        (local.set $j (i32.const 0))
                        (block $miss
                            (loop $inner
                                (br_if $miss (i32.ne
                                    (i32.load8_u (i32.add (local.get $hay) (i32.add (local.get $i) (local.get $j))))
                                    (i32.load8_u (i32.add (local.get $ndl) (local.get $j)))
                                ))
                                (local.set $j (i32.add (local.get $j) (i32.const 1)))
                                (if (i32.eq (local.get $j) (local.get $nlen))
                                    (then
                                        (local.set $cnt (i32.add (local.get $cnt) (i32.const 1)))
                                        (br $miss)
                                    )
                                )
                                (br $inner)
                            )
                        )
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br $outer)
                    )
                )
                (local.get $cnt)
            )
            (func (export "opendash_run") (result i32)
                (local $in_len i32) (local $tlen i32) (local $cnt i32)
                (local.set $in_len (call $in (i32.const 8192) (i32.const 16384)))
                (if (i32.gt_s (call $count_occ (i32.const 8192) (local.get $in_len) (i32.const {nseo_off}) (i32.const {nseo_len})) (i32.const 0))
                    (then
                        (drop (call $out (i32.const {seo_off}) (i32.const {seo_len})))
                        (return (i32.const 0))
                    )
                )
                (local.set $tlen (call $call (i32.const {tax_off}) (i32.const {tax_len}) (i32.const 24576) (i32.const 8192)))
                (if (i32.lt_s (local.get $tlen) (i32.const 1)) (then (return (i32.const 1))))
                (local.set $cnt (call $count_occ (i32.const 24576) (local.get $tlen) (i32.const {nitm_off}) (i32.const {nitm_len})))
                (if (i32.gt_s (local.get $cnt) (i32.const 9)) (then (local.set $cnt (i32.const 9))))
                (memory.copy (i32.const 32768) (i32.const {ep_off}) (i32.const {ep_len}))
                (i32.store8 (i32.add (i32.const 32768) (i32.const {ep_len})) (i32.add (i32.const 48) (local.get $cnt)))
                (memory.copy (i32.add (i32.const 32768) (i32.const {})) (i32.const {es_off}) (i32.const {es_len}))
                (drop (call $out (i32.const 32768) (i32.const {})))
                (i32.const 0)
            )
        )"#,
        ep_len + 1,
        ep_len + 1 + es_len
    );
    wat::parse_str(&wat_src).unwrap()
}

fn build_shop_theme_zip() -> Vec<u8> {
    let manifest = json!({
        "manifest_version": 1,
        "theme_id": "s04-shop-theme",
        "name": "S04 Shop Theme",
        "version": "1.0.0",
        "templates": [
            { "kind": "index", "file": "index.html" },
            { "kind": "single", "file": "single.html" },
            { "kind": "archive", "file": "archive.html" }
        ]
    })
    .to_string();
    let tmpl = r#"<main>{{ od_content() }}{{ entry.title }}|price={{ (entry.ext["shop-ext"] | default({})).price_formatted | default("none") }}|terms={{ (entry.ext["shop-ext"] | default({})).term_count | default(0) }}</main>"#;
    let archive = r#"<main>{% for e in entries %}<div>{{ e.title }}|price={{ (e.ext["shop-ext"] | default({})).price_formatted | default("none") }}|terms={{ (e.ext["shop-ext"] | default({})).term_count | default(0) }}</div>{% endfor %}</main>"#;
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut cursor);
        let opts = SimpleFileOptions::default();
        zip.start_file("theme.json", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.start_file("index.html", opts).unwrap();
        zip.write_all(b"<main>index</main>").unwrap();
        zip.start_file("single.html", opts).unwrap();
        zip.write_all(tmpl.as_bytes()).unwrap();
        zip.start_file("archive.html", opts).unwrap();
        zip.write_all(archive.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

#[tokio::test]
async fn cuj1_secret_store_redaction_async_webhook_and_maintenance_drain() {
    let temp = TempDir::new("ext_cuj1");
    let port = free_port();
    let _server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;

    // Webhook capture server
    let webhook_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let webhook_port = webhook_listener.local_addr().unwrap().port();
    let (wh_tx, mut wh_rx) = mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        for stream in webhook_listener.incoming().flatten() {
            let mut s = stream;
            let mut buf = vec![0u8; 8192];
            if let Ok(n) = s.read(&mut buf) {
                let raw = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = wh_tx.send(raw);
            }
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        }
    });

    // Hanging TCP listener to simulate in-flight slow async worker during maintenance
    let hang_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hang_port = hang_listener.local_addr().unwrap().port();
    let (hang_tx, hang_rx) = oneshot::channel::<()>();
    let hang_release = std::sync::Arc::new(AtomicBool::new(false));
    let hang_release_bg = hang_release.clone();
    std::thread::spawn(move || {
        let mut hang_opt = Some(hang_tx);
        for stream in hang_listener.incoming().flatten() {
            let mut s = stream;
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            if let Some(tx) = hang_opt.take() {
                let _ = tx.send(());
            }
            while !hang_release_bg.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    });

    let webhook_url = format!("http://127.0.0.1:{webhook_port}/webhook");
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "crm-sync",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "CRM Sync",
            "hooks": ["content/after_publish"],
            "settings_schema": [
                { "key": "api_key", "label": "API Key", "type": "secret" },
                { "key": "endpoint_tag", "label": "Tag", "type": "text", "default": "staging" }
            ],
            "permissions": {
                "content_read": ["post"],
                "network": [{ "scheme": "http", "host": "127.0.0.1", "port": webhook_port, "methods": ["POST"] }],
                "plugin_settings_read": true,
                "plugin_secrets_read": ["api_key"]
            }
        }),
        &cuj1_secret_webhook_wasm("api_key", &webhook_url),
    )
    .await;

    // 1. Check initial redacted GET settings (version 1)
    let init_get = client
        .get(format!("{base_url}/api/admin/plugins/crm-sync/settings"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(init_get.status(), StatusCode::OK);
    let init_etag = init_get.headers()[header::ETAG]
        .to_str()
        .unwrap()
        .to_owned();
    let init_body: Value = init_get.json().await.unwrap();
    assert_eq!(
        init_body["values"]["api_key"],
        json!({ "configured": false })
    );
    assert_eq!(init_body["values"]["endpoint_tag"], json!("staging"));

    // 2. Save secret + text setting via PUT with If-Match
    let put_sec = client
        .put(format!("{base_url}/api/admin/plugins/crm-sync/settings"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", init_etag)
        .json(&json!({
            "values": {
                "api_key": "sk_live_secret_xyz",
                "endpoint_tag": "prod"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_sec.status(), StatusCode::OK);
    let put_etag = put_sec.headers()[header::ETAG].to_str().unwrap().to_owned();
    let put_body: Value = put_sec.json().await.unwrap();
    assert_eq!(put_body["values"]["api_key"], json!({ "configured": true }));
    assert!(!put_body.to_string().contains("sk_live_secret_xyz"));
    assert!(!put_body.to_string().contains("secret_ref"));

    let ro_db = open_readonly_db(&temp.path).await;
    let db_api_key_json: String = sqlx::query_scalar(
        "SELECT value_json FROM theme_option_values WHERE owner_kind = 'plugin' AND owner_id = 'crm-sync' AND option_key = 'api_key'",
    )
    .fetch_one(&ro_db)
    .await
    .unwrap();
    assert!(!db_api_key_json.contains("sk_live_secret_xyz"));
    let db_ref_val: Value = serde_json::from_str(&db_api_key_json).unwrap();
    let secret_ref = db_ref_val["secret_ref"].as_str().unwrap().to_owned();
    let secret_file = temp
        .path
        .join("database/secrets/plugins/crm-sync")
        .join(format!("{secret_ref}.json"));
    assert!(secret_file.is_file(), "SecretStore file must exist");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::symlink_metadata(&secret_file)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o177, 0);
    }

    let after_get = client
        .get(format!("{base_url}/api/admin/plugins/crm-sync/settings"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    let after_get_text = after_get.text().await.unwrap();
    assert!(!after_get_text.contains("sk_live_secret_xyz"));
    assert!(!after_get_text.contains(&secret_ref));

    // 3. Passing null preserves existing secret while updating text option
    let put_keep = client
        .put(format!("{base_url}/api/admin/plugins/crm-sync/settings"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", put_etag)
        .json(&json!({
            "values": {
                "api_key": Value::Null,
                "endpoint_tag": "prod-2"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_keep.status(), StatusCode::OK);
    let keep_etag = put_keep.headers()[header::ETAG]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(secret_file.is_file());

    // 4. Publishing a post fires AsyncHookQueue content/after_publish reading plugin_secret_get
    let create = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "slug": "cuj1-post", "title": "CUJ1 Post", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let post: Value = create.json().await.unwrap();
    let post_id = post["id"].as_i64().unwrap();

    let pub_resp = client
        .post(format!(
            "{base_url}/api/admin/content-types/post/entries/{post_id}/publication"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_resp.status(), StatusCode::OK);

    let req_raw = tokio::time::timeout(Duration::from_secs(6), wh_rx.recv())
        .await
        .expect("webhook timely")
        .expect("webhook payload");
    let body_start = req_raw.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    let wh_json: Value = serde_json::from_str(&req_raw[body_start..]).unwrap();
    let secret_resp: Value =
        serde_json::from_slice(&decode_hex_bytes(wh_json["secret_hex"].as_str().unwrap())).unwrap();
    let settings_resp: Value =
        serde_json::from_slice(&decode_hex_bytes(wh_json["settings_hex"].as_str().unwrap()))
            .unwrap();
    assert_eq!(secret_resp["data"]["value"], json!("sk_live_secret_xyz"));
    assert_eq!(
        settings_resp["data"]["settings"]["endpoint_tag"],
        json!("prod-2")
    );
    assert_eq!(
        settings_resp["data"]["settings"]["api_key"],
        json!({ "configured": true })
    );

    // 5. Clear removes SecretStore file and resets redacted GET to configured: false
    let put_clear = client
        .put(format!("{base_url}/api/admin/plugins/crm-sync/settings"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", keep_etag)
        .json(&json!({
            "values": {
                "api_key": { "clear": true },
                "endpoint_tag": "prod-2"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_clear.status(), StatusCode::OK);
    let clear_body: Value = put_clear.json().await.unwrap();
    assert_eq!(
        clear_body["values"]["api_key"],
        json!({ "configured": false })
    );
    assert!(!secret_file.exists(), "SecretStore file must be removed");

    // 6. Slow async hook holding MutationLease drains under SiteCoordinator maintenance and worker loop survives
    let hang_url = format!("http://127.0.0.1:{hang_port}/hang");
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "slow-drain",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Slow Drain",
            "hooks": ["settings/after_save"],
            "settings_schema": [{ "key": "tick", "label": "Tick", "type": "text" }],
            "permissions": {
                "network": [{ "scheme": "http", "host": "127.0.0.1", "port": hang_port, "methods": ["POST"] }]
            }
        }),
        &http_post_wasm(&hang_url),
    )
    .await;

    let r1 = client
        .put(format!("{base_url}/api/admin/plugins/slow-drain/settings"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "values": { "tick": "run-1" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(6), hang_rx)
        .await
        .expect("slow worker reached hang endpoint")
        .unwrap();

    // Enqueue 2 extra jobs so both running and queued jobs must cancel/drain for maintenance
    for (ver, label) in [(2, "run-2"), (3, "run-3")] {
        let r = client
            .put(format!("{base_url}/api/admin/plugins/slow-drain/settings"))
            .header("Cookie", &cookie)
            .header("Origin", &base_url)
            .header("X-CSRF-Token", &csrf)
            .header("If-Match", format!("\"v{ver}\""))
            .json(&json!({ "values": { "tick": label } }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
    }

    let maint_start = Instant::now();
    let maint_resp = client
        .post(format!("{base_url}/api/admin/operations/reconcile/plan"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();
    let maint_elapsed = maint_start.elapsed();
    let maint_status = maint_resp.status();
    let maint_body = maint_resp.text().await.unwrap();
    assert_eq!(
        maint_status,
        StatusCode::OK,
        "reconcile/plan failed during async hook drain: {maint_body}"
    );
    assert!(
        maint_elapsed < Duration::from_secs(5),
        "maintenance drain took {maint_elapsed:?}"
    );
    hang_release.store(true, Ordering::Relaxed);

    client
        .post(format!("{base_url}/api/admin/plugins/slow-drain/disable"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();

    // Subsequent async event after maintenance finishes must still execute cleanly
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "post-maint-ok",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Post Maint OK",
            "hooks": ["settings/after_save"],
            "settings_schema": [{ "key": "tick", "label": "Tick", "type": "text" }],
            "permissions": { "sqlite": ["kv_write"] }
        }),
        &kv_set_wasm("after_maint", "ok"),
    )
    .await;
    let trigger = client
        .put(format!(
            "{base_url}/api/admin/plugins/post-maint-ok/settings"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "values": { "tick": "final" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(trigger.status(), StatusCode::OK);
    wait_for_kv(&ro_db, "post-maint-ok", "after_maint", "\"ok\"").await;
}

#[tokio::test]
async fn s01_abi_v1_1_readback_ipc_newline_kv_tombstone_and_content_delete_cas() {
    let temp = TempDir::new("ext_abi_s01");
    let port = free_port();
    let _server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;

    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "abi-s01-plugin",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "ABI S01 Plugin",
            "permissions": { "sqlite": ["kv_read", "kv_write"] },
            "actions": [
                {
                    "id": "test-readback",
                    "title": "Test Readback & KV",
                    "required_role": "admin",
                    "capabilities": ["kv_read", "kv_write", "kv_delete", "kv_list"]
                }
            ]
        }),
        &s01_abi_readback_and_kv_tombstone_wasm(),
    )
    .await;

    let act_resp = client
        .post(format!(
            "{base_url}/api/admin/plugins/abi-s01-plugin/actions/test-readback"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let act_status = act_resp.status();
    let act_body = act_resp.text().await.unwrap();
    assert_eq!(act_status, StatusCode::OK, "action failed: {act_body}");
    let parsed: Value = serde_json::from_str(&act_body).unwrap();
    assert_eq!(parsed["success"], json!(true));
    let data = &parsed["data"];

    let readback_val: Value =
        serde_json::from_slice(&decode_hex_bytes(data["readback_hex"].as_str().unwrap())).unwrap();
    assert_eq!(readback_val["ok"], json!(true));
    assert_eq!(readback_val["found"], json!(true));
    let expected_ml = format!("line1\nline2\nline3\n{}", "x".repeat(160));
    assert_eq!(readback_val["value"], json!(expected_ml));

    let after_del: Value =
        serde_json::from_slice(&decode_hex_bytes(data["after_del_hex"].as_str().unwrap())).unwrap();
    assert_eq!(after_del["ok"], json!(true));
    assert_eq!(after_del["found"], json!(false));

    let list_val: Value =
        serde_json::from_slice(&decode_hex_bytes(data["list_hex"].as_str().unwrap())).unwrap();
    assert_eq!(list_val["ok"], json!(true));
    assert_eq!(
        list_val["data"]["items"],
        json!([{ "key": "k_keep", "value": "alive" }])
    );

    let ro_db = open_readonly_db(&temp.path).await;
    let deleted_row: Option<String> = sqlx::query_scalar(
        "SELECT value_json FROM plugin_kv WHERE plugin_id = 'abi-s01-plugin' AND key = 'k_del'",
    )
    .fetch_optional(&ro_db)
    .await
    .unwrap();
    assert!(deleted_row.is_none());
    let kept_row: String = sqlx::query_scalar(
        "SELECT value_json FROM plugin_kv WHERE plugin_id = 'abi-s01-plugin' AND key = 'k_keep'",
    )
    .fetch_one(&ro_db)
    .await
    .unwrap();
    assert_eq!(kept_row, "\"alive\"");

    // content_delete CAS verification
    let create = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "slug": "del-target", "title": "Delete Target", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let entry: Value = create.json().await.unwrap();
    let entry_id = entry["id"].as_i64().unwrap();

    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "del-stale",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Del Stale",
            "permissions": { "content_delete": ["post"] },
            "actions": [{ "id": "del", "title": "Del", "required_role": "admin", "capabilities": ["content_delete"] }]
        }),
        &content_delete_action_wasm("post", entry_id, 99),
    )
    .await;

    let stale_del = client
        .post(format!(
            "{base_url}/api/admin/plugins/del-stale/actions/del"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_ne!(stale_del.status(), StatusCode::OK);

    let still_exists = client
        .get(format!(
            "{base_url}/api/admin/content-types/post/entries/{entry_id}"
        ))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(still_exists.status(), StatusCode::OK);

    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "del-exact",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Del Exact",
            "permissions": { "content_delete": ["post"] },
            "actions": [{ "id": "del", "title": "Del", "required_role": "admin", "capabilities": ["content_delete"] }]
        }),
        &content_delete_action_wasm("post", entry_id, 1),
    )
    .await;

    let ok_del = client
        .post(format!(
            "{base_url}/api/admin/plugins/del-exact/actions/del"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let ok_status = ok_del.status();
    let ok_body = ok_del.text().await.unwrap();
    assert_eq!(
        ok_status,
        StatusCode::OK,
        "exact content_delete failed: {ok_body}"
    );
    let ok_json: Value = serde_json::from_str(&ok_body).unwrap();
    assert_eq!(ok_json["data"]["deleted_entry_id"], json!(entry_id));
    assert_eq!(ok_json["data"]["deleted_type_key"], json!("post"));
    assert_eq!(ok_json["data"]["lifecycle"]["operation"], json!("trash"));

    let gone = client
        .get(format!(
            "{base_url}/api/admin/content-types/post/entries/{entry_id}"
        ))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);

    // S02: content_delete moves the entry into recoverable Trash rather than physically deleting it.
    let (trashed_at, ver): (Option<String>, i64) =
        sqlx::query_as("SELECT trashed_at, version FROM content_entries WHERE id = ?")
            .bind(entry_id)
            .fetch_one(&ro_db)
            .await
            .unwrap();
    assert!(
        trashed_at.is_some(),
        "content_delete must retain row in Trash"
    );
    assert_eq!(ver, 2);
}

#[tokio::test]
async fn cuj2_prepare_save_editor_acl_and_contact_before_submit_pre_db_gate() {
    let temp = TempDir::new("ext_cuj2");
    let port = free_port();
    let _server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;

    // 1. Custom CPT + FieldGroup with editor-writable public field and admin-only private field
    let ct_resp = client
        .post(format!("{base_url}/api/admin/content-types"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "key": "product_s04",
            "label": "Product S04",
            "settings": {
                "public": true,
                "has_archive": true,
                "publication": "immediate",
                "supports": ["editor"]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ct_resp.status().as_u16(), 201);
    let fg_resp = client
        .post(format!("{base_url}/api/admin/content-field-groups"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "local_key": "product_s04_fields",
            "title": "Product S04 Fields",
            "target_types": ["product_s04"],
            "fields": [
                {
                    "key": "sku_normalized",
                    "label": "SKU Normalized",
                    "type": "text",
                    "acl": {
                        "min_view_role": "editor",
                        "min_edit_role": "editor",
                        "public_exposure": "public"
                    }
                },
                {
                    "key": "audit_code",
                    "label": "Audit Code",
                    "type": "text",
                    "acl": {
                        "min_view_role": "admin",
                        "min_edit_role": "admin",
                        "public_exposure": "private"
                    }
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(fg_resp.status().as_u16(), 201);

    // Create editor account
    let user_resp = client
        .post(format!("{base_url}/api/admin/users"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "username": "editor_s04",
            "password": "EditorPassword123!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(user_resp.status(), StatusCode::CREATED);
    let (ed_cookie, ed_csrf) =
        login_user(&client, &base_url, "editor_s04", "EditorPassword123!").await;

    // Plugin with content_write_private tries to patch admin-only field during Editor save -> blocked 422
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "prep-private-acl",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Prep Private ACL",
            "filters": ["filter:content/prepare_save"],
            "permissions": {
                "content_read": ["product_s04"],
                "content_write": ["product_s04"],
                "content_write_private": [{ "content_type": "product_s04", "field_keys": ["audit_code"] }]
            }
        }),
        &static_output_wasm(r#"{"values_patch":{"audit_code":"ADM-999"}}"#),
    )
    .await;

    let ed_blocked = client
        .post(format!(
            "{base_url}/api/admin/content-types/product_s04/entries"
        ))
        .header("Cookie", &ed_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &ed_csrf)
        .json(&json!({ "slug": "prod-ed-deny", "title": "Ed Deny", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        ed_blocked.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "Editor must not escalate to admin-only private field via filter:content/prepare_save"
    );

    let adm_allowed = client
        .post(format!(
            "{base_url}/api/admin/content-types/product_s04/entries"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "slug": "prod-adm-ok", "title": "Adm OK", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(adm_allowed.status(), StatusCode::CREATED);
    let adm_entry: Value = adm_allowed.json().await.unwrap();
    assert_eq!(adm_entry["values"]["audit_code"], json!("ADM-999"));

    client
        .post(format!(
            "{base_url}/api/admin/plugins/prep-private-acl/disable"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();

    // Public editor-writable field patch succeeds as Editor
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "prep-public-sku",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Prep Public SKU",
            "filters": ["filter:content/prepare_save"],
            "permissions": {
                "content_read": ["product_s04"],
                "content_write": ["product_s04"]
            }
        }),
        &static_output_wasm(r#"{"values_patch":{"sku_normalized":"SKU-001"}}"#),
    )
    .await;

    let ed_ok = client
        .post(format!(
            "{base_url}/api/admin/content-types/product_s04/entries"
        ))
        .header("Cookie", &ed_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &ed_csrf)
        .json(&json!({ "slug": "prod-ed-ok", "title": "Ed OK", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_ok.status(), StatusCode::CREATED);
    let ed_entry: Value = ed_ok.json().await.unwrap();
    assert_eq!(ed_entry["values"]["sku_normalized"], json!("SKU-001"));

    // 2. Contact form filter with classifier HTTP call + disallowed kv_set check
    let spam_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let spam_port = spam_listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut idx = 0u32;
        for stream in spam_listener.incoming().flatten() {
            idx += 1;
            let mut s = stream;
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let payload = if idx == 1 { "SPAM_YES" } else { "SPAM_NO" };
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = s.write_all(reply.as_bytes());
        }
    });

    let form_resp = client
        .post(format!("{base_url}/api/admin/contact/forms"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "cuj2-contact",
            "title": "CUJ2 Contact",
            "schema": {
                "version": 1,
                "fields": [{ "key": "name", "label": "Name", "kind": "text", "required": true }]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(form_resp.status(), StatusCode::CREATED);
    let form: Value = form_resp.json().await.unwrap();
    let form_id = form["id"].as_i64().unwrap();
    let form_ver = form["version"].as_i64().unwrap();

    let classifier_url = format!("http://127.0.0.1:{spam_port}/classify");
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "anti-spam-gate",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Anti Spam Gate",
            "filters": ["filter:contact/before_submit"],
            "permissions": {
                "sqlite": ["kv_write"],
                "network": [{ "scheme": "http", "host": "127.0.0.1", "port": spam_port, "methods": ["POST"] }]
            }
        }),
        &contact_classifier_filter_wasm(&classifier_url),
    )
    .await;

    let nonce_1 = "a123456789abcdef".repeat(4);
    let rej_resp = client
        .post(format!("{base_url}/api/public/contact/{form_id}"))
        .header("Origin", &base_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "form_version={form_ver}&nonce={nonce_1}&name=Spammer"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(rej_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        rej_resp
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );

    let ro_db = open_readonly_db(&temp.path).await;
    let sub_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contact_submissions")
        .fetch_one(&ro_db)
        .await
        .unwrap();
    assert_eq!(sub_before, 0);
    let illegal_kv: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plugin_kv WHERE plugin_id = 'anti-spam-gate'")
            .fetch_one(&ro_db)
            .await
            .unwrap();
    assert_eq!(illegal_kv, 0);

    let nonce_2 = "b123456789abcdef".repeat(4);
    let ok_resp = client
        .post(format!("{base_url}/api/public/contact/{form_id}"))
        .header("Origin", &base_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "form_version={form_ver}&nonce={nonce_2}&name=Legit"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(ok_resp.status(), StatusCode::SEE_OTHER);
    let sub_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contact_submissions")
        .fetch_one(&ro_db)
        .await
        .unwrap();
    assert_eq!(sub_after, 1);
}

#[tokio::test]
async fn cuj3_projections_json_ld_escape_monotonic_noindex_and_zero_wasm_public_get() {
    let temp = TempDir::new("ext_cuj3");
    let port = free_port();
    let _server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;

    let inst_theme = client
        .post(format!("{base_url}/api/admin/themes"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(build_shop_theme_zip())
        .send()
        .await
        .unwrap();
    assert_eq!(inst_theme.status(), StatusCode::CREATED);
    let rev: Value = inst_theme.json().await.unwrap();
    let act = client
        .put(format!("{base_url}/api/admin/themes/active"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "revision_id": rev["revision_id"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(act.status(), StatusCode::OK);

    // Create 1 taxonomy term BEFORE publishing post
    let term1 = client
        .post(format!("{base_url}/api/admin/taxonomy/terms"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "kind": "category", "name": "Cat One", "slug": "cat-one", "parent_id": Value::Null }))
        .send()
        .await
        .unwrap();
    assert_eq!(term1.status(), StatusCode::CREATED);

    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "shop-ext",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Shop Ext",
            "filters": ["filter:seo/head", "filter:entry/view"],
            "permissions": {
                "content_read": ["post"],
                "taxonomy_read": true
            }
        }),
        &cuj3_dual_projection_wasm(),
    )
    .await;

    let create = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "slug": "shop-item", "title": "Shop Item", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let entry: Value = create.json().await.unwrap();
    let post_id = entry["id"].as_i64().unwrap();

    let pub_resp = client
        .post(format!(
            "{base_url}/api/admin/content-types/post/entries/{post_id}/publication"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_resp.status(), StatusCode::OK);

    let ro_db = open_readonly_db(&temp.path).await;
    let initial_updated_at: String = sqlx::query_scalar(
        "SELECT updated_at FROM plugin_entry_projections WHERE entry_id = ? AND plugin_id = 'shop-ext'",
    )
    .bind(post_id)
    .fetch_one(&ro_db)
    .await
    .unwrap();

    // Mutate taxonomy AFTER write-time projection: if public GET spawned Wasm it would see 2 instead of 1!
    let term2 = client
        .post(format!("{base_url}/api/admin/taxonomy/terms"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "kind": "category", "name": "Cat Two", "slug": "cat-two", "parent_id": Value::Null }))
        .send()
        .await
        .unwrap();
    assert_eq!(term2.status(), StatusCode::CREATED);

    let single_html = client
        .get(format!("{base_url}/shop-item"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        single_html.contains("Shop Item|price=250.000 đ|terms=1"),
        "single_html={single_html}"
    );
    assert!(
        single_html.contains(
            r"Bad\u003c/script\u003e\u003cscript\u003ealert(1)\u003c/script\u003e\u0026Co"
        )
    );
    assert!(!single_html.contains("<script>alert(1)</script>"));
    assert!(single_html.contains(r#"<meta name="robots" content="index,follow">"#));

    let archive_html = client
        .get(format!("{base_url}/posts"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        archive_html.contains("Shop Item|price=250.000 đ|terms=1"),
        "archive_html={archive_html}"
    );

    let after_get_updated_at: String = sqlx::query_scalar(
        "SELECT updated_at FROM plugin_entry_projections WHERE entry_id = ? AND plugin_id = 'shop-ext'",
    )
    .bind(post_id)
    .fetch_one(&ro_db)
    .await
    .unwrap();
    assert_eq!(after_get_updated_at, initial_updated_at);

    // Core/entry noindex: true protection cannot be bypassed
    let set_noindex = client
        .put(format!(
            "{base_url}/api/admin/content-types/post/entries/{post_id}/seo"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v2\"")
        .json(&json!({ "noindex": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(set_noindex.status(), StatusCode::OK);

    let noindex_html = client
        .get(format!("{base_url}/shop-item"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(noindex_html.contains(r#"<meta name="robots" content="noindex,nofollow">"#));

    let not_found_resp = client
        .get(format!("{base_url}/nonexistent-route-404"))
        .send()
        .await
        .unwrap();
    assert_eq!(not_found_resp.status(), StatusCode::NOT_FOUND);
    let not_found_html = not_found_resp.text().await.unwrap();
    assert!(not_found_html.contains(r#"<meta name="robots" content="noindex,nofollow">"#));
}

#[test]
fn cuj4_cli_check_plugin_validates_dir_zip_and_security_boundaries() {
    let bin = env!("CARGO_BIN_EXE_opendash");
    let temp = TempDir::new("ext_cuj4_cli");
    let empty_cwd = temp.path.join("empty_cwd");
    fs::create_dir_all(&empty_cwd).unwrap();

    // 1. Sample plugin ZIP in repo
    let sample_zip = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins/sample-plugin/sample-plugin.zip");
    let sample_out = Command::new(bin)
        .current_dir(&empty_cwd)
        .arg("--check-plugin")
        .arg(&sample_zip)
        .output()
        .unwrap();
    assert_eq!(sample_out.status.code(), Some(0));
    let sample_json: Value = serde_json::from_slice(&sample_out.stdout).unwrap();
    assert_eq!(sample_json["ok"], true);
    assert_eq!(sample_json["plugin_id"], "sample-plugin");
    assert_eq!(sample_json["settings_count"], 0);

    // 2. Custom v3 plugin directory & non-.zip archive
    let plugin_dir = temp.path.join("valid-v3-plugin");
    fs::create_dir_all(plugin_dir.join("blocks/notice")).unwrap();
    let manifest_v3 = json!({
        "id": "cli-e2e-plugin",
        "version": "2.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "CLI E2E Plugin",
        "hooks": ["content/after_publish", "content/after_delete"],
        "filters": ["filter:seo/head", "filter:entry/view"],
        "actions": [
            { "id": "z-act", "title": "Z", "capabilities": [] },
            { "id": "a-act", "title": "A", "capabilities": [] }
        ],
        "settings_schema": [
            { "key": "secret_k", "label": "Secret", "type": "secret" },
            { "key": "mode", "label": "Mode", "type": "text", "default": "auto" }
        ],
        "permissions": {}
    })
    .to_string();
    let block_schema = json!({
        "schema_version": 1,
        "id": "cli-e2e-plugin/notice",
        "title": "Notice",
        "category": "content",
        "render": "render.html",
        "fields": []
    })
    .to_string();
    let wasm = wat::parse_str(
        r#"(module
            (import "opendash_v1" "host_last_resp_len" (func (result i32)))
            (import "opendash_v1" "host_read_last_resp" (func (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "opendash_run") (result i32) (i32.const 0))
        )"#,
    )
    .unwrap();
    fs::write(plugin_dir.join("manifest.json"), manifest_v3.as_bytes()).unwrap();
    fs::write(plugin_dir.join("plugin.wasm"), &wasm).unwrap();
    fs::write(
        plugin_dir.join("blocks/notice/block.json"),
        block_schema.as_bytes(),
    )
    .unwrap();
    fs::write(
        plugin_dir.join("blocks/notice/render.html"),
        b"<p>Notice</p>",
    )
    .unwrap();

    let dir_out = Command::new(bin)
        .current_dir(&empty_cwd)
        .arg("--check-plugin")
        .arg(&plugin_dir)
        .output()
        .unwrap();
    assert_eq!(dir_out.status.code(), Some(0));
    let dir_json: Value = serde_json::from_slice(&dir_out.stdout).unwrap();
    assert_eq!(dir_json["ok"], true);
    assert_eq!(dir_json["plugin_id"], "cli-e2e-plugin");
    assert_eq!(dir_json["manifest_schema"], 3);
    assert_eq!(
        dir_json["hooks"],
        json!(["content/after_delete", "content/after_publish"])
    );
    assert_eq!(
        dir_json["filters"],
        json!(["filter:entry/view", "filter:seo/head"])
    );
    assert_eq!(dir_json["actions"], json!(["a-act", "z-act"]));
    assert_eq!(dir_json["blocks"], json!(["cli-e2e-plugin/notice"]));
    assert_eq!(dir_json["settings_count"], 2);

    // `--check-plugin=<dir>` equals form
    let mut eq_arg = std::ffi::OsString::from("--check-plugin=");
    eq_arg.push(plugin_dir.as_os_str());
    let eq_out = Command::new(bin)
        .current_dir(&empty_cwd)
        .arg(eq_arg)
        .output()
        .unwrap();
    assert_eq!(eq_out.status.code(), Some(0));
    let eq_json: Value = serde_json::from_slice(&eq_out.stdout).unwrap();
    assert_eq!(eq_json, dir_json);

    // Non-.zip archive path
    let bundle_path = temp.path.join("plugin.bundle");
    fs::write(&bundle_path, build_plugin_zip(&manifest_v3, &wasm)).unwrap();
    let bundle_out = Command::new(bin)
        .current_dir(&empty_cwd)
        .arg("--check-plugin")
        .arg(&bundle_path)
        .output()
        .unwrap();
    assert_eq!(bundle_out.status.code(), Some(0));

    // Error cases -> exit 1 + JSON { ok: false, error: ... }
    let missing_dir = temp.path.join("missing-plugin");
    let hidden_dir = temp.path.join("hidden-plugin");
    fs::create_dir_all(&hidden_dir).unwrap();
    fs::write(hidden_dir.join("manifest.json"), manifest_v3.as_bytes()).unwrap();
    fs::write(hidden_dir.join("plugin.wasm"), &wasm).unwrap();
    fs::write(hidden_dir.join(".hidden"), b"1").unwrap();

    for args in [
        vec!["--check-plugin".to_owned()],
        vec!["--check-plugin=".to_owned()],
        vec![
            "--check-plugin".to_owned(),
            plugin_dir.to_string_lossy().into_owned(),
            "--extra".to_owned(),
        ],
        vec![
            "--check-plugin".to_owned(),
            missing_dir.to_string_lossy().into_owned(),
        ],
        vec![
            "--check-plugin".to_owned(),
            hidden_dir.to_string_lossy().into_owned(),
        ],
    ] {
        let out = Command::new(bin)
            .current_dir(&empty_cwd)
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1), "args={args:?}");
        let err_json: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(err_json["ok"], false);
        assert!(!err_json["error"].as_str().unwrap_or("").is_empty());
    }

    // Must not touch cwd or create database/locks
    assert_eq!(
        fs::read_dir(&empty_cwd).unwrap().count(),
        0,
        "--check-plugin must remain completely offline with zero filesystem side effects"
    );
}
