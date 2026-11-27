use super::*;

pub const MAX_BUILDER_BLOCK_BODY_BYTES: usize = 384 * 1024;

pub(crate) const MAX_TEXT_NODE_CHARS: usize = 16_384;

pub(crate) const OPEN_PREFIX: &str = "<!-- od:";

pub(crate) const CLOSE_PREFIX: &str = "<!-- /od:";

pub(crate) const COMMENT_END: &str = "-->";

pub(crate) const TEXT_CLOSE_TAG: &str = "<!-- /od:text -->";

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
                            KnownElement::SearchForm(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "search_form",
                                    &element.id,
                                    el_meta,
                                    props,
                                )?;
                            }
                            KnownElement::SearchResults(props) => {
                                serialize_self_closing(
                                    &mut out,
                                    "search_results",
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

pub(crate) fn chunk_text_16k(text: &str) -> Vec<String> {
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
