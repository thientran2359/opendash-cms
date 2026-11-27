use super::*;

pub(crate) fn generate_richtext_nonce() -> Result<String, &'static str> {
    let mut random = [0_u8; 16];
    OsRng
        .try_fill_bytes(&mut random)
        .map_err(|_| "Không tạo được nonce RichText.")?;
    let mut nonce = String::with_capacity(BLOCK_RICHTEXT_NONCE_PREFIX.len() + 32);
    nonce.push_str(BLOCK_RICHTEXT_NONCE_PREFIX);
    for byte in random {
        use std::fmt::Write;
        write!(&mut nonce, "{byte:02x}").map_err(|_| "Không tạo được nonce RichText.")?;
    }
    Ok(nonce)
}

pub(crate) fn extract_spacing_breakpoint_values(
    val: Option<&serde_json::Value>,
    fallback: Option<opendash_core::field_schema::SpacingValue>,
    responsive: bool,
) -> (
    Option<opendash_core::field_schema::SpacingValue>,
    Option<opendash_core::field_schema::SpacingValue>,
    Option<opendash_core::field_schema::SpacingValue>,
) {
    use opendash_core::field_schema::SpacingValue;
    let parse_sv = |v: &serde_json::Value| serde_json::from_value::<SpacingValue>(v.clone()).ok();
    match val.and_then(|v| v.as_object()) {
        Some(obj)
            if obj.contains_key("top")
                || obj.contains_key("right")
                || obj.contains_key("bottom")
                || obj.contains_key("left") =>
        {
            let d = val.and_then(parse_sv).or(fallback);
            (d, None, None)
        }
        Some(obj) => {
            let d = obj.get("desktop").and_then(parse_sv).or(fallback);
            let t = if responsive {
                obj.get("tablet").and_then(parse_sv)
            } else {
                None
            };
            let m = if responsive {
                obj.get("mobile").and_then(parse_sv)
            } else {
                None
            };
            (d, t, m)
        }
        None => (fallback, None, None),
    }
}

pub(crate) fn spacing_field_tailwind_classes(
    val: Option<&serde_json::Value>,
    fallback: Option<opendash_core::field_schema::SpacingValue>,
    responsive: bool,
    unit: opendash_core::field_schema::CssUnit,
    property: opendash_core::field_schema::SpacingProperty,
) -> Option<Vec<String>> {
    use opendash_core::{
        builder::{BoxSpacing, compile_box_spacing_tailwind},
        field_schema::{CssUnit, SpacingProperty, SpacingValue},
    };
    if unit != CssUnit::Px {
        return None;
    }
    let allow_negative = property == SpacingProperty::Margin;
    let min_val = if allow_negative {
        f64::from(i16::MIN)
    } else {
        0.0
    };
    let to_box = |sv: SpacingValue| -> Option<BoxSpacing> {
        let conv = |opt: Option<f64>| -> Option<Option<i16>> {
            match opt {
                None => Some(None),
                Some(n)
                    if n.is_finite()
                        && n.fract() == 0.0
                        && n >= min_val
                        && n <= f64::from(i16::MAX) =>
                {
                    Some(Some(n as i16))
                }
                Some(_) => None,
            }
        };
        Some(BoxSpacing {
            top: conv(sv.top)?,
            right: conv(sv.right)?,
            bottom: conv(sv.bottom)?,
            left: conv(sv.left)?,
        })
    };

    let (d_sv, t_sv, m_sv) = extract_spacing_breakpoint_values(val, fallback, responsive);
    let d_box = match d_sv {
        Some(sv) => Some(to_box(sv)?),
        None => None,
    };
    let t_box = match t_sv {
        Some(sv) => Some(to_box(sv)?),
        None => None,
    };
    let m_box = match m_sv {
        Some(sv) => Some(to_box(sv)?),
        None => None,
    };
    let prefix = match property {
        SpacingProperty::Padding => "p",
        SpacingProperty::Margin => "m",
    };
    compile_box_spacing_tailwind(prefix, d_box, t_box, m_box, allow_negative)
}

pub struct BlockRenderInputs<'a> {
    pub hydrated_props: &'a BTreeMap<String, serde_json::Value>,
    pub local_richtext_nonces: Option<RichTextNonceTable>,
    pub merged_options: Option<&'a BTreeMap<String, serde_json::Value>>,
    pub site_options: Option<&'a BTreeMap<String, serde_json::Value>>,
    pub menus: Option<&'a BTreeMap<String, serde_json::Value>>,
    pub children_html: Option<&'a str>,
    pub children_meta: Option<&'a [serde_json::Value]>,
    pub parent_ctx: Option<&'a serde_json::Value>,
    pub is_preview: bool,
    pub fmt_cfg: &'a RenderFormattingConfig,
}

/// Pre-compiles a block template once into a reusable `Environment<'static>`
/// (`AutoEscape::Html`, `UndefinedBehavior::Strict`, static filters/tests).
pub fn compile_block_template_base_env(
    block_id: &str,
    template_source: &str,
) -> Result<std::sync::Arc<Environment<'static>>, &'static str> {
    let mut env = restricted_environment_core();
    env.add_template_owned(block_id.to_owned(), template_source.to_owned())
        .map_err(|_| "Không thể biên dịch template block.")?;
    Ok(std::sync::Arc::new(env))
}

