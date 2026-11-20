//! Site-owned taxonomy. All category tree validation and writes share one
//! SQLite writer reservation; no parent/subtree read precedes that reservation.

use std::collections::BTreeMap;

use opendash_core::{
    content::{TaxonomyDefinition, TaxonomyDefinitionInput, TaxonomyDefinitionPatch},
    registry::{DeclarationState, ProviderStatus, ResourceOwner},
};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<BTreeMap<String, BTreeMap<String, Value>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TaxonomyPage {
    pub items: Vec<TaxonomyTerm>,
    pub total: i64,
}

/// Small read-only projection for labels referenced by the current Admin page.
/// Unlike term lists, this does not enumerate a taxonomy's full catalog.
#[derive(Debug, Clone, Serialize)]
pub struct TaxonomyTermLabel {
    pub id: i64,
    pub kind: String,
    pub name: String,
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
        extensions: None,
        schema_token: None,
    }
}

type DefinitionRow = (
    String, String, i64, i64, i64, String, Option<String>, String, String,
);

fn definition(row: DefinitionRow) -> ApiResult<TaxonomyDefinition> {
    let (key, label, hierarchical, public, version, owner_kind, owner_id, provider, state) = row;
    let owner = match (owner_kind.as_str(), owner_id) {
        ("core", None) => ResourceOwner::Core,
        ("user", None) => ResourceOwner::User,
        ("plugin", Some(id)) => ResourceOwner::Plugin(id),
        ("theme", Some(id)) => ResourceOwner::Theme(id),
        _ => return Err(ApiError::internal("Chủ sở hữu taxonomy không hợp lệ.")),
    };
    let provider_status = match provider.as_str() {
        "active" => ProviderStatus::Active,
        "provider_inactive" => ProviderStatus::ProviderInactive,
        _ => return Err(ApiError::internal("Trạng thái provider taxonomy không hợp lệ.")),
    };
    let declaration_state = match state.as_str() {
        "declared" => DeclarationState::Declared,
        "withdrawn" => DeclarationState::Withdrawn,
        _ => return Err(ApiError::internal("Trạng thái khai báo taxonomy không hợp lệ.")),
    };
    Ok(TaxonomyDefinition {
        key, label, hierarchical: hierarchical != 0, public: public != 0,
        version, owner, provider_status, declaration_state,
    })
}

async fn definition_in_write(
    tx: &mut Transaction<'_, Sqlite>, key: &str,
) -> ApiResult<Option<TaxonomyDefinition>> {
    let row: Option<DefinitionRow> = sqlx::query_as(
        "SELECT key, label, hierarchical, public, version, owner_kind, owner_id,
                provider_status, declaration_state FROM taxonomy_definitions WHERE key = ?",
    )
    .bind(key).fetch_optional(&mut **tx).await.map_err(mutation_error)?;
    row.map(definition).transpose()
}

pub async fn list_definitions(db: &SqlitePool) -> ApiResult<Vec<TaxonomyDefinition>> {
    let rows: Vec<DefinitionRow> = sqlx::query_as(
        "SELECT key, label, hierarchical, public, version, owner_kind, owner_id,
                provider_status, declaration_state FROM taxonomy_definitions ORDER BY key",
    ).fetch_all(db).await?;
    rows.into_iter().map(definition).collect()
}

pub async fn get_definition(db: &SqlitePool, key: &str) -> ApiResult<Option<TaxonomyDefinition>> {
    let row: Option<DefinitionRow> = sqlx::query_as(
        "SELECT key, label, hierarchical, public, version, owner_kind, owner_id,
                provider_status, declaration_state FROM taxonomy_definitions WHERE key = ?",
    ).bind(key).fetch_optional(db).await?;
    row.map(definition).transpose()
}

