use super::*;
use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn migration_0045_seeds_five_core_blocks_enforces_409_conflict_and_preserves_withdrawn() {
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    crate::storage::content::preflight_builtin_key_collisions(&db)
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();
    ensure_embedded_core_blocks(&db).await.unwrap();

    let schema_cnt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(
        schema_cnt < crate::storage::restore::MAX_SCHEMA_OBJECTS,
        "sqlite_schema count {schema_cnt} must stay below 256"
    );

    let core_cnt: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM builder_block_definitions WHERE owner_kind = 'core' AND provider_status = 'active' AND declaration_state = 'declared'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(core_cnt, EMBEDDED_CORE_BLOCK_SOURCES.len() as i64);

    let mut conn = db.acquire().await.unwrap();
    let b1: BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "acme-plugin/cta",
        "title": "CTA",
        "category": "marketing",
        "render": "render.html",
        "fields": [
            { "type": "text", "key": "label", "label": "Label" }
        ]
    }))
    .unwrap();
    let b2: BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "acme-plugin/old",
        "title": "Old",
        "category": "marketing",
        "render": "render.html",
        "fields": [
            { "type": "text", "key": "label", "label": "Label" }
        ]
    }))
    .unwrap();

    sync_owner_blocks_in_tx(
        &mut conn,
        "plugin",
        "acme-plugin",
        ProviderStatus::Active,
        "rev-1",
        &[
            (b1.clone(), "<div>{{ label }}</div>".into()),
            (b2.clone(), "<div>{{ label }}</div>".into()),
        ],
    )
    .await
    .unwrap();

    // Cross-provider collision on acme-plugin/cta by another owner must return 409 Conflict
    let err = sync_owner_blocks_in_tx(
        &mut conn,
        "theme",
        "acme-plugin",
        ProviderStatus::Active,
        "rev-t1",
        &[(b1.clone(), "<div>steal</div>".into())],
    )
    .await
    .unwrap_err();
    assert_eq!(err.0, StatusCode::CONFLICT);

    // Revision 2 removes acme-plugin/old -> becomes withdrawn + provider_inactive
    sync_owner_blocks_in_tx(
        &mut conn,
        "plugin",
        "acme-plugin",
        ProviderStatus::Active,
        "rev-2",
        &[(b1, "<div>{{ label }}</div>".into())],
    )
    .await
    .unwrap();

    // Disabling then re-enabling acme-plugin must NOT revive withdrawn acme-plugin/old
    set_owner_block_status_in_tx(
        &mut conn,
        "plugin",
        "acme-plugin",
        ProviderStatus::ProviderInactive,
    )
    .await
    .unwrap();
    set_owner_block_status_in_tx(&mut conn, "plugin", "acme-plugin", ProviderStatus::Active)
        .await
        .unwrap();

    let catalog = load_unified_block_catalog_in_conn(&mut conn).await.unwrap();
    let cta_entry = catalog.get("acme-plugin/cta").unwrap();
    assert_eq!(cta_entry.status, ProviderStatus::Active);
    assert_eq!(cta_entry.declaration_state, DeclarationState::Declared);

    let old_entry = catalog.get("acme-plugin/old").unwrap();
    assert_eq!(old_entry.status, ProviderStatus::ProviderInactive);
    assert_eq!(old_entry.declaration_state, DeclarationState::Withdrawn);
}

