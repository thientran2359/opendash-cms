//! CRUD for saved content entries. Never puts SQL inside HTTP routes.

use super::*;

pub(super) use std::collections::BTreeMap;

pub(super) use opendash_core::content::{
    ContentEntryInput, ContentMode, ContentPublication, ContentSupport, ContentTypeDefinition,
    ContentTypeSettings, EffectiveContentSchema, EffectiveFieldStatus, FieldRole, PatchField,
    deserialize_patch_field,
};

pub(super) use serde::Serialize;

pub(super) use serde_json::Value;

pub(super) use sqlx::{Sqlite, SqlitePool, Transaction};

pub(super) use crate::{
    auth::{self, SessionContext},
    error::{ApiError, ApiResult},
};

/// Pattern is a separate, strictly private canonical entry protocol. Generic
/// content writes (including composite field/layout hooks) may not mutate it.
pub(in crate::storage) fn forbid_generic_pattern_write(type_key: &str) -> ApiResult<()> {
    if type_key == "pattern" {
        Err(ApiError::forbidden(
            "Pattern chỉ được sửa qua API riêng tư /api/admin/patterns.",
        ))
    } else {
        Ok(())
    }
}

pub(in crate::storage) fn verify_supported_values(
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

pub(in crate::storage) async fn sync_entry_body_refs_in_conn(
    conn: &mut sqlx::SqliteConnection,
    entry_id: i64,
    schema: &EffectiveContentSchema,
    values: &BTreeMap<String, Value>,
    previous_body: Option<&str>,
    actor_role: FieldRole,
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

    let catalog = crate::storage::page::load_unified_block_catalog_in_conn(conn).await?;
    let allowed_context_fields: Vec<String> = schema
        .effective_fields
        .iter()
        .filter(|(_, fe)| {
            fe.effective_status == EffectiveFieldStatus::Active
                && fe.definition.min_view_role <= actor_role
        })
        .map(|(k, _)| k.clone())
        .collect();
    crate::storage::fields::validate_comment_block_refs_in_conn(
        conn,
        raw_body,
        previous_body,
        &catalog,
        None,
        Some(&allowed_context_fields),
    )
    .await?;

    let refs = validated_entry_block_refs_in_conn(conn, raw_body).await?;
    if refs.pattern_ids.len() > 16 {
        return Err(ApiError::unprocessable(
            "Builder exceeds 16 Pattern references.",
        ));
    }
    let prev_patterns = previous_body
        .map(|b| {
            opendash_core::builder_blocks::extract_block_refs_lenient_with_schemas(b, &catalog)
                .pattern_ids
        })
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

pub(in crate::storage) fn extract_body_from_values_json(values_json: &str) -> Option<String> {
    serde_json::from_str::<BTreeMap<String, Value>>(values_json)
        .ok()
        .and_then(|map| {
            map.get("body")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
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
    sync_entry_body_refs_in_conn(
        &mut *tx,
        id,
        &effective,
        &input.values,
        None,
        actor_field_role(actor.user.role),
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
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(record)
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
         WHERE content_type_key = ? AND id = ? AND version = ? AND trashed_at IS NULL",
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
                actor_field_role(actor.user.role),
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
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(record)
        }
    }
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
    sync_entry_body_refs_in_conn(
        &mut *tx,
        id,
        &effective,
        &input.values,
        None,
        actor_field_role(actor.user.role),
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
    crate::cache::SiteCacheEpoch::global().bump();
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
         WHERE content_type_key = ? AND id = ? AND version = ? AND trashed_at IS NULL",
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
                actor_field_role(actor.user.role),
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
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(record)
        }
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

    let (plan_locale, plan_group_id, merged_values, preflight_prev_body) = match entry_id {
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
            (loc, None, out, None)
        }
        Some(id) => {
            let expected_ver = input.expected_version.unwrap();
            crate::storage::entry_lifecycle::ensure_active_entry_version(
                db,
                type_key,
                id,
                expected_ver,
            )
            .await?;
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
            let prev_body = existing
                .values
                .get("body")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let mut out = existing.values;
            for (k, v) in &input.values {
                if v.is_null() {
                    out.remove(k);
                } else {
                    out.insert(k.clone(), v.clone());
                }
            }
            (loc, Some(existing.translation_group_id), out, prev_body)
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
        let catalog = crate::storage::page::load_unified_block_catalog_in_conn(&mut conn).await?;
        let caller_role = actor_field_role(actor.user.role);
        let allowed_context_fields: Vec<String> = effective
            .effective_fields
            .iter()
            .filter(|(_, fe)| {
                fe.effective_status == EffectiveFieldStatus::Active
                    && fe.definition.min_view_role <= caller_role
            })
            .map(|(k, _)| k.clone())
            .collect();
        crate::storage::fields::validate_comment_block_refs_in_conn(
            &mut conn,
            body,
            preflight_prev_body.as_deref(),
            &catalog,
            None,
            Some(&allowed_context_fields),
        )
        .await?;
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
                 WHERE content_type_key = ? AND id = ? AND version = ? AND trashed_at IS NULL",
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
        actor_field_role(actor.user.role),
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
    crate::cache::SiteCacheEpoch::global().bump();
    filter_entry_values_for_role(&mut record, &effective, actor_field_role(actor.user.role));

    Ok(CompositeEntrySaveResult {
        entry: record,
        custom_path,
        seo,
        schema_token: effective.schema_token,
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
    crate::storage::settings::require_multilingual_enabled_in_conn(&mut tx).await?;
    let source_active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM content_entries e JOIN site_locales sl
           ON sl.code = e.locale AND sl.deleted_at IS NULL WHERE e.id = ?)",
    )
    .bind(source_id)
    .fetch_one(&mut *tx)
    .await?;
    if !source_active {
        return Err(ApiError::not_found("Không tìm thấy ngôn ngữ."));
    }
    let effective =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, type_key)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy loại nội dung."))?;
    let locale_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM site_locales WHERE code = ? AND deleted_at IS NULL)",
    )
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

    let source_values_json: Option<String> =
        sqlx::query_scalar("SELECT values_json FROM content_entries WHERE id = ?")
            .bind(source_id)
            .fetch_optional(&mut *tx)
            .await?;
    let source_body = source_values_json
        .as_deref()
        .and_then(extract_body_from_values_json);
    sync_entry_body_refs_in_conn(
        &mut *tx,
        new_id,
        &effective,
        &plan.values,
        source_body.as_deref(),
        actor_field_role(actor.user.role),
    )
    .await?;
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
    crate::cache::SiteCacheEpoch::global().bump();
    filter_entry_values_for_role(&mut record, &effective, actor_field_role(actor.user.role));
    Ok(record)
}

/// Legacy DELETE compatibility wrapper: at S02 activation this moves an authorized
/// active ordinary entry into recoverable Trash (never physical deletion).
pub async fn delete_entry(
    db: &SqlitePool,
    actor: &crate::auth::SessionContext,
    type_key: &str,
    id: i64,
    expected_version: i64,
) -> ApiResult<crate::storage::entry_lifecycle::EntryCommitReceipt> {
    forbid_generic_pattern_write(type_key)?;
    let cmd = crate::storage::entry_lifecycle::LifecycleCommand {
        operation: crate::storage::entry_lifecycle::LifecycleOperation::Trash,
        type_key: type_key.to_owned(),
        entry_id: id,
        expected_version,
    };
    let authority = crate::storage::entry_lifecycle::LifecycleAuthority {
        session: actor,
        plugin_fence: None,
        confirmation: crate::storage::entry_lifecycle::LifecycleConfirmationEvidence::None,
        scope: crate::storage::entry_lifecycle::LifecycleScope::Ordinary,
    };
    crate::storage::entry_lifecycle::execute_lifecycle_transition(db, &cmd, &authority).await
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
    current_entry_in_write(&mut tx, type_key, id, expected_version).await?;
    let (locale, current_status): (String, String) = sqlx::query_as(
        "SELECT locale, publication_status FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    resolve_entry_write_locale_in_tx(&mut tx, None, Some(&locale)).await?;
    if current_status == status {
        let record = get_entry_in_tx(&mut tx, type_key, id)
            .await?
            .ok_or_else(|| ApiError::internal("Không thể tải trạng thái xuất bản."))?;
        tx.commit().await?;
        return Ok(record);
    }
    let result = sqlx::query(
        "UPDATE content_entries
         SET publication_status = ?,
             published_at = CASE WHEN ? = 'published' THEN COALESCE(published_at, datetime('now')) ELSE published_at END,
             version = version + 1,
             updated_at = datetime('now')
         WHERE content_type_key = ? AND id = ? AND version = ? AND trashed_at IS NULL",
    )
    .bind(status)
    .bind(status)
    .bind(type_key)
    .bind(id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        ));
    }
    if status == "draft" {
        crate::storage::plugin_projections::delete_entry_projections_in_tx(&mut tx, id).await?;
    }
    crate::storage::seo::sync_entry_seo_media_ref_in_tx(&mut tx, id).await?;
    let needs_claim_sync = if type_key == "page" {
        true
    } else {
        let id_str = id.to_string();
        let has_claim: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM permalink_route_claims WHERE target_kind = 'entry' AND target_id = ?)",
        )
        .bind(&id_str)
        .fetch_one(&mut *tx)
        .await?;
        let has_entry_redirects: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM entry_redirects WHERE entry_id = ?)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        let has_permalink_redirects: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM permalink_route_redirects WHERE target_kind = 'entry' AND target_id = ?)",
        )
        .bind(&id_str)
        .fetch_one(&mut *tx)
        .await?;
        has_claim == 0 || has_entry_redirects != 0 || has_permalink_redirects != 0
    };
    if needs_claim_sync {
        crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    }
    let record = get_entry_in_tx(&mut tx, type_key, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể tải trạng thái xuất bản."))?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(record)
}
