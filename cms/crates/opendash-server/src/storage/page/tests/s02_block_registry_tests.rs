use super::*;
use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn migration_0045_seeds_five_core_blocks_enforces_409_conflict_and_preserves_withdrawn() {
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    crate::storage::content::preflight_builtin_key_collisions(&db)
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
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
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
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
        .public_exposure = PublicExposure::Private;
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
