//! Read-only public content lookup and media reference ACL.
//! Public controllers never issue SQL directly.

use sqlx::{Sqlite, SqlitePool, Transaction};

use super::entries::{self, ContentEntryRecord, EntryFilters, decode_row};
use crate::error::ApiResult;
use crate::public_context::{CanonicalUrlBuilder, PublicFilters, TaxonomyKind};

/// Slug is unique within each CPT; lookup never relies on a numeric admin ID.
pub async fn get_by_slug(
    db: &SqlitePool,
    type_key: &str,
    slug: &str,
) -> ApiResult<Option<ContentEntryRecord>> {
    let row: Option<(i64, String, String, String, String, String, i64, String, String)> = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at
         FROM content_entries WHERE content_type_key = ? AND slug = ? AND publication_status = 'published'",
    )
    .bind(type_key)
    .bind(slug)
    .fetch_optional(db)
    .await?;
    let res = row.map(decode_row).transpose()?;
    Ok(res)
}

pub async fn get_published_by_id(
    db: &SqlitePool,
    id: i64,
) -> ApiResult<Option<ContentEntryRecord>> {
    let row: Option<(i64, String, String, String, String, String, i64, String, String)> = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at
         FROM content_entries WHERE id = ? AND publication_status = 'published'",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    row.map(decode_row).transpose()
}

/// Publication filtering happens in SQL before paging, so a draft never
/// displaces a published entry or influences public next/previous links.
pub async fn list_published(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
) -> ApiResult<Vec<ContentEntryRecord>> {
    // Share the admin/P06 taxonomy predicates. Publication, category and tag
    // restrictions all run in SQL before paging; EXISTS cannot duplicate rows.
    // Keep the saved Builder media ACL query below unchanged (Forge P02).
    entries::list_published_filtered(db, type_key, limit, offset, filters).await
}

/// Resolve stable site-owned IDs to their current public URL. Draft/deleted
/// entries must never fall through to an older slug or another content type.
pub async fn canonical_entry_by_id(db: &SqlitePool, id: i64) -> ApiResult<Option<String>> {
    let mut conn = db.acquire().await?;
    crate::storage::seo::resolve_public_entry_path_in_tx(&mut conn, id).await
}

pub async fn canonical_term_by_id(db: &SqlitePool, id: i64) -> ApiResult<Option<String>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT kind, slug FROM taxonomy_terms WHERE id = ?")
            .bind(id)
            .fetch_optional(db)
            .await?;
    Ok(row.and_then(|(kind, slug)| {
        let kind = match kind.as_str() {
            "category" => TaxonomyKind::Category,
            "tag" => TaxonomyKind::Tag,
            _ => return None,
        };
        CanonicalUrlBuilder::term_from_id(id, kind, &slug)
    }))
}

#[derive(Debug, Clone)]
pub struct PublicTerm {
    pub id: i64,
    pub name: String,
    pub slug: String,
}

pub async fn taxonomy_public_page(
    db: &SqlitePool,
    kind: TaxonomyKind,
    slug: &str,
    filters: PublicFilters,
    limit: i64,
    offset: i64,
) -> ApiResult<Option<(PublicTerm, TaxonomyPublishedPage)>> {
    let mut tx = db.begin().await?;
    let row: Option<(i64, String, String)> =
        sqlx::query_as("SELECT id, name, slug FROM taxonomy_terms WHERE kind = ? AND slug = ?")
            .bind(kind.as_str())
            .bind(slug)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((id, name, slug)) = row else {
        return Ok(None);
    };
    // Check filter existence/kind within the same read snapshot as term and
    // page; malformed recognized query values are handled by the parser.
    for (filter_id, filter_kind) in [
        (filters.category_id, TaxonomyKind::Category),
        (filters.tag_id, TaxonomyKind::Tag),
    ] {
        if let Some(filter_id) = filter_id {
            let exists: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM taxonomy_terms WHERE id = ? AND kind = ?")
                    .bind(filter_id)
                    .bind(filter_kind.as_str())
                    .fetch_optional(&mut *tx)
                    .await?;
            if exists.is_none() {
                return Err(crate::error::ApiError::bad_request(
                    "Bộ lọc taxonomy không hợp lệ.",
                ));
            }
        }
    }
    let page = list_taxonomy_published_in_tx(&mut tx, kind, id, filters, limit, offset).await?;
    tx.commit().await?;
    Ok(Some((PublicTerm { id, name, slug }, page)))
}

