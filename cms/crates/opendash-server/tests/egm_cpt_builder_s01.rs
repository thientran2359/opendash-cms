//! EGM Spec 2 S01: any opted-in CPT owns Builder AST independently of Editor
//! values, with one entry CAS and no implicit template/other-entry mutation.
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use opendash_core::content::{ContentMode, ContentSupport, ContentTypeSettings};
use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;

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
            "opendash-egm-cpt-s01-{}-{nanos}",
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
    for _ in 0..180 {
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
        "fields": [{"key":"body","label":"Project text","type":"rich_text"}],
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
fn page_default_enabled_post_default_disabled_and_section_unchanged() {
    let page = ContentTypeSettings::default_for_type_key("page");
    let post = ContentTypeSettings::default_for_type_key("post");
    let section = ContentTypeSettings::default_for_type_key("section");
    assert!(page.supports(ContentSupport::Builder));
    assert!(!page.has_archive);
    assert!(!post.supports(ContentSupport::Builder));
    assert!(post.has_archive);
    assert!(!section.supports(ContentSupport::Builder));
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
    assert!(before["document"].is_null());
    let doc = document("Project A Design");
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
    assert_eq!(payload["document"], doc);
    assert_eq!(payload["content_mode"], "builder");
    let stale = http
        .put(&url)
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&doc)
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
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
    assert!(beta_after["document"].is_null());

    let db = SqlitePool::connect(&format!(
        "sqlite://{}",
        site.root.join("database/opendash.sqlite").display()
    ))
    .await
    .unwrap();
    let (raw_values, mode, ast): (String, String, String) = sqlx::query_as(
        "SELECT e.values_json, e.content_mode, d.document_json
         FROM content_entries e JOIN entry_builder_documents d ON d.entry_id=e.id WHERE e.id=?",
    )
    .bind(a)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(mode, "builder");
    assert_eq!(
        serde_json::from_str::<Value>(&raw_values).unwrap()["body"],
        "Original Alpha Body"
    );
    assert_eq!(serde_json::from_str::<Value>(&ast).unwrap(), doc);

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
    let retained: (String, String) = sqlx::query_as(
        "SELECT e.values_json, d.document_json FROM content_entries e
         JOIN entry_builder_documents d ON d.entry_id = e.id WHERE e.id=?",
    )
    .bind(a)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&retained.0).unwrap()["body"],
        "Original Alpha Body"
    );
    assert_eq!(serde_json::from_str::<Value>(&retained.1).unwrap(), doc);

    // Even while Builder is disabled, Editor mode may be explicitly restored.
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
        .find(|row| row["base"]["descriptor"]["key"] == "project")
        .or_else(|| {
            types
                .iter()
                .find(|row| row["descriptor"]["key"] == "project")
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
        .json(
            &json!({"title":"alpha","slug":"alpha","values":{},"content_mode":"editor",
            "expected_version":2,"expected_schema_token":token}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    let after_mode: (String, String) = sqlx::query_as(
        "SELECT e.content_mode, d.document_json FROM content_entries e
         JOIN entry_builder_documents d ON d.entry_id=e.id WHERE e.id=?",
    )
    .bind(a)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(after_mode.0, "editor");
    assert_eq!(serde_json::from_str::<Value>(&after_mode.1).unwrap(), doc);

    // Re-enabling the capability cannot erase the stored AST or editor body.
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
}