/// Renders a pre-compiled `block.json` v1 template by cloning `base_env` per node
/// and binding request-scoped `fuel`, `od_icon`, `od_hook`, `date_format` tz, and l10n.
pub fn render_block_csec_compiled(
    block_id: &str,
    base_env: &std::sync::Arc<Environment<'static>>,
    inputs: BlockRenderInputs<'_>,
    budget: &mut RenderBudget,
) -> Result<String, &'static str> {
    if budget.fuel_remaining == 0 {
        return Err("Đã hết ngân sách fuel render trang.");
    }
    let local_rt_count = inputs
        .local_richtext_nonces
        .as_ref()
        .map_or(0, RichTextNonceTable::total_count);
    if budget
        .richtext_nonces
        .total_count()
        .saturating_add(local_rt_count)
        > MAX_RICHTEXT_NONCES_PER_REQUEST
    {
        return Err("Số lượng khối RichText vượt giới hạn 128/request.");
    }
    let node_fuel = budget.fuel_remaining.min(MAX_BLOCK_FUEL_PER_NODE);

    let mut ctx_map: BTreeMap<String, Value> = BTreeMap::new();
    for (k, v) in inputs.hydrated_props {
        ctx_map.insert(k.clone(), json_to_safe_value(v));
    }

    let mut site_opts = opendash_core::field_schema::global_site_option_defaults();
    if let Some(extra) = inputs.site_options {
        for (k, v) in extra {
            site_opts.insert(k.clone(), v.clone());
        }
    }
    let mut merged_opts = site_opts.clone();
    if let Some(extra) = inputs.merged_options {
        for (k, v) in extra {
            merged_opts.insert(k.clone(), v.clone());
        }
    }

    let options_value = Value::from_pairs(
        merged_opts
            .into_iter()
            .map(|(k, v)| (k, json_to_safe_value(&v))),
    );
    let site_options_value = Value::from_pairs(
        site_opts
            .into_iter()
            .map(|(k, v)| (k, json_to_safe_value(&v))),
    );
    let menus_value = match inputs.menus {
        Some(m) => Value::from_pairs(m.iter().map(|(k, v)| (k.clone(), json_to_safe_value(v)))),
        None => Value::from_pairs(std::iter::empty::<(String, Value)>()),
    };

    ctx_map.insert("options".to_owned(), options_value);
    ctx_map.insert("site_options".to_owned(), site_options_value);
    ctx_map.insert("menus".to_owned(), menus_value);
    ctx_map.insert("od_is_preview".to_owned(), Value::from(inputs.is_preview));
    if let Some(ch) = inputs.children_html {
        ctx_map.insert("children_html".to_owned(), Value::from(ch));
    }
    if let Some(cm) = inputs.children_meta {
        ctx_map.insert(
            "children_meta".to_owned(),
            Value::from(cm.iter().map(json_to_safe_value).collect::<Vec<_>>()),
        );
    }
    if let Some(pc) = inputs.parent_ctx {
        ctx_map.insert("parent_ctx".to_owned(), json_to_safe_value(pc));
    }

    let mut env = (**base_env).clone();
    install_dynamic_formatting_handlers(&mut env, inputs.fmt_cfg);
    let local_icons = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let local_hooks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let overflowed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    install_icon_function(
        &mut env,
        &budget.icon_nonces,
        &local_icons,
        budget.html_bytes_used,
        &overflowed,
    );
    install_hook_function(&mut env, &budget.hook_nonces, &local_hooks);
    env.set_fuel(Some(node_fuel));
    let captured = env
        .get_template(block_id)
        .map_err(|_| "Không tìm thấy template block.")?
        .render_captured(Value::from_pairs(ctx_map))
        .map_err(|_| {
            if overflowed.load(std::sync::atomic::Ordering::Relaxed) {
                budget.html_bytes_used = MAX_RENDERED_HTML_BYTES + 1;
            }
            "Lỗi thực thi template block (fuel/recursion/undefined)."
        })?;

    let (consumed, _remaining) = captured
        .state()
        .fuel_levels()
        .ok_or("Thiếu bộ theo dõi fuel cho template block.")?;
    budget.fuel_remaining = budget.fuel_remaining.saturating_sub(consumed);

    let shell = captured.output();
    if shell.len() > MAX_RENDERED_HTML_BYTES {
        return Err("HTML render của block vượt quá giới hạn 512 KiB.");
    }
    validate_html_and_css_context(shell, true, true)?;

    let out = match inputs.local_richtext_nonces {
        Some(rt) if !rt.is_empty() => rt.substitute_into_document(shell.to_owned())?,
        _ => shell.to_owned(),
    };

    let charge_bytes = match inputs.children_html {
        Some(nonce) if out.contains(nonce) => out.len().saturating_sub(nonce.len()),
        _ => out.len(),
    };
    budget.charge_html_bytes(charge_bytes)?;
    budget.richtext_nonces.reserve_count(local_rt_count)?;
    let additions = local_icons
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let icon_growth: usize = additions
        .iter()
        .map(|(n, s)| s.len().saturating_sub(n.len()))
        .sum();
    budget.charge_html_bytes(icon_growth)?;
    budget.icon_nonces.extend(additions);
    let hook_additions = local_hooks
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    budget.hook_nonces.extend(hook_additions);
    Ok(out)
}

/// Renders a validated `block.json` v1 template from pre-hydrated field values under the
/// shared request `RenderBudget` (`<= 2_000` fuel per node, `<= 10_000` total fuel,
/// `<= 512 KiB` total HTML, `<= 128` RichText blocks per request).
pub fn render_block_csec_with_inputs(
    block_id: &str,
    template_source: &str,
    inputs: BlockRenderInputs<'_>,
    budget: &mut RenderBudget,
) -> Result<String, &'static str> {
    let base_env = compile_block_template_base_env(block_id, template_source)?;
    render_block_csec_compiled(block_id, &base_env, inputs, budget)
}
