use super::*;
use opendash_core::{
    content::PublicExposure,
    theme::{TemplateCandidate, TemplateCandidateKind},
};
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;

const GALLERY_SCHEMA: &str = r#"{
    "schema_version": 1,
    "id": "reef/gallery",
    "title": "Gallery",
    "category": "media",
    "render": "render.html",
    "fields": [
        {"type": "media", "key": "hero", "label": "Hero", "media_kind": "any"},
        {"type": "repeater", "key": "slides", "label": "Slides", "fields": [
            {"type": "media", "key": "thumb", "label": "Thumbnail", "media_kind": "image"}
        ]}
    ]
}"#;

async fn fixture() -> (SqlitePool, BuilderDocument, String, String) {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO builder_block_definitions
          (block_id, owner_kind, owner_id, provider_status, declaration_state, schema_json, template, revision_digest)
         VALUES ('reef/gallery', 'plugin', 'reef', 'active', 'declared', ?, '<div></div>', 'fixture')",
    )
    .bind(GALLERY_SCHEMA)
    .execute(&db)
    .await
    .unwrap();
    let img = "a".repeat(64);
    let video = "b".repeat(64);
    sqlx::query(
        "INSERT INTO media_assets
         (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
         VALUES (?, 'image', 'image.png', 'image/png', 'png', 123, 10, 10, 10, 10)",
    )
    .bind(&img)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO media_assets
         (id, kind, filename, mime_type, extension, byte_size, width, height, duration_ms, video_codec)
         VALUES (?, 'video', 'video.mp4', 'video/mp4', 'mp4', 123, 10, 10, 1000, 'h264')",
    )
    .bind(&video)
    .execute(&db)
    .await
    .unwrap();
    let doc = serde_json::from_value(json!({
        "schema_version": 1,
        "sections": [{
            "id": "sec-1", "children": [{
                "id": "row-1", "children": [{
                    "id": "col-1", "children": [{
                        "id": "gallery-1", "type": "plugin/block",
                        "props": {"block_id": "reef/gallery",
                                  "values": {"hero": video, "slides": [{"thumb": img}]}}
                    }]
                }]
            }]
        }]
    }))
    .unwrap();
    (db, doc, img, video)
}

