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
) -> Result<ParsedBlockDocument, BuilderBlockError> {
    if strict && raw_body.len() > MAX_BUILDER_BLOCK_BODY_BYTES {
        return Err(BuilderBlockError::TooLarge);
    }
    let normalized = normalize_newlines(raw_body);
    let text = normalized.as_str();

    if !text.contains(OPEN_PREFIX) && !text.contains(CLOSE_PREFIX) {
        let mut state = ParserState::new(strict);
        state.push_freeform(text, false);
        return state.finish();
    }

    let mut state = ParserState::new(strict);
    let mut cursor = 0usize;

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
            let inner = text[marker_pos + CLOSE_PREFIX.len()..marker_pos + end_rel].trim();
            if inner.ends_with('/') {
                if strict {
                    return Err(BuilderBlockError::Invalid("Malformed closing block tag"));
                }
                state.push_recovered_region(comment_slice, "Malformed closing block tag");
                cursor = comment_end;
                continue;
            }
            match inner {
                "column" => {
                    if state.cur_col.is_none() {
                        if strict {
                            return Err(BuilderBlockError::Invalid(
                                "Unexpected <!-- /od:column -->",
                            ));
                        }
                        state.push_recovered_region(comment_slice, "Stray <!-- /od:column -->");
                    } else {
                        state.close_column();
                    }
                }
                "row" => {
                    if state.cur_row.is_none() {
                        if strict {
                            return Err(BuilderBlockError::Invalid("Unexpected <!-- /od:row -->"));
                        }
                        state.push_recovered_region(comment_slice, "Stray <!-- /od:row -->");
                    } else {
                        if state.cur_col.is_some() {
                            if strict {
                                return Err(BuilderBlockError::Invalid(
                                    "Row closed while column still open",
                                ));
                            }
                            let col_id = state.cur_col.as_ref().unwrap().id.clone();
                            state
                                .record_recovery(&col_id, "Auto-closed unclosed column before row");
                        }
                        state.close_row();
                    }
                }
                "section" => {
                    if state.cur_section.is_none() {
                        if strict {
                            return Err(BuilderBlockError::Invalid(
                                "Unexpected <!-- /od:section -->",
                            ));
                        }
                        state.push_recovered_region(comment_slice, "Stray <!-- /od:section -->");
                    } else {
                        if state.cur_row.is_some() || state.cur_col.is_some() {
                            if strict {
                                return Err(BuilderBlockError::Invalid(
                                    "Section closed while inner container still open",
                                ));
                            }
                            let sec_id = state.cur_section.as_ref().unwrap().id.clone();
                            state.record_recovery(
                                &sec_id,
                                "Auto-closed inner container before section end",
                            );
                        }
                        state.close_section();
                    }
                }
                _ => {
                    if strict {
                        return Err(BuilderBlockError::Invalid("Unexpected closing block tag"));
                    }
                    state.push_recovered_region(comment_slice, "Unexpected closing block tag");
                }
            }
            cursor = comment_end;
            continue;
        }

        // Opening or self-closing tag starting with `<!-- od:`
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

        match tag {
            "section" => {
                if self_closing {
                    if strict {
                        return Err(BuilderBlockError::Invalid(
                            "od:section cannot be self-closing",
                        ));
                    }
                    state.push_recovered_region(comment_slice, "Self-closing od:section recovered");
                    cursor = comment_end;
                    continue;
                }
                let Ok(meta) = serde_json::from_str::<ContainerMeta>(json_str) else {
                    if strict {
                        return Err(BuilderBlockError::Invalid("Invalid od:section metadata"));
                    }
                    state.push_recovered_region(comment_slice, "Invalid od:section metadata");
                    cursor = comment_end;
                    continue;
                };
                if state.cur_section.is_some() {
                    if strict {
                        return Err(BuilderBlockError::Invalid("Nested od:section is forbidden"));
                    }
                    let prev_id = state.cur_section.as_ref().unwrap().id.clone();
                    state.close_section();
                    state.record_recovery(
                        &prev_id,
                        "Auto-closed unclosed section before next section",
                    );
                }
                state.used_ids.insert(meta.id.clone());
                state.cur_section = Some(SectionNode {
                    id: meta.id,
                    responsive: meta.responsive,
                    style: meta.style,
                    meta: meta.meta,
                    children: Vec::new(),
                });
                cursor = comment_end;
            }
            "row" => {
                if self_closing {
                    if strict {
                        return Err(BuilderBlockError::Invalid("od:row cannot be self-closing"));
                    }
                    state.push_recovered_region(comment_slice, "Self-closing od:row recovered");
                    cursor = comment_end;
                    continue;
                }
                let Ok(meta) = serde_json::from_str::<ContainerMeta>(json_str) else {
                    if strict {
                        return Err(BuilderBlockError::Invalid("Invalid od:row metadata"));
                    }
                    state.push_recovered_region(comment_slice, "Invalid od:row metadata");
                    cursor = comment_end;
                    continue;
                };
                if meta.style.is_some() {
                    if strict {
                        return Err(BuilderBlockError::Invalid("od:row cannot carry style"));
                    }
                    state.push_recovered_region(comment_slice, "Invalid od:row style metadata");
                    cursor = comment_end;
                    continue;
                }
                if state.cur_section.is_none() {
                    if strict {
                        return Err(BuilderBlockError::Invalid(
                            "od:row found outside od:section",
                        ));
                    }
                    state.rec_seq += 1;
                    let sec_id = state.unique_prefixed(&format!("sec-rec-{}", state.rec_seq));
                    state.cur_section = Some(SectionNode {
                        id: sec_id,
                        meta: None,
                        responsive: None,
                        style: None,
                        children: Vec::new(),
                    });
                    state.record_recovery(&meta.id, "Created synthetic section for stray od:row");
                }
                if state.cur_row.is_some() {
                    if strict {
                        return Err(BuilderBlockError::Invalid("Nested od:row is forbidden"));
                    }
                    let prev_id = state.cur_row.as_ref().unwrap().id.clone();
                    state.close_row();
                    state.record_recovery(&prev_id, "Auto-closed unclosed row before next row");
                }
                state.used_ids.insert(meta.id.clone());
                state.cur_row = Some(RowNode {
                    id: meta.id,
                    responsive: meta.responsive,
                    meta: meta.meta,
                    children: Vec::new(),
                });
                cursor = comment_end;
            }
            "column" => {
                if self_closing {
                    if strict {
                        return Err(BuilderBlockError::Invalid(
                            "od:column cannot be self-closing",
                        ));
                    }
                    state.push_recovered_region(comment_slice, "Self-closing od:column recovered");
                    cursor = comment_end;
                    continue;
                }
                let Ok(meta) = serde_json::from_str::<ContainerMeta>(json_str) else {
                    if strict {
                        return Err(BuilderBlockError::Invalid("Invalid od:column metadata"));
                    }
                    state.push_recovered_region(comment_slice, "Invalid od:column metadata");
                    cursor = comment_end;
                    continue;
                };
                if meta.style.is_some() {
                    if strict {
                        return Err(BuilderBlockError::Invalid("od:column cannot carry style"));
                    }
                    state.push_recovered_region(comment_slice, "Invalid od:column style metadata");
                    cursor = comment_end;
                    continue;
                }
                if state.cur_row.is_none() {
                    if strict {
                        return Err(BuilderBlockError::Invalid("od:column found outside od:row"));
                    }
                    if state.cur_section.is_none() {
                        state.rec_seq += 1;
                        let sec_id = state.unique_prefixed(&format!("sec-rec-{}", state.rec_seq));
                        state.cur_section = Some(SectionNode {
                            id: sec_id,
                            meta: None,
                            responsive: None,
                            style: None,
                            children: Vec::new(),
                        });
                    }
                    state.rec_seq += 1;
                    let row_id = state.unique_prefixed(&format!("row-rec-{}", state.rec_seq));
                    state.cur_row = Some(RowNode {
                        id: row_id,
                        meta: None,
                        responsive: None,
                        children: Vec::new(),
                    });
                    state.record_recovery(&meta.id, "Created synthetic row for stray od:column");
                }
                if state.cur_col.is_some() {
                    if strict {
                        return Err(BuilderBlockError::Invalid("Nested od:column is forbidden"));
                    }
                    let prev_id = state.cur_col.as_ref().unwrap().id.clone();
                    state.close_column();
                    state.record_recovery(
                        &prev_id,
                        "Auto-closed unclosed column before next column",
                    );
                }
                state.used_ids.insert(meta.id.clone());
                state.cur_col = Some(ColumnNode {
                    id: meta.id,
                    responsive: meta.responsive,
                    meta: meta.meta,
                    children: Vec::new(),
                });
                cursor = comment_end;
            }
            "text" if !self_closing => {
                let Ok(meta) = serde_json::from_str::<PairedTextMeta>(json_str) else {
                    if strict {
                        return Err(BuilderBlockError::Invalid("Invalid od:text metadata"));
                    }
                    state.push_recovered_region(comment_slice, "Invalid od:text metadata");
                    cursor = comment_end;
                    continue;
                };
                if let Some(close_rel) = text[comment_end..].find(TEXT_CLOSE_TAG) {
                    let raw_inner_text = &text[comment_end..comment_end + close_rel];
                    // Strip at most ONE leading/trailing delimiter newline inserted by the serializer,
                    // never calling `.trim()` so user whitespace/newlines remain exact.
                    let inner_text = raw_inner_text.strip_prefix('\n').unwrap_or(raw_inner_text);
                    let inner_text = inner_text.strip_suffix('\n').unwrap_or(inner_text);

                    state.used_ids.insert(meta.id.clone());
                    let next_cursor = comment_end + close_rel + TEXT_CLOSE_TAG.len();
                    let char_len = inner_text.chars().count();
                    if char_len <= MAX_TEXT_NODE_CHARS && !inner_text.contains('\0') {
                        let node = ElementNode {
                            id: meta.id,
                            meta: meta.meta,
                            content: ElementContent::Known(KnownElement::Text(TextProps {
                                text: inner_text.to_owned(),
                                rich: meta.rich,
                                binding: meta.binding,
                            })),
                        };
                        state.push_element(node)?;
                    } else if strict {
                        return Err(BuilderBlockError::Invalid(
                            "od:text content exceeds 16384 characters or contains NUL",
                        ));
                    } else {
                        let chunks = chunk_text_16k(inner_text);
                        for (i, chunk) in chunks.into_iter().enumerate() {
                            let id = if i == 0 {
                                meta.id.clone()
                            } else {
                                state.rec_seq += 1;
                                state.unique_prefixed(&format!("el-rec-{}", state.rec_seq))
                            };
                            if i == 0 {
                                state.record_recovery(
                                    &id,
                                    "Split oversized or control-sanitized od:text block",
                                );
                            }
                            state.push_element(ElementNode {
                                id,
                                meta: None,
                                content: ElementContent::Known(KnownElement::Text(TextProps {
                                    text: chunk,
                                    rich: meta.rich,
                                    binding: if i == 0 { meta.binding.clone() } else { None },
                                })),
                            })?;
                        }
                    }
                    cursor = next_cursor;
                } else {
                    if strict {
                        return Err(BuilderBlockError::Invalid(
                            "Unclosed <!-- od:text --> block",
                        ));
                    }
                    state.push_recovered_region(
                        &text[marker_pos..],
                        "Recovered unclosed <!-- od:text --> block",
                    );
                    break;
                }
            }
            _ => {
                if !self_closing {
                    if strict {
                        return Err(BuilderBlockError::Invalid(
                            "Leaf comment block must be self-closing (/-->)",
                        ));
                    }
                    state.push_recovered_region(
                        comment_slice,
                        "Recovered non-self-closing leaf block",
                    );
                    cursor = comment_end;
                    continue;
                }
                match parse_leaf_content(tag, json_str) {
                    Ok(node) => {
                        state.used_ids.insert(node.id.clone());
                        state.push_element(node)?;
                    }
                    Err(msg) => {
                        if strict {
                            return Err(BuilderBlockError::Invalid(msg));
                        }
                        state.push_recovered_region(comment_slice, msg);
                    }
                }
                cursor = comment_end;
            }
        }
    }

    state.finish()
}
