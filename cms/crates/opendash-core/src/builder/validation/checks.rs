use super::*;
use crate::block_schema::BlockSchema;

struct PrevNodeInfo<'a> {
    node: &'a BlockNode,
    parent_id: Option<&'a str>,
    subtree_ids: HashSet<&'a str>,
}

fn index_previous_nodes_rec<'a>(
    nodes: &'a [BlockNode],
    parent_id: Option<&'a str>,
    out: &mut HashMap<&'a str, PrevNodeInfo<'a>>,
) -> HashSet<&'a str> {
    let mut level_subtree = HashSet::new();
    for node in nodes {
        let mut child_ids = index_previous_nodes_rec(&node.children, Some(node.id.as_str()), out);
        level_subtree.insert(node.id.as_str());
        level_subtree.extend(child_ids.iter().copied());
        child_ids.insert(node.id.as_str());
        out.insert(
            node.id.as_str(),
            PrevNodeInfo {
                node,
                parent_id,
                subtree_ids: child_ids,
            },
        );
    }
    level_subtree
}

fn validate_list_cross_fields(props: &BTreeMap<String, Value>) -> Result<(), BuilderError> {
    let type_ok = props
        .get("type_key")
        .and_then(Value::as_str)
        .is_some_and(valid_content_type_key);
    if !type_ok {
        return Err(BuilderError::Invalid("Invalid Builder list type_key"));
    }
    if let Some(tax) = props.get("taxonomy_key").filter(|v| !v.is_null()) {
        let Some(tax_key) = tax.as_str() else {
            return Err(BuilderError::Invalid("Invalid Builder list taxonomy_key"));
        };
        if !tax_key.is_empty() && !valid_content_type_key(tax_key) {
            return Err(BuilderError::Invalid("Invalid Builder list taxonomy_key"));
        }
    }
    let has_tax = props
        .get("taxonomy_key")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty());
    let has_term = props.get("term_id").is_some_and(|v| !v.is_null());
    if has_tax != has_term || (has_term && parse_positive_i64_prop(props.get("term_id")).is_none())
    {
        return Err(BuilderError::Invalid("Invalid Builder list term_id filter"));
    }
    if props.get("include_descendants").and_then(Value::as_bool) == Some(true)
        && (!has_tax || parse_positive_i64_prop(props.get("term_id")).is_none())
    {
        return Err(BuilderError::Invalid(
            "include_descendants requires taxonomy_key and term_id",
        ));
    }
    Ok(())
}

struct ValidationWalk<'a> {
    catalog: &'a RegistryCatalog<RegisteredBlockSchema>,
    ctx: &'a BuilderContextTarget,
    prev_index: &'a HashMap<&'a str, PrevNodeInfo<'a>>,
    seen_ids: HashSet<&'a str>,
    node_count: usize,
    content_slots: usize,
    lists: usize,
    pattern_refs: usize,
}

#[derive(Default)]
struct LoopCounters {
    nodes: usize,
    third_party: usize,
}

