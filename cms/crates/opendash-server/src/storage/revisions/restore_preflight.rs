use std::collections::{BTreeMap, BTreeSet};

use opendash_core::builder::BuilderDocument;
use opendash_core::builder_blocks::{
    MAX_BUILDER_BLOCK_BODY_BYTES, contains_blocks, serialize_builder_document_to_blocks,
};
use opendash_core::content::{
    ContentEntryInput, ContentMode, ContentSupport, ContentTypeSettings, EffectiveFieldStatus,
    FieldRole, PublicExposure,
};
use opendash_core::field_schema::FieldSpec;
use opendash_core::revisions::{CanonicalRevisionPayload, CanonicalRevisionSeo};
use serde_json::Value;
use sqlx::{Sqlite, Transaction};

use crate::error::{ApiError, ApiResult};
use crate::hooks::ContentBeforeSaveVeto;
use crate::seo::SeoMetadataInput;
use crate::storage::builder::{
    entry_builder_snapshot, render_entry_builder_preview, validate_entry_builder_document,
};
use crate::storage::entries;

use super::field_refs::inspect_missing_refs_in_conn;
use super::sanitize::sanitize_snapshot_for_restore;
use super::snapshot_queries::{
    LiveEntryState, check_not_trashed, decode_builder_document_for_payload,
    load_live_entry_state_in_conn, load_revision_record_in_conn,
    require_writable_cpt_and_locale_in_conn,
};
use super::types::{RestoreError, is_body_editable, is_field_editable};

pub(super) struct PreparedRestore {
    pub(super) schema_token: String,
    pub(super) slug: String,
    pub(super) candidate_payload: CanonicalRevisionPayload,
    pub(super) candidate_doc: Option<BuilderDocument>,
    pub(super) revision_number: i64,
    pub(super) was_published: bool,
    pub(super) locale: String,
    pub(super) translation_group_id: String,
}

pub(super) fn veto_err(
    code: &str,
    message: impl Into<String>,
    field_errors: BTreeMap<String, String>,
) -> RestoreError {
    RestoreError::ValidationVeto(ContentBeforeSaveVeto {
        code: code.to_owned(),
        message: message.into(),
        field_errors,
    })
}

pub(super) fn map_validation_api_err(err: ApiError, field_key: Option<&str>) -> RestoreError {
    if matches!(
        err.0,
        axum::http::StatusCode::BAD_REQUEST | axum::http::StatusCode::UNPROCESSABLE_ENTITY
    ) {
        let mut fields = BTreeMap::new();
        if let Some(fk) = field_key {
            fields.insert(fk.to_owned(), err.1.to_string());
        }
        veto_err("restore_validation_failed", err.1.to_string(), fields)
    } else {
        RestoreError::Api(err)
    }
}

