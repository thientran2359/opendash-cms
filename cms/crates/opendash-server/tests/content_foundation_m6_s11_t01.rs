//! C4 #378: real Wasmi integration tests on isolated plugin-declared content.
//! This is intentionally RED before C4 manifest support lands.

use reqwest::{Client, Response, StatusCode};
use serde_json::{Value, json};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zip::{ZipWriter, write::SimpleFileOptions};

const CPT: &str = "listing_c4";
const TAX: &str = "genre_c4";

static NEXT_SITE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Site(PathBuf);
impl Site {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = NEXT_SITE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // macOS /var is a symlink; site-root validation rejects symlink components.
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "opendash_c4_{}_{}_{}",
            std::process::id(),
            nanos,
            seq
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Site {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn vacant_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
async fn serve(site: &Path, port: u16) -> Server {
    assert_ne!(port, 18964);
    let log_path = site.join("c4-server.log");
    let log = fs::File::create(&log_path).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_SITE_ROOT", site)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .env("OPENDASH_PLUGIN_ALLOW_LOOPBACK", "1")
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log))
        .spawn()
        .unwrap();
    let server = Server(child);
    let client = Client::new();
    // Native plugin/Wasm tests in this file run concurrently by default.
    // Give an isolated server time to initialize when several concurrent
    // fixture migrations compete for the same macOS Intel CPU; status must
    // still reach healthy, and every behavioral assertion remains mandatory.
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if let Ok(resp) = client
            .get(format!("http://127.0.0.1:{port}/api/health"))
            .send()
            .await
        {
            if resp.status().is_success() {
                return server;
            }
        }
        if Instant::now() >= deadline {
            panic!(
                "isolated server health timed out; log: {}",
                fs::read_to_string(&log_path).unwrap_or_default()
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
async fn expect_json(response: Response, status: StatusCode) -> Value {
    let actual = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(actual, status, "response: {body}");
    serde_json::from_str(&body).unwrap()
}

async fn auth(client: &Client, base: &str) -> (String, String) {
    let setup = client
        .post(format!("{base}/auth/setup"))
        .header("Origin", base)
        .json(&json!({"username": "c4_admin", "password": "C4-Isolated-Password!2026"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = client
        .post(format!("{base}/auth/login"))
        .header("Origin", base)
        .json(&json!({"username": "c4_admin", "password": "C4-Isolated-Password!2026"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let me = expect_json(
        client
            .get(format!("{base}/auth/me"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    (cookie, me["csrf_token"].as_str().unwrap().to_string())
}

fn plugin_zip(manifest: &Value, wasm: &[u8]) -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default();
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("plugin.wasm", options).unwrap();
        zip.write_all(wasm).unwrap();
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

fn theme_zip(theme_id: &str, version: &str, with_declarations: bool) -> Vec<u8> {
    let mut manifest = json!({
        "manifest_version": 3,
        "theme_id": theme_id,
        "name": theme_id,
        "version": version,
        "templates": [{"kind":"index","file":"index.html"}],
    });
    if with_declarations {
        manifest["declarations"] = json!({
            "taxonomy_definitions": [{
                "key":"theme_genre_c4", "label":"C4 theme genre",
                "hierarchical":false,"public":true,
                "rewrite_base":"theme-genres-c4"
            }],
            "content_types": [{
                "key":"theme_listing_c4","label":"C4 theme listing",
                "fields":[],"taxonomies":["theme_genre_c4"],
                "settings":{
                    "public":true,"has_archive":true,
                    "publication":"draft_publish","supports":["editor"]
                },
                "rewrite_base":"theme-listings-c4"
            }]
        });
    }
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default();
        zip.start_file("theme.json", options).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file("index.html", options).unwrap();
        zip.write_all(b"<main>C4 theme</main>").unwrap();
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

fn wasm_output(output: &str) -> Vec<u8> {
    let escaped: String = output
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect();
    wat::parse_str(format!(
        r#"(module
        (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 1024) "{escaped}")
        (func (export "opendash_run") (result i32)
            (drop (call $out (i32.const 1024) (i32.const {length})))
            i32.const 0))"#,
        length = output.len()
    ))
    .unwrap()
}

fn wasm_trap() -> Vec<u8> {
    wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func (export "opendash_run") (result i32) unreachable))"#,
    )
    .unwrap()
}

/// Execute one or two *real* Wasmi host calls. The second call is reached only
/// when the previous host response began with {"ok":true,...}.
fn wasm_host_calls(requests: &[String]) -> Vec<u8> {
    assert!((1..=2).contains(&requests.len()));
    let mut data = String::new();
    let mut calls = String::new();
    for (i, request) in requests.iter().enumerate() {
        let addr = 1024 + i * 4096;
        let escaped: String = request
            .as_bytes()
            .iter()
            .map(|b| format!("\\{b:02x}"))
            .collect();
        data.push_str(&format!("(data (i32.const {addr}) \"{escaped}\")\n"));
        calls.push_str(&format!(
            r#"
            (local.set $n (call $host (i32.const {addr}) (i32.const {len})
                (i32.const 12000) (i32.const 2048)))
            (if (i32.lt_s (local.get $n) (i32.const 7))
                (then (return (i32.const 1))))
            (if (i32.eq (i32.load8_u (i32.const 12006)) (i32.const 102))
                (then (return (i32.const 1))))
        "#,
            len = request.len()
        ));
    }
    wat::parse_str(format!(
        r#"(module
        (import "opendash_v1" "host_call" (func $host (param i32 i32 i32 i32) (result i32)))
        (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
        (memory (export "memory") 1)
        {data}
        (func (export "opendash_run") (result i32)
            (local $n i32)
            {calls}
            (drop (call $out (i32.const 12000) (local.get $n)))
            i32.const 0))"#
    ))
    .unwrap()
}

fn writer_manifest(version: &str, http_origin: Option<&str>) -> Value {
    let mut manifest = json!({
        "id":"c4-writer","name":"C4 Writer","version":version,
        "api_version":1,"manifest_schema":2,
        "permissions":{"content_write":[CPT]},
        "actions":[{"id":"write-c4","required_role":"admin","capabilities":["content_write"]}]
    });
    if let Some(origin) = http_origin {
        manifest["permissions"]["http"] = json!([origin]);
        manifest["actions"][0]["capabilities"] = json!(["content_write", "http"]);
    }
    manifest
}

async fn invoke_writer(client: &Client, base: &str, cookie: &str, csrf: &str) -> Response {
    client
        .post(format!(
            "{base}/api/admin/plugins/c4-writer/actions/write-c4"
        ))
        .header("Cookie", cookie)
        .header("Origin", base)
        .header("x-csrf-token", csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
}

async fn install(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    manifest: Value,
    wasm: Vec<u8>,
) -> Value {
    expect_json(
        client
            .post(format!("{base}/api/admin/plugins"))
            .header("Cookie", cookie)
            .header("Origin", base)
            .header("x-csrf-token", csrf)
            .header("content-type", "application/zip")
            .body(plugin_zip(&manifest, &wasm))
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await
}

async fn toggle(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    plugin_id: &str,
    action: &str,
    version: i64,
) -> Value {
    expect_json(
        client
            .post(format!("{base}/api/admin/plugins/{plugin_id}/{action}"))
            .header("Cookie", cookie)
            .header("Origin", base)
            .header("x-csrf-token", csrf)
            .header("if-match", version.to_string())
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await
}

fn provider_manifest() -> Value {
    json!({
        "id":"c4-provider", "name":"C4 Provider", "version":"1.0.0",
        "api_version":1, "manifest_schema":2,
        "hooks":["content/before_save"],
        "permissions":{"content_read":[CPT]},
        "declarations":{
            "taxonomy_definitions":[{"key":TAX, "label":"C4 genre",
                "hierarchical":false, "public":true, "rewrite_base":"genres-c4"}],
            "content_types":[{"key":CPT,"label":"C4 listing","fields":[],
                "taxonomies":[TAX],"rewrite_base":"listings-c4",
                "settings":{"public":true,"has_archive":true,
                    "publication":"draft_publish","supports":["editor"]}}]
        }
    })
}

async fn install_provider(client: &Client, base: &str, cookie: &str, csrf: &str) {
    install(
        client,
        base,
        cookie,
        csrf,
        provider_manifest(),
        wasm_output(r#"{"decision":"allow"}"#),
    )
    .await;
}

async fn add_term(client: &Client, base: &str, cookie: &str, csrf: &str) -> i64 {
    let term = expect_json(
        client
            .post(format!("{base}/api/admin/taxonomy/terms"))
            .header("Cookie", cookie)
            .header("Origin", base)
            .header("x-csrf-token", csrf)
            .json(&json!({"kind":TAX,"name":"C4 fiction","slug":"fiction-c4"}))
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await;
    term["id"].as_i64().unwrap()
}

async fn entry(client: &Client, base: &str, cookie: &str, id: i64) -> Value {
    expect_json(
        client
            .get(format!("{base}/api/admin/content-types/{CPT}/entries/{id}"))
            .header("Cookie", cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await
}

#[tokio::test]
async fn legacy_manifest_without_c4_declarations_still_saves_post() {
    let site = Site::new();
    let port = vacant_port();
    let _server = serve(&site.0, port).await;
    let base = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = auth(&client, &base).await;
    install(
        &client,
        &base,
        &cookie,
        &csrf,
        json!({
            "id":"c4-legacy","name":"Legacy C4","version":"1.0.0",
            "api_version":1,"manifest_schema":2,
            "hooks":["content/before_save"],"permissions":{"content_read":["post"]}
        }),
        wasm_output(r#"{"decision":"allow"}"#),
    )
    .await;
    let saved = expect_json(
        client
            .post(format!("{base}/api/admin/content-types/post/entries"))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("x-csrf-token", &csrf)
            .json(&json!({"slug":"legacy-c4","title":"Legacy works","values":{}}))
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(saved["slug"], "legacy-c4");
    assert_eq!(saved["version"], 1);
}

#[tokio::test]
async fn legacy_wasmi_host_call_fixture_still_saves_post() {
    let site = Site::new();
    let port = vacant_port();
    let _server = serve(&site.0, port).await;
    let base = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = auth(&client, &base).await;
    let payload = json!({"op":"content_save","type_key":"post",
        "slug":"legacy-wasmi-c4","title":"Legacy host call","values":{}})
    .to_string();
    install(
        &client,
        &base,
        &cookie,
        &csrf,
        json!({
            "id":"c4-legacy-action","name":"Legacy C4 Action","version":"1.0.0",
            "api_version":1,"manifest_schema":2,
            "permissions":{"content_write":["post"]},
            "actions":[{"id":"create-post","required_role":"admin",
                "capabilities":["content_write"]}]
        }),
        wasm_host_calls(&[payload]),
    )
    .await;
    let response = expect_json(
        client
            .post(format!(
                "{base}/api/admin/plugins/c4-legacy-action/actions/create-post"
            ))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("x-csrf-token", &csrf)
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(response["success"], true);
    assert_eq!(response["data"]["entry"]["slug"], "legacy-wasmi-c4");
    assert_eq!(response["data"]["entry"]["version"], 1);
}

#[tokio::test]
async fn c4_custom_taxonomy_veto_allow_after_save_draft_and_cas() {
    let site = Site::new();
    let port = vacant_port();
    let _server = serve(&site.0, port).await;
    let base = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = auth(&client, &base).await;

    // RED before C4: old parser rejects taxonomy_definitions/rewrite_base.
    install_provider(&client, &base, &cookie, &csrf).await;
    let term_id = add_term(&client, &base, &cookie, &csrf).await;
    let veto = install(
        &client,
        &base,
        &cookie,
        &csrf,
        json!({
            "id":"c4-veto","name":"C4 Veto","version":"1.0.0","api_version":1,
            "manifest_schema":2,"hook_priority":1,"hooks":["content/before_save"],
            "permissions":{"content_read":[CPT]}
        }),
        wasm_output(r#"{"decision":"veto","code":"c4_veto","message":"C4 blocked"}"#),
    )
    .await;
    let rejected = expect_json(client.post(format!("{base}/api/admin/content-types/{CPT}/entries"))
        .header("Cookie", &cookie).header("Origin", &base).header("x-csrf-token", &csrf)
        .json(&json!({"slug":"c4-draft","title":"Must not commit","values":{},"term_ids":[term_id]}))
        .send().await.unwrap(), StatusCode::UNPROCESSABLE_ENTITY).await;
    assert_eq!(rejected["code"], "c4_veto");
    let rows = expect_json(
        client
            .get(format!("{base}/api/admin/content-types/{CPT}/entries"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert!(
        rows.as_array().unwrap().is_empty(),
        "veto must leave no entry or links"
    );
    toggle(
        &client,
        &base,
        &cookie,
        &csrf,
        "c4-veto",
        "disable",
        veto["cas_version"].as_i64().unwrap(),
    )
    .await;
    install(
        &client,
        &base,
        &cookie,
        &csrf,
        json!({
            "id":"c4-after","name":"C4 After Trap","version":"1.0.0",
            "api_version":1,"manifest_schema":2,"hooks":["content/after_save"],
            "permissions":{"content_read":[CPT]}
        }),
        wasm_trap(),
    )
    .await;

    // Provider Allow runs in Wasmi; after_save trap must not roll back the save.
    let created = expect_json(
        client
            .post(format!("{base}/api/admin/content-types/{CPT}/entries"))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("x-csrf-token", &csrf)
            .json(&json!({"slug":"c4-draft","title":"C4 draft","values":{},"term_ids":[term_id]}))
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await;
    let id = created["id"].as_i64().unwrap();
    assert_eq!(created["publication_status"], "draft");
    assert_eq!(created["term_ids"], json!([term_id]));
    assert_eq!(created["version"], 1);
    assert_eq!(
        client
            .get(format!("{base}/listings-c4/c4-draft"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    let stale = client
        .put(format!("{base}/api/admin/content-types/{CPT}/entries/{id}"))
        .header("Cookie", &cookie)
        .header("Origin", &base)
        .header("x-csrf-token", &csrf)
        .header("if-match", "\"v99\"")
        .json(&json!({"slug":"c4-draft","title":"Stale","values":{},"term_ids":[]}))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    let current = entry(&client, &base, &cookie, id).await;
    assert_eq!(current["version"], 1);
    assert_eq!(current["term_ids"], json!([term_id]));
    let update = expect_json(
        client
            .put(format!("{base}/api/admin/content-types/{CPT}/entries/{id}"))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("x-csrf-token", &csrf)
            .header("if-match", "\"v1\"")
            .json(&json!({"slug":"c4-draft","title":"Saved C4","values":{},"term_ids":[term_id]}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(update["version"], 2);
    let publication = expect_json(
        client
            .post(format!(
                "{base}/api/admin/content-types/{CPT}/entries/{id}/publication"
            ))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("x-csrf-token", &csrf)
            .header("if-match", "\"v2\"")
            .json(&json!({"status":"published"}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(publication["publication_status"], "published");
    assert_eq!(
        client
            .get(format!("{base}/listings-c4/c4-draft"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn c4_wasmi_content_save_provider_generation_fence_and_cas() {
    let site = Site::new();
    let port = vacant_port();
    let _server = serve(&site.0, port).await;
    let base = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = auth(&client, &base).await;
    install_provider(&client, &base, &cookie, &csrf).await;
    let term_id = add_term(&client, &base, &cookie, &csrf).await;

    // The HTTP server pauses precisely between Wasmi admission and host
    // ContentSave. This deterministically races an action with disable.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mock_origin = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (connected_tx, connected_rx) = tokio::sync::oneshot::channel::<()>();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let mock = tokio::spawn(async move {
        let mut first_connection = Some(connected_tx);
        let mut first_release = Some(release_rx);
        while let Ok((mut socket, _)) = listener.accept().await {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            if let Some(signal) = first_connection.take() {
                let _ = signal.send(());
                if let Some(release) = first_release.take() {
                    let _ = release.await;
                }
            }
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await;
            let _ = socket.shutdown().await;
        }
    });
    let save = json!({"op":"content_save","type_key":CPT,
        "slug":"c4-host-save","title":"C4 via Wasmi","values":{},"term_ids":[term_id]
    })
    .to_string();
    let http = json!({"op":"http_request","method":"GET",
        "url":format!("{mock_origin}/pause")})
    .to_string();
    let installed = install(
        &client,
        &base,
        &cookie,
        &csrf,
        writer_manifest("1.0.0", Some(&mock_origin)),
        wasm_host_calls(&[http, save]),
    )
    .await;
    let action_url = format!("{base}/api/admin/plugins/c4-writer/actions/write-c4");
    assert_eq!(
        client
            .post(&action_url)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let created_editor = client
        .post(format!("{base}/api/admin/users"))
        .header("Cookie", &cookie)
        .header("Origin", &base)
        .header("x-csrf-token", &csrf)
        .json(&json!({"username":"c4_editor","password":"C4-Editor-Password!2026","role":"editor"}))
        .send()
        .await
        .unwrap();
    assert_eq!(created_editor.status(), StatusCode::CREATED);
    let editor_login = client
        .post(format!("{base}/auth/login"))
        .header("Origin", &base)
        .json(&json!({"username":"c4_editor","password":"C4-Editor-Password!2026"}))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_login.status(), StatusCode::OK);
    let editor_cookie = editor_login
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let editor_me = expect_json(
        client
            .get(format!("{base}/auth/me"))
            .header("Cookie", &editor_cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    let forbidden = client
        .post(&action_url)
        .header("Cookie", &editor_cookie)
        .header("Origin", &base)
        .header("x-csrf-token", editor_me["csrf_token"].as_str().unwrap())
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let pending = {
        let client = client.clone();
        let base = base.clone();
        let cookie = cookie.clone();
        let csrf = csrf.clone();
        tokio::spawn(async move { invoke_writer(&client, &base, &cookie, &csrf).await })
    };
    tokio::time::timeout(Duration::from_secs(5), connected_rx)
        .await
        .expect("guest never performed the first host HTTP call")
        .expect("mock did not signal connection");
    let disabled = toggle(
        &client,
        &base,
        &cookie,
        &csrf,
        "c4-writer",
        "disable",
        installed["cas_version"].as_i64().unwrap(),
    )
    .await;
    let _ = release_tx.send(());
    let fenced = pending.await.unwrap();
    let fence_status = fenced.status();
    let fence_body = fenced.text().await.unwrap();
    assert_eq!(fence_status, StatusCode::CONFLICT, "{fence_body}");
    let rows = expect_json(
        client
            .get(format!("{base}/api/admin/content-types/{CPT}/entries"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert!(
        rows.as_array().unwrap().is_empty(),
        "generation fence must prevent ghost ContentSave"
    );

    toggle(
        &client,
        &base,
        &cookie,
        &csrf,
        "c4-writer",
        "enable",
        disabled["cas_version"].as_i64().unwrap(),
    )
    .await;
    let saved = expect_json(
        invoke_writer(&client, &base, &cookie, &csrf).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(saved["success"], true);
    assert_eq!(saved["data"]["entry"]["publication_status"], "draft");
    assert_eq!(saved["data"]["entry"]["term_ids"], json!([term_id]));
    let entry_id = saved["data"]["entry"]["id"].as_i64().unwrap();
    assert_eq!(entry(&client, &base, &cookie, entry_id).await["version"], 1);
    mock.abort();

    // A fresh WASM revision stages an update with stale CAS. No title, version
    // or memberships may change; the next revision with the correct CAS saves.
    let update = |expected_version: i64| {
        json!({
            "op":"content_save","type_key":CPT,"entry_id":entry_id,
            "expected_version":expected_version,"slug":"c4-host-save",
            "title":"C4 updated by Wasmi","values":{},"term_ids":[term_id]
        })
        .to_string()
    };
    install(
        &client,
        &base,
        &cookie,
        &csrf,
        writer_manifest("1.1.0", None),
        wasm_host_calls(&[update(99)]),
    )
    .await;
    let stale = invoke_writer(&client, &base, &cookie, &csrf).await;
    let stale_status = stale.status();
    let stale_body = stale.text().await.unwrap();
    assert_eq!(
        stale_status,
        StatusCode::PRECONDITION_FAILED,
        "{stale_body}"
    );
    let before = entry(&client, &base, &cookie, entry_id).await;
    assert_eq!(before["version"], 1);
    assert_eq!(before["title"], "C4 via Wasmi");
    assert_eq!(before["term_ids"], json!([term_id]));
    install(
        &client,
        &base,
        &cookie,
        &csrf,
        writer_manifest("1.2.0", None),
        wasm_host_calls(&[update(1)]),
    )
    .await;
    let updated = expect_json(
        invoke_writer(&client, &base, &cookie, &csrf).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(updated["data"]["entry"]["version"], 2);
    assert_eq!(updated["data"]["entry"]["title"], "C4 updated by Wasmi");
    assert_eq!(updated["data"]["entry"]["term_ids"], json!([term_id]));
}

#[tokio::test]
async fn c4_conflicting_provider_zip_rolls_back_owner_and_canonical_claims() {
    let site = Site::new();
    let port = vacant_port();
    let _server = serve(&site.0, port).await;
    let base = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = auth(&client, &base).await;
    install_provider(&client, &base, &cookie, &csrf).await;
    let term_id = add_term(&client, &base, &cookie, &csrf).await;
    let entry = expect_json(
        client
            .post(format!("{base}/api/admin/content-types/{CPT}/entries"))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("x-csrf-token", &csrf)
            .json(
                &json!({"title":"C4 original","slug":"retained","values":{},"term_ids":[term_id]}),
            )
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await;
    expect_json(
        client
            .post(format!(
                "{base}/api/admin/content-types/{CPT}/entries/{}/publication",
                entry["id"]
            ))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("x-csrf-token", &csrf)
            .header(
                "if-match",
                format!("\"v{}\"", entry["version"].as_i64().unwrap()),
            )
            .json(&json!({"status":"published"}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    let before = expect_json(
        client
            .get(format!("{base}/api/admin/settings/permalinks"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;

    let mut collision = provider_manifest();
    collision["id"] = json!("c4-foreign-base");
    collision["name"] = json!("C4 Foreign Base");
    collision["permissions"]["content_read"] = json!(["listing_c4_second"]);
    collision["declarations"]["content_types"][0]["key"] = json!("listing_c4_second");
    collision["declarations"]["content_types"][0]["taxonomies"] = json!(["genre_c4_second"]);
    collision["declarations"]["taxonomy_definitions"][0]["key"] = json!("genre_c4_second");
    collision["declarations"]["taxonomy_definitions"][0]["rewrite_base"] =
        json!("genres-c4-second");
    // Reusing the first CPT's dynamic pattern must be rejected even when the
    // second CPT has no entries, and must not leave a half-installed taxonomy.
    let rejected = client
        .post(format!("{base}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base)
        .header("x-csrf-token", &csrf)
        .header("content-type", "application/zip")
        .body(plugin_zip(
            &collision,
            &wasm_output(r#"{"decision":"allow"}"#),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        rejected.status(),
        StatusCode::CONFLICT,
        "{}",
        rejected.text().await.unwrap()
    );
    assert_eq!(
        client
            .get(format!("{base}/listings-c4/retained"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
    );
    let after = expect_json(
        client
            .get(format!("{base}/api/admin/settings/permalinks"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(after, before, "failed install must leave C3 CAS unchanged");
    let definitions = expect_json(
        client
            .get(format!("{base}/api/admin/taxonomy/definitions"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert!(
        definitions
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["key"] != "genre_c4_second")
    );

    let mut foreign_key = provider_manifest();
    foreign_key["id"] = json!("c4-foreign-key");
    foreign_key["declarations"]["content_types"][0]["key"] = json!("listing_c4_foreign");
    foreign_key["declarations"]["content_types"][0]["taxonomies"] = json!([TAX]);
    foreign_key["permissions"]["content_read"] = json!(["listing_c4_foreign"]);
    foreign_key["declarations"]["content_types"][0]["rewrite_base"] = json!("listings-c4-foreign");
    let rejected = client
        .post(format!("{base}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base)
        .header("x-csrf-token", &csrf)
        .header("content-type", "application/zip")
        .body(plugin_zip(
            &foreign_key,
            &wasm_output(r#"{"decision":"allow"}"#),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        rejected.status(),
        StatusCode::CONFLICT,
        "{}",
        rejected.text().await.unwrap()
    );
    assert_eq!(
        client
            .get(format!("{base}/listings-c4/retained"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
    );
}

#[tokio::test]
async fn c4_theme_declarations_switch_preserves_terms_entries_and_rewrite() {
    let site = Site::new();
    let port = vacant_port();
    let _server = serve(&site.0, port).await;
    let base = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = auth(&client, &base).await;

    async fn install_theme(
        client: &Client,
        base: &str,
        cookie: &str,
        csrf: &str,
        theme_id: &str,
        with_declarations: bool,
    ) -> String {
        let installed = expect_json(
            client
                .post(format!("{base}/api/admin/themes"))
                .header("Cookie", cookie)
                .header("Origin", base)
                .header("X-CSRF-Token", csrf)
                .header("Content-Type", "application/zip")
                .body(theme_zip(theme_id, "1.0.0", with_declarations))
                .send()
                .await
                .unwrap(),
            StatusCode::CREATED,
        )
        .await;
        installed["revision_id"].as_str().unwrap().to_owned()
    }
    async fn activate_theme(
        client: &Client,
        base: &str,
        cookie: &str,
        csrf: &str,
        revision_id: &str,
        expected: StatusCode,
    ) {
        let selection = expect_json(
            client
                .get(format!("{base}/api/admin/themes"))
                .header("Cookie", cookie)
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
        )
        .await;
        let current_version = selection["version"].as_i64().unwrap();
        let response = client
            .put(format!("{base}/api/admin/themes/active"))
            .header("Cookie", cookie)
            .header("Origin", base)
            .header("X-CSRF-Token", csrf)
            .header("If-Match", format!("\"v{current_version}\""))
            .json(&json!({"revision_id":revision_id}))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            expected,
            "{}",
            response.text().await.unwrap()
        );
    }

    let alpha = install_theme(&client, &base, &cookie, &csrf, "c4-theme-alpha", true).await;
    let beta = install_theme(&client, &base, &cookie, &csrf, "c4-theme-beta", false).await;
    activate_theme(&client, &base, &cookie, &csrf, &alpha, StatusCode::OK).await;

    let term = expect_json(
        client.post(format!("{base}/api/admin/taxonomy/terms"))
            .header("Cookie", &cookie).header("Origin", &base)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({"kind":"theme_genre_c4","name":"C4 theme fiction","slug":"theme-fiction-c4"}))
            .send().await.unwrap(),
        StatusCode::CREATED,
    ).await;
    let created = expect_json(
        client
            .post(format!(
                "{base}/api/admin/content-types/theme_listing_c4/entries"
            ))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({"title":"C4 theme entry","slug":"theme-entry-c4",
                "values":{},"term_ids":[term["id"]]}))
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(created["publication_status"], "draft");
    expect_json(
        client
            .post(format!(
                "{base}/api/admin/content-types/theme_listing_c4/entries/{}/publication",
                created["id"],
            ))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("X-CSRF-Token", &csrf)
            .header(
                "If-Match",
                format!("\"v{}\"", created["version"].as_i64().unwrap()),
            )
            .json(&json!({"status":"published"}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        client
            .get(format!("{base}/theme-listings-c4/theme-entry-c4"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
    );
    assert_eq!(
        client
            .get(format!("{base}/theme-genres-c4/theme-fiction-c4"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
    );
    activate_theme(&client, &base, &cookie, &csrf, &beta, StatusCode::OK).await;
    assert_eq!(
        client
            .get(format!("{base}/theme-listings-c4/theme-entry-c4"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND,
        "inactive theme must not expose provider entries",
    );
    let retained = expect_json(
        client
            .get(format!(
                "{base}/api/admin/content-types/theme_listing_c4/entries/{}",
                created["id"]
            ))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(retained["term_ids"], json!([term["id"]]));
    assert_eq!(retained["title"], "C4 theme entry");
    activate_theme(&client, &base, &cookie, &csrf, &alpha, StatusCode::OK).await;
    assert_eq!(
        client
            .get(format!("{base}/theme-listings-c4/theme-entry-c4"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
        "same owner theme reactivation restores declared canonical URL",
    );

    let stolen = install_theme(&client, &base, &cookie, &csrf, "c4-theme-foreign", true).await;
    activate_theme(
        &client,
        &base,
        &cookie,
        &csrf,
        &stolen,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(
        client
            .get(format!("{base}/theme-listings-c4/theme-entry-c4"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
        "foreign owner cannot replace active theme revision or C3 claims",
    );
}

#[tokio::test]
async fn c4_hierarchy_downgrade_rejected_without_replacing_active_revision() {
    let site = Site::new();
    let port = vacant_port();
    let _server = serve(&site.0, port).await;
    let base = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = auth(&client, &base).await;
    let mut hierarchical = provider_manifest();
    hierarchical["declarations"]["taxonomy_definitions"][0]["hierarchical"] = json!(true);
    install(
        &client,
        &base,
        &cookie,
        &csrf,
        hierarchical.clone(),
        wasm_output(r#"{"decision":"allow"}"#),
    )
    .await;
    let parent = expect_json(
        client
            .post(format!("{base}/api/admin/taxonomy/terms"))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({"kind":TAX,"name":"C4 parent","slug":"c4-parent"}))
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await;
    let child = expect_json(
        client
            .post(format!("{base}/api/admin/taxonomy/terms"))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({"kind":TAX,"name":"C4 child","slug":"c4-child",
                "parent_id":parent["id"]}))
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await;
    let before = expect_json(
        client
            .get(format!("{base}/api/admin/plugins"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    let version_before = before
        .as_array()
        .and_then(|items| items.iter().find(|row| row["id"] == "c4-provider"))
        .map(|row| row["version"].clone())
        .expect("installed provider must appear in list snapshot");

    let mut downgrade = hierarchical;
    downgrade["version"] = json!("1.0.1");
    downgrade["declarations"]["taxonomy_definitions"][0]["hierarchical"] = json!(false);
    let rejected = client
        .post(format!("{base}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(plugin_zip(
            &downgrade,
            &wasm_output(r#"{"decision":"allow"}"#),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        rejected.status(),
        StatusCode::CONFLICT,
        "{}",
        rejected.text().await.unwrap()
    );
    let current = expect_json(
        client
            .get(format!("{base}/api/admin/taxonomy/definitions/{TAX}"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(current["hierarchical"], true);
    let retained_child = expect_json(
        client
            .get(format!("{base}/api/admin/taxonomy/terms/{}", child["id"]))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(retained_child["parent_id"], parent["id"]);
    let after = expect_json(
        client
            .get(format!("{base}/api/admin/plugins"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    let version_after = after
        .as_array()
        .and_then(|items| items.iter().find(|row| row["id"] == "c4-provider"))
        .map(|row| row["version"].clone())
        .expect("rejected revision must preserve previously installed provider");
    assert_eq!(
        version_before, version_after,
        "rejected provider revision must not activate an incompatible schema"
    );
}

/// No site map entry means that the package base is inherited, not frozen as
/// an override. Exercise real published routes, stable-ID redirects, and
/// same-owner update rollback at the HTTP boundary.
#[tokio::test]
async fn c4_same_owner_default_add_change_remove_published_no_site_override() {
    let site = Site::new();
    let port = vacant_port();
    let _server = serve(&site.0, port).await;
    let base = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let raw_client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let (cookie, csrf) = auth(&client, &base).await;

    async fn settings(client: &Client, base: &str, cookie: &str) -> Value {
        expect_json(
            client
                .get(format!("{base}/api/admin/settings/permalinks"))
                .header("Cookie", cookie)
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
        )
        .await
    }
    async fn route(
        client: &Client,
        base: &str,
        path: &str,
        expected: StatusCode,
        location: Option<&str>,
    ) {
        let response = client.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status(), expected, "route: {path}");
        if let Some(location) = location {
            assert_eq!(
                response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok()),
                Some(location),
                "redirect must target the current canonical path, not a chain: {path}",
            );
        }
    }

    // Same-owner installation with no declared rewrite keeps the C3 legacy
    // routes and leaves the explicit site maps empty.
    let mut manifest = provider_manifest();
    manifest["declarations"]["content_types"][0]
        .as_object_mut()
        .unwrap()
        .remove("rewrite_base");
    manifest["declarations"]["taxonomy_definitions"][0]
        .as_object_mut()
        .unwrap()
        .remove("rewrite_base");
    install(
        &client,
        &base,
        &cookie,
        &csrf,
        manifest.clone(),
        wasm_output(r#"{"decision":"allow"}"#),
    )
    .await;
    let term_id = add_term(&client, &base, &cookie, &csrf).await;
    let created = expect_json(
        client
            .post(format!("{base}/api/admin/content-types/{CPT}/entries"))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("x-csrf-token", &csrf)
            .json(
                &json!({"title":"Inherited route C4","slug":"inherited-entry",
                "values":{},"term_ids":[term_id]}),
            )
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await;
    let entry_id = created["id"].as_i64().unwrap();
    let published = expect_json(
        client
            .post(format!(
                "{base}/api/admin/content-types/{CPT}/entries/{entry_id}/publication"
            ))
            .header("Cookie", &cookie)
            .header("Origin", &base)
            .header("x-csrf-token", &csrf)
            .header(
                "if-match",
                format!("\"v{}\"", created["version"].as_i64().unwrap()),
            )
            .json(&json!({"status":"published"}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(published["publication_status"], "published");
    let initial_entry = format!("/{CPT}/inherited-entry");
    let initial_term = format!("/{TAX}/fiction-c4");
    route(&raw_client, &base, &initial_entry, StatusCode::OK, None).await;
    route(&raw_client, &base, &initial_term, StatusCode::OK, None).await;
    let initial = settings(&client, &base, &cookie).await;
    assert_eq!(initial["content_type_bases"], json!({}));
    assert_eq!(initial["taxonomy_bases"], json!({}));

    // Add provider defaults; no site PUT is made. The original canonical
    // routes become stable-ID aliases to new URLs, with no redirect chaining.
    manifest["version"] = json!("1.0.1");
    manifest["declarations"]["content_types"][0]["rewrite_base"] = json!("catalog-c4");
    manifest["declarations"]["taxonomy_definitions"][0]["rewrite_base"] = json!("topics-c4");
    install(
        &client,
        &base,
        &cookie,
        &csrf,
        manifest.clone(),
        wasm_output(r#"{"decision":"allow"}"#),
    )
    .await;
    let added = settings(&client, &base, &cookie).await;
    assert_eq!(
        added["version"].as_i64(),
        Some(initial["version"].as_i64().unwrap() + 1)
    );
    assert_eq!(added["content_type_bases"], json!({}));
    assert_eq!(added["taxonomy_bases"], json!({}));
    route(
        &raw_client,
        &base,
        "/catalog-c4/inherited-entry",
        StatusCode::OK,
        None,
    )
    .await;
    route(
        &raw_client,
        &base,
        "/topics-c4/fiction-c4",
        StatusCode::OK,
        None,
    )
    .await;
    route(
        &raw_client,
        &base,
        &initial_entry,
        StatusCode::MOVED_PERMANENTLY,
        Some("/catalog-c4/inherited-entry"),
    )
    .await;
    route(
        &raw_client,
        &base,
        &initial_term,
        StatusCode::MOVED_PERMANENTLY,
        Some("/topics-c4/fiction-c4"),
    )
    .await;

    // Change both bases. Both initial aliases and the prior provider URLs
    // redirect directly to the latest stable-ID canonical destinations.
    manifest["version"] = json!("1.0.2");
    manifest["declarations"]["content_types"][0]["rewrite_base"] = json!("library-c4");
    manifest["declarations"]["taxonomy_definitions"][0]["rewrite_base"] = json!("genres-c4-next");
    install(
        &client,
        &base,
        &cookie,
        &csrf,
        manifest.clone(),
        wasm_output(r#"{"decision":"allow"}"#),
    )
    .await;
    let changed = settings(&client, &base, &cookie).await;
    assert_eq!(
        changed["version"].as_i64(),
        Some(added["version"].as_i64().unwrap() + 1)
    );
    assert_eq!(changed["content_type_bases"], json!({}));
    assert_eq!(changed["taxonomy_bases"], json!({}));
    route(
        &raw_client,
        &base,
        "/library-c4/inherited-entry",
        StatusCode::OK,
        None,
    )
    .await;
    route(
        &raw_client,
        &base,
        "/genres-c4-next/fiction-c4",
        StatusCode::OK,
        None,
    )
    .await;
    for old in [&initial_entry[..], "/catalog-c4/inherited-entry"] {
        route(
            &raw_client,
            &base,
            old,
            StatusCode::MOVED_PERMANENTLY,
            Some("/library-c4/inherited-entry"),
        )
        .await;
    }
    for old in [&initial_term[..], "/topics-c4/fiction-c4"] {
        route(
            &raw_client,
            &base,
            old,
            StatusCode::MOVED_PERMANENTLY,
            Some("/genres-c4-next/fiction-c4"),
        )
        .await;
    }

    // A conflicting update by the *same owner* must leave the previous
    // installed revision, configuration version, canonical routes and links.
    let mut collision = manifest.clone();
    collision["version"] = json!("1.0.3");
    collision["declarations"]["content_types"][0]["rewrite_base"] = json!("genres-c4-next");
    let rejected = client
        .post(format!("{base}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base)
        .header("x-csrf-token", &csrf)
        .header("content-type", "application/zip")
        .body(plugin_zip(
            &collision,
            &wasm_output(r#"{"decision":"allow"}"#),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        rejected.status(),
        StatusCode::CONFLICT,
        "failed same-owner rewrite update: {}",
        rejected.text().await.unwrap()
    );
    assert_eq!(settings(&client, &base, &cookie).await, changed);
    let installed = expect_json(
        client
            .get(format!("{base}/api/admin/plugins"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    let active = installed
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "c4-provider")
        .unwrap();
    assert_eq!(active["version"], "1.0.2");
    route(
        &raw_client,
        &base,
        "/library-c4/inherited-entry",
        StatusCode::OK,
        None,
    )
    .await;
    route(
        &raw_client,
        &base,
        "/genres-c4-next/fiction-c4",
        StatusCode::OK,
        None,
    )
    .await;
    assert_eq!(
        entry(&client, &base, &cookie, entry_id).await["term_ids"],
        json!([term_id])
    );

    // Withdraw only the rewrite defaults in a compatible newer revision.
    // The provider owns the same CPT and term; legacy paths are canonical
    // again, prior rewrite bases remain historical direct-to-target aliases.
    manifest["version"] = json!("1.0.4");
    manifest["declarations"]["content_types"][0]
        .as_object_mut()
        .unwrap()
        .remove("rewrite_base");
    manifest["declarations"]["taxonomy_definitions"][0]
        .as_object_mut()
        .unwrap()
        .remove("rewrite_base");
    install(
        &client,
        &base,
        &cookie,
        &csrf,
        manifest,
        wasm_output(r#"{"decision":"allow"}"#),
    )
    .await;
    let removed = settings(&client, &base, &cookie).await;
    assert_eq!(
        removed["version"].as_i64(),
        Some(changed["version"].as_i64().unwrap() + 1)
    );
    assert_eq!(removed["content_type_bases"], json!({}));
    assert_eq!(removed["taxonomy_bases"], json!({}));
    route(&raw_client, &base, &initial_entry, StatusCode::OK, None).await;
    route(&raw_client, &base, &initial_term, StatusCode::OK, None).await;
    for old in ["/catalog-c4/inherited-entry", "/library-c4/inherited-entry"] {
        route(
            &raw_client,
            &base,
            old,
            StatusCode::MOVED_PERMANENTLY,
            Some(&initial_entry),
        )
        .await;
    }
    for old in ["/topics-c4/fiction-c4", "/genres-c4-next/fiction-c4"] {
        route(
            &raw_client,
            &base,
            old,
            StatusCode::MOVED_PERMANENTLY,
            Some(&initial_term),
        )
        .await;
    }
    assert_eq!(
        entry(&client, &base, &cookie, entry_id).await["term_ids"],
        json!([term_id])
    );

    // Verify durable C3 claim/redirect targets point to stable database IDs,
    // not to another URL string that can form a redirect chain.
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(site.0.join("database/opendash.sqlite"))
                .read_only(true),
        )
        .await
        .unwrap();
    for (path, target_kind, stable_id) in [
        (initial_entry.as_str(), "entry", entry_id),
        (initial_term.as_str(), "term", term_id),
    ] {
        let actual: (String, String) =
            sqlx::query_as("SELECT target_kind,target_id FROM permalink_route_claims WHERE path=?")
                .bind(path)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(actual, (target_kind.to_owned(), stable_id.to_string()));
    }
    for (path, target_kind, stable_id) in [
        ("/catalog-c4/inherited-entry", "entry", entry_id),
        ("/library-c4/inherited-entry", "entry", entry_id),
        ("/topics-c4/fiction-c4", "term", term_id),
        ("/genres-c4-next/fiction-c4", "term", term_id),
    ] {
        let actual: (String, String) = sqlx::query_as(
            "SELECT target_kind,target_id FROM permalink_route_redirects WHERE source_path=?",
        )
        .bind(path)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(actual, (target_kind.to_owned(), stable_id.to_string()));
    }
    let defaults: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT resource_kind,rewrite_base FROM provider_rewrite_defaults
         WHERE owner_kind='plugin' AND owner_id='c4-provider' ORDER BY resource_kind",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(
        defaults,
        vec![
            ("content_type".to_owned(), None),
            ("taxonomy".to_owned(), None)
        ]
    );
}

/// Upgrade a populated, *actual* 0040 database in-place through the native
/// server startup migrator. Previously selected non-null and explicit-null
/// site bases, published canonical claims, and stable-ID redirect history must
/// survive 0041 verbatim (no inferred provider ownership/backfill).
#[tokio::test]
async fn c4_upgrade_0040_to_0041_keeps_published_routes_site_choices_and_history() {
    let site = Site::new();
    let db_dir = site.0.join("database");
    fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("opendash.sqlite");
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .unwrap();
    sqlx::migrate!("../../migrations")
        .run_to(40, &db)
        .await
        .unwrap();

    let type_key = "legacy_books_c4";
    let kind = "legacy_genre_c4";
    let definition = json!({
        "key":type_key,"label":"Historical Books","fields":[],
        "settings":{"public":true,"has_archive":true,
            "publication":"draft_publish","supports":["editor"]}
    });
    sqlx::query("INSERT INTO content_types(key,definition_json,owner_kind) VALUES(?,?,'user')")
        .bind(type_key)
        .bind(definition.to_string())
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO taxonomy_definitions(key,label,hierarchical,public,owner_kind)
         VALUES(?,'Legacy genre',0,1,'user')",
    )
    .bind(kind)
    .execute(&db)
    .await
    .unwrap();
    let entry_id = sqlx::query(
        "INSERT INTO content_entries
         (content_type_key,slug,title,values_json,publication_status)
         VALUES(?,'historic-book','Historical book','{}','published')",
    )
    .bind(type_key)
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let term_id = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug) VALUES(?,'Historical genre','historic-term')",
    )
    .bind(kind)
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query(
        "UPDATE site_permalink_settings SET content_type_bases_json=?,taxonomy_bases_json=?
         WHERE id=1",
    )
    .bind(json!({(type_key):"issues-c4"}).to_string())
    .bind(json!({(kind):Value::Null}).to_string())
    .execute(&db)
    .await
    .unwrap();
    let canonical_entry = "/issues-c4/historic-book";
    let pre0045_term = format!("/taxonomy/{kind}/historic-term");
    let rebuilt_term = format!("/{kind}/historic-term");
    for (path, target_kind, target_id) in [
        (canonical_entry.to_owned(), "entry", entry_id.to_string()),
        ("/issues-c4".to_owned(), "archive", type_key.to_owned()),
        (pre0045_term.clone(), "term", term_id.to_string()),
    ] {
        sqlx::query("INSERT INTO permalink_route_claims(path,target_kind,target_id) VALUES(?,?,?)")
            .bind(path)
            .bind(target_kind)
            .bind(target_id)
            .execute(&db)
            .await
            .unwrap();
    }
    let legacy_entry = format!("/content/{type_key}/historic-book");
    for (path, target_kind, target_id) in [
        (legacy_entry.clone(), "entry", entry_id.to_string()),
        (
            "/earlier-books-c4/historic-book".to_owned(),
            "entry",
            entry_id.to_string(),
        ),
        (
            "/earlier-genres-c4/historic-term".to_owned(),
            "term",
            term_id.to_string(),
        ),
    ] {
        sqlx::query(
            "INSERT INTO permalink_route_redirects(source_path,target_kind,target_id)
             VALUES(?,?,?)",
        )
        .bind(path)
        .bind(target_kind)
        .bind(target_id)
        .execute(&db)
        .await
        .unwrap();
    }
    // Pin the exact 0040 ledger and route history independently of the
    // server's HTTP resolver; 0041 is additive-only and 0045 rebuilds claims to WordPress defaults.
    let claims_before: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT path,target_kind,target_id FROM permalink_route_claims ORDER BY path",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    let redirects_before: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT source_path,target_kind,target_id FROM permalink_route_redirects ORDER BY source_path",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    drop(db);

    let port = vacant_port();
    let _server = serve(&site.0, port).await;
    let base = format!("http://127.0.0.1:{port}");
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let (cookie, _) = auth(&client, &base).await;
    let settings = expect_json(
        client
            .get(format!("{base}/api/admin/settings/permalinks"))
            .header("Cookie", cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(settings["content_type_bases"][type_key], "issues-c4");
    assert_eq!(settings["taxonomy_bases"][kind], Value::Null);
    assert_eq!(settings["version"], 1);
    assert_eq!(
        client
            .get(format!("{base}{canonical_entry}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
    );
    assert_eq!(
        client
            .get(format!("{base}{rebuilt_term}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK,
    );
    // Under WordPress permalinks (0045), /content/... is a reserved system root
    // that always returns 404 (no fallback), while non-reserved historical
    // aliases retain single-hop 301 redirects to canonical URLs.
    assert_eq!(
        client
            .get(format!("{base}{legacy_entry}"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND,
        "{legacy_entry} must return 404 after WordPress default migration"
    );
    for (old, new) in [
        ("/earlier-books-c4/historic-book", canonical_entry),
        ("/earlier-genres-c4/historic-term", rebuilt_term.as_str()),
    ] {
        let response = client.get(format!("{base}{old}")).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::MOVED_PERMANENTLY, "{old}");
        assert_eq!(
            response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok()),
            Some(new),
            "upgrade must retain stable-ID redirect to canonical URL",
        );
    }
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&db_path)
                .read_only(true),
        )
        .await
        .unwrap();
    let migration_version: i64 =
        sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success=1")
            .fetch_one(&db)
            .await
            .unwrap();
    // EGM Spec 2 S02 adds migration 0044's private Pattern data conversion,
    // WordPress Permalinks adds 0045, Declarative Block Engine S02 adds 0046,
    // Multilingual Content & Theme L10n S01 adds 0047, Global Site Options
    // adds 0048, Theme Ergonomics adds 0049 (`published_at`), and Spec 2
    // Plugin Extensibility S02 adds 0050 (owner PK fix + `plugin_entry_projections`);
    // Admin i18n S01 adds 0051 (CMS locale and multilingual settings columns),
    // Taxonomy Featured Image adds 0052 (`taxonomy_terms.featured_image_media_id`),
    // and Spec B S01 adds 0053 (owner_kind isolation for Theme Functions runtime),
    // and Entry Lifecycle S01 adds 0054 (`content_entries.trashed_at` + lifecycle indexes);
    // the 0040→0041 permalink preservation assertions above must still hold.
    assert_eq!(migration_version, 54);
    let defaults: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_rewrite_defaults")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(defaults, 0, "0041 must never infer defaults from site maps");
    let claims_after: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT path,target_kind,target_id FROM permalink_route_claims ORDER BY path",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    let redirects_after: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT source_path,target_kind,target_id FROM permalink_route_redirects ORDER BY source_path",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    for expected in [
        (
            "/issues-c4".to_owned(),
            "archive".to_owned(),
            type_key.to_owned(),
        ),
        (
            canonical_entry.to_owned(),
            "entry".to_owned(),
            entry_id.to_string(),
        ),
        (rebuilt_term.clone(), "term".to_owned(), term_id.to_string()),
    ] {
        assert!(
            claims_after.contains(&expected),
            "missing expected claim {expected:?} in {claims_after:?}"
        );
    }
    assert!(
        claims_before
            .iter()
            .any(|(p, k, id)| p == "/issues-c4" && k == "archive" && id == type_key)
    );
    assert_eq!(redirects_after, redirects_before);
}
