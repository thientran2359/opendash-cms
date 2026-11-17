//! M7-S16-T03 forward taxonomy eligibility: preserve old Page links and
//! non-Core declarations without modifying private/owner databases.
use sqlx::sqlite::SqlitePoolOptions;

const ELIGIBILITY_SCHEMA: &str = r#"
CREATE TABLE content_types (key TEXT PRIMARY KEY);
CREATE TABLE content_type_taxonomy_eligibility (
    content_type_key TEXT NOT NULL REFERENCES content_types(key),
    taxonomy_kind TEXT NOT NULL CHECK(taxonomy_kind IN ('category','tag')),
    owner_kind TEXT NOT NULL CHECK(owner_kind IN ('core','user','theme','plugin')),
    owner_id TEXT,
    declaration_state TEXT NOT NULL DEFAULT 'declared'
        CHECK(declaration_state IN ('declared','withdrawn')),
    provider_status TEXT NOT NULL DEFAULT 'active'
        CHECK(provider_status IN ('active','provider_inactive')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY(content_type_key,taxonomy_kind)
);
CREATE TABLE entry_terms (entry_id INTEGER NOT NULL, term_id INTEGER NOT NULL);
INSERT INTO content_types(key) VALUES ('post'), ('page');
"#;

#[tokio::test]
async fn core_post_defaults_and_page_absent_on_clean_install() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql(ELIGIBILITY_SCHEMA)
        .execute(&db)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../migrations/0035_page_taxonomy_default.sql"
    ))
    .execute(&db)
    .await
    .unwrap();
    let eligible: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT content_type_key,taxonomy_kind,owner_kind
         FROM content_type_taxonomy_eligibility
         WHERE declaration_state='declared' AND provider_status='active'
         ORDER BY content_type_key,taxonomy_kind",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(
        eligible,
        vec![
            ("post".into(), "category".into(), "core".into()),
            ("post".into(), "tag".into(), "core".into()),
        ]
    );
}

#[tokio::test]
async fn upgrade_withdraws_only_core_page_default_and_preserves_legacy_links() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::raw_sql(ELIGIBILITY_SCHEMA)
        .execute(&db)
        .await
        .unwrap();
    sqlx::raw_sql(
        r#"
        INSERT INTO content_type_taxonomy_eligibility
            (content_type_key,taxonomy_kind,owner_kind,declaration_state,provider_status)
        VALUES ('page','category','core','declared','active'),
               ('page','tag','user','declared','active');
        INSERT INTO entry_terms(entry_id,term_id) VALUES (47,11),(47,12);
    "#,
    )
    .execute(&db)
    .await
    .unwrap();

    sqlx::raw_sql(include_str!(
        "../../../migrations/0035_page_taxonomy_default.sql"
    ))
    .execute(&db)
    .await
    .unwrap();
    let entries: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT content_type_key,taxonomy_kind,owner_kind,declaration_state
         FROM content_type_taxonomy_eligibility ORDER BY content_type_key,taxonomy_kind",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(
        entries,
        vec![
            (
                "page".into(),
                "category".into(),
                "core".into(),
                "withdrawn".into()
            ),
            (
                "page".into(),
                "tag".into(),
                "user".into(),
                "declared".into()
            ),
            (
                "post".into(),
                "category".into(),
                "core".into(),
                "declared".into()
            ),
            (
                "post".into(),
                "tag".into(),
                "core".into(),
                "declared".into()
            ),
        ]
    );
    let old_links: Vec<i64> =
        sqlx::query_scalar("SELECT term_id FROM entry_terms WHERE entry_id = 47 ORDER BY term_id")
            .fetch_all(&db)
            .await
            .unwrap();
    assert_eq!(old_links, vec![11, 12]);
}
