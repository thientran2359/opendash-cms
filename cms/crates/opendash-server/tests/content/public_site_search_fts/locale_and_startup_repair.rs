use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};

use super::{Site, fts_docsize_ids, fts_match_ids, setup_and_login_admin};

#[tokio::test]
async fn s01_locales_and_startup_repair_sync_fts5() {
    let mut site = Site::start("s01-fts-loc").await;
    let site_root = site.root.clone();
    let client = Client::new();
    let (cookie, csrf) = setup_and_login_admin(&client, &site.base).await;
    let db = site.db().await;

    // 1. Publish default VI post.
    let vi_resp = client
        .post(format!("{}/api/admin/content-types/post/entries", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"VI Pub","slug":"vi-pub","locale":"vi","values":{"body":"tok_vi_only"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(vi_resp.status(), StatusCode::CREATED);
    let vi_id = vi_resp.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/post/entries/{vi_id}/publication",
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

    // 2. Enable multilingual, create EN, publish EN post.
    let loc_get = client
        .get(format!("{}/api/admin/locales", site.base))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    let mut loc_etag = loc_get.headers()[header::ETAG].to_str().unwrap().to_owned();
    let ml_on = client
        .put(format!("{}/api/admin/locales/multilingual", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({"enabled":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(ml_on.status(), StatusCode::OK);
    loc_etag = ml_on.headers()[header::ETAG].to_str().unwrap().to_owned();

    let en_create = client
        .post(format!("{}/api/admin/locales", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({"code":"en","label":"English","direction":"ltr"}))
        .send()
        .await
        .unwrap();
    assert_eq!(en_create.status(), StatusCode::CREATED);
    loc_etag = en_create.headers()[header::ETAG]
        .to_str()
        .unwrap()
        .to_owned();

    let en_resp = client
        .post(format!("{}/api/admin/content-types/post/entries", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"EN Pub","slug":"en-pub","locale":"en","values":{"body":"tok_en_only"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(en_resp.status(), StatusCode::CREATED);
    let en_id = en_resp.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();
    assert_eq!(
        client
            .post(format!(
                "{}/api/admin/content-types/post/entries/{en_id}/publication",
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
    assert_eq!(fts_docsize_ids(&db).await, vec![vi_id, en_id]);

    // Disable EN -> removed; re-enable EN -> restored.
    let en_off = client
        .put(format!("{}/api/admin/locales/en", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({"label":"English","direction":"ltr","enabled":false,"position":1}))
        .send()
        .await
        .unwrap();
    assert_eq!(en_off.status(), StatusCode::OK);
    loc_etag = en_off.headers()[header::ETAG].to_str().unwrap().to_owned();
    assert_eq!(fts_docsize_ids(&db).await, vec![vi_id]);

    let en_on = client
        .put(format!("{}/api/admin/locales/en", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({"label":"English","direction":"ltr","enabled":true,"position":1}))
        .send()
        .await
        .unwrap();
    assert_eq!(en_on.status(), StatusCode::OK);
    loc_etag = en_on.headers()[header::ETAG].to_str().unwrap().to_owned();
    assert_eq!(fts_docsize_ids(&db).await, vec![vi_id, en_id]);

    // Multilingual toggle off -> drops EN; on -> restores EN.
    let ml_off = client
        .put(format!("{}/api/admin/locales/multilingual", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({"enabled":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(ml_off.status(), StatusCode::OK);
    assert_eq!(fts_docsize_ids(&db).await, vec![vi_id]);
    loc_etag = ml_off.headers()[header::ETAG].to_str().unwrap().to_owned();

    let ml_on2 = client
        .put(format!("{}/api/admin/locales/multilingual", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({"enabled":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(ml_on2.status(), StatusCode::OK);
    assert_eq!(fts_docsize_ids(&db).await, vec![vi_id, en_id]);
    loc_etag = ml_on2.headers()[header::ETAG].to_str().unwrap().to_owned();

    // Soft-delete EN -> drops EN; recreate EN -> reactivates & restores EN; soft-delete + orphan purge -> cleanly gone.
    let en_del1 = client
        .delete(format!("{}/api/admin/locales/en", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &loc_etag)
        .send()
        .await
        .unwrap();
    assert_eq!(en_del1.status(), StatusCode::OK);
    assert_eq!(fts_docsize_ids(&db).await, vec![vi_id]);
    loc_etag = en_del1.headers()[header::ETAG].to_str().unwrap().to_owned();

    let en_reactivate = client
        .post(format!("{}/api/admin/locales", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({"code":"en","label":"English","direction":"ltr"}))
        .send()
        .await
        .unwrap();
    assert_eq!(en_reactivate.status(), StatusCode::CREATED);
    assert_eq!(fts_docsize_ids(&db).await, vec![vi_id, en_id]);
    loc_etag = en_reactivate.headers()[header::ETAG]
        .to_str()
        .unwrap()
        .to_owned();

    let en_del2 = client
        .delete(format!("{}/api/admin/locales/en", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &loc_etag)
        .send()
        .await
        .unwrap();
    assert_eq!(en_del2.status(), StatusCode::OK);
    loc_etag = en_del2.headers()[header::ETAG].to_str().unwrap().to_owned();

    let en_purge = client
        .post(format!("{}/api/admin/locales/orphans/en/purge", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .header(header::IF_MATCH, &loc_etag)
        .json(&json!({"confirm_code":"en"}))
        .send()
        .await
        .unwrap();
    assert_eq!(en_purge.status(), StatusCode::OK);
    assert_eq!(fts_docsize_ids(&db).await, vec![vi_id]);
    assert!(fts_match_ids(&db, "\"tok_en_only\"").await.is_empty());

    // 3. Stop server, tamper FTS5 (same COUNT(*), swapped IDs), restart -> startup bootstrap repairs!
    let draft_resp = client
        .post(format!("{}/api/admin/content-types/post/entries", site.base))
        .header(header::COOKIE, &cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"title":"VI Draft","slug":"vi-draft","locale":"vi","values":{"body":"tok_draft_never"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(draft_resp.status(), StatusCode::CREATED);
    let draft_id = draft_resp.json::<Value>().await.unwrap()["id"]
        .as_i64()
        .unwrap();

    site.stop_server();
    sqlx::query("DELETE FROM entry_search_fts WHERE rowid = ?")
        .bind(vi_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entry_search_fts(rowid, title, summary, body, terms) VALUES (?, 'tamper', '', 'tok_draft_never', '')")
        .bind(draft_id)
        .execute(&db)
        .await
        .unwrap();
    db.close().await;

    let restarted = Site::start_at(site_root).await;
    let db2 = restarted.db().await;
    assert_eq!(fts_docsize_ids(&db2).await, vec![vi_id]);
    assert!(fts_match_ids(&db2, "\"tok_draft_never\"").await.is_empty());
    assert_eq!(fts_match_ids(&db2, "\"tok_vi_only\"").await, vec![vi_id]);
}
