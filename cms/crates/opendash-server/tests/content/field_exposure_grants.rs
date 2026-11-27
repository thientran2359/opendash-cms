use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "opendash-m7-s09-t02-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RunningServer {
    child: Option<Child>,
    base_url: String,
}

impl RunningServer {
    async fn start(site_root: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let bin = env!("CARGO_BIN_EXE_opendash");
        let child = Command::new(bin)
            .env("OPENDASH_SITE_ROOT", site_root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn opendash server");

        let base_url = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..100 {
            if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await {
                if resp.status() == StatusCode::OK {
                    return Self {
                        child: Some(child),
                        base_url,
                    };
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("server failed to become ready at {base_url}");
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
        for (name, data) in entries {
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }
    buf
}

fn make_png(r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut img = image::RgbaImage::new(4, 4);
    for px in img.pixels_mut() {
        *px = image::Rgba([r, g, b, 255]);
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

async fn upload_png(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    name: &str,
    bytes: Vec<u8>,
) -> String {
    let resp = client
        .post(format!("{base}/api/admin/media"))
        .header("Cookie", cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header("Content-Type", "image/png")
        .header("X-File-Name", name)
        .body(bytes)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = resp.json().await.unwrap();
    body["id"].as_str().unwrap().to_string()
}

async fn setup_and_login_admin(client: &Client, base: &str) -> (String, String) {
    let setup = client
        .post(format!("{base}/auth/setup"))
        .json(&json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    login(client, base, "admin", "Password123!").await
}

async fn login(client: &Client, base: &str, username: &str, password: &str) -> (String, String) {
    let resp = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = resp.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_string())
}

fn wat_bytes(src: &str) -> Vec<u8> {
    wat::parse_str(src).expect("valid wat")
}

fn escape_wat_bytes(s: &str) -> String {
    s.as_bytes().iter().map(|b| format!("\\{b:02x}")).collect()
}

/// Builds a `content/before_save` hook plugin that checks whether `input.payload.candidate_values`
/// contains `secret_key`; if it DOES contain `secret_key`, it vetoes with `code: "saw_secret"`,
/// otherwise it allows (`{"decision":"allow"}`).
fn build_hook_probe_wasm(secret_key: &str) -> Vec<u8> {
    let allow = r#"{"decision":"allow"}"#;
    let veto = r#"{"decision":"veto","code":"saw_secret","message":"Observed secret field","field_errors":{}}"#;
    let wat = format!(
        r#"(module
          (import "opendash_v1" "host_read_input" (func $in (param i32 i32) (result i32)))
          (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
          (memory (export "memory") 2)
          (data (i32.const 0) "{}")
          (data (i32.const 256) "{}")
          (data (i32.const 512) "{}")
          (func (export "opendash_run") (result i32)
            (local $len i32)
            (local $i i32)
            (local $j i32)
            (local $found i32)
            (local.set $len (call $in (i32.const 4096) (i32.const 32768)))
            (local.set $found (i32.const 0))
            (local.set $i (i32.const 0))
            (block $done
              (loop $outer
                (br_if $done (i32.gt_u (i32.add (local.get $i) (i32.const {})) (local.get $len)))
                (local.set $j (i32.const 0))
                (block $mismatch
                  (loop $inner
                    (br_if $mismatch (i32.ne
                      (i32.load8_u (i32.add (i32.const 4096) (i32.add (local.get $i) (local.get $j))))
                      (i32.load8_u (i32.add (i32.const 0) (local.get $j)))
                    ))
                    (local.set $j (i32.add (local.get $j) (i32.const 1)))
                    (if (i32.eq (local.get $j) (i32.const {}))
                      (then
                        (local.set $found (i32.const 1))
                        (br $done)
                      )
                    )
                    (br $inner)
                  )
                )
                (local.set $i (i32.add (local.get $i) (i32.const 1)))
                (br $outer)
              )
            )
            (if (i32.eq (local.get $found) (i32.const 1))
              (then (drop (call $out (i32.const 512) (i32.const {}))))
              (else (drop (call $out (i32.const 256) (i32.const {}))))
            )
            (i32.const 0)
          )
        )"#,
        escape_wat_bytes(secret_key),
        escape_wat_bytes(allow),
        escape_wat_bytes(veto),
        secret_key.len(),
        secret_key.len(),
        veto.len(),
        allow.len(),
    );
    wat_bytes(&wat)
}

/// Builds an action plugin that stages a `content_save` host call with the given JSON payload.
fn build_content_save_action_wasm(cmd_json: &str) -> Vec<u8> {
    let out_json = r#"{"ok":true}"#;
    let wat = format!(
        r#"(module
          (import "opendash_v1" "host_call" (func $call (param i32 i32 i32 i32) (result i32)))
          (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
          (memory (export "memory") 2)
          (data (i32.const 0) "{}")
          (data (i32.const 4096) "{}")
          (func (export "opendash_run") (result i32)
            (drop (call $call (i32.const 0) (i32.const {}) (i32.const 8192) (i32.const 4096)))
            (drop (call $out (i32.const 4096) (i32.const {})))
            (i32.const 0)
          )
        )"#,
        escape_wat_bytes(cmd_json),
        escape_wat_bytes(out_json),
        cmd_json.len(),
        out_json.len(),
    );
    wat_bytes(&wat)
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
async fn public_exposure_media_branches_and_plugin_scoped_field_grants() {
    let dir = TempDirGuard::new("exposure-grants");
    let srv = RunningServer::start(dir.path()).await;
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_and_login_admin(&client, &srv.base_url).await;

    // Create Editor user
    assert_eq!(
        client
            .post(format!("{}/api/admin/users", srv.base_url))
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({ "username": "editor1", "password": "EditorPassword123!", "role": "editor" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    let (ed_cookie, ed_csrf) = login(&client, &srv.base_url, "editor1", "EditorPassword123!").await;

    // 1. Upload media for AC-05a,b,c
    let img_private_only = upload_png(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        "priv.png",
        make_png(200, 10, 10),
    )
    .await;
    let img_public_base = upload_png(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        "pub.png",
        make_png(10, 200, 10),
    )
    .await;
    let img_dual_use = upload_png(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        "dual.png",
        make_png(10, 10, 200),
    )
    .await;
    let img_home_builder = upload_png(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        "home.png",
        make_png(200, 200, 10),
    )
    .await;

    // Create CPT `portfolio` with Public + Private fields
    assert_eq!(
        client
            .post(format!("{}/api/admin/content-types", srv.base_url))
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({
                "key": "portfolio",
                "label": "Dự án",
                "fields": [
                    { "key": "pub_summary", "label": "Tóm tắt công khai", "type": "text", "acl": {"public_exposure": "public"} },
                    { "key": "pub_photo", "label": "Ảnh công khai", "type": "media", "acl": {"public_exposure": "public"} },
                    { "key": "secret_note", "label": "Ghi chú nội bộ", "type": "text", "acl": {"public_exposure": "private"} },
                    { "key": "priv_photo", "label": "Ảnh nội bộ", "type": "media", "acl": {"public_exposure": "private"} },
                    { "key": "dual_photo", "label": "Ảnh dùng chung", "type": "media", "acl": {"public_exposure": "private"} }
                ]
            }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );

    // Create published `portfolio` entry
    let entry_resp = client
        .post(format!(
            "{}/api/admin/content-types/portfolio/entries",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "title": "Dự án A",
            "slug": "du-an-a",
            "values": {
                "pub_summary": "PUBLIC_TEXT_VISIBLE",
                "pub_photo": img_public_base,
                "secret_note": "PRIVATE_SECRET_DO_NOT_LEAK",
                "priv_photo": img_private_only,
                "dual_photo": img_dual_use
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(entry_resp.status(), StatusCode::CREATED);

    // Set `img_dual_use` on `home_seo` (Branch 5: seo_public_media_refs)
    assert_eq!(
        client
            .put(format!("{}/api/admin/seo/home", srv.base_url))
            .header("If-Match", "\"v1\"")
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({ "og_image_id": img_dual_use }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    // Pin img_home_builder through the published Reading Page's Builder document.
    let home_id = create_s03_home(&client, &srv.base_url, &admin_cookie, &admin_csrf).await;
    let home_doc = json!({
        "schema_version": 1,
        "blocks": [{"id":"sec-1","block_id":"core/section","children":[{"id":"row-1","block_id":"core/row","children":[
            {"id":"col-1","block_id":"core/column","children":[
                {"id":"img-1","block_id":"core/image","props":{"image":img_home_builder,"alt":"Home Banner"}}
            ]}
        ]}]}]
    });
    let home_put = save_s03_home_doc(
        &client,
        &srv.base_url,
        &admin_cookie,
        &admin_csrf,
        home_id,
        2,
        &home_doc,
    )
    .await;
    let home_status = home_put.status();
    let home_err = home_put.text().await.unwrap();
    assert_eq!(
        home_status,
        StatusCode::OK,
        "Reading Page Builder failed: {home_err}"
    );

    // AC-05a,b,c assertions:
    // - Private-only image -> 404 publicly, 409 on DELETE (pinned!)
    assert_eq!(
        client
            .get(format!("{}/media/{img_private_only}/file", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .delete(format!(
                "{}/api/admin/media/{img_private_only}",
                srv.base_url
            ))
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .header("If-Match", "\"gv1-v1\"")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    // - Public image, dual-use image (private field + home_seo), and builder_homepage image -> 200 OK
    for pub_id in [&img_public_base, &img_dual_use, &img_home_builder] {
        assert_eq!(
            client
                .get(format!("{}/media/{pub_id}/file", srv.base_url))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }

    // - Public single page must show PUBLIC_TEXT_VISIBLE and must NOT leak PRIVATE_SECRET_DO_NOT_LEAK or img_private_only
    let pub_html = client
        .get(format!("{}/portfolio/du-an-a", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(pub_html.contains("PUBLIC_TEXT_VISIBLE"));
    assert!(!pub_html.contains("PRIVATE_SECRET_DO_NOT_LEAK"));
    assert!(!pub_html.contains(&img_private_only));

    // 2. AC-06a: Reject manifest_schema: 1 with content_read_private and wildcard "*"
    let bad_v1_zip = build_zip(&[
        (
            "manifest.json",
            &serde_json::to_vec(&json!({
                "id": "bad-v1-priv",
                "version": "1.0.0",
                "api_version": 1,
                "manifest_schema": 1,
                "name": "Bad V1",
                "permissions": {
                    "content_read": ["portfolio"],
                    "content_read_private": [{ "content_type": "portfolio", "field_keys": ["secret_note"] }]
                }
            }))
            .unwrap(),
        ),
        ("plugin.wasm", &build_hook_probe_wasm("secret_note")),
    ]);
    assert_eq!(
        client
            .post(format!("{}/api/admin/plugins", srv.base_url))
            .header("Content-Type", "application/zip")
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .body(bad_v1_zip)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );

    // 3. AC-06b: Plugin without ScopedFieldGrant must NOT see `secret_note` in `content/before_save`
    let basic_zip = build_zip(&[
        (
            "manifest.json",
            &serde_json::to_vec(&json!({
                "id": "basic-hook",
                "version": "1.0.0",
                "api_version": 1,
                "manifest_schema": 1,
                "name": "Basic Hook",
                "permissions": { "content_read": ["portfolio"] },
                "hooks": ["content/before_save"]
            }))
            .unwrap(),
        ),
        ("plugin.wasm", &build_hook_probe_wasm("secret_note")),
    ]);
    let basic_inst = client
        .post(format!("{}/api/admin/plugins", srv.base_url))
        .header("Content-Type", "application/zip")
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .body(basic_zip)
        .send()
        .await
        .unwrap();
    let basic_status = basic_inst.status();
    let basic_body = basic_inst.text().await.unwrap();
    assert_eq!(
        basic_status,
        StatusCode::CREATED,
        "basic_hook install failed: {basic_body}"
    );

    // Saving an entry with `secret_note`: `basic-hook` does NOT see `secret_note`, so it ALLOWs (201)!
    let create_b = client
        .post(format!(
            "{}/api/admin/content-types/portfolio/entries",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "title": "Dự án B",
            "slug": "du-an-b",
            "values": {
                "pub_summary": "Hello",
                "secret_note": "PRIVATE_VALUE"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_b.status(), StatusCode::CREATED);

    // Now install `grant-hook` (manifest_schema: 2 with ScopedFieldGrant for `secret_note`) -> it DOES see `secret_note` and vetoes with `saw_secret` (422)!
    let grant_zip = build_zip(&[
        (
            "manifest.json",
            &serde_json::to_vec(&json!({
                "id": "grant-hook",
                "version": "1.0.0",
                "api_version": 1,
                "manifest_schema": 2,
                "min_host_version": "0.1.0",
                "name": "Grant Hook",
                "permissions": {
                    "content_read": ["portfolio"],
                    "content_read_private": [{ "content_type": "portfolio", "field_keys": ["secret_note"] }]
                },
                "hooks": ["content/before_save"]
            }))
            .unwrap(),
        ),
        ("plugin.wasm", &build_hook_probe_wasm("secret_note")),
    ]);
    assert_eq!(
        client
            .post(format!("{}/api/admin/plugins", srv.base_url))
            .header("Content-Type", "application/zip")
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .body(grant_zip)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    let create_c = client
        .post(format!(
            "{}/api/admin/content-types/portfolio/entries",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "title": "Dự án C",
            "slug": "du-an-c",
            "values": {
                "pub_summary": "Hello",
                "secret_note": "PRIVATE_VALUE"
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_c.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(create_c.text().await.unwrap().contains("saw_secret"));

    // 4. AC-06c: Action with `required_role = "editor"` keeps Admin privileges when called by Admin,
    //    and returns 403 `field_edit_forbidden` when called by Editor on `min_edit_role = "admin"` field.
    assert_eq!(
        client
            .post(format!("{}/api/admin/content-types", srv.base_url))
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({
                "key": "deal",
                "label": "Giao dịch",
                "fields": [
                    { "key": "deal_title", "label": "Tên", "type": "text", "required": true },
                    {
                        "key": "admin_margin",
                        "label": "Biên lợi nhuận",
                        "type": "number",
                        "required": true,
                        "default": 10,
                        "acl": {"min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private"}
                    }
                ]
            }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );

    let cmd = json!({
        "op": "content_save",
        "type_key": "deal",
        "title": "Deal 1",
        "slug": "deal-1",
        "values": {
            "deal_title": "Alpha",
            "admin_margin": 42
        }
    })
    .to_string();
    let action_zip = build_zip(&[
        (
            "manifest.json",
            &serde_json::to_vec(&json!({
                "id": "deal-sync",
                "version": "1.0.0",
                "api_version": 1,
                "manifest_schema": 2,
                "min_host_version": "0.1.0",
                "name": "Deal Sync",
                "permissions": {
                    "content_read": ["deal"],
                    "content_write": ["deal"],
                    "content_read_private": [{ "content_type": "deal", "field_keys": ["admin_margin"] }],
                    "content_write_private": [{ "content_type": "deal", "field_keys": ["admin_margin"] }]
                },
                "actions": [{
                    "id": "sync_deal",
                    "title": "Sync Deal",
                    "required_role": "editor",
                    "capabilities": ["content_read", "content_read_private", "content_write", "content_write_private"]
                }]
            }))
            .unwrap(),
        ),
        ("plugin.wasm", &build_content_save_action_wasm(&cmd)),
    ]);
    assert_eq!(
        client
            .post(format!("{}/api/admin/plugins", srv.base_url))
            .header("Content-Type", "application/zip")
            .header("Cookie", &admin_cookie)
            .header("Origin", &srv.base_url)
            .header("X-CSRF-Token", &admin_csrf)
            .body(action_zip)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );

    // Editor calling `sync_deal` writing `admin_margin` -> 403 `field_edit_forbidden`
    let ed_action = client
        .post(format!(
            "{}/api/admin/plugins/deal-sync/actions/sync_deal",
            srv.base_url
        ))
        .header("Cookie", &ed_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &ed_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let ed_status = ed_action.status();
    let ed_body = ed_action.text().await.unwrap();
    assert_eq!(
        ed_status,
        StatusCode::FORBIDDEN,
        "ed_action unexpected: {ed_body}"
    );
    assert!(ed_body.contains("field_edit_forbidden"));

    // Admin calling `sync_deal` (`required_role = "editor"`) -> 200 OK (not downgraded!)
    let adm_action = client
        .post(format!(
            "{}/api/admin/plugins/deal-sync/actions/sync_deal",
            srv.base_url
        ))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(adm_action.status(), StatusCode::OK);
}
