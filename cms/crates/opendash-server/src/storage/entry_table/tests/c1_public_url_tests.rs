use super::*;
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn published_private_cpt_remains_in_admin_without_view_url() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();
    sqlx::query(
        r#"INSERT INTO content_types(key,definition_json,owner_kind)
           VALUES('news','{"key":"news","label":"News","settings":{"public":false,"has_archive":true,"publication":"immediate","supports":["editor","featured_image"]}}','user')"#,
    )
    .execute(&db)
    .await
    .unwrap();
    let id = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
         VALUES('news','hidden-article','Hidden Article','{}','published','vi','eg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();

    let hidden = query(&db, "news", FieldRole::Admin, &EntryTableFilter::default())
        .await
        .unwrap();
    assert_eq!(hidden.total, 1);
    assert_eq!(hidden.status_counts.published, 1);
    assert_eq!(hidden.items[0].id, id);
    assert!(hidden.items[0].public_url.is_none());

    sqlx::query(
        "UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.public', json('true')) WHERE key='news'",
    )
    .execute(&db)
    .await
    .unwrap();
    // This legacy fixture writes directly to SQLite instead of using the
    // normal content-type writer. Rebuild C3 claims in the same serialized
    // transaction the production writer would use after its mutation.
    let mut tx = db.begin().await.unwrap();
    super::super::permalinks::sync_claims_in_tx(&mut tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let visible = query(&db, "news", FieldRole::Admin, &EntryTableFilter::default())
        .await
        .unwrap();
    assert_eq!(
        visible.items[0].public_url.as_deref(),
        Some("/news/hidden-article")
    );
}
