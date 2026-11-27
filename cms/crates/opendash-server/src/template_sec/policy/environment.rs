use super::*;

pub fn restricted_environment() -> Environment<'static> {
    restricted_environment_with_config(&RenderFormattingConfig::default())
}

pub fn restricted_environment_with_config(cfg: &RenderFormattingConfig) -> Environment<'static> {
    let mut env = Environment::empty();
    env.set_auto_escape_callback(|_| AutoEscape::Html);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_fuel(Some(TEMPLATE_FUEL_LIMIT));
    env.set_recursion_limit(TEMPLATE_RECURSION_LIMIT);
    env.set_formatter(|out, state, value| {
        if value.is_none() {
            Ok(())
        } else {
            minijinja::escape_formatter(out, state, value)
        }
    });
    env.add_filter("default", filter_default);
    env.add_filter("d", filter_default);
    env.add_filter("trim", minijinja::filters::trim);
    env.add_filter("lower", minijinja::filters::lower);
    env.add_filter("upper", minijinja::filters::upper);
    env.add_filter("length", minijinja::filters::length);
    env.add_filter("first", minijinja::filters::first);
    env.add_filter("last", minijinja::filters::last);
    env.add_filter("join", minijinja::filters::join);
    env.add_filter("replace", minijinja::filters::replace);
    env.add_filter("int", minijinja::filters::int);
    env.add_filter("float", minijinja::filters::float);
    env.add_filter("string", minijinja::filters::string);
    env.add_filter("list", minijinja::filters::list);
    env.add_filter("abs", minijinja::filters::abs);
    env.add_filter("round", minijinja::filters::round);
    env.add_filter("sort", minijinja::filters::sort);
    env.add_filter("reverse", minijinja::filters::reverse);
    env.add_filter("unique", minijinja::filters::unique);
    env.add_filter("title", minijinja::filters::title);
    env.add_filter("capitalize", minijinja::filters::capitalize);
    let default_tz =
        jiff::tz::TimeZone::get(cfg.timezone.as_str()).unwrap_or(jiff::tz::TimeZone::UTC);
    env.add_filter(
        "date_format",
        move |value: Value, fmt: Option<&str>, tz: Option<&str>| {
            filter_date_format(value, fmt, tz, &default_tz)
        },
    );
    env.add_filter("truncate", filter_truncate);
    env.add_filter("format_number", filter_format_number);
    env.add_filter("batch", filter_batch);
    env.add_filter("selectattr", filter_selectattr);
    env.add_filter("rejectattr", filter_rejectattr);
    env.add_filter("map", filter_map);

    env.add_test("defined", minijinja::tests::is_defined);
    env.add_test("undefined", minijinja::tests::is_undefined);
    env.add_test("eq", minijinja::tests::is_eq);
    env.add_test("equalto", minijinja::tests::is_eq);
    env.add_test("ne", minijinja::tests::is_ne);
    env.add_test("lt", minijinja::tests::is_lt);
    env.add_test("le", minijinja::tests::is_le);
    env.add_test("gt", minijinja::tests::is_gt);
    env.add_test("ge", minijinja::tests::is_ge);
    env.add_test("none", minijinja::tests::is_none);
    env.add_test("true", minijinja::tests::is_true);
    env.add_test("false", minijinja::tests::is_false);
    env.add_test("in", minijinja::tests::is_in);
    env.add_test("startingwith", minijinja::tests::is_startingwith);
    env.add_test("endingwith", minijinja::tests::is_endingwith);
    env.add_test("even", minijinja::tests::is_even);
    env.add_test("odd", minijinja::tests::is_odd);
    env.add_test("number", minijinja::tests::is_number);
    env.add_test("string", minijinja::tests::is_string);
    env.add_test("sequence", minijinja::tests::is_sequence);
    env.add_test("mapping", minijinja::tests::is_mapping);

    let catalogs_1 = cfg.l10n_catalogs.clone();
    let default_dom_1 = cfg.l10n_domain.clone().unwrap_or_default();
    env.add_function(
        "__",
        move |msgid: &str, domain_or_params: Option<Value>, params: Option<Value>| -> String {
            let (domain, p) = split_domain_and_params(domain_or_params, params, &default_dom_1);
            let translated = catalogs_1
                .get(domain.as_str())
                .and_then(|c| c.get_with_context(None, msgid))
                .unwrap_or(msgid);
            interpolate_l10n_params(translated, p.as_ref(), None)
        },
    );

    let catalogs_2 = cfg.l10n_catalogs.clone();
    let default_dom_2 = cfg.l10n_domain.clone().unwrap_or_default();
    env.add_function(
        "_x",
        move |msgid: &str,
              context: &str,
              domain_or_params: Option<Value>,
              params: Option<Value>|
              -> String {
            let (domain, p) = split_domain_and_params(domain_or_params, params, &default_dom_2);
            let translated = catalogs_2
                .get(domain.as_str())
                .and_then(|c| c.get_with_context(Some(context), msgid))
                .unwrap_or(msgid);
            interpolate_l10n_params(translated, p.as_ref(), None)
        },
    );

    let catalogs_3 = cfg.l10n_catalogs.clone();
    let default_dom_3 = cfg.l10n_domain.clone().unwrap_or_default();
    env.add_function(
        "_n",
        move |singular: &str,
              plural: &str,
              count: i64,
              domain_or_params: Option<Value>,
              params: Option<Value>|
              -> String {
            let (domain, p) = split_domain_and_params(domain_or_params, params, &default_dom_3);
            let fallback = if count == 1 { singular } else { plural };
            let translated = catalogs_3
                .get(domain.as_str())
                .and_then(|c| c.get_plural_with_context(None, singular, count))
                .unwrap_or(fallback);
            interpolate_l10n_params(translated, p.as_ref(), Some(count))
        },
    );
    env
}

