use super::*;

/// Validate comment-block media using the current unified provider catalog.
/// Only active, declared schemas contribute public media references; inactive
/// provider values are preserved in the entry body for the delete-pin scanner.
pub(crate) async fn validated_entry_block_refs_in_conn(
    conn: &mut sqlx::SqliteConnection,
    raw_body: &str,
) -> ApiResult<opendash_core::builder_blocks::ExtractedBlockRefs> {
    let catalog = crate::storage::page::load_unified_block_catalog_in_conn(conn).await?;
    let mut refs =
        opendash_core::builder_blocks::extract_block_refs_lenient_with_schemas(raw_body, &catalog);
    for (kind, ids) in [
        ("image", &refs.image_ids),
        ("image", &refs.poster_image_ids),
        ("video", &refs.video_ids),
    ] {
        for id in ids {
            if !crate::storage::media::valid_media_id(id) {
                return Err(ApiError::unprocessable(
                    "ID media trong comment block không hợp lệ.",
                ));
            }
            let exists: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = ? AND kind = ? AND lifecycle_state = 'active')",
            )
            .bind(id)
            .bind(kind)
            .fetch_one(&mut *conn)
            .await?;
            if exists == 0 {
                return Err(ApiError::unprocessable(
                    "Media tham chiếu trong comment block không tồn tại hoặc không đúng loại.",
                ));
            }
        }
    }
    for id in std::mem::take(&mut refs.any_media_ids)
        .into_iter()
        .chain(std::mem::take(&mut refs.file_ids))
    {
        if !crate::storage::media::valid_media_id(&id) {
            return Err(ApiError::unprocessable(
                "ID media trong comment block không hợp lệ.",
            ));
        }
        let kind: Option<String> = sqlx::query_scalar(
            "SELECT kind FROM media_assets WHERE id = ? AND lifecycle_state = 'active'",
        )
        .bind(&id)
        .fetch_optional(&mut *conn)
        .await?;
        match kind.as_deref() {
            Some("image") => {
                refs.image_ids.insert(id);
            }
            Some("video") => {
                refs.video_ids.insert(id);
            }
            _ => {
                return Err(ApiError::unprocessable(
                    "Media tham chiếu trong comment block không tồn tại hoặc không đúng loại.",
                ));
            }
        }
    }
    Ok(refs)
}

pub(in crate::storage) async fn current_settings_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
) -> ApiResult<ContentTypeSettings> {
    let schema = crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    Ok(schema.base.descriptor.settings)
}

pub(in crate::storage) type EntryRow = (
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
);

/// Category/tag/locale query predicates. Missing values mean no restriction.
#[derive(Debug, Clone, Default)]
pub struct EntryFilters {
    pub category_id: Option<i64>,
    pub tag_id: Option<i64>,
    pub taxonomy_key: Option<String>,
    pub term_id: Option<i64>,
    pub include_descendants: bool,
    pub locale: Option<String>,
}

/// The only allowed SQL orderings for a published Builder list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PublishedListSort {
    #[default]
    IdDesc,
    IdAsc,
}

/// Missing references are a valid list configuration state, not a database failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishedListMissing {
    ContentType,
    Category,
    Tag,
    Taxonomy,
}

#[derive(Debug)]
pub struct PublishedListPage {
    pub entries: Vec<ContentEntryRecord>,
    pub has_next: bool,
}

#[derive(Debug)]
pub enum PublishedListResult {
    Page(PublishedListPage),
    MissingConfig(PublishedListMissing),
}

pub(in crate::storage) async fn read_term_ids(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
) -> ApiResult<Vec<i64>> {
    Ok(
        sqlx::query_scalar("SELECT term_id FROM entry_terms WHERE entry_id = ? ORDER BY term_id")
            .bind(id)
            .fetch_all(&mut **tx)
            .await?,
    )
}

pub(crate) async fn batch_load_term_ids_in_conn(
    conn: &mut sqlx::SqliteConnection,
    entry_ids: &[i64],
) -> ApiResult<BTreeMap<i64, Vec<i64>>> {
    let mut out = BTreeMap::<i64, Vec<i64>>::new();
    if entry_ids.is_empty() {
        return Ok(out);
    }
    for chunk in entry_ids.chunks(500) {
        let ids_json = serde_json::to_string(chunk)?;
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT et.entry_id, et.term_id
               FROM entry_terms et
              WHERE et.entry_id IN (SELECT value FROM json_each(?))
              ORDER BY et.entry_id ASC, et.term_id ASC",
        )
        .bind(ids_json)
        .fetch_all(&mut *conn)
        .await?;
        for (entry_id, term_id) in rows {
            out.entry(entry_id).or_default().push(term_id);
        }
    }
    Ok(out)
}

pub(in crate::storage) async fn begin_entry_write<'a>(
    db: &'a SqlitePool,
    actor: &SessionContext,
) -> ApiResult<Transaction<'a, Sqlite>> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let current: Option<(i64, String)> = sqlx::query_as(
        "SELECT u.id, u.role FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(&actor.token_hash)
    .bind(auth::current_time_secs())
    .fetch_optional(&mut *tx)
    .await?;
    match current {
        Some((id, role)) if id == actor.user.id && role == actor.user.role.as_str() => Ok(tx),
        Some(_) => Err(ApiError::forbidden(
            "Bạn không có quyền chỉnh sửa nội dung.",
        )),
        None => Err(ApiError::unauthorized("Phiên đăng nhập đã bị thu hồi.")),
    }
}

