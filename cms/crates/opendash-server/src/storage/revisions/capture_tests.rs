use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use opendash_core::revisions::{MAX_REVISIONS_PER_ENTRY, RevisionSource};
use serde_json::json;
use sqlx::SqlitePool;

use super::*;
use crate::auth::{AuthUser, Role, SessionContext};
use crate::storage::init_db;

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("opendash-{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).expect("create temp test dir");
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

async fn setup_test_db(guard: &TempDirGuard) -> (SqlitePool, PathBuf, String) {
    let db_path = guard.path.join("site.db");
    let db = init_db(&db_path).await.expect("init test db");
    let locale = crate::storage::settings::get_default_locale_code(&db)
        .await
        .expect("default locale");
    (db, db_path, locale)
}

async fn insert_test_user(db: &SqlitePool, username: &str, role: Role) -> SessionContext {
    let id =
        sqlx::query("INSERT INTO users (username, password_hash, role) VALUES (?, 'test-hash', ?)")
            .bind(username)
            .bind(role.as_str())
            .execute(db)
            .await
            .expect("insert test user")
            .last_insert_rowid();

    SessionContext {
        user: AuthUser {
            id,
            username: username.to_owned(),
            role,
        },
        csrf_token: "csrf".to_owned(),
        token_hash: "tok".to_owned(),
    }
}

async fn insert_test_post(
    db: &SqlitePool,
    locale: &str,
    slug: &str,
    title: &str,
    body: &str,
) -> i64 {
    let values_json = serde_json::to_string(&json!({ "body": body })).unwrap();
    sqlx::query(
        "INSERT INTO content_entries
            (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, content_mode)
         VALUES ('post', ?, ?, ?, ?, ?, 'draft', 'editor')",
    )
    .bind(locale)
    .bind(format!("tg-{slug}"))
    .bind(slug)
    .bind(title)
    .bind(values_json)
    .execute(db)
    .await
    .expect("insert post")
    .last_insert_rowid()
}

#[tokio::test]
async fn migration_0059_schema_count_and_restore_rebase_preserves_revisions_clears_autosaves() {
    let guard = TempDirGuard::new("rev-schema");
    let (db, db_path, locale) = setup_test_db(&guard).await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema")
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(
        count <= 256,
        "sqlite_schema count {count} must stay <= MAX_SCHEMA_OBJECTS (256)"
    );
    assert!(
        count >= 241,
        "expected >= 241 schema objects after 0058 (responsive media) + 0059 (revisions +6), got {count}"
    );

    let user = insert_test_user(&db, "admin", Role::Admin).await;
    let entry_id = insert_test_post(&db, &locale, "hello", "Hello", "<p>Body</p>").await;

    let mut tx = db.begin().await.unwrap();
    let rev = capture_entry_revision_in_tx(
        &mut tx,
        entry_id,
        RevisionSource::Create,
        None,
        RevisionActor::User(&user),
    )
    .await
    .unwrap();
    assert_eq!(rev, Some(1));
    tx.commit().await.unwrap();

    sqlx::query(
        "INSERT INTO entry_autosaves
            (entry_id, user_id, content_type_key, source_surface, base_version, title, content_mode, values_json)
         VALUES (?, ?, 'post', 'editor', 1, 'Draft', 'editor', '{\"body\":\"<p>Autosave</p>\"}')",
    )
    .bind(entry_id)
    .bind(user.user.id)
    .execute(&db)
    .await
    .unwrap();

    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&db)
        .await
        .unwrap();
    db.close().await;

    let copy_path = guard.path.join("restore-copy.db");
    fs::copy(&db_path, &copy_path).unwrap();

    let ro_ver = crate::storage::restore::validate_restored_db_read_only(&copy_path)
        .await
        .expect("read-only restore validation");
    assert!(ro_ver >= 58);

    let rebased_ver = crate::storage::restore::validate_and_rebase_copy(&copy_path)
        .await
        .expect("rebase copy validation");
    assert!(rebased_ver >= 58);

    let rebased_pool = init_db(&copy_path).await.unwrap();
    let rev_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entry_revisions")
        .fetch_one(&rebased_pool)
        .await
        .unwrap();
    let autosave_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entry_autosaves")
        .fetch_one(&rebased_pool)
        .await
        .unwrap();
    assert_eq!(rev_count, 1);
    assert_eq!(autosave_count, 0);
}

