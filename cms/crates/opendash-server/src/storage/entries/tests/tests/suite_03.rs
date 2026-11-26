use super::*;

#[tokio::test]
async fn s02_auto_unique_slug_resolves_collisions_reserved_roots_and_64b_limit() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
    let actor = s02_admin_context(&db).await;

    let post_schema = crate::storage::content::load_effective_content_schema(&db, "post")
        .await
        .unwrap()
        .unwrap();
    let page_schema = crate::storage::content::load_effective_content_schema(&db, "page")
        .await
        .unwrap()
        .unwrap();
    let post_tok = post_schema.schema_token.clone();
    let page_tok = page_schema.schema_token.clone();

    // 1. Repeated posts with auto_unique_slug: true produce foo, foo-2, foo-3.
    for expected in ["foo", "foo-2", "foo-3"] {
        let mut input = base_composite_input(&post_tok, "foo", None);
        input.auto_unique_slug = true;
        let saved = composite_save(&db, &actor, "post", None, input)
            .await
            .unwrap();
        assert_eq!(saved.entry.slug, expected);
    }

    // 2. Page colliding with Post claims /foo, /foo-2, /foo-3 becomes foo-4;
    //    with an explicit custom_path, Page can reuse slug "foo".
    let mut page_foo = base_composite_input(&page_tok, "foo", None);
    page_foo.auto_unique_slug = true;
    let saved_page_foo = composite_save(&db, &actor, "page", None, page_foo)
        .await
        .unwrap();
    assert_eq!(saved_page_foo.entry.slug, "foo-4");

    let mut page_with_custom = base_composite_input(&page_tok, "foo", None);
    page_with_custom.auto_unique_slug = true;
    page_with_custom.custom_path = PatchField::Set("/custom-holder".to_owned());
    let saved_holder = composite_save(&db, &actor, "page", None, page_with_custom)
        .await
        .unwrap();
    assert_eq!(saved_holder.entry.slug, "foo");

    // 3. Reserved roots (media), default archive (/posts), and CPT archive (/project) disambiguate cleanly.
    sqlx::query(
        "INSERT INTO content_types(key, definition_json, owner_kind)
         VALUES('project', json_object('key','project','label','Project','fields',json('[]'),'settings',json_object('public',json('true'),'has_archive',json('true'))), 'user')",
    )
    .execute(&db)
    .await
    .unwrap();
    let mut tx = db.begin().await.unwrap();
    crate::storage::permalinks::sync_claims_in_tx(&mut tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    for (raw_slug, expected) in [
        ("media", "media-2"),
        ("posts", "posts-2"),
        ("project", "project-2"),
    ] {
        let mut input = base_composite_input(&page_tok, raw_slug, None);
        input.auto_unique_slug = true;
        let saved = composite_save(&db, &actor, "page", None, input)
            .await
            .unwrap();
        assert_eq!(saved.entry.slug, expected);
    }

    // 4. 64-byte boundary trims trailing '-' before appending '-2'.
    let long_base = format!("{}-xy", "a".repeat(61));
    assert_eq!(long_base.len(), 64);
    let mut long1 = base_composite_input(&post_tok, &long_base, None);
    long1.auto_unique_slug = true;
    let s1 = composite_save(&db, &actor, "post", None, long1)
        .await
        .unwrap();
    assert_eq!(s1.entry.slug, long_base);
    let mut long2 = base_composite_input(&post_tok, &long_base, None);
    long2.auto_unique_slug = true;
    let s2 = composite_save(&db, &actor, "post", None, long2)
        .await
        .unwrap();
    assert_eq!(s2.entry.slug, format!("{}-2", "a".repeat(61)));
    assert!(s2.entry.slug.len() <= 64);

    // 5. Opt-in false & non-composite POST /entries still return 409 Conflict.
    let strict_err = composite_save(
        &db,
        &actor,
        "post",
        None,
        base_composite_input(&post_tok, "foo", None),
    )
    .await
    .unwrap_err();
    assert_eq!(strict_err.0, axum::http::StatusCode::CONFLICT);

    let non_comp_err = create_entry(
        &db,
        &actor,
        "post",
        &post_schema.base.descriptor,
        &ContentEntryInput {
            title: "Dup".into(),
            slug: "foo".into(),
            locale: None,
            values: BTreeMap::new(),
            term_ids: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(non_comp_err.0, axum::http::StatusCode::CONFLICT);

    // 6. With post_structure = /%category%/%postname%: post can share slug with a root Page,
    //    while same-CPT slug still increments and forbidden %category% prefix rejects 409 immediately.
    let perm = crate::storage::permalinks::get_settings(&db).await.unwrap();
    crate::storage::permalinks::put_settings(
        &db,
        perm.version,
        &crate::storage::permalinks::PermalinkSettingsInput {
            post_structure: "/%category%/%postname%".to_owned(),
            content_type_bases: perm.content_type_bases,
            taxonomy_bases: perm.taxonomy_bases,
        },
    )
    .await
    .unwrap();
    let cat_news = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','News','news','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();

    let page_root = composite_save(
        &db,
        &actor,
        "page",
        None,
        base_composite_input(&page_tok, "root-story", None),
    )
    .await
    .unwrap();
    assert_eq!(page_root.entry.slug, "root-story");

    let mut post_in_news = base_composite_input(&post_tok, "root-story", None);
    post_in_news.auto_unique_slug = true;
    post_in_news.term_ids = Some(vec![cat_news]);
    let post_saved = composite_save(&db, &actor, "post", None, post_in_news)
        .await
        .unwrap();
    assert_eq!(post_saved.entry.slug, "root-story");

    // Synthetically inject a colliding category term to exercise the immediate 409 pre-check
    // in resolve_auto_unique_slug_in_tx when post_structure starts with /%category%/.
    let cat_project = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Project','project','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let mut forbidden_cat = base_composite_input(&post_tok, "bad-cat-post", None);
    forbidden_cat.auto_unique_slug = true;
    forbidden_cat.term_ids = Some(vec![cat_project]);
    let cat_err = composite_save(&db, &actor, "post", None, forbidden_cat)
        .await
        .unwrap_err();
    assert_eq!(cat_err.0, axum::http::StatusCode::CONFLICT);
    assert!(cat_err.1.contains("permalink_category_root_collision"));
}

#[tokio::test]
async fn s02_primary_category_validation_autoclear_and_redirect_history() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
    let actor = s02_admin_context(&db).await;

    let perm = crate::storage::permalinks::get_settings(&db).await.unwrap();
    crate::storage::permalinks::put_settings(
        &db,
        perm.version,
        &crate::storage::permalinks::PermalinkSettingsInput {
            post_structure: "/%category%/%postname%".to_owned(),
            content_type_bases: perm.content_type_bases,
            taxonomy_bases: perm.taxonomy_bases,
        },
    )
    .await
    .unwrap();

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
    let tag_id =
        sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','T1','t1','vi','tg-' || lower(hex(randomblob(8))))")
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();

    let post_schema = crate::storage::content::load_effective_content_schema(&db, "post")
        .await
        .unwrap()
        .unwrap();
    let page_schema = crate::storage::content::load_effective_content_schema(&db, "page")
        .await
        .unwrap()
        .unwrap();
    let post_tok = post_schema.schema_token.clone();
    let page_tok = page_schema.schema_token.clone();

    // Validation rejections -> 400 Bad Request
    let mut page_primary = base_composite_input(&page_tok, "page-pc", None);
    page_primary.primary_category_term_id = PatchField::Set(cat_a);
    assert_eq!(
        composite_save(&db, &actor, "page", None, page_primary)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );

    let mut zero_primary = base_composite_input(&post_tok, "zero-pc", None);
    zero_primary.primary_category_term_id = PatchField::Set(0);
    assert_eq!(
        composite_save(&db, &actor, "post", None, zero_primary)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );

    let mut tag_primary = base_composite_input(&post_tok, "tag-pc", None);
    tag_primary.term_ids = Some(vec![tag_id]);
    tag_primary.primary_category_term_id = PatchField::Set(tag_id);
    assert_eq!(
        composite_save(&db, &actor, "post", None, tag_primary)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );

    let mut unassigned_primary = base_composite_input(&post_tok, "unassigned-pc", None);
    unassigned_primary.term_ids = Some(vec![cat_a]);
    unassigned_primary.primary_category_term_id = PatchField::Set(cat_b);
    assert_eq!(
        composite_save(&db, &actor, "post", None, unassigned_primary)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );

    // Draft post changing primary category must NOT leave historical 301 redirect.
    let mut draft_in = base_composite_input(&post_tok, "pc-post", None);
    draft_in.publication_status = Some("draft".to_owned());
    draft_in.term_ids = Some(vec![cat_a, cat_b]);
    draft_in.primary_category_term_id = PatchField::Set(cat_b);
    let draft1 = composite_save(&db, &actor, "post", None, draft_in)
        .await
        .unwrap();
    assert_eq!(draft1.entry.primary_category_term_id, Some(cat_b));

    let mut draft_up = base_composite_input(&post_tok, "pc-post", Some(draft1.entry.version));
    draft_up.publication_status = Some("published".to_owned());
    draft_up.primary_category_term_id = PatchField::Set(cat_a);
    let published1 = composite_save(&db, &actor, "post", Some(draft1.entry.id), draft_up)
        .await
        .unwrap();
    assert_eq!(published1.entry.primary_category_term_id, Some(cat_a));
    assert_eq!(
        crate::storage::permalinks::resolve_public_path(&db, "/beta/pc-post")
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        crate::storage::permalinks::resolve_public_path(&db, "/alpha/pc-post")
            .await
            .unwrap()
            .unwrap()
            .canonical_path,
        "/alpha/pc-post"
    );

    // Published post updates primary_category_term_id WITHOUT resending term_ids -> 301 created.
    let mut switch_primary =
        base_composite_input(&post_tok, "pc-post", Some(published1.entry.version));
    switch_primary.publication_status = None;
    switch_primary.term_ids = None;
    switch_primary.primary_category_term_id = PatchField::Set(cat_b);
    let published2 = composite_save(
        &db,
        &actor,
        "post",
        Some(published1.entry.id),
        switch_primary,
    )
    .await
    .unwrap();
    assert_eq!(published2.entry.primary_category_term_id, Some(cat_b));
    let alpha_redir = crate::storage::permalinks::resolve_public_path(&db, "/alpha/pc-post")
        .await
        .unwrap()
        .unwrap();
    assert!(alpha_redir.redirect);
    assert_eq!(alpha_redir.canonical_path, "/beta/pc-post");

    // Removing cat_b while primary_category_term_id is Omitted auto-clears primary_category_term_id.
    let mut strip_cat_b =
        base_composite_input(&post_tok, "pc-post", Some(published2.entry.version));
    strip_cat_b.publication_status = None;
    strip_cat_b.term_ids = Some(vec![cat_a]);
    strip_cat_b.primary_category_term_id = PatchField::Omitted;
    let cleared = composite_save(&db, &actor, "post", Some(published2.entry.id), strip_cat_b)
        .await
        .unwrap();
    assert_eq!(cleared.entry.primary_category_term_id, None);
    assert_eq!(
        crate::storage::permalinks::resolve_public_path(&db, "/alpha/pc-post")
            .await
            .unwrap()
            .unwrap()
            .canonical_path,
        "/alpha/pc-post"
    );

    // Re-set primary = cat_b, then verify non-composite update_entry removing cat_b also clears it.
    let mut restore_cat_b = base_composite_input(&post_tok, "pc-post", Some(cleared.entry.version));
    restore_cat_b.publication_status = None;
    restore_cat_b.term_ids = Some(vec![cat_a, cat_b]);
    restore_cat_b.primary_category_term_id = PatchField::Set(cat_b);
    let restored = composite_save(
        &db,
        &actor,
        "post",
        Some(published2.entry.id),
        restore_cat_b,
    )
    .await
    .unwrap();
    assert_eq!(restored.entry.primary_category_term_id, Some(cat_b));

    let non_comp_updated = update_entry(
        &db,
        &actor,
        "post",
        &post_schema.base.descriptor,
        restored.entry.id,
        restored.entry.version,
        &ContentEntryInput {
            title: restored.entry.title.clone(),
            slug: restored.entry.slug.clone(),
            locale: None,
            values: restored.entry.values.clone(),
            term_ids: Some(vec![cat_a]),
        },
    )
    .await
    .unwrap();
    assert_eq!(non_comp_updated.primary_category_term_id, None);
}
