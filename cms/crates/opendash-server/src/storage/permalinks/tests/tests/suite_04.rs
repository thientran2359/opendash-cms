use super::*;

#[tokio::test]
async fn upgrade_0045_sets_wordpress_defaults_triggers_and_cleans_draft_redirects() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrator = &crate::storage::MIGRATOR;
    migrator.run_to(44, &db).await.unwrap();

    sqlx::query(
        "INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state,version)
         VALUES('books', json_object('key','books','label','Books','fields',json('[]'),'settings',json_object('public',json('true'),'has_archive',json('true'))), 'plugin','bk','active','declared',1)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO provider_rewrite_defaults(resource_kind,resource_key,owner_kind,owner_id,rewrite_base)
         VALUES('content_type','books','plugin','bk','library')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE site_permalink_settings SET content_type_bases_json='{\"books\":null}' WHERE id=1",
    )
    .execute(&db)
    .await
    .unwrap();

    let page_about = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES('page','about','About','{}','draft')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let post_about = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES('post','about','About Post','{}','draft')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id IN (?,?)")
        .bind(page_about)
        .bind(post_about)
        .execute(&db)
        .await
        .unwrap();
    let post_draft = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES('post','draft-item','Draft','{}','draft')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("INSERT INTO entry_public_paths(entry_id,path) VALUES(?,'/studio/custom')")
        .bind(post_draft)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entry_redirects(source_path,entry_id) VALUES('/draft-old',?)")
        .bind(post_draft)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO permalink_route_redirects(source_path,target_kind,target_id) VALUES('/draft-route','entry',?)",
    )
    .bind(post_draft.to_string())
    .execute(&db)
    .await
    .unwrap();

    migrator.run(&db).await.unwrap();

    let settings = get_settings(&db).await.unwrap();
    assert_eq!(settings.post_structure, "/%postname%");
    assert_eq!(
        resolve_public_path(&db, "/about")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(page_about)
    );
    let disambiguated_slug: String =
        sqlx::query_scalar("SELECT slug FROM content_entries WHERE id=?")
            .bind(post_about)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(disambiguated_slug, format!("about-{post_about}"));
    assert_eq!(
        resolve_public_path(&db, &format!("/{disambiguated_slug}"))
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(post_about)
    );
    // Explicit null keeps system default /books instead of provider 'library'.
    assert_eq!(
        resolve_public_path(&db, "/books")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_archive("books")
    );
    // Reserved /studio custom path & draft redirects cleaned up.
    let custom_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_public_paths WHERE path='/studio/custom'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(custom_count, 0);
    assert_eq!(resolve_public_path(&db, "/draft-old").await.unwrap(), None);
    let redir_left: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permalink_route_redirects WHERE source_path='/draft-route'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(redir_left, 0);

    // Monotonic triggers keep has_been_published = 1 on publish and revert to draft.
    let inserted_pub = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
         VALUES('books','trigger-ins','Trigger','{}','published','vi','eg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let hbp_ins: i64 =
        sqlx::query_scalar("SELECT has_been_published FROM content_entries WHERE id=?")
            .bind(inserted_pub)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(hbp_ins, 1);

    sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
        .bind(post_draft)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE content_entries SET publication_status='draft' WHERE id=?")
        .bind(post_draft)
        .execute(&db)
        .await
        .unwrap();
    let hbp_draft_reverted: i64 =
        sqlx::query_scalar("SELECT has_been_published FROM content_entries WHERE id=?")
            .bind(post_draft)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(hbp_draft_reverted, 1);
}

