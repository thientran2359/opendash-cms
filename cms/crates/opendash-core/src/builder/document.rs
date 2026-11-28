use super::*;

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeVisibility {
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide_desktop: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide_tablet: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide_mobile: bool,
}

impl NodeVisibility {
    pub fn is_empty(&self) -> bool {
        !self.hide_desktop && !self.hide_tablet && !self.hide_mobile
    }
}

pub(super) fn empty_visibility_option(value: &Option<NodeVisibility>) -> bool {
    value.as_ref().is_none_or(NodeVisibility::is_empty)
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    #[serde(default, skip_serializing_if = "empty_visibility_option")]
    pub visibility: Option<NodeVisibility>,
}

impl NodeMeta {
    pub fn is_empty(&self) -> bool {
        self.name.as_deref().is_none_or(str::is_empty)
            && self.html_id.as_deref().is_none_or(str::is_empty)
            && self.class_name.as_deref().is_none_or(str::is_empty)
            && empty_visibility_option(&self.visibility)
    }
}

pub fn empty_node_meta_option(value: &Option<NodeMeta>) -> bool {
    value.as_ref().is_none_or(NodeMeta::is_empty)
}

pub(super) fn validate_node_meta(meta: Option<&NodeMeta>) -> Result<(), BuilderError> {
    let Some(meta) = meta else {
        return Ok(());
    };
    if let Some(name) = meta.name.as_deref()
        && !name.is_empty()
        && (name.trim().is_empty()
            || name.chars().count() > 80
            || name.chars().any(char::is_control))
    {
        return Err(BuilderError::Invalid("Invalid Builder node name"));
    }
    if let Some(html_id) = meta.html_id.as_deref()
        && !html_id.is_empty()
    {
        let valid = html_id.len() <= 64
            && html_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
        if !valid {
            return Err(BuilderError::Invalid("Invalid Builder node HTML ID"));
        }
    }
    if let Some(class_name) = meta.class_name.as_deref()
        && !class_name.is_empty()
    {
        let valid = class_name.len() <= 256
            && class_name.bytes().all(|b| {
                b.is_ascii_alphanumeric()
                    || matches!(
                        b,
                        b' ' | b'-' | b'_' | b':' | b'/' | b'.' | b'[' | b']' | b'%' | b'#'
                    )
            });
        if !valid {
            return Err(BuilderError::Invalid("Invalid Builder node CSS class"));
        }
    }
    Ok(())
}

/// Recursive unified Studio AST node (`<owner>/<slug>` + schema-driven props + children).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockNode {
    pub id: String,
    pub block_id: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "empty_node_meta_option")]
    pub meta: Option<NodeMeta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<BlockNode>,
}

/// Clean-Slate V1 unified recursive Builder AST (`schema_version: 1`, `blocks: Vec<BlockNode>`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderDocument {
    pub schema_version: u32,
    #[serde(default)]
    pub blocks: Vec<BlockNode>,
}

impl Default for BuilderDocument {
    fn default() -> Self {
        Self::empty()
    }
}

impl BuilderDocument {
    pub fn empty() -> Self {
        Self {
            schema_version: BUILDER_SCHEMA_VERSION,
            blocks: Vec::new(),
        }
    }

    /// Decodes Clean-Slate V1 JSON, rejecting pre-V1 `sections` payloads or wrong `schema_version`.
    pub fn from_json(bytes: &[u8]) -> Result<Self, BuilderError> {
        if bytes.len() > MAX_BUILDER_JSON_BYTES {
            return Err(BuilderError::Invalid("Builder JSON exceeds 256 KiB"));
        }
        match serde_json::from_slice::<Self>(bytes) {
            Ok(doc) => {
                if doc.schema_version != BUILDER_SCHEMA_VERSION {
                    return Err(BuilderError::Invalid(
                        "Builder AST tiền-V1 (cấu trúc sections cũ hoặc schema_version != 1) không được hỗ trợ ở Clean-Slate V1.",
                    ));
                }
                Ok(doc)
            }
            Err(err) => {
                if let Ok(Value::Object(obj)) = serde_json::from_slice::<Value>(bytes)
                    && (obj.contains_key("sections")
                        || obj
                            .get("schema_version")
                            .and_then(Value::as_u64)
                            .is_some_and(|v| v != u64::from(BUILDER_SCHEMA_VERSION)))
                {
                    return Err(BuilderError::Invalid(
                        "Builder AST tiền-V1 (cấu trúc sections cũ hoặc schema_version != 1) không được hỗ trợ ở Clean-Slate V1.",
                    ));
                }
                Err(BuilderError::Json(err))
            }
        }
    }

    pub fn to_json(&self) -> Result<Vec<u8>, BuilderError> {
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_BUILDER_JSON_BYTES {
            return Err(BuilderError::Invalid("Builder JSON exceeds 256 KiB"));
        }
        Ok(bytes)
    }

    /// Visits every node in pre-order depth-first sequence with its 1-based depth.
    pub fn walk_blocks(&self, mut f: impl FnMut(&BlockNode, usize)) {
        fn walk_rec(nodes: &[BlockNode], depth: usize, f: &mut impl FnMut(&BlockNode, usize)) {
            for node in nodes {
                f(node, depth);
                walk_rec(&node.children, depth + 1, f);
            }
        }
        walk_rec(&self.blocks, 1, &mut f);
    }

    /// Mutably visits every node in pre-order depth-first sequence with its 1-based depth.
    pub fn walk_blocks_mut(&mut self, mut f: impl FnMut(&mut BlockNode, usize)) {
        fn walk_rec(
            nodes: &mut [BlockNode],
            depth: usize,
            f: &mut impl FnMut(&mut BlockNode, usize),
        ) {
            for node in nodes {
                f(node, depth);
                walk_rec(&mut node.children, depth + 1, f);
            }
        }
        walk_rec(&mut self.blocks, 1, &mut f);
    }

    /// Strips top-level props that strictly equal (`==`) `field.default` for Active+Declared blocks.
    pub fn strip_default_props(&mut self, catalog: &RegistryCatalog<RegisteredBlockSchema>) {
        self.walk_blocks_mut(|node, _| {
            if let Some(entry) = catalog.get(&node.block_id)
                && entry.status == ProviderStatus::Active
                && entry.declaration_state == DeclarationState::Declared
            {
                entry.descriptor.schema.strip_default_props(&mut node.props);
            }
        });
    }
}
