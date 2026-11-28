use std::collections::{BTreeMap, BTreeSet};

use opendash_core::builder::BuilderDocument;
use opendash_core::content::{EffectiveContentSchema, FieldRole};
use opendash_core::id::valid_snake_key;
use opendash_core::revisions::{
    CanonicalRevisionPayload, CanonicalRevisionSeo, RevisionSource, canonical_json_value,
};
use serde_json::Value;
use sqlx::{Row, SqliteConnection};

use crate::error::{ApiError, ApiResult};

use super::capture::parse_content_mode;
use super::types::{RevisionSummaryDto, is_field_editable};

pub(super) struct LiveEntryState {
    pub(super) id: i64,
    pub(super) content_type_key: String,
    pub(super) locale: String,
    pub(super) translation_group_id: String,
    pub(super) slug: String,
    pub(super) publication_status: String,
    pub(super) version: i64,
    pub(super) updated_at: String,
    pub(super) trashed_at: Option<String>,
    pub(super) effective: EffectiveContentSchema,
    pub(super) payload: CanonicalRevisionPayload,
}

pub(super) struct LoadedRevisionRecord {
    pub(super) summary: RevisionSummaryDto,
    pub(super) slug: String,
    pub(super) payload: CanonicalRevisionPayload,
}

pub(super) fn parse_revision_source(raw: &str) -> ApiResult<RevisionSource> {
    match raw {
        "create" => Ok(RevisionSource::Create),
        "save" => Ok(RevisionSource::Save),
        "builder_save" => Ok(RevisionSource::BuilderSave),
        "builder_reset" => Ok(RevisionSource::BuilderReset),
        "quick_edit" => Ok(RevisionSource::QuickEdit),
        "seo_save" => Ok(RevisionSource::SeoSave),
        "pattern_save" => Ok(RevisionSource::PatternSave),
        "plugin_save" => Ok(RevisionSource::PluginSave),
        "restore" => Ok(RevisionSource::Restore),
        "import" => Ok(RevisionSource::Import),
        "baseline" => Ok(RevisionSource::Baseline),
        _ => Err(ApiError::internal("Nguồn revision không hợp lệ trong DB.")),
    }
}

pub(super) fn check_not_trashed(trashed_at: Option<&str>) -> ApiResult<()> {
    if trashed_at.is_some() {
        return Err(ApiError::conflict(
            "entry_trashed: Nội dung đang ở trong Thùng rác và không thể thao tác lịch sử phiên bản hoặc bản lưu tự động.",
        ));
    }
    Ok(())
}

pub(super) async fn require_writable_cpt_and_locale_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    locale: &str,
) -> ApiResult<()> {
    let ct_row: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM content_types WHERE key = ?",
    )
    .bind(type_key)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((provider_status, declaration_state)) = ct_row else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    };
    if provider_status != "active" || declaration_state != "declared" {
        return Err(ApiError::conflict(
            "provider_inactive: Loại nội dung hiện không hoạt động hoặc đã bị thu hồi.",
        ));
    }
    let locale_row: Option<(i64, Option<String>)> =
        sqlx::query_as("SELECT is_default, deleted_at FROM site_locales WHERE code = ?")
            .bind(locale)
            .fetch_optional(&mut *conn)
            .await?;
    let Some((is_default, deleted_at)) = locale_row else {
        return Err(ApiError::conflict(
            "locale_deleted: Ngôn ngữ của nội dung không còn tồn tại trong cấu hình.",
        ));
    };
    if deleted_at.is_some() {
        return Err(ApiError::conflict(
            "locale_deleted: Ngôn ngữ của nội dung đã bị xóa; hãy dùng công cụ dọn ngôn ngữ mồ côi.",
        ));
    }
    if is_default == 0 {
        let multilingual_enabled: i64 =
            sqlx::query_scalar("SELECT multilingual_enabled FROM site_locales_state WHERE id = 1")
                .fetch_one(&mut *conn)
                .await?;
        if multilingual_enabled == 0 {
            return Err(ApiError::conflict(
                "locale_parked: Ngôn ngữ phụ đang bị tạm dừng; hãy bật lại chế độ đa ngôn ngữ trước khi thao tác.",
            ));
        }
    }
    Ok(())
}

