use super::*;

/// Loads one active entry formatted with the exact same role-filtered table projection,
/// capabilities, blocked reasons, translations, terms, and public URL as `entry_table::query`.
pub(crate) async fn load_single_active_row_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    entry_id: i64,
    caller_role: FieldRole,
    effective: &opendash_core::content::EffectiveContentSchema,
) -> ApiResult<EntryTableRow> {
    type SelectedRow = (
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
        String,
        String,
        Option<i64>,
        i32,
    );
    let row: SelectedRow = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                e.publication_status, e.version, e.created_at, e.updated_at, e.published_at,
                e.locale, e.translation_group_id, e.parent_id, e.menu_order
           FROM content_entries e
          WHERE e.content_type_key = ? AND e.id = ? AND e.trashed_at IS NULL",
    )
    .bind(type_key)
    .bind(entry_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;

    let (
        id,
        content_type_key,
        slug,
        title,
        json,
        publication_status,
        version,
        created_at,
        updated_at,
        published_at,
        locale,
        translation_group_id,
        parent_id,
        menu_order,
    ) = row;

    let is_draft_publish =
        effective.base.descriptor.settings.publication == ContentPublication::DraftPublish;
    let assigned_reading_pages = if type_key == "page" {
        entry_lifecycle::load_assigned_reading_page_ids_in_tx(tx).await?
    } else {
        std::collections::HashSet::new()
    };

    let group_ids = vec![translation_group_id.clone()];
    let translations = entries::batch_translations_by_group_in_tx(tx, type_key, &group_ids)
        .await?
        .remove(&translation_group_id)
        .unwrap_or_default();
    let term_ids = entries::batch_load_term_ids_in_conn(&mut **tx, &[id])
        .await?
        .remove(&id)
        .unwrap_or_default();
    let public_url = if publication_status == "published" {
        seo::resolve_public_entry_paths_by_entry_ids_in_conn(&mut **tx, &[id])
            .await?
            .remove(&id)
    } else {
        None
    };

    let mut record = entries::decode_row((
        id,
        content_type_key,
        slug,
        title,
        json,
        publication_status,
        version,
        created_at,
        updated_at,
        published_at.clone(),
    ))?;
    let admin_col_keys =
        crate::storage::admin_extensions::active_column_field_keys_in_conn(&mut **tx, type_key)
            .await?;
    filter_entry_table_fields_for_role(
        &mut record.values,
        effective,
        &admin_col_keys,
        caller_role,
    );
    let is_reading_assigned = assigned_reading_pages.contains(&record.id);
    let mut blocked_reasons = BTreeMap::new();
    if is_reading_assigned {
        blocked_reasons.insert("trash".into(), "reading_page_assigned".into());
    }

    Ok(EntryTableRow {
        id: record.id,
        type_key: record.content_type_key,
        locale,
        translation_group_id,
        title: record.title,
        slug: record.slug,
        publication_status: record.publication_status.clone(),
        lifecycle: "active".into(),
        version: record.version,
        created_at: record.created_at,
        updated_at: record.updated_at,
        published_at,
        trashed_at: None,
        parent_id,
        menu_order,
        term_ids,
        public_url,
        fields: record.values,
        translations,
        capabilities: EntryRowCapabilities {
            edit: true,
            publish: is_draft_publish && record.publication_status == "draft",
            unpublish: is_draft_publish && record.publication_status == "published",
            trash: !is_reading_assigned,
            restore: false,
            purge: false,
            quick_edit: true,
            immediate_public_restore: false,
        },
        blocked_reasons,
    })
}

/// Admin-only dormant retained-Trash metadata query for an inactive/withdrawn ordinary CPT.
pub(in crate::storage) async fn query_retained_trash_maintenance(
    db: &SqlitePool,
    type_key: &str,
    caller_role: FieldRole,
    filter: &EntryTableFilter,
) -> ApiResult<EntryTableResult> {
    if caller_role != FieldRole::Admin {
        return Err(ApiError::forbidden(
            "retained_trash_forbidden: Chỉ Quản trị viên được phép xem vùng bảo trì Thùng rác.",
        ));
    }
    if filter.status != "trash" {
        return Err(ApiError::bad_request(
            "retained_trash=true chỉ hỗ trợ cùng status=trash.",
        ));
    }

    let mut tx = db.begin().await?;
    let resolved_locale = match filter.locale.as_deref() {
        Some(raw) => Some(
            crate::storage::settings::resolve_requested_locale_in_conn(&mut tx, Some(raw)).await?,
        ),
        None => None,
    };
    let ct_row: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM content_types WHERE key = ?",
    )
    .bind(type_key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((provider_status, declaration_state)) = ct_row else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    };
    if provider_status == "active" && declaration_state == "declared" {
        return Err(ApiError::conflict(
            "provider_active: Loại nội dung đang hoạt động; hãy sử dụng Thùng rác thông thường.",
        ));
    }

    let total_sql = format!(
        "SELECT COUNT(*)
           FROM content_entries e
          WHERE e.content_type_key = ?
            AND e.trashed_at IS NOT NULL
            AND (? IS NULL OR e.locale = ?)
            AND {}",
        entry_lifecycle::MANAGEABLE_LOCALE_EXISTS_SQL
    );
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(total_sql))
        .bind(type_key)
        .bind(resolved_locale.as_deref())
        .bind(resolved_locale.as_deref())
        .fetch_one(&mut *tx)
        .await?;

    type RetainedRow = (
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
        Option<String>,
        String,
    );
    let rows_sql = format!(
        "SELECT e.id, e.content_type_key, e.locale, e.slug, e.title,
                e.publication_status, e.version, e.created_at, e.updated_at,
                e.published_at, e.trashed_at, e.translation_group_id
           FROM content_entries e
          WHERE e.content_type_key = ?
            AND e.trashed_at IS NOT NULL
            AND (? IS NULL OR e.locale = ?)
            AND {}
          ORDER BY e.trashed_at DESC, e.id DESC
          LIMIT ? OFFSET ?",
        entry_lifecycle::MANAGEABLE_LOCALE_EXISTS_SQL
    );
    let rows: Vec<RetainedRow> = sqlx::query_as(sqlx::AssertSqlSafe(rows_sql))
        .bind(type_key)
        .bind(resolved_locale.as_deref())
        .bind(resolved_locale.as_deref())
        .bind(filter.limit)
        .bind(filter.offset)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;

    let items = rows
        .into_iter()
        .map(
            |(
                id,
                content_type_key,
                locale,
                slug,
                title,
                publication_status,
                version,
                created_at,
                updated_at,
                published_at,
                trashed_at,
                translation_group_id,
            )| EntryTableRow {
                id,
                type_key: content_type_key,
                locale,
                translation_group_id,
                title,
                slug,
                publication_status,
                lifecycle: "trash".into(),
                version,
                created_at,
                updated_at,
                published_at,
                trashed_at,
                parent_id: None,
                menu_order: 0,
                term_ids: Vec::new(),
                public_url: None,
                fields: BTreeMap::new(),
                translations: Vec::new(),
                capabilities: EntryRowCapabilities {
                    edit: false,
                    publish: false,
                    unpublish: false,
                    trash: false,
                    restore: false,
                    purge: true,
                    quick_edit: false,
                    immediate_public_restore: false,
                },
                blocked_reasons: BTreeMap::new(),
            },
        )
        .collect();

    Ok(EntryTableResult {
        schema_version: 1,
        items,
        total,
        status_counts: StatusCounts {
            all: 0,
            draft: 0,
            published: 0,
            trash: total,
        },
        limit: filter.limit,
        offset: filter.offset,
        schema_token: None,
    })
}
