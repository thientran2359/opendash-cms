use super::*;

pub(crate) fn find_next_block_marker(haystack: &str, from: usize) -> Option<(usize, bool)> {
    let mut idx = from;
    while let Some(rel) = haystack[idx..].find("<!-- ") {
        let pos = idx + rel;
        let tail = &haystack[pos..];
        if tail.starts_with(OPEN_PREFIX) {
            return Some((pos, true));
        }
        if tail.starts_with(CLOSE_PREFIX) {
            return Some((pos, false));
        }
        idx = pos + 5;
    }
    None
}

pub(crate) fn parse_blocks_internal(
    raw_body: &str,
    strict: bool,
    catalog: &RegistryCatalog<RegisteredBlockSchema>,
) -> Result<ParsedBlockDocument, BuilderBlockError> {
    if strict && raw_body.len() > MAX_BUILDER_BLOCK_BODY_BYTES {
        return Err(BuilderBlockError::TooLarge);
    }
    let normalized = normalize_newlines(raw_body);
    let text = normalized.as_str();

    if !text.contains(OPEN_PREFIX) && !text.contains(CLOSE_PREFIX) {
        let mut state = ParserState::new(strict, catalog);
        state.push_freeform(text, false);
        return state.finish();
    }

    let mut state = ParserState::new(strict, catalog);
    let mut cursor = 0_usize;

    while cursor < text.len() {
        let Some((marker_pos, is_open)) = find_next_block_marker(text, cursor) else {
            state.push_freeform(&text[cursor..], true);
            break;
        };

        if marker_pos > cursor {
            state.push_freeform(&text[cursor..marker_pos], true);
        }

        let Some(end_rel) = text[marker_pos..].find(COMMENT_END) else {
            if strict {
                return Err(BuilderBlockError::Invalid(
                    "Unterminated OpenDash comment block",
                ));
            }
            state.push_recovered_region(&text[marker_pos..], "Unterminated OpenDash comment block");
            break;
        };
        let comment_end = marker_pos + end_rel + COMMENT_END.len();
        let comment_slice = &text[marker_pos..comment_end];

        if !is_open {
            let tag = text[marker_pos + CLOSE_PREFIX.len()..marker_pos + end_rel].trim();
            if tag.ends_with('/') || !valid_namespaced_block_id(tag) {
                if strict {
                    return Err(BuilderBlockError::Invalid("Malformed closing block tag"));
                }
                state.push_recovered_region(comment_slice, "Malformed closing block tag");
                cursor = comment_end;
                continue;
            }
            if !state.close_container(tag)? {
                state.push_recovered_region(comment_slice, "Stray closing comment block tag");
            }
            cursor = comment_end;
            continue;
        }

        let raw_inner = &text[marker_pos + OPEN_PREFIX.len()..marker_pos + end_rel];
        let trimmed_inner = raw_inner.trim();
        let (self_closing, body_inner) = if let Some(stripped) = trimmed_inner.strip_suffix('/') {
            (true, stripped.trim())
        } else {
            (false, trimmed_inner)
        };

        let Ok((tag, json_str)) = split_tag_and_json(body_inner) else {
            if strict {
                return Err(BuilderBlockError::Invalid("Malformed block header"));
            }
            state.push_recovered_region(comment_slice, "Malformed block header");
            cursor = comment_end;
            continue;
        };

        if !valid_namespaced_block_id(tag) {
            if strict {
                return Err(BuilderBlockError::Invalid("Invalid namespaced block tag"));
            }
            state.push_recovered_region(comment_slice, "Invalid namespaced block tag");
            cursor = comment_end;
            continue;
        }

        let Ok(mut payload) = serde_json::from_str::<CommentNodePayload>(json_str) else {
            if strict {
                return Err(BuilderBlockError::Invalid("Invalid comment block JSON"));
            }
            state.push_recovered_region(comment_slice, "Invalid comment block JSON");
            cursor = comment_end;
            continue;
        };
        if strict && !valid_package_id(&payload.id) {
            return Err(BuilderBlockError::Invalid("Invalid block node id"));
        }

        if tag == "core/text" && !self_closing {
            let Some(close_rel) = text[comment_end..].find(TEXT_CLOSE_TAG) else {
                if strict {
                    return Err(BuilderBlockError::Invalid(
                        "Unclosed <!-- od:core/text --> block",
                    ));
                }
                state.push_recovered_region(
                    &text[marker_pos..],
                    "Recovered unclosed <!-- od:core/text --> block",
                );
                break;
            };
            let raw_inner_text = &text[comment_end..comment_end + close_rel];
            let inner_text = raw_inner_text.strip_prefix('\n').unwrap_or(raw_inner_text);
            let inner_text = inner_text.strip_suffix('\n').unwrap_or(inner_text);
            state.used_ids.insert(payload.id.clone());
            let next_cursor = comment_end + close_rel + TEXT_CLOSE_TAG.len();

            if inner_text.chars().count() <= MAX_TEXT_NODE_CHARS && !inner_text.contains('\0') {
                payload
                    .props
                    .insert("content".to_owned(), Value::String(inner_text.to_owned()));
                state.attach_leaf(BlockNode {
                    id: payload.id,
                    block_id: "core/text".to_owned(),
                    props: payload.props,
                    meta: payload.meta,
                    children: Vec::new(),
                });
            } else if strict {
                return Err(BuilderBlockError::Invalid(
                    "od:core/text content exceeds 16384 characters or contains NUL",
                ));
            } else {
                let chunks = chunk_text_16k(inner_text);
                for (i, chunk) in chunks.into_iter().enumerate() {
                    let id = if i == 0 {
                        payload.id.clone()
                    } else {
                        state.rec_seq += 1;
                        state.unique_prefixed(&format!("txt-rec-{}", state.rec_seq))
                    };
                    let mut props = if i == 0 {
                        payload.props.clone()
                    } else {
                        BTreeMap::new()
                    };
                    props.insert("content".to_owned(), Value::String(chunk));
                    if i == 0 {
                        state.record_recovery(
                            &id,
                            "Split oversized or control-sanitized od:core/text block",
                        );
                    }
                    state.attach_leaf(BlockNode {
                        id,
                        block_id: "core/text".to_owned(),
                        props,
                        meta: if i == 0 { payload.meta.clone() } else { None },
                        children: Vec::new(),
                    });
                }
            }
            cursor = next_cursor;
            continue;
        }

        state.used_ids.insert(payload.id.clone());
        let node = BlockNode {
            id: payload.id,
            block_id: tag.to_owned(),
            props: payload.props,
            meta: payload.meta,
            children: Vec::new(),
        };
        if self_closing {
            state.attach_leaf(node);
        } else {
            if state.stack.len() >= MAX_BUILDER_DEPTH + 56 {
                return Err(BuilderBlockError::Invalid(
                    "Comment block nesting exceeds safety limit",
                ));
            }
            state.push_container(node);
        }
        cursor = comment_end;
    }

    state.finish()
}
