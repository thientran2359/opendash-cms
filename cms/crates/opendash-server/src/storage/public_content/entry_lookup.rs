//! Read-only public content lookup and media reference ACL.
//! Public controllers never issue SQL directly.

use super::*;

pub(super) use sqlx::{Sqlite, SqlitePool, Transaction};

pub(super) use crate::storage::entries::{self, ContentEntryRecord, EntryFilters, decode_row};

pub(super) use crate::error::ApiResult;

pub(super) use crate::public_context::{PublicFilters, TaxonomyKind};

pub(in crate::storage) type HydratedEntrySqlRow = (
    i64,
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    Option<String>,
    Option<i64>,
    String,
);

pub(in crate::storage) fn decode_hydrated_entry_rows(
    rows: Vec<HydratedEntrySqlRow>,
) -> ApiResult<Vec<ContentEntryRecord>> {
    let mut records = Vec::with_capacity(rows.len());
    for (
        id,
        content_type_key,
        slug,
        title,
        values_json,
        publication_status,
        version,
        created_at,
        updated_at,
        published_at,
        primary_category_term_id,
        locale,
    ) in rows
    {
        let mut record = decode_row((
            id,
            content_type_key,
            slug,
            title,
            values_json,
            publication_status,
            version,
            created_at,
            updated_at,
            published_at,
        ))?;
        record.primary_category_term_id = primary_category_term_id;
        record.locale = locale;
        records.push(record);
    }
    Ok(records)
}

/// Slug is unique per `(content_type_key, locale)`; defaults to the active
/// default locale when omitted. Never bypasses enabled locale visibility.
pub async fn get_by_slug(
    db: &SqlitePool,
    type_key: &str,
    slug: &str,
) -> ApiResult<Option<ContentEntryRecord>> {
    get_by_slug_for_locale(db, type_key, slug, None).await
}

pub async fn get_by_slug_for_locale(
    db: &SqlitePool,
    type_key: &str,
    slug: &str,
    locale: Option<&str>,
) -> ApiResult<Option<ContentEntryRecord>> {
    if type_key == "pattern" {
        return Ok(None);
    }
    let mut tx = db.begin().await?;
    let Some(active_locale) = resolve_public_locale_in_tx(&mut tx, locale).await? else {
        return Ok(None);
    };
    let row: Option<entries::EntryRow> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                e.publication_status, e.version, e.created_at, e.updated_at, e.published_at
         FROM content_entries e
         JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
         JOIN content_types t ON t.key = e.content_type_key
         WHERE e.content_type_key = ? AND e.locale = ? AND e.slug = ?
           AND e.trashed_at IS NULL
           AND e.publication_status = 'published'
           AND t.provider_status = 'active'
           AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
    )
    .bind(type_key)
    .bind(&active_locale)
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

