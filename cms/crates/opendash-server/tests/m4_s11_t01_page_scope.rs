//! D02: Page Builder documents use the exact entry and its shared CAS token.
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
            .json(&json!({"title":title,"slug":slug,"values":{"body":format!("Original text {title}")}}))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let item: Value = res.json().await.unwrap();
        ids.push(item["id"].as_i64().unwrap());
    }
    let a = format!("{}/api/admin/entries/page/{}/builder", site.base, ids[0]);
    let b = format!("{}/api/admin/entries/page/{}/builder", site.base, ids[1]);
    let entry = format!(
        "{}/api/admin/content-types/page/entries/{}",
        site.base, ids[0]
    );
    let public = format!("{}/content/page/scope-a", site.base);
    let open = http
        .get(&a)
        .header("Cookie", &editor_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(open.status(), StatusCode::OK);
    assert_eq!(open.headers()["etag"], "\"v1\"");
    assert_eq!(open.headers()["cache-control"], "no-store");
    let initial: Value = open.json().await.unwrap();
    assert_eq!(
        initial,
        json!({"entry_id":ids[0],"type_key":"page","content_mode":"editor","version":1,"document":null})
    );
    let wrong_type = format!("{}/api/admin/entries/post/{}/builder", site.base, ids[0]);
    assert_eq!(
        http.get(&wrong_type).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        http.get(&wrong_type)
            .header("Cookie", &editor_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        http.get(format!(
            "{}/api/admin/entries/page/99999999/builder",
            site.base
        ))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .status(),
        StatusCode::NOT_FOUND
    );
    // S03 removed the independent homepage API for every role. The typed
    // override endpoint still enforces Admin RBAC.
    assert_eq!(
        http.get(format!("{}/api/admin/builder/homepage", site.base))
            .header("Cookie", &editor_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        http.get(format!("{}/api/admin/template-overrides", site.base))
            .header("Cookie", &editor_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        http.get(format!(
            "{}/api/admin/builder/overrides/page?type_key=page&entry_id={}",
            site.base, ids[0]
        ))
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .status(),
        StatusCode::NOT_FOUND
    );
    // Upload real media so draft/preview/publication and reset exercise the
    // public ACL rather than merely checking that DOM includes a URL.
    let mut png = Vec::new();
    let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([40, 60, 80, 255]));
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let uploaded = http
        .post(format!("{}/api/admin/media", site.base))
        .header("Cookie", &admin_cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &admin_csrf)
        .header("Content-Type", "image/png")
        .header("X-File-Name", "page-builder.png")
        .body(png)
        .send()
        .await
        .unwrap();
    assert_eq!(uploaded.status(), StatusCode::CREATED);
    let image_id = uploaded.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let media = format!("{}/media/{image_id}/file", site.base);

    let mut document = design("Private Draft A");
    document["sections"][0]["children"][0]["children"][0]["children"].as_array_mut().unwrap()
        .push(json!({"id":"image","type":"core/image","props":{"media_id":image_id,"alt":"Page image"}}));
    let preview_path = format!("{a}/preview");
    let put = |version: i64| {
        http.put(&a)
            .header("Cookie", &editor_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &editor_csrf)
            .header("If-Match", format!("\"v{version}\""))
    };
    let preview = |url: &str| {
        http.post(url)
            .header("Cookie", &editor_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &editor_csrf)
    };
    let reset = |version: i64| {
        http.post(format!("{a}/reset"))
            .header("Cookie", &editor_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &editor_csrf)
            .header("If-Match", format!("\"v{version}\""))
    };

    assert_eq!(
        http.put(&a)
            .header("Cookie", &editor_cookie)
            .header("Origin", &site.base)
            .header("If-Match", "\"v1\"")
            .json(&document)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        http.put(&a)
            .header("Cookie", &editor_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &editor_csrf)
            .json(&document)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PRECONDITION_REQUIRED
    );
    assert_eq!(
        put(1)
            .body("wrong content type")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        preview(&format!(
            "{preview_path}?preview_type_key=post&preview_entry_slug=another"
        ))
        .json(&document)
        .send()
        .await
        .unwrap()
        .status(),
        StatusCode::BAD_REQUEST
    );
    let mut forged = document.clone();
    forged["preview_entry_id"] = json!(ids[1]);
    assert_eq!(
        preview(&preview_path)
            .json(&forged)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );

    for element in [
        json!({"id":"slot","type":"core/content_slot","props":{}}),
        json!({"id":"part","type":"core/template_part","props":{"part_key":"header"}}),
        json!({"id":"bad-media","type":"core/image","props":{"media_id":"missing-image","alt":"Missing"}}),
    ] {
        let mut invalid = document.clone();
        invalid["sections"][0]["children"][0]["children"][0]["children"]
            .as_array_mut()
            .unwrap()
            .push(element);
        assert_eq!(
            put(1).json(&invalid).send().await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }
    let mut route_bound = design("Fallback");
    route_bound["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["binding"] =
        json!({"source":{"kind":"route_context","field_key":"title"},"target":"text"});
    let result = preview(&preview_path)
        .json(&route_bound)
        .send()
        .await
        .unwrap();
    assert_eq!(result.status(), StatusCode::OK);
    let body: Value = result.json().await.unwrap();
    assert!(body["html"].as_str().unwrap().contains("Page A"));
    assert!(!body["html"].as_str().unwrap().contains("Page B"));
    let result = preview(&preview_path).json(&document).send().await.unwrap();
    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(result.headers()["cache-control"], "no-store");
    assert!(result.headers().contains_key("content-security-policy"));
    assert!(result.text().await.unwrap().contains("Private Draft A"));
    let after_preview = http
        .get(&a)
        .header("Cookie", &editor_cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(after_preview, initial);
    assert_eq!(
        http.get(&media).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );

    let saved = put(1).json(&document).send().await.unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    assert_eq!(saved.headers()["etag"], "\"v2\"");
    let saved = saved.json::<Value>().await.unwrap();
    assert_eq!(saved["document"], document);
    assert_eq!(saved["content_mode"], "builder");
    assert_eq!(saved["version"], 2);
    let b_open = http
        .get(&b)
        .header("Cookie", &admin_cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        b_open,
        json!({"entry_id":ids[1],"type_key":"page","content_mode":"editor","version":1,"document":null})
    );
    assert_eq!(
        http.get(&public).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        http.get(&media).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        put(1)
            .json(&design("Conflict must not persist"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PRECONDITION_FAILED
    );
    assert_eq!(
        http.get(&a)
            .header("Cookie", &editor_cookie)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap(),
        saved
    );
    assert_eq!(
        reset(2)
            .json(&json!({"confirm":false}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );

    // Dedicated publication must share the CAS and expose existing document.
    let publish = |version: i64, status: &str| {
        http.post(format!("{entry}/publication"))
            .header("Cookie", &editor_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &editor_csrf)
            .header("If-Match", format!("\"v{version}\""))
            .json(&json!({"status":status}))
    };
    assert_eq!(
        publish(1, "published").send().await.unwrap().status(),
        StatusCode::PRECONDITION_FAILED
    );
    let response = publish(2, "published").send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["etag"], "\"v3\"");
    assert!(
        http.get(&public)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
            .contains("Private Draft A")
    );
    assert_eq!(
        http.get(&media).send().await.unwrap().status(),
        StatusCode::OK
    );

    // Published Save is immediate; it must not change publication or body.
    let mut published_doc = document.clone();
    published_doc["sections"][0]["children"][0]["children"][0]["children"][0]["props"]["text"] =
        json!("Published edit A");
    assert_eq!(
        put(2).json(&published_doc).send().await.unwrap().status(),
        StatusCode::PRECONDITION_FAILED
    );
    let response = put(3).json(&published_doc).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["etag"], "\"v4\"");
    assert!(
        http.get(&public)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
            .contains("Published edit A")
    );
    let stored = http
        .get(&entry)
        .header("Cookie", &editor_cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(stored["publication_status"], "published");
    assert_eq!(stored["values"]["body"], "Original text Page A");
    assert_eq!(stored["version"], 4);

    // Composite mode edits share CAS and preserve both content sources.
    let schema = http
        .get(format!(
            "{}/api/admin/content-types/page/effective-editor-config",
            site.base
        ))
        .header("Cookie", &editor_cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let token = schema["schema_token"].as_str().unwrap();
    let composite = |version: i64, mode: &str, status: &str| {
        http.put(format!("{entry}/composite"))
            .header("Cookie", &editor_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &editor_csrf)
            .json(&json!({
                "expected_version":version, "expected_schema_token":token,
                "title":"Page A","slug":"scope-a","values":{"body":"Original text Page A"},
                "content_mode":mode,"publication_status":status
            }))
    };
    assert_eq!(
        composite(3, "editor", "published")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PRECONDITION_FAILED
    );
    let response = composite(4, "editor", "published").send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["etag"], "\"v5\"");
    let html = http
        .get(&public)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(html.contains("Original text Page A"));
    assert!(!html.contains("Published edit A"));
    assert_eq!(
        http.get(&media).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    let retained = http
        .get(&a)
        .header("Cookie", &editor_cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(retained["document"], published_doc);
    assert_eq!(retained["content_mode"], "editor");
    let response = composite(5, "builder", "published").send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["etag"], "\"v6\"");
    assert!(
        http.get(&public)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
            .contains("Published edit A")
    );

    let response = reset(6)
        .json(&json!({"confirm":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["etag"], "\"v7\"");
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"entry_id":ids[0],"type_key":"page","content_mode":"editor","version":7,"document":null})
    );
    assert!(
        http.get(&public)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
            .contains("Original text Page A")
    );
    assert_eq!(
        http.get(&media).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        reset(7)
            .json(&json!({"confirm":true}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    // Re-create on the current entry version, then unpublish and publish via
    // the composite endpoint. Neither path changes the document.
    let response = put(7).json(&document).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["etag"], "\"v8\"");
    assert_eq!(
        publish(8, "draft").send().await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(
        http.get(&public).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        http.get(&media).send().await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    let response = composite(9, "builder", "published").send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["etag"], "\"v10\"");
    assert!(
        http.get(&public)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
            .contains("Private Draft A")
    );
    assert_eq!(
        http.get(&media).send().await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(
        http.get(&b)
            .header("Cookie", &editor_cookie)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap(),
        b_open
    );
}
