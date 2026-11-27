//! Slice S02 E2E & regression tests for Spec B:
//! - `{{ od_hook("slot", args?) }}` C-SEC validation (body allowed; script/style/URL/set/invalid rejected)
//! - Active Theme (`functions.js`) & default Plugin (`render_trusted_html: true`) trusted render hooks
//!   (`filter:render/head`, `filter:render/slot`, `filter:render/footer`)
//! - Opt-out untrusted Plugin (`render_trusted_html: false`) strict HTML sanitization & stripping of
//!   `styles` / `head_scripts` and XSS vectors (`javascript:`, `on*`, `style=url(...)`, `<script>`, `formaction`)
//! - Core SEO & IconNonce inviolability (`<title>`, `<svg><title>`, `robots`, `http-equiv`, `canonical`,
//!   `application/ld+json`, `od_icon(...)`)
//! - `ExtensionRenderCache` + `SiteCacheEpoch` invalidation on Admin mutations, tracking-param canonicalization,
//!   404 route collapse, `FilterRender` policy (`kv_list` allowed, `kv_set`/`secret.get` denied, `http_request`
//!   denied when `render_cache: "none"`), fail-open on timeout/loop-limit without caching failures,
//!   and Safe Mode nonce clearing with zero hook execution.

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

async fn spawn_server(site_root: &Path, port: u16) -> ServerHandle {
    let exe = env!("CARGO_BIN_EXE_opendash");
    let base_url = format!("http://127.0.0.1:{port}");
    let log_path = site_root.join(format!("server_{port}.log"));
    let stderr = fs::File::create(&log_path).unwrap();
    let child = Command::new(exe)
        .env("OPENDASH_SITE_ROOT", site_root)
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("spawn opendash server");

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
    login(client, base_url).await
}

