#[path = "acf_test_helpers.rs"]
mod helpers;

use helpers::{RunningServer, TempDirGuard, api_json, build_zip, login};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

#[tokio::test]
async fn studio_preview_transient_options_pages_live_updates_and_fail_closes() {
    let dir = TempDirGuard::new("studio-opt-preview");
    let srv = RunningServer::start(&dir.0).await;
    let client = Client::new();
    let setup = client
        .post(format!("{}/auth/setup", srv.base))
        .json(&json!({"username":"admin","password":"Password123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let (admin, csrf) = login(&client, &srv.base, "admin", "Password123!").await;

    let db = SqlitePool::connect_with(
        SqliteConnectOptions::new().filename(dir.0.join("database/opendash.sqlite")),
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO site_locales(code,bcp47_tag,label,direction,is_default,enabled,position)
         VALUES('en','en','English','ltr',0,1,2) ON CONFLICT(code) DO UPDATE SET enabled=1",
    )
    .execute(&db)
    .await
    .unwrap();

    let manifest = serde_json::to_vec(&json!({
        "manifest_version": 3,
        "theme_id": "studio-opt-theme",
        "name": "Studio Options Preview Theme",
        "version": "1.0.0",
        "templates": [
            {"kind": "home", "file": "home.html"},
            {"kind": "index", "file": "index.html"}
        ],
        "declarations": {
            "options_pages": [{
                "page_key": "brand_banner",
                "title": "Brand Banner",
                "menu_label": "Brand Banner",
                "parent_menu": "appearance",
                "min_edit_role": "admin"
            }],
            "field_groups": [
                {
                    "local_key": "brand_banner_public",
                    "title": "Banner Public",
                    "location": [[{"param": "options_page", "op": "eq", "value": "custom:brand_banner"}]],
                    "fields": [
                        {"key": "headline", "label": "Headline", "type": "text", "public": true, "translatable": true},
                        {"key": "fixed_code", "label": "Fixed Code", "type": "text", "public": true, "translatable": false},
                        {"key": "internal_note", "label": "Internal Note", "type": "text", "public": false},
                        {
                            "key": "cta_group",
                            "label": "CTA",
                            "type": "group",
                            "public": true,
                            "fields": [
                                {"key": "label", "label": "Label", "type": "text", "acl": {"public_exposure": "public"}},
                                {"key": "admin_token", "label": "Admin Token", "type": "text", "acl": {"min_view_role": "admin", "min_edit_role": "admin", "public_exposure": "private"}}
                            ]
                        }
                    ]
                },
                {
                    "local_key": "brand_banner_admin_only",
                    "title": "Banner Admin Only",
                    "location": [[
                        {"param": "options_page", "op": "eq", "value": "custom:brand_banner"},
                        {"param": "user_role", "op": "eq", "value": "admin"}
                    ]],
                    "fields": [
                        {"key": "admin_flag", "label": "Admin Flag", "type": "text", "public": true}
                    ]
                },
                {
                    "local_key": "site_global_extra",
                    "title": "Global Extra",
                    "location": [[{"param": "options_page", "op": "eq", "value": "site_global"}]],
                    "fields": [
                        {"key": "promo_bar", "label": "Promo Bar", "type": "text", "public": true}
                    ]
                }
            ]
        }
    }))
    .unwrap();
    let home_html = b"<main><h1>{{ custom_options.brand_banner.headline | default('NONE') }}</h1><p>{{ custom_options.brand_banner.cta_group.label | default('') }}</p><aside>{{ site_options.promo_bar | default('NO-PROMO') }}</aside></main>";
    let zip = build_zip(&[
        ("theme.json", &manifest),
        ("templates/home.html", home_html),
        ("templates/index.html", home_html),
    ]);
    let up = client
        .post(format!("{}/api/admin/themes/upload", srv.base))
        .header("Cookie", &admin)
        .header("X-CSRF-Token", &csrf)
        .header("Content-Type", "application/zip")
        .body(zip)
        .send()
        .await
        .unwrap();
    assert_eq!(
        up.status(),
        StatusCode::CREATED,
        "{}",
        up.text().await.unwrap()
    );
    let up_body: Value = up.json().await.unwrap();
    let rev_id = up_body["revision_id"].as_str().unwrap();
    let act = api_json(
        &client,
        reqwest::Method::POST,
        &format!("{}/api/admin/themes/studio-opt-theme/activate", srv.base),
        &admin,
        Some(&csrf),
        None,
        Some(json!({"revision_id": rev_id})),
    )
    .await;
    assert_eq!(act.status(), StatusCode::OK);

    // 1. Verify `public_previewable` metadata distinguishes public vs admin-only groups.
    let vals_res = api_json(
        &client,
        reqwest::Method::GET,
        &format!("{}/api/admin/options-pages/brand_banner/values", srv.base),
        &admin,
        None,
        None,
        None,
    )
    .await;
    assert_eq!(vals_res.status(), StatusCode::OK);
    let vals_json: Value = vals_res.json().await.unwrap();
    let groups = vals_json["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    let pub_grp = groups
        .iter()
        .find(|g| g["title"] == "Banner Public")
        .unwrap();
    let adm_grp = groups
        .iter()
        .find(|g| g["title"] == "Banner Admin Only")
        .unwrap();
    assert_eq!(pub_grp["public_previewable"], Value::Bool(true));
    assert_eq!(adm_grp["public_previewable"], Value::Bool(false));

    let preview_url = format!("{}/api/admin/studio/preview", srv.base);
    let preview = |payload: Value| {
        api_json(
            &client,
            reqwest::Method::POST,
            &preview_url,
            &admin,
            Some(&csrf),
            None,
            Some(payload),
        )
    };

    // 2. Live preview renders unsaved transient custom_options & site_global extra group without persisting.
    let ok_res = preview(json!({
        "target": {"kind": "home"},
        "transient": {
            "options_pages": {
                "custom:brand_banner": {
                    "headline": "Draft Flash Sale",
                    "cta_group": {"label": "Buy Now"}
                },
                "site_global": {
                    "promo_bar": "Free Shipping Today"
                }
            }
        }
    }))
    .await;
    assert_eq!(ok_res.status(), StatusCode::OK);
    let ok_html = ok_res.json::<Value>().await.unwrap()["html"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        ok_html.contains("Draft Flash Sale"),
        "preview missing transient custom option: {ok_html}"
    );
    assert!(
        ok_html.contains("Buy Now"),
        "preview missing transient group child: {ok_html}"
    );
    assert!(
        ok_html.contains("Free Shipping Today"),
        "preview missing transient site_global extra: {ok_html}"
    );

    // Confirm nothing was written to DB.
    let after_res = api_json(
        &client,
        reqwest::Method::GET,
        &format!("{}/api/admin/options-pages/brand_banner/values", srv.base),
        &admin,
        None,
        None,
        None,
    )
    .await;
    let after_json: Value = after_res.json().await.unwrap();
    assert_eq!(
        after_json["values"]["headline"],
        Value::String(String::new())
    );

    // 3. Fail-closed 422 matrix.
    let invalid_cases = vec![
        (
            "unknown page",
            json!({"target": {"kind": "home"}, "transient": {"options_pages": {"custom:does_not_exist": {"headline": "X"}}}}),
        ),
        (
            "inactive theme",
            json!({"target": {"kind": "home"}, "transient": {"options_pages": {"theme:other-theme": {"x": "1"}}}}),
        ),
        (
            "private field",
            json!({"target": {"kind": "home"}, "transient": {"options_pages": {"custom:brand_banner": {"internal_note": "leak"}}}}),
        ),
        (
            "admin-only group field",
            json!({"target": {"kind": "home"}, "transient": {"options_pages": {"custom:brand_banner": {"admin_flag": "leak"}}}}),
        ),
        (
            "nested private child",
            json!({"target": {"kind": "home"}, "transient": {"options_pages": {"custom:brand_banner": {"cta_group": {"label": "ok", "admin_token": "secret"}}}}}),
        ),
        (
            "non-translatable on secondary locale",
            json!({"target": {"kind": "home"}, "preview_locale": "en", "transient": {"options_pages": {"custom:brand_banner": {"fixed_code": "EN-CODE"}}}}),
        ),
        (
            "colliding built-in site_global key",
            json!({"target": {"kind": "home"}, "transient": {"options_pages": {"site_global": {"contact_phone": "123"}}}}),
        ),
    ];
    for (label, payload) in invalid_cases {
        let res = preview(payload).await;
        assert_eq!(
            res.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "case `{label}` must return 422, got {}",
            res.status()
        );
    }
}
