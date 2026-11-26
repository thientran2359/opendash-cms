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

pub(super) fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Serialize)]
pub(super) struct ContainerMetaRef<'a> {
    pub(super) id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) responsive: Option<&'a Responsive>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) style: Option<&'a SectionStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) meta: Option<&'a NodeMeta>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ContainerMeta {
    pub(super) id: String,
    #[serde(default)]
    pub(super) responsive: Option<Responsive>,
    #[serde(default)]
    pub(super) style: Option<SectionStyle>,
    #[serde(default)]
    pub(super) meta: Option<NodeMeta>,
}

#[derive(Serialize)]
pub(super) struct PairedTextMetaRef<'a> {
    pub(super) id: &'a str,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(super) rich: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) meta: Option<&'a NodeMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) binding: Option<&'a ElementBinding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PairedTextMeta {
    pub(super) id: String,
    #[serde(default)]
    pub(super) rich: bool,
    #[serde(default)]
    pub(super) meta: Option<NodeMeta>,
    #[serde(default)]
    pub(super) binding: Option<ElementBinding>,
}

#[derive(Serialize)]
pub(super) struct ElementWithIdRef<'a, P: Serialize> {
    pub(super) id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) meta: Option<&'a NodeMeta>,
    #[serde(flatten)]
    pub(super) props: &'a P,
}

#[derive(Serialize)]
pub(super) struct UnresolvedRef<'a> {
    pub(super) id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) meta: Option<&'a NodeMeta>,
    pub(super) raw: &'a BTreeMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UnresolvedPayload {
    pub(super) id: String,
    #[serde(default)]
    pub(super) meta: Option<NodeMeta>,
    pub(super) raw: BTreeMap<String, Value>,
}

pub(super) fn push_line(out: &mut String, line: &str) {
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(line);
}

pub(super) fn serialize_self_closing<P: Serialize>(
    out: &mut String,
    tag: &str,
    id: &str,
    meta: Option<&NodeMeta>,
    props: &P,
) -> Result<(), BuilderBlockError> {
    let payload = encode_comment_json(&ElementWithIdRef {
        id,
        meta: meta.filter(|m| !m.is_empty()),
        props,
    })?;
    push_line(out, &format!("<!-- od:{tag} {payload} /-->"));
    Ok(())
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

pub(super) fn decode_props_with_id<P: serde::de::DeserializeOwned>(
    json_str: &str,
) -> Result<(String, Option<NodeMeta>, P), &'static str> {
    let mut map: serde_json::Map<String, Value> =
        serde_json::from_str(json_str).map_err(|_| "Invalid block JSON payload")?;
    let id_val = map.remove("id").ok_or("Missing block node id")?;
    let Value::String(id) = id_val else {
        return Err("Block node id must be a string");
    };
    let meta: Option<NodeMeta> = match map.remove("meta") {
        Some(v) => serde_json::from_value(v).map_err(|_| "Invalid block meta payload")?,
        None => None,
    };
    let props: P =
        serde_json::from_value(Value::Object(map)).map_err(|_| "Invalid block props payload")?;
    Ok((id, meta, props))
}

pub fn parse_blocks_to_builder_document(
    raw_body: &str,
) -> Result<ParsedBlockDocument, BuilderBlockError> {
    parse_blocks_internal(raw_body, true)
}

pub fn parse_blocks_lenient_or_recover(raw_body: &str) -> ParsedBlockDocument {
    parse_blocks_internal(raw_body, false).expect("lenient block parser never returns Err")
}

/// Extracts human-readable plain text from `od:text`, `od:heading`, and `od:button`
/// blocks (used for search/bindings/reset flattening).
pub fn extract_visible_text(raw_body: &str) -> String {
    extract_visible_text_with_schemas(raw_body, &RegistryCatalog::new())
}

/// Extracts human-readable plain text from core blocks and active, declared
/// `od:plugin_block` schemas (`text`, `textarea`, stripped `richtext`, and repeaters).
pub fn extract_visible_text_with_schemas(
    raw_body: &str,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> String {
    if !contains_blocks(raw_body) {
        return raw_body.to_owned();
    }
    let parsed = parse_blocks_lenient_or_recover(raw_body);
    let mut parts = Vec::new();
    for section in parsed.document.sections {
        for row in section.children {
            for col in row.children {
                for el in col.children {
                    if let ElementContent::Known(known) = el.content {
                        match known {
                            KnownElement::Heading(props) if !props.text.trim().is_empty() => {
                                parts.push(props.text);
                            }
                            KnownElement::Text(props) if !props.text.trim().is_empty() => {
                                parts.push(props.text);
                            }
                            KnownElement::Button(props) if !props.label.trim().is_empty() => {
                                parts.push(props.label);
                            }
                            KnownElement::PluginBlock(props) => {
                                if let Some(entry) = schemas.get(&props.block_id)
                                    && entry.status == ProviderStatus::Active
                                    && entry.declaration_state == DeclarationState::Declared
                                {
                                    entry
                                        .descriptor
                                        .schema
                                        .extract_visible_text(&props.values, &mut parts);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    parts.join("\n\n")
}
