use super::*;

#[tokio::test]
async fn c1_visibility_hides_single_archive_taxonomy_media_seo_and_sitemap() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();
    sqlx::query(
        r#"INSERT INTO content_types(key,definition_json,owner_kind)
           VALUES('news', '{"key":"news","label":"News","settings":{"public":false,"has_archive":false,"publication":"draft_publish","supports":["editor","featured_image"]}}', 'user')"#,
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO content_field_groups(group_id,owner_kind,owner_id,local_key,title,description,fields_json,domain,location_rules_json,presentation_json,active,status,version)
           VALUES('user:news_fields','user',NULL,'news_fields','News Fields','','[{"key":"hero","label":"Hero","type":"media","acl":{"min_view_role":"editor","min_edit_role":"editor","public_exposure":"public"}}]','content','[[{"param":"content_type","op":"eq","value":"news"}]]','{"region":"main_bottom","order":10,"collapsible":true,"collapsed_by_default":false,"style":"card","label_placement":"top"}',1,'active',1)"#,
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO content_field_group_targets(group_id,target_kind,target_key,association_state) VALUES('user:news_fields','content_type','news','declared')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json)
                 VALUES('news','[\"tag\"]')",
    )
    .execute(&db)
    .await
    .unwrap();
    let hero_id = "a".repeat(64);
    let seo_image_id = "b".repeat(64);
    for id in [&hero_id, &seo_image_id] {
        sqlx::query(
            "INSERT INTO media_assets(id,kind,filename,mime_type,extension,byte_size,width,height,thumb_width,thumb_height)
             VALUES(?, 'image', 'public-visibility.png', 'image/png', 'png', 128, 10, 10, 10, 10)",
        )
        .bind(id)
        .execute(&db)
        .await
        .unwrap();
    }
    let term_id = sqlx::query(
        "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Visibility','visibility','vi','tg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let entry_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
         VALUES('news','hidden-news','Hidden', json_object('hero', ?), 'published', 'vi', 'eg-' || lower(hex(randomblob(8))))",
    )
    .bind(&hero_id)
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES(?,?)")
        .bind(entry_id)
        .bind(term_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO entry_public_paths(entry_id,locale,path) VALUES(?, 'vi', '/hidden-news')",
    )
    .bind(entry_id)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO seo_public_media_refs(source_kind,source_id,media_id)
         VALUES('entry_seo', ?, ?)",
    )
    .bind(entry_id)
    .bind(&seo_image_id)
    .execute(&db)
    .await
    .unwrap();
    synchronize_direct_sql_fixture(&db).await;

    assert!(
        get_by_slug(&db, "news", "hidden-news")
            .await
            .unwrap()
            .is_none()
    );
    assert!(get_published_by_id(&db, entry_id).await.unwrap().is_none());
    assert_eq!(
        list_published(&db, "news", 10, 0, EntryFilters::default())
            .await
            .unwrap_err()
            .0,
        StatusCode::NOT_FOUND
    );
    let page = taxonomy_public_page(
        &db,
        TaxonomyKind::Tag,
        "visibility",
        PublicFilters::default(),
        10,
        0,
    )
    .await
    .unwrap()
    .unwrap()
    .1;
    assert_eq!(page.total, 0);
    assert!(page.entries.is_empty());
    assert!(!media_is_referenced(&db, &hero_id).await.unwrap());
    assert!(!media_is_referenced(&db, &seo_image_id).await.unwrap());
    assert_eq!(canonical_entry_by_id(&db, entry_id).await.unwrap(), None);
    assert!(matches!(
        crate::storage::seo::resolve_custom_or_redirect_path(&db, "/hidden-news")
            .await
            .unwrap(),
        crate::storage::seo::PublicRouteResolution::NotFound
    ));
    let mut conn = db.acquire().await.unwrap();
    let routes = crate::storage::sitemap::list_indexable_routes_in_tx(
        &mut conn,
        "https://example.com",
        0,
        100,
    )
    .await
    .unwrap();
    assert!(
        !routes
            .routes
            .iter()
            .any(|route| { route.path.contains("news") || route.path.contains("visibility") })
    );
    drop(conn);

    // Toggling back to public must restore singles, taxonomy and media,
    // but the separately disabled archive must remain 404.
    sqlx::query(
        "UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.public', json('true')) WHERE key='news'",
    )
    .execute(&db)
    .await
    .unwrap();
    synchronize_direct_sql_fixture(&db).await;
    assert!(
        get_by_slug(&db, "news", "hidden-news")
            .await
            .unwrap()
            .is_some()
    );
    assert!(get_published_by_id(&db, entry_id).await.unwrap().is_some());
    assert_eq!(
        list_published(&db, "news", 10, 0, EntryFilters::default())
            .await
            .unwrap_err()
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        taxonomy_public_page(
            &db,
            TaxonomyKind::Tag,
            "visibility",
            PublicFilters::default(),
            10,
            0
        )
        .await
        .unwrap()
        .unwrap()
        .1
        .total,
        1
    );
    sqlx::query("INSERT OR IGNORE INTO entry_builder_media_refs(entry_id, media_id, ref_role, source_kind) VALUES (?, ?, 'image', 'field')")
        .bind(entry_id)
        .bind(&hero_id)
        .execute(&db)
        .await
        .unwrap();
    assert!(media_is_referenced(&db, &hero_id).await.unwrap());
    assert!(media_is_referenced(&db, &seo_image_id).await.unwrap());
    assert_eq!(
        canonical_entry_by_id(&db, entry_id)
            .await
            .unwrap()
            .as_deref(),
        Some("/hidden-news")
    );
    let mut conn = db.acquire().await.unwrap();
    let routes = crate::storage::sitemap::list_indexable_routes_in_tx(
        &mut conn,
        "https://example.com",
        0,
        100,
    )
    .await
    .unwrap();
    assert!(
        routes
            .routes
            .iter()
            .any(|route| route.path == "/hidden-news")
    );
    assert!(!routes.routes.iter().any(|route| route.path == "/news"));
    drop(conn);

    sqlx::query(
        "UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.has_archive', json('true')) WHERE key='news'",
    )
    .execute(&db)
    .await
    .unwrap();
    assert_eq!(
        list_published(&db, "news", 10, 0, EntryFilters::default())
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        get_by_slug(&db, "pattern", "hidden-news")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        list_published(&db, "pattern", 10, 0, EntryFilters::default())
            .await
            .unwrap_err()
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn s01_generic_builder_body_and_media_follow_capability_and_publication_without_mode_gate() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();
    sqlx::query(
        r#"INSERT INTO content_types(key, definition_json, owner_kind)
            VALUES('project', '{"key":"project","label":"Project","settings":{"public":true,"has_archive":true,"publication":"draft_publish","supports":["editor","builder"]}}', 'user')"#,
    )
    .execute(&db)
    .await
    .unwrap();
    let media_id = "7".repeat(64);
    let document: opendash_core::builder::BuilderDocument = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "blocks": [{"id":"section", "block_id":"core/section", "children":[{"id":"row", "block_id":"core/row", "children":[
            {"id":"column", "block_id":"core/column", "children":[
                {"id":"heading", "block_id":"core/heading",
                    "props":{"level":"h2","text":"S01 generic project body"}},
                {"id":"picture", "block_id":"core/image",
                    "props":{"image":media_id,"alt":"Project picture"}}
            ]}
        ]}]}]
    }))
    .unwrap();
    let blocks =
        opendash_core::builder_blocks::serialize_builder_document_to_blocks(&document).unwrap();
    let values_json = serde_json::json!({ "body": blocks }).to_string();
    let entry_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, content_mode, locale, translation_group_id)
         VALUES('project', 's01-builder', 'Builder project', ?, 'published', 'builder', 'vi', 'eg-' || lower(hex(randomblob(8))))",
    )
    .bind(&values_json)
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query(
        "INSERT INTO media_assets(id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
         VALUES(?, 'image', 'builder-test.png', 'image/png', 'png', 128, 10, 10, 10, 10)",
    )
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO entry_builder_media_refs(entry_id, media_id, ref_role) VALUES(?, ?, 'image')",
    )
    .bind(entry_id)
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();

    let rendered = load_entry_render_snapshot(&db, entry_id, None)
        .await
        .unwrap()
        .unwrap();
    let body = rendered.body.unwrap().html;
    assert_eq!(rendered.entry.title, "Builder project");
    assert!(body.contains("S01 generic project body"));
    assert!(media_is_referenced(&db, &media_id).await.unwrap());

    // Unified `values_json["body"]` renders Builder blocks & grants public media ACL
    // regardless of `content_mode` preference (`editor` vs `builder`).
    sqlx::query("UPDATE content_entries SET content_mode = 'editor' WHERE id = ?")
        .bind(entry_id)
        .execute(&db)
        .await
        .unwrap();
    let rendered_in_editor_mode = load_entry_render_snapshot(&db, entry_id, None)
        .await
        .unwrap()
        .unwrap();
    assert!(
        rendered_in_editor_mode
            .body
            .unwrap()
            .html
            .contains("S01 generic project body")
    );
    assert!(media_is_referenced(&db, &media_id).await.unwrap());

    // Disabling ContentSupport::Builder revokes Builder rendering and public media grant
    // immediately while keeping `values_json["body"]` intact.
    sqlx::query(
        "UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.supports', json('[\"editor\"]')) WHERE key = 'project'",
    )
    .execute(&db)
    .await
    .unwrap();
    assert!(
        load_entry_render_snapshot(&db, entry_id, None)
            .await
            .unwrap()
            .unwrap()
            .body
            .is_none()
    );
    assert!(!media_is_referenced(&db, &media_id).await.unwrap());
    let saved_values: String =
        sqlx::query_scalar("SELECT values_json FROM content_entries WHERE id = ?")
            .bind(entry_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&saved_values).unwrap()["body"],
        blocks
    );

    sqlx::query("UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.supports', json('[\"editor\",\"builder\"]')) WHERE key = 'project'")
        .execute(&db).await.unwrap();
    assert!(media_is_referenced(&db, &media_id).await.unwrap());
    sqlx::query("UPDATE content_entries SET publication_status = 'draft' WHERE id = ?")
        .bind(entry_id)
        .execute(&db)
        .await
        .unwrap();
    assert!(
        load_entry_render_snapshot(&db, entry_id, None)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!media_is_referenced(&db, &media_id).await.unwrap());
}

