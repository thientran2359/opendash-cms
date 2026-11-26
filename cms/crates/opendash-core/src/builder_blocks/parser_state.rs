use super::*;

pub(super) struct ParserState {
    pub(super) strict: bool,
    pub(super) sections: Vec<SectionNode>,
    pub(super) cur_section: Option<SectionNode>,
    pub(super) cur_row: Option<RowNode>,
    pub(super) cur_col: Option<ColumnNode>,
    pub(super) diagnostics: Vec<BlockDiagnostic>,
    pub(super) had_freeform_wrap: bool,
    pub(super) had_recovery: bool,
    pub(super) wrap_seq: usize,
    pub(super) rec_seq: usize,
    pub(super) used_ids: HashSet<String>,
}

impl ParserState {
    pub(super) fn new(strict: bool) -> Self {
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

    pub(super) fn alloc_wrap_ids(&mut self) -> (String, String, String, usize) {
        self.wrap_seq += 1;
        let n = self.wrap_seq;
        let sec = self.unique_prefixed(&format!("sec-wrap-{n}"));
        let row = self.unique_prefixed(&format!("row-wrap-{n}"));
        let col = self.unique_prefixed(&format!("col-wrap-{n}"));
        (sec, row, col, n)
    }

    pub(super) fn unique_prefixed(&mut self, base: &str) -> String {
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

    pub(super) fn close_column(&mut self) {
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

    pub(super) fn close_row(&mut self) {
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

    pub(super) fn close_section(&mut self) {
        self.close_row();
        if let Some(sec) = self.cur_section.take() {
            self.sections.push(sec);
        }
    }

    pub(super) fn record_recovery(&mut self, node_id: &str, message: impl Into<String>) {
        self.had_recovery = true;
        self.diagnostics.push(BlockDiagnostic {
            node_id: node_id.to_owned(),
            code: "malformed_comment_recovered".to_owned(),
            message: message.into(),
        });
    }

    pub(super) fn push_element(&mut self, element: ElementNode) -> Result<(), BuilderBlockError> {
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

    pub(super) fn ensure_open_column_for_recovery(&mut self, diag_node_id: &str, reason: &str) {
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

    pub(super) fn push_freeform(&mut self, raw: &str, trim_outer_newlines: bool) {
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

    pub(super) fn push_recovered_region(&mut self, raw: &str, reason: &str) {
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

    pub(super) fn finish(mut self) -> Result<ParsedBlockDocument, BuilderBlockError> {
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

pub(super) fn parse_leaf_content(tag: &str, json_str: &str) -> Result<ElementNode, &'static str> {
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

pub(super) fn split_tag_and_json(inner: &str) -> Result<(&str, &str), &'static str> {
    let trimmed = inner.trim();
    match trimmed.find(|c: char| c.is_whitespace()) {
        Some(idx) => Ok((&trimmed[..idx], trimmed[idx..].trim())),
        None => Err("Missing JSON payload in comment block"),
    }
}