fn split_domain_and_params(
    domain_or_params: Option<Value>,
    params: Option<Value>,
    default_domain: &str,
) -> (String, Option<Value>) {
    match domain_or_params {
        Some(v) if v.as_str().is_some() => {
            (v.as_str().unwrap_or(default_domain).to_owned(), params)
        }
        Some(v) => (default_domain.to_owned(), Some(v)),
        None => (default_domain.to_owned(), params),
    }
}

fn interpolate_l10n_params(template: &str, params: Option<&Value>, count: Option<i64>) -> String {
    let mut out = template.to_owned();
    if let Some(c) = count {
        let s = c.to_string();
        out = out.replace("{n}", &s).replace("{count}", &s);
    }
    if let Some(map) = params
        && map.kind() == minijinja::value::ValueKind::Map
        && let Ok(iter) = map.try_iter()
    {
        for key in iter {
            if let Some(k) = key.as_str()
                && let Ok(val) = map.get_item(&key)
                && !val.is_undefined()
                && !val.is_none()
            {
                out = out.replace(&format!("{{{k}}}"), &val.to_string());
            }
        }
    }
    out
}

/// Validates a template against the C-SEC MiniJinja AST allowlist and contextual HTML/URL/CSS rules.
pub fn validate_template_csec(name: &str, source: &str) -> Result<(), &'static str> {
    validate_template_csec_with_theme_urls(name, source, &ThemeUrlPolicy::default())
}

pub fn validate_template_csec_with_theme_urls(
    name: &str,
    source: &str,
    policy: &ThemeUrlPolicy,
) -> Result<(), &'static str> {
    validate_template_csec_with_usage_flags(name, source, policy, false, false).map(|_| ())
}

pub(crate) fn validate_template_csec_with_usage_flags(
    name: &str,
    source: &str,
    policy: &ThemeUrlPolicy,
    allow_od_icon: bool,
    allow_trusted_theme_markup: bool,
) -> Result<ThemeTemplateUsageFlags, &'static str> {
    if source.trim().is_empty() {
        return Err("Template không được để trống.");
    }
    if source.contains(BLOCK_RICHTEXT_NONCE_PREFIX)
        || source.contains(ICON_NONCE_PREFIX)
        || source.contains(HOOK_NONCE_PREFIX)
    {
        return Err("Theme HTML chứa marker icon nội bộ bị cấm.");
    }
    let ast = parse(source, name, SyntaxConfig::default())
        .map_err(|_| "Cú pháp template MiniJinja không hợp lệ.")?;

    validate_stmt_with_icons_ext(&ast, allow_od_icon, allow_od_icon, false)?;
    reject_reserved_context_rebinding(&ast)?;
    let is_full_doctype_doc = source
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("<!doctype");
    let usage = walk_theme_ast_with_url_scopes(
        &ast,
        false,
        is_full_doctype_doc,
        policy,
        allow_trusted_theme_markup,
    )?;

    let mut env = restricted_environment();
    env.add_template(name, source)
        .map_err(|_| "Không thể biên dịch template trong môi trường an toàn.")?;
    Ok(usage)
}