#[tokio::test]
async fn provider_toggle_resyncs_media_acl_without_erasing_saved_homepage() {
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    crate::storage::content::preflight_builtin_key_collisions(&db)
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();
    ensure_embedded_core_blocks(&db).await.unwrap();

    let image_id = "a".repeat(64);
    sqlx::query(
        "INSERT INTO media_assets(id,kind,filename,mime_type,extension,byte_size,width,height,thumb_width,thumb_height)
         VALUES (?, 'image', 'picture.png', 'image/png', 'png', 16, 1, 1, 1, 1)",
    )
    .bind(&image_id)
    .execute(&db)
    .await
    .unwrap();

    let schema: BlockSchemaV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "id": "acme-blocks/banner",
        "title": "Banner",
        "category": "media",
        "render": "render.html",
        "fields": [
            { "type": "media", "key": "photo", "label": "Photo", "media_kind": "any" }
        ]
    }))
    .unwrap();
    let document: BuilderDocument = serde_json::from_value(json!({
        "schema_version": opendash_core::builder::BUILDER_SCHEMA_VERSION,
        "sections": [{
            "id": "s1", "children": [{
                "id": "r1", "children": [{
                    "id": "c1", "children": [{
                        "id": "e1", "type": "plugin/block",
                        "props": { "block_id": "acme-blocks/banner", "values": { "photo": image_id } }
                    }]
                }]
            }]
        }]
    }))
    .unwrap();
    let original_json = serde_json::to_string(&document).unwrap();
    sqlx::query("UPDATE builder_homepage SET document_json = ? WHERE id = 1")
        .bind(&original_json)
        .execute(&db)
        .await
        .unwrap();

    let mut conn = db.acquire().await.unwrap();
    sync_owner_blocks_in_tx(
        &mut conn,
        "plugin",
        "acme-blocks",
        ProviderStatus::Active,
        "revision-1",
        &[(schema, "<div>Static banner</div>".into())],
    )
    .await
    .unwrap();
    resync_plugin_block_acls_in_tx(&mut conn).await.unwrap();
    let inert_media: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM builder_homepage_media")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(
        inert_media, 0,
        "resync must leave builder_homepage_media inert"
    );

    // A published Page can project its public Builder body, and template
    // overrides track active vs inactive plugin blocks independently of the
    // inert builder_homepage row.
    sqlx::query(
        "INSERT INTO template_overrides(target_kind, canonical_target_key, target_json, ast_json, version)
         VALUES ('index', 'index', '{\"kind\":\"index\"}', ?, 1)",
    )
    .bind(&original_json)
    .execute(&mut *conn)
    .await
    .unwrap();
    resync_plugin_block_acls_in_tx(&mut conn).await.unwrap();
    let ov_initial: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM template_override_media_refs")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(ov_initial, 1);
    let body = serialize_builder_document_to_blocks(&document).unwrap();
    sqlx::query(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
         VALUES ('page', 'private-media-test', 'Media test', ?, 'draft', 'vi', 'eg-' || lower(hex(randomblob(8))))",
    )
    .bind(json!({ "body": body }).to_string())
    .execute(&mut *conn)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE content_entries SET publication_status = 'published' WHERE slug = 'private-media-test'",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    resync_plugin_block_acls_in_tx(&mut conn).await.unwrap();
    let public_refs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_builder_media_refs WHERE media_id = ?")
            .bind(&image_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!(public_refs, 1);

    let raw_page_definition: String =
        sqlx::query_scalar("SELECT definition_json FROM content_types WHERE key = 'page'")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    let mut page_definition: opendash_core::content::ContentTypeDefinition =
        serde_json::from_str(&raw_page_definition).unwrap();
    page_definition
        .fields
        .iter_mut()
        .find(|field| field.key == "body")
        .unwrap()
        .acl = Some(opendash_core::field_schema::FieldAcl {
        public_exposure: Some(PublicExposure::Private),
        ..Default::default()
    });
    sqlx::query("UPDATE content_types SET definition_json = ? WHERE key = 'page'")
        .bind(serde_json::to_string(&page_definition).unwrap())
        .execute(&mut *conn)
        .await
        .unwrap();
    resync_plugin_block_acls_in_tx(&mut conn).await.unwrap();
    let private_refs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entry_builder_media_refs")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(private_refs, 0, "private body must not project media ACL");

    set_owner_block_status_in_tx(
        &mut conn,
        "plugin",
        "acme-blocks",
        ProviderStatus::ProviderInactive,
    )
    .await
    .unwrap();
    resync_plugin_block_acls_in_tx(&mut conn).await.unwrap();
    let revoked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM template_override_media_refs")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(revoked, 0);

    set_owner_block_status_in_tx(&mut conn, "plugin", "acme-blocks", ProviderStatus::Active)
        .await
        .unwrap();
    resync_plugin_block_acls_in_tx(&mut conn).await.unwrap();
    let restored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM template_override_media_refs")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(restored, 1);
    let saved: String = sqlx::query_scalar("SELECT document_json FROM builder_homepage WHERE id=1")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(saved, original_json, "resync must not mutate saved AST");
}

