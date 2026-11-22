//! EGM-wordpress-style-permalinks-S03 / #480: native HTTP + SQLite acceptance.
//! Each test starts an isolated binary/site on an ephemeral loopback port.
//! The existing S01/S02 storage tests cover lower-level claim and hook logic;
//! these tests exercise the actual Admin/public HTTP contract.
use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, Response, StatusCode, header, redirect::Policy};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use zip::{ZipWriter, write::SimpleFileOptions};

struct Site {
    root: PathBuf,
    child: Child,
    base: String,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Site {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Canonicalize macOS /var -> /private/var before SitePaths verification.
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "opendash-wp-permalink-{}-{nanos}",
                std::process::id()
            ));
        fs::create_dir_all(&root).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let http = Client::new();
        for _ in 0..160 {
            if http
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|res| res.status() == StatusCode::OK)
            {
                return Self { root, child, base };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("temporary permalink server was not ready");
    }

    async fn db(&self) -> SqlitePool {
        SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(self.root.join("database/opendash.sqlite")),
        )
        .await
        .unwrap()
    }
}

struct Admin {
    cookie: String,
    csrf: String,
}

async fn login(http: &Client, site: &Site) -> Admin {
    let setup = http
        .post(format!("{}/auth/setup", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({"username":"permalink_admin","password":"Permalink-Admin-Password-2026!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let result = http
        .post(format!("{}/auth/login", site.base))
        .json(&json!({"username":"permalink_admin","password":"Permalink-Admin-Password-2026!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(result.status(), StatusCode::OK);
    let cookie = result
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = result.json().await.unwrap();
    Admin {
        cookie,
        csrf: body["csrf_token"].as_str().unwrap().to_owned(),
    }
}

fn http() -> Client {
    Client::builder().redirect(Policy::none()).build().unwrap()
}

fn plugin_zip(manifest: &str, wasm: &[u8]) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut bytes);
        let options = SimpleFileOptions::default();
        writer.start_file("manifest.json", options).unwrap();
        writer.write_all(manifest.as_bytes()).unwrap();
        writer.start_file("plugin.wasm", options).unwrap();
        writer.write_all(wasm).unwrap();
        writer.finish().unwrap();
    }
    bytes.into_inner()
}

/// Call the real content_save ABI and fail the action when the host returns
/// {ok:false}. This catches any silent slug renaming inside the plugin path.
fn plugin_content_save_wasm(request: &str) -> Vec<u8> {
    let request_bytes: String = request
        .as_bytes()
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect();
    let source = format!(
        r#"(module
            (import "opendash_v1" "host_call" (func $host_call (param i32 i32 i32 i32) (result i32)))
            (import "opendash_v1" "host_write_output" (func $out (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 1024) "{request_bytes}")
            (func (export "opendash_run") (result i32)
                (local $n i32)
                (local.set $n (call $host_call (i32.const 1024) (i32.const {request_len}) (i32.const 4096) (i32.const 2048)))
                (if (i32.lt_s (local.get $n) (i32.const 7))
                    (then (return (i32.const 1))))
                (drop (call $out (i32.const 4096) (local.get $n)))
                ;; Response boolean false has its first byte at offset 6.
                (if (i32.eq (i32.load8_u (i32.const 4102)) (i32.const 102))
                    (then (return (i32.const 1))))
                i32.const 0)
        )"#,
        request_len = request.len(),
    );
    wat::parse_str(&source).unwrap()
}

async fn expect_json(response: Response, expected: StatusCode) -> Value {
    let status = response.status();
    let text = response.text().await.unwrap();
    assert_eq!(status, expected, "HTTP {status}: {text}");
    serde_json::from_str(&text).unwrap()
}

async fn content_type_token(http: &Client, site: &Site, admin: &Admin, kind: &str) -> String {
    let all: Vec<Value> = http
        .get(format!("{}/api/admin/content-types", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    all.iter()
        .find(|row| row["descriptor"]["key"] == kind)
        .unwrap_or_else(|| panic!("missing effective content type {kind}"))["schema_token"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn composite(
    http: &Client,
    site: &Site,
    admin: &Admin,
    kind: &str,
    mut data: Value,
    entry_id: Option<i64>,
) -> Response {
    data["expected_schema_token"] = json!(content_type_token(http, site, admin, kind).await);
    let root = format!("{}/api/admin/content-types/{kind}/entries", site.base);
    let request = if let Some(id) = entry_id {
        http.put(format!("{root}/{id}/composite"))
    } else {
        http.post(format!("{root}/composite"))
    };
    request
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .json(&data)
        .send()
        .await
        .unwrap()
}

async fn publish(
    http: &Client,
    site: &Site,
    admin: &Admin,
    kind: &str,
    slug: &str,
    opts: Value,
) -> Value {
    let mut data = json!({
        "title": format!("Post {slug}"),
        "slug":slug,
        "publication_status":"published",
        "values":{}
    });
    if let (Some(dest), Some(source)) = (data.as_object_mut(), opts.as_object()) {
        dest.extend(source.clone());
    }
    expect_json(
        composite(http, site, admin, kind, data, None).await,
        StatusCode::CREATED,
    )
    .await
}

async fn get_settings(http: &Client, site: &Site, admin: &Admin) -> Value {
    expect_json(
        http.get(format!("{}/api/admin/settings/permalinks", site.base))
            .header(header::COOKIE, &admin.cookie)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await
}

fn permalink_input(structure: &str) -> Value {
    json!({"post_structure": structure, "content_type_bases": {}, "taxonomy_bases": {}})
}

async fn put_settings(
    http: &Client,
    site: &Site,
    admin: &Admin,
    version: i64,
    data: Value,
) -> Response {
    http.put(format!("{}/api/admin/settings/permalinks", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .header(header::IF_MATCH, format!("\"v{version}\""))
        .json(&data)
        .send()
        .await
        .unwrap()
}

async fn category(http: &Client, site: &Site, admin: &Admin, slug: &str) -> Value {
    expect_json(
        http.post(format!("{}/api/admin/taxonomy/terms", site.base))
            .header(header::COOKIE, &admin.cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &admin.csrf)
            .json(&json!({"kind":"category","name":format!("Category {slug}"),"slug":slug}))
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await
}

async fn assert_public(http: &Client, site: &Site, path: &str, expected: StatusCode) {
    let response = http
        .get(format!("{}{path}", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        expected,
        "GET {path} returned {}",
        response.status()
    );
}

#[tokio::test]
async fn default_root_cpt_taxonomy_archive_private_and_trailing_slash_routes() {
    let site = Site::start().await;
    let http = http();
    let admin = login(&http, &site).await;
    let settings = get_settings(&http, &site, &admin).await;
    assert_eq!(settings["post_structure"], "/%postname%");
    publish(&http, &site, &admin, "post", "root-post", json!({})).await;
    publish(&http, &site, &admin, "page", "root-page", json!({})).await;
    let draft = expect_json(
        composite(
            &http, &site, &admin, "post",
            json!({"title":"Draft", "slug":"hidden-draft", "publication_status":"draft", "values":{}}),
            None,
        ).await,
        StatusCode::CREATED,
    ).await;
    assert_eq!(draft["entry"]["publication_status"], "draft");
    let cat = category(&http, &site, &admin, "press").await;
    assert!(cat["id"].as_i64().unwrap() > 0);

    let project = expect_json(
        http.post(format!("{}/api/admin/content-types", site.base))
            .header(header::COOKIE, &admin.cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &admin.csrf)
            .json(&json!({"key":"project","label":"Projects","fields":[],
                "settings":{"public":true,"has_archive":true,
                            "publication":"draft_publish","supports":["editor"]}}))
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(project["descriptor"]["key"], "project");
    publish(&http, &site, &admin, "project", "project-one", json!({})).await;

    let private = expect_json(
        http.post(format!("{}/api/admin/content-types", site.base))
            .header(header::COOKIE, &admin.cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &admin.csrf)
            .json(
                &json!({"key":"private_note","label":"Private note","fields":[],
                "settings":{"public":false,"has_archive":false,
                            "publication":"immediate","supports":["editor"]}}),
            )
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(private["descriptor"]["settings"]["public"], false);
    publish(
        &http,
        &site,
        &admin,
        "private_note",
        "secret-note",
        json!({}),
    )
    .await;

    let pattern = expect_json(
        http.post(format!("{}/api/admin/patterns", site.base))
            .header(header::COOKIE, &admin.cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &admin.csrf)
            .json(&json!({"title":"Private Banner","slug":"private-banner"}))
            .send()
            .await
            .unwrap(),
        StatusCode::CREATED,
    )
    .await;
    assert!(pattern["id"].as_i64().unwrap() > 0);

    for path in [
        "/root-post",
        "/root-page",
        "/project/project-one",
        "/project",
        "/category/press",
        "/posts",
    ] {
        assert_public(&http, &site, path, StatusCode::OK).await;
    }
    for path in [
        "/hidden-draft",
        "/hidden-draft/",
        "/private_note/secret-note",
        "/pattern/private-banner",
        "/section/private-banner",
        "/content/post/root-post",
        "/taxonomy/category/press",
    ] {
        assert_public(&http, &site, path, StatusCode::NOT_FOUND).await;
    }

    let trailing = http
        .get(format!("{}/root-post/?lang=vi", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(trailing.status(), StatusCode::MOVED_PERMANENTLY);
    // Single-entry queries have no allowed public filter keys; the canonical
    // serializer intentionally strips the unrecognized lang parameter.
    assert_eq!(trailing.headers()[header::LOCATION], "/root-post");
    let archive = http
        .get(format!("{}/posts/?page=2&lang=vi", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(archive.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(archive.headers()[header::LOCATION], "/posts?page=2");
    // API/docs namespaces bypass the dynamic permalink slash normalizer.
    let docs = http
        .get(format!("{}/api-docs/", site.base))
        .send()
        .await
        .unwrap();
    assert_ne!(docs.status(), StatusCode::MOVED_PERMANENTLY);
    for unsafe_path in ["//evil.com/", "/%5Cevil.com/"] {
        assert_public(&http, &site, unsafe_path, StatusCode::BAD_REQUEST).await;
    }
    let db = site.db().await;
    let leaks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permalink_route_claims WHERE
            (target_kind='entry' AND target_id IN
                (SELECT CAST(id AS TEXT) FROM content_entries
                 WHERE content_type_key IN ('pattern','section','private_note')))
            OR path LIKE '/private_note/%' OR path LIKE '/pattern/%' OR path LIKE '/section/%'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        leaks, 0,
        "private Pattern/Section/CPT must never claim a route"
    );
}

#[tokio::test]
async fn primary_category_drives_public_path_with_fallback_and_renames() {
    let site = Site::start().await;
    let http = http();
    let admin = login(&http, &site).await;
    let alpha = category(&http, &site, &admin, "alpha").await;
    let beta = category(&http, &site, &admin, "beta").await;
    let alpha_id = alpha["id"].as_i64().unwrap();
    let beta_id = beta["id"].as_i64().unwrap();

    let prior = get_settings(&http, &site, &admin).await;
    let changed = put_settings(
        &http,
        &site,
        &admin,
        prior["version"].as_i64().unwrap(),
        permalink_input("/%category%/%postname%"),
    )
    .await;
    assert_eq!(
        changed.status(),
        StatusCode::OK,
        "{:?}",
        changed.text().await.ok()
    );

    let story = publish(
        &http,
        &site,
        &admin,
        "post",
        "story",
        json!({"term_ids":[alpha_id, beta_id],"primary_category_term_id":beta_id}),
    )
    .await;
    let story_id = story["entry"]["id"].as_i64().unwrap();
    assert_eq!(story["entry"]["primary_category_term_id"], beta_id);
    assert_public(&http, &site, "/beta/story", StatusCode::OK).await;
    assert_public(&http, &site, "/alpha/story", StatusCode::NOT_FOUND).await;

    // When no explicit primary is chosen, MIN(term_id) is stable regardless
    // of submission order; absent terms fall back to "uncategorized".
    publish(
        &http,
        &site,
        &admin,
        "post",
        "fallback",
        json!({"term_ids":[beta_id,alpha_id]}),
    )
    .await;
    publish(&http, &site, &admin, "post", "no-category", json!({})).await;
    assert_public(&http, &site, "/alpha/fallback", StatusCode::OK).await;
    assert_public(&http, &site, "/uncategorized/no-category", StatusCode::OK).await;

    let updated = expect_json(
        composite(
            &http,
            &site,
            &admin,
            "post",
            json!({"title":"Post story","slug":"story","values":{},
               "publication_status":"published",
               "expected_version":story["entry"]["version"],
               "primary_category_term_id":alpha_id}),
            Some(story_id),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(updated["entry"]["primary_category_term_id"], alpha_id);
    assert_public(&http, &site, "/alpha/story", StatusCode::OK).await;
    let moved = http
        .get(format!("{}/beta/story", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(moved.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(moved.headers()[header::LOCATION], "/alpha/story");

    // Updating the slug of a linked category cascades to published posts.
    let rename = http
        .patch(format!("{}/api/admin/taxonomy/terms/{alpha_id}", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .header(
            header::IF_MATCH,
            format!("\"v{}\"", alpha["version"].as_i64().unwrap()),
        )
        .json(&json!({"slug":"alpha-renamed"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        rename.status(),
        StatusCode::OK,
        "{:?}",
        rename.text().await.ok()
    );
    assert_public(&http, &site, "/alpha-renamed/story", StatusCode::OK).await;
    assert_public(&http, &site, "/alpha-renamed/fallback", StatusCode::OK).await;

    // Core category can be made non-public; post routing uses the non-leaking
    // "uncategorized" fallback when category no longer qualifies.
    let definition: Value = http
        .get(format!(
            "{}/api/admin/taxonomy/definitions/category",
            site.base
        ))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let disable = http
        .patch(format!(
            "{}/api/admin/taxonomy/definitions/category",
            site.base
        ))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .header(
            header::IF_MATCH,
            format!("\"v{}\"", definition["version"].as_i64().unwrap()),
        )
        .json(&json!({"public":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        disable.status(),
        StatusCode::OK,
        "{:?}",
        disable.text().await.ok()
    );
    assert_public(&http, &site, "/uncategorized/story", StatusCode::OK).await;
    assert_public(&http, &site, "/uncategorized/fallback", StatusCode::OK).await;
}

#[tokio::test]
async fn auto_unique_slug_reserved_roots_noncomposite_conflict_and_post_id() {
    let site = Site::start().await;
    let http = http();
    let admin = login(&http, &site).await;
    let cpt = http
        .post(format!("{}/api/admin/content-types", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .json(&json!({"key":"project","label":"Projects","fields":[],
            "settings":{"public":true,"has_archive":true,
                        "publication":"draft_publish","supports":["editor"]}}))
        .send()
        .await
        .unwrap();
    assert_eq!(cpt.status(), StatusCode::CREATED);

    for (n, wanted) in ["repeat", "repeat-2", "repeat-3"].iter().enumerate() {
        let entry = publish(
            &http,
            &site,
            &admin,
            "post",
            "repeat",
            json!({"auto_unique_slug":true}),
        )
        .await;
        assert_eq!(entry["entry"]["slug"], *wanted, "attempt {n}");
        assert_public(&http, &site, &format!("/{wanted}"), StatusCode::OK).await;
    }
    for (name, wanted) in [
        ("media", "media-2"),
        ("posts", "posts-2"),
        ("project", "project-2"),
    ] {
        let entry = publish(
            &http,
            &site,
            &admin,
            "page",
            name,
            json!({"auto_unique_slug":true}),
        )
        .await;
        assert_eq!(entry["entry"]["slug"], wanted);
        assert_public(&http, &site, &format!("/{wanted}"), StatusCode::OK).await;
    }
    let long = format!("a{}", "b".repeat(63));
    let before = publish(
        &http,
        &site,
        &admin,
        "post",
        &long,
        json!({"auto_unique_slug":true}),
    )
    .await;
    let after = publish(
        &http,
        &site,
        &admin,
        "post",
        &long,
        json!({"auto_unique_slug":true}),
    )
    .await;
    assert_eq!(before["entry"]["slug"].as_str().unwrap().len(), 64);
    let actual = after["entry"]["slug"].as_str().unwrap();
    assert_eq!(actual.len(), 64);
    assert!(actual.ends_with("-2"));

    // The explicit legacy POST retains 409 instead of invoking auto-unique.
    let conflict = http
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            site.base
        ))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .json(&json!({"title":"No silent rename","slug":"repeat","values":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    let existing = publish(&http, &site, &admin, "post", "edit-me", json!({})).await;
    let update = composite(
        &http,
        &site,
        &admin,
        "post",
        json!({"title":"Update duplicate","slug":"repeat","values":{},
               "publication_status":"published","auto_unique_slug":true,
               "expected_version":existing["entry"]["version"]}),
        Some(existing["entry"]["id"].as_i64().unwrap()),
    )
    .await;
    assert_eq!(
        update.status(),
        StatusCode::CONFLICT,
        "updates must not silently rename even with auto_unique_slug"
    );

    // A plugin's content_save ABI is deliberately outside the Admin composite
    // opt-in. A duplicate slug must return a failed action and never create a
    // silently suffixed fourth post.
    let manifest = json!({
        "id":"wp-strict-permalink",
        "name":"Strict Permalink",
        "version":"1.0.0",
        "api_version":1,
        "manifest_schema":2,
        "permissions":{"content_write":["post"]},
        "actions":[{"id":"repeat-save","required_role":"admin",
                    "capabilities":["content_write"]}]
    })
    .to_string();
    let request = r#"{"op":"content_save","type_key":"post","slug":"repeat","title":"Plugin duplicate","values":{}}"#;
    let installed = http
        .post(format!("{}/api/admin/plugins", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(plugin_zip(&manifest, &plugin_content_save_wasm(request)))
        .send()
        .await
        .unwrap();
    assert_eq!(
        installed.status(),
        StatusCode::CREATED,
        "{:?}",
        installed.text().await.ok()
    );
    let action = http
        .post(format!(
            "{}/api/admin/plugins/wp-strict-permalink/actions/repeat-save",
            site.base
        ))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        action.status(),
        StatusCode::CONFLICT,
        "plugin must reject duplicate slug without opt-in"
    );
    let db = site.db().await;
    let next: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM content_entries WHERE content_type_key = 'post' AND slug = 'repeat-4'")
        .fetch_one(&db).await.unwrap();
    assert_eq!(next, 0, "plugin content_save must not silently rename");

    let settings = get_settings(&http, &site, &admin).await;
    for unsupported in [
        "/%year%/%postname%",
        "/%monthnum%/%postname%",
        "/%day%/%postname%",
    ] {
        let rejected = put_settings(
            &http,
            &site,
            &admin,
            settings["version"].as_i64().unwrap(),
            permalink_input(unsupported),
        )
        .await;
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST, "{unsupported}");
    }
    let selected = expect_json(
        put_settings(
            &http,
            &site,
            &admin,
            settings["version"].as_i64().unwrap(),
            permalink_input("/archives/%post_id%"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(selected["post_structure"], "/archives/%post_id%");
    let old = publish(&http, &site, &admin, "post", "post-id-proof", json!({})).await;
    let id = old["entry"]["id"].as_i64().unwrap();
    assert_public(&http, &site, &format!("/archives/{id}"), StatusCode::OK).await;
}

#[tokio::test]
async fn settings_conflicts_return_bounded_structured_details_and_rollback() {
    let site = Site::start().await;
    let http = http();
    let admin = login(&http, &site).await;
    // A published Page occupies exactly the proposed /blog/{slug} routes;
    // currently each Post owns /{slug}, so collision happens during PUT.
    for i in 0..22 {
        let slug = format!("clash-{i:02}");
        publish(&http, &site, &admin, "post", &slug, json!({})).await;
        let page = publish(
            &http,
            &site,
            &admin,
            "page",
            &format!("page-{slug}"),
            json!({}),
        )
        .await;
        let page_id = page["entry"]["id"].as_i64().unwrap();
        let route = http
            .put(format!(
                "{}/api/admin/content-types/page/entries/{page_id}/route",
                site.base
            ))
            .header(header::COOKIE, &admin.cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &admin.csrf)
            .header(
                header::IF_MATCH,
                format!("\"v{}\"", page["entry"]["version"].as_i64().unwrap()),
            )
            .json(&json!({"custom_path":format!("/blog/{slug}")}))
            .send()
            .await
            .unwrap();
        assert_eq!(route.status(), StatusCode::OK);
    }
    let before = get_settings(&http, &site, &admin).await;
    let conflict = put_settings(
        &http,
        &site,
        &admin,
        before["version"].as_i64().unwrap(),
        permalink_input("/blog/%postname%"),
    )
    .await;
    let value = expect_json(conflict, StatusCode::CONFLICT).await;
    let list = value["conflicts"].as_array().unwrap();
    assert_eq!(list.len(), 20);
    assert!(value["total_conflicts"].as_u64().unwrap() >= 22);
    assert_eq!(value["truncated"], true);
    for row in list {
        assert!(row["path"].as_str().unwrap().starts_with("/blog/clash-"));
        assert!(!row["new_target"].as_str().unwrap().is_empty());
        assert!(!row["existing_target"].as_str().unwrap().is_empty());
    }
    assert_eq!(
        get_settings(&http, &site, &admin).await,
        before,
        "409 must leave settings version/claims unchanged"
    );
    assert_public(&http, &site, "/clash-00", StatusCode::OK).await;
    assert_public(&http, &site, "/blog/clash-00", StatusCode::OK).await;
}
