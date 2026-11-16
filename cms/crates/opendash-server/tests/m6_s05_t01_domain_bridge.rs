use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use opendash_core::builder::{
    BuilderDocument, ColumnNode, ElementContent, ElementNode, KnownElement, PluginBlockProps,
    RowNode, SectionNode,
};
use opendash_core::registry::{RegistryCatalog, ResourceOwner};
use opendash_core::theme::BlockDescriptor;
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
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .expect("connect sqlite");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations including 0024");

    // Insert plugin-owned settings without separate plugin registry table
    sqlx::query(
        "INSERT INTO theme_option_versions (theme_id, owner_kind, owner_id, version) VALUES ('acme-plugin', 'plugin', 'acme-plugin', 1)",
    )
    .execute(&pool)
    .await
    .expect("insert plugin option version");

    sqlx::query(
        "INSERT INTO theme_option_values (theme_id, owner_kind, owner_id, option_key, value_json) VALUES ('acme-plugin', 'plugin', 'acme-plugin', 'accent_color', '\"#112233\"')",
    )
    .execute(&pool)
    .await
    .expect("insert plugin option value");

    // Insert plugin-owned template selection in canonical template_overrides
    sqlx::query(
        "INSERT INTO template_overrides (role, scope_type_key, scope_entry_id, ast_json, version, base_theme_id, owner_kind, owner_id, template_selection_key)
         VALUES ('single', 'acme_product', NULL, '{\"schema_version\":1,\"sections\":[]}', 1, 'acme-plugin', 'plugin', 'acme-plugin', 'product-hero-v1')",
    )
    .execute(&pool)
    .await
    .expect("insert plugin template selection");

    let (owner_kind, owner_id, sel_key): (String, String, Option<String>) = sqlx::query_as(
        "SELECT owner_kind, owner_id, template_selection_key FROM template_overrides WHERE owner_kind = 'plugin' AND owner_id = 'acme-plugin'",
    )
    .fetch_one(&pool)
    .await
    .expect("query plugin template selection");
    assert_eq!(owner_kind, "plugin");
    assert_eq!(owner_id, "acme-plugin");
    assert_eq!(sel_key.as_deref(), Some("product-hero-v1"));

    // 2. Verify BuilderDocument AST validates active PluginBlock and preserves it when disabled
    let mut vals = BTreeMap::new();
    vals.insert("heading".into(), serde_json::json!("Plugin Block Title"));
    let doc = BuilderDocument {
        schema_version: 1,
        sections: vec![SectionNode {
            id: "sec-1".into(),
            responsive: None,
            children: vec![RowNode {
                id: "row-1".into(),
                responsive: None,
                children: vec![ColumnNode {
                    id: "col-1".into(),
                    responsive: None,
                    children: vec![ElementNode {
                        id: "el-1".into(),
                        content: ElementContent::Known(KnownElement::PluginBlock(PluginBlockProps {
                            block_id: "acme-plugin/hero".into(),
                            values: vals,
                        })),
                    }],
                }],
            }],
        }],
    };

    let mut active_reg = RegistryCatalog::default();
    active_reg
        .register(
            "acme-plugin/hero",
            ResourceOwner::Plugin("acme-plugin".into()),
            BlockDescriptor {
                id: "acme-plugin/hero".into(),
                fields: Default::default(),
            },
        )
        .expect("register plugin block");
    doc.validate_for_save(&active_reg, None)
        .expect("active plugin block must validate");

    // When plugin is disabled/missing, newly introducing it fails, but round-tripping existing saved AST succeeds
    let empty_reg = RegistryCatalog::default();
    assert!(doc.validate_for_save(&empty_reg, None).is_err());
    doc.validate_for_save(&empty_reg, Some(&doc))
        .expect("saved AST with disabled plugin block must be preserved");
}
