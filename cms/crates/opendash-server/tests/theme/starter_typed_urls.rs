//! S01 native conformance: typed Starter URL, CAS and actual binary downgrade.
//! Each test owns a disposable site root and a random port; never use 18964.
use std::{
    fs,
    io::Read,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};

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

struct SiteRoot(PathBuf);

impl SiteRoot {
    fn new(tag: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "opendash-starter-url-{tag}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
}

impl Drop for SiteRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

async fn spawn(bin: &Path, root: &Path) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    assert_ne!(port, 18964);
    let child = Command::new(bin)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .env("OPENDASH_SITE_ROOT", root)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut server = Server {
        child,
        origin: format!("http://127.0.0.1:{port}"),
    };
    let http = Client::new();
    for _ in 0..150 {
        if let Ok(Some(code)) = server.child.try_wait() {
            let mut err = String::new();
            if let Some(stderr) = &mut server.child.stderr {
                let _ = stderr.read_to_string(&mut err);
            }
            panic!("Starter binary exited {code}: {err}");
        }
        if let Ok(res) = http
            .get(format!("{}/api/health", server.origin))
            .send()
            .await
            && res.status() == StatusCode::OK
        {
            return server;
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
    panic!("Starter binary never became healthy");
}

async fn setup(http: &Client, srv: &Server) {
    let response = http
        .post(format!("{}/auth/setup", srv.origin))
        .json(&json!({"username":"admin","password":"StarterTestPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn login(http: &Client, srv: &Server) -> (String, String) {
    let response = http
        .post(format!("{}/auth/login", srv.origin))
        .json(&json!({"username":"admin","password":"StarterTestPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf = response.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    (cookie, csrf)
}

async fn get_admin(http: &Client, srv: &Server, cookie: &str, path: &str) -> Value {
    let response = http
        .get(format!("{}{path}", srv.origin))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "GET {path}");
    response.json().await.unwrap()
}

async fn put_option(
    http: &Client,
    srv: &Server,
    cookie: &str,
    csrf: &str,
    version: i64,
    value: &str,
) -> reqwest::Response {
    http.put(format!(
        "{}/api/themes/opendash-starter/options",
        srv.origin
    ))
    .header(header::COOKIE, cookie)
    .header(header::ORIGIN, &srv.origin)
    .header("X-CSRF-Token", csrf)
    .header(header::IF_MATCH, format!("\"v{version}\""))
    .json(&json!({"values":{"header_cta_url":value}}))
    .send()
    .await
    .unwrap()
}

async fn activate(
    http: &Client,
    srv: &Server,
    cookie: &str,
    csrf: &str,
    revision_id: &str,
    version: i64,
) -> reqwest::Response {
    http.put(format!("{}/api/admin/themes/active", srv.origin))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", csrf)
        .header(header::IF_MATCH, format!("\"v{version}\""))
        .json(&json!({"revision_id":revision_id}))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn typed_url_fresh_public_saved_and_unsaved_preview() {
    let root = SiteRoot::new("fresh");
    let http = Client::new();
    let binary = Path::new(env!("CARGO_BIN_EXE_opendash"));
    let srv = spawn(binary, &root.0).await;
    setup(&http, &srv).await;
    let (cookie, csrf) = login(&http, &srv).await;
    let themes = get_admin(&http, &srv, &cookie, "/api/admin/themes").await;
    assert_eq!(themes["revisions"].as_array().unwrap().len(), 1);
    assert_eq!(themes["revisions"][0]["version"], "1.0.0");
    assert_eq!(themes["selection"]["version"], 1);

    let options = get_admin(&http, &srv, &cookie, "/api/themes/opendash-starter/options").await;
    let version = options["version"].as_i64().unwrap();
    let original = options["values"]["header_cta_url"].clone();
    assert_eq!(
        options["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|def| def["key"] == "header_cta_url")
            .unwrap()["type"],
        "url"
    );

    let invalid_save =
        put_option(&http, &srv, &cookie, &csrf, version, "javascript:alert(1)").await;
    assert_eq!(invalid_save.status(), StatusCode::BAD_REQUEST);
    let after_invalid =
        get_admin(&http, &srv, &cookie, "/api/themes/opendash-starter/options").await;
    assert_eq!(after_invalid["version"], version);
    assert_eq!(after_invalid["values"]["header_cta_url"], original);

    let preview = http
        .post(format!("{}/api/admin/studio/preview", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"target":{"kind":"home"},
            "transient":{"theme_options":{"header_cta_url":"#contact"}}}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        preview.status(),
        StatusCode::OK,
        "{}",
        preview.text().await.unwrap_or_default()
    );
    // The preview above consumes its response body in the diagnostic expression
    // only on failure; re-request to inspect the successful HTML.
    let preview_html = http
        .post(format!("{}/api/admin/studio/preview", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"target":{"kind":"home"},
            "transient":{"theme_options":{"header_cta_url":"#contact"}}}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert!(
        preview_html["html"]
            .as_str()
            .unwrap()
            .contains("href=\"#contact\"")
    );
    let after_preview =
        get_admin(&http, &srv, &cookie, "/api/themes/opendash-starter/options").await;
    assert_eq!(after_preview["version"], version);
    assert_eq!(after_preview["values"]["header_cta_url"], original);

    let invalid_preview = http
        .post(format!("{}/api/admin/studio/preview", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"target":{"kind":"home"},
            "transient":{"theme_options":{"header_cta_url":"//evil.test"}}}))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid_preview.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let new_url = "/lien-he?from=header&x=2";
    let saved = put_option(&http, &srv, &cookie, &csrf, version, new_url).await;
    assert_eq!(saved.status(), StatusCode::OK);
    let saved_json = saved.json::<Value>().await.unwrap();
    assert_eq!(saved_json["values"]["header_cta_url"], new_url);
    assert_eq!(saved_json["version"], version + 1);
    // New sites have no assigned home by design. Publish a real Page and set
    // Reading Settings before exercising the public Home template.
    let created = http
        .post(format!("{}/api/admin/content-types/page/entries", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"Homepage","slug":"starter-url-home","values":{"body":"Public homepage"}}))
        .send().await.unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_page = created.json::<Value>().await.unwrap();
    let page_id = created_page["id"].as_i64().unwrap();
    let page_version = created_page["version"].as_i64().unwrap();
    let published = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/{page_id}/publication",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{page_version}\""))
        .json(&json!({"status":"published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::OK);
    let selected = http
        .put(format!("{}/api/admin/settings/reading", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({"homepage_page_id":page_id,"posts_page_id":null}))
        .send()
        .await
        .unwrap();
    assert_eq!(selected.status(), StatusCode::OK);
    let public_home = http.get(format!("{}/", srv.origin)).send().await.unwrap();
    assert_eq!(public_home.status(), StatusCode::OK);
    let body = public_home.text().await.unwrap();
    assert_eq!(
        body.matches("href=\"&#x2f;lien-he?from=header&amp;x=2\"")
            .count(),
        2,
        "Header and Home must both render the exact escaped stored CTA URL: {body}"
    );

    // The post back-link must follow the actual Posts Page assignment, never
    // a hardcoded /posts fallback when the authoritative path is /news.
    let posts_page = http
        .post(format!(
            "{}/api/admin/content-types/page/entries",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"News","slug":"news","values":{"body":"Assigned archive"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(posts_page.status(), StatusCode::CREATED);
    let posts_page_json = posts_page.json::<Value>().await.unwrap();
    let posts_page_id = posts_page_json["id"].as_i64().unwrap();
    let posts_page_version = posts_page_json["version"].as_i64().unwrap();
    let posts_published = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/{posts_page_id}/publication",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{posts_page_version}\""))
        .json(&json!({"status":"published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(posts_published.status(), StatusCode::OK);
    let post = http
        .post(format!("{}/api/admin/content-types/post/entries", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"A public post","slug":"starter-url-post","values":{"body":"Public post body"}}))
        .send().await.unwrap();
    assert_eq!(post.status(), StatusCode::CREATED);
    let post_json = post.json::<Value>().await.unwrap();
    let post_id = post_json["id"].as_i64().unwrap();
    let post_version = post_json["version"].as_i64().unwrap();
    let post_published = http
        .post(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/publication",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{post_version}\""))
        .json(&json!({"status":"published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(post_published.status(), StatusCode::OK);
    let with_posts = http
        .put(format!("{}/api/admin/settings/reading", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({"homepage_page_id":page_id,"posts_page_id":posts_page_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(with_posts.status(), StatusCode::OK);
    let public_post = http
        .get(format!("{}/starter-url-post", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(public_post.status(), StatusCode::OK);
    let post_html = public_post.text().await.unwrap();
    assert!(post_html.contains("href=\"&#x2f;news\""), "{post_html}");
    // The Starter's global site navigation has a separate static /posts link;
    // its existence does not affect the single-entry archive backlink.

    drop(srv);
    let srv = spawn(binary, &root.0).await;
    let (cookie, _) = login(&http, &srv).await;
    let restarted = get_admin(&http, &srv, &cookie, "/api/admin/themes").await;
    assert_eq!(restarted["selection"]["version"], 1);
    assert_eq!(restarted["revisions"].as_array().unwrap().len(), 1);
    drop(srv);
}

#[tokio::test]
#[ignore = "requires external OPENDASH_STARTER_V100_BIN legacy binary"]
async fn actual_old_new_old_binary_roundtrip_preserves_data_and_cas() {
    let Some(old_bin) = std::env::var_os("OPENDASH_STARTER_V100_BIN") else {
        eprintln!("NOT_RUN: old/new/old binary receipt requires OPENDASH_STARTER_V100_BIN");
        return;
    };
    let old_bin = PathBuf::from(old_bin);
    assert!(old_bin.is_file(), "native old binary must exist");
    let current_bin = Path::new(env!("CARGO_BIN_EXE_opendash"));
    let root = SiteRoot::new("roundtrip");
    let http = Client::new();
    let old_srv = spawn(&old_bin, &root.0).await;
    setup(&http, &old_srv).await;
    let (cookie, csrf) = login(&http, &old_srv).await;
    let original_themes = get_admin(&http, &old_srv, &cookie, "/api/admin/themes").await;
    let v100_id = original_themes["selection"]["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let v100_digest = original_themes["revisions"][0]["revision_digest"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(original_themes["revisions"][0]["version"], "1.0.0");
    let option_before = get_admin(
        &http,
        &old_srv,
        &cookie,
        "/api/themes/opendash-starter/options",
    )
    .await;
    let saved = put_option(
        &http,
        &old_srv,
        &cookie,
        &csrf,
        option_before["version"].as_i64().unwrap(),
        " javascript:unsafe",
    )
    .await;
    assert_eq!(saved.status(), StatusCode::OK);
    let added_page = http.post(format!("{}/api/admin/content-types/page/entries", old_srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &old_srv.origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"Preserved page","slug":"preserved-page","values":{"body":"Keep this text"}}))
        .send().await.unwrap();
    assert_eq!(added_page.status(), StatusCode::CREATED);
    let page_id = added_page.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    let page_path = format!("/api/admin/content-types/page/entries/{page_id}");
    let before_page = get_admin(&http, &old_srv, &cookie, &page_path).await;
    let v100_marker = root
        .0
        .join("themes/revisions")
        .join(&v100_digest)
        .join(".complete");
    assert!(v100_marker.is_file());
    drop(old_srv);

    // Remove one known revision marker; new startup must repair v100 exactly,
    // while installing v101 without changing selection or the saved value.
    fs::remove_file(&v100_marker).unwrap();
    let new_srv = spawn(current_bin, &root.0).await;
    let (cookie, csrf) = login(&http, &new_srv).await;
    let upgrades = get_admin(&http, &new_srv, &cookie, "/api/admin/themes").await;
    assert_eq!(upgrades["selection"]["revision_id"], v100_id);
    assert_eq!(
        upgrades["selection"]["version"],
        original_themes["selection"]["version"]
    );
    assert!(v100_marker.is_file(), "frozen legacy revision must repair");
    assert_eq!(upgrades["revisions"].as_array().unwrap().len(), 2);
    let v111 = upgrades["revisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|rev| rev["version"] == "1.0.0" && rev["revision_id"] != v100_id)
        .unwrap();
    let v101_id = v111["revision_id"].as_str().unwrap().to_owned();
    let option_upgrade = get_admin(
        &http,
        &new_srv,
        &cookie,
        "/api/themes/opendash-starter/options",
    )
    .await;
    assert_eq!(
        option_upgrade["values"]["header_cta_url"],
        " javascript:unsafe"
    );
    let failed_activation = activate(
        &http,
        &new_srv,
        &cookie,
        &csrf,
        &v101_id,
        upgrades["selection"]["version"].as_i64().unwrap(),
    )
    .await;
    assert_eq!(failed_activation.status(), StatusCode::CONFLICT);
    let corrected = put_option(
        &http,
        &new_srv,
        &cookie,
        &csrf,
        option_upgrade["version"].as_i64().unwrap(),
        "/lien-he?corrected=1",
    )
    .await;
    assert_eq!(corrected.status(), StatusCode::OK);
    let first_activate = activate(
        &http,
        &new_srv,
        &cookie,
        &csrf,
        &v101_id,
        upgrades["selection"]["version"].as_i64().unwrap(),
    )
    .await;
    assert_eq!(first_activate.status(), StatusCode::OK);
    let selected_new = first_activate.json::<Value>().await.unwrap();
    assert_eq!(selected_new["version"], 2);
    let back = activate(&http, &new_srv, &cookie, &csrf, &v100_id, 2).await;
    assert_eq!(back.status(), StatusCode::OK);
    let selected_old = back.json::<Value>().await.unwrap();
    assert_eq!(selected_old["version"], 3);
    let before_old_restart_options = get_admin(
        &http,
        &new_srv,
        &cookie,
        "/api/themes/opendash-starter/options",
    )
    .await;
    assert_eq!(
        get_admin(&http, &new_srv, &cookie, &page_path).await,
        before_page
    );
    drop(new_srv);

    let downgraded = spawn(&old_bin, &root.0).await;
    let (cookie, _) = login(&http, &downgraded).await;
    let selection = get_admin(&http, &downgraded, &cookie, "/api/admin/themes").await;
    assert_eq!(selection["selection"]["revision_id"], v100_id);
    assert_eq!(selection["selection"]["version"], 3);
    assert_eq!(
        get_admin(
            &http,
            &downgraded,
            &cookie,
            "/api/themes/opendash-starter/options"
        )
        .await,
        before_old_restart_options
    );
    assert_eq!(
        get_admin(&http, &downgraded, &cookie, &page_path).await,
        before_page
    );
    assert_eq!(
        http.get(format!("{}/", downgraded.origin))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    drop(downgraded);
}
