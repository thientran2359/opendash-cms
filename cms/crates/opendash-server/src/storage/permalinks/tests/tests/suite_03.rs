use super::*;

#[tokio::test]
async fn soft_deleted_redirect_is_preserved_until_active_history_reuses_path() {
    let db = site().await;
    sqlx::query(
        "INSERT INTO site_locales(code,bcp47_tag,label,enabled,deleted_at)
         VALUES('en','en','English',0,'2026-01-01')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO permalink_route_redirects
         (source_path,target_kind,target_id,locale,origin,created_at)
         VALUES('/alpha','entry','999','en','canonical','2026-01-01')",
    )
    .execute(&db)
    .await
    .unwrap();

    assert!(resolve_public_path(&db, "/alpha").await.unwrap().is_none());
    // An active canonical claim may coexist with parked history.
    let id = add_post(&db, "alpha", true).await;
    let parked: (String, String, String) = sqlx::query_as(
        "SELECT target_id,locale,created_at FROM permalink_route_redirects WHERE source_path='/alpha'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(parked, ("999".into(), "en".into(), "2026-01-01".into()));
    assert_eq!(
        resolve_public_path(&db, "/alpha")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(id)
    );

    // Moving the live canonical path creates history at the same PK.
    let mut tx = db.begin().await.unwrap();
    sqlx::query("UPDATE content_entries SET slug='beta' WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    let redirect: (String, String, String) = sqlx::query_as(
        "SELECT target_id,locale,origin FROM permalink_route_redirects WHERE source_path='/alpha'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(redirect, (id.to_string(), "vi".into(), "canonical".into()));
    assert_eq!(
        resolve_public_path(&db, "/alpha").await.unwrap(),
        Some(ResolvedPublicRoute {
            target: vi_entry(id),
            canonical_path: "/beta".into(),
            redirect: true,
        })
    );
}

#[tokio::test]
async fn historical_post_redirect_cannot_shadow_new_cpt_dynamic_family() {
    let db = site().await;
    let post_id = add_post(&db, "alpha", true).await;
    let original = get_settings(&db).await.unwrap();
    let mut blog = original.input();
    blog.post_structure = "/blog/%postname%".to_owned();
    let updated = put_settings(&db, original.version, &blog).await.unwrap();
    let mut news = blog.clone();
    news.post_structure = "/news/%postname%".to_owned();
    let latest = put_settings(&db, updated.version, &news).await.unwrap();

    // /blog/alpha remains a 301 alias after moving the post to /news.
    assert_eq!(
        resolve_public_path(&db, "/blog/alpha").await.unwrap(),
        Some(ResolvedPublicRoute {
            target: vi_entry(post_id),
            canonical_path: "/news/alpha".into(),
            redirect: true,
        })
    );
    sqlx::query(
        "INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state,version)
         SELECT 'books',definition_json,'user',NULL,'active','declared',1
           FROM content_types WHERE key='post'",
    ).execute(&db).await.unwrap();
    let mut conflicting = news.clone();
    conflicting
        .content_type_bases
        .insert("books".into(), Some("blog".into()));

    // /blog itself can host an archive, but /blog/{slug} is already
    // occupied by a historical post redirect belonging to another family.
    let err = put_settings(&db, latest.version, &conflicting)
        .await
        .unwrap_err();
    assert_eq!(err.status(), axum::http::StatusCode::CONFLICT, "{err:?}");
    assert_eq!(get_settings(&db).await.unwrap(), latest);
    assert_eq!(
        resolve_public_path(&db, "/blog/alpha").await.unwrap(),
        Some(ResolvedPublicRoute {
            target: vi_entry(post_id),
            canonical_path: "/news/alpha".into(),
            redirect: true,
        })
    );
    assert_eq!(resolve_public_path(&db, "/blog").await.unwrap(), None);

    // Reclaiming the same old path for its original post restores a
    // direct canonical route; the pre-upgrade URL remains a one-hop 301.
    let reclaimed = put_settings(&db, latest.version, &blog).await.unwrap();
    assert_eq!(reclaimed.post_structure, "/blog/%postname%");
    assert_eq!(
        resolve_public_path(&db, "/blog/alpha").await.unwrap(),
        Some(ResolvedPublicRoute {
            target: vi_entry(post_id),
            canonical_path: "/blog/alpha".into(),
            redirect: false,
        })
    );
    assert_eq!(
        resolve_public_path(&db, "/alpha")
            .await
            .unwrap()
            .unwrap()
            .canonical_path,
        "/blog/alpha"
    );
}

#[tokio::test]
async fn legacy_migration_redirect_cannot_shadow_foreign_dynamic_family() {
    let db = site().await;
    let post_id = add_post(&db, "alpha", true).await;
    let mut config = get_settings(&db).await.unwrap();
    let mut news = config.input();
    news.post_structure = "/news/%postname%".into();
    config = put_settings(&db, config.version, &news).await.unwrap();

    sqlx::query(
        "INSERT INTO entry_redirects(locale,source_path,entry_id) VALUES('vi','/books/alpha', ?)",
    )
    .bind(post_id)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state,version)
         SELECT 'novels',definition_json,'user',NULL,'active','declared',1
           FROM content_types WHERE key='post'",
    ).execute(&db).await.unwrap();

    let mut bad = config.input();
    bad.content_type_bases
        .insert("novels".into(), Some("books".into()));
    let error = put_settings(&db, config.version, &bad).await.unwrap_err();
    assert_eq!(
        error.status(),
        axum::http::StatusCode::CONFLICT,
        "{error:?}"
    );
    assert_eq!(get_settings(&db).await.unwrap(), config);
    assert_eq!(
        resolve_public_path(&db, "/news/alpha")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(post_id)
    );
    assert_eq!(resolve_public_path(&db, "/books").await.unwrap(), None);
}