#[tokio::test]
async fn s01_one_hop_pattern_acl_and_private_cpt_follow_unified_body_without_content_mode() {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();
    sqlx::query(
        r#"INSERT INTO content_types(key, definition_json, owner_kind)
            VALUES('service', '{"key":"service","label":"Service","settings":{"public":true,"has_archive":true,"publication":"draft_publish","supports":["editor","builder"]}}', 'user')"#,
    )
    .execute(&db)
    .await
    .unwrap();

    let media_id = "9".repeat(64);
    sqlx::query(
        "INSERT INTO media_assets(id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
         VALUES(?, 'image', 'pattern-img.png', 'image/png', 'png', 128, 10, 10, 10, 10)",
    )
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();

    let pattern_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, content_mode, locale, translation_group_id)
         VALUES('pattern', 'hero-pat', 'Hero Pattern', '{}', 'draft', 'builder', 'vi', 'eg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let pat_doc = serde_json::json!({
        "schema_version": 1,
        "blocks": [{"id":"s1","block_id":"core/section","children":[{"id":"r1","block_id":"core/row","children":[{"id":"c1","block_id":"core/column","children":[
            {"id":"img1","block_id":"core/image","props":{"image":media_id,"alt":"Pattern photo"}}
        ]}]}]}]
    });
    sqlx::query(
        "INSERT INTO entry_builder_documents(entry_id, document_json, updated_at) VALUES(?, ?, datetime('now'))",
    )
    .bind(pattern_id)
    .bind(pat_doc.to_string())
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO entry_builder_media_refs(entry_id, media_id, ref_role) VALUES(?, ?, 'image')",
    )
    .bind(pattern_id)
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();

    let blocks = format!(
        "<!-- od:core/section {{\"id\":\"sec-1\"}} -->\n\
         <!-- od:core/row {{\"id\":\"row-1\"}} -->\n\
         <!-- od:core/column {{\"id\":\"col-1\"}} -->\n\
         <!-- od:core/pattern-ref {{\"id\":\"pat-1\",\"props\":{{\"pattern_id\":\"{pattern_id}\"}}}} /-->\n\
         <!-- /od:core/column -->\n\
         <!-- /od:core/row -->\n\
         <!-- /od:core/section -->"
    );
    let parent_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, content_mode, locale, translation_group_id)
         VALUES('service', 'svc-a', 'Service A', ?, 'draft', 'editor', 'vi', 'eg-' || lower(hex(randomblob(8))))",
    )
    .bind(serde_json::json!({ "body": blocks }).to_string())
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("INSERT INTO entry_pattern_refs(source_entry_id, target_pattern_id) VALUES(?, ?)")
        .bind(parent_id)
        .bind(pattern_id)
        .execute(&db)
        .await
        .unwrap();

    // Draft parent keeps pattern media private.
    assert!(!media_is_referenced(&db, &media_id).await.unwrap());

    // Published public Builder parent unlocks 1-hop pattern media even with `content_mode = 'editor'`.
    sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = ?")
        .bind(parent_id)
        .execute(&db)
        .await
        .unwrap();
    assert!(media_is_referenced(&db, &media_id).await.unwrap());

    // Private CPT (`public = false`) hides 1-hop pattern media again.
    sqlx::query(
        "UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.public', json('false')) WHERE key = 'service'",
    )
    .execute(&db)
    .await
    .unwrap();
    assert!(!media_is_referenced(&db, &media_id).await.unwrap());
}

