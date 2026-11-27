use super::*;

#[tokio::test]
async fn reading_page_guard_bidirectional_and_immediate_restore_schema_and_locale_fences() {
    let pool = setup_db().await;
    let admin = seed_actor(&pool, "admin", Role::Admin).await;

    // Add disabled secondary locale 'en' with assigned homepage page; Trash must STILL be blocked!
    sqlx::query("INSERT INTO site_locales (code, bcp47_tag, label, enabled) VALUES ('en', 'en', 'English', 1)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE site_locales_state SET multilingual_enabled = 1 WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    let en_page = seed_entry(&pool, "page", "en", "home-en", "published", "{}").await;
    sqlx::query(
        "INSERT INTO site_reading_settings (locale, homepage_page_id, posts_page_id, version)
         VALUES ('en', ?, NULL, 1)
         ON CONFLICT(locale) DO UPDATE SET homepage_page_id = excluded.homepage_page_id",
    )
    .bind(en_page)
    .execute(&pool)
    .await
    .unwrap();
    // Disable 'en' (still manageable because multilingual_enabled = 1): Reading assignment MUST still block Trash.
    sqlx::query("UPDATE site_locales SET enabled = 0 WHERE code = 'en'")
        .execute(&pool)
        .await
        .unwrap();
    let reading_block = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Trash,
            type_key: "page".into(),
            entry_id: en_page,
            expected_version: 2,
        },
        &LifecycleAuthority {
            session: &admin,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::None,
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(reading_block.0, StatusCode::CONFLICT);
    assert!(reading_block.1.contains("reading_page_assigned"));

    // Unassign 'en' homepage -> Trash succeeds -> re-assigning trashed Page fails in save_reading_for_locale!
    sqlx::query("UPDATE site_reading_settings SET homepage_page_id = NULL WHERE locale = 'en'")
        .execute(&pool)
        .await
        .unwrap();
    execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Trash,
            type_key: "page".into(),
            entry_id: en_page,
            expected_version: 2,
        },
        &LifecycleAuthority {
            session: &admin,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::None,
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap();

    let assign_trashed = crate::storage::settings::save_reading_for_locale(
        &pool,
        Some("en"),
        crate::storage::settings::ReadingSettingsInput {
            homepage_page_id: Some(en_page),
            posts_page_id: None,
        },
        1,
    )
    .await
    .unwrap_err();
    assert_eq!(assign_trashed.0, StatusCode::BAD_REQUEST);

    // Restore in disabled locale 'en' must be rejected until 'en' is re-enabled!
    let disabled_restore = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Restore,
            type_key: "page".into(),
            entry_id: en_page,
            expected_version: 3,
        },
        &LifecycleAuthority {
            session: &admin,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::None,
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(disabled_restore.0, StatusCode::CONFLICT);
    assert!(disabled_restore.1.contains("locale_disabled"));

    // Immediate public CPT: restore requires PublicRestore confirmation and preserves unknown/withdrawn values!
    let imm_input: ContentTypeDefinitionInput = serde_json::from_value(serde_json::json!({
        "key": "notice",
        "label": "Notice",
        "fields": [{
            "key": "summary",
            "label": "Summary",
            "type": "text",
            "required": false,
            "options": [],
            "acl": {"min_view_role": "editor", "min_edit_role": "editor", "public_exposure": "public"}
        }],
        "settings": {
            "public": true,
            "has_archive": true,
            "publication": "immediate",
            "supports": ["editor"]
        }
    }))
    .unwrap();
    crate::storage::content::insert_user_content_type(&pool, &imm_input)
        .await
        .unwrap();
    let notice_id = seed_entry(
        &pool,
        "notice",
        "vi",
        "imm-1",
        "published",
        r#"{"summary":"ok","legacy_extra":"kept"}"#,
    )
    .await;
    execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Trash,
            type_key: "notice".into(),
            entry_id: notice_id,
            expected_version: 2,
        },
        &LifecycleAuthority {
            session: &admin,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::None,
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap();

    // Missing PublicRestore confirmation -> 409 confirmation_required
    let missing_pub_confirm = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Restore,
            type_key: "notice".into(),
            entry_id: notice_id,
            expected_version: 3,
        },
        &LifecycleAuthority {
            session: &admin,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::None,
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(missing_pub_confirm.0, StatusCode::CONFLICT);
    assert!(missing_pub_confirm.1.contains("confirmation_required"));

    // With PublicRestore confirmation -> restores as published and keeps unknown "legacy_extra"!
    let restored = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Restore,
            type_key: "notice".into(),
            entry_id: notice_id,
            expected_version: 3,
        },
        &LifecycleAuthority {
            session: &admin,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::PublicRestore {
                type_key: "notice".into(),
                entry_id: notice_id,
                expected_version: 3,
                session_token_hash: admin.token_hash.clone(),
            },
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap();
    assert_eq!(restored.publication_after.as_deref(), Some("published"));
    let stored_vals: String =
        sqlx::query_scalar("SELECT values_json FROM content_entries WHERE id = ?")
            .bind(notice_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(stored_vals.contains("legacy_extra"));
}

#[tokio::test]
async fn startup_builder_migration_preserves_trashed_row_lifecycle_and_pins() {
    let pool = setup_db().await;
    let admin = seed_actor(&pool, "admin", Role::Admin).await;

    let pic_id = "b0111111111111111111111111111111b0111111111111111111111111111111";
    sqlx::query(
        "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state)
         VALUES (?, 'image', 'pic.png', 'image/png', 'png', 16, 2, 2, 2, 2, 'active')",
    )
    .bind(pic_id)
    .execute(&pool)
    .await
    .unwrap();
    let page_id = seed_entry(&pool, "page", "vi", "legacy-demo-page", "published", "{}").await;
    let doc_json = serde_json::json!({
        "schema_version": 1,
        "blocks": [{
            "id": "sec-1",
            "type": "core/section",
            "children": [{
                "id": "row-1",
                "type": "core/row",
                "children": [{
                    "id": "col-1",
                    "type": "core/column",
                    "children": [{
                        "id": "el-1",
                        "type": "core/image",
                        "props": {
                            "image": pic_id,
                            "alt": "Pic"
                        }
                    }]
                }]
            }]
        }]
    })
    .to_string();
    sqlx::query("INSERT INTO entry_builder_documents (entry_id, document_json, updated_at) VALUES (?, ?, datetime('now'))")
        .bind(page_id)
        .bind(&doc_json)
        .execute(&pool)
        .await
        .unwrap();

    let trashed = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Trash,
            type_key: "page".into(),
            entry_id: page_id,
            expected_version: 2,
        },
        &LifecycleAuthority {
            session: &admin,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::None,
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap();

    // Run startup Builder normalization maintenance: must preserve trashed_at, status, and pins,
    // and must NOT grant public media access!
    let migrated = crate::storage::builder::migrate_demo_entry_builder_docs_on_startup(&pool)
        .await
        .unwrap();
    assert_eq!(migrated, 1);

    let (pub_status, trashed_at, values_json): (String, Option<String>, String) = sqlx::query_as(
        "SELECT publication_status, trashed_at, values_json FROM content_entries WHERE id = ?",
    )
    .bind(page_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pub_status, "published");
    assert_eq!(trashed_at, trashed.trashed_at);
    assert!(values_json.contains(pic_id));

    // Media stays conservatively pinned in media_refs, but NOT publicly granted via media_is_referenced!
    let mut tx = pool.begin().await.unwrap();
    let pins = crate::storage::media_refs::collect_pins_in_tx(&mut tx, pic_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(pins.is_referenced());
    assert!(
        !crate::storage::public_content::media_is_referenced(&pool, pic_id)
            .await
            .unwrap()
    );
}