/// Validates and renders a template under C-SEC limits (fuel, recursion, 512 KiB output cap,
/// and post-render URL/HTML boundary verification).
pub fn render_csec(name: &str, source: &str, context: Value) -> Result<String, &'static str> {
    render_csec_with_config(name, source, context, &RenderFormattingConfig::default())
}

pub fn render_csec_with_config(
    name: &str,
    source: &str,
    context: Value,
    fmt_cfg: &RenderFormattingConfig,
) -> Result<String, &'static str> {
    validate_template_csec(name, source)?;
    let mut env = restricted_environment_with_config(fmt_cfg);
    env.add_template(name, source)
        .map_err(|_| "Không thể biên dịch template.")?;
    let tpl = env
        .get_template(name)
        .map_err(|_| "Không tìm thấy template đã biên dịch.")?;
    let rendered = tpl
        .render(context)
        .map_err(|_| "Lỗi thực thi template (fuel/recursion/strict undefined).")?;
    if rendered.len() > MAX_RENDERED_HTML_BYTES {
        return Err("HTML render vượt quá giới hạn 512 KiB.");
    }
    validate_html_and_css_context(&rendered, true, false)?;
    Ok(rendered)
}

/// Checks theme v3 HTML and every declared part, then resolves the literal
/// `od_part("key")` DAG into one ordinary, C-SEC-approved MiniJinja source.
///
/// `parts` maps the literal part key to its HTML fragment, not a filesystem
/// path. No template loader, dynamic include, raw filter or function is enabled.
/// Depth counts part expansions from the root, so root -> a -> b -> c is 3.
pub fn validate_and_compile_csec_composition(
    root_name: &str,
    root_source: &str,
    parts: &BTreeMap<String, String>,
    kind: CsecContentKind,
) -> Result<CompiledCsecComposition, &'static str> {
    validate_and_compile_csec_composition_with_theme_urls(
        root_name,
        root_source,
        parts,
        kind,
        &ThemeUrlPolicy::default(),
    )
}

