//! EGM #430: Pattern replaces Section with private, Admin-controlled entry Builder.
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
        .join(format!("opendash-section-{}-{unique}", std::process::id()));
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

async fn login(client: &Client, site: &Site, username: &str, password: &str) -> (String, String) {
    let res = client
        .post(format!("{}/auth/login", site.base))
        .json(&json!({"username":username,"password":password}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = res
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ");
    let data: Value = res.json().await.unwrap();
    (cookie, data["csrf_token"].as_str().unwrap().to_owned())
}

fn ast(heading: &str) -> Value {
    json!({"schema_version":1,"blocks":[{"id":"section","block_id":"core/section","children":[{"id":"row","block_id":"core/row","children":[{"id":"column","block_id":"core/column","children":[{"id":"heading","block_id":"core/heading","props":{"level":"h2","text":heading}}]}]}]}]})
}

#[tokio::test]
async fn pattern_private_crud_document_cas_and_public_fences() {
    let site = start().await;
    let client = Client::new();
    let setup = client
        .post(format!("{}/auth/setup", site.base))
        .header("Origin", &site.base)
        .json(&json!({"username":"siteadmin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let (cookie, csrf) = login(&client, &site, "siteadmin", "AdminPassword123!").await;
    let list = format!("{}/api/admin/patterns", site.base);
    let created = client
        .post(&list)
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"Pattern A","slug":"private-pattern-a"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        created.status(),
        StatusCode::CREATED,
        "Pattern create must be a working API"
    );
    let record: Value = created.json().await.unwrap();
    let id = record["id"].as_i64().unwrap();
    let doc_url = format!("{}/api/admin/entries/pattern/{id}/builder", site.base);
    let opened = client
        .get(&doc_url)
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(opened.status(), StatusCode::OK);
    let opened_json: Value = opened.json().await.unwrap();
    assert_eq!(opened_json["entry_id"], id);
    assert_eq!(opened_json["version"], 1);
    assert_eq!(opened_json["document"]["blocks"], json!([]));
    let design = ast("Never Public");
    let preview = client
        .post(format!("{doc_url}/preview"))
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&design)
        .send()
        .await
        .unwrap();
    assert_eq!(preview.status(), StatusCode::OK);
    assert_eq!(preview.headers().get("cache-control").unwrap(), "no-store");
    assert!(
        preview.json::<Value>().await.unwrap()["html"]
            .as_str()
            .unwrap()
            .contains("Never Public")
    );
    let saved = client
        .put(&doc_url)
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&design)
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    let saved_json: Value = saved.json().await.unwrap();
    assert_eq!(saved_json["version"], 2);
    let stale = client
        .put(&doc_url)
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v1\"")
        .json(&ast("Stale"))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    let no_match = client
        .put(&doc_url)
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&design)
        .send()
        .await
        .unwrap();
    assert_eq!(no_match.status(), StatusCode::PRECONDITION_REQUIRED);
    let unauth = client.get(&doc_url).send().await.unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
    let public_single = client
        .get(format!("{}/content/pattern/private-pattern-a", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(public_single.status(), StatusCode::NOT_FOUND);
    let public_archive = client
        .get(format!("{}/content/pattern", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(public_archive.status(), StatusCode::NOT_FOUND);
    let generic_create = client
        .post(format!(
            "{}/api/admin/content-types/pattern/entries",
            site.base
        ))
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"Injected","slug":"inject","values":{"body":"public"}}))
        .send()
        .await
        .unwrap();
    assert!(!generic_create.status().is_success());
    let generic_update = client
        .put(format!(
            "{}/api/admin/content-types/pattern/entries/{id}",
            site.base
        ))
        .header("Cookie", &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header("If-Match", "\"v2\"")
        .json(&json!({"title":"Injected","slug":"private-pattern-a","values":{"body":"public"}}))
        .send()
        .await
        .unwrap();
    assert!(!generic_update.status().is_success());
    let after = client
        .get(&doc_url)
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(after["document"], design);
    assert_eq!(after["version"], 2);
}
