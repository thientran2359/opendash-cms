use super::*;
use axum::http::StatusCode;
use opendash_core::content::ContentTypeDefinitionInput;

use crate::auth::{AuthUser, Role, SessionContext};

async fn setup_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&pool).await.unwrap();
    pool
}

async fn seed_actor(pool: &SqlitePool, username: &str, role: Role) -> SessionContext {
    let user_id: i64 = sqlx::query(
        "INSERT INTO users (username, password_hash, role, version) VALUES (?, 'hash', ?, 1)",
    )
    .bind(username)
    .bind(role.as_str())
    .execute(pool)
    .await
    .unwrap()
    .last_insert_rowid();
    let token_hash = format!("tok-{username}-{user_id}");
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, csrf_token, expires_at)
         VALUES (?, ?, 'csrf-test', ?)",
    )
    .bind(&token_hash)
    .bind(user_id)
    .bind(current_time_secs() + 3600)
    .execute(pool)
    .await
    .unwrap();
    SessionContext {
        token_hash,
        csrf_token: "csrf-test".into(),
        user: AuthUser {
            id: user_id,
            username: username.into(),
            role,
        },
    }
}

async fn seed_entry(
    pool: &SqlitePool,
    type_key: &str,
    locale: &str,
    slug: &str,
    status: &str,
    values_json: &str,
) -> i64 {
    let id = sqlx::query(
        "INSERT INTO content_entries
         (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, version)
         VALUES (?, ?, 'eg-' || lower(hex(randomblob(6))), ?, 'Entry Title', ?, 'draft', 1)",
    )
    .bind(type_key)
    .bind(locale)
    .bind(slug)
    .bind(values_json)
    .execute(pool)
    .await
    .unwrap()
    .last_insert_rowid();
    if status == "published" {
        sqlx::query(
            "UPDATE content_entries
                SET publication_status = 'published',
                    published_at = '2025-05-01 10:00:00',
                    version = version + 1
              WHERE id = ?",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }
    let mut tx = pool.begin().await.unwrap();
    crate::storage::permalinks::sync_claims_in_tx(&mut tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    id
}

mod suite_01;
mod suite_02;