#[tokio::test]
async fn plugin_media_respects_catalog_kind_repeater_and_provider_state() {
    let (db, document, image_id, video_id) = fixture().await;
    let mut conn = db.acquire().await.unwrap();
    let refs = resolve_document_media_refs_in_conn(&mut conn, &document)
        .await
        .unwrap();
    assert_eq!(refs.image_ids, BTreeSet::from([image_id.clone()]));
    assert_eq!(refs.video_ids, BTreeSet::from([video_id.clone()]));
    assert!(refs.any_media_ids.is_empty());

    let raw =
        opendash_core::builder_blocks::serialize_builder_document_to_blocks(&document).unwrap();
    let body_refs = entries::validated_entry_block_refs_in_conn(&mut conn, &raw)
        .await
        .unwrap();
    assert_eq!(body_refs.image_ids, BTreeSet::from([image_id.clone()]));
    assert_eq!(body_refs.video_ids, BTreeSet::from([video_id.clone()]));

    let mut any_image = document.clone();
    {
        let ElementContent::Known(KnownElement::PluginBlock(props)) =
            &mut any_image.sections[0].children[0].children[0].children[0].content
        else {
            panic!("expected plugin block");
        };
        props.values.insert("hero".into(), json!(image_id));
    }
    let image_resolved = resolve_document_media_refs_in_conn(&mut conn, &any_image)
        .await
        .unwrap();
    assert_eq!(image_resolved.image_ids, BTreeSet::from([image_id.clone()]));
    assert!(image_resolved.video_ids.is_empty());

    {
        let ElementContent::Known(KnownElement::PluginBlock(props)) =
            &mut any_image.sections[0].children[0].children[0].children[0].content
        else {
            panic!("expected plugin block");
        };
        props
            .values
            .insert("slides".into(), json!([{"thumb": video_id}]));
    }
    let wrong_kind = resolve_document_media_refs_in_conn(&mut conn, &any_image)
        .await
        .unwrap_err();
    assert_eq!(wrong_kind.0, axum::http::StatusCode::BAD_REQUEST);
    let wrong_body =
        opendash_core::builder_blocks::serialize_builder_document_to_blocks(&any_image).unwrap();
    let wrong_body_error = entries::validated_entry_block_refs_in_conn(&mut conn, &wrong_body)
        .await
        .unwrap_err();
    assert_eq!(
        wrong_body_error.0,
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );

    sqlx::query(
        "UPDATE builder_block_definitions SET provider_status = 'provider_inactive'
         WHERE block_id = 'reef/gallery'",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    let inactive = resolve_document_media_refs_in_conn(&mut conn, &document)
        .await
        .unwrap();
    assert!(inactive.image_ids.is_empty());
    assert!(inactive.video_ids.is_empty());
    let inactive_body = entries::validated_entry_block_refs_in_conn(&mut conn, &raw)
        .await
        .unwrap();
    assert!(inactive_body.image_ids.is_empty());
    assert!(inactive_body.video_ids.is_empty());
}

#[tokio::test]
async fn plugin_media_public_read_rechecks_body_policy_with_stale_pattern_references() {
    let (db, _document, _image_id, video_id) = fixture().await;
    let parent_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES('page','stale-parent','Parent','{}','draft')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
        .bind(parent_id)
        .execute(&db)
        .await
        .unwrap();
    let pattern_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
         VALUES('pattern','stale-pattern','Pattern','{}','draft')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    for entry_id in [parent_id, pattern_id] {
        sqlx::query(
            "INSERT INTO entry_builder_media_refs(entry_id,media_id,ref_role) VALUES (?,?,'video')",
        )
        .bind(entry_id)
        .bind(&video_id)
        .execute(&db)
        .await
        .unwrap();
    }
    sqlx::query("INSERT INTO entry_pattern_refs(source_entry_id,target_pattern_id) VALUES (?,?)")
        .bind(parent_id)
        .bind(pattern_id)
        .execute(&db)
        .await
        .unwrap();
    assert!(
        crate::storage::public_content::media_is_referenced(&db, &video_id)
            .await
            .unwrap(),
        "published Builder body is allowed to make media public"
    );

    let definition_raw: String =
        sqlx::query_scalar("SELECT definition_json FROM content_types WHERE key='page'")
            .fetch_one(&db)
            .await
            .unwrap();
    let mut definition: opendash_core::content::ContentTypeDefinition =
        serde_json::from_str(&definition_raw).unwrap();
    let body = definition
        .fields
        .iter_mut()
        .find(|field| field.key == "body")
        .unwrap();
    body.public_exposure = PublicExposure::Private;
    sqlx::query("UPDATE content_types SET definition_json=? WHERE key='page'")
        .bind(serde_json::to_string(&definition).unwrap())
        .execute(&db)
        .await
        .unwrap();
    let persisted_links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entry_pattern_refs")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(persisted_links, 1, "resync must not destroy pattern links");
    assert!(
        !crate::storage::public_content::media_is_referenced(&db, &video_id)
            .await
            .unwrap(),
        "stale direct + Pattern projections must not grant private body media"
    );

    sqlx::query("DELETE FROM entry_builder_media_refs WHERE entry_id=?")
        .bind(parent_id)
        .execute(&db)
        .await
        .unwrap();
    definition
        .fields
        .iter_mut()
        .find(|field| field.key == "body")
        .unwrap()
        .public_exposure = PublicExposure::Public;
    sqlx::query("UPDATE content_types SET definition_json=? WHERE key='page'")
        .bind(serde_json::to_string(&definition).unwrap())
        .execute(&db)
        .await
        .unwrap();
    assert!(
        crate::storage::public_content::media_is_referenced(&db, &video_id)
            .await
            .unwrap(),
        "a still-linked Pattern must regain public access after body is public"
    );
}

#[tokio::test]
async fn plugin_media_projects_to_home_entry_and_template_acl_with_privacy_guard() {
    let (db, document, image_id, video_id) = fixture().await;
    let raw =
        opendash_core::builder_blocks::serialize_builder_document_to_blocks(&document).unwrap();
    let entry_id = sqlx::query(
        "INSERT INTO content_entries
          (content_type_key, slug, title, values_json, publication_status, content_mode)
         VALUES ('page', 'reef-test', 'Reef', '{}', 'draft', 'builder')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let mut tx = db.begin().await.unwrap();
    let schema = crate::storage::content::load_effective_content_schema_in_tx(&mut tx, "page")
        .await
        .unwrap()
        .unwrap();
    let values = BTreeMap::from([("body".to_owned(), json!(raw))]);
    entries::sync_entry_body_refs_in_conn(&mut tx, entry_id, &schema, &values, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_builder_media_refs WHERE entry_id = ?")
            .bind(entry_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(count, 2);
    assert!(
        !crate::storage::public_content::media_is_referenced(&db, &video_id)
            .await
            .unwrap(),
        "draft media must remain private"
    );
    sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id = ?")
        .bind(entry_id)
        .execute(&db)
        .await
        .unwrap();
    assert!(
        crate::storage::public_content::media_is_referenced(&db, &video_id)
            .await
            .unwrap()
    );

    let mut private_schema = schema.clone();
    private_schema
        .effective_fields
        .get_mut("body")
        .unwrap()
        .definition
        .public_exposure = PublicExposure::Private;
    let mut tx = db.begin().await.unwrap();
    entries::sync_entry_body_refs_in_conn(&mut tx, entry_id, &private_schema, &values, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(
        !crate::storage::public_content::media_is_referenced(&db, &video_id)
            .await
            .unwrap(),
        "private body may not emit media ACL"
    );

    let version: i64 = sqlx::query_scalar("SELECT version FROM builder_homepage WHERE id = 1")
        .fetch_one(&db)
        .await
        .unwrap();
    save_and_render(&db, &document, version, |_, _, _, _| Ok(()))
        .await
        .unwrap();
    let home_video: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM builder_homepage_video_refs WHERE media_id = ? AND ref_role = 'video'",
    )
    .bind(&video_id)
    .fetch_one(&db)
    .await
    .unwrap();
    let home_image: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM builder_homepage_media WHERE media_id = ?")
            .bind(&image_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!((home_video, home_image), (1, 1));

    let target = TemplateCandidate::new(TemplateCandidateKind::Index);
    let saved = crate::storage::overrides::save_typed_override(&db, &target, &document, None)
        .await
        .unwrap();
    let override_roles: Vec<String> = sqlx::query_scalar(
        "SELECT ref_role FROM template_override_media_refs WHERE override_id = ? ORDER BY ref_role",
    )
    .bind(saved.id)
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(override_roles, vec!["image", "video"]);
}
