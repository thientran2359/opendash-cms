use super::*;
use crate::auth::{AuthUser, Role};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::time::Duration;

async fn fixture(db: &SqlitePool) -> SessionContext {
    crate::storage::MIGRATOR.run(db).await.unwrap();
    let id = sqlx::query(
        "INSERT INTO users (username, password_hash, role) VALUES ('tax_admin', 'unused', 'admin')",
    )
    .execute(db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("INSERT INTO sessions (token_hash, user_id, csrf_token, expires_at) VALUES (?, ?, 'csrf', ?)")
        .bind("taxonomy-test-token")
        .bind(id)
        .bind(auth::current_time_secs() + 3600)
        .execute(db)
        .await
        .unwrap();
    SessionContext {
        user: AuthUser {
            id,
            username: "tax_admin".to_string(),
            role: Role::Admin,
        },
        token_hash: "taxonomy-test-token".to_string(),
        csrf_token: "csrf".to_string(),
    }
}

async fn memory_fixture() -> (SqlitePool, SessionContext) {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let actor = fixture(&db).await;
    (db, actor)
}

#[tokio::test]
async fn bulk_term_labels_only_project_requested_ids_even_with_large_catalog() {
    let (db, _) = memory_fixture().await;
    let mut requested = Vec::new();
    // Many unrelated terms must never appear in a table-row projection.
    for i in 0..250 {
        let kind = if i % 2 == 0 { "category" } else { "tag" };
        let id = sqlx::query("INSERT INTO taxonomy_terms(kind, name, slug, locale, translation_group_id) VALUES(?, ?, ?, 'vi', 'tg-' || lower(hex(randomblob(8))))")
            .bind(kind)
            .bind(format!("Term {i}"))
            .bind(format!("bulk-resolve-{i}"))
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();
        if matches!(i, 0 | 111 | 249) {
            requested.push(id);
        }
    }
    let resolved = resolve_term_labels(&db, &requested).await.unwrap();
    assert_eq!(resolved.len(), 3);
    for (found, id) in resolved.iter().zip(&requested) {
        assert_eq!(&found.id, id);
    }
    assert_eq!(resolved[0].kind, "category");
    assert_eq!(resolved[1].kind, "tag");
    assert_eq!(resolved[2].name, "Term 249");
    assert_eq!(resolve_term_labels(&db, &[999999]).await.unwrap().len(), 0);
    assert_eq!(
        resolve_term_labels(&db, &[0]).await.unwrap_err().0,
        axum::http::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        resolve_term_labels(&db, &(1..=101).collect::<Vec<_>>())
            .await
            .unwrap_err()
            .0,
        axum::http::StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn validates_depth_for_entire_subtree_and_prevents_cycles() {
    let (db, actor) = memory_fixture().await;
    let root = create_term(&db, &actor, "category", "Gốc", "root", None)
        .await
        .unwrap();
    let branch = create_term(&db, &actor, "category", "Branch", "branch", None)
        .await
        .unwrap();
    let branch_leaf = create_term(&db, &actor, "category", "Leaf", "leaf", Some(branch.id))
        .await
        .unwrap();

    let mut tail = root;
    for i in 2..=8 {
        tail = create_term(
            &db,
            &actor,
            "category",
            "Depth",
            &format!("depth-{i}"),
            Some(tail.id),
        )
        .await
        .unwrap();
    }
    let ninth = create_term(&db, &actor, "category", "Too deep", "ninth", Some(tail.id))
        .await
        .unwrap_err();
    assert_eq!(ninth.0, axum::http::StatusCode::CONFLICT);

    // A two-level branch cannot be moved under the depth-eight leaf.
    let overflow = patch_term(
        &db,
        &actor,
        branch.id,
        branch.version,
        None,
        None,
        Some(Some(tail.id)),
        None,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(overflow.0, axum::http::StatusCode::CONFLICT);
    let loop_error = patch_term(
        &db,
        &actor,
        branch.id,
        branch.version,
        None,
        None,
        Some(Some(branch_leaf.id)),
        None,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(loop_error.0, axum::http::StatusCode::CONFLICT);

    // Failed mutations are atomic: version and parent remain unchanged.
    let page = list_terms(&db, Some("category"), 100, 0).await.unwrap();
    let unchanged = page.items.iter().find(|term| term.id == branch.id).unwrap();
    assert_eq!(unchanged.parent_id, None);
    assert_eq!(unchanged.version, 1);
}

#[tokio::test]
async fn preserves_memberships_until_explicit_unlink_and_checks_cas() {
    let (db, actor) = memory_fixture().await;
    let parent = create_term(&db, &actor, "category", " Parent ", "parent", None)
        .await
        .unwrap();
    assert_eq!(parent.name, "Parent");
    let child = create_term(&db, &actor, "category", "Child", "child", Some(parent.id))
        .await
        .unwrap();
    assert_eq!(
        delete_term(&db, &actor, parent.id, 1).await.unwrap_err().0,
        axum::http::StatusCode::CONFLICT
    );
    let duplicate = create_term(&db, &actor, "category", "Other", "child", None)
        .await
        .unwrap_err();
    assert_eq!(duplicate.0, axum::http::StatusCode::CONFLICT);
    let bad_parent = create_term(&db, &actor, "tag", "Tag", "tag", Some(parent.id))
        .await
        .unwrap_err();
    assert_eq!(bad_parent.0, axum::http::StatusCode::BAD_REQUEST);

    let changed = patch_term(
        &db,
        &actor,
        child.id,
        1,
        Some("Renamed"),
        None,
        Some(None),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(changed.version, 2);
    assert_eq!(changed.parent_id, None);
    assert_eq!(
        patch_term(
            &db,
            &actor,
            child.id,
            1,
            Some("Stale"),
            None,
            None,
            None,
            None,
            None
        )
        .await
        .unwrap_err()
        .0,
        axum::http::StatusCode::PRECONDITION_FAILED
    );

    let entry_id = sqlx::query(
        "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, locale, translation_group_id)
         VALUES ('post', 'sample-post', 'Sample', '{}', 'draft', 'vi', 'eg-' || lower(hex(randomblob(8))))",
    )
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    // P10 migration 0013 now creates the entry_terms table.
    sqlx::query("INSERT INTO entry_terms (entry_id, term_id) VALUES (?, ?)")
        .bind(entry_id)
        .bind(parent.id)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        delete_term(&db, &actor, parent.id, 1).await.unwrap_err().0,
        axum::http::StatusCode::CONFLICT
    );
    sqlx::query("DELETE FROM content_entries WHERE id = ?")
        .bind(entry_id)
        .execute(&db)
        .await
        .unwrap();
    let still_linked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entry_terms")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(still_linked, 0);
    delete_term(&db, &actor, parent.id, 1).await.unwrap();
}

#[tokio::test]
async fn competing_reparents_cannot_commit_a_cycle() {
    let path = std::env::temp_dir().join(format!(
        "opendash-taxonomy-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Delete)
        .busy_timeout(Duration::from_secs(5));
    let db = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .unwrap();
    let actor = fixture(&db).await;
    let a = create_term(&db, &actor, "category", "A", "category-a", None)
        .await
        .unwrap();
    let b = create_term(&db, &actor, "category", "B", "category-b", None)
        .await
        .unwrap();
    let (a_result, b_result) = tokio::join!(
        patch_term(
            &db,
            &actor,
            a.id,
            1,
            None,
            None,
            Some(Some(b.id)),
            None,
            None,
            None
        ),
        patch_term(
            &db,
            &actor,
            b.id,
            1,
            None,
            None,
            Some(Some(a.id)),
            None,
            None,
            None
        )
    );
    assert_eq!(
        u8::from(a_result.is_ok()) + u8::from(b_result.is_ok()),
        1,
        "only one opposing move can commit: a={a_result:?}, b={b_result:?}"
    );
    let rejected = a_result.err().or_else(|| b_result.err()).unwrap();
    assert!(
        matches!(
            rejected.0,
            axum::http::StatusCode::CONFLICT | axum::http::StatusCode::SERVICE_UNAVAILABLE
        ),
        "other writer must observe the cycle or a retryable writer lock: {rejected:?}"
    );
    let page = list_terms(&db, Some("category"), 10, 0).await.unwrap();
    let a_parent = page
        .items
        .iter()
        .find(|term| term.id == a.id)
        .unwrap()
        .parent_id;
    let b_parent = page
        .items
        .iter()
        .find(|term| term.id == b.id)
        .unwrap()
        .parent_id;
    assert!(!(a_parent == Some(b.id) && b_parent == Some(a.id)));
    db.close().await;
    std::fs::remove_file(&path).unwrap();
}

#[tokio::test]
async fn persists_and_clears_term_featured_image_and_validates_asset() {
    let (db, actor) = memory_fixture().await;
    let image_id = "a".repeat(64);
    let video_id = "b".repeat(64);
    let deleting_id = "c".repeat(64);
    sqlx::query(
        "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state)
         VALUES (?, 'image', 'cover.webp', 'image/webp', 'webp', 1024, 800, 600, 320, 240, 'active')",
    )
    .bind(&image_id)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, duration_ms, video_codec, lifecycle_state)
         VALUES (?, 'video', 'clip.mp4', 'video/mp4', 'mp4', 2048, 1280, 720, 5000, 'h264', 'active')",
    )
    .bind(&video_id)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO media_assets (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height, lifecycle_state)
         VALUES (?, 'image', 'old.webp', 'image/webp', 'webp', 1024, 800, 600, 320, 240, 'deleting')",
    )
    .bind(&deleting_id)
    .execute(&db)
    .await
    .unwrap();

    let created = create_term_with_locale(
        &db,
        &actor,
        "category",
        "News",
        "news",
        None,
        None,
        None,
        Some(&image_id),
    )
    .await
    .unwrap();
    assert_eq!(
        created.featured_image_media_id.as_deref(),
        Some(image_id.as_str())
    );

    let fetched = get_term(&db, created.id).await.unwrap().unwrap();
    assert_eq!(
        fetched.featured_image_media_id.as_deref(),
        Some(image_id.as_str())
    );

    let listed = list_terms(&db, Some("category"), 50, 0).await.unwrap();
    assert_eq!(
        listed
            .items
            .iter()
            .find(|t| t.id == created.id)
            .unwrap()
            .featured_image_media_id
            .as_deref(),
        Some(image_id.as_str())
    );

    let cleared = patch_term(
        &db,
        &actor,
        created.id,
        created.version,
        None,
        None,
        None,
        Some(None),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(cleared.featured_image_media_id, None);
    assert_eq!(cleared.version, 2);

    let reattached = patch_term(
        &db,
        &actor,
        created.id,
        cleared.version,
        None,
        None,
        None,
        Some(Some(&image_id)),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        reattached.featured_image_media_id.as_deref(),
        Some(image_id.as_str())
    );

    for bad in ["not-hex", &"d".repeat(64), &video_id, &deleting_id] {
        let err = patch_term(
            &db,
            &actor,
            created.id,
            reattached.version,
            None,
            None,
            None,
            Some(Some(bad)),
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(
            err.0,
            axum::http::StatusCode::BAD_REQUEST,
            "must reject {bad}"
        );
    }
}