pub fn validate_and_compile_csec_composition_with_theme_urls(
    root_name: &str,
    root_source: &str,
    parts: &BTreeMap<String, String>,
    kind: CsecContentKind,
    policy: &ThemeUrlPolicy,
) -> Result<CompiledCsecComposition, &'static str> {
    // Each resource is bounded independently. A theme can declare many safe
    // parts which are not all rendered in one request; the flattened reachable
    // program and rendered output have their own 512 KiB checks below.
    if root_source.len() > MAX_RENDERED_HTML_BYTES
        || parts
            .values()
            .any(|part| part.len() > MAX_RENDERED_HTML_BYTES)
    {
        return Err("Template hoặc part vượt giới hạn 512 KiB.");
    }
    let root_sites = composition_sites(root_name, root_source, false, policy)?;
    let mut part_sites = BTreeMap::new();
    for (key, source) in parts {
        if !valid_part_key(key) {
            return Err("Part key không hợp lệ.");
        }
        part_sites.insert(key.clone(), composition_sites(key, source, true, policy)?);
    }

    // Check all declared parts, even unreachable ones: a dormant cycle must
    // not become executable when a different template references that part.
    for key in parts.keys() {
        let mut visiting = BTreeSet::new();
        check_part_graph(key, &part_sites, &mut visiting, 1, &mut 0)?;
    }
    let mut expansions = 0_usize;
    let mut visiting = BTreeSet::new();
    let mut content_count = 0_usize;
    let mut max_part_depth = 0_usize;
    let compiled_source = flatten_composition(
        root_source,
        &root_sites,
        parts,
        &part_sites,
        &mut visiting,
        0,
        &mut expansions,
        &mut content_count,
        &mut max_part_depth,
    )?;

    match kind {
        CsecContentKind::Entry if content_count != 1 => {
            return Err("Template bản ghi phải có đúng một od_content().");
        }
        CsecContentKind::Index if content_count > 1 => {
            return Err("Index chỉ được có tối đa một od_content().");
        }
        CsecContentKind::NonEntry if content_count != 0 => {
            return Err("Template không phải bản ghi cấm od_content().");
        }
        _ => {}
    }
    if compiled_source.len() > MAX_RENDERED_HTML_BYTES {
        return Err("Template composition vượt giới hạn 512 KiB.");
    }
    // This last check catches dangerous markup introduced across part boundaries,
    // in addition to the independent source checks before expansion.
    let usage =
        validate_template_csec_with_usage_flags(root_name, &compiled_source, policy, true, true)?;
    Ok(CompiledCsecComposition {
        source: compiled_source,
        name: root_name.to_owned(),
        content_count,
        part_expansions: expansions,
        max_part_depth,
        uses_related_entries: usage.uses_related_entries,
        uses_recent_posts: usage.uses_recent_posts,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn flatten_composition(
    source: &str,
    sites: &[CompositionSite],
    parts: &BTreeMap<String, String>,
    part_sites: &BTreeMap<String, Vec<CompositionSite>>,
    visiting: &mut BTreeSet<String>,
    depth: usize,
    expansions: &mut usize,
    content_count: &mut usize,
    max_part_depth: &mut usize,
) -> Result<String, &'static str> {
    let mut out = String::new();
    let mut cursor = 0_usize;
    for site in sites {
        out.push_str(&source[cursor..site.start]);
        match &site.call {
            CompositionCall::Content => {
                *content_count += 1;
                out.push_str(CONTENT_SLOT_TOKEN);
            }
            CompositionCall::Part(key) => {
                *max_part_depth = (*max_part_depth).max(depth + 1);
                *expansions += 1;
                if *expansions > MAX_COMPOSITION_EXPANSIONS {
                    return Err("od_part vượt quá 32 lần mở rộng.");
                }
                if depth >= MAX_COMPOSITION_PART_DEPTH {
                    return Err("Độ sâu od_part vượt giới hạn 3.");
                }
                if !visiting.insert(key.to_owned()) {
                    return Err("Phát hiện vòng lặp od_part.");
                }
                let part = parts
                    .get(key)
                    .ok_or("od_part trỏ tới part không tồn tại.")?;
                let child_sites = part_sites.get(key).ok_or("Thiếu part đã kiểm tra.")?;
                let child = flatten_composition(
                    part,
                    child_sites,
                    parts,
                    part_sites,
                    visiting,
                    depth + 1,
                    expansions,
                    content_count,
                    max_part_depth,
                )?;
                visiting.remove(key);
                out.push_str(&child);
            }
        }
        if out.len() > MAX_RENDERED_HTML_BYTES {
            return Err("Template composition vượt giới hạn 512 KiB.");
        }
        cursor = site.end;
    }
    out.push_str(&source[cursor..]);
    if out.len() > MAX_RENDERED_HTML_BYTES {
        return Err("Template composition vượt giới hạn 512 KiB.");
    }
    Ok(out)
}

pub(crate) fn composition_sites(
    name: &str,
    source: &str,
    is_part: bool,
    policy: &ThemeUrlPolicy,
) -> Result<Vec<CompositionSite>, &'static str> {
    if source.trim().is_empty() {
        return Err("Template không được để trống.");
    }
    if source.contains(CONTENT_SLOT_TOKEN)
        || source.contains(BLOCK_RICHTEXT_NONCE_PREFIX)
        || source.contains(ICON_NONCE_PREFIX)
        || source.contains(HOOK_NONCE_PREFIX)
    {
        return Err("Theme HTML chứa marker nội bộ bị cấm.");
    }
    let ast = parse(source, name, SyntaxConfig::default())
        .map_err(|_| "Cú pháp theme MiniJinja không hợp lệ.")?;
    reject_reserved_context_rebinding(&ast)?;
    let is_full_doctype_doc = !is_part
        && source
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("<!doctype");
    let _ = walk_theme_ast_with_url_scopes(&ast, is_part, is_full_doctype_doc, policy, true)?;
    let mut sites = Vec::new();
    collect_composition_sites(&ast, source, false, &mut sites)?;
    sites.sort_by_key(|site| site.start);
    let mut cursor = 0;
    for site in &sites {
        if site.start < cursor || site.end > source.len() {
            return Err("Vị trí od_part/od_content không hợp lệ.");
        }
        if is_part && matches!(site.call, CompositionCall::Content) {
            return Err("Template part không được chứa od_content().");
        }
        cursor = site.end;
    }
    Ok(sites)
}
