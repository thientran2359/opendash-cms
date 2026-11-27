//! M7-S16-T05: bounded, permission-aware entry table snapshot.
//! The existing bare-array entries API deliberately stays in storage/entries.rs.

use super::*;

pub(super) use std::collections::BTreeMap;

pub(super) use opendash_core::{
    content::{ContentPublication, EffectiveFieldStatus, FieldRole, PublicExposure},
    registry::ProviderStatus,
};

pub(super) use serde::Serialize;

pub(super) use serde_json::Value;

pub(super) use sqlx::{Sqlite, SqlitePool, Transaction};

pub(super) use crate::{
    error::{ApiError, ApiResult},
    storage::{content, entries, entry_lifecycle, seo},
};

#[derive(Debug, Clone)]
pub struct EntryTableFilter {
    pub q: String,
    pub status: String,
    pub retained_trash: bool,
    pub locale: Option<String>,
    pub category_id: Option<i64>,
    pub tag_id: Option<i64>,
    pub taxonomy_key: Option<String>,
    pub term_id: Option<i64>,
    pub include_descendants: bool,
    pub updated_month: Option<String>,
    pub sort: String,
    pub order: String,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EntryRowCapabilities {
    pub edit: bool,
    pub publish: bool,
    pub unpublish: bool,
    pub trash: bool,
    pub restore: bool,
    pub purge: bool,
    pub quick_edit: bool,
    pub immediate_public_restore: bool,
}

#[derive(Debug, Serialize)]
pub struct EntryTableRow {
    pub id: i64,
    pub type_key: String,
    pub locale: String,
    pub translation_group_id: String,
    pub title: String,
    pub slug: String,
    pub publication_status: String,
    pub lifecycle: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub published_at: Option<String>,
    pub trashed_at: Option<String>,
    #[serde(default)]
    pub parent_id: Option<i64>,
    #[serde(default)]
    pub menu_order: i32,
    pub term_ids: Vec<i64>,
    pub public_url: Option<String>,
    pub fields: BTreeMap<String, Value>,
    pub translations: Vec<entries::TranslationLinkSummary>,
    pub capabilities: EntryRowCapabilities,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub blocked_reasons: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct StatusCounts {
    pub all: i64,
    pub draft: i64,
    pub published: i64,
    pub trash: i64,
}

#[derive(Debug, Serialize)]
pub struct EntryTableResult {
    pub schema_version: i32,
    pub items: Vec<EntryTableRow>,
    pub total: i64,
    pub status_counts: StatusCounts,
    pub limit: i64,
    pub offset: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_token: Option<String>,
}

/// A single SQLite read transaction guarantees counts, ACL and rows are from
/// one snapshot; separate HTTP pages intentionally make no cursor-isolation claim.
pub async fn query(
    db: &SqlitePool,
    type_key: &str,
    caller_role: FieldRole,
    filter: &EntryTableFilter,
) -> ApiResult<EntryTableResult> {
    if type_key == "pattern" {
        return Err(ApiError::forbidden("Section dùng bảng quản lý riêng tư."));
    }
    if type_key == "section" {
        return Err(ApiError::forbidden(
            "entry_lifecycle_forbidden: Không hỗ trợ thao tác vòng đời chung cho Pattern hoặc Section.",
        ));
    }

    if filter.retained_trash {
        return query_retained_trash_maintenance(db, type_key, caller_role, filter).await;
    }

    let mut tx = db.begin().await?;
    let resolved_locale = match filter.locale.as_deref() {
        Some(raw) => Some(
            crate::storage::settings::resolve_requested_locale_in_conn(&mut tx, Some(raw)).await?,
        ),
        None => None,
    };
    let Some(effective) = content::load_effective_content_schema_in_tx(&mut tx, type_key).await?
    else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    };
    if effective.base.status != ProviderStatus::Active {
        return Err(ApiError::not_found("Loại nội dung không khả dụng."));
    }
    validate_terms(&mut tx, type_key, filter).await?;

