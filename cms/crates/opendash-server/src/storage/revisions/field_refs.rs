use std::collections::{BTreeMap, BTreeSet};

use opendash_core::content::{ContentSupport, EffectiveFieldStatus, FieldRole, PublicExposure};
use opendash_core::field_schema::FieldSpec;
use opendash_core::revisions::{
    CanonicalRevisionPayload, MissingMediaRef, MissingRefsReport, MissingTemplateRef,
    canonical_json_value,
};
use opendash_core::theme::TemplateCandidateKind;
use serde_json::Value;
use sqlx::SqliteConnection;

use crate::error::ApiResult;

use super::builder_refs::inspect_builder_doc_refs;
use super::custom_field_refs::inspect_field_map_refs;
use super::ref_cache::InspectionCaches;
use super::snapshot_queries::{LiveEntryState, decode_builder_document_for_payload};
use super::types::{can_view_body, is_body_editable, is_field_editable};

/// Inspects a candidate snapshot against current DB state across all 13 categories.
pub(super) async fn inspect_missing_refs_in_conn(
    conn: &mut SqliteConnection,
    live: &LiveEntryState,
    payload: &CanonicalRevisionPayload,
    caller_role: FieldRole,
    restore_editable_scope: bool,
) -> ApiResult<MissingRefsReport> {
    let mut report = MissingRefsReport::default();
    let mut cache = InspectionCaches::load(&mut *conn, &live.content_type_key).await?;
    let body_viewable = can_view_body(&live.effective, &live.content_type_key, caller_role);
    let body_editable = is_body_editable(&live.effective, &live.content_type_key, caller_role);
    let inspect_body = if restore_editable_scope {
        body_editable
    } else {
        body_viewable
    };

    if live.content_type_key != "pattern" {
        collect_non_editable_diff(
            live,
            payload,
            caller_role,
            body_viewable,
            body_editable,
            &mut report,
        );
        let (allowed_specs, scoped_values) = collect_scoped_custom_fields(
            live,
            payload,
            caller_role,
            restore_editable_scope,
            inspect_body,
            &mut report,
        );

        inspect_field_map_refs(
            &mut *conn,
            &mut cache,
            &allowed_specs,
            &scoped_values,
            &allowed_specs,
            &scoped_values,
            true,
            Some(live.locale.as_str()),
            None,
            None,
            None,
            &mut report,
        )
        .await?;

        inspect_entry_taxonomies(&mut *conn, &mut cache, live, payload, &mut report).await?;

        if let Some(ref seo) = payload.seo
            && let Some(ref og_id) = seo.og_image_id
            && !cache.media_ok(&mut *conn, og_id, Some("image")).await?
        {
            report.missing_media.push(MissingMediaRef {
                media_id: og_id.clone(),
                location: "seo.og_image_id".to_owned(),
                expected_kind: "image".to_owned(),
                required: false,
            });
        }

        inspect_missing_templates(&mut *conn, &mut cache, live, payload, &mut report).await?;
    }

    if inspect_body
        && let Some(ref doc) = decode_builder_document_for_payload(&live.content_type_key, payload)
    {
        let live_doc = decode_builder_document_for_payload(&live.content_type_key, &live.payload);
        let allowed_context_fields = build_allowed_context_fields(live, caller_role);
        inspect_builder_doc_refs(
            &mut *conn,
            &mut cache,
            live,
            doc,
            live_doc.as_ref(),
            caller_role,
            &allowed_context_fields,
            &mut report,
        )
        .await?;
    }

    report.missing_entry_ids.sort_unstable();
    report.missing_entry_ids.dedup();
    report.missing_term_ids.sort_unstable();
    report.missing_term_ids.dedup();
    report.missing_pattern_ids.sort_unstable();
    report.missing_pattern_ids.dedup();
    report.missing_contact_form_ids.sort_unstable();
    report.missing_contact_form_ids.dedup();
    Ok(report)
}

fn collect_non_editable_diff(
    live: &LiveEntryState,
    payload: &CanonicalRevisionPayload,
    caller_role: FieldRole,
    body_viewable: bool,
    body_editable: bool,
    report: &mut MissingRefsReport,
) {
    for (key, field) in &live.effective.effective_fields {
        if caller_role >= field.definition.effective_acl().0
            && !is_field_editable(field, caller_role)
        {
            let cur_val = live.payload.values.get(key).map(canonical_json_value);
            let target_val = payload.values.get(key).map(canonical_json_value);
            if cur_val != target_val {
                report.non_editable_fields.push(key.clone());
            }
        }
    }
    if body_viewable
        && !body_editable
        && !report.non_editable_fields.iter().any(|k| k == "body")
        && (live.payload.content_mode != payload.content_mode
            || live.payload.values.get("body") != payload.values.get("body"))
    {
        report.non_editable_fields.push("body".to_owned());
    }
}

