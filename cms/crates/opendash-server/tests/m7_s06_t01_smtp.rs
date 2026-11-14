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
    let site_root = std::env::temp_dir().join(format!("opendash-m7-s06-t01-{nanos}"));
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

#[tokio::test]
async fn smtp_settings_secrets_cas_receipt_preservation_and_manual_retry() {
    let srv = spawn_server().await;
    let client = Client::builder()
        .redirect(Policy::none())
        .build()
        .unwrap();
    let (cookie, csrf) = setup_admin(&client, &srv.base_url).await;

    // 1. Initial GET returns has_password=false and no secret fields
    let get1 = client
        .get(format!("{}/api/admin/settings/smtp", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(get1.status(), 200);
    let v1_json: Value = get1.json().await.unwrap();
    assert_eq!(v1_json["enabled"], false);
    assert_eq!(v1_json["has_password"], false);
    assert!(v1_json.get("secret_ref").is_none());
    assert!(v1_json.get("password").is_none());
    let ver1 = v1_json["version"].as_i64().unwrap();

    // Reject invalid host & CRLF mailbox
    let bad_host = client
        .put(format!("{}/api/admin/settings/smtp", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver1}\""))
        .json(&json!({
            "enabled": false,
            "host": "https://smtp.example.com/path",
            "port": 587,
            "tls_mode": "starttls",
            "username": "u",
            "from_address": "a@example.com",
            "recipient_address": "b@example.com",
            "password_update": { "mode": "keep" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_host.status(), 400);

    // Unused closed port for deterministic transport failure
    let closed_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_port = closed_listener.local_addr().unwrap().port();
    drop(closed_listener);

    // 2. Save valid SMTP config + replace password -> creates private secret blob
    let put1 = client
        .put(format!("{}/api/admin/settings/smtp", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver1}\""))
        .json(&json!({
            "enabled": true,
            "host": "127.0.0.1",
            "port": closed_port,
            "tls_mode": "implicit_tls",
            "username": "mailer",
            "from_address": "noreply@example.com",
            "recipient_address": "admin@example.com",
            "password_update": { "mode": "replace", "value": "secret-1" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put1.status(), 200);
    let put1_json: Value = put1.json().await.unwrap();
    assert_eq!(put1_json["enabled"], true);
    assert_eq!(put1_json["has_password"], true);
    assert!(put1_json.get("secret_ref").is_none());
    let ver2 = put1_json["version"].as_i64().unwrap();

    let secrets_dir = srv.site_root.join("database").join("secrets").join("smtp");
    let entries_after_put1: Vec<_> = fs::read_dir(&secrets_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries_after_put1.len(), 1);

    // Stale CAS with replace must return 409 and NOT leave an orphan secret blob
    let stale_put = client
        .put(format!("{}/api/admin/settings/smtp", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver1}\""))
        .json(&json!({
            "enabled": true,
            "host": "127.0.0.1",
            "port": closed_port,
            "tls_mode": "implicit_tls",
            "username": "mailer",
            "from_address": "noreply@example.com",
            "recipient_address": "admin@example.com",
            "password_update": { "mode": "replace", "value": "orphan-pw" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_put.status(), 409);
    let entries_after_stale: Vec<_> = fs::read_dir(&secrets_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries_after_stale.len(), 1);

    // 3. Create form, add to homepage, submit -> receipt preserved even when SMTP relay fails!
    let form_resp: Value = client
        .post(format!("{}/api/admin/contact/forms", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "slug": "lien-he",
            "title": "Lien he",
            "schema": {
                "version": 1,
                "reply_to_field": "email",
                "fields": [
                    { "key": "name", "label": "Ho ten", "kind": "text", "required": true },
                    { "key": "email", "label": "Email", "kind": "email", "required": true }
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
                        "props": { "form_id": form_id, "submit_label": "Gui" }
                    }]
                }]
            }]
        }]
    });
    let hp_get = client
        .get(format!("{}/api/admin/builder/homepage", srv.base_url))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let hp_etag = hp_get.headers().get(header::ETAG).unwrap().to_str().unwrap().to_owned();
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

    let home_html = client
        .get(format!("{}/", srv.base_url))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let nonce_marker = "name=\"nonce\" value=\"";
    let nonce_start = home_html.find(nonce_marker).unwrap() + nonce_marker.len();
    let nonce_end = nonce_start + home_html[nonce_start..].find('"').unwrap();
    let nonce = &home_html[nonce_start..nonce_end];

    tokio::time::sleep(Duration::from_millis(3100)).await;
    let submit_resp = client
        .post(format!("{}/api/public/contact/{form_id}", srv.base_url))
        .header(header::ORIGIN, &srv.base_url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!("form_version={form_ver}&nonce={nonce}&name=A&email=a@example.com"))
        .send()
        .await
        .unwrap();
    assert_eq!(submit_resp.status(), 303);

    // Wait briefly for initial async attempt to finish as unknown
    let mut sub_detail: Value = Value::Null;
    for _ in 0..40 {
        let subs: Vec<Value> = client
            .get(format!("{}/api/admin/contact/submissions", srv.base_url))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(subs.len(), 1);
        let sub_id = subs[0]["id"].as_i64().unwrap();
        let det: Value = client
            .get(format!("{}/api/admin/contact/submissions/{sub_id}", srv.base_url))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if det["notification_attempts"][0]["state"] == "unknown" {
            sub_detail = det;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_ne!(sub_detail, Value::Null);
    let sub_id = sub_detail["id"].as_i64().unwrap();
    let att1 = &sub_detail["notification_attempts"][0];
    assert_eq!(att1["state"], "unknown");
    assert!(att1.get("secret_ref").is_none());
    assert!(att1.get("transport_snapshot_json").is_none());
    let att1_ver = att1["version"].as_i64().unwrap();

    // 4. Manual retry without acknowledge_duplicate_risk=true -> 409
    let retry_no_ack = client
        .post(format!("{}/api/admin/contact/submissions/{sub_id}/notify", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{att1_ver}\""))
        .json(&json!({ "acknowledge_duplicate_risk": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(retry_no_ack.status(), 409);

    // Manual retry #2 and #3 with ack -> succeed in creating attempts up to max 3; 4th -> 409
    let retry2 = client
        .post(format!("{}/api/admin/contact/submissions/{sub_id}/notify", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{att1_ver}\""))
        .json(&json!({ "acknowledge_duplicate_risk": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(retry2.status(), 200);
    let att2: Value = retry2.json().await.unwrap();
    assert_eq!(att2["attempt_number"], 2);
    let att2_ver = att2["version"].as_i64().unwrap();

    let retry3 = client
        .post(format!("{}/api/admin/contact/submissions/{sub_id}/notify", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{att2_ver}\""))
        .json(&json!({ "acknowledge_duplicate_risk": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(retry3.status(), 200);
    let att3: Value = retry3.json().await.unwrap();
    assert_eq!(att3["attempt_number"], 3);
    let att3_ver = att3["version"].as_i64().unwrap();

    let retry4 = client
        .post(format!("{}/api/admin/contact/submissions/{sub_id}/notify", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{att3_ver}\""))
        .json(&json!({ "acknowledge_duplicate_risk": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(retry4.status(), 409);

    // 5. Test SMTP endpoint fails cleanly (502) against closed port, and mode=clear clears password
    let test_resp = client
        .post(format!("{}/api/admin/settings/smtp/test", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .send()
        .await
        .unwrap();
    assert_eq!(test_resp.status(), 502);

    let clear_put = client
        .put(format!("{}/api/admin/settings/smtp", srv.base_url))
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, format!("\"v{ver2}\""))
        .json(&json!({
            "enabled": false,
            "host": "127.0.0.1",
            "port": closed_port,
            "tls_mode": "implicit_tls",
            "username": "mailer",
            "from_address": "noreply@example.com",
            "recipient_address": "admin@example.com",
            "password_update": { "mode": "clear" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(clear_put.status(), 200);
    let clear_json: Value = clear_put.json().await.unwrap();
    assert_eq!(clear_json["has_password"], false);
}
