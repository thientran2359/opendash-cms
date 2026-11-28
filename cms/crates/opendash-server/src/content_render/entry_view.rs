use super::*;

pub(super) fn plain_text_from_field(
    definition: &opendash_core::field_schema::FieldSpec,
    value: &Value,
) -> Option<String> {
    let raw = match (definition.field_type, value) {
        (
            opendash_core::field_schema::FieldType::Text
            | opendash_core::field_schema::FieldType::Textarea
            | opendash_core::field_schema::FieldType::Richtext,
            Value::String(s),
        ) => s.as_str(),
        _ => return None,
    };
    let clean = match definition.field_type {
        opendash_core::field_schema::FieldType::Richtext => {
            let text = if opendash_core::builder_blocks::contains_blocks(raw) {
                opendash_core::builder_blocks::extract_visible_text(raw)
            } else {
                strip_richtext_to_plain(raw)
            };
            text.split_whitespace().collect::<Vec<_>>().join(" ")
        }
        opendash_core::field_schema::FieldType::Text
        | opendash_core::field_schema::FieldType::Textarea => {
            raw.split_whitespace().collect::<Vec<_>>().join(" ")
        }
        _ => String::new(),
    };
    (!clean.is_empty()).then_some(clean)
}

pub(super) fn truncate_excerpt(text: &str) -> String {
    if text.chars().count() <= 200 {
        return text.to_owned();
    }
    let mut byte_at_197 = text.len();
    let mut last_ws_char_idx = None;
    let mut last_ws_byte_idx = 0;
    for (char_idx, (byte_idx, ch)) in text.char_indices().enumerate() {
        if char_idx == 197 {
            byte_at_197 = byte_idx;
            break;
        }
        if ch.is_whitespace() {
            last_ws_char_idx = Some(char_idx);
            last_ws_byte_idx = byte_idx;
        }
    }
    let cut_byte = match last_ws_char_idx {
        Some(idx) if idx >= 98 => last_ws_byte_idx,
        _ => byte_at_197,
    };
    let prefix = text[..cut_byte].trim_end();
    format!("{prefix}...")
}

fn text_from_hydrated_value(
    field_type: opendash_core::field_schema::FieldType,
    hydrated: &Value,
    l10n: &CoreL10n,
) -> String {
    use opendash_core::field_schema::FieldType;
    match field_type {
        FieldType::Media
        | FieldType::Gallery
        | FieldType::Link
        | FieldType::Icon
        | FieldType::Spacing
        | FieldType::Group
        | FieldType::Repeater
        | FieldType::Secret
        | FieldType::Tab
        | FieldType::Accordion
        | FieldType::Message => String::new(),
        FieldType::Boolean if hydrated.is_boolean() => if hydrated == &Value::Bool(true) {
            l10n.yes.as_ref()
        } else {
            l10n.no.as_ref()
        }
        .to_owned(),
        FieldType::EntryRef => {
            let titles: Vec<&str> = match hydrated {
                Value::Object(obj) => obj
                    .get("title")
                    .and_then(Value::as_str)
                    .into_iter()
                    .collect(),
                Value::Array(arr) => arr
                    .iter()
                    .filter_map(|v| {
                        v.as_object()
                            .and_then(|o| o.get("title"))
                            .and_then(Value::as_str)
                    })
                    .collect(),
                _ => Vec::new(),
            };
            titles.join(", ")
        }
        FieldType::TermRef => {
            let names: Vec<&str> = match hydrated {
                Value::Object(obj) => obj
                    .get("name")
                    .and_then(Value::as_str)
                    .into_iter()
                    .collect(),
                Value::Array(arr) => arr
                    .iter()
                    .filter_map(|v| {
                        v.as_object()
                            .and_then(|o| o.get("name"))
                            .and_then(Value::as_str)
                    })
                    .collect(),
                _ => Vec::new(),
            };
            names.join(", ")
        }
        _ => match hydrated {
            Value::Null => String::new(),
            Value::String(text) => text.clone(),
            Value::Array(arr) => arr
                .iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Null => None,
                    other => Some(other.to_string()),
                })
                .collect::<Vec<_>>()
                .join(", "),
            other => other.to_string(),
        },
    }
}

