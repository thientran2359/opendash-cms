use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

struct ServerGuard {
    child: Child,
    root: PathBuf,
    origin: String,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

async fn start_server() -> ServerGuard {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let raw_root = std::env::temp_dir().join(format!(
        "opendash-search-ops-{}-{port}-{seq}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&raw_root).unwrap();
    let root = fs::canonicalize(&raw_root).unwrap();
    let log = fs::File::create(root.join("server.log")).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_HOST", "127.0.0.1")
        .env("OPENDASH_PORT", port.to_string())
        .env("OPENDASH_SITE_ROOT", &root)
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .spawn()
        .unwrap();
    let origin = format!("http://127.0.0.1:{port}");
    let http = Client::new();
    for _ in 0..200 {
        if http
            .get(format!("{origin}/api/health"))
            .send()
            .await
            .is_ok_and(|r| r.status() == StatusCode::OK)
        {
            return ServerGuard {
                child,
                root,
                origin,
            };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let log_text = fs::read_to_string(root.join("server.log")).unwrap_or_default();
    panic!("server did not start at {origin}: {log_text}");
}

async fn login(http: &Client, origin: &str, username: &str, password: &str) -> (String, String) {
    let res = http
        .post(format!("{origin}/auth/login"))
        .header(header::ORIGIN, origin)
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = res
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let csrf = res.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    (cookie, csrf)
}

#[tokio::test]
async fn admin_search_tools_rbac_csrf_drift_recovery_and_job_audit() {
    let srv = start_server().await;
    let http = Client::new();

    let setup = http
        .post(format!("{}/auth/setup", srv.origin))
        .header(header::ORIGIN, &srv.origin)
        .json(&json!({ "username": "admin", "password": "SearchOpsPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let (admin_cookie, admin_csrf) =
        login(&http, &srv.origin, "admin", "SearchOpsPassword123!").await;

    let ed_create = http
        .post(format!("{}/api/admin/users", srv.origin))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({ "username": "editor1", "password": "EditorPassword123!", "role": "editor" }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_create.status(), StatusCode::CREATED);
    let (ed_cookie, ed_csrf) = login(&http, &srv.origin, "editor1", "EditorPassword123!").await;

    // 1. Editor receives 403 on status and reindex; missing CSRF rejected.
    let ed_status = http
        .get(format!("{}/api/admin/tools/search/status", srv.origin))
        .header(header::COOKIE, &ed_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(ed_status.status(), StatusCode::FORBIDDEN);

    let ed_reindex = http
        .post(format!("{}/api/admin/tools/search/reindex", srv.origin))
        .header(header::COOKIE, &ed_cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &ed_csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(ed_reindex.status(), StatusCode::FORBIDDEN);

    let no_csrf = http
        .post(format!("{}/api/admin/tools/search/reindex", srv.origin))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.origin)
        .send()
        .await
        .unwrap();
    assert_eq!(no_csrf.status(), StatusCode::FORBIDDEN);

    // 2. Create published post + draft post.
    let create_pub = http
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            srv.origin
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "title": "Reindex Telemetry Article",
            "slug": "reindex-telemetry-article",
            "values": { "body": "Unique ReindexableKeyword token" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_pub.status(), StatusCode::CREATED);
    let pub_json: Value = create_pub.json().await.unwrap();
    let pub_id = pub_json["id"].as_i64().unwrap();
    let pub_ver = pub_json["version"].as_i64().unwrap();
    let pub_res = http
        .post(format!(
            "{}/api/admin/content-types/post/entries/{pub_id}/publication",
            srv.origin
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{pub_ver}\""))
        .json(&json!({ "status": "published" }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_res.status(), StatusCode::OK);

    let create_draft = http
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            srv.origin
        ))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "title": "Draft Secret Article",
            "slug": "draft-secret-article",
            "values": { "body": "Should never appear for ReindexableKeyword" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_draft.status(), StatusCode::CREATED);

    let status_ok: Value = http
        .get(format!("{}/api/admin/tools/search/status", srv.origin))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status_ok["in_sync"], true);
    let eligible = status_ok["eligible_published_entries"].as_i64().unwrap();
    assert!(eligible >= 1);
    assert_eq!(status_ok["indexed_documents"], eligible);
    assert_eq!(status_ok["maintenance_active"], false);

    // 3. Introduce deliberate FTS5 drift and verify status detects out-of-sync.
    let db_path = srv.root.join("database/opendash.sqlite");
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))
        .unwrap()
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePool::connect_with(opts).await.unwrap();
    sqlx::query("DELETE FROM entry_search_fts")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    let drifted: Value = http
        .get(format!("{}/api/admin/tools/search/status", srv.origin))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(drifted["indexed_documents"], 0);
    assert_eq!(drifted["eligible_published_entries"], eligible);
    assert_eq!(drifted["in_sync"], false);

    // 4. Rebuild search index restores sync, logs operation_jobs, and restores live search hits.
    let reindex_res = http
        .post(format!("{}/api/admin/tools/search/reindex", srv.origin))
        .header(header::COOKIE, &admin_cookie)
        .header(header::ORIGIN, &srv.origin)
        .header("X-CSRF-Token", &admin_csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(reindex_res.status(), StatusCode::OK);
    let reindex_json: Value = reindex_res.json().await.unwrap();
    assert_eq!(reindex_json["in_sync"], true);
    assert_eq!(reindex_json["indexed_documents"], eligible);
    assert_eq!(reindex_json["eligible_published_entries"], eligible);
    assert_eq!(reindex_json["job"]["kind"], "search_reindex");
    assert_eq!(reindex_json["job"]["state"], "succeeded");
    let job_id = reindex_json["job"]["id"].as_str().unwrap().to_owned();
    let summary: Value =
        serde_json::from_str(reindex_json["job"]["summary_json"].as_str().unwrap()).unwrap();
    assert_eq!(summary["indexed_documents"], eligible);

    let op_job: Value = http
        .get(format!("{}/api/admin/operations/jobs/{job_id}", srv.origin))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(op_job["id"], job_id);
    assert_eq!(op_job["kind"], "search_reindex");
    assert_eq!(op_job["state"], "succeeded");

    let after_status: Value = http
        .get(format!("{}/api/admin/tools/search/status", srv.origin))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after_status["in_sync"], true);
    assert_eq!(after_status["last_job"]["id"], job_id);

    let api_search: Value = http
        .get(format!(
            "{}/api/public/search?s=ReindexableKeyword",
            srv.origin
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let items = api_search["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["slug"], "reindex-telemetry-article");
}
