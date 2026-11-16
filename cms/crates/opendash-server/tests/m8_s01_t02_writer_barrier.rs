use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use reqwest::{Client, StatusCode};
use serde_json::json;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "opendash-m8-s01-t02-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RunningServer {
    child: Option<Child>,
    base_url: String,
}

impl RunningServer {
    async fn start(site_root: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let bin = env!("CARGO_BIN_EXE_opendash");
        let child = Command::new(bin)
            .env("OPENDASH_SITE_ROOT", site_root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn opendash server");

        let base_url = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..80 {
            if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await {
                if resp.status() == StatusCode::OK {
                    return Self {
                        child: Some(child),
                        base_url,
                    };
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("server failed to become ready at {base_url}");
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn audited_writer_inventory_covers_every_mutating_route_handler() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let routes_dir = manifest_dir.join("src").join("routes");

    // All mutating handler functions wired into build_router_with_themes_root
    // (excludes pure read-only POST preview renders: preview, homepage_preview, override_preview, menus_preview)
    let required_handlers: &[(&str, &[&str])] = &[
        ("auth.rs", &["auth_setup", "auth_login", "auth_logout"]),
        (
            "builder.rs",
            &[
                "install",
                "save",
                "homepage_put",
                "override_put",
                "override_reset",
            ],
        ),
        (
            "contact.rs",
            &[
                "admin_create_form",
                "admin_update_form",
                "admin_delete_form",
                "admin_delete_submission",
                "public_submit",
            ],
        ),
        (
            "content.rs",
            &[
                "content_type_create",
                "content_type_update",
                "field_group_create",
                "field_group_update",
                "field_group_delete",
                "editor_layout_put",
            ],
        ),
        (
            "entries.rs",
            &[
                "entry_create",
                "entry_update",
                "entry_delete",
                "entry_publication",
            ],
        ),
        ("import.rs", &["import_dry_run", "import_execute"]),
        ("media.rs", &["media_upload", "media_video_upload"]),
        (
            "menu.rs",
            &[
                "menus_create",
                "menus_put",
                "menus_delete",
                "menu_slots_put",
            ],
        ),
        (
            "plugin.rs",
            &[
                "plugin_install",
                "plugin_run",
                "plugin_action",
                "plugin_disable",
                "plugin_enable",
            ],
        ),
        (
            "seo.rs",
            &["put_entry_route", "put_entry_seo", "put_home_seo"],
        ),
        (
            "settings.rs",
            &["general_put", "public_put", "theme_options_put"],
        ),
        (
            "smtp.rs",
            &["put_smtp_settings", "test_smtp_settings", "notify_submission"],
        ),
        (
            "taxonomy.rs",
            &["taxonomy_create", "taxonomy_patch", "taxonomy_delete"],
        ),
        ("themes.rs", &["theme_install", "theme_activate"]),
        ("operations.rs", &["upload_restore_archive", "activate_recovery"]),
        (
            "users.rs",
            &[
                "user_create",
                "user_patch",
                "user_reset_password",
                "user_delete",
            ],
        ),
    ];

    for (file, handlers) in required_handlers {
        let src = fs::read_to_string(routes_dir.join(file)).expect("read route file");
        for handler in *handlers {
            let needle = format!("pub async fn {handler}(");
            let pos = src
                .find(&needle)
                .unwrap_or_else(|| panic!("missing handler {handler} in {file}"));
            let sig_end = src[pos..]
                .find(") ->")
                .map(|off| pos + off)
                .unwrap_or(src.len());
            let sig_slice = &src[pos..sig_end];
            assert!(
                sig_slice.contains("AdmittedMutation"),
                "handler {file}::{handler} is missing AdmittedMutation lease extractor"
            );
        }
    }

    // Verify background SMTP spawn requires MutationLease
    let smtp_src = fs::read_to_string(manifest_dir.join("src").join("smtp.rs")).expect("read smtp.rs");
    let spawn_pos = smtp_src
        .find("pub fn spawn_initial_attempt(")
        .expect("spawn_initial_attempt exists");
    let spawn_end = smtp_src[spawn_pos..]
        .find(") {")
        .map(|off| spawn_pos + off)
        .unwrap_or(smtp_src.len());
    let spawn_sig = &smtp_src[spawn_pos..spawn_end];
    assert!(
        spawn_sig.contains("MutationLease"),
        "spawn_initial_attempt must require MutationLease"
    );
}

#[tokio::test]
async fn admitted_mutation_routes_execute_normally_and_preserve_auth_flow() {
    let dir = TempDirGuard::new("e2e-flow");
    let server = RunningServer::start(dir.path()).await;
    let client = Client::new();

    let setup = client
        .post(format!("{}/auth/setup", server.base_url))
        .json(&json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .expect("setup request");
    assert_eq!(setup.status(), StatusCode::CREATED);

    let login = client
        .post(format!("{}/auth/login", server.base_url))
        .json(&json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .expect("login request");
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("set-cookie header")
        .to_str()
        .expect("cookie str")
        .split(';')
        .next()
        .expect("cookie pair")
        .to_string();
    let login_json: serde_json::Value = login.json().await.expect("login json");
    let csrf = login_json["csrf_token"]
        .as_str()
        .expect("csrf_token")
        .to_string();

    let get_settings = client
        .get(format!("{}/api/admin/settings/general", server.base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("get general settings");
    assert_eq!(get_settings.status(), StatusCode::OK);
    let etag = get_settings
        .headers()
        .get(reqwest::header::ETAG)
        .expect("etag header")
        .to_str()
        .expect("etag str")
        .to_string();

    let settings_res = client
        .put(format!("{}/api/admin/settings/general", server.base_url))
        .header(reqwest::header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", &etag)
        .json(&json!({
            "site_title": "M8 Barrier Site",
            "tagline": "",
            "timezone": "UTC"
        }))
        .send()
        .await
        .expect("update general settings");
    assert_eq!(settings_res.status(), StatusCode::OK);
}