pub(in crate::storage) async fn get_published_by_id_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
) -> ApiResult<Option<ContentEntryRecord>> {
    let row: Option<entries::EntryRow> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                e.publication_status, e.version, e.created_at, e.updated_at, e.published_at
         FROM content_entries e
         JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
         JOIN content_types t ON t.key = e.content_type_key
         WHERE e.id = ? AND e.trashed_at IS NULL AND e.publication_status = 'published'
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

pub async fn load_entry_render_snapshot_with_options(
    db: &SqlitePool,
    id: i64,
    themes_root: Option<&std::path::Path>,
    cache: &crate::page::BlockTemplateCache,
    list_base_path: &str,
    list_pages: &std::collections::BTreeMap<String, i64>,
) -> ApiResult<Option<PublicEntryRenderSnapshot>> {
    use crate::error::ApiError;
    use crate::storage::{builder, builder_binding, content, overrides, theme};
    use opendash_core::{
        content::{ContentSupport, FieldRole, PublicExposure},
        registry::{DeclarationState, ProviderStatus},
        theme::TemplateCandidateKind,
    };
    let mut tx = db.begin().await?;
    let Some(entry) = get_published_by_id_in_tx(&mut tx, id).await? else {
        return Ok(None);
    };
    let entry_locale = entry.locale.clone();
    let effective_schema =
        content::load_effective_content_schema_in_tx(&mut tx, &entry.content_type_key)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let eligible_public_taxonomies =
        load_eligible_public_taxonomies_in_conn(&mut tx, &entry.content_type_key).await?;
    let terms = batch_load_public_entry_terms_in_conn(
        &mut tx,
        &[(entry.id, entry.content_type_key.clone())],
    )
    .await?
    .remove(&entry.id)
    .unwrap_or_default();
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
    let body_allows_public_builder = effective_schema.base.status == ProviderStatus::Active
        && effective_schema.base.declaration_state == DeclarationState::Declared
        && effective_schema
            .effective_fields
            .get("body")
            .is_some_and(|fe| {
                fe.definition.field_type == opendash_core::field_schema::FieldType::Richtext
                    && fe.definition.effective_acl().2 == PublicExposure::Public
                    && fe.definition.effective_acl().0 == FieldRole::Editor
            });
    let mut render_budget = crate::page::RenderBudget::new();
    let raw_body = entry.values.get("body").and_then(serde_json::Value::as_str);
    let body = if !matches!(entry.content_type_key.as_str(), "pattern" | "section")
        && effective_schema
            .base
            .descriptor
            .settings
            .supports(ContentSupport::Builder)
        && body_allows_public_builder
        && raw_body.is_some_and(opendash_core::builder_blocks::contains_blocks)
    {
        let doc =
            opendash_core::builder_blocks::parse_blocks_lenient_or_recover(raw_body.unwrap_or(""))
                .document;
        let mut state = builder::read_render_context_for_locale_tx(
            &mut tx,
            Some(entry_locale.as_str()),
            false,
            Some(list_base_path),
        )
        .await?;
        builder::hydrate_render_context_for_documents_in_conn(&mut tx, &mut state, &[&doc], false)
            .await?;
        let bindings = builder_binding::resolve_in_tx_with_route_and_locale(
            &mut tx,
            &doc,
            false,
            Some((&entry.content_type_key, &entry.slug)),
            Some(entry_locale.as_str()),
        )
        .await?;
        let lists = builder::resolve_lists_in_tx_for_locale(
            &mut tx,
            &doc,
            list_pages,
            Some(entry_locale.as_str()),
        )
        .await?;
        let forms = builder::resolve_contact_forms_in_tx(&mut tx, &doc).await?;
        let patterns = builder::resolve_patterns_in_tx_with_budget(
            &mut tx,
            &mut state,
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
        effective_schema,
        eligible_public_taxonomies,
        terms,
        active,
        resolution,
        body,
        budget: render_budget,
    }))
}

/// Cross-CPT category/tag predicates are all applied in SQL *before* LIMIT.
/// EXISTS prevents duplicate entries when they have multiple matching links.
/// Publication filtering also governs COUNT, so drafts affect neither counts
/// nor page boundaries. Read COUNT and page under one SQLite snapshot.
pub(in crate::storage) async fn list_taxonomy_published_in_tx(
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
               e.publication_status, e.version, e.created_at, e.updated_at, e.published_at,
               e.primary_category_term_id, e.locale
          FROM content_entries e
          JOIN content_types t ON t.key = e.content_type_key
          LEFT JOIN content_type_taxonomy_overrides override
            ON override.content_type_key=e.content_type_key
         WHERE e.trashed_at IS NULL
           AND e.publication_status = 'published'
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
    let rows: Vec<HydratedEntrySqlRow> = sqlx::query_as(sqlx::AssertSqlSafe(page_sql))
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
    let entries = decode_hydrated_entry_rows(rows)?;
    Ok(TaxonomyPublishedPage { entries, total })
}

