use super::*;
use axum::http::StatusCode;
use sqlx::sqlite::SqlitePoolOptions;

/// Older fixtures seed site rows directly through SQLite. In C3, those
/// fixtures must explicitly build the same canonical claims that normal
/// API writers build inside their transaction; public readers never
/// fabricate URLs from unsynchronized legacy tables.
async fn synchronize_direct_sql_fixture(db: &SqlitePool) {
    let mut tx = db.begin().await.unwrap();
    crate::storage::permalinks::sync_claims_in_tx(&mut tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn add_entry(
    db: &SqlitePool,
    type_key: &str,
    slug: &str,
    published: bool,
    terms: &[i64],
) -> i64 {
    let result = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
         VALUES(?, ?, ?, '{}', 'draft', 'vi', 'eg-' || lower(hex(randomblob(8))))",
    )
    .bind(type_key)
    .bind(slug)
    .bind(slug)
    .execute(db)
    .await
    .unwrap();
    let id = result.last_insert_rowid();
    if published {
        sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
            .bind(id)
            .execute(db)
            .await
            .unwrap();
    }
    for term_id in terms {
        sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES(?,?)")
            .bind(id)
            .bind(term_id)
            .execute(db)
            .await
            .unwrap();
    }
    id
}

mod suite_01;
mod suite_02;
