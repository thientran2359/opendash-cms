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

/// Validate all three supported filter forms in the same read snapshot as
/// the query. Inactive, private or unassociated terms cannot be used to probe
/// published entry existence; Admin may still manage private taxonomies.
pub(crate) async fn validate_filter_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    filters: &EntryFilters,
    published_only: bool,
) -> ApiResult<()> {
    if filters.taxonomy_key.is_some() != filters.term_id.is_some()
        || (filters.taxonomy_key.is_some()
            && (filters.category_id.is_some() || filters.tag_id.is_some()))
    {
        return Err(ApiError::bad_request(
            "Trộn hoặc thiếu cặp taxonomy_key + term_id.",
        ));
    }
    if filters.include_descendants
        && filters.category_id.is_none()
        && filters.taxonomy_key.is_none()
    {
        return Err(ApiError::bad_request(
            "include_descendants cần taxonomy filter.",
        ));
    }
    let allowed =
        crate::storage::content::load_effective_taxonomies_in_tx(&mut **tx, type_key).await?;
    for (id, kind) in [
        (filters.category_id, "category"),
        (filters.tag_id, "tag"),
        (
            filters.term_id,
            filters.taxonomy_key.as_deref().unwrap_or(""),
        ),
    ] {
        if let Some(id) = id {
            if id <= 0 {
                return Err(ApiError::bad_request("Bộ lọc taxonomy không hợp lệ."));
            }
            let actual: Option<(String, i64, i64)> = sqlx::query_as(
                "SELECT t.kind,d.public,d.hierarchical FROM taxonomy_terms t
                  JOIN taxonomy_definitions d ON d.key=t.kind
                  JOIN site_locales sl ON sl.code=t.locale AND sl.deleted_at IS NULL
                 WHERE t.id=? AND d.provider_status='active'
                   AND d.declaration_state='declared'
                   AND (?=0 OR (sl.enabled=1 AND (sl.is_default=1 OR
                        (SELECT multilingual_enabled FROM site_locales_state WHERE id=1)=1)))",
            )
            .bind(id)
            .bind(i64::from(published_only))
            .fetch_optional(&mut **tx)
            .await?;
            if !actual.as_ref().is_some_and(|(actual, _, _)| actual == kind) {
                return Err(ApiError::bad_request(
                    "Term trong bộ lọc không tồn tại hoặc sai taxonomy.",
                ));
            }
            let (_, is_public, hierarchical) = actual.expect("checked term");
            if !allowed.iter().any(|a| a == kind) || (published_only && is_public == 0) {
                return Err(ApiError::bad_request(
                    "Taxonomy không được dùng cho CPT này.",
                ));
            }
            if filters.include_descendants
                && filters.taxonomy_key.as_deref() == Some(kind)
                && hierarchical == 0
            {
                return Err(ApiError::bad_request(
                    "Taxonomy không phân cấp không nhận descendants.",
                ));
            }
        }
    }
    Ok(())
}

pub fn filter_entry_values_for_role(
    record: &mut ContentEntryRecord,
    effective: &EffectiveContentSchema,
    caller_role: FieldRole,
) {
    record.values.retain(|key, _| {
        effective
            .effective_fields
            .get(key)
            .map(|fe| caller_role >= fe.definition.min_view_role)
            .unwrap_or(true)
    });
}

pub(in crate::storage) async fn verify_schema_token_in_tx(
    tx: &mut sqlx::SqliteConnection,
    type_key: &str,
    expected_schema_token: &str,
) -> ApiResult<EffectiveContentSchema> {
    let Some(eff) =
        crate::storage::content::load_effective_content_schema_in_tx(tx, type_key).await?
    else {
        return Err(ApiError::not_found(format!(
            "Không tìm thấy loại nội dung '{type_key}'."
        )));
    };
    if eff.schema_token != expected_schema_token {
        return Err(ApiError::conflict(format!(
            "schema_version_conflict: schema_token hiện tại là '{}', không khớp '{}'.",
            eff.schema_token, expected_schema_token
        )));
    }
    Ok(eff)
}