/// Reconcile provider-owned custom taxonomy declarations in the same writer
/// transaction as its CPTs, settings and permalink claims. Withdrawal and
/// disable are state transitions: they never delete terms or memberships.
pub async fn sync_provider_definitions_in_tx(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    declarations: &[opendash_core::content::TaxonomyDeclaration],
) -> ApiResult<()> {
    if !matches!(owner_kind, "theme" | "plugin") || owner_id.is_empty() {
        return Err(ApiError::bad_request("Provider taxonomy owner không hợp lệ."));
    }
    let mut declared = std::collections::BTreeSet::new();
    for taxonomy in declarations {
        let input = TaxonomyDefinitionInput {
            key: taxonomy.key.clone(),
            label: taxonomy.label.clone(),
            hierarchical: taxonomy.hierarchical,
            public: taxonomy.public,
        };
        input.validate().map_err(ApiError::bad_request)?;
        if !declared.insert(taxonomy.key.as_str()) {
            return Err(ApiError::bad_request("Provider khai báo taxonomy trùng key."));
        }
        let existing: Option<DefinitionRow> = sqlx::query_as(
            "SELECT key,label,hierarchical,public,version,owner_kind,owner_id,
                    provider_status,declaration_state FROM taxonomy_definitions WHERE key=?",
        )
        .bind(&taxonomy.key)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(row) = existing {
            let current = definition(row)?;
            if current.owner != match owner_kind {
                "theme" => ResourceOwner::Theme(owner_id.to_owned()),
                _ => ResourceOwner::Plugin(owner_id.to_owned()),
            } {
                return Err(ApiError::conflict("Taxonomy key đã thuộc owner khác."));
            }
            if current.hierarchical && !taxonomy.hierarchical {
                let children: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM taxonomy_terms WHERE kind=? AND parent_id IS NOT NULL",
                )
                .bind(&taxonomy.key)
                .fetch_one(&mut *conn)
                .await?;
                if children != 0 {
                    return Err(ApiError::conflict("Taxonomy còn term có parent."));
                }
            }
            let changed = current.label != taxonomy.label
                || current.hierarchical != taxonomy.hierarchical
                || current.public != taxonomy.public
                || current.provider_status != ProviderStatus::Active
                || current.declaration_state != DeclarationState::Declared;
            if changed {
                sqlx::query(
                    "UPDATE taxonomy_definitions SET label=?,hierarchical=?,public=?,
                     provider_status='active',declaration_state='declared',version=version+1
                     WHERE key=?",
                )
                .bind(&taxonomy.label)
                .bind(i64::from(taxonomy.hierarchical))
                .bind(i64::from(taxonomy.public))
                .bind(&taxonomy.key)
                .execute(&mut *conn)
                .await?;
            }
        } else {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM taxonomy_definitions")
                .fetch_one(&mut *conn)
                .await?;
            if count >= 64 {
                return Err(ApiError::conflict("Website đạt giới hạn 64 taxonomy."));
            }
            sqlx::query(
                "INSERT INTO taxonomy_definitions
                 (key,label,hierarchical,public,owner_kind,owner_id)
                 VALUES (?,?,?,?,?,?)",
            )
            .bind(&taxonomy.key)
            .bind(&taxonomy.label)
            .bind(i64::from(taxonomy.hierarchical))
            .bind(i64::from(taxonomy.public))
            .bind(owner_kind)
            .bind(owner_id)
            .execute(&mut *conn)
            .await?;
        }
    }
    let owned: Vec<String> = sqlx::query_scalar(
        "SELECT key FROM taxonomy_definitions WHERE owner_kind=? AND owner_id=?",
    )
    .bind(owner_kind)
    .bind(owner_id)
    .fetch_all(&mut *conn)
    .await?;
    for key in owned {
        if !declared.contains(key.as_str()) {
            sqlx::query(
                "UPDATE taxonomy_definitions SET declaration_state='withdrawn',
                 version=version+1 WHERE key=? AND declaration_state!='withdrawn'",
            )
            .bind(key)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

pub async fn set_provider_definitions_status_in_tx(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    active: bool,
) -> ApiResult<()> {
    let status = if active { "active" } else { "provider_inactive" };
    sqlx::query(
        "UPDATE taxonomy_definitions SET provider_status=?,version=version+1
         WHERE owner_kind=? AND owner_id=? AND provider_status!=?
           AND (?=0 OR declaration_state='declared')",
    )
    .bind(status)
    .bind(owner_kind)
    .bind(owner_id)
    .bind(status)
    .bind(i64::from(active))
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn deactivate_other_theme_definitions_in_tx(
    conn: &mut SqliteConnection,
    active_theme_id: &str,
) -> ApiResult<()> {
    sqlx::query(
        "UPDATE taxonomy_definitions SET provider_status='provider_inactive',version=version+1
         WHERE owner_kind='theme' AND owner_id!=? AND provider_status!='provider_inactive'",
    )
    .bind(active_theme_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn create_definition(
    db: &SqlitePool, actor: &SessionContext, input: &TaxonomyDefinitionInput,
) -> ApiResult<TaxonomyDefinition> {
    input.validate().map_err(ApiError::bad_request)?;
    let mut tx = begin_admin_write(db, actor, false).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM taxonomy_definitions")
        .fetch_one(&mut *tx).await.map_err(mutation_error)?;
    if count >= 64 {
        return Err(ApiError::conflict("Website đạt giới hạn 64 taxonomy."));
    }
    if definition_in_write(&mut tx, &input.key).await?.is_some() {
        return Err(ApiError::conflict("Khóa taxonomy đã được đăng ký."));
    }
    sqlx::query(
        "INSERT INTO taxonomy_definitions (key,label,hierarchical,public,owner_kind)
         VALUES (?,?,?,?, 'user')",
    )
    .bind(&input.key).bind(&input.label)
    .bind(i64::from(input.hierarchical)).bind(i64::from(input.public))
    .execute(&mut *tx).await.map_err(mutation_error)?;
    let created = definition_in_write(&mut tx, &input.key).await?
        .ok_or_else(|| ApiError::internal("Không thể đọc taxonomy vừa tạo."))?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await.map_err(mutation_error)?;
    Ok(created)
}

pub async fn patch_definition(
    db: &SqlitePool, actor: &SessionContext, key: &str,
    expected_version: i64, input: &TaxonomyDefinitionPatch,
) -> ApiResult<TaxonomyDefinition> {
    input.validate().map_err(ApiError::bad_request)?;
    let mut tx = begin_admin_write(db, actor, false).await?;
    let current = definition_in_write(&mut tx, key).await?
        .ok_or_else(|| ApiError::not_found("Taxonomy không tồn tại."))?;
    if current.version != expected_version {
        return Err(ApiError::precondition_failed("Taxonomy đã thay đổi, hãy tải lại."));
    }
    if !matches!(current.owner, ResourceOwner::Core | ResourceOwner::User) {
        return Err(ApiError::conflict("Taxonomy thuộc provider, không thể sửa từ site."));
    }
    let hierarchical = input.hierarchical.unwrap_or(current.hierarchical);
    if matches!(current.owner, ResourceOwner::Core) && hierarchical != current.hierarchical {
        return Err(ApiError::conflict("Category/Tag có chế độ phân cấp bất biến."));
    }
    if current.hierarchical && !hierarchical {
        let parents: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM taxonomy_terms WHERE kind=? AND parent_id IS NOT NULL",
        ).bind(key).fetch_one(&mut *tx).await.map_err(mutation_error)?;
        if parents != 0 {
            return Err(ApiError::conflict("Taxonomy còn term có parent."));
        }
    }
    let label = input.label.as_deref().unwrap_or(&current.label);
    let public = input.public.unwrap_or(current.public);
    let rows = sqlx::query(
        "UPDATE taxonomy_definitions
            SET label=?, hierarchical=?, public=?, version=version+1
          WHERE key=? AND version=?",
    )
    .bind(label).bind(i64::from(hierarchical)).bind(i64::from(public))
    .bind(key).bind(expected_version)
    .execute(&mut *tx).await.map_err(mutation_error)?.rows_affected();
    if rows != 1 {
        return Err(ApiError::precondition_failed("Taxonomy đã thay đổi, hãy tải lại."));
    }
    let updated = definition_in_write(&mut tx, key).await?
        .ok_or_else(|| ApiError::internal("Không thể tải taxonomy vừa sửa."))?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await.map_err(mutation_error)?;
    Ok(updated)
}

pub async fn delete_definition(
    db: &SqlitePool, actor: &SessionContext, key: &str, expected_version: i64,
) -> ApiResult<()> {
    let mut tx = begin_admin_write(db, actor, false).await?;
    let current = definition_in_write(&mut tx, key).await?
        .ok_or_else(|| ApiError::not_found("Taxonomy không tồn tại."))?;
    if current.version != expected_version {
        return Err(ApiError::precondition_failed("Taxonomy đã thay đổi, hãy tải lại."));
    }
    if current.owner != ResourceOwner::User {
        return Err(ApiError::conflict("Không thể xóa taxonomy Core/provider."));
    }
    let dependencies: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM taxonomy_terms WHERE kind=?) +
                (SELECT COUNT(*) FROM content_type_taxonomy_eligibility WHERE taxonomy_kind=?) +
                (SELECT COUNT(*) FROM content_type_taxonomy_overrides
                  WHERE EXISTS (SELECT 1 FROM json_each(taxonomies_json) WHERE value=?))",
    ).bind(key).bind(key).bind(key)
    .fetch_one(&mut *tx).await.map_err(mutation_error)?;
    if dependencies > 0 {
        return Err(ApiError::conflict("Taxonomy đang được dùng bởi term hoặc content type."));
    }
    sqlx::query("DELETE FROM taxonomy_definitions WHERE key=? AND version=?")
        .bind(key).bind(expected_version)
        .execute(&mut *tx).await.map_err(mutation_error)?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await.map_err(mutation_error)?;
    Ok(())
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
    allow_editor_extensions: bool,
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
        Some((_, role)) if role != "admin" &&
            !(allow_editor_extensions && role == "editor") => Err(ApiError::forbidden(
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
async fn check_taxonomy_parent(
    tx: &mut Transaction<'_, Sqlite>,
    taxonomy_key: &str,
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
        Some((kind,)) if kind == taxonomy_key => {}
        _ => {
            return Err(ApiError::bad_request(
                "Term cha không tồn tại hoặc thuộc taxonomy khác.",
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

/// The route enforces at most 100 distinct IDs. JSON array binding keeps the
/// query parameterized regardless of the number of requested IDs.
pub async fn resolve_term_labels(
    db: &SqlitePool,
    ids: &[i64],
) -> ApiResult<Vec<TaxonomyTermLabel>> {
    if ids.is_empty() || ids.len() > 100 || ids.iter().any(|id| *id <= 0) {
        return Err(ApiError::bad_request("Danh sách ID term không hợp lệ."));
    }
    let raw = serde_json::to_string(ids)?;
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT t.id, t.kind, t.name
         FROM taxonomy_terms t JOIN json_each(?) ids ON ids.value = t.id
         ORDER BY t.id",
    )
    .bind(raw)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(id, kind, name)| TaxonomyTermLabel {
        id, kind, name,
    }).collect())
}

/// Read one stable term ID for a dedicated editor route. Authorization belongs
/// to the route; this projection is identical to the paged list DTO.
pub async fn get_term(db: &SqlitePool, id: i64) -> ApiResult<Option<TaxonomyTerm>> {
    let row: Option<TermRow> = sqlx::query_as(
        "SELECT id, kind, name, slug, parent_id, version
           FROM taxonomy_terms WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(term))
}

/// Add only role-readable effective declarations/values to the authenticated
/// single-term projection. List DTO stays legacy-compatible and value-free.
pub async fn term_with_effective_extensions(
    db: &SqlitePool,
    mut record: TaxonomyTerm,
    role: &str,
) -> ApiResult<TaxonomyTerm> {
    let config = crate::storage::admin_extensions::effective_term(
        db, record.id, &record.kind, role,
    ).await?;
    record.extensions = Some(config.extensions);
    record.schema_token = Some(config.schema_token);
    Ok(record)
}

pub async fn create_term(
    db: &SqlitePool,
    actor: &SessionContext,
    kind: &str,
    name: &str,
    slug: &str,
    parent_id: Option<i64>,
) -> ApiResult<TaxonomyTerm> {
    if !opendash_core::content::valid_content_type_key(kind) {
        return Err(ApiError::bad_request("Khóa taxonomy không hợp lệ."));
    }
    let name = normalized_name(name)?;
    if !opendash_core::id::valid_kebab_segment_with_limit(slug, 64) {
        return Err(ApiError::bad_request(
            "Slug phải là kebab-case, tối đa 64 ký tự.",
        ));
    }
    let mut tx = begin_admin_write(db, actor, false).await?;
    let taxonomy = definition_in_write(&mut tx, kind).await?
        .ok_or_else(|| ApiError::bad_request("Taxonomy không tồn tại."))?;
    if taxonomy.provider_status != ProviderStatus::Active
        || taxonomy.declaration_state != DeclarationState::Declared
    {
        return Err(ApiError::conflict("Taxonomy không còn khả dụng."));
    }
    if !taxonomy.hierarchical && parent_id.is_some() {
        return Err(ApiError::bad_request("Taxonomy không phân cấp không nhận parent."));
    }
    if taxonomy.hierarchical {
        check_taxonomy_parent(&mut tx, kind, parent_id, None, 1).await?;
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
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
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
    extensions: Option<&BTreeMap<String, Value>>,
    schema_token: Option<&str>,
) -> ApiResult<TaxonomyTerm> {
    let name = name.map(normalized_name).transpose()?;
    if slug.is_some_and(|s| !opendash_core::id::valid_kebab_segment_with_limit(s, 64)) {
        return Err(ApiError::bad_request(
            "Slug phải là kebab-case, tối đa 64 ký tự.",
        ));
    }
    let editor_extension_only =
        actor.user.role == crate::auth::Role::Editor &&
        name.is_none() && slug.is_none() && parent_id.is_none() && extensions.is_some();
    let mut tx = begin_admin_write(db, actor, editor_extension_only).await?;
    if actor.user.role != crate::auth::Role::Admin && !editor_extension_only {
        return Err(ApiError::forbidden("Editor không được sửa trường taxonomy lõi."));
    }
    let existing = current_term(&mut tx, id, expected_version).await?;
    let next_parent = parent_id.unwrap_or(existing.parent_id);
    let definition = definition_in_write(&mut tx, &existing.kind).await?
        .ok_or_else(|| ApiError::bad_request("Taxonomy không tồn tại."))?;
    if !definition.hierarchical && next_parent.is_some() {
        return Err(ApiError::bad_request("Taxonomy không phân cấp không nhận parent."));
    }
    if definition.hierarchical && parent_id.is_some() {
        let height = subtree_height(&mut tx, id).await?;
        check_taxonomy_parent(&mut tx, &existing.kind, next_parent, Some(id), height).await?;
    }
    if let Some(patch) = extensions {
        let token = schema_token.ok_or_else(|| ApiError::bad_request(
            "Thiếu schema_token để ghi extension field.",
        ))?;
        // All validation and writes happen inside the same writer reservation
        // as the core version CAS, so any failed field rolls everything back.
        crate::storage::admin_extensions::update_term_values_in_conn(
            &mut tx, id, &existing.kind,
            if actor.user.role == crate::auth::Role::Admin { "admin" } else { "editor" },
            token, patch,
        ).await?;
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
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await.map_err(mutation_error)?;
    Ok(record)
}

pub async fn delete_term(
    db: &SqlitePool,
    actor: &SessionContext,
    id: i64,
    expected_version: i64,
) -> ApiResult<()> {
    let mut tx = begin_admin_write(db, actor, false).await?;
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
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
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
    async fn bulk_term_labels_only_project_requested_ids_even_with_large_catalog() {
        let (db, _) = memory_fixture().await;
        let mut requested = Vec::new();
        // Many unrelated terms must never appear in a table-row projection.
        for i in 0..250 {
            let kind = if i % 2 == 0 { "category" } else { "tag" };
            let id = sqlx::query("INSERT INTO taxonomy_terms(kind, name, slug) VALUES(?, ?, ?)")
                .bind(kind).bind(format!("Term {i}")).bind(format!("bulk-resolve-{i}"))
                .execute(&db).await.unwrap().last_insert_rowid();
            if matches!(i, 0 | 111 | 249) { requested.push(id); }
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
        assert_eq!(resolve_term_labels(&db, &[0]).await.unwrap_err().0,
            axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(resolve_term_labels(&db, &(1..=101).collect::<Vec<_>>()).await.unwrap_err().0,
            axum::http::StatusCode::BAD_REQUEST);
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

        let changed = patch_term(&db, &actor, child.id, 1, Some("Renamed"), None, Some(None), None, None)
            .await
            .unwrap();
        assert_eq!(changed.version, 2);
        assert_eq!(changed.parent_id, None);
        assert_eq!(
            patch_term(&db, &actor, child.id, 1, Some("Stale"), None, None, None, None)
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
            patch_term(&db, &actor, a.id, 1, None, None, Some(Some(b.id)), None, None),
            patch_term(&db, &actor, b.id, 1, None, None, Some(Some(a.id)), None, None)
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
