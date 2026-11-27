use std::collections::BTreeMap;

use opendash_core::builder_blocks::contains_blocks;
use opendash_core::content::{ContentMode, ContentSupport};
use serde_json::Value;
use sqlx::SqlitePool;

use crate::auth::{Role, SessionContext};
use crate::error::{ApiError, ApiResult};
use crate::routes::entries::MAX_CONTENT_ENTRY_JSON_BYTES;
use crate::storage::entries;

use super::autosave_merge::{
    build_editor_autosave_row, build_pattern_autosave_row, build_studio_autosave_row,
    load_existing_autosave_in_conn, validate_pattern_autosave_input,
};
use super::diff_builder::has_hidden_fields_in_map;
use super::field_refs::inspect_missing_refs_in_conn;
use super::read_and_diff::materialize_autosave_payload_in_conn;
use super::snapshot_queries::{
    check_not_trashed, load_live_entry_state_in_conn, require_writable_cpt_and_locale_in_conn,
};
use super::types::{
    AutosaveSnapshotDto, AutosaveSnapshotResponse, AutosaveSurface, AutosaveUpsertInput,
    AutosaveUpsertResponse, caller_field_role, can_view_body, is_body_editable,
};

pub async fn get_entry_autosave(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<AutosaveSnapshotResponse> {
    let mut conn = db.acquire().await?;
    let live = load_live_entry_state_in_conn(&mut conn, type_key, entry_id).await?;
    check_not_trashed(live.trashed_at.as_deref())?;

    let Some(saved) =
        load_existing_autosave_in_conn(&mut conn, entry_id, actor.user.id, type_key).await?
    else {
        return Ok(AutosaveSnapshotResponse {
            current_version: live.version,
            autosave: None,
        });
    };

    let caller_role = caller_field_role(actor);
    let body_viewable = can_view_body(&live.effective, type_key, caller_role);
    let mut visible_values = BTreeMap::new();
    for (k, v) in &saved.values {
        if let Some(fe) = live.effective.effective_fields.get(k) {
            if caller_role >= fe.definition.effective_acl().0 {
                visible_values.insert(k.clone(), v.clone());
            }
        } else {
            visible_values.insert(k.clone(), v.clone());
        }
    }
    let visible_cleared: Vec<String> = saved
        .cleared_fields
        .into_iter()
        .filter(|k| {
            live.effective
                .effective_fields
                .get(k)
                .map_or(true, |fe| caller_role >= fe.definition.effective_acl().0)
        })
        .collect();

    let builder_document = if !body_viewable {
        None
    } else if type_key == "pattern" {
        saved
            .builder_document_json
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok())
    } else {
        let raw_body = visible_values
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("");
        if saved.content_mode == ContentMode::Builder
            || saved.source_surface == AutosaveSurface::Studio
            || contains_blocks(raw_body)
        {
            Some(opendash_core::builder_blocks::parse_blocks_lenient_or_recover(raw_body).document)
        } else {
            None
        }
    };

    let (_meta, materialized) =
        materialize_autosave_payload_in_conn(&mut conn, &live, actor.user.id, caller_role).await?;
    let missing_refs =
        inspect_missing_refs_in_conn(&mut conn, &live, &materialized, caller_role, false).await?;
    let has_hidden_admin_fields =
        has_hidden_fields_in_map(&live.effective, &live.payload.values, caller_role);

    Ok(AutosaveSnapshotResponse {
        current_version: live.version,
        autosave: Some(AutosaveSnapshotDto {
            source_surface: saved.source_surface,
            base_version: saved.base_version,
            is_stale_base: saved.base_version < live.version,
            updated_at: saved.updated_at,
            client_seq: saved.client_seq,
            title: saved.title,
            content_mode: saved.content_mode,
            values: visible_values,
            cleared_fields: visible_cleared,
            primary_category_touched: saved.primary_category_touched,
            primary_category_term_id: saved.primary_category_term_id,
            page_layout_touched: saved.page_layout_touched,
            page_layout_key: saved.page_layout_key,
            custom_template_touched: saved.custom_template_touched,
            custom_template_key: saved.custom_template_key,
            seo_touched: saved.seo_touched,
            seo: saved.seo,
            builder_document,
            term_ids: saved.term_ids,
            has_hidden_admin_fields,
            missing_refs,
        }),
    })
}

