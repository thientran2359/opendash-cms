use super::*;
use sqlx::sqlite::SqlitePoolOptions;

async fn list_test_entry(
    db: &SqlitePool,
    type_key: &str,
    slug: &str,
    published: bool,
    term_ids: &[i64],
) -> i64 {
    let id = sqlx::query(
        "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
         VALUES(?,?,'Safe','{}','draft','vi','eg-' || lower(hex(randomblob(8))))",
    )
    .bind(type_key)
    .bind(slug)
    .execute(db)
    .await
    .unwrap()
    .last_insert_rowid();
    if published {
        sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = ?")
            .bind(id)
            .execute(db)
            .await
            .unwrap();
    }
    for &term in term_ids {
        sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES (?,?)")
            .bind(id)
            .bind(term)
            .execute(db)
            .await
            .unwrap();
    }
    id
}

fn expect_list_page(result: PublishedListResult) -> PublishedListPage {
    match result {
        PublishedListResult::Page(page) => page,
        PublishedListResult::MissingConfig(reason) => {
            panic!("unexpected missing list configuration: {reason:?}")
        }
    }
}

async fn s02_admin_context(db: &SqlitePool) -> SessionContext {
    let user_id = sqlx::query(
        "INSERT INTO users(username, password_hash, role) VALUES('s02_admin', 'unused', 'admin')",
    )
    .execute(db)
    .await
    .unwrap()
    .last_insert_rowid();
    let expires_at = auth::current_time_secs() + 3600;
    sqlx::query(
        "INSERT INTO sessions(token_hash, user_id, csrf_token, expires_at)
         VALUES('s02-hash', ?, 's02-csrf', ?)",
    )
    .bind(user_id)
    .bind(expires_at)
    .execute(db)
    .await
    .unwrap();
    SessionContext {
        user: auth::AuthUser {
            id: user_id,
            username: "s02_admin".to_owned(),
            role: auth::Role::Admin,
        },
        token_hash: "s02-hash".to_owned(),
        csrf_token: "s02-csrf".to_owned(),
    }
}

async fn composite_save(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: Option<i64>,
    input: CompositeEntrySaveInput,
) -> ApiResult<CompositeEntrySaveResult> {
    let plan = prepare_composite_entry_save(db, actor, type_key, entry_id, &input).await?;
    commit_composite_entry_save(db, actor, type_key, entry_id, &input, &plan).await
}

fn base_composite_input(
    schema_token: &str,
    slug: &str,
    expected_version: Option<i64>,
) -> CompositeEntrySaveInput {
    CompositeEntrySaveInput {
        expected_version,
        expected_schema_token: Some(schema_token.to_owned()),
        locale: None,
        title: format!("Title {slug}"),
        slug: slug.to_owned(),
        publication_status: Some("published".to_owned()),
        values: BTreeMap::new(),
        term_ids: None,
        primary_category_term_id: PatchField::Omitted,
        auto_unique_slug: false,
        custom_path: PatchField::Omitted,
        page_layout_key: PatchField::Omitted,
        custom_template_key: PatchField::Omitted,
        content_mode: PatchField::Omitted,
        seo: PatchField::Omitted,
        created_at: None,
        updated_at: None,
    }
}

mod suite_01;
mod suite_02;
mod suite_03;
