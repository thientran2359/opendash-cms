use super::*;

pub fn strip_richtext_to_plain(html: &str) -> String {
    fn is_block_tag(name: &str) -> bool {
        matches!(
            name,
            "p" | "div"
                | "br"
                | "li"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "tr"
                | "td"
                | "th"
                | "blockquote"
                | "section"
                | "article"
                | "header"
                | "footer"
                | "pre"
                | "hr"
                | "ul"
                | "ol"
        )
    }

    let mut stripped = String::with_capacity(html.len());
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let next = bytes.get(i + 1).copied();
            let looks_like_tag =
                next.is_some_and(|b| b.is_ascii_alphabetic() || matches!(b, b'/' | b'!' | b'?'));
            if looks_like_tag {
                let mut quote: Option<u8> = None;
                let mut j = i + 1;
                let mut closed = false;
                while j < bytes.len() {
                    let b = bytes[j];
                    if let Some(q) = quote {
                        if b == q {
                            quote = None;
                        }
                    } else if b == b'"' || b == b'\'' {
                        quote = Some(b);
                    } else if b == b'>' {
                        closed = true;
                        break;
                    }
                    j += 1;
                }
                if closed {
                    let inner = html[i + 1..j].trim_start();
                    let inner = inner.strip_prefix('/').unwrap_or(inner);
                    let tag_name = inner
                        .split(|c: char| c.is_ascii_whitespace() || c == '/')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if is_block_tag(&tag_name) && !stripped.ends_with(' ') {
                        stripped.push(' ');
                    }
                    i = j + 1;
                    continue;
                }
            }
        }
        let ch = html[i..].chars().next().unwrap_or('\0');
        stripped.push(ch);
        i += ch.len_utf8();
    }

    // Single-pass basic HTML entity decoding so "&amp;lt;" stays "&lt;".
    let mut decoded = String::with_capacity(stripped.len());
    let mut rest = stripped.as_str();
    while let Some(pos) = rest.find('&') {
        decoded.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let mut matched = None;
        for (entity, replacement) in [
            ("&nbsp;", " "),
            ("&#160;", " "),
            ("&amp;", "&"),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", "\""),
            ("&#39;", "'"),
            ("&#x27;", "'"),
            ("&apos;", "'"),
        ] {
            if tail.starts_with(entity) {
                decoded.push_str(replacement);
                rest = &tail[entity.len()..];
                matched = Some(());
                break;
            }
        }
        if matched.is_none() {
            decoded.push('&');
            rest = &tail[1..];
        }
    }
    decoded.push_str(rest);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

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