fn pure_media_fallback(val: &Value) -> Option<Value> {
    let url = media_url(val)?;
    let id = val.as_str()?;
    Some(serde_json::json!({
        "id": id,
        "kind": "image",
        "url": url,
        "thumbnail_url": format!("/media/{id}/thumbnail"),
        "width": null,
        "height": null,
        "alt": "",
        "version": 1,
        "variants": [],
        "webp_srcset": null,
        "fallback_srcset": null,
        crate::template_sec::MEDIA_PROVENANCE_KEY: crate::template_sec::media_provenance_token(),
    }))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_entry_view_with_defs(
    record: &ContentEntryRecord,
    type_key: &str,
    ordered_defs: &[&opendash_core::field_schema::FieldSpec],
    url: Option<String>,
    terms: &[PublicTermView],
    eligible_public_taxonomies: &[String],
    max_view_role: FieldRole,
    hydration_ctx: Option<&crate::field_hydrate::HydrationContext<'_>>,
    mut richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> EntryView {
    use opendash_core::field_schema::{FieldType, MediaFieldKind};

    let l10n = core_l10n(&record.locale);
    let mut featured_cover_url: Option<String> = None;
    let mut featured_cover_media: Option<Value> = None;
    let mut first_image_cover_url: Option<String> = None;
    let mut first_image_cover_media: Option<Value> = None;
    let mut public_values = BTreeMap::new();
    let mut richtext_html = BTreeMap::new();
    let mut fields = Vec::with_capacity(ordered_defs.len());
    let mut excerpt_plain: Option<String> = None;
    let mut summary_plain: Option<String> = None;
    let mut body_plain: Option<String> = None;
    let mut fallback_plain_parts: Vec<String> = Vec::new();
    let mut all_plain_parts: Vec<String> = Vec::new();

    for &definition in ordered_defs {
        if definition.is_ui_only() {
            continue;
        }
        let raw_opt = record.values.get(&definition.key);
        let value = raw_opt.unwrap_or(&Value::Null);
        let is_builder_body = definition.key == "body"
            && definition.field_type == FieldType::Richtext
            && value
                .as_str()
                .is_some_and(opendash_core::builder_blocks::contains_blocks);

        let vis = crate::field_hydrate::HydrationVisibility::ContentPublic { max_view_role };
        let hydrated = if is_builder_body {
            Value::String(opendash_core::builder_blocks::extract_visible_text(
                value.as_str().unwrap_or(""),
            ))
        } else if let Some(ctx) = hydration_ctx {
            crate::field_hydrate::hydrate_field_value_with_richtext(
                definition,
                raw_opt,
                ctx,
                richtext_nonces.as_deref_mut(),
                vis,
                Some(&mut richtext_html),
                None,
            )
            .unwrap_or(Value::Null)
        } else {
            // Pure unit/fallback mode without a DB hydration snapshot.
            match definition.field_type {
                FieldType::Media => pure_media_fallback(value).unwrap_or(Value::Null),
                FieldType::Gallery => Value::Array(
                    value
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(pure_media_fallback)
                        .collect(),
                ),
                FieldType::Richtext => {
                    if let Some(raw_str) = value.as_str().filter(|s| !s.trim().is_empty())
                        && let Ok(clean) = opendash_core::richtext::sanitize_richtext(raw_str)
                        && !clean.is_empty()
                    {
                        richtext_html.insert(definition.key.clone(), clean);
                    }
                    value.clone()
                }
                FieldType::Link | FieldType::Group | FieldType::Repeater if !value.is_null() => {
                    crate::field_hydrate::hydrate_field_value_stateless(definition, raw_opt, vis)
                        .unwrap_or(Value::Null)
                }
                _ => value.clone(),
            }
        };

        public_values.insert(definition.key.clone(), hydrated.clone());
        if let Some(plain) = plain_text_from_field(definition, value) {
            all_plain_parts.push(plain.clone());
            if definition.key == "excerpt" && excerpt_plain.is_none() {
                excerpt_plain = Some(plain);
            } else if definition.key == "summary" && summary_plain.is_none() {
                summary_plain = Some(plain);
            } else if definition.key == "body" && body_plain.is_none() {
                body_plain = Some(plain);
            } else {
                fallback_plain_parts.push(plain);
            }
        }

        let media = if definition.field_type == FieldType::Media && !hydrated.is_null() {
            if definition.effective_media_kind() == MediaFieldKind::Image
                && let Some(u) = hydrated
                    .as_object()
                    .and_then(|o| o.get("url"))
                    .and_then(Value::as_str)
            {
                if definition.key == "featured_image" {
                    featured_cover_url = Some(u.to_owned());
                    featured_cover_media = Some(hydrated.clone());
                } else if first_image_cover_url.is_none() {
                    first_image_cover_url = Some(u.to_owned());
                    first_image_cover_media = Some(hydrated.clone());
                }
            }
            Some(hydrated.clone())
        } else {
            None
        };
        let gallery = if definition.field_type == FieldType::Gallery {
            hydrated.as_array().cloned().unwrap_or_default()
        } else {
            Vec::new()
        };
        let link = if definition.field_type == FieldType::Link && !hydrated.is_null() {
            Some(hydrated.clone())
        } else {
            None
        };
        let text = text_from_hydrated_value(definition.field_type, &hydrated, &l10n);

        fields.push(FieldView {
            key: definition.key.clone(),
            label: definition.label.clone(),
            field_type: definition.field_type.as_str(),
            text,
            media,
            gallery,
            link,
        });
    }
    let cover_url = featured_cover_url.or(first_image_cover_url);
    let cover_media = featured_cover_media.or(first_image_cover_media);

    let excerpt_source = if let Some(ref explicit) = excerpt_plain {
        explicit.as_str().to_owned()
    } else if let Some(ref summary) = summary_plain {
        summary.as_str().to_owned()
    } else if let Some(ref body) = body_plain {
        body.as_str().to_owned()
    } else {
        fallback_plain_parts.join(" ")
    };
    let excerpt = if excerpt_source.is_empty() {
        String::new()
    } else {
        truncate_excerpt(&excerpt_source)
    };

    let reading_source = if let Some(ref body) = body_plain {
        body.as_str().to_owned()
    } else {
        all_plain_parts.join(" ")
    };
    let words = reading_source.split_whitespace().count();
    let reading_time_minutes = if words == 0 {
        0
    } else {
        ((words + 199) / 200).max(1) as u32
    };

    let mut terms_map: BTreeMap<String, Vec<PublicTermView>> = BTreeMap::new();
    terms_map.insert("category".to_owned(), Vec::new());
    terms_map.insert("tag".to_owned(), Vec::new());
    for tax in eligible_public_taxonomies {
        terms_map.entry(tax.clone()).or_default();
    }
    let eligible_set: BTreeSet<&str> = eligible_public_taxonomies
        .iter()
        .map(String::as_str)
        .collect();
    let mut all_terms = Vec::with_capacity(terms.len());
    for term in terms {
        if !eligible_set.is_empty() && !eligible_set.contains(term.kind.as_str()) {
            continue;
        }
        terms_map
            .entry(term.kind.clone())
            .or_default()
            .push(term.clone());
        all_terms.push(term.clone());
    }

    let category_terms = terms_map.get("category").map(Vec::as_slice).unwrap_or(&[]);
    let primary_category = record
        .primary_category_term_id
        .and_then(|pid| category_terms.iter().find(|t| t.id == pid).cloned())
        .or_else(|| category_terms.iter().min_by_key(|t| t.id).cloned());

    EntryView {
        id: record.id,
        type_key: type_key.to_owned(),
        title: record.title.clone(),
        slug: record.slug.clone(),
        url,
        parent: None,
        menu_order: record.menu_order,
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
        published_at: record.published_at.clone(),
        excerpt,
        reading_time_minutes,
        fields: public_values,
        field_list: fields,
        cover_url,
        cover: cover_media.as_ref().and_then(HydratedMedia::from_value),
        cover_media,
        richtext_html,
        terms: terms_map,
        all_terms,
        primary_category,
        ext: BTreeMap::new(),
        theme_ext: BTreeMap::new(),
        snippet_segments: Vec::new(),
        type_label: String::new(),
    }
}

pub fn entry_view(record: ContentEntryRecord, schema: &ContentTypeDefinition) -> EntryView {
    entry_view_with_url(record, schema, None)
}

pub fn ordered_public_field_defs<'a>(
    effective_schema: &'a EffectiveContentSchema,
    max_view_role: FieldRole,
) -> Vec<&'a opendash_core::field_schema::FieldSpec> {
    let mut seen = BTreeSet::new();
    let mut ordered_keys: Vec<&str> = Vec::with_capacity(effective_schema.effective_fields.len());
    for group in &effective_schema.attached_groups {
        for field in &group.fields {
            if seen.insert(field.key.as_str()) {
                ordered_keys.push(field.key.as_str());
            }
        }
    }
    for key in effective_schema.effective_fields.keys() {
        if seen.insert(key.as_str()) {
            ordered_keys.push(key.as_str());
        }
    }

    ordered_keys
        .into_iter()
        .filter_map(|key| effective_schema.effective_fields.get(key))
        .filter(|fe| {
            !fe.definition.is_ui_only()
                && fe.effective_status == EffectiveFieldStatus::Active
                && fe.definition.effective_acl().2 == PublicExposure::Public
                && fe.definition.effective_acl().0 <= max_view_role
        })
        .map(|fe| &fe.definition)
        .collect()
}