pub async fn upsert_entry_autosave(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
    input: &AutosaveUpsertInput,
) -> ApiResult<AutosaveUpsertResponse> {
    if type_key == "pattern" && actor.user.role != Role::Admin {
        return Err(ApiError::forbidden(
            "Pattern chỉ cho phép Admin lưu tự động.",
        ));
    }
    let caller_role = caller_field_role(actor);
    let mut tx = entries::begin_entry_write(db, actor).await?;
    let live = load_live_entry_state_in_conn(&mut tx, type_key, entry_id).await?;
    require_writable_cpt_and_locale_in_conn(&mut tx, type_key, &live.locale).await?;

    if type_key != "pattern" && input.source_surface == AutosaveSurface::Studio {
        let body_editable = is_body_editable(&live.effective, type_key, caller_role);
        let builder_enabled = live
            .effective
            .base
            .descriptor
            .settings
            .supports(ContentSupport::Builder);
        if !body_editable || !builder_enabled {
            return Err(ApiError::forbidden(
                "Trình dựng trang bị khóa hoặc chưa bật cho tài khoản này.",
            ));
        }
    }

    if input.base_version > live.version {
        return Err(ApiError::bad_request(
            "base_version vượt quá version hiện tại của nội dung.",
        ));
    }

    if type_key == "pattern" {
        validate_pattern_autosave_input(input)?;
    }

    check_not_trashed(live.trashed_at.as_deref())?;

    let existing =
        load_existing_autosave_in_conn(&mut tx, entry_id, actor.user.id, type_key).await?;
    if let Some(ref prev) = existing
        && input.client_seq <= prev.client_seq
    {
        let current_version = live.version;
        tx.commit().await?;
        return Ok(AutosaveUpsertResponse {
            applied: false,
            saved_at: prev.updated_at.clone(),
            base_version: prev.base_version,
            current_version,
            is_stale_base: prev.base_version < current_version,
            client_seq: prev.client_seq,
        });
    }

    let (
        title,
        content_mode,
        values,
        cleared_fields,
        builder_document_json,
        term_ids,
        primary_category_touched,
        primary_category_term_id,
        page_layout_touched,
        page_layout_key,
        custom_template_touched,
        custom_template_key,
        seo_touched,
        seo,
    ) = if type_key == "pattern" {
        build_pattern_autosave_row(&live, existing.as_ref(), input)?
    } else if input.source_surface == AutosaveSurface::Studio {
        build_studio_autosave_row(&live, existing.as_ref(), input, caller_role)?
    } else {
        build_editor_autosave_row(&live, existing.as_ref(), input, caller_role)?
    };

    let values_json = serde_json::to_string(&values)?;
    if values_json.len() > MAX_CONTENT_ENTRY_JSON_BYTES {
        return Err(ApiError::bad_request(
            "Dữ liệu bản lưu tự động vượt giới hạn 768 KiB.",
        ));
    }
    let cleared_json = serde_json::to_string(&cleared_fields)?;
    let term_ids_json = serde_json::to_string(&term_ids)?;
    let seo_json = seo.as_ref().map(serde_json::to_string).transpose()?;
    let surface_str = match input.source_surface {
        AutosaveSurface::Editor => "editor",
        AutosaveSurface::Studio => "studio",
    };

    let saved_at: String = sqlx::query_scalar(
        "INSERT INTO entry_autosaves (
             entry_id, user_id, content_type_key, source_surface, base_version, client_seq,
             title, content_mode, values_json, cleared_fields_json, builder_document_json,
             term_ids_json, primary_category_touched, primary_category_term_id,
             page_layout_touched, page_layout_key, custom_template_touched,
             custom_template_key, seo_touched, seo_json, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
         ON CONFLICT(entry_id, user_id) DO UPDATE SET
             content_type_key = excluded.content_type_key,
             source_surface = excluded.source_surface,
             base_version = excluded.base_version,
             client_seq = excluded.client_seq,
             title = excluded.title,
             content_mode = excluded.content_mode,
             values_json = excluded.values_json,
             cleared_fields_json = excluded.cleared_fields_json,
             builder_document_json = excluded.builder_document_json,
             term_ids_json = excluded.term_ids_json,
             primary_category_touched = excluded.primary_category_touched,
             primary_category_term_id = excluded.primary_category_term_id,
             page_layout_touched = excluded.page_layout_touched,
             page_layout_key = excluded.page_layout_key,
             custom_template_touched = excluded.custom_template_touched,
             custom_template_key = excluded.custom_template_key,
             seo_touched = excluded.seo_touched,
             seo_json = excluded.seo_json,
             updated_at = excluded.updated_at
         RETURNING updated_at",
    )
    .bind(entry_id)
    .bind(actor.user.id)
    .bind(type_key)
    .bind(surface_str)
    .bind(input.base_version)
    .bind(input.client_seq)
    .bind(&title)
    .bind(content_mode.as_str())
    .bind(&values_json)
    .bind(&cleared_json)
    .bind(builder_document_json.as_deref())
    .bind(&term_ids_json)
    .bind(i64::from(primary_category_touched))
    .bind(primary_category_term_id)
    .bind(i64::from(page_layout_touched))
    .bind(page_layout_key.as_deref())
    .bind(i64::from(custom_template_touched))
    .bind(custom_template_key.as_deref())
    .bind(i64::from(seo_touched))
    .bind(seo_json.as_deref())
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(AutosaveUpsertResponse {
        applied: true,
        saved_at,
        base_version: input.base_version,
        current_version: live.version,
        is_stale_base: input.base_version < live.version,
        client_seq: input.client_seq,
    })
}

pub async fn delete_entry_autosave(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<()> {
    let mut tx = entries::begin_entry_write(db, actor).await?;
    let live = load_live_entry_state_in_conn(&mut tx, type_key, entry_id).await?;
    check_not_trashed(live.trashed_at.as_deref())?;
    sqlx::query("DELETE FROM entry_autosaves WHERE entry_id = ? AND user_id = ?")
        .bind(entry_id)
        .bind(actor.user.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
