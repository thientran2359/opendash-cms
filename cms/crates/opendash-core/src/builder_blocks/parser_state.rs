use super::*;

pub(super) struct ParserState<'a> {
    pub(super) strict: bool,
    pub(super) catalog: &'a RegistryCatalog<RegisteredBlockSchema>,
    pub(super) roots: Vec<BlockNode>,
    pub(super) stack: Vec<BlockNode>,
    pub(super) pending_hoists: Vec<Vec<BlockNode>>,
    pub(super) diagnostics: Vec<BlockDiagnostic>,
    pub(super) had_freeform_wrap: bool,
    pub(super) had_recovery: bool,
    pub(super) wrap_seq: usize,
    pub(super) rec_seq: usize,
    pub(super) used_ids: HashSet<String>,
}

impl<'a> ParserState<'a> {
    pub(super) fn new(strict: bool, catalog: &'a RegistryCatalog<RegisteredBlockSchema>) -> Self {
        Self {
            strict,
            catalog,
            roots: Vec::new(),
            stack: Vec::new(),
            pending_hoists: Vec::new(),
            diagnostics: Vec::new(),
            had_freeform_wrap: false,
            had_recovery: false,
            wrap_seq: 0,
            rec_seq: 0,
            used_ids: HashSet::new(),
        }
    }

    pub(super) fn push_container(&mut self, node: BlockNode) {
        self.stack.push(node);
        self.pending_hoists.push(Vec::new());
    }

    fn pop_and_attach(&mut self) -> Option<String> {
        let node = self.stack.pop()?;
        let hoists = self.pending_hoists.pop().unwrap_or_default();
        let id = node.id.clone();
        self.attach_leaf(node);
        if !hoists.is_empty() {
            if let Some(parent) = self.stack.last_mut() {
                parent.children.extend(hoists);
            } else {
                self.roots.extend(hoists);
            }
        }
        Some(id)
    }

