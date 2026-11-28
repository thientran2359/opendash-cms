use serde_json::{Value, json};
use sqlx::SqlitePool;

#[tokio::test]
async fn migration_0056_upgrades_baseline_0001_with_zero_schema_delta_and_under_256_objects() {
    let db_path = std::env::temp_dir().join(format!("opendash_0056_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&db_path);
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/baseline_0001.sqlite"
        ),
        &db_path,
    )
    .unwrap();

    let pool = SqlitePool::connect(&format!("sqlite://{}", db_path.display()))
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let post_objects: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(post_objects < 256, "schema objects {post_objects} >= 256");

    let post_def: String =
        sqlx::query_scalar("SELECT definition_json FROM content_types WHERE key = 'post'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let post_json: Value = serde_json::from_str(&post_def).unwrap();
    assert!(post_json.get("fields").is_none());
    assert_eq!(
        post_json["settings"]["supports"],
        json!(["editor", "featured_image"])
    );
    assert!(post_json["settings"].is_object());
    serde_json::from_value::<opendash_core::content::ContentTypeDefinition>(post_json).unwrap();

    let pattern_def: String =
        sqlx::query_scalar("SELECT definition_json FROM content_types WHERE key = 'pattern'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let pattern_json: Value = serde_json::from_str(&pattern_def).unwrap();
    assert!(pattern_json.get("fields").is_none());
    serde_json::from_value::<opendash_core::content::ContentTypeDefinition>(pattern_json).unwrap();
}
