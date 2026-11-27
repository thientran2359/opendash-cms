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
    json!({"schema_version":1,"blocks":[{"id":"section","type":"core/section","children":[{"id":"row","type":"core/row","children":[{"id":"column","type":"core/column","children":[{"id":"heading","type":"core/heading","props":{"level":"h2","text":title}}]}]}]}]})
}

// The Builder GET/PUT/Reset contract now includes content_entries.updated_at.
// Keep the original exact payload comparison while requiring a real timestamp.
fn without_updated_at(mut response: Value) -> Value {
    let timestamp = response["updated_at"]
        .as_str()
        .expect("Builder response must include updated_at");
    assert!(!timestamp.is_empty());
    response.as_object_mut().unwrap().remove("updated_at");
    response
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
    let public = format!("{}/scope-a", site.base);
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
    assert_eq!(initial["entry_id"], ids[0]);
    assert_eq!(initial["type_key"], "page");
    assert_eq!(initial["content_mode"], "editor");
    assert_eq!(initial["version"], 1);
    assert_eq!(
        initial["document"]["blocks"][0]["props"]["content"],
        "Original text Page A"
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
    document["blocks"][0]["children"][0]["children"][0]["children"]
        .as_array_mut()
        .unwrap()
        .push(
            json!({"id":"image","type":"core/image","props":{"image":image_id,"alt":"Page image"}}),
        );
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
        json!({"id":"slot","type":"core/content-slot","props":{}}),
        json!({"id":"part","type":"core/template-part","props":{"part_key":"header"}}),
        json!({"id":"bad-media","type":"core/image","props":{"image":"missing-image","alt":"Missing"}}),
    ] {
        let mut invalid = document.clone();
        invalid["blocks"][0]["children"][0]["children"][0]["children"]
            .as_array_mut()
            .unwrap()
            .push(element);
        assert_eq!(
            put(1).json(&invalid).send().await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }
    let mut legacy_bound = design("Fallback");
    legacy_bound["blocks"][0]["children"][0]["children"][0]["children"][0]["props"]["binding"] =
        json!({"source":{"kind":"route_context","field_key":"title"},"target":"text"});
    assert_eq!(
        preview(&preview_path)
            .json(&legacy_bound)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let result = preview(&preview_path).json(&document).send().await.unwrap();
    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(result.headers()["cache-control"], "no-store");
    assert!(result.headers().contains_key("content-security-policy"));
    let html = result.text().await.unwrap();
    assert!(html.contains("Private Draft A"));
    assert!(!html.contains("Page B"));
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
    let b_without_ts = without_updated_at(b_open.clone());
    assert_eq!(b_without_ts["entry_id"], ids[1]);
    assert_eq!(b_without_ts["type_key"], "page");
    assert_eq!(b_without_ts["content_mode"], "editor");
    assert_eq!(b_without_ts["version"], 1);
    assert_eq!(
        b_without_ts["document"]["blocks"][0]["props"]["content"],
        "Original text Page B"
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
    published_doc["blocks"][0]["children"][0]["children"][0]["children"][0]["props"]["text"] =
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
    let stored_body = stored["values"]["body"].as_str().unwrap();
    assert!(opendash_core::builder_blocks::contains_blocks(stored_body));
    assert!(stored_body.contains("Published edit A"));
    assert_eq!(stored["version"], 4);

    // Unified `values_json["body"]`: Classic composite edit and UX Builder edit share the same CAS and field.
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
    let composite = |version: i64, mode: &str, status: &str, values: Value| {
        http.put(format!("{entry}/composite"))
            .header("Cookie", &editor_cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &editor_csrf)
            .json(&json!({
                "expected_version":version, "expected_schema_token":token,
                "title":"Page A","slug":"scope-a","values":values,
                "content_mode":mode,"publication_status":status
            }))
    };
    assert_eq!(
        composite(
            3,
            "editor",
            "published",
            json!({"body":"Original text Page A"})
        )
        .send()
        .await
        .unwrap()
        .status(),
        StatusCode::PRECONDITION_FAILED
    );
    let response = composite(
        4,
        "editor",
        "published",
        json!({"body":"Original text Page A"}),
    )
    .send()
    .await
    .unwrap();
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
    let wrapped_after_classic = http
        .get(&a)
        .header("Cookie", &editor_cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        wrapped_after_classic["document"]["blocks"][0]["props"]["content"],
        "Original text Page A"
    );
    assert_eq!(wrapped_after_classic["content_mode"], "editor");

    // Re-save Builder design (`v5 -> v6`), verify public render & media ACL, then reset (`v6 -> v7`).
    let response = put(5).json(&published_doc).send().await.unwrap();
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
    assert_eq!(
        http.get(&media).send().await.unwrap().status(),
        StatusCode::OK
    );

    let response = reset(6)
        .json(&json!({"confirm":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["etag"], "\"v7\"");
    let reset_open = response.json::<Value>().await.unwrap();
    assert_eq!(reset_open["entry_id"], ids[0]);
    assert_eq!(reset_open["type_key"], "page");
    assert_eq!(reset_open["content_mode"], "editor");
    assert_eq!(reset_open["version"], 7);
    assert_eq!(
        reset_open["document"]["blocks"][0]["props"]["content"],
        "Published edit A"
    );
    let flattened_entry: Value = http
        .get(&entry)
        .header("Cookie", &editor_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(flattened_entry["values"]["body"], "Published edit A");
    let public_after_reset = http
        .get(&public)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(public_after_reset.contains("Published edit A"));
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
    // the composite endpoint without overwriting `body`.
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
    let response = composite(9, "builder", "published", json!({}))
        .send()
        .await
        .unwrap();
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
