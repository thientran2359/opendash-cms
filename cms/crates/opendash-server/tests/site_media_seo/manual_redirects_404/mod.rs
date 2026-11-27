mod crud_and_precedence;
mod csv_and_telemetry;

use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header, redirect::Policy};
use serde_json::{Value, json};
use sqlx::SqlitePool;

static NEXT_SITE_SEQ: AtomicU64 = AtomicU64::new(1);

pub struct Site {
    pub root: PathBuf,
    pub child: Option<Child>,
    pub base: String,
}

impl Drop for Site {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Site {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "opendash-redirects-404-{}-{nanos}-{}",
                std::process::id(),
                NEXT_SITE_SEQ.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&root).unwrap();
        let stderr = fs::File::create(root.join("startup.log")).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr))
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let http = Client::new();
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                panic!(
                    "temporary redirects server exited {status}: {}",
                    fs::read_to_string(root.join("startup.log")).unwrap_or_default()
                );
            }
            if http
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|res| res.status() == StatusCode::OK)
            {
                return Self {
                    root,
                    child: Some(child),
                    base,
                };
            }
            if Instant::now() >= deadline {
                panic!(
                    "temporary redirects server was not ready: {}",
                    fs::read_to_string(root.join("startup.log")).unwrap_or_default()
                );
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub async fn db(&self) -> SqlitePool {
        SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(self.root.join("database/opendash.sqlite")),
        )
        .await
        .unwrap()
    }
}

pub struct AuthSession {
    pub cookie: String,
    pub csrf: String,
}

pub fn no_redirect_client() -> Client {
    Client::builder().redirect(Policy::none()).build().unwrap()
}

pub async fn setup_and_login_admin(http: &Client, site: &Site) -> AuthSession {
    let setup = http
        .post(format!("{}/auth/setup", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({"username":"redir_admin","password":"Redirect-Admin-Password-2026!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    login_user(http, site, "redir_admin", "Redirect-Admin-Password-2026!").await
}

pub async fn login_user(http: &Client, site: &Site, username: &str, password: &str) -> AuthSession {
    let result = http
        .post(format!("{}/auth/login", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({"username": username, "password": password}))
        .send()
        .await
        .unwrap();
    assert_eq!(result.status(), StatusCode::OK);
    let cookie = result
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = result.json().await.unwrap();
    AuthSession {
        cookie,
        csrf: body["csrf_token"].as_str().unwrap().to_owned(),
    }
}

pub async fn create_editor_and_login(
    http: &Client,
    site: &Site,
    admin: &AuthSession,
) -> AuthSession {
    let create = http
        .post(format!("{}/api/admin/users", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({
            "username": "redir_editor",
            "password": "Redirect-Editor-Password-2026!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    login_user(http, site, "redir_editor", "Redirect-Editor-Password-2026!").await
}
