use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuilderBlockError {
    Invalid(&'static str),
    TooLarge,
    Json(String),
}

impl std::error::Error for BuilderBlockError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockDiagnostic {
    pub node_id: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedBlockDocument {
    pub document: BuilderDocument,
    pub diagnostics: Vec<BlockDiagnostic>,
    pub had_freeform_wrap: bool,
    pub had_recovery: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractedBlockRefs {
    pub image_ids: BTreeSet<String>,
    pub video_ids: BTreeSet<String>,
    pub poster_image_ids: BTreeSet<String>,
    pub file_ids: BTreeSet<String>,
    pub any_media_ids: BTreeSet<String>,
    pub pattern_ids: BTreeSet<i64>,
}

/// Serializes `value` as compact JSON and escapes any `<`, `>`, or `--` sequences
/// so the payload can never prematurely close or corrupt an HTML comment.
pub fn encode_comment_json<T: Serialize>(value: &T) -> Result<String, BuilderBlockError> {
    let mut encoded =
        serde_json::to_string(value).map_err(|err| BuilderBlockError::Json(err.to_string()))?;
    if encoded.contains('<') {
        encoded = encoded.replace('<', "\\u003c");
    }
    if encoded.contains('>') {
        encoded = encoded.replace('>', "\\u003e");
    }
    while encoded.contains("--") {
        encoded = encoded.replace("--", "\\u002d\\u002d");
    }
    Ok(encoded)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommentNodePayload {
    pub(super) id: String,
    #[serde(default)]
    pub(super) props: BTreeMap<String, Value>,
    #[serde(default)]
    pub(super) meta: Option<NodeMeta>,
}

pub(super) fn push_line(out: &mut String, line: &str) {
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(line);
}

pub(super) fn text_has_comment_boundary(text: &str) -> bool {
    text.contains("<!--") || text.contains("-->") || text.contains("--!>")
}

pub(super) fn normalize_newlines(raw: &str) -> String {
    if !raw.contains('\r') {
        return raw.to_owned();
    }
    raw.replace("\r\n", "\n").replace('\r', "\n")
}

pub(super) fn sanitize_text_for_builder(raw: &str) -> String {
    raw.chars()
        .filter(|&ch| ch != '\0' && (!ch.is_control() || ch == '\n' || ch == '\t'))
        .collect()
}

pub fn parse_blocks_to_builder_document(
    raw_body: &str,
) -> Result<ParsedBlockDocument, BuilderBlockError> {
    parse_blocks_internal(raw_body, true, &RegistryCatalog::new())
}

pub fn parse_blocks_lenient_or_recover(raw_body: &str) -> ParsedBlockDocument {
    parse_blocks_internal(raw_body, false, &RegistryCatalog::new())
        .expect("lenient block parser never returns Err")
}

pub fn parse_blocks_lenient_with_schemas(
    raw_body: &str,
    catalog: &RegistryCatalog<RegisteredBlockSchema>,
) -> ParsedBlockDocument {
    parse_blocks_internal(raw_body, false, catalog).expect("lenient block parser never returns Err")
}

pub fn extract_visible_text(raw_body: &str) -> String {
    extract_visible_text_with_schemas(raw_body, &RegistryCatalog::new())
}

fn collect_visible_text_fallback(node: &BlockNode, parts: &mut Vec<String>) {
    match node.block_id.as_str() {
        "core/heading" => {
            if let Some(t) = node.props.get("text").and_then(Value::as_str)
                && !t.trim().is_empty()
            {
                parts.push(t.to_owned());
            }
        }
        "core/text" => {
            if let Some(c) = node.props.get("content").and_then(Value::as_str) {
                let plain = crate::richtext::strip_richtext_to_plain(c);
                if !plain.is_empty() {
                    parts.push(plain);
                }
            }
        }
        "core/button" => {
            if let Some(lbl) = node
                .props
                .get("cta")
                .and_then(|c| c.get("label"))
                .and_then(Value::as_str)
                && !lbl.trim().is_empty()
            {
                parts.push(lbl.to_owned());
            }
        }
        "core/accordion-item" => {
            if let Some(t) = node.props.get("title").and_then(Value::as_str)
                && !t.trim().is_empty()
            {
                parts.push(t.to_owned());
            }
        }
        "core/image" => {
            if let Some(cap) = node.props.get("caption").and_then(Value::as_str)
                && !cap.trim().is_empty()
            {
                parts.push(cap.to_owned());
            }
        }
        _ => {}
    }
}

fn collect_visible_text_nodes(
    nodes: &[BlockNode],
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
    parts: &mut Vec<String>,
) {
    for node in nodes {
        if let Some(entry) = schemas.get(&node.block_id) {
            if entry.status != ProviderStatus::Active
                || entry.declaration_state != DeclarationState::Declared
            {
                continue;
            }
            let mut raw_parts = Vec::new();
            entry
                .descriptor
                .schema
                .extract_visible_text(&node.props, &mut raw_parts);
            for raw_part in raw_parts {
                let clean = crate::richtext::strip_richtext_to_plain(&raw_part);
                if !clean.is_empty() {
                    parts.push(clean);
                }
            }
            collect_visible_text_nodes(&node.children, schemas, parts);
        } else if schemas.is_empty() || node.block_id.starts_with("core/") {
            collect_visible_text_fallback(node, parts);
            collect_visible_text_nodes(&node.children, schemas, parts);
        }
    }
}

/// Extracts human-readable plain text from an already parsed V1 `BuilderDocument`,
/// skipping inactive/withdrawn plugin/theme subtrees when a `schemas` catalog is supplied.
pub fn extract_document_visible_text(
    doc: &BuilderDocument,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> String {
    let mut parts = Vec::new();
    collect_visible_text_nodes(&doc.blocks, schemas, &mut parts);
    parts.join("\n\n")
}

/// Extracts human-readable plain text recursively across V1 `BlockNode`s, skipping
/// inactive/withdrawn container subtrees when a non-empty `schemas` catalog is supplied.
pub fn extract_visible_text_with_schemas(
    raw_body: &str,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> String {
    if !contains_blocks(raw_body) {
        return raw_body.to_owned();
    }
    let parsed = parse_blocks_lenient_or_recover(raw_body);
    extract_document_visible_text(&parsed.document, schemas)
}