fn collect_scoped_custom_fields(
    live: &LiveEntryState,
    payload: &CanonicalRevisionPayload,
    caller_role: FieldRole,
    restore_editable_scope: bool,
    inspect_body: bool,
    report: &mut MissingRefsReport,
) -> (Vec<FieldSpec>, BTreeMap<String, Value>) {
    let allowed_specs: Vec<FieldSpec> = live
        .effective
        .effective_fields
        .iter()
        .filter_map(|(k, fe)| {
            if k == "body" {
                return None;
            }
            let ok = if restore_editable_scope {
                is_field_editable(fe, caller_role)
            } else {
                caller_role >= fe.definition.effective_acl().0
                    && fe.effective_status == EffectiveFieldStatus::Active
            };
            ok.then(|| fe.definition.clone())
        })
        .collect();
    let allowed_keys: BTreeSet<&str> = allowed_specs.iter().map(|s| s.key.as_str()).collect();
    let mut scoped_values = BTreeMap::new();
    let mut dropped_keys = Vec::new();

    for (k, v) in &payload.values {
        if k == "body" {
            if inspect_body
                && matches!(live.content_type_key.as_str(), "post" | "page")
                && !(live
                    .effective
                    .base
                    .descriptor
                    .settings
                    .supports(ContentSupport::Editor)
                    || live
                        .effective
                        .base
                        .descriptor
                        .settings
                        .supports(ContentSupport::Builder))
            {
                dropped_keys.push(k.clone());
            }
            continue;
        }
        if live.effective.effective_fields.get(k).is_some_and(|fe| {
            caller_role >= fe.definition.effective_acl().0
                && fe.effective_status == EffectiveFieldStatus::Withdrawn
        }) {
            continue;
        }
        if allowed_keys.contains(k.as_str()) {
            if k == "featured_image"
                && matches!(live.content_type_key.as_str(), "post" | "page")
                && !live
                    .effective
                    .base
                    .descriptor
                    .settings
                    .supports(ContentSupport::FeaturedImage)
            {
                if !restore_editable_scope
                    || live
                        .effective
                        .effective_fields
                        .get(k)
                        .is_some_and(|fe| is_field_editable(fe, caller_role))
                {
                    dropped_keys.push(k.clone());
                }
                continue;
            }
            scoped_values.insert(k.clone(), v.clone());
        } else {
            let should_report = live.effective.effective_fields.get(k).map_or(true, |fe| {
                if restore_editable_scope {
                    is_field_editable(fe, caller_role)
                } else {
                    caller_role >= fe.definition.effective_acl().0
                }
            });
            if should_report {
                dropped_keys.push(k.clone());
            }
        }
    }
    dropped_keys.sort();
    dropped_keys.dedup();
    report.dropped_field_keys = dropped_keys;
    (allowed_specs, scoped_values)
}

async fn inspect_entry_taxonomies(
    conn: &mut SqliteConnection,
    cache: &mut InspectionCaches,
    live: &LiveEntryState,
    payload: &CanonicalRevisionPayload,
    report: &mut MissingRefsReport,
) -> ApiResult<()> {
    let mut missing_term_set: BTreeSet<i64> = report.missing_term_ids.iter().copied().collect();
    for &tid in &payload.term_ids {
        let valid = matches!(
            cache.term_info(&mut *conn, tid).await?,
            Some((ref kind, ref loc))
                if loc == &live.locale && cache.eligible_taxonomies.contains(kind)
        );
        if !valid {
            missing_term_set.insert(tid);
        }
    }
    if live.content_type_key == "post"
        && let Some(ptid) = payload.primary_category_term_id
    {
        let info = cache.term_info(&mut *conn, ptid).await?;
        if !matches!(info, Some((ref kind, ref loc)) if loc == &live.locale && kind == "category") {
            missing_term_set.insert(ptid);
        }
    }
    report.missing_term_ids = missing_term_set.into_iter().collect();
    Ok(())
}

pub(super) fn build_allowed_context_fields(
    live: &LiveEntryState,
    caller_role: FieldRole,
) -> Vec<String> {
    live.effective
        .effective_fields
        .iter()
        .filter(|(_, fe)| {
            fe.effective_status == EffectiveFieldStatus::Active
                && fe.definition.effective_acl().0 <= caller_role
                && fe.definition.effective_acl().2 == PublicExposure::Public
        })
        .map(|(k, _)| k.clone())
        .collect()
}

async fn inspect_missing_templates(
    conn: &mut SqliteConnection,
    cache: &mut InspectionCaches,
    live: &LiveEntryState,
    payload: &CanonicalRevisionPayload,
    report: &mut MissingRefsReport,
) -> ApiResult<()> {
    let layout_changed = payload.page_layout_key.is_some()
        && payload.page_layout_key != live.payload.page_layout_key;
    let custom_changed = payload.custom_template_key.is_some()
        && payload.custom_template_key != live.payload.custom_template_key;
    if !layout_changed && !custom_changed {
        return Ok(());
    }
    let manifest = cache.ensure_manifest(&mut *conn).await?.cloned();
    if layout_changed && let Some(ref key) = payload.page_layout_key {
        let ok = manifest
            .as_ref()
            .is_some_and(|m| m.page_layouts.iter().any(|l| &l.key == key));
        if !ok {
            report.missing_templates.push(MissingTemplateRef {
                kind: "page_layout".to_owned(),
                key: key.clone(),
            });
        }
    }
    if custom_changed && let Some(ref key) = payload.custom_template_key {
        let ok = manifest.as_ref().is_some_and(|m| {
            m.templates.iter().any(|tpl| {
                tpl.kind == TemplateCandidateKind::SingleCustom
                    && tpl.custom_key.as_deref() == Some(key.as_str())
                    && tpl
                        .allowed_types
                        .iter()
                        .any(|t| t == &live.content_type_key)
            })
        });
        if !ok {
            report.missing_templates.push(MissingTemplateRef {
                kind: "custom_template".to_owned(),
                key: key.clone(),
            });
        }
    }
    Ok(())
}
