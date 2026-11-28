use super::*;

pub async fn list_entries(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
) -> ApiResult<Vec<ContentEntryRecord>> {
    list_entries_filtered(db, type_key, limit, offset, filters, false).await
}

/// Reusable published-only filtering contract for public archives and P06.
/// Draft exclusion runs in SQL before LIMIT/OFFSET.
pub async fn list_published_filtered(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
) -> ApiResult<Vec<ContentEntryRecord>> {
    if type_key == "pattern" {
        return Err(ApiError::not_found("Pattern là nội dung riêng tư."));
    }
    list_entries_filtered(db, type_key, limit, offset, filters, true).await
}

pub(in crate::storage) async fn list_entries_filtered(
    db: &SqlitePool,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
    published_only: bool,
) -> ApiResult<Vec<ContentEntryRecord>> {
    let mut tx = db.begin().await?;
    validate_filter_in_tx(&mut tx, type_key, &filters, published_only).await?;
    let entries = query_entries_filtered(
        &mut tx,
        type_key,
        limit,
        offset,
        filters,
        published_only,
        published_only,
        PublishedListSort::IdDesc,
    )
    .await?;
    tx.commit().await?;
    Ok(entries)
}

pub(in crate::storage) const FILTERED_ENTRIES_CTE_AND_FROM: &str =
    "WITH RECURSIVE category_scope(id, depth) AS (
     SELECT id, 1 FROM taxonomy_terms WHERE id = ? AND kind = 'category'
     UNION ALL
     SELECT child.id, category_scope.depth + 1
     FROM taxonomy_terms child JOIN category_scope ON child.parent_id = category_scope.id
     WHERE category_scope.depth < 8 AND ? = 1
 ), generic_scope(id, depth) AS (
     SELECT id, 1 FROM taxonomy_terms WHERE id = ? AND kind = ?
     UNION ALL
     SELECT child.id, generic_scope.depth+1 FROM taxonomy_terms child
     JOIN generic_scope ON child.parent_id=generic_scope.id
     WHERE generic_scope.depth < 8 AND ? = 1
 )";

pub(in crate::storage) const FILTERED_ENTRIES_WHERE: &str = "FROM content_entries e
   JOIN content_types t ON t.key = e.content_type_key
  WHERE e.content_type_key = ?
    AND e.trashed_at IS NULL
    AND (? = 0 OR (e.publication_status = 'published'
         AND e.content_type_key != 'pattern' AND t.provider_status = 'active'
         AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1))
    AND (? IS NULL OR EXISTS (
        SELECT 1 FROM entry_terms et
         WHERE et.entry_id = e.id AND et.term_id IN (SELECT id FROM category_scope)
    ))
    AND (? IS NULL OR EXISTS (
        SELECT 1 FROM entry_terms et WHERE et.entry_id = e.id AND et.term_id = ?
    ))
    AND (? IS NULL OR EXISTS (
        SELECT 1 FROM entry_terms et WHERE et.entry_id=e.id
          AND et.term_id IN (SELECT id FROM generic_scope)
    ))
    AND (? IS NULL OR e.locale = ?)
    AND EXISTS (SELECT 1 FROM site_locales sl WHERE sl.code = e.locale
        AND sl.deleted_at IS NULL
        AND (? = 0 OR (sl.enabled = 1 AND (sl.is_default = 1 OR
             (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1))))";

#[derive(sqlx::FromRow)]
pub(in crate::storage) struct FullEntryDbRow {
    pub(in crate::storage) id: i64,
    pub(in crate::storage) content_type_key: String,
    pub(in crate::storage) slug: String,
    pub(in crate::storage) title: String,
    pub(in crate::storage) values_json: String,
    pub(in crate::storage) publication_status: String,
    pub(in crate::storage) version: i64,
    pub(in crate::storage) created_at: String,
    pub(in crate::storage) updated_at: String,
    pub(in crate::storage) published_at: Option<String>,
    pub(in crate::storage) page_layout_key: Option<String>,
    pub(in crate::storage) custom_template_key: Option<String>,
    pub(in crate::storage) content_mode: String,
    pub(in crate::storage) primary_category_term_id: Option<i64>,
    pub(in crate::storage) parent_id: Option<i64>,
    pub(in crate::storage) menu_order: i32,
    pub(in crate::storage) locale: String,
    pub(in crate::storage) translation_group_id: String,
}

pub(crate) async fn count_entries_filtered_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    filters: &EntryFilters,
    published_only: bool,
    enforce_public_locale: bool,
) -> ApiResult<i64> {
    let statement =
        format!("{FILTERED_ENTRIES_CTE_AND_FROM} SELECT COUNT(*) {FILTERED_ENTRIES_WHERE}");
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(statement))
        .bind(filters.category_id)
        .bind(i64::from(filters.include_descendants))
        .bind(filters.term_id)
        .bind(&filters.taxonomy_key)
        .bind(i64::from(filters.include_descendants))
        .bind(type_key)
        .bind(i64::from(published_only))
        .bind(filters.category_id)
        .bind(filters.tag_id)
        .bind(filters.tag_id)
        .bind(filters.term_id)
        .bind(&filters.locale)
        .bind(&filters.locale)
        .bind(i64::from(enforce_public_locale))
        .fetch_one(&mut **tx)
        .await?;
    Ok(total)
}

