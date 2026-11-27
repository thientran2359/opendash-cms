use super::*;
use sqlx::sqlite::SqlitePoolOptions;

async fn site() -> SqlitePool {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::storage::MIGRATOR.run(&db).await.unwrap();
    db
}

fn vi_home() -> PublicRouteTarget {
    PublicRouteTarget::Home {
        locale: "vi".to_owned(),
    }
}

fn vi_entry(id: i64) -> PublicRouteTarget {
    PublicRouteTarget::Entry {
        id,
        locale: "vi".to_owned(),
        is_home: false,
    }
}

fn vi_home_entry(id: i64) -> PublicRouteTarget {
    PublicRouteTarget::Entry {
        id,
        locale: "vi".to_owned(),
        is_home: true,
    }
}

fn vi_term(id: i64) -> PublicRouteTarget {
    PublicRouteTarget::Term {
        id,
        locale: "vi".to_owned(),
    }
}

fn vi_archive(type_key: &str) -> PublicRouteTarget {
    PublicRouteTarget::Archive {
        type_key: type_key.to_owned(),
        locale: "vi".to_owned(),
    }
}

async fn reading_page(db: &SqlitePool, slug: &str, published: bool) -> i64 {
    let id = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
         VALUES('page',?,'Page','{}','draft','vi','eg-' || lower(hex(randomblob(8))))",
    )
    .bind(slug)
    .execute(db)
    .await
    .unwrap()
    .last_insert_rowid();
    if published {
        sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
            .bind(id)
            .execute(db)
            .await
            .unwrap();
    }
    id
}

async fn add_post(db: &SqlitePool, slug: &str, published: bool) -> i64 {
    let id = sqlx::query(
        "INSERT INTO content_entries
         (content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
         VALUES('post',?,'Example','{}','draft','vi','eg-' || lower(hex(randomblob(8))))",
    )
    .bind(slug)
    .execute(db)
    .await
    .unwrap()
    .last_insert_rowid();
    if published {
        sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
            .bind(id)
            .execute(db)
            .await
            .unwrap();
    }
    let mut tx = db.begin().await.unwrap();
    sync_claims_in_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();
    id
}

mod suite_01;
mod suite_02;
mod suite_03;
mod suite_04;
