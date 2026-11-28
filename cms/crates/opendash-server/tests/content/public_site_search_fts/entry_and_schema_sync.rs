use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};

use super::{Site, fts_docsize_ids, fts_match_ids, schema_token, setup_and_login_admin};

#[tokio::test]
async fn s01_writer_hooks_sync_fts5_across_entry_lifecycle_and_schema_mutations() {
    let site = Site::start("s01-fts-sync").await;
    let client = Client::new();
    let (cookie, csrf) = setup_and_login_admin(&client, &site.base).await;
    let db = site.db().await;

    // 1. Draft post excluded; publish indexes it; update refreshes FTS5.
    let p_create = client
        .post(format!(
            "{}/api/admin/content-types/post/entries",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"Alpha Draft","slug":"alpha-post","values":{"body":"alpha_tok"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(p_create.status(), StatusCode::CREATED);
    let post_id = p_create.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    assert!(fts_docsize_ids(&db).await.is_empty());

    let pub_resp = client
        .post(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/publication",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v1\"")
        .json(&json!({"status":"published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(pub_resp.status(), StatusCode::OK);
    assert_eq!(fts_match_ids(&db, "\"alpha_tok\"").await, vec![post_id]);

    let p_update = client
        .put(format!(
            "{}/api/admin/content-types/post/entries/{post_id}",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({"title":"Đường dẫn mới","slug":"alpha-post","values":{"body":"beta_tok"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(p_update.status(), StatusCode::OK);
    assert!(fts_match_ids(&db, "\"alpha_tok\"").await.is_empty());
    assert_eq!(
        fts_match_ids(&db, "\"duong\" \"beta_tok\"").await,
        vec![post_id]
    );

    // 2. Quick Edit metadata patch (title + unpublish + republish) syncs FTS5.
    let post_tok = schema_token(&client, &site.base, &cookie, "post").await;
    let qe_unpub = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v3\"")
        .json(&json!({"expected_schema_token": post_tok, "title": "Kiến trúc QE", "publication_status": "draft"}))
        .send()
        .await
        .unwrap();
    assert_eq!(qe_unpub.status(), StatusCode::OK);
    assert!(fts_docsize_ids(&db).await.is_empty());

    let qe_repub = client
        .patch(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/metadata",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v4\"")
        .json(&json!({"expected_schema_token": post_tok, "publication_status": "published"}))
        .send()
        .await
        .unwrap();
    assert_eq!(qe_repub.status(), StatusCode::OK);
    assert_eq!(fts_match_ids(&db, "\"kien\" \"truc\"").await, vec![post_id]);

    // 3. Trash -> Restore (draft) -> Publish -> Trash -> Purge lifecycle sync.
    for (method, path, ver, extra) in [
        (
            "POST",
            format!(
                "{}/api/admin/content-types/post/entries/{post_id}/trash",
                site.base
            ),
            "\"v5\"",
            None,
        ),
        (
            "POST",
            format!(
                "{}/api/admin/content-types/post/entries/{post_id}/restore",
                site.base
            ),
            "\"v6\"",
            None,
        ),
    ] {
        let _ = method;
        let r = client
            .post(path)
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, ver)
            .json(&extra.unwrap_or_else(|| json!({})))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert!(fts_docsize_ids(&db).await.is_empty());
    }

    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/post/entries/{post_id}/publication",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v7\"")
            .json(&json!({"status":"published"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(fts_docsize_ids(&db).await, vec![post_id]);

    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/post/entries/{post_id}/trash",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v8\"")
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let purge = client
        .delete(format!(
            "{}/api/admin/content-types/post/entries/{post_id}/purge",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v9\"")
        .header("X-Confirm-Purge", "true")
        .send()
        .await
        .unwrap();
    assert_eq!(purge.status(), StatusCode::NO_CONTENT);
    assert!(fts_docsize_ids(&db).await.is_empty());

    // 4. Page Builder save/reset + Custom CPT + Taxonomy & Term + FieldGroup + CPT public toggle.
    let pg = client
        .post(format!(
            "{}/api/admin/content-types/page/entries",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"Trang Builder","slug":"trang-builder","values":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(pg.status(), StatusCode::CREATED);
    let page_id = pg.json::<Value>().await.unwrap()["id"].as_i64().unwrap();
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/page/entries/{page_id}/publication",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({"status":"published"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let b_put = client
        .put(format!("{}/api/admin/entries/page/{page_id}/builder", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v2\"")
        .json(&json!({
            "schema_version": 1,
            "blocks": [{
                "id": "section",
                "block_id": "core/section",
                "props": {},
                "children": [{
                    "id": "row",
                    "block_id": "core/row",
                    "props": {},
                    "children": [{
                        "id": "column",
                        "block_id": "core/column",
                        "props": {},
                        "children": [{"id": "heading", "block_id": "core/heading", "props": {"level": "h2", "text": "Khối ĐườngBuilder"}}]
                    }]
                }]
            }]
        }))
        .send()
        .await
        .unwrap();
    let b_put_status = b_put.status();
    let b_put_txt = b_put.text().await.unwrap();
    assert_eq!(b_put_status, StatusCode::OK, "b_put failed: {b_put_txt}");
    assert_eq!(fts_match_ids(&db, "\"duongbuilder\"").await, vec![page_id]);

    let b_put2 = client
        .put(format!("{}/api/admin/entries/page/{page_id}/builder", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v3\"")
        .json(&json!({
            "schema_version": 1,
            "blocks": [{
                "id": "section",
                "block_id": "core/section",
                "props": {},
                "children": [{
                    "id": "row",
                    "block_id": "core/row",
                    "props": {},
                    "children": [{
                        "id": "column",
                        "block_id": "core/column",
                        "props": {},
                        "children": [{"id": "heading", "block_id": "core/heading", "props": {"level": "h2", "text": "Khối ĐườngThayThế"}}]
                    }]
                }]
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(b_put2.status(), StatusCode::OK);
    assert!(fts_match_ids(&db, "\"duongbuilder\"").await.is_empty());
    assert_eq!(fts_match_ids(&db, "\"duongthaythe\"").await, vec![page_id]);

    let b_reset = client
        .post(format!(
            "{}/api/admin/entries/page/{page_id}/builder/reset",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, "\"v4\"")
        .json(&json!({"confirm": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(b_reset.status(), StatusCode::OK);
    assert_eq!(fts_match_ids(&db, "\"duongthaythe\"").await, vec![page_id]);

    assert_eq!(
        client
            .post(format!("{}/api/admin/content-types", site.base))
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({
                "key": "guide",
                "label": "Guide",
                "settings": {"public": true, "has_archive": true, "publication": "immediate", "supports": ["editor"]},
                "fields": [{"key": "body", "label": "Body", "type": "richtext"}]
            }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        client
            .post(format!("{}/api/admin/taxonomy/definitions", site.base))
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({"key":"topic","label":"Topic","hierarchical":false,"public":true}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        client
            .put(format!(
                "{}/api/admin/taxonomy/eligibility/guide",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({"taxonomies":["topic"],"expected_version":1}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    let term_res = client
        .post(format!("{}/api/admin/taxonomy/terms", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"kind":"topic","name":"Đô thị Cổ","slug":"do-thi-co","locale":"vi"}))
        .send()
        .await
        .unwrap();
    assert_eq!(term_res.status(), StatusCode::CREATED);
    let topic_tid = term_res.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    assert_eq!(
        client
            .post(format!("{}/api/admin/content-field-groups", site.base))
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .json(&json!({
                "local_key": "guide_extra",
                "title": "Extra",
                "target_types": ["guide"],
                "fields": [{"key":"fg_note","label":"FG Note","type":"text","acl":{"public_exposure":"public"}}]
            }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );

    let guide_tok = schema_token(&client, &site.base, &cookie, "guide").await;
    let g_res = client
        .post(format!(
            "{}/api/admin/content-types/guide/entries/composite",
            site.base
        ))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({
            "expected_schema_token": guide_tok,
            "title": "Guide One",
            "slug": "guide-one",
            "locale": "vi",
            "publication_status": "published",
            "term_ids": [topic_tid],
            "values": {"body":"<p>guide_body_tok</p>","fg_note":"fg_public_tok"}
        }))
        .send()
        .await
        .unwrap();
    let g_status = g_res.status();
    let g_body: Value = g_res.json().await.unwrap();
    assert_eq!(g_status, StatusCode::CREATED, "composite create: {g_body}");
    let guide_id = g_body["entry"]["id"].as_i64().unwrap();
    assert_eq!(fts_match_ids(&db, "\"do\" \"thi\"").await, vec![guide_id]);
    assert_eq!(
        fts_match_ids(&db, "\"fg_public_tok\"").await,
        vec![guide_id]
    );

    assert_eq!(
        client
            .patch(format!(
                "{}/api/admin/taxonomy/terms/{topic_tid}",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({"name":"Khu phố Mới"}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(fts_match_ids(&db, "\"do\" \"thi\"").await.is_empty());
    assert_eq!(fts_match_ids(&db, "\"khu\" \"pho\"").await, vec![guide_id]);

    assert_eq!(
        client
            .patch(format!(
                "{}/api/admin/taxonomy/definitions/topic",
                site.base
            ))
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({"public":false}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(fts_match_ids(&db, "\"khu\" \"pho\"").await.is_empty());

    assert_eq!(
        client
            .put(format!("{}/api/admin/content-field-groups/user:guide_extra", site.base))
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v1\"")
            .json(&json!({
                "local_key": "guide_extra",
                "title": "Extra",
                "target_types": ["guide"],
                "fields": [{"key":"fg_note","label":"FG Note","type":"text","acl":{"public_exposure":"private"}}]
            }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(fts_match_ids(&db, "\"fg_public_tok\"").await.is_empty());
    assert_eq!(
        fts_match_ids(&db, "\"guide_body_tok\"").await,
        vec![guide_id]
    );

    // Toggle CPT public: false -> drops guide; true -> restores guide
    assert_eq!(
        client
            .put(format!("{}/api/admin/content-types/guide", site.base))
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v2\"")
            .json(&json!({
                "key": "guide",
                "label": "Guide",
                "settings": {"public": false, "has_archive": false, "publication": "immediate", "supports": ["editor"]},
                "fields": [{"key": "body", "label": "Body", "type": "richtext"}]
            }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(fts_match_ids(&db, "\"guide_body_tok\"").await.is_empty());
    assert_eq!(
        client
            .put(format!("{}/api/admin/content-types/guide", site.base))
            .header(header::COOKIE, &cookie)
            .header("Origin", &site.base)
            .header("X-CSRF-Token", &csrf)
            .header(header::IF_MATCH, "\"v3\"")
            .json(&json!({
                "key": "guide",
                "label": "Guide",
                "settings": {"public": true, "has_archive": true, "publication": "immediate", "supports": ["editor"]},
                "fields": [{"key": "body", "label": "Body", "type": "richtext"}]
            }))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        fts_match_ids(&db, "\"guide_body_tok\"").await,
        vec![guide_id]
    );
}
