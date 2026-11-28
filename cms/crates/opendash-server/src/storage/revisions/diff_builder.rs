use std::collections::{BTreeMap, BTreeSet};

use opendash_core::content::{
    ContentMode, EffectiveContentSchema, EffectiveFieldStatus, FieldRole,
};
use opendash_core::field_schema::FieldType;
use opendash_core::revisions::{
    BlockDiffSummary, CanonicalRevisionPayload, CanonicalRevisionSeo, FieldDiffItem,
    ModeTransition, RevisionDiffPayload, RevisionTermRef, ScalarChange, TaxonomyDiffSummary,
    TextDiffResult, canonical_json_value, compute_builder_block_diff, compute_line_word_diff,
};
use serde_json::Value;
use sqlx::SqliteConnection;

use crate::error::ApiResult;

use super::snapshot_queries::{LiveEntryState, decode_builder_document_for_payload};
use super::types::{can_view_body, is_field_editable};

pub(super) async fn build_revision_diff(
    conn: &mut SqliteConnection,
    live: &LiveEntryState,
    from: &CanonicalRevisionPayload,
    to: &CanonicalRevisionPayload,
    caller_role: FieldRole,
) -> ApiResult<RevisionDiffPayload> {
    let title_diff = compute_line_word_diff(&from.title, &to.title);
    let body_viewable = can_view_body(&live.effective, &live.content_type_key, caller_role);
    let mut has_hidden_admin_changes = false;

    let (mode_transition, body_text_diff, body_block_diff) =
        build_body_diffs(live, from, to, body_viewable, &mut has_hidden_admin_changes);

    let mut field_diffs = Vec::new();
    let mut seen = BTreeSet::new();
    if live.content_type_key != "pattern" {
        for (key, fe) in &live.effective.effective_fields {
            seen.insert(key.as_str());
            if key == "body" {
                continue;
            }
            let bv = from.values.get(key).unwrap_or(&Value::Null);
            let av = to.values.get(key).unwrap_or(&Value::Null);
            let changed = canonical_json_value(bv) != canonical_json_value(av);
            if caller_role < fe.definition.effective_acl().0 {
                if changed {
                    has_hidden_admin_changes = true;
                }
                continue;
            }
            if changed {
                field_diffs.push(FieldDiffItem {
                    key: key.clone(),
                    label: fe.definition.label.clone(),
                    field_type: field_type_label(fe.definition.field_type).to_owned(),
                    editable: is_field_editable(fe, caller_role),
                    before: bv.clone(),
                    after: av.clone(),
                });
            }
        }
        let mut extra_keys: BTreeSet<&str> = from.values.keys().map(String::as_str).collect();
        extra_keys.extend(to.values.keys().map(String::as_str));
        for key in extra_keys {
            if key == "body" || seen.contains(key) {
                continue;
            }
            let bv = from.values.get(key).unwrap_or(&Value::Null);
            let av = to.values.get(key).unwrap_or(&Value::Null);
            if canonical_json_value(bv) != canonical_json_value(av) {
                field_diffs.push(FieldDiffItem {
                    key: key.to_owned(),
                    label: key.to_owned(),
                    field_type: "unknown".to_owned(),
                    editable: true,
                    before: bv.clone(),
                    after: av.clone(),
                });
            }
        }
    }

    let from_terms: BTreeSet<i64> = from.term_ids.iter().copied().collect();
    let to_terms: BTreeSet<i64> = to.term_ids.iter().copied().collect();
    let added_ids: Vec<i64> = to_terms.difference(&from_terms).copied().collect();
    let removed_ids: Vec<i64> = from_terms.difference(&to_terms).copied().collect();
    let mut lookup_ids = Vec::new();
    lookup_ids.extend_from_slice(&added_ids);
    lookup_ids.extend_from_slice(&removed_ids);
    if from.primary_category_term_id != to.primary_category_term_id {
        if let Some(id) = from.primary_category_term_id {
            lookup_ids.push(id);
        }
        if let Some(id) = to.primary_category_term_id {
            lookup_ids.push(id);
        }
    }
    let hydrated = hydrate_terms_in_conn(&mut *conn, &lookup_ids).await?;
    let term_map: BTreeMap<i64, RevisionTermRef> =
        hydrated.into_iter().map(|t| (t.id, t)).collect();
    let resolve_term = |id: i64| -> RevisionTermRef {
        term_map.get(&id).cloned().unwrap_or(RevisionTermRef {
            id,
            name: format!("#{id}"),
            taxonomy_key: None,
            missing: true,
        })
    };
    let taxonomy_diff = TaxonomyDiffSummary {
        added_terms: added_ids.into_iter().map(resolve_term).collect(),
        removed_terms: removed_ids.into_iter().map(resolve_term).collect(),
        primary_category_before: if from.primary_category_term_id != to.primary_category_term_id {
            from.primary_category_term_id.map(resolve_term)
        } else {
            None
        },
        primary_category_after: if from.primary_category_term_id != to.primary_category_term_id {
            to.primary_category_term_id.map(resolve_term)
        } else {
            None
        },
    };

    let layout_diff = (from.page_layout_key != to.page_layout_key).then(|| ScalarChange {
        before: from.page_layout_key.clone(),
        after: to.page_layout_key.clone(),
    });
    let template_diff =
        (from.custom_template_key != to.custom_template_key).then(|| ScalarChange {
            before: from.custom_template_key.clone(),
            after: to.custom_template_key.clone(),
        });
    let seo_diff = build_seo_diff(from.seo.as_ref(), to.seo.as_ref());
    let parent_diff = (from.parent_id != to.parent_id).then(|| ScalarChange {
        before: from.parent_id,
        after: to.parent_id,
    });
    let menu_order_diff = (from.menu_order != to.menu_order).then(|| ScalarChange {
        before: from.menu_order,
        after: to.menu_order,
    });

    Ok(RevisionDiffPayload {
        title_diff,
        mode_transition,
        body_text_diff,
        body_block_diff,
        field_diffs,
        taxonomy_diff,
        layout_diff,
        template_diff,
        seo_diff,
        parent_diff,
        menu_order_diff,
        has_hidden_admin_changes,
    })
}