pub(in crate::storage) fn normalize_entry_timestamp(
    raw: &str,
    field_name: &str,
) -> ApiResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 32 || !trimmed.is_ascii() {
        return Err(ApiError::unprocessable(format!(
            "Định dạng thời gian {field_name} không hợp lệ (dùng YYYY-MM-DD HH:MM:SS hoặc ISO-8601)."
        )));
    }
    let normalized = trimmed.trim_end_matches('Z').trim_end_matches('z');
    let normalized = normalized.split('.').next().unwrap_or(normalized);
    let (date_part, time_part) = normalized
        .split_once('T')
        .or_else(|| normalized.split_once('t'))
        .or_else(|| normalized.split_once(' '))
        .ok_or_else(|| {
            ApiError::unprocessable(format!(
                "Định dạng thời gian {field_name} không hợp lệ (thiếu ngày hoặc giờ)."
            ))
        })?;
    let mut date_it = date_part.split('-');
    let year: u32 = date_it
        .next()
        .and_then(|s| (s.len() == 4).then(|| s.parse().ok()).flatten())
        .ok_or_else(|| ApiError::unprocessable(format!("Năm trong {field_name} không hợp lệ.")))?;
    let month: u32 = date_it
        .next()
        .and_then(|s| (s.len() == 2).then(|| s.parse().ok()).flatten())
        .ok_or_else(|| {
            ApiError::unprocessable(format!("Tháng trong {field_name} không hợp lệ."))
        })?;
    let day: u32 = date_it
        .next()
        .and_then(|s| (s.len() == 2).then(|| s.parse().ok()).flatten())
        .ok_or_else(|| ApiError::unprocessable(format!("Ngày trong {field_name} không hợp lệ.")))?;
    if date_it.next().is_some()
        || !(1970..=2100).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
    {
        return Err(ApiError::unprocessable(format!(
            "Ngày trong {field_name} nằm ngoài phạm vi hợp lệ."
        )));
    }
    let mut time_it = time_part.split(':');
    let hour: u32 = time_it
        .next()
        .and_then(|s| (s.len() == 2).then(|| s.parse().ok()).flatten())
        .ok_or_else(|| ApiError::unprocessable(format!("Giờ trong {field_name} không hợp lệ.")))?;
    let minute: u32 = time_it
        .next()
        .and_then(|s| (s.len() == 2).then(|| s.parse().ok()).flatten())
        .ok_or_else(|| ApiError::unprocessable(format!("Phút trong {field_name} không hợp lệ.")))?;
    let second: u32 = match time_it.next() {
        Some(s) => (s.len() == 2)
            .then(|| s.parse().ok())
            .flatten()
            .ok_or_else(|| {
                ApiError::unprocessable(format!("Giây trong {field_name} không hợp lệ."))
            })?,
        None => 0,
    };
    if time_it.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return Err(ApiError::unprocessable(format!(
            "Giờ phút giây trong {field_name} không hợp lệ."
        )));
    }
    Ok(format!(
        "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"
    ))
}

#[derive(Debug, Clone, Serialize)]
pub struct CompositeEntrySaveResult {
    pub entry: ContentEntryRecord,
    pub custom_path: Option<String>,
    pub seo: Option<crate::seo::EntrySeoSnapshot>,
    pub schema_token: String,
}

pub struct CompositePreflightPlan {
    pub preflight_schema_token: String,
    pub locale: String,
    pub translation_group_id: Option<String>,
    pub effective: EffectiveContentSchema,
    pub effective_ct_def: ContentTypeDefinition,
    pub merged_input: ContentEntryInput,
    pub publication_status: Option<String>,
}