pub struct TaxonomyPublishedPage {
    pub entries: Vec<ContentEntryRecord>,
    pub total: i64,
}

/// Cross-CPT category/tag predicates are all applied in SQL *before* LIMIT.
/// EXISTS prevents duplicate entries when they have multiple matching links.
/// Publication filtering also governs COUNT, so drafts affect neither counts
/// nor page boundaries. Read COUNT and page under one SQLite snapshot.
async fn list_taxonomy_published_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    kind: TaxonomyKind,
    term_id: i64,
    filters: PublicFilters,
    limit: i64,
    offset: i64,
) -> ApiResult<TaxonomyPublishedPage> {
    let root_category = if kind == TaxonomyKind::Category {
        Some(term_id)
    } else {
        filters.category_id
    };
    let root_tag = if kind == TaxonomyKind::Tag {
        Some(term_id)
    } else {
        filters.tag_id
    };
    let extra_category = (kind == TaxonomyKind::Category)
        .then_some(filters.category_id)
        .flatten();
    let extra_tag = (kind == TaxonomyKind::Tag)
        .then_some(filters.tag_id)
        .flatten();
    // Every input below is a bind parameter. The statement is fixed source.
    const FILTERED: &str = "WITH RECURSIVE category_scope(id, depth) AS (
            SELECT id, 1 FROM taxonomy_terms WHERE id = ? AND kind = 'category'
            UNION ALL
            SELECT child.id, category_scope.depth + 1
              FROM taxonomy_terms child JOIN category_scope ON child.parent_id = category_scope.id
             WHERE category_scope.depth < 8 AND ? = 1
        )
        SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
               e.publication_status, e.version, e.created_at, e.updated_at
          FROM content_entries e
         WHERE e.publication_status = 'published'
           AND (? IS NULL OR EXISTS (
                SELECT 1 FROM entry_terms et WHERE et.entry_id = e.id
                  AND et.term_id IN (SELECT id FROM category_scope)
           ))
           AND (? IS NULL OR EXISTS (
                SELECT 1 FROM entry_terms et WHERE et.entry_id = e.id AND et.term_id = ?
           ))
           AND (? IS NULL OR EXISTS (
                SELECT 1 FROM entry_terms et WHERE et.entry_id = e.id AND et.term_id = ?
           ))
           AND (? IS NULL OR EXISTS (
                SELECT 1 FROM entry_terms et WHERE et.entry_id = e.id AND et.term_id = ?
           ))";
    let count_sql = format!("SELECT COUNT(*) FROM ({FILTERED}) AS published");
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
        .bind(root_category)
        .bind(i64::from(filters.descendants))
        .bind(root_category)
        .bind(root_tag)
        .bind(root_tag)
        .bind(extra_category)
        .bind(extra_category)
        .bind(extra_tag)
        .bind(extra_tag)
        .fetch_one(&mut **tx)
        .await?;
    let page_sql = format!("{FILTERED} ORDER BY e.id DESC LIMIT ? OFFSET ?");
    let rows: Vec<(
        i64,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
    )> = sqlx::query_as(sqlx::AssertSqlSafe(page_sql))
        .bind(root_category)
        .bind(i64::from(filters.descendants))
        .bind(root_category)
        .bind(root_tag)
        .bind(root_tag)
        .bind(extra_category)
        .bind(extra_category)
        .bind(extra_tag)
        .bind(extra_tag)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut **tx)
        .await?;
    let entries = rows
        .into_iter()
        .map(decode_row)
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(TaxonomyPublishedPage { entries, total })
}