pub(super) fn canonical_seo_to_input(seo: &CanonicalRevisionSeo) -> SeoMetadataInput {
    SeoMetadataInput {
        title: seo.title.clone(),
        description: seo.description.clone(),
        og_image_id: seo.og_image_id.clone(),
        canonical_url: seo.canonical_url.clone(),
        noindex: seo.noindex,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn prepare_restore_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    caller_role: FieldRole,
    type_key: &str,
    entry_id: i64,
    rev_id: i64,
    expected_version: i64,
    acknowledge_missing_refs: bool,
) -> Result<PreparedRestore, RestoreError> {
    let live = load_live_entry_state_in_conn(&mut **tx, type_key, entry_id).await?;
    let target = load_revision_record_in_conn(&mut **tx, type_key, entry_id, rev_id).await?;

    require_writable_cpt_and_locale_in_conn(&mut **tx, type_key, &live.locale).await?;
    if live.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi khôi phục.",
        )
        .into());
    }
    check_not_trashed(live.trashed_at.as_deref())?;

    let mut payload = target.payload.clone();
    let body_editable = is_body_editable(&live.effective, type_key, caller_role);

    if type_key != "pattern" {
        preserve_locked_fields(&live, &mut payload, caller_role, body_editable);
        if body_editable
            && payload.content_mode == ContentMode::Builder
            && !live
                .effective
                .base
                .descriptor
                .settings
                .supports(ContentSupport::Builder)
        {
            return Err(veto_err(
                "builder_disabled",
                "Trình dựng trang (Builder) hiện đang tắt cho loại nội dung này.",
                BTreeMap::from([("body".to_owned(), "Trình dựng trang đã bị tắt.".to_owned())]),
            ));
        }
    }

    let missing =
        inspect_missing_refs_in_conn(&mut **tx, &live, &payload, caller_role, true).await?;
    if missing.has_blocking_refs() && !acknowledge_missing_refs {
        return Err(RestoreError::MissingRefs(missing));
    }

    let mut candidate_doc = if body_editable {
        decode_builder_document_for_payload(type_key, &payload)
    } else {
        None
    };

    let schemas = crate::storage::page::load_unified_block_catalog_in_conn(&mut **tx).await?;
    let valid_cat_ids =
        load_valid_category_ids_for_terms(&mut **tx, type_key, &payload.term_ids).await?;
    sanitize_snapshot_for_restore(
        &live,
        &mut payload,
        candidate_doc.as_mut(),
        &missing,
        &schemas,
        &valid_cat_ids,
        caller_role,
    );

    if body_editable && type_key != "pattern" {
        if let Some(ref doc) = candidate_doc {
            let serialized = serialize_builder_document_to_blocks(doc).map_err(|err| {
                veto_err(
                    "restore_validation_failed",
                    err.to_string(),
                    BTreeMap::from([("body".to_owned(), err.to_string())]),
                )
            })?;
            if serialized.len() > MAX_BUILDER_BLOCK_BODY_BYTES {
                return Err(veto_err(
                    "restore_validation_failed",
                    "Nội dung Builder vượt quá giới hạn 384 KiB sau khi dọn.",
                    BTreeMap::from([(
                        "body".to_owned(),
                        "Nội dung Builder vượt quá giới hạn cho phép.".to_owned(),
                    )]),
                ));
            }
            payload
                .values
                .insert("body".to_owned(), Value::String(serialized));
        }
    } else if type_key == "pattern" {
        payload.builder_document_json = candidate_doc
            .as_ref()
            .map(|d| {
                d.to_json()
                    .map_err(|e| {
                        veto_err("restore_validation_failed", e.to_string(), BTreeMap::new())
                    })
                    .and_then(|bytes| {
                        String::from_utf8(bytes).map_err(|_| {
                            veto_err(
                                "restore_validation_failed",
                                "Dữ liệu Builder JSON không phải UTF-8 hợp lệ.",
                                BTreeMap::new(),
                            )
                        })
                    })
            })
            .transpose()?;
    }

    validate_sanitized_candidate_in_tx(
        tx,
        &live,
        &payload,
        candidate_doc.as_ref(),
        caller_role,
        body_editable,
    )
    .await?;

    Ok(PreparedRestore {
        schema_token: live.effective.schema_token.clone(),
        slug: live.slug.clone(),
        candidate_payload: payload,
        candidate_doc,
        revision_number: target.summary.revision_number,
        was_published: live.publication_status == "published",
        locale: live.locale,
        translation_group_id: live.translation_group_id,
    })
}

fn preserve_locked_fields(
    live: &LiveEntryState,
    payload: &mut CanonicalRevisionPayload,
    caller_role: FieldRole,
    body_editable: bool,
) {
    let mut next_vals = BTreeMap::new();
    for (k, v) in &payload.values {
        if k == "body" {
            continue;
        }
        if let Some(fe) = live.effective.effective_fields.get(k)
            && !is_field_editable(fe, caller_role)
        {
            continue;
        }
        next_vals.insert(k.clone(), v.clone());
    }
    for (k, fe) in &live.effective.effective_fields {
        if k != "body" && !is_field_editable(fe, caller_role) {
            if let Some(live_v) = live.payload.values.get(k) {
                next_vals.insert(k.clone(), live_v.clone());
            } else {
                next_vals.remove(k);
            }
        }
    }
    if body_editable {
        if let Some(body) = payload.values.get("body") {
            next_vals.insert("body".to_owned(), body.clone());
        }
    } else {
        payload.content_mode = live.payload.content_mode;
        if let Some(live_body) = live.payload.values.get("body") {
            next_vals.insert("body".to_owned(), live_body.clone());
        } else {
            next_vals.remove("body");
        }
    }
    payload.values = next_vals;
}

