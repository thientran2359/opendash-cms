//! S02: exact source tests for curated icons, C-SEC validation and CLI.
//! All CLI fixtures live in temporary directories; no live site is touched.

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

fn temp_theme_dir() -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("opendash-s02-icons-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write_theme(path: &Path, icon: &str) {
    fs::write(path.join("theme.json"), r#"{"manifest_version":1,"theme_id":"safe-icons-test","name":"Safe Icons","version":"1.0.0","parts":{"header":"header.html"},"templates":[{"kind":"page","file":"page.html"}]}"#).unwrap();
    fs::write(
        path.join("header.html"),
        "<header>{{ od_icon('arrow-right') }}</header>",
    )
    .unwrap();
    fs::write(
        path.join("page.html"),
        format!("<main>{icon}{{{{ od_content() }}}}</main>"),
    )
    .unwrap();
}

fn check_theme(path: &Path) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .arg("--check-theme")
        .arg(path)
        .output()
        .unwrap();
    (
        output.status.success(),
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

#[test]
fn cli_accepts_inline_icon_and_rejects_untrusted_icon_forms() {
    let path = temp_theme_dir();
    write_theme(&path, "{{ od_icon('calendar', 'w-3.5 h-3.5') }}");
    let (ok, output) = check_theme(&path);
    assert!(ok, "theme validation failed: {output}");

    for invalid in [
        "{{ od_icon('not-real') }}",
        "{{ od_icon('calendar', 'a\" onclick=\"x') }}",
        "<div title=\"{{ od_icon('calendar') }}\">bad</div>",
        "{% set captured %}{{ od_icon('calendar') }}{% endset %}",
        "{{ od_icon('calendar') | upper }}",
        "OD_ICON_0123456789abcdef0123456789abcdef",
        "<object><param name='unsafe'/></object>",
    ] {
        write_theme(&path, invalid);
        let (ok, output) = check_theme(&path);
        assert!(!ok, "unsafe fixture was accepted: {invalid}: {output}");
    }
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn curated_icon_registry_matches_expected_names_and_safe_markup() {
    assert_eq!(opendash_core::icons::CORE_ICONS.len(), 48);
    for (name, svg) in opendash_core::icons::CORE_ICONS {
        assert!(opendash_core::icons::lookup_core_icon_paths(name).is_some());
        assert!(svg.contains('<'));
        for dangerous in [
            "<script",
            "<svg",
            " onload",
            " style=",
            " href=",
            "foreignObject",
        ] {
            assert!(!svg.contains(dangerous), "invalid static icon {name}");
        }
    }
}

struct TestServer {
    child: Child,
    origin: String,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn spawn_server(site_root: &Path) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    assert_ne!(port, 18964, "never touch pinned live site port");
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .env("OPENDASH_SITE_ROOT", site_root)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut srv = TestServer {
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
            panic!("server exited early {exit}: {log}");
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
    panic!("server did not become healthy");
}

fn icons_theme_zip() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut writer = ZipWriter::new(std::io::Cursor::new(&mut bytes));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = br#"{
            "manifest_version": 1,
            "theme_id": "safe-icons-e2e",
            "name": "Safe Icons E2E",
            "version": "1.0.0",
            "parts": { "header": "header.html", "footer": "footer.html" },
            "templates": [
                { "kind": "page", "file": "page.html" },
                { "kind": "index", "file": "index.html" }
            ],
            "page_layouts": [
                { "key": "default", "label": "Default", "file": "page-layouts/default.html" },
                { "key": "canvas", "label": "Canvas", "file": "page-layouts/canvas.html", "chrome": "none" }
            ]
        }"#;
        let block_json = br#"{
            "schema_version": 1,
            "id": "safe-icons-e2e/badge",
            "title": "Badge",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "key": "label", "label": "Label", "type": "text", "default": "Verified" }
            ]
        }"#;
        for (path, content) in [
            ("theme.json", manifest.as_slice()),
            (
                "header.html",
                b"<header class=\"e2e-hdr\">{{ od_icon('arrow-right', 'w-5 h-5') }}</header>"
                    .as_slice(),
            ),
            ("footer.html", b"<footer class=\"e2e-ftr\">end</footer>"),
            (
                "page.html",
                b"<main>{{ od_icon('calendar', 'w-3.5 h-3.5') }}{{ od_content() }}</main>",
            ),
            ("index.html", b"<main>{{ od_content() }}</main>"),
            (
                "page-layouts/default.html",
                b"<main class=\"layout-default\">{{ od_content() }}</main>",
            ),
            (
                "page-layouts/canvas.html",
                b"<main class=\"layout-canvas\">{{ od_content() }}</main>",
            ),
            ("blocks/badge/block.json", block_json.as_slice()),
            (
                "blocks/badge/render.html",
                b"<div class=\"e2e-badge\">{{ od_icon('check-circle', 'w-4 h-4') }}<span>{{ label }}</span></div>",
            ),
        ] {
            writer.start_file(path, options).unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
    }
    bytes
}