/// Check a selection under the same SQLite writer as the entry mutation. A
/// theme switch and entry Save cannot silently race and validate against two
/// different active revisions. Existing values are retained on omission even
/// if their original provider is no longer active.
pub(in crate::storage) async fn resolve_entry_template_patches_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    entry_id: Option<i64>,
    input: &CompositeEntrySaveInput,
) -> ApiResult<(Option<String>, Option<String>)> {
    let existing: (Option<String>, Option<String>) = match entry_id {
        Some(id) => sqlx::query_as(
            "SELECT page_layout_key, custom_template_key FROM content_entries WHERE id = ? AND content_type_key = ?",
        )
        .bind(id)
        .bind(type_key)
        .fetch_one(&mut **tx)
        .await?,
        None => (None, None),
    };

    let mut next = existing.clone();
    let mut needs_layout_validation = false;
    let mut needs_custom_validation = false;
    match &input.page_layout_key {
        PatchField::Omitted => {}
        PatchField::Clear => next.0 = None,
        PatchField::Set(key) => {
            needs_layout_validation = existing.0.as_deref() != Some(key.as_str());
            next.0 = Some(key.clone());
        }
    }
    match &input.custom_template_key {
        PatchField::Omitted => {}
        PatchField::Clear => next.1 = None,
        PatchField::Set(key) => {
            needs_custom_validation = existing.1.as_deref() != Some(key.as_str());
            next.1 = Some(key.clone());
        }
    }
    if !needs_layout_validation && !needs_custom_validation {
        return Ok(next);
    }
    let manifest_json: Option<String> = sqlx::query_scalar(
        "SELECT r.manifest_json FROM theme_active_selection s
           JOIN theme_revisions r ON r.revision_id = s.revision_id
          WHERE s.id = 1 AND r.source_format = 'v3'",
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(manifest_json) = manifest_json else {
        return Err(ApiError::unprocessable(
            "Chưa có theme v3 hoạt động để chọn layout/template.",
        ));
    };
    let manifest: opendash_core::theme::ThemeV3Manifest = serde_json::from_str(&manifest_json)
        .map_err(|_| ApiError::internal("Manifest theme hoạt động không hợp lệ."))?;
    manifest.validate().map_err(ApiError::internal)?;
    if needs_layout_validation
        && !manifest
            .page_layouts
            .iter()
            .any(|layout| Some(layout.key.as_str()) == next.0.as_deref())
    {
        return Err(ApiError::unprocessable(
            "Page Layout không có trong theme đang hoạt động.",
        ));
    }
    if needs_custom_validation
        && !manifest.templates.iter().any(|tpl| {
            tpl.kind == opendash_core::theme::TemplateCandidateKind::SingleCustom
                && tpl.custom_key.as_deref() == next.1.as_deref()
                && tpl.allowed_types.iter().any(|allowed| allowed == type_key)
        })
    {
        return Err(ApiError::unprocessable(
            "Custom Single Template không dành cho loại nội dung này.",
        ));
    }
    Ok(next)
}

pub(in crate::storage) fn actor_field_role(role: auth::Role) -> FieldRole {
    match role {
        auth::Role::Admin => FieldRole::Admin,
        auth::Role::Editor => FieldRole::Editor,
    }
}

pub(in crate::storage) fn effective_as_ct_def(
    effective: &EffectiveContentSchema,
) -> ContentTypeDefinition {
    ContentTypeDefinition {
        key: effective.base.descriptor.key.clone(),
        label: effective.base.descriptor.label.clone(),
        settings: effective.base.descriptor.settings.clone(),
        fields: effective
            .effective_fields
            .values()
            .map(|fe| fe.definition.clone())
            .collect(),
    }
}

pub(in crate::storage) async fn resolve_primary_category_patch_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    entry_id: Option<i64>,
    patch: &PatchField<i64>,
    term_ids_provided: bool,
    effective_term_ids: &[i64],
) -> ApiResult<Option<i64>> {
    match *patch {
        PatchField::Clear => Ok(None),
        PatchField::Set(tid) => {
            if type_key != "post" {
                return Err(ApiError::bad_request(
                    "primary_category_term_id chỉ áp dụng cho bài viết (post).",
                ));
            }
            if tid <= 0 || !effective_term_ids.contains(&tid) {
                return Err(ApiError::bad_request(
                    "primary_category_term_id phải là một term đã được gán cho bài viết.",
                ));
            }
            let is_category: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM taxonomy_terms WHERE id = ? AND kind = 'category')",
            )
            .bind(tid)
            .fetch_one(&mut **tx)
            .await?;
            if is_category == 0 {
                return Err(ApiError::bad_request(
                    "primary_category_term_id phải thuộc taxonomy 'category'.",
                ));
            }
            Ok(Some(tid))
        }
        PatchField::Omitted => match entry_id {
            None => Ok(None),
            Some(id) => {
                if type_key != "post" {
                    return Ok(None);
                }
                let current_primary: Option<i64> = sqlx::query_scalar(
                    "SELECT primary_category_term_id FROM content_entries WHERE id = ?",
                )
                .bind(id)
                .fetch_one(&mut **tx)
                .await?;
                Ok(current_primary
                    .filter(|pid| !term_ids_provided || effective_term_ids.contains(pid)))
            }
        },
    }
}

pub(in crate::storage) async fn resolve_effective_rewrite_base_in_tx(
    conn: &mut sqlx::SqliteConnection,
    resource_kind: &str,
    resource_key: &str,
    site_bases: &BTreeMap<String, Option<String>>,
) -> ApiResult<String> {
    match site_bases.get(resource_key) {
        Some(Some(base)) if !base.trim().is_empty() => return Ok(base.trim().to_owned()),
        Some(None) => return Ok(resource_key.to_owned()),
        _ => {}
    }
    let provider_default: Option<String> = sqlx::query_scalar(
        "SELECT rewrite_base FROM provider_rewrite_defaults
          WHERE resource_kind = ? AND resource_key = ?
            AND provider_status = 'active' AND declaration_state = 'declared'",
    )
    .bind(resource_kind)
    .bind(resource_key)
    .fetch_optional(&mut *conn)
    .await?
    .flatten();
    Ok(provider_default
        .filter(|b| !b.trim().is_empty())
        .unwrap_or_else(|| resource_key.to_owned()))
}

