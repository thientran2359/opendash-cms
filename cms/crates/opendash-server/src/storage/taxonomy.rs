//! Site-owned taxonomy. All category tree validation and writes share one
//! SQLite writer reservation; no parent/subtree read precedes that reservation.

use serde::Serialize;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    auth::{self, SessionContext},
    error::{ApiError, ApiResult},
};

const MAX_CATEGORY_DEPTH: i64 = 8;

#[derive(Debug, Clone, Serialize)]
pub struct TaxonomyTerm {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub slug: String,
    pub parent_id: Option<i64>,
    pub version: i64,
}

#[derive(Debug, Serialize)]
pub struct TaxonomyPage {
    pub items: Vec<TaxonomyTerm>,
    pub total: i64,
}

type TermRow = (i64, String, String, String, Option<i64>, i64);

fn term(row: TermRow) -> TaxonomyTerm {
    TaxonomyTerm {
        id: row.0,
        kind: row.1,
        name: row.2,
        slug: row.3,
        parent_id: row.4,
        version: row.5,
    }
}

pub fn valid_kind(kind: &str) -> bool {
    matches!(kind, "category" | "tag")
}

fn normalized_name(value: &str) -> ApiResult<&str> {
    let trimmed = value.trim();
    if !(1..=120).contains(&trimmed.chars().count()) {
        return Err(ApiError::bad_request(
            "Tên term phải dài từ 1 đến 120 ký tự.",
        ));
    }
    Ok(trimmed)
}

fn mutation_error(error: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref database_error) = error {
        if database_error.is_unique_violation() {
            return ApiError::conflict("Slug đã tồn tại trong loại taxonomy.");
        }
        if let Some(code) = database_error.code() {
            if code.parse::<u32>().is_ok_and(|n| matches!(n & 0xff, 5 | 6))
                || code.starts_with("SQLITE_BUSY")
                || code.starts_with("SQLITE_LOCKED")
            {
                return ApiError::service_unavailable(
                    "Dữ liệu đang được cập nhật, vui lòng thử lại.",
                );
            }
            if code.parse::<u32>().is_ok_and(|n| n & 0xff == 19)
                || code.starts_with("SQLITE_CONSTRAINT")
            {
                return ApiError::conflict("Dữ liệu taxonomy vi phạm ràng buộc.");
            }
        }
    }
    if matches!(error, sqlx::Error::PoolTimedOut) {
        return ApiError::service_unavailable("Dữ liệu đang được cập nhật, vui lòng thử lại.");
    }
    ApiError::from(error)
}

/// The first statement in the transaction reserves SQLite's single writer.
/// A deferred transaction that reads the tree first can race with a second
/// reparent and accept both sides of a newly created cycle.
async fn begin_admin_write<'a>(
    db: &'a SqlitePool,
    actor: &SessionContext,
) -> ApiResult<Transaction<'a, Sqlite>> {
    let mut tx = db.begin().await.map_err(mutation_error)?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?;

    // Recheck the actor within the serialized write so session revocation and
    // privilege changes cannot race with a previously authenticated request.
    let current: Option<(i64, String)> = sqlx::query_as(
        "SELECT u.id, u.role FROM sessions s
         JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(&actor.token_hash)
    .bind(auth::current_time_secs())
    .fetch_optional(&mut *tx)
    .await
    .map_err(mutation_error)?;
    match current {
        None => Err(ApiError::unauthorized("Phiên đăng nhập đã bị thu hồi.")),
        Some((id, _)) if id != actor.user.id => {
            Err(ApiError::unauthorized("Phiên đăng nhập đã bị thu hồi."))
        }
        Some((_, role)) if role != "admin" => Err(ApiError::forbidden(
            "Chỉ quản trị viên được quản lý taxonomy.",
        )),
        Some(_) => Ok(tx),
    }
}

async fn term_in_write(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
) -> ApiResult<Option<TaxonomyTerm>> {
    let row: Option<TermRow> = sqlx::query_as(
        "SELECT id, kind, name, slug, parent_id, version
         FROM taxonomy_terms WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(mutation_error)?;
    Ok(row.map(term))
}

async fn current_term(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
    expected_version: i64,
) -> ApiResult<TaxonomyTerm> {
    let value = term_in_write(tx, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy term."))?;
    if value.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Term đã thay đổi, hãy tải lại phiên bản mới.",
        ));
    }
    Ok(value)
}

/// Height includes the selected node (a leaf has height one). The recursion
/// is capped so corrupted cyclic data cannot keep the CTE running forever.
async fn subtree_height(tx: &mut Transaction<'_, Sqlite>, id: i64) -> ApiResult<i64> {
    let height: i64 = sqlx::query_scalar(
        "WITH RECURSIVE subtree(id, depth) AS (
             SELECT id, 1 FROM taxonomy_terms WHERE id = ?
             UNION ALL
             SELECT child.id, subtree.depth + 1
               FROM taxonomy_terms child JOIN subtree ON child.parent_id = subtree.id
              WHERE subtree.depth <= 8
         )
         SELECT COALESCE(MAX(depth), 0) FROM subtree",
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .map_err(mutation_error)?;
    Ok(height)
}

