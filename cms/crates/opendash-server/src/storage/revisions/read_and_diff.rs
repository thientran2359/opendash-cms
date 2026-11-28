use std::collections::{BTreeMap, BTreeSet};

use opendash_core::content::FieldRole;
use opendash_core::field_schema::FieldType;
use opendash_core::revisions::{
    CanonicalRevisionPayload, CanonicalRevisionSeo, canonical_json_value,
};
use serde_json::Value;
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::auth::SessionContext;
use crate::error::{ApiError, ApiResult};

use super::capture::parse_content_mode;
use super::diff_builder::{build_revision_diff, field_type_label, hydrate_terms_in_conn};
use super::field_refs::inspect_missing_refs_in_conn;
use super::snapshot_queries::{
    LiveEntryState, check_not_trashed, decode_builder_document_for_payload,
    load_live_entry_state_in_conn, load_revision_record_in_conn, parse_revision_source,
};
use super::types::{
    CompareTarget, CompareTargetMeta, RevisionCompareResponse, RevisionDetailDto,
    RevisionFieldView, RevisionSummaryDto, RevisionsListResponse, caller_field_role, can_view_body,
    is_field_editable,
};

pub async fn list_entry_revisions(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
    limit: i64,
    offset: i64,
) -> ApiResult<RevisionsListResponse> {
    let mut conn = db.acquire().await?;
    let live = load_live_entry_state_in_conn(&mut conn, type_key, entry_id).await?;
    check_not_trashed(live.trashed_at.as_deref())?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM entry_revisions WHERE entry_id = ? AND content_type_key = ?",
    )
    .bind(entry_id)
    .bind(type_key)
    .fetch_one(&mut *conn)
    .await?;

    let rows = sqlx::query(
        "SELECT id, revision_number, content_hash, source, restored_from_revision_number,
                author_user_id, author_username, title, content_mode, entry_version, created_at
           FROM entry_revisions
          WHERE entry_id = ? AND content_type_key = ?
          ORDER BY revision_number DESC
          LIMIT ? OFFSET ?",
    )
    .bind(entry_id)
    .bind(type_key)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *conn)
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(RevisionSummaryDto {
            id: row.try_get("id")?,
            revision_number: row.try_get("revision_number")?,
            content_hash: row.try_get("content_hash")?,
            source: parse_revision_source(&row.try_get::<String, _>("source")?)?,
            restored_from_revision_number: row.try_get("restored_from_revision_number")?,
            author_user_id: row.try_get("author_user_id")?,
            author_username: row.try_get("author_username")?,
            title: row.try_get("title")?,
            content_mode: parse_content_mode(&row.try_get::<String, _>("content_mode")?)?,
            entry_version: row.try_get("entry_version")?,
            created_at: row.try_get("created_at")?,
        });
    }

    let has_autosave: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM entry_autosaves WHERE entry_id = ? AND user_id = ?)",
    )
    .bind(entry_id)
    .bind(actor.user.id)
    .fetch_one(&mut *conn)
    .await?;

    Ok(RevisionsListResponse {
        items,
        total,
        limit,
        offset,
        current_version: live.version,
        has_autosave: has_autosave != 0,
    })
}

pub async fn get_entry_revision_filtered(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
    rev_id: i64,
) -> ApiResult<RevisionDetailDto> {
    let mut conn = db.acquire().await?;
    let live = load_live_entry_state_in_conn(&mut conn, type_key, entry_id).await?;
    check_not_trashed(live.trashed_at.as_deref())?;
    let rec = load_revision_record_in_conn(&mut conn, type_key, entry_id, rev_id).await?;
    let caller_role = caller_field_role(actor);
    let body_viewable = can_view_body(&live.effective, type_key, caller_role);

    let mut fields = Vec::new();
    let mut has_hidden_admin_fields = false;
    if type_key != "pattern" {
        let mut seen_keys = BTreeSet::new();
        for (key, fe) in &live.effective.effective_fields {
            seen_keys.insert(key.as_str());
            if caller_role < fe.definition.effective_acl().0 {
                if rec.payload.values.contains_key(key) {
                    has_hidden_admin_fields = true;
                }
                continue;
            }
            let Some(val) = rec.payload.values.get(key) else {
                continue;
            };
            fields.push(RevisionFieldView {
                key: key.clone(),
                label: fe.definition.label.clone(),
                field_type: field_type_label(fe.definition.field_type).to_owned(),
                value: val.clone(),
                editable: is_field_editable(fe, caller_role),
            });
        }
        for (key, val) in &rec.payload.values {
            if seen_keys.contains(key.as_str()) {
                continue;
            }
            fields.push(RevisionFieldView {
                key: key.clone(),
                label: key.clone(),
                field_type: "unknown".to_owned(),
                value: val.clone(),
                editable: true,
            });
        }
    }

    let builder_document = if body_viewable {
        decode_builder_document_for_payload(type_key, &rec.payload)
    } else {
        None
    };

    let terms = hydrate_terms_in_conn(&mut conn, &rec.payload.term_ids).await?;
    let missing_refs =
        inspect_missing_refs_in_conn(&mut conn, &live, &rec.payload, caller_role, false).await?;

    Ok(RevisionDetailDto {
        revision: rec.summary,
        current_version: live.version,
        slug: rec.slug,
        title: rec.payload.title,
        content_mode: rec.payload.content_mode,
        fields,
        builder_document,
        term_ids: rec.payload.term_ids,
        terms,
        primary_category_term_id: rec.payload.primary_category_term_id,
        page_layout_key: rec.payload.page_layout_key,
        custom_template_key: rec.payload.custom_template_key,
        seo: rec.payload.seo,
        parent_id: rec.payload.parent_id,
        menu_order: rec.payload.menu_order,
        has_hidden_admin_fields,
        missing_refs,
    })
}

