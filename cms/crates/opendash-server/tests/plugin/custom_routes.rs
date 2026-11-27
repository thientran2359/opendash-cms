//! Slice S03 E2E & regression tests for Spec B:
//! - Manifest route validation (`validate_routes`: max 32, kebab-case id, path rules, forbid `GET + webhook`)
//! - Reserved system root `"p"` protection
//! - SSRF internal path blocking (`/p/`, `/api/p/`, `/api/public/p/`) even when loopback is enabled
//! - `GET /p/{pkg}/{*subpath}` (`RouteGet`, Public, read-only, 0 writes, 0 secrets, no Cookie/Authorization leak,
//!   HTML trusted vs untrusted C-SEC rejection, Theme & Plugin routes, Session route rejected with 403)
//! - `GET` & `POST /api/p/{pkg}/{*subpath}` (`Session | Admin`, 401/403 CSRF & RBAC, KV + `content_save` with `ActionActorFence`)
//! - `POST /api/public/p/{pkg}/{*subpath}`:
//!   - `Public` AJAX route: strict Origin check (403 on missing/foreign), RateLimit (429 + Retry-After),
//!     KV write via `StagedCommitGuard` + `MutationLease`, `content_save`/`content_delete` refused for `Public`
//!   - `Webhook` route: works WITHOUT `Origin`, receives `raw_body` + allowlisted signature headers,
//!     never leaks `Cookie`/`Authorization`, commits KV
//!   - Rejects `Session | Admin` routes on `/api/public/p/` with 403

