//! S05: End-to-End Multilingual CUJ suite (CUJ-1 through CUJ-6).
use std::{
    fs,
    io::Read,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct SiteRoot(PathBuf);
impl SiteRoot {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "opendash-s05-{label}-{}-{nanos}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for SiteRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

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
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .env("OPENDASH_SITE_ROOT", site_root)
        .env("OPENDASH_PUBLIC_ORIGIN", format!("http://127.0.0.1:{port}"))
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
            panic!("Server exited {exit}: {log}");
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
    panic!("Server did not become healthy");
}

async fn setup_and_login(client: &Client, origin: &str) -> (String, String) {
    let init = client
        .post(format!("{origin}/auth/setup"))
        .header(header::ORIGIN, origin)
        .json(&json!({
            "username": "admin",
            "password": "StarterTestPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(init.status(), StatusCode::CREATED);

    let resp = client
        .post(format!("{origin}/auth/login"))
        .header(header::ORIGIN, origin)
        .json(&json!({
            "username": "admin",
            "password": "StarterTestPassword123!"
        }))
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
    let body: Value = resp.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

fn etag_of(resp: &reqwest::Response) -> String {
    resp.headers()[header::ETAG].to_str().unwrap().to_owned()
}

#[tokio::test]
async fn multilingual_end_to_end_cuj_suite() {
    let root = SiteRoot::new("e2e");
    let srv = spawn(&root.0).await;
    let mut default_headers = header::HeaderMap::new();
    default_headers.insert(
        header::ORIGIN,
        header::HeaderValue::from_str(&srv.origin).unwrap(),
    );
    let client = Client::builder()
        .default_headers(default_headers)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let (cookie, csrf) = setup_and_login(&client, &srv.origin).await;

    // -----------------------------------------------------------------------
    // CUJ-1: Single-locale parity
    // -----------------------------------------------------------------------
    let locales_res = client
        .get(format!("{}/api/admin/locales", srv.origin))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(locales_res.status(), StatusCode::OK);
    let locales_etag = etag_of(&locales_res);
    let locales_json: Value = locales_res.json().await.unwrap();
    let items = locales_json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let default_code = items[0]["code"].as_str().unwrap().to_owned();
    let secondary_code = if default_code == "vi" { "en" } else { "vi" };

    let home_html = client
        .get(format!("{}/", srv.origin))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(home_html.contains(&format!("lang=\"{default_code}\"")));
    let missing_secondary = client
        .get(format!("{}/{secondary_code}", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(missing_secondary.status(), StatusCode::NOT_FOUND);

    // -----------------------------------------------------------------------
    // CUJ-2: Add secondary locale cleanly
    // -----------------------------------------------------------------------
    let enabled = client
        .put(format!("{}/api/admin/locales/multilingual", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &locales_etag)
        .json(&json!({"enabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(enabled.status(), StatusCode::OK);
    let locales_etag = etag_of(&enabled);
    let add_locale = client
        .post(format!("{}/api/admin/locales", srv.origin))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &locales_etag)
        .json(&json!({
            "code": secondary_code,
            "label": if secondary_code == "en" { "English" } else { "Tiếng Việt" },
            "direction": "ltr"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(add_locale.status(), StatusCode::CREATED);

    // Configure public_origin + indexing_enabled so hreflang & sitemap alternates emit
    let pub_get = client
        .get(format!("{}/api/admin/settings/public", srv.origin))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(pub_get.status(), StatusCode::OK);
    let pub_ver = pub_get.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();
    let pub_put = client
        .put(format!("{}/api/admin/settings/public", srv.origin))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{pub_ver}\""))
        .json(&json!({
            "public_origin": srv.origin,
            "content_language": default_code,
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_put.status(), StatusCode::OK);

    let sec_home = client
        .get(format!("{}/{secondary_code}", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(sec_home.status(), StatusCode::OK);
    let sec_home_html = sec_home.text().await.unwrap();
    assert!(sec_home_html.contains(&format!("lang=\"{secondary_code}\"")));

    // -----------------------------------------------------------------------
    // CUJ-3: Copy-on-create translation, taxonomy & hreflang/sitemap
    // -----------------------------------------------------------------------
    let cat_def = client
        .post(format!("{}/api/admin/taxonomy/terms", srv.origin))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "kind": "category",
            "name": "Tin tuc",
            "slug": "tin-tuc",
            "locale": default_code
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(cat_def.status(), StatusCode::CREATED);
    let cat_def_id = cat_def.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    let post_def = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "title": "Bai viet goc",
            "slug": "bai-viet-goc",
            "locale": default_code,
            "term_ids": [cat_def_id],
            "values": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(post_def.status(), StatusCode::CREATED);
    let post_def_id = post_def.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/post/entries/{post_def_id}/publication",
                srv.origin
            ))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({ "status": "published" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let trans_res = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{post_def_id}/translations",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({
            "target_locale": secondary_code,
            "title": "Translated Post",
            "slug": "translated-post"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(trans_res.status(), StatusCode::CREATED);
    let trans_json: Value = trans_res.json().await.unwrap();
    let trans_id = trans_json["id"].as_i64().unwrap();
    assert_eq!(trans_json["publication_status"], "draft");

    // Draft translation must not be publicly accessible
    assert_eq!(
        client
            .get(format!("{}/{secondary_code}/translated-post", srv.origin))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    // Publish translation
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/post/entries/{trans_id}/publication",
                srv.origin
            ))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({ "status": "published" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let pub_trans_html = client
        .get(format!("{}/{secondary_code}/translated-post", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_trans_html.status(), StatusCode::OK);
    let pub_trans_text = pub_trans_html.text().await.unwrap();
    assert!(pub_trans_text.contains("hreflang="));

    let sitemap_xml = client
        .get(format!("{}/sitemaps/1.xml", srv.origin))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(sitemap_xml.contains("bai-viet-goc"));
    assert!(sitemap_xml.contains("translated-post"));

    // -----------------------------------------------------------------------
    // CUJ-5 & CUJ-6: Studio Preview Locale & Per-locale General Settings
    // -----------------------------------------------------------------------
    let gen_sec = client
        .get(format!(
            "{}/api/admin/settings/general?locale={secondary_code}",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(gen_sec.status(), StatusCode::OK);
    let gen_sec_etag = etag_of(&gen_sec);
    let gen_sec_body: Value = gen_sec.json().await.unwrap();
    let put_gen_sec = client
        .put(format!(
            "{}/api/admin/settings/general?locale={secondary_code}",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &gen_sec_etag)
        .json(&json!({
            "site_title": "Secondary Site Title",
            "tagline": "Secondary Tagline",
            "timezone": gen_sec_body["timezone"].as_str().unwrap_or("UTC")
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_gen_sec.status(), StatusCode::OK);

    let preview_sec = client
        .post(format!("{}/api/admin/studio/preview", srv.origin))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "target": { "kind": "index" },
            "preview_locale": secondary_code
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(preview_sec.status(), StatusCode::OK);
    let preview_html = preview_sec.json::<Value>().await.unwrap()["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(preview_html.contains(&format!("lang=\"{secondary_code}\"")));
    assert!(preview_html.contains("Secondary Site Title"));

    // -----------------------------------------------------------------------
    // CUJ-4: Default-locale swap & disable locale
    // -----------------------------------------------------------------------
    let locales_now = client
        .get(format!("{}/api/admin/locales", srv.origin))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let locales_etag_2 = etag_of(&locales_now);

    let swap_resp = client
        .post(format!(
            "{}/api/admin/locales/{secondary_code}/set-default",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &locales_etag_2)
        .send()
        .await
        .unwrap();
    assert_eq!(swap_resp.status(), StatusCode::OK);

    let swapped_root = client
        .get(format!("{}/", srv.origin))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(swapped_root.contains(&format!("lang=\"{secondary_code}\"")));
    assert!(swapped_root.contains("Secondary Site Title"));

    // Disable old default locale
    let locales_now_3 = client
        .get(format!("{}/api/admin/locales", srv.origin))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let locales_etag_3 = etag_of(&locales_now_3);
    let disable_old = client
        .put(format!("{}/api/admin/locales/{default_code}", srv.origin))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &locales_etag_3)
        .json(&json!({
            "label": "Default",
            "direction": "ltr",
            "enabled": false,
            "position": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(disable_old.status(), StatusCode::OK);

    assert_eq!(
        client
            .get(format!("{}/{default_code}", srv.origin))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn cuj2_root_collision_returns_409() {
    let root = SiteRoot::new("cuj2-collision");
    let srv = spawn(&root.0).await;
    let mut default_headers = header::HeaderMap::new();
    default_headers.insert(
        header::ORIGIN,
        header::HeaderValue::from_str(&srv.origin).unwrap(),
    );
    let client = Client::builder()
        .default_headers(default_headers)
        .build()
        .unwrap();
    let (cookie, csrf) = setup_and_login(&client, &srv.origin).await;

    let locales_res = client
        .get(format!("{}/api/admin/locales", srv.origin))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let locales_etag = etag_of(&locales_res);
    let locales_json: Value = locales_res.json().await.unwrap();
    let default_code = locales_json["items"][0]["code"].as_str().unwrap();
    let secondary_code = if default_code == "vi" { "en" } else { "vi" };

    let colliding_page = client
        .post(format!(
            "{}/api/admin/content-types/page/entries",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "title": "Collision Page",
            "slug": secondary_code,
            "values": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(colliding_page.status(), StatusCode::CREATED);
    let colliding_id = colliding_page.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/page/entries/{colliding_id}/publication",
                srv.origin
            ))
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({ "status": "published" }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let enabled = client
        .put(format!("{}/api/admin/locales/multilingual", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &locales_etag)
        .json(&json!({"enabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(enabled.status(), StatusCode::OK);
    let locales_etag = etag_of(&enabled);
    let conflict_resp = client
        .post(format!("{}/api/admin/locales", srv.origin))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &locales_etag)
        .json(&json!({
            "code": secondary_code,
            "label": "Secondary",
            "direction": "ltr"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(conflict_resp.status(), StatusCode::CONFLICT);
    let conflict_body: Value = conflict_resp.json().await.unwrap();
    assert!(
        !conflict_body["conflicts"].as_array().unwrap().is_empty(),
        "409 conflict must list colliding permalink"
    );
}