pub async fn compare_entry_revisions(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
    from: CompareTarget,
    to: CompareTarget,
) -> ApiResult<RevisionCompareResponse> {
    let mut conn = db.acquire().await?;
    let live = load_live_entry_state_in_conn(&mut conn, type_key, entry_id).await?;
    check_not_trashed(live.trashed_at.as_deref())?;
    let caller_role = caller_field_role(actor);

    let (from_meta, from_payload) =
        load_compare_target_in_conn(&mut conn, &live, actor, from, caller_role).await?;
    let (to_meta, to_payload) =
        load_compare_target_in_conn(&mut conn, &live, actor, to, caller_role).await?;

    let from_missing_refs =
        inspect_missing_refs_in_conn(&mut conn, &live, &from_payload, caller_role, false).await?;
    let to_missing_refs =
        inspect_missing_refs_in_conn(&mut conn, &live, &to_payload, caller_role, false).await?;

    let diff =
        build_revision_diff(&mut conn, &live, &from_payload, &to_payload, caller_role).await?;

    Ok(RevisionCompareResponse {
        entry_id: live.id,
        content_type_key: live.content_type_key,
        current_version: live.version,
        from_meta,
        to_meta,
        diff,
        from_missing_refs,
        to_missing_refs,
    })
}

async fn load_compare_target_in_conn(
    conn: &mut SqliteConnection,
    live: &LiveEntryState,
    actor: &SessionContext,
    target: CompareTarget,
    caller_role: FieldRole,
) -> ApiResult<(CompareTargetMeta, CanonicalRevisionPayload)> {
    match target {
        CompareTarget::Current => Ok((
            CompareTargetMeta {
                target: CompareTarget::Current,
                revision_number: None,
                label: "Current".to_owned(),
                updated_at: Some(live.updated_at.clone()),
                entry_version: live.version,
            },
            live.payload.clone(),
        )),
        CompareTarget::Revision { rev_id } => {
            let rec =
                load_revision_record_in_conn(&mut *conn, &live.content_type_key, live.id, rev_id)
                    .await?;
            let meta = CompareTargetMeta {
                target: CompareTarget::Revision { rev_id },
                revision_number: Some(rec.summary.revision_number),
                label: format!("#{}", rec.summary.revision_number),
                updated_at: Some(rec.summary.created_at.clone()),
                entry_version: rec.summary.entry_version,
            };
            Ok((meta, rec.payload))
        }
        CompareTarget::Autosave => {
            materialize_autosave_payload_in_conn(&mut *conn, live, actor.user.id, caller_role).await
        }
    }
}

