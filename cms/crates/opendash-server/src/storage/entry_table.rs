//! M7-S16-T05: bounded, permission-aware entry table snapshot.
//! The existing bare-array entries API deliberately stays in storage/entries.rs.

use std::collections::BTreeMap;

use opendash_core::{
    content::{ContentPublication, EffectiveFieldStatus, FieldRole, PublicExposure},
    registry::ProviderStatus,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
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

impl Default for EntryTableFilter {
    fn default() -> Self {
        Self {
            q: String::new(),
            status: "all".into(),
            retained_trash: false,
            locale: None,
            category_id: None,
            tag_id: None,
            taxonomy_key: None,
            term_id: None,
            include_descendants: false,
            updated_month: None,
            sort: "updated_at".into(),
            order: "desc".into(),
            limit: 20,
            offset: 0,
        }
    }
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

// Identical filters for count and row queries. Status is deliberately absent:
// status_counts represent the current filter selection before status selection.
const CATEGORY_SCOPE: &str = "
 WITH RECURSIVE category_scope(id, depth) AS (
   SELECT id, 1 FROM taxonomy_terms WHERE id = ? AND kind = 'category'
   UNION ALL
   SELECT c.id, category_scope.depth + 1 FROM taxonomy_terms c
   JOIN category_scope ON c.parent_id = category_scope.id
   WHERE category_scope.depth < 8 AND ? = 1
 ), generic_scope(id, depth) AS (
   SELECT id, 1 FROM taxonomy_terms WHERE id = ? AND kind = ?
   UNION ALL
   SELECT c.id, generic_scope.depth + 1 FROM taxonomy_terms c
   JOIN generic_scope ON c.parent_id = generic_scope.id
   WHERE generic_scope.depth < 8 AND ? = 1
 )";

// Identical filters for count and row queries. Status & lifecycle are applied
// explicitly so one snapshot yields both active status counts and Trash count.
const SCOPE: &str = "
 FROM content_entries e
 WHERE e.content_type_key = ?
   AND (? IS NULL OR e.locale = ?)
   AND EXISTS (SELECT 1 FROM site_locales sl WHERE sl.code = e.locale AND sl.deleted_at IS NULL)
   AND (? = '' OR instr(lower(e.title), lower(?)) > 0
                OR instr(lower(e.slug), lower(?)) > 0)
   AND (? IS NULL OR substr(e.updated_at, 1, 7) = ?)
   AND (? IS NULL OR EXISTS (
     SELECT 1 FROM entry_terms et WHERE et.entry_id = e.id
       AND et.term_id IN (SELECT id FROM category_scope)
   ))
   AND (? IS NULL OR EXISTS (
     SELECT 1 FROM entry_terms et WHERE et.entry_id = e.id AND et.term_id = ?
   ))
   AND (? IS NULL OR EXISTS (
     SELECT 1 FROM entry_terms et WHERE et.entry_id = e.id
       AND et.term_id IN (SELECT id FROM generic_scope)
   ))";

macro_rules! bind_scope {
    ($query:expr, $key:expr, $filter:expr, $locale:expr) => {
        $query
            .bind($filter.category_id)
            .bind(i64::from($filter.include_descendants))
            .bind($filter.term_id)
            .bind(&$filter.taxonomy_key)
            .bind(i64::from($filter.include_descendants))
            .bind($key)
            .bind($locale)
            .bind($locale)
            .bind(&$filter.q)
            .bind(&$filter.q)
            .bind(&$filter.q)
            .bind(&$filter.updated_month)
            .bind(&$filter.updated_month)
            .bind($filter.category_id)
            .bind($filter.tag_id)
            .bind($filter.tag_id)
            .bind($filter.term_id)
    };
}

async fn validate_terms(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    filter: &EntryTableFilter,
) -> ApiResult<()> {
    entries::validate_filter_in_tx(
        tx,
        type_key,
        &entries::EntryFilters {
            category_id: filter.category_id,
            tag_id: filter.tag_id,
            taxonomy_key: filter.taxonomy_key.clone(),
            term_id: filter.term_id,
            include_descendants: filter.include_descendants,
            locale: None,
        },
        false,
    )
    .await
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

    let count_sql = format!(
        "{CATEGORY_SCOPE} SELECT
         COALESCE(SUM(CASE WHEN e.trashed_at IS NULL THEN 1 ELSE 0 END),0),
         COALESCE(SUM(CASE WHEN e.trashed_at IS NULL AND e.publication_status='draft' THEN 1 ELSE 0 END),0),
         COALESCE(SUM(CASE WHEN e.trashed_at IS NULL AND e.publication_status='published' THEN 1 ELSE 0 END),0),
         COALESCE(SUM(CASE WHEN e.trashed_at IS NOT NULL THEN 1 ELSE 0 END),0)
         {SCOPE}"
    );
    let (all, draft, published, trash): (i64, i64, i64, i64) = bind_scope!(
        sqlx::query_as(sqlx::AssertSqlSafe(count_sql)),
        type_key,
        filter,
        resolved_locale.as_deref()
    )
    .fetch_one(&mut *tx)
    .await?;
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
        "draft" => "AND e.trashed_at IS NULL AND e.publication_status = 'draft'",
        "published" => "AND e.trashed_at IS NULL AND e.publication_status = 'published'",
        "trash" => "AND e.trashed_at IS NOT NULL",
        _ => "AND e.trashed_at IS NULL",
    };

    // Closed allowlists only, never interpolated user data.
    let column = match filter.sort.as_str() {
        "title" => "e.title COLLATE NOCASE",
        "created_at" => "e.created_at",
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
    let rows_sql = format!(
        "{CATEGORY_SCOPE} SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                e.publication_status, e.version, e.created_at, e.updated_at, e.published_at,
                e.locale, e.translation_group_id, e.trashed_at
         {SCOPE} {status_clause}
         ORDER BY {column} {direction}{tie} LIMIT ? OFFSET ?"
    );
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
    );
    let selected: Vec<SelectedRow> = bind_scope!(
        sqlx::query_as(sqlx::AssertSqlSafe(rows_sql)),
        type_key,
        filter,
        resolved_locale.as_deref()
    )
    .bind(filter.limit)
    .bind(filter.offset)
    .fetch_all(&mut *tx)
    .await?;

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

    let (translations_by_group, mut terms_by_entry, public_urls) = if is_trash_view {
        (BTreeMap::new(), BTreeMap::new(), BTreeMap::new())
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
        (tr, terms, urls)
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
        record.values.retain(|key, _| {
            effective.effective_fields.get(key).is_some_and(|field| {
                field.effective_status == EffectiveFieldStatus::Active
                    && caller_role >= field.definition.min_view_role
                    && field.definition.public_exposure == PublicExposure::Public
            })
        });
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
    );
    let row: SelectedRow = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json,
                e.publication_status, e.version, e.created_at, e.updated_at, e.published_at,
                e.locale, e.translation_group_id
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
    record.values.retain(|key, _| {
        effective.effective_fields.get(key).is_some_and(|field| {
            field.effective_status == EffectiveFieldStatus::Active
                && caller_role >= field.definition.min_view_role
                && field.definition.public_exposure == PublicExposure::Public
        })
    });
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
async fn query_retained_trash_maintenance(
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

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM content_entries e
          WHERE e.content_type_key = ?
            AND e.trashed_at IS NOT NULL
            AND (? IS NULL OR e.locale = ?)
            AND EXISTS (SELECT 1 FROM site_locales sl WHERE sl.code = e.locale AND sl.deleted_at IS NULL)",
    )
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
    let rows: Vec<RetainedRow> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.locale, e.slug, e.title,
                e.publication_status, e.version, e.created_at, e.updated_at,
                e.published_at, e.trashed_at, e.translation_group_id
           FROM content_entries e
          WHERE e.content_type_key = ?
            AND e.trashed_at IS NOT NULL
            AND (? IS NULL OR e.locale = ?)
            AND EXISTS (SELECT 1 FROM site_locales sl WHERE sl.code = e.locale AND sl.deleted_at IS NULL)
          ORDER BY e.trashed_at DESC, e.id DESC
          LIMIT ? OFFSET ?",
    )
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

#[cfg(test)]
mod c1_public_url_tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn published_private_cpt_remains_in_admin_without_view_url() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        sqlx::query(
            r#"INSERT INTO content_types(key,definition_json,owner_kind)
               VALUES('news','{"key":"news","label":"News","fields":[],"settings":{"public":false,"has_archive":true,"publication":"immediate","supports":["editor","featured_image"]}}','user')"#,
        )
        .execute(&db)
        .await
        .unwrap();
        let id = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
             VALUES('news','hidden-article','Hidden Article','{}','published','vi','eg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();

        let hidden = query(&db, "news", FieldRole::Admin, &EntryTableFilter::default())
            .await
            .unwrap();
        assert_eq!(hidden.total, 1);
        assert_eq!(hidden.status_counts.published, 1);
        assert_eq!(hidden.items[0].id, id);
        assert!(hidden.items[0].public_url.is_none());

        sqlx::query(
            "UPDATE content_types SET definition_json = json_set(definition_json, '$.settings.public', json('true')) WHERE key='news'",
        )
        .execute(&db)
        .await
        .unwrap();
        // This legacy fixture writes directly to SQLite instead of using the
        // normal content-type writer. Rebuild C3 claims in the same serialized
        // transaction the production writer would use after its mutation.
        let mut tx = db.begin().await.unwrap();
        super::super::permalinks::sync_claims_in_tx(&mut tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let visible = query(&db, "news", FieldRole::Admin, &EntryTableFilter::default())
            .await
            .unwrap();
        assert_eq!(
            visible.items[0].public_url.as_deref(),
            Some("/news/hidden-article")
        );
    }
}
