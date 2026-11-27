use super::*;

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

/// Optional common UX Builder metadata on any Section, Row, Column, or Element.
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub synthetic: bool,
}

impl NodeMeta {
    pub fn is_empty(&self) -> bool {
        self.name.as_deref().is_none_or(str::is_empty)
            && self.html_id.as_deref().is_none_or(str::is_empty)
            && self.class_name.as_deref().is_none_or(str::is_empty)
            && empty_visibility_option(&self.visibility)
            && !self.synthetic
    }
}

pub fn empty_node_meta_option(value: &Option<NodeMeta>) -> bool {
    value.as_ref().is_none_or(NodeMeta::is_empty)
}

pub(super) fn validate_node_meta(
    meta: Option<&NodeMeta>,
    has_responsive: bool,
    is_element: bool,
) -> Result<(), BuilderError> {
    let Some(meta) = meta else {
        return Ok(());
    };
    if is_element && meta.synthetic {
        return Err(BuilderError::Invalid("Element cannot be synthetic"));
    }
    if meta.synthetic
        && (has_responsive
            || meta.html_id.as_deref().is_some_and(|s| !s.is_empty())
            || meta.class_name.as_deref().is_some_and(|s| !s.is_empty())
            || !empty_visibility_option(&meta.visibility))
    {
        return Err(BuilderError::Invalid(
            "Synthetic wrapper cannot carry responsive or custom HTML attributes",
        ));
    }
    if let Some(name) = meta.name.as_deref()
        && !name.is_empty()
    {
        if name.trim().is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control)
        {
            return Err(BuilderError::Invalid("Invalid Builder node name"));
        }
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

/// Children preserve render order, while IDs remain independent of position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderDocument {
    pub schema_version: u32,
    pub sections: Vec<SectionNode>,
}

impl SectionNode {
    pub fn is_synthetic(&self) -> bool {
        self.meta.as_ref().is_some_and(|m| m.synthetic)
    }
}

impl RowNode {
    pub fn is_synthetic(&self) -> bool {
        self.meta.as_ref().is_some_and(|m| m.synthetic)
    }
}

impl ColumnNode {
    pub fn is_synthetic(&self) -> bool {
        self.meta.as_ref().is_some_and(|m| m.synthetic)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementNode {
    pub id: String,
    #[serde(default, skip_serializing_if = "empty_node_meta_option")]
    pub meta: Option<NodeMeta>,
    #[serde(flatten)]
    pub content: ElementContent,
}

pub(super) fn index_previous_plugin_values<'a>(
    previous: Option<&'a BuilderDocument>,
) -> HashMap<(&'a str, &'a str), &'a BTreeMap<String, Value>> {
    let mut map = HashMap::new();
    let Some(prev) = previous else {
        return map;
    };
    for section in &prev.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if let ElementContent::Known(KnownElement::PluginBlock(props)) =
                        &element.content
                    {
                        map.insert(
                            (element.id.as_str(), props.block_id.as_str()),
                            &props.values,
                        );
                    }
                }
            }
        }
    }
    map
}

pub fn is_preserved(
    previous: Option<&BuilderDocument>,
    (s, r, c, e): (usize, usize, usize, usize),
    section: &SectionNode,
    row: &RowNode,
    column: &ColumnNode,
    element: &ElementNode,
) -> bool {
    previous
        .and_then(|old| old.sections.get(s))
        .filter(|old| old.id == section.id)
        .and_then(|old| old.children.get(r))
        .filter(|old| old.id == row.id)
        .and_then(|old| old.children.get(c))
        .filter(|old| old.id == column.id)
        .and_then(|old| old.children.get(e))
        .is_some_and(|old| old == element)
}

impl BuilderDocument {
    /// RouteContext bindings are valid only on `single` or `page` role overrides,
    /// and custom field keys must belong to the scoped CPT when provided.
    pub fn validate_route_context_for_role(
        &self,
        role: Option<&str>,
        scoped_cpt_fields: Option<&[String]>,
    ) -> Result<(), BuilderError> {
        let role_allows_route_context = matches!(role, Some("single") | Some("page"));
        for section in &self.sections {
            for row in &section.children {
                for column in &row.children {
                    for element in &column.children {
                        let ElementContent::Known(known) = &element.content else {
                            continue;
                        };
                        let Some(binding) = known.binding() else {
                            continue;
                        };
                        if let BindingSource::RouteContext { field_key } = &binding.source {
                            if !role_allows_route_context {
                                return Err(BuilderError::Invalid(
                                    "RouteContext binding is only valid on single/page role overrides",
                                ));
                            }
                            if !matches!(field_key.as_str(), "title" | "slug" | "url" | "cover_url")
                                && let Some(allowed) = scoped_cpt_fields
                                && !allowed.iter().any(|k| k == field_key)
                            {
                                return Err(BuilderError::Invalid(
                                    "RouteContext field_key is not in scoped CPT fields",
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
