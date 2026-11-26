use super::*;

#[tokio::test]
async fn trash_restore_purge_invariants_and_cas_precedence() {
    let pool = setup_db().await;
    let admin = seed_actor(&pool, "admin", Role::Admin).await;
    let editor = seed_actor(&pool, "editor", Role::Editor).await;

    let post_id = seed_entry(&pool, "post", "vi", "recover-me", "published", "{}").await;
    let orig_pub_at: Option<String> =
        sqlx::query_scalar("SELECT published_at FROM content_entries WHERE id = ?")
            .bind(post_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(orig_pub_at.as_deref(), Some("2025-05-01 10:00:00"));

    // Attach canonical SEO + derived public SEO ref + plugin projection to verify Trash revokes derived grants only.
    let og_id = "a0111111111111111111111111111111a0111111111111111111111111111111";
    sqlx::query(
        "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state)
         VALUES (?, 'image', 'og.png', 'image/png', 'png', 16, 2, 2, 2, 2, 'active')",
    )
    .bind(og_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO entry_seo (entry_id, title, description, og_image_id, noindex)
         VALUES (?, 'SEO Title', 'SEO Desc', ?, 0)",
    )
    .bind(post_id)
    .bind(og_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO seo_public_media_refs (source_kind, source_id, media_id)
         VALUES ('entry_seo', ?, ?)",
    )
    .bind(post_id)
    .bind(og_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO plugins (id, name, version, manifest_json, wasm_blob, enabled)
         VALUES ('p-seo', 'SEO', '1.0.0', '{}', X'00', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO plugin_entry_projections (entry_id, plugin_id, locale, ext_json)
         VALUES (?, 'p-seo', 'vi', '{\"ok\":true}')",
    )
    .bind(post_id)
    .execute(&pool)
    .await
    .unwrap();

    // Active Purge must reject with 409 (must Trash first).
    let active_purge_err = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Purge,
            type_key: "post".into(),
            entry_id: post_id,
            expected_version: 2,
        },
        &LifecycleAuthority {
            session: &admin,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::ExplicitPurge {
                type_key: "post".into(),
                entry_id: post_id,
                expected_version: 2,
                session_token_hash: admin.token_hash.clone(),
            },
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(active_purge_err.0, StatusCode::CONFLICT);

    // Editor Trash succeeds, advances version 2 -> 3, preserves publication_status & published_at,
    // revokes derived SEO public media refs & plugin projections, preserves canonical entry_seo & route claims.
    let trash_receipt = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Trash,
            type_key: "post".into(),
            entry_id: post_id,
            expected_version: 2,
        },
        &LifecycleAuthority {
            session: &editor,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::None,
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap();
    assert_eq!(trash_receipt.operation, EntryMutationKind::Trash);
    assert_eq!(trash_receipt.version_before, Some(2));
    assert_eq!(trash_receipt.version_after, Some(3));
    assert_eq!(
        trash_receipt.publication_before.as_deref(),
        Some("published")
    );
    assert_eq!(
        trash_receipt.publication_after.as_deref(),
        Some("published")
    );
    assert!(trash_receipt.trashed_at.is_some());
    assert_eq!(
        trash_receipt.published_at.as_deref(),
        Some("2025-05-01 10:00:00")
    );

    let seo_pub_refs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id = ?",
    )
    .bind(post_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(seo_pub_refs, 0);
    let proj_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plugin_entry_projections WHERE entry_id = ?")
            .bind(post_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(proj_rows, 0);
    let canonical_seo: Option<String> =
        sqlx::query_scalar("SELECT og_image_id FROM entry_seo WHERE entry_id = ?")
            .bind(post_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(canonical_seo.as_deref(), Some(og_id));
    let retained_claim: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?",
    )
    .bind(post_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retained_claim, 1);

    // Stale Trash yields 412 BEFORE 409 already-trashed!
    let stale_err = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Trash,
            type_key: "post".into(),
            entry_id: post_id,
            expected_version: 2,
        },
        &LifecycleAuthority {
            session: &editor,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::None,
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(stale_err.0, StatusCode::PRECONDITION_FAILED);

    // Repeated Trash with fresh version 3 yields 409 and does NOT bump version.
    let repeat_err = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Trash,
            type_key: "post".into(),
            entry_id: post_id,
            expected_version: 3,
        },
        &LifecycleAuthority {
            session: &editor,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::None,
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(repeat_err.0, StatusCode::CONFLICT);
    let ver_after_repeat: i64 =
        sqlx::query_scalar("SELECT version FROM content_entries WHERE id = ?")
            .bind(post_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ver_after_repeat, 3);

    // Editor Purge must be 403.
    let editor_purge_err = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Purge,
            type_key: "post".into(),
            entry_id: post_id,
            expected_version: 3,
        },
        &LifecycleAuthority {
            session: &editor,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::ExplicitPurge {
                type_key: "post".into(),
                entry_id: post_id,
                expected_version: 3,
                session_token_hash: editor.token_hash.clone(),
            },
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(editor_purge_err.0, StatusCode::FORBIDDEN);

    // Restore draft_publish post returns to 'draft', increments 3 -> 4, preserves published_at!
    let restore_receipt = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Restore,
            type_key: "post".into(),
            entry_id: post_id,
            expected_version: 3,
        },
        &LifecycleAuthority {
            session: &editor,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::None,
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap();
    assert_eq!(restore_receipt.operation, EntryMutationKind::Restore);
    assert_eq!(restore_receipt.version_before, Some(3));
    assert_eq!(restore_receipt.version_after, Some(4));
    assert_eq!(restore_receipt.publication_after.as_deref(), Some("draft"));
    assert!(restore_receipt.trashed_at.is_none());
    assert_eq!(
        restore_receipt.published_at.as_deref(),
        Some("2025-05-01 10:00:00")
    );

    // Trash again (4 -> 5) and Admin Purge without confirmation -> 409, then with confirmation -> succeeds!
    execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Trash,
            type_key: "post".into(),
            entry_id: post_id,
            expected_version: 4,
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

    let no_confirm = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Purge,
            type_key: "post".into(),
            entry_id: post_id,
            expected_version: 5,
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
    assert_eq!(no_confirm.0, StatusCode::CONFLICT);

    let purge_receipt = execute_lifecycle_transition(
        &pool,
        &LifecycleCommand {
            operation: LifecycleOperation::Purge,
            type_key: "post".into(),
            entry_id: post_id,
            expected_version: 5,
        },
        &LifecycleAuthority {
            session: &admin,
            plugin_fence: None,
            confirmation: LifecycleConfirmationEvidence::ExplicitPurge {
                type_key: "post".into(),
                entry_id: post_id,
                expected_version: 5,
                session_token_hash: admin.token_hash.clone(),
            },
            scope: LifecycleScope::Ordinary,
        },
    )
    .await
    .unwrap();
    assert_eq!(purge_receipt.operation, EntryMutationKind::Purge);
    assert_eq!(purge_receipt.version_before, Some(5));
    assert_eq!(purge_receipt.version_after, None);
    assert_eq!(purge_receipt.publication_after, None);
    assert!(purge_receipt.trashed_at.is_some());

    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM content_entries WHERE id = ?")
        .bind(post_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
}
