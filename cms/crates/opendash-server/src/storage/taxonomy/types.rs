//! Site-owned taxonomy. All category tree validation and writes share one
//! SQLite writer reservation; no parent/subtree read precedes that reservation.

use super::*;

pub(super) use std::collections::BTreeMap;

pub(super) use opendash_core::{
    content::{TaxonomyDefinition, TaxonomyDefinitionInput, TaxonomyDefinitionPatch},
    registry::{DeclarationState, ProviderStatus, ResourceOwner},
};

pub(super) use serde::Serialize;

pub(super) use serde_json::Value;

pub(super) use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

pub(super) use crate::{
    auth::{self, SessionContext},
    error::{ApiError, ApiResult},
};

pub(in crate::storage) const MAX_CATEGORY_DEPTH: i64 = 8;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TermTranslationSummary {
    pub id: i64,
    pub locale: String,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaxonomyTerm {
    pub id: i64,
    pub kind: String,
    pub locale: String,
    pub translation_group_id: String,
    pub name: String,
    pub slug: String,
    pub parent_id: Option<i64>,
    pub version: i64,
    pub featured_image_media_id: Option<String>,
    pub translations: Vec<TermTranslationSummary>,
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
    pub locale: String,
    pub name: String,
}

pub(in crate::storage) type TermRow = (
    i64,
    String,
    String,
    String,
    Option<i64>,
    i64,
    String,
    String,
    Option<String>,
);

pub(in crate::storage) fn term(row: TermRow) -> TaxonomyTerm {
    TaxonomyTerm {
        id: row.0,
        kind: row.1,
        locale: row.6,
        translation_group_id: row.7,
        name: row.2,
        slug: row.3,
        parent_id: row.4,
        version: row.5,
        featured_image_media_id: row.8,
        translations: Vec::new(),
        extensions: None,
        schema_token: None,
    }
}

pub(in crate::storage) async fn batch_term_translations_in_conn(
    conn: &mut SqliteConnection,
    group_ids: &[String],
) -> ApiResult<BTreeMap<(String, String), Vec<TermTranslationSummary>>> {
    let mut out = BTreeMap::<(String, String), Vec<TermTranslationSummary>>::new();
    if group_ids.is_empty() {
        return Ok(out);
    }
    let ids_json = serde_json::to_string(group_ids)?;
    let rows: Vec<(String, String, i64, String, String, String)> = sqlx::query_as(
        "SELECT t.kind, t.translation_group_id, t.id, t.locale, t.name, t.slug
           FROM taxonomy_terms t
           JOIN site_locales sl ON sl.code = t.locale AND sl.deleted_at IS NULL
          WHERE t.translation_group_id IN (SELECT value FROM json_each(?))
          ORDER BY sl.is_default DESC, sl.position ASC, sl.code ASC",
    )
    .bind(ids_json)
    .fetch_all(conn)
    .await?;
    for (kind, group_id, id, locale, name, slug) in rows {
        out.entry((kind, group_id))
            .or_default()
            .push(TermTranslationSummary {
                id,
                locale,
                name,
                slug,
            });
    }
    Ok(out)
}

pub(in crate::storage) async fn term_in_write(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
) -> ApiResult<Option<TaxonomyTerm>> {
    let row: Option<TermRow> = sqlx::query_as(
        "SELECT id, kind, name, slug, parent_id, version, locale, translation_group_id, featured_image_media_id
         FROM taxonomy_terms WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(mutation_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let mut rec = term(row);
    let mut map =
        batch_term_translations_in_conn(&mut **tx, &[rec.translation_group_id.clone()]).await?;
    rec.translations = map
        .remove(&(rec.kind.clone(), rec.translation_group_id.clone()))
        .unwrap_or_default();
    Ok(Some(rec))
}

pub(in crate::storage) async fn current_term(
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

/// Check the candidate parent and all its ancestors while holding the writer.
/// A moving node anywhere in that lineage means the requested edge is cyclic.
pub(in crate::storage) async fn check_taxonomy_parent(
    tx: &mut Transaction<'_, Sqlite>,
    taxonomy_key: &str,
    parent_id: Option<i64>,
    moving_id: Option<i64>,
    moving_height: i64,
    target_locale: &str,
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
    let parent: Option<(String, String)> =
        sqlx::query_as("SELECT kind, locale FROM taxonomy_terms WHERE id = ?")
            .bind(parent_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(mutation_error)?;
    match parent {
        Some((kind, locale)) if kind == taxonomy_key && locale == target_locale => {}
        Some((kind, _)) if kind == taxonomy_key => {
            return Err(ApiError::bad_request(
                "Term cha thuộc ngôn ngữ khác với term hiện tại.",
            ));
        }
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
    list_terms_for_locale(db, kind, None, limit, offset).await
}

pub async fn list_terms_for_locale(
    db: &SqlitePool,
    kind: Option<&str>,
    locale: Option<&str>,
    limit: i64,
    offset: i64,
) -> ApiResult<TaxonomyPage> {
    list_terms_for_host(db, kind, locale, limit, offset, false).await
}

pub(crate) async fn list_terms_for_host(
    db: &SqlitePool,
    kind: Option<&str>,
    locale: Option<&str>,
    limit: i64,
    offset: i64,
    enforce_public_locale: bool,
) -> ApiResult<TaxonomyPage> {
    let mut tx = db.begin().await?;
    let resolved_locale = match locale {
        Some(raw) => Some(
            crate::storage::settings::resolve_requested_locale_in_conn(&mut tx, Some(raw)).await?,
        ),
        None => None,
    };
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM taxonomy_terms
          WHERE (? IS NULL OR kind = ?)
            AND (? IS NULL OR locale = ?)
            AND EXISTS (SELECT 1 FROM site_locales sl WHERE sl.code = taxonomy_terms.locale
                AND sl.deleted_at IS NULL AND (? = 0 OR
                    (sl.enabled = 1 AND (sl.is_default = 1 OR
                      (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1))))",
    )
    .bind(kind)
    .bind(kind)
    .bind(resolved_locale.as_deref())
    .bind(resolved_locale.as_deref())
    .bind(i64::from(enforce_public_locale))
    .fetch_one(&mut *tx)
    .await?;
    let rows: Vec<TermRow> = sqlx::query_as(
        "SELECT id, kind, name, slug, parent_id, version, locale, translation_group_id, featured_image_media_id
           FROM taxonomy_terms
          WHERE (? IS NULL OR kind = ?)
            AND (? IS NULL OR locale = ?)
            AND EXISTS (SELECT 1 FROM site_locales sl WHERE sl.code = taxonomy_terms.locale
                AND sl.deleted_at IS NULL AND (? = 0 OR
                    (sl.enabled = 1 AND (sl.is_default = 1 OR
                      (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1))))
          ORDER BY name COLLATE BINARY ASC, id ASC
          LIMIT ? OFFSET ?",
    )
    .bind(kind)
    .bind(kind)
    .bind(resolved_locale.as_deref())
    .bind(resolved_locale.as_deref())
    .bind(i64::from(enforce_public_locale))
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await?;
    let group_ids: Vec<String> = rows
        .iter()
        .map(|row| row.7.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let translations = batch_term_translations_in_conn(&mut tx, &group_ids).await?;
    tx.commit().await?;
    let items = rows
        .into_iter()
        .map(|row| {
            let mut rec = term(row);
            rec.translations = translations
                .get(&(rec.kind.clone(), rec.translation_group_id.clone()))
                .cloned()
                .unwrap_or_default();
            rec
        })
        .collect();
    Ok(TaxonomyPage { items, total })
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
    let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT t.id, t.kind, t.locale, t.name
         FROM taxonomy_terms t JOIN json_each(?) ids ON ids.value = t.id
         ORDER BY t.id",
    )
    .bind(raw)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, kind, locale, name)| TaxonomyTermLabel {
            id,
            kind,
            locale,
            name,
        })
        .collect())
}

/// Read one stable term ID for a dedicated editor route. Authorization belongs
/// to the route; this projection is identical to the paged list DTO.
pub async fn get_term(db: &SqlitePool, id: i64) -> ApiResult<Option<TaxonomyTerm>> {
    let mut tx = db.begin().await?;
    let row: Option<TermRow> = sqlx::query_as(
        "SELECT id, kind, name, slug, parent_id, version, locale, translation_group_id, featured_image_media_id
           FROM taxonomy_terms WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let rec = match row {
        Some(row) => {
            let mut rec = term(row);
            let mut map =
                batch_term_translations_in_conn(&mut tx, &[rec.translation_group_id.clone()])
                    .await?;
            rec.translations = map
                .remove(&(rec.kind.clone(), rec.translation_group_id.clone()))
                .unwrap_or_default();
            Some(rec)
        }
        None => None,
    };
    tx.commit().await?;
    Ok(rec)
}

/// Add only role-readable effective declarations/values to the authenticated
/// single-term projection. List DTO stays legacy-compatible and value-free.
pub async fn term_with_effective_extensions(
    db: &SqlitePool,
    mut record: TaxonomyTerm,
    role: &str,
) -> ApiResult<TaxonomyTerm> {
    let config =
        crate::storage::admin_extensions::effective_term(db, record.id, &record.kind, role).await?;
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
    create_term_with_locale(db, actor, kind, name, slug, parent_id, None, None, None).await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_term_with_locale(
    db: &SqlitePool,
    actor: &SessionContext,
    kind: &str,
    name: &str,
    slug: &str,
    parent_id: Option<i64>,
    locale: Option<&str>,
    source_term_id: Option<i64>,
    featured_image_media_id: Option<&str>,
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
    if source_term_id.is_some_and(|sid| sid <= 0) {
        return Err(ApiError::bad_request("source_term_id không hợp lệ."));
    }
    let mut tx = begin_admin_write(db, actor, false).await?;
    let taxonomy = definition_in_write(&mut tx, kind)
        .await?
        .ok_or_else(|| ApiError::bad_request("Taxonomy không tồn tại."))?;
    if taxonomy.provider_status != ProviderStatus::Active
        || taxonomy.declaration_state != DeclarationState::Declared
    {
        return Err(ApiError::conflict("Taxonomy không còn khả dụng."));
    }
    if !taxonomy.hierarchical && parent_id.is_some() {
        return Err(ApiError::bad_request(
            "Taxonomy không phân cấp không nhận parent.",
        ));
    }
    let target_locale =
        crate::storage::settings::resolve_requested_locale_in_conn(&mut tx, locale).await?;
    let default_code = crate::storage::settings::get_default_locale_code_in_conn(&mut tx).await?;
    if target_locale != default_code || source_term_id.is_some() {
        crate::storage::settings::require_multilingual_enabled_in_conn(&mut tx).await?;
    }

    let mut effective_parent_id = parent_id;
    let group_id = if let Some(src_id) = source_term_id {
        let src: Option<(String, String, String, Option<i64>)> = sqlx::query_as(
            "SELECT kind, locale, translation_group_id, parent_id
               FROM taxonomy_terms WHERE id = ?
                 AND EXISTS (SELECT 1 FROM site_locales sl
                    WHERE sl.code = taxonomy_terms.locale AND sl.deleted_at IS NULL)",
        )
        .bind(src_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(mutation_error)?;
        let (src_kind, src_locale, src_group, src_parent_id) =
            src.ok_or_else(|| ApiError::not_found("Không tìm thấy term nguồn để dịch."))?;
        if src_kind != kind {
            return Err(ApiError::bad_request(
                "Term nguồn không thuộc cùng loại taxonomy.",
            ));
        }
        if src_locale == target_locale {
            return Err(ApiError::unprocessable(
                "Ngôn ngữ đích phải khác ngôn ngữ của term nguồn.",
            ));
        }
        let dup_group: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1 FROM taxonomy_terms
                WHERE kind = ? AND translation_group_id = ? AND locale = ?
             )",
        )
        .bind(kind)
        .bind(&src_group)
        .bind(&target_locale)
        .fetch_one(&mut *tx)
        .await
        .map_err(mutation_error)?;
        if dup_group != 0 {
            return Err(ApiError::conflict(
                "Nhóm dịch đã có bản dịch ở ngôn ngữ đích.",
            ));
        }
        if effective_parent_id.is_none()
            && taxonomy.hierarchical
            && let Some(sp_id) = src_parent_id
        {
            effective_parent_id = sqlx::query_scalar(
                "SELECT dst.id
                   FROM taxonomy_terms src
                   JOIN taxonomy_terms dst
                     ON dst.kind = src.kind
                    AND dst.translation_group_id = src.translation_group_id
                    AND dst.locale = ?
                  WHERE src.id = ?",
            )
            .bind(&target_locale)
            .bind(sp_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(mutation_error)?;
        }
        src_group
    } else {
        crate::storage::settings::generate_translation_group_id("tg")
    };

    if taxonomy.hierarchical {
        check_taxonomy_parent(&mut tx, kind, effective_parent_id, None, 1, &target_locale).await?;
    }
    let validated_featured_image = match featured_image_media_id {
        Some(raw) => Some(validate_featured_image_in_tx(&mut tx, raw).await?),
        None => None,
    };
    let id = sqlx::query(
        "INSERT INTO taxonomy_terms(kind, locale, translation_group_id, name, slug, parent_id, featured_image_media_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(kind)
    .bind(&target_locale)
    .bind(&group_id)
    .bind(name)
    .bind(slug)
    .bind(effective_parent_id)
    .bind(validated_featured_image.as_deref())
    .execute(&mut *tx)
    .await
    .map_err(mutation_error)?
    .last_insert_rowid();
    let created = term_in_write(&mut tx, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải term vừa tạo."))?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await.map_err(mutation_error)?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(created)
}

#[allow(clippy::too_many_arguments)]
pub async fn patch_term(
    db: &SqlitePool,
    actor: &SessionContext,
    id: i64,
    expected_version: i64,
    name: Option<&str>,
    slug: Option<&str>,
    parent_id: Option<Option<i64>>,
    featured_image_media_id: Option<Option<&str>>,
    extensions: Option<&BTreeMap<String, Value>>,
    schema_token: Option<&str>,
) -> ApiResult<TaxonomyTerm> {
    let name = name.map(normalized_name).transpose()?;
    if slug.is_some_and(|s| !opendash_core::id::valid_kebab_segment_with_limit(s, 64)) {
        return Err(ApiError::bad_request(
            "Slug phải là kebab-case, tối đa 64 ký tự.",
        ));
    }
    let editor_extension_only = actor.user.role == crate::auth::Role::Editor
        && name.is_none()
        && slug.is_none()
        && parent_id.is_none()
        && featured_image_media_id.is_none()
        && extensions.is_some();
    let mut tx = begin_admin_write(db, actor, editor_extension_only).await?;
    if actor.user.role != crate::auth::Role::Admin && !editor_extension_only {
        return Err(ApiError::forbidden(
            "Editor không được sửa trường taxonomy lõi.",
        ));
    }
    let existing = current_term(&mut tx, id, expected_version).await?;
    let next_parent = parent_id.unwrap_or(existing.parent_id);
    let next_featured_image = match featured_image_media_id {
        Some(Some(raw)) => Some(validate_featured_image_in_tx(&mut tx, raw).await?),
        Some(None) => None,
        None => existing.featured_image_media_id.clone(),
    };
    let definition = definition_in_write(&mut tx, &existing.kind)
        .await?
        .ok_or_else(|| ApiError::bad_request("Taxonomy không tồn tại."))?;
    if !definition.hierarchical && next_parent.is_some() {
        return Err(ApiError::bad_request(
            "Taxonomy không phân cấp không nhận parent.",
        ));
    }
    if definition.hierarchical && parent_id.is_some() {
        let height = subtree_height(&mut tx, id).await?;
        check_taxonomy_parent(
            &mut tx,
            &existing.kind,
            next_parent,
            Some(id),
            height,
            &existing.locale,
        )
        .await?;
    }
    if let Some(patch) = extensions {
        let token = schema_token
            .ok_or_else(|| ApiError::bad_request("Thiếu schema_token để ghi extension field."))?;
        // All validation and writes happen inside the same writer reservation
        // as the core version CAS, so any failed field rolls everything back.
        crate::storage::admin_extensions::update_term_values_in_conn(
            &mut tx,
            id,
            &existing.kind,
            if actor.user.role == crate::auth::Role::Admin {
                "admin"
            } else {
                "editor"
            },
            token,
            patch,
        )
        .await?;
    }
    let updated = sqlx::query(
        "UPDATE taxonomy_terms
            SET name = ?, slug = ?, parent_id = ?, featured_image_media_id = ?, version = version + 1
          WHERE id = ? AND version = ?",
    )
    .bind(name.unwrap_or(&existing.name))
    .bind(slug.unwrap_or(&existing.slug))
    .bind(next_parent)
    .bind(next_featured_image.as_deref())
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
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(record)
}
