use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use opendash_core::block_schema::{BlockSchema, RegisteredBlockSchema};
use opendash_core::builder::{BlockNode, BuilderContextTarget, BuilderDocument};
use opendash_core::registry::{RegistryCatalog, ResourceOwner};
use sqlx::sqlite::SqlitePoolOptions;

fn unique_db_path(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("opendash-m6-s05-{name}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("test.sqlite")
}

#[tokio::test]
async fn m6_s05_t01_migration_0024_owner_neutral_bridge_and_builder_ast_roundtrip() {
    // 1. Verify migration 0024 applies cleanly and supports owner_kind='plugin' alongside 'theme'
    let db_path = unique_db_path("bridge");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await
        .expect("connect sqlite");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations including 0024");

    // Insert plugin-owned settings without separate plugin registry table
    sqlx::query(
        "INSERT INTO theme_option_versions (owner_kind, owner_id, version) VALUES ('plugin', 'acme-plugin', 1)",
    )
    .execute(&pool)
    .await
    .expect("insert plugin option version");

    sqlx::query(
        "INSERT INTO theme_option_values (owner_kind, owner_id, option_key, value_json) VALUES ('plugin', 'acme-plugin', 'accent_color', '\"#112233\"')",
    )
    .execute(&pool)
    .await
    .expect("insert plugin option value");

    // 2. Verify BuilderDocument AST validates active PluginBlock and preserves it when disabled
    let mut vals = BTreeMap::new();
    vals.insert("heading".into(), serde_json::json!("Plugin Block Title"));
    let doc = BuilderDocument {
        schema_version: 1,
        blocks: vec![BlockNode {
            id: "el-1".into(),
            block_id: "acme-plugin/hero".into(),
            props: vals,
            children: vec![],
            meta: None,
        }],
    };

    // D02: Two providers may independently own the same role and scope.
    let raw = serde_json::to_string(&doc).unwrap();
    for (kind, key) in [("plugin", "product-hero-v1"), ("theme", "theme-hero-v1")] {
        sqlx::query(
            "INSERT INTO owner_template_selections (owner_kind, owner_id, role, scope_type_key, template_selection_key, ast_json, updated_at)
             VALUES (?, 'acme-plugin', 'single', 'acme_product', ?, ?, datetime('now'))",
        ).bind(kind).bind(key).bind(&raw).execute(&pool).await.unwrap();
    }
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT owner_kind, template_selection_key, ast_json FROM owner_template_selections WHERE owner_id = 'acme-plugin' ORDER BY owner_kind",
    ).fetch_all(&pool).await.unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "plugin".to_owned(),
                "product-hero-v1".to_owned(),
                raw.clone()
            ),
            ("theme".to_owned(), "theme-hero-v1".to_owned(), raw)
        ]
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM template_overrides")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    let hero_schema: BlockSchema = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": "acme-plugin/hero",
        "title": "Hero",
        "category": "plugin",
        "render": "render.html",
        "fields": [
            { "type": "text", "key": "heading", "label": "Heading" }
        ]
    }))
    .unwrap();
    let mut active_reg = RegistryCatalog::default();
    active_reg
        .register(
            "acme-plugin/hero",
            ResourceOwner::Plugin("acme-plugin".into()),
            RegisteredBlockSchema {
                schema: hero_schema,
            },
        )
        .expect("register plugin block");
    let ctx = BuilderContextTarget::EntryBody {
        content_type: "page".into(),
    };
    doc.validate_for_save(&active_reg, &ctx, None)
        .expect("active plugin block must validate");

    // When plugin is disabled/missing, newly introducing it fails, but round-tripping existing saved AST succeeds
    let empty_reg = RegistryCatalog::default();
    assert!(doc.validate_for_save(&empty_reg, &ctx, None).is_err());
    doc.validate_for_save(&empty_reg, &ctx, Some(&doc))
        .expect("saved AST with disabled plugin block must be preserved");
}
