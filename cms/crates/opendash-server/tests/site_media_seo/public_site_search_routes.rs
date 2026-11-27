use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header, redirect::Policy};
use serde_json::{Value, json};

static NEXT_SITE_SEQ: AtomicU64 = AtomicU64::new(1);

struct SearchSite {
    root: PathBuf,
    child: Option<Child>,
    base: String,
}

impl Drop for SearchSite {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl SearchSite {
    async fn start() -> Self {
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
                "opendash-public-search-{}-{nanos}-{}",
                std::process::id(),
                NEXT_SITE_SEQ.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&root).unwrap();
        let stderr = fs::File::create(root.join("startup.log")).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr))
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let http = Client::new();
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                panic!(
                    "search test server exited {status}: {}",
                    fs::read_to_string(root.join("startup.log")).unwrap_or_default()
                );
            }
            if http
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|res| res.status() == StatusCode::OK)
            {
                return Self {
                    root,
                    child: Some(child),
                    base,
                };
            }
            assert!(
                Instant::now() < deadline,
                "search test server timed out: {}",
                fs::read_to_string(root.join("startup.log")).unwrap_or_default()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

async fn setup_admin(http: &Client, base: &str) -> (String, String) {
    let setup = http
        .post(format!("{base}/auth/setup"))
        .header(header::ORIGIN, base)
        .json(&json!({"username":"search_admin","password":"Search-Admin-Password-2026!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = http
        .post(format!("{base}/auth/login"))
        .header(header::ORIGIN, base)
        .json(&json!({"username":"search_admin","password":"Search-Admin-Password-2026!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = login.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

async fn create_and_publish(
    http: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    type_key: &str,
    slug: &str,
    title: &str,
    locale: &str,
    values: Value,
    publish: bool,
) -> i64 {
    let create = http
        .post(format!("{base}/api/admin/content-types/{type_key}/entries"))
        .header(header::COOKIE, cookie)
        .header("x-csrf-token", csrf)
        .header(header::ORIGIN, base)
        .json(&json!({
            "slug": slug,
            "title": title,
            "locale": locale,
            "values": values
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created: Value = create.json().await.unwrap();
    let id = created["id"].as_i64().unwrap();
    let ver = created["version"].as_i64().unwrap();
    if publish {
        let pub_res = http
            .post(format!(
                "{base}/api/admin/content-types/{type_key}/entries/{id}/publication"
            ))
            .header(header::COOKIE, cookie)
            .header("x-csrf-token", csrf)
            .header(header::ORIGIN, base)
            .header(header::IF_MATCH, format!("\"v{ver}\""))
            .json(&json!({ "status": "published" }))
            .send()
            .await
            .unwrap();
        assert_eq!(pub_res.status(), StatusCode::OK);
    }
    id
}

#[tokio::test]
async fn public_ssr_and_json_search_enforces_locale_validation_csp_and_headers() {
    let srv = SearchSite::start().await;
    let http = Client::builder().redirect(Policy::none()).build().unwrap();
    let (cookie, csrf) = setup_admin(&http, &srv.base).await;

    let locales_get = http
        .get(format!("{}/api/admin/locales", srv.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(locales_get.status(), StatusCode::OK);
    let mut locales_etag = locales_get.headers()[header::ETAG]
        .to_str()
        .unwrap()
        .to_owned();
    let enable_ml = http
        .put(format!("{}/api/admin/locales/multilingual", srv.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &locales_etag)
        .json(&json!({"enabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(enable_ml.status(), StatusCode::OK);
    locales_etag = enable_ml.headers()[header::ETAG]
        .to_str()
        .unwrap()
        .to_owned();
    let add_en = http
        .post(format!("{}/api/admin/locales", srv.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.base)
        .header("x-csrf-token", &csrf)
        .header(header::IF_MATCH, &locales_etag)
        .json(&json!({"code":"en","label":"English","direction":"ltr"}))
        .send()
        .await
        .unwrap();
    assert_eq!(add_en.status(), StatusCode::CREATED);

    create_and_publish(
        &http,
        &srv.base,
        &cookie,
        &csrf,
        "post",
        "kien-truc-xanh",
        "Kiến trúc xanh Hà Nội",
        "vi",
        json!({"body":"Thiết kế kiến trúc xanh bền vững"}),
        true,
    )
    .await;
    create_and_publish(
        &http,
        &srv.base,
        &cookie,
        &csrf,
        "page",
        "about-studio",
        "Giới thiệu Studio kiến trúc",
        "vi",
        json!({}),
        true,
    )
    .await;
    create_and_publish(
        &http,
        &srv.base,
        &cookie,
        &csrf,
        "post",
        "secret-draft",
        "Bí mật kiến trúc chưa xuất bản",
        "vi",
        json!({}),
        false,
    )
    .await;
    create_and_publish(
        &http,
        &srv.base,
        &cookie,
        &csrf,
        "post",
        "green-architecture",
        "Green Architecture Studio",
        "en",
        json!({"body":"Sustainable studio designs"}),
        true,
    )
    .await;

    // 1. Default homepage keeps form-action 'none' while SSR /?s= allows form-action 'self'.
    let home = http.get(format!("{}/", srv.base)).send().await.unwrap();
    let home_csp = home.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        home_csp.contains("form-action 'none'"),
        "unexpected home CSP: {home_csp}"
    );

    let ssr = http
        .get(format!("{}/?s=kien+truc", srv.base))
        .send()
        .await
        .unwrap();
    assert_eq!(ssr.status(), StatusCode::OK);
    assert_eq!(ssr.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(ssr.headers()["x-robots-tag"], "noindex, follow");
    let ssr_csp = ssr.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        ssr_csp.contains("form-action 'self'"),
        "unexpected search CSP: {ssr_csp}"
    );
    let html = ssr.text().await.unwrap();
    assert!(html.contains("Kiến trúc xanh Hà Nội"));
    assert!(html.contains("<mark"));
    assert!(!html.contains("Bí mật kiến trúc chưa xuất bản"));

    // 2. Empty ?s=, type filter, and out-of-range valid page <= 100.
    assert_eq!(
        http.get(format!("{}/?s=", srv.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let page_100 = http
        .get(format!("{}/?s=kien&page=100", srv.base))
        .send()
        .await
        .unwrap();
    assert_eq!(page_100.status(), StatusCode::OK);
    let typed = http
        .get(format!("{}/?s=kien+truc&type=page", srv.base))
        .send()
        .await
        .unwrap();
    assert_eq!(typed.status(), StatusCode::OK);
    let typed_html = typed.text().await.unwrap();
    assert!(typed_html.contains("Giới thiệu Studio kiến trúc"));
    assert!(!typed_html.contains("Kiến trúc xanh Hà Nội"));

    // 3. Invalid queries & percent encodings return 400 (not 404).
    for bad in [
        "/?s=kien&category_id=1",
        "/?s=kien&page=101",
        "/?%ZZ&s=abc",
        "/en?%ZZ&s=abc",
        "/missing?%ZZ&s=abc",
    ] {
        let resp = http.get(format!("{}{bad}", srv.base)).send().await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "expected 400 for {bad}"
        );
    }

    // 4. Secondary locale search works with unconfigured homepage and preserves ?s= on 301.
    let en_search = http
        .get(format!("{}/en?s=green", srv.base))
        .send()
        .await
        .unwrap();
    assert_eq!(en_search.status(), StatusCode::OK);
    let en_html = en_search.text().await.unwrap();
    assert!(en_html.contains("Green Architecture Studio"));
    assert!(!en_html.contains("Kiến trúc xanh Hà Nội"));

    for (from, to) in [("/en/?s=", "/en?s="), ("/en/?s=green", "/en?s=green")] {
        let redir = http
            .get(format!("{}{from}", srv.base))
            .send()
            .await
            .unwrap();
        assert_eq!(
            redir.status(),
            StatusCode::MOVED_PERMANENTLY,
            "expected 301 for {from}"
        );
        assert_eq!(redir.headers()[header::LOCATION], to);
    }

    let fr = http
        .get(format!("{}/fr?s=hello", srv.base))
        .send()
        .await
        .unwrap();
    assert_eq!(fr.status(), StatusCode::NOT_FOUND);

    // 5. Public JSON API returns hits, facets, pagination, and typed error envelopes.
    let api_ok = http
        .get(format!(
            "{}/api/public/search?s=kien+truc&limit=10",
            srv.base
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(api_ok.status(), StatusCode::OK);
    assert_eq!(api_ok.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(api_ok.headers()["x-content-type-options"], "nosniff");
    assert_eq!(api_ok.headers()["x-robots-tag"], "noindex, nofollow");
    let api_json: Value = api_ok.json().await.unwrap();
    assert_eq!(api_json["has_query"], true);
    assert_eq!(api_json["items"].as_array().unwrap().len(), 2);
    assert!(!api_json["facets"].as_array().unwrap().is_empty());

    let api_bad = http
        .get(format!("{}/api/public/search?limit=21", srv.base))
        .send()
        .await
        .unwrap();
    assert_eq!(api_bad.status(), StatusCode::BAD_REQUEST);
    let api_bad_json: Value = api_bad.json().await.unwrap();
    assert_eq!(api_bad_json["code"], "invalid_query");

    let api_missing_loc = http
        .get(format!("{}/api/public/search?locale=fr&s=a", srv.base))
        .send()
        .await
        .unwrap();
    assert_eq!(api_missing_loc.status(), StatusCode::NOT_FOUND);
    let api_loc_json: Value = api_missing_loc.json().await.unwrap();
    assert_eq!(api_loc_json["code"], "locale_not_found");
}