pub(super) async fn load_live_entry_state_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<LiveEntryState> {
    let Some(effective) =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *conn, type_key).await?
    else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    };

    let row = sqlx::query(
        "SELECT id, content_type_key, locale, translation_group_id, slug, title,
                values_json, publication_status, content_mode, page_layout_key,
                custom_template_key, primary_category_term_id, parent_id, menu_order,
                version, updated_at, trashed_at
           FROM content_entries
          WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;

    let Some(row) = row else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };

    let id: i64 = row.try_get("id")?;
    let content_type_key: String = row.try_get("content_type_key")?;
    let locale: String = row.try_get("locale")?;
    let translation_group_id: String = row.try_get("translation_group_id")?;
    let slug: String = row.try_get("slug")?;
    let title: String = row.try_get("title")?;
    let values_raw: String = row.try_get("values_json")?;
    let publication_status: String = row.try_get("publication_status")?;
    let content_mode = parse_content_mode(&row.try_get::<String, _>("content_mode")?)?;
    let page_layout_key: Option<String> = row
        .try_get::<Option<String>, _>("page_layout_key")?
        .filter(|s| !s.is_empty());
    let custom_template_key: Option<String> = row
        .try_get::<Option<String>, _>("custom_template_key")?
        .filter(|s| !s.is_empty());
    let primary_category_term_id: Option<i64> = row.try_get("primary_category_term_id")?;
    let parent_id: Option<i64> = row.try_get("parent_id")?;
    let menu_order: i32 = row.try_get("menu_order")?;
    let version: i64 = row.try_get("version")?;
    let updated_at: String = row.try_get("updated_at")?;
    let trashed_at: Option<String> = row.try_get("trashed_at")?;

    let parsed_values: BTreeMap<String, Value> = serde_json::from_str(&values_raw)
        .map_err(|_| ApiError::internal("Dữ liệu custom fields không hợp lệ."))?;
    let mut values = BTreeMap::new();
    for (k, v) in parsed_values {
        values.insert(k, canonical_json_value(&v));
    }

    let builder_document_json = if type_key == "pattern" {
        let doc_raw: Option<String> = sqlx::query_scalar(
            "SELECT document_json FROM entry_builder_documents WHERE entry_id = ?",
        )
        .bind(entry_id)
        .fetch_optional(&mut *conn)
        .await?;
        match doc_raw {
            Some(raw) => {
                let parsed: Value = serde_json::from_str(&raw)
                    .map_err(|_| ApiError::conflict("Dữ liệu Builder JSON không hợp lệ."))?;
                Some(serde_json::to_string(&canonical_json_value(&parsed))?)
            }
            None => None,
        }
    } else {
        None
    };

    let mut term_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT term_id FROM entry_terms WHERE entry_id = ? ORDER BY term_id ASC",
    )
    .bind(entry_id)
    .fetch_all(&mut *conn)
    .await?;
    term_ids.sort_unstable();
    term_ids.dedup();

    let seo_row = sqlx::query(
        "SELECT title, description, og_image_id, canonical_url, noindex
           FROM entry_seo
          WHERE entry_id = ?",
    )
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;
    let seo = seo_row
        .map(|srow| -> Result<CanonicalRevisionSeo, sqlx::Error> {
            Ok(CanonicalRevisionSeo {
                title: srow.try_get("title")?,
                description: srow.try_get("description")?,
                og_image_id: srow.try_get("og_image_id")?,
                canonical_url: srow.try_get("canonical_url")?,
                noindex: srow.try_get::<i64, _>("noindex")? != 0,
            })
        })
        .transpose()?
        .and_then(CanonicalRevisionSeo::into_normalized);

    Ok(LiveEntryState {
        id,
        content_type_key,
        locale,
        translation_group_id,
        slug,
        publication_status,
        version,
        updated_at,
        trashed_at,
        effective,
        payload: CanonicalRevisionPayload {
            title,
            content_mode,
            values,
            builder_document_json,
            term_ids,
            primary_category_term_id,
            page_layout_key,
            custom_template_key,
            seo,
            parent_id,
            menu_order,
        },
    })
}

