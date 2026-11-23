//! S02: built-in Starter v3 is active on fresh install and self-heals without
//! clobbering a site-owned theme selection or sample/imported content.
use std::{
    fs,
    io::Read,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

struct Server {
    child: Child,
    origin: String,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn spawn(site_root: &Path) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    assert_ne!(port, 18964, "the pinned live site is never a test target");
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .env("OPENDASH_SITE_ROOT", site_root)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut srv = Server {
        child,
        origin: format!("http://127.0.0.1:{port}"),
    };
    let client = Client::new();
    for _ in 0..125 {
        if let Ok(Some(exit)) = srv.child.try_wait() {
            let mut log = String::new();
            if let Some(stderr) = &mut srv.child.stderr {
                let _ = stderr.read_to_string(&mut log);
            }
            panic!("Starter server exited {exit}: {log}");
        }
        if let Ok(resp) = client
            .get(format!("{}/api/health", srv.origin))
            .send()
            .await
            && resp.status() == StatusCode::OK
        {
            return srv;
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
    panic!("the native Starter v3 server did not become healthy");
}

async fn login(client: &Client, origin: &str) -> (String, String) {
    let resp = client
        .post(format!("{origin}/auth/login"))
        .json(&json!({"username":"admin","password":"StarterTestPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf = resp.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    (cookie, csrf)
}
async fn get_json(client: &Client, origin: &str, cookie: &str, path: &str) -> Value {
    let resp = client
        .get(format!("{origin}{path}"))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "GET {path}");
    resp.json().await.unwrap()
}
fn custom_theme_zip() -> Vec<u8> {
    let manifest = br#"{"manifest_version":3,"theme_id":"test-custom-v3","name":"User Custom Theme","version":"1.0.0","templates":[{"kind":"single","file":"single.html"}]}"#;
    let mut bytes = Vec::new();
    {
        let mut writer = ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.start_file("theme.json", options).unwrap();
        writer.write_all(manifest).unwrap();
        writer.start_file("single.html", options).unwrap();
        writer
            .write_all(b"<main><article>{{ od_content() }}</article></main>")
            .unwrap();
        writer.finish().unwrap();
    }
    bytes
}

#[tokio::test]
async fn starter_bootstrap_import_repair_and_admin_theme_selection_are_durable() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let local_root = std::env::temp_dir().join(format!(
        "opendash-starter-v3-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&local_root).unwrap();
    let root: PathBuf = fs::canonicalize(&local_root).unwrap(); // macOS /var -> /private/var
    let client = Client::new();
    let srv = spawn(&root).await;
    let init = client
        .post(format!("{}/auth/setup", srv.origin))
        .json(&json!({"username":"admin","password":"StarterTestPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(init.status(), StatusCode::CREATED);
    let (cookie, csrf) = login(&client, &srv.origin).await;

    let themes = get_json(&client, &srv.origin, &cookie, "/api/admin/themes").await;
    assert_eq!(themes["selection"]["theme_id"], "opendash-starter");
    assert_eq!(themes["selection"]["version"], 1, "fresh ETag remains v1");
    assert_eq!(themes["revisions"].as_array().unwrap().len(), 1);
    assert_eq!(themes["revisions"][0]["version"], "1.0.1");
    // Strict-undefined C-SEC must not turn a brand-new site's missing menu
    // assignments or unpublished content into a public HTTP 500.
    let blank_home = client.get(format!("{}/", srv.origin)).send().await.unwrap();
    assert_eq!(blank_home.status(), StatusCode::OK);
    assert!(
        blank_home
            .text()
            .await
            .unwrap()
            .contains("/assets/opendash-tailwind.css")
    );
    let missing_page = client
        .get(format!("{}/starter-no-such-page", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_page.status(), StatusCode::NOT_FOUND);
    let starter_revision = themes["selection"]["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let starter_digest = themes["revisions"][0]["revision_digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let rev_root = root.join("themes/revisions").join(&starter_digest);
    assert_eq!(
        fs::read_to_string(rev_root.join(".complete"))
            .unwrap()
            .trim(),
        starter_digest
    );
    let manifest: Value =
        serde_json::from_slice(&fs::read(rev_root.join("theme.json")).unwrap()).unwrap();
    assert_eq!(manifest["version"], "1.0.1");
    assert_eq!(
        manifest["options"]
            .as_array()
            .unwrap()
            .iter()
            .find(|opt| opt["key"] == "header_cta_url")
            .unwrap()["type"],
        "url"
    );
    assert_eq!(manifest["templates"].as_array().unwrap().len(), 9);
    assert_eq!(manifest["patterns"].as_array().unwrap().len(), 6);
    assert_eq!(manifest["page_layouts"].as_array().unwrap().len(), 3);
    assert_eq!(manifest["sample"]["pages"].as_array().unwrap().len(), 5);
    assert_eq!(manifest["sample"]["menus"].as_array().unwrap().len(), 2);
    assert_eq!(manifest["sample"]["settings"].as_array().unwrap().len(), 2);
    assert!(fs::read(rev_root.join("blocks/stats-bar/block.json")).is_ok());
    assert!(fs::read(rev_root.join("blocks/quote-box/block.json")).is_ok());
    let stylesheet = client
        .get(format!("{}/assets/opendash-tailwind.css", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(stylesheet.status(), StatusCode::OK);
    assert_eq!(
        stylesheet.headers()[header::CONTENT_TYPE],
        "text/css; charset=utf-8"
    );
    assert!(stylesheet.text().await.unwrap().contains(".od-prose"));

    let layouts = get_json(
        &client,
        &srv.origin,
        &cookie,
        "/api/admin/themes/active/layouts",
    )
    .await;
    assert_eq!(layouts["page_layouts"].as_array().unwrap().len(), 3);
    let patterns = get_json(&client, &srv.origin, &cookie, "/api/admin/patterns/library").await;
    assert_eq!(patterns["items"].as_array().unwrap().len(), 6);
    let blocks = get_json(&client, &srv.origin, &cookie, "/api/admin/builder/blocks").await;
    let builtin = blocks["items"].as_array().unwrap();
    for id in [
        "core/banner",
        "core/card",
        "core/accordion",
        "core/divider",
        "core/spacer",
        "opendash-starter/stats-bar",
        "opendash-starter/quote-box",
    ] {
        assert!(
            builtin
                .iter()
                .any(|block| block["block_id"] == id && block["status"] == "active"),
            "missing active {id}"
        );
    }
    for key in [
        "hero_launchpad",
        "features_bento_grid",
        "about_story_split",
        "pricing_tiers",
        "faq_accordion",
        "cta_newsletter_banner",
    ] {
        let resp = client
            .get(format!(
                "{}/api/admin/patterns/library/theme/{key}",
                srv.origin
            ))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "pattern {key}");
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["document"]["schema_version"], 1);
    }

    let plan_resp = client
        .post(format!(
            "{}/api/themes/opendash-starter/import/dry-run",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &srv.origin)
        .json(&json!({"groups":["pages","menus","settings"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan: Value = plan_resp.json().await.unwrap();
    assert_eq!(plan["items"].as_array().unwrap().len(), 9);
    assert!(
        plan["unselected_dependencies"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(plan["revision_id"], starter_revision);
    let outcome = client
        .post(format!(
            "{}/api/themes/opendash-starter/import/execute",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &srv.origin)
        .json(
            &json!({"groups": plan["selected_groups"],"revision_id":plan["revision_id"],
            "plan_digest":plan["plan_digest"],"expected_versions":plan["expected_versions"],
            "acknowledge_public_changes":true,"run_id":"starter-seed-1"}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        outcome.status(),
        StatusCode::OK,
        "sample import: {}",
        outcome.text().await.unwrap_or_default()
    );
    let draft = client
        .get(format!("{}/gioi-thieu", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(
        draft.status(),
        StatusCode::NOT_FOUND,
        "imported sample remains draft"
    );
    let home = client.get(format!("{}/", srv.origin)).send().await.unwrap();
    assert_eq!(home.status(), StatusCode::OK);
    let html = home.text().await.unwrap();
    assert!(html.contains("/assets/opendash-tailwind.css"));

    drop(srv);
    // Interrupted or missing marker is repaired using embedded bytes on startup.
    fs::remove_file(rev_root.join(".complete")).unwrap();
    let srv = spawn(&root).await;
    let (cookie, csrf) = login(&client, &srv.origin).await;
    let restored = get_json(&client, &srv.origin, &cookie, "/api/admin/themes").await;
    assert_eq!(restored["selection"]["revision_id"], starter_revision);
    assert_eq!(restored["selection"]["version"], 1);
    assert_eq!(
        fs::read_to_string(rev_root.join(".complete"))
            .unwrap()
            .trim(),
        starter_digest
    );

    let installed = client
        .post(format!("{}/api/admin/themes", srv.origin))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &srv.origin)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(custom_theme_zip())
        .send()
        .await
        .unwrap();
    assert_eq!(installed.status(), StatusCode::CREATED);
    let custom_id = installed.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let active = client
        .put(format!("{}/api/admin/themes/active", srv.origin))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::ORIGIN, &srv.origin)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({"revision_id":custom_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(active.status(), StatusCode::OK);
    drop(srv);
    // Missing entire Starter revision must be reconstructed even though a
    // different revision remains active. No user choice or content is reset.
    fs::remove_dir_all(&rev_root).unwrap();
    let srv = spawn(&root).await;
    let (cookie, _) = login(&client, &srv.origin).await;
    let chosen = get_json(&client, &srv.origin, &cookie, "/api/admin/themes").await;
    assert_eq!(chosen["selection"]["revision_id"], custom_id);
    assert_eq!(chosen["selection"]["theme_id"], "test-custom-v3");
    assert_eq!(chosen["selection"]["version"], 2);
    assert!(rev_root.join(".complete").is_file());
    drop(srv);
    fs::remove_dir_all(&root).unwrap();
}
