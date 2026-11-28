use super::*;

pub(crate) async fn batch_translations_by_group_in_tx(
    conn: &mut sqlx::SqliteConnection,
    type_key: &str,
    group_ids: &[String],
) -> ApiResult<BTreeMap<String, Vec<TranslationLinkSummary>>> {
    let mut out = BTreeMap::<String, Vec<TranslationLinkSummary>>::new();
    if group_ids.is_empty() {
        return Ok(out);
    }
    for chunk in group_ids.chunks(500) {
        let ids_json = serde_json::to_string(chunk)?;
        let rows: Vec<(String, i64, String, String, String, String)> = sqlx::query_as(
            "SELECT e.translation_group_id, e.id, e.locale, e.slug, e.title, e.publication_status
               FROM content_entries e
               JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
              WHERE e.content_type_key = ?
                AND e.translation_group_id IN (SELECT value FROM json_each(?))
                AND e.trashed_at IS NULL
              ORDER BY sl.is_default DESC, sl.position ASC, sl.code ASC",
        )
        .bind(type_key)
        .bind(ids_json)
        .fetch_all(&mut *conn)
        .await?;
        for (group_id, id, locale, slug, title, publication_status) in rows {
            out.entry(group_id)
                .or_default()
                .push(TranslationLinkSummary {
                    id,
                    locale,
                    slug,
                    title,
                    publication_status,
                });
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerFence {
    Plugin {
        plugin_id: String,
        expected_generation: i64,
    },
    Theme {
        theme_id: String,
        revision_id: String,
        selection_version: i64,
    },
}

/// Acquire a writer before checking a term's existence or entry CAS. The same
/// SQLite writer reservation is used by taxonomy term deletion, so no race can
/// commit a link to a term that has just been deleted.
#[derive(Debug, Clone)]
pub struct PluginActionFence {
    pub owner_fence: OwnerFence,
    pub required_role: String,
}

pub(in crate::storage) async fn verify_plugin_action_fence(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &SessionContext,
    fence: &PluginActionFence,
) -> ApiResult<()> {
    let active = match &fence.owner_fence {
        OwnerFence::Plugin {
            plugin_id,
            expected_generation,
        } => {
            crate::storage::plugin::is_plugin_generation_active(
                &mut **tx,
                plugin_id,
                *expected_generation,
            )
            .await?
        }
        OwnerFence::Theme {
            revision_id,
            selection_version,
            ..
        } => {
            crate::storage::plugin::verify_active_theme_fence_in_tx(
                &mut **tx,
                revision_id,
                *selection_version,
            )
            .await?
        }
    };
    if !active {
        return Err(ApiError::conflict(
            "Plugin đã bị vô hiệu hóa hoặc thay đổi phiên bản trong lúc thực thi.",
        ));
    }
    let role_ok = crate::storage::plugin::is_actor_role_active(
        &mut **tx,
        actor.user.id,
        &fence.required_role,
    )
    .await?;
    if !role_ok {
        return Err(ApiError::forbidden("Quyền của tài khoản đã bị thu hồi."));
    }
    Ok(())
}

pub(in crate::storage) async fn validate_term_ids_for_locale(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    ids: &[i64],
    entry_locale: &str,
) -> ApiResult<()> {
    if ids.len() > 96 || ids.iter().any(|id| *id <= 0) {
        return Err(ApiError::bad_request("term_ids không hợp lệ."));
    }
    let mut unique = std::collections::HashSet::new();
    let mut counts = std::collections::HashMap::<String, usize>::new();
    let eligible =
        crate::storage::content::load_effective_taxonomies_in_tx(&mut **tx, type_key).await?;
    for &id in ids {
        if !unique.insert(id) {
            return Err(ApiError::bad_request("term_ids không được trùng lặp."));
        }
        let found: Option<(String, String)> = sqlx::query_as(
            "SELECT t.kind, t.locale FROM taxonomy_terms t
               JOIN taxonomy_definitions d ON d.key=t.kind
              WHERE t.id=? AND d.provider_status='active'
                AND d.declaration_state='declared'",
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
        let (kind, term_locale) =
            found.ok_or_else(|| ApiError::bad_request("Term ID không tồn tại hoặc inactive."))?;
        if term_locale != entry_locale {
            return Err(ApiError::unprocessable(
                "Term thuộc ngôn ngữ khác với nội dung.",
            ));
        }
        if !eligible.contains(&kind) {
            return Err(ApiError::bad_request(
                "Taxonomy của term không được phép dùng cho CPT.",
            ));
        }
        let count = counts.entry(kind.clone()).or_insert(0);
        *count += 1;
        if *count > if kind == "category" { 32 } else { 64 } {
            return Err(ApiError::bad_request("Số term của taxonomy vượt giới hạn."));
        }
    }
    enforce_taxonomy_selection_mode_in_conn(&mut **tx, type_key, &counts).await?;
    Ok(())
}

/// Shared category/tag AND predicates for the old archives and new list
/// blocks. Only an internal enum can select the ORDER BY clause.
pub(in crate::storage) async fn query_entries_filtered(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
    published_only: bool,
    enforce_public_locale: bool,
    sort: PublishedListSort,
) -> ApiResult<Vec<ContentEntryRecord>> {
    let order_by = match sort {
        PublishedListSort::IdDesc => "e.id DESC",
        PublishedListSort::IdAsc => "e.id ASC",
        PublishedListSort::PublishedAtDesc => {
            "COALESCE(e.published_at, e.created_at) DESC, e.id DESC"
        }
        PublishedListSort::CreatedAtDesc => "e.created_at DESC, e.id DESC",
        PublishedListSort::UpdatedAtDesc => "e.updated_at DESC, e.id DESC",
        PublishedListSort::TitleAsc => "e.title COLLATE NOCASE ASC, e.id ASC",
        PublishedListSort::MenuOrderAsc => "e.menu_order ASC, e.id ASC",
    };
    // Audited dynamic SQL: order_by is a closed Rust enum mapped to static literals;
    // all user-controlled values remain bound params.
    let statement = format!(
        "{FILTERED_ENTRIES_CTE_AND_FROM}
         SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json, e.publication_status,
                e.version, e.created_at, e.updated_at, e.published_at,
                e.page_layout_key, e.custom_template_key, e.content_mode,
                e.primary_category_term_id, e.parent_id, e.menu_order,
                e.locale, e.translation_group_id
          {FILTERED_ENTRIES_WHERE}
          ORDER BY {order_by} LIMIT ? OFFSET ?"
    );
    let rows: Vec<FullEntryDbRow> = sqlx::query_as(sqlx::AssertSqlSafe(statement))
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
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut **tx)
        .await?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let entry_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let mut terms_by_entry = batch_load_term_ids_in_conn(&mut **tx, &entry_ids).await?;
    let group_ids: Vec<String> = rows
        .iter()
        .map(|r| r.translation_group_id.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let translations_by_group =
        batch_translations_by_group_in_tx(&mut **tx, type_key, &group_ids).await?;

    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        let mut record = decode_row((
            row.id,
            row.content_type_key,
            row.slug,
            row.title,
            row.values_json,
            row.publication_status,
            row.version,
            row.created_at,
            row.updated_at,
            row.published_at,
        ))?;
        record.locale = row.locale;
        record.page_layout_key = row.page_layout_key;
        record.custom_template_key = row.custom_template_key;
        record.primary_category_term_id = row.primary_category_term_id;
        record.parent_id = row.parent_id;
        record.menu_order = row.menu_order;
        record.content_mode = match row.content_mode.as_str() {
            "editor" => ContentMode::Editor,
            "builder" => ContentMode::Builder,
            _ => return Err(ApiError::internal("Chế độ nội dung không hợp lệ.")),
        };
        record.term_ids = terms_by_entry.remove(&record.id).unwrap_or_default();
        record.translations = translations_by_group
            .get(&row.translation_group_id)
            .cloned()
            .unwrap_or_default();
        record.translation_group_id = row.translation_group_id;
        entries.push(record);
    }
    Ok(entries)
}

pub async fn create_entry(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &opendash_core::content::ContentTypeDefinition,
    input: &ContentEntryInput,
) -> ApiResult<ContentEntryRecord> {
    create_entry_with_fence(db, actor, type_key, schema, input, None).await
}

pub async fn update_entry(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &opendash_core::content::ContentTypeDefinition,
    id: i64,
    expected_version: i64,
    input: &ContentEntryInput,
) -> ApiResult<ContentEntryRecord> {
    update_entry_with_fence(
        db,
        actor,
        type_key,
        schema,
        id,
        expected_version,
        input,
        None,
    )
    .await
}