pub(in crate::storage) async fn resolve_entry_write_locale_in_tx(
    conn: &mut sqlx::SqliteConnection,
    requested: Option<&str>,
    existing_locale: Option<&str>,
) -> ApiResult<String> {
    let locale = match existing_locale {
        Some(cur) => {
            if let Some(raw) = requested {
                let norm = raw.trim().to_ascii_lowercase();
                if norm.is_empty() || norm != cur {
                    return Err(ApiError::bad_request(
                        "Không thể đổi ngôn ngữ của nội dung đã tạo.",
                    ));
                }
            }
            Ok(cur.to_owned())
        }
        None => crate::storage::settings::resolve_requested_locale_in_conn(conn, requested).await,
    }?;
    let active: Option<i64> = sqlx::query_scalar(
        "SELECT is_default FROM site_locales WHERE code = ? AND deleted_at IS NULL",
    )
    .bind(&locale)
    .fetch_optional(&mut *conn)
    .await?;
    let is_default = active.ok_or_else(|| ApiError::not_found("Không tìm thấy ngôn ngữ."))?;
    if is_default == 0 {
        crate::storage::settings::require_multilingual_enabled_in_conn(conn).await?;
    }
    Ok(locale)
}

pub async fn get_entry_locale_and_group(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
) -> ApiResult<Option<(String, String)>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT locale, translation_group_id FROM content_entries WHERE content_type_key = ? AND id = ? AND trashed_at IS NULL",
    )
    .bind(type_key)
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

pub(in crate::storage) async fn replace_term_ids(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
    ids: &[i64],
) -> ApiResult<()> {
    sqlx::query("DELETE FROM entry_terms WHERE entry_id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    for &term_id in ids {
        sqlx::query("INSERT INTO entry_terms (entry_id, term_id) VALUES (?, ?)")
            .bind(id)
            .bind(term_id)
            .execute(&mut **tx)
            .await?;
    }
    let current_primary: Option<i64> =
        sqlx::query_scalar("SELECT primary_category_term_id FROM content_entries WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?
            .flatten();
    if let Some(pid) = current_primary
        && !ids.contains(&pid)
    {
        sqlx::query("UPDATE content_entries SET primary_category_term_id = NULL WHERE id = ?")
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub(in crate::storage) async fn current_entry_in_write(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    id: i64,
    expected_version: i64,
) -> ApiResult<()> {
    crate::storage::entry_lifecycle::ensure_active_entry_version_in_conn(
        &mut **tx,
        type_key,
        id,
        expected_version,
    )
    .await?;
    Ok(())
}

pub async fn get_entry(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
) -> ApiResult<Option<ContentEntryRecord>> {
    let mut tx = db.begin().await?;
    let entry = get_entry_in_tx(&mut tx, type_key, id).await?;
    tx.commit().await?;
    Ok(entry)
}

/// Query one published-only Builder list page on a consistent SQLite read
/// snapshot. Missing CPT/terms are distinct from actual storage errors, so
/// the Builder can preserve configuration and display a diagnostic.
pub async fn list_published_page(
    db: &SqlitePool,
    type_key: &str,
    page: i64,
    page_size: i64,
    sort: PublishedListSort,
    filters: EntryFilters,
) -> ApiResult<PublishedListResult> {
    let mut tx = db.begin().await?;
    let result =
        list_published_page_in_tx(&mut tx, type_key, page, page_size, sort, filters).await?;
    tx.commit().await?;
    Ok(result)
}

/// #106: Use the same #105 predicate and limits within an existing Builder
/// read/write snapshot. Never issue a second independent query on Save.
pub async fn list_published_page_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    page: i64,
    page_size: i64,
    sort: PublishedListSort,
    filters: EntryFilters,
) -> ApiResult<PublishedListResult> {
    if type_key == "pattern" {
        return Ok(PublishedListResult::MissingConfig(
            PublishedListMissing::ContentType,
        ));
    }
    if !(1..=100).contains(&page) || !(1..=20).contains(&page_size) {
        return Err(ApiError::bad_request(
            "Trang danh sách phải từ 1–100 và page_size từ 1–20.",
        ));
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
        return Ok(PublishedListResult::MissingConfig(
            PublishedListMissing::ContentType,
        ));
    }

    // Persisted Builder list nodes are configuration, not user-provided
    // request filters. A site admin may withdraw a taxonomy association
    // without deleting the old node or its entry links; render an empty list
    // with the existing MissingConfig diagnostic rather than aborting the
    // entire public homepage with 400.
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
                return Ok(PublishedListResult::MissingConfig(missing));
            }
        }
    }

    if let (Some(kind), Some(id)) = (&filters.taxonomy_key, filters.term_id) {
        // The same consistent transaction includes association, definition
        // visibility and term kind. Do not expose drafts or private taxonomy
        // membership by probing an unavailable saved Builder node.
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
        // A saved list can outlive a legitimate hierarchical -> flat edit
        // (allowed once all terms are roots). Treat its now-incompatible
        // descendant filter as missing configuration, while request filters
        // still receive the strict 400 from validate_filter_in_tx below.
        if usable.is_none()
            || (filters.include_descendants && usable == Some(0))
            || !effective_taxonomies.iter().any(|enabled| enabled == kind)
        {
            return Ok(PublishedListResult::MissingConfig(
                PublishedListMissing::Taxonomy,
            ));
        }
    }

    validate_filter_in_tx(tx, type_key, &filters, true).await?;
    // One extra row tells the renderer whether to expose a next-page link.
    // Bounds above guarantee no integer overflow or unbounded fetch.
    let offset = (page - 1) * page_size;
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
