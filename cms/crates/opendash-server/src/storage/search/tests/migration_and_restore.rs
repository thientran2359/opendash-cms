use std::{fs, path::Path};

use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

use super::super::*;
use super::TempDirGuard;

#[tokio::test]
async fn migration_0058_fresh_and_baseline_upgrade_and_restore_validation() {
    let tmp = TempDirGuard::new("mig58");

    // 1. Fresh init_db: verify version 58, <= 256 objects, FTS5 shadow tables, CHECKs & unique index.
    let fresh_db = tmp.path().join("fresh.sqlite");
    let pool = crate::storage::init_db(&fresh_db).await.unwrap();

    let max_ver: i64 =
        sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(max_ver, 58);

    let obj_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        obj_count <= 256,
        "schema object budget exceeded: {obj_count}"
    );

    for fts_obj in [
        "entry_search_fts",
        "entry_search_fts_data",
        "entry_search_fts_idx",
        "entry_search_fts_config",
        "entry_search_fts_docsize",
    ] {
        let found: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name = ?")
            .bind(fts_obj)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(found, 1, "missing FTS5 object: {fts_obj}");
    }

    sqlx::query(
        "INSERT INTO template_overrides (target_kind, canonical_target_key, target_json, ast_json)
         VALUES ('search', 'search', '{\"kind\":\"search\"}', '[]')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let job_a = "a".repeat(64);
    let job_b = "b".repeat(64);
    sqlx::query(
        "INSERT INTO operation_jobs (id, kind, state) VALUES (?, 'search_reindex', 'running')",
    )
    .bind(&job_a)
    .execute(&pool)
    .await
    .unwrap();
    let second_heavy =
        sqlx::query("INSERT INTO operation_jobs (id, kind, state) VALUES (?, 'backup', 'queued')")
            .bind(&job_b)
            .execute(&pool)
            .await;
    assert!(
        second_heavy.is_err(),
        "idx_operation_jobs_single_active_heavy must block backup while search_reindex is active"
    );
    pool.close().await;

    // 2. Upgrade from baseline_0001.sqlite and pass read-only trusted_schema=OFF + rebase validation.
    let up_db = tmp.path().join("upgraded.sqlite");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/baseline_0001.sqlite");
    fs::copy(&fixture, &up_db).unwrap();

    let up_pool = crate::storage::init_db(&up_db).await.unwrap();
    let up_ver: i64 =
        sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(&up_pool)
            .await
            .unwrap();
    assert_eq!(up_ver, 58);
    let up_objs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
            .fetch_one(&up_pool)
            .await
            .unwrap();
    assert!(
        up_objs <= 256,
        "upgraded schema objects exceeded: {up_objs}"
    );

    sqlx::query(
        "INSERT INTO users (username, password_hash, role) VALUES ('rest_admin', 'hash_placeholder', 'admin')",
    )
    .execute(&up_pool)
    .await
    .unwrap();

    let eid = sqlx::query(
        "INSERT INTO content_entries
         (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status)
         VALUES ('post', 'vi', 'grp-rest', 'bai-restore', 'Khôi phục Tìm kiếm', '{\"body\":\"Dữ liệu khôi phục\"}', 'draft')",
    )
    .execute(&up_pool)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query(
        "UPDATE content_entries SET publication_status = 'published', published_at = datetime('now') WHERE id = ?",
    )
    .bind(eid)
    .execute(&up_pool)
    .await
    .unwrap();
    let before_docsize: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entry_search_fts_docsize")
        .fetch_one(&up_pool)
        .await
        .unwrap();
    assert_eq!(before_docsize, 0);
    up_pool.close().await;

    let ro_ver = crate::storage::restore::validate_restored_db_read_only(&up_db)
        .await
        .unwrap();
    assert_eq!(ro_ver, 58);

    let rebased_ver = crate::storage::restore::validate_and_rebase_copy(&up_db)
        .await
        .unwrap();
    assert_eq!(rebased_ver, 58);

    let verify_pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&up_db))
        .await
        .unwrap();
    let restored_fts: i64 = sqlx::query_scalar(
        "SELECT rowid FROM entry_search_fts WHERE entry_search_fts MATCH '\"khoi\" \"phuc\"'",
    )
    .fetch_one(&verify_pool)
    .await
    .unwrap();
    assert_eq!(restored_fts, eid);
    verify_pool.close().await;
}

