//! Read-only public content lookup and media reference ACL.
//! Public controllers never issue SQL directly.

use sqlx::{Sqlite, SqlitePool, Transaction};

use super::entries::{self, ContentEntryRecord, EntryFilters, decode_row};
use crate::error::ApiResult;
use crate::public_context::{PublicFilters, TaxonomyKind};

/// Slug is unique within each CPT; lookup never relies on a numeric admin ID.
pub async fn get_by_slug(
    db: &SqlitePool,
    type_key: &str,
    slug: &str,
) -> ApiResult<Option<ContentEntryRecord>> {
    if type_key == "pattern" {
        return Ok(None);
    }
    let mut tx = db.begin().await?;
    let row: Option<entries::EntryRow> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                e.publication_status, e.version, e.created_at, e.updated_at
         FROM content_entries e
         JOIN content_types t ON t.key = e.content_type_key
         WHERE e.content_type_key = ? AND e.slug = ? AND e.publication_status = 'published'
           AND t.provider_status = 'active'
           AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
    )
    .bind(type_key)
    .bind(slug)
    .fetch_optional(&mut *tx)
    .await?;
    let record = match row {
        Some(row) => Some(entries::decoded_with_terms(&mut tx, row).await?),
        None => None,
    };
    tx.commit().await?;
    Ok(record)
}

pub async fn get_published_by_id(
    db: &SqlitePool,
    id: i64,
) -> ApiResult<Option<ContentEntryRecord>> {
    let mut tx = db.begin().await?;
    let result = get_published_by_id_in_tx(&mut tx, id).await?;
    tx.commit().await?;
    Ok(result)
}

async fn get_published_by_id_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
) -> ApiResult<Option<ContentEntryRecord>> {
    let row: Option<entries::EntryRow> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                e.publication_status, e.version, e.created_at, e.updated_at
         FROM content_entries e
         JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1
         JOIN content_types t ON t.key = e.content_type_key
         WHERE e.id = ? AND e.publication_status = 'published'
           AND e.content_type_key != 'pattern' AND t.provider_status = 'active'
           AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some(row) => Ok(Some(entries::decoded_with_terms(tx, row).await?)),
        None => Ok(None),
    }
}

pub struct PublicEntryRenderSnapshot {
    pub entry: ContentEntryRecord,
    pub locale: String,
    pub schema: opendash_core::content::ContentTypeDefinition,
    pub active: Option<super::theme::ActiveThemeV3>,
    pub resolution: super::overrides::TemplateResolution,
    pub body: Option<crate::page::RenderedLayout>,
    pub budget: crate::page::RenderBudget,
}

/// Resolve published entry, its own Builder body, and the selected shared
/// template using one read snapshot. The entry document never participates in
/// the shared template candidate chain.
pub async fn load_entry_render_snapshot(
    db: &SqlitePool,
    id: i64,
    themes_root: Option<&std::path::Path>,
) -> ApiResult<Option<PublicEntryRenderSnapshot>> {
    load_entry_render_snapshot_with_cache(
        db,
        id,
        themes_root,
        crate::page::BlockTemplateCache::global(),
    )
    .await
}

pub async fn load_entry_render_snapshot_with_cache(
    db: &SqlitePool,
    id: i64,
    themes_root: Option<&std::path::Path>,
    cache: &crate::page::BlockTemplateCache,
) -> ApiResult<Option<PublicEntryRenderSnapshot>> {
    load_entry_render_snapshot_with_options(
        db,
        id,
        themes_root,
        cache,
        "/",
        &std::collections::BTreeMap::new(),
    )
    .await
}

