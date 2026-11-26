//! Pure-Rust Gutenberg-style Comment Block (`<!-- od:... -->`) codec for unified
//! entry `values_json["body"]` storage.
//!
//! Contains zero I/O, HTTP, or SQL. Matches `crate::builder::BuilderDocument`
//! 1:1 while preserving `#[serde(deny_unknown_fields)]` fidelity on all node
//! payloads.

use super::*;

pub(super) use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt,
};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use serde_json::Value;

pub(super) use crate::{
    block_schema::RegisteredBlockSchema,
    builder::{
        BUILDER_SCHEMA_VERSION, BindingTarget, BuilderDocument, ButtonProps, ColumnNode,
        ContactFormProps, ContentSlotProps, ElementBinding, ElementContent, ElementNode,
        HeadingProps, ImageProps, KnownElement, LegacyBlockProps, ListProps, NodeMeta,
        PatternRefProps, PluginBlockProps, Responsive, RowNode, SectionNode, SectionStyle,
        TemplatePartProps, TextProps, VideoProps, empty_node_meta_option,
        empty_section_style_option,
    },
    id::valid_package_id,
    registry::{DeclarationState, ProviderStatus, RegistryCatalog},
};

/// Maximum serialized UTF-8 byte length of a `values_json["body"]` string
/// containing `<!-- od:... -->` blocks (`384 KiB`).
pub const MAX_BUILDER_BLOCK_BODY_BYTES: usize = 384 * 1024;

pub(super) const MAX_TEXT_NODE_CHARS: usize = 16_384;

pub(super) const OPEN_PREFIX: &str = "<!-- od:";

pub(super) const CLOSE_PREFIX: &str = "<!-- /od:";

pub(super) const COMMENT_END: &str = "-->";

pub(super) const TEXT_CLOSE_TAG: &str = "<!-- /od:text -->";

impl fmt::Display for BuilderBlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(msg) => f.write_str(msg),
            Self::TooLarge => write!(
                f,
                "Comment block body exceeds maximum {} bytes.",
                MAX_BUILDER_BLOCK_BODY_BYTES
            ),
            Self::Json(msg) => write!(f, "Comment block JSON error: {msg}"),
        }
    }
}

pub fn contains_blocks(raw_body: &str) -> bool {
    raw_body.contains(OPEN_PREFIX)
}