pub(super) fn build_entry_view_with_defs(
    record: &ContentEntryRecord,
    type_key: &str,
    ordered_defs: &[&opendash_core::field_schema::FieldSpec],
    url: Option<String>,
    terms: &[PublicTermView],
    eligible_public_taxonomies: &[String],
    media_map: &BTreeMap<String, crate::storage::fields::HydratedMediaRow>,
) -> EntryView {
    let l10n = core_l10n(&record.locale);
    let mut cover_url = None;
    let mut cover_media = None;
    let mut public_values = BTreeMap::new();
    let mut fields = Vec::with_capacity(ordered_defs.len());

    let mut summary_plain: Option<String> = None;
    let mut body_plain: Option<String> = None;
    let mut fallback_plain_parts: Vec<String> = Vec::new();
    let mut all_plain_parts: Vec<String> = Vec::new();

    for &definition in ordered_defs {
        let value = record.values.get(&definition.key).unwrap_or(&Value::Null);
        let extracted_rich =
            if definition.field_type == opendash_core::field_schema::FieldType::Richtext {
                value.as_str().map(|s| {
                    if opendash_core::builder_blocks::contains_blocks(s) {
                        opendash_core::builder_blocks::extract_visible_text(s)
                    } else {
                        s.to_owned()
                    }
                })
            } else {
                None
            };
        if let Some(ref clean) = extracted_rich {
            public_values.insert(definition.key.clone(), Value::String(clean.clone()));
        } else {
            public_values.insert(definition.key.clone(), value.clone());
        }
        if let Some(plain) = plain_text_from_field(definition, value) {
            all_plain_parts.push(plain.clone());
            if definition.key == "summary" && summary_plain.is_none() {
                summary_plain = Some(plain);
            } else if definition.key == "body" && body_plain.is_none() {
                body_plain = Some(plain);
            } else {
                fallback_plain_parts.push(plain);
            }
        }

        let kind = match definition.field_type {
            opendash_core::field_schema::FieldType::Text => "text",
            opendash_core::field_schema::FieldType::Textarea => "textarea",
            opendash_core::field_schema::FieldType::Richtext => "richtext",
            opendash_core::field_schema::FieldType::Number => "number",
            opendash_core::field_schema::FieldType::Boolean => "boolean",
            opendash_core::field_schema::FieldType::Date => "date",
            opendash_core::field_schema::FieldType::Select => "select",
            opendash_core::field_schema::FieldType::Media => "media",
            opendash_core::field_schema::FieldType::Gallery => "gallery",
            _ => "text",
        };
        let (image_url, image_media) =
            if definition.field_type == opendash_core::field_schema::FieldType::Media {
                if let Some(row) = value
                    .as_str()
                    .filter(|id| valid_media_id(id))
                    .and_then(|id| media_map.get(id))
                {
                    (
                        Some(format!("/media/{}/file?v={}", row.id, row.version.max(1))),
                        Some(crate::field_hydrate::hydrate_media_row_for_template(
                            row, false,
                        )),
                    )
                } else {
                    (media_url(value), None)
                }
            } else {
                (None, None)
            };
        if cover_url.is_none() {
            cover_url = image_url.clone();
            cover_media = image_media.clone();
        }
        let (gallery_urls, gallery_media) =
            if definition.field_type == opendash_core::field_schema::FieldType::Gallery {
                let mut urls = Vec::new();
                let mut hydrated = Vec::new();
                for item in value.as_array().into_iter().flatten() {
                    if let Some(row) = item
                        .as_str()
                        .filter(|id| valid_media_id(id))
                        .and_then(|id| media_map.get(id))
                    {
                        urls.push(format!("/media/{}/file?v={}", row.id, row.version.max(1)));
                        hydrated.push(crate::field_hydrate::hydrate_media_row_for_template(
                            row, false,
                        ));
                    } else if let Some(u) = media_url(item) {
                        urls.push(u);
                    }
                }
                (urls, hydrated)
            } else {
                (Vec::new(), Vec::new())
            };
        let text = match definition.field_type {
            opendash_core::field_schema::FieldType::Media
            | opendash_core::field_schema::FieldType::Gallery => String::new(),
            opendash_core::field_schema::FieldType::Boolean if value.is_boolean() => {
                if value == &Value::Bool(true) {
                    l10n.yes.as_ref()
                } else {
                    l10n.no.as_ref()
                }
                .to_string()
            }
            _ => match (extracted_rich, value) {
                (Some(clean), _) => clean,
                (None, Value::Null) => String::new(),
                (None, Value::String(text)) => text.clone(),
                (None, other) => other.to_string(),
            },
        };
        fields.push(FieldView {
            key: definition.key.clone(),
            label: definition.label.clone(),
            kind,
            text,
            image_url,
            gallery_urls,
            image_media,
            gallery_media,
        });
    }

    let excerpt_source = if let Some(ref summary) = summary_plain {
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
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
        published_at: record.published_at.clone(),
        excerpt,
        reading_time_minutes,
        fields: public_values,
        field_list: fields,
        cover_url,
        cover_media,
        terms: terms_map,
        all_terms,
        primary_category,
        ext: BTreeMap::new(),
        theme_ext: BTreeMap::new(),
    }
}

pub(super) fn rewrite_hydrated_media_preview(val: &mut Value) {
    let Some(obj) = val.as_object_mut() else {
        return;
    };
    let rewrite_str = |s: &str| {
        if s.starts_with("/media/") || s.contains(", /media/") {
            s.replace("/media/", "/api/admin/media/")
        } else {
            s.to_owned()
        }
    };
    for key in ["url", "thumbnail_url", "webp_srcset", "fallback_srcset"] {
        if let Some(Value::String(s)) = obj.get_mut(key) {
            *s = rewrite_str(s);
        }
    }
    if let Some(Value::Array(variants)) = obj.get_mut("variants") {
        for item in variants {
            if let Some(Value::String(u)) = item.as_object_mut().and_then(|m| m.get_mut("url")) {
                *u = rewrite_str(u);
            }
        }
    }
}

