//! EGM S02: private Pattern CPT, transactional one-hop references and public
//! ACL follow the referencing Page's publication, never the Pattern itself.
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, Response, StatusCode, header};
use serde_json::{Value, json};
use sqlx::SqlitePool;

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
        .join(format!(
            "opendash-egm-pattern-s02-{}-{unique}",
            std::process::id()
        ));
    fs::create_dir_all(&root).unwrap();
    let stderr = fs::File::create(root.join("server.log")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_opendash"))
        .env("OPENDASH_SITE_ROOT", &root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
        .unwrap();
    let base = format!("http://127.0.0.1:{port}");
    let http = Client::new();
    for _ in 0..200 {
        if let Some(status) = child.try_wait().unwrap() {
            panic!(
                "server exited {status}: {}",
                fs::read_to_string(root.join("server.log")).unwrap()
            );
        }
        if http
            .get(format!("{base}/api/health"))
            .send()
            .await
            .is_ok_and(|r| r.status() == StatusCode::OK)
        {
            return Site { child, root, base };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "isolated server did not start: {}",
        fs::read_to_string(root.join("server.log")).unwrap()
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
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let body: Value = res.json().await.unwrap();
    (cookie, body["csrf_token"].as_str().unwrap().to_string())
}

async fn put_builder(
    http: &Client,
    site: &Site,
    cookie: &str,
    csrf: &str,
    type_key: &str,
    id: i64,
    version: i64,
    doc: &Value,
) -> Response {
    http.put(format!(
        "{}/api/admin/entries/{type_key}/{id}/builder",
        site.base
    ))
    .header(header::COOKIE, cookie)
    .header(header::ORIGIN, &site.base)
    .header("X-CSRF-Token", csrf)
    .header(header::IF_MATCH, format!("\"v{version}\""))
    .json(doc)
    .send()
    .await
    .unwrap()
}

fn document(element: Value) -> Value {
    json!({"schema_version":1,"sections":[{"id":"section","children":[
        {"id":"row","children":[{"id":"column","children":[element]}]}
    ]}]})
}

fn heading(text: &str) -> Value {
    json!({"id":"heading","type":"core/heading","props":{"level":2,"text":text}})
}

async fn create_page(http: &Client, site: &Site, cookie: &str, csrf: &str, slug: &str) -> i64 {
    let res = http
        .post(format!(
            "{}/api/admin/content-types/page/entries",
            site.base
        ))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", csrf)
        .json(&json!({"title":slug,"slug":slug,"values":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    res.json::<Value>().await.unwrap()["id"].as_i64().unwrap()
}

async fn publish_page(http: &Client, site: &Site, cookie: &str, csrf: &str, id: i64) {
    let res = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/{id}/publication",
            site.base
        ))
        .header(header::COOKIE, cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({"status":"published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn pattern_references_are_synced_private_one_hop_and_cas_guarded() {
    let site = start().await;
    let http = Client::new();
    let setup = http
        .post(format!("{}/auth/setup", site.base))
        .header(header::ORIGIN, &site.base)
        .json(&json!({"username":"admin","password":"AdminPassword123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let (cookie, csrf) = login(&http, &site, "admin", "AdminPassword123!").await;

    let editor_create = http
        .post(format!("{}/api/admin/users", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"username":"editor","password":"EditorPassword123!","role":"editor"}))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_create.status(), StatusCode::CREATED);
    let (editor_cookie, editor_csrf) = login(&http, &site, "editor", "EditorPassword123!").await;

    let created = http
        .post(format!("{}/api/admin/patterns", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"Synced Banner","slug":"synced-banner"}))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(created.headers()[header::ETAG], "\"v1\"");
    let pattern: Value = created.json().await.unwrap();
    let pattern_id = pattern["id"].as_i64().unwrap();

    let pattern_root = format!("{}/api/admin/patterns", site.base);
    let pattern_url = format!("{pattern_root}/{pattern_id}");
    for response in [
        http.post(&pattern_root)
            .header(header::COOKIE, &editor_cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &editor_csrf)
            .json(&json!({"title":"No","slug":"no"}))
            .send()
            .await
            .unwrap(),
        http.put(&pattern_url)
            .header(header::COOKIE, &editor_cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &editor_csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({"title":"No","slug":"no"}))
            .send()
            .await
            .unwrap(),
        http.delete(&pattern_url)
            .header(header::COOKIE, &editor_cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &editor_csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .send()
            .await
            .unwrap(),
    ] {
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    let editable = document(heading("Initial Banner"));
    let editor_attempt = put_builder(
        &http,
        &site,
        &editor_cookie,
        &editor_csrf,
        "pattern",
        pattern_id,
        1,
        &editable,
    )
    .await;
    assert_eq!(editor_attempt.status(), StatusCode::FORBIDDEN);
    let nested = document(
        json!({"id":"nested","type":"core/pattern_ref","props":{"pattern_id":pattern_id}}),
    );
    assert_eq!(
        put_builder(
            &http, &site, &cookie, &csrf, "pattern", pattern_id, 1, &nested
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let nested_list = document(json!({"id":"list","type":"core/list","props":{
        "type_key":"post","include_descendants":false,"sort":"id_desc","page_size":5
    }}));
    assert_eq!(
        put_builder(
            &http,
            &site,
            &cookie,
            &csrf,
            "pattern",
            pattern_id,
            1,
            &nested_list
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );

    // A real upload referenced only by a private Pattern is never public.
    let mut png = Vec::new();
    let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([30, 80, 110, 255]));
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let upload = http
        .post(format!("{}/api/admin/media", site.base))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::CONTENT_TYPE, "image/png")
        .header("X-File-Name", "pattern.png")
        .body(png)
        .send()
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::CREATED);
    let media_id = upload.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let media_url = format!("{}/media/{media_id}/file", site.base);
    let mut pattern_doc = editable.clone();
    pattern_doc["sections"][0]["children"][0]["children"][0]["children"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"media","type":"core/image","props":{"media_id":media_id,"alt":"private banner"}
        }));
    assert_eq!(
        put_builder(
            &http,
            &site,
            &cookie,
            &csrf,
            "pattern",
            pattern_id,
            1,
            &pattern_doc
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        http.get(&media_url).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        http.get(format!("{}/content/pattern/synced-banner", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    let ref_doc = document(json!({
        "id":"shared","type":"core/pattern_ref","props":{"pattern_id":pattern_id}
    }));
    let first_id = create_page(&http, &site, &cookie, &csrf, "pattern-home").await;
    let second_id = create_page(&http, &site, &cookie, &csrf, "pattern-contact").await;
    let third_id = create_page(&http, &site, &cookie, &csrf, "pattern-about").await;
    for id in [first_id, second_id, third_id] {
        assert_eq!(
            put_builder(&http, &site, &cookie, &csrf, "page", id, 1, &ref_doc)
                .await
                .status(),
            StatusCode::OK
        );
    }
    let bad_ref = document(json!({
        "id":"other","type":"core/pattern_ref","props":{"pattern_id":first_id}
    }));
    let fourth_id = create_page(&http, &site, &cookie, &csrf, "pattern-other").await;
    assert_eq!(
        put_builder(&http, &site, &cookie, &csrf, "page", fourth_id, 1, &bad_ref)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let db = SqlitePool::connect(&format!(
        "sqlite://{}",
        site.root.join("database/opendash.sqlite").display()
    ))
    .await
    .unwrap();
    let refs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_pattern_refs WHERE target_pattern_id=?")
            .bind(pattern_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(refs, 3);

    assert_eq!(
        http.get(&media_url).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    for id in [first_id, second_id, third_id] {
        publish_page(&http, &site, &cookie, &csrf, id).await;
    }
    for slug in ["pattern-home", "pattern-contact", "pattern-about"] {
        let page = http
            .get(format!("{}/content/page/{slug}", site.base))
            .send()
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);
        let html = page.text().await.unwrap();
        assert!(
            html.contains("Initial Banner"),
            "pattern must appear on {slug}"
        );
        assert!(html.contains("od-pattern-ref"));
    }
    assert_eq!(
        http.get(&media_url).send().await.unwrap().status(),
        StatusCode::OK
    );

    pattern_doc["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["text"] =
        json!("Updated Synced Banner");
    assert_eq!(
        put_builder(
            &http,
            &site,
            &cookie,
            &csrf,
            "pattern",
            pattern_id,
            1,
            &pattern_doc
        )
        .await
        .status(),
        StatusCode::PRECONDITION_FAILED
    );
    assert_eq!(
        put_builder(
            &http,
            &site,
            &cookie,
            &csrf,
            "pattern",
            pattern_id,
            2,
            &pattern_doc
        )
        .await
        .status(),
        StatusCode::OK
    );
    for slug in ["pattern-home", "pattern-contact", "pattern-about"] {
        let html = http
            .get(format!("{}/content/page/{slug}", site.base))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(html.contains("Updated Synced Banner"));
        assert!(!html.contains("Initial Banner"));
    }
    let listed: Value = http
        .get(format!("{}/api/admin/patterns", site.base))
        .header(header::COOKIE, &editor_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed["items"][0]["referencing_count"], 3);
    // All three Page references stay in the AST, but an unpublish immediately
    // revokes the public media grant when no published parent remains.
    for id in [first_id, second_id, third_id] {
        let response = http
            .post(format!(
                "{}/api/admin/content-types/page/entries/{id}/publication",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v3\"")
            .json(&json!({"status":"draft"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert_eq!(
        http.get(&media_url).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    let delete_url = format!("{}/api/admin/patterns/{pattern_id}", site.base);
    let blocked = http
        .delete(&delete_url)
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v3\"")
        .send()
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::CONFLICT);
    assert_eq!(
        blocked.json::<Value>().await.unwrap()["referencing_count"],
        3
    );
    let deleted = http
        .delete(format!("{delete_url}?force=true"))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v3\"")
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let gone: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_pattern_refs WHERE target_pattern_id=?")
            .bind(pattern_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(gone, 0);
    let page_id = first_id;
    let response = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/{page_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v4\"")
        .json(&json!({"status":"published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let page = http
        .get(format!("{}/content/page/pattern-home", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert!(!page.text().await.unwrap().contains("Updated Synced Banner"));
    assert_eq!(
        http.get(&media_url).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
}
