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
    // (excludes read-only POST previews: entry_builder_preview,
    // template_overrides_preview, menus_preview)
    let required_handlers: &[(&str, &[&str])] = &[
        ("auth.rs", &["auth_setup", "auth_login", "auth_logout"]),
        ("builder.rs", &["entry_builder_put", "entry_builder_reset"]),
        (
            "contact.rs",
            &[
                "admin_create_form",
                "admin_update_form",
                "admin_delete_form",
                "admin_delete_submission",
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
                "content_entry_publication",
            ],
        ),
        (
            "entries.rs",
            &[
                "entry_create",
                "entry_update",
                "entry_delete",
                "entry_publication",
                "entry_composite_create",
                "entry_composite_update",
                "entry_translation_create",
                "entry_table_provider_action",
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
            "patterns.rs",
            &["pattern_create", "pattern_update", "pattern_delete"],
        ),
        (
            "plugin.rs",
            &[
                "plugin_install",
                "plugin_run",
                "plugin_action",
                "plugin_disable",
                "plugin_enable",
                "plugin_put_settings",
            ],
        ),
        (
            "seo.rs",
            &["put_entry_route", "put_entry_seo", "put_home_seo"],
        ),
        (
            "settings.rs",
            &[
                "general_put",
                "public_put",
                "reading_put",
                "site_options_put",
                "theme_options_put",
                "locales_create",
                "locales_update",
                "locales_set_default",
                "locales_multilingual_put",
                "locales_delete",
                "locales_orphans_purge",
            ],
        ),
        (
            "smtp.rs",
            &[
                "put_smtp_settings",
                "test_smtp_settings",
                "notify_submission",
            ],
        ),
        (
            "taxonomy.rs",
            &[
                "taxonomy_definitions_create",
                "taxonomy_definitions_patch",
                "taxonomy_definitions_delete",
                "taxonomy_create",
                "taxonomy_patch",
                "taxonomy_delete",
                "taxonomy_eligibility_put",
            ],
        ),
        (
            "themes.rs",
            &[
                "theme_install",
                "theme_activate",
                "template_overrides_put",
                "template_overrides_reset",
            ],
        ),
        (
            "operations.rs",
            &["upload_restore_archive", "activate_recovery"],
        ),
        (
            "users.rs",
            &[
                "user_create",
                "user_patch",
                "user_reset_password",
                "user_delete",
                "profile_put",
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

    // public_submit uses two-phase coordinator admission so filter:contact/before_submit runs
    // after cheap shields and without holding MutationLease across Wasm/HTTP execution.
    let contact_route_src =
        fs::read_to_string(routes_dir.join("contact.rs")).expect("read contact.rs route");
    let pub_pos = contact_route_src
        .find("pub async fn public_submit(")
        .expect("missing contact.rs::public_submit");
    let pub_sig_end = contact_route_src[pub_pos..]
        .find(") ->")
        .map(|off| pub_pos + off)
        .unwrap_or(contact_route_src.len());
    let pub_sig = &contact_route_src[pub_pos..pub_sig_end];
    assert!(
        pub_sig.contains("State(coordinator)"),
        "contact.rs::public_submit must extract State(coordinator) for admission"
    );
    let pub_body = &contact_route_src[pub_sig_end..];
    assert!(
        pub_body.contains("coordinator.admit_mutation()"),
        "contact.rs::public_submit must run coordinator.admit_mutation() pre-check"
    );
    let contact_src =
        fs::read_to_string(manifest_dir.join("src").join("contact.rs")).expect("read contact.rs");
    assert!(
        contact_src.contains("coordinator.admit_mutation()")
            && contact_src.contains("dispatch_contact_before_submit"),
        "contact::submit must admit SQLite writes via coordinator.admit_mutation() and run dispatch_contact_before_submit without holding a lease"
    );

    // Verify background SMTP spawn requires MutationLease
    let smtp_src =
        fs::read_to_string(manifest_dir.join("src").join("smtp.rs")).expect("read smtp.rs");
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
