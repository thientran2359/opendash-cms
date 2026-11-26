use super::*;

#[tokio::test]
async fn c2_archive_root_eligibility_gates_count_rows_and_cross_filters() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO taxonomy_definitions(key,label,owner_kind) VALUES('brand','Brand','user')",
    )
    .execute(&db)
    .await
    .unwrap();
    let mut term_ids = Vec::new();
    for (kind, slug) in [
        ("category", "c2-root-category"),
        ("tag", "c2-root-tag"),
        ("brand", "c2-extra-brand"),
    ] {
        let id = sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES(?, ?, ?, 'vi', 'tg-' || lower(hex(randomblob(8))))")
            .bind(kind)
            .bind(slug)
            .bind(slug)
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();
        term_ids.push(id);
    }
    sqlx::query("INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json) VALUES('post','[\"category\",\"tag\",\"brand\"]')")
        .execute(&db).await.unwrap();
    let mut published_ids = Vec::new();
    for index in 0..3 {
        published_ids.push(
            add_entry(
                &db,
                "post",
                &format!("c2-p2-published-{index}"),
                true,
                &term_ids,
            )
            .await,
        );
    }
    add_entry(&db, "post", "c2-p2-draft", false, &term_ids).await;

    let generic_brand = PublicFilters {
        taxonomy_key: Some("brand".into()),
        term_id: Some(term_ids[2]),
        ..PublicFilters::default()
    };
    for (kind, slug) in [
        (TaxonomyKind::Category, "c2-root-category"),
        (TaxonomyKind::Tag, "c2-root-tag"),
    ] {
        for filters in [PublicFilters::default(), generic_brand.clone()] {
            let first = taxonomy_public_page(&db, kind, slug, filters.clone(), 2, 0)
                .await
                .unwrap()
                .unwrap()
                .1;
            let second = taxonomy_public_page(&db, kind, slug, filters.clone(), 2, 2)
                .await
                .unwrap()
                .unwrap()
                .1;
            assert_eq!(first.total, 3, "{kind:?} count before withdrawal");
            assert_eq!(second.total, 3, "{kind:?} count on second page");
            assert_eq!(
                first.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
                [published_ids[2], published_ids[1]]
            );
            assert_eq!(
                second.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
                [published_ids[0]]
            );
        }
    }

    // Retain all links while removing only the archive root's CPT association.
    // The extra generic brand remains eligible: it must never bypass the root.
    sqlx::query("UPDATE content_type_taxonomy_overrides SET taxonomies_json='[\"brand\"]' WHERE content_type_key='post'")
        .execute(&db).await.unwrap();
    for (kind, slug) in [
        (TaxonomyKind::Category, "c2-root-category"),
        (TaxonomyKind::Tag, "c2-root-tag"),
    ] {
        for filters in [PublicFilters::default(), generic_brand.clone()] {
            for offset in [0, 2] {
                let page = taxonomy_public_page(&db, kind, slug, filters.clone(), 2, offset)
                    .await
                    .unwrap()
                    .unwrap()
                    .1;
                assert_eq!(page.total, 0, "{kind:?} must not count withdrawn root");
                assert!(
                    page.entries.is_empty(),
                    "{kind:?} must not page withdrawn root"
                );
            }
        }
    }
    let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entry_terms")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(
        links, 12,
        "withdrawing eligibility must preserve term memberships"
    );

    // Without a site override, the provider declaration is authoritative.
    sqlx::query("DELETE FROM content_type_taxonomy_overrides WHERE content_type_key='post'")
        .execute(&db)
        .await
        .unwrap();
    let provider_rows = sqlx::query("UPDATE content_type_taxonomy_eligibility SET provider_status='provider_inactive' WHERE content_type_key='post' AND taxonomy_kind IN ('category','tag')")
        .execute(&db).await.unwrap().rows_affected();
    assert_eq!(provider_rows, 2);
    for (kind, slug) in [
        (TaxonomyKind::Category, "c2-root-category"),
        (TaxonomyKind::Tag, "c2-root-tag"),
    ] {
        let page = taxonomy_public_page(&db, kind, slug, PublicFilters::default(), 2, 0)
            .await
            .unwrap()
            .unwrap()
            .1;
        assert_eq!(
            page.total, 0,
            "{kind:?} provider-inactive must exclude root"
        );
        assert!(page.entries.is_empty());
    }
    sqlx::query("UPDATE content_type_taxonomy_eligibility SET provider_status='active', declaration_state='withdrawn' WHERE content_type_key='post' AND taxonomy_kind IN ('category','tag')")
        .execute(&db).await.unwrap();
    for (kind, slug) in [
        (TaxonomyKind::Category, "c2-root-category"),
        (TaxonomyKind::Tag, "c2-root-tag"),
    ] {
        let page = taxonomy_public_page(&db, kind, slug, PublicFilters::default(), 2, 0)
            .await
            .unwrap()
            .unwrap()
            .1;
        assert_eq!(
            page.total, 0,
            "{kind:?} withdrawn declaration must exclude root"
        );
        assert!(page.entries.is_empty());
    }
}

