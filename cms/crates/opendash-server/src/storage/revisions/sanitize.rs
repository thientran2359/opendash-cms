use std::collections::{BTreeMap, BTreeSet};

use opendash_core::block_schema::RegisteredBlockSchema;
use opendash_core::builder::{
    BindingTarget, BuilderDocument, ElementContent, KnownElement, valid_button_url,
};
use opendash_core::content::{EffectiveContentSchema, FieldRole};
use opendash_core::field_schema::{FieldSpec, FieldType, SelectOptionsSource, evaluate_conditions};
use opendash_core::registry::{DeclarationState, ProviderStatus, RegistryCatalog};
use opendash_core::revisions::{CanonicalRevisionPayload, MissingRefsReport};
use serde_json::Value;

use crate::storage::media::valid_media_id;

use super::custom_field_refs::resolve_term_ref_taxonomy;
use super::snapshot_queries::LiveEntryState;
use super::types::{is_body_editable, is_field_editable};

struct SanitizeCtx<'a> {
    missing_media_ids: BTreeSet<&'a str>,
    missing_terms: BTreeSet<i64>,
    missing_entries: BTreeSet<i64>,
    missing_patterns: BTreeSet<i64>,
    missing_forms: BTreeSet<i64>,
    missing_list_nodes: BTreeSet<&'a str>,
    missing_block_nodes: BTreeSet<&'a str>,
    cleared_bindings: BTreeSet<&'a str>,
    missing_dynamic: BTreeMap<(&'a str, &'a str), BTreeSet<&'a str>>,
    dropped_block_fields: BTreeMap<&'a str, BTreeSet<&'a str>>,
}

impl<'a> SanitizeCtx<'a> {
    fn from_report(report: &'a MissingRefsReport) -> Self {
        let mut missing_dynamic: BTreeMap<(&'a str, &'a str), BTreeSet<&'a str>> = BTreeMap::new();
        for item in &report.missing_dynamic_options {
            missing_dynamic
                .entry((item.field_path.as_str(), item.source.as_str()))
                .or_default()
                .insert(item.value.as_str());
        }
        let mut dropped_block_fields: BTreeMap<&'a str, BTreeSet<&'a str>> = BTreeMap::new();
        for item in &report.dropped_block_field_keys {
            dropped_block_fields
                .entry(item.node_id.as_str())
                .or_default()
                .insert(item.field_path.as_str());
        }
        Self {
            missing_media_ids: report
                .missing_media
                .iter()
                .map(|m| m.media_id.as_str())
                .collect(),
            missing_terms: report.missing_term_ids.iter().copied().collect(),
            missing_entries: report.missing_entry_ids.iter().copied().collect(),
            missing_patterns: report.missing_pattern_ids.iter().copied().collect(),
            missing_forms: report.missing_contact_form_ids.iter().copied().collect(),
            missing_list_nodes: report
                .missing_list_refs
                .iter()
                .map(|m| m.node_id.as_str())
                .collect(),
            missing_block_nodes: report
                .missing_block_types
                .iter()
                .map(|m| m.node_id.as_str())
                .collect(),
            cleared_bindings: report
                .cleared_binding_nodes
                .iter()
                .map(|m| m.node_id.as_str())
                .collect(),
            missing_dynamic,
            dropped_block_fields,
        }
    }
}

/// Sanitizes an editable revision snapshot according to `report`, removing dead references,
/// withdrawn fields/blocks, broken fallback bindings, and empty `un_emptiable` Repeater items/blocks.
pub(super) fn sanitize_snapshot_for_restore(
    live: &LiveEntryState,
    payload: &mut CanonicalRevisionPayload,
    mut doc: Option<&mut BuilderDocument>,
    report: &MissingRefsReport,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
    valid_category_primary_ids: &BTreeSet<i64>,
    caller_role: FieldRole,
) {
    let ctx = SanitizeCtx::from_report(report);
    let body_editable = is_body_editable(&live.effective, &live.content_type_key, caller_role);

    if live.content_type_key != "pattern" {
        for key in &report.dropped_field_keys {
            if key == "body" && !body_editable {
                continue;
            }
            payload.values.remove(key);
        }
        clean_custom_fields_map(&live.effective, caller_role, &ctx, &mut payload.values);

        if let Some(ref mut seo) = payload.seo
            && let Some(ref og) = seo.og_image_id
            && ctx.missing_media_ids.contains(og.as_str())
        {
            seo.og_image_id = None;
        }
        payload.seo = payload.seo.take().and_then(|s| s.into_normalized());

        payload
            .term_ids
            .retain(|id| !ctx.missing_terms.contains(id));
        payload.term_ids.sort_unstable();
        payload.term_ids.dedup();

        if live.content_type_key != "post" {
            payload.primary_category_term_id = None;
        } else if let Some(ptid) = payload.primary_category_term_id
            && (!payload.term_ids.contains(&ptid) || !valid_category_primary_ids.contains(&ptid))
        {
            payload.primary_category_term_id = None;
        }

        for mt in &report.missing_templates {
            match mt.kind.as_str() {
                "page_layout" => payload.page_layout_key = live.payload.page_layout_key.clone(),
                "custom_template" => {
                    payload.custom_template_key = live.payload.custom_template_key.clone();
                }
                _ => {}
            }
        }
        if report.parent_id.is_some() || !live.effective.base.descriptor.settings.hierarchical {
            payload.parent_id = None;
        }
    }

    if let Some(ref mut builder_doc) = doc
        && body_editable
    {
        sanitize_builder_document(builder_doc, &ctx, schemas);
    }
}

