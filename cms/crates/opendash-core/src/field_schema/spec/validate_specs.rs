use super::*;

pub fn validate_field_specs(specs: &[FieldSpec], ctx: FieldContext) -> Result<(), &'static str> {
    let mut seen_css_vars = BTreeSet::new();
    let mut total_block_repeater_max = 0usize;
    let mut leaf_data_count = 0usize;
    let top_map = build_scope_lookup_map(specs);

    validate_scope_specs(
        specs,
        ctx,
        &top_map,
        0,
        false,
        &mut seen_css_vars,
        &mut total_block_repeater_max,
        &mut leaf_data_count,
    )?;

    if ctx == FieldContext::StudioBlock {
        let top_data_count = specs.iter().filter(|s| !s.is_ui_only()).count();
        if specs.len() > MAX_BLOCK_TOTAL_SPECS {
            return Err("StudioBlock tối đa 64 phần tử top-level.");
        }
        if top_data_count > MAX_BLOCK_TOP_DATA_FIELDS {
            return Err("StudioBlock tối đa 32 trường dữ liệu top-level.");
        }
        if leaf_data_count > MAX_BLOCK_LEAF_DATA_FIELDS {
            return Err("StudioBlock tối đa 48 trường dữ liệu lá.");
        }
        if total_block_repeater_max > MAX_BLOCK_REPEATER_TOTAL_ITEMS {
            return Err("Tổng max_items của các repeater trên 1 StudioBlock tối đa 64.");
        }
    } else if leaf_data_count > MAX_SCHEMA_DATA_FIELDS {
        return Err("Mỗi schema tối đa 64 trường dữ liệu.");
    }

    validate_dependency_cycles(specs)?;
    Ok(())
}

pub(crate) fn build_scope_lookup_map(specs: &[FieldSpec]) -> BTreeMap<&str, &FieldSpec> {
    let mut map = BTreeMap::new();
    for spec in specs {
        map.entry(spec.key.as_str()).or_insert(spec);
    }
    map
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_scope_specs<'a>(
    specs: &'a [FieldSpec],
    ctx: FieldContext,
    top_map: &BTreeMap<&'a str, &'a FieldSpec>,
    group_depth: usize,
    in_repeater: bool,
    seen_css_vars: &mut BTreeSet<&'a str>,
    total_block_repeater_max: &mut usize,
    leaf_data_count: &mut usize,
) -> Result<(), &'static str> {
    let mut seen_keys = BTreeSet::new();
    let mut seen_sync_taxonomies = BTreeSet::new();
    let sibling_map = build_scope_lookup_map(specs);
    let in_group = group_depth > 0;

    for spec in specs {
        if !valid_snake_key(&spec.key) {
            return Err("Khóa field không hợp lệ (snake_case 1..=64 ký tự).");
        }
        if !seen_keys.insert(spec.key.as_str()) {
            return Err("Khóa field không được trùng lặp trong cùng phạm vi.");
        }
        if in_group && spec.key == "secret_ref" {
            return Err("Cấm đặt khóa con 'secret_ref' bên trong group.");
        }
        if !ctx.supports_type(spec.field_type) {
            return Err("Kiểu field không được hỗ trợ trong ngữ cảnh này.");
        }
        if spec.sync_entry_terms
            && let Some(tax) = spec.taxonomy.as_deref()
            && !seen_sync_taxonomies.insert(tax)
        {
            return Err(
                "duplicate_sync_entry_terms_taxonomy: Mỗi phân loại chỉ được đồng bộ tối đa bởi một trường term_ref trên cùng loại nội dung.",
            );
        }

        validate_single_field_spec(
            spec,
            ctx,
            top_map,
            &sibling_map,
            group_depth,
            in_repeater,
            seen_css_vars,
            total_block_repeater_max,
            leaf_data_count,
        )?;
    }

    Ok(())
}

