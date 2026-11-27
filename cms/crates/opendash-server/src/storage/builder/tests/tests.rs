use super::*;

#[tokio::test]
async fn migrate_demo_entry_builder_docs_on_startup_cleans_non_pattern_and_keeps_pattern() {
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();

    let media_id = "5".repeat(64);
    sqlx::query(
        "INSERT INTO media_assets(id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
         VALUES(?, 'image', 'demo.png', 'image/png', 'png', 100, 10, 10, 10, 10)",
    )
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();

    let doc: BuilderDocument = serde_json::from_value(json!({
        "schema_version": 1,
        "sections": [{"id":"sec-1","children":[{"id":"row-1","children":[{"id":"col-1","children":[
            {"id":"hd-1","type":"core/heading","props":{"level":2,"text":"Demo migrated"}},
            {"id":"im-1","type":"core/image","props":{"media_id":media_id,"alt":"Demo"}}
        ]}]}]}]
    }))
    .unwrap();
    let doc_json = String::from_utf8(doc.to_json().unwrap()).unwrap();

    let page_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, content_mode, locale, translation_group_id)
         VALUES('page', 'demo-page', 'Demo Page', '{}', 'draft', 'builder', 'vi', 'eg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = ?")
        .bind(page_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO entry_builder_documents(entry_id, document_json, updated_at) VALUES(?, ?, datetime('now'))",
    )
    .bind(page_id)
    .bind(&doc_json)
    .execute(&db)
    .await
    .unwrap();

    let pattern_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, content_mode, locale, translation_group_id)
         VALUES('pattern', 'demo-pat', 'Demo Pattern', '{}', 'draft', 'builder', 'vi', 'eg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query(
        "INSERT INTO entry_builder_documents(entry_id, document_json, updated_at) VALUES(?, ?, datetime('now'))",
    )
    .bind(pattern_id)
    .bind(&doc_json)
    .execute(&db)
    .await
    .unwrap();

    let migrated = migrate_demo_entry_builder_docs_on_startup(&db)
        .await
        .unwrap();
    assert_eq!(migrated, 1);
    assert_eq!(
        migrate_demo_entry_builder_docs_on_startup(&db)
            .await
            .unwrap(),
        0
    );

    let page_values: String =
        sqlx::query_scalar("SELECT values_json FROM content_entries WHERE id = ?")
            .bind(page_id)
            .fetch_one(&db)
            .await
            .unwrap();
    let body = serde_json::from_str::<Value>(&page_values).unwrap()["body"]
        .as_str()
        .unwrap()
        .to_owned();
    let parsed = opendash_core::builder_blocks::parse_blocks_to_builder_document(&body).unwrap();
    assert_eq!(parsed.document, doc);

    let non_pattern_left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_builder_documents WHERE entry_id = ?")
            .bind(page_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(non_pattern_left, 0);

    let pattern_left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_builder_documents WHERE entry_id = ?")
            .bind(pattern_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(pattern_left, 1);

    let media_refs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entry_builder_media_refs WHERE entry_id = ? AND media_id = ?",
    )
    .bind(page_id)
    .bind(&media_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(media_refs, 1);
}

#[tokio::test]
async fn entry_builder_enforces_body_min_edit_role_and_private_exposure_media_acl() {
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();

    // Private CPT without `body` capability blocks Builder read/save (`check_entry_body_edit_acl`),
    // and synthetic Admin-only `body` proves both edit-ACL rejection and non-public media grant.
    sqlx::query(
        r#"INSERT INTO content_types(key, definition_json, owner_kind)
           VALUES('secret_no_builder', '{"key":"secret_no_builder","label":"NoBuilder","settings":{"public":false,"has_archive":false,"publication":"draft_publish","supports":[]}}', 'user')"#,
    )
    .execute(&db)
    .await
    .unwrap();
    let no_builder_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
         VALUES('secret_no_builder', 'nb-1', 'NB 1', '{}', 'draft', 'vi', 'eg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();

    sqlx::query(
        r#"INSERT INTO content_types(key, definition_json, owner_kind)
           VALUES('secret_doc', '{"key":"secret_doc","label":"Secret","settings":{"public":false,"has_archive":false,"publication":"draft_publish","supports":["editor","builder"]}}', 'user')"#,
    )
    .execute(&db)
    .await
    .unwrap();

    let mut body_spec = opendash_core::field_schema::FieldSpec::new(
        opendash_core::field_schema::FieldType::Richtext,
        "body",
        "Body",
    );
    body_spec.acl = Some(opendash_core::field_schema::FieldAcl::canonical(
        opendash_core::content::FieldRole::Admin,
        opendash_core::content::FieldRole::Admin,
        opendash_core::content::PublicExposure::Private,
    ));
    let mut eff_fields = std::collections::BTreeMap::new();
    eff_fields.insert(
        "body".to_owned(),
        opendash_core::content::EffectiveFieldEntry {
            definition: body_spec,
            owner: opendash_core::registry::ResourceOwner::Core,
            source_id: "base:secret_doc".into(),
            effective_status: opendash_core::content::EffectiveFieldStatus::Active,
        },
    );
    let restricted_schema = opendash_core::content::EffectiveContentSchema {
        base: opendash_core::registry::OwnedDescriptor {
            descriptor: opendash_core::content::ContentTypeDefinition {
                key: "secret_doc".into(),
                label: "Secret".into(),
                singular_label: None,
                description: None,
                settings: opendash_core::content::ContentTypeSettings::default(),
            },
            owner: opendash_core::registry::ResourceOwner::User,
            status: opendash_core::registry::ProviderStatus::Active,
            declaration_state: opendash_core::registry::DeclarationState::Declared,
        },
        base_version: 1,
        attached_groups: Vec::new(),
        effective_fields: eff_fields,
        ui_fields: Vec::new(),
        schema_token: String::new(),
    };

    let admin_id = sqlx::query(
        "INSERT INTO users(username, password_hash, role) VALUES('admin1', 'hash', 'admin')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let editor_id = sqlx::query(
        "INSERT INTO users(username, password_hash, role) VALUES('editor1', 'hash', 'editor')",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    let future = crate::auth::current_time_secs() + 3600;
    sqlx::query("INSERT INTO sessions(token_hash, user_id, csrf_token, expires_at) VALUES('adm_h', ?, 'c1', ?)")
        .bind(admin_id)
        .bind(future)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sessions(token_hash, user_id, csrf_token, expires_at) VALUES('ed_h', ?, 'c2', ?)")
        .bind(editor_id)
        .bind(future)
        .execute(&db)
        .await
        .unwrap();

    let admin_ctx = SessionContext {
        user: crate::auth::AuthUser {
            id: admin_id,
            username: "admin1".into(),
            role: Role::Admin,
        },
        csrf_token: "c1".into(),
        token_hash: "adm_h".into(),
    };
    let editor_ctx = SessionContext {
        user: crate::auth::AuthUser {
            id: editor_id,
            username: "editor1".into(),
            role: Role::Editor,
        },
        csrf_token: "c2".into(),
        token_hash: "ed_h".into(),
    };

    assert_eq!(
        crate::storage::builder::mutations_and_save::check_entry_body_edit_acl(
            &restricted_schema,
            &editor_ctx,
        )
        .unwrap_err()
        .0,
        axum::http::StatusCode::FORBIDDEN
    );
    assert!(
        !crate::storage::builder::mutations_and_save::body_allows_public_refs(&restricted_schema)
    );

    let media_id = "6".repeat(64);
    sqlx::query(
        "INSERT INTO media_assets(id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
         VALUES(?, 'image', 'priv.png', 'image/png', 'png', 100, 10, 10, 10, 10)",
    )
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();
    let entry_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
         VALUES('secret_doc', 'sec-1', 'Secret 1', '{}', 'published', 'vi', 'eg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();

    assert_eq!(
        read_entry_builder(&db, &editor_ctx, "secret_no_builder", no_builder_id)
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::FORBIDDEN
    );

    let doc: BuilderDocument = serde_json::from_value(json!({
        "schema_version": 1,
        "sections": [{"id":"sec-1","children":[{"id":"row-1","children":[{"id":"col-1","children":[
            {"id":"im-1","type":"core/image","props":{"media_id":media_id,"alt":"Secret"}}
        ]}]}]}]
    }))
    .unwrap();
    let plugins = std::sync::Arc::new(crate::plugin::PluginManager::new(db.clone(), false));
    let hooks = HookDispatcher::new(plugins);

    match save_entry_builder(
        &db,
        &editor_ctx,
        &hooks,
        "secret_no_builder",
        no_builder_id,
        1,
        &doc,
    )
    .await
    {
        Err(EntryBuilderMutationError::Api(err)) => {
            assert_eq!(err.0, axum::http::StatusCode::FORBIDDEN);
        }
        other => panic!("expected 403 forbidden for Editor without builder support, got {other:?}"),
    }

    let saved = save_entry_builder(&db, &admin_ctx, &hooks, "secret_doc", entry_id, 1, &doc)
        .await
        .unwrap();
    assert_eq!(saved.version, 2);

    // Private `body` field must NOT grant public media access, while media remains delete-pinned.
    assert!(
        !crate::storage::public_content::media_is_referenced(&db, &media_id)
            .await
            .unwrap()
    );
    let mut conn = db.acquire().await.unwrap();
    let pins = crate::storage::media_refs::collect_pins_in_tx(&mut conn, &media_id)
        .await
        .unwrap();
    assert_eq!(pins.total_pins, 1);
    assert_eq!(pins.pins[0].source_kind, "content_entry");
}
