use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

struct Server {
    child: Child,
    root: PathBuf,
    origin: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

async fn spawn_server() -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let raw_root = std::env::temp_dir().join(format!(
        "opendash-search-theme-{}-{port}-{seq}-{nanos}",
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
    let origin = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    for _ in 0..200 {
        if client
            .get(format!("{origin}/api/health"))
            .send()
            .await
            .is_ok_and(|r| r.status() == StatusCode::OK)
        {
            return Server {
                child,
                root,
                origin,
            };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let log_text = fs::read_to_string(root.join("server.log")).unwrap_or_default();
    panic!("server did not start at {origin}: {log_text}");
}

async fn setup_and_login(client: &Client, origin: &str) -> (String, String) {
    let setup = client
        .post(format!("{origin}/auth/setup"))
        .header(header::ORIGIN, origin)
        .json(&json!({ "username": "admin", "password": "SearchThemePassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = client
        .post(format!("{origin}/auth/login"))
        .header(header::ORIGIN, origin)
        .json(&json!({ "username": "admin", "password": "SearchThemePassword123!" }))
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

fn build_theme_zip(theme_id: &str, templates: &[(&str, &str)], search_html: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let tpl_manifest: Vec<Value> = templates
            .iter()
            .map(|(kind, file)| json!({ "kind": kind, "file": file }))
            .collect();
        let manifest = json!({
            "manifest_version": 1,
            "theme_id": theme_id,
            "name": theme_id,
            "version": "1.0.0",
            "templates": tpl_manifest,
        });
        zip.start_file("theme.json", opts).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        for (_, file) in templates {
            zip.start_file(*file, opts).unwrap();
            let body = if *file == "search.html" {
                search_html
            } else {
                "<main><h1>{{ site.title }}</h1>{{ od_content() }}</main>"
            };
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

fn csp_header(headers: &header::HeaderMap) -> &str {
    headers
        .get(header::CONTENT_SECURITY_POLICY)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

#[tokio::test]
async fn starter_search_template_header_option_and_csec_theme_validation() {
    let srv = spawn_server().await;
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &srv.origin).await;

    // 1. Default Starter Home keeps link to / and CSP form-action 'none'.
    let home = client.get(format!("{}/", srv.origin)).send().await.unwrap();
    assert_eq!(home.status(), StatusCode::OK);
    assert!(csp_header(home.headers()).contains("form-action 'none'"));
    let home_html = home.text().await.unwrap();
    assert!(!home_html.contains("<form "));

    // 2. Public search on Starter renders search.html with form-action 'self', no-store, noindex.
    let create = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "title": "Starter Rust FTS5 Guide",
            "slug": "starter-rust-fts5-guide",
            "values": { "body": "Indexing Rust single binary search with FTS5." }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created: Value = create.json().await.unwrap();
    let created_id = created["id"].as_i64().unwrap();
    let created_ver = created["version"].as_i64().unwrap();
    let pub_post = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{created_id}/publication",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{created_ver}\""))
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_post.status(), StatusCode::OK);

    let search = client
        .get(format!("{}/?s=FTS5", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(search.status(), StatusCode::OK);
    assert!(csp_header(search.headers()).contains("form-action 'self'"));
    assert_eq!(
        search.headers()[header::CACHE_CONTROL]
            .to_str()
            .unwrap_or(""),
        "no-store"
    );
    assert_eq!(
        search.headers()["x-robots-tag"].to_str().unwrap_or(""),
        "noindex, follow"
    );
    let search_html = search.text().await.unwrap();
    assert!(search_html.contains(r#"<form method="get" action="&#x2f;""#));
    assert!(search_html.contains("Starter Rust FTS5 Guide"));
    assert!(search_html.contains("<mark"));

    // 3. Enabling show_header_search puts a GET search form in header on /.
    let opts = client
        .get(format!(
            "{}/api/themes/opendash-starter/options",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(opts.status(), StatusCode::OK);
    let etag = opts
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("\"v0\"")
        .to_owned();
    let put_opt = client
        .put(format!(
            "{}/api/themes/opendash-starter/options",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, etag)
        .json(&json!({ "values": { "show_header_search": true } }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_opt.status(), StatusCode::OK);

    let home_with_search = client
        .get(format!("{}/posts", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(home_with_search.status(), StatusCode::OK);
    assert!(csp_header(home_with_search.headers()).contains("form-action 'self'"));
    let home_search_html = home_with_search.text().await.unwrap();
    assert!(home_search_html.contains(r#"<form method="get" action="&#x2f;""#));

    // 4. Malicious theme form markup is rejected on install; safe bounded GET form succeeds.
    for (id, bad_html) in [
        (
            "bad-post-form",
            r#"<main><form method="post" action="/"></form></main>"#,
        ),
        (
            "bad-api-action",
            r#"<main><form method="get" action="/api/public/search"></form></main>"#,
        ),
        (
            "bad-password-input",
            r#"<main><form method="get" action="/"><input type="password" name="s"></form></main>"#,
        ),
    ] {
        let zip = build_theme_zip(
            id,
            &[("index", "index.html"), ("search", "search.html")],
            bad_html,
        );
        let upload = client
            .post(format!("{}/api/admin/themes", srv.origin))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &srv.origin)
            .header("X-CSRF-Token", &csrf)
            .header(header::CONTENT_TYPE, "application/zip")
            .body(zip)
            .send()
            .await
            .unwrap();
        assert!(
            upload.status() == StatusCode::BAD_REQUEST
                || upload.status() == StatusCode::UNPROCESSABLE_ENTITY,
            "{id} unexpected status: {}",
            upload.status()
        );
    }

    let safe_zip = build_theme_zip(
        "good-search-theme",
        &[("index", "index.html"), ("search", "search.html")],
        r#"<main><search><form method="get" action="{{ search.action_url }}"><input type="search" name="s" value="{{ search.query }}"><button type="submit">Go</button></form></search></main>"#,
    );
    let safe_upload = client
        .post(format!("{}/api/admin/themes", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(safe_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(safe_upload.status(), StatusCode::CREATED);
}
