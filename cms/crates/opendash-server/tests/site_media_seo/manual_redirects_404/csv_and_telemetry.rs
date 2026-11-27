use reqwest::{StatusCode, header};
use serde_json::{Value, json};

use super::{Site, no_redirect_client, setup_and_login_admin};

#[tokio::test]
async fn csv_import_export_localization_and_prelocalized_code_over_1mib() {
    let site = Site::start().await;
    let http = no_redirect_client();
    let admin = setup_and_login_admin(&http, &site).await;

    // Generate > 500 failing CSV rows (~1.2 MiB response if unfiltered, or 500 verbose rows)
    // and ensure PrelocalizedErrorResponse keeps exact x-opendash-error-code header.
    let mut big_csv = String::from("source_path,target_url,status_code,enabled,note\n");
    for idx in 0..550 {
        big_csv.push_str(&format!(
            "/src-{idx},javascript:alert({idx}),301,true,note\n"
        ));
    }
    let bad_commit = http
        .post(format!("{}/api/admin/redirects/import", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({"csv_text": big_csv, "dry_run": false, "conflict_policy": "skip"}))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_commit.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        bad_commit.headers().get("x-opendash-error-code").unwrap(),
        "redirects.csv_import_validation_failed"
    );
    let err_json: Value = bad_commit.json().await.unwrap();
    assert_eq!(err_json["error_count"], 550);
    assert_eq!(err_json["truncated"], true);
    assert_eq!(err_json["rows"].as_array().unwrap().len(), 500);
    assert_eq!(err_json["code"], "redirects.csv_import_validation_failed");

    // Dry-run valid + overwrite + stats preservation + CSV formula-safe export.
    let seed = http
        .post(format!("{}/api/admin/redirects", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({"source_path":"/csv-old","target_url":"/v1","status_code":301,"note":"initial"}))
        .send()
        .await
        .unwrap();
    assert_eq!(seed.status(), StatusCode::CREATED);
    assert_eq!(
        http.get(format!("{}/csv-old", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::MOVED_PERMANENTLY
    );

    // Flush hits by calling GET /api/admin/redirects.
    let before: Value = http
        .get(format!("{}/api/admin/redirects?q=csv-old", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(before["items"][0]["hit_count"], 1);

    // Conflict policy = error returns 409 + redirects.csv_import_conflicts.
    let conflict_res = http
        .post(format!("{}/api/admin/redirects/import", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({
            "csv_text": "From,To,Note\n/csv-old,/v2,=CMD()\n",
            "dry_run": false,
            "conflict_policy": "error"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(conflict_res.status(), StatusCode::CONFLICT);
    assert_eq!(
        conflict_res.headers().get("x-opendash-error-code").unwrap(),
        "redirects.csv_import_conflicts"
    );

    // Overwrite keeps hit_count == 1 and bumps version -> 2.
    let ok_csv = "Target,From,Status,Active,Note\r\n/v2,/csv-old,302,yes,\"=1+1\"\r\n/dest-a,/src-a,,,\"+SUM(1,2)\"\r\n";
    let commit = http
        .post(format!("{}/api/admin/redirects/import", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({"csv_text": ok_csv, "dry_run": false, "conflict_policy": "overwrite"}))
        .send()
        .await
        .unwrap();
    assert_eq!(commit.status(), StatusCode::OK);
    let commit_json: Value = commit.json().await.unwrap();
    assert_eq!(commit_json["create_count"], 1);
    assert_eq!(commit_json["update_count"], 1);

    let after: Value = http
        .get(format!("{}/api/admin/redirects?q=csv-old", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after["items"][0]["hit_count"], 1);
    assert_eq!(after["items"][0]["version"], 2);
    assert_eq!(after["items"][0]["status_code"], 302);

    // Export CSV must contain BOM and single-quote neutralized formula cells.
    let export = http
        .get(format!("{}/api/admin/redirects/export", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(export.status(), StatusCode::OK);
    let csv_bytes = export.bytes().await.unwrap();
    assert!(csv_bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    let csv_str = String::from_utf8(csv_bytes.to_vec()).unwrap();
    assert!(csv_str.contains("'=1+1"));
    assert!(csv_str.contains("'+SUM(1,2)"));
}

#[tokio::test]
async fn not_found_telemetry_resolve_bulk_delete_clear_and_schema_budget() {
    let site = Site::start().await;
    let http = no_redirect_client();
    let admin = setup_and_login_admin(&http, &site).await;

    // Trigger 404 hits on /missing-a (2 GET + 1 HEAD) and /missing-b (1 GET).
    assert_eq!(
        http.get(format!("{}/missing-a", site.base))
            .header(header::REFERER, "https://user:secret@google.com/search?q=1")
            .header(header::USER_AGENT, "OpenDashTest/1.0")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        http.get(format!("{}/missing-a", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        http.head(format!("{}/missing-a", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        http.get(format!("{}/missing-b", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );

    let list: Value = http
        .get(format!("{}/api/admin/404-logs?sort=hits_desc", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["total"], 2);
    let rows = list["items"].as_array().unwrap();
    assert_eq!(rows[0]["path"], "/missing-a");
    assert_eq!(rows[0]["hit_count"], 2);
    // Secret userinfo in Referer must be stripped before persistence.
    assert!(
        !rows[0]["last_referrer"]
            .as_str()
            .unwrap()
            .contains("secret")
    );

    // Resolving /missing-a via POST /api/admin/redirects removes its 404 row and evicts RAM.
    let resolve = http
        .post(format!("{}/api/admin/redirects", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({
            "source_path": "/missing-a",
            "target_url": "/resolved",
            "status_code": 301,
            "resolve_not_found": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resolve.status(), StatusCode::CREATED);

    let list_after: Value = http
        .get(format!("{}/api/admin/404-logs", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list_after["total"], 1);
    assert_eq!(list_after["items"][0]["path"], "/missing-b");

    // Atomic clear(older_than_days = 30) flushes pending RAM /missing-fresh before deleting older rows.
    assert_eq!(
        http.get(format!("{}/missing-fresh", site.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let clear_old = http
        .post(format!("{}/api/admin/404-logs/clear", site.base))
        .header(header::COOKIE, &admin.cookie)
        .header("x-csrf-token", &admin.csrf)
        .header(header::ORIGIN, &site.base)
        .json(&json!({"older_than_days": 30}))
        .send()
        .await
        .unwrap();
    assert_eq!(clear_old.status(), StatusCode::OK);
    let after_clear: Value = http
        .get(format!("{}/api/admin/404-logs", site.base))
        .header(header::COOKIE, &admin.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after_clear["total"], 2);

    // Schema object count remains safely below MAX_SCHEMA_OBJECTS (256).
    let db = site.db().await;
    let schema_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(schema_count < 256, "schema_count={schema_count}");
}