fn clean_custom_fields_map(
    effective: &EffectiveContentSchema,
    caller_role: FieldRole,
    ctx: &SanitizeCtx<'_>,
    values: &mut BTreeMap<String, Value>,
) {
    let specs: Vec<FieldSpec> = effective
        .effective_fields
        .iter()
        .filter(|(k, fe)| k.as_str() != "body" && is_field_editable(fe, caller_role))
        .map(|(_, fe)| fe.definition.clone())
        .collect();
    clean_field_map(&specs, None, values, ctx, false);
}

fn clean_field_map(
    specs: &[FieldSpec],
    block_node_id: Option<&str>,
    values: &mut BTreeMap<String, Value>,
    ctx: &SanitizeCtx<'_>,
    is_block: bool,
) -> bool {
    let root_snapshot = values.clone();
    if is_block
        && let Some(node_id) = block_node_id
        && let Some(dropped) = ctx.dropped_block_fields.get(node_id)
    {
        values.retain(|k, _| !dropped.contains(k.as_str()));
    }
    clean_field_map_with_root(
        specs,
        &specs.to_vec(),
        values,
        &root_snapshot,
        true,
        None,
        block_node_id,
        ctx,
        is_block,
    )
}

#[allow(clippy::too_many_arguments)]
fn clean_field_map_with_root(
    specs: &[FieldSpec],
    root_specs: &[FieldSpec],
    values: &mut BTreeMap<String, Value>,
    root_values: &BTreeMap<String, Value>,
    parent_visible: bool,
    path_prefix: Option<&str>,
    block_node_id: Option<&str>,
    ctx: &SanitizeCtx<'_>,
    is_block: bool,
) -> bool {
    for spec in specs {
        if spec.is_ui_only() || spec.field_type == FieldType::Secret {
            continue;
        }
        let field_path = match path_prefix {
            Some(p) => format!("{p}.{}", spec.key),
            None => spec.key.clone(),
        };
        if is_block
            && let Some(node_id) = block_node_id
            && let Some(dropped) = ctx.dropped_block_fields.get(node_id)
            && dropped.contains(field_path.as_str())
        {
            values.remove(spec.key.as_str());
        }
        let visible = parent_visible
            && evaluate_conditions(
                spec.conditions.as_ref(),
                specs,
                values,
                root_specs,
                root_values,
            );
        let un_emptiable = visible
            && (spec.effective_required()
                || (is_block
                    && spec.field_type == FieldType::Repeater
                    && spec.min_items.is_some_and(|m| m > 0)));

        match spec.field_type {
            FieldType::Media => {
                if let Some(id) = values
                    .get(spec.key.as_str())
                    .and_then(Value::as_str)
                    .map(str::trim)
                    && !id.is_empty()
                    && ctx.missing_media_ids.contains(id)
                {
                    if is_block {
                        values.insert(spec.key.clone(), Value::String(String::new()));
                        if un_emptiable {
                            return false;
                        }
                    } else {
                        values.remove(spec.key.as_str());
                    }
                }
            }
            FieldType::Gallery => {
                if let Some(arr) = values
                    .get_mut(spec.key.as_str())
                    .and_then(Value::as_array_mut)
                {
                    arr.retain(|v| {
                        v.as_str()
                            .map(str::trim)
                            .is_none_or(|id| !id.is_empty() && !ctx.missing_media_ids.contains(id))
                    });
                    if un_emptiable
                        && (arr.is_empty() || spec.min_items.is_some_and(|m| arr.len() < m))
                    {
                        if is_block {
                            return false;
                        }
                    } else if !un_emptiable
                        && !arr.is_empty()
                        && spec.min_items.is_some_and(|m| arr.len() < m)
                    {
                        arr.clear();
                    }
                }
            }
            FieldType::EntryRef => {
                if clean_id_field(values, spec, &ctx.missing_entries, un_emptiable)
                    && un_emptiable
                    && is_block
                {
                    return false;
                }
            }
            FieldType::TermRef => {
                let tax_missing =
                    resolve_term_ref_taxonomy(spec, specs, values, root_specs, root_values)
                        .is_none()
                        && spec.taxonomy_from_field.is_some();
                if tax_missing {
                    if spec.multiple {
                        if let Some(arr) = values
                            .get_mut(spec.key.as_str())
                            .and_then(Value::as_array_mut)
                        {
                            arr.clear();
                        }
                    } else {
                        values.remove(spec.key.as_str());
                    }
                    if un_emptiable && is_block {
                        return false;
                    }
                } else if clean_id_field(values, spec, &ctx.missing_terms, un_emptiable)
                    && un_emptiable
                    && is_block
                {
                    return false;
                }
            }
            FieldType::Select => {
                if is_block
                    && let Some(src) = spec.options_source
                    && let Some(src_key) = options_source_key(src)
                {
                    let bad_vals = ctx.missing_dynamic.get(&(field_path.as_str(), src_key));
                    let is_patterns = src == SelectOptionsSource::Patterns;
                    let is_forms = src == SelectOptionsSource::ContactForms;
                    let invalid = |raw: &str| -> bool {
                        let t = raw.trim();
                        if t.is_empty() {
                            return false;
                        }
                        if is_patterns {
                            return t
                                .parse::<i64>()
                                .map_or(true, |id| ctx.missing_patterns.contains(&id));
                        }
                        if is_forms {
                            return t
                                .parse::<i64>()
                                .map_or(true, |id| ctx.missing_forms.contains(&id));
                        }
                        bad_vals.is_some_and(|set| set.contains(t))
                    };
                    if let Some(val) = values.get_mut(spec.key.as_str()) {
                        match val {
                            Value::String(s) => {
                                if invalid(s) {
                                    *val = Value::String(String::new());
                                    if un_emptiable {
                                        return false;
                                    }
                                }
                            }
                            Value::Array(arr) if spec.multiple => {
                                arr.retain(|v| v.as_str().is_none_or(|s| !invalid(s)));
                                if un_emptiable
                                    && (arr.is_empty()
                                        || spec.min_items.is_some_and(|m| arr.len() < m))
                                {
                                    return false;
                                } else if !un_emptiable
                                    && !arr.is_empty()
                                    && spec.min_items.is_some_and(|m| arr.len() < m)
                                {
                                    arr.clear();
                                }
                            }
                            _ => {}
                        }
                    } else if un_emptiable
                        && let Some(Value::String(def_s)) = spec.default.as_ref()
                        && invalid(def_s)
                    {
                        values.insert(spec.key.clone(), Value::String(String::new()));
                        return false;
                    }
                }
            }
            FieldType::Group => {
                if let Some(Value::Object(obj)) = values.get_mut(spec.key.as_str()) {
                    let mut sub_map: BTreeMap<String, Value> =
                        obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    let ok = clean_field_map_with_root(
                        &spec.fields,
                        root_specs,
                        &mut sub_map,
                        root_values,
                        visible,
                        Some(&field_path),
                        block_node_id,
                        ctx,
                        is_block,
                    );
                    *obj = sub_map.into_iter().collect();
                    if !ok {
                        return false;
                    }
                }
            }
            FieldType::Repeater => {
                if let Some(Value::Array(items)) = values.get_mut(spec.key.as_str()) {
                    let mut kept = Vec::with_capacity(items.len());
                    for (idx, item) in items.iter().enumerate() {
                        if let Some(obj) = item.as_object() {
                            let mut sub_map: BTreeMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            let item_path = format!("{field_path}[{idx}]");
                            let ok = clean_field_map_with_root(
                                &spec.fields,
                                root_specs,
                                &mut sub_map,
                                root_values,
                                visible,
                                Some(&item_path),
                                block_node_id,
                                ctx,
                                is_block,
                            );
                            if ok {
                                kept.push(Value::Object(sub_map.into_iter().collect()));
                            }
                        } else {
                            kept.push(item.clone());
                        }
                    }
                    *items = kept;
                    if is_block
                        && un_emptiable
                        && (items.is_empty() || spec.min_items.is_some_and(|m| items.len() < m))
                    {
                        return false;
                    }
                    if !un_emptiable
                        && !items.is_empty()
                        && spec.min_items.is_some_and(|m| items.len() < m)
                    {
                        items.clear();
                    }
                }
            }
            _ => {}
        }
    }
    true
}

