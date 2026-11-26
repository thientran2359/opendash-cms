use super::*;

#[tokio::test]
async fn builder_published_list_filters_before_paging_for_post_page_cpt() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
    // Migration 0008 seeds Post/Page; only the custom CPT is new here.
    sqlx::query(
        "INSERT INTO content_types(key,definition_json,owner_kind)
         VALUES('news',json_object('key','news','fields',json_array()),'user')",
    )
    .execute(&db)
    .await
    .unwrap();
    // Page stays unassociated by default; this fixture explicitly opts in
    // Page and the custom CPT, as a real C2 Admin configuration would.
    sqlx::query(
        "INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json)
                 VALUES ('page','[\"category\",\"tag\"]'),('news','[\"category\",\"tag\"]')",
    )
    .execute(&db)
    .await
    .unwrap();
    let root = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Root','list-root','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let child = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,parent_id,locale,translation_group_id)
         VALUES('category','Child','list-child',?,'vi','tg-' || lower(hex(randomblob(8))))",
    )
    .bind(root)
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let tag = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Tag','list-tag','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let other_tag = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Other','other-tag','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();

    let first = list_test_entry(&db, "post", "p1", true, &[root, child, tag]).await;
    list_test_entry(&db, "post", "draft", false, &[child, tag]).await;
    let second = list_test_entry(&db, "post", "p2", true, &[root, tag]).await;
    list_test_entry(&db, "post", "wrong-tag", true, &[child, other_tag]).await;
    let third = list_test_entry(&db, "post", "p3", true, &[child, tag]).await;
    let page_id = list_test_entry(&db, "page", "page1", true, &[child, tag]).await;
    let cpt_id = list_test_entry(&db, "news", "news1", true, &[child, tag]).await;
    list_test_entry(&db, "news", "draft-news", false, &[child, tag]).await;

    let filters = EntryFilters {
        category_id: Some(root),
        tag_id: Some(tag),
        include_descendants: true,
        ..EntryFilters::default()
    };
    for (sort, expected) in [
        (PublishedListSort::IdDesc, [third, second, first]),
        (PublishedListSort::IdAsc, [first, second, third]),
    ] {
        for (i, &id) in expected.iter().enumerate() {
            let page = expect_list_page(
                list_published_page(&db, "post", i as i64 + 1, 1, sort, filters.clone())
                    .await
                    .unwrap(),
            );
            assert_eq!(page.entries.iter().map(|e| e.id).collect::<Vec<_>>(), [id]);
            assert_eq!(page.has_next, i < 2);
        }
    }

    let exact = expect_list_page(
        list_published_page(
            &db,
            "post",
            1,
            20,
            PublishedListSort::IdAsc,
            EntryFilters {
                include_descendants: false,
                ..filters.clone()
            },
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        exact.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
        [first, second],
        "exact category only; a duplicated direct+descendant link cannot duplicate rows"
    );
    assert!(!exact.has_next);
    for (type_key, id) in [("page", page_id), ("news", cpt_id)] {
        let result = expect_list_page(
            list_published_page(
                &db,
                type_key,
                1,
                20,
                PublishedListSort::IdDesc,
                filters.clone(),
            )
            .await
            .unwrap(),
        );
        assert_eq!(
            result.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            [id]
        );
        assert!(!result.has_next);
    }

    let past_end = expect_list_page(
        list_published_page(
            &db,
            "post",
            100,
            1,
            PublishedListSort::IdAsc,
            filters.clone(),
        )
        .await
        .unwrap(),
    );
    assert!(past_end.entries.is_empty());
    assert!(!past_end.has_next);

    // Existing public archive contract stays descending with offset paging.
    let old = list_published_filtered(&db, "post", 2, 1, filters)
        .await
        .unwrap();
    assert_eq!(
        old.iter().map(|e| e.id).collect::<Vec<_>>(),
        [second, first]
    );
}

#[tokio::test]
async fn builder_list_rejects_bad_bounds_and_distinguishes_missing_config() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO content_types(key,definition_json,owner_kind)
         VALUES('news',json_object('key','news','fields',json_array()),'user')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO content_types(key,definition_json,owner_kind)
         VALUES('deleted-cpt',json_object('key','deleted-cpt','fields',json_array()),'user')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("DELETE FROM content_types WHERE key = 'deleted-cpt'")
        .execute(&db)
        .await
        .unwrap();
    let category = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','List','list-category','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let tag = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','List','list-tag','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    for (page, size) in [(0, 6), (101, 6), (-1, 6), (1, 0), (1, 21), (1, -1)] {
        let error = list_published_page(
            &db,
            "news",
            page,
            size,
            PublishedListSort::IdDesc,
            EntryFilters::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.0, axum::http::StatusCode::BAD_REQUEST);
    }
    for filters in [
        EntryFilters {
            include_descendants: true,
            ..Default::default()
        },
        EntryFilters {
            category_id: Some(-1),
            ..Default::default()
        },
    ] {
        let error = list_published_page(&db, "news", 1, 6, PublishedListSort::IdDesc, filters)
            .await
            .unwrap_err();
        assert_eq!(error.0, axum::http::StatusCode::BAD_REQUEST);
    }
    for (type_key, filters, expected) in [
        (
            "deleted-cpt",
            EntryFilters::default(),
            PublishedListMissing::ContentType,
        ),
        (
            "news",
            EntryFilters {
                category_id: Some(tag),
                ..Default::default()
            },
            PublishedListMissing::Category,
        ),
        (
            "news",
            EntryFilters {
                tag_id: Some(category),
                ..Default::default()
            },
            PublishedListMissing::Tag,
        ),
        (
            "news",
            EntryFilters {
                category_id: Some(999999),
                ..Default::default()
            },
            PublishedListMissing::Category,
        ),
        (
            "news",
            EntryFilters {
                tag_id: Some(999999),
                ..Default::default()
            },
            PublishedListMissing::Tag,
        ),
    ] {
        let result = list_published_page(&db, type_key, 1, 6, PublishedListSort::IdDesc, filters)
            .await
            .unwrap();
        assert!(matches!(result, PublishedListResult::MissingConfig(reason) if reason == expected));
    }

    // Deleting a previously valid, unused term must not turn into a 500.
    sqlx::query("DELETE FROM taxonomy_terms WHERE id = ?")
        .bind(tag)
        .execute(&db)
        .await
        .unwrap();
    let result = list_published_page(
        &db,
        "news",
        1,
        6,
        PublishedListSort::IdDesc,
        EntryFilters {
            tag_id: Some(tag),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        result,
        PublishedListResult::MissingConfig(PublishedListMissing::Tag)
    ));

    // Even when more than 100 pages exist, page 100 is the last linkable page.
    for i in 0..101 {
        list_test_entry(&db, "news", &format!("entry-{i}"), true, &[]).await;
    }
    let final_page = expect_list_page(
        list_published_page(
            &db,
            "news",
            100,
            1,
            PublishedListSort::IdDesc,
            EntryFilters::default(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(final_page.entries.len(), 1);
    assert!(!final_page.has_next);

    db.close().await;
    let error = list_published_page(
        &db,
        "news",
        1,
        6,
        PublishedListSort::IdDesc,
        EntryFilters::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.0, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
}