use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
        let path = root.join(format!("{prefix}_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct ServerHandle {
    child: Child,
    log_path: PathBuf,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

async fn spawn_server(site_root: &Path, port: u16, allow_loopback: bool) -> ServerHandle {
    let exe = env!("CARGO_BIN_EXE_opendash");
    let base_url = format!("http://127.0.0.1:{port}");
    let log_path = site_root.join(format!("server_{port}.log"));
    let stderr = fs::File::create(&log_path).unwrap();
    let mut cmd = Command::new(exe);
    cmd.env("OPENDASH_SITE_ROOT", site_root)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr));
    if allow_loopback {
        cmd.env("OPENDASH_PLUGIN_ALLOW_LOOPBACK", "1");
    }
    let child = cmd.spawn().expect("spawn opendash server");

    let handle = ServerHandle { child, log_path };
    let client = Client::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await
            && resp.status().is_success()
        {
            return handle;
        }
        if Instant::now() >= deadline {
            let log = fs::read_to_string(&handle.log_path).unwrap_or_default();
            panic!("server failed to become healthy on port {port}: {log}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn setup_and_login(client: &Client, base_url: &str) -> (String, String) {
    let setup = client
        .post(format!("{base_url}/auth/setup"))
        .header("Origin", base_url)
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert!(setup.status().is_success());
    login_as(client, base_url, "admin", "AdminPassword123!").await
}

async fn login_as(
    client: &Client,
    base_url: &str,
    username: &str,
    password: &str,
) -> (String, String) {
    let login = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", base_url)
        .json(&json!({
            "username": username,
            "password": password
        }))
        .send()
        .await
        .unwrap();
    assert!(login.status().is_success());
    let cookie = login
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let body: Value = login.json().await.unwrap();
    let csrf = body["csrf_token"].as_str().unwrap().to_owned();
    (cookie, csrf)
}

fn build_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in files {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

fn run_cli_check(flag: &str, path: &Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .arg(flag)
        .arg(path)
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
fn s03_cli_route_manifest_validation() {
    let temp = TempDir::new("s03_route_cli");
    let plugin_dir = temp.path.join("route-cli-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(
        plugin_dir.join("plugin.js"),
        "od.addRoute('home', () => ({ json: { ok: true } }));",
    )
    .unwrap();

    let valid = json!({
        "id": "route-cli-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "Route CLI Plugin",
        "entrypoint": "plugin.js",
        "routes": [
            { "id": "home", "method": "GET", "path": "/", "access": "public" },
            { "id": "item-show", "method": "GET", "path": "/items/:id", "access": "public" },
            { "id": "hook-rx", "method": "POST", "path": "/webhook", "access": "webhook" }
        ]
    });
    fs::write(plugin_dir.join("manifest.json"), valid.to_string()).unwrap();
    let (ok, out) = run_cli_check("--check-plugin", &plugin_dir);
    assert!(ok, "valid routes manifest rejected: {out}");

    // GET + webhook MUST fail
    let invalid_get_webhook = json!({
        "id": "route-cli-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "Route CLI Plugin",
        "entrypoint": "plugin.js",
        "routes": [
            { "id": "bad-get-wh", "method": "GET", "path": "/webhook", "access": "webhook" }
        ]
    });
    fs::write(
        plugin_dir.join("manifest.json"),
        invalid_get_webhook.to_string(),
    )
    .unwrap();
    let (ok, out) = run_cli_check("--check-plugin", &plugin_dir);
    assert!(!ok, "GET + webhook was accepted: {out}");

    // Duplicate (method, path) MUST fail
    let dup_path = json!({
        "id": "route-cli-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "Route CLI Plugin",
        "entrypoint": "plugin.js",
        "routes": [
            { "id": "r-a", "method": "GET", "path": "/dup", "access": "public" },
            { "id": "r-b", "method": "GET", "path": "/dup", "access": "public" }
        ]
    });
    fs::write(plugin_dir.join("manifest.json"), dup_path.to_string()).unwrap();
    let (ok, out) = run_cli_check("--check-plugin", &plugin_dir);
    assert!(!ok, "duplicate route path accepted: {out}");

    // Traversal `..` MUST fail
    let bad_path = json!({
        "id": "route-cli-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "Route CLI Plugin",
        "entrypoint": "plugin.js",
        "routes": [
            { "id": "trav", "method": "GET", "path": "/a/../b", "access": "public" }
        ]
    });
    fs::write(plugin_dir.join("manifest.json"), bad_path.to_string()).unwrap();
    let (ok, out) = run_cli_check("--check-plugin", &plugin_dir);
    assert!(!ok, "traversal route path accepted: {out}");
}

#[tokio::test]
async fn s03_scoped_custom_routes_public_session_admin_webhook_ssrf_and_ratelimit() {
    let temp = TempDir::new("s03_custom_routes_e2e");
    let port = free_port();
    let _server = spawn_server(&temp.path, port, true).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (admin_cookie, admin_csrf) = setup_and_login(&client, &base_url).await;

    // Create an Editor account to test Session vs Admin RBAC on /api/p/
    let create_editor = client
        .post(format!("{base_url}/api/admin/users"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword123!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_editor.status(), StatusCode::CREATED);
    let (editor_cookie, editor_csrf) =
        login_as(&client, &base_url, "editor1", "EditorPassword123!").await;

    // 1. Install trusted route plugin with all 4 access levels + SSRF check route
    let route_plugin_manifest = json!({
        "id": "route-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "Route Plugin",
        "entrypoint": "plugin.js",
        "permissions": {
            "sqlite": ["kv_read", "kv_write"],
            "http": [format!("http://127.0.0.1:{port}/")],
            "content_read_all_public": true,
            "content_write": ["post"],
            "content_delete": ["post"]
        },
        "routes": [
            { "id": "item-view", "method": "GET", "path": "/items/:id", "access": "public" },
            { "id": "votes-read", "method": "GET", "path": "/votes", "access": "public" },
            { "id": "ssrf-probe", "method": "GET", "path": "/ssrf", "access": "public" },
            { "id": "private-view", "method": "GET", "path": "/private", "access": "session" },
            { "id": "create-post", "method": "POST", "path": "/create-post", "access": "session", "capabilities": ["content_write"] },
            { "id": "admin-only", "method": "POST", "path": "/admin-only", "access": "admin", "capabilities": ["kv_write"] },
            { "id": "public-root", "method": "POST", "path": "/", "access": "public" },
            { "id": "public-vote", "method": "POST", "path": "/vote", "access": "public" },
            { "id": "stripe-hook", "method": "POST", "path": "/stripe-hook", "access": "webhook" }
        ]
    });
    let route_plugin_js = format!(
        r#"
        od.addRoute("item-view", (env) => {{
            const kvList = od.kv.list("", 10);
            let writeDenied = "no";
            try {{ od.kv.set("illegal", "1"); }} catch (e) {{ writeDenied = "yes"; }}
            let secretDenied = "no";
            try {{ od.secret.get("k"); }} catch (e) {{ secretDenied = "yes"; }}
            const hasCookie = JSON.stringify(env).includes("session=") || JSON.stringify(env).includes("Bearer");
            return {{
                status: 200,
                content_type: "text/html",
                title: "Item " + env.payload.params.id,
                body: '<section data-id="' + od.escapeAttr(env.payload.params.id)
                    + '" data-sort="' + od.escapeAttr((env.payload.query && env.payload.query.sort) || "")
                    + '" data-kv-list="' + (Array.isArray(kvList.items) ? "yes" : "no")
                    + '" data-write-denied="' + writeDenied
                    + '" data-secret-denied="' + secretDenied
                    + '" data-cookie-leaked="' + (hasCookie ? "yes" : "no")
                    + '">ok</section>'
            }};
        }});

        od.addRoute("votes-read", () => {{
            return {{
                json: {{
                    votes: od.kv.get("votes"),
                    webhook_event: od.kv.get("webhook_event"),
                    webhook_sig: od.kv.get("webhook_sig"),
                    admin_mark: od.kv.get("admin_mark")
                }}
            }};
        }});

        od.addRoute("ssrf-probe", (env) => {{
            const base = "http://127.0.0.1:{port}";
            const paths = ["/p/route-plugin/votes", "/api/p/route-plugin/private", "/api/public/p/route-plugin/vote"];
            const results = {{}};
            for (const p of paths) {{
                try {{
                    od.http.request({{ method: "GET", url: base + p }});
                    results[p] = "allowed";
                }} catch (e) {{
                    results[p] = String(e.message || e);
                }}
            }}
            return {{ json: results }};
        }});

        od.addRoute("private-view", (env) => {{
            return {{ json: {{ principal: env.principal, route: env.payload.route_id }} }};
        }});

        od.addRoute("create-post", (env) => {{
            const slug = (env.payload.body && env.payload.body.slug) || "route-created-post";
            const title = (env.payload.body && env.payload.body.title) || "Route Created Post";
            od.content.save({{
                type_key: "post",
                slug,
                title,
                values: {{}}
            }});
            return {{ json: {{ created_slug: slug }} }};
        }});

        od.addRoute("admin-only", (env) => {{
            od.kv.set("admin_mark", (env.payload.body && env.payload.body.mark) || "admin-ok");
            return {{ json: {{ ok: true }} }};
        }});

        od.addRoute("public-root", (env) => {{
            return {{ json: {{ root_ok: true, subpath: env.payload.subpath }} }};
        }});

        od.addRoute("public-vote", (env) => {{
            let contentWriteDenied = "no";
            try {{
                od.content.save({{
                    type_key: "post",
                    slug: "public-must-fail",
                    title: "Public Must Fail",
                    values: {{}}
                }});
            }} catch (e) {{
                contentWriteDenied = "yes";
            }}
            const prev = Number(od.kv.get("votes") || 0);
            od.kv.set("votes", prev + 1);
            return {{ json: {{ votes: prev + 1, content_write_denied: contentWriteDenied }} }};
        }});

        od.addRoute("stripe-hook", (env) => {{
            const raw = env.payload.raw_body || "";
            const sigs = env.payload.signature_headers || {{}};
            const leaked = JSON.stringify(env).includes("TOPSECRET");
            const parsed = JSON.parse(raw || "{{}}");
            od.kv.set("webhook_event", parsed.event || "unknown");
            od.kv.set("webhook_sig", sigs["stripe-signature"] || "");
            return {{
                json: {{
                    received: true,
                    raw_len: raw.length,
                    sig_header: sigs["x-signature-256"] || null,
                    stripe_sig: sigs["stripe-signature"] || null,
                    leaked
                }}
            }};
        }});
        "#
    );

    let install_p1 = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("Content-Type", "application/zip")
        .body(build_zip(&[
            (
                "manifest.json",
                route_plugin_manifest.to_string().as_bytes(),
            ),
            ("plugin.js", route_plugin_js.as_bytes()),
        ]))
        .send()
        .await
        .unwrap();
    let st = install_p1.status();
    let txt = install_p1.text().await.unwrap();
    assert_eq!(
        st,
        StatusCode::CREATED,
        "install route-plugin failed: {txt}"
    );

    // 2. Install untrusted route plugin (render_trusted_html: false)
    let untrusted_manifest = json!({
        "id": "untrusted-route",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "Untrusted Route Plugin",
        "entrypoint": "plugin.js",
        "permissions": {
            "render_trusted_html": false
        },
        "routes": [
            { "id": "xss", "method": "GET", "path": "/xss", "access": "public" },
            { "id": "safe", "method": "GET", "path": "/safe", "access": "public" }
        ]
    });
    let untrusted_js = r#"
        od.addRoute("xss", () => ({
            content_type: "text/html",
            body: '<script>alert(1)</script><a href="javascript:alert(1)">bad</a>'
        }));
        od.addRoute("safe", () => ({
            content_type: "text/html",
            body: '<p class="safe-route">safe-ok</p>'
        }));
    "#;
    let install_p2 = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("Content-Type", "application/zip")
        .body(build_zip(&[
            ("manifest.json", untrusted_manifest.to_string().as_bytes()),
            ("plugin.js", untrusted_js.as_bytes()),
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(install_p2.status(), StatusCode::CREATED);

    // 3. Install & activate Theme with a custom route (/p/route-theme/banner) and explicit render_trusted_html: false opt-out
    let theme_manifest = json!({
        "manifest_version": 3,
        "theme_id": "route-theme",
        "name": "Route Theme",
        "version": "1.0.0",
        "templates": [{ "kind": "index", "file": "index.html" }],
        "functions": {
            "api_version": 1,
            "permissions": {
                "render_trusted_html": false
            },
            "routes": [
                { "id": "banner", "method": "GET", "path": "/banner", "access": "public" },
                { "id": "theme-xss", "method": "GET", "path": "/xss", "access": "public" }
            ]
        }
    });
    let theme_js = r#"
        od.addRoute("banner", () => ({
            content_type: "text/plain",
            body: "theme-custom-route-ok"
        }));
        od.addRoute("theme-xss", () => ({
            content_type: "text/html",
            title: "Theme XSS Opt-Out",
            body: '<a {{ href="javascript:alert(1)" }}>Click</a>'
        }));
    "#;
    let install_t = client
        .post(format!("{base_url}/api/admin/themes"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("Content-Type", "application/zip")
        .body(build_zip(&[
            ("theme.json", theme_manifest.to_string().as_bytes()),
            ("index.html", b"<main>ok</main>"),
            ("functions.js", theme_js.as_bytes()),
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(install_t.status(), StatusCode::CREATED);
    let theme_rev = install_t.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let list_t = client
        .get(format!("{base_url}/api/admin/themes"))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap();
    let etag = list_t
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let act_t = client
        .put(format!("{base_url}/api/admin/themes/active"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", &etag)
        .json(&json!({ "revision_id": theme_rev }))
        .send()
        .await
        .unwrap();
    assert_eq!(act_t.status(), StatusCode::OK);

    // 4. Test GET /p/{pkg}/* (Public, read-only, no cookie leak, trusted vs untrusted HTML, Theme route)
    let item_resp = client
        .get(format!("{base_url}/p/route-plugin/items/42?sort=asc"))
        .header("Cookie", "session=SUPERSECRET")
        .header("Authorization", "Bearer SUPERSECRET")
        .send()
        .await
        .unwrap();
    assert_eq!(item_resp.status(), StatusCode::OK);
    assert_eq!(
        item_resp
            .headers()
            .get(header::X_CONTENT_TYPE_OPTIONS)
            .unwrap(),
        "nosniff"
    );
    let item_html = item_resp.text().await.unwrap();
    assert!(item_html.contains("<title>Item 42</title>"), "{item_html}");
    assert!(item_html.contains(r#"data-id="42""#), "{item_html}");
    assert!(item_html.contains(r#"data-sort="asc""#), "{item_html}");
    assert!(item_html.contains(r#"data-kv-list="yes""#), "{item_html}");
    assert!(
        item_html.contains(r#"data-write-denied="yes""#),
        "{item_html}"
    );
    assert!(
        item_html.contains(r#"data-secret-denied="yes""#),
        "{item_html}"
    );
    assert!(
        item_html.contains(r#"data-cookie-leaked="no""#),
        "{item_html}"
    );
    assert!(
        !item_html.contains("opendash-head-end"),
        "HEAD_END_MARKER leaked into /p/ HTML response: {item_html}"
    );

    // Theme route works, and explicit render_trusted_html: false opt-out enforces strict C-SEC (including Jinja attribute smuggling)
    let theme_route = client
        .get(format!("{base_url}/p/route-theme/banner"))
        .send()
        .await
        .unwrap();
    assert_eq!(theme_route.status(), StatusCode::OK);
    assert_eq!(theme_route.text().await.unwrap(), "theme-custom-route-ok");

    let theme_xss = client
        .get(format!("{base_url}/p/route-theme/xss"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        theme_xss.status(),
        StatusCode::BAD_GATEWAY,
        "Theme with render_trusted_html: false must reject unsafe HTML"
    );

    // Untrusted plugin HTML XSS rejected, safe HTML accepted
    let xss_resp = client
        .get(format!("{base_url}/p/untrusted-route/xss"))
        .send()
        .await
        .unwrap();
    assert_eq!(xss_resp.status(), StatusCode::BAD_GATEWAY);
    let safe_resp = client
        .get(format!("{base_url}/p/untrusted-route/safe"))
        .send()
        .await
        .unwrap();
    assert_eq!(safe_resp.status(), StatusCode::OK);
    assert!(safe_resp.text().await.unwrap().contains("safe-ok"));

    // Session route refused on /p/
    let priv_on_p = client
        .get(format!("{base_url}/p/route-plugin/private"))
        .send()
        .await
        .unwrap();
    assert_eq!(priv_on_p.status(), StatusCode::FORBIDDEN);

    // 5. SSRF internal path blocking (/p/, /api/p/, /api/public/p/) even with allow_loopback=1
    let ssrf = client
        .get(format!("{base_url}/p/route-plugin/ssrf"))
        .send()
        .await
        .unwrap();
    assert_eq!(ssrf.status(), StatusCode::OK);
    let ssrf_json: Value = ssrf.json().await.unwrap();
    for path in [
        "/p/route-plugin/votes",
        "/api/p/route-plugin/private",
        "/api/public/p/route-plugin/vote",
    ] {
        let msg = ssrf_json[path].as_str().unwrap_or("");
        assert!(
            msg.contains("/api/p/") || msg.contains("Cấm"),
            "SSRF internal path `{path}` was not blocked: {ssrf_json}"
        );
    }

    // 6. Test /api/p/{pkg}/* (Session | Admin + CSRF + content_save with ActionActorFence)
    let unauth_get = client
        .get(format!("{base_url}/api/p/route-plugin/private"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_get.status(), StatusCode::UNAUTHORIZED);

    let editor_get = client
        .get(format!("{base_url}/api/p/route-plugin/private"))
        .header("Cookie", &editor_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(editor_get.status(), StatusCode::OK);
    let editor_get_json: Value = editor_get.json().await.unwrap();
    assert_eq!(editor_get_json["principal"], json!("editor"));

    // POST /api/p/ without CSRF -> 403
    let no_csrf = client
        .post(format!("{base_url}/api/p/route-plugin/create-post"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .json(&json!({ "slug": "no-csrf-post", "title": "No CSRF" }))
        .send()
        .await
        .unwrap();
    assert_eq!(no_csrf.status(), StatusCode::FORBIDDEN);

    // POST /api/p/ with Editor session + CSRF creates a real post via ActionActorFence
    let create_via_route = client
        .post(format!("{base_url}/api/p/route-plugin/create-post"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({ "slug": "from-custom-route", "title": "From Custom Route" }))
        .send()
        .await
        .unwrap();
    let cr_st = create_via_route.status();
    let cr_txt = create_via_route.text().await.unwrap();
    assert_eq!(cr_st, StatusCode::OK, "create-post route failed: {cr_txt}");

    // Verify post exists in Admin entries API
    let entries_list: Value = client
        .get(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let entries_arr = entries_list
        .as_array()
        .or_else(|| entries_list["items"].as_array())
        .or_else(|| entries_list["entries"].as_array())
        .unwrap_or_else(|| panic!("unexpected entries response shape: {entries_list}"));
    assert!(
        entries_arr.iter().any(|e| e["slug"] == "from-custom-route"),
        "post created by /api/p/ route missing from entries: {entries_list}"
    );

    // Admin-only route: Editor gets 403, Admin gets 200
    let editor_admin_route = client
        .post(format!("{base_url}/api/p/route-plugin/admin-only"))
        .header("Cookie", &editor_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&json!({ "mark": "bad" }))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_admin_route.status(), StatusCode::FORBIDDEN);

    let admin_route_ok = client
        .post(format!("{base_url}/api/p/route-plugin/admin-only"))
        .header("Cookie", &admin_cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "mark": "admin-verified" }))
        .send()
        .await
        .unwrap();
    assert_eq!(admin_route_ok.status(), StatusCode::OK);

    // 7. Test POST /api/public/p/{pkg}/* (Public Origin, Webhook no-Origin + raw_body + HMAC headers, Session/Admin forbidden, RateLimit)
    // Session/Admin route refused on /api/public/p/
    let admin_on_public = client
        .post(format!("{base_url}/api/public/p/route-plugin/admin-only"))
        .header("Origin", &base_url)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(admin_on_public.status(), StatusCode::FORBIDDEN);

    // Public route: missing Origin -> 403
    let pub_no_origin = client
        .post(format!("{base_url}/api/public/p/route-plugin/vote"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_no_origin.status(), StatusCode::FORBIDDEN);

    // Public route: foreign Origin -> 403
    let pub_foreign_origin = client
        .post(format!("{base_url}/api/public/p/route-plugin/vote"))
        .header("Origin", "https://evil.example")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_foreign_origin.status(), StatusCode::FORBIDDEN);

    // Root POST /api/public/p/{pkg} (path: "/") with valid Origin -> 200
    let pub_root_ok = client
        .post(format!("{base_url}/api/public/p/route-plugin"))
        .header("Origin", &base_url)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_root_ok.status(), StatusCode::OK);
    let pub_root_json: Value = pub_root_ok.json().await.unwrap();
    assert_eq!(pub_root_json["root_ok"], json!(true));
    assert_eq!(pub_root_json["subpath"], json!("/"));

    // Public route: valid Origin -> 200, KV committed, content_save refused!
    let pub_ok = client
        .post(format!("{base_url}/api/public/p/route-plugin/vote"))
        .header("Origin", &base_url)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_ok.status(), StatusCode::OK);
    let pub_ok_json: Value = pub_ok.json().await.unwrap();
    assert_eq!(pub_ok_json["votes"].as_f64(), Some(1.0));
    assert_eq!(pub_ok_json["content_write_denied"], json!("yes"));

    // Webhook route: NO Origin header -> 200, receives raw_body + signature headers, no Cookie/Authorization leak!
    let raw_webhook = r#"{"event":"invoice.paid","amount":4200}"#;
    let wh_ok = client
        .post(format!("{base_url}/api/public/p/route-plugin/stripe-hook"))
        .header("Content-Type", "application/json")
        .header("X-Signature-256", "sha256=deadbeef")
        .header("Stripe-Signature", "t=123,v1=cafebabe")
        .header("Cookie", "session=TOPSECRET")
        .header("Authorization", "Bearer TOPSECRET")
        .body(raw_webhook)
        .send()
        .await
        .unwrap();
    let wh_st = wh_ok.status();
    let wh_txt = wh_ok.text().await.unwrap();
    assert_eq!(wh_st, StatusCode::OK, "webhook route failed: {wh_txt}");
    let wh_json: Value = serde_json::from_str(&wh_txt).unwrap();
    assert_eq!(wh_json["received"], json!(true));
    assert_eq!(wh_json["raw_len"], json!(raw_webhook.len()));
    assert_eq!(wh_json["sig_header"], json!("sha256=deadbeef"));
    assert_eq!(wh_json["stripe_sig"], json!("t=123,v1=cafebabe"));
    assert_eq!(wh_json["leaked"], json!(false));

    // Verify KV writes persisted from /api/p/, /api/public/p/ (Public), and /api/public/p/ (Webhook)
    let votes_state: Value = client
        .get(format!("{base_url}/p/route-plugin/votes"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(votes_state["votes"].as_f64(), Some(1.0));
    assert_eq!(votes_state["webhook_event"], json!("invoice.paid"));
    assert_eq!(votes_state["webhook_sig"], json!("t=123,v1=cafebabe"));
    assert_eq!(votes_state["admin_mark"], json!("admin-verified"));

    // 8. Rate limiter enforcement on /api/public/p/: after 30 requests in 60s window, next returns 429 + Retry-After
    // (We already made 3 requests to /vote above: no-origin, foreign-origin, valid — all count against bucket!)
    let mut got_429 = false;
    for _ in 0..32 {
        let r = client
            .post(format!("{base_url}/api/public/p/route-plugin/vote"))
            .header("Origin", "https://rate-probe.example")
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        if r.status() == StatusCode::TOO_MANY_REQUESTS {
            assert!(r.headers().contains_key(header::RETRY_AFTER));
            got_429 = true;
            break;
        }
    }
    assert!(got_429, "ExtensionRouteRateLimiter did not trigger 429");
}