/// Fetches up to `limit` (clamped <= 20) published entries in the same CPT and locale
/// sharing at least one public, active, declared, CPT-eligible taxonomy term.
pub async fn query_related_entries(
    db: &SqlitePool,
    entry_id: i64,
    type_key: &str,
    limit: usize,
) -> ApiResult<Vec<crate::content_render::EntryView>> {
    let clamped = (limit.min(20)) as i64;
    if clamped == 0 {
        return Ok(Vec::new());
    }
    let mut tx = db.begin().await?;
    let Some(source_locale): Option<String> = sqlx::query_scalar(
        "SELECT locale FROM content_entries WHERE id = ? AND trashed_at IS NULL",
    )
    .bind(entry_id)
    .fetch_optional(&mut *tx)
    .await?
    else {
        tx.commit().await?;
        return Ok(Vec::new());
    };
    let rows: Vec<HydratedEntrySqlRow> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                e.publication_status, e.version, e.created_at, e.updated_at, e.published_at,
                e.primary_category_term_id, e.locale
           FROM content_entries e
           JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN content_types t ON t.key = e.content_type_key
           LEFT JOIN content_type_taxonomy_overrides override
             ON override.content_type_key = e.content_type_key
          WHERE e.content_type_key = ?
            AND e.locale = ?
            AND e.id != ?
            AND e.trashed_at IS NULL
            AND e.publication_status = 'published'
            AND e.content_type_key != 'pattern'
            AND t.provider_status = 'active'
            AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
            AND EXISTS (
                SELECT 1
                  FROM entry_terms et1
                  JOIN entry_terms et2 ON et2.term_id = et1.term_id
                  JOIN taxonomy_terms tt ON tt.id = et1.term_id
                  JOIN taxonomy_definitions d ON d.key = tt.kind
                 WHERE et1.entry_id = ?
                   AND et2.entry_id = e.id
                   AND tt.locale = e.locale
                   AND d.public = 1
                   AND d.provider_status = 'active'
                   AND d.declaration_state = 'declared'
                   AND (
                       (override.content_type_key IS NOT NULL AND EXISTS (
                           SELECT 1 FROM json_each(override.taxonomies_json) sel
                            WHERE sel.value = tt.kind
                       ))
                       OR
                       (override.content_type_key IS NULL AND EXISTS (
                           SELECT 1 FROM content_type_taxonomy_eligibility el
                            WHERE el.content_type_key = e.content_type_key
                              AND el.taxonomy_kind = tt.kind
                              AND el.provider_status = 'active'
                              AND el.declaration_state = 'declared'
                       ))
                   )
            )
          ORDER BY COALESCE(e.published_at, e.updated_at) DESC, e.id DESC
          LIMIT ?",
    )
    .bind(type_key)
    .bind(&source_locale)
    .bind(entry_id)
    .bind(entry_id)
    .bind(clamped)
    .fetch_all(&mut *tx)
    .await?;

    let records = decode_hydrated_entry_rows(rows)?;
    let out = hydrate_entry_views_in_conn(&mut tx, records).await?;
    tx.commit().await?;
    Ok(out)
}

/// Fetches up to `limit` (clamped <= 20) recent published `post` entries in the given
/// (or default) locale, optionally excluding `exclude_post_id`.
pub async fn query_recent_posts(
    db: &SqlitePool,
    exclude_post_id: Option<i64>,
    locale: Option<&str>,
    limit: usize,
) -> ApiResult<Vec<crate::content_render::EntryView>> {
    let clamped = (limit.min(20)) as i64;
    if clamped == 0 {
        return Ok(Vec::new());
    }
    let mut tx = db.begin().await?;
    let active_locale = match locale {
        Some(loc) => loc.to_owned(),
        None => crate::storage::settings::get_default_locale_code_in_conn(&mut tx).await?,
    };
    let rows: Vec<HydratedEntrySqlRow> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                e.publication_status, e.version, e.created_at, e.updated_at, e.published_at,
                e.primary_category_term_id, e.locale
           FROM content_entries e
           JOIN site_locales sl ON sl.code = e.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
           JOIN content_types t ON t.key = e.content_type_key
          WHERE e.content_type_key = 'post'
            AND e.locale = ?
            AND e.trashed_at IS NULL
            AND e.publication_status = 'published'
            AND (? IS NULL OR e.id != ?)
            AND t.provider_status = 'active'
            AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
          ORDER BY COALESCE(e.published_at, e.updated_at) DESC, e.id DESC
          LIMIT ?",
    )
    .bind(&active_locale)
    .bind(exclude_post_id)
    .bind(exclude_post_id)
    .bind(clamped)
    .fetch_all(&mut *tx)
    .await?;

    let records = decode_hydrated_entry_rows(rows)?;
    let out = hydrate_entry_views_in_conn(&mut tx, records).await?;
    tx.commit().await?;
    Ok(out)
}
