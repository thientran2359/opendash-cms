use super::*;

#[tokio::test]
async fn upgrade_0039_custom_page_keeps_default_slug_redirect_without_blanket_aliases() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrator = &crate::storage::MIGRATOR;
    migrator.run_to(39, &db).await.unwrap();

    let page_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES ('page','upgrade-page','Upgrade','{}','draft')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
        .bind(page_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entry_public_paths(entry_id,path) VALUES(?, '/old-page-custom')")
        .bind(page_id)
        .execute(&db)
        .await
        .unwrap();
    let untouched = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES ('post','untouched-post','Untouched','{}','draft')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
        .bind(untouched)
        .execute(&db)
        .await
        .unwrap();

    migrator.run_to(40, &db).await.unwrap();
    let redirect_row: (String, String, String) = sqlx::query_as(
        "SELECT target_kind, target_id, source_path FROM permalink_route_redirects WHERE source_path='/content/page/upgrade-page'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        redirect_row,
        (
            "entry".to_owned(),
            page_id.to_string(),
            "/content/page/upgrade-page".to_owned(),
        ),
    );
    let claim_page: (String, String) = sqlx::query_as(
        "SELECT target_kind, target_id FROM permalink_route_claims WHERE path='/old-page-custom'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(claim_page, ("entry".to_owned(), page_id.to_string()));
    let claim_post: (String, String) = sqlx::query_as(
        "SELECT target_kind, target_id FROM permalink_route_claims WHERE path='/content/post/untouched-post'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(claim_post, ("entry".to_owned(), untouched.to_string()));
    let redirect_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permalink_route_redirects WHERE target_kind='entry'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        redirect_count, 1,
        "no blanket aliases for untouched entries"
    );
    migrator.run(&db).await.unwrap();
    assert_eq!(
        resolve_public_path(&db, "/content/page/upgrade-page")
            .await
            .unwrap(),
        Some(ResolvedPublicRoute {
            target: vi_entry(page_id),
            canonical_path: "/old-page-custom".to_owned(),
            redirect: true,
        }),
    );
    assert_eq!(
        resolve_public_path(&db, "/old-page-custom").await.unwrap(),
        Some(ResolvedPublicRoute {
            target: vi_entry(page_id),
            canonical_path: "/old-page-custom".to_owned(),
            redirect: false,
        }),
    );
    assert_eq!(
        resolve_public_path(&db, "/untouched-post").await.unwrap(),
        Some(ResolvedPublicRoute {
            target: vi_entry(untouched),
            canonical_path: "/untouched-post".to_owned(),
            redirect: false,
        }),
    );
}

#[tokio::test]
async fn upgrade_0039_custom_page_alias_collision_fails_closed_without_partial_migration() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrator = &crate::storage::MIGRATOR;
    migrator.run_to(39, &db).await.unwrap();

    let page_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES('page','upgrade-page','Upgrade','{}','draft')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("INSERT INTO entry_public_paths(entry_id,path) VALUES(?,'/old-page-custom')")
        .bind(page_id)
        .execute(&db)
        .await
        .unwrap();
    // Different Page claims exactly the old implicit URL of page_id.
    let other_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES('page','other-page','Other','{}','draft')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query(
        "INSERT INTO entry_public_paths(entry_id,path) VALUES(?,'/content/page/upgrade-page')",
    )
    .bind(other_id)
    .execute(&db)
    .await
    .unwrap();

    let err = migrator.run(&db).await.unwrap_err();
    assert!(
        err.to_string().contains("CHECK constraint failed"),
        "expected fail-closed canonical/alias collision: {err:?}"
    );
    let migrated_0040: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version=40 AND success=1")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(migrated_0040, 0);
    let claims_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='permalink_route_claims'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        claims_table, 0,
        "failed migration must not leave partial 0040 tables"
    );
    let existing_path: String =
        sqlx::query_scalar("SELECT path FROM entry_public_paths WHERE entry_id=?")
            .bind(other_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(existing_path, "/content/page/upgrade-page");
}

#[tokio::test]
async fn upgrade_0039_reuses_same_owner_redirect_preserving_original_timestamp() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrator = &crate::storage::MIGRATOR;
    migrator.run_to(39, &db).await.unwrap();
    let page_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES('page','upgrade-page','Upgrade','{}','draft')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("INSERT INTO entry_public_paths(entry_id,path) VALUES(?,'/old-page-custom')")
        .bind(page_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO entry_redirects(source_path,entry_id,created_at)
         VALUES('/content/page/upgrade-page',?,'2026-11-18T01:23:45Z')",
    )
    .bind(page_id)
    .execute(&db)
    .await
    .unwrap();
    migrator.run_to(40, &db).await.unwrap();
    let (owner, created_at): (String, String) = sqlx::query_as(
        "SELECT target_id,created_at FROM permalink_route_redirects
          WHERE source_path='/content/page/upgrade-page'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(owner, page_id.to_string());
    assert_eq!(created_at, "2026-11-18T01:23:45Z");
}