/// Serializes `BuilderDocument` into clean Gutenberg-style comment blocks
/// (`<!-- od:... -->`) without inserting redundant wrapper `<div>` elements.
pub fn serialize_builder_document_to_blocks(
    doc: &BuilderDocument,
) -> Result<String, BuilderBlockError> {
    if doc.schema_version != BUILDER_SCHEMA_VERSION {
        return Err(BuilderBlockError::Invalid(
            "Unsupported Builder schema version",
        ));
    }
    let mut out = String::new();
    for section in &doc.sections {
        let sec_meta = encode_comment_json(&ContainerMetaRef {
            id: &section.id,
            responsive: section.responsive.as_ref().filter(|r| !r.is_empty()),
            style: section
                .style
                .as_ref()
                .filter(|s| !empty_section_style_option(&Some((*s).clone()))),
            meta: section
                .meta
                .as_ref()
                .filter(|m| !empty_node_meta_option(&Some((*m).clone()))),
        })?;
        push_line(&mut out, &format!("<!-- od:section {sec_meta} -->"));

        for row in &section.children {
            let row_meta = encode_comment_json(&ContainerMetaRef {
                id: &row.id,
                responsive: row.responsive.as_ref().filter(|r| !r.is_empty()),
                style: None,
                meta: row.meta.as_ref().filter(|m| !m.is_empty()),
            })?;
            push_line(&mut out, &format!("<!-- od:row {row_meta} -->"));

            for col in &row.children {
                let col_meta = encode_comment_json(&ContainerMetaRef {
                    id: &col.id,
                    responsive: col.responsive.as_ref().filter(|r| !r.is_empty()),
                    style: None,
                    meta: col.meta.as_ref().filter(|m| !m.is_empty()),
                })?;
                push_line(&mut out, &format!("<!-- od:column {col_meta} -->"));

                for element in &col.children {
                    let el_meta = element.meta.as_ref().filter(|m| !m.is_empty());
                    match &element.content {
                        ElementContent::Known(known) => match known {
                            KnownElement::Heading(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "heading",
                                    &element.id,
                                    el_meta,
                                    props,
                                )?;
                            }
                            KnownElement::Text(props) => {
                                if text_has_comment_boundary(&props.text) {
                                    serialize_self_closing(
                                        &mut out,
                                        "text",
                                        &element.id,
                                        el_meta,
                                        props,
                                    )?;
                                } else {
                                    let meta = encode_comment_json(&PairedTextMetaRef {
                                        id: &element.id,
                                        rich: props.rich,
                                        meta: el_meta,
                                        binding: props.binding.as_ref(),
                                    })?;
                                    push_line(&mut out, &format!("<!-- od:text {meta} -->"));
                                    if !out.is_empty() {
                                        out.push('\n');
                                    }
                                    out.push_str(&props.text);
                                    out.push('\n');
                                    out.push_str(TEXT_CLOSE_TAG);
                                }
                            }
                            KnownElement::Image(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "image",
                                    &element.id,
                                    el_meta,
                                    props,
                                )?;
                            }
                            KnownElement::Video(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "video",
                                    &element.id,
                                    el_meta,
                                    props,
                                )?;
                            }
                            KnownElement::Button(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "button",
                                    &element.id,
                                    el_meta,
                                    props,
                                )?;
                            }
                            KnownElement::List(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "list",
                                    &element.id,
                                    el_meta,
                                    props,
                                )?;
                            }
                            KnownElement::ContactForm(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "contact_form",
                                    &element.id,
                                    el_meta,
                                    props,
                                )?;
                            }
                            KnownElement::ContentSlot(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "content_slot",
                                    &element.id,
                                    el_meta,
                                    props,
                                )?;
                            }
                            KnownElement::PatternRef(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "pattern_ref",
                                    &element.id,
                                    el_meta,
                                    props,
                                )?;
                            }
                            KnownElement::TemplatePart(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "template_part",
                                    &element.id,
                                    el_meta,
                                    props,
                                )?;
                            }
                            KnownElement::PluginBlock(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "plugin_block",
                                    &element.id,
                                    el_meta,
                                    props,
                                )?;
                            }
                            KnownElement::Legacy(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "legacy",
                                    &element.id,
                                    el_meta,
                                    props,
                                )?;
                            }
                        },
                        ElementContent::Unresolved(raw) => {
                            let payload = encode_comment_json(&UnresolvedRef {
                                id: &element.id,
                                meta: el_meta,
                                raw,
                            })?;
                            push_line(&mut out, &format!("<!-- od:unresolved {payload} /-->"));
                        }
                    }
                }

                push_line(&mut out, "<!-- /od:column -->");
            }

            push_line(&mut out, "<!-- /od:row -->");
        }

        push_line(&mut out, "<!-- /od:section -->");
    }

    if out.len() > MAX_BUILDER_BLOCK_BODY_BYTES {
        return Err(BuilderBlockError::TooLarge);
    }
    Ok(out)
}

pub(super) fn chunk_text_16k(text: &str) -> Vec<String> {
    let clean = sanitize_text_for_builder(text);
    if clean.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut remaining = clean.as_str();
    while !remaining.is_empty() {
        let mut char_count = 0usize;
        let mut split_at = remaining.len();
        let mut last_newline_after = None;
        for (idx, ch) in remaining.char_indices() {
            if char_count == MAX_TEXT_NODE_CHARS {
                split_at = idx;
                break;
            }
            char_count += 1;
            if ch == '\n' {
                last_newline_after = Some(idx + ch.len_utf8());
            }
        }
        if split_at == remaining.len() {
            chunks.push(remaining.to_owned());
            break;
        }
        let cut = last_newline_after
            .filter(|&pos| pos > 0)
            .unwrap_or(split_at);
        chunks.push(remaining[..cut].to_owned());
        remaining = &remaining[cut..];
    }
    chunks
}

