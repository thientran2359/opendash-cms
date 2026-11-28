#[path = "../../src/template_sec/mod.rs"]
mod template_sec;

use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::Client;
use serde_json::Value;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

struct TestServer {
    child: Child,
    site_root: PathBuf,
    base_url: String,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.site_root);
    }
}

async fn spawn_server() -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // SitePaths rejects symlinked ancestors. On macOS /tmp redirects to
    // /private/tmp, so canonicalize the temporary parent before spawning.
    let site_root = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "opendash-m5-s03-t01-{}-{nanos}",
            std::process::id()
        ));
    fs::create_dir_all(&site_root).unwrap();

    let bin = env!("CARGO_BIN_EXE_opendash");
    let child = Command::new(bin)
        .env("OPENDASH_SITE_ROOT", &site_root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn opendash server");

    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    for _ in 0..50 {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await
            && resp.status().is_success()
        {
            return TestServer {
                child,
                site_root,
                base_url,
            };
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("opendash server did not become healthy in time");
}

fn build_zip(entries: &[(&str, &[u8], CompressionMethod)]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
        for (name, data, method) in entries {
            let opts = SimpleFileOptions::default().compression_method(*method);
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }
    buf
}

#[test]
fn csec_accepts_safe_subset_and_rejects_dangerous_constructs() {
    let safe_tpl = r#"
<article class="post-card">
  <h1>{{ entry.title | default("Untitled") | trim }}</h1>
  {% if entry.url %}<a href="{{ entry.url }}">Read</a>{% endif %}
  {% if entry.cover_url %}<img src="{{ entry.cover_url }}" alt="{{ entry.title }}">{% endif %}
  {% for f in entry.fields %}
    <p>{{ f.label }}: {{ f.text }}</p>
  {% endfor %}
</article>
"#;
    template_sec::validate_template_csec("single", safe_tpl).expect("safe template must pass");

    let dangerous_templates = [
        ("<script>alert(1)</script>", "script tag"),
        ("<div onclick=\"alert(1)\">hi</div>", "inline event handler"),
        ("<a href=\"javascript:alert(1)\">x</a>", "javascript url"),
        ("<iframe src=\"/x\"></iframe>", "iframe tag"),
        ("<svg><circle cx=\"1\"/></svg>", "svg tag"),
        ("<style>@import url('/x.css');</style>", "style @import"),
        (
            "<a href=\"{{ untrusted_url }}\">x</a>",
            "non-allowlisted dynamic href",
        ),
        ("{% macro evil() %}hi{% endmacro %}", "jinja macro"),
        ("{% include 'other.html' %}", "jinja include"),
        ("{% import 'other.html' as o %}", "jinja import"),
        ("{{ entry.title | safe }}", "safe filter"),
        ("{{ [1,2,3] | unknown_filter }}", "non-allowlisted filter"),
    ];
    for (tpl, label) in dangerous_templates {
        assert!(
            template_sec::validate_template_csec("test", tpl).is_err(),
            "C-SEC must reject {label}: {tpl}"
        );
    }

    assert!(template_sec::validate_safe_css("body { color: #111; }").is_ok());
    assert!(template_sec::validate_safe_css("@import 'evil.css';").is_err());
    assert!(template_sec::validate_safe_css("body { background: url(/x.png); }").is_err());

    let ctx = template_sec::json_to_safe_value(&serde_json::json!({
        "entry": { "title": "<script>xss</script>", "url": "/hi", "cover_url": null, "fields": [] }
    }));
    let rendered = template_sec::render_csec("single", safe_tpl, ctx).unwrap();
    assert!(
        rendered.contains("&lt;script&gt;xss&lt;&#x2f;script&gt;")
            || rendered.contains("&lt;script&gt;xss&lt;/script&gt;")
    );
    assert!(!rendered.contains("<script>"));
}

// S03: Home is a published Page selected atomically in Reading Settings.
async fn seed_builder_homepage(client: &Client, base_url: &str, cookie: &str, csrf: &str) -> i64 {
    let types: Vec<Value> = client
        .get(format!("{base_url}/api/admin/content-types"))
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let schema_token = types
        .iter()
        .find(|v| v["descriptor"]["key"] == "page")
        .unwrap()["schema_token"]
        .as_str()
        .unwrap();
    let created = client
        .post(format!(
            "{base_url}/api/admin/content-types/page/entries/composite"
        ))
        .header("Cookie", cookie)
        .header("Origin", base_url)
        .header("X-CSRF-Token", csrf)
        .json(&serde_json::json!({
            "expected_schema_token": schema_token,
            "slug":"theme-home-fixture",
            "title":"Theme Home Fixture",
            "values":{"body":"Theme Home Fixture"},
            "publication_status":"published"
        }))
        .send()
        .await
        .unwrap();
    let status = created.status();
    let raw = created.text().await.unwrap();
    assert_eq!(status, 201, "S03 Home Page create: {raw}");
    let page_id = serde_json::from_str::<Value>(&raw).unwrap()["entry"]["id"]
        .as_i64()
        .unwrap();
    let reading = client
        .get(format!("{base_url}/api/admin/settings/reading"))
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(reading.status(), 200);
    let etag = reading.headers()["etag"].to_str().unwrap().to_owned();
    let assigned = client
        .put(format!("{base_url}/api/admin/settings/reading"))
        .header("Cookie", cookie)
        .header("Origin", base_url)
        .header("X-CSRF-Token", csrf)
        .header("If-Match", etag)
        .json(&serde_json::json!({"homepage_page_id":page_id,"posts_page_id":null}))
        .send()
        .await
        .unwrap();
    let status = assigned.status();
    let body = assigned.text().await.unwrap();
    assert_eq!(status, 200, "S03 Reading assignment: {body}");
    page_id
}

#[tokio::test]
async fn m5_s03_t01_roles_csec_install_compose_and_fallback() {
    let srv = spawn_server().await;
    let client = Client::new();

    // 1. Setup & login
    let setup_resp = client
        .post(format!("{}/auth/setup", srv.base_url))
        .json(&serde_json::json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup_resp.status(), 201);

    let login_resp = client
        .post(format!("{}/auth/login", srv.base_url))
        .json(&serde_json::json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login_resp.status(), 200);
    let cookie = login_resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
        .collect::<Vec<_>>()
        .join("; ");

    let login_json: Value = login_resp.json().await.unwrap();
    let csrf = login_json["csrf_token"].as_str().unwrap().to_string();

    // 2. Reject v2 theme violating C-SEC at install time
    let bad_manifest = br#"{
        "manifest_version": 1,
        "theme_id": "bad-csec",
        "name": "Bad CSEC",
        "version": "1.0.0",
        "templates": [{ "kind": "single", "file": "single.html" }]
    }"#;
    let bad_zip = build_zip(&[
        ("theme.json", bad_manifest, CompressionMethod::Deflated),
        (
            "single.html",
            b"<article>{{ entry.title }}<iframe src='/'></iframe>{{ od_content() }}</article>",
            CompressionMethod::Deflated,
        ),
    ]);
    let bad_install = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(reqwest::header::CONTENT_TYPE, "application/zip")
        .body(bad_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(bad_install.status(), 400);

    // 3. Install & activate valid v2 theme with roles
    let good_manifest = br#"{
        "manifest_version": 1,
        "theme_id": "editorial-roles",
        "name": "Editorial Roles",
        "version": "1.0.0",
        "assets": ["assets/style.css"],
        "templates": [{ "kind": "page", "file": "page.html" }, { "kind": "single", "file": "single.html" }, { "kind": "archive", "file": "archive.html" }, { "kind": "taxonomy", "file": "taxonomy.html" }], "parts": { "header": "header.html", "footer": "footer.html" }
    }"#;
    let good_zip = build_zip(&[
        ("theme.json", good_manifest, CompressionMethod::Deflated),
        (
            "header.html",
            b"<header data-role=\"header\">{{ site.title }}</header>",
            CompressionMethod::Deflated,
        ),
        (
            "footer.html",
            b"<footer data-role=\"footer\">Footer</footer>",
            CompressionMethod::Deflated,
        ),
        (
            "page.html",
            b"<main data-role=\"home\"><h1>{{ site.title }}</h1>{{ od_content() }}</main>",
            CompressionMethod::Deflated,
        ),
        (
            "single.html",
            b"<main data-role=\"single\"><h1>{{ entry.title }}</h1>{{ od_content() }}</main>",
            CompressionMethod::Deflated,
        ),
        (
            "archive.html",
            b"<main data-role=\"archive\"><h1>{{ content_type.label }}</h1></main>",
            CompressionMethod::Deflated,
        ),
        (
            "taxonomy.html",
            b"<main data-role=\"taxonomy\"><h1>{{ content_type.label }}</h1></main>",
            CompressionMethod::Deflated,
        ),
        (
            "assets/style.css",
            b"body { color: #222; }",
            CompressionMethod::Deflated,
        ),
    ]);
    let install_resp = client
        .post(format!("{}/api/admin/themes", srv.base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(reqwest::header::CONTENT_TYPE, "application/zip")
        .body(good_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(install_resp.status(), 201);
    let installed: Value = install_resp.json().await.unwrap();
    let rev_id = installed["revision_id"].as_str().unwrap().to_string();
    let rev_digest = installed["revision_digest"].as_str().unwrap().to_string();

    let activate_resp = client
        .put(format!("{}/api/admin/themes/active", srv.base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&serde_json::json!({ "revision_id": rev_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(activate_resp.status(), 200);

    let home_id = seed_builder_homepage(&client, &srv.base_url, &cookie, &csrf).await;

    // 4. Create and publish a post entry
    let create_entry = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            srv.base_url
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .json(&serde_json::json!({
            "slug": "hello-roles",
            "title": "Hello Roles",
            "values": {},
            "term_ids": []
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_entry.status(), 201);
    let entry_json: Value = create_entry.json().await.unwrap();
    let entry_id = entry_json["id"].as_i64().unwrap();
    let entry_ver = entry_json["version"].as_i64().unwrap();

    let publish_entry = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{entry_id}/publication",
            srv.base_url
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header("If-Match", format!("\"v{entry_ver}\""))
        .json(&serde_json::json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(publish_entry.status(), 200);

    // 5. Verify public routes compose header + role + footer and include CSP style-src 'self' 'unsafe-inline'
    for (path, marker) in [
        ("/", "Theme Home Fixture"),
        ("/posts", "data-role=\"archive\""),
        ("/hello-roles", "data-role=\"single\""),
    ] {
        let resp = client
            .get(format!("{}{}", srv.base_url, path))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "path={path}");
        let csp = resp
            .headers()
            .get(reqwest::header::CONTENT_SECURITY_POLICY)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            csp.contains("style-src 'self' 'unsafe-inline'"),
            "csp={csp}"
        );
        let body = resp.text().await.unwrap();
        assert!(body.contains("data-role=\"header\""), "{path}: {body}");
        assert!(body.contains(marker), "{path}: {body}");
        assert!(body.contains("data-role=\"footer\""), "{path}: {body}");
    }

    // 6. S03 selects the typed Home override for the assigned published Page.
    // Editing the Page's own Builder document leaves the shared override intact.
    let shared = client.put(format!("{}/api/admin/template-overrides", srv.base_url))
        .header("Cookie", &cookie).header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&serde_json::json!({
            "target":{"kind":"page"},
            "document":{"schema_version":1,"blocks":[{"id":"shared-sec","block_id":"core/section","children":[
                {"id":"shared-row","block_id":"core/row","children":[{"id":"shared-col","block_id":"core/column","children":[
                    {"id":"shared-title","block_id":"core/heading","props":{"level":"h1","text":"Shared Home Template"}},
                    {"id":"shared-slot","block_id":"core/entry-content","props":{}}
                ]}]}
            ]}]}
        })).send().await.unwrap();
    assert_eq!(shared.status(), 201);

    let public_before = client
        .get(format!("{}/", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(public_before.contains("Theme Home Fixture"));
    assert!(public_before.contains("Shared Home Template"));

    // The retired standalone homepage writer must remain unreachable.
    assert_eq!(
        client
            .get(format!("{}/api/admin/builder/homepage", srv.base_url))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let entry_path = format!("{}/api/admin/entries/page/{home_id}/builder", srv.base_url);
    let current = client
        .get(&entry_path)
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(current.status(), 200);
    let etag = current.headers()["etag"].to_str().unwrap().to_owned();
    let updated = serde_json::json!({
        "schema_version":1,"blocks":[{"id":"home-sec","block_id":"core/section","children":[
            {"id":"home-row","block_id":"core/row","children":[{"id":"home-col","block_id":"core/column","children":[
                {"id":"home-title","block_id":"core/heading","props":{"level":"h1","text":"Saved Builder Home"}}
            ]}]}
        ]}]
    });
    let saved = client
        .put(&entry_path)
        .header("Cookie", &cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", etag)
        .json(&updated)
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), 200);

    let home_after = client
        .get(format!("{}/", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(home_after.status(), 200);
    let home_body = home_after.text().await.unwrap();
    assert!(home_body.contains("Saved Builder Home"), "{home_body}");
    assert!(home_body.contains("Shared Home Template"), "{home_body}");
    assert!(home_body.contains("data-role=\"header\""), "{home_body}");
    assert!(home_body.contains("data-role=\"footer\""), "{home_body}");
    assert!(!home_body.contains("<script>"), "{home_body}");

    // 7. Corrupt v3 revision on disk -> public routes fall back cleanly to core default
    let rev_single = srv
        .site_root
        .join("themes")
        .join("revisions")
        .join(&rev_digest)
        .join("single.html");
    fs::write(&rev_single, b"<main>tampered</main>").unwrap();

    let fallback_single = client
        .get(format!("{}/hello-roles", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(fallback_single.status(), 200);
    let fallback_body = fallback_single.text().await.unwrap();
    assert!(fallback_body.contains("Hello Roles"), "{fallback_body}");
    assert!(!fallback_body.contains("tampered"), "{fallback_body}");
}