async fn login(client: &Client, base_url: &str) -> (String, String) {
    let login = client
        .post(format!("{base_url}/auth/login"))
        .header("Origin", base_url)
        .json(&json!({
            "username": "admin",
            "password": "AdminPassword123!"
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

fn extract_attr_value(html: &str, attr_prefix: &str) -> Option<String> {
    let start = html.find(attr_prefix)? + attr_prefix.len();
    let rest = &html[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

#[test]
fn s02_csec_od_hook_validation_in_cli_check_theme() {
    let temp = TempDir::new("s02_csec_od_hook");
    let theme_dir = temp.path.join("theme-hook-check");
    fs::create_dir_all(&theme_dir).unwrap();
    let manifest = json!({
        "manifest_version": 3,
        "theme_id": "hook-check-theme",
        "name": "Hook Check Theme",
        "version": "1.0.0",
        "templates": [{ "kind": "index", "file": "index.html" }]
    });
    fs::write(theme_dir.join("theme.json"), manifest.to_string()).unwrap();

    // Valid body invocations (with and without args object)
    fs::write(
        theme_dir.join("index.html"),
        r#"<main>{{ od_hook("hero_slot") }}{{ od_hook("promo_card-1", {"variant": "compact"}) }}</main>"#,
    )
    .unwrap();
    let (ok, out) = run_cli_check("--check-theme", &theme_dir);
    assert!(ok, "valid od_hook template rejected: {out}");

    // Invalid contexts MUST all fail C-SEC validation
    let invalid_templates = [
        ("in_script", r#"<script>{{ od_hook("bad") }}</script>"#),
        ("in_style", r#"<style>{{ od_hook("bad") }}</style>"#),
        ("in_url_attr", r#"<a href="{{ od_hook('bad') }}">link</a>"#),
        ("in_attr", r#"<div data-x="{{ od_hook('bad') }}">hi</div>"#),
        (
            "in_set_block",
            r#"{% set captured %}{{ od_hook("bad") }}{% endset %}<main>ok</main>"#,
        ),
        (
            "invalid_slot_name",
            r#"<main>{{ od_hook("Bad Slot!") }}</main>"#,
        ),
        (
            "spoofed_hook_nonce",
            r#"<main>OD_HOOK_0123456789abcdef0123456789abcdef</main>"#,
        ),
    ];
    for (label, tpl) in invalid_templates {
        fs::write(theme_dir.join("index.html"), tpl).unwrap();
        let (ok, out) = run_cli_check("--check-theme", &theme_dir);
        assert!(
            !ok,
            "invalid od_hook case `{label}` was unexpectedly accepted: {out}"
        );
    }
}

#[tokio::test]
async fn s02_render_hooks_trust_seo_protection_cache_invalidation_and_safe_mode() {
    let temp = TempDir::new("s02_render_hooks_e2e");
    let port = free_port();
    let server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;

    // 1. Install Trusted Plugin A (default render_trusted_html: true, render_cache: by_route)
    let plugin_a_manifest = json!({
        "id": "trusted-render-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "Trusted Render Plugin",
        "entrypoint": "plugin.js",
        "filters": ["filter:render/slot", "filter:render/footer"],
        "permissions": {
            "sqlite": ["kv_read", "kv_write"],
            "content_read_all_public": true
        }
    });
    let plugin_a_js = r#"
        od.addFilter("filter:render/slot", (env) => {
            if (env.payload.slot === "hero_slot") {
                return {
                    html: '<div class="plugin-trusted-slot" onclick="void(0)">trusted-plugin-ok</div>'
                };
            }
            return { html: "" };
        });
        od.addFilter("filter:render/footer", (env) => {
            return {
                html: '<div class="plugin-trusted-footer">plugin-footer-ok</div>'
            };
        });
    "#;
    let resp = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(build_zip(&[
            ("manifest.json", plugin_a_manifest.to_string().as_bytes()),
            ("plugin.js", plugin_a_js.as_bytes()),
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "install plugin A");

    // 2. Install Opt-out Untrusted Plugin B (render_trusted_html: false, render_cache: none)
    let plugin_b_manifest = json!({
        "id": "untrusted-render-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "Untrusted Render Plugin",
        "entrypoint": "plugin.js",
        "render_cache": "none",
        "filters": ["filter:render/head", "filter:render/slot"],
        "permissions": {
            "sqlite": ["kv_read"],
            "http": ["https://example.com/"],
            "render_trusted_html": false
        }
    });
    let plugin_b_js = r#"
        od.addFilter("filter:render/head", (env) => {
            return {
                html: '<meta name="untrusted-head" content="blocked">',
                styles: ['body { display: none; }'],
                head_scripts: ['window.__untrustedHead = 1;']
            };
        });
        od.addFilter("filter:render/slot", (env) => {
            const slot = env.payload.slot;
            if (slot === "xss_slot") {
                return {
                    html: '<a href="javascript:alert(1)">xss-link</a><img src="x" onerror="alert(1)"><div style="background:url(https://evil.example/a)">bg</div><script>alert(1)</script><form action="/ok"><button formaction="javascript:alert(1)">btn</button></form>',
                    styles: ['body { color: purple; }'],
                    head_scripts: ['alert("head-xss");']
                };
            }
            if (slot === "safe_slot") {
                let httpDenied = "no";
                try {
                    od.http.request({ method: "GET", url: "https://example.com/" });
                } catch (e) {
                    httpDenied = "yes";
                }
                return {
                    html: '<p class="safe-untrusted" data-http-denied="' + httpDenied + '">hello-untrusted</p>'
                };
            }
            return { html: "" };
        });
    "#;
    let resp = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(build_zip(&[
            ("manifest.json", plugin_b_manifest.to_string().as_bytes()),
            ("plugin.js", plugin_b_js.as_bytes()),
        ]))
        .send()
        .await
        .unwrap();
    let b_st = resp.status();
    let b_txt = resp.text().await.unwrap();
    assert_eq!(b_st, StatusCode::CREATED, "install plugin B: {b_txt}");

    // 3. Install Timeout/Fail-Open Plugin C
    let plugin_c_manifest = json!({
        "id": "timeout-render-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "Timeout Render Plugin",
        "entrypoint": "plugin.js",
        "filters": ["filter:render/slot"]
    });
    let plugin_c_js = r#"
        od.addFilter("filter:render/slot", (env) => {
            if (env.payload.slot === "timeout_slot") {
                while (true) {}
            }
            return { html: "" };
        });
    "#;
    let resp = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(build_zip(&[
            ("manifest.json", plugin_c_manifest.to_string().as_bytes()),
            ("plugin.js", plugin_c_js.as_bytes()),
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "install plugin C");

    // 4. Install & Activate Theme with functions.js subscribing to all 3 render filters
    let theme_manifest = json!({
        "manifest_version": 3,
        "theme_id": "render-hooks-theme",
        "name": "Render Hooks Theme",
        "version": "1.0.0",
        "templates": [
            { "kind": "index", "file": "index.html" },
            { "kind": "single", "file": "single.html" },
            { "kind": "not_found", "file": "404.html" }
        ],
        "functions": {
            "api_version": 1,
            "render_cache": "by_route",
            "filters": ["filter:render/head", "filter:render/slot", "filter:render/footer"]
        }
    });
    let theme_js = r#"
        od.addFilter("filter:render/head", (env) => {
            return {
                html: '<meta name="x-theme-hook" content="ok">'
                    + '<title>HijackThemeTitle</title>'
                    + '<svg><title>SvgHijackTitle</title></svg>'
                    + '<meta name="robots" content="index,follow">'
                    + '<meta http-equiv="refresh" content="0;url=https://evil.example">'
                    + '<link rel="canonical" href="https://evil.example/canonical">'
                    + '<link rel="alternate" hreflang="fr" href="https://evil.example/fr">'
                    + '<script type="application/ld+json">{"@type":"FakeLd"}</script>',
                styles: ['.theme-dynamic-css { color: crimson; }'],
                head_scripts: ['window.__themeHeadReady = true;']
            };
        });

        od.addFilter("filter:render/slot", (env) => {
            const slot = env.payload.slot;
            if (slot === "hero_slot") {
                const kvList = od.kv.list("", 10);
                let writeDenied = "no";
                try { od.kv.set("should_fail", "1"); } catch (e) { writeDenied = "yes"; }
                let secretDenied = "no";
                try { od.secret.get("any_key"); } catch (e) { secretDenied = "yes"; }
                const token = String(Date.now()) + "-" + String(Math.random()).slice(2, 10);
                const variant = (env.payload.args && env.payload.args.variant) || "none";
                return {
                    html: '<section class="theme-hero" data-cache-token="' + token
                        + '" data-variant="' + od.escapeAttr(variant)
                        + '" data-kv-list-ok="' + (Array.isArray(kvList.items) ? "yes" : "no")
                        + '" data-write-denied="' + writeDenied
                        + '" data-secret-denied="' + secretDenied
                        + '"><svg class="theme-custom-svg"><circle r="4"></circle></svg>'
                        + '<script>window.__themeSlotScript = 1;</script></section>'
                };
            }
            if (slot === "not_found_slot") {
                const token = String(Date.now()) + "-" + String(Math.random()).slice(2, 10);
                return {
                    html: '<div class="nf-marker" data-nf-token="' + token + '">not-found-hook</div>'
                };
            }
            return { html: "" };
        });

        od.addFilter("filter:render/footer", (env) => {
            return {
                html: '<footer class="theme-hook-footer">theme-footer-ok</footer>'
            };
        });
    "#;
    let single_html = r#"<main>
        {{ od_content() }}
        <span class="core-icon">{{ od_icon("check", "w-4") }}</span>
        {{ od_hook("hero_slot", {"variant": "spotlight"}) }}
        {{ od_hook("xss_slot") }}
        {{ od_hook("safe_slot") }}
        {{ od_hook("timeout_slot") }}
    </main>"#;
    let not_found_html = r#"<main>{{ od_hook("not_found_slot") }}</main>"#;
    let theme_zip = build_zip(&[
        ("theme.json", theme_manifest.to_string().as_bytes()),
        ("index.html", b"<main>index-ok</main>"),
        ("single.html", single_html.as_bytes()),
        ("404.html", not_found_html.as_bytes()),
        ("functions.js", theme_js.as_bytes()),
    ]);
    let install_theme = client
        .post(format!("{base_url}/api/admin/themes"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(theme_zip)
        .send()
        .await
        .unwrap();
    let t_st = install_theme.status();
    let t_txt = install_theme.text().await.unwrap();
    assert_eq!(t_st, StatusCode::CREATED, "install theme failed: {t_txt}");
    let theme_rev = serde_json::from_str::<Value>(&t_txt).unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let list_themes = client
        .get(format!("{base_url}/api/admin/themes"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    let etag = list_themes
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let act = client
        .put(format!("{base_url}/api/admin/themes/active"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", &etag)
        .json(&json!({ "revision_id": theme_rev }))
        .send()
        .await
        .unwrap();
    assert_eq!(act.status(), StatusCode::OK, "activate theme failed");

    // Create & publish a post so single.html renders on GET /render-hook-post
    let create_main_post = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "render-hook-post",
            "title": "Render Hook Post",
            "values": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_main_post.status(), StatusCode::CREATED);
    let main_post_id = create_main_post.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    let pub_main = client
        .post(format!(
            "{base_url}/api/admin/content-types/post/entries/{main_post_id}/publication"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_main.status(), StatusCode::OK);

    // 5. First GET /render-hook-post -> verifies all hooks, trust levels, SEO protection, IconNonce, FilterRender policy
    let resp1 = client
        .get(format!("{base_url}/render-hook-post"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let html1 = resp1.text().await.unwrap();

    // No leaked nonces
    assert!(
        !html1.contains("OD_HOOK_"),
        "leaked OD_HOOK_ nonce: {html1}"
    );
    assert!(
        !html1.contains("OD_ICON_"),
        "leaked OD_ICON_ nonce: {html1}"
    );

    // Core IconNonce still rendered real SVG
    assert!(
        html1.contains(r#"<span class="core-icon"><svg"#),
        "Core od_icon SVG broken: {html1}"
    );

    // Trusted Theme head styles, head scripts, allowed meta, and footer rendered
    assert!(html1.contains(r#"<meta name="x-theme-hook" content="ok">"#));
    assert!(html1.contains(".theme-dynamic-css { color: crimson; }"));
    assert!(html1.contains("window.__themeHeadReady = true;"));
    assert!(html1.contains(r#"<footer class="theme-hook-footer">theme-footer-ok</footer>"#));

    // Core SEO inviolability: all forbidden head tags stripped!
    for forbidden in [
        "HijackThemeTitle",
        "SvgHijackTitle",
        "evil.example",
        "http-equiv=\"refresh\"",
        "FakeLd",
    ] {
        assert!(
            !html1.contains(forbidden),
            "Core SEO guard failed to strip `{forbidden}`: {html1}"
        );
    }

    // Trusted Theme slot + Trusted Plugin A slot & footer + FilterRender permissions
    assert!(html1.contains(r#"data-variant="spotlight""#));
    assert!(html1.contains(r#"data-kv-list-ok="yes""#));
    assert!(html1.contains(r#"data-write-denied="yes""#));
    assert!(html1.contains(r#"data-secret-denied="yes""#));
    assert!(html1.contains(r#"<svg class="theme-custom-svg"><circle r="4"></circle></svg>"#));
    assert!(html1.contains("window.__themeSlotScript = 1;"));
    assert!(html1.contains(r#"class="plugin-trusted-slot" onclick="void(0)""#));
    assert!(html1.contains(r#"class="plugin-trusted-footer">plugin-footer-ok"#));

    // Opt-out Untrusted Plugin B: XSS rejected, styles/head_scripts stripped, safe HTML & http denial verified
    assert!(!html1.contains("javascript:alert(1)"));
    assert!(!html1.contains("onerror=\"alert(1)\""));
    assert!(!html1.contains("display: none"));
    assert!(!html1.contains("__untrustedHead"));
    assert!(!html1.contains("head-xss"));
    assert!(!html1.contains("untrusted-head"));
    assert!(
        html1.contains(r#"<p class="safe-untrusted" data-http-denied="yes">hello-untrusted</p>"#),
        "safe untrusted HTML or render_cache=none http_request denial missing: {html1}"
    );

    // 6. Cache hit + tracking-param canonicalization
    let token1 = extract_attr_value(&html1, r#"data-cache-token=""#).expect("token1 in html1");
    let html2 = client
        .get(format!(
            "{base_url}/render-hook-post?utm_source=facebook&fbclid=abc123"
        ))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let token2 = extract_attr_value(&html2, r#"data-cache-token=""#).expect("token2 in html2");
    assert_eq!(
        token1, token2,
        "second GET with tracking params must hit ExtensionRenderCache"
    );

    // 7. Admin mutation bumps SiteCacheEpoch -> cache invalidates immediately!
    let create_post = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "epoch-bump-post",
            "title": "Bump Epoch",
            "values": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_post.status(), StatusCode::CREATED);

    let html3 = client
        .get(format!("{base_url}/render-hook-post"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let token3 = extract_attr_value(&html3, r#"data-cache-token=""#).expect("token3 in html3");
    assert_ne!(
        token1, token3,
        "Admin write mutation must bump SiteCacheEpoch and invalidate ExtensionRenderCache"
    );

    // 8. 404 route collapse: two random missing URLs share the same cached "not_found" key
    let nf_a = client
        .get(format!("{base_url}/random-missing-alpha-111"))
        .send()
        .await
        .unwrap();
    assert_eq!(nf_a.status(), StatusCode::NOT_FOUND);
    let nf_a_html = nf_a.text().await.unwrap();
    let nf_token_a = extract_attr_value(&nf_a_html, r#"data-nf-token=""#).expect("nf_token_a");

    let nf_b = client
        .get(format!("{base_url}/random-missing-beta-222"))
        .send()
        .await
        .unwrap();
    assert_eq!(nf_b.status(), StatusCode::NOT_FOUND);
    let nf_b_html = nf_b.text().await.unwrap();
    let nf_token_b = extract_attr_value(&nf_b_html, r#"data-nf-token=""#).expect("nf_token_b");
    assert_eq!(
        nf_token_a, nf_token_b,
        "404 responses must collapse onto shared not_found cache key"
    );

    // 9. Safe Mode: zero hook execution + OD_HOOK_* nonces stripped cleanly!
    drop(server);
    let cfg_dir = temp.path.join("config");
    fs::create_dir_all(&cfg_dir).unwrap();
    let safe_port = free_port();
    fs::write(
        cfg_dir.join("site.json"),
        json!({
            "version": 1,
            "listen_ip": "127.0.0.1",
            "port": safe_port,
            "trusted_public_origin": null,
            "log_level": "info",
            "recovery_safe_mode": true
        })
        .to_string(),
    )
    .unwrap();
    let _safe_server = spawn_server(&temp.path, safe_port).await;
    let safe_base = format!("http://127.0.0.1:{safe_port}");
    let safe_resp = client
        .get(format!("{safe_base}/render-hook-post"))
        .send()
        .await
        .unwrap();
    assert_eq!(safe_resp.status(), StatusCode::OK);
    let safe_html = safe_resp.text().await.unwrap();
    assert!(!safe_html.contains("OD_HOOK_"), "Safe Mode leaked OD_HOOK_");
    assert!(!safe_html.contains("OD_ICON_"), "Safe Mode leaked OD_ICON_");
    assert!(
        !safe_html.contains("theme-hero"),
        "Safe Mode must not execute render hooks"
    );
    assert!(
        !safe_html.contains("plugin-trusted-slot"),
        "Safe Mode must not execute plugin render hooks"
    );
    assert!(
        safe_html.contains(r#"<span class="core-icon"><svg"#),
        "Core od_icon still works in Safe Mode"
    );
}