pub(super) fn find_next_block_marker(haystack: &str, from: usize) -> Option<(usize, bool)> {
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

pub(super) fn parse_blocks_internal(
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

/// Scans `<!-- od:... -->` comments leniently to extract referenced media and
/// pattern IDs, including `Media` fields from active, declared `od:plugin_block`
/// schemas (`ProviderInactive`/`Withdrawn` entries emit zero media).
pub fn extract_block_refs_lenient_with_schemas(
    raw_body: &str,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> ExtractedBlockRefs {
    let mut refs = ExtractedBlockRefs::default();
    if !contains_blocks(raw_body) {
        return refs;
    }
    let mut cursor = 0usize;
    while let Some(rel) = raw_body[cursor..].find(OPEN_PREFIX) {
        let marker_pos = cursor + rel;
        let Some(end_rel) = raw_body[marker_pos..].find(COMMENT_END) else {
            break;
        };
        let comment_end = marker_pos + end_rel + COMMENT_END.len();
        let raw_inner = &raw_body[marker_pos + OPEN_PREFIX.len()..marker_pos + end_rel];
        let body_inner = raw_inner
            .trim()
            .strip_suffix('/')
            .map(str::trim)
            .unwrap_or_else(|| raw_inner.trim());

        if let Ok((tag, json_str)) = split_tag_and_json(body_inner) {
            match tag {
                "section" => {
                    if let Ok(meta) = serde_json::from_str::<ContainerMeta>(json_str)
                        && let Some(bg_media) = meta
                            .style
                            .as_ref()
                            .and_then(|s| s.background_media_id.as_deref())
                            .filter(|id| !id.is_empty())
                    {
                        refs.image_ids.insert(bg_media.to_owned());
                    }
                }
                "image" => {
                    if let Ok((_, _, props)) = decode_props_with_id::<ImageProps>(json_str)
                        && !props.media_id.is_empty()
                        && !props
                            .binding
                            .as_ref()
                            .is_some_and(|b| b.target == BindingTarget::MediaId)
                    {
                        refs.image_ids.insert(props.media_id);
                    }
                }
                "video" => {
                    if let Ok((_, _, props)) = decode_props_with_id::<VideoProps>(json_str) {
                        if !props.media_id.is_empty() {
                            refs.video_ids.insert(props.media_id);
                        }
                        if let Some(poster) = props.poster_image_id
                            && !poster.is_empty()
                        {
                            refs.poster_image_ids.insert(poster);
                        }
                    }
                }
                "pattern_ref" => {
                    if let Ok((_, _, props)) = decode_props_with_id::<PatternRefProps>(json_str) {
                        refs.pattern_ids.insert(props.pattern_id);
                    }
                }
                "plugin_block" => {
                    if let Ok((_, _, props)) = decode_props_with_id::<PluginBlockProps>(json_str)
                        && let Some(entry) = schemas.get(&props.block_id)
                        && entry.status == ProviderStatus::Active
                        && entry.declaration_state == DeclarationState::Declared
                    {
                        let collected = entry.descriptor.schema.collect_media_ids(&props.values);
                        refs.image_ids.extend(collected.image_ids);
                        refs.video_ids.extend(collected.video_ids);
                        refs.file_ids.extend(collected.file_ids);
                        refs.any_media_ids.extend(collected.any_ids);
                        refs.pattern_ids
                            .extend(entry.descriptor.schema.collect_pattern_ids(&props.values));
                    }
                }
                _ => {}
            }
        }
        cursor = comment_end;
    }
    refs
}
