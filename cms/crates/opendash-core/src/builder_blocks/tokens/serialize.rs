use super::*;

pub const MAX_BUILDER_BLOCK_BODY_BYTES: usize = 384 * 1024;
pub(crate) const MAX_TEXT_NODE_CHARS: usize = 16_384;
pub(crate) const OPEN_PREFIX: &str = "<!-- od:";
pub(crate) const CLOSE_PREFIX: &str = "<!-- /od:";
pub(crate) const COMMENT_END: &str = "-->";
pub(crate) const TEXT_CLOSE_TAG: &str = "<!-- /od:core/text -->";

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

#[derive(Serialize)]
struct CommentHeaderRef<'a> {
    id: &'a str,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    props: &'a BTreeMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<&'a NodeMeta>,
}

fn serialize_node_rec(
    node: &BlockNode,
    out: &mut String,
    depth: usize,
) -> Result<(), BuilderBlockError> {
    if depth > MAX_BUILDER_DEPTH {
        return Err(BuilderBlockError::Invalid("Builder exceeds max depth 8"));
    }
    if !valid_namespaced_block_id(&node.block_id) {
        return Err(BuilderBlockError::Invalid("Invalid Builder block_id"));
    }
    let meta = node
        .meta
        .as_ref()
        .filter(|m| !empty_node_meta_option(&Some((*m).clone())));

    if node.block_id == "core/text"
        && node.children.is_empty()
        && let Some(Value::String(content)) = node.props.get("content")
        && !text_has_comment_boundary(content)
    {
        let mut header_props = node.props.clone();
        header_props.remove("content");
        let payload = encode_comment_json(&CommentHeaderRef {
            id: &node.id,
            props: &header_props,
            meta,
        })?;
        push_line(out, &format!("<!-- od:core/text {payload} -->"));
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(content);
        out.push('\n');
        out.push_str(TEXT_CLOSE_TAG);
        return Ok(());
    }

    let payload = encode_comment_json(&CommentHeaderRef {
        id: &node.id,
        props: &node.props,
        meta,
    })?;
    if node.children.is_empty() {
        push_line(out, &format!("<!-- od:{} {payload} /-->", node.block_id));
    } else {
        push_line(out, &format!("<!-- od:{} {payload} -->", node.block_id));
        for child in &node.children {
            serialize_node_rec(child, out, depth + 1)?;
        }
        push_line(out, &format!("<!-- /od:{} -->", node.block_id));
    }
    Ok(())
}

/// Serializes `BuilderDocument` into recursive Gutenberg-style comment blocks (`<!-- od:<block_id> ... -->`).
pub fn serialize_builder_document_to_blocks(
    doc: &BuilderDocument,
) -> Result<String, BuilderBlockError> {
    if doc.schema_version != BUILDER_SCHEMA_VERSION {
        return Err(BuilderBlockError::Invalid(
            "Unsupported Builder schema version",
        ));
    }
    let mut out = String::new();
    for block in &doc.blocks {
        serialize_node_rec(block, &mut out, 1)?;
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
