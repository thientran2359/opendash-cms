use super::*;

pub(super) fn format_css_number(
    n: f64,
    unit: Option<opendash_core::field_schema::CssUnit>,
) -> Option<String> {
    if !n.is_finite() {
        return None;
    }
    let suffix = unit.map(|u| u.as_css_suffix()).unwrap_or("");
    if n.fract() == 0.0 && n.abs() < 1e15 {
        Some(format!("{}{suffix}", n as i64))
    } else {
        Some(format!("{n}{suffix}"))
    }
}

/// Emits `<style data-builder-responsive>` for per-node breakpoint visibility rules.
pub(super) fn builder_visibility_css(document: &BuilderDocument) -> String {
    let mut vis_desktop = String::new();
    let mut vis_tablet = String::new();
    let mut vis_mobile = String::new();
    document.walk_blocks(|node, _| {
        let Some(vis) = node.meta.as_ref().and_then(|m| m.visibility.as_ref()) else {
            return;
        };
        if vis.hide_desktop {
            push_visibility_rule(&mut vis_desktop, &node.id);
        }
        if vis.hide_tablet {
            push_visibility_rule(&mut vis_tablet, &node.id);
        }
        if vis.hide_mobile {
            push_visibility_rule(&mut vis_mobile, &node.id);
        }
    });
    if vis_desktop.is_empty() && vis_tablet.is_empty() && vis_mobile.is_empty() {
        return String::new();
    }
    let mut css = "<style data-builder-responsive>".to_owned();
    if !vis_desktop.is_empty() {
        css.push_str(&format!("@media (width >= 1024px){{{vis_desktop}}}"));
    }
    if !vis_tablet.is_empty() {
        css.push_str(&format!(
            "@media (width >= 768px) and (width < 1024px){{{vis_tablet}}}"
        ));
    }
    if !vis_mobile.is_empty() {
        css.push_str(&format!("@media (width < 768px){{{vis_mobile}}}"));
    }
    css.push_str("</style>");
    css
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_scoped_css_for_fields(
    fields: &[opendash_core::field_schema::FieldSpec],
    values: &std::collections::BTreeMap<String, serde_json::Value>,
    path_prefix: Option<&str>,
    node_scope: &str,
    is_preview: bool,
    hydration_media: &std::collections::BTreeMap<String, crate::storage::fields::HydratedMediaRow>,
    desktop_props: &mut String,
    tablet_props: &mut String,
    mobile_props: &mut String,
    desktop_rules: &mut String,
    tablet_rules: &mut String,
    mobile_rules: &mut String,
) {
    use opendash_core::field_schema::{
        FieldType, MediaFieldKind, SpacingProperty, SpacingValue, valid_color_value,
        valid_css_select_value, valid_css_var_name, valid_sha256_media_id,
    };

    for spec in fields {
        if spec.is_ui_only() || spec.field_type == FieldType::Secret {
            continue;
        }
        let field_path = match path_prefix {
            Some(prefix) => format!("{prefix}.{}", spec.key),
            None => spec.key.clone(),
        };
        match spec.field_type {
            FieldType::Group => {
                let child_values: std::collections::BTreeMap<String, serde_json::Value> = values
                    .get(&spec.key)
                    .and_then(serde_json::Value::as_object)
                    .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();
                collect_scoped_css_for_fields(
                    &spec.fields,
                    &child_values,
                    Some(&field_path),
                    node_scope,
                    is_preview,
                    hydration_media,
                    desktop_props,
                    tablet_props,
                    mobile_props,
                    desktop_rules,
                    tablet_rules,
                    mobile_rules,
                );
            }
            FieldType::Number | FieldType::Range => {
                let Some(var_name) = spec.css_var.as_deref() else {
                    continue;
                };
                if !valid_css_var_name(var_name) {
                    continue;
                }
                let default_num = spec
                    .default
                    .as_ref()
                    .and_then(serde_json::Value::as_f64)
                    .or(if spec.field_type == FieldType::Range {
                        spec.min
                    } else {
                        None
                    });
                let raw = values.get(&spec.key);
                if spec.responsive
                    && let Some(obj) = raw.and_then(|v| v.as_object())
                {
                    if let Some(d) = obj
                        .get("desktop")
                        .and_then(|v| v.as_f64())
                        .or(default_num)
                        .and_then(|n| format_css_number(n, spec.unit))
                    {
                        desktop_props.push_str(&format!("{var_name}:{d};"));
                    }
                    if let Some(t) = obj
                        .get("tablet")
                        .and_then(|v| v.as_f64())
                        .and_then(|n| format_css_number(n, spec.unit))
                    {
                        tablet_props.push_str(&format!("{var_name}:{t};"));
                    }
                    if let Some(m) = obj
                        .get("mobile")
                        .and_then(|v| v.as_f64())
                        .and_then(|n| format_css_number(n, spec.unit))
                    {
                        mobile_props.push_str(&format!("{var_name}:{m};"));
                    }
                } else if let Some(d) = raw
                    .and_then(|v| v.as_f64())
                    .or(default_num)
                    .and_then(|n| format_css_number(n, spec.unit))
                {
                    desktop_props.push_str(&format!("{var_name}:{d};"));
                }
            }
            FieldType::Select | FieldType::RadioButtons => {
                let Some(var_name) = spec.css_var.as_deref() else {
                    continue;
                };
                if !valid_css_var_name(var_name) {
                    continue;
                }
                let default_str = spec.default.as_ref().and_then(serde_json::Value::as_str);
                let raw = values.get(&spec.key);
                if spec.responsive
                    && let Some(obj) = raw.and_then(|v| v.as_object())
                {
                    if let Some(d) = obj
                        .get("desktop")
                        .and_then(|v| v.as_str())
                        .or(default_str)
                        .filter(|s| valid_css_select_value(s))
                    {
                        desktop_props.push_str(&format!("{var_name}:{d};"));
                    }
                    if let Some(t) = obj
                        .get("tablet")
                        .and_then(|v| v.as_str())
                        .filter(|s| valid_css_select_value(s))
                    {
                        tablet_props.push_str(&format!("{var_name}:{t};"));
                    }
                    if let Some(m) = obj
                        .get("mobile")
                        .and_then(|v| v.as_str())
                        .filter(|s| valid_css_select_value(s))
                    {
                        mobile_props.push_str(&format!("{var_name}:{m};"));
                    }
                } else if let Some(d) = raw
                    .and_then(|v| v.as_str())
                    .or(default_str)
                    .filter(|s| valid_css_select_value(s))
                {
                    desktop_props.push_str(&format!("{var_name}:{d};"));
                }
            }
            FieldType::Spacing => {
                let css_unit = spec.effective_unit();
                let spacing_prop = spec.effective_spacing_property();
                let default_sv = spec
                    .default
                    .as_ref()
                    .and_then(|v| serde_json::from_value::<SpacingValue>(v.clone()).ok());
                let raw = values.get(&spec.key);
                let (d_sv, t_sv, m_sv) = crate::template_sec::extract_spacing_breakpoint_values(
                    raw,
                    default_sv,
                    spec.responsive,
                );
                if let Some(var_name) = spec.css_var.as_deref() {
                    if !valid_css_var_name(var_name) {
                        continue;
                    }
                    if let Some(d_css) = d_sv.and_then(|sv| sv.format_css(css_unit)) {
                        desktop_props.push_str(&format!("{var_name}:{d_css};"));
                    }
                    if let Some(t_css) = t_sv.and_then(|sv| sv.format_css(css_unit)) {
                        tablet_props.push_str(&format!("{var_name}:{t_css};"));
                    }
                    if let Some(m_css) = m_sv.and_then(|sv| sv.format_css(css_unit)) {
                        mobile_props.push_str(&format!("{var_name}:{m_css};"));
                    }
                } else if crate::template_sec::spacing_field_tailwind_classes(
                    raw,
                    default_sv,
                    spec.responsive,
                    css_unit,
                    spacing_prop,
                )
                .is_none()
                {
                    let css_prop = match spacing_prop {
                        SpacingProperty::Padding => "padding",
                        SpacingProperty::Margin => "margin",
                    };
                    if let Some(d_css) = d_sv.and_then(|sv| sv.format_css(css_unit)) {
                        desktop_rules.push_str(&format!(
                            "[data-od-node-scope=\"{node_scope}\"][data-od-spacing=\"{field_path}\"],[data-od-node-scope=\"{node_scope}\"] [data-od-spacing=\"{field_path}\"]{{{css_prop}:{d_css};}}"
                        ));
                    }
                    if let Some(t_css) = t_sv.and_then(|sv| sv.format_css(css_unit)) {
                        tablet_rules.push_str(&format!(
                            "[data-od-node-scope=\"{node_scope}\"][data-od-spacing=\"{field_path}\"],[data-od-node-scope=\"{node_scope}\"] [data-od-spacing=\"{field_path}\"]{{{css_prop}:{t_css};}}"
                        ));
                    }
                    if let Some(m_css) = m_sv.and_then(|sv| sv.format_css(css_unit)) {
                        mobile_rules.push_str(&format!(
                            "[data-od-node-scope=\"{node_scope}\"][data-od-spacing=\"{field_path}\"],[data-od-node-scope=\"{node_scope}\"] [data-od-spacing=\"{field_path}\"]{{{css_prop}:{m_css};}}"
                        ));
                    }
                }
            }
            FieldType::Color => {
                let Some(var_name) = spec.css_var.as_deref() else {
                    continue;
                };
                if !valid_css_var_name(var_name) {
                    continue;
                }
                let default_str = spec.default.as_ref().and_then(serde_json::Value::as_str);
                if let Some(c) = values
                    .get(&spec.key)
                    .and_then(|v| v.as_str())
                    .or(default_str)
                    .filter(|s| !s.is_empty() && valid_color_value(s))
                {
                    desktop_props.push_str(&format!("{var_name}:{c};"));
                }
            }
            FieldType::Media if spec.effective_media_kind() == MediaFieldKind::Image => {
                let Some(var_name) = spec.css_var.as_deref() else {
                    continue;
                };
                if !valid_css_var_name(var_name) {
                    continue;
                }
                let default_str = spec.default.as_ref().and_then(serde_json::Value::as_str);
                if let Some(id) = values
                    .get(&spec.key)
                    .and_then(|v| v.as_str())
                    .or(default_str)
                    .filter(|s| !s.is_empty() && valid_sha256_media_id(s))
                {
                    let url_path = super::media_render::versioned_media_file_url(
                        id,
                        hydration_media,
                        is_preview,
                    );
                    desktop_props.push_str(&format!("{var_name}:url(\"{url_path}\");"));
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_block_css_rec(
    nodes: &[BlockNode],
    state: &BuilderRenderContext,
    scope: &str,
    is_preview: bool,
    desktop_rules: &mut String,
    tablet_rules: &mut String,
    mobile_rules: &mut String,
) {
    use opendash_core::registry::{DeclarationState, ProviderStatus};

    for node in nodes {
        let Some(def) = state.declared_blocks.get(&node.block_id) else {
            continue;
        };
        if def.status != ProviderStatus::Active
            || def.declaration_state != DeclarationState::Declared
        {
            continue;
        }
        if valid_package_id(&node.id) {
            let node_scope = format!("{scope}-{}", node.id);
            let mut desktop_props = String::new();
            let mut tablet_props = String::new();
            let mut mobile_props = String::new();

            collect_scoped_css_for_fields(
                &def.schema.schema.fields,
                &node.props,
                None,
                &node_scope,
                is_preview,
                &state.hydration_media,
                &mut desktop_props,
                &mut tablet_props,
                &mut mobile_props,
                desktop_rules,
                tablet_rules,
                mobile_rules,
            );

            if !desktop_props.is_empty() {
                desktop_rules.push_str(&format!(
                    "[data-od-node-scope=\"{node_scope}\"]{{{desktop_props}}}"
                ));
            }
            if !tablet_props.is_empty() {
                tablet_rules.push_str(&format!(
                    "[data-od-node-scope=\"{node_scope}\"]{{{tablet_props}}}"
                ));
            }
            if !mobile_props.is_empty() {
                mobile_rules.push_str(&format!(
                    "[data-od-node-scope=\"{node_scope}\"]{{{mobile_props}}}"
                ));
            }
        }
        collect_block_css_rec(
            &node.children,
            state,
            scope,
            is_preview,
            desktop_rules,
            tablet_rules,
            mobile_rules,
        );
    }
}

/// Emits scoped CSS custom properties (`--od-*`) and `[data-od-spacing]` fallback
/// rules for all active, declared `BlockNode` subtrees under `[data-od-node-scope="{scope}-{id}"]`.
pub fn serialize_scoped_block_css(
    state: &BuilderRenderContext,
    document: &BuilderDocument,
    scope: &str,
    is_preview: bool,
) -> ApiResult<String> {
    if scope.is_empty()
        || !scope
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(ApiError::bad_request("Scope CSS block không hợp lệ."));
    }

    let mut desktop_rules = String::new();
    let mut tablet_rules = String::new();
    let mut mobile_rules = String::new();

    collect_block_css_rec(
        &document.blocks,
        state,
        scope,
        is_preview,
        &mut desktop_rules,
        &mut tablet_rules,
        &mut mobile_rules,
    );

    if desktop_rules.is_empty() && tablet_rules.is_empty() && mobile_rules.is_empty() {
        return Ok(String::new());
    }

    let mut css_body = desktop_rules;
    if !tablet_rules.is_empty() {
        css_body.push_str(&format!("@media (width < 1024px){{{tablet_rules}}}"));
    }
    if !mobile_rules.is_empty() {
        css_body.push_str(&format!("@media (width < 768px){{{mobile_rules}}}"));
    }
    crate::template_sec::validate_safe_scoped_block_css(&css_body, is_preview)
        .map_err(ApiError::bad_request)?;
    Ok(format!("<style data-od-block-styles>{css_body}</style>"))
}