pub(crate) async fn list_published_filtered_with_total_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
    sort: PublishedListSort,
) -> ApiResult<(Vec<ContentEntryRecord>, i64)> {
    if type_key == "pattern" {
        return Err(ApiError::not_found("Pattern là nội dung riêng tư."));
    }
    validate_filter_in_tx(tx, type_key, &filters, true).await?;
    let total = count_entries_filtered_in_tx(tx, type_key, &filters, true, true).await?;
    let entries =
        query_entries_filtered(tx, type_key, limit, offset, filters, true, true, sort).await?;
    Ok((entries, total))
}

/// Plugin host reads share the same SQL predicate and snapshot for rows and total.
pub(in crate::storage) async fn list_host_entries_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
    published_only: bool,
    enforce_public_locale: bool,
) -> ApiResult<(Vec<ContentEntryRecord>, i64)> {
    if published_only && type_key == "pattern" {
        return Err(ApiError::not_found("Pattern là nội dung riêng tư."));
    }
    validate_filter_in_tx(tx, type_key, &filters, published_only).await?;
    let total = count_entries_filtered_in_tx(
        tx,
        type_key,
        &filters,
        published_only,
        enforce_public_locale,
    )
    .await?;
    let rows = query_entries_filtered(
        tx,
        type_key,
        limit,
        offset,
        filters,
        published_only,
        enforce_public_locale,
        PublishedListSort::IdDesc,
    )
    .await?;
    Ok((rows, total))
}