#[tokio::test]
async fn deleted_term_legacy_alias_remains_404_without_blocking_atomic_delete() {
    let db = site().await;
    let term_id = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Parent','parent','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let mut tx = db.begin().await.unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();

    let mut tx = db.begin().await.unwrap();
    sqlx::query("DELETE FROM taxonomy_terms WHERE id=?")
        .bind(term_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        resolve_public_path(&db, "/category/parent").await.unwrap(),
        None,
    );
    let historical: Option<(String, String)> = sqlx::query_as(
        "SELECT target_kind,target_id FROM permalink_route_redirects
          WHERE source_path='/category/parent'",
    )
    .fetch_optional(&db)
    .await
    .unwrap();
    assert_eq!(historical, Some(("term".to_owned(), term_id.to_string())));
}

#[tokio::test]
async fn deleted_post_legacy_alias_remains_404_without_blocking_atomic_delete() {
    let db = site().await;
    let post_id = add_post(&db, "old-post", true).await;
    let mut tx = db.begin().await.unwrap();
    sqlx::query("DELETE FROM content_entries WHERE id=?")
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(resolve_public_path(&db, "/old-post").await.unwrap(), None,);
}

#[tokio::test]
async fn deleted_rewritten_post_retains_dead_aliases_without_blocking_delete() {
    let db = site().await;
    let original = get_settings(&db).await.unwrap();
    let mut blog = original.input();
    blog.post_structure = "/blog/%postname%".to_owned();
    put_settings(&db, original.version, &blog).await.unwrap();
    let post_id = add_post(&db, "gone", true).await;
    assert_eq!(
        resolve_public_path(&db, "/blog/gone")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_entry(post_id),
    );

    let mut tx = db.begin().await.unwrap();
    sqlx::query("DELETE FROM content_entries WHERE id=?")
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(resolve_public_path(&db, "/blog/gone").await.unwrap(), None);
    assert_eq!(resolve_public_path(&db, "/gone").await.unwrap(), None);
}

