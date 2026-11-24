use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use reqwest::{Client, StatusCode, header, redirect::Policy};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

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
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await
            && resp.status().is_success()
        {
            break;
        }
        assert!(Instant::now() < deadline, "server failed to become healthy");
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
    assert!(setup.status().is_success());

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
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
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

fn build_s03_theme_zip() -> Vec<u8> {
    let manifest = json!({
        "manifest_version": 3,
        "theme_id": "s03-proj-theme",
        "name": "S03 Projection Theme",
        "version": "1.0.0",
        "templates": [
            { "kind": "index", "file": "index.html" },
            { "kind": "single", "file": "single.html" }
        ]
    });
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, content) in [
            ("theme.json", manifest.to_string()),
            ("index.html", "<main>index</main>".to_owned()),
            (
                "single.html",
                r#"<main>{{ od_content() }}{{ entry.title }}|ext={{ (entry.ext["ext-view"] | default({})).badge | default("none") }}|missing={{ entry.ext["not-installed"] | default("ok") }}</main>"#.to_owned(),
            ),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

fn static_output_wasm(output_json: &str) -> Vec<u8> {
    let bytes = output_json.as_bytes();
    let escaped: String = bytes.iter().map(|b| format!("\\{b:02x}")).collect();
    let wat_src = format!(
        r#"(module
            (import "opendash_v1" "host_write_output" (func $set_output (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{escaped}")
            (func (export "opendash_run") (result i32)
                (drop (call $set_output (i32.const 1024) (i32.const {len})))
                i32.const 0)
        )"#,
        len = bytes.len()
    );
    wat::parse_str(&wat_src).unwrap()
}

fn kv_set_wasm(key: &str, value: &str) -> Vec<u8> {
    let req = json!({ "op": "kv_set", "key": key, "value": value }).to_string();
    let out = "{}";
    let req_esc: String = req
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect();
    let out_esc: String = out
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
            (data (i32.const 1536) "{out_esc}")
            (func (export "opendash_run") (result i32)
                (drop (call $host_call (i32.const 1024) (i32.const {rlen}) (i32.const 2048) (i32.const 512)))
                (drop (call $set_output (i32.const 1536) (i32.const {olen})))
                i32.const 0)
        )"#,
        rlen = req.len(),
        olen = out.len()
    );
    wat::parse_str(&wat_src).unwrap()
}

fn kv_write_attempt_wasm() -> Vec<u8> {
    let req = r#"{"op":"kv_set","key":"tamper","value":"1"}"#;
    let out = r#"{"decision":"allow"}"#;
    let req_esc: String = req
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect();
    let out_esc: String = out
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
            (data (i32.const 1536) "{out_esc}")
            (func (export "opendash_run") (result i32)
                (local $rc i32)
                (local.set $rc (call $host_call (i32.const 1024) (i32.const {rlen}) (i32.const 2048) (i32.const 512)))
                (if (i32.lt_s (local.get $rc) (i32.const 7))
                    (then (return (i32.const 1))))
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

async fn open_readonly_db(site_root: &Path) -> SqlitePool {
    let db_path = site_root.join("database").join("opendash.sqlite");
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    SqlitePool::connect_with(opts).await.unwrap()
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

#[tokio::test]
async fn s03_async_hook_queue_runs_after_events_via_http() {
    let temp = TempDir::new("s03_async_hooks");
    let port = free_port();
    let _server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;

    for (pid, event_name, kv_key) in [
        ("hk-after-save", "content/after_save", "after_save"),
        ("hk-after-pub", "content/after_publish", "after_pub"),
        ("hk-after-unpub", "content/after_unpublish", "after_unpub"),
        ("hk-after-del", "content/after_delete", "after_del"),
        ("hk-plugin-en", "plugin/after_enable", "after_en"),
        ("hk-plugin-dis", "plugin/after_disable", "after_dis"),
        ("hk-settings", "settings/after_save", "after_cfg"),
    ] {
        install_plugin(
            &client,
            &base_url,
            &cookie,
            &csrf,
            json!({
                "id": pid,
                "version": "1.0.0",
                "api_version": 1,
                "manifest_schema": 3,
                "name": pid,
                "hooks": [event_name],
                "permissions": { "content_read": ["post"], "sqlite": ["kv_write"] }
            }),
            &kv_set_wasm(kv_key, "1"),
        )
        .await;
    }

    // Target plugin to toggle enable/disable & save settings
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "toggle-target",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Toggle Target",
            "settings_schema": [{ "key": "note", "label": "Note", "type": "text" }]
        }),
        &static_output_wasm("{}"),
    )
    .await;

    let dis = client
        .post(format!(
            "{base_url}/api/admin/plugins/toggle-target/disable"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(dis.status(), StatusCode::OK);

    let en = client
        .post(format!("{base_url}/api/admin/plugins/toggle-target/enable"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(en.status(), StatusCode::OK);

    let cfg = client
        .put(format!(
            "{base_url}/api/admin/plugins/toggle-target/settings"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "values": { "note": "hello" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(cfg.status(), StatusCode::OK);

    let create = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "slug": "async-post", "title": "Async Post", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created: Value = create.json().await.unwrap();
    let entry_id = created["id"].as_i64().unwrap();

    let pub_resp = client
        .post(format!(
            "{base_url}/api/admin/content-types/post/entries/{entry_id}/publication"
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

    let unpub_resp = client
        .post(format!(
            "{base_url}/api/admin/content-types/post/entries/{entry_id}/publication"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v2\"")
        .json(&json!({ "status": "draft" }))
        .send()
        .await
        .unwrap();
    assert_eq!(unpub_resp.status(), StatusCode::OK);

    let del_resp = client
        .delete(format!(
            "{base_url}/api/admin/content-types/post/entries/{entry_id}"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v3\"")
        .send()
        .await
        .unwrap();
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

    let ro_db = open_readonly_db(&temp.path).await;
    wait_for_kv(&ro_db, "hk-after-save", "after_save", "\"1\"").await;
    wait_for_kv(&ro_db, "hk-after-pub", "after_pub", "\"1\"").await;
    wait_for_kv(&ro_db, "hk-after-unpub", "after_unpub", "\"1\"").await;
    wait_for_kv(&ro_db, "hk-after-del", "after_del", "\"1\"").await;
    wait_for_kv(&ro_db, "hk-plugin-dis", "after_dis", "\"1\"").await;
    wait_for_kv(&ro_db, "hk-plugin-en", "after_en", "\"1\"").await;
    wait_for_kv(&ro_db, "hk-settings", "after_cfg", "\"1\"").await;
}

#[tokio::test]
async fn s03_prepare_save_patch_acl_and_revalidation() {
    let temp = TempDir::new("s03_prepare_save");
    let port = free_port();
    let _server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;

    // 1. Plugin lacking `content_write` returns a patch -> rejected with 422
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "prep-unauthorized",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Prep Unauth",
            "filters": ["filter:content/prepare_save"],
            "permissions": { "content_read": ["post"] }
        }),
        &static_output_wasm(r#"{"title":"Tampered"}"#),
    )
    .await;

    let blocked = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "slug": "orig-slug", "title": "Orig", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::UNPROCESSABLE_ENTITY);

    client
        .post(format!(
            "{base_url}/api/admin/plugins/prep-unauthorized/disable"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();

    // 2. Authorized plugin returning an invalid slug triggers full re-validation -> 422
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "prep-bad-slug",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Prep Bad Slug",
            "filters": ["filter:content/prepare_save"],
            "permissions": { "content_read": ["post"], "content_write": ["post"] }
        }),
        &static_output_wasm(r#"{"slug":"INVALID SLUG!!"}"#),
    )
    .await;

    let bad_reval = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "slug": "valid-slug", "title": "Valid", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_reval.status(), StatusCode::UNPROCESSABLE_ENTITY);

    client
        .post(format!(
            "{base_url}/api/admin/plugins/prep-bad-slug/disable"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();

    // 3. Authorized plugin rewriting title & slug succeeds on create
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "prep-normalize",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Prep Normalize",
            "filters": ["filter:content/prepare_save"],
            "permissions": { "content_read": ["post"], "content_write": ["post"] }
        }),
        &static_output_wasm(r#"{"title":"Normalized Title","slug":"normalized-slug"}"#),
    )
    .await;

    let ok_create = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "slug": "raw-slug", "title": "Raw Title", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(ok_create.status(), StatusCode::CREATED);
    let body: Value = ok_create.json().await.unwrap();
    assert_eq!(body["title"], "Normalized Title");
    assert_eq!(body["slug"], "normalized-slug");
}

#[tokio::test]
async fn s03_contact_before_submit_reject_isolation_and_allow_after_submit() {
    let temp = TempDir::new("s03_contact_filter");
    let port = free_port();
    let _server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;

    let form_resp = client
        .post(format!("{base_url}/api/admin/contact/forms"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "lien-he-s03",
            "title": "Lien He",
            "schema": {
                "version": 1,
                "fields": [{ "key": "name", "label": "Name", "kind": "text", "required": true }]
            }
        }))
        .send()
        .await
        .unwrap();
    let form_status = form_resp.status();
    let form_body = form_resp.text().await.unwrap();
    assert_eq!(form_status, StatusCode::CREATED, "form create: {form_body}");
    let form: Value = serde_json::from_str(&form_body).unwrap();
    let form_id = form["id"].as_i64().unwrap();
    let form_ver = form["version"].as_i64().unwrap();

    // 1. Plugin trying kv_set inside filter:contact/before_submit fails closed & writes 0 DB
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "contact-kv-tamper",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Contact KV Tamper",
            "filters": ["filter:contact/before_submit"],
            "permissions": { "sqlite": ["kv_write"] }
        }),
        &kv_write_attempt_wasm(),
    )
    .await;

    let nonce_1 = "0123456789abcdef".repeat(4);
    let tamper_resp = client
        .post(format!("{base_url}/api/public/contact/{form_id}"))
        .header("Origin", &base_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "form_version={form_ver}&nonce={nonce_1}&name=Alice"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(tamper_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let tamper_body = tamper_resp.text().await.unwrap();
    assert!(
        tamper_body.contains("Plugin kết thúc với mã lỗi 1")
            || tamper_body.contains("filter:contact/before_submit"),
        "expected plugin failure rather than validation error; body = {tamper_body}"
    );
    let ro_db = open_readonly_db(&temp.path).await;
    let tamper_kv: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plugin_kv WHERE plugin_id = 'contact-kv-tamper'")
            .fetch_one(&ro_db)
            .await
            .unwrap();
    assert_eq!(tamper_kv, 0);

    client
        .post(format!(
            "{base_url}/api/admin/plugins/contact-kv-tamper/disable"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();

    // 2. Reject filter returns 422 + no-store + escaped HTML, leaving 0 submissions in DB
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "contact-reject",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Contact Reject",
            "filters": ["filter:contact/before_submit"]
        }),
        &static_output_wasm(
            r#"{"decision":"reject","code":"spam","message":"Spam <b>blocked</b>"}"#,
        ),
    )
    .await;

    let nonce_2 = "1123456789abcdef".repeat(4);
    let rej_resp = client
        .post(format!("{base_url}/api/public/contact/{form_id}"))
        .header("Origin", &base_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!("form_version={form_ver}&nonce={nonce_2}&name=Bob"))
        .send()
        .await
        .unwrap();
    assert_eq!(rej_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let rej_cc = rej_resp
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let rej_html = rej_resp.text().await.unwrap();
    assert_eq!(
        rej_cc.as_deref(),
        Some("no-store"),
        "missing no-store Cache-Control header; body = {rej_html}"
    );
    assert!(
        rej_html.contains("Spam &lt;b&gt;blocked&lt;/b&gt;"),
        "body = {rej_html}"
    );

    let sub_count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contact_submissions")
        .fetch_one(&ro_db)
        .await
        .unwrap();
    assert_eq!(sub_count_before, 0);

    client
        .post(format!(
            "{base_url}/api/admin/plugins/contact-reject/disable"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();

    // 3. Allow filter + contact/after_submit hook -> 303 See Other + DB write + async hook
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "contact-allow",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Contact Allow",
            "filters": ["filter:contact/before_submit"]
        }),
        &static_output_wasm(r#"{"decision":"allow"}"#),
    )
    .await;
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "contact-after",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Contact After",
            "hooks": ["contact/after_submit"],
            "permissions": { "sqlite": ["kv_write"] }
        }),
        &kv_set_wasm("submitted", "yes"),
    )
    .await;

    let nonce_3 = "2123456789abcdef".repeat(4);
    let ok_resp = client
        .post(format!("{base_url}/api/public/contact/{form_id}"))
        .header("Origin", &base_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "form_version={form_ver}&nonce={nonce_3}&name=Carol"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(ok_resp.status(), StatusCode::SEE_OTHER);

    let sub_count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contact_submissions")
        .fetch_one(&ro_db)
        .await
        .unwrap();
    assert_eq!(sub_count_after, 1);
    wait_for_kv(&ro_db, "contact-after", "submitted", "\"yes\"").await;
}

#[tokio::test]
async fn s03_projections_seo_jsonld_escape_noindex_entry_ext_and_lifecycle() {
    let temp = TempDir::new("s03_projections");
    let port = free_port();
    let _server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;

    // 1. Install and activate minimal Theme v3 reading entry.ext under UndefinedBehavior::Strict
    let inst_theme = client
        .post(format!("{base_url}/api/admin/themes"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(build_s03_theme_zip())
        .send()
        .await
        .unwrap();
    let inst_status = inst_theme.status();
    let inst_body = inst_theme.text().await.unwrap();
    assert_eq!(
        inst_status,
        StatusCode::CREATED,
        "theme install: {inst_body}"
    );
    let rev: Value = serde_json::from_str(&inst_body).unwrap();
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

    // 2. Create and publish a post BEFORE projection plugins exist -> default({}) renders cleanly
    let create = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "slug": "proj-post", "title": "Proj Post", "values": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let post: Value = create.json().await.unwrap();
    let post_id = post["id"].as_i64().unwrap();

    let publish = client
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
    assert_eq!(publish.status(), StatusCode::OK);

    let baseline_html = client
        .get(format!("{base_url}/proj-post"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(baseline_html.contains("Proj Post|ext=none|missing=ok"));

    // 3. Install filter:seo/head + filter:entry/view plugins -> async batched recompute runs on install
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "seo-proj",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "SEO Proj",
            "filters": ["filter:seo/head"],
            "permissions": { "content_read": ["post"] },
            "settings_schema": [{ "key": "mode", "label": "Mode", "type": "text" }]
        }),
        &static_output_wasm(
            r#"{"title":"Plugin Head Title","description":"Plugin Head Desc","noindex":false,"json_ld":[{"@context":"https://schema.org","@type":"Article","headline":"Hi</script><script>alert(1)</script>"}]}"#,
        ),
    )
    .await;
    install_plugin(
        &client,
        &base_url,
        &cookie,
        &csrf,
        json!({
            "id": "ext-view",
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "name": "Ext View",
            "filters": ["filter:entry/view"],
            "permissions": { "content_read": ["post"] }
        }),
        &static_output_wasm(r#"{"badge":"vip"}"#),
    )
    .await;

    let ro_db = open_readonly_db(&temp.path).await;
    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM plugin_entry_projections WHERE entry_id = ?")
                .bind(post_id)
                .fetch_one(&ro_db)
                .await
                .unwrap_or(0);
        if count == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }

    let proj_html = client
        .get(format!("{base_url}/proj-post"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(proj_html.contains("Plugin Head Title"));
    assert!(proj_html.contains("Plugin Head Desc"));
    assert!(proj_html.contains(r#"<meta name="robots" content="index,follow">"#));
    assert!(
        proj_html.contains(r"\u003c/script\u003e\u003cscript\u003ealert(1)\u003c/script\u003e")
    );
    assert!(!proj_html.contains("<script>alert(1)</script>"));
    assert!(proj_html.contains("Proj Post|ext=vip|missing=ok"));

    // 4. Core/Entry noindex: true cannot be downgraded to false by plugin
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
        .get(format!("{base_url}/proj-post"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(noindex_html.contains(r#"<meta name="robots" content="noindex,nofollow">"#));

    // 5. Disable deletes projections synchronously; enable & settings recompute in background batches
    let dis = client
        .post(format!("{base_url}/api/admin/plugins/ext-view/disable"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(dis.status(), StatusCode::OK);

    let remaining_ext: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM plugin_entry_projections WHERE entry_id = ? AND plugin_id = 'ext-view'",
    )
    .bind(post_id)
    .fetch_one(&ro_db)
    .await
    .unwrap();
    assert_eq!(remaining_ext, 0);

    let after_dis_html = client
        .get(format!("{base_url}/proj-post"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(after_dis_html.contains("Proj Post|ext=none|missing=ok"));

    let en = client
        .post(format!("{base_url}/api/admin/plugins/ext-view/enable"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(en.status(), StatusCode::OK);

    let deadline_en = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline_en {
        let html = client
            .get(format!("{base_url}/proj-post"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        if html.contains("Proj Post|ext=vip|missing=ok") {
            break;
        }
        assert!(
            Instant::now() < deadline_en,
            "re-enable batched projection missing"
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
    }

    let put_cfg = client
        .put(format!("{base_url}/api/admin/plugins/seo-proj/settings"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "values": { "mode": "strict" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_cfg.status(), StatusCode::OK);
}
