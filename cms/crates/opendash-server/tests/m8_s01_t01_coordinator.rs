use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use reqwest::{Client, StatusCode};
use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "opendash-m8-s01-t01-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp test dir");
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
    async fn start(site_root: &Path, db_path: Option<&Path>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let bin = env!("CARGO_BIN_EXE_opendash");
        let mut cmd = Command::new(bin);
        cmd.env("OPENDASH_SITE_ROOT", site_root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if let Some(db) = db_path {
            cmd.env("OPENDASH_DB_PATH", db);
        } else {
            cmd.env_remove("OPENDASH_DB_PATH");
        }

        let child = cmd.spawn().expect("spawn opendash server");
        let base_url = format!("http://127.0.0.1:{port}");
        wait_ready(&base_url).await;
        Self {
            child: Some(child),
            base_url,
        }
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn wait_ready(base_url: &str) {
    let client = Client::new();
    for _ in 0..80 {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await {
            if resp.status() == StatusCode::OK {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server failed to become ready at {base_url}");
}

fn run_expect_fail(site_root: &Path, db_path: Option<&Path>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    drop(listener);

    let bin = env!("CARGO_BIN_EXE_opendash");
    let mut cmd = Command::new(bin);
    cmd.env("OPENDASH_SITE_ROOT", site_root)
        .env("OPENDASH_PORT", port.to_string());
    if let Some(db) = db_path {
        cmd.env("OPENDASH_DB_PATH", db);
    } else {
        cmd.env_remove("OPENDASH_DB_PATH");
    }

    let output = cmd.output().expect("run second opendash instance");
    assert!(
        !output.status.success(),
        "expected instance to fail, but exited with {:?}",
        output.status
    );
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[tokio::test]
async fn dual_process_same_site_root_fails_second_instance_without_breaking_first() {
    let dir = TempDirGuard::new("same-root");
    let site_root = dir.path().join("site");
    let server = RunningServer::start(&site_root, None).await;

    let alias_root = site_root.join(".");
    let stderr = run_expect_fail(&alias_root, None);
    assert!(
        stderr.contains("Busy") && (stderr.contains("site.lock") || stderr.contains("opendash.sqlite.opendash.lock")),
        "unexpected stderr: {stderr}"
    );

    let resp = Client::new()
        .get(format!("{}/api/health", server.base_url))
        .send()
        .await
        .expect("health request");
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn dual_process_distinct_roots_same_external_db_fails_second_instance() {
    let dir = TempDirGuard::new("same-ext-db");
    let root_a = dir.path().join("site-a");
    let root_b = dir.path().join("site-b");
    let ext_db = dir.path().join("shared").join("custom.sqlite");

    let server = RunningServer::start(&root_a, Some(&ext_db)).await;

    let ext_db_alias = dir.path().join("shared").join(".").join("custom.sqlite");
    let stderr = run_expect_fail(&root_b, Some(&ext_db_alias));
    assert!(
        stderr.contains("Busy") && stderr.contains("custom.sqlite.opendash.lock"),
        "unexpected stderr: {stderr}"
    );

    let resp = Client::new()
        .get(format!("{}/api/health", server.base_url))
        .send()
        .await
        .expect("health request");
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn distinct_roots_and_distinct_dbs_run_concurrently() {
    let dir = TempDirGuard::new("distinct");
    let root_a = dir.path().join("site-a");
    let root_b = dir.path().join("site-b");
    let server_a = RunningServer::start(&root_a, None).await;
    let server_b = RunningServer::start(&root_b, None).await;

    let client = Client::new();
    let ra = client
        .get(format!("{}/api/health", server_a.base_url))
        .send()
        .await
        .expect("health a");
    let rb = client
        .get(format!("{}/api/health", server_b.base_url))
        .send()
        .await
        .expect("health b");
    assert_eq!(ra.status(), StatusCode::OK);
    assert_eq!(rb.status(), StatusCode::OK);
}

#[cfg(windows)]
#[test]
fn windows_junction_site_root_is_rejected_before_db_creation() {
    let dir = TempDirGuard::new("junction");
    let target = dir.path().join("real-target");
    fs::create_dir_all(&target).expect("create target dir");
    let junction = dir.path().join("junction-root");

    let status = Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            &junction.to_string_lossy(),
            &target.to_string_lossy(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("create junction");
    assert!(status.success(), "mklink /J failed");

    let stderr = run_expect_fail(&junction, None);
    assert!(
        stderr.contains("symlink or reparse point not allowed"),
        "unexpected stderr: {stderr}"
    );
    assert!(!target.join("database").join("opendash.sqlite").exists());
}

#[tokio::test]
async fn upgrade_from_0031_preserves_data_and_startup_marks_inflight_jobs_interrupted() {
    let dir = TempDirGuard::new("upgrade-recover");
    let site_root = dir.path().join("site");
    let db_dir = site_root.join("database");
    fs::create_dir_all(&db_dir).expect("create db dir");
    let db_path = db_dir.join("opendash.sqlite");

    // First prepare a 0031-only database using migrations 0001..=0031 and seed user data
    let mig_src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("migrations");
    let mig_tmp = dir.path().join("mig-0031");
    fs::create_dir_all(&mig_tmp).expect("create temp migrations dir");
    for entry in fs::read_dir(&mig_src).expect("read migrations dir") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.ends_with(".sql") && !name_str.starts_with("0032_") {
            fs::copy(entry.path(), mig_tmp.join(&name)).expect("copy migration");
        }
    }

    let pre_opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pre_pool = SqlitePool::connect_with(pre_opts)
        .await
        .expect("connect pre-upgrade db");
    let migrator = sqlx::migrate::Migrator::new(mig_tmp.as_path())
        .await
        .expect("build 0031 migrator");
    migrator.run(&pre_pool).await.expect("run 0001..=0031");
    sqlx::query("UPDATE site_general_settings SET site_title = 'PreUpgradeSite' WHERE id = 1")
        .execute(&pre_pool)
        .await
        .expect("insert pre-upgrade setting");
    let max_pre: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&pre_pool)
        .await
        .expect("max pre version");
    assert_eq!(max_pre, 31);
    pre_pool.close().await;

    let mut server = RunningServer::start(&site_root, Some(&db_path)).await;
    server.stop();

    let opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(false);
    let pool = SqlitePool::connect_with(opts).await.expect("connect db");

    let preserved: String =
        sqlx::query_scalar("SELECT site_title FROM site_general_settings WHERE id = 1")
            .fetch_one(&pool)
            .await
            .expect("query preserved setting");
    assert_eq!(preserved, "PreUpgradeSite");

    let has_0032: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 32")
            .fetch_one(&pool)
            .await
            .expect("query _sqlx_migrations");
    assert_eq!(has_0032, 1);

    let gen_row: i64 =
        sqlx::query_scalar("SELECT generation FROM maintenance_generation WHERE id = 1")
            .fetch_one(&pool)
            .await
            .expect("query maintenance_generation");
    assert_eq!(gen_row, 0);

    let queued_id = format!("{:064x}", 1);
    let running_id = format!("{:064x}", 2);
    let succeeded_id = format!("{:064x}", 3);

    sqlx::query("INSERT INTO operation_jobs (id, kind, state) VALUES (?, 'media_delete', 'queued')")
        .bind(&queued_id)
        .execute(&pool)
        .await
        .expect("insert queued job");
    sqlx::query("INSERT INTO operation_jobs (id, kind, state) VALUES (?, 'backup', 'running')")
        .bind(&running_id)
        .execute(&pool)
        .await
        .expect("insert running job");
    sqlx::query("INSERT INTO operation_jobs (id, kind, state) VALUES (?, 'restore', 'succeeded')")
        .bind(&succeeded_id)
        .execute(&pool)
        .await
        .expect("insert succeeded job");

    // Partial unique index enforces single active heavy job
    let heavy_conflict_id = format!("{:064x}", 4);
    let conflict = sqlx::query(
        "INSERT INTO operation_jobs (id, kind, state) VALUES (?, 'purge', 'queued')",
    )
    .bind(&heavy_conflict_id)
    .execute(&pool)
    .await;
    assert!(conflict.is_err(), "expected partial unique index conflict");

    pool.close().await;

    // Restart server: queued and running must become interrupted; succeeded stays succeeded
    let mut server2 = RunningServer::start(&site_root, Some(&db_path)).await;
    server2.stop();

    let opts2 = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(false);
    let pool2 = SqlitePool::connect_with(opts2).await.expect("reconnect db");

    for id in [&queued_id, &running_id] {
        let row = sqlx::query("SELECT state, error_code FROM operation_jobs WHERE id = ?")
            .bind(id)
            .fetch_one(&pool2)
            .await
            .expect("fetch recovered job");
        let state: String = row.try_get("state").unwrap();
        let error_code: Option<String> = row.try_get("error_code").unwrap();
        assert_eq!(state, "interrupted");
        assert_eq!(error_code.as_deref(), Some("server_restarted"));
    }

    let ok_state: String = sqlx::query_scalar("SELECT state FROM operation_jobs WHERE id = ?")
        .bind(&succeeded_id)
        .fetch_one(&pool2)
        .await
        .expect("fetch succeeded job");
    assert_eq!(ok_state, "succeeded");
    pool2.close().await;
}
