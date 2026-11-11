//! Read-only M3 Dashboard counts. One SQLite SELECT gives one consistent snapshot.
use serde::Serialize;
use sqlx::SqlitePool;

use crate::error::ApiResult;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct PublicationCounts {
    pub draft: i64,
    pub published: i64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct DashboardCounts {
    pub post: PublicationCounts,
    pub page: PublicationCounts,
}

pub async fn get_counts(db: &SqlitePool) -> ApiResult<DashboardCounts> {
    // CASE returns 0 for all nonmatching rows. The aggregate has exactly
    // one output row even when content_entries is empty. CPTs are excluded
    // at the SQL boundary; pagination cannot affect any of these counts.
    let (post_draft, post_published, page_draft, page_published): (i64, i64, i64, i64) =
        sqlx::query_as(
            "SELECT
                COUNT(CASE WHEN content_type_key = 'post' AND publication_status = 'draft' THEN 1 END),
                COUNT(CASE WHEN content_type_key = 'post' AND publication_status = 'published' THEN 1 END),
                COUNT(CASE WHEN content_type_key = 'page' AND publication_status = 'draft' THEN 1 END),
                COUNT(CASE WHEN content_type_key = 'page' AND publication_status = 'published' THEN 1 END)
             FROM content_entries
             WHERE content_type_key IN ('post', 'page')",
        )
        .fetch_one(db)
        .await?;

    Ok(DashboardCounts {
        post: PublicationCounts {
            draft: post_draft,
            published: post_published,
        },
        page: PublicationCounts {
            draft: page_draft,
            published: page_published,
        },
    })
}
