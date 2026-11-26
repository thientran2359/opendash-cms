use super::*;

#[tokio::test]
async fn builder_generic_taxonomy_list_hides_drafts_and_survives_withdrawn_references() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO taxonomy_definitions(key,label,hierarchical,public,owner_kind)
         VALUES('genre','Genre',1,1,'user')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json)
         VALUES('post','[\"genre\"]')",
    )
    .execute(&db)
    .await
    .unwrap();
    let root = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('genre','Root','genre-root','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let child = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,parent_id,locale,translation_group_id)
         VALUES('genre','Child','genre-child',?,'vi','tg-' || lower(hex(randomblob(8))))",
    )
    .bind(root)
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let published = list_test_entry(&db, "post", "genre-public", true, &[child]).await;
    list_test_entry(&db, "post", "genre-draft", false, &[child]).await;
    let filters = EntryFilters {
        taxonomy_key: Some("genre".into()),
        term_id: Some(root),
        include_descendants: true,
        ..EntryFilters::default()
    };
    let page = expect_list_page(
        list_published_page(
            &db,
            "post",
            1,
            1,
            PublishedListSort::IdDesc,
            filters.clone(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        page.entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>(),
        [published]
    );
    assert!(!page.has_next, "drafts must not create a next page");

    // A malformed query remains a 400, even though a withdrawn persisted
    // Builder reference is treated as unavailable configuration.
    let malformed = EntryFilters {
        tag_id: Some(child),
        ..filters.clone()
    };
    let error = list_published_page(&db, "post", 1, 6, PublishedListSort::IdDesc, malformed)
        .await
        .unwrap_err();
    assert_eq!(error.0, axum::http::StatusCode::BAD_REQUEST);

    for change in [
        "UPDATE taxonomy_definitions SET public=0 WHERE key='genre'",
        "UPDATE taxonomy_definitions SET provider_status='provider_inactive',public=1 WHERE key='genre'",
        "UPDATE taxonomy_definitions SET declaration_state='withdrawn',
            provider_status='active' WHERE key='genre'",
    ] {
        sqlx::query(change).execute(&db).await.unwrap();
        let result = list_published_page(
            &db,
            "post",
            1,
            6,
            PublishedListSort::IdDesc,
            filters.clone(),
        )
        .await
        .unwrap();
        assert!(matches!(
            result,
            PublishedListResult::MissingConfig(PublishedListMissing::Taxonomy)
        ));
    }
    sqlx::query(
        "UPDATE taxonomy_definitions
            SET public=1, provider_status='active',declaration_state='declared'
          WHERE key='genre'",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE content_type_taxonomy_overrides SET taxonomies_json='[]'
         WHERE content_type_key='post'",
    )
    .execute(&db)
    .await
    .unwrap();
    let withdrawn = list_published_page(
        &db,
        "post",
        1,
        6,
        PublishedListSort::IdDesc,
        filters.clone(),
    )
    .await
    .unwrap();
    assert!(matches!(
        withdrawn,
        PublishedListResult::MissingConfig(PublishedListMissing::Taxonomy)
    ));
    sqlx::query(
        "UPDATE content_type_taxonomy_overrides SET taxonomies_json='[\"genre\"]'
         WHERE content_type_key='post'",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("DELETE FROM entry_terms WHERE term_id=?")
        .bind(child)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM taxonomy_terms WHERE id=?")
        .bind(child)
        .execute(&db)
        .await
        .unwrap();
    let deleted = list_published_page(
        &db,
        "post",
        1,
        6,
        PublishedListSort::IdDesc,
        EntryFilters {
            term_id: Some(child),
            ..filters.clone()
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        deleted,
        PublishedListResult::MissingConfig(PublishedListMissing::Taxonomy)
    ));
    // An Admin can switch a taxonomy to flat once every remaining term
    // is a root. An older saved Builder node with descendants=true must
    // not turn the entire public homepage into HTTP 400.
    sqlx::query("UPDATE taxonomy_definitions SET hierarchical=0 WHERE key='genre'")
        .execute(&db)
        .await
        .unwrap();
    let incompatible = list_published_page(
        &db,
        "post",
        1,
        6,
        PublishedListSort::IdDesc,
        filters.clone(),
    )
    .await
    .unwrap();
    assert!(matches!(
        incompatible,
        PublishedListResult::MissingConfig(PublishedListMissing::Taxonomy)
    ));
    let mut tx = db.begin().await.unwrap();
    let query_error = validate_filter_in_tx(&mut tx, "post", &filters, true)
        .await
        .unwrap_err();
    assert_eq!(
        query_error.0,
        axum::http::StatusCode::BAD_REQUEST,
        "user-provided filters remain strict after the definition change"
    );
}

#[tokio::test]
async fn published_taxonomy_filter_excludes_drafts_before_pagination() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
    let root = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Root','root','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let child = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,parent_id,locale,translation_group_id) VALUES('category','Child','child',?,'vi','tg-' || lower(hex(randomblob(8))))",
    )
    .bind(root)
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let tag =
        sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Tag','tag','vi','tg-' || lower(hex(randomblob(8))))")
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();

    let mut ids = vec![];
    for (slug, is_published) in [
        ("published-first", true),
        ("draft-newer", false),
        ("published-last", true),
    ] {
        let id = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
             VALUES('post',?,'Safe','{}','draft','vi','eg-' || lower(hex(randomblob(8))))",
        )
        .bind(slug)
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        if is_published {
            sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
                .bind(id)
                .execute(&db)
                .await
                .unwrap();
        }
        // More than one matching category link to one entry must never
        // inflate public results or displace a later page.
        for term in [root, child, tag] {
            sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES (?,?)")
                .bind(id)
                .bind(term)
                .execute(&db)
                .await
                .unwrap();
        }
        ids.push(id);
    }
    let filters = EntryFilters {
        category_id: Some(root),
        tag_id: Some(tag),
        include_descendants: true,
        ..EntryFilters::default()
    };
    let first = list_published_filtered(&db, "post", 1, 0, filters.clone())
        .await
        .unwrap();
    let second = list_published_filtered(&db, "post", 1, 1, filters.clone())
        .await
        .unwrap();
    let last = list_published_filtered(&db, "post", 1, 2, filters.clone())
        .await
        .unwrap();
    assert_eq!(first.iter().map(|e| e.id).collect::<Vec<_>>(), [ids[2]]);
    assert_eq!(second.iter().map(|e| e.id).collect::<Vec<_>>(), [ids[0]]);
    assert!(last.is_empty());
    assert_eq!(first[0].term_ids, [root, child, tag]);
    let admin = list_entries(&db, "post", 10, 0, filters.clone())
        .await
        .unwrap();
    assert_eq!(
        admin.len(),
        3,
        "Admin sees unpublished draft before pagination"
    );

    // Reparented category links retain stable IDs and public filters must
    // follow the current tree, not stale slug/path snapshots.
    let other = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Other','other','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    for id in &ids {
        sqlx::query("DELETE FROM entry_terms WHERE entry_id=? AND term_id=?")
            .bind(id)
            .bind(root)
            .execute(&db)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE taxonomy_terms SET parent_id=? WHERE id=?")
        .bind(other)
        .bind(child)
        .execute(&db)
        .await
        .unwrap();
    assert!(
        list_published_filtered(&db, "post", 10, 0, filters.clone())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        list_published_filtered(
            &db,
            "post",
            10,
            0,
            EntryFilters {
                category_id: Some(other),
                ..filters
            }
        )
        .await
        .unwrap()
        .len(),
        2
    );
}