#[tokio::test]
async fn taxonomy_cross_cpt_publication_and_filters_apply_before_count_and_page() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO content_types(key,definition_json,owner_kind)
         VALUES('news','{\"key\":\"news\",\"label\":\"News\",\"fields\":[]}', 'user')",
    )
    .execute(&db)
    .await
    .unwrap();
    // C2: these types explicitly opt into term association; Page alone
    // remains unassociated under the Core default migration.
    sqlx::query(
        "INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json)
                 VALUES('page','[\"category\",\"tag\"]'),('news','[\"category\",\"tag\"]')",
    )
    .execute(&db)
    .await
    .unwrap();

    let root = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Sea','sea','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let child = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,parent_id,locale,translation_group_id)
         VALUES('category','Reefs','reefs',?,'vi','tg-' || lower(hex(randomblob(8))))",
    )
    .bind(root)
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let tag =
        sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Fish','fish','vi','tg-' || lower(hex(randomblob(8))))")
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();
    let other_tag =
        sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Other','other','vi','tg-' || lower(hex(randomblob(8))))")
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();

    let post = add_entry(&db, "post", "post-a", true, &[root, child, tag]).await;
    let news = add_entry(&db, "news", "news-a", true, &[child, tag]).await;
    let page_id = add_entry(&db, "page", "page-a", true, &[child, tag]).await;
    add_entry(&db, "news", "news-other", true, &[child, other_tag]).await;
    let draft = add_entry(&db, "post", "draft-a", false, &[root, child, tag]).await;
    add_entry(&db, "news", "no-terms", true, &[]).await;
    synchronize_direct_sql_fixture(&db).await;
    let exact = taxonomy_public_page(
        &db,
        TaxonomyKind::Category,
        "sea",
        PublicFilters::default(),
        20,
        0,
    )
    .await
    .unwrap()
    .unwrap()
    .1;
    assert_eq!(exact.total, 1);
    assert_eq!(
        exact.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
        [post]
    );
    let descendants = PublicFilters {
        tag_id: Some(tag),
        descendants: true,
        ..PublicFilters::default()
    };
    let filtered = taxonomy_public_page(&db, TaxonomyKind::Category, "sea", descendants, 20, 0)
        .await
        .unwrap()
        .unwrap()
        .1;
    assert_eq!(filtered.total, 3);
    assert_eq!(
        filtered.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
        [page_id, news, post],
    );
    let tag_archive = taxonomy_public_page(
        &db,
        TaxonomyKind::Tag,
        "fish",
        PublicFilters {
            category_id: Some(root),
            descendants: true,
            ..PublicFilters::default()
        },
        20,
        0,
    )
    .await
    .unwrap()
    .unwrap()
    .1;
    assert_eq!(tag_archive.total, 3);
    assert!(
        tag_archive
            .entries
            .iter()
            .all(|e| e.publication_status == "published")
    );
    assert!(!tag_archive.entries.iter().any(|e| e.id == draft));
    assert!(
        taxonomy_public_page(
            &db,
            TaxonomyKind::Tag,
            "sea",
            PublicFilters::default(),
            20,
            0,
        )
        .await
        .unwrap()
        .is_none()
    );
    assert_eq!(
        taxonomy_public_page(
            &db,
            TaxonomyKind::Category,
            "sea",
            PublicFilters {
                tag_id: Some(child),
                ..PublicFilters::default()
            },
            20,
            0,
        )
        .await
        .err()
        .unwrap()
        .0,
        StatusCode::BAD_REQUEST,
    );
    assert_eq!(
        taxonomy_public_page(
            &db,
            TaxonomyKind::Category,
            "sea",
            PublicFilters {
                tag_id: Some(999_999),
                ..PublicFilters::default()
            },
            20,
            0,
        )
        .await
        .err()
        .unwrap()
        .0,
        StatusCode::BAD_REQUEST,
    );

    // Stable ID follows a slug change; old slug and draft remain invisible.
    sqlx::query("UPDATE content_entries SET slug='new-post-a' WHERE id=?")
        .bind(post)
        .execute(&db)
        .await
        .unwrap();
    synchronize_direct_sql_fixture(&db).await;
    assert!(get_by_slug(&db, "post", "post-a").await.unwrap().is_none());
    assert!(get_by_slug(&db, "post", "draft-a").await.unwrap().is_none());
    assert_eq!(
        canonical_entry_by_id(&db, post).await.unwrap().as_deref(),
        Some("/new-post-a")
    );
    assert_eq!(canonical_entry_by_id(&db, draft).await.unwrap(), None);
    sqlx::query("UPDATE taxonomy_terms SET slug='sea-new' WHERE id=?")
        .bind(root)
        .execute(&db)
        .await
        .unwrap();
    synchronize_direct_sql_fixture(&db).await;
    assert_eq!(
        canonical_term_by_id(&db, root).await.unwrap().as_deref(),
        Some("/category/sea-new")
    );
    assert!(
        taxonomy_public_page(
            &db,
            TaxonomyKind::Category,
            "sea",
            PublicFilters::default(),
            20,
            0,
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn category_pagination_deduplicates_memberships_and_excludes_newer_drafts() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
    let root =
        sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','A','a','vi','tg-' || lower(hex(randomblob(8))))")
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();
    let child = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,parent_id,locale,translation_group_id) VALUES('category','B','b',?,'vi','tg-' || lower(hex(randomblob(8))))",
    )
    .bind(root)
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let mut ids = Vec::new();
    for index in 0..23 {
        ids.push(
            add_entry(
                &db,
                "post",
                &format!("published-{index}"),
                true,
                &[root, child],
            )
            .await,
        );
    }
    let draft = add_entry(&db, "post", "new-draft", false, &[root, child]).await;
    let flags = PublicFilters {
        descendants: true,
        ..PublicFilters::default()
    };
    let first = taxonomy_public_page(&db, TaxonomyKind::Category, "a", flags.clone(), 20, 0)
        .await
        .unwrap()
        .unwrap()
        .1;
    assert_eq!(first.total, 23);
    assert_eq!(first.entries.len(), 20);
    assert_eq!(first.entries[0].id, *ids.last().unwrap());
    let second = taxonomy_public_page(&db, TaxonomyKind::Category, "a", flags, 20, 20)
        .await
        .unwrap()
        .unwrap()
        .1;
    assert_eq!(second.total, 23);
    assert_eq!(second.entries.len(), 3);
    assert_eq!(second.entries[2].id, ids[0]);
    assert!(
        first
            .entries
            .iter()
            .chain(second.entries.iter())
            .all(|e| e.id != draft)
    );
}