#[tokio::test]
async fn deleted_rewritten_term_retains_dead_aliases_without_blocking_delete() {
    let db = site().await;
    let original = get_settings(&db).await.unwrap();
    let mut genres = original.input();
    genres
        .taxonomy_bases
        .insert("category".to_owned(), Some("genres".to_owned()));
    put_settings(&db, original.version, &genres).await.unwrap();
    let term_id = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Gone','gone','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let mut tx = db.begin().await.unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        resolve_public_path(&db, "/genres/gone")
            .await
            .unwrap()
            .unwrap()
            .target,
        vi_term(term_id),
    );

    let mut tx = db.begin().await.unwrap();
    sqlx::query("DELETE FROM taxonomy_terms WHERE id=?")
        .bind(term_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        resolve_public_path(&db, "/genres/gone").await.unwrap(),
        None
    );
    assert_eq!(
        resolve_public_path(&db, "/category/gone").await.unwrap(),
        None,
    );
}

#[tokio::test]
async fn dead_rewritten_post_alias_does_not_deadlock_new_cpt_family() {
    let db = site().await;
    let original = get_settings(&db).await.unwrap();
    let mut blog = original.input();
    blog.post_structure = "/blog/%postname%".to_owned();
    let blog_saved = put_settings(&db, original.version, &blog).await.unwrap();
    let post_id = add_post(&db, "gone", true).await;
    let mut tx = db.begin().await.unwrap();
    sqlx::query("DELETE FROM content_entries WHERE id=?")
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();

    let mut news = blog.clone();
    news.post_structure = "/news/%postname%".to_owned();
    let news_saved = put_settings(&db, blog_saved.version, &news).await.unwrap();
    sqlx::query(
        "INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state,version)
         SELECT 'books',definition_json,'user',NULL,'active','declared',1
           FROM content_types WHERE key='post'",
    )
    .execute(&db)
    .await
    .unwrap();
    let mut allowed = news.clone();
    allowed
        .content_type_bases
        .insert("books".into(), Some("blog".into()));
    let updated = put_settings(&db, news_saved.version, &allowed)
        .await
        .unwrap();
    assert_eq!(
        updated.content_type_bases.get("books"),
        Some(&Some("blog".into()))
    );
    assert_eq!(resolve_public_path(&db, "/blog/gone").await.unwrap(), None);
}

#[tokio::test]
async fn orphan_redirect_never_deadlocks_new_cpt_rewrite_while_exact_slug_stays_protected() {
    let db = site().await;
    sqlx::query(
        "INSERT INTO permalink_route_redirects(source_path,target_kind,target_id,locale,origin)
         VALUES('/books/orphan','entry','999999','vi','canonical')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO content_types(key,definition_json,owner_kind,owner_id,provider_status,declaration_state,version)
         SELECT 'books',definition_json,'user',NULL,'active','declared',1
           FROM content_types WHERE key='post'",
    )
    .execute(&db)
    .await
    .unwrap();

    let initial = get_settings(&db).await.unwrap();
    let mut attempted = initial.input();
    attempted
        .content_type_bases
        .insert("books".to_owned(), Some("books".to_owned()));
    let updated = put_settings(&db, initial.version, &attempted)
        .await
        .unwrap();
    assert_eq!(
        updated.content_type_bases.get("books"),
        Some(&Some("books".to_owned()))
    );
    assert_eq!(
        resolve_public_path(&db, "/books/orphan").await.unwrap(),
        None
    );
}

#[test]
fn claims_beyond_fifty_thousand_are_rejected_before_writing() {
    let mut claims = BTreeMap::new();
    let mut collector = ConflictCollector::new(false);
    for id in 1..=MAX_CLAIMS {
        add_claim(
            &mut claims,
            format!("/claim-{id}"),
            "entry",
            id.to_string(),
            "vi",
            Some("post".to_owned()),
            true,
            &mut collector,
        )
        .unwrap();
    }
    let err = add_claim(
        &mut claims,
        "/claim-overflow".into(),
        "entry",
        "200001".into(),
        "vi",
        Some("post".to_owned()),
        true,
        &mut collector,
    )
    .unwrap_err();
    assert_eq!(err.status(), axum::http::StatusCode::CONFLICT);
    assert_eq!(claims.len(), MAX_CLAIMS + 1);
}