fn build_body_diffs(
    live: &LiveEntryState,
    from: &CanonicalRevisionPayload,
    to: &CanonicalRevisionPayload,
    body_viewable: bool,
    has_hidden_admin_changes: &mut bool,
) -> (
    Option<ModeTransition>,
    Option<TextDiffResult>,
    Option<BlockDiffSummary>,
) {
    let from_raw = body_text_for_diff(&live.content_type_key, from);
    let to_raw = body_text_for_diff(&live.content_type_key, to);
    if !body_viewable {
        if from_raw != to_raw || from.content_mode != to.content_mode {
            *has_hidden_admin_changes = true;
        }
        return (None, None, None);
    }

    let mode_transition = (from.content_mode != to.content_mode).then_some(ModeTransition {
        from: from.content_mode,
        to: to.content_mode,
    });
    let text_diff = compute_line_word_diff(&from_raw, &to_raw);
    let from_doc = decode_builder_document_for_payload(&live.content_type_key, from);
    let to_doc = decode_builder_document_for_payload(&live.content_type_key, to);
    let block_diff = if from.content_mode == ContentMode::Builder
        || to.content_mode == ContentMode::Builder
        || live.content_type_key == "pattern"
        || opendash_core::builder_blocks::contains_blocks(&from_raw)
        || opendash_core::builder_blocks::contains_blocks(&to_raw)
    {
        Some(compute_builder_block_diff(
            from_doc.as_ref(),
            to_doc.as_ref(),
        ))
    } else {
        None
    };

    (mode_transition, Some(text_diff), block_diff)
}

fn body_text_for_diff(type_key: &str, payload: &CanonicalRevisionPayload) -> String {
    if type_key == "pattern" {
        payload.builder_document_json.clone().unwrap_or_default()
    } else {
        payload
            .values
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    }
}