pub(in crate::storage) async fn remap_term_ids_to_locale_in_conn(
    conn: &mut sqlx::SqliteConnection,
    type_key: &str,
    ids: &[i64],
    target_locale: &str,
) -> ApiResult<BTreeMap<i64, i64>> {
    let mut map = BTreeMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    let eligible = crate::storage::content::load_effective_taxonomies_in_tx(conn, type_key).await?;
    let ids_json = serde_json::to_string(ids)?;
    let rows: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT src.id, dst.id, dst.kind
           FROM taxonomy_terms src
           JOIN taxonomy_terms dst
             ON dst.kind = src.kind
            AND dst.translation_group_id = src.translation_group_id
            AND dst.locale = ?2
           JOIN taxonomy_definitions d ON d.key = dst.kind
          WHERE src.id IN (SELECT value FROM json_each(?1))
            AND d.provider_status = 'active'
            AND d.declaration_state = 'declared'",
    )
    .bind(ids_json)
    .bind(target_locale)
    .fetch_all(conn)
    .await?;
    for (src_id, dst_id, kind) in rows {
        if eligible.contains(&kind) {
            map.insert(src_id, dst_id);
        }
    }
    Ok(map)
}

pub(in crate::storage) async fn remap_list_term_ids_in_conn(
    conn: &mut sqlx::SqliteConnection,
    ids: &[i64],
    target_locale: &str,
) -> ApiResult<BTreeMap<i64, i64>> {
    let mut map = BTreeMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    let ids_json = serde_json::to_string(ids)?;
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT src.id, dst.id
           FROM taxonomy_terms src
           JOIN taxonomy_terms dst
             ON dst.kind = src.kind
            AND dst.translation_group_id = src.translation_group_id
            AND dst.locale = ?2
           JOIN taxonomy_definitions d ON d.key = dst.kind
          WHERE src.id IN (SELECT value FROM json_each(?1))
            AND d.public = 1
            AND d.provider_status = 'active'
            AND d.declaration_state = 'declared'",
    )
    .bind(ids_json)
    .bind(target_locale)
    .fetch_all(conn)
    .await?;
    for (src_id, dst_id) in rows {
        map.insert(src_id, dst_id);
    }
    Ok(map)
}

pub(in crate::storage) fn collect_builder_list_term_ids(
    doc: &opendash_core::builder::BuilderDocument,
) -> Vec<i64> {
    let mut out = Vec::new();
    for section in &doc.sections {
        for row in &section.children {
            for col in &row.children {
                for el in &col.children {
                    if let opendash_core::builder::ElementContent::Known(
                        opendash_core::builder::KnownElement::List(props),
                    ) = &el.content
                    {
                        if let Some(id) = props.category_id {
                            out.push(id);
                        }
                        if let Some(id) = props.tag_id {
                            out.push(id);
                        }
                        if let Some(id) = props.term_id {
                            out.push(id);
                        }
                    }
                }
            }
        }
    }
    out
}

pub(in crate::storage) fn apply_builder_list_term_remap(
    doc: &mut opendash_core::builder::BuilderDocument,
    remap: &BTreeMap<i64, i64>,
) -> bool {
    let mut changed = false;
    for section in &mut doc.sections {
        for row in &mut section.children {
            for col in &mut row.children {
                for el in &mut col.children {
                    if let opendash_core::builder::ElementContent::Known(
                        opendash_core::builder::KnownElement::List(props),
                    ) = &mut el.content
                    {
                        if let Some(old) = props.category_id {
                            let next = remap.get(&old).copied();
                            if next != props.category_id {
                                props.category_id = next;
                                changed = true;
                            }
                        }
                        if let Some(old) = props.tag_id {
                            let next = remap.get(&old).copied();
                            if next != props.tag_id {
                                props.tag_id = next;
                                changed = true;
                            }
                        }
                        if let Some(old) = props.term_id {
                            let next = remap.get(&old).copied();
                            if next != props.term_id {
                                props.term_id = next;
                                if next.is_none() {
                                    props.taxonomy_key = None;
                                }
                                changed = true;
                            }
                        }
                        if props.include_descendants
                            && props.category_id.is_none()
                            && props.taxonomy_key.is_none()
                        {
                            props.include_descendants = false;
                            changed = true;
                        }
                    }
                }
            }
        }
    }
    changed
}

pub(in crate::storage) async fn missing_or_stale(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
) -> ApiResult<ApiError> {
    if get_entry(db, type_key, id).await?.is_some() {
        Ok(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        ))
    } else {
        Ok(ApiError::not_found("Không tìm thấy nội dung."))
    }
}
