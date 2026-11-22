//! M7-S15-T03: native HTTP/SQLite proof for atomic composite publication.
//! All roots and HTTP traffic are disposable and loopback-only.
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, Response, StatusCode};
use serde_json::{Value, json};
use sqlx::SqlitePool;

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
        // macOS /var -> /private/var: canonicalize to satisfy runtime path guards.
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "opendash-m7-s15-t03-{}-{nanos}",
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
        let client = Client::new();
        for _ in 0..140 {
            if client
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|response| response.status() == StatusCode::OK)
            {
                return Self { root, child, base };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("native server did not become healthy");
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

async fn login(client: &Client, base: &str, username: &str, password: &str) -> (String, String) {
    let response = client
        .post(format!("{base}/auth/login"))
        .json(&json!({"username": username, "password": password}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .map(|cookie| cookie.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = response.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

async fn setup(client: &Client, site: &Site) -> (String, String) {
    let response = client
        .post(format!("{}/auth/setup", site.base))
        .json(&json!({"username":"admin", "password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    login(client, &site.base, "admin", "AdminPassword123!").await
}

async fn token(client: &Client, site: &Site, cookie: &str, kind: &str) -> String {
    let rows: Vec<Value> = client
        .get(format!("{}/api/admin/content-types", site.base))
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    rows.iter()
        .find(|item| item["descriptor"]["key"] == kind)
        .unwrap_or_else(|| panic!("missing built-in content type {kind}"))["schema_token"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn create(
    client: &Client,
    site: &Site,
    kind: &str,
    cookie: Option<&str>,
    csrf: Option<&str>,
    body: Value,
    origin: Option<&str>,
) -> Response {
    let mut request = client
        .post(format!(
            "{}/api/admin/content-types/{kind}/entries/composite",
            site.base
        ))
        .json(&body);
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    if let Some(csrf) = csrf {
        request = request.header("X-CSRF-Token", csrf);
    }
    if let Some(origin) = origin {
        request = request.header("Origin", origin);
    }
    request.send().await.unwrap()
}

async fn count_slug(db: &SqlitePool, slug: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM content_entries WHERE slug = ?")
        .bind(slug)
        .fetch_one(db)
        .await
        .unwrap()
}

#[tokio::test]
async fn composite_published_create_is_atomic_v1_and_draft_stays_private() {
    let site = Site::start().await;
    let client = Client::new();
    let (cookie, csrf) = setup(&client, &site).await;
    let db = site.db().await;
    // Sitemap requires explicit public origin/indexing; never infer Host headers.
    let settings = client
        .get(format!("{}/api/admin/settings/public", site.base))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    let etag = settings
        .headers()
        .get("ETag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let existing: Value = settings.json().await.unwrap();
    let configured = client
        .put(format!("{}/api/admin/settings/public", site.base))
        .header("Cookie", &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", etag)
        .json(&json!({
            "public_origin":"https://example.com",
            "content_language":existing["content_language"].as_str().unwrap_or("vi"),
            "indexing_enabled":true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(configured.status(), StatusCode::OK);

    for kind in ["post", "page"] {
        let schema = token(&client, &site, &cookie, kind).await;
        let slug = format!("published-{kind}");
        let custom_path = format!("/g5-{kind}");
        let response = create(
            &client,
            &site,
            kind,
            Some(&cookie),
            Some(&csrf),
            json!({
                "expected_schema_token":schema,
                "slug":slug,
                "title":format!("Published {kind}"),
                "values":{"body":"Visible body"},
                "publication_status":"published",
                "custom_path":custom_path,
                "seo":{"title":format!("SEO {kind}"),"noindex":false}
            }),
            Some(&site.base),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "{kind}: {:?}",
            response.text().await
        );
        let body: Value = response.json().await.unwrap();
        let id = body["entry"]["id"].as_i64().unwrap();
        assert_eq!(body["entry"]["publication_status"], "published");
        assert_eq!(body["entry"]["version"], 1, "one logical create = v1");
        assert_eq!(body["custom_path"], custom_path);
        assert_eq!(body["seo"]["title"], format!("SEO {kind}"));
        let state: (String, i64) =
            sqlx::query_as("SELECT publication_status, version FROM content_entries WHERE id = ?")
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(state, ("published".into(), 1));
        let public = client
            .get(format!("{}{custom_path}", site.base))
            .send()
            .await
            .unwrap();
        assert_eq!(public.status(), StatusCode::OK, "public {custom_path}");

        let draft_slug = format!("private-{kind}");
        let draft = create(
            &client,
            &site,
            kind,
            Some(&cookie),
            Some(&csrf),
            json!({
                "expected_schema_token": token(&client, &site, &cookie, kind).await,
                "title": format!("Private {kind}"), "slug": draft_slug,
                "values":{"body":"Unpublished body"}
            }),
            Some(&site.base),
        )
        .await;
        assert_eq!(draft.status(), StatusCode::CREATED);
        let draft_body: Value = draft.json().await.unwrap();
        assert_eq!(draft_body["entry"]["publication_status"], "draft");
        assert_eq!(draft_body["entry"]["version"], 1);
        assert_eq!(
            client
                .get(format!("{}/content/{kind}/{draft_slug}", site.base))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        let forbidden = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status) VALUES (?, ?, 'Bypass', '{}', 'published')",
        )
        .bind(kind)
        .bind(format!("bypass-{kind}"))
        .execute(&db)
        .await;
        assert!(
            forbidden.is_err(),
            "migration 0008 trigger must remain enforced"
        );
        assert_eq!(count_slug(&db, &format!("bypass-{kind}")).await, 0);
    }
    let sitemap = client
        .get(format!("{}/sitemaps/1.xml", site.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(sitemap.contains("/g5-post") && sitemap.contains("/g5-page"));
    assert!(!sitemap.contains("private-post") && !sitemap.contains("private-page"));
    db.close().await;
}

#[tokio::test]
async fn composite_publication_rejections_leave_no_partial_entry_or_sidecars() {
    let site = Site::start().await;
    let client = Client::new();
    let (cookie, csrf) = setup(&client, &site).await;
    let db = site.db().await;
    let schema = token(&client, &site, &cookie, "page").await;
    let base = json!({
        "expected_schema_token": schema, "slug":"first", "title":"First Page",
        "publication_status":"published", "values":{},
        "custom_path":"/same-path", "seo":{"title":"A page"}
    });
    let first = create(
        &client,
        &site,
        "page",
        Some(&cookie),
        Some(&csrf),
        base.clone(),
        Some(&site.base),
    )
    .await;
    assert_eq!(
        first.status(),
        StatusCode::CREATED,
        "{}",
        first.text().await.unwrap()
    );

    // Editor is an authorized content author; this must work for the same
    // published-on-create transaction rather than being Admin-only.
    let created_editor = client
        .post(format!("{}/api/admin/users", site.base))
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"username":"editor1","password":"EditorPassword123!","role":"editor"}))
        .send()
        .await
        .unwrap();
    assert_eq!(created_editor.status(), StatusCode::CREATED);
    let (editor_cookie, editor_csrf) =
        login(&client, &site.base, "editor1", "EditorPassword123!").await;
    let editor_create = create(
        &client,
        &site,
        "post",
        Some(&editor_cookie),
        Some(&editor_csrf),
        json!({
            "expected_schema_token":token(&client, &site, &editor_cookie, "post").await,
            "slug":"editor-published", "title":"Editor Post", "values":{},
            "publication_status":"published"
        }),
        Some(&site.base),
    )
    .await;
    assert_eq!(
        editor_create.status(),
        StatusCode::CREATED,
        "Editor create must be authorized"
    );
    let editor_entry: Value = editor_create.json().await.unwrap();
    assert_eq!(editor_entry["entry"]["version"], 1);
    assert_eq!(editor_entry["entry"]["publication_status"], "published");
    assert_eq!(
        client
            .get(format!("{}/content/post/editor-published", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let cases = [
        (
            "duplicate",
            json!({"slug":"first", "custom_path":"/duplicate-path"}),
        ),
        (
            "route-clash",
            json!({"slug":"route-clash","custom_path":"/same-path"}),
        ),
        (
            "bad-seo",
            json!({"slug":"bad-seo","custom_path":"/bad-seo","seo":{"og_image_id":"no-such-media"}}),
        ),
        (
            "bad-media",
            json!({"slug":"bad-media","custom_path":"/bad-media","values":{"featured_image":"no-such-media"}}),
        ),
        (
            "bad-schema",
            json!({"slug":"bad-schema","expected_schema_token":"sha256:stale"}),
        ),
        (
            "bad-field",
            json!({"slug":"bad-field","values":{"unknown_field":"x"}}),
        ),
    ];
    for (name, override_fields) in cases {
        let mut input = base.clone();
        for (key, value) in override_fields.as_object().unwrap() {
            input[key] = value.clone();
        }
        let response = create(
            &client,
            &site,
            "page",
            Some(&cookie),
            Some(&csrf),
            input,
            Some(&site.base),
        )
        .await;
        assert!(
            !response.status().is_success(),
            "{name} unexpectedly succeeded"
        );
        let slug = match name {
            "duplicate" => "first",
            other => other,
        };
        let expected = if name == "duplicate" { 1 } else { 0 };
        assert_eq!(
            count_slug(&db, slug).await,
            expected,
            "{name} left entry behind"
        );
    }
    let missing_auth = create(&client, &site, "page", None, None, json!({
        "expected_schema_token":schema,"slug":"unauthorized","title":"No session","publication_status":"published"
    }), Some(&site.base)).await;
    assert_eq!(missing_auth.status(), StatusCode::UNAUTHORIZED);
    let missing_csrf = create(&client, &site, "page", Some(&cookie), None, json!({
        "expected_schema_token":schema,"slug":"bad-csrf","title":"No csrf","publication_status":"published"
    }), Some(&site.base)).await;
    assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);
    let bad_origin = create(&client, &site, "page", Some(&cookie), Some(&csrf), json!({
        "expected_schema_token":schema,"slug":"bad-origin","title":"Wrong origin","publication_status":"published"
    }), Some("http://not-this-site.invalid")).await;
    assert_eq!(bad_origin.status(), StatusCode::FORBIDDEN);
    for slug in ["unauthorized", "bad-csrf", "bad-origin"] {
        assert_eq!(count_slug(&db, slug).await, 0);
    }
    // If route conflict was detected after INSERT/UPDATE, neither the entry nor
    // any sidecar is allowed to survive the transaction rollback.
    let bad_routes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entry_public_paths WHERE path IN ('/bad-seo','/bad-media','/same-path')",
    )
    .fetch_one(&db).await.unwrap();
    assert_eq!(bad_routes, 1, "only the successful page route may remain");
    let seo_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entry_seo")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(seo_rows, 1, "failed saves must not persist SEO rows");
    db.close().await;
}
