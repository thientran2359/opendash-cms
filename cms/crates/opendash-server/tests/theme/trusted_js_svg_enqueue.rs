//! S02 regression: trusted Theme v3 markup and ordered, conditional asset
//! delivery. All HTTP requests use a fresh site root and a random port.

use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const MANIFEST: &str = r#"{
    "manifest_version":3, "theme_id":"trusted-html-e2e", "name":"Trusted HTML",
    "version":"1.0.0",
    "parts":{"header":"header.html","footer":"footer.html"},
    "templates":[{"kind":"page","file":"page.html"},{"kind":"index","file":"index.html"}],
    "assets":[
        "assets/base.css","assets/home.css","assets/vendor.js",
        "assets/app.js","assets/init.mjs","assets/logo.svg"
    ],
    "styles":[
        {"handle":"base","src":"assets/base.css","deps":["opendash-tailwind"]},
        {"handle":"home-hero","src":"assets/home.css","deps":["base"],"conditions":["home"]}
    ],
    "scripts":[
        {"handle":"vendor","src":"assets/vendor.js","location":"footer","strategy":"defer"},
        {"handle":"app","src":"assets/app.js","deps":["vendor"],"location":"footer","strategy":"defer"},
        {"handle":"init","src":"assets/init.mjs","location":"head","module":true}
    ]
}"#;

const HEADER: &str = r##"<header class="trusted-header"><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 120 32"><title>Brand</title><defs><path id="logo" d="M0 0 L10 10"/></defs><use xlink:href="#logo"/></svg><button onclick="document.body.classList.toggle('nav-open')">Menu</button>{{ od_icon('calendar') }}</header>"##;
const FOOTER: &str = r#"<footer>Trusted Footer</footer><script>if (window.innerWidth < 768) { document.documentElement.dataset.mobile = "1"; }</script>"#;
const BLOCK: &str = r#"<aside class="trusted-block"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><title>Badge</title><circle cx="5" cy="5" r="4"/></svg><script>if (2 < 3) { window.badgeReady = true; }</script>{{ od_icon('check-circle') }}<span>{{ label }}</span></aside>"#;
const BLOCK_SCHEMA: &str = r#"{
    "schema_version":1, "id":"trusted-html-e2e/badge",
    "title":"Badge", "category":"content", "render":"render.html",
    "fields":[{"key":"label","label":"Label","type":"text","default":"Verified"}]
}"#;

fn fixtures() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("theme.json", MANIFEST.as_bytes()),
        ("header.html", HEADER.as_bytes()),
        ("footer.html", FOOTER.as_bytes()),
        ("page.html", b"<main>{{ od_content() }}</main>"),
        ("index.html", b"<main>{{ od_content() }}</main>"),
        ("blocks/badge/block.json", BLOCK_SCHEMA.as_bytes()),
        ("blocks/badge/render.html", BLOCK.as_bytes()),
        ("assets/base.css", b"body{margin:0}"),
        ("assets/home.css", b".trusted-header{display:block}"),
        ("assets/vendor.js", b"window.vendorReady = true;"),
        ("assets/app.js", b"window.appReady = window.vendorReady;"),
        ("assets/init.mjs", b"export const initialized = true;"),
        (
            "assets/logo.svg",
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"4\"/></svg>",
        ),
    ]
}

fn test_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "opendash-trusted-theme-{}-{nanos}",
            std::process::id()
        ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_fixture_theme(root: &Path) {
    for (path, bytes) in fixtures() {
        let target = root.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, bytes).unwrap();
    }
}