fn build_seo_diff(
    from: Option<&CanonicalRevisionSeo>,
    to: Option<&CanonicalRevisionSeo>,
) -> Option<Vec<FieldDiffItem>> {
    if from == to {
        return None;
    }
    let f = from.cloned().unwrap_or(CanonicalRevisionSeo {
        title: None,
        description: None,
        og_image_id: None,
        canonical_url: None,
        noindex: false,
    });
    let t = to.cloned().unwrap_or(CanonicalRevisionSeo {
        title: None,
        description: None,
        og_image_id: None,
        canonical_url: None,
        noindex: false,
    });
    let mut items = Vec::new();
    let pairs: [(&str, &str, &str, Value, Value); 5] = [
        (
            "seo.title",
            "SEO Title",
            "text",
            f.title.into(),
            t.title.into(),
        ),
        (
            "seo.description",
            "SEO Description",
            "textarea",
            f.description.into(),
            t.description.into(),
        ),
        (
            "seo.og_image_id",
            "OG Image",
            "media",
            f.og_image_id.into(),
            t.og_image_id.into(),
        ),
        (
            "seo.canonical_url",
            "Canonical URL",
            "url",
            f.canonical_url.into(),
            t.canonical_url.into(),
        ),
        (
            "seo.noindex",
            "Noindex",
            "boolean",
            f.noindex.into(),
            t.noindex.into(),
        ),
    ];
    for (key, label, ft, bv, av) in pairs {
        if bv != av {
            items.push(FieldDiffItem {
                key: key.to_owned(),
                label: label.to_owned(),
                field_type: ft.to_owned(),
                editable: true,
                before: bv,
                after: av,
            });
        }
    }
    (!items.is_empty()).then_some(items)
}

pub(super) async fn hydrate_terms_in_conn(
    conn: &mut SqliteConnection,
    term_ids: &[i64],
) -> ApiResult<Vec<RevisionTermRef>> {
    let mut unique: Vec<i64> = term_ids.to_vec();
    unique.sort_unstable();
    unique.dedup();
    if unique.is_empty() {
        return Ok(Vec::new());
    }
    let raw = serde_json::to_string(&unique)?;
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT id, name, kind FROM taxonomy_terms WHERE id IN (SELECT value FROM json_each(?1))",
    )
    .bind(raw)
    .fetch_all(&mut *conn)
    .await?;
    let mut by_id: BTreeMap<i64, (String, String)> = rows
        .into_iter()
        .map(|(id, name, kind)| (id, (name, kind)))
        .collect();
    let mut out = Vec::with_capacity(unique.len());
    for id in unique {
        match by_id.remove(&id) {
            Some((name, kind)) => out.push(RevisionTermRef {
                id,
                name,
                taxonomy_key: Some(kind),
                missing: false,
            }),
            None => out.push(RevisionTermRef {
                id,
                name: format!("#{id}"),
                taxonomy_key: None,
                missing: true,
            }),
        }
    }
    Ok(out)
}

pub(super) fn field_type_label(ft: FieldType) -> &'static str {
    match ft {
        FieldType::Text => "text",
        FieldType::Textarea => "textarea",
        FieldType::Number => "number",
        FieldType::Range => "range",
        FieldType::Email => "email",
        FieldType::Url => "url",
        FieldType::Secret => "secret",
        FieldType::Richtext => "richtext",
        FieldType::Media => "media",
        FieldType::Gallery => "gallery",
        FieldType::Link => "link",
        FieldType::Select => "select",
        FieldType::Checkbox => "checkbox",
        FieldType::RadioButtons => "radio_buttons",
        FieldType::Boolean => "boolean",
        FieldType::Color => "color",
        FieldType::Icon => "icon",
        FieldType::Date => "date",
        FieldType::Datetime => "datetime",
        FieldType::Time => "time",
        FieldType::Spacing => "spacing",
        FieldType::EntryRef => "entry_ref",
        FieldType::TermRef => "term_ref",
        FieldType::Group => "group",
        FieldType::Repeater => "repeater",
        FieldType::Tab => "tab",
        FieldType::Accordion => "accordion",
        FieldType::Message => "message",
    }
}

pub(super) fn has_hidden_fields_in_map(
    effective: &EffectiveContentSchema,
    values: &BTreeMap<String, Value>,
    caller_role: FieldRole,
) -> bool {
    effective.effective_fields.iter().any(|(k, fe)| {
        caller_role < fe.definition.effective_acl().0
            && (values.contains_key(k)
                || !matches!(fe.effective_status, EffectiveFieldStatus::Withdrawn))
    })
}