/// Check the candidate parent and all its ancestors while holding the writer.
/// A moving node anywhere in that lineage means the requested edge is cyclic.
async fn check_category_parent(
    tx: &mut Transaction<'_, Sqlite>,
    parent_id: Option<i64>,
    moving_id: Option<i64>,
    moving_height: i64,
) -> ApiResult<()> {
    let Some(parent_id) = parent_id else {
        if moving_height > MAX_CATEGORY_DEPTH {
            return Err(ApiError::conflict("Nhánh category vượt quá 8 cấp."));
        }
        return Ok(());
    };
    if parent_id <= 0 {
        return Err(ApiError::bad_request(
            "ID category cha phải là số nguyên dương.",
        ));
    }
    let parent: Option<(String,)> = sqlx::query_as("SELECT kind FROM taxonomy_terms WHERE id = ?")
        .bind(parent_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(mutation_error)?;
    match parent {
        Some((kind,)) if kind == "category" => {}
        _ => {
            return Err(ApiError::bad_request(
                "Category cha không tồn tại hoặc sai loại.",
            ));
        }
    }

    let lineage: Vec<(i64, Option<i64>, i64)> = sqlx::query_as(
        "WITH RECURSIVE lineage(id, parent_id, depth) AS (
             SELECT id, parent_id, 1 FROM taxonomy_terms WHERE id = ?
             UNION ALL
             SELECT parent.id, parent.parent_id, lineage.depth + 1
               FROM taxonomy_terms parent JOIN lineage ON parent.id = lineage.parent_id
              WHERE lineage.depth <= 8
         )
         SELECT id, parent_id, depth FROM lineage",
    )
    .bind(parent_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(mutation_error)?;
    if lineage
        .iter()
        .any(|(id, _, _)| moving_id.is_some_and(|moving| moving == *id))
    {
        return Err(ApiError::conflict("Không thể tạo chu trình category."));
    }
    let parent_depth = lineage
        .iter()
        .map(|(_, _, depth)| *depth)
        .max()
        .unwrap_or(0);
    if parent_depth + moving_height > MAX_CATEGORY_DEPTH
        || lineage
            .iter()
            .any(|(_, parent, depth)| *depth > MAX_CATEGORY_DEPTH && parent.is_some())
    {
        return Err(ApiError::conflict("Nhánh category vượt quá 8 cấp."));
    }
    Ok(())
}

pub async fn list_terms(
    db: &SqlitePool,
    kind: Option<&str>,
    limit: i64,
    offset: i64,
) -> ApiResult<TaxonomyPage> {
    let mut tx = db.begin().await?;
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM taxonomy_terms WHERE (? IS NULL OR kind = ?)")
            .bind(kind)
            .bind(kind)
            .fetch_one(&mut *tx)
            .await?;
    let rows: Vec<TermRow> = sqlx::query_as(
        "SELECT id, kind, name, slug, parent_id, version
           FROM taxonomy_terms
          WHERE (? IS NULL OR kind = ?)
          ORDER BY name COLLATE BINARY ASC, id ASC
          LIMIT ? OFFSET ?",
    )
    .bind(kind)
    .bind(kind)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(TaxonomyPage {
        items: rows.into_iter().map(term).collect(),
        total,
    })
}

pub async fn create_term(
    db: &SqlitePool,
    actor: &SessionContext,
    kind: &str,
    name: &str,
    slug: &str,
    parent_id: Option<i64>,
) -> ApiResult<TaxonomyTerm> {
    if !valid_kind(kind) {
        return Err(ApiError::bad_request(
            "Loại term phải là category hoặc tag.",
        ));
    }
    let name = normalized_name(name)?;
    if !opendash_core::id::valid_kebab_segment_with_limit(slug, 64) {
        return Err(ApiError::bad_request(
            "Slug phải là kebab-case, tối đa 64 ký tự.",
        ));
    }
    if kind == "tag" && parent_id.is_some() {
        return Err(ApiError::bad_request("Tag không được có category cha."));
    }
    let mut tx = begin_admin_write(db, actor).await?;
    if kind == "category" {
        check_category_parent(&mut tx, parent_id, None, 1).await?;
    }
    let id =
        sqlx::query("INSERT INTO taxonomy_terms(kind, name, slug, parent_id) VALUES (?, ?, ?, ?)")
            .bind(kind)
            .bind(name)
            .bind(slug)
            .bind(parent_id)
            .execute(&mut *tx)
            .await
            .map_err(mutation_error)?
            .last_insert_rowid();
    let created = term_in_write(&mut tx, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải term vừa tạo."))?;
    tx.commit().await.map_err(mutation_error)?;
    Ok(created)
}

pub async fn patch_term(
    db: &SqlitePool,
    actor: &SessionContext,
    id: i64,
    expected_version: i64,
    name: Option<&str>,
    slug: Option<&str>,
    parent_id: Option<Option<i64>>,
) -> ApiResult<TaxonomyTerm> {
    let name = name.map(normalized_name).transpose()?;
    if slug.is_some_and(|s| !opendash_core::id::valid_kebab_segment_with_limit(s, 64)) {
        return Err(ApiError::bad_request(
            "Slug phải là kebab-case, tối đa 64 ký tự.",
        ));
    }
    let mut tx = begin_admin_write(db, actor).await?;
    let existing = current_term(&mut tx, id, expected_version).await?;
    let next_parent = parent_id.unwrap_or(existing.parent_id);
    if existing.kind == "tag" && next_parent.is_some() {
        return Err(ApiError::bad_request("Tag không được có category cha."));
    }
    if existing.kind == "category" && parent_id.is_some() {
        let height = subtree_height(&mut tx, id).await?;
        check_category_parent(&mut tx, next_parent, Some(id), height).await?;
    }
    let updated = sqlx::query(
        "UPDATE taxonomy_terms
            SET name = ?, slug = ?, parent_id = ?, version = version + 1
          WHERE id = ? AND version = ?",
    )
    .bind(name.unwrap_or(&existing.name))
    .bind(slug.unwrap_or(&existing.slug))
    .bind(next_parent)
    .bind(id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await
    .map_err(mutation_error)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::precondition_failed(
            "Term đã thay đổi, hãy tải lại.",
        ));
    }
    let record = term_in_write(&mut tx, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải term vừa cập nhật."))?;
    tx.commit().await.map_err(mutation_error)?;
    Ok(record)
}

pub async fn delete_term(
    db: &SqlitePool,
    actor: &SessionContext,
    id: i64,
    expected_version: i64,
) -> ApiResult<()> {
    let mut tx = begin_admin_write(db, actor).await?;
    current_term(&mut tx, id, expected_version).await?;
    let children: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM taxonomy_terms WHERE parent_id = ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(mutation_error)?;
    if children != 0 {
        return Err(ApiError::conflict(
            "Category còn mục con. Hãy chuyển mục con trước.",
        ));
    }
    // P10 creates entry_terms in its own migration. After that migration the
    // same deletion path checks dependencies explicitly, while FK RESTRICT
    // remains the database's final guard against concurrent entry updates.
    let has_memberships: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'entry_terms'",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(mutation_error)?;
    if has_memberships != 0 {
        let linked_entries: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM entry_terms WHERE term_id = ?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(mutation_error)?;
        if linked_entries != 0 {
            return Err(ApiError::conflict(
                "Term đang được gắn vào nội dung. Hãy gỡ liên kết trước.",
            ));
        }
    }
    let affected = sqlx::query("DELETE FROM taxonomy_terms WHERE id = ? AND version = ?")
        .bind(id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await
        .map_err(mutation_error)?
        .rows_affected();
    if affected != 1 {
        return Err(ApiError::precondition_failed(
            "Term đã thay đổi, hãy tải lại.",
        ));
    }
    tx.commit().await.map_err(mutation_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthUser, Role};
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
    use std::{str::FromStr, time::Duration};

    async fn fixture(db: &SqlitePool) -> SessionContext {
        sqlx::migrate!("../../migrations").run(db).await.unwrap();
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

        let changed = patch_term(&db, &actor, child.id, 1, Some("Renamed"), None, Some(None))
            .await
            .unwrap();
        assert_eq!(changed.version, 2);
        assert_eq!(changed.parent_id, None);
        assert_eq!(
            patch_term(&db, &actor, child.id, 1, Some("Stale"), None, None)
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::PRECONDITION_FAILED
        );

        let entry_id = sqlx::query(
            "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status)
             VALUES ('post', 'sample-post', 'Sample', '{}', 'draft')",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        // Simulate the membership table introduced later by P10; this fixture
        // checks FK/restrict compatibility without preempting its migration.
        sqlx::query(
            "CREATE TABLE entry_terms (
                entry_id INTEGER NOT NULL REFERENCES content_entries(id) ON DELETE CASCADE,
                term_id INTEGER NOT NULL REFERENCES taxonomy_terms(id) ON DELETE RESTRICT,
                PRIMARY KEY (entry_id, term_id)
            )",
        )
        .execute(&db)
        .await
        .unwrap();
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
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
            .unwrap()
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
            patch_term(&db, &actor, a.id, 1, None, None, Some(Some(b.id))),
            patch_term(&db, &actor, b.id, 1, None, None, Some(Some(a.id)))
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
}
