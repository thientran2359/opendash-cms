use std::collections::BTreeMap;

use opendash_core::content::{ContentMode, FieldRole, PatchField};
use opendash_core::hooks::{ExecutionPrincipal, HookEvent};
use opendash_core::revisions::RevisionSource;
use serde_json::{Value, json};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::auth::{Role, SessionContext};
use crate::cache::SiteCacheEpoch;
use crate::error::ApiError;
use crate::hooks::{AsyncHookJob, HookDispatcher};
use crate::operations::MutationLease;
use crate::storage::builder::{entry_builder_snapshot, replace_entry_builder_refs};
use crate::storage::entries::{
    self, actor_field_role, decoded_with_terms, verify_schema_token_in_tx,
};

use super::capture::{
    capture_baseline_if_missing_in_tx, capture_entry_revision_in_tx, clear_actor_autosave_in_tx,
    parse_content_mode,
};
use super::field_refs::inspect_missing_refs_in_conn;
use super::restore_preflight::{
    PreparedRestore, canonical_seo_to_input, map_validation_api_err, prepare_restore_in_tx,
    veto_err,
};
use super::snapshot_queries::{
    check_not_trashed, load_live_entry_state_in_conn, load_revision_record_in_conn,
    parse_revision_source, require_writable_cpt_and_locale_in_conn,
};
use super::types::{
    RestoreError, RestoreRevisionOutcome, RevisionActor, RevisionSummaryDto, caller_field_role,
    is_body_editable,
};

/// Restores an immutable revision via 4 phases (read preflight, outside-lock veto hook,
/// single reserved SQLite writer TX, and post-commit non-fatal projections + async hooks).
#[allow(clippy::too_many_arguments)]
pub async fn restore_entry_revision(
    db: &SqlitePool,
    hooks: &HookDispatcher,
    actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
    rev_id: i64,
    expected_version: i64,
    acknowledge_missing_refs: bool,
    lease: MutationLease,
) -> Result<RestoreRevisionOutcome, RestoreError> {
    if type_key == "pattern" && actor.user.role != Role::Admin {
        return Err(
            ApiError::forbidden("Chỉ Quản trị viên mới được phép thao tác Pattern.").into(),
        );
    }
    let caller_role = caller_field_role(actor);

    // Phase 1: Read-only preflight transaction
    let mut read_tx = db.begin().await?;
    let prepared = prepare_restore_in_tx(
        &mut read_tx,
        caller_role,
        type_key,
        entry_id,
        rev_id,
        expected_version,
        acknowledge_missing_refs,
    )
    .await?;
    read_tx.commit().await?;

    // Phase 2: Before-save veto hook outside writer lock
    let candidate_json = json!({
        "operation": "update",
        "type_key": type_key,
        "entry_id": entry_id,
        "expected_version": expected_version,
        "locale": &prepared.locale,
        "translation_group_id": &prepared.translation_group_id,
        "title": &prepared.candidate_payload.title,
        "slug": &prepared.slug,
        "candidate_values": &prepared.candidate_payload.values,
        "term_ids": &prepared.candidate_payload.term_ids,
        "restore_from_revision": prepared.revision_number,
    });
    if let Err(veto) = hooks
        .dispatch_content_before_save_for_actor(type_key, caller_role, candidate_json)
        .await
    {
        return Err(RestoreError::ValidationVeto(veto));
    }

    // Phase 3: Single reserved writer transaction
    let outcome = execute_restore_in_writer_tx(
        db,
        actor,
        caller_role,
        type_key,
        entry_id,
        rev_id,
        expected_version,
        &prepared,
    )
    .await?;

    // Phase 4: Post-commit non-fatal projections + async hooks
    SiteCacheEpoch::global().bump();
    let _ = crate::storage::entry_lifecycle::recompute_projections_nonfatal(
        db,
        hooks,
        entry_id,
        prepared.was_published,
    )
    .await;

    let principal = match actor.user.role {
        Role::Admin => ExecutionPrincipal::Admin,
        Role::Editor => ExecutionPrincipal::Editor,
    };
    let (new_version, pub_status) = match &outcome {
        RestoreRevisionOutcome::Entry { entry, .. } => {
            (entry.version, entry.publication_status.as_str())
        }
        RestoreRevisionOutcome::Pattern {
            version,
            publication_status,
            ..
        } => (*version, publication_status.as_str()),
    };
    if prepared.was_published && pub_status == "published" {
        hooks.enqueue_async(AsyncHookJob {
            event: HookEvent::ContentAfterPublish,
            principal,
            locale: Some(prepared.locale.clone()),
            type_key: Some(type_key.to_owned()),
            payload: json!({
                "type_key": type_key,
                "id": entry_id,
                "entry_id": entry_id,
                "version": new_version,
                "locale": &prepared.locale,
                "publication_status": pub_status,
                "translation_group_id": &prepared.translation_group_id,
            }),
            dispatch_depth: 0,
            lease: lease.clone(),
        });
    }
    let after_save_locale = prepared.locale;
    hooks.enqueue_async(AsyncHookJob {
        event: HookEvent::ContentAfterSave,
        principal,
        locale: Some(after_save_locale.clone()),
        type_key: Some(type_key.to_owned()),
        payload: json!({
            "type_key": type_key,
            "id": entry_id,
            "entry_id": entry_id,
            "operation": "update",
            "version": new_version,
            "locale": &after_save_locale,
            "translation_group_id": &prepared.translation_group_id,
        }),
        dispatch_depth: 0,
        lease,
    });

    Ok(outcome)
}

