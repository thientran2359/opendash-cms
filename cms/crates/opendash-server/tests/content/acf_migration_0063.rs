#[path = "acf_test_helpers.rs"]
mod helpers;

use helpers::TempDirGuard;
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

#[tokio::test]
async fn cuj2_0063_migration_preserves_legacy_cpt_term_extensions_and_zero_delta() {
    let dir = TempDirGuard::new("cuj2");
    let db_path = dir.0.join("upgrade_0063.sqlite");
    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true),
    )
    .await
    .unwrap();
    let migrator = sqlx::migrate!("../../migrations");
    migrator.run_to(62, &pool).await.unwrap();

    let pre_objects: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE name != '_sqlx_migrations' AND name != 'sqlite_sequence' AND name NOT LIKE 'sqlite_stat%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"INSERT INTO content_types(key, definition_json, owner_kind, version)
           VALUES('project', '{"key":"project","label":"Projects","fields":[{"key":"body","label":"Body","type":"richtext"},{"key":"excerpt","label":"Secret Excerpt","type":"text","acl":{"public_exposure":"private","min_view_role":"admin","min_edit_role":"admin"}},{"key":"client","label":"Client","type":"text"}],"settings":{"public":true,"has_archive":true,"publication":"draft_publish","supports":[]}}', 'user', 1)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
           VALUES('project', 'proj-1', 'Project One', '{"body":"<p>Ok</p>","excerpt":"Confidential","client":"Acme"}', 'published', 'vi', 'eg-cuj2')"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO content_editor_layouts(type_key, layout_json, version)
           VALUES('project', '{"schema_version":1,"type_key":"project","version":1,"base_field_panels":[],"regions":{"main_top":[{"panel_id":"core:base_fields","enabled":true,"order":10}],"main_primary":[],"main_bottom":[],"sidebar":[]}}', 1)"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO plugins(id,version,name,manifest_json,wasm_blob) VALUES('ext_p','1.0.0','ExtP',?,X'00')",
    )
    .bind(json!({"id":"ext_p","declarations":{"content_types":[{"key":"legacy_p","label":"LP","fields":[]}]}}).to_string())
    .execute(&pool)
    .await
    .unwrap();
    let term_id: i64 = sqlx::query_scalar(
        "INSERT INTO taxonomy_terms(kind,slug,name,locale,translation_group_id) VALUES('category','cuj2-cat','CUJ2 Cat','vi','tg-cuj2') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO admin_extensions(owner_kind,owner_id,local_key,declaration_json)
           VALUES('plugin','ext_p','cat_panel','{"kind":"term_panel","config":{"local_key":"cat_panel","label":"CatPanel","kind":"category","order":3,"fields":[{"key":"badge","label":"Badge","type":"boolean"},{"key":"meta_obj","label":"Meta","type":"text"}]}}')"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO term_extension_values(term_id,owner_kind,owner_id,local_key,values_json)
           VALUES(?,'plugin','ext_p','cat_panel','{"badge":true,"count":7,"meta_obj":"hi"}')"#,
    )
    .bind(term_id)
    .execute(&pool)
    .await
    .unwrap();

    migrator.run(&pool).await.unwrap();
    let post_objects: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE name != '_sqlx_migrations' AND name != 'sqlite_sequence' AND name NOT LIKE 'sqlite_stat%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        post_objects, pre_objects,
        "0063 must maintain +0 net sqlite_schema objects"
    );
    assert!(post_objects < 256);

    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
    let fk: Vec<(i64, String, i64, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(fk.is_empty());

    let proj_def: String =
        sqlx::query_scalar("SELECT definition_json FROM content_types WHERE key='project'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let proj_val: Value = serde_json::from_str(&proj_def).unwrap();
    assert!(proj_val.get("fields").is_none());
    assert!(
        proj_val["settings"]["supports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "editor")
    );
    let fg_fields_raw: String = sqlx::query_scalar(
        "SELECT fields_json FROM content_field_groups WHERE group_id='user:project_fields'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let fg_fields: Value = serde_json::from_str(&fg_fields_raw).unwrap();
    assert_eq!(fg_fields[0]["key"], "excerpt_legacy");
    assert_eq!(fg_fields[1]["key"], "client");
    let entry_vals_raw: String =
        sqlx::query_scalar("SELECT values_json FROM content_entries WHERE slug='proj-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let entry_vals: Value = serde_json::from_str(&entry_vals_raw).unwrap();
    assert_eq!(entry_vals["excerpt_legacy"], "Confidential");
    assert_eq!(entry_vals["client"], "Acme");

    let term_vals_raw: String =
        sqlx::query_scalar("SELECT values_json FROM taxonomy_terms WHERE id=?")
            .bind(term_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let term_vals: Value = serde_json::from_str(&term_vals_raw).unwrap();
    assert_eq!(term_vals["badge"], json!(true));
    assert_eq!(term_vals["count"], json!(7));
}