#[tokio::test]
async fn s01_core_sync_cleanup_list_offset_and_fail_closed_locale_filters() {
    use opendash_core::builder::{
        BuilderDocument, ColumnNode, ElementContent, ElementNode, KnownElement, ListProps, RowNode,
        SectionNode,
    };

    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    crate::storage::content::preflight_builtin_key_collisions(&db)
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();
    ensure_embedded_core_blocks(&db).await.unwrap();

    let mut conn = db.acquire().await.unwrap();
    sqlx::query(
        "INSERT INTO builder_block_definitions
            (block_id, owner_kind, owner_id, provider_status, declaration_state, schema_json, template, revision_digest)
         VALUES ('core/stale-obsolete', 'core', 'core', 'active', 'declared', '{}', '<div></div>', 'stale-rev')",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    ensure_embedded_core_blocks_in_conn(&mut conn)
        .await
        .unwrap();
    let stale_cnt: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM builder_block_definitions WHERE block_id = 'core/stale-obsolete'",
    )
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    assert_eq!(stale_cnt, 0);
    let declared = load_declared_blocks_in_conn(&mut conn).await.unwrap();
    assert!(declared.len() >= EMBEDDED_CORE_BLOCK_SOURCES.len());
    assert!(!declared.contains_key("core/stale-obsolete"));

    for idx in 1..=3 {
        sqlx::query(
            "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
             VALUES ('post', ?, ?, '{}', 'draft', 'vi', ?)",
        )
        .bind(format!("s01-vi-post-{idx}"))
        .bind(format!("VI Post {idx}"))
        .bind(format!("tg-s01-vi-{idx}"))
        .execute(&mut *conn)
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE content_entries SET publication_status = 'published' WHERE translation_group_id LIKE 'tg-s01-vi-%'",
    )
    .execute(&mut *conn)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO site_locales (code,bcp47_tag,label,direction,is_default,enabled,position)
         VALUES ('en','en','English','ltr',0,1,1)",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    sqlx::query("UPDATE site_locales_state SET multilingual_enabled=1 WHERE id=1")
        .execute(&mut *conn)
        .await
        .unwrap();
    let vi_cat_id: i64 = sqlx::query_scalar(
        "INSERT INTO taxonomy_terms(kind, locale, translation_group_id, name, slug, version)
         VALUES ('category', 'vi', 'tg-s01-cat', 'Tin tức', 'tin-tuc', 1)
         RETURNING id",
    )
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    assert_eq!(
        crate::storage::builder::resolve_term_id_for_locale_in_conn(
            &mut conn, vi_cat_id, "category", "en",
        )
        .await
        .unwrap(),
        None
    );
    drop(conn);

    let list_doc = |cat_id: i64| -> BuilderDocument {
        let props: ListProps = serde_json::from_value(
            json!({ "type_key": "post", "page_size": 5, "category_id": cat_id }),
        )
        .unwrap();
        BuilderDocument {
            schema_version: 1,
            sections: vec![SectionNode {
                id: "s1".into(),
                meta: None,
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "r1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "c1".into(),
                        meta: None,
                        responsive: None,
                        children: vec![ElementNode {
                            id: "list-1".into(),
                            meta: None,
                            content: ElementContent::Known(KnownElement::List(props)),
                        }],
                    }],
                }],
            }],
        }
    };

    let mut tx = db.begin().await.unwrap();
    let vi_filters = crate::storage::entries::EntryFilters {
        category_id: None,
        tag_id: None,
        taxonomy_key: None,
        term_id: None,
        include_descendants: false,
        locale: Some("vi".into()),
    };
    let crate::storage::entries::PublishedListResult::Page(offset_page) =
        crate::storage::entries::list_published_page_with_offset_in_tx(
            &mut tx,
            "post",
            1,
            24,
            1,
            crate::storage::entries::PublishedListSort::IdDesc,
            vi_filters.clone(),
        )
        .await
        .unwrap()
    else {
        panic!("expected published page");
    };
    assert_eq!(offset_page.entries.len(), 2);
    assert_eq!(offset_page.entries[0].slug, "s01-vi-post-2");
    assert_eq!(offset_page.entries[1].slug, "s01-vi-post-1");
    assert!(
        crate::storage::entries::list_published_page_with_offset_in_tx(
            &mut tx,
            "post",
            1,
            25,
            0,
            crate::storage::entries::PublishedListSort::IdDesc,
            vi_filters.clone(),
        )
        .await
        .is_err()
    );
    assert!(
        crate::storage::entries::list_published_page_with_offset_in_tx(
            &mut tx,
            "post",
            1,
            24,
            201,
            crate::storage::entries::PublishedListSort::IdDesc,
            vi_filters,
        )
        .await
        .is_err()
    );

    let mut budget = crate::template_sec::RenderBudget::new();
    let en_untranslated = crate::storage::builder::resolve_lists_in_tx_for_locale_with_budget(
        &mut tx,
        &list_doc(vi_cat_id),
        &std::collections::BTreeMap::new(),
        Some("en"),
        &mut budget,
    )
    .await
    .unwrap();
    assert_eq!(budget.builder_lists_used, 1);
    match en_untranslated.get("list-1").unwrap() {
        crate::storage::builder::ResolvedList::Page { items, .. } => assert!(items.is_empty()),
        other => panic!("expected empty fail-closed Page, got {other:?}"),
    }

    let missing = crate::storage::builder::resolve_lists_in_tx_for_locale_with_budget(
        &mut tx,
        &list_doc(999_999),
        &std::collections::BTreeMap::new(),
        Some("en"),
        &mut budget,
    )
    .await
    .unwrap();
    assert!(matches!(
        missing.get("list-1"),
        Some(crate::storage::builder::ResolvedList::MissingConfig)
    ));

    let en_cat_id: i64 = sqlx::query_scalar(
        "INSERT INTO taxonomy_terms(kind, locale, translation_group_id, name, slug, version)
         VALUES ('category', 'en', 'tg-s01-cat', 'News', 'news', 1)
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let en_post_id: i64 = sqlx::query_scalar(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
         VALUES ('post', 's01-en-post', 'EN Post', '{}', 'draft', 'en', 'tg-s01-en-1')
         RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = ?")
        .bind(en_post_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entry_terms(entry_id, term_id) VALUES (?, ?)")
        .bind(en_post_id)
        .bind(en_cat_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        crate::storage::builder::resolve_term_id_for_locale_in_conn(
            &mut *tx, vi_cat_id, "category", "en",
        )
        .await
        .unwrap(),
        Some(en_cat_id)
    );
    let en_resolved = crate::storage::builder::resolve_lists_in_tx_for_locale_with_budget(
        &mut tx,
        &list_doc(vi_cat_id),
        &std::collections::BTreeMap::new(),
        Some("en"),
        &mut budget,
    )
    .await
    .unwrap();
    match en_resolved.get("list-1").unwrap() {
        crate::storage::builder::ResolvedList::Page { items, .. } => {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].slug, "s01-en-post");
        }
        other => panic!("expected translated en list items, got {other:?}"),
    }
    tx.commit().await.unwrap();
}