#[tokio::test]
async fn legacy_draft_is_private_and_redirect_tracks_stable_entry_id() {
    let db = site().await;
    let id = add_post(&db, "first", false).await;
    assert_eq!(resolve_public_path(&db, "/first").await.unwrap(), None);
    let config = get_settings(&db).await.unwrap();
    assert_eq!(config.version, 1);
    assert_eq!(config.post_structure, "/%postname%");
    let changed = put_settings(
        &db,
        config.version,
        &PermalinkSettingsInput {
            post_structure: "/blog/%postname%".into(),
            content_type_bases: BTreeMap::new(),
            taxonomy_bases: BTreeMap::new(),
        },
    )
    .await
    .unwrap();
    assert_eq!(changed.version, 2);
    // Draft moving from /first to /blog/first never records a redirect at /first.
    assert_eq!(resolve_public_path(&db, "/first").await.unwrap(), None);

    let mut tx = db.begin().await.unwrap();
    sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    // Pre-publish draft URL /first stays 404 even after publishing on /blog/first.
    assert_eq!(resolve_public_path(&db, "/first").await.unwrap(), None);
    assert_eq!(
        resolve_public_path(&db, "/blog/first").await.unwrap(),
        Some(ResolvedPublicRoute {
            target: vi_entry(id),
            canonical_path: "/blog/first".into(),
            redirect: false,
        }),
    );

    let moved = put_settings(
        &db,
        changed.version,
        &PermalinkSettingsInput {
            post_structure: "/news/%postname%".into(),
            content_type_bases: BTreeMap::new(),
            taxonomy_bases: BTreeMap::new(),
        },
    )
    .await
    .unwrap();
    // Published entry moving /blog/first -> /news/first records 301 at /blog/first.
    assert_eq!(
        resolve_public_path(&db, "/blog/first").await.unwrap(),
        Some(ResolvedPublicRoute {
            target: vi_entry(id),
            canonical_path: "/news/first".into(),
            redirect: true,
        }),
    );
    // Reclaimed custom URL that equals the generated route is unambiguous.
    let mut tx = db.begin().await.unwrap();
    sqlx::query("INSERT INTO entry_public_paths(entry_id,locale,path) VALUES(?,'vi',?)")
        .bind(id)
        .bind("/news/first")
        .execute(&mut *tx)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let prior = get_settings(&db).await.unwrap();
    assert_eq!(prior.version, moved.version);
    assert_eq!(
        put_settings(&db, 1, &prior.input())
            .await
            .unwrap_err()
            .status(),
        axum::http::StatusCode::PRECONDITION_FAILED
    );
}

#[tokio::test]
async fn conflicting_permalinks_abort_configuration_and_preserve_history() {
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
    let id = add_post(&db, "admin", true).await;
    let bad = PermalinkSettingsInput {
        post_structure: "/%postname%".into(),
        content_type_bases: BTreeMap::new(),
        taxonomy_bases: BTreeMap::new(),
    };
    let err = put_settings(&db, blog.version, &bad).await.unwrap_err();
    assert_eq!(err.status(), axum::http::StatusCode::CONFLICT);
    assert_eq!(get_settings(&db).await.unwrap(), blog);
    assert_eq!(
        canonical_entry_path_in_tx(&mut db.acquire().await.unwrap(), id)
            .await
            .unwrap(),
        Some("/blog/admin".into())
    );

    let mut other = blog.input();
    other.post_structure = "/blog/%postname%".into();
    other
        .taxonomy_bases
        .insert("category".into(), Some("blog".into()));
    let err = put_settings(&db, blog.version, &other).await.unwrap_err();
    assert_eq!(err.status(), axum::http::StatusCode::CONFLICT);
    assert_eq!(get_settings(&db).await.unwrap(), blog);
    assert!(
        resolve_public_path(&db, "/blog/admin")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn term_history_follows_rewrite_and_slug_updates_and_hides_private_terms() {
    let db = site().await;
    let id = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Sea','sea','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let mut tx = db.begin().await.unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let previous = get_settings(&db).await.unwrap();
    let mut changed = previous.input();
    changed
        .taxonomy_bases
        .insert("category".into(), Some("genres".into()));
    put_settings(&db, previous.version, &changed).await.unwrap();
    assert_eq!(
        resolve_public_path(&db, "/category/sea")
            .await
            .unwrap()
            .unwrap(),
        ResolvedPublicRoute {
            target: vi_term(id),
            canonical_path: "/genres/sea".into(),
            redirect: true,
        },
    );
    let mut tx = db.begin().await.unwrap();
    sqlx::query("UPDATE taxonomy_terms SET slug='ocean' WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        resolve_public_path(&db, "/category/sea")
            .await
            .unwrap()
            .unwrap()
            .canonical_path,
        "/genres/ocean",
    );
    assert_eq!(
        resolve_public_path(&db, "/genres/sea")
            .await
            .unwrap()
            .unwrap()
            .canonical_path,
        "/genres/ocean",
    );
    sqlx::query("UPDATE taxonomy_definitions SET public=0 WHERE key='category'")
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        resolve_public_path(&db, "/genres/ocean").await.unwrap(),
        None
    );
    assert_eq!(resolve_public_path(&db, "/genres/sea").await.unwrap(), None);
}

#[tokio::test]
async fn failed_writer_claim_rebuild_rolls_back_original_entry() {
    let db = site().await;
    let id = add_post(&db, "safe", true).await;
    let config = get_settings(&db).await.unwrap();
    assert_eq!(config.post_structure, "/%postname%");
    let mut tx = db.begin().await.unwrap();
    sqlx::query("UPDATE content_entries SET slug='admin' WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let collision = sync_claims_in_tx(&mut tx).await.unwrap_err();
    assert_eq!(collision.0, axum::http::StatusCode::CONFLICT);
    tx.rollback().await.unwrap();
    let slug: String = sqlx::query_scalar("SELECT slug FROM content_entries WHERE id=?")
        .bind(id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(slug, "safe");
    assert_eq!(
        resolve_public_path(&db, "/safe")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(id),
    );
    assert_eq!(resolve_public_path(&db, "/admin").await.unwrap(), None);
}
