#[path = "acf_test_helpers.rs"]
mod helpers;

use helpers::{RunningServer, TempDirGuard, api_json, build_zip, login, upload_media};
use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

#[tokio::test]
async fn cuj1_cuj3_cuj4_acf_end_to_end_api_public_seo_and_acl() {
    let dir = TempDirGuard::new("e2e");
    let srv = RunningServer::start(&dir.0).await;
    let client = Client::new();
    let setup = client
        .post(format!("{}/auth/setup", srv.base))
        .json(&json!({"username":"admin","password":"Password123!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let (admin, acsrf) = login(&client, &srv.base, "admin", "Password123!").await;

    let ed_mk = api_json(
        &client,
        reqwest::Method::POST,
        &format!("{}/api/admin/users", srv.base),
        &admin,
        Some(&acsrf),
        None,
        Some(json!({"username":"ed1","password":"EditorPassword123!","role":"editor"})),
    )
    .await;
    assert_eq!(ed_mk.status(), StatusCode::CREATED);
    let (editor, ecsrf) = login(&client, &srv.base, "ed1", "EditorPassword123!").await;

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

    let theme_manifest = serde_json::to_vec(&json!({
        "manifest_version": 1,
        "theme_id": "acf-e2e-theme",
        "name": "ACF Theme",
        "version": "1.0.0",
        "templates": [
            {"kind": "page", "file": "page.html"},
            {"kind": "single", "file": "single.html"},
            {"kind": "archive", "file": "archive.html"},
            {"kind": "taxonomy", "taxonomy": "theme_topic", "file": "taxonomy-theme_topic.html"},
            {"kind": "index", "file": "index.html"}
        ],
        "declarations": {
            "taxonomy_definitions": [{
                "key": "theme_topic", "label": "Theme Topic", "hierarchical": false, "public": true
            }],
            "options_pages": [{
                "page_key": "theme_opts", "title": "Theme Info", "menu_label": "Theme Info",
                "parent_menu": "settings", "min_edit_role": "editor"
            }],
            "field_groups": [
                {
                    "local_key": "theme_topic_meta",
                    "title": "Theme Topic Meta",
                    "location": [[{"param": "taxonomy_term", "op": "eq", "value": "theme_topic"}]],
                    "fields": [
                        {"key": "banner_img", "label": "Banner", "type": "media", "media_kind": "image", "acl": {"public_exposure": "public"}},
                        {"key": "accent_color", "label": "Accent", "type": "color", "acl": {"public_exposure": "public"}}
                    ]
                },
                {
                    "local_key": "theme_opts_fields",
                    "title": "Theme Opts Fields",
                    "location": [[{"param": "options_page", "op": "eq", "value": "custom:theme_opts"}]],
                    "fields": [
                        {"key": "hotline", "label": "Hotline", "type": "text", "public": true, "translatable": true},
                        {"key": "logo", "label": "Logo", "type": "media", "media_kind": "image", "public": true, "translatable": true},
                        {"key": "secret_key", "label": "Secret", "type": "text", "public": false, "acl": {"min_view_role":"admin","min_edit_role":"admin","public_exposure":"private"}}
                    ]
                }
            ]
        }
    }))
    .unwrap();
    let zip = build_zip(&[
        ("theme.json", &theme_manifest),
        ("page.html", b"<main data-hotline='{{ custom_options.theme_opts.hotline | default(\"\") }}'>{% if custom_options.theme_opts.logo %}{{ od_image(custom_options.theme_opts.logo) }}{% endif %}{{ od_content() }}</main>"),
        ("single.html", b"<article data-order='{{ entry.menu_order }}' data-excerpt='{{ entry.excerpt }}'><h1>{{ entry.title }}</h1>{% if entry.parent %}<a href='{{ entry.parent.url }}'>{{ entry.parent.title }}</a>{% endif %}{{ od_content() }}</article>"),
        ("archive.html", b"<section>{% for e in entries %}<div class='item'>{{ e.title }}</div>{% endfor %}</section>"),
        ("taxonomy-theme_topic.html", b"<section data-desc='{{ term.description }}' data-sort='{{ term.sort_order }}' data-color='{{ term.fields.accent_color | default(\"\") }}'>{% if term.featured_image %}{{ od_image(term.featured_image) }}{% endif %}{% if term.fields.banner_img %}{{ od_image(term.fields.banner_img) }}{% endif %}</section>"),
        ("index.html", b"<main>Index</main>"),
    ]);
    let t_inst = client
        .post(format!("{}/api/admin/themes", srv.base))
        .header(header::COOKIE, &admin)
        .header(header::ORIGIN, &srv.base)
        .header("X-CSRF-Token", &acsrf)
        .header("Content-Type", "application/zip")
        .body(zip)
        .send()
        .await
        .unwrap();
    let t_status = t_inst.status();
    let t_text = t_inst.text().await.unwrap();
    assert_eq!(
        t_status,
        StatusCode::CREATED,
        "theme install failed: {t_text}"
    );
    let rev_id = serde_json::from_str::<Value>(&t_text).unwrap()["revision_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let t_act = api_json(
        &client,
        reqwest::Method::PUT,
        &format!("{}/api/admin/themes/active", srv.base),
        &admin,
        Some(&acsrf),
        Some("\"v1\""),
        Some(json!({ "revision_id": rev_id })),
    )
    .await;
    assert_eq!(t_act.status(), StatusCode::OK);

    // CUJ 1: Temp CPT delete guard + Hierarchical CPT `docs`
    let tmp_ct = api_json(
        &client,
        reqwest::Method::POST,
        &format!("{}/api/admin/content-types", srv.base),
        &admin,
        Some(&acsrf),
        None,
        Some(json!({"key":"temp_cuj1","label":"Temp"})),
    )
    .await;
    assert_eq!(tmp_ct.status(), StatusCode::CREATED);
    let tmp_e = api_json(
        &client,
        reqwest::Method::POST,
        &format!("{}/api/admin/content-types/temp_cuj1/entries", srv.base),
        &admin,
        Some(&acsrf),
        None,
        Some(json!({"title":"T","slug":"t","values":{}})),
    )
    .await;
    let tmp_eid = tmp_e.json::<Value>().await.unwrap()["id"].as_i64().unwrap();
    let del_busy = api_json(
        &client,
        reqwest::Method::DELETE,
        &format!("{}/api/admin/content-types/temp_cuj1", srv.base),
        &admin,
        Some(&acsrf),
        Some("\"v1\""),
        None,
    )
    .await;
    assert_eq!(del_busy.status(), StatusCode::CONFLICT);
    let trash_tmp = api_json(
        &client,
        reqwest::Method::POST,
        &format!(
            "{}/api/admin/content-types/temp_cuj1/entries/{tmp_eid}/trash",
            srv.base
        ),
        &admin,
        Some(&acsrf),
        Some("\"v1\""),
        Some(json!({})),
    )
    .await;
    assert_eq!(trash_tmp.status(), StatusCode::OK);
    let tmp_v2 = trash_tmp.json::<Value>().await.unwrap()["version"]
        .as_i64()
        .unwrap();
    let purge_tmp = client
        .delete(format!(
            "{}/api/admin/content-types/temp_cuj1/entries/{tmp_eid}/purge",
            srv.base
        ))
        .header(header::COOKIE, &admin)
        .header(header::ORIGIN, &srv.base)
        .header("X-CSRF-Token", &acsrf)
        .header(header::IF_MATCH, format!("\"v{tmp_v2}\""))
        .header("X-Confirm-Purge", "true")
        .send()
        .await
        .unwrap();
    assert_eq!(purge_tmp.status(), StatusCode::NO_CONTENT);
    let del_ok = api_json(
        &client,
        reqwest::Method::DELETE,
        &format!("{}/api/admin/content-types/temp_cuj1", srv.base),
        &admin,
        Some(&acsrf),
        Some("\"v1\""),
        None,
    )
    .await;
    assert_eq!(del_ok.status(), StatusCode::OK);

    let docs_ct = api_json(
        &client, reqwest::Method::POST, &format!("{}/api/admin/content-types", srv.base),
        &admin, Some(&acsrf), None, Some(json!({
            "key": "docs",
            "label": "Docs",
            "singular_label": "Doc",
            "rewrite_base": "tai-lieu",
            "settings": {
                "public": true, "has_archive": true, "publication": "immediate",
                "supports": ["editor", "featured_image", "excerpt", "menu_order", "custom_template"],
                "hierarchical": true, "menu_icon": "book-open",
                "archive_per_page": 1, "default_sort": "menu_order_asc",
                "exclude_from_sitemap": true,
                "list_columns": [{"kind": "field", "key": "difficulty"}, {"kind": "menu_order", "key": ""}]
            }
        })),
    ).await;
    assert_eq!(docs_ct.status(), StatusCode::CREATED);
    let docs_fg = api_json(
        &client, reqwest::Method::POST, &format!("{}/api/admin/content-field-groups", srv.base),
        &admin, Some(&acsrf), None, Some(json!({
            "local_key": "docs_meta",
            "title": "Docs Meta",
            "location": [[
                {"param": "content_type", "op": "eq", "value": "docs"},
                {"param": "user_role", "op": "eq", "value": "admin"}
            ]],
            "presentation": {"region":"sidebar","order":15,"style":"seamless","label_placement":"left","collapsed_by_default":true,"collapsible":true},
            "fields": [{"key":"difficulty","label":"Difficulty","type":"text","required":true,"default":"basic","acl":{"min_view_role":"admin","min_edit_role":"admin","public_exposure":"private"}}]
        })),
    ).await;
    assert_eq!(docs_fg.status(), StatusCode::CREATED);
    let ed_docs_cfg: Value = api_json(
        &client,
        reqwest::Method::GET,
        &format!(
            "{}/api/admin/content-types/docs/effective-editor-config",
            srv.base
        ),
        &editor,
        None,
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    let ed_docs_tok = ed_docs_cfg["schema_token"].as_str().unwrap().to_owned();
    let ed_forbid = api_json(
        &client, reqwest::Method::POST, &format!("{}/api/admin/content-types/docs/entries/composite", srv.base),
        &editor, Some(&ecsrf), None, Some(json!({"expected_schema_token":&ed_docs_tok,"title":"Bad","slug":"bad","values":{"difficulty":"x"}})),
    ).await;
    assert_eq!(ed_forbid.status(), StatusCode::FORBIDDEN);
    let parent_resp = api_json(
        &client, reqwest::Method::POST, &format!("{}/api/admin/content-types/docs/entries/composite", srv.base),
        &editor, Some(&ecsrf), None, Some(json!({"expected_schema_token":&ed_docs_tok,"title":"Cha Tai Lieu","slug":"cha-tl","menu_order":2,"values":{"body":"<p>P</p>","excerpt":"P ex"}})),
    ).await;
    assert_eq!(parent_resp.status(), StatusCode::CREATED);
    let parent_json: Value = parent_resp.json().await.unwrap();
    let parent_id = parent_json["entry"]["id"].as_i64().unwrap();
    let child_resp = api_json(
        &client, reqwest::Method::POST, &format!("{}/api/admin/content-types/docs/entries/composite", srv.base),
        &editor, Some(&ecsrf), None, Some(json!({"expected_schema_token":&ed_docs_tok,"title":"Con Tai Lieu","slug":"con-tl","parent_id":parent_id,"menu_order":1,"values":{"body":"<p>C</p>","excerpt":"Tom tat con"}})),
    ).await;
    assert_eq!(child_resp.status(), StatusCode::CREATED);
    let trash_parent = api_json(
        &client,
        reqwest::Method::POST,
        &format!(
            "{}/api/admin/content-types/docs/entries/{parent_id}/trash",
            srv.base
        ),
        &admin,
        Some(&acsrf),
        Some("\"v1\""),
        Some(json!({})),
    )
    .await;
    assert_eq!(trash_parent.status(), StatusCode::CONFLICT);
    assert!(
        trash_parent
            .text()
            .await
            .unwrap()
            .contains("cannot_trash_entry_with_children")
    );

    let pub_child = client
        .get(format!("{}/tai-lieu/con-tl", srv.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        pub_child.contains("Cha Tai Lieu")
            && (pub_child.contains("/tai-lieu/cha-tl")
                || pub_child.contains("&#x2f;tai-lieu&#x2f;cha-tl"))
            && pub_child.contains("data-order='1'")
            && pub_child.contains("Tom tat con"),
        "pub_child mismatch: {pub_child}"
    );
    let pub_arch = client
        .get(format!("{}/tai-lieu", srv.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        pub_arch.contains("Con Tai Lieu") && !pub_arch.contains("Cha Tai Lieu"),
        "archive_per_page=1 + menu_order_asc"
    );
    let sitemap = client
        .get(format!("{}/sitemap.xml", srv.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!sitemap.contains("/tai-lieu/"));

    // CUJ 3: User taxonomy `topic` settings + sync_entry_terms + Term Meta & SEO public + media ACL
    let banner_img = upload_media(&client, &srv.base, &admin, &acsrf, "banner.png").await;
    let feat_img = upload_media(&client, &srv.base, &admin, &acsrf, "feat.png").await;
    let topic_def = api_json(
        &client, reqwest::Method::POST, &format!("{}/api/admin/taxonomy/definitions", srv.base),
        &admin, Some(&acsrf), None, Some(json!({
            "key":"topic","label":"Topics","hierarchical":false,"public":true,"rewrite_base":"chu-de",
            "content_types":["post"],
            "settings":{"selection_mode":"single","picker_style":"select","required_on_publish":true,"editor_can_create_terms":true}
        })),
    ).await;
    assert_eq!(topic_def.status(), StatusCode::CREATED);
    let ed_term_create = api_json(
        &client, reqwest::Method::POST, &format!("{}/api/admin/taxonomy/terms", srv.base),
        &editor, Some(&ecsrf), None, Some(json!({"kind":"topic","name":"Huong Dan","slug":"huong-dan","description":"Mo ta chu de","sort_order":4})),
    ).await;
    assert_eq!(ed_term_create.status(), StatusCode::CREATED);
    let term_cycle = api_json(
        &client, reqwest::Method::POST, &format!("{}/api/admin/content-field-groups", srv.base),
        &admin, Some(&acsrf), None, Some(json!({
            "local_key":"cycle_fg","title":"Cycle",
            "location":[[{"param":"content_type","op":"eq","value":"post"},{"param":"entry_taxonomy","op":"eq","value":"topic:1"}]],
            "fields":[{"key":"t_sync","label":"Sync","type":"term_ref","taxonomy":"topic","sync_entry_terms":true}]
        })),
    ).await;
    assert_eq!(term_cycle.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let tt_cfg: Value = api_json(
        &client,
        reqwest::Method::GET,
        &format!(
            "{}/api/admin/taxonomy/definitions/theme_topic/effective-editor-config",
            srv.base
        ),
        &editor,
        None,
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    let tt_tok = tt_cfg["schema_token"].as_str().unwrap().to_owned();
    let tt_new = api_json(
        &client, reqwest::Method::POST, &format!("{}/api/admin/taxonomy/terms", srv.base),
        &admin, Some(&acsrf), None, Some(json!({
            "kind":"theme_topic","name":"Visual","slug":"visual","description":"Desc TT","sort_order":9,
            "featured_image_media_id":&feat_img,
            "values":{"banner_img":&banner_img,"accent_color":"#1188ff"},
            "schema_token":&tt_tok,
            "seo":{"title":"Visual SEO","noindex":false}
        })),
    ).await;
    assert_eq!(tt_new.status(), StatusCode::CREATED);
    let tt_html = client
        .get(format!("{}/theme_topic/visual", srv.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        tt_html.contains("Visual SEO")
            && tt_html.contains("data-desc='Desc TT'")
            && tt_html.contains("data-sort='9'")
            && tt_html.contains("#1188ff")
    );
    assert_eq!(
        client
            .get(format!("{}/media/{banner_img}/file", srv.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    // CUJ 4: Custom Options Page multilingual + Admin private preservation + public render
    let logo_img = upload_media(&client, &srv.base, &admin, &acsrf, "logo.png").await;
    let opt_adm = api_json(
        &client,
        reqwest::Method::PUT,
        &format!(
            "{}/api/admin/options-pages/theme_opts/values?locale=vi",
            srv.base
        ),
        &admin,
        Some(&acsrf),
        Some("\"v1\""),
        Some(json!({"values":{"secret_key":"ADMIN_PRIVATE_999"}})),
    )
    .await;
    assert_eq!(opt_adm.status(), StatusCode::OK);
    let opt_ed_forbid = api_json(
        &client,
        reqwest::Method::PUT,
        &format!(
            "{}/api/admin/options-pages/theme_opts/values?locale=vi",
            srv.base
        ),
        &editor,
        Some(&ecsrf),
        Some("\"v2\""),
        Some(json!({"values":{"secret_key":"HACK"}})),
    )
    .await;
    assert_eq!(opt_ed_forbid.status(), StatusCode::FORBIDDEN);
    let opt_ed_ok = api_json(
        &client,
        reqwest::Method::PUT,
        &format!(
            "{}/api/admin/options-pages/theme_opts/values?locale=vi",
            srv.base
        ),
        &editor,
        Some(&ecsrf),
        Some("\"v2\""),
        Some(json!({"values":{"hotline":"1900-8888","logo":&logo_img}})),
    )
    .await;
    assert_eq!(opt_ed_ok.status(), StatusCode::OK);
    let home_page = api_json(
        &client,
        reqwest::Method::POST,
        &format!("{}/api/admin/content-types/page/entries", srv.base),
        &admin,
        Some(&acsrf),
        None,
        Some(json!({"title":"Home","slug":"home-cuj4","values":{}})),
    )
    .await;
    assert_eq!(home_page.status(), StatusCode::CREATED);
    let home_json: Value = home_page.json().await.unwrap();
    let home_id = home_json["id"].as_i64().unwrap();
    let home_ver = home_json["version"].as_i64().unwrap();
    let home_pub = api_json(
        &client,
        reqwest::Method::POST,
        &format!(
            "{}/api/admin/content-types/page/entries/{home_id}/publication",
            srv.base
        ),
        &admin,
        Some(&acsrf),
        Some(&format!("\"v{home_ver}\"")),
        Some(json!({"status":"published"})),
    )
    .await;
    assert_eq!(home_pub.status(), StatusCode::OK);
    let reading_get = api_json(
        &client,
        reqwest::Method::GET,
        &format!("{}/api/admin/settings/reading", srv.base),
        &admin,
        None,
        None,
        None,
    )
    .await;
    let reading_etag = reading_get
        .headers()
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let reading_put = api_json(
        &client,
        reqwest::Method::PUT,
        &format!("{}/api/admin/settings/reading", srv.base),
        &admin,
        Some(&acsrf),
        Some(&reading_etag),
        Some(json!({"homepage_page_id":home_id})),
    )
    .await;
    assert_eq!(reading_put.status(), StatusCode::OK);
    let home_html = client
        .get(format!("{}/", srv.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        home_html.contains("data-hotline='1900-8888'") && !home_html.contains("ADMIN_PRIVATE_999"),
        "home_html mismatch: {home_html}"
    );
    assert_eq!(
        client
            .get(format!("{}/media/{logo_img}/file", srv.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let opt_adm_read: Value = api_json(
        &client,
        reqwest::Method::GET,
        &format!(
            "{}/api/admin/options-pages/theme_opts/values?locale=vi",
            srv.base
        ),
        &admin,
        None,
        None,
        None,
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(opt_adm_read["values"]["secret_key"], "ADMIN_PRIVATE_999");
}