fn zip_fixture_theme() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (path, bytes) in fixtures() {
            zip.start_file(path, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

fn check_theme(root: &Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .arg("--check-theme")
        .arg(root)
        .output()
        .unwrap();
    (
        out.status.success(),
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn cli_accepts_trusted_svg_scripts_and_rejects_interpolations_or_cycles() {
    let root = test_dir();
    write_fixture_theme(&root);
    let (valid, output) = check_theme(&root);
    assert!(valid, "valid trusted theme: {output}");

    for invalid in [
        r#"<script>const value = "{{ title }}";</script>"#,
        r#"<button onclick="run('{{ title }}')">Bad</button>"#,
        "<script/>",
        "<iframe src='/'></iframe>",
    ] {
        fs::write(root.join("header.html"), invalid).unwrap();
        let (accepted, output) = check_theme(&root);
        assert!(!accepted, "accepted unsafe template {invalid}: {output}");
    }
    fs::write(root.join("header.html"), HEADER).unwrap();

    let cycle = MANIFEST.replace(
        r#""handle":"base","src":"assets/base.css","deps":["opendash-tailwind"]"#,
        r#""handle":"base","src":"assets/base.css","deps":["home-hero"]"#,
    );
    fs::write(root.join("theme.json"), cycle).unwrap();
    let (accepted, output) = check_theme(&root);
    assert!(!accepted, "accepted enqueue dependency cycle: {output}");

    fs::remove_dir_all(root).unwrap();
}

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

async fn spawn_site() -> Site {
    let root = test_dir();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    assert_ne!(port, 18964);
    let stderr = fs::File::create(root.join("server.log")).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_SITE_ROOT", &root)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
        .unwrap();
    let site = Site {
        root,
        child,
        base: format!("http://127.0.0.1:{port}"),
    };
    let http = Client::new();
    for _ in 0..400 {
        if http
            .get(format!("{}/api/health", site.base))
            .send()
            .await
            .is_ok_and(|r| r.status() == StatusCode::OK)
        {
            return site;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let log = fs::read_to_string(site.root.join("server.log")).unwrap_or_default();
    panic!("server failed to start: {log}");
}

struct Admin {
    cookie: String,
    csrf: String,
}

async fn admin_login(http: &Client, site: &Site) -> Admin {
    let setup = http
        .post(format!("{}/auth/setup", site.base))
        .json(&json!({"username":"admin","password":"TrustedThemePassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = http
        .post(format!("{}/auth/login", site.base))
        .json(&json!({"username":"admin","password":"TrustedThemePassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf = login.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    Admin { cookie, csrf }
}

async fn create_published_page(
    http: &Client,
    site: &Site,
    admin: &Admin,
    slug: &str,
    body: &str,
) -> i64 {
    let cfg: Value = http
        .get(format!(
            "{}/api/admin/content-types/page/effective-editor-config",
            site.base
        ))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let resp = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/composite",
            site.base
        ))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .json(&json!({
            "expected_schema_token":cfg["schema_token"],
            "title":slug,"slug":slug,"values":{"body":body}
        }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let text = resp.text().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{text}");
    let entry: Value = serde_json::from_str(&text).unwrap();
    let entry = entry.get("entry").unwrap_or(&entry);
    let id = entry["id"].as_i64().unwrap();
    let version = entry["version"].as_i64().unwrap();
    let pub_resp = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/{id}/publication",
            site.base
        ))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .header(header::IF_MATCH, format!("\"v{version}\""))
        .json(&json!({"status":"published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_resp.status(), StatusCode::OK);
    id
}

fn assert_order(html: &str, fragments: &[&str]) {
    let mut previous_end = 0;
    for fragment in fragments {
        let position = html[previous_end..]
            .find(fragment)
            .unwrap_or_else(|| panic!("missing '{fragment}' in HTML: {html}"));
        previous_end += position + fragment.len();
    }
}

#[tokio::test]
async fn http_renders_trusted_theme_with_conditional_enqueues_and_untrusted_body_safety() {
    let site = spawn_site().await;
    let http = Client::new();
    let admin = admin_login(&http, &site).await;

    let install = http
        .post(format!("{}/api/admin/themes", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(zip_fixture_theme())
        .send()
        .await
        .unwrap();
    let status = install.status();
    let body = install.text().await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "install: {body}");
    let installed: Value = serde_json::from_str(&body).unwrap();
    let rev_id = installed["revision_id"].as_str().unwrap();

    let themes: Value = http
        .get(format!("{}/api/admin/themes", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ver = themes["selection"]["version"].as_i64().unwrap();
    let activate = http
        .put(format!("{}/api/admin/themes/active", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .header(header::IF_MATCH, format!("\"v{ver}\""))
        .json(&json!({"revision_id":rev_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(activate.status(), StatusCode::OK);

    let builder_body = concat!(
        "<!-- od:section {\"id\":\"sec-1\"} -->\n",
        "<!-- od:row {\"id\":\"row-1\"} -->\n",
        "<!-- od:column {\"id\":\"col-1\"} -->\n",
        "<!-- od:plugin_block {\"id\":\"blk-1\",\"block_id\":\"trusted-html-e2e/badge\",\"values\":{\"label\":\"Passed\"}} /-->\n",
        "<!-- /od:column -->\n<!-- /od:row -->\n<!-- /od:section -->"
    );
    let home_id = create_published_page(&http, &site, &admin, "trusted-home", builder_body).await;
    create_published_page(
        &http,
        &site,
        &admin,
        "trusted-page",
        "<p>Reader text</p><script>alert('untrusted')</script><svg onload=evil()>",
    )
    .await;

    let reading = http
        .get(format!("{}/api/admin/settings/reading", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap();
    let etag = reading.headers()[header::ETAG].to_str().unwrap().to_owned();
    let reading_set = http
        .put(format!("{}/api/admin/settings/reading", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &admin.csrf)
        .header(header::IF_MATCH, etag)
        .json(&json!({"homepage_page_id":home_id,"posts_page_id":null}))
        .send()
        .await
        .unwrap();
    assert_eq!(reading_set.status(), StatusCode::OK);

    let home = http.get(format!("{}/", site.base)).send().await.unwrap();
    assert_eq!(home.status(), StatusCode::OK);
    let csp = home.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(csp.contains("script-src 'self' 'unsafe-inline'"), "{csp}");
    assert!(csp.contains("object-src 'none'"), "{csp}");
    let html = home.text().await.unwrap();
    assert!(html.contains("<title>Brand</title>"), "{html}");
    assert!(html.contains("xlink:href=\"#logo\""), "{html}");
    assert!(
        html.contains("onclick=\"document.body.classList.toggle('nav-open')\""),
        "{html}"
    );
    assert!(html.contains("window.innerWidth < 768"), "{html}");
    assert!(html.contains("if (2 < 3)"), "{html}");
    assert!(html.contains("Passed"), "{html}");
    assert!(!html.contains("OD_ICON_"), "{html}");
    assert_order(
        &html,
        &[
            "/assets/opendash-tailwind.css",
            "/assets/base.css",
            "/assets/home.css",
            "/assets/init.mjs",
            "</head>",
            "trusted-header",
            "Trusted Footer",
            "/assets/vendor.js",
            "/assets/app.js",
        ],
    );
    assert_eq!(html.matches("/assets/base.css").count(), 1);
    assert!(html.contains("<script src="));

    let page = http
        .get(format!("{}/trusted-page", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let page_html = page.text().await.unwrap();
    assert!(page_html.contains("/assets/base.css"), "{page_html}");
    assert!(!page_html.contains("/assets/home.css"), "{page_html}");
    assert!(
        !page_html.contains("<script>alert('untrusted')</script>"),
        "{page_html}"
    );
    assert!(!page_html.contains("<svg onload=evil()>"), "{page_html}");

    let digest = html
        .split("/theme/")
        .nth(1)
        .unwrap()
        .split('/')
        .next()
        .unwrap();
    assert_eq!(digest.len(), 64);
    let js = http
        .get(format!("{}/theme/{digest}/assets/app.js", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(js.status(), StatusCode::OK);
    assert_eq!(
        js.headers()[header::CONTENT_TYPE],
        "text/javascript; charset=utf-8"
    );
    let module = http
        .get(format!("{}/theme/{digest}/assets/init.mjs", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(module.status(), StatusCode::OK);
    assert_eq!(
        module.headers()[header::CONTENT_TYPE],
        "text/javascript; charset=utf-8"
    );
    let svg = http
        .get(format!("{}/theme/{digest}/assets/logo.svg", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(svg.status(), StatusCode::OK);
    assert_eq!(svg.headers()[header::CONTENT_TYPE], "image/svg+xml");
    assert_eq!(
        svg.headers()[header::CONTENT_SECURITY_POLICY],
        "default-src 'none'; style-src 'unsafe-inline'; sandbox"
    );
}
