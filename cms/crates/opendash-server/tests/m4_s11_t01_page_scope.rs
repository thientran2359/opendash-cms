//! M4-S11-T01: Page overrides must be bound to an existing, exact Page.
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

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
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = tcp.local_addr().unwrap().port();
    drop(tcp);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "opendash-m4-s11-t01-{}-{nanos}",
            std::process::id()
        ));
    fs::create_dir_all(&root).unwrap();
    let log_path = root.join("startup.log");
    let log_file = fs::File::create(&log_path).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_SITE_ROOT", &root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log_file))
        .spawn()
        .unwrap();
    let base = format!("http://127.0.0.1:{port}");
    for _ in 0..100 {
        if let Some(exit) = child.try_wait().unwrap() {
            panic!(
                "isolated server quit ({exit}): {}",
                fs::read_to_string(&log_path).unwrap_or_default()
            );
        }
        if let Ok(res) = Client::new().get(format!("{base}/api/health")).send().await
            && res.status().is_success()
        {
            return Site { child, root, base };
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
    panic!(
        "isolated server did not start: {}",
        fs::read_to_string(&log_path).unwrap_or_default()
    );
}

async fn login(http: &Client, site: &Site, username: &str, password: &str) -> (String, String) {
    let res = http
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
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or(v))
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = res.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_owned())
}

fn design(title: &str) -> Value {
    json!({"schema_version":1,"sections":[{"id":"section","children":[{"id":"row","children":[{"id":"column","children":[{"id":"heading","type":"core/heading","props":{"level":2,"text":title}}]}]}]}]})
}

#[tokio::test]
async fn editor_page_exact_scope_acl_draft_cas_reset() {
    let site = start().await;
    let http = Client::new();
    let setup = http
        .post(format!("{}/auth/setup", site.base))
        .header("Origin", &site.base)
        .json(&json!({"username":"siteadmin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let (admin_cookie, admin_csrf) = login(&http, &site, "siteadmin", "AdminPassword123!").await;
    let create_editor = http
        .post(format!("{}/api/admin/users", site.base))
        .header("Cookie", &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({"username":"pageeditor","password":"EditorPassword123!","role":"editor"}))
        .send()
        .await
        .unwrap();
    assert_eq!(create_editor.status(), StatusCode::CREATED);
    let (editor_cookie, editor_csrf) =
        login(&http, &site, "pageeditor", "EditorPassword123!").await;
    let mut ids = vec![];
    for (slug, title) in [("scope-a", "Page A"), ("scope-b", "Page B")] {
        let res = http
            .post(format!(
                "{}/api/admin/content-types/page/entries",
                site.base
            ))
            .header("Cookie", &admin_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &admin_csrf)
            .json(&json!({"title":title,"slug":slug,"values":{}}))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let item: Value = res.json().await.unwrap();
        ids.push(item["id"].as_i64().unwrap());
    }
    let a = format!(
        "{}/api/admin/builder/overrides/page?type_key=page&entry_id={}",
        site.base, ids[0]
    );
    let b = format!(
        "{}/api/admin/builder/overrides/page?type_key=page&entry_id={}",
        site.base, ids[1]
    );
    let a_open = http
        .get(&a)
        .header("Cookie", &editor_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(a_open.status(), StatusCode::OK);
    let entry: Value = a_open.json().await.unwrap();
    assert_eq!(entry["scope_entry_id"], ids[0]);
    assert_eq!(entry["exists"], false);
    assert_eq!(entry["version"], 0);

    // Never return a 200 empty document for nonexistent/mismatched entries.
    for url in [
        format!(
            "{}/api/admin/builder/overrides/page?type_key=page&entry_id=99999999",
            site.base
        ),
        format!(
            "{}/api/admin/builder/overrides/single?type_key=post&entry_id={}",
            site.base, ids[0]
        ),
    ] {
        assert_eq!(
            http.get(url)
                .header("Cookie", &admin_cookie)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    for url in [
        format!("{}/api/admin/builder/homepage", site.base),
        format!("{}/api/admin/builder/overrides/header", site.base),
        format!(
            "{}/api/admin/builder/overrides/single?type_key=page&entry_id={}",
            site.base, ids[0]
        ),
    ] {
        assert_eq!(
            http.get(url)
                .header("Cookie", &editor_cookie)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        http.get(format!(
            "{}/api/admin/builder/overrides/page?type_key=post&entry_id={}",
            site.base, ids[0]
        ))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .status(),
        StatusCode::BAD_REQUEST
    );
    let document = design("Private Draft A");
    let forged_preview = http.post(format!(
        "{}/api/admin/builder/overrides/page/preview?type_key=page&entry_id={}&preview_type_key=post&preview_entry_slug=another",
        site.base, ids[0]
    ))
        .header("Cookie",&editor_cookie).header("Origin",&site.base)
        .header("X-CSRF-Token",&editor_csrf).json(&document).send().await.unwrap();
    assert_eq!(
        forged_preview.status(),
        StatusCode::BAD_REQUEST,
        "Page scope cannot preview another entry through query overrides"
    );
    let preview = http
        .post(format!(
            "{}/api/admin/builder/overrides/page/preview?type_key=page&entry_id={}",
            site.base, ids[0]
        ))
        .header("Cookie", &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .json(&document)
        .send()
        .await
        .unwrap();
    assert_eq!(preview.status(), StatusCode::OK);
    assert_eq!(preview.headers().get("cache-control").unwrap(), "no-store");
    assert!(preview.text().await.unwrap().contains("Private Draft A"));
    let saved = http
        .put(&a)
        .header("Cookie", &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header("If-Match", "\"v0\"")
        .json(&document)
        .send()
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    assert_eq!(
        http.get(&b)
            .header("Cookie", &editor_cookie)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()["exists"],
        false
    );
    assert_eq!(
        http.get(format!("{}/content/page/scope-a", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND,
        "draft must remain private"
    );
    let stale = http
        .put(&a)
        .header("Cookie", &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header("If-Match", "\"v0\"")
        .json(&design("Conflict must not persist"))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    let stored: Value = http
        .get(&a)
        .header("Cookie", &editor_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(stored["document"], document);
    let reset = http
        .post(format!(
            "{}/api/admin/builder/overrides/page/reset?type_key=page&entry_id={}",
            site.base, ids[0]
        ))
        .header("Cookie", &editor_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &editor_csrf)
        .header("If-Match", "\"v1\"")
        .json(&json!({"confirm":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(reset.status(), StatusCode::OK);
    assert_eq!(
        http.get(&a)
            .header("Cookie", &editor_cookie)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()["exists"],
        false
    );
}
