//! Local template catalog remains authenticated; Pattern CPT is a separate
//! private collection after EGM S02 migration.
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Site {
    child: Child,
    root: PathBuf,
    base: String,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

async fn start() -> Site {
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    drop(socket);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("opendash-library-{}-{unique}", std::process::id()));
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
    for _ in 0..100 {
        if let Some(status) = child.try_wait().unwrap() {
            panic!(
                "server {status}: {}",
                fs::read_to_string(root.join("startup.log")).unwrap()
            );
        }
        if let Ok(result) = Client::new().get(format!("{base}/api/health")).send().await
            && result.status().is_success()
        {
            return Site { child, root, base };
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
    panic!(
        "server startup: {}",
        fs::read_to_string(root.join("startup.log")).unwrap()
    );
}

#[tokio::test]
async fn local_library_catalog_is_authenticated_and_pattern_scoped() {
    let site = start().await;
    let client = Client::new();
    let root = format!("{}/api/admin/library", site.base);
    assert_eq!(
        client.get(&root).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let setup = client
        .post(format!("{}/auth/setup", site.base))
        .header("Origin", &site.base)
        .json(&json!({"username":"siteadmin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let login = client
        .post(format!("{}/auth/login", site.base))
        .json(&json!({"username":"siteadmin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let csrf: Value = login.json().await.unwrap();
    let csrf = csrf["csrf_token"].as_str().unwrap();
    let create = client
        .post(format!("{}/api/admin/patterns", site.base))
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", csrf)
        .json(&json!({"title":"Private local banner","slug":"local-banner"}))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created: Value = create.json().await.unwrap();
    let response = client
        .get(&root)
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "local library API missing"
    );
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let data: Value = response.json().await.unwrap();
    let patterns: Value = client
        .get(format!("{}/api/admin/patterns", site.base))
        .header("Cookie", &cookie)
        .send().await.unwrap().json().await.unwrap();
    assert_eq!(patterns["items"].as_array().unwrap().len(), 1);
    assert_eq!(patterns["items"][0]["id"], created["id"]);
    assert_eq!(patterns["items"][0]["title"], "Private local banner");
    assert_eq!(data["theme_templates"], json!([]));
    assert!(
        patterns["items"][0].get("document").is_none(),
        "catalog must not leak private AST"
    );
    assert_eq!(
        client
            .get(format!("{}/content/pattern/local-banner", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}