pub(super) async fn load_revision_record_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    entry_id: i64,
    rev_id: i64,
) -> ApiResult<LoadedRevisionRecord> {
    let row = sqlx::query(
        "SELECT id, revision_number, content_hash, source, restored_from_revision_number,
                author_user_id, author_username, title, slug, content_mode, values_json,
                builder_document_json, term_ids_json, primary_category_term_id,
                page_layout_key, custom_template_key, seo_json, parent_id, menu_order,
                entry_version, created_at
           FROM entry_revisions
          WHERE id = ? AND entry_id = ? AND content_type_key = ?",
    )
    .bind(rev_id)
    .bind(entry_id)
    .bind(type_key)
    .fetch_optional(&mut *conn)
    .await?;

    let Some(row) = row else {
        return Err(ApiError::not_found("Không tìm thấy phiên bản lịch sử."));
    };

    let id: i64 = row.try_get("id")?;
    let revision_number: i64 = row.try_get("revision_number")?;
    let content_hash: String = row.try_get("content_hash")?;
    let source = parse_revision_source(&row.try_get::<String, _>("source")?)?;
    let restored_from_revision_number: Option<i64> =
        row.try_get("restored_from_revision_number")?;
    let author_user_id: Option<i64> = row.try_get("author_user_id")?;
    let author_username: String = row.try_get("author_username")?;
    let title: String = row.try_get("title")?;
    let slug: String = row.try_get("slug")?;
    let content_mode = parse_content_mode(&row.try_get::<String, _>("content_mode")?)?;
    let values_raw: String = row.try_get("values_json")?;
    let builder_document_json: Option<String> = row.try_get("builder_document_json")?;
    let term_ids_raw: String = row.try_get("term_ids_json")?;
    let primary_category_term_id: Option<i64> = row.try_get("primary_category_term_id")?;
    let page_layout_key: Option<String> = row
        .try_get::<Option<String>, _>("page_layout_key")?
        .filter(|s| !s.is_empty());
    let custom_template_key: Option<String> = row
        .try_get::<Option<String>, _>("custom_template_key")?
        .filter(|s| !s.is_empty());
    let seo_raw: Option<String> = row.try_get("seo_json")?;
    let parent_id: Option<i64> = row.try_get("parent_id")?;
    let menu_order: i32 = row.try_get("menu_order")?;
    let entry_version: i64 = row.try_get("entry_version")?;
    let created_at: String = row.try_get("created_at")?;

    let parsed_values: BTreeMap<String, Value> = serde_json::from_str(&values_raw)
        .map_err(|_| ApiError::internal("Dữ liệu values_json của revision bị hỏng."))?;
    let mut values = BTreeMap::new();
    for (k, v) in parsed_values {
        values.insert(k, canonical_json_value(&v));
    }
    let mut term_ids: Vec<i64> = serde_json::from_str(&term_ids_raw)
        .map_err(|_| ApiError::internal("Dữ liệu term_ids_json của revision bị hỏng."))?;
    term_ids.sort_unstable();
    term_ids.dedup();
    let seo = seo_raw
        .as_deref()
        .map(serde_json::from_str::<CanonicalRevisionSeo>)
        .transpose()
        .map_err(|_| ApiError::internal("Dữ liệu seo_json của revision bị hỏng."))?
        .and_then(CanonicalRevisionSeo::into_normalized);

    Ok(LoadedRevisionRecord {
        summary: RevisionSummaryDto {
            id,
            revision_number,
            content_hash,
            source,
            restored_from_revision_number,
            author_user_id,
            author_username,
            title: title.clone(),
            content_mode,
            entry_version,
            created_at,
        },
        slug,
        payload: CanonicalRevisionPayload {
            title,
            content_mode,
            values,
            builder_document_json,
            term_ids,
            primary_category_term_id,
            page_layout_key,
            custom_template_key,
            seo,
            parent_id,
            menu_order,
        },
    })
}

pub(super) fn sanitize_cleared_field_list(
    raw_cleared: &[String],
    effective: &EffectiveContentSchema,
    type_key: &str,
    caller_role: FieldRole,
) -> Vec<String> {
    if type_key == "pattern" {
        return Vec::new();
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for key in raw_cleared {
        if !valid_snake_key(key) || !seen.insert(key.clone()) {
            continue;
        }
        let Some(field) = effective.effective_fields.get(key) else {
            continue;
        };
        if !is_field_editable(field, caller_role) {
            continue;
        }
        out.push(key.clone());
    }
    out
}

pub(super) fn decode_builder_document_for_payload(
    type_key: &str,
    payload: &opendash_core::revisions::CanonicalRevisionPayload,
) -> Option<BuilderDocument> {
    if type_key == "pattern" {
        return payload
            .builder_document_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<BuilderDocument>(raw).ok());
    }
    let raw_body = payload
        .values
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("");
    if payload.content_mode == opendash_core::content::ContentMode::Builder
        || opendash_core::builder_blocks::contains_blocks(raw_body)
    {
        Some(opendash_core::builder_blocks::parse_blocks_lenient_or_recover(raw_body).document)
    } else {
        None
    }
}
