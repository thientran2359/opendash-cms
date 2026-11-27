//! M6-S10-T01 HTTP/SQLite permission and atomic term-extension integration.
//! Uses a disposable site root with a random loopback port, never owner data.
use std::{
    fs,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

struct Site {
    base: String,
    root: PathBuf,
    process: Child,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Site {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "opendash-m6-s10-t01-{}-{stamp}",
                std::process::id()
            ));
        fs::create_dir_all(&root).unwrap();
        let process = Command::new(env!("CARGO_BIN_EXE_opendash"))
            .env("OPENDASH_SITE_ROOT", &root)
            .env("OPENDASH_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        for _ in 0..180 {
            if Client::new()
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                return Self {
                    base,
                    root,
                    process,
                };
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("isolated native server not ready");
    }

    async fn database(&self) -> SqlitePool {
        let opts = SqliteConnectOptions::new()
            .filename(self.root.join("database/opendash.sqlite"))
            .foreign_keys(true)
            .create_if_missing(false);
        SqlitePool::connect_with(opts).await.unwrap()
    }
}

async fn login(client: &Client, site: &Site, user: &str, password: &str) -> (String, String) {
    let resp = client
        .post(format!("{}/auth/login", site.base))
        .json(&json!({"username":user,"password":password}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|c| c.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or("").to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let value: Value = resp.json().await.unwrap();
    (cookie, value["csrf_token"].as_str().unwrap().to_string())
}

async fn get(client: &Client, site: &Site, cookie: &str, path: &str) -> Value {
    let resp = client
        .get(format!("{}{}", site.base, path))
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "{path}");
    resp.json().await.unwrap()
}

async fn patch(
    client: &Client,
    site: &Site,
    cookie: &str,
    csrf: &str,
    id: i64,
    version: i64,
    body: Value,
) -> reqwest::Response {
    client
        .patch(format!("{}/api/admin/taxonomy/terms/{id}", site.base))
        .header("Cookie", cookie)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", csrf)
        .header("If-Match", format!("\"v{version}\""))
        .json(&body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn migrations_acl_token_cas_and_values_are_atomic_with_core_update() {
    let site = Site::start().await;
    let client = Client::new();
    let setup = client
        .post(format!("{}/auth/setup", site.base))
        .json(&json!({"username":"admin_ext","password":"Local-Admin-Extensions-123456!"}))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), StatusCode::CREATED);
    let (admin, csrf) = login(
        &client,
        &site,
        "admin_ext",
        "Local-Admin-Extensions-123456!",
    )
    .await;
    let db = site.database().await;

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name = 'admin_extensions'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(
        count, 1,
        "migration 0036 + 0063 must keep admin_extensions without legacy term_extension_values"
    );
    let baseline = get(
        &client,
        &site,
        &admin,
        "/api/admin/content-types/post/effective-list-config",
    )
    .await;
    assert!(baseline["columns"].as_array().unwrap().is_empty());

    let create = client
        .post(format!("{}/api/admin/taxonomy/terms", site.base))
        .header("Cookie", &admin)
        .header("Origin", &site.base)
        .header("X-CSRF-Token", &csrf)
        .json(&json!({"kind":"category","name":"Admin ext","slug":"admin-ext"}))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let term: Value = create.json().await.unwrap();
    let id = term["id"].as_i64().unwrap();
    let first_version = term["version"].as_i64().unwrap();

    // Only the native HTTP ZIP test publishes an actual plugin. This DB
    // fixture isolates the schema-token/CAS code path with deterministic data.
    sqlx::query("INSERT INTO plugins(id,version,name,manifest_json,wasm_blob) VALUES ('case-provider','1.0.0','Case','{}',X'00')")
        .execute(&db).await.unwrap();
    let group_id = "plugin:case-provider:internal_note";
    let fields_json = json!([
        {"key":"secret","label":"Admin secret","type":"text","required":true,"acl":{"min_view_role":"admin","min_edit_role":"admin","public_exposure":"private"}},
        {"key":"shared","label":"Shared","type":"boolean","required":false,"acl":{"min_view_role":"editor","min_edit_role":"editor","public_exposure":"private"}}
    ]);
    let loc_json = json!([[{"param":"taxonomy_term","op":"eq","value":"category"}]]);
    let pres_json = json!({"region":"main_bottom","order":1,"collapsible":true,"collapsed_by_default":false,"style":"card","label_placement":"top"});
    sqlx::query(
        "INSERT INTO content_field_groups(group_id,owner_kind,owner_id,local_key,title,description,fields_json,domain,location_rules_json,presentation_json,active,status,version)
         VALUES (?,'plugin','case-provider','internal_note','Internal Note','',?,'term',?,?,1,'active',1)",
    )
    .bind(group_id)
    .bind(fields_json.to_string())
    .bind(loc_json.to_string())
    .bind(pres_json.to_string())
    .execute(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO content_field_group_targets(group_id,target_kind,target_key,association_state) VALUES (?,'taxonomy_term','category','declared')",
    )
    .bind(group_id)
    .execute(&db).await.unwrap();
    let scope = format!("/api/admin/taxonomy/terms/{id}");
    let config = get(
        &client,
        &site,
        &admin,
        &format!("{scope}/effective-editor-config"),
    )
    .await;
    assert_eq!(config["groups"][0]["group_id"], group_id);
    let token = config["schema_token"].as_str().unwrap();
    assert!(token.starts_with("sha256:") && token.len() == 71);

    let updated = patch(
        &client,
        &site,
        &admin,
        &csrf,
        id,
        first_version,
        json!({"name":"Admin ext changed",
          "values":{"secret":"Original","shared":true},
          "schema_token":token}),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let saved: Value = updated.json().await.unwrap();
    assert_eq!(saved["name"], "Admin ext changed");
    assert_eq!(saved["values"]["secret"], "Original");
    let current = saved["version"].as_i64().unwrap();

    let stale = patch(
        &client,
        &site,
        &admin,
        &csrf,
        id,
        first_version,
        json!({"values":{"secret":"Stale"},"schema_token":token}),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    let changed_token = patch(
        &client,
        &site,
        &admin,
        &csrf,
        id,
        current,
        json!({"values":{"secret":"Bypass"},"schema_token":format!("sha256:{}", "0".repeat(64))}),
    )
    .await;
    assert_eq!(changed_token.status(), StatusCode::CONFLICT);
    let invalid_field = patch(
        &client,
        &site,
        &admin,
        &csrf,
        id,
        current,
        json!({"values":{"undeclared":"Oops"},"schema_token":token}),
    )
    .await;
    assert_eq!(invalid_field.status(), StatusCode::FORBIDDEN);
    let malformed = patch(
        &client,
        &site,
        &admin,
        &csrf,
        id,
        current,
        json!({"name":"Core field must not save",
            "values":{"shared":"yes"},"schema_token":token}),
    )
    .await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    let forbidden_null = patch(
        &client,
        &site,
        &admin,
        &csrf,
        id,
        current,
        json!({"values":{"secret":null},"schema_token":token}),
    )
    .await;
    assert_eq!(forbidden_null.status(), StatusCode::BAD_REQUEST);
    let absent_token = patch(
        &client,
        &site,
        &admin,
        &csrf,
        id,
        current,
        json!({"values":{"shared":true}}),
    )
    .await;
    assert_eq!(absent_token.status(), StatusCode::BAD_REQUEST);
    let current_term = get(&client, &site, &admin, &scope).await;
    assert_eq!(current_term["version"], current);
    assert_eq!(current_term["name"], "Admin ext changed");
    assert_eq!(current_term["values"]["secret"], "Original");

    // Even a maliciously registered provider column may not project a
    // private field into server table config or authenticated row DTOs.
    let private_schema = json!({"key":"private_cpt","label":"Private CPT"});
    sqlx::query(
        "INSERT INTO content_types(key,definition_json,owner_kind,provider_status,declaration_state,version)
         VALUES('private_cpt',?,'user','active','declared',1)",
    ).bind(private_schema.to_string()).execute(&db).await.unwrap();
    let private_fg = json!([{
        "key":"admin_note","label":"Admin note","type":"text","required":false,
        "acl":{"min_view_role":"admin","min_edit_role":"admin","public_exposure":"private"}
    }]);
    sqlx::query(
        "INSERT INTO content_field_groups(group_id,owner_kind,owner_id,local_key,title,description,fields_json,domain,location_rules_json,presentation_json,active,status,version)
         VALUES('user:private_cpt_fields','user',NULL,'private_cpt_fields','Private CPT Fields','',?,'content','[[{\"param\":\"content_type\",\"op\":\"eq\",\"value\":\"private_cpt\"}]]','{\"region\":\"main_bottom\",\"order\":10,\"collapsible\":true,\"collapsed_by_default\":false,\"style\":\"card\",\"label_placement\":\"top\"}',1,'active',1)",
    ).bind(private_fg.to_string()).execute(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO content_field_group_targets(group_id,target_kind,target_key,association_state) VALUES('user:private_cpt_fields','content_type','private_cpt','declared')",
    ).execute(&db).await.unwrap();
    sqlx::query(
        r#"INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
         VALUES('private_cpt','private-entry','Private entry','{"admin_note":"NO-LEAK"}','draft','vi','eg-' || lower(hex(randomblob(8))))"#,
    )
    .execute(&db)
    .await
    .unwrap();
    let private_column = json!({
      "kind":"column",
      "config":{"local_key":"private_column","label":"Secret column",
        "type_keys":["private_cpt"],"field_key":"admin_note",
        "format":"text","order":1}
    });
    sqlx::query(
        "INSERT INTO admin_extensions(owner_kind,owner_id,local_key,declaration_json)
         VALUES('plugin','case-provider','private_column',?)",
    )
    .bind(private_column.to_string())
    .execute(&db)
    .await
    .unwrap();
    let private_config = get(
        &client,
        &site,
        &admin,
        "/api/admin/content-types/private_cpt/effective-list-config",
    )
    .await;
    assert!(private_config["columns"].as_array().unwrap().is_empty());
    let private_rows = get(
        &client,
        &site,
        &admin,
        "/api/admin/content-types/private_cpt/entry-table",
    )
    .await;
    assert_eq!(private_rows["total"], 1);
    assert!(
        private_rows["items"][0]["fields"]
            .get("admin_note")
            .is_none()
    );

    let added_user = client.post(format!("{}/api/admin/users",site.base))
        .header("Cookie",&admin).header("Origin",&site.base)
        .header("X-CSRF-Token",&csrf)
        .json(&json!({"username":"editor_ext","password":"Local-Editor-Extensions-123456!","role":"editor"}))
        .send().await.unwrap();
    assert_eq!(added_user.status(), StatusCode::CREATED);
    let (editor, editor_csrf) = login(
        &client,
        &site,
        "editor_ext",
        "Local-Editor-Extensions-123456!",
    )
    .await;
    let editor_cfg = get(
        &client,
        &site,
        &editor,
        &format!("{scope}/effective-editor-config"),
    )
    .await;
    assert_ne!(editor_cfg["schema_token"], token);
    assert_eq!(
        editor_cfg["groups"][0]["fields"].as_array().unwrap().len(),
        1
    );
    let editor_term = get(&client, &site, &editor, &scope).await;
    assert!(editor_term["values"].get("secret").is_none());
    let forged = patch(
        &client,
        &site,
        &editor,
        &editor_csrf,
        id,
        current,
        json!({"values":{"secret":"Forged"},"schema_token":token}),
    )
    .await;
    assert_eq!(forged.status(), StatusCode::FORBIDDEN);
    let editor_saved = patch(
        &client,
        &site,
        &editor,
        &editor_csrf,
        id,
        current,
        json!({"values":{"shared":false},
            "schema_token":editor_cfg["schema_token"]}),
    )
    .await;
    assert_eq!(
        editor_saved.status(),
        StatusCode::OK,
        "editor may edit only explicitly permitted provider field"
    );
    let editor_saved: Value = editor_saved.json().await.unwrap();
    assert_eq!(editor_saved["values"]["shared"], false);
    assert!(editor_saved["values"].get("secret").is_none());
    let unchanged_secret = get(&client, &site, &admin, &scope).await;
    assert_eq!(unchanged_secret["values"]["secret"], "Original");

    sqlx::query("UPDATE plugins SET enabled=0 WHERE id='case-provider'")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE content_field_groups SET status='provider_inactive' WHERE owner_kind='plugin' AND owner_id='case-provider'")
        .execute(&db)
        .await
        .unwrap();
    let withdrawn = get(
        &client,
        &site,
        &admin,
        &format!("{scope}/effective-editor-config"),
    )
    .await;
    assert!(withdrawn["groups"].as_array().unwrap().is_empty());
    let stored: String = sqlx::query_scalar("SELECT values_json FROM taxonomy_terms WHERE id=?")
        .bind(id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&stored).unwrap()["secret"],
        "Original"
    );

    sqlx::query("UPDATE plugins SET enabled=1 WHERE id='case-provider'")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE content_field_groups SET status='active' WHERE owner_kind='plugin' AND owner_id='case-provider'")
        .execute(&db)
        .await
        .unwrap();
    let revived = get(&client, &site, &admin, &scope).await;
    assert_eq!(revived["values"]["secret"], "Original");

    let fk: Vec<(i64, String, i64, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&db)
        .await
        .unwrap();
    assert!(fk.is_empty());
    let intact: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(intact, "ok");
}