pub(crate) fn lookup_condition_target<'a>(
    key: &str,
    sibling_map: &BTreeMap<&'a str, &'a FieldSpec>,
    top_map: &BTreeMap<&'a str, &'a FieldSpec>,
) -> Option<&'a FieldSpec> {
    sibling_map
        .get(key)
        .copied()
        .or_else(|| top_map.get(key).copied())
}

pub(crate) fn validate_conditions<'a>(
    conds: &ConditionGroups,
    owner: &FieldSpec,
    ctx: FieldContext,
    sibling_map: &BTreeMap<&'a str, &'a FieldSpec>,
    top_map: &BTreeMap<&'a str, &'a FieldSpec>,
) -> Result<(), &'static str> {
    if conds.is_empty() || conds.len() > MAX_CONDITION_GROUPS {
        return Err("conditions phải có từ 1..=8 nhóm OR.");
    }
    for group in conds {
        if group.is_empty() || group.len() > MAX_CONDITION_RULES_PER_GROUP {
            return Err("Mỗi nhóm AND trong conditions phải có từ 1..=8 điều kiện.");
        }
        for rule in group {
            if rule.field == owner.key {
                return Err("conditions không được tự tham chiếu chính mình.");
            }
            let Some(target) = lookup_condition_target(&rule.field, sibling_map, top_map) else {
                return Err("conditions phải trỏ tới field cùng phạm vi hoặc top-level tồn tại.");
            };
            if target.is_ui_only()
                || matches!(
                    target.field_type,
                    FieldType::Secret | FieldType::Spacing | FieldType::Group | FieldType::Repeater
                )
                || target.responsive
            {
                return Err(
                    "conditions không được tham chiếu field UI layout, secret, spacing, group, repeater hoặc responsive.",
                );
            }
            if matches!(
                ctx,
                FieldContext::Content | FieldContext::Term | FieldContext::Options
            ) {
                let (target_view, _, _) = target.effective_acl();
                let (owner_view, _, _) = owner.effective_acl();
                if target_view > owner_view {
                    return Err(
                        "Vi phạm ACL Oracle: field có thể xem bởi Editor không được phụ thuộc điều kiện vào field Admin-only.",
                    );
                }
            }
            match rule.op {
                ConditionOp::Empty | ConditionOp::NotEmpty => {
                    if matches!(rule.value, Some(ref v) if !v.is_null()) {
                        return Err("Toán tử empty/not_empty không nhận giá trị value.");
                    }
                }
                ConditionOp::In => {
                    let Some(Value::Array(arr)) = rule.value.as_ref() else {
                        return Err("Toán tử in yêu cầu mảng giá trị.");
                    };
                    if arr.is_empty() || arr.len() > 64 {
                        return Err("Mảng giá trị cho toán tử in phải có 1..=64 phần tử.");
                    }
                    if !arr
                        .iter()
                        .all(|v| v.is_string() || v.is_number() || v.is_boolean())
                    {
                        return Err("Phần tử toán tử in phải là giá trị vô hướng.");
                    }
                }
                ConditionOp::Eq | ConditionOp::Neq => {
                    let Some(val) = rule.value.as_ref() else {
                        return Err("Toán tử eq/neq yêu cầu giá trị value.");
                    };
                    if !(val.is_string() || val.is_number() || val.is_boolean() || val.is_null()) {
                        return Err("Giá trị toán tử eq/neq phải là giá trị vô hướng hoặc null.");
                    }
                }
                ConditionOp::Contains => {
                    let Some(val) = rule.value.as_ref() else {
                        return Err("Toán tử contains yêu cầu giá trị value.");
                    };
                    if !(val.is_string() || val.is_number() || val.is_boolean()) {
                        return Err("Giá trị toán tử contains phải là chuỗi hoặc số.");
                    }
                }
                ConditionOp::Gt | ConditionOp::Lt => {
                    let Some(val) = rule.value.as_ref() else {
                        return Err("Toán tử gt/lt yêu cầu giá trị so sánh.");
                    };
                    match target.field_type {
                        FieldType::Number | FieldType::Range => {
                            if val.as_f64().is_none_or(|f| !f.is_finite()) {
                                return Err("Toán tử gt/lt cho number/range yêu cầu số hữu hạn.");
                            }
                        }
                        FieldType::Date => {
                            if val.as_str().is_none_or(|s| !valid_iso_date(s)) {
                                return Err("Toán tử gt/lt cho date yêu cầu YYYY-MM-DD.");
                            }
                        }
                        FieldType::Datetime => {
                            if val.as_str().is_none_or(|s| !valid_iso_datetime_utc(s)) {
                                return Err("Toán tử gt/lt cho datetime yêu cầu ISO-8601 UTC.");
                            }
                        }
                        FieldType::Time => {
                            if val.as_str().is_none_or(|s| !valid_iso_time(s)) {
                                return Err("Toán tử gt/lt cho time yêu cầu HH:MM hoặc HH:MM:SS.");
                            }
                        }
                        _ => {
                            return Err(
                                "Toán tử gt/lt chỉ hỗ trợ number, range, date, datetime, time.",
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// 3-color DFS cycle detection across condition & `taxonomy_from_field` dependency graphs.
pub(crate) fn validate_dependency_cycles(specs: &[FieldSpec]) -> Result<(), &'static str> {
    let mut nodes: Vec<&FieldSpec> = Vec::new();
    let mut key_to_id_by_scope: Vec<BTreeMap<&str, usize>> = Vec::new();
    let mut top_lookup: BTreeMap<&str, usize> = BTreeMap::new();

    fn index_scope<'a>(
        specs: &'a [FieldSpec],
        nodes: &mut Vec<&'a FieldSpec>,
        scope_map: &mut BTreeMap<&'a str, usize>,
        all_edges_meta: &mut Vec<(usize, Vec<String>, usize)>,
        scope_maps: &mut Vec<BTreeMap<&'a str, usize>>,
    ) {
        let scope_idx = scope_maps.len();
        let mut local = BTreeMap::new();
        for spec in specs {
            let id = nodes.len();
            nodes.push(spec);
            local.insert(spec.key.as_str(), id);
            scope_map.insert(spec.key.as_str(), id);
            let mut deps = Vec::new();
            if let Some(conds) = spec.conditions.as_ref() {
                for g in conds {
                    for r in g {
                        deps.push(r.field.clone());
                    }
                }
            }
            if let Some(tf) = spec.taxonomy_from_field.as_ref() {
                deps.push(tf.clone());
            }
            all_edges_meta.push((id, deps, scope_idx));
        }
        scope_maps.push(local);
        for spec in specs {
            if !spec.fields.is_empty() {
                let mut child_map = BTreeMap::new();
                index_scope(
                    &spec.fields,
                    nodes,
                    &mut child_map,
                    all_edges_meta,
                    scope_maps,
                );
            }
        }
    }

    let mut edges_meta: Vec<(usize, Vec<String>, usize)> = Vec::new();
    index_scope(
        specs,
        &mut nodes,
        &mut top_lookup,
        &mut edges_meta,
        &mut key_to_id_by_scope,
    );

    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (node_id, dep_keys, scope_idx) in edges_meta {
        for dep in dep_keys {
            if let Some(&target_id) = key_to_id_by_scope[scope_idx]
                .get(dep.as_str())
                .or_else(|| top_lookup.get(dep.as_str()))
            {
                adj[node_id].push(target_id);
            }
        }
    }

    // 0 = unvisited, 1 = visiting, 2 = visited
    let mut color = vec![0u8; nodes.len()];
    fn dfs(u: usize, adj: &[Vec<usize>], color: &mut [u8]) -> bool {
        color[u] = 1;
        for &v in &adj[u] {
            if color[v] == 1 {
                return true;
            }
            if color[v] == 0 && dfs(v, adj, color) {
                return true;
            }
        }
        color[u] = 2;
        false
    }

    for i in 0..nodes.len() {
        if color[i] == 0 && dfs(i, &adj, &mut color) {
            return Err("Phát hiện chu trình phụ thuộc trong conditions/taxonomy_from_field.");
        }
    }
    Ok(())
}