pub async fn load_entry_render_snapshot_with_options(
    db: &SqlitePool,
    id: i64,
    themes_root: Option<&std::path::Path>,
    cache: &crate::page::BlockTemplateCache,
    list_base_path: &str,
    list_pages: &std::collections::BTreeMap<String, i64>,
) -> ApiResult<Option<PublicEntryRenderSnapshot>> {
    use super::{builder, builder_binding, content, overrides, theme};
    use crate::error::ApiError;
    use opendash_core::{
        content::{ContentFieldType, ContentSupport, FieldRole, PublicExposure},
        registry::{DeclarationState, ProviderStatus},
        theme::TemplateCandidateKind,
    };
    let mut tx = db.begin().await?;
    let Some(entry) = get_published_by_id_in_tx(&mut tx, id).await? else {
        return Ok(None);
    };
    let entry_locale: String =
        sqlx::query_scalar("SELECT locale FROM content_entries WHERE id = ?")
            .bind(entry.id)
            .fetch_one(&mut *tx)
            .await?;
    let mut schema =
        content::load_content_type_definition_in_conn(&mut tx, &entry.content_type_key)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let effective =
        content::load_effective_content_schema_in_tx(&mut tx, &entry.content_type_key).await?;
    if let Some(eff) = &effective
        && !schema.fields.iter().any(|f| f.key == "body")
        && let Some(body_fe) = eff.effective_fields.get("body")
    {
        schema.fields.push(body_fe.definition.clone());
    }
    let active = match themes_root {
        Some(root) => theme::load_active_theme_v3_in_conn(&mut tx, root).await?,
        None => None,
    };
    let context = if entry.content_type_key == "page" {
        let page_layout_key = entry
            .page_layout_key
            .as_ref()
            .filter(|key| {
                active.as_ref().is_some_and(|theme| {
                    theme
                        .manifest
                        .page_layouts
                        .iter()
                        .any(|layout| &layout.key == *key)
                })
            })
            .cloned();
        overrides::PublicTemplateContext::Page { page_layout_key }
    } else {
        let custom_template_key = entry
            .custom_template_key
            .as_ref()
            .filter(|key| {
                active.as_ref().is_some_and(|theme| {
                    theme.manifest.templates.iter().any(|tpl| {
                        tpl.kind == TemplateCandidateKind::SingleCustom
                            && tpl.custom_key.as_ref() == Some(*key)
                            && tpl
                                .allowed_types
                                .iter()
                                .any(|allowed| allowed == &entry.content_type_key)
                    })
                })
            })
            .cloned();
        overrides::PublicTemplateContext::Single {
            content_type: entry.content_type_key.clone(),
            entry_id: entry.id,
            slug: entry.slug.clone(),
            custom_template_key,
        }
    };
    let candidates = overrides::build_candidates(&context)?;
    let resolution = overrides::resolve_template_2axis_on_connection(
        &mut tx,
        &candidates,
        active.as_ref().map(|v| &v.templates),
        active.as_ref().map(|v| &v.parts),
        active.as_ref().map(|v| &v.page_layouts),
    )
    .await?;
    let body_allows_public_builder = effective.as_ref().is_some_and(|eff| {
        eff.base.status == ProviderStatus::Active
            && eff.base.declaration_state == DeclarationState::Declared
            && eff.effective_fields.get("body").is_some_and(|fe| {
                fe.definition.field_type == ContentFieldType::RichText
                    && fe.definition.public_exposure == PublicExposure::Public
                    && fe.definition.min_view_role == FieldRole::Editor
            })
    });
    let mut render_budget = crate::page::RenderBudget::new();
    let raw_body = entry.values.get("body").and_then(serde_json::Value::as_str);
    let body = if !matches!(entry.content_type_key.as_str(), "pattern" | "section")
        && schema.settings.supports(ContentSupport::Builder)
        && body_allows_public_builder
        && raw_body.is_some_and(opendash_core::builder_blocks::contains_blocks)
    {
        let doc =
            opendash_core::builder_blocks::parse_blocks_lenient_or_recover(raw_body.unwrap_or(""))
                .document;
        let state = builder::read_home_tx(&mut tx).await?;
        let bindings = builder_binding::resolve_in_tx_with_route(
            &mut tx,
            &doc,
            false,
            Some((&entry.content_type_key, &entry.slug)),
        )
        .await?;
        let lists = builder::resolve_lists_in_tx(&mut tx, &doc, list_pages).await?;
        let forms = builder::resolve_contact_forms_in_tx(&mut tx, &doc).await?;
        let patterns = builder::resolve_patterns_in_tx_with_budget(
            &mut tx,
            &state,
            &doc,
            false,
            &mut render_budget,
            cache,
        )
        .await?;
        Some(crate::page::render_builder_with_budget_and_base_path(
            &state,
            &doc,
            false,
            &bindings,
            &lists,
            &forms,
            None,
            &patterns,
            false,
            crate::page::RenderAnnotationScope::None,
            "entry",
            list_base_path,
            &mut render_budget,
            cache,
        )?)
    } else {
        None
    };
    tx.commit().await?;
    Ok(Some(PublicEntryRenderSnapshot {
        entry,
        locale: entry_locale,
        schema,
        active,
        resolution,
        body,
        budget: render_budget,
    }))
}

/// Deny public archives independently from entry visibility. Legacy CPTs
/// without settings retain their existing public archive behavior.
pub async fn require_public_archive(db: &SqlitePool, type_key: &str) -> ApiResult<()> {
    let visible: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM content_types
            WHERE key = ? AND key != 'pattern' AND provider_status = 'active'
              AND COALESCE(json_extract(definition_json, '$.settings.public'), 1) = 1
              AND COALESCE(json_extract(definition_json, '$.settings.has_archive'), 1) = 1
        )",
    )
    .bind(type_key)
    .fetch_one(db)
    .await?;
    if visible == 0 {
        return Err(crate::error::ApiError::not_found(
            "Archive nội dung không tồn tại.",
        ));
    }
    Ok(())
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
    require_public_archive(db, type_key).await?;
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
    let mut conn = db.acquire().await?;
    crate::storage::permalinks::canonical_term_path_in_tx(&mut conn, id).await
}

#[derive(Debug, Clone)]
pub struct PublicTerm {
    pub id: i64,
    pub name: String,
    pub slug: String,
}

/// Validate category/tag/generic filters for a builtin (`category`/`tag`) or
/// custom taxonomy archive within one read snapshot.
async fn validate_taxonomy_page_filters_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    kind: &str,
    hierarchical: i64,
    filters: &PublicFilters,
) -> ApiResult<()> {
    if !matches!(kind, "category" | "tag") {
        // C3 introduces custom taxonomy archives without inheriting another
        // taxonomy's query language. Its own root term is the only generic filter;
        // rejecting extra recognized filters avoids silently dropping constraints.
        if filters.category_id.is_some()
            || filters.tag_id.is_some()
            || filters.taxonomy_key.is_some()
            || filters.term_id.is_some()
            || (filters.descendants && hierarchical == 0)
        {
            return Err(crate::error::ApiError::bad_request(
                "Bộ lọc trang taxonomy tùy chỉnh không hợp lệ.",
            ));
        }
        return Ok(());
    }
    for (filter_id, filter_kind) in [
        (filters.category_id, TaxonomyKind::Category),
        (filters.tag_id, TaxonomyKind::Tag),
    ] {
        if let Some(filter_id) = filter_id {
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM taxonomy_terms t JOIN taxonomy_definitions d ON d.key=t.kind
                 WHERE t.id=? AND t.kind=? AND d.public=1
                   AND d.provider_status='active' AND d.declaration_state='declared'",
            )
            .bind(filter_id)
            .bind(filter_kind.as_str())
            .fetch_optional(&mut **tx)
            .await?;
            if exists.is_none() {
                return Err(crate::error::ApiError::bad_request(
                    "Bộ lọc taxonomy không hợp lệ.",
                ));
            }
        }
    }
    if filters.taxonomy_key.is_some() != filters.term_id.is_some()
        || (filters.taxonomy_key.is_some()
            && (filters.category_id.is_some() || filters.tag_id.is_some()))
    {
        return Err(crate::error::ApiError::bad_request(
            "Generic taxonomy filter thiếu cặp hoặc trộn aliases.",
        ));
    }
    if let (Some(key), Some(id)) = (&filters.taxonomy_key, filters.term_id) {
        let record: Option<(i64, i64)> = sqlx::query_as(
            "SELECT d.hierarchical,d.public FROM taxonomy_terms t
             JOIN taxonomy_definitions d ON d.key=t.kind
             WHERE t.id=? AND t.kind=? AND d.provider_status='active'
               AND d.declaration_state='declared'",
        )
        .bind(id)
        .bind(key)
        .fetch_optional(&mut **tx)
        .await?;
        let Some((generic_hierarchical, public)) = record else {
            return Err(crate::error::ApiError::bad_request(
                "Generic taxonomy filter không hợp lệ.",
            ));
        };
        if public == 0 || (filters.descendants && generic_hierarchical == 0) {
            return Err(crate::error::ApiError::bad_request(
                "Taxonomy private hoặc không phân cấp.",
            ));
        }
    }
    Ok(())
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
    let row: Option<(i64, String, String, String, i64)> = sqlx::query_as(
        "SELECT t.id, t.name, t.slug, t.locale, d.hierarchical
           FROM taxonomy_terms t
           JOIN site_locales sl ON sl.code = t.locale AND sl.is_default = 1
           JOIN taxonomy_definitions d ON d.key = t.kind
          WHERE t.kind = ? AND t.slug = ? AND d.public = 1
            AND d.provider_status = 'active' AND d.declaration_state = 'declared'",
    )
    .bind(kind.as_str())
    .bind(slug)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, name, slug, term_locale, hierarchical)) = row else {
        return Ok(None);
    };
    validate_taxonomy_page_filters_in_tx(&mut tx, kind.as_str(), hierarchical, &filters).await?;
    let page = list_taxonomy_published_in_tx(
        &mut tx,
        kind.as_str(),
        id,
        &term_locale,
        filters,
        limit,
        offset,
    )
    .await?;
    tx.commit().await?;
    Ok(Some((PublicTerm { id, name, slug }, page)))
}