pub fn ordered_public_field_defs_for_entry<'a>(
    effective_schema: &'a EffectiveContentSchema,
    max_view_role: FieldRole,
    eval_ctx: &opendash_core::content::LocationEvalContext,
) -> Vec<&'a opendash_core::field_schema::FieldSpec> {
    let active_groups: BTreeSet<&str> = effective_schema
        .attached_groups
        .iter()
        .filter(|grp| {
            grp.is_runtime_active()
                && !opendash_core::content::is_restricted_to_admin_only(&grp.location)
                && opendash_core::content::evaluate_location_rules(&grp.location, eval_ctx)
        })
        .map(|grp| grp.group_id.as_str())
        .collect();
    let mut seen = BTreeSet::new();
    let mut ordered_keys: Vec<&str> = Vec::with_capacity(effective_schema.effective_fields.len());
    for group in &effective_schema.attached_groups {
        if !active_groups.contains(group.group_id.as_str()) {
            continue;
        }
        for field in &group.fields {
            if seen.insert(field.key.as_str()) {
                ordered_keys.push(field.key.as_str());
            }
        }
    }
    for (key, fe) in &effective_schema.effective_fields {
        if fe.source_id.starts_with("base:") && seen.insert(key.as_str()) {
            ordered_keys.push(key.as_str());
        }
    }

    ordered_keys
        .into_iter()
        .filter_map(|key| effective_schema.effective_fields.get(key))
        .filter(|fe| {
            (fe.source_id.starts_with("base:") || active_groups.contains(fe.source_id.as_str()))
                && !fe.definition.is_ui_only()
                && fe.effective_status == EffectiveFieldStatus::Active
                && fe.definition.effective_acl().2 == PublicExposure::Public
                && fe.definition.effective_acl().0 <= max_view_role
        })
        .map(|fe| &fe.definition)
        .collect()
}