    pub(super) fn unique_prefixed(&mut self, base: &str) -> String {
        if valid_package_id(base) && self.used_ids.insert(base.to_owned()) {
            return base.to_owned();
        }
        let mut suffix = 2_usize;
        loop {
            let candidate = format!("{base}-{suffix}");
            if valid_package_id(&candidate) && self.used_ids.insert(candidate.clone()) {
                return candidate;
            }
            suffix += 1;
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

    fn container_accepts_text(&self, block_id: &str) -> bool {
        if self.catalog.is_empty() {
            return true;
        }
        let Some(entry) = self.catalog.get(block_id) else {
            return false;
        };
        entry.status == ProviderStatus::Active
            && entry.declaration_state == DeclarationState::Declared
            && entry.descriptor.schema.supports_children
            && matches_pattern_list(
                entry.descriptor.schema.allowed_children.as_deref(),
                "core/text",
            )
    }

    pub(super) fn attach_leaf(&mut self, node: BlockNode) {
        if let Some(parent) = self.stack.last_mut() {
            parent.children.push(node);
        } else {
            self.roots.push(node);
        }
    }

    pub(super) fn attach_text_with_hoist(&mut self, text_nodes: Vec<BlockNode>) {
        if self.stack.is_empty() {
            self.roots.extend(text_nodes);
            return;
        }
        let top_idx = self.stack.len() - 1;
        if let Some(target_idx) = self
            .stack
            .iter()
            .rposition(|c| self.container_accepts_text(&c.block_id))
        {
            if target_idx < top_idx {
                if let Some(first) = text_nodes.first() {
                    self.diagnostics.push(BlockDiagnostic {
                        node_id: first.id.clone(),
                        code: "hoisted_freeform_html".to_owned(),
                        message: "Freeform HTML inside restricted container was hoisted to nearest valid ancestor.".to_owned(),
                    });
                }
                self.pending_hoists[target_idx + 1].extend(text_nodes);
            } else {
                self.stack[target_idx].children.extend(text_nodes);
            }
        } else {
            if let Some(first) = text_nodes.first() {
                self.diagnostics.push(BlockDiagnostic {
                    node_id: first.id.clone(),
                    code: "hoisted_freeform_html".to_owned(),
                    message:
                        "Freeform HTML inside restricted container was hoisted to document root."
                            .to_owned(),
                });
            }
            self.pending_hoists[0].extend(text_nodes);
        }
    }

    fn make_text_node(id: String, content: String) -> BlockNode {
        BlockNode {
            id,
            block_id: "core/text".to_owned(),
            props: BTreeMap::from([("content".to_owned(), Value::String(content))]),
            meta: None,
            children: Vec::new(),
        }
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
        self.wrap_seq += 1;
        let wrap_idx = self.wrap_seq;
        let mut nodes = Vec::with_capacity(chunks.len());
        for (i, chunk) in chunks.into_iter().enumerate() {
            let id = self.unique_prefixed(&format!("txt-wrap-{wrap_idx}-{}", i + 1));
            nodes.push(Self::make_text_node(id, chunk));
        }
        self.attach_text_with_hoist(nodes);
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
        let mut nodes = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            self.rec_seq += 1;
            let id = self.unique_prefixed(&format!("txt-rec-{}", self.rec_seq));
            if first_id.is_none() {
                first_id = Some(id.clone());
            }
            nodes.push(Self::make_text_node(id, chunk));
        }
        let diag_id = first_id.unwrap_or_else(|| "txt-rec-0".to_owned());
        self.record_recovery(&diag_id, reason);
        self.attach_text_with_hoist(nodes);
    }

    pub(super) fn close_container(&mut self, tag: &str) -> Result<bool, BuilderBlockError> {
        if self.strict {
            let Some(top) = self.stack.last() else {
                return Err(BuilderBlockError::Invalid("Unexpected closing block tag"));
            };
            if top.block_id != tag {
                return Err(BuilderBlockError::Invalid("Mismatched closing block tag"));
            }
            self.pop_and_attach();
            return Ok(true);
        }
        let Some(found_idx) = self.stack.iter().rposition(|n| n.block_id == tag) else {
            return Ok(false);
        };
        while self.stack.len() - 1 > found_idx {
            if let Some(unclosed_id) = self.pop_and_attach() {
                self.record_recovery(&unclosed_id, "Auto-closed unclosed inner container block");
            }
        }
        self.pop_and_attach();
        Ok(true)
    }

    pub(super) fn finish(mut self) -> Result<ParsedBlockDocument, BuilderBlockError> {
        if !self.stack.is_empty() {
            if self.strict {
                return Err(BuilderBlockError::Invalid(
                    "Unclosed Builder container block",
                ));
            }
            while let Some(id) = self.pop_and_attach() {
                self.record_recovery(&id, "Auto-closed unclosed Builder container at EOF");
            }
        }
        if self.had_freeform_wrap || self.had_recovery {
            let mut seen = HashSet::new();
            let mut seq = 0_usize;
            dedup_ids_rec(&mut self.roots, &mut seen, &mut seq);
        }
        Ok(ParsedBlockDocument {
            document: BuilderDocument {
                schema_version: BUILDER_SCHEMA_VERSION,
                blocks: self.roots,
            },
            diagnostics: self.diagnostics,
            had_freeform_wrap: self.had_freeform_wrap,
            had_recovery: self.had_recovery,
        })
    }
}

fn dedup_ids_rec(nodes: &mut [BlockNode], seen: &mut HashSet<String>, seq: &mut usize) {
    for node in nodes {
        if !seen.insert(node.id.clone()) {
            loop {
                *seq += 1;
                let candidate = format!("blk-wrap-u{}", *seq);
                if seen.insert(candidate.clone()) {
                    node.id = candidate;
                    break;
                }
            }
        }
        dedup_ids_rec(&mut node.children, seen, seq);
    }
}

pub(super) fn split_tag_and_json(inner: &str) -> Result<(&str, &str), &'static str> {
    let trimmed = inner.trim();
    match trimmed.find(char::is_whitespace) {
        Some(idx) => Ok((&trimmed[..idx], trimmed[idx..].trim())),
        None => Err("Missing JSON payload in comment block"),
    }
}
