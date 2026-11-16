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

#[tokio::test]
async fn builder_contact_form_preview_live_csp_and_nojs_submit() {
    let srv = spawn_server().await;
    let client = Client::builder()
        .redirect(Policy::none())
        .build()
        .unwrap();
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

    // 2. Build a BuilderDocument with core/contact_form
    let doc = json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec-1",
            "children": [{
                "id": "row-1",
                "children": [{
                    "id": "col-1",
                    "children": [{
                        "id": "el-form-1",
                        "type": "core/contact_form",
                        "props": {
                            "form_id": form_id,
                            "submit_label": "Gui ngay"
                        }
                    }]
                }]
            }]
        }]
    });

    // Preview with unsaved draft: must be inert (<fieldset disabled>, no action, no nonce)
    let preview_resp = client
        .post(format!("{}/api/admin/builder/homepage/preview", srv.base_url))
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
    let home_before = client.get(format!("{}/", srv.base_url)).send().await.unwrap();
    assert_eq!(home_before.status(), 200);
    let csp_before = home_before
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(csp_before.contains("form-action 'none'"));

    // 3. Save homepage draft + publish
    let hp_get = client
        .get(format!("{}/api/admin/builder/homepage", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let hp_etag = hp_get
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let hp_put = client
        .put(format!("{}/api/admin/builder/homepage", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, hp_etag)
        .json(&doc)
        .send()
        .await
        .unwrap();
    assert_eq!(hp_put.status(), 200);

    // 4. Live homepage now renders active <form> with nonce and CSP form-action 'self'
    let home_live = client.get(format!("{}/", srv.base_url)).send().await.unwrap();
    assert_eq!(home_live.status(), 200);
    let csp_live = home_live
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(csp_live.contains("form-action 'self'"));
    assert!(csp_live.contains("script-src 'none'"));

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
        .put(format!("{}/api/admin/contact/forms/{form_id}", srv.base_url))
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

    let home_disabled = client.get(format!("{}/", srv.base_url)).send().await.unwrap();
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