/// Validates list query configuration, taxonomy visibility, and eligibility
/// before executing the paginated query. Distinguishes request errors (`Err`)
/// from missing/ineligible Builder configuration (`Ok(Err(missing))`).
pub(in crate::storage) async fn preflight_published_list_filters_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    filters: &EntryFilters,
) -> ApiResult<Result<(), PublishedListMissing>> {
    if type_key == "pattern" {
        return Ok(Err(PublishedListMissing::ContentType));
    }
    if filters.include_descendants
        && filters.category_id.is_none()
        && filters.taxonomy_key.is_none()
    {
        return Err(ApiError::bad_request(
            "include_descendants yêu cầu category_id hoặc generic taxonomy.",
        ));
    }
    // Invalid filter shapes remain a request error. A valid persisted pair
    // that points at a withdrawn/private/deleted taxonomy is instead a
    // missing Builder configuration, so the public homepage still renders.
    if filters.taxonomy_key.is_some() != filters.term_id.is_some()
        || (filters.taxonomy_key.is_some()
            && (filters.category_id.is_some() || filters.tag_id.is_some()))
        || filters.term_id.is_some_and(|id| id <= 0)
    {
        return Err(ApiError::bad_request("Bộ lọc taxonomy không hợp lệ."));
    }

    let type_exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM content_types WHERE key = ? AND provider_status = 'active'
         AND key != 'pattern' AND COALESCE(json_extract(definition_json, '$.settings.public'), 1) = 1",
    )
    .bind(type_key)
    .fetch_optional(&mut **tx)
    .await?;
    if type_exists.is_none() {
        return Ok(Err(PublishedListMissing::ContentType));
    }

    let effective_taxonomies =
        crate::storage::content::load_effective_taxonomies_in_tx(&mut **tx, type_key).await?;

    for (id, kind, missing) in [
        (
            filters.category_id,
            "category",
            PublishedListMissing::Category,
        ),
        (filters.tag_id, "tag", PublishedListMissing::Tag),
    ] {
        if let Some(id) = id {
            if id <= 0 {
                return Err(ApiError::bad_request("Bộ lọc taxonomy không hợp lệ."));
            }
            let actual: Option<String> = sqlx::query_scalar(
                "SELECT term.kind FROM taxonomy_terms term
                   JOIN taxonomy_definitions d ON d.key=term.kind
                   JOIN site_locales sl ON sl.code=term.locale AND sl.deleted_at IS NULL
                  WHERE term.id=? AND d.public=1 AND d.provider_status='active'
                    AND d.declaration_state='declared' AND sl.enabled=1
                    AND (sl.is_default=1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id=1)=1)",
            )
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?;
            if actual.as_deref() != Some(kind)
                || !effective_taxonomies.iter().any(|enabled| enabled == kind)
            {
                return Ok(Err(missing));
            }
        }
    }

    if let (Some(kind), Some(id)) = (&filters.taxonomy_key, filters.term_id) {
        let usable: Option<i64> = sqlx::query_scalar(
            "SELECT d.hierarchical FROM taxonomy_terms term
              JOIN taxonomy_definitions d ON d.key = term.kind
              JOIN site_locales sl ON sl.code=term.locale AND sl.deleted_at IS NULL
             WHERE term.id = ? AND term.kind = ? AND d.public = 1
               AND d.provider_status = 'active'
               AND d.declaration_state = 'declared' AND sl.enabled=1
               AND (sl.is_default=1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id=1)=1)",
        )
        .bind(id)
        .bind(kind)
        .fetch_optional(&mut **tx)
        .await?;
        if usable.is_none()
            || (filters.include_descendants && usable == Some(0))
            || !effective_taxonomies.iter().any(|enabled| enabled == kind)
        {
            return Ok(Err(PublishedListMissing::Taxonomy));
        }
    }

    validate_filter_in_tx(tx, type_key, filters, true).await?;
    Ok(Ok(()))
}

/// Public list query accepting `page_size` 1..=24 and `extra_offset` 0..=200
/// for Loop pagination and item-skip offsets while keeping legacy callers at 1..=20.
pub async fn list_published_page_with_offset_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    page: i64,
    page_size: i64,
    extra_offset: i64,
    sort: PublishedListSort,
    filters: EntryFilters,
) -> ApiResult<PublishedListResult> {
    if !(1..=100).contains(&page) || !(1..=24).contains(&page_size) {
        return Err(ApiError::bad_request(
            "Trang danh sách phải từ 1–100 và page_size từ 1–24.",
        ));
    }
    if !(0..=200).contains(&extra_offset) {
        return Err(ApiError::bad_request(
            "extra_offset danh sách phải từ 0–200.",
        ));
    }
    if let Err(missing) = preflight_published_list_filters_in_tx(tx, type_key, &filters).await? {
        return Ok(PublishedListResult::MissingConfig(missing));
    }

    // One extra row tells the renderer whether to expose a next-page link.
    // Bounds above guarantee no integer overflow or unbounded fetch.
    let offset = (page - 1) * page_size + extra_offset;
    let mut entries = query_entries_filtered(
        tx,
        type_key,
        page_size + 1,
        offset,
        filters,
        true,
        true,
        sort,
    )
    .await?;
    // ADR-0011 stops pagination at page 100 even if more records exist.
    let has_next = page < 100 && entries.len() > page_size as usize;
    entries.truncate(page_size as usize);
    Ok(PublishedListResult::Page(PublishedListPage {
        entries,
        has_next,
    }))
}
