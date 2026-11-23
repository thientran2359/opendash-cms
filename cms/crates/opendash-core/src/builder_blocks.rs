//! Pure-Rust Gutenberg-style Comment Block (`<!-- od:... -->`) codec for unified
//! entry `values_json["body"]` storage.
//!
//! Contains zero I/O, HTTP, or SQL. Matches `crate::builder::BuilderDocument`
//! 1:1 while preserving `#[serde(deny_unknown_fields)]` fidelity on all node
//! payloads.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
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
const MAX_TEXT_NODE_CHARS: usize = 16_384;
const OPEN_PREFIX: &str = "<!-- od:";
const CLOSE_PREFIX: &str = "<!-- /od:";
const COMMENT_END: &str = "-->";
const TEXT_CLOSE_TAG: &str = "<!-- /od:text -->";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuilderBlockError {
    Invalid(&'static str),
    TooLarge,
    Json(String),
}

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
    pub any_media_ids: BTreeSet<String>,
    pub pattern_ids: BTreeSet<i64>,
}

pub fn contains_blocks(raw_body: &str) -> bool {
    raw_body.contains(OPEN_PREFIX)
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

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Serialize)]
struct ContainerMetaRef<'a> {
    id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    responsive: Option<&'a Responsive>,
    #[serde(skip_serializing_if = "Option::is_none")]
    style: Option<&'a SectionStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<&'a NodeMeta>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContainerMeta {
    id: String,
    #[serde(default)]
    responsive: Option<Responsive>,
    #[serde(default)]
    style: Option<SectionStyle>,
    #[serde(default)]
    meta: Option<NodeMeta>,
}

#[derive(Serialize)]
struct PairedTextMetaRef<'a> {
    id: &'a str,
    #[serde(default, skip_serializing_if = "is_false")]
    rich: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<&'a NodeMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    binding: Option<&'a ElementBinding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PairedTextMeta {
    id: String,
    #[serde(default)]
    rich: bool,
    #[serde(default)]
    meta: Option<NodeMeta>,
    #[serde(default)]
    binding: Option<ElementBinding>,
}

#[derive(Serialize)]
struct ElementWithIdRef<'a, P: Serialize> {
    id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<&'a NodeMeta>,
    #[serde(flatten)]
    props: &'a P,
}

#[derive(Serialize)]
struct UnresolvedRef<'a> {
    id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<&'a NodeMeta>,
    raw: &'a BTreeMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnresolvedPayload {
    id: String,
    #[serde(default)]
    meta: Option<NodeMeta>,
    raw: BTreeMap<String, Value>,
}

fn push_line(out: &mut String, line: &str) {
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(line);
}

