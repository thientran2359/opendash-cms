//! EGM Spec 2 S01: any opted-in CPT owns Builder AST independently of Editor
//! values, with one entry CAS and no implicit template/other-entry mutation.
use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use opendash_core::content::{ContentMode, ContentSupport, ContentTypeSettings};
use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

struct Site {
    child: Child,
    root: PathBuf,
    base: String,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

async fn start() -> Site {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "opendash-egm-cpt-s01-{}-{port}-{nanos}",
            std::process::id()
        ));
    fs::create_dir_all(&root).unwrap();
    let stderr = fs::File::create(root.join("server.log")).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_SITE_ROOT", &root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
        .unwrap();
    let base = format!("http://127.0.0.1:{port}");
    let http = Client::new();
    for _ in 0..400 {
        if http
            .get(format!("{base}/api/health"))
            .send()
            .await
            .is_ok_and(|res| res.status() == StatusCode::OK)
        {
            return Site { child, root, base };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("isolated EGM S01 server not ready");
}

async fn admin(http: &Client, site: &Site) -> (String, String) {
    let setup = http
        .post(format!("{}/auth/setup", site.base))
        .json(&json!({"username":"admin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = http
        .post(format!("{}/auth/login", site.base))
        .json(&json!({"username":"admin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let auth: Value = login.json().await.unwrap();
    (cookie, auth["csrf_token"].as_str().unwrap().to_string())
}

fn type_input(builder: bool) -> Value {
    let mut supports = vec!["editor", "featured_image"];
    if builder {
        supports.push("builder");
    }
    json!({
        "key": "project",
        "label": "Projects",
        "fields": [{"key":"body","label":"Project text","type":"richtext"}],
        "settings": {
            "public":true,"has_archive":true,"publication":"draft_publish","supports":supports
        }
    })
}

fn document(heading: &str) -> Value {
    json!({
        "schema_version":1,
        "sections":[{"id":"section","children":[
            {"id":"row","children":[
                {"id":"column","children":[
                    {"id":"heading","type":"core/heading","props":{"level":2,"text":heading}}
                ]}
            ]}
        ]}]
    })
}

#[test]
fn page_default_enabled_post_default_disabled_and_pattern_private() {
    let page = ContentTypeSettings::default_for_type_key("page");
    let post = ContentTypeSettings::default_for_type_key("post");
    let pattern = ContentTypeSettings::default_for_type_key("pattern");
    assert!(page.supports(ContentSupport::Builder));
    assert!(!page.has_archive);
    assert!(!post.supports(ContentSupport::Builder));
    assert!(post.has_archive);
    assert!(pattern.supports(ContentSupport::Builder));
    assert!(!pattern.public);
    assert!(!pattern.has_archive);
    assert_eq!(ContentMode::Editor.as_str(), "editor");
}

#[tokio::test]
async fn cpt_builder_capability_cas_body_isolation_and_disable_preservation() {
    let site = start().await;
    let http = Client::new();
    let (cookie, csrf) = admin(&http, &site).await;
    let catalog: Vec<Value> = http
        .get(format!("{}/api/admin/content-types", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let page = catalog
        .iter()
        .find(|row| row["descriptor"]["key"] == "page")
        .unwrap();
    let post = catalog
        .iter()
        .find(|row| row["descriptor"]["key"] == "post")
        .unwrap();
    assert_eq!(page["descriptor"]["settings"]["has_archive"], false);
    assert_eq!(post["descriptor"]["settings"]["has_archive"], true);
    assert!(
        page["descriptor"]["settings"]["supports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|support| support == "builder")
    );
    assert!(
        !post["descriptor"]["settings"]["supports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|support| support == "builder")
    );
    let created = http
        .post(format!("{}/api/admin/content-types", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&type_input(false))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(created.headers()[header::ETAG], "\"v1\"");
    for (slug, text) in [
        ("alpha", "Original Alpha Body"),
        ("beta", "Original Beta Body"),
    ] {
        let res = http
            .post(format!(
                "{}/api/admin/content-types/project/entries",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({"title":slug,"slug":slug,"values":{"body":text}}))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }
    let entries: Vec<Value> = http
        .get(format!(
            "{}/api/admin/content-types/project/entries",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let alpha = entries.iter().find(|row| row["slug"] == "alpha").unwrap();
    let beta = entries.iter().find(|row| row["slug"] == "beta").unwrap();
    let a = alpha["id"].as_i64().unwrap();
    let b = beta["id"].as_i64().unwrap();
    let url = format!("{}/api/admin/entries/project/{a}/builder", site.base);
    let auth_get = || http.get(&url).header(header::COOKIE, &cookie);
    assert_eq!(
        auth_get().send().await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    // Exercise an actual Post ID, not an entry of a different content type.
    let post_created = http
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"Post S01","slug":"post-s01","values":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(post_created.status(), StatusCode::CREATED);
    let post_id = post_created.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    let disabled_post_builder = http
        .get(format!(
            "{}/api/admin/entries/post/{post_id}/builder",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    // Post has no Builder capability by default, even when a Post exists.
    assert_eq!(disabled_post_builder.status(), StatusCode::FORBIDDEN);

    let enabling = http
        .put(format!("{}/api/admin/content-types/project", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&type_input(true))
        .send()
        .await
        .unwrap();
    assert_eq!(enabling.status(), StatusCode::OK);
    assert_eq!(enabling.headers()[header::ETAG], "\"v2\"");
    let before: Value = auth_get().send().await.unwrap().json().await.unwrap();
    assert_eq!(before["version"], 1);
    assert!(before["document"].is_object());
    let wrapped_before = before["document"].to_string();
    assert!(wrapped_before.contains("Original Alpha Body"));
    let doc = document("Project A Design");
    let expected_raw_body = opendash_core::builder_blocks::serialize_builder_document_to_blocks(
        &serde_json::from_value::<opendash_core::builder::BuilderDocument>(doc.clone()).unwrap(),
    )
    .unwrap();
    let missing_if_match = http
        .put(&url)
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&doc)
        .send()
        .await
        .unwrap();
    assert_eq!(missing_if_match.status(), StatusCode::PRECONDITION_REQUIRED);
    let saved = http
        .put(&url)
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&doc)
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    assert_eq!(saved.headers()[header::ETAG], "\"v2\"");
    let payload: Value = saved.json().await.unwrap();
    assert_eq!(payload["entry_id"], a);
    assert_eq!(payload["version"], 2);
    assert_eq!(payload["document"], doc);
    assert_eq!(payload["content_mode"], "builder");
    let returned_updated_at = payload["updated_at"]
        .as_str()
        .expect("Builder PUT returns content entry updated_at");
    assert!(!returned_updated_at.is_empty());
    let db = SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(site.root.join("database/opendash.sqlite")),
    )
    .await
    .unwrap();
    // Unified storage: non-pattern entries store Builder AST inside
    // content_entries.values_json["body"] and leave entry_builder_documents empty.
    let doc_row_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_builder_documents WHERE entry_id = ?")
            .bind(a)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(doc_row_count, 0);
    let persisted: (i64, String, String) = sqlx::query_as(
        "SELECT version, updated_at, values_json
           FROM content_entries
          WHERE id=?",
    )
    .bind(a)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(persisted.0, payload["version"].as_i64().unwrap());
    assert_eq!(persisted.1, returned_updated_at);
    let persisted_body = serde_json::from_str::<Value>(&persisted.2).unwrap()["body"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(persisted_body, expected_raw_body);
    assert_eq!(
        opendash_core::builder_blocks::parse_blocks_to_builder_document(&persisted_body)
            .unwrap()
            .document,
        serde_json::from_value::<opendash_core::builder::BuilderDocument>(doc.clone()).unwrap()
    );
    let reopened: Value = auth_get().send().await.unwrap().json().await.unwrap();
    assert_eq!(reopened["version"], payload["version"]);
    assert_eq!(reopened["updated_at"], payload["updated_at"]);
    assert_eq!(reopened["document"], doc);
    let stale = http
        .put(&url)
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&document("Stale design must not replace committed AST"))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    let after_stale: (i64, String, String) = sqlx::query_as(
        "SELECT version, updated_at, values_json
           FROM content_entries
          WHERE id=?",
    )
    .bind(a)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        after_stale, persisted,
        "stale 412 must preserve committed row"
    );
    let beta_url = format!("{}/api/admin/entries/project/{b}/builder", site.base);
    let beta_after: Value = http
        .get(&beta_url)
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(beta_after["version"], 1);
    assert!(
        beta_after["document"]
            .to_string()
            .contains("Original Beta Body")
    );

    let (raw_values, mode): (String, String) =
        sqlx::query_as("SELECT values_json, content_mode FROM content_entries WHERE id=?")
            .bind(a)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(mode, "builder");
    assert_eq!(
        serde_json::from_str::<Value>(&raw_values).unwrap()["body"],
        expected_raw_body
    );

    let disabling = http
        .put(format!("{}/api/admin/content-types/project", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&type_input(false))
        .send()
        .await
        .unwrap();
    assert_eq!(disabling.status(), StatusCode::OK);
    assert_eq!(
        auth_get().send().await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let retained: String = sqlx::query_scalar("SELECT values_json FROM content_entries WHERE id=?")
        .bind(a)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&retained).unwrap()["body"],
        expected_raw_body
    );

    // Even while Builder is disabled, Editor mode may be explicitly restored
    // while preserving or updating the unified values_json["body"].
    let types: Vec<Value> = http
        .get(format!("{}/api/admin/content-types", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = types
        .iter()
        .find(|row| row["descriptor"]["key"] == "project")
        .or_else(|| {
            types
                .iter()
                .find(|row| row["base"]["descriptor"]["key"] == "project")
        })
        .unwrap()["schema_token"]
        .as_str()
        .unwrap();
    let updated = http
        .put(format!(
            "{}/api/admin/content-types/project/entries/{a}/composite",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "title": "alpha",
            "slug": "alpha",
            "values": {"body": expected_raw_body},
            "content_mode": "editor",
            "expected_version": 2,
            "expected_schema_token": token
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    let after_mode: (String, String) =
        sqlx::query_as("SELECT content_mode, values_json FROM content_entries WHERE id=?")
            .bind(a)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(after_mode.0, "editor");
    assert_eq!(
        serde_json::from_str::<Value>(&after_mode.1).unwrap()["body"],
        expected_raw_body
    );

    // Re-enabling the capability reloads the unified body blocks without erasing AST.
    let enabling_again = http
        .put(format!("{}/api/admin/content-types/project", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v3\"")
        .json(&type_input(true))
        .send()
        .await
        .unwrap();
    assert_eq!(enabling_again.status(), StatusCode::OK);
    let reloaded: Value = auth_get().send().await.unwrap().json().await.unwrap();
    assert_eq!(reloaded["document"], doc);
    assert_eq!(reloaded["content_mode"], "editor");

    // Classic Editor replacement overwrites unified values_json["body"], which
    // UX Builder auto-wraps on reopen rather than reviving stale AST.
    let types_after: Vec<Value> = http
        .get(format!("{}/api/admin/content-types", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token_after = types_after
        .iter()
        .find(|row| row["descriptor"]["key"] == "project")
        .or_else(|| {
            types_after
                .iter()
                .find(|row| row["base"]["descriptor"]["key"] == "project")
        })
        .unwrap()["schema_token"]
        .as_str()
        .unwrap();
    let replacement = http
        .put(format!(
            "{}/api/admin/content-types/project/entries/{a}/composite",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "title": "alpha",
            "slug": "alpha",
            "values": {"body": "<p>Classic Editor Replacement</p>"},
            "content_mode": "editor",
            "expected_version": 3,
            "expected_schema_token": token_after
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(replacement.status(), StatusCode::OK);
    let wrapped_after_classic: Value = auth_get().send().await.unwrap().json().await.unwrap();
    assert_eq!(wrapped_after_classic["content_mode"], "editor");
    assert!(
        wrapped_after_classic["document"]
            .to_string()
            .contains("Classic Editor Replacement")
    );
}

#[tokio::test]
async fn auto_injected_cpt_body_without_declared_fields_syncs_media_refs_and_acl() {
    let site = start().await;
    let http = Client::new();
    let (cookie, csrf) = admin(&http, &site).await;
    let created_type = http
        .post(format!("{}/api/admin/content-types", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "key": "case_study",
            "label": "Case Studies",
            "fields": [],
            "settings": {
                "public": true,
                "has_archive": true,
                "publication": "draft_publish",
                "supports": ["builder"]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created_type.status(), StatusCode::CREATED);

    let mut png = Vec::new();
    let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([40, 60, 80, 255]));
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let media_res = http
        .post(format!("{}/api/admin/media", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", "case.png")
        .body(png)
        .send()
        .await
        .unwrap();
    assert_eq!(media_res.status(), StatusCode::CREATED);
    let media_id = media_res.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let image_doc = serde_json::from_value::<opendash_core::builder::BuilderDocument>(json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec-1",
            "children": [{
                "id": "row-1",
                "children": [{
                    "id": "col-1",
                    "children": [{
                        "id": "img-1",
                        "type": "core/image",
                        "props": { "media_id": media_id, "alt": "Case visual" }
                    }]
                }]
            }]
        }]
    }))
    .unwrap();
    let body_markup =
        opendash_core::builder_blocks::serialize_builder_document_to_blocks(&image_doc).unwrap();

    let created_entry = http
        .post(format!(
            "{}/api/admin/content-types/case_study/entries",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "title": "Auto Body Study",
            "slug": "auto-body-study",
            "values": { "body": body_markup }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created_entry.status(), StatusCode::CREATED);
    let entry_id = created_entry.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let db = SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(site.root.join("database/opendash.sqlite")),
    )
    .await
    .unwrap();
    let ref_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entry_builder_media_refs WHERE entry_id = ? AND media_id = ?",
    )
    .bind(entry_id)
    .bind(&media_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        ref_count, 1,
        "auto-injected body creation must sync entry_builder_media_refs"
    );

    // Media attached via draft auto-injected body must stay private until publish.
    let unauth_before = http
        .get(format!("{}/media/{media_id}/file", site.base))
        .send()
        .await
        .unwrap();
    assert_ne!(unauth_before.status(), StatusCode::OK);

    let published = http
        .post(format!(
            "{}/api/admin/content-types/case_study/entries/{entry_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({"status": "published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::OK);

    // Works regardless of content_mode ("editor" by default on entry create).
    let unauth_after = http
        .get(format!("{}/media/{media_id}/file", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_after.status(), StatusCode::OK);
}

fn escape_wat_bytes(s: &str) -> String {
    s.as_bytes().iter().map(|b| format!("\\{b:02x}")).collect()
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

fn build_content_save_action_wasm(cmd_json: &str) -> Vec<u8> {
    assert!(cmd_json.len() < 4096);
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
    wat::parse_str(&wat).expect("valid wat")
}

#[tokio::test]
async fn plugin_content_save_syncs_entry_builder_media_refs_for_block_body() {
    let site = start().await;
    let http = Client::new();
    let (cookie, csrf) = admin(&http, &site).await;

    let mut png = Vec::new();
    let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([20, 100, 180, 255]));
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let media_res = http
        .post(format!("{}/api/admin/media", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", "plugin-block.png")
        .body(png)
        .send()
        .await
        .unwrap();
    assert_eq!(media_res.status(), StatusCode::CREATED);
    let media_id = media_res.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let doc = serde_json::from_value::<opendash_core::builder::BuilderDocument>(json!({
        "schema_version": 1,
        "sections": [{
            "id": "s1",
            "children": [{
                "id": "r1",
                "children": [{
                    "id": "c1",
                    "children": [{
                        "id": "i1",
                        "type": "core/image",
                        "props": { "media_id": media_id, "alt": "Plugin image" }
                    }]
                }]
            }]
        }]
    }))
    .unwrap();
    let body_markup =
        opendash_core::builder_blocks::serialize_builder_document_to_blocks(&doc).unwrap();
    let cmd = json!({
        "op": "content_save",
        "type_key": "page",
        "slug": "plugin-block-page",
        "title": "Plugin Block Page",
        "values": { "body": body_markup }
    })
    .to_string();

    let plugin_zip = build_zip(&[
        (
            "manifest.json",
            &serde_json::to_vec(&json!({
                "id": "s01-body-sync",
                "version": "1.0.0",
                "api_version": 1,
                "manifest_schema": 2,
                "min_host_version": "0.1.0",
                "name": "S01 Body Sync",
                "permissions": {
                    "content_write": ["page"]
                },
                "actions": [{
                    "id": "save_block_page",
                    "title": "Save Block Page",
                    "required_role": "admin",
                    "capabilities": ["content_write"]
                }]
            }))
            .unwrap(),
        ),
        ("plugin.wasm", &build_content_save_action_wasm(&cmd)),
    ]);

    let installed = http
        .post(format!("{}/api/admin/plugins", site.base))
        .header(header::CONTENT_TYPE, "application/zip")
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .body(plugin_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(installed.status(), StatusCode::CREATED);

    let action_res = http
        .post(format!(
            "{}/api/admin/plugins/s01-body-sync/actions/save_block_page",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(action_res.status(), StatusCode::OK);

    let db = SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(site.root.join("database/opendash.sqlite")),
    )
    .await
    .unwrap();
    let entry_id: i64 = sqlx::query_scalar(
        "SELECT id FROM content_entries WHERE content_type_key = 'page' AND slug = 'plugin-block-page'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let ref_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entry_builder_media_refs WHERE entry_id = ? AND media_id = ?",
    )
    .bind(entry_id)
    .bind(&media_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        ref_count, 1,
        "Wasmi content_save writing comment-block body must sync entry_builder_media_refs"
    );

    let unauth_draft = http
        .get(format!("{}/media/{media_id}/file", site.base))
        .send()
        .await
        .unwrap();
    assert_ne!(unauth_draft.status(), StatusCode::OK);

    let published = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/{entry_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({"status": "published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::OK);

    let unauth_published = http
        .get(format!("{}/media/{media_id}/file", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_published.status(), StatusCode::OK);
}