async fn load_valid_category_ids_for_terms(
    conn: &mut sqlx::SqliteConnection,
    type_key: &str,
    term_ids: &[i64],
) -> ApiResult<BTreeSet<i64>> {
    if type_key != "post" || term_ids.is_empty() {
        return Ok(BTreeSet::new());
    }
    let raw = serde_json::to_string(term_ids)?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM taxonomy_terms WHERE kind = 'category' AND id IN (SELECT value FROM json_each(?1))",
    )
    .bind(raw)
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids.into_iter().collect())
}

fn support_allows_field(type_key: &str, key: &str, settings: &ContentTypeSettings) -> bool {
    if matches!(type_key, "pattern" | "section") {
        return true;
    }
    match key {
        "body" => {
            settings.supports(ContentSupport::Editor) || settings.supports(ContentSupport::Builder)
        }
        "featured_image" => settings.supports(ContentSupport::FeaturedImage),
        "excerpt" => settings.supports(ContentSupport::Excerpt),
        _ => true,
    }
}

async fn validate_sanitized_candidate_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    live: &LiveEntryState,
    payload: &CanonicalRevisionPayload,
    doc: Option<&BuilderDocument>,
    caller_role: FieldRole,
    body_editable: bool,
) -> Result<(), RestoreError> {
    if live.content_type_key == "pattern" {
        if let Some(builder_doc) = doc {
            let current = entry_builder_snapshot(tx, &live.content_type_key, live.id).await?;
            let mut state = validate_entry_builder_document(tx, &current, caller_role, builder_doc)
                .await
                .map_err(|e| map_validation_api_err(e, Some("body")))?;
            render_entry_builder_preview(tx, &current, &mut state, builder_doc, false)
                .await
                .map_err(|e| map_validation_api_err(e, Some("body")))?;
        } else if payload.content_mode != ContentMode::Editor {
            return Err(veto_err(
                "restore_validation_failed",
                "Pattern không có Builder document chỉ hỗ trợ content_mode = 'editor'.",
                BTreeMap::from([("body".to_owned(), "content_mode không hợp lệ.".to_owned())]),
            ));
        }
        return Ok(());
    }

    entries::validate_entry_parent_in_conn(
        &mut **tx,
        &live.content_type_key,
        &live.effective.base.descriptor.settings,
        &live.locale,
        Some(live.id),
        payload.parent_id,
    )
    .await
    .map_err(|e| map_validation_api_err(e, Some("parent_id")))?;

    let loc_ctx = entries::build_entry_location_ctx_in_conn(
        &mut **tx,
        &live.content_type_key,
        Some(live.id),
        &live.locale,
        caller_role,
        payload.page_layout_key.clone(),
        payload.custom_template_key.clone(),
        &payload.term_ids,
    )
    .await?;

    let mut req_missing = BTreeMap::new();
    for (k, fe) in &live.effective.effective_fields {
        if is_field_editable(fe, caller_role)
            && fe.effective_required()
            && live.effective.is_field_active_in_context(fe, &loc_ctx)
            && support_allows_field(
                &live.content_type_key,
                k,
                &live.effective.base.descriptor.settings,
            )
        {
            let empty = payload.values.get(k).is_none_or(|v| {
                v.is_null()
                    || v.as_str().is_some_and(|s| s.trim().is_empty())
                    || v.as_array().is_some_and(Vec::is_empty)
            });
            if empty {
                req_missing.insert(k.clone(), fe.definition.label.clone());
            }
        }
    }
    if !req_missing.is_empty() {
        return Err(veto_err(
            "required_field_missing",
            "Thiếu giá trị cho trường bắt buộc sau khi dọn dữ liệu tham chiếu đã xóa.",
            req_missing,
        ));
    }

    let candidate_input = ContentEntryInput {
        title: payload.title.clone(),
        slug: live.slug.clone(),
        locale: Some(live.locale.clone()),
        values: payload.values.clone(),
        term_ids: Some(payload.term_ids.clone()),
    };
    candidate_input
        .validate_against_effective(&live.effective, &loc_ctx)
        .map_err(|msg| veto_err("restore_validation_failed", msg, BTreeMap::new()))?;

    entries::verify_supported_values(
        &live.content_type_key,
        &payload.values,
        &live.effective.base.descriptor.settings,
    )
    .map_err(|e| map_validation_api_err(e, None))?;

    let editable_specs: Vec<FieldSpec> = live
        .effective
        .effective_fields
        .iter()
        .filter(|(k, fe)| k.as_str() != "body" && is_field_editable(fe, caller_role))
        .map(|(_, fe)| fe.definition.clone())
        .collect();
    crate::storage::fields::validate_relational_refs_in_conn(
        &mut **tx,
        &editable_specs,
        &payload.values,
        None,
        Some(&live.locale),
    )
    .await
    .map_err(|e| map_validation_api_err(e, None))?;

    entries::validate_term_ids_for_locale(
        tx,
        &live.content_type_key,
        &payload.term_ids,
        &live.locale,
    )
    .await
    .map_err(|e| map_validation_api_err(e, Some("term_ids")))?;

    if live.content_type_key == "post"
        && let Some(ptid) = payload.primary_category_term_id
        && (ptid <= 0 || !payload.term_ids.contains(&ptid))
    {
        return Err(veto_err(
            "restore_validation_failed",
            "Chuyên mục chính không nằm trong term_ids.",
            BTreeMap::from([(
                "primary_category_term_id".to_owned(),
                "Chuyên mục chính không hợp lệ.".to_owned(),
            )]),
        ));
    }

    if let Some(ref seo) = payload.seo {
        let seo_input = canonical_seo_to_input(seo);
        crate::seo::validate_seo_input(&seo_input)
            .map_err(|e| map_validation_api_err(e, Some("seo")))?;
    }

    if body_editable {
        if payload.content_mode == ContentMode::Builder {
            if let Some(builder_doc) = doc {
                let current = entry_builder_snapshot(tx, &live.content_type_key, live.id).await?;
                let mut state =
                    validate_entry_builder_document(tx, &current, caller_role, builder_doc)
                        .await
                        .map_err(|e| map_validation_api_err(e, Some("body")))?;
                render_entry_builder_preview(tx, &current, &mut state, builder_doc, false)
                    .await
                    .map_err(|e| map_validation_api_err(e, Some("body")))?;
            }
        } else if let Some(raw_body) = payload.values.get("body").and_then(Value::as_str)
            && contains_blocks(raw_body)
        {
            entries::validated_entry_block_refs_in_conn(&mut **tx, raw_body)
                .await
                .map_err(|e| map_validation_api_err(e, Some("body")))?;
            let catalog =
                crate::storage::page::load_unified_block_catalog_in_conn(&mut **tx).await?;
            let allowed_context_fields: Vec<String> = live
                .effective
                .effective_fields
                .iter()
                .filter(|(_, fe)| {
                    fe.effective_status == EffectiveFieldStatus::Active
                        && fe.definition.effective_acl().0 <= caller_role
                        && fe.definition.effective_acl().2 == PublicExposure::Public
                })
                .map(|(k, _)| k.clone())
                .collect();
            crate::storage::fields::validate_comment_block_refs_in_conn(
                &mut **tx,
                raw_body,
                live.payload.values.get("body").and_then(Value::as_str),
                &catalog,
                None,
                Some(&allowed_context_fields),
            )
            .await
            .map_err(|e| map_validation_api_err(e, Some("body")))?;
        }
    }

    Ok(())
}