#[tokio::test]
async fn e2e_page_layout_chrome_and_inline_icons_with_strict_csp() {
    let raw_dir = temp_theme_dir();
    let root: PathBuf = fs::canonicalize(&raw_dir).unwrap();
    let srv = spawn_server(&root).await;
    let client = Client::new();

    let setup = client
        .post(format!("{}/auth/setup", srv.origin))
        .json(&json!({"username":"admin","password":"IconsE2ePassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);

    let login = client
        .post(format!("{}/auth/login", srv.origin))
        .json(&json!({"username":"admin","password":"IconsE2ePassword123!"}))
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

    let editor_cfg: Value = client
        .get(format!(
            "{}/api/admin/content-types/page/effective-editor-config",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let schema_token = editor_cfg["schema_token"].as_str().unwrap().to_owned();

    // 1. CUJ 1 on built-in opendash-starter: default & full_width keep Header+Footer, canvas hides both.
    for (slug, layout, expect_chrome) in [
        ("layout-default", "default", true),
        ("layout-full", "full_width", true),
        ("layout-canvas", "canvas", false),
    ] {
        let created = client
            .post(format!(
                "{}/api/admin/content-types/page/entries/composite",
                srv.origin
            ))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &srv.origin)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({
                "expected_schema_token": schema_token,
                "title": slug,
                "slug": slug,
                "values": { "body": "<p>Hello layout</p>" },
                "page_layout_key": layout
            }))
            .send()
            .await
            .unwrap();
        let status = created.status();
        let body_text = created.text().await.unwrap();
        assert_eq!(status, StatusCode::CREATED, "create {slug}: {body_text}");
        let entry: Value = serde_json::from_str(&body_text).unwrap();
        let entry_obj = entry.get("entry").unwrap_or(&entry);
        assert_eq!(
            entry_obj["page_layout_key"], layout,
            "stored page_layout_key for {slug}: {body_text}"
        );
        let id = entry_obj["id"].as_i64().unwrap();
        let version = entry_obj["version"].as_i64().unwrap();
        let pub_resp = client
            .post(format!(
                "{}/api/admin/content-types/page/entries/{id}/publication",
                srv.origin
            ))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &srv.origin)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, format!("\"v{version}\""))
            .json(&json!({"status": "published"}))
            .send()
            .await
            .unwrap();
        assert_eq!(pub_resp.status(), StatusCode::OK, "publish {slug}");

        let public = client
            .get(format!("{}/{slug}", srv.origin))
            .send()
            .await
            .unwrap();
        assert_eq!(public.status(), StatusCode::OK, "GET /{slug}");
        let html = public.text().await.unwrap();
        assert_eq!(
            html.contains("OPENDASH STARTER"),
            expect_chrome,
            "starter header for {slug}: {html}"
        );
        assert_eq!(
            html.contains("<footer"),
            expect_chrome,
            "starter footer for {slug}: {html}"
        );
    }

    // 2. CUJ 3: Install & activate Theme v3 with od_icon() in header, page.html and Declarative Block.
    let install = client
        .post(format!("{}/api/admin/themes", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "application/zip")
        .body(icons_theme_zip())
        .send()
        .await
        .unwrap();
    let install_status = install.status();
    let install_text = install.text().await.unwrap();
    assert_eq!(
        install_status,
        StatusCode::CREATED,
        "install theme: {install_text}"
    );
    let installed: Value = serde_json::from_str(&install_text).unwrap();
    let rev_id = installed["revision_id"].as_str().unwrap();

    let themes: Value = client
        .get(format!("{}/api/admin/themes", srv.origin))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let sel_version = themes["selection"]["version"].as_i64().unwrap();
    let activate = client
        .put(format!("{}/api/admin/themes/active", srv.origin))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{sel_version}\""))
        .json(&json!({"revision_id": rev_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(activate.status(), StatusCode::OK);

    let editor_cfg2: Value = client
        .get(format!(
            "{}/api/admin/content-types/page/effective-editor-config",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let schema_token2 = editor_cfg2["schema_token"].as_str().unwrap().to_owned();

    let block_body = concat!(
        "<!-- od:core/section {\"id\":\"sec-1\"} -->\n",
        "<!-- od:core/row {\"id\":\"row-1\"} -->\n",
        "<!-- od:core/column {\"id\":\"col-1\"} -->\n",
        "<!-- od:safe-icons-e2e/badge {\"id\":\"blk-1\",\"props\":{\"label\":\"OK\"}} /-->\n",
        "<!-- /od:core/column -->\n<!-- /od:core/row -->\n<!-- /od:core/section -->"
    );
    let icon_page = client
        .post(format!(
            "{}/api/admin/content-types/page/entries/composite",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "expected_schema_token": schema_token2,
            "title": "Icon E2E",
            "slug": "icon-e2e",
            "values": { "body": block_body }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(icon_page.status(), StatusCode::CREATED);
    let icon_entry: Value = icon_page.json().await.unwrap();
    let icon_obj = icon_entry.get("entry").unwrap_or(&icon_entry);
    let icon_id = icon_obj["id"].as_i64().unwrap();
    let icon_ver = icon_obj["version"].as_i64().unwrap();
    let icon_pub = client
        .post(format!(
            "{}/api/admin/content-types/page/entries/{icon_id}/publication",
            srv.origin
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{icon_ver}\""))
        .json(&json!({"status": "published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(icon_pub.status(), StatusCode::OK);

    let public = client
        .get(format!("{}/icon-e2e", srv.origin))
        .send()
        .await
        .unwrap();
    assert_eq!(public.status(), StatusCode::OK);
    let csp = public
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(csp.contains("img-src 'self'"), "unexpected CSP: {csp}");
    assert!(
        csp.contains("script-src 'self' 'unsafe-inline'"),
        "unexpected CSP: {csp}"
    );
    let html = public.text().await.unwrap();
    assert!(!html.contains("OD_ICON_"), "raw icon nonce leaked");
    assert!(
        html.contains("<svg xmlns=\"http://www.w3.org/2000/svg\""),
        "missing inline svg"
    );
    assert!(html.contains("class=\"w-5 h-5\""), "missing header icon");
    assert!(
        html.contains("class=\"w-3.5 h-3.5\""),
        "missing page.html icon"
    );
    assert!(
        html.contains("class=\"w-4 h-4\""),
        "missing declarative block icon via od_content(): {html}"
    );

    drop(srv);
    let _ = fs::remove_dir_all(&root);
}
