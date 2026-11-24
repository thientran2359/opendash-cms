//! CRUD for saved content entries. Never puts SQL inside HTTP routes.

use std::collections::BTreeMap;

use opendash_core::content::{
    ContentEntryInput, ContentMode, ContentPublication, ContentSupport, ContentTypeDefinition,
    ContentTypeSettings, EffectiveContentSchema, EffectiveFieldStatus, FieldRole, PatchField,
    deserialize_patch_field,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    auth::{self, SessionContext},
    error::{ApiError, ApiResult},
};

/// Pattern is a separate, strictly private canonical entry protocol. Generic
/// content writes (including composite field/layout hooks) may not mutate it.
fn forbid_generic_pattern_write(type_key: &str) -> ApiResult<()> {
    if type_key == "pattern" {
        Err(ApiError::forbidden(
            "Pattern chỉ được sửa qua API riêng tư /api/admin/patterns.",
        ))
    } else {
        Ok(())
    }
}

fn verify_supported_values(
    type_key: &str,
    values: &BTreeMap<String, Value>,
    settings: &ContentTypeSettings,
) -> ApiResult<()> {
    // These are protected Core fields only on Post/Page. A custom CPT may
    // legitimately define unrelated fields named `body` or `featured_image`.
    if !matches!(type_key, "post" | "page") {
        return Ok(());
    }
    if values.contains_key("body")
        && !(settings.supports(ContentSupport::Editor)
            || settings.supports(ContentSupport::Builder))
    {
        return Err(ApiError::unprocessable(
            "Không thể chỉnh sửa 'body' khi tính năng này bị tắt.",
        ));
    }
    if values.contains_key("featured_image") && !settings.supports(ContentSupport::FeaturedImage) {
        return Err(ApiError::unprocessable(
            "Không thể chỉnh sửa 'featured_image' khi tính năng này bị tắt.",
        ));
    }
    Ok(())
}

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
    for id in std::mem::take(&mut refs.any_media_ids) {
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

pub(super) async fn sync_entry_body_refs_in_conn(
    conn: &mut sqlx::SqliteConnection,
    entry_id: i64,
    schema: &EffectiveContentSchema,
    values: &BTreeMap<String, Value>,
    previous_body: Option<&str>,
) -> ApiResult<()> {
    if matches!(schema.base.descriptor.key.as_str(), "pattern" | "section") {
        return Ok(());
    }
    sqlx::query("DELETE FROM entry_pattern_refs WHERE source_entry_id = ?")
        .bind(entry_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM entry_builder_media_refs WHERE entry_id = ?")
        .bind(entry_id)
        .execute(&mut *conn)
        .await?;

    let Some(body_field) = schema.effective_fields.get("body") else {
        return Ok(());
    };
    if body_field.definition.field_type != opendash_core::content::ContentFieldType::RichText {
        return Ok(());
    }
    let Some(raw_body) = values.get("body").and_then(Value::as_str) else {
        return Ok(());
    };
    if !opendash_core::builder_blocks::contains_blocks(raw_body) {
        return Ok(());
    }

    let refs = validated_entry_block_refs_in_conn(conn, raw_body).await?;
    if refs.pattern_ids.len() > 16 {
        return Err(ApiError::unprocessable(
            "Builder exceeds 16 Pattern references.",
        ));
    }
    let prev_patterns = previous_body
        .map(|b| opendash_core::builder_blocks::extract_block_refs_lenient(b).pattern_ids)
        .unwrap_or_default();
    let mut existing_patterns = Vec::new();
    for &pid in &refs.pattern_ids {
        if pid <= 0 || pid == entry_id {
            return Err(ApiError::unprocessable(
                "Pattern tham chiếu không hợp lệ hoặc tự tham chiếu.",
            ));
        }
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM content_entries WHERE id = ? AND content_type_key = 'pattern')",
        )
        .bind(pid)
        .fetch_one(&mut *conn)
        .await?;
        if exists != 0 {
            existing_patterns.push(pid);
        } else if !prev_patterns.contains(&pid) {
            return Err(ApiError::unprocessable(
                "Pattern tham chiếu trong comment block không tồn tại.",
            ));
        }
    }

    let body_is_public = schema
        .base
        .descriptor
        .settings
        .supports(ContentSupport::Builder)
        && body_field.definition.public_exposure == opendash_core::content::PublicExposure::Public
        && body_field.definition.min_view_role == FieldRole::Editor;
    if !body_is_public {
        return Ok(());
    }

    for pid in existing_patterns {
        sqlx::query(
            "INSERT INTO entry_pattern_refs(source_entry_id, target_pattern_id) VALUES (?, ?)",
        )
        .bind(entry_id)
        .bind(pid)
        .execute(&mut *conn)
        .await?;
    }
    for (role, ids) in [
        ("image", &refs.image_ids),
        ("video", &refs.video_ids),
        ("poster", &refs.poster_image_ids),
    ] {
        for media_id in ids {
            sqlx::query(
                "INSERT INTO entry_builder_media_refs(entry_id, media_id, ref_role) VALUES (?, ?, ?)",
            )
            .bind(entry_id)
            .bind(media_id)
            .bind(role)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

fn extract_body_from_values_json(values_json: &str) -> Option<String> {
    serde_json::from_str::<BTreeMap<String, Value>>(values_json)
        .ok()
        .and_then(|map| {
            map.get("body")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
}

async fn current_settings_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
) -> ApiResult<ContentTypeSettings> {
    let schema = crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, type_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    Ok(schema.base.descriptor.settings)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TranslationLinkSummary {
    pub id: i64,
    pub locale: String,
    pub slug: String,
    pub title: String,
    pub publication_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContentEntryRecord {
    pub id: i64,
    pub content_type_key: String,
    pub locale: String,
    pub translation_group_id: String,
    pub slug: String,
    pub title: String,
    pub values: BTreeMap<String, Value>,
    /// Site-owned term IDs. Stable across content slug edits, publication and provider disable.
    pub term_ids: Vec<i64>,
    pub primary_category_term_id: Option<i64>,
    /// Site-owned theme selection; retained even if its provider is withdrawn.
    pub page_layout_key: Option<String>,
    pub custom_template_key: Option<String>,
    pub content_mode: ContentMode,
    /// Server-owned status; legacy CPT entries remain published on Save.
    pub publication_status: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub published_at: Option<String>,
    pub translations: Vec<TranslationLinkSummary>,
}

pub(super) type EntryRow = (
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

pub(super) fn decode_row(row: EntryRow) -> ApiResult<ContentEntryRecord> {
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
    ) = row;
    let values: BTreeMap<String, Value> = serde_json::from_str(&json)
        .map_err(|_| ApiError::internal("Dữ liệu custom fields không hợp lệ."))?;
    Ok(ContentEntryRecord {
        id,
        content_type_key,
        locale: String::new(),
        translation_group_id: String::new(),
        slug,
        title,
        values,
        term_ids: Vec::new(),
        primary_category_term_id: None,
        page_layout_key: None,
        custom_template_key: None,
        content_mode: ContentMode::Editor,
        publication_status,
        version,
        created_at,
        updated_at,
        published_at,
        translations: Vec::new(),
    })
}

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

pub(super) async fn read_term_ids(
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

pub(super) async fn decoded_with_terms(
    tx: &mut Transaction<'_, Sqlite>,
    row: EntryRow,
) -> ApiResult<ContentEntryRecord> {
    let mut record = decode_row(row)?;
    record.term_ids = read_term_ids(tx, record.id).await?;
    let (locale, translation_group_id, layout, custom, mode, primary_category_term_id): (
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<i64>,
    ) = sqlx::query_as(
        "SELECT locale, translation_group_id, page_layout_key, custom_template_key,
                content_mode, primary_category_term_id
           FROM content_entries WHERE id = ?",
    )
    .bind(record.id)
    .fetch_one(&mut **tx)
    .await?;
    let sibling_rows: Vec<(i64, String, String, String, String)> = sqlx::query_as(
        "SELECT e.id, e.locale, e.slug, e.title, e.publication_status
           FROM content_entries e
           JOIN site_locales sl ON sl.code = e.locale
          WHERE e.content_type_key = ?
            AND e.translation_group_id = ?
          ORDER BY sl.is_default DESC, sl.position ASC, sl.code ASC",
    )
    .bind(&record.content_type_key)
    .bind(&translation_group_id)
    .fetch_all(&mut **tx)
    .await?;
    record.locale = locale;
    record.translation_group_id = translation_group_id;
    record.page_layout_key = layout;
    record.custom_template_key = custom;
    record.primary_category_term_id = primary_category_term_id;
    record.content_mode = match mode.as_str() {
        "editor" => ContentMode::Editor,
        "builder" => ContentMode::Builder,
        _ => return Err(ApiError::internal("Chế độ nội dung không hợp lệ.")),
    };
    record.translations = sibling_rows
        .into_iter()
        .map(
            |(id, locale, slug, title, publication_status)| TranslationLinkSummary {
                id,
                locale,
                slug,
                title,
                publication_status,
            },
        )
        .collect();
    Ok(record)
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
               JOIN site_locales sl ON sl.code = e.locale
              WHERE e.content_type_key = ?
                AND e.translation_group_id IN (SELECT value FROM json_each(?))
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

/// Acquire a writer before checking a term's existence or entry CAS. The same
/// SQLite writer reservation is used by taxonomy term deletion, so no race can
/// commit a link to a term that has just been deleted.
#[derive(Debug, Clone)]
pub struct PluginActionFence {
    pub plugin_id: String,
    pub expected_generation: i64,
    pub required_role: String,
}

async fn verify_plugin_action_fence(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &SessionContext,
    fence: &PluginActionFence,
) -> ApiResult<()> {
    let active = crate::storage::plugin::is_plugin_generation_active(
        &mut **tx,
        &fence.plugin_id,
        fence.expected_generation,
    )
    .await?;
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

pub(super) async fn begin_entry_write<'a>(
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

async fn validate_term_ids_for_locale(
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
    Ok(())
}

async fn resolve_entry_write_locale_in_tx(
    conn: &mut sqlx::SqliteConnection,
    requested: Option<&str>,
    existing_locale: Option<&str>,
) -> ApiResult<String> {
    match existing_locale {
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
    }
}

pub async fn get_entry_locale_and_group(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
) -> ApiResult<Option<(String, String)>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT locale, translation_group_id FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

async fn replace_term_ids(tx: &mut Transaction<'_, Sqlite>, id: i64, ids: &[i64]) -> ApiResult<()> {
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

pub(super) async fn current_entry_in_write(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    id: i64,
    expected_version: i64,
) -> ApiResult<()> {
    let version: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    match version {
        None => Err(ApiError::not_found("Không tìm thấy nội dung.")),
        Some(v) if v != expected_version => Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        )),
        Some(_) => Ok(()),
    }
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

pub(super) async fn get_entry_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    id: i64,
) -> ApiResult<Option<ContentEntryRecord>> {
    let row: Option<EntryRow> = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
         FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    let entry = match row {
        Some(row) => Some(decoded_with_terms(tx, row).await?),
        None => None,
    };
    Ok(entry)
}

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
                  WHERE term.id=? AND d.public=1 AND d.provider_status='active'
                    AND d.declaration_state='declared'",
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
             WHERE term.id = ? AND term.kind = ? AND d.public = 1
               AND d.provider_status = 'active'
               AND d.declaration_state = 'declared'",
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
    let mut entries =
        query_entries_filtered(tx, type_key, page_size + 1, offset, filters, true, sort).await?;
    // ADR-0011 stops pagination at page 100 even if more records exist.
    let has_next = page < 100 && entries.len() > page_size as usize;
    entries.truncate(page_size as usize);
    Ok(PublishedListResult::Page(PublishedListPage {
        entries,
        has_next,
    }))
}

async fn list_entries_filtered(
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
        PublishedListSort::IdDesc,
    )
    .await?;
    tx.commit().await?;
    Ok(entries)
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
                 WHERE t.id=? AND d.provider_status='active'
                   AND d.declaration_state='declared'",
            )
            .bind(id)
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

const FILTERED_ENTRIES_CTE_AND_FROM: &str = "WITH RECURSIVE category_scope(id, depth) AS (
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

const FILTERED_ENTRIES_WHERE: &str = "FROM content_entries e
   JOIN content_types t ON t.key = e.content_type_key
  WHERE e.content_type_key = ?
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
    AND (? IS NULL OR e.locale = ?)";

#[derive(sqlx::FromRow)]
struct FullEntryDbRow {
    id: i64,
    content_type_key: String,
    slug: String,
    title: String,
    values_json: String,
    publication_status: String,
    version: i64,
    created_at: String,
    updated_at: String,
    published_at: Option<String>,
    page_layout_key: Option<String>,
    custom_template_key: Option<String>,
    content_mode: String,
    primary_category_term_id: Option<i64>,
    locale: String,
    translation_group_id: String,
}

pub(crate) async fn count_entries_filtered_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    filters: &EntryFilters,
    published_only: bool,
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
    let total = count_entries_filtered_in_tx(tx, type_key, &filters, true).await?;
    let entries = query_entries_filtered(tx, type_key, limit, offset, filters, true, sort).await?;
    Ok((entries, total))
}

/// Shared category/tag AND predicates for the old archives and new list
/// blocks. Only an internal enum can select the ORDER BY direction.
async fn query_entries_filtered(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    limit: i64,
    offset: i64,
    filters: EntryFilters,
    published_only: bool,
    sort: PublishedListSort,
) -> ApiResult<Vec<ContentEntryRecord>> {
    let direction = match sort {
        PublishedListSort::IdDesc => "DESC",
        PublishedListSort::IdAsc => "ASC",
    };
    // Audited dynamic SQL: direction is a closed Rust enum mapped to one of
    // two literal keywords; all user-controlled values remain bound params.
    let statement = format!(
        "{FILTERED_ENTRIES_CTE_AND_FROM}
         SELECT e.id, e.content_type_key, e.slug, e.title, e.values_json, e.publication_status,
                e.version, e.created_at, e.updated_at, e.published_at,
                e.page_layout_key, e.custom_template_key, e.content_mode,
                e.primary_category_term_id, e.locale, e.translation_group_id
          {FILTERED_ENTRIES_WHERE}
          ORDER BY e.id {direction} LIMIT ? OFFSET ?"
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

pub async fn create_entry_with_fence(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &opendash_core::content::ContentTypeDefinition,
    input: &ContentEntryInput,
    fence: Option<&PluginActionFence>,
) -> ApiResult<ContentEntryRecord> {
    forbid_generic_pattern_write(type_key)?;
    let mut tx = begin_entry_write(db, actor).await?;
    let effective =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, type_key)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let settings = &effective.base.descriptor.settings;
    verify_supported_values(type_key, &input.values, settings)?;
    if let Some(f) = fence {
        verify_plugin_action_fence(&mut tx, actor, f).await?;
    }
    let entry_locale =
        resolve_entry_write_locale_in_tx(&mut tx, input.locale.as_deref(), None).await?;
    crate::storage::media::require_existing_references_in_tx(&mut tx, schema, input).await?;
    if let Some(ids) = &input.term_ids {
        validate_term_ids_for_locale(&mut tx, type_key, ids, &entry_locale).await?;
    }
    let group_id = crate::storage::settings::generate_translation_group_id("eg");
    let json = serde_json::to_string(&input.values)?;
    let status = if settings.publication == ContentPublication::DraftPublish {
        "draft"
    } else {
        "published"
    };
    let result = sqlx::query(
        "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, published_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, CASE WHEN ? = 'published' THEN datetime('now') ELSE NULL END)",
    )
    .bind(type_key)
    .bind(&entry_locale)
    .bind(&group_id)
    .bind(&input.slug)
    .bind(&input.title)
    .bind(json)
    .bind(status)
    .bind(status)
    .execute(&mut *tx)
    .await;
    let id = match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            return Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."));
        }
        other => other?.last_insert_rowid(),
    };
    if let Some(ids) = &input.term_ids {
        replace_term_ids(&mut tx, id, ids).await?;
    }
    sync_entry_body_refs_in_conn(&mut *tx, id, &effective, &input.values, None).await?;
    let row: EntryRow = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
         FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let record = decoded_with_terms(&mut tx, row).await?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await?;
    Ok(record)
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

#[allow(clippy::too_many_arguments)]
pub async fn update_entry_with_fence(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &opendash_core::content::ContentTypeDefinition,
    id: i64,
    expected_version: i64,
    input: &ContentEntryInput,
    fence: Option<&PluginActionFence>,
) -> ApiResult<ContentEntryRecord> {
    forbid_generic_pattern_write(type_key)?;
    let mut tx = begin_entry_write(db, actor).await?;
    let effective =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, type_key)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let settings = &effective.base.descriptor.settings;
    verify_supported_values(type_key, &input.values, settings)?;
    if let Some(f) = fence {
        verify_plugin_action_fence(&mut tx, actor, f).await?;
    }
    current_entry_in_write(&mut tx, type_key, id, expected_version).await?;
    let (old_values_json, existing_locale): (String, String) = sqlx::query_as(
        "SELECT values_json, locale FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let entry_locale =
        resolve_entry_write_locale_in_tx(&mut tx, input.locale.as_deref(), Some(&existing_locale))
            .await?;
    let previous_body = extract_body_from_values_json(&old_values_json);
    crate::storage::media::require_existing_references_in_tx(&mut tx, schema, input).await?;
    if let Some(ids) = &input.term_ids {
        validate_term_ids_for_locale(&mut tx, type_key, ids, &entry_locale).await?;
    }
    let json = serde_json::to_string(&input.values)?;
    let result = sqlx::query(
        "UPDATE content_entries
         SET slug = ?, title = ?, values_json = ?, version = version + 1,
             updated_at = datetime('now')
         WHERE content_type_key = ? AND id = ? AND version = ?",
    )
    .bind(&input.slug)
    .bind(&input.title)
    .bind(json)
    .bind(type_key)
    .bind(id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await;
    match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."))
        }
        other => {
            if other?.rows_affected() == 0 {
                return Err(ApiError::precondition_failed(
                    "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
                ));
            }
            if let Some(ids) = &input.term_ids {
                replace_term_ids(&mut tx, id, ids).await?;
            }
            sync_entry_body_refs_in_conn(
                &mut *tx,
                id,
                &effective,
                &input.values,
                previous_body.as_deref(),
            )
            .await?;
            let row: EntryRow = sqlx::query_as(
                "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
                 FROM content_entries WHERE content_type_key = ? AND id = ?",
            )
            .bind(type_key)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            let record = decoded_with_terms(&mut tx, row).await?;
            crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
            tx.commit().await?;
            Ok(record)
        }
    }
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

pub(super) async fn verify_schema_token_in_tx(
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

pub async fn create_entry_with_schema_fence(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &ContentTypeDefinition,
    input: &ContentEntryInput,
    preflight_schema_token: &str,
) -> ApiResult<ContentEntryRecord> {
    forbid_generic_pattern_write(type_key)?;
    let mut tx = begin_entry_write(db, actor).await?;
    let effective = verify_schema_token_in_tx(&mut tx, type_key, preflight_schema_token).await?;
    let settings = &effective.base.descriptor.settings;
    verify_supported_values(type_key, &input.values, settings)?;
    let entry_locale =
        resolve_entry_write_locale_in_tx(&mut tx, input.locale.as_deref(), None).await?;
    crate::storage::media::require_existing_references_in_tx(&mut tx, schema, input).await?;
    if let Some(ids) = &input.term_ids {
        validate_term_ids_for_locale(&mut tx, type_key, ids, &entry_locale).await?;
    }
    let group_id = crate::storage::settings::generate_translation_group_id("eg");
    let json = serde_json::to_string(&input.values)?;
    let status = if settings.publication == ContentPublication::DraftPublish {
        "draft"
    } else {
        "published"
    };
    let result = sqlx::query(
        "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, published_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, CASE WHEN ? = 'published' THEN datetime('now') ELSE NULL END)",
    )
    .bind(type_key)
    .bind(&entry_locale)
    .bind(&group_id)
    .bind(&input.slug)
    .bind(&input.title)
    .bind(json)
    .bind(status)
    .bind(status)
    .execute(&mut *tx)
    .await;
    let id = match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            return Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."));
        }
        other => other?.last_insert_rowid(),
    };
    if let Some(ids) = &input.term_ids {
        replace_term_ids(&mut tx, id, ids).await?;
    }
    sync_entry_body_refs_in_conn(&mut *tx, id, &effective, &input.values, None).await?;
    let row: EntryRow = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
         FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let record = decoded_with_terms(&mut tx, row).await?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await?;
    Ok(record)
}

pub async fn update_entry_with_schema_fence(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    schema: &ContentTypeDefinition,
    id: i64,
    expected_version: i64,
    input: &ContentEntryInput,
    preflight_schema_token: &str,
) -> ApiResult<ContentEntryRecord> {
    forbid_generic_pattern_write(type_key)?;
    let mut tx = begin_entry_write(db, actor).await?;
    let effective = verify_schema_token_in_tx(&mut tx, type_key, preflight_schema_token).await?;
    verify_supported_values(type_key, &input.values, &effective.base.descriptor.settings)?;
    current_entry_in_write(&mut tx, type_key, id, expected_version).await?;
    let (old_values_json, existing_locale): (String, String) = sqlx::query_as(
        "SELECT values_json, locale FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let entry_locale =
        resolve_entry_write_locale_in_tx(&mut tx, input.locale.as_deref(), Some(&existing_locale))
            .await?;
    let previous_body = extract_body_from_values_json(&old_values_json);
    crate::storage::media::require_existing_references_in_tx(&mut tx, schema, input).await?;
    if let Some(ids) = &input.term_ids {
        validate_term_ids_for_locale(&mut tx, type_key, ids, &entry_locale).await?;
    }
    let json = serde_json::to_string(&input.values)?;
    let result = sqlx::query(
        "UPDATE content_entries
         SET slug = ?, title = ?, values_json = ?, version = version + 1,
             updated_at = datetime('now')
         WHERE content_type_key = ? AND id = ? AND version = ?",
    )
    .bind(&input.slug)
    .bind(&input.title)
    .bind(json)
    .bind(type_key)
    .bind(id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await;
    match result {
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."))
        }
        other => {
            if other?.rows_affected() == 0 {
                return Err(ApiError::precondition_failed(
                    "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
                ));
            }
            if let Some(ids) = &input.term_ids {
                replace_term_ids(&mut tx, id, ids).await?;
            }
            sync_entry_body_refs_in_conn(
                &mut *tx,
                id,
                &effective,
                &input.values,
                previous_body.as_deref(),
            )
            .await?;
            let row: EntryRow = sqlx::query_as(
                "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
                 FROM content_entries WHERE content_type_key = ? AND id = ?",
            )
            .bind(type_key)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            let record = decoded_with_terms(&mut tx, row).await?;
            crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
            tx.commit().await?;
            Ok(record)
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositeEntrySaveInput {
    #[serde(default)]
    pub expected_version: Option<i64>,
    #[serde(default)]
    pub expected_schema_token: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub publication_status: Option<String>,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub term_ids: Option<Vec<i64>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub primary_category_term_id: PatchField<i64>,
    #[serde(default)]
    pub auto_unique_slug: bool,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub custom_path: PatchField<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub page_layout_key: PatchField<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub custom_template_key: PatchField<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub content_mode: PatchField<ContentMode>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub seo: PatchField<crate::seo::SeoMetadataInput>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

fn normalize_entry_timestamp(raw: &str, field_name: &str) -> ApiResult<String> {
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
async fn resolve_entry_template_patches_in_tx(
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

fn actor_field_role(role: auth::Role) -> FieldRole {
    match role {
        auth::Role::Admin => FieldRole::Admin,
        auth::Role::Editor => FieldRole::Editor,
    }
}

fn effective_as_ct_def(effective: &EffectiveContentSchema) -> ContentTypeDefinition {
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

pub async fn prepare_composite_entry_save(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: Option<i64>,
    input: &CompositeEntrySaveInput,
) -> ApiResult<CompositePreflightPlan> {
    forbid_generic_pattern_write(type_key)?;
    let client_token = input
        .expected_schema_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ApiError(
                axum::http::StatusCode::PRECONDITION_REQUIRED,
                "missing_schema_token: Thiếu expected_schema_token.".into(),
            )
        })?;

    let effective = crate::storage::content::load_effective_content_schema(db, type_key)
        .await?
        .ok_or_else(|| {
            ApiError::not_found(format!("Không tìm thấy loại nội dung '{type_key}'."))
        })?;

    if client_token != effective.schema_token {
        return Err(ApiError::conflict(format!(
            "schema_version_conflict: schema_token hiện tại là '{}'.",
            effective.schema_token
        )));
    }

    if entry_id.is_none() && input.expected_version.is_some() {
        return Err(ApiError::bad_request(
            "unexpected_version_on_create: Không truyền expected_version khi tạo mới.",
        ));
    }
    if entry_id.is_some() && input.expected_version.is_none() {
        return Err(ApiError(
            axum::http::StatusCode::PRECONDITION_REQUIRED,
            "missing_expected_version: Thiếu expected_version khi cập nhật.".into(),
        ));
    }

    if let Some(status) = &input.publication_status {
        if status != "draft" && status != "published" {
            return Err(ApiError::unprocessable(
                "publication_status chỉ hỗ trợ 'draft' hoặc 'published'.",
            ));
        }
        if effective.base.descriptor.settings.publication == ContentPublication::Immediate
            && status == "draft"
        {
            return Err(ApiError::unprocessable(
                "Loại nội dung lưu ngay không hỗ trợ trạng thái bản nháp.",
            ));
        }
    }
    verify_supported_values(type_key, &input.values, &effective.base.descriptor.settings)?;

    // Check Field ACL BEFORE reading existing entry / comparing values (no value-comparison oracle)
    let caller_role = actor_field_role(actor.user.role);
    for key in input.values.keys() {
        let Some(fe) = effective.effective_fields.get(key) else {
            return Err(ApiError::unprocessable(format!(
                "unknown_field_key: Trường '{key}' không thuộc loại nội dung '{type_key}'."
            )));
        };
        if caller_role < fe.definition.min_edit_role {
            return Err(ApiError::forbidden(format!(
                "field_edit_forbidden: Vai trò hiện tại không có quyền sửa trường '{key}'."
            )));
        }
    }

    // Validate patch syntax before DB merge/tx
    if let PatchField::Set(raw_path) = &input.custom_path {
        let trimmed = raw_path.trim();
        if !trimmed.is_empty() {
            crate::seo::validate_custom_entry_path(trimmed).map_err(ApiError::unprocessable)?;
        }
    }
    if let PatchField::Set(seo_in) = &input.seo {
        crate::seo::validate_seo_input(seo_in)?;
    }
    // A missing provider must never erase a previously saved choice. New
    // assignments are validated against the *currently active* v3 manifest
    // again after the writer lock is acquired at commit time.
    if let PatchField::Set(tid) = input.primary_category_term_id {
        if type_key != "post" {
            return Err(ApiError::bad_request(
                "primary_category_term_id chỉ áp dụng cho bài viết (post).",
            ));
        }
        if tid <= 0 {
            return Err(ApiError::bad_request(
                "primary_category_term_id phải là ID nguyên dương.",
            ));
        }
    }
    if type_key != "page" && !matches!(input.page_layout_key, PatchField::Omitted) {
        return Err(ApiError::bad_request(
            "page_layout_key chỉ áp dụng cho Page.",
        ));
    }
    if type_key == "page" && !matches!(input.custom_template_key, PatchField::Omitted) {
        return Err(ApiError::bad_request(
            "custom_template_key không áp dụng cho Page.",
        ));
    }
    if !matches!(input.content_mode, PatchField::Omitted) {
        // The editor mode is always available as a safe fallback (including
        // after an Admin disables Builder). Entering Builder mode requires an
        // explicit capability, regardless of CPT key.
        if matches!(input.content_mode, PatchField::Set(ContentMode::Builder))
            && !effective
                .base
                .descriptor
                .settings
                .supports(ContentSupport::Builder)
        {
            return Err(ApiError::unprocessable(
                "Builder chưa được bật cho loại nội dung này.",
            ));
        }
        if matches!(input.content_mode, PatchField::Clear) {
            return Err(ApiError::bad_request("content_mode không được null."));
        }
    }
    for patch in [&input.page_layout_key, &input.custom_template_key] {
        if let PatchField::Set(key) = patch {
            if !opendash_core::id::valid_snake_key(key) {
                return Err(ApiError::unprocessable("Tên layout/template không hợp lệ."));
            }
        }
    }

    let (plan_locale, plan_group_id, merged_values) = match entry_id {
        None => {
            let mut conn = db.acquire().await?;
            let loc =
                resolve_entry_write_locale_in_tx(&mut conn, input.locale.as_deref(), None).await?;
            let mut out = BTreeMap::new();
            for (k, fe) in &effective.effective_fields {
                if !input.values.contains_key(k)
                    && fe.effective_status != EffectiveFieldStatus::Withdrawn
                    && let Some(def) = &fe.definition.default_value
                {
                    out.insert(k.clone(), def.clone());
                }
            }
            for (k, v) in &input.values {
                if !v.is_null() {
                    out.insert(k.clone(), v.clone());
                }
            }
            (loc, None, out)
        }
        Some(id) => {
            let expected_ver = input.expected_version.unwrap();
            let existing = get_entry(db, type_key, id)
                .await?
                .ok_or_else(|| ApiError::not_found("Không tìm thấy nội dung."))?;
            if existing.version != expected_ver {
                return Err(ApiError::precondition_failed(
                    "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
                ));
            }
            let mut conn = db.acquire().await?;
            let loc = resolve_entry_write_locale_in_tx(
                &mut conn,
                input.locale.as_deref(),
                Some(&existing.locale),
            )
            .await?;
            let mut out = existing.values;
            for (k, v) in &input.values {
                if v.is_null() {
                    out.remove(k);
                } else {
                    out.insert(k.clone(), v.clone());
                }
            }
            (loc, Some(existing.translation_group_id), out)
        }
    };

    let merged_input = ContentEntryInput {
        title: input.title.clone(),
        slug: input.slug.clone(),
        locale: Some(plan_locale.clone()),
        values: merged_values,
        term_ids: input.term_ids.clone(),
    };
    merged_input
        .validate_against_effective(&effective)
        .map_err(ApiError::unprocessable)?;

    let effective_ct_def = effective_as_ct_def(&effective);
    crate::storage::media::require_existing_references(db, &effective_ct_def, &merged_input)
        .await?;
    if let Some(body) = merged_input.values.get("body").and_then(Value::as_str)
        && effective.effective_fields.get("body").is_some_and(|field| {
            field.definition.field_type == opendash_core::content::ContentFieldType::RichText
        })
        && opendash_core::builder_blocks::contains_blocks(body)
    {
        let mut conn = db.acquire().await?;
        validated_entry_block_refs_in_conn(&mut conn, body).await?;
    }

    Ok(CompositePreflightPlan {
        preflight_schema_token: effective.schema_token.clone(),
        locale: plan_locale,
        translation_group_id: plan_group_id,
        effective,
        effective_ct_def,
        merged_input,
        publication_status: input.publication_status.clone(),
    })
}

async fn resolve_primary_category_patch_in_tx(
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

async fn resolve_effective_rewrite_base_in_tx(
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

pub(super) async fn resolve_auto_unique_slug_in_tx(
    conn: &mut sqlx::SqliteConnection,
    type_key: &str,
    base_slug: &str,
    term_ids: Option<&[i64]>,
    primary_category_term_id: Option<i64>,
    has_custom_path: bool,
) -> ApiResult<String> {
    resolve_auto_unique_slug_for_locale_in_tx(
        conn,
        type_key,
        base_slug,
        term_ids,
        primary_category_term_id,
        has_custom_path,
        None,
    )
    .await
}

pub(crate) async fn resolve_auto_unique_slug_for_locale_in_tx(
    conn: &mut sqlx::SqliteConnection,
    type_key: &str,
    base_slug: &str,
    term_ids: Option<&[i64]>,
    primary_category_term_id: Option<i64>,
    has_custom_path: bool,
    locale: Option<&str>,
) -> ApiResult<String> {
    if !opendash_core::id::valid_kebab_segment_with_limit(base_slug, 64) {
        return Err(ApiError::unprocessable("Slug không hợp lệ."));
    }
    let default_locale = crate::storage::settings::get_default_locale_code_in_conn(conn).await?;
    let target_locale = match locale {
        Some(loc) => loc.to_owned(),
        None => default_locale.clone(),
    };
    let secondary_roots =
        crate::storage::permalinks::load_secondary_locale_roots_in_conn(conn).await?;
    let (post_structure, cpt_bases_json, tax_bases_json): (String, String, String) =
        sqlx::query_as(
            "SELECT post_structure, content_type_bases_json, taxonomy_bases_json
               FROM site_permalink_settings WHERE id = 1",
        )
        .fetch_one(&mut *conn)
        .await?;
    let posts_page_id: Option<i64> = sqlx::query_scalar(
        "SELECT r.posts_page_id
           FROM site_reading_settings r
          WHERE r.locale = ?",
    )
    .bind(&target_locale)
    .fetch_optional(&mut *conn)
    .await?
    .flatten();
    let cpt_bases: BTreeMap<String, Option<String>> =
        serde_json::from_str(&cpt_bases_json).unwrap_or_default();
    let tax_bases: BTreeMap<String, Option<String>> =
        serde_json::from_str(&tax_bases_json).unwrap_or_default();

    let is_public_type = if matches!(type_key, "pattern" | "section") {
        false
    } else {
        sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(json_extract(definition_json, '$.settings.public'), 1)
               FROM content_types WHERE key = ? AND provider_status = 'active'",
        )
        .bind(type_key)
        .fetch_optional(&mut *conn)
        .await?
            == Some(1)
    };

    let cat_slug = if type_key == "post" && post_structure.contains("%category%") {
        let cat_public: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1 FROM taxonomy_definitions
                WHERE key = 'category' AND public = 1
                  AND provider_status = 'active' AND declaration_state = 'declared'
             )",
        )
        .fetch_one(&mut *conn)
        .await?;
        let cat_eligible = if cat_public == 1 {
            crate::storage::content::load_effective_taxonomies_in_tx(&mut *conn, "post")
                .await?
                .iter()
                .any(|k| k == "category")
        } else {
            false
        };
        let resolved_cat = if !cat_eligible {
            "uncategorized".to_owned()
        } else {
            let primary_slug: Option<String> = match primary_category_term_id {
                Some(pid) => {
                    sqlx::query_scalar(
                        "SELECT slug FROM taxonomy_terms WHERE id = ? AND kind = 'category' AND locale = ?",
                    )
                    .bind(pid)
                    .bind(&target_locale)
                    .fetch_optional(&mut *conn)
                    .await?
                }
                None => None,
            };
            match primary_slug {
                Some(slug) => slug,
                None => match term_ids {
                    Some(ids) if !ids.is_empty() => {
                        let ids_json = serde_json::to_string(ids)?;
                        let min_slug: Option<String> = sqlx::query_scalar(
                            "SELECT slug FROM taxonomy_terms
                              WHERE kind = 'category'
                                AND locale = ?
                                AND id IN (SELECT value FROM json_each(?))
                              ORDER BY id ASC LIMIT 1",
                        )
                        .bind(&target_locale)
                        .bind(ids_json)
                        .fetch_optional(&mut *conn)
                        .await?;
                        min_slug.unwrap_or_else(|| "uncategorized".to_owned())
                    }
                    _ => "uncategorized".to_owned(),
                },
            }
        };
        if post_structure.starts_with("/%category%/") {
            if crate::seo::is_reserved_system_root(&resolved_cat)
                || (target_locale == default_locale
                    && crate::storage::permalinks::is_secondary_locale_root(
                        &resolved_cat,
                        &secondary_roots,
                    ))
            {
                return Err(ApiError::conflict(format!(
                    "permalink_category_root_collision: slug '{resolved_cat}' trùng gốc hệ thống."
                )));
            }
            if posts_page_id.is_none() && resolved_cat == "posts" {
                return Err(ApiError::conflict(
                    "permalink_category_root_collision: slug 'posts' trùng trang lưu trữ mặc định.",
                ));
            }
            let public_cpts: Vec<String> = sqlx::query_scalar(
                "SELECT key FROM content_types
                  WHERE key NOT IN ('pattern', 'section', 'page', 'post')
                    AND provider_status = 'active'
                    AND COALESCE(json_extract(definition_json, '$.settings.public'), 1) = 1",
            )
            .fetch_all(&mut *conn)
            .await?;
            for key in public_cpts {
                let base = resolve_effective_rewrite_base_in_tx(
                    &mut *conn,
                    "content_type",
                    &key,
                    &cpt_bases,
                )
                .await?;
                if base.trim_start_matches('/').split('/').next().unwrap_or("") == resolved_cat {
                    return Err(ApiError::conflict(format!(
                        "permalink_category_root_collision: slug '{resolved_cat}' trùng CPT '{key}'."
                    )));
                }
            }
            let public_taxes: Vec<String> = sqlx::query_scalar(
                "SELECT key FROM taxonomy_definitions
                  WHERE public = 1 AND provider_status = 'active' AND declaration_state = 'declared'",
            )
            .fetch_all(&mut *conn)
            .await?;
            for key in public_taxes {
                let base =
                    resolve_effective_rewrite_base_in_tx(&mut *conn, "taxonomy", &key, &tax_bases)
                        .await?;
                if base.trim_start_matches('/').split('/').next().unwrap_or("") == resolved_cat {
                    return Err(ApiError::conflict(format!(
                        "permalink_category_root_collision: slug '{resolved_cat}' trùng taxonomy '{key}'."
                    )));
                }
            }
        }
        resolved_cat
    } else {
        "uncategorized".to_owned()
    };

    let cpt_base = if !matches!(type_key, "post" | "page") {
        Some(
            resolve_effective_rewrite_base_in_tx(&mut *conn, "content_type", type_key, &cpt_bases)
                .await?,
        )
    } else {
        None
    };
    let uses_post_id = type_key == "post" && post_structure.contains("%post_id%");
    let check_public_path = !has_custom_path && is_public_type && !uses_post_id;
    let check_root_reserved =
        type_key == "page" || (type_key == "post" && post_structure == "/%postname%");

    for n in 1..=100usize {
        let candidate = if n == 1 {
            base_slug.to_owned()
        } else {
            let suffix = format!("-{n}");
            let max_prefix = 64usize.saturating_sub(suffix.len());
            let prefix = base_slug[..base_slug.len().min(max_prefix)].trim_end_matches('-');
            if prefix.is_empty() {
                continue;
            }
            format!("{prefix}{suffix}")
        };
        let slug_taken: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM content_entries WHERE content_type_key = ? AND locale = ? AND slug = ?)",
        )
        .bind(type_key)
        .bind(&target_locale)
        .bind(&candidate)
        .fetch_one(&mut *conn)
        .await?;
        if slug_taken != 0 {
            continue;
        }
        if check_public_path {
            if check_root_reserved
                && (crate::seo::is_reserved_system_root(&candidate)
                    || (target_locale == default_locale
                        && crate::storage::permalinks::is_secondary_locale_root(
                            &candidate,
                            &secondary_roots,
                        )))
            {
                continue;
            }
            let candidate_path = match type_key {
                "page" => format!("/{candidate}"),
                "post" => post_structure
                    .replace("%category%", &cat_slug)
                    .replace("%postname%", &candidate),
                _ => format!("/{}/{candidate}", cpt_base.as_deref().unwrap_or(type_key)),
            };
            let effective_path = crate::storage::permalinks::prefix_for_locale(
                &target_locale,
                &default_locale,
                &candidate_path,
            );
            let path_taken: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM permalink_route_claims WHERE path = ?)
                     OR EXISTS(SELECT 1 FROM permalink_route_redirects WHERE source_path = ? AND origin != 'locale_swap')
                     OR EXISTS(SELECT 1 FROM entry_redirects WHERE locale = ? AND source_path = ?)",
            )
            .bind(&effective_path)
            .bind(&effective_path)
            .bind(&target_locale)
            .bind(&candidate_path)
            .fetch_one(&mut *conn)
            .await?;
            if path_taken != 0 {
                continue;
            }
        }
        return Ok(candidate);
    }

    Err(ApiError::conflict(
        "Không thể tự động tạo slug duy nhất trong giới hạn 100 lần thử.",
    ))
}

pub async fn commit_composite_entry_save(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: Option<i64>,
    input: &CompositeEntrySaveInput,
    plan: &CompositePreflightPlan,
) -> ApiResult<CompositeEntrySaveResult> {
    let mut tx = begin_entry_write(db, actor).await?;
    let effective =
        verify_schema_token_in_tx(&mut tx, type_key, &plan.preflight_schema_token).await?;
    verify_supported_values(type_key, &input.values, &effective.base.descriptor.settings)?;
    let existing_locale: Option<String> = if let Some(id) = entry_id {
        let expected_ver = input
            .expected_version
            .ok_or_else(|| ApiError::bad_request("Thiếu phiên bản nội dung."))?;
        current_entry_in_write(&mut tx, type_key, id, expected_ver).await?;
        let loc: String = sqlx::query_scalar(
            "SELECT locale FROM content_entries WHERE content_type_key = ? AND id = ?",
        )
        .bind(type_key)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        Some(loc)
    } else {
        None
    };
    let entry_locale = resolve_entry_write_locale_in_tx(
        &mut tx,
        input.locale.as_deref(),
        existing_locale.as_deref(),
    )
    .await?;
    let requested_mode = match &input.content_mode {
        PatchField::Omitted => None,
        PatchField::Clear => return Err(ApiError::bad_request("content_mode không được null.")),
        PatchField::Set(mode) => {
            if *mode == ContentMode::Builder
                && !effective
                    .base
                    .descriptor
                    .settings
                    .supports(ContentSupport::Builder)
            {
                return Err(ApiError::unprocessable(
                    "Builder chưa được bật cho loại nội dung này.",
                ));
            }
            Some(mode.as_str())
        }
    };
    let is_draft_capable =
        effective.base.descriptor.settings.publication == ContentPublication::DraftPublish;
    if !is_draft_capable && plan.publication_status.as_deref() == Some("draft") {
        return Err(ApiError::unprocessable(
            "Loại nội dung lưu ngay không hỗ trợ trạng thái bản nháp.",
        ));
    }

    crate::storage::media::require_existing_references_in_tx(
        &mut tx,
        &plan.effective_ct_def,
        &plan.merged_input,
    )
    .await?;
    if let Some(ids) = &plan.merged_input.term_ids {
        validate_term_ids_for_locale(&mut tx, type_key, ids, &entry_locale).await?;
    }
    let effective_term_ids: Vec<i64> = match &plan.merged_input.term_ids {
        Some(ids) => ids.clone(),
        None => match entry_id {
            Some(id) => read_term_ids(&mut tx, id).await?,
            None => Vec::new(),
        },
    };
    let next_primary_category = resolve_primary_category_patch_in_tx(
        &mut tx,
        type_key,
        entry_id,
        &input.primary_category_term_id,
        plan.merged_input.term_ids.is_some(),
        &effective_term_ids,
    )
    .await?;

    let resolved_slug = if entry_id.is_none() && input.auto_unique_slug {
        let has_custom_path =
            matches!(&input.custom_path, PatchField::Set(raw) if !raw.trim().is_empty());
        resolve_auto_unique_slug_for_locale_in_tx(
            &mut tx,
            type_key,
            &plan.merged_input.slug,
            plan.merged_input.term_ids.as_deref(),
            next_primary_category,
            has_custom_path,
            Some(&entry_locale),
        )
        .await?
    } else {
        plan.merged_input.slug.clone()
    };

    let values_json = serde_json::to_string(&plan.merged_input.values)?;
    let (page_layout_key, custom_template_key) =
        resolve_entry_template_patches_in_tx(&mut tx, type_key, entry_id, input).await?;
    let custom_created_at = match input.created_at.as_deref() {
        Some(raw) if !raw.trim().is_empty() => Some(normalize_entry_timestamp(raw, "created_at")?),
        _ => None,
    };
    let custom_updated_at = match input.updated_at.as_deref() {
        Some(raw) if !raw.trim().is_empty() => Some(normalize_entry_timestamp(raw, "updated_at")?),
        _ => None,
    };

    let (saved_id, previous_body, was_ever_published) = match entry_id {
        None => {
            // Migration 0008 deliberately rejects a directly published INSERT
            // for the built-in types. Keep that security trigger intact and
            // transition to published *within this same writer transaction*.
            // No intermediate draft can be observed or committed.
            let group_id = crate::storage::settings::generate_translation_group_id("eg");
            let status_clause = if is_draft_capable {
                "draft"
            } else {
                "published"
            };
            let res = sqlx::query(
                "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, content_mode, page_layout_key, custom_template_key, primary_category_term_id, created_at, updated_at, published_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, COALESCE(?, 'editor'), ?, ?, ?, COALESCE(?, datetime('now')), COALESCE(?, ?, datetime('now')), CASE WHEN ? = 'published' THEN datetime('now') ELSE NULL END)",
            )
            .bind(type_key)
            .bind(&entry_locale)
            .bind(&group_id)
            .bind(&resolved_slug)
            .bind(&plan.merged_input.title)
            .bind(&values_json)
            .bind(status_clause)
            .bind(requested_mode)
            .bind(page_layout_key.as_deref())
            .bind(custom_template_key.as_deref())
            .bind(next_primary_category)
            .bind(custom_created_at.as_deref())
            .bind(custom_updated_at.as_deref())
            .bind(custom_created_at.as_deref())
            .bind(status_clause)
            .execute(&mut *tx)
            .await;
            let new_id = match res {
                Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                    return Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."));
                }
                other => other?.last_insert_rowid(),
            };
            if is_draft_capable && plan.publication_status.as_deref() == Some("published") {
                // Initial create remains v1; this is one logical atomic Save,
                // not a separate publication/CAS transition.
                sqlx::query(
                    "UPDATE content_entries
                        SET publication_status = 'published',
                            published_at = COALESCE(published_at, datetime('now'))
                      WHERE id = ? AND content_type_key = ? AND publication_status = 'draft'",
                )
                .bind(new_id)
                .bind(type_key)
                .execute(&mut *tx)
                .await?;
            }
            (new_id, None, false)
        }
        Some(id) => {
            let expected_ver = input.expected_version.unwrap();
            current_entry_in_write(&mut tx, type_key, id, expected_ver).await?;
            let (old_values_json, was_ever_published_int): (String, i64) = sqlx::query_as(
                "SELECT values_json, has_been_published FROM content_entries WHERE content_type_key = ? AND id = ?",
            )
            .bind(type_key)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            let prev_body = extract_body_from_values_json(&old_values_json);
            let res = sqlx::query(
                "UPDATE content_entries
                 SET slug = ?, title = ?, values_json = ?,
                     publication_status = COALESCE(?, publication_status),
                     published_at = CASE
                         WHEN COALESCE(?, publication_status) = 'published'
                         THEN COALESCE(published_at, datetime('now'))
                         ELSE published_at
                     END,
                     page_layout_key = ?, custom_template_key = ?,
                     primary_category_term_id = ?,
                     content_mode = COALESCE(?, content_mode),
                     created_at = COALESCE(?, created_at),
                     version = version + 1,
                     updated_at = COALESCE(?, datetime('now'))
                 WHERE content_type_key = ? AND id = ? AND version = ?",
            )
            .bind(&resolved_slug)
            .bind(&plan.merged_input.title)
            .bind(&values_json)
            .bind(plan.publication_status.as_deref())
            .bind(plan.publication_status.as_deref())
            .bind(page_layout_key.as_deref())
            .bind(custom_template_key.as_deref())
            .bind(next_primary_category)
            .bind(requested_mode)
            .bind(custom_created_at.as_deref())
            .bind(custom_updated_at.as_deref())
            .bind(type_key)
            .bind(id)
            .bind(expected_ver)
            .execute(&mut *tx)
            .await;
            match res {
                Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                    return Err(ApiError::conflict("Slug đã tồn tại trong loại nội dung."));
                }
                other => {
                    if other?.rows_affected() == 0 {
                        return Err(ApiError::precondition_failed(
                            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
                        ));
                    }
                }
            }
            (id, prev_body, was_ever_published_int != 0)
        }
    };

    if let Some(ids) = &plan.merged_input.term_ids {
        replace_term_ids(&mut tx, saved_id, ids).await?;
    }
    sync_entry_body_refs_in_conn(
        &mut *tx,
        saved_id,
        &effective,
        &plan.merged_input.values,
        previous_body.as_deref(),
    )
    .await?;

    let row: EntryRow = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
         FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(saved_id)
    .fetch_one(&mut *tx)
    .await?;
    let mut record = decoded_with_terms(&mut tx, row).await?;

    let custom_path = crate::storage::seo::apply_entry_route_patch_in_tx(
        &mut tx,
        saved_id,
        &input.custom_path,
        was_ever_published,
    )
    .await?;
    let seo = crate::storage::seo::apply_entry_seo_patch_in_tx(
        &mut tx,
        saved_id,
        record.version,
        &input.seo,
    )
    .await?;
    crate::storage::seo::sync_entry_seo_media_ref_in_tx(&mut tx, saved_id).await?;

    if !was_ever_published {
        sqlx::query(
            "DELETE FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?",
        )
        .bind(saved_id.to_string())
        .execute(&mut *tx)
        .await?;
    }
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await?;
    filter_entry_values_for_role(&mut record, &effective, actor_field_role(actor.user.role));

    Ok(CompositeEntrySaveResult {
        entry: record,
        custom_path,
        seo,
        schema_token: effective.schema_token,
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateEntryTranslationInput {
    pub target_locale: String,
    pub slug: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub auto_unique_slug: bool,
}

pub struct EntryTranslationPlan {
    pub target_locale: String,
    pub translation_group_id: String,
    pub slug: String,
    pub title: String,
    pub values: BTreeMap<String, Value>,
    pub term_ids: Vec<i64>,
    pub primary_category_term_id: Option<i64>,
    pub page_layout_key: Option<String>,
    pub custom_template_key: Option<String>,
    pub content_mode: ContentMode,
}

async fn remap_term_ids_to_locale_in_conn(
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

async fn remap_list_term_ids_in_conn(
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

fn collect_builder_list_term_ids(doc: &opendash_core::builder::BuilderDocument) -> Vec<i64> {
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

fn apply_builder_list_term_remap(
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

pub async fn prepare_entry_translation(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    source_id: i64,
    expected_source_version: i64,
    input: &CreateEntryTranslationInput,
) -> ApiResult<EntryTranslationPlan> {
    if matches!(type_key, "pattern" | "section") {
        return Err(ApiError::forbidden(
            "Không hỗ trợ tạo bản dịch trực tiếp cho Pattern/Section.",
        ));
    }
    let source = get_entry(db, type_key, source_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy bài gốc để dịch."))?;
    if source.version != expected_source_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung gốc đã thay đổi. Hãy tải lại trước khi tạo bản dịch.",
        ));
    }
    let target_locale = input.target_locale.trim().to_ascii_lowercase();
    if target_locale.is_empty() || !opendash_core::id::valid_lowercase_locale_code(&target_locale) {
        return Err(ApiError::unprocessable("Mã ngôn ngữ đích không hợp lệ."));
    }
    if target_locale == source.locale {
        return Err(ApiError::unprocessable(
            "Ngôn ngữ đích phải khác ngôn ngữ của bài gốc.",
        ));
    }
    let mut conn = db.acquire().await?;
    let locale_exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM site_locales WHERE code = ?)")
            .bind(&target_locale)
            .fetch_one(&mut *conn)
            .await?;
    if locale_exists == 0 {
        return Err(ApiError::unprocessable(
            "Ngôn ngữ đích không tồn tại trong site_locales.",
        ));
    }
    let dup_group: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM content_entries
            WHERE content_type_key = ? AND translation_group_id = ? AND locale = ?
         )",
    )
    .bind(type_key)
    .bind(&source.translation_group_id)
    .bind(&target_locale)
    .fetch_one(&mut *conn)
    .await?;
    if dup_group != 0 {
        return Err(ApiError::conflict(
            "Nhóm dịch đã có bản dịch ở ngôn ngữ đích.",
        ));
    }
    if !opendash_core::id::valid_kebab_segment_with_limit(&input.slug, 64) {
        return Err(ApiError::bad_request(
            "Slug bản dịch phải là kebab-case, tối đa 64 ký tự.",
        ));
    }
    if !input.auto_unique_slug {
        let slug_taken: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1 FROM content_entries
                WHERE content_type_key = ? AND locale = ? AND slug = ?
             )",
        )
        .bind(type_key)
        .bind(&target_locale)
        .bind(&input.slug)
        .fetch_one(&mut *conn)
        .await?;
        if slug_taken != 0 {
            return Err(ApiError::conflict("Slug đã tồn tại ở ngôn ngữ đích."));
        }
    }

    let effective =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *conn, type_key)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let caller_role = actor_field_role(actor.user.role);
    let mut copied_values = BTreeMap::new();
    for (k, v) in &source.values {
        if let Some(fe) = effective.effective_fields.get(k)
            && (caller_role < fe.definition.min_view_role
                || caller_role < fe.definition.min_edit_role)
        {
            continue;
        }
        copied_values.insert(k.clone(), v.clone());
    }
    for (k, fe) in &effective.effective_fields {
        if fe.effective_status != EffectiveFieldStatus::Withdrawn
            && fe.definition.required
            && (caller_role < fe.definition.min_view_role
                || caller_role < fe.definition.min_edit_role)
        {
            if let Some(def) = &fe.definition.default_value {
                copied_values
                    .entry(k.clone())
                    .or_insert_with(|| def.clone());
            } else {
                return Err(ApiError::forbidden(format!(
                    "field_copy_forbidden: Vai trò hiện tại không có quyền sao chép trường bắt buộc '{k}'."
                )));
            }
        }
    }

    if let Some(raw_body) = copied_values
        .get("body")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        && opendash_core::builder_blocks::contains_blocks(&raw_body)
    {
        let mut parsed = opendash_core::builder_blocks::parse_blocks_lenient_or_recover(&raw_body);
        let list_ids = collect_builder_list_term_ids(&parsed.document);
        let list_remap = remap_list_term_ids_in_conn(&mut *conn, &list_ids, &target_locale).await?;
        if apply_builder_list_term_remap(&mut parsed.document, &list_remap) {
            let serialized = opendash_core::builder_blocks::serialize_builder_document_to_blocks(
                &parsed.document,
            )
            .map_err(|_| {
                ApiError::internal("Không thể đóng gói lại Builder block sau ánh xạ term.")
            })?;
            copied_values.insert("body".to_owned(), Value::String(serialized));
        }
    }

    let term_map =
        remap_term_ids_to_locale_in_conn(&mut *conn, type_key, &source.term_ids, &target_locale)
            .await?;
    let mut remapped_term_ids = Vec::new();
    let mut seen_terms = std::collections::HashSet::new();
    for old_id in &source.term_ids {
        if let Some(&new_id) = term_map.get(old_id)
            && seen_terms.insert(new_id)
        {
            remapped_term_ids.push(new_id);
        }
    }
    let remapped_primary = source
        .primary_category_term_id
        .and_then(|old_id| term_map.get(&old_id).copied())
        .filter(|id| remapped_term_ids.contains(id));

    let title = match input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(t) => t.to_owned(),
        None => source.title.clone(),
    };
    let check_input = ContentEntryInput {
        title: title.clone(),
        slug: input.slug.clone(),
        locale: Some(target_locale.clone()),
        values: copied_values.clone(),
        term_ids: Some(remapped_term_ids.clone()),
    };
    check_input
        .validate_against_effective(&effective)
        .map_err(ApiError::unprocessable)?;

    Ok(EntryTranslationPlan {
        target_locale,
        translation_group_id: source.translation_group_id,
        slug: input.slug.clone(),
        title,
        values: copied_values,
        term_ids: remapped_term_ids,
        primary_category_term_id: remapped_primary,
        page_layout_key: source.page_layout_key,
        custom_template_key: source.custom_template_key,
        content_mode: source.content_mode,
    })
}

pub async fn commit_entry_translation(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    source_id: i64,
    expected_source_version: i64,
    input: &CreateEntryTranslationInput,
    plan: &EntryTranslationPlan,
) -> ApiResult<ContentEntryRecord> {
    if matches!(type_key, "pattern" | "section") {
        return Err(ApiError::forbidden(
            "Không hỗ trợ tạo bản dịch trực tiếp cho Pattern/Section.",
        ));
    }
    let mut tx = begin_entry_write(db, actor).await?;
    current_entry_in_write(&mut tx, type_key, source_id, expected_source_version).await?;
    let effective =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, type_key)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let locale_exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM site_locales WHERE code = ?)")
            .bind(&plan.target_locale)
            .fetch_one(&mut *tx)
            .await?;
    if locale_exists == 0 {
        return Err(ApiError::unprocessable(
            "Ngôn ngữ đích không tồn tại trong site_locales.",
        ));
    }
    let dup_group: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM content_entries
            WHERE content_type_key = ? AND translation_group_id = ? AND locale = ?
         )",
    )
    .bind(type_key)
    .bind(&plan.translation_group_id)
    .bind(&plan.target_locale)
    .fetch_one(&mut *tx)
    .await?;
    if dup_group != 0 {
        return Err(ApiError::conflict(
            "Nhóm dịch đã có bản dịch ở ngôn ngữ đích.",
        ));
    }

    let resolved_slug = if input.auto_unique_slug {
        resolve_auto_unique_slug_for_locale_in_tx(
            &mut tx,
            type_key,
            &plan.slug,
            Some(&plan.term_ids),
            plan.primary_category_term_id,
            false,
            Some(&plan.target_locale),
        )
        .await?
    } else {
        plan.slug.clone()
    };

    let status =
        if effective.base.descriptor.settings.publication == ContentPublication::DraftPublish {
            "draft"
        } else {
            "published"
        };
    let values_json = serde_json::to_string(&plan.values)?;
    let res = sqlx::query(
        "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, published_at, content_mode, page_layout_key, custom_template_key, primary_category_term_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, CASE WHEN ? = 'published' THEN datetime('now') ELSE NULL END, ?, ?, ?, ?)",
    )
    .bind(type_key)
    .bind(&plan.target_locale)
    .bind(&plan.translation_group_id)
    .bind(&resolved_slug)
    .bind(&plan.title)
    .bind(&values_json)
    .bind(status)
    .bind(status)
    .bind(plan.content_mode.as_str())
    .bind(plan.page_layout_key.as_deref())
    .bind(plan.custom_template_key.as_deref())
    .bind(plan.primary_category_term_id)
    .execute(&mut *tx)
    .await;
    let new_id = match res {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::conflict("Slug đã tồn tại ở ngôn ngữ đích."));
        }
        other => other?.last_insert_rowid(),
    };

    if !plan.term_ids.is_empty() {
        validate_term_ids_for_locale(&mut tx, type_key, &plan.term_ids, &plan.target_locale)
            .await?;
        replace_term_ids(&mut tx, new_id, &plan.term_ids).await?;
    }

    let source_seo: Option<(String, String, Option<String>, i64)> = sqlx::query_as(
        "SELECT title, description, og_image_id, noindex FROM entry_seo WHERE entry_id = ?",
    )
    .bind(source_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((seo_title, seo_desc, og_image_id, noindex)) = source_seo {
        sqlx::query(
            "INSERT INTO entry_seo (entry_id, title, description, og_image_id, canonical_url, noindex)
             VALUES (?, ?, ?, ?, NULL, ?)",
        )
        .bind(new_id)
        .bind(&seo_title)
        .bind(&seo_desc)
        .bind(og_image_id.as_deref())
        .bind(noindex)
        .execute(&mut *tx)
        .await?;
        crate::storage::seo::sync_entry_seo_media_ref_in_tx(&mut tx, new_id).await?;
    }

    sync_entry_body_refs_in_conn(&mut *tx, new_id, &effective, &plan.values, None).await?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;

    let row: EntryRow = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
         FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(new_id)
    .fetch_one(&mut *tx)
    .await?;
    let mut record = decoded_with_terms(&mut tx, row).await?;
    tx.commit().await?;
    filter_entry_values_for_role(&mut record, &effective, actor_field_role(actor.user.role));
    Ok(record)
}

