use super::*;

#[tokio::test]
async fn reading_settings_reclaims_home_and_posts_page_paths_without_collision() {
    use crate::storage::settings::{ReadingSettingsInput, save_reading};
    let db = site().await;
    let front = reading_page(&db, "front", true).await;
    let blog = reading_page(&db, "blog", true).await;
    let updates = reading_page(&db, "updates", true).await;

    let first = save_reading(
        &db,
        ReadingSettingsInput {
            homepage_page_id: Some(front),
            posts_page_id: Some(blog),
        },
        1,
    )
    .await
    .unwrap();
    assert_eq!(first.version, 2);
    assert_eq!(
        resolve_public_path(&db, "/").await.unwrap().unwrap().target,
        vi_home_entry(front)
    );
    let home_alias = resolve_public_path(&db, "/front").await.unwrap().unwrap();
    assert!(home_alias.redirect);
    assert_eq!(home_alias.canonical_path, "/");
    let blog_route = resolve_public_path(&db, "/blog").await.unwrap().unwrap();
    assert_eq!(blog_route.target, vi_archive("post"));
    assert!(!blog_route.redirect);
    let old_archive = resolve_public_path(&db, "/posts").await.unwrap().unwrap();
    assert_eq!(old_archive.canonical_path, "/blog");
    assert!(old_archive.redirect);
    assert_eq!(
        canonical_entry_path_in_tx(&mut db.acquire().await.unwrap(), blog)
            .await
            .unwrap(),
        None,
        "Posts Page must have no competing entry claim"
    );

    let moved = save_reading(
        &db,
        ReadingSettingsInput {
            homepage_page_id: Some(front),
            posts_page_id: Some(updates),
        },
        first.version,
    )
    .await
    .unwrap();
    assert_eq!(moved.version, 3);
    assert_eq!(
        resolve_public_path(&db, "/blog")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(blog),
        "Former Posts Page regains its Page claim"
    );
    assert_eq!(
        resolve_public_path(&db, "/updates")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_archive("post")
    );
    let unset = save_reading(
        &db,
        ReadingSettingsInput {
            homepage_page_id: None,
            posts_page_id: None,
        },
        moved.version,
    )
    .await
    .unwrap();
    assert_eq!(unset.version, 4);
    assert_eq!(
        resolve_public_path(&db, "/").await.unwrap().unwrap().target,
        vi_home()
    );
    assert_eq!(
        resolve_public_path(&db, "/front")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(front)
    );
    assert_eq!(
        resolve_public_path(&db, "/updates")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(updates)
    );
    assert_eq!(
        resolve_public_path(&db, "/posts")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_archive("post")
    );
    save_reading(
        &db,
        ReadingSettingsInput {
            homepage_page_id: Some(front),
            posts_page_id: Some(blog),
        },
        unset.version,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn reading_settings_reject_invalid_ids_and_stale_cas_without_writes() {
    use crate::storage::settings::{ReadingSettingsInput, get_reading, save_reading};
    let db = site().await;
    let draft = reading_page(&db, "draft-page", false).await;
    let published = reading_page(&db, "published-page", true).await;
    let post = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
         VALUES('post','article','Article','{}','draft','vi','eg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
        .bind(post)
        .execute(&db)
        .await
        .unwrap();
    for (home, posts) in [
        (Some(published), Some(published)),
        (Some(draft), None),
        (Some(post), None),
        (Some(999999), None),
        (Some(-1), None),
    ] {
        let error = save_reading(
            &db,
            ReadingSettingsInput {
                homepage_page_id: home,
                posts_page_id: posts,
            },
            1,
        )
        .await
        .unwrap_err();
        assert_eq!(error.0, axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(get_reading(&db).await.unwrap().version, 1);
    }
    let saved = save_reading(
        &db,
        ReadingSettingsInput {
            homepage_page_id: Some(published),
            posts_page_id: None,
        },
        1,
    )
    .await
    .unwrap();
    assert_eq!(saved.version, 2);
    let error = save_reading(
        &db,
        ReadingSettingsInput {
            homepage_page_id: None,
            posts_page_id: None,
        },
        1,
    )
    .await
    .unwrap_err();
    assert_eq!(error.0, axum::http::StatusCode::PRECONDITION_FAILED);
    assert_eq!(
        get_reading(&db).await.unwrap().homepage_page_id,
        Some(published)
    );
}

#[tokio::test]
async fn reading_settings_custom_blog_path_reclaims_page_alias_on_unset() {
    use crate::storage::settings::{ReadingSettingsInput, save_reading};
    let db = site().await;
    let blog = reading_page(&db, "blog", true).await;
    let mut tx = db.begin().await.unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    sqlx::query("INSERT INTO entry_public_paths(entry_id,locale,path) VALUES(?, 'vi', '/news')")
        .bind(blog)
        .execute(&db)
        .await
        .unwrap();

    let selected = save_reading(
        &db,
        ReadingSettingsInput {
            homepage_page_id: None,
            posts_page_id: Some(blog),
        },
        1,
    )
    .await
    .unwrap();
    assert_eq!(
        resolve_public_path(&db, "/news")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_archive("post")
    );
    let alias = resolve_public_path(&db, "/blog").await.unwrap().unwrap();
    assert_eq!(alias.canonical_path, "/news");
    assert!(alias.redirect);

    let unselected = save_reading(
        &db,
        ReadingSettingsInput {
            homepage_page_id: None,
            posts_page_id: None,
        },
        selected.version,
    )
    .await
    .unwrap();
    assert_eq!(
        resolve_public_path(&db, "/news")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(blog)
    );
    let page_alias = resolve_public_path(&db, "/blog").await.unwrap().unwrap();
    assert_eq!(page_alias.target, vi_entry(blog));
    assert_eq!(page_alias.canonical_path, "/news");

    save_reading(
        &db,
        ReadingSettingsInput {
            homepage_page_id: None,
            posts_page_id: Some(blog),
        },
        unselected.version,
    )
    .await
    .unwrap();
    assert_eq!(
        resolve_public_path(&db, "/news")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_archive("post")
    );
}

#[tokio::test]
async fn assigned_draft_and_deleted_pages_stay_dead_and_advance_delete_etag() {
    use crate::storage::settings::{
        ReadingPageStatus, ReadingSettingsInput, bump_reading_version_on_deleted_entry_in_tx,
        get_reading, save_reading,
    };
    let db = site().await;
    let front = reading_page(&db, "front", true).await;
    let blog = reading_page(&db, "blog", true).await;
    let saved = save_reading(
        &db,
        ReadingSettingsInput {
            homepage_page_id: Some(front),
            posts_page_id: Some(blog),
        },
        1,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE content_entries SET publication_status='draft' WHERE id IN (?,?)")
        .bind(front)
        .bind(blog)
        .execute(&db)
        .await
        .unwrap();
    let mut tx = db.begin().await.unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let draft = get_reading(&db).await.unwrap();
    assert_eq!(draft.homepage_status, ReadingPageStatus::Draft);
    assert_eq!(draft.posts_page_status, ReadingPageStatus::Draft);
    assert_eq!(draft.version, saved.version);
    assert!(resolve_public_path(&db, "/").await.unwrap().is_none());
    assert!(resolve_public_path(&db, "/blog").await.unwrap().is_none());

    let mut tx = db.begin().await.unwrap();
    sqlx::query("DELETE FROM content_entries WHERE id IN (?,?)")
        .bind(front)
        .bind(blog)
        .execute(&mut *tx)
        .await
        .unwrap();
    bump_reading_version_on_deleted_entry_in_tx(&mut tx, front)
        .await
        .unwrap();
    bump_reading_version_on_deleted_entry_in_tx(&mut tx, blog)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let deleted = get_reading(&db).await.unwrap();
    assert_eq!(deleted.homepage_page_id, Some(front));
    assert_eq!(deleted.posts_page_id, Some(blog));
    assert_eq!(deleted.homepage_status, ReadingPageStatus::Deleted);
    assert_eq!(deleted.posts_page_status, ReadingPageStatus::Deleted);
    assert_eq!(deleted.version, saved.version + 2);
    assert!(resolve_public_path(&db, "/").await.unwrap().is_none());
    assert!(resolve_public_path(&db, "/blog").await.unwrap().is_none());
    let claimed: (String, String) = sqlx::query_as(
        "SELECT target_kind,target_id FROM permalink_route_claims WHERE path='/blog'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(claimed, ("archive".into(), "post".into()));
}

#[tokio::test]
async fn upgrade_0040_preserves_explicit_site_null_and_absent_choices() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrator = &crate::storage::MIGRATOR;
    migrator.run_to(40, &db).await.unwrap();
    sqlx::query(
        r#"UPDATE site_permalink_settings
         SET content_type_bases_json='{"books":null,"magazines":"issues"}',
             taxonomy_bases_json='{"genre":null}' WHERE id=1"#,
    )
    .execute(&db)
    .await
    .unwrap();
    let before = get_settings(&db).await.unwrap();
    migrator.run(&db).await.unwrap();
    let after = get_settings(&db).await.unwrap();
    assert_eq!(after.post_structure, "/%postname%");
    assert_eq!(
        after.content_type_bases, before.content_type_bases,
        "migration must preserve explicit content_type_bases"
    );
    assert_eq!(
        after.taxonomy_bases, before.taxonomy_bases,
        "migration must preserve explicit taxonomy_bases"
    );
    assert_eq!(after.content_type_bases.get("books"), Some(&None));
    assert!(!after.content_type_bases.contains_key("not_selected"));
    assert_eq!(after.taxonomy_bases.get("genre"), Some(&None));
    let defaults: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_rewrite_defaults")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(defaults, 0);
}

#[tokio::test]
async fn provider_default_respects_site_choice_and_c3_version() {
    let db = site().await;
    let schema = serde_json::to_string(&opendash_core::content::ContentTypeDefinition {
        key: "books".to_owned(),
        label: "Books".to_owned(),
        singular_label: None,
        description: None,
        settings: Default::default(),
    })
    .unwrap();
    sqlx::query(
        "INSERT INTO content_types(key,definition_json,owner_kind,owner_id)
         VALUES('books',?,'plugin','books_plugin')",
    )
    .bind(schema)
    .execute(&db)
    .await
    .unwrap();
    let before = get_settings(&db).await.unwrap();
    let declaration = ProviderRewriteDeclaration {
        resource_kind: "content_type",
        resource_key: "books".to_owned(),
        rewrite_base: Some("library".to_owned()),
    };
    let mut tx = db.begin().await.unwrap();
    sync_provider_rewrite_defaults_in_tx(&mut tx, "plugin", "books_plugin", &[declaration.clone()])
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let enabled = get_settings(&db).await.unwrap();
    assert_eq!(enabled.version, before.version + 1);
    assert!(!enabled.content_type_bases.contains_key("books"));
    let mut conn = db.acquire().await.unwrap();
    let effective = read_effective_settings(&mut conn).await.unwrap();
    assert_eq!(
        effective.content_type_bases["books"],
        Some("library".into())
    );
    drop(conn);

    let mut changed = declaration.clone();
    changed.rewrite_base = Some("catalog".into());
    let mut tx = db.begin().await.unwrap();
    sync_provider_rewrite_defaults_in_tx(&mut tx, "plugin", "books_plugin", &[changed.clone()])
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let modified = get_settings(&db).await.unwrap();
    assert_eq!(
        modified.version,
        enabled.version + 1,
        "updating an effective provider default invalidates an older site editor CAS"
    );
    assert!(!modified.content_type_bases.contains_key("books"));
    let mut conn = db.acquire().await.unwrap();
    assert_eq!(
        read_effective_settings(&mut conn)
            .await
            .unwrap()
            .content_type_bases["books"],
        Some("catalog".to_owned())
    );
    drop(conn);

    let mut explicit_legacy = get_settings(&db).await.unwrap().input();
    explicit_legacy
        .content_type_bases
        .insert("books".into(), None);
    let saved = put_settings(&db, modified.version, &explicit_legacy)
        .await
        .unwrap();
    let mut tx = db.begin().await.unwrap();
    changed.rewrite_base = Some("reading".into());
    sync_provider_rewrite_defaults_in_tx(&mut tx, "plugin", "books_plugin", &[changed])
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let final_site = get_settings(&db).await.unwrap();
    assert_eq!(
        final_site.version, saved.version,
        "provider defaults hidden by site choice"
    );
    assert_eq!(final_site.content_type_bases.get("books"), Some(&None));
    let mut conn = db.acquire().await.unwrap();
    assert_eq!(
        read_effective_settings(&mut conn)
            .await
            .unwrap()
            .content_type_bases["books"],
        None
    );
    drop(conn);
    let mut tx = db.begin().await.unwrap();
    set_provider_rewrite_status_in_tx(&mut tx, "plugin", "books_plugin", false)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(get_settings(&db).await.unwrap().version, saved.version);
    let mut tx = db.begin().await.unwrap();
    set_provider_rewrite_status_in_tx(&mut tx, "plugin", "books_plugin", true)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        get_settings(&db).await.unwrap().version,
        saved.version,
        "explicit null must survive disable and re-enable"
    );
}
