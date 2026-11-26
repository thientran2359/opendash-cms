use super::*;

pub(in crate::storage) fn collect_menu_slot_labels(
    menus_val: Option<&Value>,
) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    let Some(val) = menus_val else {
        return labels;
    };
    let collect_arr = |arr: &[Value], out: &mut BTreeMap<String, String>| {
        for item in arr {
            if let Some(obj) = item.as_object()
                && let Some(key) = obj
                    .get("key")
                    .or_else(|| obj.get("slot_key"))
                    .and_then(Value::as_str)
                && let Some(label) = obj
                    .get("label")
                    .or_else(|| obj.get("title"))
                    .and_then(Value::as_str)
                && !label.trim().is_empty()
            {
                out.insert(key.to_owned(), label.to_owned());
            }
        }
    };
    if let Some(arr) = val.as_array() {
        collect_arr(arr, &mut labels);
    } else if let Some(obj) = val.as_object() {
        if let Some(arr) = obj.get("slots").and_then(Value::as_array) {
            collect_arr(arr, &mut labels);
        } else {
            for (k, v) in obj {
                if let Some(label) = v.as_str().filter(|s| !s.trim().is_empty()).or_else(|| {
                    v.as_object()
                        .and_then(|o| o.get("label").or_else(|| o.get("title")))
                        .and_then(Value::as_str)
                        .filter(|s| !s.trim().is_empty())
                }) {
                    labels.insert(k.clone(), label.to_owned());
                }
            }
        }
    }
    labels
}

pub(in crate::storage) fn truncate_entry_excerpt(plain: &str) -> String {
    let normalized: String = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let head: String = chars.by_ref().take(160).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

pub async fn verify_collected_media_ids_in_conn(
    conn: &mut SqliteConnection,
    collected: &CollectedMediaIds,
) -> ApiResult<()> {
    let mut any_and_file = collected.file_ids.clone();
    any_and_file.extend(collected.any_ids.iter().cloned());
    if count_active_media_with_kind_in_conn(conn, &collected.image_ids, Some("image")).await?
        != collected.image_ids.len()
        || count_active_media_with_kind_in_conn(conn, &collected.video_ids, Some("video")).await?
            != collected.video_ids.len()
        || count_active_media_with_kind_in_conn(conn, &any_and_file, None).await?
            != any_and_file.len()
    {
        return Err(ApiError::bad_request("Media không tồn tại."));
    }
    Ok(())
}

pub(in crate::storage) fn extract_ids_from_value(val: &Value) -> BTreeSet<i64> {
    let mut out = BTreeSet::new();
    match val {
        Value::Number(n) => {
            if let Some(id) = n.as_i64()
                && id > 0
            {
                out.insert(id);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                if let Some(id) = item.as_i64()
                    && id > 0
                {
                    out.insert(id);
                }
            }
        }
        _ => {}
    }
    out
}

pub(in crate::storage) fn collect_all_options_sources_with_paths(
    specs: &[FieldSpec],
    values: &BTreeMap<String, Value>,
    prefix: Option<&str>,
    out: &mut Vec<(String, SelectOptionsSource, Vec<String>)>,
) {
    for spec in specs {
        if spec.is_ui_only() || spec.field_type == FieldType::Secret {
            continue;
        }
        let path = match prefix {
            Some(p) => format!("{p}.{}", spec.key),
            None => spec.key.clone(),
        };
        match spec.field_type {
            FieldType::Select => {
                if let Some(src) = spec.options_source {
                    let mut selected = Vec::new();
                    match values.get(spec.key.as_str()).or(spec.default.as_ref()) {
                        Some(Value::String(s)) => {
                            let trimmed = s.trim();
                            if !trimmed.is_empty() {
                                selected.push(trimmed.to_owned());
                            }
                        }
                        Some(Value::Array(arr)) if spec.multiple => {
                            for item in arr {
                                if let Some(s) = item.as_str() {
                                    let trimmed = s.trim();
                                    if !trimmed.is_empty() {
                                        selected.push(trimmed.to_owned());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    if !selected.is_empty() {
                        out.push((path, src, selected));
                    }
                }
            }
            FieldType::Group => {
                let child_map: BTreeMap<String, Value> = values
                    .get(spec.key.as_str())
                    .and_then(Value::as_object)
                    .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();
                collect_all_options_sources_with_paths(&spec.fields, &child_map, Some(&path), out);
            }
            _ => {}
        }
    }
}

pub async fn validate_document_block_refs_in_conn(
    conn: &mut SqliteConnection,
    document: &BuilderDocument,
    previous: Option<&BuilderDocument>,
    catalog: &RegistryCatalog<RegisteredBlockSchema>,
    required_locale: Option<&str>,
    allowed_context_fields: Option<&[String]>,
) -> ApiResult<()> {
    let mut prev_nodes: BTreeMap<String, (&str, &BTreeMap<String, Value>)> = BTreeMap::new();
    if let Some(prev_doc) = previous {
        for section in &prev_doc.sections {
            for row in &section.children {
                for column in &row.children {
                    for element in &column.children {
                        if let ElementContent::Known(KnownElement::PluginBlock(props)) =
                            &element.content
                        {
                            prev_nodes.insert(
                                element.id.clone(),
                                (props.block_id.as_str(), &props.values),
                            );
                        }
                    }
                }
            }
        }
    }

    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    let ElementContent::Known(KnownElement::PluginBlock(props)) = &element.content
                    else {
                        continue;
                    };
                    let Some(entry) = catalog.get(&props.block_id) else {
                        continue;
                    };
                    if entry.status != ProviderStatus::Active
                        || entry.declaration_state != DeclarationState::Declared
                    {
                        continue;
                    }
                    let prev_vals =
                        prev_nodes
                            .get(&element.id)
                            .and_then(|(prev_block_id, vals)| {
                                (*prev_block_id == props.block_id.as_str()).then_some(*vals)
                            });
                    validate_relational_refs_in_conn(
                        conn,
                        &entry.descriptor.schema.fields,
                        &props.values,
                        prev_vals,
                        required_locale,
                    )
                    .await?;
                    validate_block_dynamic_options_in_tx(
                        conn,
                        &entry.descriptor.schema.fields,
                        &props.values,
                        prev_vals,
                        allowed_context_fields,
                    )
                    .await?;
                }
            }
        }
    }
    Ok(())
}

pub async fn validate_comment_block_refs_in_conn(
    conn: &mut SqliteConnection,
    raw_body: &str,
    previous_body: Option<&str>,
    catalog: &RegistryCatalog<RegisteredBlockSchema>,
    required_locale: Option<&str>,
    allowed_context_fields: Option<&[String]>,
) -> ApiResult<()> {
    let doc = parse_blocks_lenient_or_recover(raw_body).document;
    let prev_doc = previous_body.map(|b| parse_blocks_lenient_or_recover(b).document);
    validate_document_block_refs_in_conn(
        conn,
        &doc,
        prev_doc.as_ref(),
        catalog,
        required_locale,
        allowed_context_fields,
    )
    .await
}
