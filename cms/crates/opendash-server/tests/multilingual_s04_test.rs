//! S04: Theme v3 L10n dictionaries, C-SEC hardening, CoreL10n fallbacks, and opendash-starter-1.1.0.
use std::{
    fs,
    io::{Read, Write},
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

struct SiteRoot(PathBuf);
impl SiteRoot {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "opendash-s04-{label}-{}-{nanos}",
            std::process::id()
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

async fn spawn(site_root: &Path) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
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

fn build_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (path, content) in files {
        zip.start_file(path, opts).unwrap();
        zip.write_all(content).unwrap();
    }
    zip.finish().unwrap();
    buf
}

async fn upload_theme(
    client: &Client,
    origin: &str,
    cookie: &str,
    csrf: &str,
    zip_bytes: Vec<u8>,
) -> reqwest::Response {
    client
        .post(format!("{origin}/api/admin/themes"))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, origin)
        .header("X-CSRF-Token", csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(zip_bytes)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn s04_theme_l10n_csec_hardening_per_key_merge_and_starter_v110() {
    let root = SiteRoot::new("l10n");
    let srv = spawn(&root.0).await;
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &srv.origin).await;

    // 1. Starter v1.1.0 is active on fresh install; single-locale hides switcher & legacy fallback menus.
    let themes: Value = client
        .get(format!("{}/api/admin/themes", srv.origin))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(themes["revisions"][0]["version"], "1.1.0");

    let home_single = client.get(format!("{}/", srv.origin)).send().await.unwrap();
    assert_eq!(home_single.status(), StatusCode::OK);
    let home_single_html = home_single.text().await.unwrap();
    assert!(!home_single_html.contains("<details"));
    assert!(!home_single_html.contains("/gioi-thieu\">Giới thiệu</a>"));

    // 2. C-SEC Hardening rejections (400 Bad Request).
    let base_manifest = r#"{"manifest_version":3,"theme_id":"bad-l10n","name":"Bad","version":"1.0.0","default_locale":"vi","templates":[{"kind":"single","file":"single.html"},{"kind":"archive","file":"archive.html"}]}"#;
    let vi_dict = r#"{"hello":"Xin chào"}"#.as_bytes();

    // 2a. Missing key in default_locale
    let r_missing_key = upload_theme(
        &client,
        &srv.origin,
        &cookie,
        &csrf,
        build_zip(&[
            ("theme.json", base_manifest.as_bytes()),
            ("locales/vi.json", vi_dict),
            (
                "single.html",
                b"<main>{{ t.missing_key }}{{ od_content() }}</main>",
            ),
            ("archive.html", b"<main>{{ t.hello }}</main>"),
        ]),
    )
    .await;
    assert_eq!(r_missing_key.status(), StatusCode::BAD_REQUEST);

    // 2b. Expr::GetItem on `t` (`t["hello"]`)
    let r_getitem = upload_theme(
        &client,
        &srv.origin,
        &cookie,
        &csrf,
        build_zip(&[
            ("theme.json", base_manifest.as_bytes()),
            ("locales/vi.json", vi_dict),
            (
                "single.html",
                b"<main>{{ t[\"hello\"] }}{{ od_content() }}</main>",
            ),
            ("archive.html", b"<main>{{ t.hello }}</main>"),
        ]),
    )
    .await;
    assert_eq!(r_getitem.status(), StatusCode::BAD_REQUEST);

    // 2c. Rebinding `locale` or `t` or `site`
    let r_rebind = upload_theme(
        &client,
        &srv.origin,
        &cookie,
        &csrf,
        build_zip(&[
            ("theme.json", base_manifest.as_bytes()),
            ("locales/vi.json", vi_dict),
            (
                "single.html",
                b"{% set locale = {'home_url':'/evil'} %}<main>{{ od_content() }}</main>",
            ),
            ("archive.html", b"<main>{{ t.hello }}</main>"),
        ]),
    )
    .await;
    assert_eq!(r_rebind.status(), StatusCode::BAD_REQUEST);

    // 2d. `locale.home_url` in <img src>
    let r_img_src = upload_theme(
        &client,
        &srv.origin,
        &cookie,
        &csrf,
        build_zip(&[
            ("theme.json", base_manifest.as_bytes()),
            ("locales/vi.json", vi_dict),
            (
                "single.html",
                b"<main><img src=\"{{ locale.home_url }}\">{{ od_content() }}</main>",
            ),
            ("archive.html", b"<main>{{ t.hello }}</main>"),
        ]),
    )
    .await;
    assert_eq!(r_img_src.status(), StatusCode::BAD_REQUEST);

    // 3. Enable secondary locale `en` and test Starter 1.1.0 + CoreL10n + language-switcher.
    let locales_get = client
        .get(format!("{}/api/admin/locales", srv.origin))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let etag = locales_get
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let enabled = client
        .put(format!("{}/api/admin/locales/multilingual", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &etag)
        .json(&json!({"enabled": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(enabled.status(), StatusCode::OK);
    let etag = enabled.headers()[header::ETAG].to_str().unwrap().to_owned();
    let add_en = client
        .post(format!("{}/api/admin/locales", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, etag)
        .json(&json!({ "code": "en", "label": "English", "direction": "ltr" }))
        .send()
        .await
        .unwrap();
    assert_eq!(add_en.status(), StatusCode::CREATED);

    // /en unconfigured home uses CoreL10n English
    let en_home = client
        .get(format!("{}/en", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(en_home.status(), StatusCode::OK);
    let en_home_html = en_home.text().await.unwrap();
    assert!(en_home_html.contains("<html lang=\"en\" dir=\"ltr\">"));
    assert!(en_home_html.contains("Homepage is not configured."));

    // /en/no-such-page 404 uses Starter 1.1.0 English dictionary & language-switcher
    let en_404 = client
        .get(format!("{}/en/no-such-page", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(en_404.status(), StatusCode::NOT_FOUND);
    let csp = en_404
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(csp.contains("script-src 'none'"));
    let en_404_html = en_404.text().await.unwrap();
    assert!(en_404_html.contains("This page does not exist or has been moved."));
    assert!(en_404_html.contains("<details"));
    assert!(en_404_html.contains("href=\"&#x2f;\""));
    assert!(en_404_html.contains("href=\"&#x2f;en\""));

    // 4. Install valid theme with partial `en.json` (missing a key present in `vi.json`) -> per-key merge 200 OK!
    let good_manifest = r#"{"manifest_version":3,"theme_id":"good-l10n","name":"Good L10n","version":"1.0.0","default_locale":"vi","templates":[{"kind":"single","file":"single.html"},{"kind":"archive","file":"archive.html"},{"kind":"not_found","file":"404.html"}]}"#;
    let r_good = upload_theme(
        &client,
        &srv.origin,
        &cookie,
        &csrf,
        build_zip(&[
            ("theme.json", good_manifest.as_bytes()),
            ("locales/vi.json", r#"{"greeting":"Xin chào","fallback_only":"Chỉ có ở gốc"}"#.as_bytes()),
            ("locales/en.json", br#"{"greeting":"Hello"}"#),
            ("single.html", b"<main><a href=\"{{ locale.home_url }}\">{{ t.greeting }}</a>{{ od_content() }}</main>"),
            ("archive.html", b"<main>{{ t.greeting }} - {{ t.fallback_only }}</main>"),
            ("404.html", b"<main>{{ t.greeting }} - {{ t.fallback_only }}</main>"),
        ]),
    )
    .await;
    assert_eq!(r_good.status(), StatusCode::CREATED);
    let good_rev = r_good.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let themes_now: Value = client
        .get(format!("{}/api/admin/themes", srv.origin))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let sel_ver = themes_now["selection"]["version"].as_i64().unwrap();
    let act = client
        .put(format!("{}/api/admin/themes/active", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{sel_ver}\""))
        .json(&json!({ "revision_id": good_rev }))
        .send()
        .await
        .unwrap();
    assert_eq!(act.status(), StatusCode::OK);

    let en_partial = client
        .get(format!("{}/en/no-such-page", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(en_partial.status(), StatusCode::NOT_FOUND);
    let partial_html = en_partial.text().await.unwrap();
    assert!(partial_html.contains("Hello - Chỉ có ở gốc"));
}
