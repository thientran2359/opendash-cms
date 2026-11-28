use std::collections::{BTreeMap, HashMap, HashSet};

use crate::{
    content::valid_content_type_key,
    field_schema::{FieldContext, validate_field_specs},
    icons::lookup_core_icon_paths,
    id::{valid_kebab_segment_with_limit, valid_namespaced_block_id, valid_snake_key},
    registry::{DeclarationState, ProviderStatus, RegistryCatalog},
};

use super::{
    BLOCK_SCHEMA_VERSION, BlockSchema, DefaultChildSpec, MAX_BLOCK_PRESETS,
    MAX_DEFAULT_CHILDREN_DEPTH, MAX_DEFAULT_CHILDREN_NODES, RESERVED_BLOCK_FIELD_KEYS,
    RegisteredBlockSchema,
};

fn valid_icon_name(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 32
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn valid_allowed_parent_token(token: &str) -> bool {
    if token == "root" || token == "*" {
        return true;
    }
    if let Some(neg) = token.strip_prefix('!') {
        return neg == "root" || valid_namespaced_block_id(neg);
    }
    valid_namespaced_block_id(token)
}

pub fn valid_allowed_child_token(token: &str) -> bool {
    if token == "*" {
        return true;
    }
    if let Some(neg) = token.strip_prefix('!') {
        return valid_namespaced_block_id(neg);
    }
    valid_namespaced_block_id(token)
}

pub fn valid_allowed_context_token(token: &str) -> bool {
    if token == "!*" {
        return false;
    }
    let raw = token.strip_prefix('!').unwrap_or(token);
    if matches!(
        raw,
        "*" | "entry_body"
            | "pattern"
            | "theme_pattern"
            | "template"
            | "part"
            | "single"
            | "single_custom"
            | "singular"
            | "page"
            | "page_layout"
            | "home"
            | "index"
            | "archive"
            | "taxonomy"
            | "not_found"
            | "search"
            | "loop_item"
    ) {
        return true;
    }
    if let Some(cpt) = raw
        .strip_prefix("entry_body:")
        .or_else(|| raw.strip_prefix("single:"))
        .or_else(|| raw.strip_prefix("archive:"))
        .or_else(|| raw.strip_prefix("taxonomy:"))
    {
        return valid_content_type_key(cpt);
    }
    if let Some(part_key) = raw.strip_prefix("part:") {
        return valid_kebab_segment_with_limit(part_key, 64);
    }
    false
}

/// Evaluates an `allowed_parents` or `allowed_children` pattern list against `candidate`.
/// Negations (`!token`) always take precedence; an exclusion-only list implicitly allows `*`.
pub fn matches_pattern_list(list: Option<&[String]>, candidate: &str) -> bool {
    let Some(tokens) = list else {
        return true;
    };
    let mut has_positive = false;
    let mut matched_positive = false;
    for token in tokens {
        if let Some(neg) = token.strip_prefix('!') {
            if neg == candidate {
                return false;
            }
        } else {
            has_positive = true;
            if token == "*" || token == candidate {
                matched_positive = true;
            }
        }
    }
    if has_positive { matched_positive } else { true }
}

fn validate_constraint_tokens(
    items: Option<&Vec<String>>,
    predicate: fn(&str) -> bool,
    err_msg: &'static str,
) -> Result<(), &'static str> {
    let Some(list) = items else {
        return Ok(());
    };
    if list.is_empty() || list.len() > 32 {
        return Err("Danh sách ràng buộc block chỉ được chứa 1..=32 phần tử.");
    }
    if !list.iter().all(|item| predicate(item)) {
        return Err(err_msg);
    }
    Ok(())
}