#[tokio::test]
async fn resolve_public_locale_and_bootstrap_id_mismatch_repair() {
    let tmp = TempDirGuard::new("boot");
    let db_path = tmp.path().join("test.sqlite");
    let pool = crate::storage::init_db(&db_path).await.unwrap();

    {
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            crate::storage::settings::resolve_public_locale_in_conn(&mut conn, None)
                .await
                .unwrap(),
            "vi"
        );
        assert_eq!(
            crate::storage::settings::resolve_public_locale_in_conn(&mut conn, Some("   "))
                .await
                .unwrap(),
            "vi"
        );
        assert_eq!(
            crate::storage::settings::resolve_public_locale_in_conn(&mut conn, Some("bad_!"))
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::BAD_REQUEST
        );
        assert_eq!(
            crate::storage::settings::resolve_public_locale_in_conn(&mut conn, Some("fr"))
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::NOT_FOUND
        );

        sqlx::query(
            "INSERT INTO site_locales (code, bcp47_tag, label, direction, is_default, enabled, position)
             VALUES ('en', 'en', 'English', 'ltr', 0, 1, 1)",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        assert_eq!(
            crate::storage::settings::resolve_public_locale_in_conn(&mut conn, Some("en"))
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::NOT_FOUND
        );
        sqlx::query("UPDATE site_locales_state SET multilingual_enabled = 1 WHERE id = 1")
            .execute(&mut *conn)
            .await
            .unwrap();
        assert_eq!(
            crate::storage::settings::resolve_public_locale_in_conn(&mut conn, Some("EN"))
                .await
                .unwrap(),
            "en"
        );
        sqlx::query("UPDATE site_locales SET enabled = 0 WHERE code = 'en'")
            .execute(&mut *conn)
            .await
            .unwrap();
        assert_eq!(
            crate::storage::settings::resolve_public_locale_in_conn(&mut conn, Some("en"))
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::NOT_FOUND
        );
    }

    let mut ids = Vec::new();
    for (idx, status) in ["published", "published", "draft"].iter().enumerate() {
        let id = sqlx::query(
            "INSERT INTO content_entries
             (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status)
             VALUES ('post', 'vi', ?, ?, ?, '{\"body\":\"Kiến trúc lõi\"}', 'draft')",
        )
        .bind(format!("g-{idx}"))
        .bind(format!("s-{idx}"))
        .bind(format!("Post {idx}"))
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        if *status == "published" {
            sqlx::query(
                "UPDATE content_entries SET publication_status = 'published', published_at = datetime('now') WHERE id = ?",
            )
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }
        ids.push(id);
    }

    rebuild_search_index(&pool).await.unwrap();

    // Corrupt FTS5 preserving identical COUNT(*) (2 == 2) but diverging rowid sets ({p1, draft} != {p1, p2}).
    sqlx::query("DELETE FROM entry_search_fts WHERE rowid = ?")
        .bind(ids[1])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO entry_search_fts(rowid, title, summary, body, terms) VALUES (?, 'stale draft', '', '', '')",
    )
    .bind(ids[2])
    .execute(&pool)
    .await
    .unwrap();

    let status_before = get_search_index_status(&pool).await.unwrap();
    assert_eq!(status_before.indexed_documents, 2);
    assert_eq!(status_before.eligible_published_entries, 2);
    assert!(
        !status_before.in_sync,
        "ID mismatch with same count must report out-of-sync"
    );

    assert!(ensure_search_index_bootstrapped(&pool).await.unwrap());
    let indexed_ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM entry_search_fts_docsize ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(indexed_ids, vec![ids[0], ids[1]]);
    assert!(!ensure_search_index_bootstrapped(&pool).await.unwrap());

    pool.close().await;
}
