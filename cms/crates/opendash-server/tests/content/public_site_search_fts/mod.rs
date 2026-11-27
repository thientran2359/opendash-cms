//! Slice S01 E2E SQLite FTS5 synchronization tests across HTTP writers and startup recovery.

use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

mod entry_and_schema_sync;
mod locale_and_startup_repair;

pub(super) struct Site {
    pub(super) root: PathBuf,
    child: Option<Child>,
    pub(super) base: String,
}

impl Drop for Site {
    fn drop(&mut self) {
        self.stop_server();
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Site {
    pub(super) async fn start_at(root: PathBuf) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        fs::create_dir_all(&root).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_HOST", "127.0.0.1")
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let client = Client::new();
        for _ in 0..400 {
            if client
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status() == StatusCode::OK)
            {
                return Self {
                    root,
                    child: Some(child),
                    base,
                };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("opendash server did not become healthy");
    }

    pub(super) async fn start(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap_or_else(|_| std::env::temp_dir())
            .join(format!("opendash-{prefix}-{}-{nanos}", std::process::id()));
        Self::start_at(root).await
    }

    pub(super) fn stop_server(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub(super) async fn db(&self) -> SqlitePool {
        SqlitePool::connect_with(
            SqliteConnectOptions::new().filename(self.root.join("database/opendash.sqlite")),
        )
        .await
        .unwrap()
    }
}

pub(super) async fn setup_and_login_admin(client: &Client, base: &str) -> (String, String) {
    let setup = client
        .post(format!("{base}/auth/setup"))
        .header("Origin", base)
        .json(&json!({"username":"admin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = client
        .post(format!("{base}/auth/login"))
        .header("Origin", base)
        .json(&json!({"username":"admin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .map(|h| h.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let csrf = login.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    (cookie, csrf)
}

pub(super) async fn fts_match_ids(db: &SqlitePool, query: &str) -> Vec<i64> {
    sqlx::query_scalar(
        "SELECT rowid FROM entry_search_fts WHERE entry_search_fts MATCH ? ORDER BY rowid",
    )
    .bind(query)
    .fetch_all(db)
    .await
    .unwrap()
}

pub(super) async fn fts_docsize_ids(db: &SqlitePool) -> Vec<i64> {
    sqlx::query_scalar("SELECT id FROM entry_search_fts_docsize ORDER BY id")
        .fetch_all(db)
        .await
        .unwrap()
}

pub(super) async fn schema_token(
    client: &Client,
    base: &str,
    cookie: &str,
    type_key: &str,
) -> String {
    let resp = client
        .get(format!(
            "{base}/api/admin/content-types/{type_key}/entry-table?limit=1"
        ))
        .header(header::COOKIE, cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    resp.json::<Value>().await.unwrap()["schema_token"]
        .as_str()
        .unwrap()
        .to_owned()
}