impl<'a> ValidationWalk<'a> {
    fn visit_nodes(
        &mut self,
        nodes: &'a [BlockNode],
        depth: usize,
        parent_id: Option<&'a str>,
        parent_schema: Option<&'a BlockSchema>,
        inactive_ancestor_subtree: Option<&'a HashSet<&'a str>>,
        mut loop_scope: Option<(&mut LoopCounters, usize)>,
    ) -> Result<(), BuilderError> {
        if nodes.is_empty() {
            return Ok(());
        }
        if depth > MAX_BUILDER_DEPTH {
            return Err(BuilderError::Invalid("Builder exceeds max depth 8"));
        }
        for node in nodes {
            self.node_count += 1;
            if self.node_count > MAX_BUILDER_NODES {
                return Err(BuilderError::Invalid("Builder exceeds 512 nodes"));
            }
            if !valid_package_id(&node.id) {
                return Err(BuilderError::Invalid("Invalid Builder node ID"));
            }
            if !self.seen_ids.insert(node.id.as_str()) {
                return Err(BuilderError::Invalid("Duplicate Builder node ID"));
            }
            if !valid_namespaced_block_id(&node.block_id) {
                return Err(BuilderError::Invalid("Invalid Builder block_id"));
            }
            validate_node_meta(node.meta.as_ref())?;
            if (node.block_id == "core/loop-item" || loop_scope.is_some())
                && node
                    .meta
                    .as_ref()
                    .and_then(|m| m.html_id.as_deref())
                    .is_some_and(|s| !s.is_empty())
            {
                return Err(BuilderError::Invalid("Loop item nodes cannot declare html_id"));
            }

            if let Some((lc, rel_depth)) = loop_scope.as_mut().map(|(lc, rd)| (&mut **lc, *rd)) {
                if rel_depth > MAX_LOOP_ITEM_REL_DEPTH {
                    return Err(BuilderError::Invalid(
                        "Loop item exceeds max relative depth 4",
                    ));
                }
                lc.nodes += 1;
                if lc.nodes > MAX_LOOP_ITEM_NODES {
                    return Err(BuilderError::Invalid(
                        "Loop item exceeds max 16 descendant nodes",
                    ));
                }
            }

            match node.block_id.as_str() {
                "core/entry-content" => {
                    self.content_slots += 1;
                    if self.content_slots > 1 {
                        return Err(BuilderError::Invalid(
                            "Builder allows at most one core/entry-content",
                        ));
                    }
                }
                "core/list" => {
                    self.lists += 1;
                    if self.lists > MAX_BUILDER_LISTS_PER_REQUEST {
                        return Err(BuilderError::Invalid("Builder exceeds 8 list blocks"));
                    }
                }
                "core/pattern-ref" => {
                    self.pattern_refs += 1;
                    if self.pattern_refs > MAX_BUILDER_PATTERN_INSTANCES {
                        return Err(BuilderError::Invalid(
                            "Builder exceeds 16 pattern-ref instances",
                        ));
                    }
                }
                _ => {}
            }

            if let Some(allowed_subtree) = inactive_ancestor_subtree {
                let prev = self
                    .prev_index
                    .get(node.id.as_str())
                    .ok_or(BuilderError::Invalid(
                        "Cannot insert node inside inactive container",
                    ))?;
                if !allowed_subtree.contains(node.id.as_str())
                    || prev.node.block_id != node.block_id
                    || prev.node.props != node.props
                {
                    return Err(BuilderError::Invalid(
                        "Cannot insert or mutate node inside inactive container",
                    ));
                }
                let next_loop = loop_scope.as_mut().map(|(lc, rd)| (&mut **lc, *rd + 1));
                self.visit_nodes(
                    &node.children,
                    depth + 1,
                    Some(node.id.as_str()),
                    None,
                    Some(allowed_subtree),
                    next_loop,
                )?;
                continue;
            }

            let active_schema = self
                .catalog
                .get(&node.block_id)
                .filter(|entry| {
                    entry.status == ProviderStatus::Active
                        && entry.declaration_state == DeclarationState::Declared
                })
                .map(|entry| &entry.descriptor.schema);

            let Some(schema) = active_schema else {
                let prev = self
                    .prev_index
                    .get(node.id.as_str())
                    .ok_or(BuilderError::Invalid(
                        "Block descriptor is unavailable or inactive",
                    ))?;
                if prev.node.block_id != node.block_id || prev.node.props != node.props {
                    return Err(BuilderError::Invalid(
                        "Cannot mutate props of inactive block",
                    ));
                }
                let same_parent = prev.parent_id == parent_id;
                if !same_parent
                    && let Some(ps) = parent_schema
                    && !(matches_pattern_list(ps.allowed_children.as_deref(), "*")
                        && matches_pattern_list(ps.allowed_children.as_deref(), &node.block_id))
                {
                    return Err(BuilderError::Invalid(
                        "Target parent does not allow moving inactive block",
                    ));
                }
                let next_loop = loop_scope.as_mut().map(|(lc, rd)| (&mut **lc, *rd + 1));
                self.visit_nodes(
                    &node.children,
                    depth + 1,
                    Some(node.id.as_str()),
                    None,
                    Some(&prev.subtree_ids),
                    next_loop,
                )?;
                continue;
            };

            let effective_ctx = if loop_scope.is_some() {
                &BuilderContextTarget::LoopItem
            } else {
                self.ctx
            };
            if let Some((lc, _)) = loop_scope.as_mut().map(|(lc, rd)| (&mut **lc, *rd)) {
                if node.block_id.starts_with("core/") {
                    if !LOOP_ITEM_CORE_ALLOWLIST.contains(&node.block_id.as_str()) {
                        return Err(BuilderError::Invalid(
                            "Block is not allowed inside core/loop-item",
                        ));
                    }
                } else {
                    if !schema.is_loop_schema_compatible() {
                        return Err(BuilderError::Invalid(
                            "Third-party block is not compatible with core/loop-item",
                        ));
                    }
                    lc.third_party += 1;
                    if lc.third_party > MAX_THIRD_PARTY_BLOCKS_IN_LOOP {
                        return Err(BuilderError::Invalid(
                            "Loop item exceeds max 2 third-party blocks",
                        ));
                    }
                }
            }
            if !effective_ctx.allows_contexts(schema.allowed_contexts.as_deref()) {
                return Err(BuilderError::Invalid(
                    "Block is not allowed in this Builder context",
                ));
            }
            let parent_token = parent_schema.map_or("root", |ps| ps.id.as_str());
            if !matches_pattern_list(schema.allowed_parents.as_deref(), parent_token) {
                return Err(BuilderError::Invalid(
                    "Block is not allowed under this parent",
                ));
            }
            if let Some(ps) = parent_schema
                && !matches_pattern_list(ps.allowed_children.as_deref(), &node.block_id)
            {
                return Err(BuilderError::Invalid(
                    "Parent container forbids this child block",
                ));
            }
            if !schema.supports_children && !node.children.is_empty() {
                return Err(BuilderError::Invalid(
                    "Leaf block cannot contain child nodes",
                ));
            }

            let prev_props = self
                .prev_index
                .get(node.id.as_str())
                .filter(|prev| prev.node.block_id == node.block_id)
                .map(|prev| &prev.node.props);
            schema
                .validate_values(&node.props, prev_props)
                .map_err(BuilderError::Invalid)?;

            if node.block_id == "core/list" {
                validate_list_cross_fields(&node.props)?;
            } else if node.block_id == "core/pattern-ref"
                && parse_positive_i64_prop(node.props.get("pattern_id")).is_none()
            {
                return Err(BuilderError::Invalid("Invalid Pattern reference ID"));
            } else if node.block_id == "core/contact-form"
                && parse_positive_i64_prop(node.props.get("form_id")).is_none()
            {
                return Err(BuilderError::Invalid("Invalid Contact Form reference ID"));
            } else if node.block_id == "core/template-part"
                && !node
                    .props
                    .get("part_key")
                    .and_then(Value::as_str)
                    .is_some_and(|key| valid_kebab_segment_with_limit(key, 64))
            {
                return Err(BuilderError::Invalid("Invalid Builder template part key"));
            }

            if let Some((_, min_loop, max_loop)) = LOOP_PARENT_CARDINALITY
                .iter()
                .find(|(id, _, _)| *id == node.block_id.as_str())
            {
                let loop_cnt = node
                    .children
                    .iter()
                    .filter(|c| c.block_id == "core/loop-item")
                    .count();
                if loop_cnt < *min_loop || loop_cnt > *max_loop {
                    return Err(BuilderError::Invalid(
                        "Invalid core/loop-item child cardinality for loop container",
                    ));
                }
            }

            if node.block_id == "core/loop-item" {
                let mut child_loop_counters = LoopCounters::default();
                self.visit_nodes(
                    &node.children,
                    depth + 1,
                    Some(node.id.as_str()),
                    Some(schema),
                    None,
                    Some((&mut child_loop_counters, 1)),
                )?;
            } else {
                let next_loop = loop_scope.as_mut().map(|(lc, rd)| (&mut **lc, *rd + 1));
                self.visit_nodes(
                    &node.children,
                    depth + 1,
                    Some(node.id.as_str()),
                    Some(schema),
                    None,
                    next_loop,
                )?;
            }
        }
        Ok(())
    }
}

impl BuilderDocument {
    /// Clean-Slate V1 single-pass recursive validation against `catalog`, `ctx`, and `previous`.
    pub fn validate_for_save(
        &self,
        catalog: &RegistryCatalog<RegisteredBlockSchema>,
        ctx: &BuilderContextTarget,
        previous: Option<&BuilderDocument>,
    ) -> Result<(), BuilderError> {
        if self.schema_version != BUILDER_SCHEMA_VERSION {
            return Err(BuilderError::Invalid(
                "Builder AST tiền-V1 (cấu trúc sections cũ hoặc schema_version != 1) không được hỗ trợ ở Clean-Slate V1.",
            ));
        }
        let mut prev_index = HashMap::new();
        if let Some(prev) = previous {
            index_previous_nodes_rec(&prev.blocks, None, &mut prev_index);
        }
        let mut walk = ValidationWalk {
            catalog,
            ctx,
            prev_index: &prev_index,
            seen_ids: HashSet::new(),
            node_count: 0,
            content_slots: 0,
            lists: 0,
            pattern_refs: 0,
        };
        walk.visit_nodes(&self.blocks, 1, None, None, None, None)?;
        self.to_json()?;
        Ok(())
    }
}
