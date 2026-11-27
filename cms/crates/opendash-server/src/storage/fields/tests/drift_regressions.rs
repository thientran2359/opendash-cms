use super::*;
use crate::storage::{
    init_db, media::require_existing_effective_references, media_refs::collect_pins_in_tx,
    public_content::media_is_referenced,
};
use opendash_core::{
    content::{ContentTypeDefinition, ContentTypeSettings, EffectiveContentSchema},
    field_schema::{
        FieldAcl, FieldRole, FieldSpec, FieldType, MediaFieldKind, PublicExposure,
        SelectOptionsSource,
    },
    registry::{DeclarationState, OwnedDescriptor, ProviderStatus, ResourceOwner},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(1);
        let dir = std::env::temp_dir().join(format!(
            "opendash-drift-reg-{prefix}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn f01_media_is_referenced_detects_nested_repeater_and_group_media_on_published_entries() {
    let tmp = TempDir::new("f01");
    let db = init_db(&tmp.0.join("database/opendash.sqlite"))
        .await
        .unwrap();
    let default_locale: String =
        sqlx::query_scalar("SELECT code FROM site_locales WHERE is_default = 1 LIMIT 1")
            .fetch_one(&db)
            .await
            .unwrap();

    let card_media = "a".repeat(64);
    let grp_media = "b".repeat(64);
    for mid in [&card_media, &grp_media] {
        sqlx::query(
            "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state, generation, version)
             VALUES (?, 'image', 'a.png', 'image/png', 'png', 100, 10, 10, 10, 10, 'active', 1, 1)",
        )
        .bind(mid)
        .execute(&db)
        .await
        .unwrap();
    }

    let mut slide_img = FieldSpec::new(FieldType::Media, "slide_img", "Ảnh slide");
    slide_img.media_kind = Some(MediaFieldKind::Image);
    let mut rep = FieldSpec::new(FieldType::Repeater, "slides", "Slides");
    rep.fields = vec![slide_img];

    let badge_img = FieldSpec::new(FieldType::Media, "badge_img", "Huy hiệu");
    let mut grp = FieldSpec::new(FieldType::Group, "meta_box", "Meta");
    grp.fields = vec![badge_img];

    let def = ContentTypeDefinition {
        key: "showcase".into(),
        label: "Showcase".into(),
        fields: vec![rep, grp],
        settings: ContentTypeSettings::default(),
    };
    sqlx::query(
        "INSERT INTO content_types (key, definition_json, owner_kind, provider_status, declaration_state, version)
         VALUES ('showcase', ?, 'user', 'active', 'declared', 1)",
    )
    .bind(serde_json::to_string(&def).unwrap())
    .execute(&db)
    .await
    .unwrap();

    let vals = json!({
        "slides": [{"slide_img": card_media}],
        "meta_box": {"badge_img": grp_media}
    });
    let entry_id: i64 = sqlx::query_scalar(
        "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status)
         VALUES ('showcase', ?, 'tg-show', 'demo', 'Demo', ?, 'published') RETURNING id",
    )
    .bind(&default_locale)
    .bind(vals.to_string())
    .fetch_one(&db)
    .await
    .unwrap();

    for mid in [&card_media, &grp_media] {
        sqlx::query(
            "INSERT INTO entry_builder_media_refs (entry_id, media_id, ref_role, source_kind) VALUES (?, ?, 'image', 'field')",
        )
        .bind(entry_id)
        .bind(mid)
        .execute(&db)
        .await
        .unwrap();
        assert!(media_is_referenced(&db, mid).await.unwrap());
    }

    // F-01: Deactivating owning CPT or withdrawing Field Group must revoke public media grants.
    sqlx::query(
        "UPDATE content_types SET provider_status = 'provider_inactive' WHERE key = 'showcase'",
    )
    .execute(&db)
    .await
    .unwrap();
    assert!(!media_is_referenced(&db, &card_media).await.unwrap());
    sqlx::query("UPDATE content_types SET provider_status = 'active' WHERE key = 'showcase'")
        .execute(&db)
        .await
        .unwrap();
    assert!(media_is_referenced(&db, &card_media).await.unwrap());

    let fg_media = "f".repeat(64);
    sqlx::query(
        "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state, generation, version)
         VALUES (?, 'image', 'fg.png', 'image/png', 'png', 100, 10, 10, 10, 10, 'active', 1, 1)",
    )
    .bind(&fg_media)
    .execute(&db)
    .await
    .unwrap();
    let mut fg_img = FieldSpec::new(FieldType::Media, "fg_banner", "Banner");
    fg_img.media_kind = Some(MediaFieldKind::Image);
    sqlx::query(
        "INSERT INTO content_field_groups (group_id, owner_kind, owner_id, local_key, title, description, fields_json, default_region, default_order, collapsible, status, version)
         VALUES ('user:showcase_fg', 'user', NULL, 'showcase_fg', 'Extra', '', ?, 'main_bottom', 0, 0, 'active', 1)",
    )
    .bind(serde_json::to_string(&vec![fg_img]).unwrap())
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO content_field_group_targets (group_id, type_key, association_state) VALUES ('user:showcase_fg', 'showcase', 'declared')")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE content_entries SET values_json = ? WHERE id = ?")
        .bind(json!({"slides": [{"slide_img": card_media}], "meta_box": {"badge_img": grp_media}, "fg_banner": fg_media}).to_string())
        .bind(entry_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entry_builder_media_refs (entry_id, media_id, ref_role, source_kind) VALUES (?, ?, 'image', 'field')")
        .bind(entry_id)
        .bind(&fg_media)
        .execute(&db)
        .await
        .unwrap();
    assert!(media_is_referenced(&db, &fg_media).await.unwrap());

    sqlx::query(
        "UPDATE content_field_groups SET status = 'withdrawn' WHERE group_id = 'user:showcase_fg'",
    )
    .execute(&db)
    .await
    .unwrap();
    assert!(!media_is_referenced(&db, &fg_media).await.unwrap());
}

#[tokio::test]
async fn f04_load_entry_ref_hydration_prefers_featured_image_and_ignores_non_image_media() {
    let tmp = TempDir::new("f04");
    let db = init_db(&tmp.0.join("database/opendash.sqlite"))
        .await
        .unwrap();
    let default_locale: String =
        sqlx::query_scalar("SELECT code FROM site_locales WHERE is_default = 1 LIMIT 1")
            .fetch_one(&db)
            .await
            .unwrap();

    let file_id = "c".repeat(64);
    let img_id = "d".repeat(64);
    sqlx::query(
        "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, duration_ms, video_codec, lifecycle_state, generation, version)
         VALUES (?, 'video', 'clip.mp4', 'video/mp4', 'mp4', 200, 640, 360, NULL, NULL, 1000, 'h264', 'active', 1, 1)",
    )
    .bind(&file_id)
    .execute(&db)
    .await
    .unwrap();

    let mut a_file = FieldSpec::new(FieldType::Media, "a_attachment", "Đính kèm");
    a_file.media_kind = Some(MediaFieldKind::Any);
    let mut b_feat = FieldSpec::new(FieldType::Media, "featured_image", "Ảnh đại diện");
    b_feat.media_kind = Some(MediaFieldKind::Image);
    let def = ContentTypeDefinition {
        key: "report".into(),
        label: "Báo cáo".into(),
        fields: vec![a_file, b_feat],
        settings: ContentTypeSettings::default(),
    };
    sqlx::query(
        "INSERT INTO content_types (key, definition_json, owner_kind, provider_status, declaration_state, version)
         VALUES ('report', ?, 'user', 'active', 'declared', 1)",
    )
    .bind(serde_json::to_string(&def).unwrap())
    .execute(&db)
    .await
    .unwrap();

    // Even when featured_image is not set on the entry, a_attachment points to a non-image ('file')
    // asset -> cover_url must stay None (m.kind == "image" strictly enforced).
    let entry_id: i64 = sqlx::query_scalar(
        "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status)
         VALUES ('report', ?, 'tg-r1', 'r1', 'Report 1', ?, 'published') RETURNING id",
    )
    .bind(&default_locale)
    .bind(json!({"a_attachment": file_id}).to_string())
    .fetch_one(&db)
    .await
    .unwrap();

    let mut conn = db.acquire().await.unwrap();
    let ids: BTreeSet<i64> = [entry_id].into_iter().collect();
    let map = load_entry_ref_hydration_map_in_conn(&mut conn, &ids, true, Some(&default_locale))
        .await
        .unwrap();
    assert_eq!(map.get(&entry_id).unwrap().cover_url, None);

    sqlx::query(
        "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state, generation, version)
         VALUES (?, 'image', 'cover.png', 'image/png', 'png', 100, 10, 10, 10, 10, 'active', 1, 1)",
    )
    .bind(&img_id)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("UPDATE content_entries SET values_json = ? WHERE id = ?")
        .bind(json!({"a_attachment": file_id, "featured_image": img_id}).to_string())
        .bind(entry_id)
        .execute(&db)
        .await
        .unwrap();
    let map2 = load_entry_ref_hydration_map_in_conn(&mut conn, &ids, true, Some(&default_locale))
        .await
        .unwrap();
    assert_eq!(
        map2.get(&entry_id).unwrap().cover_url.as_deref(),
        Some(format!("/api/admin/media/{img_id}/file?v=1").as_str())
    );
}

#[tokio::test]
async fn f05_media_validation_rejects_wrong_kind_and_preserves_409_for_quarantined() {
    let tmp = TempDir::new("f05");
    let db = init_db(&tmp.0.join("database/opendash.sqlite"))
        .await
        .unwrap();

    let pdf_id = "1".repeat(64);
    let q_img_id = "2".repeat(64);
    sqlx::query(
        "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, duration_ms, video_codec, lifecycle_state, generation, version)
         VALUES (?, 'video', 'clip.mp4', 'video/mp4', 'mp4', 100, 640, 360, NULL, NULL, 1000, 'h264', 'active', 1, 1),
                (?, 'image', 'del.png', 'image/png', 'png', 100, 10, 10, 10, 10, NULL, NULL, 'deleting', 1, 1)",
    )
    .bind(&pdf_id)
    .bind(&q_img_id)
    .execute(&db)
    .await
    .unwrap();

    let mut photo = FieldSpec::new(FieldType::Media, "photo", "Ảnh");
    photo.media_kind = Some(MediaFieldKind::Image);
    let eff = EffectiveContentSchema::assemble(
        OwnedDescriptor {
            owner: ResourceOwner::User,
            status: ProviderStatus::Active,
            declaration_state: DeclarationState::Declared,
            descriptor: ContentTypeDefinition {
                key: "gallery_item".into(),
                label: "Ảnh".into(),
                fields: vec![photo],
                settings: ContentTypeSettings::default(),
            },
        },
        1,
        vec![],
    )
    .unwrap();

    let mut wrong_vals = BTreeMap::new();
    wrong_vals.insert("photo".into(), json!(pdf_id));
    let wrong_err = require_existing_effective_references(&db, &eff, &wrong_vals)
        .await
        .unwrap_err();
    assert_eq!(wrong_err.0, axum::http::StatusCode::BAD_REQUEST);

    let mut q_vals = BTreeMap::new();
    q_vals.insert("photo".into(), json!(q_img_id));
    let q_err = require_existing_effective_references(&db, &eff, &q_vals)
        .await
        .unwrap_err();
    assert_eq!(q_err.0, axum::http::StatusCode::CONFLICT);
}

#[tokio::test]
async fn f11_f16_context_fields_without_list_and_term_extension_media_pins() {
    let tmp = TempDir::new("f11-f16");
    let db = init_db(&tmp.0.join("database/opendash.sqlite"))
        .await
        .unwrap();

    let mut ctx_field = FieldSpec::new(FieldType::Select, "bind_key", "Khóa");
    ctx_field.options_source = Some(SelectOptionsSource::ContextFields);
    let specs = vec![ctx_field];

    let mut valid_vals = BTreeMap::new();
    valid_vals.insert("bind_key".into(), json!("custom_subtitle"));
    let mut conn = db.acquire().await.unwrap();
    validate_block_dynamic_options_in_tx(&mut conn, &specs, &valid_vals, None, None)
        .await
        .unwrap();

    let mut bad_vals = BTreeMap::new();
    bad_vals.insert("bind_key".into(), json!("NotValid-Key!"));
    assert!(
        validate_block_dynamic_options_in_tx(&mut conn, &specs, &bad_vals, None, None)
            .await
            .is_err()
    );

    // Private ContextFields must never leak into editor ContextFields dynamic options list.
    let mut priv_f = FieldSpec::new(FieldType::Text, "secret_key", "Bí mật");
    priv_f.acl = Some(FieldAcl {
        min_view_role: Some(FieldRole::Editor),
        min_edit_role: Some(FieldRole::Editor),
        public_exposure: Some(PublicExposure::Private),
    });
    sqlx::query(
        "INSERT INTO content_types (key, definition_json, owner_kind, provider_status, declaration_state, version)
         VALUES ('priv_ctx', ?, 'user', 'active', 'declared', 1)",
    )
    .bind(
        serde_json::to_string(&ContentTypeDefinition {
            key: "priv_ctx".into(),
            label: "Private".into(),
            fields: vec![priv_f],
            settings: ContentTypeSettings::default(),
        })
        .unwrap(),
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    let opts = list_dynamic_options(
        &db,
        &tmp.0,
        SelectOptionsSource::ContextFields,
        Some("priv_ctx"),
        None,
        FieldRole::Editor,
    )
    .await
    .unwrap();
    assert!(!opts.iter().any(|o| o.value == "secret_key"));

    // F-16: term_extension_values JSON is parsed and pinned media IDs are detected.
    let pin_id = "e".repeat(64);
    sqlx::query("INSERT INTO plugins(id,version,name,manifest_json,wasm_blob) VALUES ('p1','1.0.0','P1','{}',X'00')")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO taxonomy_terms(kind,slug,name,locale,translation_group_id) VALUES ('category','news','News','vi','tg-news')")
        .execute(&mut *conn)
        .await
        .unwrap();
    let term_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO term_extension_values (term_id, owner_kind, owner_id, local_key, values_json)
         VALUES (?, 'plugin', 'p1', 'ext_panel', ?)",
    )
    .bind(term_id)
    .bind(json!({"banner": pin_id}).to_string())
    .execute(&mut *conn)
    .await
    .unwrap();

    let mut tx = db.begin().await.unwrap();
    let summary = collect_pins_in_tx(&mut tx, &pin_id).await.unwrap();
    assert!(summary.is_referenced());
    assert!(
        summary
            .pins
            .iter()
            .any(|p| p.source_kind == "term_extension")
    );
}

#[tokio::test]
async fn f11_cpt_target_fields_excludes_private_and_includes_public_field_groups() {
    let tmp = TempDir::new("f11-cpt-target");
    let db = init_db(&tmp.0.join("database/opendash.sqlite"))
        .await
        .unwrap();

    let ui_tab = FieldSpec::new(FieldType::Tab, "tab_info", "Tab");
    let pub_base = FieldSpec::new(FieldType::Text, "pub_sub", "Public Sub");
    let mut priv_base = FieldSpec::new(FieldType::Text, "secret_note", "Secret");
    priv_base.acl = Some(FieldAcl {
        min_view_role: Some(FieldRole::Editor),
        min_edit_role: Some(FieldRole::Editor),
        public_exposure: Some(PublicExposure::Private),
    });
    let mut admin_base = FieldSpec::new(FieldType::Text, "admin_only", "Admin Only");
    admin_base.acl = Some(FieldAcl {
        min_view_role: Some(FieldRole::Admin),
        min_edit_role: Some(FieldRole::Admin),
        public_exposure: Some(PublicExposure::Private),
    });
    let def = ContentTypeDefinition {
        key: "guide".into(),
        label: "Guide".into(),
        fields: vec![ui_tab, pub_base, priv_base, admin_base],
        settings: ContentTypeSettings::default(),
    };
    sqlx::query(
        "INSERT INTO content_types (key, definition_json, owner_kind, provider_status, declaration_state, version)
         VALUES ('guide', ?, 'user', 'active', 'declared', 1)",
    )
    .bind(serde_json::to_string(&def).unwrap())
    .execute(&db)
    .await
    .unwrap();

    let mut fg_pub = FieldSpec::new(FieldType::Text, "fg_badge", "FG Badge");
    fg_pub.acl = Some(FieldAcl {
        min_view_role: Some(FieldRole::Editor),
        min_edit_role: Some(FieldRole::Editor),
        public_exposure: Some(PublicExposure::Public),
    });
    let mut fg_priv = FieldSpec::new(FieldType::Text, "fg_secret", "FG Secret");
    fg_priv.acl = Some(FieldAcl {
        min_view_role: Some(FieldRole::Editor),
        min_edit_role: Some(FieldRole::Editor),
        public_exposure: Some(PublicExposure::Private),
    });
    sqlx::query(
        "INSERT INTO content_field_groups (group_id, owner_kind, owner_id, local_key, title, description, fields_json, default_region, default_order, collapsible, status, version)
         VALUES ('user:guide_fg', 'user', NULL, 'guide_fg', 'Guide FG', '', ?, 'main_bottom', 0, 0, 'active', 1)",
    )
    .bind(serde_json::to_string(&vec![fg_pub, fg_priv]).unwrap())
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO content_field_group_targets (group_id, type_key, association_state) VALUES ('user:guide_fg', 'guide', 'declared')")
        .execute(&db)
        .await
        .unwrap();

    let mut tx = db.begin().await.unwrap();
    let allowed =
        crate::storage::overrides::validate_cpt_target_fields_in_tx(&mut tx, "guide", None)
            .await
            .unwrap()
            .unwrap();
    assert!(allowed.contains(&"pub_sub".to_owned()));
    assert!(allowed.contains(&"fg_badge".to_owned()));
    assert!(!allowed.contains(&"tab_info".to_owned()));
    assert!(!allowed.contains(&"secret_note".to_owned()));
    assert!(!allowed.contains(&"admin_only".to_owned()));
    assert!(!allowed.contains(&"fg_secret".to_owned()));
}