#[allow(clippy::too_many_arguments)]
async fn execute_restore_in_writer_tx(
    db: &SqlitePool,
    actor: &SessionContext,
    caller_role: FieldRole,
    type_key: &str,
    entry_id: i64,
    rev_id: i64,
    expected_version: i64,
    prepared: &PreparedRestore,
) -> Result<RestoreRevisionOutcome, RestoreError> {
    let mut tx = entries::begin_entry_write(db, actor).await?;
    let effective = verify_schema_token_in_tx(&mut tx, type_key, &prepared.schema_token).await?;
    let live = load_live_entry_state_in_conn(&mut tx, type_key, entry_id).await?;
    let _ = load_revision_record_in_conn(&mut tx, type_key, entry_id, rev_id).await?;

    require_writable_cpt_and_locale_in_conn(&mut tx, type_key, &live.locale).await?;
    if live.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi trước khi khôi phục.",
        )
        .into());
    }
    check_not_trashed(live.trashed_at.as_deref())?;

    let missing_now = inspect_missing_refs_in_conn(
        &mut tx,
        &live,
        &prepared.candidate_payload,
        caller_role,
        true,
    )
    .await?;
    if missing_now.has_blocking_refs() {
        return Err(RestoreError::MissingRefs(missing_now));
    }

    capture_baseline_if_missing_in_tx(&mut tx, entry_id).await?;

    if type_key == "pattern" {
        let mode_db = match prepared.candidate_payload.content_mode {
            ContentMode::Editor => "editor",
            ContentMode::Builder => "builder",
        };
        if let Some(ref raw_doc) = prepared.candidate_payload.builder_document_json {
            sqlx::query(
                "INSERT INTO entry_builder_documents(entry_id, document_json, updated_at)
                 VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(entry_id) DO UPDATE SET document_json=excluded.document_json, updated_at=excluded.updated_at",
            )
            .bind(entry_id)
            .bind(raw_doc)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query("DELETE FROM entry_builder_documents WHERE entry_id = ?")
                .bind(entry_id)
                .execute(&mut *tx)
                .await?;
        }
        replace_entry_builder_refs(&mut tx, entry_id, prepared.candidate_doc.as_ref()).await?;
        sqlx::query(
            "UPDATE content_entries
                SET title = ?, content_mode = ?, version = version + 1,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
              WHERE content_type_key = 'pattern' AND id = ? AND version = ?",
        )
        .bind(&prepared.candidate_payload.title)
        .bind(mode_db)
        .bind(entry_id)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?;

        capture_entry_revision_in_tx(
            &mut tx,
            entry_id,
            RevisionSource::Restore,
            Some(prepared.revision_number),
            RevisionActor::User(actor),
        )
        .await?;
        clear_actor_autosave_in_tx(&mut tx, entry_id, actor.user.id).await?;
        let opened = entry_builder_snapshot(&mut tx, type_key, entry_id)
            .await?
            .opened;
        let rev_summary = load_latest_revision_summary_in_tx(&mut tx, entry_id).await?;
        tx.commit().await?;

        return Ok(RestoreRevisionOutcome::Pattern {
            entry_id: opened.entry_id,
            type_key: opened.type_key,
            slug: live.slug,
            title: prepared.candidate_payload.title.clone(),
            content_mode: opened.content_mode,
            version: opened.version,
            updated_at: opened.updated_at,
            document: opened.document,
            publication_status: opened.publication_status,
            publication_policy: opened.publication_policy,
            diagnostics: opened.diagnostics,
            had_freeform_wrap: opened.had_freeform_wrap,
            had_recovery: opened.had_recovery,
            revision: rev_summary,
        });
    }

    let values_json = serde_json::to_string(&prepared.candidate_payload.values)?;
    if values_json.len() > crate::routes::entries::MAX_CONTENT_ENTRY_JSON_BYTES {
        return Err(veto_err(
            "restore_validation_failed",
            "Dữ liệu nội dung vượt giới hạn 768 KiB.",
            BTreeMap::new(),
        ));
    }
    let previous_body = live
        .payload
        .values
        .get("body")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let body_editable = is_body_editable(&effective, type_key, caller_role);

    let res = sqlx::query(
        "UPDATE content_entries
            SET title = ?, values_json = ?, content_mode = ?,
                page_layout_key = ?, custom_template_key = ?,
                primary_category_term_id = ?,
                parent_id = ?, menu_order = ?,
                version = version + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
          WHERE content_type_key = ? AND id = ? AND version = ? AND trashed_at IS NULL",
    )
    .bind(&prepared.candidate_payload.title)
    .bind(&values_json)
    .bind(prepared.candidate_payload.content_mode.as_str())
    .bind(prepared.candidate_payload.page_layout_key.as_deref())
    .bind(prepared.candidate_payload.custom_template_key.as_deref())
    .bind(prepared.candidate_payload.primary_category_term_id)
    .bind(prepared.candidate_payload.parent_id)
    .bind(prepared.candidate_payload.menu_order)
    .bind(type_key)
    .bind(entry_id)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() != 1 {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi trước khi khôi phục.",
        )
        .into());
    }

    sqlx::query("DELETE FROM entry_builder_documents WHERE entry_id = ?")
        .bind(entry_id)
        .execute(&mut *tx)
        .await?;
    entries::replace_term_ids(&mut tx, entry_id, &prepared.candidate_payload.term_ids).await?;
    let loc_ctx = entries::build_entry_location_ctx_in_conn(
        &mut *tx,
        type_key,
        Some(entry_id),
        &live.locale,
        caller_role,
        prepared.candidate_payload.page_layout_key.clone(),
        prepared.candidate_payload.custom_template_key.clone(),
        &prepared.candidate_payload.term_ids,
    )
    .await?;
    entries::sync_entry_values_json_with_terms_in_tx(
        &mut tx,
        entry_id,
        &effective,
        &loc_ctx,
        &prepared.candidate_payload.term_ids,
    )
    .await
    .map_err(|e| map_validation_api_err(e, None))?;
    if prepared.was_published {
        entries::enforce_taxonomy_publish_rules_in_tx(
            &mut tx,
            type_key,
            entry_id,
            &live.locale,
            Some(&effective),
            Some(&loc_ctx),
        )
        .await
        .map_err(|e| map_validation_api_err(e, Some("term_ids")))?;
    }

    if body_editable {
        entries::sync_entry_body_refs_in_conn(
            &mut tx,
            entry_id,
            &effective,
            &prepared.candidate_payload.values,
            previous_body.as_deref(),
            actor_field_role(actor.user.role),
        )
        .await
        .map_err(|e| map_validation_api_err(e, Some("body")))?;
    } else {
        entries::sync_entry_field_media_refs_in_conn(
            &mut tx,
            entry_id,
            &effective,
            &prepared.candidate_payload.values,
        )
        .await
        .map_err(|e| map_validation_api_err(e, None))?;
    }

    crate::storage::permalinks::sync_claims_in_tx(&mut tx).await?;
    let seo_patch = match &prepared.candidate_payload.seo {
        None => PatchField::Clear,
        Some(seo) => PatchField::Set(canonical_seo_to_input(seo)),
    };
    let new_version = expected_version + 1;
    let seo_snapshot = crate::storage::seo::apply_entry_seo_patch_in_tx(
        &mut tx,
        entry_id,
        new_version,
        &seo_patch,
    )
    .await
    .map_err(|e| map_validation_api_err(e, Some("seo")))?;

    capture_entry_revision_in_tx(
        &mut tx,
        entry_id,
        RevisionSource::Restore,
        Some(prepared.revision_number),
        RevisionActor::User(actor),
    )
    .await?;
    clear_actor_autosave_in_tx(&mut tx, entry_id, actor.user.id).await?;

    let row: entries::EntryRow = sqlx::query_as(
        "SELECT id, content_type_key, slug, title, values_json, publication_status, version, created_at, updated_at, published_at
           FROM content_entries WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(entry_id)
    .fetch_one(&mut *tx)
    .await?;
    let mut record = decoded_with_terms(&mut tx, row).await?;
    let custom_path: Option<String> =
        sqlx::query_scalar("SELECT path FROM entry_public_paths WHERE entry_id = ?")
            .bind(entry_id)
            .fetch_optional(&mut *tx)
            .await?;
    let rev_summary = load_latest_revision_summary_in_tx(&mut tx, entry_id).await?;
    tx.commit().await?;

    entries::filter_entry_values_for_role(
        &mut record,
        &effective,
        actor_field_role(actor.user.role),
    );

    Ok(RestoreRevisionOutcome::Entry {
        entry: Box::new(record),
        custom_path,
        seo: seo_snapshot,
        schema_token: effective.schema_token,
        revision: rev_summary,
    })
}

async fn load_latest_revision_summary_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    entry_id: i64,
) -> Result<RevisionSummaryDto, RestoreError> {
    let row = sqlx::query(
        "SELECT id, revision_number, content_hash, source, restored_from_revision_number,
                author_user_id, author_username, title, content_mode, entry_version, created_at
           FROM entry_revisions
          WHERE entry_id = ?
          ORDER BY revision_number DESC
          LIMIT 1",
    )
    .bind(entry_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(RevisionSummaryDto {
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
    })
}