pub fn entry_view_with_url(
    record: ContentEntryRecord,
    schema: &ContentTypeDefinition,
    url: Option<String>,
) -> EntryView {
    let core_specs = core_specs_for_settings(&schema.key, &schema.settings);
    let ordered_defs: Vec<&opendash_core::field_schema::FieldSpec> = core_specs.iter().collect();
    build_entry_view_with_defs(
        &record,
        &schema.key,
        &ordered_defs,
        url,
        &[],
        &[],
        FieldRole::Editor,
        None,
        None,
    )
}

fn core_specs_for_settings(
    type_key: &str,
    settings: &opendash_core::content::ContentTypeSettings,
) -> Vec<opendash_core::field_schema::FieldSpec> {
    use opendash_core::content::ContentSupport;
    use opendash_core::field_schema::{FieldSpec, FieldType, MediaFieldKind};
    if matches!(type_key, "pattern" | "section") {
        return Vec::new();
    }
    let mut out = Vec::new();
    if settings.supports(ContentSupport::Editor) || settings.supports(ContentSupport::Builder) {
        out.push(FieldSpec::new(FieldType::Richtext, "body", "Nội dung"));
    }
    if settings.supports(ContentSupport::FeaturedImage) {
        let mut fi = FieldSpec::new(FieldType::Media, "featured_image", "Ảnh đại diện");
        fi.media_kind = Some(MediaFieldKind::Image);
        out.push(fi);
    }
    if settings.supports(ContentSupport::Excerpt) {
        out.push(FieldSpec::new(FieldType::Textarea, "excerpt", "Tóm tắt"));
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn build_hydrated_entry_view_with_context(
    record: &ContentEntryRecord,
    effective_schema: &EffectiveContentSchema,
    url: Option<String>,
    terms: &[PublicTermView],
    eligible_public_taxonomies: &[String],
    max_view_role: FieldRole,
    hydration_ctx: Option<&crate::field_hydrate::HydrationContext<'_>>,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> EntryView {
    let ordered_defs = ordered_public_field_defs(effective_schema, max_view_role);
    build_entry_view_with_defs(
        record,
        &effective_schema.base.descriptor.key,
        &ordered_defs,
        url,
        terms,
        eligible_public_taxonomies,
        max_view_role,
        hydration_ctx,
        richtext_nonces,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_hydrated_entry_view_with_eval_context(
    record: &ContentEntryRecord,
    effective_schema: &EffectiveContentSchema,
    url: Option<String>,
    terms: &[PublicTermView],
    eligible_public_taxonomies: &[String],
    max_view_role: FieldRole,
    eval_ctx: &opendash_core::content::LocationEvalContext,
    hydration_ctx: Option<&crate::field_hydrate::HydrationContext<'_>>,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> EntryView {
    let ordered_defs =
        ordered_public_field_defs_for_entry(effective_schema, max_view_role, eval_ctx);
    build_entry_view_with_defs(
        record,
        &effective_schema.base.descriptor.key,
        &ordered_defs,
        url,
        terms,
        eligible_public_taxonomies,
        max_view_role,
        hydration_ctx,
        richtext_nonces,
    )
}

pub fn entry_view_with_context(
    record: &ContentEntryRecord,
    effective_schema: &EffectiveContentSchema,
    url: Option<String>,
    terms: &[PublicTermView],
    eligible_public_taxonomies: &[String],
    max_view_role: FieldRole,
) -> EntryView {
    build_hydrated_entry_view_with_context(
        record,
        effective_schema,
        url,
        terms,
        eligible_public_taxonomies,
        max_view_role,
        None,
        None,
    )
}

pub fn authenticated_preview_view_with_context(
    record: &ContentEntryRecord,
    effective_schema: &EffectiveContentSchema,
    url: Option<String>,
    terms: &[PublicTermView],
    eligible_public_taxonomies: &[String],
    actor_role: FieldRole,
) -> EntryView {
    let view = entry_view_with_context(
        record,
        effective_schema,
        url,
        terms,
        eligible_public_taxonomies,
        actor_role,
    );
    rewrite_preview_media_urls(view, &effective_schema.base.descriptor.key)
}