fn validate_default_children_local(
    owner_block_id: &str,
    allowed_children: Option<&[String]>,
    children: &[DefaultChildSpec],
    depth: usize,
    total_nodes: &mut usize,
) -> Result<(), &'static str> {
    if children.is_empty() {
        return Ok(());
    }
    if depth > MAX_DEFAULT_CHILDREN_DEPTH {
        return Err("default_children chỉ hỗ trợ độ sâu tối đa 2 tầng.");
    }
    for child in children {
        *total_nodes += 1;
        if *total_nodes > MAX_DEFAULT_CHILDREN_NODES {
            return Err("Mỗi mẫu default_children / preset tối đa 8 node.");
        }
        if !valid_namespaced_block_id(&child.block_id) {
            return Err("block_id trong default_children phải là <owner>/<slug> hợp lệ.");
        }
        if child.block_id == owner_block_id {
            return Err("default_children không được tự tham chiếu chính block cha.");
        }
        if depth == 1 && !matches_pattern_list(allowed_children, &child.block_id) {
            return Err("default_children vi phạm allowed_children của block cha.");
        }
        validate_default_children_local(
            owner_block_id,
            allowed_children,
            &child.children,
            depth + 1,
            total_nodes,
        )?;
    }
    Ok(())
}

pub(super) fn validate_block_schema(
    schema: &BlockSchema,
    expected_owner_prefix: &str,
) -> Result<(), &'static str> {
    if schema.schema_version != BLOCK_SCHEMA_VERSION {
        return Err("block.json chỉ hỗ trợ schema_version == 1.");
    }
    let render_ok = if expected_owner_prefix == "core" {
        matches!(schema.render.as_str(), "native" | "render.html")
    } else {
        schema.render == "render.html"
    };
    if !render_ok {
        return Err("block.json bắt buộc khai báo render == \"render.html\".");
    }
    if schema.title.trim().is_empty()
        || schema.title.chars().count() > 80
        || schema.title.chars().any(char::is_control)
    {
        return Err("Tiêu đề block phải không rỗng, <= 80 ký tự và không chứa ký tự điều khiển.");
    }
    if let Some(icon) = &schema.icon
        && !valid_icon_name(icon)
    {
        return Err("Tên icon block chỉ được gồm [a-z0-9-] (1..=32 ký tự).");
    }
    if !valid_namespaced_block_id(&schema.id) {
        return Err("ID block phải có dạng <owner>/<slug> kebab-case hợp lệ.");
    }
    let Some((owner_prefix, _)) = schema.id.split_once('/') else {
        return Err("ID block thiếu tiền tố owner.");
    };
    if owner_prefix != expected_owner_prefix {
        return Err("ID block không khớp tiền tố owner của gói sở hữu.");
    }

    validate_constraint_tokens(
        schema.allowed_parents.as_ref(),
        valid_allowed_parent_token,
        "Phần tử trong allowed_parents không hợp lệ.",
    )?;
    validate_constraint_tokens(
        schema.allowed_children.as_ref(),
        valid_allowed_child_token,
        "Phần tử trong allowed_children không hợp lệ.",
    )?;
    validate_constraint_tokens(
        schema.allowed_contexts.as_ref(),
        valid_allowed_context_token,
        "Phần tử trong allowed_contexts không hợp lệ.",
    )?;

    if !schema.supports_children
        && (schema.allowed_children.is_some()
            || !schema.default_children.is_empty()
            || schema.presets.iter().any(|p| !p.children.is_empty()))
    {
        return Err(
            "Block không bật supports_children cấm khai báo allowed_children hoặc default_children.",
        );
    }

    for field in &schema.fields {
        if RESERVED_BLOCK_FIELD_KEYS.contains(&field.key.as_str()) {
            return Err("Khóa trường trùng với biến hệ thống C-SEC bị cấm.");
        }
    }
    validate_field_specs(&schema.fields, FieldContext::StudioBlock)?;

    let mut default_nodes = 0_usize;
    validate_default_children_local(
        &schema.id,
        schema.allowed_children.as_deref(),
        &schema.default_children,
        1,
        &mut default_nodes,
    )?;

    if schema.presets.len() > MAX_BLOCK_PRESETS {
        return Err("Số lượng preset trên một block tối đa 12.");
    }
    let mut preset_keys = HashSet::new();
    for preset in &schema.presets {
        if !valid_snake_key(&preset.key) {
            return Err("Khóa preset phải tuân thủ snake_case hợp lệ.");
        }
        if !preset_keys.insert(preset.key.as_str()) {
            return Err("Khóa preset bị trùng lặp trong cùng một block.");
        }
        if preset.title.trim().is_empty()
            || preset.title.chars().count() > 80
            || preset.title.chars().any(char::is_control)
        {
            return Err("Tiêu đề preset phải không rỗng và <= 80 ký tự.");
        }
        if let Some(icon) = &preset.icon
            && lookup_core_icon_paths(icon).is_none()
        {
            return Err("Icon preset phải thuộc danh mục CORE_ICONS hợp lệ.");
        }
        schema.validate_values(&preset.props, None)?;
        let mut preset_nodes = 0_usize;
        validate_default_children_local(
            &schema.id,
            schema.allowed_children.as_deref(),
            &preset.children,
            1,
            &mut preset_nodes,
        )?;
    }

    Ok(())
}