fn clean_id_field(
    values: &mut BTreeMap<String, Value>,
    spec: &FieldSpec,
    bad_ids: &BTreeSet<i64>,
    un_emptiable: bool,
) -> bool {
    let Some(val) = values.get_mut(spec.key.as_str()) else {
        return false;
    };
    match val {
        Value::Number(n) => {
            if n.as_i64().is_some_and(|id| bad_ids.contains(&id)) {
                values.remove(spec.key.as_str());
                return true;
            }
            false
        }
        Value::Array(arr) => {
            arr.retain(|item| item.as_i64().is_none_or(|id| !bad_ids.contains(&id)));
            let collapsed =
                !un_emptiable && !arr.is_empty() && spec.min_items.is_some_and(|m| arr.len() < m);
            if collapsed {
                arr.clear();
            }
            arr.is_empty() || spec.min_items.is_some_and(|m| arr.len() < m)
        }
        _ => false,
    }
}

fn options_source_key(src: SelectOptionsSource) -> Option<&'static str> {
    match src {
        SelectOptionsSource::Menus => Some("menus"),
        SelectOptionsSource::MenuSlots => Some("menu_slots"),
        SelectOptionsSource::ContentTypes => Some("content_types"),
        SelectOptionsSource::Taxonomies => Some("taxonomies"),
        SelectOptionsSource::ContextFields => Some("context_fields"),
        SelectOptionsSource::TemplateParts => Some("template_parts"),
        SelectOptionsSource::ContactForms => Some("contact_forms"),
        SelectOptionsSource::Patterns => None,
    }
}