/// Images become public only while referenced by an image/gallery custom field
/// of an existing CPT entry. A private unreferenced upload remains 404 publicly.
/// JSON paths are derived from validated field keys persisted in CPT schemas.
pub async fn media_is_referenced(db: &SqlitePool, id: &str) -> ApiResult<bool> {
    let result: i64 = sqlx::query_scalar(
        r#"SELECT EXISTS (
           SELECT 1
           FROM content_entries AS e
           JOIN content_types AS t ON t.key = e.content_type_key
           JOIN json_each(t.definition_json, '$.fields') AS f
           WHERE e.publication_status = 'published'
             AND (
             (
               json_extract(f.value, '$.type') = 'image'
               AND json_extract(e.values_json, '$.' || json_extract(f.value, '$.key')) = ?
             )
             OR (
               json_extract(f.value, '$.type') = 'gallery'
               AND EXISTS (
                 SELECT 1
                 FROM json_each(json_extract(e.values_json, '$.' || json_extract(f.value, '$.key'))) AS item
                 WHERE item.value = ?
               )
             )
             )
           LIMIT 1
         ) OR EXISTS (
           SELECT 1 FROM builder_homepage_media AS bm
           JOIN builder_homepage AS b ON b.id = 1
           WHERE bm.media_id = ? AND b.document_json IS NOT NULL
         ) OR EXISTS (
           SELECT 1
           FROM theme_option_media_refs AS tom
           JOIN theme_revisions AS r ON r.theme_id = tom.theme_id
           JOIN theme_active_selection AS a ON a.revision_id = r.revision_id AND a.id = 1
           JOIN json_each(
             CASE
               WHEN json_type(r.manifest_json, '$.options') = 'array' THEN json_extract(r.manifest_json, '$.options')
               ELSE '[]'
             END
           ) AS opt
           WHERE tom.media_id = ?
             AND json_extract(opt.value, '$.key') = tom.option_key
             AND json_extract(opt.value, '$.public') = 1
         )"#,
    )
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .fetch_one(db)
    .await?;
    Ok(result != 0)
}

#[cfg(test)]
mod m5_public_context_tests {
    use super::*;
    use axum::http::StatusCode;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn add_entry(
        db: &SqlitePool,
        type_key: &str,
        slug: &str,
        published: bool,
        terms: &[i64],
    ) -> i64 {
        let result = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status)
             VALUES(?, ?, ?, '{}', 'draft')",
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