    let manageable_locales: Vec<String> = sqlx::query_scalar(
        "SELECT code FROM site_locales
          WHERE deleted_at IS NULL
            AND (is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
          ORDER BY code ASC",
    )
    .fetch_all(&mut *tx)
    .await?;
    let (resolved_locale, active_locales): (Option<String>, Vec<String>) = match resolved_locale {
        Some(loc) if manageable_locales.iter().any(|c| c == &loc) => (Some(loc), Vec::new()),
        Some(_) => (None, Vec::new()),
        None => (None, manageable_locales),
    };

    let mut count_qb: sqlx::QueryBuilder<Sqlite> = sqlx::QueryBuilder::new("");
    push_entry_table_ctes(&mut count_qb, filter);
    count_qb.push(
        "SELECT \
         COALESCE(SUM(CASE WHEN e.trashed_at IS NULL THEN 1 ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN e.trashed_at IS NULL AND e.publication_status = 'draft' THEN 1 ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN e.trashed_at IS NULL AND e.publication_status = 'published' THEN 1 ELSE 0 END), 0), \
         COALESCE(SUM(CASE WHEN e.trashed_at IS NOT NULL THEN 1 ELSE 0 END), 0)",
    );
    push_entry_table_scope(
        &mut count_qb,
        type_key,
        filter,
        resolved_locale.as_deref(),
        &active_locales,
    );
    let (all, draft, published, trash): (i64, i64, i64, i64) =
        count_qb.build_query_as().fetch_one(&mut *tx).await?;
    let status_counts = StatusCounts {
        all,
        draft,
        published,
        trash,
    };
    let total = match filter.status.as_str() {
        "draft" => draft,
        "published" => published,
        "trash" => trash,
        _ => all,
    };

    let is_trash_view = filter.status == "trash";
    let status_clause = match filter.status.as_str() {
        "draft" => " AND e.trashed_at IS NULL AND e.publication_status = 'draft'",
        "published" => " AND e.trashed_at IS NULL AND e.publication_status = 'published'",
        "trash" => " AND e.trashed_at IS NOT NULL",
        _ => " AND e.trashed_at IS NULL",
    };

    // Closed allowlists only, never interpolated user data.
    let column = match filter.sort.as_str() {
        "title" => "e.title COLLATE NOCASE",
        "created_at" => "e.created_at",
        "published_at" => "COALESCE(e.published_at, e.created_at)",
        "menu_order" => "e.menu_order",
        "id" => "e.id",
        "trashed_at" => "e.trashed_at",
        _ if is_trash_view => "e.trashed_at",
        _ => "e.updated_at",
    };
    let direction = if filter.order == "asc" { "ASC" } else { "DESC" };
    let tie = if filter.sort == "id" {
        String::new()
    } else {
        format!(", e.id {direction}")
    };
    let mut rows_qb: sqlx::QueryBuilder<Sqlite> = sqlx::QueryBuilder::new("");
    push_entry_table_ctes(&mut rows_qb, filter);
    rows_qb.push(
        "SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json, \
                e.publication_status, e.version, e.created_at, e.updated_at, e.published_at, \
                e.locale, e.translation_group_id, e.trashed_at, e.parent_id, e.menu_order",
    );
    push_entry_table_scope(
        &mut rows_qb,
        type_key,
        filter,
        resolved_locale.as_deref(),
        &active_locales,
    );
    rows_qb.push(status_clause);
    rows_qb.push(format!(" ORDER BY {column} {direction}{tie} LIMIT "));
    rows_qb.push_bind(filter.limit);
    rows_qb.push(" OFFSET ");
    rows_qb.push_bind(filter.offset);
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
        Option<String>,
        Option<i64>,
        i32,
    );
    let selected: Vec<SelectedRow> = rows_qb.build_query_as().fetch_all(&mut *tx).await?;

    let is_draft_publish =
        effective.base.descriptor.settings.publication == ContentPublication::DraftPublish;
    let is_public_immediate = effective.base.descriptor.settings.publication
        == ContentPublication::Immediate
        && effective.base.descriptor.settings.public;
    let is_admin = caller_role == FieldRole::Admin;

    let assigned_reading_pages = if !is_trash_view && type_key == "page" {
        entry_lifecycle::load_assigned_reading_page_ids_in_tx(&mut tx).await?
    } else {
        std::collections::HashSet::new()
    };

    // Load enabled state of manageable locales in the same snapshot so Trash rows know if restore is blocked.
    let disabled_locales: std::collections::HashSet<String> = if is_trash_view {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT code FROM site_locales WHERE deleted_at IS NULL AND enabled = 0",
        )
        .fetch_all(&mut *tx)
        .await?;
        rows.into_iter().collect()
    } else {
        std::collections::HashSet::new()
    };

    let (translations_by_group, mut terms_by_entry, public_urls, admin_col_keys) = if is_trash_view
    {
        (
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            std::collections::BTreeSet::new(),
        )
    } else {
        let group_ids: Vec<String> = selected
            .iter()
            .map(|row| row.11.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let tr = entries::batch_translations_by_group_in_tx(&mut tx, type_key, &group_ids).await?;
        let entry_ids: Vec<i64> = selected.iter().map(|row| row.0).collect();
        let terms = entries::batch_load_term_ids_in_conn(&mut tx, &entry_ids).await?;
        let published_ids: Vec<i64> = selected
            .iter()
            .filter(|row| row.5 == "published")
            .map(|row| row.0)
            .collect();
        let urls =
            seo::resolve_public_entry_paths_by_entry_ids_in_conn(&mut tx, &published_ids).await?;
        let col_keys =
            crate::storage::admin_extensions::active_column_field_keys_in_conn(&mut tx, type_key)
                .await?;
        (tr, terms, urls, col_keys)
    };

    let mut items = Vec::with_capacity(selected.len());
    for (
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
        trashed_at,
        parent_id,
        menu_order,
    ) in selected
    {
        if is_trash_view {
            let locale_disabled = disabled_locales.contains(&locale);
            let mut blocked_reasons = BTreeMap::new();
            if locale_disabled {
                blocked_reasons.insert("restore".into(), "locale_disabled".into());
            }
            if !is_admin {
                blocked_reasons.insert("purge".into(), "admin_required".into());
            }
            items.push(EntryTableRow {
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
                    restore: !locale_disabled,
                    purge: is_admin,
                    quick_edit: false,
                    immediate_public_restore: is_public_immediate,
                },
                blocked_reasons,
            });
            continue;
        }

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
        let term_ids = terms_by_entry.remove(&record.id).unwrap_or_default();
        filter_entry_table_fields_for_role(
            &mut record.values,
            &effective,
            &admin_col_keys,
            caller_role,
        );
        let public_url = if record.publication_status == "published" {
            public_urls.get(&record.id).cloned()
        } else {
            None
        };
        let translations = translations_by_group
            .get(&translation_group_id)
            .cloned()
            .unwrap_or_default();
        let is_reading_assigned = assigned_reading_pages.contains(&record.id);
        let mut blocked_reasons = BTreeMap::new();
        if is_reading_assigned {
            blocked_reasons.insert("trash".into(), "reading_page_assigned".into());
        }
        items.push(EntryTableRow {
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
        });
    }
    tx.commit().await?;
    Ok(EntryTableResult {
        schema_version: 1,
        items,
        total,
        status_counts,
        limit: filter.limit,
        offset: filter.offset,
        schema_token: Some(effective.schema_token),
    })
}
