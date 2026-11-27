use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "opendash-m7-s12-t02-{label}-{}-{id}",
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
        for _ in 0..100 {
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

async fn setup_and_login_admin(client: &Client, base: &str) -> (String, String, String) {
    let setup = client
        .post(format!("{base}/auth/setup"))
        .json(&json!({ "username": "admin", "password": "Password123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    login(client, base, "admin", "Password123!").await
}

async fn login(
    client: &Client,
    base: &str,
    username: &str,
    password: &str,
) -> (String, String, String) {
    let resp = client
        .post(format!("{base}/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = resp.json().await.unwrap();
    let site_id = body["site_instance_id"].as_str().unwrap().to_string();
    (
        cookie,
        body["csrf_token"].as_str().unwrap().to_string(),
        site_id,
    )
}

#[tokio::test]
async fn site_instance_id_downgrade_scrub_and_fallback_save() {
    let dir = TempDirGuard::new("single-edit");
    let srv = RunningServer::start(dir.path()).await;
    let client = Client::new();
    let (admin_cookie, admin_csrf, site_id) = setup_and_login_admin(&client, &srv.base_url).await;
    assert!(site_id.starts_with("site-"));

    // Create second Admin user who will be downgraded to Editor (`AC-11`)
    let u2_resp = client
        .post(format!("{}/api/admin/users", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "username": "alice", "password": "AlicePassword123!", "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(u2_resp.status(), StatusCode::CREATED);
    let alice: Value = u2_resp.json().await.unwrap();
    let alice_id = alice["id"].as_i64().unwrap();
    let alice_ver = alice["version"].as_i64().unwrap();

    // Create CPT `portfolio` with admin-only field and editor field
    let cpt = client
        .post(format!("{}/api/admin/content-types", srv.base_url))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "key": "portfolio",
            "label": "Portfolio",
            "fields": [
                { "key": "client", "label": "Client", "type": "text", "required": true },
                {
                    "key": "margin",
                    "label": "Margin",
                    "type": "number",
                    "required": false,
                    "acl": {"min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private"}
                }
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(cpt.status(), StatusCode::CREATED);

    // Alice logs in as Admin -> sees `margin` in effective-editor-config
    let (alice_cookie1, _, alice_site1) =
        login(&client, &srv.base_url, "alice", "AlicePassword123!").await;
    assert_eq!(alice_site1, site_id);
    let cfg_before: Value = client
        .get(format!(
            "{}/api/admin/content-types/portfolio/effective-editor-config",
            srv.base_url
        ))
        .header("Cookie", &alice_cookie1)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(cfg_before["effective_fields"].get("margin").is_some());

    // Admin downgrades Alice from Admin -> Editor
    let downgrade = client
        .patch(format!("{}/api/admin/users/{}", srv.base_url, alice_id))
        .header("Cookie", &admin_cookie)
        .header("Origin", &srv.base_url)
        .header("X-CSRF-Token", &admin_csrf)
        .header("If-Match", format!("\"v{alice_ver}\""))
        .json(&json!({ "role": "editor" }))
        .send()
        .await
        .unwrap();
    assert_eq!(downgrade.status(), StatusCode::OK);

    // Alice logs back in as Editor -> `margin` is stripped from effective-editor-config
    let (alice_cookie2, _, alice_site2) =
        login(&client, &srv.base_url, "alice", "AlicePassword123!").await;
    assert_eq!(alice_site2, site_id);
    let cfg_after: Value = client
        .get(format!(
            "{}/api/admin/content-types/portfolio/effective-editor-config",
            srv.base_url
        ))
        .header("Cookie", &alice_cookie2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(cfg_after["effective_fields"].get("margin").is_none());
    assert_eq!(cfg_after["effective_fields"]["client"]["editable"], true);
}