fn check_catalog_child_tree(
    root_block_id: &str,
    parent_schema: &BlockSchema,
    children: &[DefaultChildSpec],
    active: &BTreeMap<&str, &BlockSchema>,
    edges: &mut HashMap<String, Vec<String>>,
) -> Result<(), &'static str> {
    for child in children {
        let Some(&child_schema) = active.get(child.block_id.as_str()) else {
            return Err("Mẫu default_children tham chiếu block không tồn tại hoặc chưa active.");
        };
        if !matches_pattern_list(parent_schema.allowed_children.as_deref(), &child.block_id) {
            return Err("Mẫu default_children vi phạm allowed_children của block cha.");
        }
        if !matches_pattern_list(child_schema.allowed_parents.as_deref(), &parent_schema.id) {
            return Err("Mẫu default_children vi phạm allowed_parents của block con.");
        }
        if !child.children.is_empty() && !child_schema.supports_children {
            return Err("Block con trong default_children không bật supports_children.");
        }
        child_schema.validate_values(&child.props, None)?;
        edges
            .entry(root_block_id.to_owned())
            .or_default()
            .push(child.block_id.clone());
        check_catalog_child_tree(root_block_id, child_schema, &child.children, active, edges)?;
    }
    Ok(())
}

fn detect_template_cycle<'a>(
    node: &'a str,
    edges: &'a HashMap<String, Vec<String>>,
    state: &mut HashMap<&'a str, u8>,
) -> Result<(), &'static str> {
    match state.get(node).copied().unwrap_or(0) {
        1 => return Err("Phát hiện chu trình tham chiếu chéo trong default_children / presets."),
        2 => return Ok(()),
        _ => {}
    }
    state.insert(node, 1);
    if let Some(neighbors) = edges.get(node) {
        for next in neighbors {
            detect_template_cycle(next.as_str(), edges, state)?;
        }
    }
    state.insert(node, 2);
    Ok(())
}

/// Phase 2 cross-catalog validation: ensures all `default_children` / `preset.children`
/// point to active blocks, satisfy mutual parent/child constraints + props schemas, and are acyclic.
pub fn validate_block_catalog_templates(
    catalog: &RegistryCatalog<RegisteredBlockSchema>,
) -> Result<(), &'static str> {
    let mut active: BTreeMap<&str, &BlockSchema> = BTreeMap::new();
    for (id, entry) in catalog.iter() {
        if entry.status == ProviderStatus::Active
            && entry.declaration_state == DeclarationState::Declared
        {
            active.insert(id.as_str(), &entry.descriptor.schema);
        }
    }

    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    for (&block_id, &schema) in &active {
        check_catalog_child_tree(
            block_id,
            schema,
            &schema.default_children,
            &active,
            &mut edges,
        )?;
        for preset in &schema.presets {
            check_catalog_child_tree(block_id, schema, &preset.children, &active, &mut edges)?;
        }
    }

    let mut dfs_state = HashMap::new();
    for &block_id in active.keys() {
        detect_template_cycle(block_id, &edges, &mut dfs_state)?;
    }
    Ok(())
}
