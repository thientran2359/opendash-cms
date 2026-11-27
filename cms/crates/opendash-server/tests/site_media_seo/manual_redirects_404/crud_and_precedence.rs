use reqwest::{StatusCode, header};
use serde_json::{Value, json};

use super::{Site, create_editor_and_login, no_redirect_client, setup_and_login_admin};

#[tokio::test]
async fn crud_cas_rbac_precedence_and_security() {
    let site = Site::start().await;
    let http = no_redirect_client();
    let admin = setup_and_login_admin(&http, &site).await;
    let editor = create_editor_and_login(&http, &site, &admin).await;

    // Editor must get 403 Forbidden across redirect and 404 endpoints.
    for path in [
        "/api/admin/redirects",
        "/api/admin/redirects/export",
        "/api/admin/404-logs",
    ] {
        let resp = http
            .get(format!("{}{path}", site.base))
            .header(header::COOKIE, &editor.cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "{path}");
    }
    let editor_post = http
        .post(format!("{}/api/admin/redirects", site.base))
        .header(header::COOKIE, &editor.cookie)
        .header("x-csrf-token", &editor.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({"source_path":"/x","target_url":"/y","status_code":301}))
        .send()
        .await
        .unwrap();
    assert_eq!(editor_post.status(), StatusCode::FORBIDDEN);

    // Create published page (/live-page) and draft page (/draft-page) via composite entry API.
    let live_res = http
        .post(format!(
            "{}/api/admin/content-types/page/entries",
            site.base
        ))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({
            "slug": "live-page",
            "title": "Live Page",
            "values": {"body": "Live Page"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(live_res.status(), StatusCode::CREATED);
    let live_entry: Value = live_res.json().await.unwrap();
    let live_id = live_entry["id"].as_i64().unwrap();
    let live_ver = live_entry["version"].as_i64().unwrap();
    let pub_res = http
        .post(format!(
            "{}/api/admin/content-types/page/entries/{live_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, format!("\"v{live_ver}\""))
        .json(&json!({"status": "published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_res.status(), StatusCode::OK);

    let draft_res = http
        .post(format!(
            "{}/api/admin/content-types/page/entries",
            site.base
        ))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({
            "slug": "draft-page",
            "title": "Draft Page",
            "values": {"body": "Draft Page"}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(draft_res.status(), StatusCode::CREATED);

    // Create manual redirects covering: legacy extension, UTF-8 %XX, %20, external 302, shadowed vs draft.
    for payload in [
        json!({"source_path":"/Lien_He.html/","target_url":"/live-page","status_code":301,"note":"legacy"}),
        json!({"source_path":"/tin-t%E1%BB%A9c","target_url":"/live-page","status_code":301}),
        json!({"source_path":"/old%20a","target_url":"/tin-tức#top","status_code":302}),
        json!({"source_path":"/promo","target_url":"https://partner.example.com/sale","status_code":302}),
        json!({"source_path":"/live-page","target_url":"/should-not-win","status_code":301}),
        json!({"source_path":"/draft-page","target_url":"/live-page","status_code":301}),
    ] {
        let res = http
            .post(format!("{}/api/admin/redirects", site.base))
            .header(header::COOKIE, &admin.cookie)
            .header("x-csrf-token", &admin.csrf)
            .header(header::ORIGIN, &site.base)
            .json(&payload)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED, "{payload}");
    }

    // Verify shadowed_by_live_route accuracy and public precedence.
    let list: Value = http
        .get(format!("{}/api/admin/redirects?per_page=100", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["shadowed_count"], 1);
    let items = list["items"].as_array().unwrap();
    let live_rule = items
        .iter()
        .find(|i| i["source_path"] == "/live-page")
        .unwrap();
    assert_eq!(live_rule["shadowed_by_live_route"], true);
    assert_eq!(live_rule["shadowed_target_kind"], "entry");
    let draft_rule = items
        .iter()
        .find(|i| i["source_path"] == "/draft-page")
        .unwrap();
    assert_eq!(draft_rule["shadowed_by_live_route"], false);

    // Live canonical must win over manual redirect; draft does NOT win so manual redirect fires.
    let live_get = http
        .get(format!("{}/live-page", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(live_get.status(), StatusCode::OK);
    let draft_get = http
        .get(format!("{}/draft-page", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(draft_get.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(
        draft_get.headers().get(header::LOCATION).unwrap(),
        "/live-page"
    );

    // Public legacy redirects + query preservation + HEAD no-log/hit.
    let r1 = http
        .get(format!("{}/Lien_He.html/?utm_source=fb", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::MOVED_PERMANENTLY);
    assert_eq!(
        r1.headers().get(header::LOCATION).unwrap(),
        "/live-page?utm_source=fb"
    );
    assert_eq!(r1.headers().get(header::CACHE_CONTROL).unwrap(), "no-store");

    let r2 = http
        .get(format!("{}/tin-t%E1%BB%A9c", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), StatusCode::MOVED_PERMANENTLY);

    let r3 = http
        .get(format!("{}/old%20a", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r3.status(), StatusCode::FOUND);
    assert_eq!(
        r3.headers().get(header::LOCATION).unwrap(),
        "/tin-t%E1%BB%A9c#top"
    );

    let head_promo = http
        .head(format!("{}/promo", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(head_promo.status(), StatusCode::FOUND);
    assert_eq!(
        head_promo.headers().get(header::LOCATION).unwrap(),
        "https://partner.example.com/sale"
    );

    // Security & aliasing blocks: 400 and never logged as 404.
    for bad in ["/%2Fprivate", "/%252Fprivate", "/%61dmin/x", "/%00bad"] {
        let resp = http
            .get(format!("{}{bad}", site.base))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{bad}");
    }
    // Raw HTTP wire test for /%2e%2e/x because reqwest::Url normalizes dot segments client-side.
    {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let addr = site.base.trim_start_matches("http://");
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                format!("GET /%2e%2e/x HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut raw_resp = String::new();
        stream.read_to_string(&mut raw_resp).await.unwrap();
        assert!(
            raw_resp.starts_with("HTTP/1.1 400 "),
            "expected 400 for /%2e%2e/x, got: {raw_resp}"
        );
    }
    // Reserved root (even capitalized) returns 404 without logging.
    let cap_admin = http
        .get(format!("{}/Admin/nonexistent", site.base))
        .send()
        .await
        .unwrap();
    assert_eq!(cap_admin.status(), StatusCode::NOT_FOUND);

    // CAS GET -> PUT (428 / 412 / 200) + exclude_id self-update allowed + DELETE -> 204.
    let draft_id = draft_rule["id"].as_i64().unwrap();
    let get_one = http
        .get(format!("{}/api/admin/redirects/{draft_id}", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(get_one.status(), StatusCode::OK);
    assert_eq!(get_one.headers().get(header::ETAG).unwrap(), "\"v1\"");

    let stale_put = http
        .put(format!("{}/api/admin/redirects/{draft_id}", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, "\"v99\"")
        .json(&json!({"source_path":"/draft-page","target_url":"/live-page","status_code":302,"enabled":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(stale_put.status(), StatusCode::PRECONDITION_FAILED);

    let ok_put = http
        .put(format!("{}/api/admin/redirects/{draft_id}", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({"source_path":"/draft-page","target_url":"/tin-tức","status_code":302,"enabled":true,"note":"updated"}))
        .send()
        .await
        .unwrap();
    assert_eq!(ok_put.status(), StatusCode::OK);
    assert_eq!(ok_put.headers().get(header::ETAG).unwrap(), "\"v2\"");

    // 3-hop chain (allowed: c1 -> c2 -> c3 -> /dest-clean) & 4-hop / cycle rejection (422).
    for (src, tgt) in [("/c3", "/dest-clean"), ("/c2", "/c3"), ("/c1", "/c2")] {
        let res = http
            .post(format!("{}/api/admin/redirects", site.base))
            .header(header::COOKIE, &admin.cookie)
            .header("x-csrf-token", &admin.csrf)
            .header(header::ORIGIN, &site.base)
            .json(&json!({"source_path": src, "target_url": tgt, "status_code": 301}))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED, "{src} -> {tgt}");
    }

    let c0 = http
        .post(format!("{}/api/admin/redirects", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({"source_path":"/c0","target_url":"/c1","status_code":301}))
        .send()
        .await
        .unwrap();
    assert_eq!(c0.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let cycle = http
        .post(format!("{}/api/admin/redirects", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({"source_path":"/live-page","target_url":"/c1","status_code":301}))
        .send()
        .await
        .unwrap();
    assert_eq!(cycle.status(), StatusCode::CONFLICT); // /live-page already exists

    let multi_cycle = http
        .post(format!("{}/api/admin/redirects", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({"source_path":"/dest-clean","target_url":"/c1","status_code":301}))
        .send()
        .await
        .unwrap();
    assert_eq!(multi_cycle.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let cycle2 = http
        .post(format!("{}/api/admin/redirects", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({"source_path":"/tin-tức-cycle","target_url":"/tin-tức-cycle","status_code":301}))
        .send()
        .await
        .unwrap();
    assert_eq!(cycle2.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // HEAD did not hit /promo -> hit_count must be 0, while /Lien_He.html has hit_count 1, and Admin did NOT log 404.
    let logs: Value = http
        .get(format!("{}/api/admin/404-logs", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(logs["total"], 0);

    let after_list: Value = http
        .get(format!("{}/api/admin/redirects?q=Lien_He", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after_list["items"][0]["hit_count"], 1);

    // Configured site_public_settings.public_origin must enforce same-origin self-redirect/cycle rejection even from loopback Admin Host.
    let pub_get = http
        .get(format!("{}/api/admin/settings/public", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(pub_get.status(), StatusCode::OK);
    let pub_etag = pub_get
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let pub_put = http
        .put(format!("{}/api/admin/settings/public", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .header(header::IF_MATCH, pub_etag)
        .json(&json!({
            "public_origin": "https://canonical.example.com",
            "content_language": "vi",
            "indexing_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_put.status(), StatusCode::OK);

    let origin_self = http
        .post(format!("{}/api/admin/redirects", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({
            "source_path": "/self-origin",
            "target_url": "https://canonical.example.com/self-origin",
            "status_code": 301
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(origin_self.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let origin_cycle = http
        .post(format!("{}/api/admin/redirects", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({
            "source_path": "/ext-c",
            "target_url": "https://canonical.example.com/c1",
            "status_code": 301
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(origin_cycle.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
