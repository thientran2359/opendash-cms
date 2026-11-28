use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};

static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

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

async fn start_site() -> Site {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let raw_root = std::env::temp_dir().join(format!(
        "opendash-search-studio-{}-{port}-{seq}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&raw_root).unwrap();
    let root = fs::canonicalize(&raw_root).unwrap();
    let log = fs::File::create(root.join("server.log")).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .env("OPENDASH_SITE_ROOT", &root)
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .spawn()
        .unwrap();
    let base = format!("http://127.0.0.1:{port}");
    let http = Client::new();
    for _ in 0..200 {
        if http
            .get(format!("{base}/api/health"))
            .send()
            .await
            .is_ok_and(|r| r.status() == StatusCode::OK)
        {
            return Site { child, root, base };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let log_text = fs::read_to_string(root.join("server.log")).unwrap_or_default();
    panic!("server did not start at {base}: {log_text}");
}

async fn setup_admin(http: &Client, base: &str) -> (String, String) {
    let setup = http
        .post(format!("{base}/auth/setup"))
        .header(header::ORIGIN, base)
        .json(&json!({ "username": "admin", "password": "SearchStudioPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = http
        .post(format!("{base}/auth/login"))
        .header(header::ORIGIN, base)
        .json(&json!({ "username": "admin", "password": "SearchStudioPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let csrf = login.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    (cookie, csrf)
}

#[tokio::test]
async fn studio_search_target_blocks_diagnostics_and_public_override() {
    let site = start_site().await;
    let http = Client::new();
    let (cookie, csrf) = setup_admin(&http, &site.base).await;

    // 1. Studio targets include "search".
    let targets: Value = http
        .get(format!("{}/api/admin/studio/targets", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        targets["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["canonical_target_key"] == "search")
    );

    // 2. Previewing core/search-results inside "home" emits diagnostic without leaking results.
    let home_doc = json!({
        "schema_version": 1,
        "blocks": [{
            "id": "sec1",
            "block_id": "core/section",
            "props": {},
            "children": [{
                "id": "row1",
                "block_id": "core/row",
                "props": {},
                "children": [{
                    "id": "col1",
                    "block_id": "core/column",
                    "props": {},
                    "children": [
                        { "id": "slot1", "block_id": "core/content-slot", "props": {} },
                        { "id": "sf1", "block_id": "core/search-form", "props": { "placeholder": "Find...", "submit_label": "Go", "show_type_filter": true, "preserve_type_filter": false } },
                        { "id": "sr1", "block_id": "core/search-results", "props": { "show_facets": true, "show_excerpt": true, "empty_message": "" } }
                    ]
                }]
            }]
        }]
    });
    let search_doc = json!({
        "schema_version": 1,
        "blocks": [{
            "id": "sec1",
            "block_id": "core/section",
            "props": {},
            "children": [{
                "id": "row1",
                "block_id": "core/row",
                "props": {},
                "children": [{
                    "id": "col1",
                    "block_id": "core/column",
                    "props": {},
                    "children": [
                        { "id": "sf1", "block_id": "core/search-form", "props": { "placeholder": "Find...", "submit_label": "Go", "show_type_filter": true, "preserve_type_filter": false } },
                        { "id": "sr1", "block_id": "core/search-results", "props": { "show_facets": true, "show_excerpt": true, "empty_message": "" } }
                    ]
                }]
            }]
        }]
    });
    let home_preview = http
        .post(format!("{}/api/admin/studio/preview", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "target": { "kind": "home" },
            "document": home_doc,
            "preview_annotations": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(home_preview.status(), StatusCode::OK);
    let home_json: Value = home_preview.json().await.unwrap();
    assert!(
        home_json["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["node_id"] == "sr1" && d["code"] == "search_results_outside_search_route")
    );
    assert!(
        home_json["html"]
            .as_str()
            .unwrap()
            .contains("od-search-preview")
    );

    // 3. Previewing the same blocks inside "search" target renders sample hits with 0 diagnostics.
    let search_preview = http
        .post(format!("{}/api/admin/studio/preview", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "target": { "kind": "search" },
            "document": search_doc,
            "preview_annotations": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(search_preview.status(), StatusCode::OK);
    let search_json: Value = search_preview.json().await.unwrap();
    assert_eq!(search_json["diagnostics"].as_array().unwrap().len(), 0);
    let sp_html = search_json["html"].as_str().unwrap();
    assert!(sp_html.contains("od-search-results-block"));
    assert!(sp_html.contains("od-search-preview"));

    // 4. Saving Studio override for "search" takes over GET /?s=... with live FTS5 results & CSP.
    let save_ov = http
        .put(format!("{}/api/admin/template-overrides", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "target": { "kind": "search" },
            "document": search_doc
        }))
        .send()
        .await
        .unwrap();
    assert!(matches!(
        save_ov.status(),
        StatusCode::CREATED | StatusCode::OK
    ));

    let create_post = http
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "title": "Builder Search Override Target",
            "slug": "builder-search-override-target",
            "values": { "body": "Studio Builder override search match" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_post.status(), StatusCode::CREATED);
    let created_json: Value = create_post.json().await.unwrap();
    let created_id = created_json["id"].as_i64().unwrap();
    let created_ver = created_json["version"].as_i64().unwrap();
    let pub_post = http
        .post(format!(
            "{}/api/admin/content-types/post/entries/{created_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{created_ver}\""))
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_post.status(), StatusCode::OK);

    let public_res = http
        .get(format!("{}/?s=Override", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(public_res.status(), StatusCode::OK);
    let csp = public_res.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap_or("");
    assert!(csp.contains("form-action 'self'"));
    let body = public_res.text().await.unwrap();
    assert!(body.contains(r#"data-builder-version="1""#));
    assert!(body.contains("od-search-results-block"));
    assert!(body.contains("Builder Search Override Target"));
}