pub async fn delete_entry(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
    expected_version: i64,
) -> ApiResult<()> {
    forbid_generic_pattern_write(type_key)?;
    let mut tx = db.begin().await?;
    let result = sqlx::query(
        "DELETE FROM content_entries
         WHERE content_type_key = ? AND id = ? AND version = ?",
    )
    .bind(type_key)
    .bind(id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Err(missing_or_stale(db, type_key, id).await?);
    }
    if type_key == "page" {
        // Preserve deleted Reading IDs and advance their CAS token atomically
        // with removal and permalink-claim rebuilding.
        crate::storage::settings::bump_reading_version_on_deleted_entry_in_tx(&mut tx, id).await?;
    }
    sqlx::query(
        "DELETE FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id = ?",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    super::plugin_projections::delete_entry_projections_in_tx(&mut tx, id).await?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Version-CAS transition. Save/update never touches publication_status.
pub async fn set_publication_status(
    db: &SqlitePool,
    type_key: &str,
    id: i64,
    expected_version: i64,
    status: &str,
) -> ApiResult<ContentEntryRecord> {
    forbid_generic_pattern_write(type_key)?;
    if status != "draft" && status != "published" {
        return Err(ApiError::bad_request("Trạng thái xuất bản không hợp lệ."));
    }
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE bootstrap SET initialized = initialized WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let settings = current_settings_in_tx(&mut tx, type_key).await?;
    if settings.publication != ContentPublication::DraftPublish {
        return Err(ApiError::bad_request(
            "Loại nội dung này sử dụng chế độ lưu ngay.",
        ));
    }
    let result = sqlx::query(
        "UPDATE content_entries
         SET publication_status = ?,
             published_at = CASE WHEN ? = 'published' THEN COALESCE(published_at, datetime('now')) ELSE published_at END,
             version = version + 1,
             updated_at = datetime('now')
         WHERE content_type_key = ? AND id = ? AND version = ?",
    )
    .bind(status)
    .bind(status)
    .bind(type_key)
    .bind(id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Err(missing_or_stale(db, type_key, id).await?);
    }
    if status == "draft" {
        super::plugin_projections::delete_entry_projections_in_tx(&mut tx, id).await?;
    }
    crate::storage::seo::sync_entry_seo_media_ref_in_tx(&mut tx, id).await?;
    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    tx.commit().await?;
    get_entry(db, type_key, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải trạng thái xuất bản."))
}

async fn missing_or_stale(db: &SqlitePool, type_key: &str, id: i64) -> ApiResult<ApiError> {
    if get_entry(db, type_key, id).await?.is_some() {
        Ok(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        ))
    } else {
        Ok(ApiError::not_found("Không tìm thấy nội dung."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn list_test_entry(
        db: &SqlitePool,
        type_key: &str,
        slug: &str,
        published: bool,
        term_ids: &[i64],
    ) -> i64 {
        let id = sqlx::query(
            "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
             VALUES(?,?,'Safe','{}','draft','vi','eg-' || lower(hex(randomblob(8))))",
        )
        .bind(type_key)
        .bind(slug)
        .execute(db)
        .await
        .unwrap()
        .last_insert_rowid();
        if published {
            sqlx::query("UPDATE content_entries SET publication_status = 'published' WHERE id = ?")
                .bind(id)
                .execute(db)
                .await
                .unwrap();
        }
        for &term in term_ids {
            sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES (?,?)")
                .bind(id)
                .bind(term)
                .execute(db)
                .await
                .unwrap();
        }
        id
    }

    fn expect_list_page(result: PublishedListResult) -> PublishedListPage {
        match result {
            PublishedListResult::Page(page) => page,
            PublishedListResult::MissingConfig(reason) => {
                panic!("unexpected missing list configuration: {reason:?}")
            }
        }
    }

    #[tokio::test]
    async fn builder_published_list_filters_before_paging_for_post_page_cpt() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        // Migration 0008 seeds Post/Page; only the custom CPT is new here.
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind)
             VALUES('news',json_object('key','news','fields',json_array()),'user')",
        )
        .execute(&db)
        .await
        .unwrap();
        // Page stays unassociated by default; this fixture explicitly opts in
        // Page and the custom CPT, as a real C2 Admin configuration would.
        sqlx::query(
            "INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json)
                     VALUES ('page','[\"category\",\"tag\"]'),('news','[\"category\",\"tag\"]')",
        )
        .execute(&db)
        .await
        .unwrap();
        let root = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Root','list-root','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let child = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,parent_id,locale,translation_group_id)
             VALUES('category','Child','list-child',?,'vi','tg-' || lower(hex(randomblob(8))))",
        )
        .bind(root)
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let tag = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Tag','list-tag','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let other_tag = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Other','other-tag','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();

        let first = list_test_entry(&db, "post", "p1", true, &[root, child, tag]).await;
        list_test_entry(&db, "post", "draft", false, &[child, tag]).await;
        let second = list_test_entry(&db, "post", "p2", true, &[root, tag]).await;
        list_test_entry(&db, "post", "wrong-tag", true, &[child, other_tag]).await;
        let third = list_test_entry(&db, "post", "p3", true, &[child, tag]).await;
        let page_id = list_test_entry(&db, "page", "page1", true, &[child, tag]).await;
        let cpt_id = list_test_entry(&db, "news", "news1", true, &[child, tag]).await;
        list_test_entry(&db, "news", "draft-news", false, &[child, tag]).await;

        let filters = EntryFilters {
            category_id: Some(root),
            tag_id: Some(tag),
            include_descendants: true,
            ..EntryFilters::default()
        };
        for (sort, expected) in [
            (PublishedListSort::IdDesc, [third, second, first]),
            (PublishedListSort::IdAsc, [first, second, third]),
        ] {
            for (i, &id) in expected.iter().enumerate() {
                let page = expect_list_page(
                    list_published_page(&db, "post", i as i64 + 1, 1, sort, filters.clone())
                        .await
                        .unwrap(),
                );
                assert_eq!(page.entries.iter().map(|e| e.id).collect::<Vec<_>>(), [id]);
                assert_eq!(page.has_next, i < 2);
            }
        }

        let exact = expect_list_page(
            list_published_page(
                &db,
                "post",
                1,
                20,
                PublishedListSort::IdAsc,
                EntryFilters {
                    include_descendants: false,
                    ..filters.clone()
                },
            )
            .await
            .unwrap(),
        );
        assert_eq!(
            exact.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            [first, second],
            "exact category only; a duplicated direct+descendant link cannot duplicate rows"
        );
        assert!(!exact.has_next);
        for (type_key, id) in [("page", page_id), ("news", cpt_id)] {
            let result = expect_list_page(
                list_published_page(
                    &db,
                    type_key,
                    1,
                    20,
                    PublishedListSort::IdDesc,
                    filters.clone(),
                )
                .await
                .unwrap(),
            );
            assert_eq!(
                result.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
                [id]
            );
            assert!(!result.has_next);
        }

        let past_end = expect_list_page(
            list_published_page(
                &db,
                "post",
                100,
                1,
                PublishedListSort::IdAsc,
                filters.clone(),
            )
            .await
            .unwrap(),
        );
        assert!(past_end.entries.is_empty());
        assert!(!past_end.has_next);

        // Existing public archive contract stays descending with offset paging.
        let old = list_published_filtered(&db, "post", 2, 1, filters)
            .await
            .unwrap();
        assert_eq!(
            old.iter().map(|e| e.id).collect::<Vec<_>>(),
            [second, first]
        );
    }

    #[tokio::test]
    async fn builder_list_rejects_bad_bounds_and_distinguishes_missing_config() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind)
             VALUES('news',json_object('key','news','fields',json_array()),'user')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO content_types(key,definition_json,owner_kind)
             VALUES('deleted-cpt',json_object('key','deleted-cpt','fields',json_array()),'user')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("DELETE FROM content_types WHERE key = 'deleted-cpt'")
            .execute(&db)
            .await
            .unwrap();
        let category = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','List','list-category','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let tag = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','List','list-tag','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        for (page, size) in [(0, 6), (101, 6), (-1, 6), (1, 0), (1, 21), (1, -1)] {
            let error = list_published_page(
                &db,
                "news",
                page,
                size,
                PublishedListSort::IdDesc,
                EntryFilters::default(),
            )
            .await
            .unwrap_err();
            assert_eq!(error.0, axum::http::StatusCode::BAD_REQUEST);
        }
        for filters in [
            EntryFilters {
                include_descendants: true,
                ..Default::default()
            },
            EntryFilters {
                category_id: Some(-1),
                ..Default::default()
            },
        ] {
            let error = list_published_page(&db, "news", 1, 6, PublishedListSort::IdDesc, filters)
                .await
                .unwrap_err();
            assert_eq!(error.0, axum::http::StatusCode::BAD_REQUEST);
        }
        for (type_key, filters, expected) in [
            (
                "deleted-cpt",
                EntryFilters::default(),
                PublishedListMissing::ContentType,
            ),
            (
                "news",
                EntryFilters {
                    category_id: Some(tag),
                    ..Default::default()
                },
                PublishedListMissing::Category,
            ),
            (
                "news",
                EntryFilters {
                    tag_id: Some(category),
                    ..Default::default()
                },
                PublishedListMissing::Tag,
            ),
            (
                "news",
                EntryFilters {
                    category_id: Some(999999),
                    ..Default::default()
                },
                PublishedListMissing::Category,
            ),
            (
                "news",
                EntryFilters {
                    tag_id: Some(999999),
                    ..Default::default()
                },
                PublishedListMissing::Tag,
            ),
        ] {
            let result =
                list_published_page(&db, type_key, 1, 6, PublishedListSort::IdDesc, filters)
                    .await
                    .unwrap();
            assert!(
                matches!(result, PublishedListResult::MissingConfig(reason) if reason == expected)
            );
        }

        // Deleting a previously valid, unused term must not turn into a 500.
        sqlx::query("DELETE FROM taxonomy_terms WHERE id = ?")
            .bind(tag)
            .execute(&db)
            .await
            .unwrap();
        let result = list_published_page(
            &db,
            "news",
            1,
            6,
            PublishedListSort::IdDesc,
            EntryFilters {
                tag_id: Some(tag),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            result,
            PublishedListResult::MissingConfig(PublishedListMissing::Tag)
        ));

        // Even when more than 100 pages exist, page 100 is the last linkable page.
        for i in 0..101 {
            list_test_entry(&db, "news", &format!("entry-{i}"), true, &[]).await;
        }
        let final_page = expect_list_page(
            list_published_page(
                &db,
                "news",
                100,
                1,
                PublishedListSort::IdDesc,
                EntryFilters::default(),
            )
            .await
            .unwrap(),
        );
        assert_eq!(final_page.entries.len(), 1);
        assert!(!final_page.has_next);

        db.close().await;
        let error = list_published_page(
            &db,
            "news",
            1,
            6,
            PublishedListSort::IdDesc,
            EntryFilters::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.0, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn builder_generic_taxonomy_list_hides_drafts_and_survives_withdrawn_references() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO taxonomy_definitions(key,label,hierarchical,public,owner_kind)
             VALUES('genre','Genre',1,1,'user')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO content_type_taxonomy_overrides(content_type_key,taxonomies_json)
             VALUES('post','[\"genre\"]')",
        )
        .execute(&db)
        .await
        .unwrap();
        let root = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('genre','Root','genre-root','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let child = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,parent_id,locale,translation_group_id)
             VALUES('genre','Child','genre-child',?,'vi','tg-' || lower(hex(randomblob(8))))",
        )
        .bind(root)
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let published = list_test_entry(&db, "post", "genre-public", true, &[child]).await;
        list_test_entry(&db, "post", "genre-draft", false, &[child]).await;
        let filters = EntryFilters {
            taxonomy_key: Some("genre".into()),
            term_id: Some(root),
            include_descendants: true,
            ..EntryFilters::default()
        };
        let page = expect_list_page(
            list_published_page(
                &db,
                "post",
                1,
                1,
                PublishedListSort::IdDesc,
                filters.clone(),
            )
            .await
            .unwrap(),
        );
        assert_eq!(
            page.entries
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            [published]
        );
        assert!(!page.has_next, "drafts must not create a next page");

        // A malformed query remains a 400, even though a withdrawn persisted
        // Builder reference is treated as unavailable configuration.
        let malformed = EntryFilters {
            tag_id: Some(child),
            ..filters.clone()
        };
        let error = list_published_page(&db, "post", 1, 6, PublishedListSort::IdDesc, malformed)
            .await
            .unwrap_err();
        assert_eq!(error.0, axum::http::StatusCode::BAD_REQUEST);

        for change in [
            "UPDATE taxonomy_definitions SET public=0 WHERE key='genre'",
            "UPDATE taxonomy_definitions SET provider_status='provider_inactive',public=1 WHERE key='genre'",
            "UPDATE taxonomy_definitions SET declaration_state='withdrawn',
                provider_status='active' WHERE key='genre'",
        ] {
            sqlx::query(change).execute(&db).await.unwrap();
            let result = list_published_page(
                &db,
                "post",
                1,
                6,
                PublishedListSort::IdDesc,
                filters.clone(),
            )
            .await
            .unwrap();
            assert!(matches!(
                result,
                PublishedListResult::MissingConfig(PublishedListMissing::Taxonomy)
            ));
        }
        sqlx::query(
            "UPDATE taxonomy_definitions
                SET public=1, provider_status='active',declaration_state='declared'
              WHERE key='genre'",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE content_type_taxonomy_overrides SET taxonomies_json='[]'
             WHERE content_type_key='post'",
        )
        .execute(&db)
        .await
        .unwrap();
        let withdrawn = list_published_page(
            &db,
            "post",
            1,
            6,
            PublishedListSort::IdDesc,
            filters.clone(),
        )
        .await
        .unwrap();
        assert!(matches!(
            withdrawn,
            PublishedListResult::MissingConfig(PublishedListMissing::Taxonomy)
        ));
        sqlx::query(
            "UPDATE content_type_taxonomy_overrides SET taxonomies_json='[\"genre\"]'
             WHERE content_type_key='post'",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("DELETE FROM entry_terms WHERE term_id=?")
            .bind(child)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM taxonomy_terms WHERE id=?")
            .bind(child)
            .execute(&db)
            .await
            .unwrap();
        let deleted = list_published_page(
            &db,
            "post",
            1,
            6,
            PublishedListSort::IdDesc,
            EntryFilters {
                term_id: Some(child),
                ..filters.clone()
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            deleted,
            PublishedListResult::MissingConfig(PublishedListMissing::Taxonomy)
        ));
        // An Admin can switch a taxonomy to flat once every remaining term
        // is a root. An older saved Builder node with descendants=true must
        // not turn the entire public homepage into HTTP 400.
        sqlx::query("UPDATE taxonomy_definitions SET hierarchical=0 WHERE key='genre'")
            .execute(&db)
            .await
            .unwrap();
        let incompatible = list_published_page(
            &db,
            "post",
            1,
            6,
            PublishedListSort::IdDesc,
            filters.clone(),
        )
        .await
        .unwrap();
        assert!(matches!(
            incompatible,
            PublishedListResult::MissingConfig(PublishedListMissing::Taxonomy)
        ));
        let mut tx = db.begin().await.unwrap();
        let query_error = validate_filter_in_tx(&mut tx, "post", &filters, true)
            .await
            .unwrap_err();
        assert_eq!(
            query_error.0,
            axum::http::StatusCode::BAD_REQUEST,
            "user-provided filters remain strict after the definition change"
        );
    }

    #[tokio::test]
    async fn published_taxonomy_filter_excludes_drafts_before_pagination() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let root = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Root','root','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let child = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,parent_id,locale,translation_group_id) VALUES('category','Child','child',?,'vi','tg-' || lower(hex(randomblob(8))))",
        )
        .bind(root)
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let tag =
            sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','Tag','tag','vi','tg-' || lower(hex(randomblob(8))))")
                .execute(&db)
                .await
                .unwrap()
                .last_insert_rowid();

        let mut ids = vec![];
        for (slug, is_published) in [
            ("published-first", true),
            ("draft-newer", false),
            ("published-last", true),
        ] {
            let id = sqlx::query(
                "INSERT INTO content_entries(content_type_key,slug,title,values_json,publication_status,locale,translation_group_id)
                 VALUES('post',?,'Safe','{}','draft','vi','eg-' || lower(hex(randomblob(8))))",
            )
            .bind(slug)
            .execute(&db)
            .await
            .unwrap()
            .last_insert_rowid();
            if is_published {
                sqlx::query("UPDATE content_entries SET publication_status='published' WHERE id=?")
                    .bind(id)
                    .execute(&db)
                    .await
                    .unwrap();
            }
            // More than one matching category link to one entry must never
            // inflate public results or displace a later page.
            for term in [root, child, tag] {
                sqlx::query("INSERT INTO entry_terms(entry_id,term_id) VALUES (?,?)")
                    .bind(id)
                    .bind(term)
                    .execute(&db)
                    .await
                    .unwrap();
            }
            ids.push(id);
        }
        let filters = EntryFilters {
            category_id: Some(root),
            tag_id: Some(tag),
            include_descendants: true,
            ..EntryFilters::default()
        };
        let first = list_published_filtered(&db, "post", 1, 0, filters.clone())
            .await
            .unwrap();
        let second = list_published_filtered(&db, "post", 1, 1, filters.clone())
            .await
            .unwrap();
        let last = list_published_filtered(&db, "post", 1, 2, filters.clone())
            .await
            .unwrap();
        assert_eq!(first.iter().map(|e| e.id).collect::<Vec<_>>(), [ids[2]]);
        assert_eq!(second.iter().map(|e| e.id).collect::<Vec<_>>(), [ids[0]]);
        assert!(last.is_empty());
        assert_eq!(first[0].term_ids, [root, child, tag]);
        let admin = list_entries(&db, "post", 10, 0, filters.clone())
            .await
            .unwrap();
        assert_eq!(
            admin.len(),
            3,
            "Admin sees unpublished draft before pagination"
        );

        // Reparented category links retain stable IDs and public filters must
        // follow the current tree, not stale slug/path snapshots.
        let other = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Other','other','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        for id in &ids {
            sqlx::query("DELETE FROM entry_terms WHERE entry_id=? AND term_id=?")
                .bind(id)
                .bind(root)
                .execute(&db)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE taxonomy_terms SET parent_id=? WHERE id=?")
            .bind(other)
            .bind(child)
            .execute(&db)
            .await
            .unwrap();
        assert!(
            list_published_filtered(&db, "post", 10, 0, filters.clone())
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            list_published_filtered(
                &db,
                "post",
                10,
                0,
                EntryFilters {
                    category_id: Some(other),
                    ..filters
                }
            )
            .await
            .unwrap()
            .len(),
            2
        );
    }

    async fn s02_admin_context(db: &SqlitePool) -> SessionContext {
        let user_id = sqlx::query(
            "INSERT INTO users(username, password_hash, role) VALUES('s02_admin', 'unused', 'admin')",
        )
        .execute(db)
        .await
        .unwrap()
        .last_insert_rowid();
        let expires_at = auth::current_time_secs() + 3600;
        sqlx::query(
            "INSERT INTO sessions(token_hash, user_id, csrf_token, expires_at)
             VALUES('s02-hash', ?, 's02-csrf', ?)",
        )
        .bind(user_id)
        .bind(expires_at)
        .execute(db)
        .await
        .unwrap();
        SessionContext {
            user: auth::AuthUser {
                id: user_id,
                username: "s02_admin".to_owned(),
                role: auth::Role::Admin,
            },
            token_hash: "s02-hash".to_owned(),
            csrf_token: "s02-csrf".to_owned(),
        }
    }

    async fn composite_save(
        db: &SqlitePool,
        actor: &SessionContext,
        type_key: &str,
        entry_id: Option<i64>,
        input: CompositeEntrySaveInput,
    ) -> ApiResult<CompositeEntrySaveResult> {
        let plan = prepare_composite_entry_save(db, actor, type_key, entry_id, &input).await?;
        commit_composite_entry_save(db, actor, type_key, entry_id, &input, &plan).await
    }

    fn base_composite_input(
        schema_token: &str,
        slug: &str,
        expected_version: Option<i64>,
    ) -> CompositeEntrySaveInput {
        CompositeEntrySaveInput {
            expected_version,
            expected_schema_token: Some(schema_token.to_owned()),
            locale: None,
            title: format!("Title {slug}"),
            slug: slug.to_owned(),
            publication_status: Some("published".to_owned()),
            values: BTreeMap::new(),
            term_ids: None,
            primary_category_term_id: PatchField::Omitted,
            auto_unique_slug: false,
            custom_path: PatchField::Omitted,
            page_layout_key: PatchField::Omitted,
            custom_template_key: PatchField::Omitted,
            content_mode: PatchField::Omitted,
            seo: PatchField::Omitted,
            created_at: None,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn s02_auto_unique_slug_resolves_collisions_reserved_roots_and_64b_limit() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let actor = s02_admin_context(&db).await;

        let post_schema = crate::storage::content::load_effective_content_schema(&db, "post")
            .await
            .unwrap()
            .unwrap();
        let page_schema = crate::storage::content::load_effective_content_schema(&db, "page")
            .await
            .unwrap()
            .unwrap();
        let post_tok = post_schema.schema_token.clone();
        let page_tok = page_schema.schema_token.clone();

        // 1. Repeated posts with auto_unique_slug: true produce foo, foo-2, foo-3.
        for expected in ["foo", "foo-2", "foo-3"] {
            let mut input = base_composite_input(&post_tok, "foo", None);
            input.auto_unique_slug = true;
            let saved = composite_save(&db, &actor, "post", None, input)
                .await
                .unwrap();
            assert_eq!(saved.entry.slug, expected);
        }

        // 2. Page colliding with Post claims /foo, /foo-2, /foo-3 becomes foo-4;
        //    with an explicit custom_path, Page can reuse slug "foo".
        let mut page_foo = base_composite_input(&page_tok, "foo", None);
        page_foo.auto_unique_slug = true;
        let saved_page_foo = composite_save(&db, &actor, "page", None, page_foo)
            .await
            .unwrap();
        assert_eq!(saved_page_foo.entry.slug, "foo-4");

        let mut page_with_custom = base_composite_input(&page_tok, "foo", None);
        page_with_custom.auto_unique_slug = true;
        page_with_custom.custom_path = PatchField::Set("/custom-holder".to_owned());
        let saved_holder = composite_save(&db, &actor, "page", None, page_with_custom)
            .await
            .unwrap();
        assert_eq!(saved_holder.entry.slug, "foo");

        // 3. Reserved roots (media), default archive (/posts), and CPT archive (/project) disambiguate cleanly.
        sqlx::query(
            "INSERT INTO content_types(key, definition_json, owner_kind)
             VALUES('project', json_object('key','project','label','Project','fields',json('[]'),'settings',json_object('public',json('true'),'has_archive',json('true'))), 'user')",
        )
        .execute(&db)
        .await
        .unwrap();
        let mut tx = db.begin().await.unwrap();
        crate::storage::permalinks::sync_claims_in_tx(&mut tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        for (raw_slug, expected) in [
            ("media", "media-2"),
            ("posts", "posts-2"),
            ("project", "project-2"),
        ] {
            let mut input = base_composite_input(&page_tok, raw_slug, None);
            input.auto_unique_slug = true;
            let saved = composite_save(&db, &actor, "page", None, input)
                .await
                .unwrap();
            assert_eq!(saved.entry.slug, expected);
        }

        // 4. 64-byte boundary trims trailing '-' before appending '-2'.
        let long_base = format!("{}-xy", "a".repeat(61));
        assert_eq!(long_base.len(), 64);
        let mut long1 = base_composite_input(&post_tok, &long_base, None);
        long1.auto_unique_slug = true;
        let s1 = composite_save(&db, &actor, "post", None, long1)
            .await
            .unwrap();
        assert_eq!(s1.entry.slug, long_base);
        let mut long2 = base_composite_input(&post_tok, &long_base, None);
        long2.auto_unique_slug = true;
        let s2 = composite_save(&db, &actor, "post", None, long2)
            .await
            .unwrap();
        assert_eq!(s2.entry.slug, format!("{}-2", "a".repeat(61)));
        assert!(s2.entry.slug.len() <= 64);

        // 5. Opt-in false & non-composite POST /entries still return 409 Conflict.
        let strict_err = composite_save(
            &db,
            &actor,
            "post",
            None,
            base_composite_input(&post_tok, "foo", None),
        )
        .await
        .unwrap_err();
        assert_eq!(strict_err.0, axum::http::StatusCode::CONFLICT);

        let non_comp_err = create_entry(
            &db,
            &actor,
            "post",
            &post_schema.base.descriptor,
            &ContentEntryInput {
                title: "Dup".into(),
                slug: "foo".into(),
                locale: None,
                values: BTreeMap::new(),
                term_ids: None,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(non_comp_err.0, axum::http::StatusCode::CONFLICT);

        // 6. With post_structure = /%category%/%postname%: post can share slug with a root Page,
        //    while same-CPT slug still increments and forbidden %category% prefix rejects 409 immediately.
        let perm = crate::storage::permalinks::get_settings(&db).await.unwrap();
        crate::storage::permalinks::put_settings(
            &db,
            perm.version,
            &crate::storage::permalinks::PermalinkSettingsInput {
                post_structure: "/%category%/%postname%".to_owned(),
                content_type_bases: perm.content_type_bases,
                taxonomy_bases: perm.taxonomy_bases,
            },
        )
        .await
        .unwrap();
        let cat_news = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','News','news','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();

        let page_root = composite_save(
            &db,
            &actor,
            "page",
            None,
            base_composite_input(&page_tok, "root-story", None),
        )
        .await
        .unwrap();
        assert_eq!(page_root.entry.slug, "root-story");

        let mut post_in_news = base_composite_input(&post_tok, "root-story", None);
        post_in_news.auto_unique_slug = true;
        post_in_news.term_ids = Some(vec![cat_news]);
        let post_saved = composite_save(&db, &actor, "post", None, post_in_news)
            .await
            .unwrap();
        assert_eq!(post_saved.entry.slug, "root-story");

        // Synthetically inject a colliding category term to exercise the immediate 409 pre-check
        // in resolve_auto_unique_slug_in_tx when post_structure starts with /%category%/.
        let cat_project = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Project','project','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let mut forbidden_cat = base_composite_input(&post_tok, "bad-cat-post", None);
        forbidden_cat.auto_unique_slug = true;
        forbidden_cat.term_ids = Some(vec![cat_project]);
        let cat_err = composite_save(&db, &actor, "post", None, forbidden_cat)
            .await
            .unwrap_err();
        assert_eq!(cat_err.0, axum::http::StatusCode::CONFLICT);
        assert!(cat_err.1.contains("permalink_category_root_collision"));
    }

    #[tokio::test]
    async fn s02_primary_category_validation_autoclear_and_redirect_history() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let actor = s02_admin_context(&db).await;

        let perm = crate::storage::permalinks::get_settings(&db).await.unwrap();
        crate::storage::permalinks::put_settings(
            &db,
            perm.version,
            &crate::storage::permalinks::PermalinkSettingsInput {
                post_structure: "/%category%/%postname%".to_owned(),
                content_type_bases: perm.content_type_bases,
                taxonomy_bases: perm.taxonomy_bases,
            },
        )
        .await
        .unwrap();

        let cat_a = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Alpha','alpha','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let cat_b = sqlx::query(
            "INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('category','Beta','beta','vi','tg-' || lower(hex(randomblob(8))))",
        )
        .execute(&db)
        .await
        .unwrap()
        .last_insert_rowid();
        let tag_id =
            sqlx::query("INSERT INTO taxonomy_terms(kind,name,slug,locale,translation_group_id) VALUES('tag','T1','t1','vi','tg-' || lower(hex(randomblob(8))))")
                .execute(&db)
                .await
                .unwrap()
                .last_insert_rowid();

        let post_schema = crate::storage::content::load_effective_content_schema(&db, "post")
            .await
            .unwrap()
            .unwrap();
        let page_schema = crate::storage::content::load_effective_content_schema(&db, "page")
            .await
            .unwrap()
            .unwrap();
        let post_tok = post_schema.schema_token.clone();
        let page_tok = page_schema.schema_token.clone();

        // Validation rejections -> 400 Bad Request
        let mut page_primary = base_composite_input(&page_tok, "page-pc", None);
        page_primary.primary_category_term_id = PatchField::Set(cat_a);
        assert_eq!(
            composite_save(&db, &actor, "page", None, page_primary)
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::BAD_REQUEST
        );

        let mut zero_primary = base_composite_input(&post_tok, "zero-pc", None);
        zero_primary.primary_category_term_id = PatchField::Set(0);
        assert_eq!(
            composite_save(&db, &actor, "post", None, zero_primary)
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::BAD_REQUEST
        );

        let mut tag_primary = base_composite_input(&post_tok, "tag-pc", None);
        tag_primary.term_ids = Some(vec![tag_id]);
        tag_primary.primary_category_term_id = PatchField::Set(tag_id);
        assert_eq!(
            composite_save(&db, &actor, "post", None, tag_primary)
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::BAD_REQUEST
        );

        let mut unassigned_primary = base_composite_input(&post_tok, "unassigned-pc", None);
        unassigned_primary.term_ids = Some(vec![cat_a]);
        unassigned_primary.primary_category_term_id = PatchField::Set(cat_b);
        assert_eq!(
            composite_save(&db, &actor, "post", None, unassigned_primary)
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::BAD_REQUEST
        );

        // Draft post changing primary category must NOT leave historical 301 redirect.
        let mut draft_in = base_composite_input(&post_tok, "pc-post", None);
        draft_in.publication_status = Some("draft".to_owned());
        draft_in.term_ids = Some(vec![cat_a, cat_b]);
        draft_in.primary_category_term_id = PatchField::Set(cat_b);
        let draft1 = composite_save(&db, &actor, "post", None, draft_in)
            .await
            .unwrap();
        assert_eq!(draft1.entry.primary_category_term_id, Some(cat_b));

        let mut draft_up = base_composite_input(&post_tok, "pc-post", Some(draft1.entry.version));
        draft_up.publication_status = Some("published".to_owned());
        draft_up.primary_category_term_id = PatchField::Set(cat_a);
        let published1 = composite_save(&db, &actor, "post", Some(draft1.entry.id), draft_up)
            .await
            .unwrap();
        assert_eq!(published1.entry.primary_category_term_id, Some(cat_a));
        assert_eq!(
            crate::storage::permalinks::resolve_public_path(&db, "/beta/pc-post")
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            crate::storage::permalinks::resolve_public_path(&db, "/alpha/pc-post")
                .await
                .unwrap()
                .unwrap()
                .canonical_path,
            "/alpha/pc-post"
        );

        // Published post updates primary_category_term_id WITHOUT resending term_ids -> 301 created.
        let mut switch_primary =
            base_composite_input(&post_tok, "pc-post", Some(published1.entry.version));
        switch_primary.publication_status = None;
        switch_primary.term_ids = None;
        switch_primary.primary_category_term_id = PatchField::Set(cat_b);
        let published2 = composite_save(
            &db,
            &actor,
            "post",
            Some(published1.entry.id),
            switch_primary,
        )
        .await
        .unwrap();
        assert_eq!(published2.entry.primary_category_term_id, Some(cat_b));
        let alpha_redir = crate::storage::permalinks::resolve_public_path(&db, "/alpha/pc-post")
            .await
            .unwrap()
            .unwrap();
        assert!(alpha_redir.redirect);
        assert_eq!(alpha_redir.canonical_path, "/beta/pc-post");

        // Removing cat_b while primary_category_term_id is Omitted auto-clears primary_category_term_id.
        let mut strip_cat_b =
            base_composite_input(&post_tok, "pc-post", Some(published2.entry.version));
        strip_cat_b.publication_status = None;
        strip_cat_b.term_ids = Some(vec![cat_a]);
        strip_cat_b.primary_category_term_id = PatchField::Omitted;
        let cleared = composite_save(&db, &actor, "post", Some(published2.entry.id), strip_cat_b)
            .await
            .unwrap();
        assert_eq!(cleared.entry.primary_category_term_id, None);
        assert_eq!(
            crate::storage::permalinks::resolve_public_path(&db, "/alpha/pc-post")
                .await
                .unwrap()
                .unwrap()
                .canonical_path,
            "/alpha/pc-post"
        );

        // Re-set primary = cat_b, then verify non-composite update_entry removing cat_b also clears it.
        let mut restore_cat_b =
            base_composite_input(&post_tok, "pc-post", Some(cleared.entry.version));
        restore_cat_b.publication_status = None;
        restore_cat_b.term_ids = Some(vec![cat_a, cat_b]);
        restore_cat_b.primary_category_term_id = PatchField::Set(cat_b);
        let restored = composite_save(
            &db,
            &actor,
            "post",
            Some(published2.entry.id),
            restore_cat_b,
        )
        .await
        .unwrap();
        assert_eq!(restored.entry.primary_category_term_id, Some(cat_b));

        let non_comp_updated = update_entry(
            &db,
            &actor,
            "post",
            &post_schema.base.descriptor,
            restored.entry.id,
            restored.entry.version,
            &ContentEntryInput {
                title: restored.entry.title.clone(),
                slug: restored.entry.slug.clone(),
                locale: None,
                values: restored.entry.values.clone(),
                term_ids: Some(vec![cat_a]),
            },
        )
        .await
        .unwrap();
        assert_eq!(non_comp_updated.primary_category_term_id, None);
    }
}
