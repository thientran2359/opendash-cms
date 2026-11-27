use super::*;

fn collect_core_fallback_refs(
    tag: &str,
    props: &BTreeMap<String, Value>,
    refs: &mut ExtractedBlockRefs,
) {
    let str_prop = |k: &str| {
        props
            .get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    match tag {
        "core/section" | "core/banner" => {
            if let Some(bg) = str_prop("bg_image") {
                refs.image_ids.insert(bg.to_owned());
            }
        }
        "core/card" | "core/image" => {
            if let Some(img) = str_prop("image") {
                refs.image_ids.insert(img.to_owned());
            }
        }
        "core/video" => {
            if let Some(vid) = str_prop("video") {
                refs.video_ids.insert(vid.to_owned());
            }
            if let Some(poster) = str_prop("poster") {
                refs.poster_image_ids.insert(poster.to_owned());
            }
        }
        "core/pattern-ref" => {
            if let Some(pid) = parse_positive_i64_prop(props.get("pattern_id")) {
                refs.pattern_ids.insert(pid);
            }
        }
        _ => {}
    }
}

fn close_ref_stack(open_stack: &mut Vec<(String, bool)>, inactive_depth: &mut usize, tag: &str) {
    if let Some(pos) = open_stack.iter().rposition(|(name, _)| name == tag) {
        while open_stack.len() > pos {
            if let Some((_, was_inactive)) = open_stack.pop()
                && was_inactive
            {
                *inactive_depth = inactive_depth.saturating_sub(1);
            }
        }
    }
}

pub(crate) fn extract_block_refs_impl(
    raw_body: &str,
    schemas: Option<&RegistryCatalog<RegisteredBlockSchema>>,
) -> ExtractedBlockRefs {
    let mut refs = ExtractedBlockRefs::default();
    if !contains_blocks(raw_body) {
        return refs;
    }

    let mut open_stack: Vec<(String, bool)> = Vec::new();
    let mut inactive_depth = 0_usize;
    let mut cursor = 0_usize;

    while let Some((marker_pos, is_open)) = find_next_block_marker(raw_body, cursor) {
        let Some(end_rel) = raw_body[marker_pos..].find(COMMENT_END) else {
            break;
        };
        let comment_end = marker_pos + end_rel + COMMENT_END.len();

        if !is_open {
            let tag = raw_body[marker_pos + CLOSE_PREFIX.len()..marker_pos + end_rel].trim();
            close_ref_stack(&mut open_stack, &mut inactive_depth, tag);
            cursor = comment_end;
            continue;
        }

        let raw_inner = &raw_body[marker_pos + OPEN_PREFIX.len()..marker_pos + end_rel];
        let trimmed = raw_inner.trim();
        let (self_closing, body_inner) = if let Some(stripped) = trimmed.strip_suffix('/') {
            (true, stripped.trim())
        } else {
            (false, trimmed)
        };

        let Ok((tag, json_str)) =
            crate::builder_blocks::parser_state::split_tag_and_json(body_inner)
        else {
            cursor = comment_end;
            continue;
        };

        let payload = serde_json::from_str::<CommentNodePayload>(json_str).ok();
        let Some(catalog) = schemas else {
            if !self_closing && tag == "core/text" {
                if let Some(close_rel) = raw_body[comment_end..].find(TEXT_CLOSE_TAG) {
                    if let Some(p) = payload {
                        collect_core_fallback_refs(tag, &p.props, &mut refs);
                    }
                    cursor = comment_end + close_rel + TEXT_CLOSE_TAG.len();
                    continue;
                }
            }
            if let Some(p) = payload {
                collect_core_fallback_refs(tag, &p.props, &mut refs);
            }
            cursor = comment_end;
            continue;
        };

        let active_schema = catalog
            .get(tag)
            .filter(|entry| {
                entry.status == ProviderStatus::Active
                    && entry.declaration_state == DeclarationState::Declared
            })
            .map(|entry| &entry.descriptor.schema);

        if tag == "core/text" && !self_closing {
            if let Some(close_rel) = raw_body[comment_end..].find(TEXT_CLOSE_TAG) {
                if inactive_depth == 0
                    && let (Some(schema), Some(p)) = (active_schema, payload)
                {
                    let collected = schema.collect_media_ids(&p.props);
                    refs.image_ids.extend(collected.image_ids);
                    refs.video_ids.extend(collected.video_ids);
                    refs.file_ids.extend(collected.file_ids);
                    refs.any_media_ids.extend(collected.any_ids);
                }
                cursor = comment_end + close_rel + TEXT_CLOSE_TAG.len();
                continue;
            }
        }

        let Some(schema) = active_schema else {
            if !self_closing {
                open_stack.push((tag.to_owned(), true));
                inactive_depth += 1;
            }
            cursor = comment_end;
            continue;
        };

        let Some(p) = payload else {
            if !self_closing {
                open_stack.push((tag.to_owned(), true));
                inactive_depth += 1;
            }
            cursor = comment_end;
            continue;
        };

        if !self_closing {
            open_stack.push((tag.to_owned(), false));
        }
        if inactive_depth == 0 {
            let mut collected = schema.collect_media_ids(&p.props);
            if tag == "core/video"
                && let Some(poster) = p
                    .props
                    .get("poster")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            {
                collected.image_ids.remove(poster);
                refs.poster_image_ids.insert(poster.to_owned());
            }
            refs.image_ids.extend(collected.image_ids);
            refs.video_ids.extend(collected.video_ids);
            refs.file_ids.extend(collected.file_ids);
            refs.any_media_ids.extend(collected.any_ids);
            refs.pattern_ids
                .extend(schema.collect_pattern_ids(&p.props));
            if tag == "core/pattern-ref"
                && let Some(pid) = parse_positive_i64_prop(p.props.get("pattern_id"))
            {
                refs.pattern_ids.insert(pid);
            }
        }

        cursor = comment_end;
    }

    refs
}

/// Scans `<!-- od:... -->` comments using a stack-aware provider gate (`inactive_depth`).
/// Any subtree under a `ProviderInactive`, `Withdrawn`, or missing block emits ZERO media or pattern refs.
pub fn extract_block_refs_lenient_with_schemas(
    raw_body: &str,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> ExtractedBlockRefs {
    extract_block_refs_impl(raw_body, Some(schemas))
}