pub fn collect_entry_media_ids(
    records: &[&ContentEntryRecord],
    effective_schema: &EffectiveContentSchema,
) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for record in records {
        for fe in effective_schema.effective_fields.values() {
            if fe.effective_status != EffectiveFieldStatus::Active
                || fe.definition.effective_acl().2 != PublicExposure::Public
            {
                continue;
            }
            let Some(val) = record.values.get(&fe.definition.key) else {
                continue;
            };
            match fe.definition.field_type {
                opendash_core::field_schema::FieldType::Media => {
                    if let Some(id) = val.as_str().filter(|s| valid_media_id(s)) {
                        ids.insert(id.to_owned());
                    }
                }
                opendash_core::field_schema::FieldType::Gallery => {
                    for item in val.as_array().into_iter().flatten() {
                        if let Some(id) = item.as_str().filter(|s| valid_media_id(s)) {
                            ids.insert(id.to_owned());
                        }
                    }
                }
                _ => {}
            }
        }
    }
    ids
}

pub fn collect_multi_type_entry_media_ids(
    records: &[ContentEntryRecord],
    schemas: &BTreeMap<String, EffectiveContentSchema>,
) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for record in records {
        if let Some(schema) = schemas.get(&record.content_type_key) {
            ids.extend(collect_entry_media_ids(&[record], schema));
        }
    }
    ids
}

pub fn entry_view(record: ContentEntryRecord, schema: &ContentTypeDefinition) -> EntryView {
    entry_view_with_url(record, schema, None)
}

pub fn entry_view_with_url(
    record: ContentEntryRecord,
    schema: &ContentTypeDefinition,
    url: Option<String>,
) -> EntryView {
    let ordered_defs: Vec<&opendash_core::field_schema::FieldSpec> = schema
        .fields
        .iter()
        .filter(|d| d.effective_acl().2 == PublicExposure::Public)
        .collect();
    let empty_media = BTreeMap::new();
    build_entry_view_with_defs(
        &record,
        &schema.key,
        &ordered_defs,
        url,
        &[],
        &[],
        &empty_media,
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
    let empty_media = BTreeMap::new();
    entry_view_with_context_and_media(
        record,
        effective_schema,
        url,
        terms,
        eligible_public_taxonomies,
        max_view_role,
        &empty_media,
    )
}

pub fn entry_view_with_context_and_media(
    record: &ContentEntryRecord,
    effective_schema: &EffectiveContentSchema,
    url: Option<String>,
    terms: &[PublicTermView],
    eligible_public_taxonomies: &[String],
    max_view_role: FieldRole,
    media_map: &BTreeMap<String, crate::storage::fields::HydratedMediaRow>,
) -> EntryView {
    let mut seen = BTreeSet::new();
    let mut ordered_keys: Vec<&str> = Vec::with_capacity(effective_schema.effective_fields.len());
    for field in &effective_schema.base.descriptor.fields {
        if seen.insert(field.key.as_str()) {
            ordered_keys.push(field.key.as_str());
        }
    }
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

    let ordered_defs: Vec<&opendash_core::field_schema::FieldSpec> = ordered_keys
        .into_iter()
        .filter_map(|key| effective_schema.effective_fields.get(key))
        .filter(|fe| {
            fe.effective_status == EffectiveFieldStatus::Active
                && fe.definition.effective_acl().2 == PublicExposure::Public
                && fe.definition.effective_acl().0 <= max_view_role
        })
        .map(|fe| &fe.definition)
        .collect();

    build_entry_view_with_defs(
        record,
        &effective_schema.base.descriptor.key,
        &ordered_defs,
        url,
        terms,
        eligible_public_taxonomies,
        media_map,
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
    let empty_media = BTreeMap::new();
    authenticated_preview_view_with_context_and_media(
        record,
        effective_schema,
        url,
        terms,
        eligible_public_taxonomies,
        actor_role,
        &empty_media,
    )
}

pub fn authenticated_preview_view_with_context_and_media(
    record: &ContentEntryRecord,
    effective_schema: &EffectiveContentSchema,
    url: Option<String>,
    terms: &[PublicTermView],
    eligible_public_taxonomies: &[String],
    actor_role: FieldRole,
    media_map: &BTreeMap<String, crate::storage::fields::HydratedMediaRow>,
) -> EntryView {
    let view = entry_view_with_context_and_media(
        record,
        effective_schema,
        url,
        terms,
        eligible_public_taxonomies,
        actor_role,
        media_map,
    );
    rewrite_preview_media_urls(view, &effective_schema.base.descriptor.key)
}