/// Resolve a term archive by its stable ID, including site-defined taxonomies
/// across any enabled locale. Never re-queries by slug to avoid cross-locale aliasing.
pub async fn taxonomy_public_page_by_id(
    db: &SqlitePool,
    term_id: i64,
    filters: PublicFilters,
    limit: i64,
    offset: i64,
) -> ApiResult<Option<(String, PublicTerm, TaxonomyPublishedPage)>> {
    let mut tx = db.begin().await?;
    let row: Option<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT t.kind, t.name, t.slug, t.locale, d.hierarchical
           FROM taxonomy_terms t
           JOIN site_locales sl ON sl.code = t.locale AND sl.enabled = 1
           JOIN taxonomy_definitions d ON d.key = t.kind
          WHERE t.id = ? AND d.public = 1 AND d.provider_status = 'active'
            AND d.declaration_state = 'declared'",
    )
    .bind(term_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((kind, name, slug, term_locale, hierarchical)) = row else {
        return Ok(None);
    };
    validate_taxonomy_page_filters_in_tx(&mut tx, &kind, hierarchical, &filters).await?;
    let page = list_taxonomy_published_in_tx(
        &mut tx,
        &kind,
        term_id,
        &term_locale,
        filters,
        limit,
        offset,
    )
    .await?;
    tx.commit().await?;
    Ok(Some((
        kind,
        PublicTerm {
            id: term_id,
            name,
            slug,
        },
        page,
    )))
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
    kind: &str,
    term_id: i64,
    term_locale: &str,
    filters: PublicFilters,
    limit: i64,
    offset: i64,
) -> ApiResult<TaxonomyPublishedPage> {
    let root_category = if kind == "category" {
        Some(term_id)
    } else {
        filters.category_id
    };
    let root_tag = if kind == "tag" {
        Some(term_id)
    } else {
        filters.tag_id
    };
    let extra_category = (kind == "category")
        .then_some(filters.category_id)
        .flatten();
    let extra_tag = (kind == "tag").then_some(filters.tag_id).flatten();
    let (generic_term_id, generic_kind) = if kind == "category" || kind == "tag" {
        (filters.term_id, filters.taxonomy_key.clone())
    } else {
        (Some(term_id), Some(kind.to_owned()))
    };
    // Every input below is a bind parameter. The statement is fixed source.
    const FILTERED: &str = "WITH RECURSIVE category_scope(id, depth) AS (
            SELECT id, 1 FROM taxonomy_terms WHERE id = ? AND kind = 'category'
            UNION ALL
            SELECT child.id, category_scope.depth + 1
              FROM taxonomy_terms child JOIN category_scope ON child.parent_id = category_scope.id
             WHERE category_scope.depth < 8 AND ? = 1
        ), generic_scope(id, depth) AS (
            SELECT id, 1 FROM taxonomy_terms WHERE id = ? AND kind = ?
            UNION ALL
            SELECT child.id, generic_scope.depth + 1 FROM taxonomy_terms child
            JOIN generic_scope ON child.parent_id=generic_scope.id
            WHERE generic_scope.depth < 8 AND ? = 1
        )
        SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
               e.publication_status, e.version, e.created_at, e.updated_at
          FROM content_entries e
          JOIN content_types t ON t.key = e.content_type_key
          LEFT JOIN content_type_taxonomy_overrides override
            ON override.content_type_key=e.content_type_key
         WHERE e.publication_status = 'published'
           AND e.locale = ?
           AND e.content_type_key != 'pattern'
           AND t.provider_status = 'active'
           AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
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
           ))
           AND (? IS NULL OR EXISTS (
                SELECT 1 FROM entry_terms et WHERE et.entry_id=e.id
                  AND et.term_id IN (SELECT id FROM generic_scope)
           ))
           AND NOT EXISTS (
                SELECT 1 FROM json_each(json_array(
                    ?, CASE WHEN ? IS NOT NULL THEN 'category' END,
                    CASE WHEN ? IS NOT NULL THEN 'tag' END, ?
                )) requested
                WHERE requested.value IS NOT NULL AND NOT (
                    (override.content_type_key IS NOT NULL AND EXISTS (
                        SELECT 1 FROM json_each(override.taxonomies_json) selected
                        WHERE selected.value=requested.value
                    )) OR (override.content_type_key IS NULL AND EXISTS (
                        SELECT 1 FROM content_type_taxonomy_eligibility eligible
                        WHERE eligible.content_type_key=e.content_type_key
                          AND eligible.taxonomy_kind=requested.value
                          AND eligible.provider_status='active'
                          AND eligible.declaration_state='declared'
                    ))
                )
           )";
    let count_sql = format!("SELECT COUNT(*) FROM ({FILTERED}) AS published");
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
        .bind(root_category)
        .bind(i64::from(filters.descendants))
        .bind(generic_term_id)
        .bind(&generic_kind)
        .bind(i64::from(filters.descendants))
        .bind(term_locale)
        .bind(root_category)
        .bind(root_tag)
        .bind(root_tag)
        .bind(extra_category)
        .bind(extra_category)
        .bind(extra_tag)
        .bind(extra_tag)
        .bind(generic_term_id)
        .bind(kind) // Root archive taxonomy is always required.
        .bind(filters.category_id)
        .bind(filters.tag_id)
        .bind(&filters.taxonomy_key)
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
        .bind(generic_term_id)
        .bind(&generic_kind)
        .bind(i64::from(filters.descendants))
        .bind(term_locale)
        .bind(root_category)
        .bind(root_tag)
        .bind(root_tag)
        .bind(extra_category)
        .bind(extra_category)
        .bind(extra_tag)
        .bind(extra_tag)
        .bind(generic_term_id)
        .bind(kind) // Root archive taxonomy is always required.
        .bind(filters.category_id)
        .bind(filters.tag_id)
        .bind(&filters.taxonomy_key)
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
           JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1
           JOIN content_types AS t ON t.key = e.content_type_key
           JOIN json_each(t.definition_json, '$.fields') AS f
           WHERE e.publication_status = 'published' AND e.content_type_key != 'pattern'
             AND t.provider_status = 'active'
             AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
             AND COALESCE(json_extract(f.value, '$.public_exposure'), 'public') = 'public'
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
           SELECT 1
           FROM content_entries AS e
           JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1
           JOIN content_types AS t ON t.key = e.content_type_key
           JOIN content_field_group_targets AS gt
             ON gt.type_key = e.content_type_key
            AND gt.association_state IN ('declared', 'withdrawn')
           JOIN content_field_groups AS g
             ON g.group_id = gt.group_id
            AND g.status IN ('active', 'provider_inactive', 'withdrawn')
           JOIN json_each(g.fields_json) AS f
           WHERE e.publication_status = 'published' AND e.content_type_key != 'pattern'
             AND t.provider_status = 'active'
             AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
             AND COALESCE(json_extract(f.value, '$.public_exposure'), 'private') = 'public'
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
         ) OR EXISTS (
           SELECT 1
           FROM seo_public_media_refs AS sm
           JOIN content_entries AS e ON sm.source_kind = 'entry_seo' AND e.id = sm.source_id
           JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1
           JOIN content_types AS t ON t.key = e.content_type_key
           WHERE sm.media_id = ?
             AND e.publication_status = 'published'
             AND t.provider_status = 'active'
             AND e.content_type_key != 'pattern'
             AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
         ) OR EXISTS (
           SELECT 1
           FROM site_home_seo AS h
           JOIN site_locales AS sl ON sl.code = h.locale
           WHERE h.og_image_id = ? AND sl.enabled = 1
         ) OR EXISTS (
           SELECT 1 FROM builder_homepage_video_refs AS bv
           JOIN builder_homepage AS b ON b.id = 1
           WHERE bv.media_id = ? AND b.document_json IS NOT NULL
         ) OR EXISTS (
           SELECT 1
           FROM template_override_media_refs AS tom
           JOIN template_overrides AS ov ON ov.id = tom.override_id
           WHERE tom.media_id = ?
             AND (
               ov.target_kind IN ('index', 'not_found', 'singular', 'part')
               OR (ov.target_kind = 'home' AND EXISTS (
                 SELECT 1 FROM site_reading_settings AS reading
                 JOIN site_locales AS sl ON sl.code = reading.locale AND sl.enabled = 1
                 JOIN content_entries AS e ON e.id = reading.homepage_page_id
                 WHERE e.content_type_key = 'page'
                   AND e.locale = reading.locale
                   AND e.publication_status = 'published'
               ))
               OR (ov.target_kind = 'page' AND EXISTS (
                 SELECT 1 FROM content_entries AS e
                 JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1
                 WHERE e.content_type_key = 'page' AND e.publication_status = 'published'
               ))
               OR (ov.target_kind = 'page_layout' AND EXISTS (
                 SELECT 1 FROM content_entries AS e
                 JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1
                 WHERE e.content_type_key = 'page' AND e.publication_status = 'published'
                   AND e.page_layout_key = json_extract(ov.target_json, '$.layout_key')
               ))
               OR (ov.target_kind = 'single_custom' AND EXISTS (
                 SELECT 1 FROM content_entries AS e
                 JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1
                 JOIN content_types AS t ON t.key = e.content_type_key
                 WHERE e.custom_template_key = json_extract(ov.target_json, '$.custom_key')
                   AND e.publication_status = 'published'
                   AND t.provider_status = 'active' AND t.key != 'pattern' AND t.key != 'page'
                   AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
               ))
               OR (ov.target_kind = 'single' AND EXISTS (
                 SELECT 1 FROM content_entries AS e
                 JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1
                 JOIN content_types AS t ON t.key = e.content_type_key
                 WHERE e.publication_status = 'published'
                   AND t.provider_status = 'active' AND t.key NOT IN ('pattern', 'page')
                   AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                   AND (json_extract(ov.target_json, '$.content_type') IS NULL
                     OR e.content_type_key = json_extract(ov.target_json, '$.content_type'))
                   AND (json_extract(ov.target_json, '$.entry_id') IS NULL
                     OR e.id = json_extract(ov.target_json, '$.entry_id'))
                   AND (json_extract(ov.target_json, '$.selector_slug') IS NULL
                     OR e.slug = json_extract(ov.target_json, '$.selector_slug'))
               ))
               OR (ov.target_kind = 'archive' AND EXISTS (
                 SELECT 1 FROM content_types AS t
                 WHERE t.provider_status = 'active' AND t.key != 'pattern'
                   AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                   AND COALESCE(json_extract(t.definition_json, '$.settings.has_archive'), 0) = 1
                   AND (json_extract(ov.target_json, '$.content_type') IS NULL
                     OR t.key = json_extract(ov.target_json, '$.content_type'))
               ))
               OR (ov.target_kind = 'taxonomy' AND EXISTS (
                 SELECT 1 FROM taxonomy_definitions AS d
                 LEFT JOIN taxonomy_terms AS term ON term.kind = d.key
                 LEFT JOIN site_locales AS sl ON sl.code = term.locale AND sl.enabled = 1
                 WHERE d.public = 1 AND d.provider_status = 'active'
                   AND d.declaration_state = 'declared'
                   AND (json_extract(ov.target_json, '$.taxonomy') IS NULL
                     OR d.key = json_extract(ov.target_json, '$.taxonomy'))
                   AND (json_extract(ov.target_json, '$.term_id') IS NULL
                     OR (term.id = json_extract(ov.target_json, '$.term_id') AND sl.code IS NOT NULL))
                   AND (json_extract(ov.target_json, '$.selector_slug') IS NULL
                     OR (term.slug = json_extract(ov.target_json, '$.selector_slug') AND sl.code IS NOT NULL))
               ))
             )
         )"#,
    )
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(id)
    .fetch_one(db)
    .await?;
    if result != 0 {
        return Ok(true);
    }

    // Builder + one-hop Pattern ACL projections are only candidates: a change
    // in Content Type / Field Group visibility can revoke body access without
    // rewriting old entry_pattern_refs (which intentionally preserve content).
    // Re-evaluate the effective schema at the public read boundary so stale
    // projections cannot serve a private body or an inactive provider.
    let builder_type_keys: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT e.content_type_key FROM content_entries AS e
         JOIN site_locales AS sl ON sl.code = e.locale AND sl.enabled = 1
         WHERE e.publication_status = 'published'
           AND e.content_type_key NOT IN ('pattern', 'section')
           AND (
             EXISTS (SELECT 1 FROM entry_builder_media_refs AS r
                     WHERE r.entry_id = e.id AND r.media_id = ?)
             OR EXISTS (
               SELECT 1 FROM entry_pattern_refs AS pref
               JOIN entry_builder_media_refs AS r ON r.entry_id = pref.target_pattern_id
               WHERE pref.source_entry_id = e.id AND r.media_id = ?
             )
           )",
    )
    .bind(id)
    .bind(id)
    .fetch_all(db)
    .await?;
    if builder_type_keys.is_empty() {
        return Ok(false);
    }
    let mut conn = db.acquire().await?;
    for type_key in builder_type_keys {
        if let Some(schema) =
            crate::storage::content::load_effective_content_schema_in_tx(&mut conn, &type_key)
                .await?
            && crate::storage::page::body_can_grant_media(&schema)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod m5_public_context_tests {
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

    #[tokio::test]
    async fn c2_archive_root_eligibility_gates_count_rows_and_cross_filters() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO taxonomy_definitions(key,label,owner_kind) VALUES('brand','Brand','user')",
        )
        .execute(&db)
        .await
        .unwrap();
        let mut term_ids = Vec::new();
        for (kind, slug) in [
            ("category", "c2-root-category"),
            ("tag", "c2-root-tag"),
            ("brand", "c2-extra-brand"),
        ] {
            let id = sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES(?, ?, ?, 'vi', 'tg-' || lower(hex(randomblob(8))))")
                .bind(kind)
                .bind(slug)
                .bind(slug)
                .execute(&db)
                .await
                .unwrap()
                .last_insert_rowid();
            term_ids.push(id);
        }
        sqlx::query("INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json) VALUES('post','[\"category\",\"tag\",\"brand\"]')")
            .execute(&db).await.unwrap();
        let mut published_ids = Vec::new();
        for index in 0..3 {
            published_ids.push(
                add_entry(
                    &db,
                    "post",
                    &format!("c2-p2-published-{index}"),
                    true,
                    &term_ids,
                )
                .await,
            );
        }
        add_entry(&db, "post", "c2-p2-draft", false, &term_ids).await;

        let generic_brand = PublicFilters {
            taxonomy_key: Some("brand".into()),
            term_id: Some(term_ids[2]),
            ..PublicFilters::default()
        };
        for (kind, slug) in [
            (TaxonomyKind::Category, "c2-root-category"),
            (TaxonomyKind::Tag, "c2-root-tag"),
        ] {
            for filters in [PublicFilters::default(), generic_brand.clone()] {
                let first = taxonomy_public_page(&db, kind, slug, filters.clone(), 2, 0)
                    .await
                    .unwrap()
                    .unwrap()
                    .1;
                let second = taxonomy_public_page(&db, kind, slug, filters.clone(), 2, 2)
                    .await
                    .unwrap()
                    .unwrap()
                    .1;
                assert_eq!(first.total, 3, "{kind:?} count before withdrawal");
                assert_eq!(second.total, 3, "{kind:?} count on second page");
                assert_eq!(
                    first.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
                    [published_ids[2], published_ids[1]]
                );
                assert_eq!(
                    second.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
                    [published_ids[0]]
                );
            }
        }

        // Retain all links while removing only the archive root's CPT association.
        // The extra generic brand remains eligible: it must never bypass the root.
        sqlx::query("UPDATE content_type_taxonomy_overrides SET taxonomies_json='[\"brand\"]' WHERE content_type_key='post'")
            .execute(&db).await.unwrap();
        for (kind, slug) in [
            (TaxonomyKind::Category, "c2-root-category"),
            (TaxonomyKind::Tag, "c2-root-tag"),
        ] {
            for filters in [PublicFilters::default(), generic_brand.clone()] {
                for offset in [0, 2] {
                    let page = taxonomy_public_page(&db, kind, slug, filters.clone(), 2, offset)
                        .await
                        .unwrap()
                        .unwrap()
                        .1;
                    assert_eq!(page.total, 0, "{kind:?} must not count withdrawn root");
                    assert!(
                        page.entries.is_empty(),
                        "{kind:?} must not page withdrawn root"
                    );
                }
            }
        }
        let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entry_terms")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(
            links, 12,
            "withdrawing eligibility must preserve term memberships"
        );

        // Without a site override, the provider declaration is authoritative.
        sqlx::query("DELETE FROM content_type_taxonomy_overrides WHERE content_type_key='post'")
            .execute(&db)
            .await
            .unwrap();
        let provider_rows = sqlx::query("UPDATE content_type_taxonomy_eligibility SET provider_status='provider_inactive' WHERE content_type_key='post' AND taxonomy_kind IN ('category','tag')")
            .execute(&db).await.unwrap().rows_affected();
        assert_eq!(provider_rows, 2);
        for (kind, slug) in [
            (TaxonomyKind::Category, "c2-root-category"),
            (TaxonomyKind::Tag, "c2-root-tag"),
        ] {
            let page = taxonomy_public_page(&db, kind, slug, PublicFilters::default(), 2, 0)
                .await
                .unwrap()
                .unwrap()
                .1;
            assert_eq!(
                page.total, 0,
                "{kind:?} provider-inactive must exclude root"
            );
            assert!(page.entries.is_empty());
        }
        sqlx::query("UPDATE content_type_taxonomy_eligibility SET provider_status='active', declaration_state='withdrawn' WHERE content_type_key='post' AND taxonomy_kind IN ('category','tag')")
            .execute(&db).await.unwrap();
        for (kind, slug) in [
            (TaxonomyKind::Category, "c2-root-category"),
            (TaxonomyKind::Tag, "c2-root-tag"),
        ] {
            let page = taxonomy_public_page(&db, kind, slug, PublicFilters::default(), 2, 0)
                .await
                .unwrap()
                .unwrap()
                .1;
            assert_eq!(
                page.total, 0,
                "{kind:?} withdrawn declaration must exclude root"
            );
            assert!(page.entries.is_empty());
        }
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
        // C2: these types explicitly opt into term association; Page alone
        // remains unassociated under the Core default migration.
        sqlx::query(
            "INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json)
                     VALUES('page','[\"category\",\"tag\"]'),('news','[\"category\",\"tag\"]')",
        )
        .execute(&db)
        .await
        .unwrap();

        let root = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Sea','sea','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let child = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,parent_id,locale,translation_group_id)
             VALUES('category','Reefs','reefs',?,'vi','tg-' || lower(hex(randomblob(8))))",
        )
        .bind(root)
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let tag =
            sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Fish','fish','vi','tg-' || lower(hex(randomblob(8))))")
                .execute(&db)
                .await
                .unwrap()
                .last_insert_rowid();
        let other_tag =
            sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Other','other','vi','tg-' || lower(hex(randomblob(8))))")
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
        synchronize_direct_sql_fixture(&db).await;
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
        synchronize_direct_sql_fixture(&db).await;
        assert!(get_by_slug(&db, "post", "post-a").await.unwrap().is_none());
        assert!(get_by_slug(&db, "post", "draft-a").await.unwrap().is_none());
        assert_eq!(
            canonical_entry_by_id(&db, post).await.unwrap().as_deref(),
            Some("/new-post-a")
        );
        assert_eq!(canonical_entry_by_id(&db, draft).await.unwrap(), None);
        sqlx::query("UPDATE taxonomy_terms SET slug='sea-new' WHERE id=?")
            .bind(root)
            .execute(&db)
            .await
            .unwrap();
        synchronize_direct_sql_fixture(&db).await;
        assert_eq!(
            canonical_term_by_id(&db, root).await.unwrap().as_deref(),
            Some("/category/sea-new")
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
            sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','A','a','vi','tg-' || lower(hex(randomblob(8))))")
                .execute(&db)
                .await
                .unwrap()
                .last_insert_rowid();
        let child = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,parent_id,locale,translation_group_id) VALUES('category','B','b',?,'vi','tg-' || lower(hex(randomblob(8))))",
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
        let first = taxonomy_public_page(&db, TaxonomyKind::Category, "a", flags.clone(), 20, 0)
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

    #[tokio::test]
    async fn c1_visibility_hides_single_archive_taxonomy_media_seo_and_sitemap() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        sqlx::query(
            r#"INSERT INTO content_types(key,definition_json,owner_kind)
               VALUES('news', '{"key":"news","label":"News","fields":[{"key":"hero","label":"Hero","type":"image"}],"settings":{"public":false,"has_archive":false,"publication":"draft_publish","supports":["editor","featured_image"]}}', 'user')"#,
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json)
                     VALUES('news','[\"tag\"]')",
        )
        .execute(&db)
        .await
        .unwrap();
        let hero_id = "a".repeat(64);
        let seo_image_id = "b".repeat(64);
        for id in [&hero_id, &seo_image_id] {
            sqlx::query(
                "INSERT INTO media_assets(id,kind,filename,mime_type,extension,byte_size,width,height,thumb_width,thumb_height)
                 VALUES(?, 'image', 'public-visibility.png', 'image/png', 'png', 128, 10, 10, 10, 10)",
            )
            .bind(id)
            .execute(&db)
            .await
            .unwrap();
        }
        let term_id = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Visibility','visibility','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let entry_id = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
             VALUES('news','hidden-news','Hidden', json_object('hero', ?), 'published', 'vi', 'eg-' || lower(hex(randomblob(8))))",
        )
        .bind(&hero_id)
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES(?,?)")
            .bind(entry_id)
            .bind(term_id)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO entry_public_paths(entry_id,locale,path) VALUES(?, 'vi', '/hidden-news')",
        )
        .bind(entry_id)
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO seo_public_media_refs(source_kind,source_id,media_id)
             VALUES('entry_seo', ?, ?)",
        )
        .bind(entry_id)
        .bind(&seo_image_id)
        .execute(&db)
        .await
        .unwrap();
        synchronize_direct_sql_fixture(&db).await;

        assert!(
            get_by_slug(&db, "news", "hidden-news")
                .await
                .unwrap()
                .is_none()
        );
        assert!(get_published_by_id(&db, entry_id).await.unwrap().is_none());
        assert_eq!(
            list_published(&db, "news", 10, 0, EntryFilters::default())
                .await
                .unwrap_err()
                .0,
            StatusCode::NOT_FOUND
        );
        let page = taxonomy_public_page(
            &db,
            TaxonomyKind::Tag,
            "visibility",
            PublicFilters::default(),
            10,
            0,
        )
        .await
        .unwrap()
        .unwrap()
        .1;
        assert_eq!(page.total, 0);
        assert!(page.entries.is_empty());
        assert!(!media_is_referenced(&db, &hero_id).await.unwrap());
        assert!(!media_is_referenced(&db, &seo_image_id).await.unwrap());
        assert_eq!(canonical_entry_by_id(&db, entry_id).await.unwrap(), None);
        assert!(matches!(
            crate::storage::seo::resolve_custom_or_redirect_path(&db, "/hidden-news")
                .await
                .unwrap(),
            crate::storage::seo::PublicRouteResolution::NotFound
        ));
        let mut conn = db.acquire().await.unwrap();
        let routes = crate::storage::sitemap::list_indexable_routes_in_tx(
            &mut conn,
            "https://example.com",
            0,
            100,
        )
        .await
        .unwrap();
        assert!(
            !routes
                .routes
                .iter()
                .any(|route| { route.path.contains("news") || route.path.contains("visibility") })
        );
        drop(conn);

        // Toggling back to public must restore singles, taxonomy and media,
        // but the separately disabled archive must remain 404.
        sqlx::query(
            "UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.public', json('true')) WHERE key='news'",
        )
        .execute(&db)
        .await
        .unwrap();
        synchronize_direct_sql_fixture(&db).await;
        assert!(
            get_by_slug(&db, "news", "hidden-news")
                .await
                .unwrap()
                .is_some()
        );
        assert!(get_published_by_id(&db, entry_id).await.unwrap().is_some());
        assert_eq!(
            list_published(&db, "news", 10, 0, EntryFilters::default())
                .await
                .unwrap_err()
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            taxonomy_public_page(
                &db,
                TaxonomyKind::Tag,
                "visibility",
                PublicFilters::default(),
                10,
                0
            )
            .await
            .unwrap()
            .unwrap()
            .1
            .total,
            1
        );
        assert!(media_is_referenced(&db, &hero_id).await.unwrap());
        assert!(media_is_referenced(&db, &seo_image_id).await.unwrap());
        assert_eq!(
            canonical_entry_by_id(&db, entry_id)
                .await
                .unwrap()
                .as_deref(),
            Some("/hidden-news")
        );
        let mut conn = db.acquire().await.unwrap();
        let routes = crate::storage::sitemap::list_indexable_routes_in_tx(
            &mut conn,
            "https://example.com",
            0,
            100,
        )
        .await
        .unwrap();
        assert!(
            routes
                .routes
                .iter()
                .any(|route| route.path == "/hidden-news")
        );
        assert!(!routes.routes.iter().any(|route| route.path == "/news"));
        drop(conn);

        sqlx::query(
            "UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.has_archive', json('true')) WHERE key='news'",
        )
        .execute(&db)
        .await
        .unwrap();
        assert_eq!(
            list_published(&db, "news", 10, 0, EntryFilters::default())
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            get_by_slug(&db, "pattern", "hidden-news")
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            list_published(&db, "pattern", 10, 0, EntryFilters::default())
                .await
                .unwrap_err()
                .0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn s01_generic_builder_body_and_media_follow_capability_and_publication_without_mode_gate()
     {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        sqlx::query(
            r#"INSERT INTO content_types(key, definition_json, owner_kind)
                VALUES('project', '{"key":"project","label":"Project","fields":[],"settings":{"public":true,"has_archive":true,"publication":"draft_publish","supports":["editor","builder"]}}', 'user')"#,
        )
        .execute(&db)
        .await
        .unwrap();
        let media_id = "7".repeat(64);
        let document: opendash_core::builder::BuilderDocument = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "sections": [{"id":"section", "children":[{"id":"row", "children":[
                {"id":"column", "children":[
                    {"id":"heading", "type":"core/heading",
                        "props":{"level":2,"text":"S01 generic project body"}},
                    {"id":"route-title", "type":"core/heading",
                        "props":{"level":3,"text":"Unresolved title",
                            "binding":{"source":{"kind":"route_context","field_key":"title"},"target":"text"}}},
                    {"id":"picture", "type":"core/image",
                        "props":{"media_id":media_id,"alt":"Project picture"}}
                ]}
            ]}]}]
        }))
        .unwrap();
        let blocks =
            opendash_core::builder_blocks::serialize_builder_document_to_blocks(&document).unwrap();
        let values_json = serde_json::json!({ "body": blocks }).to_string();
        let entry_id = sqlx::query(
            "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, content_mode, locale, translation_group_id)
             VALUES('project', 's01-builder', 'Builder project', ?, 'published', 'builder', 'vi', 'eg-' || lower(hex(randomblob(8))))",
        )
        .bind(&values_json)
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO media_assets(id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
             VALUES(?, 'image', 'builder-test.png', 'image/png', 'png', 128, 10, 10, 10, 10)",
        )
        .bind(&media_id)
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO entry_builder_media_refs(entry_id, media_id, ref_role) VALUES(?, ?, 'image')",
        )
        .bind(entry_id)
        .bind(&media_id)
        .execute(&db)
        .await
        .unwrap();

        let rendered = load_entry_render_snapshot(&db, entry_id, None)
            .await
            .unwrap()
            .unwrap();
        let body = rendered.body.unwrap().html;
        assert!(body.contains("S01 generic project body"));
        assert!(body.contains("Builder project"));
        assert!(!body.contains("Unresolved title"));
        assert!(media_is_referenced(&db, &media_id).await.unwrap());

        // Unified `values_json["body"]` renders Builder blocks & grants public media ACL
        // regardless of `content_mode` preference (`editor` vs `builder`).
        sqlx::query("UPDATE content_entries SET content_mode = 'editor' WHERE id = ?")
            .bind(entry_id)
            .execute(&db)
            .await
            .unwrap();
        let rendered_in_editor_mode = load_entry_render_snapshot(&db, entry_id, None)
            .await
            .unwrap()
            .unwrap();
        assert!(
            rendered_in_editor_mode
                .body
                .unwrap()
                .html
                .contains("S01 generic project body")
        );
        assert!(media_is_referenced(&db, &media_id).await.unwrap());

        // Disabling ContentSupport::Builder revokes Builder rendering and public media grant
        // immediately while keeping `values_json["body"]` intact.
        sqlx::query(
            "UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.supports', json('[\"editor\"]')) WHERE key = 'project'",
        )
        .execute(&db)
        .await
        .unwrap();
        assert!(
            load_entry_render_snapshot(&db, entry_id, None)
                .await
                .unwrap()
                .unwrap()
                .body
                .is_none()
        );
        assert!(!media_is_referenced(&db, &media_id).await.unwrap());
        let saved_values: String =
            sqlx::query_scalar("SELECT values_json FROM content_entries WHERE id = ?")
                .bind(entry_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&saved_values).unwrap()["body"],
            blocks
        );

        sqlx::query("UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.supports', json('[\"editor\",\"builder\"]')) WHERE key = 'project'")
            .execute(&db).await.unwrap();
        assert!(media_is_referenced(&db, &media_id).await.unwrap());
        sqlx::query("UPDATE content_entries SET publication_status = 'draft' WHERE id = ?")
            .bind(entry_id)
            .execute(&db)
            .await
            .unwrap();
        assert!(
            load_entry_render_snapshot(&db, entry_id, None)
                .await
                .unwrap()
                .is_none()
        );
        assert!(!media_is_referenced(&db, &media_id).await.unwrap());
    }

    #[tokio::test]
    async fn s01_one_hop_pattern_acl_and_private_cpt_follow_unified_body_without_content_mode() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        sqlx::query(
            r#"INSERT INTO content_types(key, definition_json, owner_kind)
                VALUES('service', '{"key":"service","label":"Service","fields":[],"settings":{"public":true,"has_archive":true,"publication":"draft_publish","supports":["editor","builder"]}}', 'user')"#,
        )
        .execute(&db)
        .await
        .unwrap();

        let media_id = "9".repeat(64);
        sqlx::query(
            "INSERT INTO media_assets(id, kind, filename, mime_type, extension, byte_size, width, height, thumb_width, thumb_height)
             VALUES(?, 'image', 'pattern-img.png', 'image/png', 'png', 128, 10, 10, 10, 10)",
        )
        .bind(&media_id)
        .execute(&db)
        .await
        .unwrap();

        let pattern_id = sqlx::query(
            "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, content_mode, locale, translation_group_id)
             VALUES('pattern', 'hero-pat', 'Hero Pattern', '{}', 'draft', 'builder', 'vi', 'eg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let pat_doc = serde_json::json!({
            "schema_version": 1,
            "sections": [{"id":"s1","children":[{"id":"r1","children":[{"id":"c1","children":[
                {"id":"img1","type":"core/image","props":{"media_id":media_id,"alt":"Pattern photo"}}
            ]}]}]}]
        });
        sqlx::query(
            "INSERT INTO entry_builder_documents(entry_id, document_json, updated_at) VALUES(?, ?, datetime('now'))",
        )
        .bind(pattern_id)
        .bind(pat_doc.to_string())
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO entry_builder_media_refs(entry_id, media_id, ref_role) VALUES(?, ?, 'image')",
        )
        .bind(pattern_id)
        .bind(&media_id)
        .execute(&db)
        .await
        .unwrap();

        let blocks = format!(
            "<!-- od:section {{\"id\":\"sec-1\"}} -->\n\
             <!-- od:row {{\"id\":\"row-1\"}} -->\n\
             <!-- od:column {{\"id\":\"col-1\"}} -->\n\
             <!-- od:pattern_ref {{\"id\":\"pat-1\",\"pattern_id\":{pattern_id}}} /-->\n\
             <!-- /od:column -->\n\
             <!-- /od:row -->\n\
             <!-- /od:section -->"
        );
        let parent_id = sqlx::query(
            "INSERT INTO content_entries(content_type_key, slug, title, values_json, publication_status, content_mode, locale, translation_group_id)
             VALUES('service', 'svc-a', 'Service A', ?, 'draft', 'editor', 'vi', 'eg-' || lower(hex(randomblob(8))))",
        )
        .bind(serde_json::json!({ "body": blocks }).to_string())
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query(
            "INSERT INTO entry_pattern_refs(source_entry_id, target_pattern_id) VALUES(?, ?)",
        )
        .bind(parent_id)
        .bind(pattern_id)
        .execute(&db)
        .await
        .unwrap();

        // Draft parent keeps pattern media private.
        assert!(!media_is_referenced(&db, &media_id).await.unwrap());

        // Published public Builder parent unlocks 1-hop pattern media even with `content_mode = 'editor'`.
        sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = ?")
            .bind(parent_id)
            .execute(&db)
            .await
            .unwrap();
        assert!(media_is_referenced(&db, &media_id).await.unwrap());

        // Private CPT (`public = false`) hides 1-hop pattern media again.
        sqlx::query(
            "UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.public', json('false')) WHERE key = 'service'",
        )
        .execute(&db)
        .await
        .unwrap();
        assert!(!media_is_referenced(&db, &media_id).await.unwrap());
    }
}