    #[tokio::test]
    async fn taxonomy_cross_cpt_publication_and_filters_apply_before_count_and_page() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind)
             VALUES('news','{\"key\":\"news\",\"label\":\"News\",\"fields\":[]}', 'user')",
        )
        .execute(&db)
        .await
        .unwrap();

        let root = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug) VALUES('category','Sea','sea')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let child = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,parent_id)
             VALUES('category','Reefs','reefs',?)",
        )
        .bind(root)
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let tag =
            sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug) VALUES('tag','Fish','fish')")
                .execute(&db)
                .await
                .unwrap()
                .last_insert_rowid();
        let other_tag =
            sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug) VALUES('tag','Other','other')")
                .execute(&db)
                .await
                .unwrap()
                .last_insert_rowid();

        let post = add_entry(&db, "post", "post-a", true, &[root, child, tag]).await;
        let news = add_entry(&db, "news", "news-a", true, &[child, tag]).await;
        let page_id = add_entry(&db, "page", "page-a", true, &[child, tag]).await;
        add_entry(&db, "news", "news-other", true, &[child, other_tag]).await;
        let draft = add_entry(&db, "post", "draft-a", false, &[root, child, tag]).await;
        add_entry(&db, "news", "no-terms", true, &[]).await;
        let exact = taxonomy_public_page(
            &db,
            TaxonomyKind::Category,
            "sea",
            PublicFilters::default(),
            20,
            0,
        )
        .await
        .unwrap()
        .unwrap()
        .1;
        assert_eq!(exact.total, 1);
        assert_eq!(
            exact.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            [post]
        );
        let descendants = PublicFilters {
            tag_id: Some(tag),
            descendants: true,
            ..PublicFilters::default()
        };
        let filtered = taxonomy_public_page(&db, TaxonomyKind::Category, "sea", descendants, 20, 0)
            .await
            .unwrap()
            .unwrap()
            .1;
        assert_eq!(filtered.total, 3);
        assert_eq!(
            filtered.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            [page_id, news, post],
        );
        let tag_archive = taxonomy_public_page(
            &db,
            TaxonomyKind::Tag,
            "fish",
            PublicFilters {
                category_id: Some(root),
                descendants: true,
                ..PublicFilters::default()
            },
            20,
            0,
        )
        .await
        .unwrap()
        .unwrap()
        .1;
        assert_eq!(tag_archive.total, 3);
        assert!(
            tag_archive
                .entries
                .iter()
                .all(|e| e.publication_status == "published")
        );
        assert!(!tag_archive.entries.iter().any(|e| e.id == draft));
        assert!(
            taxonomy_public_page(
                &db,
                TaxonomyKind::Tag,
                "sea",
                PublicFilters::default(),
                20,
                0,
            )
            .await
            .unwrap()
            .is_none()
        );
        assert_eq!(
            taxonomy_public_page(
                &db,
                TaxonomyKind::Category,
                "sea",
                PublicFilters {
                    tag_id: Some(child),
                    ..PublicFilters::default()
                },
                20,
                0,
            )
            .await
            .err()
            .unwrap()
            .0,
            StatusCode::BAD_REQUEST,
        );
        assert_eq!(
            taxonomy_public_page(
                &db,
                TaxonomyKind::Category,
                "sea",
                PublicFilters {
                    tag_id: Some(999_999),
                    ..PublicFilters::default()
                },
                20,
                0,
            )
            .await
            .err()
            .unwrap()
            .0,
            StatusCode::BAD_REQUEST,
        );

        // Stable ID follows a slug change; old slug and draft remain invisible.
        sqlx::query("UPDATE content_entries SET slug='new-post-a' WHERE id=?")
            .bind(post)
            .execute(&db)
            .await
            .unwrap();
        assert!(get_by_slug(&db, "post", "post-a").await.unwrap().is_none());
        assert!(get_by_slug(&db, "post", "draft-a").await.unwrap().is_none());
        assert_eq!(
            canonical_entry_by_id(&db, post).await.unwrap().as_deref(),
            Some("/content/post/new-post-a")
        );
        assert_eq!(canonical_entry_by_id(&db, draft).await.unwrap(), None);
        sqlx::query("UPDATE taxonomy_terms SET slug='sea-new' WHERE id=?")
            .bind(root)
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(
            canonical_term_by_id(&db, root).await.unwrap().as_deref(),
            Some("/taxonomy/category/sea-new")
        );
        assert!(
            taxonomy_public_page(
                &db,
                TaxonomyKind::Category,
                "sea",
                PublicFilters::default(),
                20,
                0,
            )
            .await
            .unwrap()
            .is_none()
        );
    }

    #[tokio::test]
    async fn category_pagination_deduplicates_memberships_and_excludes_newer_drafts() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let root =
            sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug) VALUES('category','A','a')")
                .execute(&db)
                .await
                .unwrap()
                .last_insert_rowid();
        let child = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,parent_id) VALUES('category','B','b',?)",
        )
        .bind(root)
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let mut ids = Vec::new();
        for index in 0..23 {
            ids.push(
                add_entry(
                    &db,
                    "post",
                    &format!("published-{index}"),
                    true,
                    &[root, child],
                )
                .await,
            );
        }
        let draft = add_entry(&db, "post", "new-draft", false, &[root, child]).await;
        let flags = PublicFilters {
            descendants: true,
            ..PublicFilters::default()
        };
        let first = taxonomy_public_page(&db, TaxonomyKind::Category, "a", flags, 20, 0)
            .await
            .unwrap()
            .unwrap()
            .1;
        assert_eq!(first.total, 23);
        assert_eq!(first.entries.len(), 20);
        assert_eq!(first.entries[0].id, *ids.last().unwrap());
        let second = taxonomy_public_page(&db, TaxonomyKind::Category, "a", flags, 20, 20)
            .await
            .unwrap()
            .unwrap()
            .1;
        assert_eq!(second.total, 23);
        assert_eq!(second.entries.len(), 3);
        assert_eq!(second.entries[2].id, ids[0]);
        assert!(
            first
                .entries
                .iter()
                .chain(second.entries.iter())
                .all(|e| e.id != draft)
        );
    }
}