#[tokio::test]
async fn category_permalink_uses_primary_min_or_uncategorized_and_guards_roots() {
    let db = site().await;
    let root_page = reading_page(&db, "about", true).await;
    let cat_a = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Alpha','alpha','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let cat_b = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Beta','beta','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let post_primary = add_post(&db, "story", true).await;
    let post_none = add_post(&db, "plain", true).await;
    sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES(?,?),(?,?)")
        .bind(post_primary)
        .bind(cat_a)
        .bind(post_primary)
        .bind(cat_b)
        .execute(&db)
        .await
        .unwrap();

    let s1 = get_settings(&db).await.unwrap();
    let s2 = put_settings(
        &db,
        s1.version,
        &PermalinkSettingsInput {
            post_structure: "/%category%/%postname%".into(),
            content_type_bases: BTreeMap::new(),
            taxonomy_bases: BTreeMap::new(),
        },
    )
    .await
    .unwrap();
    // Page stays at root, post_primary picks min(term_id)=alpha, post_none falls back to uncategorized.
    assert_eq!(
        resolve_public_path(&db, "/about")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(root_page)
    );
    assert_eq!(
        resolve_public_path(&db, "/alpha/story")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(post_primary)
    );
    assert_eq!(
        resolve_public_path(&db, "/uncategorized/plain")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(post_none)
    );

    // Selecting cat_b as primary switches route to /beta/story with 301 at /alpha/story.
    let mut tx = db.begin().await.unwrap();
    sqlx::query("UPDATE content_entries SET primary_category_term_id=? WHERE id=?")
        .bind(cat_b)
        .bind(post_primary)
        .execute(&mut *tx)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        resolve_public_path(&db, "/beta/story")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(post_primary)
    );
    assert!(
        resolve_public_path(&db, "/alpha/story")
            .await
            .unwrap()
            .unwrap()
            .redirect
    );

    // Switching to a reserved category slug is rejected under /%category%/%postname%.
    sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Media','media','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap();
    let mut tx = db.begin().await.unwrap();
    assert_eq!(
        sync_claims_in_tx(&mut tx).await.unwrap_err().0,
        axum::http::StatusCode::CONFLICT
    );
    tx.rollback().await.unwrap();

    // /blog/%category%/%postname% (3 segs) allows CPT with /blog/{slug} (2 segs) but rejects /blog/docs/{slug} (3 segs).
    sqlx::query("DELETE FROM taxonomy_terms WHERE slug='media'")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state,version)
         SELECT 'project',definition_json,'user',NULL,'active','declared',1 FROM content_types WHERE key='post'",
    ).execute(&db).await.unwrap();
    let mut ok_mixed = s2.input();
    ok_mixed.post_structure = "/blog/%category%/%postname%".into();
    ok_mixed
        .content_type_bases
        .insert("project".into(), Some("blog".into()));
    let s3 = put_settings(&db, s2.version, &ok_mixed).await.unwrap();
    let mut conflict_3seg = s3.input();
    conflict_3seg
        .content_type_bases
        .insert("project".into(), Some("blog/docs".into()));
    assert_eq!(
        put_settings(&db, s3.version, &conflict_3seg)
            .await
            .unwrap_err()
            .status(),
        axum::http::StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn put_settings_collects_structured_409_conflicts_capped_at_20() {
    let db = site().await;
    let v1 = get_settings(&db).await.unwrap();
    let blog = put_settings(
        &db,
        v1.version,
        &PermalinkSettingsInput {
            post_structure: "/blog/%postname%".into(),
            content_type_bases: BTreeMap::new(),
            taxonomy_bases: BTreeMap::new(),
        },
    )
    .await
    .unwrap();
    for i in 1..=25 {
        reading_page(&db, &format!("dup-{i}"), true).await;
        add_post(&db, &format!("dup-{i}"), true).await;
    }
    let err = put_settings(
        &db,
        blog.version,
        &PermalinkSettingsInput {
            post_structure: "/%postname%".into(),
            content_type_bases: BTreeMap::new(),
            taxonomy_bases: BTreeMap::new(),
        },
    )
    .await
    .unwrap_err();
    match err {
        PermalinkPutError::Conflicts(payload) => {
            assert_eq!(payload.total_conflicts, 25);
            assert!(payload.truncated);
            assert_eq!(payload.conflicts.len(), 20);
        }
        other => panic!("expected structured PermalinkPutError::Conflicts, got {other:?}"),
    }
}

#[tokio::test]
async fn max_redirects_prunes_oldest_orphans_fifo_before_rejecting() {
    assert_eq!(MAX_REDIRECTS, 200_000);
    const TEST_LIMIT: usize = 25;
    let db = site().await;
    sqlx::query(
        "WITH RECURSIVE cnt(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM cnt WHERE x <= 26)
         INSERT INTO permalink_route_redirects(source_path, target_kind, target_id, locale, origin, created_at)
         SELECT '/old-orphan-' || x, 'entry', CAST(1000000 + x AS TEXT), 'vi', 'canonical',
                strftime('%Y-%m-%dT%H:%M:%fZ', 1700000000 + x, 'unixepoch')
           FROM cnt",
    )
    .execute(&db)
    .await
    .unwrap();

    let mut tx = db.begin().await.unwrap();
    sync_claims_with_options_and_swap_and_limit(&mut tx, false, None, TEST_LIMIT)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM permalink_route_redirects")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(remaining as usize, TEST_LIMIT);
    // Oldest orphan (/old-orphan-1) pruned FIFO while newest remains.
    let oldest: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permalink_route_redirects WHERE source_path='/old-orphan-1'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(oldest, 0);
}
