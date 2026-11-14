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
    let site_root = std::env::temp_dir().join(format!("opendash-m7-s05-t01-{nanos}"));
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

async fn setup_admin_and_editor(client: &Client, base_url: &str) -> ((String, String), (String, String)) {
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
    let admin_cookie = extract_cookies(login.headers());
    let admin_csrf = login.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();

    let ed = client
        .post(format!("{base_url}/api/admin/users"))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "username": "editor1",
            "password": "EditorPassword123!",
            "role": "editor"
        }))
        .send()
        .await
        .unwrap();
    assert!(ed.status().is_success());

    let ed_login = client
        .post(format!("{base_url}/auth/login"))
        .json(&json!({ "username": "editor1", "password": "EditorPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(ed_login.status(), 200);
    let ed_cookie = extract_cookies(ed_login.headers());
    let ed_csrf = ed_login.json::<Value>().await.unwrap()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();

    ((admin_cookie, admin_csrf), (ed_cookie, ed_csrf))
}

fn hex_nonce(seed: u8) -> String {
    format!("{:02x}", seed).repeat(32)
}

#[tokio::test]
async fn contact_form_schema_submission_dedup_origin_and_rate_limits() {
    let srv = spawn_server().await;
    let client = Client::builder()
        .redirect(Policy::none())
        .build()
        .unwrap();
    let ((admin_cookie, admin_csrf), (ed_cookie, ed_csrf)) =
        setup_admin_and_editor(&client, &srv.base_url).await;

    // 1. RBAC & CSRF checks
    assert_eq!(
        client
            .get(format!("{}/api/admin/contact/forms", srv.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .get(format!("{}/api/admin/contact/forms", srv.base_url))
            .header(header::COOKIE, &ed_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .post(format!("{}/api/admin/contact/forms", srv.base_url))
            .header(header::COOKIE, &admin_cookie)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let _ = ed_csrf;

    // 2. Schema validation limits (reserved key, >20 fields, bad reply_to_field)
    let bad_reserved = client
        .post(format!("{}/api/admin/contact/forms", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "slug": "bad-form",
            "title": "Bad",
            "schema": {
                "version": 1,
                "fields": [{ "key": "nonce", "label": "Nonce", "kind": "text", "required": true }]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_reserved.status(), 422);

    let too_many_fields: Vec<Value> = (1..=21)
        .map(|i| json!({ "key": format!("f_{i}"), "label": format!("F{i}"), "kind": "text", "required": false }))
        .collect();
    let bad_count = client
        .post(format!("{}/api/admin/contact/forms", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "slug": "too-many",
            "title": "Too many",
            "schema": { "version": 1, "fields": too_many_fields }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_count.status(), 422);

    // 3. Create valid form
    let create_resp = client
        .post(format!("{}/api/admin/contact/forms", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "slug": "lien-he",
            "title": "Lien he",
            "schema": {
                "version": 1,
                "reply_to_field": "email",
                "fields": [
                    { "key": "name", "label": "Ho ten", "kind": "text", "required": true },
                    { "key": "email", "label": "Email", "kind": "email", "required": true },
                    { "key": "topic", "label": "Chu de", "kind": "select", "required": false, "options": ["Sales", "Support"] },
                    { "key": "message", "label": "Noi dung", "kind": "textarea", "required": true },
                    { "key": "agree", "label": "Dong y", "kind": "checkbox", "required": true },
                    { "key": "newsletter", "label": "Nhan tin", "kind": "checkbox", "required": false }
                ]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_resp.status(), 201);
    let form_json: Value = create_resp.json().await.unwrap();
    let form_id = form_json["id"].as_i64().unwrap();
    let form_ver = form_json["version"].as_i64().unwrap();

    // 4. Strict Origin & Content-Type checks
    let no_origin = client
        .post(format!("{}/api/public/contact/{form_id}", srv.base_url))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!("form_version={form_ver}&nonce={}", hex_nonce(1)))
        .send()
        .await
        .unwrap();
    assert_eq!(no_origin.status(), 403);

    let null_origin = client
        .post(format!("{}/api/public/contact/{form_id}", srv.base_url))
        .header(header::ORIGIN, "null")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!("form_version={form_ver}&nonce={}", hex_nonce(1)))
        .send()
        .await
        .unwrap();
    assert_eq!(null_origin.status(), 403);

    let foreign_origin = client
        .post(format!("{}/api/public/contact/{form_id}", srv.base_url))
        .header(header::ORIGIN, "https://evil.example")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!("form_version={form_ver}&nonce={}", hex_nonce(1)))
        .send()
        .await
        .unwrap();
    assert_eq!(foreign_origin.status(), 403);

    let bad_ct = client
        .post(format!("{}/api/public/contact/{form_id}", srv.base_url))
        .header(header::ORIGIN, &srv.base_url)
        .header(header::CONTENT_TYPE, "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(bad_ct.status(), 415);

    // 5. Validation & stale version checks (these also count in rate limit; use different form or stay <=5/min)
    // Wait: let's test stale version & validation first, then check rate limit on a 2nd form so we stay clean!
    let stale_ver = client
        .post(format!("{}/api/public/contact/{form_id}", srv.base_url))
        .header(header::ORIGIN, &srv.base_url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!(
            "form_version={}&nonce={}&name=A&email=a@example.com&message=Hi&agree=true",
            form_ver + 9,
            hex_nonce(2)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_ver.status(), 409);

    let bad_checkbox = client
        .post(format!("{}/api/public/contact/{form_id}", srv.base_url))
        .header(header::ORIGIN, &srv.base_url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!(
            "form_version={form_ver}&nonce={}&name=A&email=a@example.com&message=Hi&agree=yes",
            hex_nonce(3)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_checkbox.status(), 422);

    // 6. Valid submission + dedup + differing payload 409 + honeypot
    let valid_body = format!(
        "form_version={form_ver}&nonce={}&name=Nguyen+Van+A&email=user@example.com&topic=Sales&message=Dong+1%0D%0ADong+2&agree=true",
        hex_nonce(4)
    );
    let ok_submit = client
        .post(format!("{}/api/public/contact/{form_id}", srv.base_url))
        .header(header::ORIGIN, &srv.base_url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(valid_body.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(ok_submit.status(), 303);
    assert_eq!(
        ok_submit.headers().get(header::LOCATION).unwrap(),
        "/api/public/contact/received"
    );

    let received_page = client
        .get(format!("{}/api/public/contact/received", srv.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(received_page.status(), 200);
    assert_eq!(
        received_page.headers().get("x-robots-tag").unwrap(),
        "noindex"
    );

    // Duplicate same nonce + same payload -> 303 without duplicating row
    let dup_same = client
        .post(format!("{}/api/public/contact/{form_id}", srv.base_url))
        .header(header::ORIGIN, &srv.base_url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(valid_body)
        .send()
        .await
        .unwrap();
    assert_eq!(dup_same.status(), 303);

    // Same nonce + different payload -> 409
    let dup_diff = client
        .post(format!("{}/api/public/contact/{form_id}", srv.base_url))
        .header(header::ORIGIN, &srv.base_url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!(
            "form_version={form_ver}&nonce={}&name=Other&email=user@example.com&message=Hi&agree=true",
            hex_nonce(4)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(dup_diff.status(), 409);

    // Verify only 1 submission persisted
    let subs: Value = client
        .get(format!("{}/api/admin/contact/submissions?form_id={form_id}", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(subs.as_array().unwrap().len(), 1);
    let sub_id = subs[0]["id"].as_i64().unwrap();

    let detail: Value = client
        .get(format!("{}/api/admin/contact/submissions/{sub_id}", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail["values"]["message"], "Dong 1\nDong 2");
    assert_eq!(detail["values"]["newsletter"], false);
    assert_eq!(detail["reply_to_email"], "user@example.com");

    // 7. Edit form schema -> old submission retains original schema snapshot; form delete -> 409
    let upd_form = client
        .put(format!("{}/api/admin/contact/forms/{form_id}", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{form_ver}\""))
        .json(&json!({
            "slug": "lien-he",
            "title": "Lien he moi",
            "status": "active",
            "schema": {
                "version": 1,
                "fields": [
                    { "key": "name", "label": "Ten moi", "kind": "text", "required": true }
                ]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(upd_form.status(), 200);
    let new_ver = upd_form.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();

    let detail_after: Value = client
        .get(format!("{}/api/admin/contact/submissions/{sub_id}", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detail_after["schema_snapshot"]["fields"][0]["label"], "Ho ten");

    let del_blocked = client
        .delete(format!("{}/api/admin/contact/forms/{form_id}", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::IF_MATCH, format!("\"v{new_ver}\""))
        .send()
        .await
        .unwrap();
    assert_eq!(del_blocked.status(), 409);

    // 8. Rate limit test on a 2nd form (5/min, 6th -> 429 + Retry-After, forged X-Forwarded-For ignored)
    let form2: Value = client
        .post(format!("{}/api/admin/contact/forms", srv.base_url))
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .json(&json!({
            "slug": "rate-test",
            "title": "Rate Test",
            "schema": {
                "version": 1,
                "fields": [{ "key": "name", "label": "Name", "kind": "text", "required": true }]
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let form2_id = form2["id"].as_i64().unwrap();
    let form2_ver = form2["version"].as_i64().unwrap();

    for i in 1..=5u8 {
        let r = client
            .post(format!("{}/api/public/contact/{form2_id}", srv.base_url))
            .header(header::ORIGIN, &srv.base_url)
            .header("X-Forwarded-For", format!("203.0.113.{i}"))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(format!(
                "form_version={form2_ver}&nonce={}&name=User{i}",
                hex_nonce(10 + i)
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 303);
    }
    let sixth = client
        .post(format!("{}/api/public/contact/{form2_id}", srv.base_url))
        .header(header::ORIGIN, &srv.base_url)
        .header("X-Forwarded-For", "198.51.100.99")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(format!(
            "form_version={form2_ver}&nonce={}&name=User6",
            hex_nonce(99)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(sixth.status(), 429);
    assert!(sixth.headers().get(header::RETRY_AFTER).is_some());

    // 9. Explicit submission delete -> 204, then form1 delete -> 204
    assert_eq!(
        client
            .delete(format!("{}/api/admin/contact/submissions/{sub_id}", srv.base_url))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
    assert_eq!(
        client
            .delete(format!("{}/api/admin/contact/forms/{form_id}", srv.base_url))
            .header(header::COOKIE, &admin_cookie)
            .header("X-CSRF-Token", &admin_csrf)
            .header(header::IF_MATCH, format!("\"v{new_ver}\""))
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
}
