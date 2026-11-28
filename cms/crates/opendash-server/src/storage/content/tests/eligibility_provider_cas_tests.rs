use super::*;
use opendash_core::content::{
    CntDeclarations, ContentTypeDeclaration, TaxonomyEligibilityDeclaration,
};
use sqlx::sqlite::SqlitePoolOptions;

fn provider_declarations(taxonomies: &[&str]) -> CntDeclarations {
    CntDeclarations {
        content_types: vec![ContentTypeDeclaration {
            key: "provider_books_cas".to_owned(),
            label: "Provider books".to_owned(),
            singular_label: None,
            description: None,
            taxonomies: taxonomies.iter().map(|s| (*s).to_owned()).collect(),
            settings: None,
            rewrite_base: None,
        }],
        taxonomy_definitions: vec![],
        taxonomy_eligibility: vec![],
        field_groups: vec![],
        options_pages: vec![],
    }
}

async fn declare(db: &SqlitePool, taxonomies: &[&str]) {
    let mut tx = db.begin().await.unwrap();
    sync_plugin_cnt_declarations_in_tx(
        &mut tx,
        "provider_cas_test",
        &provider_declarations(taxonomies),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn provider_redeclaration_advances_taxonomy_cas_once_only_when_changed() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();

    declare(&db, &["category"]).await;
    let initial = load_configured_taxonomies(&db, "provider_books_cas")
        .await
        .unwrap();
    assert_eq!(initial.taxonomies, vec!["category"]);
    let old_version = initial.type_version;

    // Plugin redeclare commits new default while Admin holds old CAS.
    declare(&db, &["category", "tag"]).await;
    let next = load_configured_taxonomies(&db, "provider_books_cas")
        .await
        .unwrap();
    assert_eq!(next.taxonomies, vec!["category", "tag"]);
    assert_eq!(next.type_version, old_version + 1);
    let stale = sqlx::query("UPDATE content_types SET version=version+1 WHERE key=? AND version=?")
        .bind("provider_books_cas")
        .bind(old_version)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(stale.rows_affected(), 0);

    declare(&db, &["category", "tag"]).await;
    let same = load_configured_taxonomies(&db, "provider_books_cas")
        .await
        .unwrap();
    assert_eq!(same.type_version, next.type_version);

    // Re-declaring an existing provider CPT with different schema but the
    // same taxonomy defaults also invalidates a stale eligibility CAS.
    let mut renamed_declarations = provider_declarations(&["category", "tag"]);
    renamed_declarations.content_types[0].label = "Provider books renamed".to_owned();
    let mut tx = db.begin().await.unwrap();
    sync_plugin_cnt_declarations_in_tx(&mut tx, "provider_cas_test", &renamed_declarations)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let renamed = load_configured_taxonomies(&db, "provider_books_cas")
        .await
        .unwrap();
    assert_eq!(renamed.type_version, same.type_version + 1);
    // Identical re-declaration must not create perpetual CAS conflicts.
    let mut tx = db.begin().await.unwrap();
    sync_plugin_cnt_declarations_in_tx(&mut tx, "provider_cas_test", &renamed_declarations)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        load_configured_taxonomies(&db, "provider_books_cas")
            .await
            .unwrap()
            .type_version,
        renamed.type_version
    );

    // Provider enable/disable changes effective eligibility and also
    // fences stale Admin settings/associations from an earlier lifetime.
    let mut tx = db.begin().await.unwrap();
    set_plugin_cnt_provider_status_in_tx(&mut tx, "provider_cas_test", false)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let inactive = load_configured_taxonomies(&db, "provider_books_cas")
        .await
        .unwrap();
    assert_eq!(inactive.type_version, renamed.type_version + 1);
    assert!(inactive.effective.is_empty());
    let mut tx = db.begin().await.unwrap();
    set_plugin_cnt_provider_status_in_tx(&mut tx, "provider_cas_test", true)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let reactivated = load_configured_taxonomies(&db, "provider_books_cas")
        .await
        .unwrap();
    assert_eq!(reactivated.type_version, inactive.type_version + 1);
    assert_eq!(reactivated.effective, vec!["category", "tag"]);

    declare(&db, &["category"]).await;
    let withdrawn = load_configured_taxonomies(&db, "provider_books_cas")
        .await
        .unwrap();
    assert_eq!(withdrawn.type_version, reactivated.type_version + 1);
    assert_eq!(withdrawn.taxonomies, vec!["category", "tag"]);
    assert_eq!(withdrawn.effective, vec!["category"]);

    // Provider may keep explicit eligibility for its owned CPT after
    // omitting the CPT declaration. Enable then changes the eligibility
    // status even though the owning CPT stays 'withdrawn'; that still
    // needs a version bump to reject a stale Admin site override.
    let eligibility_only = CntDeclarations {
        content_types: vec![],
        taxonomy_definitions: vec![],
        taxonomy_eligibility: vec![TaxonomyEligibilityDeclaration {
            content_type: "provider_books_cas".into(),
            taxonomies: vec!["category".into(), "tag".into()],
        }],
        field_groups: vec![],
        options_pages: vec![],
    };
    let mut tx = db.begin().await.unwrap();
    sync_plugin_cnt_declarations_in_tx(&mut tx, "provider_cas_test", &eligibility_only)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let eligible_withdrawn_cpt = load_configured_taxonomies(&db, "provider_books_cas")
        .await
        .unwrap();
    assert_eq!(
        eligible_withdrawn_cpt.type_version,
        withdrawn.type_version + 1
    );

    let mut tx = db.begin().await.unwrap();
    set_plugin_cnt_provider_status_in_tx(&mut tx, "provider_cas_test", false)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let off = load_configured_taxonomies(&db, "provider_books_cas")
        .await
        .unwrap();
    assert_eq!(off.type_version, eligible_withdrawn_cpt.type_version + 1);

    let mut tx = db.begin().await.unwrap();
    set_plugin_cnt_provider_status_in_tx(&mut tx, "provider_cas_test", true)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let on = load_configured_taxonomies(&db, "provider_books_cas")
        .await
        .unwrap();
    assert_eq!(on.type_version, off.type_version + 1);
    assert_eq!(on.effective, vec!["category", "tag"]);
    let stale = sqlx::query("UPDATE content_types SET version=version+1 WHERE key=? AND version=?")
        .bind("provider_books_cas")
        .bind(off.type_version)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(stale.rows_affected(), 0);
}