#[tokio::test]
async fn adjacent_entries_use_idx_0063_without_temp_btree_and_respect_visibility() {
    let dir = std::env::temp_dir().join(format!(
        "opendash-s02-adjacent-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let db = crate::storage::init_db(&dir.join("cms.db")).await.unwrap();
    super::synchronize_direct_sql_fixture(&db).await;

    for (slug, status, published_at) in [
        ("p-old", "published", "2026-11-20T09:00:00Z"),
        ("p-mid-a", "published", "2026-11-22T09:00:00Z"),
        ("p-mid-b", "published", "2026-11-22T09:00:00Z"),
        ("p-draft", "draft", "2026-11-23T09:00:00Z"),
        ("p-new", "published", "2026-11-25T09:00:00Z"),
    ] {
        let eid = super::add_entry(&db, "post", slug, status == "published", &[]).await;
        sqlx::query("UPDATE content_entries SET published_at = ? WHERE id = ?")
            .bind(published_at)
            .bind(eid)
            .execute(&db)
            .await
            .unwrap();
    }

    let mid_b_id: i64 = sqlx::query_scalar(
        "SELECT id FROM content_entries WHERE content_type_key = 'post' AND slug = 'p-mid-b'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let mid_a_id: i64 = sqlx::query_scalar(
        "SELECT id FROM content_entries WHERE content_type_key = 'post' AND slug = 'p-mid-a'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let new_id: i64 = sqlx::query_scalar(
        "SELECT id FROM content_entries WHERE content_type_key = 'post' AND slug = 'p-new'",
    )
    .fetch_one(&db)
    .await
    .unwrap();

    for (newer, tax_filter) in [
        (
            false,
            crate::storage::public_content::AdjacentTaxFilter::None,
        ),
        (
            true,
            crate::storage::public_content::AdjacentTaxFilter::None,
        ),
        (
            false,
            crate::storage::public_content::AdjacentTaxFilter::PrimaryCategory,
        ),
        (
            true,
            crate::storage::public_content::AdjacentTaxFilter::PrimaryCategory,
        ),
    ] {
        let plan_sql = format!(
            "EXPLAIN QUERY PLAN {}",
            crate::storage::public_content::build_adjacent_sql(newer, tax_filter)
        );
        let plan_rows: Vec<(i64, i64, i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(plan_sql))
            .fetch_all(&db)
            .await
            .unwrap();
        let detail = plan_rows
            .iter()
            .map(|r| r.3.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            detail.contains("idx_content_entries_pub_order"),
            "expected idx_content_entries_pub_order in plan: {detail}"
        );
        assert!(
            !detail.contains("USE TEMP B-TREE FOR ORDER BY"),
            "unexpected temp B-tree sort in plan: {detail}"
        );
    }

    let mut tx = db.begin().await.unwrap();
    let adj = crate::storage::public_content::query_adjacent_entries_in_tx(
        &mut tx, mid_b_id, "post", "vi", false, false,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(adj.prev.as_ref().map(|p| p.id), Some(mid_a_id));
    assert_eq!(adj.next.as_ref().map(|n| n.id), Some(new_id));
    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}