fn sanitize_builder_document(
    doc: &mut BuilderDocument,
    ctx: &SanitizeCtx<'_>,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) {
    for section in &mut doc.sections {
        if let Some(style) = section.style.as_mut()
            && let Some(ref bg) = style.background_media_id
            && ctx.missing_media_ids.contains(bg.as_str())
        {
            style.background_media_id = None;
        }
        for row in &mut section.children {
            for col in &mut row.children {
                col.children
                    .retain_mut(|el| sanitize_element(el, ctx, schemas));
            }
        }
    }
}

fn valid_fallback_text(value: &str, max_chars: usize, required: bool) -> bool {
    value.chars().count() <= max_chars
        && (!required || !value.trim().is_empty())
        && !value
            .chars()
            .any(|ch| ch == '\0' || (ch.is_control() && ch != '\n' && ch != '\t'))
}

fn sanitize_element(
    el: &mut opendash_core::builder::ElementNode,
    ctx: &SanitizeCtx<'_>,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> bool {
    if ctx.missing_block_nodes.contains(el.id.as_str())
        || ctx.missing_list_nodes.contains(el.id.as_str())
    {
        return false;
    }
    let ElementContent::Known(known) = &mut el.content else {
        return true;
    };
    let was_binding_cleared = ctx.cleared_bindings.contains(el.id.as_str());
    match known {
        KnownElement::Heading(props) => {
            if was_binding_cleared {
                props.binding = None;
                return (1..=6).contains(&props.level)
                    && valid_fallback_text(&props.text, 512, true);
            }
            true
        }
        KnownElement::Text(props) => {
            if was_binding_cleared {
                props.binding = None;
                let text_ok = valid_fallback_text(&props.text, 16_384, false)
                    && (!props.rich
                        || opendash_core::richtext::sanitize_richtext(&props.text).is_ok());
                return text_ok;
            }
            true
        }
        KnownElement::Image(props) => {
            if was_binding_cleared {
                props.binding = None;
                return !props.media_id.trim().is_empty()
                    && valid_media_id(&props.media_id)
                    && !ctx.missing_media_ids.contains(props.media_id.as_str());
            }
            let bound_media = props
                .binding
                .as_ref()
                .is_some_and(|b| b.target == BindingTarget::MediaId);
            if !bound_media && ctx.missing_media_ids.contains(props.media_id.as_str()) {
                return false;
            }
            true
        }
        KnownElement::Video(props) => {
            if ctx.missing_media_ids.contains(props.media_id.as_str()) {
                return false;
            }
            if let Some(ref pid) = props.poster_image_id
                && ctx.missing_media_ids.contains(pid.as_str())
            {
                props.poster_image_id = None;
            }
            true
        }
        KnownElement::Button(props) => {
            if was_binding_cleared {
                props.binding = None;
                return valid_fallback_text(&props.label, 200, true)
                    && valid_button_url(&props.url);
            }
            true
        }
        KnownElement::PatternRef(props) => !ctx.missing_patterns.contains(&props.pattern_id),
        KnownElement::ContactForm(props) => !ctx.missing_forms.contains(&props.form_id),
        KnownElement::PluginBlock(props) => {
            let Some(entry) = schemas.get(&props.block_id) else {
                return true;
            };
            if entry.status != ProviderStatus::Active
                || entry.declaration_state != DeclarationState::Declared
            {
                return true;
            }
            clean_field_map(
                &entry.descriptor.schema.fields,
                Some(el.id.as_str()),
                &mut props.values,
                ctx,
                true,
            )
        }
        _ => true,
    }
}
