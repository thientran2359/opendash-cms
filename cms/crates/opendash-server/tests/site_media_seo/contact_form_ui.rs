use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, header, redirect::Policy};
use serde_json::{Value, json};

struct TestServer {
    child: Child,
    site_root: PathBuf,
    base_url: String,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.site_root);
    }
}

async fn spawn_server() -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let site_root = std::env::temp_dir().join(format!("opendash-m7-s05-t02-{nanos}"));
    fs::create_dir_all(&site_root).unwrap();

    let bin = env!("CARGO_BIN_EXE_opendash");
    let child = Command::new(bin)
        .env("OPENDASH_SITE_ROOT", &site_root)
        .env("OPENDASH_PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn opendash server");

    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::new();
    for _ in 0..60 {
        if let Ok(resp) = client.get(format!("{base_url}/api/health")).send().await
            && resp.status().is_success()
        {
            return TestServer {
                child,
                site_root,
                base_url,
            };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server did not become healthy at {base_url}");
}

fn extract_cookies(headers: &header::HeaderMap) -> String {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

async fn setup_admin(client: &Client, base_url: &str) -> (String, String) {
    let setup = client
        .post(format!("{base_url}/auth/setup"))
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert!(setup.status().is_success());
    let login = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);
    let cookie = extract_cookies(login.headers());
    let csrf = login.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    (cookie, csrf)
}

fn extract_hidden_value(html: &str, name: &str) -> Option<String> {
    let needle = format!("name=\"{name}\" value=\"");
    let start = html.find(&needle)? + needle.len();
    let end = html[start..].find('"')? + start;
    Some(html[start..end].to_string())
}

// S03 public Home is selected from a published Page through Reading Settings.
// Keep this setup through HTTP so the tests exercise publication and CAS.
async fn create_s03_home(client: &Client, base: &str, cookie: &str, csrf: &str) -> i64 {
    let created = client
        .post(format!("{base}/api/admin/content-types/page/entries"))
        .header("Cookie", cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .json(&json!({"title":"S03 Home","slug":"s03-home","values":{"body":"Home"}}))
        .send()
        .await
        .unwrap();
    let status = created.status();
    let text = created.text().await.unwrap();
    assert_eq!(status.as_u16(), 201, "create S03 Page: {text}");
    let entry: Value = serde_json::from_str(&text).unwrap();
    let id = entry["id"].as_i64().unwrap();
    let publish = client
        .post(format!(
            "{base}/api/admin/content-types/page/entries/{id}/publication"
        ))
        .header("Cookie", cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header(
            "If-Match",
            format!("\"v{}\"", entry["version"].as_i64().unwrap()),
        )
        .json(&json!({"status":"published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(publish.status().as_u16(), 200, "publish Home Page");
    let reading = client
        .get(format!("{base}/api/admin/settings/reading"))
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(reading.status().as_u16(), 200);
    let token = reading.headers()["etag"].to_str().unwrap().to_owned();
    let selected = client
        .put(format!("{base}/api/admin/settings/reading"))
        .header("Cookie", cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header("If-Match", token)
        .json(&json!({"homepage_page_id":id,"posts_page_id":null}))
        .send()
        .await
        .unwrap();
    let status = selected.status();
    let text = selected.text().await.unwrap();
    assert_eq!(status.as_u16(), 200, "save S03 Reading: {text}");
    id
}

async fn save_s03_home_doc(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    page_id: i64,
    version: i64,
    document: &Value,
) -> reqwest::Response {
    client
        .put(format!("{base}/api/admin/entries/page/{page_id}/builder"))
        .header("Cookie", cookie)
        .header("Origin", base)
        .header("X-CSRF-Token", csrf)
        .header("If-Match", format!("\"v{version}\""))
        .json(document)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn builder_contact_form_preview_live_csp_and_nojs_submit() {
    let srv = spawn_server().await;
    let client = Client::builder().redirect(Policy::none()).build().unwrap();
    let (cookie, csrf) = setup_admin(&client, &srv.base_url).await;

    // 1. Create a contact form
    let form_resp: Value = client
        .post(format!("{}/api/admin/contact/forms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "tu-van",
            "title": "Tu van",
            "schema": {
                "version": 1,
                "reply_to_field": "email",
                "fields": [
                    { "key": "name", "label": "Ho ten", "kind": "text", "required": true },
                    { "key": "email", "label": "Email", "kind": "email", "required": true },
                    { "key": "message", "label": "Noi dung", "kind": "textarea", "required": true }
                ]
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let form_id = form_resp["id"].as_i64().unwrap();
    let form_ver = form_resp["version"].as_i64().unwrap();

    // 2. Build a BuilderDocument with core/contact-form
    let doc = json!({
        "schema_version": 1,
        "blocks": [{
            "id": "sec-1",
            "type": "core/section",
            "children": [{
                "id": "row-1",
                "type": "core/row",
                "children": [{
                    "id": "col-1",
                    "type": "core/column",
                    "children": [{
                        "id": "el-form-1",
                        "type": "core/contact-form",
                        "props": {
                            "form_id": form_id.to_string(),
                            "submit_label": "Gui ngay"
                        }
                    }]
                }]
            }]
        }]
    });

    let home_id = create_s03_home(&client, &srv.base_url, &cookie, &csrf).await;

    // Preview with unsaved draft: must be inert (<fieldset disabled>, no action, no nonce)
    let preview_resp = client
        .post(format!(
            "{}/api/admin/entries/page/{home_id}/builder/preview",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&doc)
        .send()
        .await
        .unwrap();
    assert_eq!(preview_resp.status(), 200);
    let preview_html = preview_resp.json::<Value>().await.unwrap()["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(preview_html.contains("<fieldset disabled>"));
    assert!(!preview_html.contains("action=\"/api/public/contact/"));
    assert!(!preview_html.contains("name=\"nonce\""));

    // Before Save, public / still has no active contact form & CSP has form-action 'none'
    let home_before = client
        .get(format!("{}/", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(home_before.status(), 200);
    let csp_before = home_before
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(csp_before.contains("form-action 'none'"));

    // 3. Save the published Reading Page's own Builder document.
    let hp_put = save_s03_home_doc(&client, &srv.base_url, &cookie, &csrf, home_id, 2, &doc).await;
    assert_eq!(hp_put.status(), 200);

    // 4. Live homepage now renders active <form> with nonce and CSP form-action 'self'
    let home_live = client
        .get(format!("{}/", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(home_live.status(), 200);
    let csp_live = home_live
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(csp_live.contains("form-action 'self'"));
    assert!(csp_live.contains("script-src 'self' 'unsafe-inline'"));

    let live_html = home_live.text().await.unwrap();
    assert!(live_html.contains(&format!("action=\"/api/public/contact/{form_id}\"")));
    assert!(live_html.contains("<label for=\"cf_el-form-1_name\">Ho ten</label>"));
    assert!(live_html.contains("<button type=\"submit\">Gui ngay</button>"));

    let rendered_ver = extract_hidden_value(&live_html, "form_version").unwrap();
    let rendered_nonce = extract_hidden_value(&live_html, "nonce").unwrap();
    assert_eq!(rendered_ver, form_ver.to_string());
    assert_eq!(rendered_nonce.len(), 64);

    // 5. Submit using extracted form_version + nonce (simulating no-JS HTML submit)
    let submit_resp = client
        .post(format!("{}/api/public/contact/{form_id}", srv.base_url))
        .header(header::ORIGIN, &srv.base_url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!(
            "form_version={rendered_ver}&nonce={rendered_nonce}&name=Khach&email=khach@example.com&message=Xin+chao"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(submit_resp.status(), 303);
    assert_eq!(
        submit_resp.headers().get(header::LOCATION).unwrap(),
        "/api/public/contact/received"
    );

    // 6. Disable form -> homepage shows unavailable fallback and CSP reverts to form-action 'none'
    let upd_disabled = client
        .put(format!(
            "{}/api/admin/contact/forms/{form_id}",
            srv.base_url
        ))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{form_ver}\""))
        .json(&json!({
            "slug": "tu-van",
            "title": "Tu van",
            "status": "disabled",
            "schema": form_resp["schema"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(upd_disabled.status(), 200);

    let home_disabled = client
        .get(format!("{}/", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(home_disabled.status(), 200);
    let csp_disabled = home_disabled
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(csp_disabled.contains("form-action 'none'"));
    let html_disabled = home_disabled.text().await.unwrap();
    assert!(html_disabled.contains("builder-contact-unavailable"));
    assert!(!html_disabled.contains("<form "));
}