fn serialize_self_closing<P: Serialize>(
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

fn text_has_comment_boundary(text: &str) -> bool {
    text.contains("<!--") || text.contains("-->") || text.contains("--!>")
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

fn normalize_newlines(raw: &str) -> String {
    if !raw.contains('\r') {
        return raw.to_owned();
    }
    raw.replace("\r\n", "\n").replace('\r', "\n")
}

fn sanitize_text_for_builder(raw: &str) -> String {
    raw.chars()
        .filter(|&ch| ch != '\0' && (!ch.is_control() || ch == '\n' || ch == '\t'))
        .collect()
}

fn chunk_text_16k(text: &str) -> Vec<String> {
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

fn decode_props_with_id<P: serde::de::DeserializeOwned>(
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

fn find_next_block_marker(haystack: &str, from: usize) -> Option<(usize, bool)> {
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

struct ParserState {
    strict: bool,
    sections: Vec<SectionNode>,
    cur_section: Option<SectionNode>,
    cur_row: Option<RowNode>,
    cur_col: Option<ColumnNode>,
    diagnostics: Vec<BlockDiagnostic>,
    had_freeform_wrap: bool,
    had_recovery: bool,
    wrap_seq: usize,
    rec_seq: usize,
    used_ids: HashSet<String>,
}

impl ParserState {
    fn new(strict: bool) -> Self {
        Self {
            strict,
            sections: Vec::new(),
            cur_section: None,
            cur_row: None,
            cur_col: None,
            diagnostics: Vec::new(),
            had_freeform_wrap: false,
            had_recovery: false,
            wrap_seq: 0,
            rec_seq: 0,
            used_ids: HashSet::new(),
        }
    }

    fn alloc_wrap_ids(&mut self) -> (String, String, String, usize) {
        self.wrap_seq += 1;
        let n = self.wrap_seq;
        let sec = self.unique_prefixed(&format!("sec-wrap-{n}"));
        let row = self.unique_prefixed(&format!("row-wrap-{n}"));
        let col = self.unique_prefixed(&format!("col-wrap-{n}"));
        (sec, row, col, n)
    }

    fn unique_prefixed(&mut self, base: &str) -> String {
        if valid_package_id(base) && self.used_ids.insert(base.to_owned()) {
            return base.to_owned();
        }
        let mut suffix = 2usize;
        loop {
            let candidate = format!("{base}-{suffix}");
            if valid_package_id(&candidate) && self.used_ids.insert(candidate.clone()) {
                return candidate;
            }
            suffix += 1;
        }
    }

    fn close_column(&mut self) {
        if let Some(col) = self.cur_col.take() {
            if let Some(row) = self.cur_row.as_mut() {
                row.children.push(col);
            } else {
                self.rec_seq += 1;
                let row_id = self.unique_prefixed(&format!("row-rec-{}", self.rec_seq));
                let row = RowNode {
                    id: row_id,
                    meta: None,
                    responsive: None,
                    children: vec![col],
                };
                if let Some(sec) = self.cur_section.as_mut() {
                    sec.children.push(row);
                } else {
                    self.rec_seq += 1;
                    let sec_id = self.unique_prefixed(&format!("sec-rec-{}", self.rec_seq));
                    self.sections.push(SectionNode {
                        id: sec_id,
                        meta: None,
                        responsive: None,
                        style: None,
                        children: vec![row],
                    });
                }
            }
        }
    }

    fn close_row(&mut self) {
        self.close_column();
        if let Some(row) = self.cur_row.take() {
            if let Some(sec) = self.cur_section.as_mut() {
                sec.children.push(row);
            } else {
                self.rec_seq += 1;
                let sec_id = self.unique_prefixed(&format!("sec-rec-{}", self.rec_seq));
                self.sections.push(SectionNode {
                    id: sec_id,
                    meta: None,
                    responsive: None,
                    style: None,
                    children: vec![row],
                });
            }
        }
    }

    fn close_section(&mut self) {
        self.close_row();
        if let Some(sec) = self.cur_section.take() {
            self.sections.push(sec);
        }
    }

    fn record_recovery(&mut self, node_id: &str, message: impl Into<String>) {
        self.had_recovery = true;
        self.diagnostics.push(BlockDiagnostic {
            node_id: node_id.to_owned(),
            code: "malformed_comment_recovered".to_owned(),
            message: message.into(),
        });
    }

    fn push_element(&mut self, element: ElementNode) -> Result<(), BuilderBlockError> {
        if self.cur_col.is_some() {
            self.cur_col.as_mut().unwrap().children.push(element);
            return Ok(());
        }
        if self.strict {
            return Err(BuilderBlockError::Invalid(
                "Builder element found outside open column",
            ));
        }
        self.ensure_open_column_for_recovery(&element.id.clone(), "Element found outside column");
        self.cur_col.as_mut().unwrap().children.push(element);
        Ok(())
    }

    fn ensure_open_column_for_recovery(&mut self, diag_node_id: &str, reason: &str) {
        if self.cur_section.is_none() {
            self.rec_seq += 1;
            let n = self.rec_seq;
            let id = self.unique_prefixed(&format!("sec-rec-{n}"));
            self.cur_section = Some(SectionNode {
                id,
                meta: None,
                responsive: None,
                style: None,
                children: Vec::new(),
            });
        }
        if self.cur_row.is_none() {
            self.rec_seq += 1;
            let n = self.rec_seq;
            let id = self.unique_prefixed(&format!("row-rec-{n}"));
            self.cur_row = Some(RowNode {
                id,
                meta: None,
                responsive: None,
                children: Vec::new(),
            });
        }
        if self.cur_col.is_none() {
            self.rec_seq += 1;
            let n = self.rec_seq;
            let id = self.unique_prefixed(&format!("col-rec-{n}"));
            self.cur_col = Some(ColumnNode {
                id,
                meta: None,
                responsive: None,
                children: Vec::new(),
            });
        }
        self.record_recovery(diag_node_id, reason);
    }

    fn push_freeform(&mut self, raw: &str, trim_outer_newlines: bool) {
        let candidate = if trim_outer_newlines {
            raw.trim_matches('\n')
        } else {
            raw
        };
        if candidate.trim().is_empty() {
            return;
        }
        let chunks = chunk_text_16k(candidate);
        if chunks.is_empty() {
            return;
        }
        self.had_freeform_wrap = true;
        if self.cur_col.is_some() {
            self.wrap_seq += 1;
            let wrap_idx = self.wrap_seq;
            let mut elements = Vec::with_capacity(chunks.len());
            for (i, chunk) in chunks.into_iter().enumerate() {
                let id = self.unique_prefixed(&format!("el-wrap-{wrap_idx}-{}", i + 1));
                elements.push(ElementNode {
                    id,
                    meta: None,
                    content: ElementContent::Known(KnownElement::Text(TextProps {
                        text: chunk,
                        ..Default::default()
                    })),
                });
            }
            self.cur_col.as_mut().unwrap().children.extend(elements);
            return;
        }
        let (sec_id, row_id, col_id, wrap_idx) = self.alloc_wrap_ids();
        let mut elements = Vec::with_capacity(chunks.len());
        for (i, chunk) in chunks.into_iter().enumerate() {
            let id = self.unique_prefixed(&format!("el-wrap-{wrap_idx}-{}", i + 1));
            elements.push(ElementNode {
                id,
                meta: None,
                content: ElementContent::Known(KnownElement::Text(TextProps {
                    text: chunk,
                    ..Default::default()
                })),
            });
        }
        let col = ColumnNode {
            id: col_id,
            meta: None,
            responsive: None,
            children: elements,
        };
        if let Some(row) = self.cur_row.as_mut() {
            row.children.push(col);
            return;
        }
        let row = RowNode {
            id: row_id,
            meta: None,
            responsive: None,
            children: vec![col],
        };
        if let Some(sec) = self.cur_section.as_mut() {
            sec.children.push(row);
            return;
        }
        self.sections.push(SectionNode {
            id: sec_id,
            meta: None,
            responsive: None,
            style: None,
            children: vec![row],
        });
    }

    fn push_recovered_region(&mut self, raw: &str, reason: &str) {
        let chunks = chunk_text_16k(raw);
        let chunks = if chunks.is_empty() {
            vec![raw.trim().to_owned()]
        } else {
            chunks
        };
        if chunks.iter().all(|c| c.is_empty()) {
            return;
        }
        let mut first_id = None;
        let mut elements = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            self.rec_seq += 1;
            let id = self.unique_prefixed(&format!("el-rec-{}", self.rec_seq));
            if first_id.is_none() {
                first_id = Some(id.clone());
            }
            elements.push(ElementNode {
                id,
                meta: None,
                content: ElementContent::Known(KnownElement::Text(TextProps {
                    text: chunk,
                    ..Default::default()
                })),
            });
        }
        let diag_id = first_id.unwrap_or_else(|| "el-rec-0".to_owned());
        self.record_recovery(&diag_id, reason);

        if let Some(col) = self.cur_col.as_mut() {
            col.children.extend(elements);
            return;
        }
        self.rec_seq += 1;
        let n = self.rec_seq;
        let col = ColumnNode {
            id: self.unique_prefixed(&format!("col-rec-{n}")),
            meta: None,
            responsive: None,
            children: elements,
        };
        if let Some(row) = self.cur_row.as_mut() {
            row.children.push(col);
            return;
        }
        let row = RowNode {
            id: self.unique_prefixed(&format!("row-rec-{n}")),
            meta: None,
            responsive: None,
            children: vec![col],
        };
        if let Some(sec) = self.cur_section.as_mut() {
            sec.children.push(row);
            return;
        }
        let sec_id = self.unique_prefixed(&format!("sec-rec-{n}"));
        self.sections.push(SectionNode {
            id: sec_id,
            meta: None,
            responsive: None,
            style: None,
            children: vec![row],
        });
    }

    fn finish(mut self) -> Result<ParsedBlockDocument, BuilderBlockError> {
        if self.cur_col.is_some() || self.cur_row.is_some() || self.cur_section.is_some() {
            if self.strict {
                return Err(BuilderBlockError::Invalid(
                    "Unclosed Builder container block",
                ));
            }
            let open_id = self
                .cur_col
                .as_ref()
                .map(|c| c.id.clone())
                .or_else(|| self.cur_row.as_ref().map(|r| r.id.clone()))
                .or_else(|| self.cur_section.as_ref().map(|s| s.id.clone()))
                .unwrap_or_else(|| "sec-rec-eof".to_owned());
            self.close_section();
            self.record_recovery(&open_id, "Auto-closed unclosed Builder container at EOF");
        }
        // Ensure no earlier synthetic wrapper/recovery ID collides with a later explicit ID.
        if self.had_freeform_wrap || self.had_recovery {
            let mut seen = HashSet::new();
            let mut dedup_seq = 0usize;
            for sec in &mut self.sections {
                if !seen.insert(sec.id.clone()) {
                    while {
                        dedup_seq += 1;
                        !seen.insert(format!("sec-wrap-u{dedup_seq}"))
                    } {}
                    sec.id = format!("sec-wrap-u{dedup_seq}");
                }
                for row in &mut sec.children {
                    if !seen.insert(row.id.clone()) {
                        while {
                            dedup_seq += 1;
                            !seen.insert(format!("row-wrap-u{dedup_seq}"))
                        } {}
                        row.id = format!("row-wrap-u{dedup_seq}");
                    }
                    for col in &mut row.children {
                        if !seen.insert(col.id.clone()) {
                            while {
                                dedup_seq += 1;
                                !seen.insert(format!("col-wrap-u{dedup_seq}"))
                            } {}
                            col.id = format!("col-wrap-u{dedup_seq}");
                        }
                        for el in &mut col.children {
                            if !seen.insert(el.id.clone()) {
                                while {
                                    dedup_seq += 1;
                                    !seen.insert(format!("el-wrap-u{dedup_seq}"))
                                } {}
                                el.id = format!("el-wrap-u{dedup_seq}");
                            }
                        }
                    }
                }
            }
        }
        Ok(ParsedBlockDocument {
            document: BuilderDocument {
                schema_version: BUILDER_SCHEMA_VERSION,
                sections: self.sections,
            },
            diagnostics: self.diagnostics,
            had_freeform_wrap: self.had_freeform_wrap,
            had_recovery: self.had_recovery,
        })
    }
}

fn parse_leaf_content(tag: &str, json_str: &str) -> Result<ElementNode, &'static str> {
    match tag {
        "heading" => {
            let (id, meta, props) = decode_props_with_id::<HeadingProps>(json_str)?;
            Ok(ElementNode {
                id,
                meta,
                content: ElementContent::Known(KnownElement::Heading(props)),
            })
        }
        "text" => {
            let (id, meta, props) = decode_props_with_id::<TextProps>(json_str)?;
            Ok(ElementNode {
                id,
                meta,
                content: ElementContent::Known(KnownElement::Text(props)),
            })
        }
        "image" => {
            let (id, meta, props) = decode_props_with_id::<ImageProps>(json_str)?;
            Ok(ElementNode {
                id,
                meta,
                content: ElementContent::Known(KnownElement::Image(props)),
            })
        }
        "video" => {
            let (id, meta, props) = decode_props_with_id::<VideoProps>(json_str)?;
            Ok(ElementNode {
                id,
                meta,
                content: ElementContent::Known(KnownElement::Video(props)),
            })
        }
        "button" => {
            let (id, meta, props) = decode_props_with_id::<ButtonProps>(json_str)?;
            Ok(ElementNode {
                id,
                meta,
                content: ElementContent::Known(KnownElement::Button(props)),
            })
        }
        "list" => {
            let (id, meta, props) = decode_props_with_id::<ListProps>(json_str)?;
            Ok(ElementNode {
                id,
                meta,
                content: ElementContent::Known(KnownElement::List(props)),
            })
        }
        "contact_form" => {
            let (id, meta, props) = decode_props_with_id::<ContactFormProps>(json_str)?;
            Ok(ElementNode {
                id,
                meta,
                content: ElementContent::Known(KnownElement::ContactForm(props)),
            })
        }
        "content_slot" => {
            let (id, meta, props) = decode_props_with_id::<ContentSlotProps>(json_str)?;
            Ok(ElementNode {
                id,
                meta,
                content: ElementContent::Known(KnownElement::ContentSlot(props)),
            })
        }
        "pattern_ref" => {
            let (id, meta, props) = decode_props_with_id::<PatternRefProps>(json_str)?;
            Ok(ElementNode {
                id,
                meta,
                content: ElementContent::Known(KnownElement::PatternRef(props)),
            })
        }
        "template_part" => {
            let (id, meta, props) = decode_props_with_id::<TemplatePartProps>(json_str)?;
            Ok(ElementNode {
                id,
                meta,
                content: ElementContent::Known(KnownElement::TemplatePart(props)),
            })
        }
        "plugin_block" => {
            let (id, meta, props) = decode_props_with_id::<PluginBlockProps>(json_str)?;
            Ok(ElementNode {
                id,
                meta,
                content: ElementContent::Known(KnownElement::PluginBlock(props)),
            })
        }
        "legacy" => {
            let (id, meta, props) = decode_props_with_id::<LegacyBlockProps>(json_str)?;
            Ok(ElementNode {
                id,
                meta,
                content: ElementContent::Known(KnownElement::Legacy(props)),
            })
        }
        "unresolved" => {
            let payload: UnresolvedPayload =
                serde_json::from_str(json_str).map_err(|_| "Invalid unresolved block JSON")?;
            Ok(ElementNode {
                id: payload.id,
                meta: payload.meta,
                content: ElementContent::Unresolved(payload.raw),
            })
        }
        _ => Err("Unknown OpenDash comment block tag"),
    }
}

fn split_tag_and_json(inner: &str) -> Result<(&str, &str), &'static str> {
    let trimmed = inner.trim();
    match trimmed.find(|c: char| c.is_whitespace()) {
        Some(idx) => Ok((&trimmed[..idx], trimmed[idx..].trim())),
        None => Err("Missing JSON payload in comment block"),
    }
}

fn parse_blocks_internal(
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
                                    && !entry.descriptor.legacy_mode
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

/// Scans `<!-- od:... -->` comments leniently to extract referenced media and
/// pattern IDs even when sibling comments are malformed.
pub fn extract_block_refs_lenient(raw_body: &str) -> ExtractedBlockRefs {
    extract_block_refs_lenient_with_schemas(raw_body, &RegistryCatalog::new())
}

/// Scans `<!-- od:... -->` comments leniently to extract referenced media and
/// pattern IDs, including `Media` fields from active, declared `od:plugin_block`
/// schemas (`ProviderInactive`/`Withdrawn`/`legacy_mode` entries emit zero media).
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
                        && !entry.descriptor.legacy_mode
                    {
                        let collected = entry.descriptor.schema.collect_media_ids(&props.values);
                        refs.image_ids.extend(collected.image_ids);
                        refs.video_ids.extend(collected.video_ids);
                        refs.any_media_ids.extend(collected.any_ids);
                    }
                }
                _ => {}
            }
        }
        cursor = comment_end;
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        builder::{BindingSource, ListSort, ResponsiveStyle, TextAlign},
        registry::ResourceOwner,
        theme::{BlockDescriptor, Fields},
    };
    use serde_json::json;

    fn el(id: &str, content: KnownElement) -> ElementNode {
        ElementNode {
            id: id.into(),
            meta: None,
            content: ElementContent::Known(content),
        }
    }

    #[test]
    fn roundtrip_all_known_elements_and_unresolved_without_div_wrappers() {
        let mut unresolved = BTreeMap::new();
        unresolved.insert("type".into(), json!("vendor/custom-widget"));
        unresolved.insert("props".into(), json!({"foo": "bar", "nested": [1, 2]}));

        let doc = BuilderDocument {
            schema_version: BUILDER_SCHEMA_VERSION,
            sections: vec![SectionNode {
                id: "sec-1".into(),
                meta: None,
                style: None,
                responsive: Some(Responsive {
                    desktop: Some(ResponsiveStyle {
                        padding_px: Some(24),
                        ..Default::default()
                    }),
                    tablet: None,
                    mobile: Some(ResponsiveStyle {
                        padding_px: Some(12),
                        ..Default::default()
                    }),
                }),
                children: vec![RowNode {
                    id: "row-1".into(),
                    meta: None,
                    responsive: Some(Responsive {
                        desktop: Some(ResponsiveStyle {
                            gap_px: Some(16),
                            columns_per_row: Some(2),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    children: vec![ColumnNode {
                        id: "col-1".into(),
                        meta: None,
                        responsive: Some(Responsive {
                            mobile: Some(ResponsiveStyle {
                                text_align: Some(TextAlign::Center),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        children: vec![
                            el(
                                "el-heading",
                                KnownElement::Heading(HeadingProps {
                                    level: 2,
                                    text: "Title with -- and --> and <script>".into(),
                                    binding: Some(ElementBinding {
                                        type_key: "post".into(),
                                        entry_slug: "hello".into(),
                                        source: BindingSource::Title,
                                        target: BindingTarget::Text,
                                    }),
                                }),
                            ),
                            el(
                                "el-text",
                                KnownElement::Text(TextProps {
                                    text: "  Line 1\nLine 2  ".into(),
                                    ..Default::default()
                                }),
                            ),
                            el(
                                "el-image",
                                KnownElement::Image(ImageProps {
                                    media_id: "a".repeat(64),
                                    alt: "Alt".into(),
                                    binding: None,
                                }),
                            ),
                            el(
                                "el-video",
                                KnownElement::Video(VideoProps {
                                    media_id: "b".repeat(64),
                                    poster_image_id: Some("c".repeat(64)),
                                    controls: true,
                                    muted: true,
                                }),
                            ),
                            el(
                                "el-button",
                                KnownElement::Button(ButtonProps {
                                    label: "Click".into(),
                                    url: "/contact".into(),
                                    ..Default::default()
                                }),
                            ),
                            el(
                                "el-list",
                                KnownElement::List(ListProps {
                                    type_key: "post".into(),
                                    category_id: None,
                                    tag_id: None,
                                    taxonomy_key: Some("topic".into()),
                                    term_id: Some(7),
                                    include_descendants: true,
                                    sort: ListSort::IdAsc,
                                    page_size: 4,
                                }),
                            ),
                            el(
                                "el-form",
                                KnownElement::ContactForm(ContactFormProps {
                                    form_id: 3,
                                    submit_label: "Send".into(),
                                }),
                            ),
                            el("el-slot", KnownElement::ContentSlot(ContentSlotProps {})),
                            el(
                                "el-pattern",
                                KnownElement::PatternRef(PatternRefProps { pattern_id: 9 }),
                            ),
                            el(
                                "el-part",
                                KnownElement::TemplatePart(TemplatePartProps {
                                    part_key: "header".into(),
                                }),
                            ),
                            el(
                                "el-plugin",
                                KnownElement::PluginBlock(PluginBlockProps {
                                    block_id: "demo-plugin/pricing".into(),
                                    values: BTreeMap::from([("tier".into(), json!("pro"))]),
                                }),
                            ),
                            el(
                                "el-legacy",
                                KnownElement::Legacy(LegacyBlockProps {
                                    block_id: "ocean/hero".into(),
                                    values: Fields::from([("title".into(), "Ocean".into())]),
                                }),
                            ),
                            ElementNode {
                                id: "el-unresolved".into(),
                                meta: None,
                                content: ElementContent::Unresolved(unresolved),
                            },
                        ],
                    }],
                }],
            }],
        };

        let blocks = serialize_builder_document_to_blocks(&doc).unwrap();
        assert!(!blocks.contains("<div"), "must not emit wrapper <div>s");
        assert!(blocks.contains("<!-- od:section "));
        assert!(blocks.contains("<!-- /od:section -->"));
        // Escaped `--` and `-->` inside heading JSON
        assert!(!blocks.contains("Title with --"));
        assert!(blocks.contains("\\u002d\\u002d"));
        assert!(blocks.contains("\\u003cscript\\u003e"));

        let parsed = parse_blocks_to_builder_document(&blocks).unwrap();
        assert!(!parsed.had_freeform_wrap);
        assert!(!parsed.had_recovery);
        assert!(parsed.diagnostics.is_empty());
        assert_eq!(parsed.document, doc);

        // Validate against RegistryCatalog (keeping unresolved preserved against `doc`)
        let mut catalog = crate::registry::RegistryCatalog::new();
        catalog
            .register(
                "ocean/hero",
                ResourceOwner::Theme("ocean".into()),
                BlockDescriptor {
                    id: "ocean/hero".into(),
                    fields: Fields::from([("title".into(), "Tiêu đề".into())]),
                },
            )
            .unwrap();
        catalog
            .register(
                "demo-plugin/pricing",
                ResourceOwner::Plugin("demo-plugin".into()),
                BlockDescriptor {
                    id: "demo-plugin/pricing".into(),
                    fields: Fields::new(),
                },
            )
            .unwrap();
        parsed
            .document
            .validate_for_save(&catalog, Some(&doc))
            .unwrap();
    }

    #[test]
    fn text_comment_boundary_injection_and_exact_whitespace_roundtrip() {
        let doc = BuilderDocument {
            schema_version: BUILDER_SCHEMA_VERSION,
            sections: vec![SectionNode {
                id: "sec-1".into(),
                meta: None,
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "row-1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "col-1".into(),
                        meta: None,
                        responsive: None,
                        children: vec![
                            el(
                                "el-empty",
                                KnownElement::Text(TextProps {
                                    text: String::new(),
                                    ..Default::default()
                                }),
                            ),
                            el(
                                "el-spaces",
                                KnownElement::Text(TextProps {
                                    text: "\n  leading and trailing \n".into(),
                                    ..Default::default()
                                }),
                            ),
                            el(
                                "el-comment-break",
                                KnownElement::Text(TextProps {
                                    text: "Try <!-- and --> and --!> and --->".into(),
                                    ..Default::default()
                                }),
                            ),
                        ],
                    }],
                }],
            }],
        };

        let blocks = serialize_builder_document_to_blocks(&doc).unwrap();
        // Self-closing form used only for `el-comment-break`
        assert!(blocks.contains("<!-- od:text {\"id\":\"el-comment-break\""));
        assert!(!blocks.contains("Try <!-- and -->"));
        let parsed = parse_blocks_to_builder_document(&blocks).unwrap();
        assert_eq!(parsed.document, doc);
    }

    #[test]
    fn freeform_over_16k_with_crlf_auto_wraps_into_valid_text_chunks() {
        let line = "Dòng văn bản tiếng Việt dài để kiểm tra chia đoạn.\r\n";
        let raw = line.repeat(500); // > 25,000 chars
        assert!(raw.chars().count() > 16_384);

        let parsed = parse_blocks_to_builder_document(&raw).unwrap();
        assert!(parsed.had_freeform_wrap);
        assert!(!parsed.had_recovery);
        assert_eq!(parsed.document.sections.len(), 1);
        let elements = &parsed.document.sections[0].children[0].children[0].children;
        assert!(elements.len() >= 2);
        let mut combined = String::new();
        for node in elements {
            let ElementContent::Known(KnownElement::Text(t)) = &node.content else {
                panic!("expected text node");
            };
            assert!(t.text.chars().count() <= 16_384);
            assert!(!t.text.contains('\r'));
            combined.push_str(&t.text);
        }
        assert_eq!(combined, raw.replace("\r\n", "\n"));
        parsed
            .document
            .validate_for_save(&crate::registry::RegistryCatalog::new(), None)
            .unwrap();
    }

    #[test]
    fn empty_or_whitespace_only_body_parses_to_empty_document() {
        for raw in ["", "   \n\t\r\n  "] {
            let parsed = parse_blocks_to_builder_document(raw).unwrap();
            assert_eq!(parsed.document, BuilderDocument::empty());
            assert!(!parsed.had_freeform_wrap);
            assert!(!parsed.had_recovery);
        }
    }

    #[test]
    fn malformed_comments_fail_strict_and_recover_losslessly_lenient() {
        let img_id = "d".repeat(64);
        let broken = format!(
            "<!-- od:section {{\"id\":\"sec-1\"}} -->\n\
             <!-- od:row {{\"id\":\"row-1\"}} -->\n\
             <!-- od:column {{\"id\":\"col-1\"}} -->\n\
             <!-- od:image {{\"id\":\"img-ok\",\"media_id\":\"{img_id}\",\"alt\":\"ok\"}} /-->\n\
             <!-- od:heading {{\"id\":\"bad-json\", broken -->\n\
             <!-- od:text {{\"id\":\"txt-unclosed\"}} -->\n\
             Important text preserved"
        );
        assert!(parse_blocks_to_builder_document(&broken).is_err());

        let recovered = parse_blocks_lenient_or_recover(&broken);
        assert!(recovered.had_recovery);
        assert!(!recovered.diagnostics.is_empty());
        assert_eq!(recovered.diagnostics[0].code, "malformed_comment_recovered");
        let visible = extract_visible_text(
            &serialize_builder_document_to_blocks(&recovered.document).unwrap(),
        );
        assert!(visible.contains("Important text preserved"));
        assert!(visible.contains("bad-json"));

        // Ref extractor must keep extracting valid refs alongside malformed siblings.
        let refs = extract_block_refs_lenient(&broken);
        assert!(refs.image_ids.contains(&img_id));
    }

    #[test]
    fn extract_visible_text_handles_plain_and_block_content() {
        assert_eq!(extract_visible_text("Plain text"), "Plain text");
        let doc = BuilderDocument {
            schema_version: BUILDER_SCHEMA_VERSION,
            sections: vec![SectionNode {
                id: "s1".into(),
                meta: None,
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "r1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "c1".into(),
                        meta: None,
                        responsive: None,
                        children: vec![
                            el(
                                "h1",
                                KnownElement::Heading(HeadingProps {
                                    level: 2,
                                    text: "Heading One".into(),
                                    binding: None,
                                }),
                            ),
                            el(
                                "t1",
                                KnownElement::Text(TextProps {
                                    text: "Paragraph One".into(),
                                    ..Default::default()
                                }),
                            ),
                            el(
                                "b1",
                                KnownElement::Button(ButtonProps {
                                    label: "Read More".into(),
                                    url: "/more".into(),
                                    ..Default::default()
                                }),
                            ),
                        ],
                    }],
                }],
            }],
        };
        let blocks = serialize_builder_document_to_blocks(&doc).unwrap();
        assert_eq!(
            extract_visible_text(&blocks),
            "Heading One\n\nParagraph One\n\nRead More"
        );
    }

    #[test]
    fn schema_aware_refs_and_visible_text_gate_on_active_declared_blocks() {
        use crate::{
            block_schema::BlockSchemaV1,
            registry::{OwnedDescriptor, ResourceOwner},
        };

        let img_hex = "a".repeat(64);
        let vid_hex = "b".repeat(64);
        let any_hex = "c".repeat(64);
        let schema: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/showcase",
            "title": "Showcase",
            "category": "media",
            "render": "render.html",
            "fields": [
                { "type": "text", "key": "title", "label": "Title" },
                { "type": "richtext", "key": "summary", "label": "Summary" },
                { "type": "media", "key": "poster", "label": "Poster", "media_kind": "image" },
                { "type": "media", "key": "clip", "label": "Clip", "media_kind": "video" },
                {
                    "type": "repeater",
                    "key": "slides",
                    "label": "Slides",
                    "fields": [
                        { "type": "text", "key": "caption", "label": "Caption" },
                        { "type": "media", "key": "asset", "label": "Asset", "media_kind": "any" }
                    ]
                }
            ]
        }))
        .unwrap();

        let doc = BuilderDocument {
            schema_version: BUILDER_SCHEMA_VERSION,
            sections: vec![SectionNode {
                id: "s1".into(),
                meta: None,
                responsive: None,
                style: None,
                children: vec![RowNode {
                    id: "r1".into(),
                    meta: None,
                    responsive: None,
                    children: vec![ColumnNode {
                        id: "c1".into(),
                        meta: None,
                        responsive: None,
                        children: vec![el(
                            "p1",
                            KnownElement::PluginBlock(PluginBlockProps {
                                block_id: "core/showcase".into(),
                                values: BTreeMap::from([
                                    ("title".into(), json!("Tiêu đề block")),
                                    ("summary".into(), json!("<p>Mô tả <b>hay</b></p>")),
                                    ("poster".into(), json!(img_hex)),
                                    ("clip".into(), json!(vid_hex)),
                                    (
                                        "slides".into(),
                                        json!([{ "caption": "Slide 1", "asset": any_hex }]),
                                    ),
                                ]),
                            }),
                        )],
                    }],
                }],
            }],
        };
        let body = serialize_builder_document_to_blocks(&doc).unwrap();

        let mut active_catalog = RegistryCatalog::new();
        active_catalog
            .register(
                "core/showcase",
                ResourceOwner::Core,
                RegisteredBlockSchema {
                    schema: schema.clone(),
                    legacy_mode: false,
                },
            )
            .unwrap();

        let refs = extract_block_refs_lenient_with_schemas(&body, &active_catalog);
        assert_eq!(refs.image_ids, BTreeSet::from([img_hex]));
        assert_eq!(refs.video_ids, BTreeSet::from([vid_hex]));
        assert_eq!(refs.any_media_ids, BTreeSet::from([any_hex]));
        assert_eq!(
            extract_visible_text_with_schemas(&body, &active_catalog),
            "Tiêu đề block\n\nMô tả hay\n\nSlide 1"
        );

        let mut inactive_catalog = RegistryCatalog::new();
        inactive_catalog.insert_loaded(
            "core/showcase",
            OwnedDescriptor {
                owner: ResourceOwner::Core,
                status: ProviderStatus::ProviderInactive,
                declaration_state: DeclarationState::Declared,
                descriptor: RegisteredBlockSchema {
                    schema: schema.clone(),
                    legacy_mode: false,
                },
            },
        );
        assert_eq!(
            extract_block_refs_lenient_with_schemas(&body, &inactive_catalog),
            ExtractedBlockRefs::default()
        );
        assert_eq!(
            extract_visible_text_with_schemas(&body, &inactive_catalog),
            ""
        );

        let mut withdrawn_catalog = RegistryCatalog::new();
        withdrawn_catalog.insert_loaded(
            "core/showcase",
            OwnedDescriptor {
                owner: ResourceOwner::Core,
                status: ProviderStatus::Active,
                declaration_state: DeclarationState::Withdrawn,
                descriptor: RegisteredBlockSchema {
                    schema,
                    legacy_mode: false,
                },
            },
        );
        assert_eq!(
            extract_block_refs_lenient_with_schemas(&body, &withdrawn_catalog),
            ExtractedBlockRefs::default()
        );
    }

    #[test]
    fn node_meta_and_synthetic_wrappers_roundtrip() {
        use crate::builder::{NodeMeta, NodeVisibility, SectionStyle, SectionWidth};

        let bg_media = "e".repeat(64);
        let doc = BuilderDocument {
            schema_version: BUILDER_SCHEMA_VERSION,
            sections: vec![
                SectionNode {
                    id: "sec-syn-1".into(),
                    meta: Some(NodeMeta {
                        synthetic: true,
                        ..Default::default()
                    }),
                    responsive: None,
                    style: None,
                    children: vec![RowNode {
                        id: "row-syn-1".into(),
                        meta: Some(NodeMeta {
                            synthetic: true,
                            ..Default::default()
                        }),
                        responsive: None,
                        children: vec![ColumnNode {
                            id: "col-syn-1".into(),
                            meta: Some(NodeMeta {
                                synthetic: true,
                                ..Default::default()
                            }),
                            responsive: None,
                            children: vec![ElementNode {
                                id: "el-head-1".into(),
                                meta: Some(NodeMeta {
                                    name: Some("Hero Heading".into()),
                                    html_id: Some("hero-title".into()),
                                    class_name: Some("text-4xl font-bold".into()),
                                    visibility: Some(NodeVisibility {
                                        hide_desktop: false,
                                        hide_tablet: false,
                                        hide_mobile: true,
                                    }),
                                    synthetic: false,
                                }),
                                content: ElementContent::Known(KnownElement::Heading(
                                    HeadingProps {
                                        level: 1,
                                        text: "Welcome".into(),
                                        binding: None,
                                    },
                                )),
                            }],
                        }],
                    }],
                },
                SectionNode {
                    id: "sec-styled-1".into(),
                    meta: None,
                    responsive: None,
                    style: Some(SectionStyle {
                        background_color: Some("#0f172a".into()),
                        background_media_id: Some(bg_media.clone()),
                        content_width: Some(SectionWidth::Wide),
                    }),
                    children: vec![RowNode {
                        id: "row-styled-1".into(),
                        meta: None,
                        responsive: None,
                        children: vec![ColumnNode {
                            id: "col-styled-1".into(),
                            meta: None,
                            responsive: None,
                            children: vec![
                                el(
                                    "el-rich-1",
                                    KnownElement::Text(TextProps {
                                        text: "<p><strong>Rich</strong> copy</p>".into(),
                                        rich: true,
                                        binding: None,
                                    }),
                                ),
                                el(
                                    "el-btn-1",
                                    KnownElement::Button(ButtonProps {
                                        label: "CTA".into(),
                                        url: "https://opendash.vn".into(),
                                        new_tab: true,
                                        nofollow: true,
                                        binding: None,
                                    }),
                                ),
                            ],
                        }],
                    }],
                },
            ],
        };

        let catalog = RegistryCatalog::new();
        doc.validate_for_save_with_schemas(&catalog, None)
            .expect("synthetic carrier + styled section + rich text must pass validation");
        let serialized = serialize_builder_document_to_blocks(&doc).unwrap();
        let reparsed = parse_blocks_to_builder_document(&serialized).unwrap();
        assert_eq!(reparsed.document, doc);
        let refs = extract_block_refs_lenient(&serialized);
        assert!(refs.image_ids.contains(&bg_media));
    }
}