pub(super) async fn materialize_autosave_payload_in_conn(
    conn: &mut SqliteConnection,
    live: &LiveEntryState,
    user_id: i64,
    caller_role: FieldRole,
) -> ApiResult<(CompareTargetMeta, CanonicalRevisionPayload)> {
    let row = sqlx::query(
        "SELECT source_surface, base_version, title, content_mode, values_json,
                cleared_fields_json, builder_document_json, term_ids_json,
                primary_category_touched, primary_category_term_id,
                page_layout_touched, page_layout_key,
                custom_template_touched, custom_template_key,
                seo_touched, seo_json,
                parent_id_touched, parent_id, menu_order_touched, menu_order,
                updated_at
           FROM entry_autosaves
          WHERE entry_id = ? AND user_id = ? AND content_type_key = ?",
    )
    .bind(live.id)
    .bind(user_id)
    .bind(&live.content_type_key)
    .fetch_optional(&mut *conn)
    .await?;

    let Some(row) = row else {
        return Err(ApiError::not_found("Không tìm thấy bản lưu tự động."));
    };

    let base_version: i64 = row.try_get("base_version")?;
    let title: String = row.try_get("title")?;
    let content_mode = parse_content_mode(&row.try_get::<String, _>("content_mode")?)?;
    let updated_at: String = row.try_get("updated_at")?;
    let builder_document_json: Option<String> = row.try_get("builder_document_json")?;

    let meta = CompareTargetMeta {
        target: CompareTarget::Autosave,
        revision_number: None,
        label: "Autosave".to_owned(),
        updated_at: Some(updated_at),
        entry_version: base_version,
    };

    if live.content_type_key == "pattern" {
        return Ok((
            meta,
            CanonicalRevisionPayload {
                title,
                content_mode,
                values: BTreeMap::new(),
                builder_document_json,
                term_ids: Vec::new(),
                primary_category_term_id: None,
                page_layout_key: None,
                custom_template_key: None,
                seo: None,
                parent_id: None,
                menu_order: 0,
            },
        ));
    }

    let raw_vals: String = row.try_get("values_json")?;
    let parsed_vals: BTreeMap<String, Value> = serde_json::from_str(&raw_vals)?;
    let raw_cleared: String = row.try_get("cleared_fields_json")?;
    let cleared: Vec<String> = serde_json::from_str(&raw_cleared)?;
    let mut term_ids: Vec<i64> = serde_json::from_str(&row.try_get::<String, _>("term_ids_json")?)?;
    term_ids.sort_unstable();
    term_ids.dedup();

    let body_editable = live
        .effective
        .effective_fields
        .get("body")
        .is_some_and(|f| {
            f.definition.field_type == FieldType::Richtext && is_field_editable(f, caller_role)
        });
    let mut values = live.payload.values.clone();
    for key in &cleared {
        if key == "body" && !body_editable {
            continue;
        }
        if live
            .effective
            .effective_fields
            .get(key)
            .is_some_and(|fe| is_field_editable(fe, caller_role))
        {
            values.remove(key);
        }
    }
    for (k, v) in parsed_vals {
        if k == "body" && !body_editable {
            continue;
        }
        if live
            .effective
            .effective_fields
            .get(&k)
            .is_some_and(|fe| is_field_editable(fe, caller_role))
        {
            if v.is_null() {
                values.remove(&k);
            } else {
                values.insert(k, canonical_json_value(&v));
            }
        }
    }

    let final_mode = if body_editable {
        content_mode
    } else {
        live.payload.content_mode
    };
    let primary_category_term_id = if row.try_get::<i64, _>("primary_category_touched")? != 0 {
        row.try_get("primary_category_term_id")?
    } else {
        live.payload.primary_category_term_id
    };
    let page_layout_key = if row.try_get::<i64, _>("page_layout_touched")? != 0 {
        row.try_get::<Option<String>, _>("page_layout_key")?
            .filter(|s| !s.is_empty())
    } else {
        live.payload.page_layout_key.clone()
    };
    let custom_template_key = if row.try_get::<i64, _>("custom_template_touched")? != 0 {
        row.try_get::<Option<String>, _>("custom_template_key")?
            .filter(|s| !s.is_empty())
    } else {
        live.payload.custom_template_key.clone()
    };
    let seo = if row.try_get::<i64, _>("seo_touched")? != 0 {
        row.try_get::<Option<String>, _>("seo_json")?
            .as_deref()
            .map(serde_json::from_str::<CanonicalRevisionSeo>)
            .transpose()?
            .and_then(CanonicalRevisionSeo::into_normalized)
    } else {
        live.payload.seo.clone()
    };
    let parent_id = if row.try_get::<i64, _>("parent_id_touched")? != 0 {
        row.try_get("parent_id")?
    } else {
        live.payload.parent_id
    };
    let menu_order = if row.try_get::<i64, _>("menu_order_touched")? != 0 {
        row.try_get::<Option<i32>, _>("menu_order")?.unwrap_or(0)
    } else {
        live.payload.menu_order
    };

    Ok((
        meta,
        CanonicalRevisionPayload {
            title,
            content_mode: final_mode,
            values,
            builder_document_json: None,
            term_ids,
            primary_category_term_id,
            page_layout_key,
            custom_template_key,
            seo,
            parent_id,
            menu_order,
        },
    ))
}