#[tokio::test]
async fn baseline_capture_empty_seo_dedup_pruning_and_pattern_builder() {
    let guard = TempDirGuard::new("rev-pruning");
    let (db, _path, locale) = setup_test_db(&guard).await;
    let user = insert_test_user(&db, "admin", Role::Admin).await;
    let entry_id = insert_test_post(&db, &locale, "legacy", "Legacy", "<p>V1</p>").await;

    let mut tx = db.begin().await.unwrap();
    assert!(
        capture_baseline_if_missing_in_tx(&mut tx, entry_id)
            .await
            .unwrap()
    );
    assert!(
        !capture_baseline_if_missing_in_tx(&mut tx, entry_id)
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();

    // Inserting an all-empty entry_seo row must normalize to None and deduplicate hash.
    sqlx::query(
        "INSERT INTO entry_seo (entry_id, title, description, og_image_id, canonical_url, noindex)
         VALUES (?, '', '   ', NULL, NULL, 0)",
    )
    .bind(entry_id)
    .execute(&db)
    .await
    .unwrap();
    let mut tx = db.begin().await.unwrap();
    let dedup = capture_entry_revision_in_tx(
        &mut tx,
        entry_id,
        RevisionSource::SeoSave,
        None,
        RevisionActor::User(&user),
    )
    .await
    .unwrap();
    assert_eq!(dedup, None);
    tx.commit().await.unwrap();

    // Capture 26 more edits -> total kept must be pruned to MAX_REVISIONS_PER_ENTRY (25).
    for i in 2..=27 {
        let values = serde_json::to_string(&json!({ "body": format!("<p>V{i}</p>") })).unwrap();
        sqlx::query(
            "UPDATE content_entries SET values_json = ?, version = version + 1 WHERE id = ?",
        )
        .bind(values)
        .bind(entry_id)
        .execute(&db)
        .await
        .unwrap();
        let mut tx = db.begin().await.unwrap();
        let rev_num = capture_entry_revision_in_tx(
            &mut tx,
            entry_id,
            RevisionSource::Save,
            None,
            RevisionActor::User(&user),
        )
        .await
        .unwrap();
        assert_eq!(rev_num, Some(i));
        tx.commit().await.unwrap();
    }

    let (count, min_rev, max_rev): (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), MIN(revision_number), MAX(revision_number) FROM entry_revisions WHERE entry_id = ?",
    )
    .bind(entry_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(count, MAX_REVISIONS_PER_ENTRY as i64);
    assert_eq!(min_rev, 3);
    assert_eq!(max_rev, 27);

    // Pattern capture & reset snapshot check.
    let pat_id = sqlx::query(
        "INSERT INTO content_entries
            (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, content_mode)
         VALUES ('pattern', ?, 'tg-pat', 'hero-pat', 'Hero Pattern', '{}', 'draft', 'builder')",
    )
    .bind(&locale)
    .execute(&db)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query("INSERT INTO entry_builder_documents (entry_id, document_json, updated_at) VALUES (?, '{\"schema_version\":1,\"sections\":[]}', datetime('now'))")
        .bind(pat_id)
        .execute(&db)
        .await
        .unwrap();

    let mut tx = db.begin().await.unwrap();
    assert_eq!(
        capture_entry_revision_in_tx(
            &mut tx,
            pat_id,
            RevisionSource::PatternSave,
            None,
            RevisionActor::User(&user),
        )
        .await
        .unwrap(),
        Some(1)
    );
    tx.commit().await.unwrap();

    sqlx::query("DELETE FROM entry_builder_documents WHERE entry_id = ?")
        .bind(pat_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE content_entries SET content_mode = 'editor', version = version + 1 WHERE id = ?",
    )
    .bind(pat_id)
    .execute(&db)
    .await
    .unwrap();
    let mut tx = db.begin().await.unwrap();
    assert_eq!(
        capture_entry_revision_in_tx(
            &mut tx,
            pat_id,
            RevisionSource::BuilderReset,
            None,
            RevisionActor::User(&user),
        )
        .await
        .unwrap(),
        Some(2)
    );
    tx.commit().await.unwrap();

    let doc_json: Option<String> = sqlx::query_scalar(
        "SELECT builder_document_json FROM entry_revisions WHERE entry_id = ? AND revision_number = 2",
    )
    .bind(pat_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(doc_json.is_none());
}

#[tokio::test]
async fn autosave_isolation_cascades_and_media_non_pinning() {
    let guard = TempDirGuard::new("rev-media-fk");
    let (db, _path, locale) = setup_test_db(&guard).await;
    let admin = insert_test_user(&db, "admin", Role::Admin).await;
    let editor = insert_test_user(&db, "editor1", Role::Editor).await;
    let entry_id = insert_test_post(&db, &locale, "p1", "Post One", "<p>Clean</p>").await;

    for uid in [admin.user.id, editor.user.id] {
        sqlx::query(
            "INSERT INTO entry_autosaves
                (entry_id, user_id, content_type_key, source_surface, base_version, title, content_mode, values_json)
             VALUES (?, ?, 'post', 'editor', 1, 'Draft', 'editor', '{\"body\":\"<p>WIP</p>\"}')",
        )
        .bind(entry_id)
        .bind(uid)
        .execute(&db)
        .await
        .unwrap();
    }

    sqlx::query("UPDATE content_entries SET title = 'Saved', version = 2 WHERE id = ?")
        .bind(entry_id)
        .execute(&db)
        .await
        .unwrap();
    let mut tx = db.begin().await.unwrap();
    record_entry_revision_and_clear_autosave_in_tx(
        &mut tx,
        entry_id,
        RevisionSource::Save,
        RevisionActor::User(&admin),
        Some(admin.user.id),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let remaining_users: Vec<i64> =
        sqlx::query_scalar("SELECT user_id FROM entry_autosaves WHERE entry_id = ?")
            .bind(entry_id)
            .fetch_all(&db)
            .await
            .unwrap();
    assert_eq!(remaining_users, vec![editor.user.id]);

    // Deleting user cascades entry_autosaves and nulls author_user_id on entry_revisions.
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(editor.user.id)
        .execute(&db)
        .await
        .unwrap();
    let remaining_after_user_del: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_autosaves WHERE entry_id = ?")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(remaining_after_user_del, 0);

    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(admin.user.id)
        .execute(&db)
        .await
        .unwrap();
    let (rev_uid, rev_uname): (Option<i64>, String) = sqlx::query_as(
        "SELECT author_user_id, author_username FROM entry_revisions WHERE entry_id = ?",
    )
    .bind(entry_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(rev_uid, None);
    assert_eq!(rev_uname, "admin");

    // Insert private active media asset referenced ONLY inside revisions & autosaves.
    let admin2 = insert_test_user(&db, "admin2", Role::Admin).await;
    let media_id = "c".repeat(64);
    sqlx::query(
        "INSERT INTO media_assets
            (id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
         VALUES (?, 'image', 'hist.png', 'image/png', 'png', 123, 10, 10, 10, 10)",
    )
    .bind(&media_id)
    .execute(&db)
    .await
    .unwrap();

    let rev_val = format!(r#"{{"body":"<img src='/media/{media_id}/file'>"}}"#);
    let rev_seo = format!(r#"{{"og_image_id":"{media_id}","noindex":false}}"#);
    sqlx::query("UPDATE entry_revisions SET values_json = ?, seo_json = ? WHERE entry_id = ?")
        .bind(&rev_val)
        .bind(&rev_seo)
        .bind(entry_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO entry_autosaves
            (entry_id, user_id, content_type_key, source_surface, base_version, title, content_mode, values_json, seo_touched, seo_json)
         VALUES (?, ?, 'post', 'editor', 2, 'Draft', 'editor', ?, 1, ?)",
    )
    .bind(entry_id)
    .bind(admin2.user.id)
    .bind(&rev_val)
    .bind(&rev_seo)
    .execute(&db)
    .await
    .unwrap();

    let mut tx = db.begin().await.unwrap();
    let pins = crate::storage::media_refs::collect_pins_in_tx(&mut tx, &media_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(pins.total_pins, 0);
    assert!(!pins.is_referenced());
    assert!(
        !crate::storage::public_content::media_is_referenced(&db, &media_id)
            .await
            .unwrap()
    );

    // Clear all entry autosaves & verify entry deletion cascades entry_revisions.
    let mut tx = db.begin().await.unwrap();
    clear_all_entry_autosaves_in_tx(&mut tx, entry_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let autosave_left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_autosaves WHERE entry_id = ?")
            .bind(entry_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(autosave_left, 0);

    sqlx::query("DELETE FROM content_entries WHERE id = ?")
        .bind(entry_id)
        .execute(&db)
        .await
        .unwrap();
    let rev_left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entry_revisions WHERE entry_id = ?")
            .bind(entry_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(rev_left, 0);
}
