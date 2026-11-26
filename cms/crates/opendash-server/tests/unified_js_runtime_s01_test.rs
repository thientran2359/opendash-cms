//! Slice S01 E2E & regression tests for Spec B:
//! - Unified `boa_engine` JS worker (`plugin.js` & `functions.js`) with 0 build step
//! - Static JS syntax validation in `--check-theme` (`has_functions`) & `--check-plugin`
//! - Storage isolation (`owner_kind`) for `plugin_kv`, `plugin_logs`, `plugin_entry_projections`
//! - Active Theme default permissions (public reads + KV write; sensitive writes denied)
//! - SEO precedence (`Admin > Plugin > Theme`) and `entry.theme_ext` vs `entry.ext`
//! - Top-level host call rejection & Boa `RuntimeLimits` infinite loop/recursion DoS protection
//! - Monotonic theme fencing (`A -> B -> A` cleanup & `selection_version`)
//! - Safe Mode blocks JS/Wasm execution while `PUT /api/admin/themes/active` succeeds

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
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
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

async fn open_readonly_db(site_root: &Path) -> SqlitePool {
    let db_path = site_root.join("database").join("opendash.sqlite");
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    SqlitePool::connect_with(opts).await.unwrap()
}

async fn wait_for_owner_kv(
    db: &SqlitePool,
    owner_kind: &str,
    owner_id: &str,
    key: &str,
    expected: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let row: Option<String> = sqlx::query_scalar(
            "SELECT value_json FROM plugin_kv WHERE owner_kind = ? AND plugin_id = ? AND key = ?",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .bind(key)
        .fetch_optional(db)
        .await
        .unwrap_or(None);
        if row.as_deref() == Some(expected) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let current: Option<String> = sqlx::query_scalar(
        "SELECT value_json FROM plugin_kv WHERE owner_kind = ? AND plugin_id = ? AND key = ?",
    )
    .bind(owner_kind)
    .bind(owner_id)
    .bind(key)
    .fetch_optional(db)
    .await
    .unwrap_or(None);
    panic!(
        "timed out waiting for plugin_kv ({owner_kind}:{owner_id}/{key}) == {expected}, got {current:?}"
    );
}

#[test]
fn s01_cli_check_theme_and_plugin_static_js_validation() {
    let temp = TempDir::new("s01_cli_check");

    // 1. Valid theme with functions.js (top-level throw MUST NOT execute during static check)
    let theme_dir = temp.path.join("theme-valid");
    fs::create_dir_all(&theme_dir).unwrap();
    let theme_manifest = json!({
        "manifest_version": 3,
        "theme_id": "js-cli-theme",
        "name": "JS CLI Theme",
        "version": "1.0.0",
        "templates": [{ "kind": "index", "file": "index.html" }],
        "functions": {
            "api_version": 1,
            "hooks": ["theme/after_activate"],
            "filters": ["filter:entry/view"]
        }
    });
    fs::write(theme_dir.join("theme.json"), theme_manifest.to_string()).unwrap();
    fs::write(theme_dir.join("index.html"), "<main>ok</main>").unwrap();
    fs::write(
        theme_dir.join("functions.js"),
        "throw new Error('must_not_execute_during_check');\nod.on('theme/after_activate', () => ({}));",
    )
    .unwrap();

    let (ok, out) = run_cli_check("--check-theme", &theme_dir);
    assert!(ok, "--check-theme valid JS theme failed: {out}");
    let parsed: Value = serde_json::from_str(out.lines().next().unwrap()).unwrap();
    assert_eq!(parsed["has_functions"], json!(true));

    // 2. Broken JS syntax in functions.js -> reject
    fs::write(theme_dir.join("functions.js"), "function broken( {").unwrap();
    let (ok, out) = run_cli_check("--check-theme", &theme_dir);
    assert!(!ok, "--check-theme accepted broken functions.js: {out}");

    // 3. Asymmetric: functions declared in theme.json without functions.js -> reject
    fs::remove_file(theme_dir.join("functions.js")).unwrap();
    let (ok, out) = run_cli_check("--check-theme", &theme_dir);
    assert!(
        !ok,
        "--check-theme accepted missing functions.js when declared: {out}"
    );

    // 4. Valid plugin.js (top-level throw MUST NOT execute during static check)
    let plugin_dir = temp.path.join("plugin-valid");
    fs::create_dir_all(&plugin_dir).unwrap();
    let plugin_manifest = json!({
        "id": "js-cli-plugin",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "JS CLI Plugin",
        "entrypoint": "plugin.js",
        "hooks": ["content/after_publish"]
    });
    fs::write(
        plugin_dir.join("manifest.json"),
        plugin_manifest.to_string(),
    )
    .unwrap();
    fs::write(
        plugin_dir.join("plugin.js"),
        "throw new Error('must_not_run');\nod.on('plugin/manual_run', (env) => ({ ok: true }));",
    )
    .unwrap();
    let (ok, out) = run_cli_check("--check-plugin", &plugin_dir);
    assert!(ok, "--check-plugin valid plugin.js failed: {out}");

    // 5. Broken JS syntax in plugin.js -> reject
    fs::write(plugin_dir.join("plugin.js"), "const x = ([").unwrap();
    let (ok, out) = run_cli_check("--check-plugin", &plugin_dir);
    assert!(!ok, "--check-plugin accepted broken plugin.js: {out}");

    // 6. Both plugin.js and plugin.wasm simultaneously -> reject
    fs::write(
        plugin_dir.join("plugin.js"),
        "od.on('plugin/manual_run', () => ({}));",
    )
    .unwrap();
    let minimal_wasm = wat::parse_str(r#"(module (memory (export "memory") 1))"#).unwrap();
    fs::write(plugin_dir.join("plugin.wasm"), &minimal_wasm).unwrap();
    let (ok, out) = run_cli_check("--check-plugin", &plugin_dir);
    assert!(
        !ok,
        "--check-plugin accepted simultaneous plugin.js and plugin.wasm: {out}"
    );
}

#[tokio::test]
async fn s01_unified_js_runtime_theme_and_plugin_e2e_isolation_seo_limits_and_safe_mode() {
    let temp = TempDir::new("s01_unified_js_e2e");
    let port = free_port();
    let server = spawn_server(&temp.path, port).await;
    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    let (cookie, csrf) = setup_and_login(&client, &base_url).await;

    // 1. Install JS Plugin with id = "shared-id"
    let plugin_manifest = json!({
        "id": "shared-id",
        "version": "1.0.0",
        "api_version": 1,
        "manifest_schema": 3,
        "min_host_version": "0.1.0",
        "name": "Shared ID JS Plugin",
        "entrypoint": "plugin.js",
        "hooks": ["content/after_publish"],
        "filters": ["filter:content/prepare_save", "filter:seo/head", "filter:entry/view"],
        "actions": [{
            "id": "run-js",
            "title": "Run JS",
            "capabilities": ["kv_read", "kv_list", "kv_write", "kv_delete", "content_read", "site_options_read"]
        }],
        "permissions": {
            "sqlite": ["kv_read", "kv_write"],
            "content_read_all_public": true,
            "content_write": ["post"],
            "site_options_read": true
        }
    });
    let plugin_js = r#"
        od.addFilter("filter:content/prepare_save", (env) => {
            const title = (env.payload && env.payload.title) || "";
            if (!title.startsWith("[JS] ")) {
                return { title: "[JS] " + title };
            }
            return {};
        });

        od.addFilter("filter:seo/head", (env) => {
            return {
                title: "Plugin SEO Title",
                json_ld: [{
                    "@context": "https://schema.org",
                    "@type": "Article",
                    "headline": env.payload.title
                }]
            };
        });

        od.addFilter("filter:entry/view", (env) => {
            return {
                badge: "from-plugin",
                escaped: od.escapeHtml("<b>plugin</b>"),
                attr: od.escapeAttr("\"quoted\"")
            };
        });

        od.on("content/after_publish", (env) => {
            od.kv.set("who", "plugin");
            od.kv.set("published_slug", env.payload.slug || "");
            od.log.info("plugin-after-publish:" + (env.payload.slug || ""));
            return { ok: true };
        });

        od.on("plugin/manual_run", (env) => {
            return {
                echo: env.payload,
                escaped: od.escapeHtml("<tag>"),
                attr: od.escapeAttr("`tick`")
            };
        });

        od.addAction("run-js", (env) => {
            od.kv.set("action_tmp", "hello");
            const got = od.kv.get("action_tmp");
            const missing = od.kv.get("does_not_exist");
            const posts = od.content.list("post", { limit: 5 });
            const opts = od.site.options();
            od.kv.delete("action_tmp");
            const listAfter = od.kv.list("action_", 10);
            od.log.info("plugin-action-ok");
            return {
                got,
                missing,
                post_count: Array.isArray(posts.items) ? posts.items.length : -1,
                has_site_options: typeof opts === "object" && opts !== null,
                list_after_count: Array.isArray(listAfter.items) ? listAfter.items.length : -1
            };
        });
    "#;
    let plugin_zip = build_zip(&[
        ("manifest.json", plugin_manifest.to_string().as_bytes()),
        ("plugin.js", plugin_js.as_bytes()),
    ]);
    let resp = client
        .post(format!("{base_url}/api/admin/plugins"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(plugin_zip)
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let text = resp.text().await.unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "install JS plugin failed: {text}"
    );

    // 2. Install & Activate Theme v3 with SAME ID ("shared-id") and NO explicit permissions
    let theme_a_manifest = json!({
        "manifest_version": 3,
        "theme_id": "shared-id",
        "name": "Shared ID JS Theme A",
        "version": "1.0.0",
        "templates": [
            { "kind": "index", "file": "index.html" },
            { "kind": "single", "file": "single.html" }
        ],
        "functions": {
            "api_version": 1,
            "hooks": ["theme/after_activate", "content/after_publish"],
            "filters": ["filter:seo/head", "filter:entry/view"]
        }
    });
    let theme_a_js = r#"
        od.on("theme/after_activate", (env) => {
            od.kv.set("activated", "yes");
            od.kv.set("who", "theme");
            const posts = od.content.list("post", { limit: 5 });
            od.kv.set("posts_readable", Array.isArray(posts.items) ? "yes" : "no");
            let writeDenied = "no";
            try {
                od.content.save({
                    type_key: "post",
                    slug: "should-fail",
                    title: "Should Fail",
                    values: {}
                });
            } catch (e) {
                writeDenied = "yes";
            }
            od.kv.set("write_denied", writeDenied);
            od.log.info("theme-after-activate:" + env.payload.theme_id);
            return { ok: true };
        });

        od.on("content/after_publish", (env) => {
            od.kv.set("theme_seen_entry", String(env.payload.entry_id || ""));
            od.log.info("theme-after-publish:" + String(env.payload.entry_id || ""));
            return { ok: true };
        });

        od.addFilter("filter:seo/head", (env) => {
            return {
                title: "Theme SEO Title Should Lose To Plugin",
                description: "Theme SEO Desc Wins Fallback"
            };
        });

        od.addFilter("filter:entry/view", (env) => {
            return {
                badge: "from-theme",
                escaped: od.escapeHtml("<i>theme</i>")
            };
        });
    "#;
    let theme_a_zip = build_zip(&[
        ("theme.json", theme_a_manifest.to_string().as_bytes()),
        ("index.html", b"<main>theme-a-index</main>"),
        (
            "single.html",
            br#"<main>{{ od_content() }}title={{ entry.title }}|theme_ext={{ entry.theme_ext.badge | default("none") }}|plugin_ext={{ (entry.ext["shared-id"] | default({})).badge | default("none") }}</main>"#,
        ),
        ("functions.js", theme_a_js.as_bytes()),
    ]);
    let install_theme_a = client
        .post(format!("{base_url}/api/admin/themes"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(theme_a_zip)
        .send()
        .await
        .unwrap();
    let st = install_theme_a.status();
    let txt = install_theme_a.text().await.unwrap();
    assert_eq!(st, StatusCode::CREATED, "install theme A failed: {txt}");
    let theme_a_rev = serde_json::from_str::<Value>(&txt).unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Get current theme selection ETag from GET /api/admin/themes
    let list_themes = client
        .get(format!("{base_url}/api/admin/themes"))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(list_themes.status(), StatusCode::OK);
    let etag = list_themes
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let act_a = client
        .put(format!("{base_url}/api/admin/themes/active"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", &etag)
        .json(&json!({ "revision_id": theme_a_rev }))
        .send()
        .await
        .unwrap();
    let act_a_status = act_a.status();
    let act_a_etag = act_a
        .headers()
        .get(header::ETAG)
        .map(|v| v.to_str().unwrap().to_owned());
    let act_a_body = act_a.text().await.unwrap();
    assert_eq!(
        act_a_status,
        StatusCode::OK,
        "activate theme A failed: {act_a_body}"
    );
    let etag_after_a = act_a_etag.unwrap();

    // Verify theme/after_activate ran & Active Theme default permissions work
    let db = open_readonly_db(&temp.path).await;
    wait_for_owner_kv(&db, "theme", "shared-id", "activated", "\"yes\"").await;
    wait_for_owner_kv(&db, "theme", "shared-id", "who", "\"theme\"").await;
    wait_for_owner_kv(&db, "theme", "shared-id", "posts_readable", "\"yes\"").await;
    wait_for_owner_kv(&db, "theme", "shared-id", "write_denied", "\"yes\"").await;

    // 3. Create & publish post -> filter:content/prepare_save + projections + after_publish
    let create = client
        .post(format!("{base_url}/api/admin/content-types/post/entries"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "js-runtime-post",
            "title": "Hello Boa",
            "values": {}
        }))
        .send()
        .await
        .unwrap();
    let create_st = create.status();
    let create_txt = create.text().await.unwrap();
    assert_eq!(
        create_st,
        StatusCode::CREATED,
        "create post failed: {create_txt}"
    );
    let created: Value = serde_json::from_str(&create_txt).unwrap();
    assert_eq!(created["title"], json!("[JS] Hello Boa"));
    let entry_id = created["id"].as_i64().unwrap();

    let pub_resp = client
        .post(format!(
            "{base_url}/api/admin/content-types/post/entries/{entry_id}/publication"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    let pub_st = pub_resp.status();
    let pub_txt = pub_resp.text().await.unwrap();
    assert_eq!(pub_st, StatusCode::OK, "publish post failed: {pub_txt}");

    // Wait for both plugin and theme after_publish hooks
    wait_for_owner_kv(&db, "plugin", "shared-id", "who", "\"plugin\"").await;
    wait_for_owner_kv(
        &db,
        "theme",
        "shared-id",
        "theme_seen_entry",
        &format!("\"{entry_id}\""),
    )
    .await;

    // Confirm owner_kind isolation in plugin_kv (same ID "shared-id" holds separate values!)
    let theme_who: String = sqlx::query_scalar(
        "SELECT value_json FROM plugin_kv WHERE owner_kind = 'theme' AND plugin_id = 'shared-id' AND key = 'who'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let plugin_who: String = sqlx::query_scalar(
        "SELECT value_json FROM plugin_kv WHERE owner_kind = 'plugin' AND plugin_id = 'shared-id' AND key = 'who'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(theme_who, "\"theme\"");
    assert_eq!(plugin_who, "\"plugin\"");

    // Confirm separate logs in plugin_logs by owner_kind
    let plugin_logs_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM plugin_logs WHERE owner_kind = 'plugin' AND plugin_id = 'shared-id'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let theme_logs_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM plugin_logs WHERE owner_kind = 'theme' AND plugin_id = 'shared-id'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(plugin_logs_count >= 1, "expected plugin logs");
    assert!(theme_logs_count >= 1, "expected theme logs");

    // 4. Public GET /js-runtime-post -> SEO precedence (Plugin > Theme) & theme_ext vs ext & 0 worker spawns
    let logs_before_get: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_logs")
        .fetch_one(&db)
        .await
        .unwrap();
    let page = client
        .get(format!("{base_url}/js-runtime-post"))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let html = page.text().await.unwrap();
    assert!(
        html.contains("<title>Plugin SEO Title</title>") || html.contains("Plugin SEO Title"),
        "Plugin SEO title must win over Theme SEO title; html={html}"
    );
    assert!(
        html.contains("Theme SEO Desc Wins Fallback"),
        "Theme SEO description fallback must apply; html={html}"
    );
    assert!(
        html.contains("theme_ext=from-theme|plugin_ext=from-plugin"),
        "entry.theme_ext and entry.ext must render independently; html={html}"
    );
    let logs_after_get: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plugin_logs")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(
        logs_before_get, logs_after_get,
        "public GET must perform 0 JS/Wasm worker spawns in S01"
    );

    // 5. Execute JS Plugin manual_run & Action (od.kv.*, od.content.list, od.site.options, escape*)
    let manual = client
        .post(format!("{base_url}/api/admin/plugins/shared-id/run"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({ "ping": 42 }))
        .send()
        .await
        .unwrap();
    assert_eq!(manual.status(), StatusCode::OK);
    let manual_json: Value = manual.json().await.unwrap();
    assert_eq!(manual_json["success"], json!(true));
    assert_eq!(manual_json["data"]["echo"]["ping"], json!(42));
    assert_eq!(manual_json["data"]["escaped"], json!("&lt;tag&gt;"));
    assert_eq!(manual_json["data"]["attr"], json!("&#96;tick&#96;"));

    let action = client
        .post(format!(
            "{base_url}/api/admin/plugins/shared-id/actions/run-js"
        ))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let act_st = action.status();
    let act_txt = action.text().await.unwrap();
    assert_eq!(act_st, StatusCode::OK, "JS action failed: {act_txt}");
    let act_json: Value = serde_json::from_str(&act_txt).unwrap();
    assert_eq!(act_json["data"]["got"], json!("hello"));
    assert_eq!(act_json["data"]["missing"], Value::Null);
    assert!(act_json["data"]["post_count"].as_i64().unwrap() >= 1);
    assert_eq!(act_json["data"]["has_site_options"], json!(true));
    assert_eq!(act_json["data"]["list_after_count"], json!(0));

    // 6. Safety tests: top-level host call forbidden, infinite loop, infinite recursion
    for (pid, js_code, expected_code_or_msg) in [
        (
            "js-top-level-io",
            r#"od.kv.set("evil", 1); od.on("plugin/manual_run", () => ({ ok: true }));"#,
            "top_level_host_call_forbidden",
        ),
        (
            "js-inf-loop",
            r#"od.on("plugin/manual_run", () => { while (true) {} });"#,
            "out_of_fuel",
        ),
        (
            "js-inf-recur",
            r#"od.on("plugin/manual_run", () => { function f(n) { return f(n + 1); } return f(0); });"#,
            "out_of_fuel",
        ),
    ] {
        let m = json!({
            "id": pid,
            "version": "1.0.0",
            "api_version": 1,
            "manifest_schema": 3,
            "min_host_version": "0.1.0",
            "name": pid,
            "entrypoint": "plugin.js",
            "permissions": { "sqlite": ["kv_write"] }
        });
        let z = build_zip(&[
            ("manifest.json", m.to_string().as_bytes()),
            ("plugin.js", js_code.as_bytes()),
        ]);
        let r = client
            .post(format!("{base_url}/api/admin/plugins"))
            .header("Cookie", &cookie)
            .header("Origin", &base_url)
            .header("X-CSRF-Token", &csrf)
            .header("Content-Type", "application/zip")
            .body(z)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::CREATED, "install {pid} failed");

        let started = Instant::now();
        let run_resp = client
            .post(format!("{base_url}/api/admin/plugins/{pid}/run"))
            .header("Cookie", &cookie)
            .header("Origin", &base_url)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        let body: Value = run_resp.json().await.unwrap();
        assert_eq!(body["success"], json!(false), "{pid} should fail: {body}");
        let combined = format!(
            "{} {}",
            body["code"].as_str().unwrap_or(""),
            body["error"].as_str().unwrap_or("")
        );
        let loop_debug_timeout_ok = cfg!(debug_assertions)
            && pid == "js-inf-loop"
            && body["code"].as_str() == Some("timeout");
        assert!(
            combined.contains(expected_code_or_msg) || loop_debug_timeout_ok,
            "{pid} expected '{expected_code_or_msg}', got: {body}"
        );
        if !loop_debug_timeout_ok {
            assert!(
                started.elapsed() < Duration::from_secs(4),
                "{pid} took too long (should hit RuntimeLimits before 5s timeout)"
            );
        }
    }

    let evil_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plugin_kv WHERE plugin_id = 'js-top-level-io'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(evil_count, 0, "top-level kv.set must never commit");

    // 7. Monotonic Theme Switch A -> B -> A clears theme projections & increments selection_version
    let theme_b_manifest = json!({
        "manifest_version": 3,
        "theme_id": "theme-b-plain",
        "name": "Theme B Plain",
        "version": "1.0.0",
        "templates": [
            { "kind": "index", "file": "index.html" },
            { "kind": "single", "file": "single.html" }
        ]
    });
    let theme_b_zip = build_zip(&[
        ("theme.json", theme_b_manifest.to_string().as_bytes()),
        ("index.html", b"<main>theme-b-index</main>"),
        (
            "single.html",
            br#"<main>{{ od_content() }}B|theme_ext={{ entry.theme_ext.badge | default("none") }}|plugin_ext={{ (entry.ext["shared-id"] | default({})).badge | default("none") }}</main>"#,
        ),
    ]);
    let install_b = client
        .post(format!("{base_url}/api/admin/themes"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(theme_b_zip)
        .send()
        .await
        .unwrap();
    assert_eq!(install_b.status(), StatusCode::CREATED);
    let theme_b_rev = install_b.json::<Value>().await.unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let sel_ver_a: i64 =
        sqlx::query_scalar("SELECT version FROM theme_active_selection WHERE id = 1")
            .fetch_one(&db)
            .await
            .unwrap();

    let act_b = client
        .put(format!("{base_url}/api/admin/themes/active"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", &etag_after_a)
        .json(&json!({ "revision_id": theme_b_rev }))
        .send()
        .await
        .unwrap();
    assert_eq!(act_b.status(), StatusCode::OK);
    let etag_after_b = act_b
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let theme_proj_after_b: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM plugin_entry_projections WHERE owner_kind = 'theme'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let plugin_proj_after_b: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM plugin_entry_projections WHERE owner_kind = 'plugin'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        theme_proj_after_b, 0,
        "switching to Theme B must purge theme projections"
    );
    assert_eq!(
        plugin_proj_after_b, 1,
        "plugin projections must survive theme switch"
    );

    let page_b = client
        .get(format!("{base_url}/js-runtime-post"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page_b.contains("B|theme_ext=none|plugin_ext=from-plugin"),
        "Theme B must see theme_ext=none and preserve plugin_ext; html={page_b}"
    );

    // Re-activate Theme A (A -> B -> A) and verify monotonic version > sel_ver_a
    let act_a2 = client
        .put(format!("{base_url}/api/admin/themes/active"))
        .header("Cookie", &cookie)
        .header("Origin", &base_url)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", &etag_after_b)
        .json(&json!({ "revision_id": theme_a_rev }))
        .send()
        .await
        .unwrap();
    assert_eq!(act_a2.status(), StatusCode::OK);
    let etag_after_a2 = act_a2
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let sel_ver_a2: i64 =
        sqlx::query_scalar("SELECT version FROM theme_active_selection WHERE id = 1")
            .fetch_one(&db)
            .await
            .unwrap();
    assert!(
        sel_ver_a2 >= sel_ver_a + 2,
        "selection_version must increase monotonically"
    );

    // Wait for batched recompute of Theme A projection
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM plugin_entry_projections WHERE owner_kind = 'theme' AND plugin_id = 'shared-id'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        if count == 1 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Theme A projection recompute timed out"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // 8. Safe Mode: blocks JS/Wasm execution, but allows theme_activate!
    db.close().await;
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
    let (safe_cookie, safe_csrf) = login(&client, &safe_base).await;

    let blocked_action = client
        .post(format!(
            "{safe_base}/api/admin/plugins/shared-id/actions/run-js"
        ))
        .header("Cookie", &safe_cookie)
        .header("Origin", &safe_base)
        .header("X-CSRF-Token", &safe_csrf)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(blocked_action.status(), StatusCode::SERVICE_UNAVAILABLE);
    let blocked_json: Value = blocked_action.json().await.unwrap();
    assert_eq!(blocked_json["code"], json!("safe_mode"));

    // Operator MUST still be able to switch away to Theme B in Safe Mode
    let safe_switch = client
        .put(format!("{safe_base}/api/admin/themes/active"))
        .header("Cookie", &safe_cookie)
        .header("Origin", &safe_base)
        .header("X-CSRF-Token", &safe_csrf)
        .header("If-Match", &etag_after_a2)
        .json(&json!({ "revision_id": theme_b_rev }))
        .send()
        .await
        .unwrap();
    let safe_st = safe_switch.status();
    let safe_txt = safe_switch.text().await.unwrap();
    assert_eq!(
        safe_st,
        StatusCode::OK,
        "theme_activate must succeed in Safe Mode: {safe_txt}"
    );
}
